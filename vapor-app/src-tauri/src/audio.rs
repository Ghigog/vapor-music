//! Audio output (TD-03).
//!
//! `vapor-engine` has always played correctly from the `play` binary; the shell
//! had no device at all, so the transport buttons moved queue state and nothing
//! was heard. This is the missing link: a device, a stream, and a way to steer
//! the mixer from a Tauri command without touching the audio thread's rules.
//!
//! ## Three threads, and what each is allowed to do
//!
//! | Thread | Does | Never does |
//! |---|---|---|
//! | Command (Tauri IPC) | Enqueues commands, reads a snapshot | Blocks on audio |
//! | Loader (one per track) | Fetch, decode, resample | Touches the mixer |
//! | Audio (cpal callback) | Renders, applies commands | Allocates, frees, blocks |
//!
//! The audio thread's constraint is the one that shapes everything else.
//! `Mixer::render` is allocation-free and proven so by a counting allocator
//! (MIG-010); that guarantee is worth nothing if the code *around* it allocates,
//! so the plumbing here has to hold to the same standard.
//!
//! ## Why a mutex on the audio thread is not the mistake it looks like
//!
//! Commands cross the boundary in a `VecDeque` behind a `Mutex`, and the audio
//! thread takes it with `try_lock`. It never waits: a failed acquisition means
//! the control thread happens to hold the lock this instant, so the drain is
//! skipped and retried in the next callback — a few milliseconds later. Audio
//! keeps rendering throughout. The failure mode is a command applied one block
//! late, not a dropout, and priority inversion cannot occur because the audio
//! thread never blocks on the lock.
//!
//! A lock-free ring buffer would remove even that, at the cost of hand-written
//! unsafe code. The deferral is measured (`Snapshot::commands_deferred`) rather
//! than assumed, so if it ever proves to matter there is evidence to act on.
//!
//! ## The buffer that must not be freed here
//!
//! Loading a track hands the deck a decoded buffer — tens of megabytes — and
//! displaces the previous one. Freeing that on the audio thread is a real-time
//! violation with a nasty signature: a dropout that only happens at a track
//! change. So [`Deck::swap_samples`](vapor_engine::deck::Deck::swap_samples)
//! returns the old buffer, the audio thread parks it in `retired`, and the
//! control thread frees it on its next command.

use std::collections::VecDeque;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use serde::Serialize;

use vapor_engine::{Mixer, TransitionType};

/// Largest block the mixer will render in one callback.
///
/// Sized once, before the stream starts. Devices ask for far less in practice;
/// if one ever asks for more, the tail is filled with silence rather than
/// growing a buffer on the audio thread.
const MAX_BLOCK: usize = 4096;

/// How many commands may be in flight.
///
/// Generous — commands come from human actions, not a loop. The bound exists so
/// the queue has a fixed allocation rather than to ration anything.
const COMMAND_CAPACITY: usize = 32;

/// What the audio thread is doing.
///
/// Loading is deliberately absent: fetching and decoding a track is the shell's
/// business, and the audio thread has no way to know it is happening.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Idle,
    Playing,
    Paused,
}

impl Status {
    fn from_u8(v: u8) -> Status {
        match v {
            1 => Status::Playing,
            2 => Status::Paused,
            _ => Status::Idle,
        }
    }
}

enum Command {
    /// Replace the loaded track. `play` starts it immediately, which is what
    /// every caller wants — cueing without playing has no UI yet.
    Load {
        frames: Vec<[f32; 2]>,
        play: bool,
    },
    /// Put the next track on the other deck — silent, cued, ready to be mixed
    /// in. The transition is what makes it audible.
    Preload {
        frames: Vec<[f32; 2]>,
    },
    /// Begin a transition once the outgoing deck reaches `start_at`.
    ///
    /// Carries scalars, never beat grids — see `Mixer::schedule_prepared`.
    ScheduleTransition {
        kind: TransitionType,
        duration: f32,
        incoming_pos: f32,
        ratio: f64,
        /// Position **within the outgoing track** at which to start mixing.
        ///
        /// A position rather than a delay, because only the audio thread knows
        /// where the playhead truly is. A delay computed on the control thread
        /// would be stale by however long the command waited in the queue, and
        /// beat alignment is measured in single-digit milliseconds.
        start_at: f64,
    },
    CancelTransition,
    Play,
    Pause,
    Stop,
    Seek(f64),
}

