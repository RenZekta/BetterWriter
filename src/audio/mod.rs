use std::{
    fs::File,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, TryRecvError},
    },
    time::{Duration, Instant},
};

use cpal::{
    FromSample, Sample, SizedSample,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use oxisynth::{MidiEvent, SoundFont, Synth, SynthDescriptor};
use ringbuf::{
    HeapCons, HeapProd, HeapRb,
    traits::{Consumer, Producer, Split},
};
use thiserror::Error;
use crate::{
    plugins::{
        host::{Graph, LivePlugin},
        vst3::NoteEvent,
    },
};
use crate::core::{
    AutomationLane, BwxProject, MidiEventKind, MidiPlaybackEvent, TempoMap, Tick,
    automation::controller_steps, sounds::midi_pan,
};

/// What a scheduled event does to its channel.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Action {
    NoteOn { key: u8, velocity: u8 },
    NoteOff { key: u8 },
    /// Controller 7 value from volume automation.
    Volume(u8),
    /// Controller 10 value from pan automation.
    Pan(u8),
    /// Another preset for the notes that follow on this channel.
    Sound { bank: u16, program: u8 },
}

/// An event with the moment it sounds, in seconds from the start of the
/// song. Every track is timed through its own tempo map, so the audio clock
/// only counts seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Scheduled {
    seconds: f64,
    channel: u8,
    action: Action,
}

impl Scheduled {
    /// Controller changes land before the notes that share their moment, and
    /// note ends before note starts.
    fn rank(&self) -> u8 {
        match self.action {
            Action::Volume(_) | Action::Pan(_) | Action::Sound { .. } => 0,
            Action::NoteOff { .. } => 1,
            Action::NoteOn { .. } => 2,
        }
    }
}

/// Everything the audio thread needs to play a song from `start_seconds`.
struct Schedule {
    events: Vec<Scheduled>,
    start_seconds: f64,
    /// Playback counts as finished once the clock reaches this.
    end_seconds: f64,
}

/// Times the notes and automation of `project`. `start_tick` is a position
/// on `start_track`'s own axis; the other tracks start at the same moment.
/// Notes before the start are dropped; of the volume and pan changes before
/// it, the latest of each channel is kept and applied at the start so a song
/// begun mid-way sounds as it would at that point.
fn build_schedule(
    project: &BwxProject,
    notes: &[MidiPlaybackEvent],
    start_tick: Tick,
    start_track: u64,
) -> Schedule {
    let mut maps: std::collections::HashMap<u64, TempoMap> = std::collections::HashMap::new();
    let seconds_of = |track_id: u64, tick: Tick, maps: &mut std::collections::HashMap<u64, TempoMap>| {
        maps.entry(track_id)
            .or_insert_with(|| project.tempo_map_for(track_id))
            .seconds_at(tick as f64)
    };

    let start_seconds = seconds_of(start_track, start_tick, &mut maps);
    let mut events: Vec<Scheduled> = notes
        .iter()
        .map(|event| Scheduled {
            seconds: seconds_of(event.track_id, event.tick, &mut maps),
            channel: event.channel,
            action: match event.kind {
                MidiEventKind::NoteOn => Action::NoteOn {
                    key: event.key,
                    velocity: event.velocity,
                },
                MidiEventKind::NoteOff => Action::NoteOff { key: event.key },
            },
        })
        .collect();
    let last_note = events.iter().map(|event| event.seconds).fold(0.0_f64, f64::max);

    // A note that sounds with another preset than its track's gets a preset
    // change just before it, and the track's own preset is restored for the
    // next note that uses it. A chord shares the preset of its last note.
    let base_sounds: std::collections::HashMap<u8, (u16, u8)> = project
        .tracks
        .iter()
        .map(|track| (track.channel, track.sound()))
        .collect();
    let mut onsets: Vec<(f64, u8, Option<(u16, u8)>)> = notes
        .iter()
        .filter(|event| event.kind == MidiEventKind::NoteOn)
        .map(|event| (seconds_of(event.track_id, event.tick, &mut maps), event.channel, event.sound))
        .filter(|&(seconds, ..)| seconds >= start_seconds)
        .collect();
    onsets.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut in_use = base_sounds.clone();
    for (seconds, channel, sound) in onsets {
        let Some(&base) = base_sounds.get(&channel) else {
            continue;
        };
        let wanted = sound.unwrap_or(base);
        if wanted != base || in_use.get(&channel) != Some(&wanted) {
            events.push(Scheduled {
                seconds,
                channel,
                action: Action::Sound {
                    bank: wanted.0,
                    program: wanted.1,
                },
            });
            in_use.insert(channel, wanted);
        }
    }
    let song_end = project
        .tracks
        .iter()
        .filter_map(|track| {
            let end = track.playback_end_tick()?;
            Some(seconds_of(track.id, end, &mut maps))
        })
        .fold(0.0_f64, f64::max);

    let mut controllers: Vec<Scheduled> = Vec::new();
    let any_solo = project.tracks.iter().any(|track| track.solo);
    for track in &project.tracks {
        let scope = project.scope_for(track.id);
        // A muted track, or one a solo silences, keeps its volume at 0 so
        // automation does not bring it back.
        let silent = track.mute || (any_solo && !track.solo);
        for (lane, make) in [
            (AutomationLane::Volume, Action::Volume as fn(u8) -> Action),
            (AutomationLane::Pan, Action::Pan as fn(u8) -> Action),
        ] {
            if silent && lane == AutomationLane::Volume {
                continue;
            }
            let points = project.automation_points(lane, scope);
            for (tick, value) in controller_steps(lane, &points) {
                controllers.push(Scheduled {
                    seconds: seconds_of(track.id, tick, &mut maps),
                    channel: track.channel,
                    action: make(value),
                });
            }
        }
    }
    // Keep the state in force at the start, then everything after it.
    let (earlier, later): (Vec<Scheduled>, Vec<Scheduled>) = controllers
        .into_iter()
        .partition(|event| event.seconds < start_seconds);
    let mut in_force: Vec<Scheduled> = Vec::new();
    for event in earlier {
        let same = |other: &Scheduled| {
            other.channel == event.channel && other.rank() == event.rank() && {
                matches!(
                    (other.action, event.action),
                    (Action::Volume(_), Action::Volume(_)) | (Action::Pan(_), Action::Pan(_))
                )
            }
        };
        match in_force.iter_mut().find(|other| same(other)) {
            Some(slot) if slot.seconds <= event.seconds => *slot = event,
            Some(_) => {}
            None => in_force.push(event),
        }
    }
    events.retain(|event| event.seconds >= start_seconds);
    events.extend(in_force.into_iter().map(|event| Scheduled {
        seconds: start_seconds,
        ..event
    }));
    events.extend(later);
    events.sort_by(|a, b| {
        a.seconds
            .total_cmp(&b.seconds)
            .then(a.rank().cmp(&b.rank()))
            .then(a.channel.cmp(&b.channel))
    });

    Schedule {
        events,
        start_seconds,
        end_seconds: song_end.max(last_note),
    }
}

/// Synth state for one MIDI channel, taken from the track that plays on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelSetup {
    channel: u8,
    /// Soundfont bank; controller 0 carries its high part, controller 32
    /// its low 7 bits.
    bank: u16,
    program: u8,
    /// Controller 7 value; 0 for a muted track.
    volume: u8,
    /// Controller 10 value, 64 being centre.
    pan: u8,
}

/// One setup per track. A muted track, or any track while another one is
/// soloed, is silenced through channel volume so it can change mid-playback.
pub fn channel_setups(project: &BwxProject) -> Vec<ChannelSetup> {
    let any_solo = project.tracks.iter().any(|track| track.solo);
    project
        .tracks
        .iter()
        .map(|track| {
            let silent = track.mute || (any_solo && !track.solo);
            let (bank, program) = track.sound();
            ChannelSetup {
                channel: track.channel.min(15),
                bank,
                program: program.min(127),
                volume: if silent { 0 } else { track.volume.min(127) },
                pan: midi_pan(track.pan),
            }
        })
        .collect()
}

/// Events that put a channel into a setup. The bank is selected explicitly so
/// a program number always means the sound the track asked for; bank 128
/// (the drum kits) is controller 0 = 1 and controller 32 = 0.
fn setup_events(setup: ChannelSetup) -> [MidiEvent; 5] {
    let channel = setup.channel;
    let [bank_high, bank_low, program] = sound_events(channel, setup.bank, setup.program);
    [
        bank_high,
        bank_low,
        program,
        MidiEvent::ControlChange {
            channel,
            ctrl: 7,
            value: setup.volume,
        },
        MidiEvent::ControlChange {
            channel,
            ctrl: 10,
            value: setup.pan,
        },
    ]
}

