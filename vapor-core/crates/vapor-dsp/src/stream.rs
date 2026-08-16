//! Decoding a track a chunk at a time, for playback (TD-09).
//!
//! [`crate::decode_for_playback`] decodes a whole track into memory before a
//! note is heard: 55 MB for five minutes, and 110 MB with a second deck loaded
//! near a transition. This is the same conversion — decode, downmix to stereo,
//! convert the rate, quantise to `i16` — arranged so that a deck can hold a few
//! seconds of it instead of all of it.
//!
//! ## What is deliberately *not* here
//!
//! No thread, no buffer, no locking. This is a cursor over a file that answers
//! "give me the next few thousand frames" and "start again from here". The
//! thread that drives it and the window it fills belong to
//! [`vapor_engine::source`] and the shell, because they are the parts with
//! real-time rules attached. Keeping this half plain and synchronous is what
//! lets it be tested without any concurrency at all.
//!
//! ## Ported from the Godot build, not invented
//!
//! `AudioDSP` in `src/audio_dsp.cpp` already streamed: `load_cache_at` held a
//! 5-second window of the file, a background thread refilled it, and
//! `seek_pos` moved it. Two of its decisions are carried over deliberately —
//! the window size, and that a seek waits on the producer rather than the
//! consumer inventing samples. Two are not:
//!
//! * **It re-opened the file for every window** (a fresh `EasyLoader` with
//!   `startTime`/`endTime`). Sequential playback does not need that; the
//!   decoder here stays open and keeps going, and only a real seek costs a
//!   seek. On a compressed format that is the difference between decoding the
//!   run-up to every window and decoding each packet once.
//! * **It loaded tracks under 900 s whole anyway**, which is every track in the
//!   library, so in practice the streaming path was dead code for music and
//!   only ran for DJ sets. Nothing here has a length exception.

use std::path::{Path, PathBuf};

use symphonia::core::codecs::Decoder;
use symphonia::core::formats::{FormatReader, SeekMode, SeekTo};

use crate::decode::{self, DecodeError};
use crate::frame_to_i16;
use crate::resample::StreamResampler;

/// Frames decoded per `read` call before the resampler is asked for output.
///
/// A packet is typically 1024 (AAC) or 1152 (MP3) frames, so this is a handful
/// of packets: enough that the per-call overhead disappears, small enough that
/// a seek does not decode a wasted second first.
const DECODE_CHUNK: usize = 8192;

/// A track being decoded as it plays.
pub struct PlaybackStream {
    path: PathBuf,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    /// The file's own rate, before conversion.
    source_rate: u32,
    /// The device's rate, which every frame handed out is at.
    target_rate: u32,
    resampler: StreamResampler,
    /// Decoded frames awaiting rate conversion.
    decoded: Vec<[f32; 2]>,
    /// Converted frames awaiting quantisation.
    converted: Vec<[f32; 2]>,
    /// Length at the device's rate, if the container declared one.
    total: Option<u64>,
    finished: bool,
}

impl PlaybackStream {
    /// Open `path` and prepare to deliver frames at `target_rate`.
    ///
    /// Opening decodes nothing. The first [`read`](Self::read) is what touches
    /// audio, which keeps the cost of arming a track that is then cancelled to
    /// a file handle.
    pub fn open(path: &Path, target_rate: u32) -> Result<PlaybackStream, DecodeError> {
        let opened = decode::open(decode::source_from_path(path)?)?;
        let source_rate = opened.sample_rate;
        if source_rate == 0 {
            return Err(DecodeError::Unsupported(
                "the container declares no sample rate".into(),
            ));
        }

        let resampler = StreamResampler::new(source_rate, target_rate);
        // Taken from the container before any audio is decoded, so a deck knows
        // the track's length from the first frame rather than growing one as it
        // plays. `None` is possible and handled: a stream whose length is not
        // declared simply is not finished until the decoder says so.
        let total = opened.n_frames.map(|n| resampler.output_len_for(n));

        Ok(PlaybackStream {
            path: path.to_path_buf(),
            format: opened.format,
            decoder: opened.decoder,
            track_id: opened.track_id,
            source_rate,
            target_rate,
            resampler,
            decoded: Vec::with_capacity(DECODE_CHUNK),
            converted: Vec::with_capacity(DECODE_CHUNK),
            total,
            finished: false,
        })
    }

    /// Frames at the device's rate, when the container declared a length.
    pub fn total_frames(&self) -> Option<u64> {
        self.total
    }

    pub fn target_rate(&self) -> u32 {
        self.target_rate
    }

