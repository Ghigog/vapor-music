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
        let owned = path.to_path_buf();
        Self::start_with(
            Box::new(move || vapor_dsp::decode::source_from_path(&owned)),
            rate,
            from,
        )
    }

    /// Start from a source that is not a file — a track still arriving over the
    /// network, in practice. See `crate::remote_source`.
    ///
    /// The closure may be called more than once: seeking falls back to opening
    /// the track again when the container cannot seek by itself.
    pub fn start_with(
        reopen: vapor_dsp::stream::ReopenSource,
        rate: u32,
        from: u64,
    ) -> Result<Streamer, String> {
        let mut stream = PlaybackStream::open_with(reopen, rate).map_err(|e| e.to_string())?;

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

#[cfg(test)]
mod tests {
    use super::*;

    /// The device rate every test decodes at, and the rate the fixtures are
    /// written at. Equal on purpose: `StreamResampler` passes through when the
    /// two match, so a frame's value survives the decode unchanged and the
    /// window can be asserted against sample by sample.
    const RATE: u32 = 8_000;

    /// Three seconds. Comfortably inside a window, which holds five.
    const FRAMES: u64 = 24_000;

    /// The sample value at frame `i`.
    ///
    /// A ramp that repeats every 512 frames, so a sample says which frame it
    /// came from. Adjacent frames differ by 60, which is far more than the
    /// quantisation this makes a round trip through — the point is that a seek
    /// landing one frame off is visible, not that the arithmetic is exact.
    fn ramp(i: u64) -> i16 {
        ((i % 512) as i32 * 60 - 15_360) as i16
    }

    /// Which frame a sample came from, modulo the ramp's period.
    fn frame_of(sample: i16) -> u64 {
        ((sample as i32 + 15_360) as f32 / 60.0).round() as u64
    }

    /// A 16-bit stereo WAV of `frames` frames, left channel ramped and right
    /// its negation. Real enough for symphonia to decode and seek, without
    /// needing anyone's music.
    fn ramped_wav(frames: u64) -> Vec<u8> {
        let data_len = (frames * 4) as u32;
        let mut v = Vec::with_capacity(44 + data_len as usize);
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data_len).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes()); // PCM
        v.extend_from_slice(&2u16.to_le_bytes()); // stereo
        v.extend_from_slice(&RATE.to_le_bytes());
        v.extend_from_slice(&(RATE * 4).to_le_bytes());
        v.extend_from_slice(&4u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data_len.to_le_bytes());
        for i in 0..frames {
            v.extend_from_slice(&ramp(i).to_le_bytes());
            v.extend_from_slice(&(-ramp(i)).to_le_bytes());
        }
        v
    }

    /// A file in its own directory, removed when the returned guard drops.
    ///
    /// A counter rather than a timestamp, for the reason the suite in `cache`
    /// already documents: macOS resolves the clock coarsely enough that two
    /// tests starting together collide on the name.
    struct Fixture(std::path::PathBuf);

    impl Fixture {
        fn new(bytes: &[u8]) -> Fixture {
            use std::sync::atomic::AtomicU64;
            static SEQ: AtomicU64 = AtomicU64::new(0);

            let dir = std::env::temp_dir().join(format!(
                "vapor-decoder-test-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).expect("fixture dir");
            let path = dir.join("track.wav");
            std::fs::write(&path, bytes).expect("fixture file");
            Fixture(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Some(dir) = self.0.parent() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }

    /// Poll `done` until it is true or a second has passed.
    ///
    /// The decoder is a thread with no handshake other than the window itself,
    /// so every assertion about what it has produced is eventually-true. A
    /// second is thousands of times what decoding three seconds of PCM takes.
    fn settles(mut done: impl FnMut() -> bool) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while std::time::Instant::now() < deadline {
            if done() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        done()
    }

    #[test]
    fn a_track_started_at_the_beginning_holds_its_first_frame() {
        let fixture = Fixture::new(&ramped_wav(FRAMES));
        let streamer = Streamer::start(fixture.path(), RATE, 0).expect("a plain WAV should open");

        let window = streamer.window();
        assert_eq!(
            window.span().0,
            0,
            "a track played from the top began somewhere else"
        );
        let first = window.view().get(0).expect("frame zero should be decoded");
        assert_eq!(
            frame_of(first[0]),
            0,
            "the frame at position zero is not the first frame of the file"
        );
    }

    #[test]
    fn a_track_cued_part_way_in_does_not_decode_what_comes_before_it() {
        // The whole point of cueing at a position rather than at the start:
        // arming a transition must not cost the minutes of track in front of
        // the cue point.
        let cue = 16_000;
        let fixture = Fixture::new(&ramped_wav(FRAMES));
        let streamer = Streamer::start(fixture.path(), RATE, cue).expect("open");

        let window = streamer.window();
        let (start, end) = window.span();
        assert!(
            start > 0 && start <= cue,
            "a cue at {cue} left the window starting at {start}"
        );
        assert!(end > start, "nothing was decoded at the cue point");
        assert!(
            window.view().get(0).is_none(),
            "the window holds frame zero, so the decoder read the track from the top"
        );
    }

    #[test]
    fn the_audio_at_a_cue_point_is_the_audio_from_that_point() {
        // A seek reports where it landed because the window indexes frames by
        // their absolute position in the track. If the landing frame and the
        // window's origin ever disagree, every frame in the window is the
        // wrong audio by the size of the disagreement — inaudible on a cue,
        // and exactly the error a beat-matched transition is built on.
        let fixture = Fixture::new(&ramped_wav(FRAMES));
        let streamer = Streamer::start(fixture.path(), RATE, 16_000).expect("open");

        let window = streamer.window();
        let (start, end) = window.span();
        let view = window.view();
        for probe in [start, start + 1, start + 733, end - 1] {
            let frame = view
                .get(probe)
                .unwrap_or_else(|| panic!("frame {probe} is inside the span but not readable"));
            assert_eq!(
                frame_of(frame[0]),
                probe % 512,
                "the window says frame {probe} but the audio there came from somewhere else"
            );
        }
    }

    #[test]
    fn a_seek_inside_the_window_costs_no_decoding_and_does_not_reopen_the_track() {
        let fixture = Fixture::new(&ramped_wav(FRAMES));
        let streamer = Streamer::start(fixture.path(), RATE, 0).expect("open");
        let window = streamer.window();

        assert!(
            settles(|| window.is_complete()),
            "a three-second WAV did not decode within a second"
        );
        let before = window.span();

        window.request_seek(12_000);
        assert!(
            settles(|| !window.seek_pending()),
            "the seek was never served"
        );

        assert_eq!(
            window.span(),
            before,
            "a seek to audio the window already held threw that audio away"
        );
        assert!(
            window.is_complete(),
            "a seek within a finished track made it unfinished again, so the deck \
             will wait for frames that are never coming"
        );
    }

    #[test]
    fn a_seek_outside_the_window_moves_the_window_to_it() {
        let fixture = Fixture::new(&ramped_wav(FRAMES));
        let streamer = Streamer::start(fixture.path(), RATE, 16_000).expect("open");
        let window = streamer.window();
        assert!(window.span().0 > 0, "the cue did not take");

        // Backwards, out of everything the window holds.
        window.request_seek(0);
        assert!(
            settles(|| !window.seek_pending()),
            "the seek was never served"
        );
        assert!(
            settles(|| window.view().get(0).is_some()),
            "a seek to the top of the track produced no audio there"
        );

        assert_eq!(
            window.span().0,
            0,
            "the window did not move back to the seek target"
        );
        let frame = window.view().get(0).expect("frame zero");
        assert_eq!(
            frame_of(frame[0]),
            0,
            "the window claims frame zero but holds audio from elsewhere"
        );
    }

    #[test]
    fn a_file_that_decodes_to_no_audio_is_silent_rather_than_a_failure() {
        // TD-12's malformed AAC, in the shape streaming meets it: the
        // container probes fine and the decoder reaches the end having
        // produced nothing. A track that is complete and empty, not an error.
        let fixture = Fixture::new(&ramped_wav(0));
        let streamer = Streamer::start(fixture.path(), RATE, 0)
            .expect("a container that decodes to nothing is not an open failure");

        assert!(
            streamer.is_silent(),
            "a file that produced no frames at all was not reported as silent"
        );
    }

    #[test]
    fn a_track_with_audio_is_not_silent_once_it_finishes() {
        let fixture = Fixture::new(&ramped_wav(FRAMES));
        let streamer = Streamer::start(fixture.path(), RATE, 0).expect("open");
        let window = streamer.window();

        assert!(
            settles(|| window.is_complete()),
            "a three-second WAV did not decode within a second"
        );
        assert!(
            !streamer.is_silent(),
            "a track that decoded {FRAMES} frames was reported as silent"
        );
        assert_eq!(
            window.total_frames(),
            Some(FRAMES),
            "finishing the track did not fix its length at what was actually decoded"
        );
    }

    #[test]
    fn a_file_that_is_not_audio_at_all_is_an_error_rather_than_silence() {
        // The distinction `is_silent` exists to make. A file that cannot be
        // opened must fail loudly here, not arrive at the deck as a track that
        // is complete and empty.
        let fixture = Fixture::new(b"this is not a RIFF chunk and never was");
        assert!(
            Streamer::start(fixture.path(), RATE, 0).is_err(),
            "a file with no container in it opened as a track"
        );
    }

    #[test]
    fn dropping_a_streamer_stops_its_thread_rather_than_leaving_it_reading() {
        let fixture = Fixture::new(&ramped_wav(FRAMES));
        let streamer = Streamer::start(fixture.path(), RATE, 0).expect("open");

        let at = std::time::Instant::now();
        drop(streamer);
        let took = at.elapsed();

        // The loop checks the flag between sleeps of three milliseconds, so
        // this is four orders of magnitude of slack. It is a liveness check:
        // a `Drop` that stopped joining, or joined without setting the flag,
        // fails it rather than hanging the suite forever.
        assert!(
            took < std::time::Duration::from_secs(1),
            "dropping a streamer took {took:?}, so its thread is not being stopped"
        );
    }
}