/// Events that select a preset on a channel.
fn sound_events(channel: u8, bank: u16, program: u8) -> [MidiEvent; 3] {
    [
        MidiEvent::ControlChange {
            channel,
            ctrl: 0,
            value: ((bank >> 7) & 0x7f) as u8,
        },
        MidiEvent::ControlChange {
            channel,
            ctrl: 32,
            value: (bank & 0x7f) as u8,
        },
        MidiEvent::ProgramChange {
            channel,
            program_id: program,
        },
    ]
}

/// Stack size for the soundfont loader thread. Decoding an SF3 allocates and
/// recurses far more than a default thread stack comfortably allows.
const LOADER_STACK_BYTES: usize = 64 * 1024 * 1024;

enum AudioCommand {
    InstallSynth(Box<Synth>),
    /// The plug-in chains to run; replaces the previous graph.
    SetGraph(Arc<Graph>),
    Play {
        events: Arc<Vec<Scheduled>>,
        start_seconds: f64,
        /// Playback counts as finished once the clock reaches this.
        end_seconds: f64,
        channels: Arc<Vec<ChannelSetup>>,
    },
    /// Applies new channel setups without touching playback.
    SetChannels(Arc<Vec<ChannelSetup>>),
    /// A key played live; velocity 0 releases it.
    Live { channel: u8, key: u8, velocity: u8 },
    Stop,
}

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("no default output device is available")]
    NoOutputDevice,
    #[error("default output config failed: {0}")]
    DefaultConfig(#[from] cpal::DefaultStreamConfigError),
    #[error("stream build failed: {0}")]
    BuildStream(#[from] cpal::BuildStreamError),
    #[error("stream play failed: {0}")]
    PlayStream(#[from] cpal::PlayStreamError),
    #[error("soundfont load failed: {0}")]
    SoundFontLoad(String),
    #[error("audio command queue is full")]
    CommandQueueFull,
}

/// State shared between the UI thread and the audio callback thread.
#[derive(Default)]
struct SharedFlags {
    /// True from the moment the callback starts a song until it stops or the
    /// clock reaches the song's end. Notes still ringing at the end keep
    /// sounding after this goes false.
    playing: AtomicBool,
    /// The song position in seconds, as `f64` bits, kept current while
    /// playing.
    position: AtomicU64,
    /// Set when the audio callback panicked; the runtime rebuilds the stream
    /// the next time playback is requested.
    faulted: AtomicBool,
    /// Set by the stream's error callback when the device went away or its
    /// format changed, which leaves the stream silent until it is rebuilt.
    stream_lost: AtomicBool,
}

/// What a stream was opened with, to notice the device changing under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeviceFormat {
    sample_rate: u32,
    channels: u16,
    sample_format: cpal::SampleFormat,
}

/// How often the default output device's format is compared with the one
/// the stream was opened with.
const DEVICE_CHECK_INTERVAL: Duration = Duration::from_secs(1);

fn default_device_format() -> Option<DeviceFormat> {
    let device = cpal::default_host().default_output_device()?;
    let config = device.default_output_config().ok()?;
    Some(DeviceFormat {
        sample_rate: config.sample_rate(),
        channels: config.channels(),
        sample_format: config.sample_format(),
    })
}

struct PendingPlay {
    schedule: Schedule,
}

type LoaderResult = Result<Box<Synth>, String>;

pub struct AudioRuntime {
    soundfont_path: PathBuf,
    producer: HeapProd<AudioCommand>,
    stream: Option<cpal::Stream>,
    sample_rate: f32,
    flags: Arc<SharedFlags>,
    /// Receives the synth built by the soundfont loader thread.
    loader: Option<mpsc::Receiver<LoaderResult>>,
    synth_installed: bool,
    /// A Play request made while the soundfont was still loading; it is sent
    /// as soon as the synth is installed.
    pending_play: Option<PendingPlay>,
    /// Channel setups last handed to the synth, or about to be with the next
    /// Play; `sync_mix` only sends a command when the project's differ.
    mix: Vec<ChannelSetup>,
    /// The plug-in chains last handed to the audio thread.
    graph: Arc<Graph>,
    graph_key: Vec<u64>,
    status: String,
    /// What the open stream was built for.
    format: Option<DeviceFormat>,
    last_device_check: Instant,
    /// Where a song that a device change interrupted was, in seconds.
    resume_seconds: Option<f64>,
}

impl AudioRuntime {
    pub fn new(soundfont_path: impl Into<PathBuf>) -> Self {
        // Placeholder queue; `ensure_stream` replaces it with a fresh pair
        // every time it builds a stream, so a failed attempt can be retried.
        let (producer, _consumer) = HeapRb::<AudioCommand>::new(16).split();
        Self {
            soundfont_path: soundfont_path.into(),
            producer,
            stream: None,
            sample_rate: 44_100.0,
            flags: Arc::new(SharedFlags::default()),
            loader: None,
            synth_installed: false,
            pending_play: None,
            mix: Vec::new(),
            graph: Arc::new(Graph::default()),
            graph_key: Vec::new(),
            status: "Audio engine idle".to_owned(),
            format: None,
            last_device_check: Instant::now(),
            resume_seconds: None,
        }
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Hands the audio thread a new set of plug-in chains. Does nothing when
    /// the chains are the same plug-ins in the same places as before.
    pub fn set_graph(&mut self, graph: Graph) {
        let key = graph.key();
        if key == self.graph_key {
            return;
        }
        self.graph_key = key;
        self.graph = Arc::new(graph);
        if self.stream.is_some() {
            let _ = self.producer.try_push(AudioCommand::SetGraph(self.graph.clone()));
        }
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    /// True while a song is sounding, or while a Play request is waiting for
    /// the soundfont to finish loading.
    /// True while the soundfont is still being decoded on the loader thread.
    pub fn is_loading(&self) -> bool {
        self.loader.is_some()
    }

    /// Where the song is right now, in seconds from the start of the song,
    /// while it plays. The position follows the clock continuously, not the
    /// notes; `TempoMap::tick_at` turns it into a track's own ticks.
    pub fn playback_seconds(&self) -> Option<f64> {
        self.flags
            .playing
            .load(Ordering::Relaxed)
            .then(|| f64::from_bits(self.flags.position.load(Ordering::Relaxed)))
    }

    pub fn is_playing(&self) -> bool {
        self.pending_play.is_some() || self.flags.playing.load(Ordering::Relaxed)
    }

    /// Call once per UI frame. Moves a finished soundfont load into the audio
    /// thread, releases a waiting Play request, and recovers from audio
    /// thread faults. Returns true while the UI should keep repainting.
    pub fn poll(&mut self) -> bool {
        if self.flags.stream_lost.swap(false, Ordering::Relaxed) || self.device_changed() {
            self.reopen_stream();
        }
        if self.flags.faulted.swap(false, Ordering::Relaxed) {
            self.stream = None;
            self.loader = None;
            self.synth_installed = false;
            self.pending_play = None;
            self.mix.clear();
            self.status = "Audio engine stopped after an internal error; press Play to restart it"
                .to_owned();
        }

        if let Some(receiver) = &self.loader {
            match receiver.try_recv() {
                Ok(Ok(synth)) => {
                    self.loader = None;
                    if self.producer.try_push(AudioCommand::InstallSynth(synth)).is_ok() {
                        self.synth_installed = true;
                        self.status = format!(
                            "Audio online: {} Hz, {}",
                            self.sample_rate,
                            self.soundfont_path.display()
                        );
                        if let Some(pending) = self.pending_play.take() {
                            if let Err(err) = self.send_play(pending) {
                                self.status = format!("Playback failed: {err}");
                            }
                        }
                    } else {
                        self.pending_play = None;
                        self.status = "Audio command queue is full".to_owned();
                    }
                }
                Ok(Err(message)) => {
                    self.loader = None;
                    self.pending_play = None;
                    self.status = format!("Soundfont failed to load: {message}");
                }
                Err(TryRecvError::Disconnected) => {
                    self.loader = None;
                    self.pending_play = None;
                    self.status = "Soundfont loader stopped unexpectedly".to_owned();
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        self.loader.is_some() || self.is_playing()
    }

    /// True when the default output device now has another sample rate,
    /// channel count or sample format than the open stream.
    fn device_changed(&mut self) -> bool {
        if self.stream.is_none() || self.last_device_check.elapsed() < DEVICE_CHECK_INTERVAL {
            return false;
        }
        self.last_device_check = Instant::now();
        default_device_format().is_some_and(|now| Some(now) != self.format)
    }

    /// Replaces a stream the device invalidated: the soundfont is decoded
    /// again for the new sample rate and a song that was playing is offered
    /// back through `take_resume`.
    fn reopen_stream(&mut self) {
        let was_playing = self.flags.playing.load(Ordering::Relaxed);
        let position = f64::from_bits(self.flags.position.load(Ordering::Relaxed));
        self.stream = None;
        self.format = None;
        self.loader = None;
        self.synth_installed = false;
        self.pending_play = None;
        self.mix.clear();
        match self.ensure_stream() {
            Ok(()) => {
                self.preload();
                self.resume_seconds = was_playing.then_some(position.max(0.0));
                self.status = format!(
                    "Audio device changed, reopened at {} Hz",
                    self.sample_rate
                );
            }
            Err(err) => {
                self.status = format!("Audio device unavailable: {err}");
            }
        }
    }

    /// The time a song was interrupted at by a device change, once.
    pub fn take_resume(&mut self) -> Option<f64> {
        self.resume_seconds.take()
    }

    pub fn ensure_stream(&mut self) -> Result<(), AudioError> {
        if self.stream.is_some() {
            return Ok(());
        }

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::NoOutputDevice)?;
        let supported = device.default_output_config()?;
        let config: cpal::StreamConfig = supported.clone().into();
        let sample_rate = config.sample_rate as f32;
        let channels = config.channels as usize;
        let format = DeviceFormat {
            sample_rate: config.sample_rate,
            channels: config.channels,
            sample_format: supported.sample_format(),
        };

        let (producer, consumer) = HeapRb::<AudioCommand>::new(2048).split();
        let flags = Arc::new(SharedFlags::default());

        macro_rules! build {
            ($sample:ty) => {
                build_stream::<$sample>(
                    &device,
                    &config,
                    channels,
                    sample_rate,
                    consumer,
                    flags.clone(),
                )?
            };
        }
        let stream = match supported.sample_format() {
            cpal::SampleFormat::I8 => build!(i8),
            cpal::SampleFormat::I16 => build!(i16),
            cpal::SampleFormat::I24 => build!(cpal::I24),
            cpal::SampleFormat::I32 => build!(i32),
            cpal::SampleFormat::I64 => build!(i64),
            cpal::SampleFormat::U8 => build!(u8),
            cpal::SampleFormat::U16 => build!(u16),
            cpal::SampleFormat::U32 => build!(u32),
            cpal::SampleFormat::U64 => build!(u64),
            cpal::SampleFormat::F32 => build!(f32),
            cpal::SampleFormat::F64 => build!(f64),
            other => {
                return Err(AudioError::SoundFontLoad(format!(
                    "unsupported sample format {other:?}"
                )));
            }
        };

        stream.play()?;
        self.stream = Some(stream);
        self.format = Some(format);
        self.last_device_check = Instant::now();
        self.producer = producer;
        self.flags = flags;
        self.sample_rate = sample_rate;
        self.synth_installed = false;
        self.loader = None;
        let _ = self.producer.try_push(AudioCommand::SetGraph(self.graph.clone()));
        self.status = format!("Audio device open: {sample_rate} Hz, {channels} channels");
        Ok(())
    }

    /// Opens the output device and starts decoding the soundfont in the
    /// background so the first Play press does not have to wait for it.
    /// Failures are silent here; `play` reports them to the user.
    pub fn preload(&mut self) {
        if self.ensure_stream().is_err() {
            return;
        }
        if !self.synth_installed && self.loader.is_none() {
            self.loader = Some(spawn_synth_loader(
                self.sample_rate,
                self.soundfont_path.clone(),
            ));
        }
    }

    /// Starts playback at `start_tick` of `start_track`; the other tracks
    /// start at the same moment. Earlier notes are skipped and the clock
    /// begins at that moment on the tempo map of `start_track`.
    pub fn play(
        &mut self,
        project: &BwxProject,
        events: &[MidiPlaybackEvent],
        start_tick: Tick,
        start_track: u64,
    ) -> Result<(), AudioError> {
        self.ensure_stream()?;
        self.mix = channel_setups(project);
        let pending = PendingPlay {
            schedule: build_schedule(project, events, start_tick, start_track),
        };
        if self.synth_installed {
            return self.send_play(pending);
        }
        if self.loader.is_none() {
            self.loader = Some(spawn_synth_loader(
                self.sample_rate,
                self.soundfont_path.clone(),
            ));
        }
        self.pending_play = Some(pending);
        self.status = "Loading soundfont...".to_owned();
        Ok(())
    }

    /// Call once per UI frame. Applies a changed sound, volume, pan, mute or
    /// solo to the running synth immediately; otherwise the next Play picks
    /// the new setup up.
    pub fn sync_mix(&mut self, project: &BwxProject) {
        let setups = channel_setups(project);
        if setups == self.mix {
            return;
        }
        self.mix = setups;
        if self.synth_installed {
            let _ = self
                .producer
                .try_push(AudioCommand::SetChannels(Arc::new(self.mix.clone())));
        }
    }

    /// Sounds a key on `channel` right away (velocity 0 releases it), for
    /// keys played on a MIDI keyboard. Silent until the soundfont is ready.
    pub fn live_note(&mut self, channel: u8, key: u8, velocity: u8) {
        if self.synth_installed {
            let _ = self
                .producer
                .try_push(AudioCommand::Live { channel, key, velocity });
        }
    }

    pub fn stop(&mut self) -> Result<(), AudioError> {
        self.pending_play = None;
        if self.stream.is_none() {
            return Ok(());
        }
        self.flags.playing.store(false, Ordering::Relaxed);
        self.producer
            .try_push(AudioCommand::Stop)
            .map_err(|_| AudioError::CommandQueueFull)
    }

    fn send_play(&mut self, pending: PendingPlay) -> Result<(), AudioError> {
        // The position still holds where the last playback stopped until the
        // audio thread takes the command, a few milliseconds from now: the
        // playback line would show there for those frames, next to the new
        // one. It starts where the song is about to.
        self.flags
            .position
            .store(pending.schedule.start_seconds.to_bits(), Ordering::Relaxed);
        self.flags.playing.store(true, Ordering::Relaxed);
        self.producer
            .try_push(AudioCommand::Play {
                events: Arc::new(pending.schedule.events),
                start_seconds: pending.schedule.start_seconds,
                end_seconds: pending.schedule.end_seconds,
                channels: Arc::new(self.mix.clone()),
            })
            .map_err(|_| {
                self.flags.playing.store(false, Ordering::Relaxed);
                AudioError::CommandQueueFull
            })
    }
}

/// Audio groups of the synth: channel `n` is group `n`, the last is the
/// reverb and chorus return.
const SYNTH_GROUPS: u8 = 17;

/// The synth renders, and plug-ins run, in blocks of this many frames.
const BLOCK: usize = 64;

/// Builds the synth and loads the soundfont. Runs on the loader thread.
fn build_synth(sample_rate: f32, soundfont_path: &Path) -> LoaderResult {
    let mut synth = Synth::new(SynthDescriptor {
        sample_rate,
        audio_channels: 2,
        // One group per MIDI channel plus one for reverb and chorus, so
        // each track's sound can run through its own plug-in chain.
        audio_groups: SYNTH_GROUPS,
        gain: 0.35,
        ..Default::default()
    })
    .map_err(|err| err.to_string())?;

    let mut file = File::open(soundfont_path)
        .map_err(|err| format!("{}: {err}", soundfont_path.display()))?;
    let font = SoundFont::load(&mut file).map_err(|err| err.to_string())?;
    synth.add_font(font, true);
    Ok(Box::new(synth))
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "unknown panic".to_owned()
    }
}

/// Loads the soundfont off the UI thread. The SF3 decoder `unwrap()`s on
/// malformed samples, so a panic is caught and reported as an ordinary error
/// instead of taking the whole application down.
fn spawn_synth_loader(sample_rate: f32, soundfont_path: PathBuf) -> mpsc::Receiver<LoaderResult> {
    let (sender, receiver) = mpsc::channel();
    let _ = std::thread::Builder::new()
        .name("betterwriter-soundfont".to_owned())
        .stack_size(LOADER_STACK_BYTES)
        .spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                build_synth(sample_rate, &soundfont_path)
            }))
            .unwrap_or_else(|payload| {
                Err(format!(
                    "the soundfont decoder crashed ({})",
                    panic_message(payload.as_ref())
                ))
            });
            let _ = sender.send(result);
        });
    receiver
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    sample_rate: f32,
    consumer: HeapCons<AudioCommand>,
    flags: Arc<SharedFlags>,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + FromSample<f32>,
{
    let mut state = CallbackState::new(consumer, sample_rate, flags.clone());
    let lost = flags.clone();
    let err_fn = move |err: cpal::StreamError| {
        eprintln!("BetterWriter audio stream error: {err}");
        // An underrun is a glitch the stream survives; anything else leaves
        // it dead until it is rebuilt.
        if !matches!(err, cpal::StreamError::BufferUnderrun) {
            lost.stream_lost.store(true, Ordering::Relaxed);
        }
    };

    Ok(device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            flush_denormals();
            // A panic must never unwind into the platform audio thread.
            let finished = catch_unwind(AssertUnwindSafe(|| {
                if !state.faulted {
                    write_data(data, channels, &mut state);
                }
            }));
            if finished.is_err() {
                state.faulted = true;
                flags.playing.store(false, Ordering::Relaxed);
                flags.faulted.store(true, Ordering::Relaxed);
            }
            if state.faulted {
                data.fill(T::from_sample(0.0_f32));
            }
        },
        err_fn,
        None,
    )?)
}

