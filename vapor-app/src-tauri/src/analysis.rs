//! Library analysis.
//!
//! Runs `vapor-dsp` over scanned tracks to fill in BPM, key, beat grid and cue
//! points. Until this runs, a scanned row knows its name and nothing else, so
//! the Vibe DJ has nothing to work with.
//!
//! ## Why this is a background job rather than a command
//!
//! Analysis is ~0.5 s per track, so a 1,000-track library is roughly ten
//! minutes. That cannot be a blocking call, and it cannot be all-or-nothing
//! either: a person should be able to play the tracks that are already done
//! while the rest continue.
//!
//! So results are published per track as they land, and the cache is written
//! incrementally. Quitting halfway loses only the track in flight.
//!
//! ## What is not here
//!
//! Downloading. Analysis needs local bytes, and fetching them is the cache
//! layer's job — this takes a path and reads it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Everything analysis learns about a track.
///
/// Persisted, so a library is analysed once rather than on every launch.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub bpm: f32,
    /// Camelot key.
    pub key: String,
    /// Beat positions in seconds, covering the whole track.
    #[serde(default)]
    pub beats: Vec<f32>,
    pub cue_in: f32,
    pub cue_out: f32,
    pub lufs: f32,
    pub duration: f64,
    /// Envelope peaks for drawing a waveform. See `vapor_dsp::WAVEFORM_BINS`.
    #[serde(default)]
    pub waveform: Vec<f32>,
    /// Camelot key at each end of the track (TD-13). Empty on a track too
    /// short for a segment to differ from the whole.
    #[serde(default)]
    pub intro_key: String,
    #[serde(default)]
    pub outro_key: String,
    /// Perceived energy, 0–1 — loudness, brightness and tempo (TD-42b).
    #[serde(default)]
    pub energy: f32,
    /// Schema version, so a future change to what analysis produces can
    /// invalidate stale entries instead of silently mixing formats.
    #[serde(default)]
    pub version: u32,
}

/// Bump when the meaning of any field changes. Entries at a lower version are
/// re-analysed rather than trusted.
///
/// 3 added segment keys and a real energy figure; 2 added the waveform. A `#[serde(default)]` empty vector would have avoided
/// the re-analysis, and would have left a library where some tracks draw a
/// waveform and others cannot, with no way to tell why or to fix it short of
/// deleting everything. Invalidating is what this constant is for — the pass is
/// resumable, cached per track, and now has a progress bar and a stop button.
pub const ANALYSIS_VERSION: u32 = 3;

/// Analysis results keyed by href.
pub type Cache = HashMap<String, Analysis>;

/// Progress, reported to the UI as the pass runs.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    /// The track just finished, so the table can update one row rather than
    /// re-querying the whole library.
    pub href: String,
    pub analysis: Option<Analysis>,
    /// Present when this track failed. The pass continues either way — one
    /// unreadable file must not stop a library-wide analysis.
    pub error: Option<String>,
    /// Whether trying again could plausibly succeed.
    ///
    /// "Not downloaded yet" is retryable; "decodes to zero samples" is not. The
    /// distinction is what lets the shell remember the second kind and refuse
    /// to play it, without also condemning every track that simply had not been
    /// fetched when the pass ran (TD-12).
    pub retryable: bool,
}

/// Analyse one decoded file.
pub fn analyse_file(path: &Path) -> Result<Analysis, String> {
    let result = vapor_dsp::analyze_file(path).map_err(|e| e.to_string())?;
    Ok(Analysis {
        bpm: result.bpm,
        key: result.camelot,
        beats: result.beats,
        cue_in: result.cue_in,
        cue_out: result.cue_out,
        lufs: result.lufs,
        duration: result.duration_secs,
        waveform: result.waveform,
        intro_key: result.intro_key,
        outro_key: result.outro_key,
        energy: result.energy,
        version: ANALYSIS_VERSION,
    })
}

/// Which tracks still need work.
///
/// Skips anything already analysed at the current version. This is what makes
/// a second launch instant rather than another ten minutes.
pub fn pending(hrefs: &[String], cache: &Cache) -> Vec<String> {
    hrefs
        .iter()
        .filter(|h| cache.get(*h).is_none_or(|a| a.version < ANALYSIS_VERSION))
        .cloned()
        .collect()
}

/// A running pass, so the UI can stop one.
///
/// Cancellation matters more than it looks: analysis saturates a core for
/// minutes, and a person who started it by accident on a 5,000-track library
/// needs a way out that is not force-quitting.
#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stop(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_stopped(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub fn reset(&self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

