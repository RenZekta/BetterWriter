use std::{
    fs::File,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
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
use vst3::Steinberg::{IPluginFactory, Vst::IAudioProcessor};

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

/// Time after the last scheduled event before playback counts as finished, so
/// note releases and reverb are not cut off.
const END_TAIL_SECONDS: f64 = 2.0;

/// Stack size for the soundfont loader thread. Decoding an SF3 allocates and
/// recurses far more than a default thread stack comfortably allows.
const LOADER_STACK_BYTES: usize = 64 * 1024 * 1024;

enum AudioCommand {
    InstallSynth(Box<Synth>),
    Play {
        events: Arc<Vec<MidiPlaybackEvent>>,
        tempo_segments: Arc<Vec<TempoSegment>>,
        start_tick: Tick,
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
    /// song (plus its release tail) has finished.
    playing: AtomicBool,
    /// Set when the audio callback panicked; the runtime rebuilds the stream
    /// the next time playback is requested.
    faulted: AtomicBool,
}

struct PendingPlay {
    events: Vec<MidiPlaybackEvent>,
    tempo_segments: Vec<TempoSegment>,
    start_tick: Tick,
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
            status: "Audio engine idle".to_owned(),
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
        let pending = PendingPlay {
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
                channels: Arc::new(self.mix.clone()),
            })
            .map_err(|_| {
                self.flags.playing.store(false, Ordering::Relaxed);
                AudioError::CommandQueueFull
            })
    }
}

/// Builds the synth and loads the soundfont. Runs on the loader thread.
fn build_synth(sample_rate: f32, soundfont_path: &Path) -> LoaderResult {
    let mut synth = Synth::new(SynthDescriptor {
        sample_rate,
        audio_channels: 2,
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

pub trait VstHost {
    fn load_plugin(&mut self, path: &Path) -> Result<(), String>;
    fn process_replacing(&mut self, input: &[f32], output: &mut [f32]);
    fn status(&self) -> String;
}

#[derive(Default)]
pub struct Vst3HostSlot {
    loaded_plugins: Vec<PathBuf>,
    _api_marker: PhantomData<fn() -> (*mut IPluginFactory, *mut IAudioProcessor)>,
}

impl VstHost for Vst3HostSlot {
    fn load_plugin(&mut self, path: &Path) -> Result<(), String> {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if extension != "vst3" {
            return Err("expected a .vst3 bundle or module".to_owned());
        }
        self.loaded_plugins.push(path.to_path_buf());
        Ok(())
    }

    fn process_replacing(&mut self, input: &[f32], output: &mut [f32]) {
        let len = input.len().min(output.len());
        output[..len].copy_from_slice(&input[..len]);
        output[len..].fill(0.0);
    }

    fn status(&self) -> String {
        if self.loaded_plugins.is_empty() {
            "VST3 host slot: ready, no plug-in loaded".to_owned()
        } else {
            format!("VST3 plug-ins loaded: {}", self.loaded_plugins.len())
        }
    }
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
    /// Sample count at which a song whose events are all dispatched is
    /// considered finished.
    finish_at_sample: Option<u64>,
    flags: Arc<SharedFlags>,
    faulted: bool,
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
            finish_at_sample: None,
            flags,
            faulted: false,
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
    }

    fn stop_playback(&mut self) {
        self.playing = false;
        self.finish_at_sample = None;
        self.flags.playing.store(false, Ordering::Relaxed);
        self.all_notes_off();
    }

    fn drain_commands(&mut self) {
        while let Some(command) = self.command_consumer.try_pop() {
            match command {
                AudioCommand::InstallSynth(synth) => {
                    self.synth = Some(synth);
                }
                AudioCommand::Play {
                    events,
                    tempo_segments,
                    start_tick,
                    channels,
                } => {
                    self.all_notes_off();
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
                    self.finish_at_sample = None;
                    self.playing = true;
                    self.flags.playing.store(true, Ordering::Relaxed);
                }
                AudioCommand::SetChannels(channels) => self.apply_channels(&channels),
                AudioCommand::Stop => self.stop_playback(),
            }
        }
    }

    fn next_frame(&mut self) -> (f32, f32) {
        self.drain_commands();
        if self.playing {
            let tick = self.current_tick();
            while self
                .events
                .get(self.next_event_index)
                .is_some_and(|event| event.tick <= tick)
            {
                let event = self.events[self.next_event_index];
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
                self.next_event_index += 1;
            }
            if self.next_event_index >= self.events.len() {
                let tail = (END_TAIL_SECONDS * self.sample_rate as f64) as u64;
                let deadline = *self
                    .finish_at_sample
                    .get_or_insert(self.samples_since_start + tail);
                if self.samples_since_start >= deadline {
                    self.stop_playback();
                }
            }
            self.samples_since_start += 1;
        }

        match self.synth.as_mut() {
            Some(synth) => synth.read_next(),
            None => (0.0, 0.0),
        }
    }

    /// Converts elapsed playback time into a musical tick, walking forward
    /// through the tempo automation's precomputed segments.
    fn current_tick(&mut self) -> Tick {
        let elapsed_seconds = self.samples_since_start as f64 / self.sample_rate as f64;
        while self.current_segment_index + 1 < self.tempo_segments.len()
            && self.tempo_segments[self.current_segment_index + 1].start_seconds
                <= elapsed_seconds
        {
            self.current_segment_index += 1;
        }
        let segment = self.tempo_segments[self.current_segment_index];
        let tick_offset = (elapsed_seconds - segment.start_seconds) / segment.seconds_per_tick;
        segment.start_tick + tick_offset.max(0.0) as Tick
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
            channels: no_channels(),
        });
        state.next_frame();
        let _ = producer.try_push(AudioCommand::Stop);
        state.next_frame();
        assert!(!flags.playing.load(Ordering::Relaxed));
    }

    #[test]
    fn song_finishes_by_itself_after_the_release_tail() {
        let (mut producer, mut state, flags) = state_with_queue();
        let _ = producer.try_push(AudioCommand::Play {
            events: one_note(0, 10),
            tempo_segments: segments(),
            start_tick: 0,
            channels: no_channels(),
        });
        // 1 kHz sample rate: 2.0 s tail plus a little slack for the note.
        for _ in 0..2_500 {
            state.next_frame();
        }
        assert!(!flags.playing.load(Ordering::Relaxed));
        assert!(!state.playing);
    }

    #[test]
    fn song_with_events_keeps_playing_before_the_tail_elapses() {
        let (mut producer, mut state, flags) = state_with_queue();
        let _ = producer.try_push(AudioCommand::Play {
            events: one_note(0, 10),
            tempo_segments: segments(),
            start_tick: 0,
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
            channels: no_channels(),
        });
        state.next_frame();
        assert_eq!(state.next_event_index, 3);
    }

    #[test]
    fn play_past_the_last_event_only_waits_for_the_release_tail() {
        let (mut producer, mut state, flags) = state_with_queue();
        let _ = producer.try_push(AudioCommand::Play {
            events: one_note(0, 10),
            tempo_segments: segments(),
            start_tick: TICKS_PER_QUARTER * 100,
            channels: no_channels(),
        });
        for _ in 0..2_500 {
            state.next_frame();
        }
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
            channels: no_channels(),
        });
        state.next_frame();
        assert_eq!(state.current_segment_index, 1);
        // One quarter at 120 bpm is 0.5 s: the note is due 500 frames in.
        for _ in 0..450 {
            state.next_frame();
        }
        assert_eq!(state.next_event_index, 0);
        for _ in 0..100 {
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
}
