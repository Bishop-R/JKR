//! Allocation-free rodio callback and bounded command/decode queues.

use jkr_audio::{
    ChannelId, DecodedSound, Mixer, MixerConfig, PlayRequest, SoundHandle, SourceId, decode_encoded,
};
use ringbuf::{HeapCons, HeapProd, HeapRb, traits::*};
use rodio::{OutputStream, Source};
use std::sync::{
    Arc,
    atomic::{AtomicU32, AtomicU64, Ordering},
    mpsc::{SyncSender, sync_channel},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[path = "audio_pending.rs"]
mod pending;

#[path = "audio_player_mutes.rs"]
mod player_mutes;

pub(super) const SAMPLE_RATE: u32 = 44_100;
pub(super) const VOICES: usize = 32;
pub(super) const LOOPS: usize = 32;
const MIX_SAMPLES: usize = 256;
const MIX_BLOCK_BUDGET: Duration =
    Duration::from_micros((MIX_SAMPLES as u64 / 2) * 1_000_000 / SAMPLE_RATE as u64);
const COMMANDS: usize = 8_192;
const DECODE_JOBS: usize = 8_192;

/// Counters the audio thread publishes for the render thread's diagnostics;
/// the audio thread only ever touches atomics.
#[derive(Default)]
pub(super) struct OutputStats {
    /// Mix blocks that took longer than their playback duration to render.
    pub(super) underruns: AtomicU64,
    /// Bits of the loudest sample rendered since the last `take_peak`.
    peak_bits: AtomicU32,
    pub(super) rendered_blocks: AtomicU64,
    pub(super) music_starts: AtomicU64,
    /// Music starts the mixer refused (a handle without decoded samples).
    pub(super) music_rejections: AtomicU64,
    /// Sounds registered as silence because their bytes did not decode.
    pub(super) decode_failures: AtomicU64,
    /// Registrations whose bank index differed from the handle handed out.
    pub(super) handle_mismatches: AtomicU64,
    /// Mix blocks rendered while the render thread was still sending a
    /// frame's loops; each one used to cut the unsent loops for a block.
    pub(super) split_loop_frames: AtomicU64,
}

impl OutputStats {
    /// Loudest sample since the previous call.
    pub(super) fn take_peak(&self) -> f32 {
        f32::from_bits(self.peak_bits.swap(0, Ordering::Relaxed))
    }

    fn observe(&self, block: &[f32]) {
        let peak = block
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        // Non-negative floats order like their bit patterns.
        self.peak_bits.fetch_max(peak.to_bits(), Ordering::Relaxed);
        self.rendered_blocks.fetch_add(1, Ordering::Relaxed);
    }
}

struct MixerSource {
    mixer: Mixer,
    commands: HeapCons<AudioCommand>,
    registrations: HeapCons<DecodedAction>,
    scratch: [f32; MIX_SAMPLES],
    cursor: usize,
    running: bool,
    stats: Arc<OutputStats>,
    pending: pending::PendingStarts,
}

impl Iterator for MixerSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.running {
            return None;
        }
        if self.cursor == self.scratch.len() {
            while let Some(action) = self.registrations.try_pop() {
                match action {
                    DecodedAction::Register(expected, decoded) => {
                        // Registration must run in release too: the bank
                        // index is the handle the render thread already
                        // handed out, so it has to be assigned in order.
                        let registered = self.mixer.sound_bank_mut().register_decoded(decoded);
                        if registered != expected {
                            self.stats.handle_mismatches.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    DecodedAction::StopMusic => self.mixer.stop_background_track(),
                    DecodedAction::StartMusic(intro, repeating) => {
                        let counter = if self.mixer.start_background_track(intro, repeating, 1.0) {
                            &self.stats.music_starts
                        } else {
                            &self.stats.music_rejections
                        };
                        counter.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            while let Some(command) = self.commands.try_pop() {
                if let AudioCommand::Play(handle, request) = command {
                    self.pending.play(&mut self.mixer, handle, request);
                    continue;
                }
                if let AudioCommand::StopChannel(source, channel) = &command {
                    self.pending.cancel(*source, *channel);
                }
                if matches!(command, AudioCommand::StopAll | AudioCommand::StopEffects) {
                    self.pending.clear();
                }
                self.running = apply_command(&mut self.mixer, command);
                if !self.running {
                    return None;
                }
            }
            self.pending.flush(&mut self.mixer);
            if self.mixer.loop_frame_open() {
                self.stats.split_loop_frames.fetch_add(1, Ordering::Relaxed);
            }
            let started = Instant::now();
            self.mixer.render(&mut self.scratch);
            if started.elapsed() > MIX_BLOCK_BUDGET {
                self.stats.underruns.fetch_add(1, Ordering::Relaxed);
            }
            self.stats.observe(&self.scratch);
            self.cursor = 0;
        }
        let sample = self.scratch[self.cursor];
        self.cursor += 1;
        Some(sample)
    }
}

impl Source for MixerSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        2
    }

    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

pub(super) enum AudioCommand {
    Play(SoundHandle, PlayRequest),
    Listener {
        origin: [f32; 3],
        forward: [f32; 3],
        right: [f32; 3],
        up: [f32; 3],
        velocity: [f32; 3],
    },
    ListenerSource(Option<SourceId>),
    SourcePosition(SourceId, [f32; 3]),
    StopChannel(SourceId, ChannelId),
    StopLoops(SourceId),
    /// Start a frame's loop set; the previous set plays on until `CommitLoops`.
    ClearLoops,
    /// End the loop set begun by `ClearLoops`.
    CommitLoops,
    StopAll,
    StopEffects,
    SetLoop(SoundHandle, PlayRequest, [f32; 3]),
    Gains(f32, f32),
    Doppler(bool),
    Separation(f32),
    Shutdown,
}

enum DecodedAction {
    Register(SoundHandle, DecodedSound),
    StartMusic(SoundHandle, SoundHandle),
    StopMusic,
}

pub(super) struct AudioOutput {
    pub(super) muted_players: u32,
    _stream: Option<OutputStream>,
    pub(super) description: String,
    commands: HeapProd<AudioCommand>,
    decode_jobs: Option<SyncSender<DecodeJob>>,
    decoder: Option<JoinHandle<()>>,
    pub(super) stats: Arc<OutputStats>,
    pub(super) show: bool,
    pub(super) started: std::collections::VecDeque<SoundHandle>,
    pub(super) cache: std::cell::RefCell<Vec<(SoundHandle, usize, String)>>,
}

enum DecodeJob {
    Sound {
        handle: SoundHandle,
        bytes: Box<[u8]>,
        extension: Box<str>,
    },
    StartMusic(SoundHandle, SoundHandle),
    StopMusic,
}

impl AudioOutput {
    /// Open the default output device; a failure is logged, not swallowed,
    /// so a silent game names its cause.
    pub(super) fn start(config: MixerConfig) -> Option<Self> {
        let (stream, output) = match OutputStream::try_default() {
            Ok(stream) => stream,
            Err(error) => {
                crate::log::progress(format_args!("audio output unavailable: {error}"));
                return None;
            }
        };
        let description = describe_default_device();
        crate::log::progress(format_args!("audio output: {description}"));
        let (command_producer, command_consumer) = HeapRb::new(COMMANDS).split();
        let (registration_producer, registration_consumer) = HeapRb::new(COMMANDS).split();
        let (decode_sender, decode_receiver) = sync_channel::<DecodeJob>(DECODE_JOBS);
        let stats = Arc::new(OutputStats::default());
        let decoder_stats = Arc::clone(&stats);
        let decoder = thread::Builder::new()
            .name("jkr-audio-decode".into())
            .spawn(move || {
                let mut registrations = registration_producer;
                while let Ok(job) = decode_receiver.recv() {
                    let mut action = match job {
                        DecodeJob::Sound {
                            handle,
                            bytes,
                            extension,
                        } => DecodedAction::Register(
                            handle,
                            decode_encoded(&bytes, &extension, SAMPLE_RATE).unwrap_or_else(|_| {
                                decoder_stats
                                    .decode_failures
                                    .fetch_add(1, Ordering::Relaxed);
                                DecodedSound::silence()
                            }),
                        ),
                        DecodeJob::StopMusic => DecodedAction::StopMusic,
                        DecodeJob::StartMusic(intro, repeating) => {
                            DecodedAction::StartMusic(intro, repeating)
                        }
                    };
                    loop {
                        match registrations.try_push(action) {
                            Ok(()) => break,
                            Err(returned) => {
                                action = returned;
                                thread::yield_now();
                            }
                        }
                    }
                }
            })
            .ok()?;
        if let Err(error) = output.play_raw(MixerSource {
            mixer: Mixer::new(config),
            commands: command_consumer,
            registrations: registration_consumer,
            scratch: [0.0; MIX_SAMPLES],
            cursor: MIX_SAMPLES,
            running: true,
            stats: Arc::clone(&stats),
            pending: pending::PendingStarts::new(),
        }) {
            crate::log::progress(format_args!("audio mixer could not start: {error}"));
            return None;
        }
        Some(Self {
            muted_players: 0,
            _stream: Some(stream),
            description,
            commands: command_producer,
            decode_jobs: Some(decode_sender),
            decoder: Some(decoder),
            stats,
            show: false,
            started: std::collections::VecDeque::with_capacity(64),
            cache: std::cell::RefCell::new(Vec::new()),
        })
    }

    pub(super) fn send(&mut self, mut command: AudioCommand) {
        if player_mutes::blocked(self.muted_players, &command) {
            return;
        }
        if self.show
            && let AudioCommand::Play(handle, _) = &command
        {
            if self.started.len() == 64 {
                self.started.pop_front();
            }
            self.started.push_back(*handle);
        }
        loop {
            match self.commands.try_push(command) {
                Ok(()) => return,
                Err(returned) => {
                    command = returned;
                    thread::yield_now();
                }
            }
        }
    }

    pub(super) fn decode(&self, handle: SoundHandle, bytes: &[u8], extension: &str) {
        self.cache
            .borrow_mut()
            .push((handle, bytes.len(), extension.to_owned()));
        let Some(sender) = &self.decode_jobs else {
            return;
        };
        let job = DecodeJob::Sound {
            handle,
            bytes: bytes.into(),
            extension: extension.into(),
        };
        if sender.try_send(job).is_err() {
            crate::log::progress(format_args!(
                "audio decode queue is full; dropping {handle:?}"
            ));
        }
    }

    pub(super) fn stop_music(&self) {
        if let Some(sender) = &self.decode_jobs {
            let _ = sender.try_send(DecodeJob::StopMusic);
        }
    }

    pub(super) fn start_music(&self, intro: SoundHandle, repeating: SoundHandle) {
        let Some(sender) = &self.decode_jobs else {
            return;
        };
        if sender
            .try_send(DecodeJob::StartMusic(intro, repeating))
            .is_err()
        {
            crate::log::progress(format_args!("audio decode queue is full; music deferred"));
        }
    }
}

impl Drop for AudioOutput {
    fn drop(&mut self) {
        self.decode_jobs.take();
        if let Some(decoder) = self.decoder.take() {
            let _ = decoder.join();
        }
        let _ = self.commands.try_push(AudioCommand::Shutdown);
    }
}

/// Name and format of the device rodio just opened, for the startup log.
fn describe_default_device() -> String {
    use rodio::cpal::traits::{DeviceTrait, HostTrait};
    let Some(device) = rodio::cpal::default_host().default_output_device() else {
        return "no default output device".into();
    };
    let name = device.name().unwrap_or_else(|_| "unnamed device".into());
    match device.default_output_config() {
        Ok(config) => format!(
            "device={name} {}Hz {}ch {:?}",
            config.sample_rate().0,
            config.channels(),
            config.sample_format()
        ),
        Err(error) => format!("device={name} (format unknown: {error})"),
    }
}

fn apply_command(mixer: &mut Mixer, command: AudioCommand) -> bool {
    match command {
        AudioCommand::Play(handle, request) => {
            mixer.play(handle, request);
        }
        AudioCommand::Listener {
            origin,
            forward,
            right,
            up,
            velocity,
        } => {
            mixer.listener(origin, forward, right, up);
            mixer.set_listener_velocity(velocity);
        }
        AudioCommand::ListenerSource(source) => mixer.set_listener_source(source),
        AudioCommand::SourcePosition(source, origin) => {
            mixer.update_source_position(source, origin);
        }
        AudioCommand::StopChannel(source, channel) => {
            mixer.stop_source_channel(source, channel);
        }
        AudioCommand::StopLoops(source) => {
            mixer.stop_source_loops(source);
        }
        AudioCommand::ClearLoops => {
            mixer.clear_loops();
        }
        AudioCommand::CommitLoops => mixer.commit_loops(),
        AudioCommand::StopAll => mixer.stop_all(),
        AudioCommand::StopEffects => mixer.stop_effects(),
        AudioCommand::SetLoop(handle, request, velocity) => {
            mixer.set_loop_request(handle, request, velocity);
        }
        AudioCommand::Gains(effects, music) => {
            mixer.set_master_gain(effects);
            mixer.set_music_gain(music);
        }
        AudioCommand::Doppler(enabled) => {
            mixer.set_doppler_enabled(enabled);
        }
        AudioCommand::Separation(value) => mixer.set_separation(value),
        AudioCommand::Shutdown => return false,
    }
    true
}