/// The two queues crossing the thread boundary.
struct Channel {
    to_audio: VecDeque<Command>,
    /// Track buffers the audio thread has finished with, awaiting a free on the
    /// control thread. See the module docs.
    retired: VecDeque<Vec<[f32; 2]>>,
}

/// Everything shared between the control side and the audio thread.
///
/// Scalars are atomics rather than more channel traffic: a dragged volume
/// slider would otherwise fill the command queue, and position has to be
/// readable at any moment without disturbing the audio thread at all.
pub struct Link {
    channel: Mutex<Channel>,
    /// Playback position in seconds, as `f64` bits.
    position: AtomicU64,
    /// Loaded track duration in seconds, as `f64` bits.
    duration: AtomicU64,
    status: AtomicU8,
    /// Set when a track runs out. Consumed by the supervisor, which advances
    /// the queue — the audio thread must not know what a queue is.
    ended: AtomicBool,
    /// Set when a transition completes and the decks change roles. The track
    /// that was "next" is now the one playing, and the queue has to catch up.
    swapped: AtomicBool,
    /// True while a transition is scheduled or under way, so the control side
    /// does not arrange a second one on top of it.
    transition_armed: AtomicBool,
    /// Master volume, as `f32` bits.
    volume: AtomicU32,
    /// Callbacks that could not take the command lock. A health signal, not an
    /// error: see the module docs.
    commands_deferred: AtomicU64,
    sample_rate: u32,
}

/// A consistent read of the audio thread's state.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub status: Status,
    pub position: f64,
    pub duration: f64,
    pub volume: f32,
    pub commands_deferred: u64,
}

impl Link {
    /// Public so the real-time test can drive an [`Engine`] without a device.
    pub fn new(sample_rate: u32) -> Link {
        Link {
            channel: Mutex::new(Channel {
                to_audio: VecDeque::with_capacity(COMMAND_CAPACITY),
                // Twice the command capacity, because one drain can retire at
                // most one buffer per queued command and the control thread
                // clears this before every send. That makes growth — and so a
                // reallocation on the audio thread — unreachable.
                retired: VecDeque::with_capacity(COMMAND_CAPACITY * 2),
            }),
            position: AtomicU64::new(0),
            duration: AtomicU64::new(0),
            status: AtomicU8::new(Status::Idle as u8),
            ended: AtomicBool::new(false),
            swapped: AtomicBool::new(false),
            transition_armed: AtomicBool::new(false),
            volume: AtomicU32::new(1.0f32.to_bits()),
            commands_deferred: AtomicU64::new(0),
            sample_rate,
        }
    }

    /// The rate the output stream runs at, fixed for the life of the app.
    ///
    /// Tracks are converted to it at load rather than the device being
    /// reconfigured per track — see `vapor_dsp::decode_for_playback`.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Enqueue a command, freeing anything the audio thread handed back.
    ///
    /// Returns false only if the queue is full, which needs 32 unserviced
    /// commands and therefore means the audio thread has stopped.
    fn send(&self, command: Command) -> bool {
        let (accepted, retired) = {
            let mut channel = self.channel.lock().unwrap_or_else(|e| e.into_inner());
            // Take the finished buffers out under the lock but drop them
            // outside it: freeing tens of megabytes while holding the lock
            // would make the audio thread defer its drain for that whole time.
            let retired: Vec<Vec<[f32; 2]>> = channel.retired.drain(..).collect();
            let accepted = channel.to_audio.len() < COMMAND_CAPACITY;
            if accepted {
                channel.to_audio.push_back(command);
            }
            (accepted, retired)
        };
        drop(retired);
        accepted
    }

    /// Hand a decoded track to the deck and start it.
    pub fn load(&self, frames: Vec<[f32; 2]>, play: bool) -> bool {
        self.send(Command::Load { frames, play })
    }