/// Makes the calling thread treat denormal floats as zero. Reverbs and
/// filters decay into denormals, which are many times slower to compute, and
/// the audio thread then misses its deadline and crackles. DAWs set this on
/// their audio threads; plug-ins rely on it.
fn flush_denormals() {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: reads and writes the thread's own MXCSR through valid pointers;
    // bits 6 (denormals are zero) and 15 (flush to zero) only change rounding
    // of denormal results.
    unsafe {
        let mut csr: u32 = 0;
        std::arch::asm!("stmxcsr [{}]", in(reg) &mut csr, options(nostack));
        csr |= 0x8040;
        std::arch::asm!("ldmxcsr [{}]", in(reg) &csr, options(nostack, readonly));
    }
    #[cfg(target_arch = "aarch64")]
    // SAFETY: sets the flush-to-zero bit (24) of the thread's FPCR.
    unsafe {
        let mut fpcr: u64;
        std::arch::asm!("mrs {}, fpcr", out(reg) fpcr, options(nostack, nomem));
        fpcr |= 1 << 24;
        std::arch::asm!("msr fpcr, {}", in(reg) fpcr, options(nostack, nomem));
    }
}

struct CallbackState {
    command_consumer: HeapCons<AudioCommand>,
    /// `None` until the loader thread hands the synth over; the stream
    /// outputs silence in the meantime.
    synth: Option<Box<Synth>>,
    events: Arc<Vec<Scheduled>>,
    next_event_index: usize,
    playing: bool,
    sample_rate: f32,
    /// Position of the clock in samples, counted from the start of the song.
    /// A play that starts mid-song begins at its start moment's offset.
    samples_since_start: u64,
    end_seconds: f64,
    flags: Arc<SharedFlags>,
    faulted: bool,
    graph: Arc<Graph>,
    /// Channels whose chain holds an instrument plug-in: the soundfont is
    /// silent there and the notes go to the plug-in.
    instrument_channels: [bool; 16],
    /// Volume and pan of each channel, for the instrument plug-ins' output.
    channel_mix: [(u8, u8); 16],
    /// Notes for the plug-ins in the block being rendered, per channel.
    notes: [Vec<NoteEvent>; 16],
    /// Instrument notes that were started and not yet ended, so a stop can
    /// end them.
    sounding: Vec<(u8, u8)>,
    block_left: [f32; BLOCK],
    block_right: [f32; BLOCK],
    block_pos: usize,
}