/// Run a pass, calling `on_progress` after each track.
///
/// Synchronous and single-threaded by design at this stage: analysis is
/// CPU-bound, and a parallel version should be measured rather than assumed —
/// saturating every core makes the rest of the app stutter, which is a worse
/// experience than a slower scan.
pub fn run<F>(
    hrefs: &[String],
    resolve: impl Fn(&str) -> Option<PathBuf>,
    cancel: &Cancel,
    mut on_progress: F,
) where
    F: FnMut(Progress),
{
    let total = hrefs.len();

    for (i, href) in hrefs.iter().enumerate() {
        if cancel.is_stopped() {
            return;
        }

        // Resolved separately from analysed, because the two failures mean
        // opposite things: one is "come back later", the other is "this file
        // will never work".
        let (outcome, retryable) = match resolve(href) {
            Some(path) => (analyse_file(&path), false),
            None => (Err("not available locally".to_string()), true),
        };

        let (analysis, error) = match outcome {
            Ok(a) => (Some(a), None),
            Err(e) => (None, Some(e)),
        };

        on_progress(Progress {
            done: i + 1,
            total,
            href: href.clone(),
            analysis,
            error,
            retryable,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analysed(version: u32) -> Analysis {
        Analysis {
            bpm: 128.0,
            key: "8A".into(),
            version,
            ..Default::default()
        }
    }

    #[test]
    fn already_analysed_tracks_are_skipped() {
        let mut cache = Cache::new();
        cache.insert("/a.mp3".into(), analysed(ANALYSIS_VERSION));

        let todo = pending(&["/a.mp3".into(), "/b.mp3".into()], &cache);
        assert_eq!(todo, vec!["/b.mp3"], "a second launch should be instant");
    }

    /// A schema change must invalidate stale entries rather than silently
    /// mixing formats.
    #[test]
    fn stale_versions_are_re_analysed() {
        let mut cache = Cache::new();
        cache.insert("/a.mp3".into(), analysed(ANALYSIS_VERSION - 1));

        let todo = pending(&["/a.mp3".into()], &cache);
        assert_eq!(todo, vec!["/a.mp3"]);
    }

    /// One unreadable file must not stop a library-wide pass.
    #[test]
    fn a_failure_does_not_halt_the_pass() {
        let hrefs: Vec<String> = vec!["/a.mp3".into(), "/b.mp3".into(), "/c.mp3".into()];
        let mut seen = Vec::new();

        run(
            &hrefs,
            // Nothing resolves, so every track fails.
            |_| None,
            &Cancel::new(),
            |p| seen.push((p.href, p.error.is_some())),
        );

        assert_eq!(seen.len(), 3, "the pass stopped early: {seen:?}");
        assert!(seen.iter().all(|(_, failed)| *failed));
    }

    /// A file that is merely not downloaded yet must not be remembered as
    /// broken — otherwise a pass run before the cache warmed would condemn the
    /// whole library.
    #[test]
    fn a_missing_file_is_retryable_and_a_broken_one_is_not() {
        let mut seen = Vec::new();
        run(
            &["/absent.mp3".to_string()],
            |_| None,
            &Cancel::new(),
            |p| seen.push(p.retryable),
        );
        assert_eq!(seen, vec![true], "an uncached file was condemned");

        // A file that resolves but cannot be analysed is the other case: the
        // path exists, the bytes are unusable.
        let mut seen = Vec::new();
        run(
            &["/broken.mp3".to_string()],
            |_| Some(PathBuf::from("/nonexistent/vapor-broken-fixture.mp3")),
            &Cancel::new(),
            |p| seen.push(p.retryable),
        );
        assert_eq!(seen, vec![false], "an unreadable file was marked retryable");
    }

    #[test]
    fn progress_counts_up_to_the_total() {
        let hrefs: Vec<String> = (0..5).map(|i| format!("/{i}.mp3")).collect();
        let mut last = None;

        run(
            &hrefs,
            |_| None,
            &Cancel::new(),
            |p| {
                last = Some((p.done, p.total));
            },
        );

        assert_eq!(last, Some((5, 5)));
    }

    /// Cancelling has to actually stop it — the reason it exists is a person
    /// who started a pass over 5,000 tracks by accident.
    #[test]
    fn cancelling_stops_the_pass() {
        let hrefs: Vec<String> = (0..100).map(|i| format!("/{i}.mp3")).collect();
        let cancel = Cancel::new();
        let mut count = 0usize;

        run(
            &hrefs,
            |_| None,
            &cancel,
            |_| {
                count += 1;
                if count == 3 {
                    cancel.stop();
                }
            },
        );

        assert_eq!(count, 3, "kept going after cancellation");
    }

    #[test]
    fn a_cancel_can_be_reused_after_reset() {
        let cancel = Cancel::new();
        cancel.stop();
        assert!(cancel.is_stopped());
        cancel.reset();
        assert!(!cancel.is_stopped());
    }
}