    /// True once the file has been decoded to its end.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// The next output frame this stream will produce.
    pub fn position(&self) -> u64 {
        self.resampler.output_position()
    }

    /// Decode until `out` holds `want` more frames, or the track ends.
    ///
    /// Appends rather than replacing, and returns how many frames were added —
    /// zero means the end of the track and nothing else.
    pub fn read(&mut self, out: &mut Vec<[i16; 2]>, want: usize) -> Result<usize, DecodeError> {
        let before = out.len();

        while out.len() - before < want && !self.finished {
            self.decoded.clear();
            self.decode_some()?;

            self.converted.clear();
            self.resampler.push(&self.decoded, &mut self.converted);
            if self.finished {
                // The kernel's tail: the frames whose taps run past the end of
                // the file. Without this a streamed track stops a few
                // milliseconds short of a loaded one.
                self.resampler.flush(&mut self.converted);
            }
            out.extend(self.converted.iter().copied().map(frame_to_i16));
        }

        Ok(out.len() - before)
    }

    /// Decode roughly [`DECODE_CHUNK`] frames into `self.decoded`.
    fn decode_some(&mut self) -> Result<(), DecodeError> {
        while self.decoded.len() < DECODE_CHUNK {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                // Symphonia signals clean end-of-stream as an io error of kind
                // UnexpectedEof; anything else is a real failure.
                Err(symphonia::core::errors::Error::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    self.finished = true;
                    return Ok(());
                }
                Err(symphonia::core::errors::Error::ResetRequired) => {
                    self.finished = true;
                    return Ok(());
                }
                Err(e) => return Err(DecodeError::Decode(e.to_string())),
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(buf) => decode::append(&buf, &mut decode::Stereo(&mut self.decoded)),
                // A corrupt packet mid-file must not end the track. The
                // whole-file path tolerates one because the analysis is
                // statistical; here it matters more, since the alternative is
                // playback stopping in the middle of a song.
                Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
                Err(e) => return Err(DecodeError::Decode(e.to_string())),
            }
        }
        Ok(())
    }

    /// Restart decoding at output frame `frame`.
    ///
    /// Returns the frame actually landed on, which is at or before the one
    /// asked for: a compressed stream can only resume at a packet boundary.
    /// The caller is told rather than the difference being hidden, because the
    /// window that holds these frames indexes them by absolute position.
    pub fn seek(&mut self, frame: u64) -> Result<u64, DecodeError> {
        let seconds = frame as f64 / self.target_rate as f64;

        let landed = match self.format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time: seconds.into(),
                track_id: Some(self.track_id),
            },
        ) {
            Ok(to) => to.actual_ts,
            // Not every container can seek. Re-opening and decoding forward is
            // slow and always works, which for the one case that needs it —
            // scrubbing a file whose index is missing — beats refusing.
            Err(_) => self.reopen_and_skip(seconds)?,
        };

        self.decoder.reset();
        self.decoded.clear();
        self.converted.clear();
        self.resampler.seek_to_input(landed);
        self.finished = false;
        Ok(self.resampler.output_position())
    }

    /// Seek by brute force: open the file again and throw away everything
    /// before the target. Returns the source frame reached.
    fn reopen_and_skip(&mut self, seconds: f64) -> Result<u64, DecodeError> {
        let opened = decode::open(decode::source_from_path(&self.path)?)?;
        self.format = opened.format;
        self.decoder = opened.decoder;
        self.track_id = opened.track_id;
        self.finished = false;

        let target = (seconds * self.source_rate as f64) as u64;
        let mut at = 0u64;
        while at < target && !self.finished {
            self.decoded.clear();
            self.decode_some()?;
            at += self.decoded.len() as u64;
        }
        self.decoded.clear();
        Ok(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A WAV of a known signal on disk. Synthetic on purpose — the analysis
    /// fixtures need a personal library (TD-43) and this does not.
    fn wav_file(rate: u32, secs: f32) -> PathBuf {
        // A counter rather than a timestamp: macOS resolves the clock coarsely
        // enough that two tests starting together collide on a name.
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "vapor-stream-test-{}-{}.wav",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));

        let n = (secs * rate as f32) as usize;
        let data_len = (n * 2 * 2) as u32;
        let mut v = Vec::with_capacity(44 + data_len as usize);
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data_len).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes()); // PCM
        v.extend_from_slice(&2u16.to_le_bytes()); // stereo
        v.extend_from_slice(&rate.to_le_bytes());
        v.extend_from_slice(&(rate * 4).to_le_bytes());
        v.extend_from_slice(&4u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data_len.to_le_bytes());

        for i in 0..n {
            let t = i as f32 / rate as f32;
            // Left and right deliberately differ, so a collapsed image or a
            // swapped channel is visible rather than silently equal.
            let l = (t * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            let r = (t * 660.0 * std::f32::consts::TAU).sin() * 0.25;
            for s in [l, r] {
                v.extend_from_slice(&((s * i16::MAX as f32) as i16).to_le_bytes());
            }
        }

        std::fs::write(&path, v).expect("write test wav");
        path
    }

    fn stream_all(path: &Path, rate: u32) -> Vec<[i16; 2]> {
        let mut s = PlaybackStream::open(path, rate).expect("open");
        let mut out = Vec::new();
        while !s.is_finished() {
            s.read(&mut out, 4096).expect("read");
        }
        out
    }

    /// The property that makes streaming safe to adopt: what comes out of the
    /// streaming path is what the whole-file path produces, sample for sample.
    ///
    /// Not "close enough" — identical. Anything else means playback would sound
    /// different depending on how the track happened to be loaded, and every
    /// measurement in `docs/MIGRATION.md` was taken through the loaded path.
    #[test]
    fn streaming_matches_decoding_the_whole_file() {
        let path = wav_file(44_100, 3.0);
        let whole = crate::decode_for_playback(&path, 44_100).expect("whole");
        let streamed = stream_all(&path, 44_100);
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            streamed.len(),
            whole.len(),
            "streamed {} frames, whole-file decode gave {}",
            streamed.len(),
            whole.len()
        );
        assert!(
            streamed == whole,
            "streamed audio differs from the loaded audio"
        );
    }

    /// And it must still hold when the rate has to be converted, which is where
    /// the chunk boundaries actually bite.
    #[test]
    fn streaming_matches_the_whole_file_across_a_rate_conversion() {
        let path = wav_file(44_100, 3.0);
        let whole = crate::decode_for_playback(&path, 48_000).expect("whole");
        let streamed = stream_all(&path, 48_000);
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            streamed.len(),
            whole.len(),
            "lengths differ after conversion"
        );
        assert!(
            streamed == whole,
            "rate-converted streaming differs from the loaded path"
        );
    }

    /// The length has to be known before the track has been decoded, or a deck
    /// cannot show a duration until the song is over.
    #[test]
    fn the_length_is_known_before_anything_is_decoded() {
        let path = wav_file(44_100, 2.0);
        let s = PlaybackStream::open(&path, 44_100).expect("open");
        let declared = s.total_frames();
        drop(s);

        let actual = stream_all(&path, 44_100).len() as u64;
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            declared,
            Some(actual),
            "the container's declared length disagrees with what was decoded"
        );
    }

    /// Seeking must land on the audio that is actually at that position, not
    /// merely somewhere plausible. Comparing against the whole-file decode at
    /// the same offset is what distinguishes the two.
    #[test]
    fn seeking_lands_on_the_right_audio() {
        let path = wav_file(44_100, 4.0);
        let whole = crate::decode_for_playback(&path, 44_100).expect("whole");

        let mut s = PlaybackStream::open(&path, 44_100).expect("open");
        let landed = s.seek(44_100 * 2).expect("seek");
        let mut out = Vec::new();
        s.read(&mut out, 4096).expect("read");
        let _ = std::fs::remove_file(&path);

        assert!(
            landed <= 44_100 * 2,
            "a seek landed past its target, which would skip audio"
        );

        // Compare from where it actually landed — a packet boundary is allowed,
        // being wrong about the audio there is not.
        let expected = &whole[landed as usize..landed as usize + out.len().min(2048)];
        let got = &out[..expected.len()];
        assert!(
            got == expected,
            "the audio after a seek is not the audio at that position"
        );
    }

    /// Seeking backwards has to work as well as forwards; the window behind the
    /// playhead is exactly what the WSOLA search reads into.
    #[test]
    fn seeking_backwards_returns_to_earlier_audio() {
        let path = wav_file(44_100, 4.0);
        let whole = crate::decode_for_playback(&path, 44_100).expect("whole");

        let mut s = PlaybackStream::open(&path, 44_100).expect("open");
        s.seek(44_100 * 3).expect("forward");
        let landed = s.seek(44_100 / 2).expect("backward");
        let mut out = Vec::new();
        s.read(&mut out, 4096).expect("read");
        let _ = std::fs::remove_file(&path);

        let expected = &whole[landed as usize..landed as usize + 2048];
        assert!(
            &out[..2048] == expected,
            "seeking backwards did not return to the earlier audio"
        );
    }
}