impl CallbackState {
    fn new(
        command_consumer: HeapCons<AudioCommand>,
        sample_rate: f32,
        flags: Arc<SharedFlags>,
    ) -> Self {
        Self {
            command_consumer,
            synth: None,
            events: Arc::new(Vec::new()),
            next_event_index: 0,
            playing: false,
            sample_rate,
            samples_since_start: 0,
            end_seconds: 0.0,
            flags,
            faulted: false,
            graph: Arc::new(Graph::default()),
            instrument_channels: [false; 16],
            channel_mix: [(100, 64); 16],
            notes: std::array::from_fn(|_| Vec::with_capacity(64)),
            sounding: Vec::with_capacity(256),
            block_left: [0.0; BLOCK],
            block_right: [0.0; BLOCK],
            block_pos: BLOCK,
        }
    }

    fn set_graph(&mut self, graph: Arc<Graph>) {
        self.end_instrument_notes();
        self.instrument_channels = std::array::from_fn(|channel| {
            graph.channels[channel].iter().any(|plugin| plugin.is_instrument)
        });
        self.graph = graph;
    }

    /// Ends every note an instrument plug-in is still holding, at the start
    /// of the next block.
    fn end_instrument_notes(&mut self) {
        for (channel, key) in self.sounding.drain(..) {
            self.notes[usize::from(channel)].push(NoteEvent {
                offset: 0,
                on: false,
                channel: 0,
                key,
                velocity: 0.0,
            });
        }
    }

    fn all_notes_off(&mut self) {
        if let Some(synth) = self.synth.as_mut() {
            for channel in 0..16 {
                let _ = synth.send_event(MidiEvent::AllNotesOff { channel });
            }
        }
    }

    fn apply_channels(&mut self, setups: &[ChannelSetup]) {
        if let Some(synth) = self.synth.as_mut() {
            for setup in setups {
                for event in setup_events(*setup) {
                    let _ = synth.send_event(event);
                }
            }
        }
        for setup in setups {
            self.channel_mix[usize::from(setup.channel.min(15))] = (setup.volume, setup.pan);
        }
    }

    /// Ends the transport but leaves the synth alone, so releases, reverb
    /// and delay ring out.
    fn finish_playback(&mut self) {
        self.playing = false;
        self.flags.playing.store(false, Ordering::Relaxed);
    }

    fn stop_playback(&mut self) {
        self.finish_playback();
        self.all_notes_off();
        self.end_instrument_notes();
    }

    fn drain_commands(&mut self) {
        while let Some(command) = self.command_consumer.try_pop() {
            match command {
                AudioCommand::InstallSynth(synth) => {
                    self.synth = Some(synth);
                }
                AudioCommand::SetGraph(graph) => self.set_graph(graph),
                AudioCommand::Play {
                    events,
                    start_seconds,
                    end_seconds,
                    channels,
                } => {
                    self.all_notes_off();
                    self.end_instrument_notes();
                    self.apply_channels(&channels);
                    self.events = events;
                    self.next_event_index = 0;
                    self.samples_since_start =
                        (start_seconds * f64::from(self.sample_rate)).ceil() as u64;
                    self.end_seconds = end_seconds;
                    self.playing = true;
                    self.flags.playing.store(true, Ordering::Relaxed);
                }
                AudioCommand::SetChannels(channels) => self.apply_channels(&channels),
                AudioCommand::Live { channel, key, velocity } => {
                    let action = if velocity > 0 {
                        Action::NoteOn { key, velocity }
                    } else {
                        Action::NoteOff { key }
                    };
                    self.dispatch(Scheduled { seconds: 0.0, channel, action }, 0);
                }
                AudioCommand::Stop => self.stop_playback(),
            }
        }
    }

    /// The next frame of audio. A block of `BLOCK` frames is rendered at a
    /// time, because the synth and the plug-ins both work on blocks.
    fn next_frame(&mut self) -> (f32, f32) {
        self.drain_commands();
        if self.block_pos >= BLOCK {
            self.render_block();
        }
        let frame = (self.block_left[self.block_pos], self.block_right[self.block_pos]);
        self.block_pos += 1;
        frame
    }

    /// Advances the song clock by one block, sending the notes and the
    /// volume and pan changes that fall in it to the synth and to instrument
    /// plug-ins.
    fn advance_clock(&mut self) {
        for offset in 0..BLOCK {
            if !self.playing {
                break;
            }
            let seconds = self.current_position();
            self.flags.position.store(seconds.to_bits(), Ordering::Relaxed);
            while self
                .events
                .get(self.next_event_index)
                .is_some_and(|event| event.seconds <= seconds)
            {
                let event = self.events[self.next_event_index];
                self.next_event_index += 1;
                self.dispatch(event, offset as u32);
            }
            if seconds >= self.end_seconds && self.next_event_index >= self.events.len() {
                self.finish_playback();
            }
            self.samples_since_start += 1;
        }
    }

