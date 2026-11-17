use std::{
    fs::File,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, TryRecvError},
    },
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
    BwxProject, MidiEventKind, MidiPlaybackEvent, TICKS_PER_QUARTER, TempoTransition, Tick,
    sounds::midi_pan,
};

/// One constant-rate playback segment derived from the project's tempo
/// automation. `Progressive` points use the average of their start/end bpm
/// as the segment's rate - a good approximation for audio scheduling; the
/// automation graph (in the UI) still shows the exact linear ramp. See
/// `build_tempo_segments`.
#[derive(Clone, Copy)]
struct TempoSegment {
    start_tick: Tick,
    start_seconds: f64,
    seconds_per_tick: f64,
}

fn seconds_per_tick_for(bpm: f64) -> f64 {
    60.0 / (bpm.max(1.0) * TICKS_PER_QUARTER as f64)
}

/// Converts the project's tempo automation timeline into a sequence of
/// constant-rate segments anchored to real elapsed seconds, so playback can
/// convert "seconds since play started" into a musical tick position without
/// assuming a single flat tempo for the whole piece.
fn build_tempo_segments(project: &BwxProject) -> Vec<TempoSegment> {
    let mut points: Vec<_> = project.tempo_points.iter().collect();
    points.sort_by_key(|point| point.at_tick);
    if points.is_empty() {
        return vec![TempoSegment {
            start_tick: 0,
            start_seconds: 0.0,
            seconds_per_tick: seconds_per_tick_for(120.0),
        }];
    }

    let mut segments = Vec::with_capacity(points.len());
    let mut elapsed_seconds = 0.0_f64;
    for (index, point) in points.iter().enumerate() {
        let next = points.get(index + 1);
        let rate_bpm = if point.transition == TempoTransition::Progressive {
            match next {
                Some(next) => (point.bpm as f64 + next.bpm as f64) / 2.0,
                None => point.bpm as f64,
            }
        } else {
            point.bpm as f64
        };
        let seconds_per_tick = seconds_per_tick_for(rate_bpm);
        segments.push(TempoSegment {
            start_tick: point.at_tick,
            start_seconds: elapsed_seconds,
            seconds_per_tick,
        });
        if let Some(next) = next {
            let tick_span = (next.at_tick - point.at_tick).max(0) as f64;
            elapsed_seconds += tick_span * seconds_per_tick;
        }
    }
    segments
}

/// Index of the tempo segment containing `tick` and the elapsed seconds at
/// that tick, so playback can start partway into the song on the same clock
/// a play from tick 0 would have reached it on.
fn locate_tick(segments: &[TempoSegment], tick: Tick) -> (usize, f64) {
    let index = segments
        .iter()
        .rposition(|segment| segment.start_tick <= tick)
        .unwrap_or(0);
    let Some(segment) = segments.get(index) else {
        return (0, 0.0);
    };
    let ticks_in = (tick - segment.start_tick).max(0) as f64;
    (index, segment.start_seconds + ticks_in * segment.seconds_per_tick)
}

/// Synth state for one MIDI channel, taken from the track that plays on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelSetup {
    channel: u8,
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
            ChannelSetup {
                channel: track.channel.min(15),
                program: track.program.min(127),
                volume: if silent { 0 } else { track.volume.min(127) },
                pan: midi_pan(track.pan),
            }
        })
        .collect()
}