    /// Cue the next track on the other deck, silent.
    pub fn preload(&self, frames: Vec<[f32; 2]>) -> bool {
        self.send(Command::Preload { frames })
    }

    /// Arrange a beat-matched mix into the preloaded track.
    ///
    /// The alignment is computed by the caller — see `Mixer::schedule_prepared`
    /// for why the grids stay on this side of the boundary.
    pub fn schedule_transition(
        &self,
        kind: TransitionType,
        duration: f32,
        incoming_pos: f32,
        ratio: f64,
        start_at: f64,
    ) -> bool {
        self.send(Command::ScheduleTransition {
            kind,
            duration,
            incoming_pos,
            ratio,
            start_at,
        })
    }

    pub fn cancel_transition(&self) {
        self.send(Command::CancelTransition);
    }

    /// True while a mix is scheduled or under way.
    pub fn transition_armed(&self) -> bool {
        self.transition_armed.load(Ordering::Acquire)
    }

    /// True once per completed transition, for whoever advances the queue.
    pub fn take_swapped(&self) -> bool {
        self.swapped
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn play(&self) {
        self.send(Command::Play);
    }

    pub fn pause(&self) {
        self.send(Command::Pause);
    }

    pub fn stop(&self) {
        self.send(Command::Stop);
    }

    pub fn seek(&self, seconds: f64) {
        self.send(Command::Seek(seconds.max(0.0)));
    }

    /// Set master volume, 0.0 to 1.0.
    ///
    /// Applied after the mixer rather than as a deck gain: deck gain is written
    /// by the transition automation every block, so a volume set there would be
    /// overwritten mid-mix.
    pub fn set_volume(&self, volume: f32) {
        self.volume
            .store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            status: Status::from_u8(self.status.load(Ordering::Acquire)),
            position: f64::from_bits(self.position.load(Ordering::Relaxed)),
            duration: f64::from_bits(self.duration.load(Ordering::Relaxed)),
            volume: f32::from_bits(self.volume.load(Ordering::Relaxed)),
            commands_deferred: self.commands_deferred.load(Ordering::Relaxed),
        }
    }