    fn dispatch(&mut self, event: Scheduled, offset: u32) {
        let channel = usize::from(event.channel.min(15));
        let instrument = self.instrument_channels[channel];
        match event.action {
            Action::NoteOn { key, velocity } => {
                if instrument {
                    self.notes[channel].push(NoteEvent {
                        offset,
                        on: true,
                        channel: 0,
                        key,
                        velocity: f32::from(velocity) / 127.0,
                    });
                    let held = (channel as u8, key);
                    if !self.sounding.contains(&held) {
                        self.sounding.push(held);
                    }
                } else if let Some(synth) = self.synth.as_mut() {
                    let _ = synth.send_event(MidiEvent::NoteOn {
                        channel: event.channel,
                        key,
                        vel: velocity,
                    });
                }
            }
            Action::NoteOff { key } => {
                if instrument {
                    self.notes[channel].push(NoteEvent {
                        offset,
                        on: false,
                        channel: 0,
                        key,
                        velocity: 0.0,
                    });
                    let held = (channel as u8, key);
                    self.sounding.retain(|note| *note != held);
                } else if let Some(synth) = self.synth.as_mut() {
                    let _ = synth.send_event(MidiEvent::NoteOff {
                        channel: event.channel,
                        key,
                    });
                }
            }
            Action::Volume(value) => {
                self.channel_mix[channel].0 = value;
                self.send_controller(event.channel, 7, value);
            }
            Action::Pan(value) => {
                self.channel_mix[channel].1 = value;
                self.send_controller(event.channel, 10, value);
            }
            Action::Sound { bank, program } => {
                if let Some(synth) = self.synth.as_mut() {
                    for event in sound_events(event.channel, bank, program) {
                        let _ = synth.send_event(event);
                    }
                }
            }
        }
    }

    /// An instrument plug-in channel has no soundfont sound; its volume and
    /// pan are applied from `channel_mix` after the plug-in. The synth gets
    /// the controller either way, which is harmless for a silent channel.
    fn send_controller(&mut self, channel: u8, ctrl: u8, value: u8) {
        if let Some(synth) = self.synth.as_mut() {
            let _ = synth.send_event(MidiEvent::ControlChange {
                channel,
                ctrl,
                value,
            });
        }
    }

    /// Renders one block: the soundfont per channel, each channel's chain,
    /// the mix, the master chain.
    fn render_block(&mut self) {
        self.advance_clock();

        let mut left = [0.0_f32; BLOCK];
        let mut right = [0.0_f32; BLOCK];
        let graph = self.graph.clone();
        match self.synth.as_mut() {
            Some(synth) => {
                synth.render_groups();
                for channel in 0..16 {
                    let chain = &graph.channels[channel];
                    let (mut dry_left, mut dry_right) = if self.instrument_channels[channel] {
                        // The soundfont stays out; the plug-in is the sound.
                        ([0.0_f32; BLOCK], [0.0_f32; BLOCK])
                    } else {
                        let (source_left, source_right) = synth.group_block(channel);
                        (*source_left, *source_right)
                    };
                    run_chain(
                        chain,
                        &mut dry_left,
                        &mut dry_right,
                        &mut self.notes[channel],
                        self.channel_mix[channel],
                    );
                    for index in 0..BLOCK {
                        left[index] += dry_left[index];
                        right[index] += dry_right[index];
                    }
                }
                let (send_left, send_right) = synth.group_block(usize::from(SYNTH_GROUPS) - 1);
                for index in 0..BLOCK {
                    left[index] += send_left[index];
                    right[index] += send_right[index];
                }
            }
            None => {
                // No soundfont yet: instrument plug-ins can still play.
                for channel in 0..16 {
                    if graph.channels[channel].is_empty() {
                        continue;
                    }
                    let (mut dry_left, mut dry_right) = ([0.0_f32; BLOCK], [0.0_f32; BLOCK]);
                    run_chain(
                        &graph.channels[channel],
                        &mut dry_left,
                        &mut dry_right,
                        &mut self.notes[channel],
                        self.channel_mix[channel],
                    );
                    for index in 0..BLOCK {
                        left[index] += dry_left[index];
                        right[index] += dry_right[index];
                    }
                }
            }
        }
        for notes in &mut self.notes {
            notes.clear();
        }

        for plugin in graph.master.iter().filter(|plugin| !plugin.is_instrument) {
            if let Ok(mut instance) = plugin.instance.try_lock() {
                instance.process(&mut left, &mut right, &[]);
            }
        }
        self.block_left = left;
        self.block_right = right;
        self.block_pos = 0;
    }

    /// Seconds since the start of the song.
    fn current_position(&self) -> f64 {
        self.samples_since_start as f64 / f64::from(self.sample_rate)
    }
}

/// Runs a channel's plug-ins over its block, in order. An instrument takes
/// the channel's notes and replaces the sound; an effect changes it. A
/// plug-in the UI is busy with this instant is skipped for the block.
fn run_chain(
    chain: &[Arc<LivePlugin>],
    left: &mut [f32; BLOCK],
    right: &mut [f32; BLOCK],
    notes: &mut Vec<NoteEvent>,
    (volume, pan): (u8, u8),
) {
    for plugin in chain {
        let Ok(mut instance) = plugin.instance.try_lock() else {
            if plugin.is_instrument {
                left.fill(0.0);
                right.fill(0.0);
            }
            continue;
        };
        if plugin.is_instrument {
            instance.process(left, right, notes);
            // The soundfont has its volume and pan inside the synth; an
            // instrument plug-in gets the track's here.
            let gain = f32::from(volume.min(127)) / 127.0;
            let pan = f32::from(pan.min(127));
            let left_gain = gain * if pan <= 64.0 { 1.0 } else { (127.0 - pan) / 63.0 };
            let right_gain = gain * if pan >= 64.0 { 1.0 } else { pan / 64.0 };
            for index in 0..BLOCK {
                left[index] *= left_gain;
                right[index] *= right_gain;
            }
        } else {
            instance.process(left, right, &[]);
        }
    }
}

