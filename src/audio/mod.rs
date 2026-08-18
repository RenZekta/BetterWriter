use std::{
    fs::File,
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::Arc,
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
};

/// One constant-rate playback segment derived from the project's tempo
/// automation. `Progressive` points use the average of their start/end bpm
/// as the segment's rate — a good approximation for audio scheduling; the
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

#[derive(Clone)]
enum AudioCommand {
    Play {
        events: Arc<Vec<MidiPlaybackEvent>>,
        tempo_segments: Arc<Vec<TempoSegment>>,
        start_tick: Tick,
    },
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

pub struct AudioRuntime {
    soundfont_path: PathBuf,
    producer: HeapProd<AudioCommand>,
    consumer: Option<HeapCons<AudioCommand>>,
    stream: Option<cpal::Stream>,
    status: String,
}

impl AudioRuntime {
    pub fn new(soundfont_path: impl Into<PathBuf>) -> Self {
        let rb = HeapRb::<AudioCommand>::new(2048);
        let (producer, consumer) = rb.split();
        Self {
            soundfont_path: soundfont_path.into(),
            producer,
            consumer: Some(consumer),
            stream: None,
            status: "Audio engine idle".to_owned(),
        }
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn ensure_stream(&mut self) -> Result<(), AudioError> {
        if self.stream.is_some() {
            return Ok(());
        }

        let Some(consumer) = self.consumer.take() else {
            self.status = "Audio stream already consumed".to_owned();
            return Ok(());
        };

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::NoOutputDevice)?;
        let supported = device.default_output_config()?;
        let config: cpal::StreamConfig = supported.clone().into();
        let sample_rate = config.sample_rate as f32;
        let channels = config.channels as usize;
        let soundfont_path = self.soundfont_path.clone();

        let stream = match supported.sample_format() {
            cpal::SampleFormat::I8 => build_stream::<i8>(
                &device,
                &config,
                channels,
                sample_rate,
                soundfont_path,
                consumer,
            )?,
            cpal::SampleFormat::I16 => build_stream::<i16>(
                &device,
                &config,
                channels,
                sample_rate,
                soundfont_path,
                consumer,
            )?,
            cpal::SampleFormat::I24 => build_stream::<cpal::I24>(
                &device,
                &config,
                channels,
                sample_rate,
                soundfont_path,
                consumer,
            )?,
            cpal::SampleFormat::I32 => build_stream::<i32>(
                &device,
                &config,
                channels,
                sample_rate,
                soundfont_path,
                consumer,
            )?,
            cpal::SampleFormat::I64 => build_stream::<i64>(
                &device,
                &config,
                channels,
                sample_rate,
                soundfont_path,
                consumer,
            )?,
            cpal::SampleFormat::U8 => build_stream::<u8>(
                &device,
                &config,
                channels,
                sample_rate,
                soundfont_path,
                consumer,
            )?,
            cpal::SampleFormat::U16 => build_stream::<u16>(
                &device,
                &config,
                channels,
                sample_rate,
                soundfont_path,
                consumer,
            )?,
            cpal::SampleFormat::U32 => build_stream::<u32>(
                &device,
                &config,
                channels,
                sample_rate,
                soundfont_path,
                consumer,
            )?,
            cpal::SampleFormat::U64 => build_stream::<u64>(
                &device,
                &config,
                channels,
                sample_rate,
                soundfont_path,
                consumer,
            )?,
            cpal::SampleFormat::F32 => build_stream::<f32>(
                &device,
                &config,
                channels,
                sample_rate,
                soundfont_path,
                consumer,
            )?,
            cpal::SampleFormat::F64 => build_stream::<f64>(
                &device,
                &config,
                channels,
                sample_rate,
                soundfont_path,
                consumer,
            )?,
            other => {
                return Err(AudioError::SoundFontLoad(format!(
                    "unsupported sample format {other:?}"
                )));
            }
        };

        stream.play()?;
        self.stream = Some(stream);
        self.status = format!(
            "Audio online: {} Hz, {} channels, {}",
            sample_rate,
            channels,
            self.soundfont_path.display()
        );
        Ok(())
    }