    /// True once per track that reached its end, for whoever advances the
    /// queue. Consuming it here means two callers cannot both act on one
    /// ending.
    pub fn take_ended(&self) -> bool {
        self.ended
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

/// A live audio device.
///
/// Dropping this stops the stream and releases the device — the audio thread
/// blocks on a channel whose only sender lives here.
pub struct Player {
    link: Arc<Link>,
    _shutdown: mpsc::Sender<()>,
}

impl Deref for Player {
    type Target = Link;

    fn deref(&self) -> &Link {
        &self.link
    }
}

impl Player {
    /// Open the default output device and start its stream.
    ///
    /// Fails rather than panics when there is no device: CI runners have none,
    /// and neither does a desktop with every output unplugged. The rest of the
    /// app has to keep working — a library you cannot hear is still a library
    /// you can scan, browse and analyse.
    pub fn start() -> Result<Player, String> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
        let (link_tx, link_rx) = mpsc::channel::<Arc<Link>>();

        // The device lives on its own thread because `cpal::Stream` is not
        // `Send`, and Tauri's managed state must be. Everything the rest of the
        // app touches is the `Link`, which is.
        std::thread::Builder::new()
            .name("vapor-audio".to_string())
            .spawn(move || match open_stream() {
                Ok((stream, link)) => {
                    let rate = link.sample_rate;
                    let _ = link_tx.send(link);
                    if ready_tx.send(Ok(rate)).is_err() {
                        return;
                    }
                    // Hold the stream until the Player is dropped. `recv`
                    // returns an error when the last sender goes, which is
                    // exactly the shutdown signal.
                    let _ = shutdown_rx.recv();
                    drop(stream);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(|e| format!("could not start the audio thread: {e}"))?;

        match ready_rx.recv() {
            Ok(Ok(_rate)) => {
                let link = link_rx
                    .recv()
                    .map_err(|_| "the audio thread stopped before reporting".to_string())?;
                Ok(Player {
                    link,
                    _shutdown: shutdown_tx,
                })
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err("the audio thread stopped before reporting".to_string()),
        }
    }

    pub fn link(&self) -> Arc<Link> {
        Arc::clone(&self.link)
    }
}

/// What the audio thread believes it is doing, kept beside the deck's own flag.
///
/// The deck clears its own `playing` when the source runs out, which is
/// indistinguishable from a pause unless the intent is recorded separately.
/// Without this, stopping a track would be reported as the track ending, and
/// the queue would advance every time someone pressed stop.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Intent {
    Idle,
    Playing,
    Paused,
}

/// Everything the audio thread owns, and the one step it performs.
///
/// A struct rather than captured variables inside the callback closure, so the
/// real-time claim can be *measured* on the code that actually runs — see
/// `tests/audio_realtime.rs`. A closure is not reachable from a test, and a
/// test of something adjacent to the audio path proves nothing about the audio
/// path.
pub struct Engine {
    link: Arc<Link>,
    mixer: Mixer,
    /// Where the mixer renders before interleaving. Sized once, here.
    scratch: Vec<[f32; 2]>,
    intent: Intent,
    /// Volume at the end of the previous block, so the next one can ramp from
    /// it rather than step.
    gain: f32,
    channels: usize,
}

impl Engine {
    pub fn new(link: Arc<Link>, channels: usize) -> Engine {
        let gain = f32::from_bits(link.volume.load(Ordering::Relaxed));
        Engine {
            mixer: Mixer::new(link.sample_rate as f32, MAX_BLOCK),
            scratch: vec![[0.0f32; 2]; MAX_BLOCK],
            intent: Intent::Idle,
            gain,
            channels,
            link,
        }
    }

    /// One callback's worth of work: apply pending commands, render, publish.
    ///
    /// Everything here is allocation-free and lock-free. If the device asks for
    /// more frames than the scratch holds, the tail is silenced rather than the
    /// buffer being grown.
    pub fn render<T>(&mut self, out: &mut [T])
    where
        T: SizedSample + FromSample<f32>,
    {
        self.apply_commands();

        let frames = (out.len() / self.channels).min(self.scratch.len());
        let was_transitioning = self.mixer.is_transitioning();
        let produced = self.mixer.render(&mut self.scratch[..frames]);

        // A transition that was running and now is not has completed, and the
        // decks have changed roles: what was cued is now what is playing. The
        // queue has to catch up, and only the supervisor can do that — the
        // audio thread does not know what a queue is.
        if was_transitioning && !self.mixer.is_transitioning() {
            self.link.duration.store(
                self.mixer.outgoing().duration_seconds().to_bits(),
                Ordering::Relaxed,
            );
            self.link.swapped.store(true, Ordering::Release);
        }

        // Published every block rather than tracked by hand, so it cannot fall
        // out of step with what the mixer actually holds.
        self.link.transition_armed.store(
            self.mixer.is_transitioning() || self.mixer.has_pending_transition(),
            Ordering::Release,
        );

        let target = f32::from_bits(self.link.volume.load(Ordering::Relaxed));
        write_out(
            &self.scratch[..frames],
            out,
            self.channels,
            self.gain,
            target,
        );
        self.gain = target;

        // The source ran out. The deck cannot say so — it only knows it
        // stopped — hence the separate intent.
        //
        // `frames > 0` matters: a device that asks for an empty buffer produces
        // nothing for the same reason a finished track does, and without this
        // the queue would skip to the next song because a callback came back
        // empty.
        if self.intent == Intent::Playing && produced == 0 && frames > 0 {
            self.intent = Intent::Idle;
            self.link
                .status
                .store(Status::Idle as u8, Ordering::Release);
            self.link.ended.store(true, Ordering::Release);
        }

        self.link.position.store(
            self.mixer.outgoing().position_seconds().to_bits(),
            Ordering::Relaxed,
        );
    }

    /// Drain and apply pending commands. Never blocks; never allocates.
    fn apply_commands(&mut self) {
        let Ok(mut channel) = self.link.channel.try_lock() else {
            self.link.commands_deferred.fetch_add(1, Ordering::Relaxed);
            return;
        };

        while let Some(command) = channel.to_audio.pop_front() {
            match command {
                Command::Load { frames, play } => {
                    let seconds = frames.len() as f64 / self.link.sample_rate as f64;
                    // An explicit choice outranks a mix arranged before it was
                    // made. Without this, picking a track would be interrupted
                    // seconds later by a transition into something else.
                    self.mixer.cancel_transition();
                    let deck = self.mixer.outgoing();
                    // Reset what a previous track may have left behind. A deck
                    // still carrying a transition's EQ or stretch ratio would
                    // play the new track through the tail of the old mix.
                    deck.ratio = 1.0;
                    deck.set_gain_db(0.0);
                    deck.set_eq_db(0.0, 0.0, 0.0);
                    deck.set_sweep(None);
                    let previous = deck.swap_samples(frames);

                    debug_assert!(
                        channel.retired.len() < channel.retired.capacity(),
                        "the retired queue would grow, which allocates on the audio thread"
                    );
                    channel.retired.push_back(previous);

                    self.link
                        .duration
                        .store(seconds.to_bits(), Ordering::Relaxed);
                    self.link.position.store(0f64.to_bits(), Ordering::Relaxed);
                    if play {
                        self.mixer.outgoing().play();
                        self.intent = Intent::Playing;
                    } else {
                        self.intent = Intent::Paused;
                    }
                    self.link.status.store(
                        if play {
                            Status::Playing as u8
                        } else {
                            Status::Paused as u8
                        },
                        Ordering::Release,
                    );
                }

                Command::Preload { frames } => {
                    let deck = self.mixer.incoming();
                    // Silent until the transition's envelope raises it. The
                    // mixer's `activate` seeks and sets the ratio, so this only
                    // has to undo whatever the deck was left holding by a
                    // previous mix.
                    deck.ratio = 1.0;
                    deck.set_gain_db(-60.0);
                    deck.set_eq_db(0.0, 0.0, 0.0);
                    deck.set_sweep(None);
                    let previous = deck.swap_samples(frames);

                    debug_assert!(
                        channel.retired.len() < channel.retired.capacity(),
                        "the retired queue would grow, which allocates on the audio thread"
                    );
                    channel.retired.push_back(previous);
                }

                Command::ScheduleTransition {
                    kind,
                    duration,
                    incoming_pos,
                    ratio,
                    start_at,
                } => {
                    // The delay is derived here, against the playhead as it
                    // actually stands, rather than against wherever the control
                    // thread last saw it. A mix that starts a queue-latency
                    // late is a mix on the offbeat.
                    let now = self.mixer.outgoing().position_seconds();
                    let ahead = (start_at - now).max(0.0);
                    let delay_frames = (ahead * self.link.sample_rate as f64) as usize;
                    self.mixer
                        .schedule_prepared(kind, duration, incoming_pos, ratio, delay_frames);
                }

                Command::CancelTransition => self.mixer.cancel_transition(),

                // Resuming a finished track would restart it from its end and
                // immediately report another ending, so an idle deck ignores
                // this.
                Command::Play => {
                    if self.intent == Intent::Paused {
                        self.mixer.outgoing().play();
                        self.intent = Intent::Playing;
                        self.link
                            .status
                            .store(Status::Playing as u8, Ordering::Release);
                    }
                }

                Command::Pause => {
                    if self.intent == Intent::Playing {
                        // The deck keeps its read position, so this resumes
                        // where it left off rather than restarting.
                        self.mixer.outgoing().stop();
                        self.intent = Intent::Paused;
                        self.link
                            .status
                            .store(Status::Paused as u8, Ordering::Release);
                    }
                }

                Command::Stop => {
                    self.mixer.cancel_transition();
                    let deck = self.mixer.outgoing();
                    deck.stop();
                    deck.seek_seconds(0.0);
                    self.intent = Intent::Idle;
                    self.link.position.store(0f64.to_bits(), Ordering::Relaxed);
                    self.link
                        .status
                        .store(Status::Idle as u8, Ordering::Release);
                }

                Command::Seek(seconds) => {
                    self.mixer.outgoing().seek_seconds(seconds);
                    self.link
                        .position
                        .store(seconds.to_bits(), Ordering::Relaxed);
                }
            }
        }
    }
}

fn open_stream() -> Result<(cpal::Stream, Arc<Link>), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "No audio output device is available.".to_string())?;

    let supported = device
        .default_output_config()
        .map_err(|e| format!("The audio device reported no usable configuration: {e}"))?;

    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let rate = config.sample_rate.0;
    let channels = config.channels as usize;

    let link = Arc::new(Link::new(rate));

    // The device's own rate and channel count are taken as given rather than
    // requested. Forcing a configuration a device does not offer is how a
    // player ends up working on one machine and silent on the next.
    let stream = match format {
        cpal::SampleFormat::F32 => build::<f32>(&device, &config, channels, Arc::clone(&link)),
        cpal::SampleFormat::I16 => build::<i16>(&device, &config, channels, Arc::clone(&link)),
        cpal::SampleFormat::U16 => build::<u16>(&device, &config, channels, Arc::clone(&link)),
        other => Err(format!("Unsupported audio sample format: {other}")),
    }?;

    stream
        .play()
        .map_err(|e| format!("The audio stream would not start: {e}"))?;

    Ok((stream, link))
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    link: Arc<Link>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    // Built here, before the stream exists. Every allocation the audio path
    // needs — the mixer's internals, the scratch buffer — happens on this call.
    let mut engine = Engine::new(link, channels);

    device
        .build_output_stream(
            config,
            move |out: &mut [T], _: &cpal::OutputCallbackInfo| engine.render(out),
            |err| eprintln!("audio stream error: {err}"),
            None,
        )
        .map_err(|e| format!("Could not open the audio output stream: {e}"))
}

/// Interleave the rendered stereo frames into the device's buffer.
///
/// Gain ramps linearly across the block from `from` to `to`.
fn write_out<T>(rendered: &[[f32; 2]], out: &mut [T], channels: usize, from: f32, to: f32)
where
    T: SizedSample + FromSample<f32>,
{
    let step = if rendered.is_empty() {
        0.0
    } else {
        (to - from) / rendered.len() as f32
    };

    for (i, frame) in rendered.iter().enumerate() {
        let g = from + step * i as f32;
        let base = i * channels;
        if channels == 1 {
            // A mono device gets the sum, not the left channel — otherwise half
            // the recording is inaudible on it.
            out[base] = T::from_sample((frame[0] + frame[1]) * 0.5 * g);
        } else {
            out[base] = T::from_sample(frame[0] * g);
            out[base + 1] = T::from_sample(frame[1] * g);
            // Surround devices get silence beyond the front pair rather than a
            // copy, which would place the mix in every speaker at once.
            for c in 2..channels {
                out[base + c] = T::EQUILIBRIUM;
            }
        }
    }

    for v in out[rendered.len() * channels..].iter_mut() {
        *v = T::EQUILIBRIUM;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(n: usize, value: f32) -> Vec<[f32; 2]> {
        vec![[value, value * 0.5]; n]
    }

    /// The invariant the module depends on: the audio thread can retire a
    /// buffer per queued command without the deque ever growing.
    #[test]
    fn the_retired_queue_cannot_grow_within_one_drain() {
        let link = Link::new(44_100);
        for _ in 0..COMMAND_CAPACITY {
            assert!(link.load(frames(16, 0.0), false));
        }
        let channel = link.channel.lock().expect("lock");
        assert!(
            channel.retired.capacity() >= channel.to_audio.len(),
            "a full command queue could retire more buffers than the deque holds"
        );
    }

    /// A full queue must be refused rather than growing without bound.
    #[test]
    fn the_command_queue_is_bounded() {
        let link = Link::new(44_100);
        for _ in 0..COMMAND_CAPACITY {
            assert!(link.load(frames(4, 0.0), false));
        }
        assert!(
            !link.load(frames(4, 0.0), false),
            "the queue accepted more than its capacity"
        );
    }

    /// Sending frees whatever the audio thread handed back — that is the whole
    /// reason retired buffers travel back rather than being dropped in place.
    #[test]
    fn sending_a_command_clears_retired_buffers() {
        let link = Link::new(44_100);
        link.channel
            .lock()
            .expect("lock")
            .retired
            .push_back(frames(1024, 0.5));

        link.play();

        assert!(
            link.channel.lock().expect("lock").retired.is_empty(),
            "a retired buffer was left for the audio thread to free"
        );
    }

    /// An ending must be reported exactly once, or the queue advances twice on
    /// one track.
    #[test]
    fn an_ending_is_consumed_by_the_first_caller() {
        let link = Link::new(44_100);
        assert!(!link.take_ended());

        link.ended.store(true, Ordering::Release);
        assert!(link.take_ended());
        assert!(!link.take_ended(), "the same ending was reported twice");
    }

    #[test]
    fn volume_is_clamped_to_a_sane_range() {
        let link = Link::new(44_100);
        link.set_volume(4.0);
        assert_eq!(link.snapshot().volume, 1.0);
        link.set_volume(-1.0);
        assert_eq!(link.snapshot().volume, 0.0);
    }

    /// Stereo must reach the device as stereo. Collapsing it here would undo
    /// the point of decoding to stereo in the first place.
    #[test]
    fn stereo_frames_land_on_the_first_two_device_channels() {
        let rendered = vec![[0.8f32, -0.4]; 4];
        let mut out = vec![0.0f32; 4 * 2];
        write_out(&rendered, &mut out, 2, 1.0, 1.0);

        for i in 0..4 {
            assert!((out[i * 2] - 0.8).abs() < 1e-6);
            assert!((out[i * 2 + 1] + 0.4).abs() < 1e-6);
        }
    }

    /// A mono device must hear both channels, not just the left one.
    #[test]
    fn a_mono_device_receives_the_sum_of_both_channels() {
        let rendered = vec![[1.0f32, 0.0]; 2];
        let mut out = vec![0.0f32; 2];
        write_out(&rendered, &mut out, 1, 1.0, 1.0);

        for v in &out {
            assert!((v - 0.5).abs() < 1e-6, "got {v}");
        }
    }

    /// Extra speakers get silence; a copy would put the mix everywhere at once.
    #[test]
    fn surround_channels_beyond_the_front_pair_are_silent() {
        let rendered = vec![[0.5f32, 0.5]; 2];
        let mut out = vec![9.0f32; 2 * 6];
        write_out(&rendered, &mut out, 6, 1.0, 1.0);

        for i in 0..2 {
            for c in 2..6 {
                assert_eq!(out[i * 6 + c], 0.0, "channel {c} was not silent");
            }
        }
    }

    /// A device asking for more frames than were rendered must get silence, not
    /// whatever the buffer happened to contain.
    #[test]
    fn the_unrendered_tail_is_silenced() {
        let rendered = vec![[1.0f32, 1.0]; 2];
        let mut out = vec![9.0f32; 8 * 2];
        write_out(&rendered, &mut out, 2, 1.0, 1.0);

        assert!(
            out[4..].iter().all(|&v| v == 0.0),
            "stale samples were left in the tail: {out:?}"
        );
    }

    /// A jump in volume is a click. The ramp is what prevents it.
    #[test]
    fn volume_changes_are_ramped_across_the_block() {
        let rendered = vec![[1.0f32, 1.0]; 64];
        let mut out = vec![0.0f32; 64 * 2];
        write_out(&rendered, &mut out, 2, 0.0, 1.0);

        assert!(
            out[0].abs() < 1e-6,
            "the ramp did not start at the old gain"
        );
        let steps: Vec<f32> = (0..64).map(|i| out[i * 2]).collect();
        for pair in steps.windows(2) {
            assert!(pair[1] >= pair[0], "the ramp was not monotonic");
            assert!(
                pair[1] - pair[0] < 0.05,
                "the ramp stepped by {}, which is audible",
                pair[1] - pair[0]
            );
        }
    }
}