fn write_data<T>(output: &mut [T], channels: usize, state: &mut CallbackState)
where
    T: Sample + FromSample<f32>,
{
    for frame in output.chunks_mut(channels.max(1)) {
        let (left, right) = state.next_frame();
        for (index, sample) in frame.iter_mut().enumerate() {
            let value = if index % 2 == 0 { left } else { right };
            *sample = T::from_sample(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{MidiEventKind, TICKS_PER_QUARTER};

    fn flags() -> Arc<SharedFlags> {
        Arc::new(SharedFlags::default())
    }

    fn state_with_queue() -> (HeapProd<AudioCommand>, CallbackState, Arc<SharedFlags>) {
        let (producer, consumer) = HeapRb::<AudioCommand>::new(16).split();
        let flags = flags();
        let state = CallbackState::new(consumer, 1_000.0, flags.clone());
        (producer, state, flags)
    }

    fn one_note(on: f64, off: f64) -> Arc<Vec<Scheduled>> {
        Arc::new(vec![
            Scheduled {
                seconds: on,
                channel: 0,
                action: Action::NoteOn { key: 60, velocity: 100 },
            },
            Scheduled {
                seconds: off,
                channel: 0,
                action: Action::NoteOff { key: 60 },
            },
        ])
    }

    fn no_channels() -> Arc<Vec<ChannelSetup>> {
        Arc::new(Vec::new())
    }

    fn play_command(
        events: Arc<Vec<Scheduled>>,
        start_seconds: f64,
        end_seconds: f64,
        channels: Arc<Vec<ChannelSetup>>,
    ) -> AudioCommand {
        AudioCommand::Play {
            events,
            start_seconds,
            end_seconds,
            channels,
        }
    }

    #[test]
    fn silent_until_synth_installed_and_play_sets_flag() {
        let (mut producer, mut state, flags) = state_with_queue();
        assert_eq!(state.next_frame(), (0.0, 0.0));
        assert!(!flags.playing.load(Ordering::Relaxed));

        let _ = producer.try_push(play_command(one_note(0.0, 0.01), 0.0, 1.0, no_channels()));
        state.next_frame();
        assert!(flags.playing.load(Ordering::Relaxed));
    }

    #[test]
    fn playback_that_starts_elsewhere_does_not_show_where_it_stopped() {
        let mut audio = AudioRuntime::new("none.sf2");
        // Stopped at 7 s earlier.
        audio.flags.position.store(7.0_f64.to_bits(), Ordering::Relaxed);
        let schedule = Schedule { events: Vec::new(), start_seconds: 2.0, end_seconds: 9.0 };
        let _ = audio.send_play(PendingPlay { schedule });
        // Before the audio thread has taken the command.
        assert_eq!(audio.playback_seconds(), Some(2.0));
    }

    #[test]
    fn stop_command_clears_playing_flag() {
        let (mut producer, mut state, flags) = state_with_queue();
        let _ = producer.try_push(play_command(one_note(0.0, 0.01), 0.0, 1.0, no_channels()));
        state.next_frame();
        let _ = producer.try_push(AudioCommand::Stop);
        state.next_frame();
        assert!(!flags.playing.load(Ordering::Relaxed));
    }

    #[test]
    fn song_finishes_by_itself_when_the_clock_reaches_the_end() {
        let (mut producer, mut state, flags) = state_with_queue();
        let _ = producer.try_push(play_command(one_note(0.0, 0.01), 0.0, 1.0, no_channels()));
        // 1 kHz sample rate: the end is 1 s, i.e. 1000 frames.
        for _ in 0..1_100 {
            state.next_frame();
        }
        assert!(!flags.playing.load(Ordering::Relaxed));
        assert!(!state.playing);
    }

    #[test]
    fn the_position_follows_the_clock_between_notes() {
        let (mut producer, mut state, flags) = state_with_queue();
        let _ = producer.try_push(play_command(one_note(0.0, 0.01), 0.0, 4.0, no_channels()));
        for _ in 0..500 {
            state.next_frame();
        }
        let position = f64::from_bits(flags.position.load(Ordering::Relaxed));
        // The position is stamped per frame while a block of 64 is played
        // ahead of the 500 frames asked for.
        assert!((position - 0.5).abs() <= 64.0 / 1_000.0, "{position}");
    }

    #[test]
    fn song_keeps_playing_until_the_end() {
        let (mut producer, mut state, flags) = state_with_queue();
        let _ = producer.try_push(play_command(one_note(0.0, 0.01), 0.0, 1.0, no_channels()));
        for _ in 0..500 {
            state.next_frame();
        }
        assert!(flags.playing.load(Ordering::Relaxed));
    }

    #[test]
    fn play_from_a_later_moment_sounds_without_waiting_for_the_skipped_time() {
        let (mut producer, mut state, _flags) = state_with_queue();
        let _ = producer.try_push(play_command(one_note(4.0, 4.5), 4.0, 4.5, no_channels()));
        for _ in 0..10 {
            state.next_frame();
        }
        assert_eq!(state.next_event_index, 1);
        assert!(state.playing);
    }

    #[test]
    fn play_past_the_last_event_finishes_at_once() {
        let (mut producer, mut state, flags) = state_with_queue();
        let _ = producer.try_push(play_command(one_note(0.0, 0.01), 50.0, 50.0, no_channels()));
        state.next_frame();
        assert!(!flags.playing.load(Ordering::Relaxed));
    }

    #[test]
    fn volume_and_pan_changes_reach_the_mix_when_they_fall_due() {
        let (mut producer, mut state, _flags) = state_with_queue();
        let events = Arc::new(vec![
            Scheduled { seconds: 0.1, channel: 2, action: Action::Volume(30) },
            Scheduled { seconds: 0.2, channel: 2, action: Action::Pan(10) },
        ]);
        let _ = producer.try_push(play_command(events, 0.0, 1.0, no_channels()));
        for _ in 0..150 {
            state.next_frame();
        }
        assert_eq!(state.channel_mix[2].0, 30);
        assert_ne!(state.channel_mix[2].1, 10, "the pan is not due yet");
        for _ in 0..250 {
            state.next_frame();
        }
        assert_eq!(state.channel_mix[2], (30, 10));
    }

    fn event(track_id: u64, tick: Tick, kind: MidiEventKind) -> MidiPlaybackEvent {
        MidiPlaybackEvent {
            tick,
            kind,
            track_id,
            channel: track_id as u8 - 1,
            key: 60,
            velocity: 100,
            sound: None,
        }
    }

    fn notes_for_both_tracks(project: &BwxProject) -> Vec<MidiPlaybackEvent> {
        let (a, b) = (project.tracks[0].id, project.tracks[1].id);
        vec![
            event(a, TICKS_PER_QUARTER, MidiEventKind::NoteOn),
            event(a, TICKS_PER_QUARTER * 2, MidiEventKind::NoteOff),
            event(b, TICKS_PER_QUARTER, MidiEventKind::NoteOn),
            event(b, TICKS_PER_QUARTER * 2, MidiEventKind::NoteOff),
        ]
    }

    fn on_times(schedule: &Schedule) -> Vec<(u8, f64)> {
        schedule
            .events
            .iter()
            .filter(|event| matches!(event.action, Action::NoteOn { .. }))
            .map(|event| (event.channel, event.seconds))
            .collect()
    }

    #[test]
    fn one_tempo_curve_times_every_track_alike() {
        let project = BwxProject::demo();
        let notes = notes_for_both_tracks(&project);
        let schedule = build_schedule(&project, &notes, 0, project.tracks[0].id);
        // One quarter at 120 bpm.
        let times = on_times(&schedule);
        assert_eq!(times.len(), 2);
        assert!(times.iter().all(|&(_, seconds)| (seconds - 0.5).abs() < 1e-9));
    }

    #[test]
    fn per_track_tempo_times_each_track_on_its_own_curve() {
        let mut project = BwxProject::demo();
        let (a, b) = (project.tracks[0].id, project.tracks[1].id);
        project.set_per_track_automation(true);
        project.set_automation_point(
            AutomationLane::Tempo,
            project.scope_for(b),
            0,
            60.0,
            crate::core::TempoTransition::Constant,
        );
        let notes = notes_for_both_tracks(&project);
        let schedule = build_schedule(&project, &notes, 0, a);
        let mut times = on_times(&schedule);
        times.sort_by(|x, y| x.1.total_cmp(&y.1));
        assert!((times[0].1 - 0.5).abs() < 1e-9, "120 bpm: {times:?}");
        assert!((times[1].1 - 1.0).abs() < 1e-9, "60 bpm: {times:?}");
    }

    #[test]
    fn the_start_moment_is_read_on_the_start_tracks_curve() {
        let mut project = BwxProject::demo();
        let (a, b) = (project.tracks[0].id, project.tracks[1].id);
        project.set_per_track_automation(true);
        project.set_automation_point(
            AutomationLane::Tempo,
            project.scope_for(b),
            0,
            60.0,
            crate::core::TempoTransition::Constant,
        );
        let notes = notes_for_both_tracks(&project);
        // One quarter in on the 60 bpm track is 1 s; the other track's note
        // at 0.5 s is behind it and is skipped.
        let schedule = build_schedule(&project, &notes, TICKS_PER_QUARTER, b);
        assert!((schedule.start_seconds - 1.0).abs() < 1e-9);
        let times = on_times(&schedule);
        assert_eq!(times.len(), 1, "{times:?}");
        assert_eq!(times[0].0, project.tracks[1].channel);
        assert!(schedule.events.iter().all(|event| event.seconds >= 1.0));
        let _ = a;
    }

    fn sound_changes(schedule: &Schedule, channel: u8) -> Vec<(f64, u16, u8)> {
        schedule
            .events
            .iter()
            .filter(|event| event.channel == channel)
            .filter_map(|event| match event.action {
                Action::Sound { bank, program } => Some((event.seconds, bank, program)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_muted_note_gets_its_preset_and_the_next_plain_note_the_tracks_own() {
        let project = BwxProject::demo();
        let track = &project.tracks[0];
        let (id, channel, base) = (track.id, track.channel, track.sound());
        let note = |tick: Tick, kind, sound| MidiPlaybackEvent {
            tick,
            kind,
            track_id: id,
            channel,
            key: 60,
            velocity: 100,
            sound,
        };
        let q = TICKS_PER_QUARTER;
        let notes = [
            note(0, MidiEventKind::NoteOn, None),
            note(q, MidiEventKind::NoteOff, None),
            note(q, MidiEventKind::NoteOn, Some((0, 28))),
            note(q * 2, MidiEventKind::NoteOff, None),
            note(q * 2, MidiEventKind::NoteOn, Some((0, 28))),
            note(q * 3, MidiEventKind::NoteOff, None),
            note(q * 3, MidiEventKind::NoteOn, None),
            note(q * 4, MidiEventKind::NoteOff, None),
        ];
        let schedule = build_schedule(&project, &notes, 0, id);
        let changes = sound_changes(&schedule, channel);
        assert_eq!(
            changes,
            [(0.5, 0, 28), (1.0, 0, 28), (1.5, base.0, base.1)]
        );
        // The change lands before the note it is for.
        let at = |action: fn(&Action) -> bool| {
            schedule.events.iter().position(|e| e.seconds == 0.5 && action(&e.action)).unwrap()
        };
        assert!(at(|a| matches!(a, Action::Sound { .. })) < at(|a| matches!(a, Action::NoteOn { .. })));
        // Starting after the muted notes begins with the track's own sound.
        let later = build_schedule(&project, &notes, q * 3, id);
        assert!(sound_changes(&later, channel).is_empty());
        let muted_later = build_schedule(&project, &notes, q * 2, id);
        assert_eq!(sound_changes(&muted_later, channel)[0], (1.0, 0, 28));
    }

    #[test]
    fn automation_becomes_controller_changes_on_the_tracks_channel() {
        let mut project = BwxProject::demo();
        let a = project.tracks[0].id;
        let scope = project.scope_for(a);
        let hold = crate::core::TempoTransition::Constant;
        project.set_automation_point(AutomationLane::Volume, scope, 0, 100.0, hold);
        project.set_automation_point(AutomationLane::Volume, scope, TICKS_PER_QUARTER * 2, 40.0, hold);
        project.set_automation_point(AutomationLane::Pan, scope, TICKS_PER_QUARTER * 2, -64.0, hold);
        let schedule = build_schedule(&project, &[], 0, a);
        let channel = project.tracks[0].channel;
        let volumes: Vec<(u8, f64)> = schedule
            .events
            .iter()
            .filter_map(|event| match event.action {
                Action::Volume(value) if event.channel == channel => Some((value, event.seconds)),
                _ => None,
            })
            .collect();
        assert_eq!(volumes.len(), 2);
        assert_eq!(volumes[0], (100, 0.0));
        assert_eq!(volumes[1].0, 40);
        assert!((volumes[1].1 - 1.0).abs() < 1e-9);
        assert!(schedule
            .events
            .iter()
            .any(|event| event.action == Action::Pan(0) && event.channel == channel));
    }

    #[test]
    fn starting_mid_song_applies_the_volume_in_force_at_that_moment() {
        let mut project = BwxProject::demo();
        let a = project.tracks[0].id;
        let scope = project.scope_for(a);
        let hold = crate::core::TempoTransition::Constant;
        project.set_automation_point(AutomationLane::Volume, scope, 0, 100.0, hold);
        project.set_automation_point(AutomationLane::Volume, scope, TICKS_PER_QUARTER * 2, 40.0, hold);
        project.set_automation_point(AutomationLane::Volume, scope, TICKS_PER_QUARTER * 8, 90.0, hold);
        let schedule = build_schedule(&project, &[], TICKS_PER_QUARTER * 4, a);
        let channel = project.tracks[0].channel;
        let volumes: Vec<(u8, f64)> = schedule
            .events
            .iter()
            .filter_map(|event| match event.action {
                Action::Volume(value) if event.channel == channel => Some((value, event.seconds)),
                _ => None,
            })
            .collect();
        // The change to 40 is in force at the start; 90 follows later.
        assert_eq!(volumes[0], (40, schedule.start_seconds));
        assert_eq!(volumes[1].0, 90);
        assert_eq!(volumes.len(), 2);
    }

    #[test]
    fn volume_automation_does_not_unmute_a_silent_track() {
        let mut project = BwxProject::demo();
        let a = project.tracks[0].id;
        let scope = project.scope_for(a);
        project.set_automation_point(
            AutomationLane::Volume,
            scope,
            0,
            100.0,
            crate::core::TempoTransition::Constant,
        );
        project.tracks[0].mute = true;
        let schedule = build_schedule(&project, &[], 0, a);
        let volume_on = |channel: u8| {
            schedule
                .events
                .iter()
                .any(|event| event.channel == channel && matches!(event.action, Action::Volume(_)))
        };
        assert!(!volume_on(project.tracks[0].channel));
        assert!(volume_on(project.tracks[1].channel), "the shared curve still reaches the other track");
    }

    fn two_track_project() -> BwxProject {
        let mut project = BwxProject::demo();
        project.tracks[0].channel = 0;
        project.tracks[1].channel = 1;
        project
    }

    #[test]
    fn channel_setups_carry_each_tracks_sound_volume_and_pan() {
        let mut project = two_track_project();
        project.tracks[0].program = 29;
        project.tracks[0].volume = 80;
        project.tracks[0].pan = -20;
        project.tracks[1].program = 33;
        project.tracks[1].pan = 63;
        let setups = channel_setups(&project);
        assert_eq!(setups.len(), 2);
        assert_eq!(
            setups[0],
            ChannelSetup {
                channel: 0,
                bank: 0,
                program: 29,
                volume: 80,
                pan: 44,
            }
        );
        assert_eq!((setups[1].channel, setups[1].program, setups[1].pan), (1, 33, 127));
    }

    #[test]
    fn a_muted_track_is_silent_and_keeps_its_volume_for_unmuting() {
        let mut project = two_track_project();
        project.tracks[0].mute = true;
        project.tracks[0].volume = 90;
        assert_eq!(channel_setups(&project)[0].volume, 0);
        assert_eq!(channel_setups(&project)[1].volume, project.tracks[1].volume);
        project.tracks[0].mute = false;
        assert_eq!(channel_setups(&project)[0].volume, 90);
    }

    #[test]
    fn solo_silences_every_other_track() {
        let mut project = two_track_project();
        project.tracks[1].solo = true;
        let setups = channel_setups(&project);
        assert_eq!(setups[0].volume, 0);
        assert_eq!(setups[1].volume, project.tracks[1].volume);
        project.tracks[0].solo = true;
        assert!(channel_setups(&project).iter().all(|setup| setup.volume > 0));
    }

    #[test]
    fn a_channel_is_set_up_with_its_bank_before_its_program() {
        let events = setup_events(ChannelSetup {
            channel: 3,
            bank: 0,
            program: 35,
            volume: 100,
            pan: 64,
        });
        assert!(events[0] == MidiEvent::ControlChange { channel: 3, ctrl: 0, value: 0 });
        assert!(events[1] == MidiEvent::ControlChange { channel: 3, ctrl: 32, value: 0 });
        assert!(events[2] == MidiEvent::ProgramChange { channel: 3, program_id: 35 });
        assert!(events[3] == MidiEvent::ControlChange { channel: 3, ctrl: 7, value: 100 });
        assert!(events[4] == MidiEvent::ControlChange { channel: 3, ctrl: 10, value: 64 });
    }

    #[test]
    fn a_drum_kit_bank_is_split_over_the_two_bank_controllers() {
        let events = setup_events(ChannelSetup {
            channel: 2,
            bank: 128,
            program: 8,
            volume: 100,
            pan: 64,
        });
        assert!(events[0] == MidiEvent::ControlChange { channel: 2, ctrl: 0, value: 1 });
        assert!(events[1] == MidiEvent::ControlChange { channel: 2, ctrl: 32, value: 0 });
        let events = setup_events(ChannelSetup {
            channel: 2,
            bank: 17,
            program: 16,
            volume: 100,
            pan: 64,
        });
        assert!(events[0] == MidiEvent::ControlChange { channel: 2, ctrl: 0, value: 0 });
        assert!(events[1] == MidiEvent::ControlChange { channel: 2, ctrl: 32, value: 17 });
    }

    #[test]
    fn an_automatic_sound_reaches_the_synth_as_the_instruments_preset() {
        let mut project = two_track_project();
        project.tracks[0].set_instrument(crate::core::instruments::instrument("flute").unwrap());
        project.tracks[0].set_sound_automatic();
        let flute = crate::core::instruments::instrument("flute").unwrap();
        let setup = channel_setups(&project)[0];
        assert_eq!((setup.bank, setup.program), (flute.bank, flute.program));
    }

    #[test]
    fn mix_changes_are_sent_once_and_only_while_the_synth_is_installed() {
        let mut runtime = AudioRuntime::new("unused.sf3");
        let (producer, mut consumer) = HeapRb::<AudioCommand>::new(16).split();
        runtime.producer = producer;
        let mut project = two_track_project();

        runtime.sync_mix(&project);
        assert!(consumer.try_pop().is_none(), "no synth yet, nothing to send");

        runtime.synth_installed = true;
        project.tracks[0].program = 29;
        runtime.sync_mix(&project);
        assert!(matches!(consumer.try_pop(), Some(AudioCommand::SetChannels(_))));
        runtime.sync_mix(&project);
        assert!(consumer.try_pop().is_none(), "an unchanged mix sends nothing");
    }

    #[test]
    fn missing_soundfont_is_an_error_not_a_panic() {
        let result = build_synth(48_000.0, Path::new("definitely/not/here.sf3"));
        assert!(result.is_err());
    }

    #[test]
    fn corrupt_soundfont_is_reported_through_the_loader_channel() {
        let dir = std::env::temp_dir().join(format!("bw-bad-sf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.sf3");
        std::fs::write(&path, b"RIFF....sfbkgarbagegarbagegarbage").unwrap();

        let receiver = spawn_synth_loader(48_000.0, path);
        let result = receiver
            .recv_timeout(std::time::Duration::from_secs(20))
            .expect("loader thread must always answer");
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn panic_messages_are_readable() {
        let caught = catch_unwind(|| panic!("boom")).unwrap_err();
        assert_eq!(panic_message(caught.as_ref()), "boom");
    }

    #[test]
    #[ignore = "slow: loads the real 51 MB soundfont and renders audio"]
    fn real_soundfont_loads_and_renders_a_song() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join("soundfonts")
            .join("MS Basic.sf3");
        let started = std::time::Instant::now();
        let synth = build_synth(48_000.0, &path).expect("soundfont should load");
        eprintln!("soundfont load took {:?}", started.elapsed());

        let (mut producer, consumer) = HeapRb::<AudioCommand>::new(16).split();
        let flags = flags();
        let mut state = CallbackState::new(consumer, 48_000.0, flags.clone());
        let _ = producer.try_push(AudioCommand::InstallSynth(synth));
        let channels = Arc::new(vec![ChannelSetup {
            channel: 0,
            bank: 0,
            program: 33,
            volume: 100,
            pan: 64,
        }]);
        let _ = producer.try_push(play_command(one_note(0.0, 0.25), 0.0, 0.25, channels));
        let started = std::time::Instant::now();
        let mut peak = 0.0_f32;
        for _ in 0..48_000 * 2 {
            let (left, right) = state.next_frame();
            peak = peak.max(left.abs()).max(right.abs());
        }
        eprintln!("rendered 2 s of audio in {:?}, peak {peak}", started.elapsed());
        let preset = state.synth.as_ref().unwrap().channel_preset(0);
        assert_eq!(preset.as_ref().map(|preset| preset.name()), Some("Fingered Bass"));
        assert!(peak > 0.001, "the note should be audible");
    }

    #[test]
    #[ignore = "slow: loads the real 51 MB soundfont"]
    fn real_soundfont_has_every_automatic_and_technique_sound() {
        use crate::core::instruments::INSTRUMENTS;
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join("soundfonts")
            .join("MS Basic.sf3");
        let mut synth = build_synth(48_000.0, &path).expect("soundfont should load");
        let mut missing = Vec::new();
        let mut wanted: Vec<(u16, u8)> = INSTRUMENTS.iter().map(|def| (def.bank, def.program)).collect();
        wanted.extend([(0, 28), (0, 36), (0, 37)]);
        wanted.sort_unstable();
        wanted.dedup();
        for (bank, program) in wanted {
            for event in sound_events(0, bank, program) {
                let _ = synth.send_event(event);
            }
            match synth.channel_preset(0) {
                Some(preset) if bank == 0 && [28, 36, 37].contains(&program) => {
                    eprintln!("{bank}/{program}: {}", preset.name());
                }
                Some(_) => {}
                None => missing.push((bank, program)),
            }
        }
        assert!(missing.is_empty(), "no such preset: {missing:?}");
    }

    /// The test plug-ins come from the vst3 crate's `gain` example (effect)
    /// and a variant of it that plays a sine on note-on (instrument); the
    /// tests run when BW_TEST_GAIN_VST3 / BW_TEST_SYNTH_VST3 name them.
    fn live_plugin(var: &str, id: u64, instrument: bool) -> Option<Arc<LivePlugin>> {
        let path = std::env::var_os(var)?;
        let module = crate::plugins::vst3::Module::load(Path::new(&path)).expect("load");
        let instance = crate::plugins::vst3::Instance::create(&module, "", 1_000.0).expect("create");
        Some(Arc::new(LivePlugin::new(id, instrument, instance)))
    }

    fn play_one_note(state: &mut CallbackState, producer: &mut HeapProd<AudioCommand>, frames: usize) -> f32 {
        let _ = producer.try_push(play_command(
            one_note(0.0, 2.0),
            0.0,
            2.0,
            Arc::new(vec![ChannelSetup { channel: 0, bank: 0, program: 0, volume: 127, pan: 64 }]),
        ));
        let mut peak = 0.0_f32;
        for _ in 0..frames {
            let (left, right) = state.next_frame();
            peak = peak.max(left.abs()).max(right.abs());
        }
        peak
    }

    #[test]
    fn an_instrument_plug_in_gets_the_notes_of_its_channel_and_is_the_only_sound() {
        let Some(synth) = live_plugin("BW_TEST_SYNTH_VST3", 1, true) else { return };
        let (mut producer, mut state, _flags) = state_with_queue();
        let mut graph = Graph::default();
        graph.channels[0].push(synth);
        let _ = producer.try_push(AudioCommand::SetGraph(Arc::new(graph)));
        let peak = play_one_note(&mut state, &mut producer, 400);
        // Velocity 100 of 127 at half scale.
        assert!((peak - 0.5 * 100.0 / 127.0).abs() < 0.02, "{peak}");
        assert_eq!(state.sounding, vec![(0, 60)]);
        let _ = producer.try_push(AudioCommand::Stop);
        for _ in 0..200 {
            state.next_frame();
        }
        assert!(state.sounding.is_empty(), "stop ends the plug-in's notes");
    }

    #[test]
    fn a_muted_track_silences_its_instrument_plug_in() {
        let Some(synth) = live_plugin("BW_TEST_SYNTH_VST3", 1, true) else { return };
        let (mut producer, mut state, _flags) = state_with_queue();
        let mut graph = Graph::default();
        graph.channels[0].push(synth);
        let _ = producer.try_push(AudioCommand::SetGraph(Arc::new(graph)));
        let _ = producer.try_push(play_command(
            one_note(0.0, 2.0),
            0.0,
            2.0,
            Arc::new(vec![ChannelSetup { channel: 0, bank: 0, program: 0, volume: 0, pan: 64 }]),
        ));
        let mut peak = 0.0_f32;
        for _ in 0..400 {
            let (left, right) = state.next_frame();
            peak = peak.max(left.abs()).max(right.abs());
        }
        assert_eq!(peak, 0.0);
    }

    #[test]
    fn effects_on_a_channel_and_on_the_master_change_the_mix() {
        let (Some(synth), Some(gain_a), Some(gain_b)) = (
            live_plugin("BW_TEST_SYNTH_VST3", 1, true),
            live_plugin("BW_TEST_GAIN_VST3", 2, false),
            live_plugin("BW_TEST_GAIN_VST3", 3, false),
        ) else {
            return;
        };
        for plugin in [&gain_a, &gain_b] {
            let handle = plugin.controller_handle().unwrap();
            handle.set(0, 0.5);
        }
        let (mut producer, mut state, _flags) = state_with_queue();
        let mut graph = Graph::default();
        graph.channels[0].push(synth);
        graph.channels[0].push(gain_a);
        graph.master.push(gain_b);
        let _ = producer.try_push(AudioCommand::SetGraph(Arc::new(graph)));
        let peak = play_one_note(&mut state, &mut producer, 600);
        // Half twice over.
        let expected = 0.5 * 100.0 / 127.0 * 0.25;
        assert!((peak - expected).abs() < 0.02, "{peak} vs {expected}");
    }

    #[test]
    #[ignore = "slow: loads the real 51 MB soundfont"]
    fn an_instrument_plug_in_leaves_no_soundfont_sound_behind() {
        if live_plugin("BW_TEST_SYNTH_VST3", 1, true).is_none() {
            return;
        }
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join("soundfonts")
            .join("MS Basic.sf3");
        let run = |with_soundfont: bool| {
            let (mut producer, consumer) = HeapRb::<AudioCommand>::new(16).split();
            let mut state = CallbackState::new(consumer, 48_000.0, flags());
            if with_soundfont {
                let _ = producer.try_push(AudioCommand::InstallSynth(
                    build_synth(48_000.0, &path).expect("soundfont"),
                ));
            }
            let mut graph = Graph::default();
            // A fresh plug-in each run, so both start from the same phase.
            graph.channels[0].push(live_plugin("BW_TEST_SYNTH_VST3", 1, true).unwrap());
            let _ = producer.try_push(AudioCommand::SetGraph(Arc::new(graph)));
            let _ = producer.try_push(play_command(
                one_note(0.0, 0.25),
                0.0,
                0.25,
                Arc::new(vec![ChannelSetup { channel: 0, bank: 0, program: 33, volume: 127, pan: 64 }]),
            ));
            (0..48_000).map(|_| state.next_frame()).collect::<Vec<_>>()
        };
        let with = run(true);
        let without = run(false);
        let worst = with
            .iter()
            .zip(&without)
            .map(|(a, b)| (a.0 - b.0).abs().max((a.1 - b.1).abs()))
            .fold(0.0_f32, f32::max);
        assert!(worst < 1e-4, "the soundfont leaked into the plug-in's channel: {worst}");
    }
}
