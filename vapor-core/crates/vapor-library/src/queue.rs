//! Playback queue and history.
//!
//! The control-plane half of `audio_manager.gd`: what plays next, what played
//! before, and how back/forward navigation interacts with both. The audio half
//! lives in `vapor-engine`; this decides *which* track, never *how* it sounds.
//!
//! History here is a browser-style trail, not a log. Going back and then
//! forward retraces the same path, and playing something new from the middle
//! truncates the forward branch — the GDScript did this with a `history_pointer`
//! and a `_navigating_history` flag threaded through several methods, which is
//! the sort of implicit state that only stays correct while someone remembers
//! it exists. Here the navigation state is the type.

use serde::{Deserialize, Serialize};

/// Longest trail kept. From `audio_manager.gd`.
const MAX_HISTORY: usize = 100;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Queue {
    tracks: Vec<String>,
    /// Index into `tracks`, or `None` when nothing is playing.
    current: Option<usize>,

    /// Trail of played tracks, oldest first.
    history: Vec<String>,
    /// Position within `history` when navigating back and forth.
    history_pos: Option<usize>,

    /// One-shot override for the next track, set by "play this next".
    override_next: Option<String>,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tracks(&self) -> &[String] {
        &self.tracks
    }

    pub fn history(&self) -> &[String] {
        &self.history
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current
    }

    pub fn current(&self) -> Option<&str> {
        self.current.map(|i| self.tracks[i].as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// Replace the queue and start at `start`, recording it as played.
    pub fn set_tracks(&mut self, tracks: Vec<String>, start: Option<&str>) {
        self.tracks = tracks;
        self.current = start
            .and_then(|s| self.tracks.iter().position(|t| t == s))
            .or(if self.tracks.is_empty() {
                None
            } else {
                Some(0)
            });
        self.override_next = None;
        if let Some(href) = self.current().map(str::to_string) {
            self.record(&href);
        }
    }

    /// Queue a specific track to play next, overriding normal ordering.
    ///
    /// Ignored when the track is not in the queue, since playing something
    /// outside the current context would silently change what the user is
    /// listening to.
    pub fn set_next(&mut self, href: &str) -> bool {
        if self.tracks.iter().any(|t| t == href) {
            self.override_next = Some(href.to_string());
            true
        } else {
            false
        }
    }

    pub fn pending_next(&self) -> Option<&str> {
        self.override_next.as_deref()
    }

    /// What plays after the current track.
    ///
    /// `smart_choice` is the pathfinder's suggestion; it is consulted only when
    /// an override is not set. Passing it in rather than calling the pathfinder
    /// keeps this free of the cost model and testable on its own.
    pub fn peek_next(&self, smart_choice: Option<&str>) -> Option<&str> {
        if self.tracks.is_empty() {
            return None;
        }
        if let Some(o) = &self.override_next {
            return Some(o);
        }
        if let Some(s) = smart_choice {
            // Return the queue's own copy, not the caller's reference: the
            // result borrows from `self`, and they are the same string anyway.
            if let Some(t) = self.tracks.iter().find(|t| *t == s) {
                return Some(t.as_str());
            }
        }
        self.current
            .map(|i| self.tracks[(i + 1) % self.tracks.len()].as_str())
    }

    /// Advance, preferring the forward history branch if one exists.
    ///
    /// Returns the new current track.
    pub fn next(&mut self, smart_choice: Option<&str>) -> Option<&str> {
        if self.tracks.is_empty() {
            return None;
        }

        // An explicit override wins over retracing.
        //
        // The GDScript checked forward history first, so asking for a specific
        // track and then pressing next gave you whatever you had already played
        // instead. An explicit request beating an implicit retrace is the
        // behaviour a person expects, so this deliberately differs.
        if self.override_next.is_none() {
            if let Some(pos) = self.history_pos {
                if pos + 1 < self.history.len() {
                    let href = self.history[pos + 1].clone();
                    if let Some(i) = self.tracks.iter().position(|t| *t == href) {
                        self.history_pos = Some(pos + 1);
                        self.current = Some(i);
                        return self.current();
                    }
                }
            }
        }

        let chosen = self
            .peek_next(smart_choice)
            .map(str::to_string)
            .and_then(|h| self.tracks.iter().position(|t| *t == h))
            .unwrap_or_else(|| self.current.map_or(0, |i| (i + 1) % self.tracks.len()));

        self.current = Some(chosen);
        self.override_next = None;
        let href = self.tracks[chosen].clone();
        self.record(&href);
        self.current()
    }

    /// Step back through history, or wrap backwards through the queue when
    /// there is no history to step through.
    pub fn previous(&mut self) -> Option<&str> {
        if self.tracks.is_empty() {
            return None;
        }
        self.override_next = None;

        if let Some(pos) = self.history_pos {
            if pos > 0 {
                let href = self.history[pos - 1].clone();
                if let Some(i) = self.tracks.iter().position(|t| *t == href) {
                    self.history_pos = Some(pos - 1);
                    self.current = Some(i);
                    return self.current();
                }
            }
        }

        let i = self.current.unwrap_or(0);
        self.current = Some((i + self.tracks.len() - 1) % self.tracks.len());
        self.current()
    }

    /// Record a track as played.
    ///
    /// Playing something new while partway back through history truncates the
    /// forward branch — the same rule a browser applies, and the reason this is
    /// a trail rather than a log.
    fn record(&mut self, href: &str) {
        if let Some(pos) = self.history_pos {
            if pos + 1 < self.history.len() {
                self.history.truncate(pos + 1);
            }
        }
        if self.history.last().map(String::as_str) != Some(href) {
            self.history.push(href.to_string());
            if self.history.len() > MAX_HISTORY {
                self.history.remove(0);
            }
        }
        self.history_pos = Some(self.history.len() - 1);
    }

    /// Up to three tracks to prefetch and analyse: now playing, next, and the
    /// one after.
    pub fn lookahead(&self, smart_choice: Option<&str>) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let Some(now) = self.current() else {
            return out;
        };
        out.push(now.to_string());

        if let Some(next) = self.peek_next(smart_choice) {
            if !out.iter().any(|t| t == next) {
                out.push(next.to_string());
            }
            if let Some(i) = self.tracks.iter().position(|t| t == next) {
                if self.tracks.len() > 1 {
                    let after = &self.tracks[(i + 1) % self.tracks.len()];
                    if !out.iter().any(|t| t == after) {
                        out.push(after.clone());
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue() -> Queue {
        let mut q = Queue::new();
        q.set_tracks(
            vec!["a".into(), "b".into(), "c".into(), "d".into()],
            Some("a"),
        );
        q
    }

    #[test]
    fn advances_and_wraps() {
        let mut q = queue();
        assert_eq!(q.current(), Some("a"));
        assert_eq!(q.next(None), Some("b"));
        q.next(None);
        q.next(None);
        assert_eq!(q.next(None), Some("a"), "should wrap to the start");
    }

    #[test]
    fn an_override_takes_precedence_and_is_consumed() {
        let mut q = queue();
        assert!(q.set_next("d"));
        assert_eq!(q.peek_next(None), Some("d"));
        assert_eq!(q.next(None), Some("d"));
        assert!(q.pending_next().is_none(), "override is one-shot");
    }

    /// Playing something outside the queue would silently change context.
    #[test]
    fn an_override_outside_the_queue_is_refused() {
        let mut q = queue();
        assert!(!q.set_next("zzz"));
        assert!(q.pending_next().is_none());
    }

    #[test]
    fn a_smart_choice_is_used_when_no_override_is_set() {
        let mut q = queue();
        assert_eq!(q.peek_next(Some("c")), Some("c"));
        q.set_next("d");
        assert_eq!(q.peek_next(Some("c")), Some("d"), "override wins");
    }

    /// Back then forward must retrace the same path rather than re-deciding.
    #[test]
    fn back_then_forward_retraces_history() {
        let mut q = queue();
        q.next(None); // b
        q.next(None); // c
        assert_eq!(q.previous(), Some("b"));
        assert_eq!(q.next(None), Some("c"), "forward should retrace");
    }

    /// The browser rule: playing something new from partway back drops the
    /// forward branch.
    #[test]
    fn playing_something_new_truncates_the_forward_branch() {
        let mut q = queue();
        q.next(None); // b
        q.next(None); // c
        q.previous(); // back to b
        q.set_next("d");
        q.next(None); // d — a new branch

        assert_eq!(
            q.history(),
            &["a".to_string(), "b".to_string(), "d".to_string()],
            "c should have been dropped from the trail"
        );
    }

    #[test]
    fn previous_wraps_when_there_is_no_history_to_step_back_through() {
        let mut q = queue();
        assert_eq!(q.previous(), Some("d"), "wraps backwards from the start");
    }

    #[test]
    fn history_does_not_record_the_same_track_twice_in_a_row() {
        let mut q = Queue::new();
        q.set_tracks(vec!["a".into()], Some("a"));
        q.next(None);
        q.next(None);
        assert_eq!(q.history(), &["a".to_string()]);
    }

    #[test]
    fn history_is_bounded() {
        let mut q = Queue::new();
        let tracks: Vec<String> = (0..200).map(|i| format!("t{i}")).collect();
        q.set_tracks(tracks, Some("t0"));
        for _ in 0..250 {
            q.next(None);
        }
        assert!(q.history().len() <= MAX_HISTORY, "history grew unbounded");
    }

    #[test]
    fn lookahead_returns_now_next_and_the_one_after() {
        let q = queue();
        assert_eq!(q.lookahead(None), vec!["a", "b", "c"]);
    }

    #[test]
    fn lookahead_follows_the_smart_choice() {
        let q = queue();
        assert_eq!(q.lookahead(Some("c")), vec!["a", "c", "d"]);
    }

    #[test]
    fn lookahead_never_repeats_a_track() {
        let mut q = Queue::new();
        q.set_tracks(vec!["a".into(), "b".into()], Some("a"));
        let ahead = q.lookahead(None);
        let mut sorted = ahead.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ahead.len(), "duplicates in {ahead:?}");
    }

    #[test]
    fn an_empty_queue_is_inert() {
        let mut q = Queue::new();
        assert!(q.next(None).is_none());
        assert!(q.previous().is_none());
        assert!(q.lookahead(None).is_empty());
    }
}