/// Events that put a channel into a setup. Bank 0 is selected explicitly so
/// a program number always means the General MIDI sound.
fn setup_events(setup: ChannelSetup) -> [MidiEvent; 5] {
    let channel = setup.channel;
    [
        MidiEvent::ControlChange {
            channel,
            ctrl: 0,
            value: 0,
        },
        MidiEvent::ControlChange {
            channel,
            ctrl: 32,
            value: 0,
        },
        MidiEvent::ProgramChange {
            channel,
            program_id: setup.program,
        },
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

/// Stack size for the soundfont loader thread. Decoding an SF3 allocates and
/// recurses far more than a default thread stack comfortably allows.
const LOADER_STACK_BYTES: usize = 64 * 1024 * 1024;

enum AudioCommand {
    InstallSynth(Box<Synth>),
    /// The plug-in chains to run; replaces the previous graph.
    SetGraph(Arc<Graph>),
    Play {
        events: Arc<Vec<MidiPlaybackEvent>>,
        tempo_segments: Arc<Vec<TempoSegment>>,
        start_tick: Tick,
        /// Playback counts as finished once the clock reaches this tick.
        end_tick: Tick,
        channels: Arc<Vec<ChannelSetup>>,
    },
    /// Applies new channel setups without touching playback.
    SetChannels(Arc<Vec<ChannelSetup>>),
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
    /// The song position in ticks, as `f64` bits, kept current while playing.
    position: AtomicU64,
    /// Set when the audio callback panicked; the runtime rebuilds the stream
    /// the next time playback is requested.
    faulted: AtomicBool,
}

struct PendingPlay {
    events: Vec<MidiPlaybackEvent>,
    tempo_segments: Vec<TempoSegment>,
    start_tick: Tick,
    end_tick: Tick,
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

    /// Where the song is right now, in ticks, while it plays. The position
    /// follows the tempo clock continuously, not the notes.
    pub fn playback_tick(&self) -> Option<f64> {
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

    /// Starts playback at `start_tick`; earlier events are skipped and the
    /// tempo clock begins at that tick's position in the tempo map.
    pub fn play(
        &mut self,
        project: &BwxProject,
        events: Vec<MidiPlaybackEvent>,
        start_tick: Tick,
    ) -> Result<(), AudioError> {
        self.ensure_stream()?;
        self.mix = channel_setups(project);
        let last_event_tick = events.iter().map(|event| event.tick).max().unwrap_or(0);
        let pending = PendingPlay {
            end_tick: project
                .playback_end_tick()
                .unwrap_or(0)
                .max(last_event_tick),
            events,
            tempo_segments: build_tempo_segments(project),
            start_tick,
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
        self.flags.playing.store(true, Ordering::Relaxed);
        self.producer
            .try_push(AudioCommand::Play {
                events: Arc::new(pending.events),
                tempo_segments: Arc::new(pending.tempo_segments),
                start_tick: pending.start_tick,
                end_tick: pending.end_tick,
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
    let err_fn = |err| eprintln!("BetterWriter audio stream error: {err}");

    Ok(device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
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

struct CallbackState {
    command_consumer: HeapCons<AudioCommand>,
    /// `None` until the loader thread hands the synth over; the stream
    /// outputs silence in the meantime.
    synth: Option<Box<Synth>>,
    events: Arc<Vec<MidiPlaybackEvent>>,
    next_event_index: usize,
    playing: bool,
    tempo_segments: Arc<Vec<TempoSegment>>,
    /// Monotonically advances forward through `tempo_segments` as playback
    /// progresses, mirroring `next_event_index`'s incremental scan - cheap
    /// per audio frame even with many automation points.
    current_segment_index: usize,
    sample_rate: f32,
    /// Position on the tempo clock in samples, counted from tick 0 of the
    /// song. A play that starts mid-song begins at its start tick's offset.
    samples_since_start: u64,
    end_tick: Tick,
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
            tempo_segments: Arc::new(vec![TempoSegment {
                start_tick: 0,
                start_seconds: 0.0,
                seconds_per_tick: seconds_per_tick_for(120.0),
            }]),
            current_segment_index: 0,
            sample_rate,
            samples_since_start: 0,
            end_tick: 0,
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
                    tempo_segments,
                    start_tick,
                    end_tick,
                    channels,
                } => {
                    self.all_notes_off();
                    self.end_instrument_notes();
                    self.apply_channels(&channels);
                    let (segment_index, start_seconds) =
                        locate_tick(&tempo_segments, start_tick);
                    self.events = events;
                    self.tempo_segments = tempo_segments;
                    self.current_segment_index = segment_index;
                    self.next_event_index = self
                        .events
                        .iter()
                        .position(|event| event.tick >= start_tick)
                        .unwrap_or(self.events.len());
                    self.samples_since_start =
                        (start_seconds * self.sample_rate as f64).ceil() as u64;
                    self.end_tick = end_tick;
                    self.playing = true;
                    self.flags.playing.store(true, Ordering::Relaxed);
                }
                AudioCommand::SetChannels(channels) => self.apply_channels(&channels),
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

    /// Advances the song clock by one block, sending the notes that fall in
    /// it to the synth and to instrument plug-ins.
    fn advance_clock(&mut self) {
        for offset in 0..BLOCK {
            if !self.playing {
                break;
            }
            let position = self.current_position();
            let tick = position as Tick;
            self.flags.position.store(position.to_bits(), Ordering::Relaxed);
            while self
                .events
                .get(self.next_event_index)
                .is_some_and(|event| event.tick <= tick)
            {
                let event = self.events[self.next_event_index];
                self.next_event_index += 1;
                let channel = usize::from(event.channel.min(15));
                if self.instrument_channels[channel] {
                    let on = event.kind == MidiEventKind::NoteOn;
                    self.notes[channel].push(NoteEvent {
                        offset: offset as u32,
                        on,
                        channel: 0,
                        key: event.key,
                        velocity: if on { f32::from(event.velocity) / 127.0 } else { 0.0 },
                    });
                    let held = (channel as u8, event.key);
                    if on {
                        if !self.sounding.contains(&held) {
                            self.sounding.push(held);
                        }
                    } else {
                        self.sounding.retain(|note| *note != held);
                    }
                }
                if self.instrument_channels[channel] {
                    // The plug-in is this channel's sound: the soundfont
                    // does not play it, so nothing of it (not even its
                    // reverb) reaches the mix.
                    continue;
                }
                let midi_event = match event.kind {
                    MidiEventKind::NoteOn => MidiEvent::NoteOn {
                        channel: event.channel,
                        key: event.key,
                        vel: event.velocity,
                    },
                    MidiEventKind::NoteOff => MidiEvent::NoteOff {
                        channel: event.channel,
                        key: event.key,
                    },
                };
                if let Some(synth) = self.synth.as_mut() {
                    let _ = synth.send_event(midi_event);
                }
            }
            if tick >= self.end_tick && self.next_event_index >= self.events.len() {
                self.finish_playback();
            }
            self.samples_since_start += 1;
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

    /// Converts elapsed playback time into a musical tick, walking forward
    /// through the tempo automation's precomputed segments.
    fn current_position(&mut self) -> f64 {
        let elapsed_seconds = self.samples_since_start as f64 / self.sample_rate as f64;
        while self.current_segment_index + 1 < self.tempo_segments.len()
            && self.tempo_segments[self.current_segment_index + 1].start_seconds
                <= elapsed_seconds
        {
            self.current_segment_index += 1;
        }
        let segment = self.tempo_segments[self.current_segment_index];
        let tick_offset = (elapsed_seconds - segment.start_seconds) / segment.seconds_per_tick;
        segment.start_tick as f64 + tick_offset.max(0.0)
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
    use crate::core::MidiEventKind;

    fn flags() -> Arc<SharedFlags> {
        Arc::new(SharedFlags::default())
    }

    fn state_with_queue() -> (HeapProd<AudioCommand>, CallbackState, Arc<SharedFlags>) {
        let (producer, consumer) = HeapRb::<AudioCommand>::new(16).split();
        let flags = flags();
        let state = CallbackState::new(consumer, 1_000.0, flags.clone());
        (producer, state, flags)
    }

    fn one_note(tick_on: Tick, tick_off: Tick) -> Arc<Vec<MidiPlaybackEvent>> {
        Arc::new(vec![
            MidiPlaybackEvent {
                tick: tick_on,
                kind: MidiEventKind::NoteOn,
                track_id: 0,
                channel: 0,
                key: 60,
                velocity: 100,
            },
            MidiPlaybackEvent {
                tick: tick_off,
                kind: MidiEventKind::NoteOff,
                track_id: 0,
                channel: 0,
                key: 60,
                velocity: 0,
            },
        ])
    }

    fn no_channels() -> Arc<Vec<ChannelSetup>> {
        Arc::new(Vec::new())
    }

    fn segments() -> Arc<Vec<TempoSegment>> {
        Arc::new(vec![TempoSegment {
            start_tick: 0,
            start_seconds: 0.0,
            seconds_per_tick: seconds_per_tick_for(120.0),
        }])
    }

    #[test]
    fn silent_until_synth_installed_and_play_sets_flag() {
        let (mut producer, mut state, flags) = state_with_queue();
        assert_eq!(state.next_frame(), (0.0, 0.0));
        assert!(!flags.playing.load(Ordering::Relaxed));

        let _ = producer.try_push(AudioCommand::Play {
            events: one_note(0, 10),
            tempo_segments: segments(),
            start_tick: 0,
            end_tick: TICKS_PER_QUARTER * 2,
            channels: no_channels(),
        });
        state.next_frame();
        assert!(flags.playing.load(Ordering::Relaxed));
    }

    #[test]
    fn stop_command_clears_playing_flag() {
        let (mut producer, mut state, flags) = state_with_queue();
        let _ = producer.try_push(AudioCommand::Play {
            events: one_note(0, 10),
            tempo_segments: segments(),
            start_tick: 0,
            end_tick: TICKS_PER_QUARTER * 2,
            channels: no_channels(),
        });
        state.next_frame();
        let _ = producer.try_push(AudioCommand::Stop);
        state.next_frame();
        assert!(!flags.playing.load(Ordering::Relaxed));
    }

    #[test]
    fn song_finishes_by_itself_when_the_clock_reaches_the_end() {
        let (mut producer, mut state, flags) = state_with_queue();
        let _ = producer.try_push(AudioCommand::Play {
            events: one_note(0, 10),
            tempo_segments: segments(),
            start_tick: 0,
            end_tick: TICKS_PER_QUARTER * 2,
            channels: no_channels(),
        });
        // 1 kHz sample rate: the end tick is two quarters, 1 s at 120 bpm.
        for _ in 0..1_100 {
            state.next_frame();
        }
        assert!(!flags.playing.load(Ordering::Relaxed));
        assert!(!state.playing);
    }

    #[test]
    fn the_position_follows_the_clock_between_notes() {
        let (mut producer, mut state, flags) = state_with_queue();
        let _ = producer.try_push(AudioCommand::Play {
            events: one_note(0, 10),
            tempo_segments: segments(),
            start_tick: 0,
            end_tick: TICKS_PER_QUARTER * 8,
            channels: no_channels(),
        });
        for _ in 0..500 {
            state.next_frame();
        }
        let position = f64::from_bits(flags.position.load(Ordering::Relaxed));
        // The position is stamped per frame while a block of 64 is played
        // ahead of the 500 frames asked for.
        let block = 64.0 / 1_000.0 * 2.0 * TICKS_PER_QUARTER as f64;
        assert!((position - TICKS_PER_QUARTER as f64).abs() <= block, "{position}");
    }

    #[test]
    fn song_keeps_playing_until_the_end_tick() {
        let (mut producer, mut state, flags) = state_with_queue();
        let _ = producer.try_push(AudioCommand::Play {
            events: one_note(0, 10),
            tempo_segments: segments(),
            start_tick: 0,
            end_tick: TICKS_PER_QUARTER * 2,
            channels: no_channels(),
        });
        for _ in 0..500 {
            state.next_frame();
        }
        assert!(flags.playing.load(Ordering::Relaxed));
    }

    #[test]
    fn play_from_a_later_tick_sounds_without_waiting_for_the_skipped_time() {
        let (mut producer, mut state, _flags) = state_with_queue();
        let start = TICKS_PER_QUARTER * 8;
        let _ = producer.try_push(AudioCommand::Play {
            events: one_note(start, start + TICKS_PER_QUARTER),
            tempo_segments: segments(),
            start_tick: start,
            end_tick: start + TICKS_PER_QUARTER,
            channels: no_channels(),
        });
        // 8 quarters at 120 bpm is 4 s, i.e. 4000 frames at 1 kHz.
        for _ in 0..10 {
            state.next_frame();
        }
        assert_eq!(state.next_event_index, 1);
        assert!(state.playing);
    }

    #[test]
    fn play_skips_events_before_the_start_tick() {
        let (mut producer, mut state, _flags) = state_with_queue();
        let events = Arc::new(vec![
            one_note(0, 100)[0],
            one_note(0, 100)[1],
            one_note(2_000, 2_100)[0],
            one_note(2_000, 2_100)[1],
        ]);
        let _ = producer.try_push(AudioCommand::Play {
            events,
            tempo_segments: segments(),
            start_tick: 2_000,
            end_tick: 2_100,
            channels: no_channels(),
        });
        state.next_frame();
        // The two notes before the start are skipped; the first block holds
        // the note at the start tick, and may hold its end too.
        assert!(state.next_event_index >= 3, "{}", state.next_event_index);
    }

    #[test]
    fn play_past_the_last_event_finishes_at_once() {
        let (mut producer, mut state, flags) = state_with_queue();
        let _ = producer.try_push(AudioCommand::Play {
            events: one_note(0, 10),
            tempo_segments: segments(),
            start_tick: TICKS_PER_QUARTER * 100,
            end_tick: TICKS_PER_QUARTER * 100,
            channels: no_channels(),
        });
        state.next_frame();
        assert!(!flags.playing.load(Ordering::Relaxed));
    }

    #[test]
    fn locate_tick_follows_the_tempo_map() {
        let slow = seconds_per_tick_for(60.0);
        let fast = seconds_per_tick_for(120.0);
        let boundary = TICKS_PER_QUARTER * 4;
        let segments = [
            TempoSegment {
                start_tick: 0,
                start_seconds: 0.0,
                seconds_per_tick: slow,
            },
            TempoSegment {
                start_tick: boundary,
                start_seconds: boundary as f64 * slow,
                seconds_per_tick: fast,
            },
        ];
        assert_eq!(locate_tick(&segments, 0), (0, 0.0));
        assert_eq!(locate_tick(&segments, boundary - 1).0, 0);
        let (index, seconds) = locate_tick(&segments, boundary + TICKS_PER_QUARTER);
        assert_eq!(index, 1);
        // Four quarters at 60 bpm, then one quarter at 120 bpm.
        assert!((seconds - 4.5).abs() < 1e-9);
    }

    #[test]
    fn play_inside_a_later_tempo_segment_continues_on_that_tempo() {
        let (mut producer, mut state, _flags) = state_with_queue();
        let boundary = TICKS_PER_QUARTER * 4;
        let tempo_segments = Arc::new(vec![
            TempoSegment {
                start_tick: 0,
                start_seconds: 0.0,
                seconds_per_tick: seconds_per_tick_for(60.0),
            },
            TempoSegment {
                start_tick: boundary,
                start_seconds: 4.0,
                seconds_per_tick: seconds_per_tick_for(120.0),
            },
        ]);
        let start = boundary + TICKS_PER_QUARTER;
        let _ = producer.try_push(AudioCommand::Play {
            events: one_note(start + TICKS_PER_QUARTER, start + TICKS_PER_QUARTER * 2),
            tempo_segments,
            start_tick: start,
            end_tick: start + TICKS_PER_QUARTER * 2,
            channels: no_channels(),
        });
        state.next_frame();
        assert_eq!(state.current_segment_index, 1);
        // One quarter at 120 bpm is 0.5 s: the note is due 500 frames in.
        // The clock moves a block (64 frames) at a time.
        for _ in 0..390 {
            state.next_frame();
        }
        assert_eq!(state.next_event_index, 0);
        for _ in 0..160 {
            state.next_frame();
        }
        assert_eq!(state.next_event_index, 1);
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
    fn a_channel_is_set_up_with_bank_zero_before_its_program() {
        let events = setup_events(ChannelSetup {
            channel: 3,
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
            program: 33,
            volume: 100,
            pan: 64,
        }]);
        let _ = producer.try_push(AudioCommand::Play {
            events: one_note(0, 240),
            tempo_segments: segments(),
            start_tick: 0,
            end_tick: 240,
            channels,
        });
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

    /// The test plug-ins come from the vst3 crate's `gain` example (effect)
    /// and a variant of it that plays a sine on note-on (instrument); the
    /// tests run when BW_TEST_GAIN_VST3 / BW_TEST_SYNTH_VST3 name them.
    fn live_plugin(var: &str, id: u64, instrument: bool) -> Option<Arc<LivePlugin>> {
        let path = std::env::var_os(var)?;
        let module = crate::plugins::vst3::Module::load(Path::new(&path)).expect("load");
        let instance = crate::plugins::vst3::Instance::create(&module, "", 1_000.0).expect("create");
        Some(Arc::new(LivePlugin {
            id,
            is_instrument: instrument,
            instance: std::sync::Mutex::new(instance),
        }))
    }

    fn play_one_note(state: &mut CallbackState, producer: &mut HeapProd<AudioCommand>, frames: usize) -> f32 {
        let _ = producer.try_push(AudioCommand::Play {
            events: one_note(0, TICKS_PER_QUARTER * 4),
            tempo_segments: segments(),
            start_tick: 0,
            end_tick: TICKS_PER_QUARTER * 4,
            channels: Arc::new(vec![ChannelSetup { channel: 0, program: 0, volume: 127, pan: 64 }]),
        });
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
        let _ = producer.try_push(AudioCommand::Play {
            events: one_note(0, TICKS_PER_QUARTER * 4),
            tempo_segments: segments(),
            start_tick: 0,
            end_tick: TICKS_PER_QUARTER * 4,
            channels: Arc::new(vec![ChannelSetup { channel: 0, program: 0, volume: 0, pan: 64 }]),
        });
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
            let _ = producer.try_push(AudioCommand::Play {
                events: one_note(0, 240),
                tempo_segments: segments(),
                start_tick: 0,
                end_tick: 240,
                channels: Arc::new(vec![ChannelSetup { channel: 0, program: 33, volume: 127, pan: 64 }]),
            });
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
