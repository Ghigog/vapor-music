//! The decoder thread behind a streaming deck (TD-09).
//!
//! `vapor_dsp::stream::PlaybackStream` knows how to decode a track a chunk at a
//! time and `vapor_engine::source::Window` knows how to hold a few seconds of
//! one for the audio thread to read. This is the thread that joins them, and it
//! lives in the shell for the same reason the audio device does: `vapor-core`
//! owns no I/O and compiles to wasm, where there are no threads to spawn.
//!
//! ## What it is allowed to be
//!
//! Slow. It is a control-plane thread with no deadline — it may block on a
//! file, allocate, and sleep. Everything real-time about the design lives on
//! the other side of the window, and the whole point of the split is that this
//! side needs no discipline at all beyond keeping ahead.
//!
//! ## Why it sleeps rather than waits on a condition variable
//!
//! The Godot original (`AudioDSP::thread_loop`) used a condvar with a 10 ms
//! timeout, woken by the consumer after every read. Here the consumer is the
//! audio callback, and notifying a condition variable from it means taking a
//! lock it is not allowed to wait for. So the producer polls instead, at a
//! rate set by how much audio the window holds: with five seconds of buffer,
//! waking every few milliseconds is already thousands of times more often than
//! strictly needed.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use vapor_dsp::stream::PlaybackStream;
use vapor_engine::source::{Window, WINDOW_SECS};

/// Frames asked of the decoder at a time. One or two dozen packets.
const CHUNK: usize = 8192;

/// How long to sleep when there is nothing to do.
///
/// Short enough that a seek is served promptly, long enough that a decoder
/// which is comfortably ahead — the normal state — costs nothing.
const IDLE: std::time::Duration = std::time::Duration::from_millis(3);

/// How long a track load will wait for its first audio before giving up.
///
/// The file is already in the local cache by this point, so this is generous
/// to the point of being a liveness check rather than a real deadline.
const PREFILL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Audio buffered before playback is allowed to start.
///
/// One second. The decoder fills the remaining four while that second plays,
/// and it means a track opens with music rather than with a hole that the
/// person hears as a broken player.
const PREFILL_SECS: f32 = 1.0;

/// A track being decoded into a window, and the thread doing it.
///
/// Dropping this stops the thread and waits for it, which is why it must be
/// held by the control side and never by the audio thread.
pub struct Streamer {
    window: Arc<Window>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Streamer {
    /// Open `path`, start decoding it at `rate`, and wait for enough audio to
    /// begin playing.
    ///
    /// `from` is where playback will start: zero for a track being played, and
    /// the aligned cue position for one being cued for a mix. Waiting for
    /// *that* position rather than for the beginning is what lets a transition
    /// be armed without decoding the minutes of track in front of it.
    pub fn start(path: &Path, rate: u32, from: u64) -> Result<Streamer, String> {
        let mut stream = PlaybackStream::open(path, rate).map_err(|e| e.to_string())?;

        let window = Arc::new(Window::for_seconds(rate, WINDOW_SECS));
        if let Some(total) = stream.total_frames() {
            window.set_total(total);
        }

        // Seeking before the thread starts, so the first thing decoded is audio
        // that will actually be heard.
        let start_at = if from > 0 {
            let landed = stream.seek(from).map_err(|e| e.to_string())?;
            window.restart_at(landed);
            landed
        } else {
            0
        };

        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let window = Arc::clone(&window);
            let stop = Arc::clone(&stop);
            std::thread::Builder::new()
                .name("vapor-decode".to_string())
                .spawn(move || run(stream, &window, &stop))
                .map_err(|e| format!("could not start the decoder thread: {e}"))?
        };

        let streamer = Streamer {
            window,
            stop,
            thread: Some(thread),
        };

        let prefill = (rate as f32 * PREFILL_SECS) as u64;
        if !streamer
            .window
            .wait_until_ready(start_at, prefill, PREFILL_TIMEOUT)
        {
            return Err("The track could not be decoded fast enough to play.".to_string());
        }

        Ok(streamer)
    }

    /// The window the audio thread reads from.
    pub fn window(&self) -> Arc<Window> {
        Arc::clone(&self.window)
    }

    /// The file decoded to nothing at all.
    ///
    /// The malformed-AAC case (TD-12), which the whole-file path reported as a
    /// decode error. Streaming meets it differently: the container probes fine
    /// and the decoder simply reaches the end having produced no frames, so the
    /// symptom is a track that is complete and empty rather than a failure.
    pub fn is_silent(&self) -> bool {
        self.window.is_complete() && self.window.total_frames() == Some(0)
    }
}

impl Drop for Streamer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            // Joining matters: without it a decoder for a track nobody is
            // listening to keeps reading files and filling a window until it
            // happens to notice the flag.
            let _ = t.join();
        }
    }
}

/// Keep `window` full until told to stop.
fn run(mut stream: PlaybackStream, window: &Window, stop: &AtomicBool) {
    // Frames decoded but not yet accepted — the window was full when they
    // arrived. Held rather than dropped, because dropping them would silently
    // skip audio.
    let mut pending: Vec<[i16; 2]> = Vec::with_capacity(CHUNK * 2);

    while !stop.load(Ordering::Acquire) {
        if let Some(target) = window.pending_seek() {
            serve_seek(&mut stream, window, &mut pending, target);
            continue;
        }

        // Hand over whatever is waiting first, in order.
        if !pending.is_empty() {
            let taken = window.write_frames(&pending);
            pending.drain(..taken);
            if taken == 0 {
                // The window is full, which is the normal state of a decoder
                // that is keeping up. Nothing to do until the consumer moves.
                std::thread::sleep(IDLE);
            }
            continue;
        }

        if stream.is_finished() {
            // Fixes the track's true length at what was decoded, and tells the
            // deck that running out of frames now means the end of the song
            // rather than a decoder that is behind.
            if !window.is_complete() {
                window.set_complete();
            }
            std::thread::sleep(IDLE);
            continue;
        }

        match stream.read(&mut pending, CHUNK) {
            Ok(_) => {}
            Err(e) => {
                // A file that stops decoding mid-track is not something the
                // audio thread can do anything about. Declaring the track over
                // ends it cleanly at the last good frame, which is what the
                // person hears as the song finishing early rather than as the
                // player hanging.
                eprintln!("decode stopped: {e}");
                window.set_complete();
                return;
            }
        }
    }
}

/// Move the window to `target`.
///
/// The frames a window holds are addressed by their position in the track, so a
/// target already inside it is already the right audio and needs no decoding at
/// all — which is what makes scrubbing a few seconds gapless. Only a jump
/// outside costs a seek.
fn serve_seek(
    stream: &mut PlaybackStream,
    window: &Window,
    pending: &mut Vec<[i16; 2]>,
    target: u64,
) {
    let (start, end) = window.span();
    if target < start || target > end {
        match stream.seek(target) {
            Ok(landed) => {
                pending.clear();
                window.restart_at(landed);
            }
            Err(e) => {
                // Refusing to move is better than moving somewhere wrong: the
                // deck keeps playing what it has. Marking the request served
                // stops this retrying every few milliseconds forever.
                eprintln!("seek failed: {e}");
            }
        }
    }
    window.served_seek();
}