    pub fn play(
        &mut self,
        project: &BwxProject,
        events: Vec<MidiPlaybackEvent>,
    ) -> Result<(), AudioError> {
        self.ensure_stream()?;
        self.producer
            .try_push(AudioCommand::Play {
                events: Arc::new(events),
                tempo_segments: Arc::new(build_tempo_segments(project)),
                start_tick: 0,
            })
            .map_err(|_| AudioError::CommandQueueFull)
    }

    pub fn stop(&mut self) -> Result<(), AudioError> {
        self.producer
            .try_push(AudioCommand::Stop)
            .map_err(|_| AudioError::CommandQueueFull)
    }
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
    soundfont_path: PathBuf,
    consumer: HeapCons<AudioCommand>,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + FromSample<f32>,
{
    let mut state = CallbackState::new(consumer, sample_rate, &soundfont_path)?;
    let err_fn = |err| eprintln!("BetterWriter audio stream error: {err}");

    Ok(device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            write_data(data, channels, &mut state);
        },
        err_fn,
        None,
    )?)
}

struct CallbackState {
    command_consumer: HeapCons<AudioCommand>,
    synth: Synth,
    events: Arc<Vec<MidiPlaybackEvent>>,
    next_event_index: usize,
    playing: bool,
    tempo_segments: Arc<Vec<TempoSegment>>,
    /// Monotonically advances forward through `tempo_segments` as playback
    /// progresses, mirroring `next_event_index`'s incremental scan — cheap
    /// per audio frame even with many automation points.
    current_segment_index: usize,
    sample_rate: f32,
    samples_since_start: u64,
}

impl CallbackState {
    fn new(
        command_consumer: HeapCons<AudioCommand>,
        sample_rate: f32,
        soundfont_path: &Path,
    ) -> Result<Self, AudioError> {
        let mut synth = Synth::new(SynthDescriptor {
            sample_rate,
            audio_channels: 2,
            gain: 0.35,
            ..Default::default()
        })
        .map_err(|err| AudioError::SoundFontLoad(err.to_string()))?;

        let mut file = File::open(soundfont_path).map_err(|err| {
            AudioError::SoundFontLoad(format!("{}: {err}", soundfont_path.display()))
        })?;
        let font =
            SoundFont::load(&mut file).map_err(|err| AudioError::SoundFontLoad(err.to_string()))?;
        synth.add_font(font, true);

        for channel in 0..16 {
            let _ = synth.send_event(MidiEvent::ProgramChange {
                channel,
                program_id: 24,
            });
        }

        Ok(Self {
            command_consumer,
            synth,
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
        })
    }

    fn drain_commands(&mut self) {
        while let Some(command) = self.command_consumer.try_pop() {
            match command {
                AudioCommand::Play {
                    events,
                    tempo_segments,
                    start_tick,
                } => {
                    self.events = events;
                    self.tempo_segments = tempo_segments;
                    self.current_segment_index = 0;
                    self.next_event_index = self
                        .events
                        .iter()
                        .position(|event| event.tick >= start_tick)
                        .unwrap_or(self.events.len());
                    self.samples_since_start = 0;
                    self.playing = true;
                    for channel in 0..16 {
                        let _ = self.synth.send_event(MidiEvent::AllNotesOff { channel });
                    }
                }
                AudioCommand::Stop => {
                    self.playing = false;
                    for channel in 0..16 {
                        let _ = self.synth.send_event(MidiEvent::AllNotesOff { channel });
                    }
                }
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
                let _ = self.synth.send_event(midi_event);
                self.next_event_index += 1;
            }
            self.samples_since_start += 1;
        }

        self.synth.read_next()
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
    for frame in output.chunks_mut(channels) {
        let (left, right) = state.next_frame();
        for (index, sample) in frame.iter_mut().enumerate() {
            let value = if index % 2 == 0 { left } else { right };
            *sample = T::from_sample(value);
        }
    }
}
