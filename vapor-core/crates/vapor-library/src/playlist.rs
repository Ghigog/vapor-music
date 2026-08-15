//! Playlists.
//!
//! Port of `scripts/services/playlist_service.gd`, minus the persistence and
//! the signals.
//!
//! The GDScript service mixes three jobs: it owns the data, it writes JSON to
//! `user://playlists.json` after every mutation, and it emits signals so the UI
//! can react. Here it only owns the data. Saving and notifying belong to the
//! shell, and separating them is what lets the whole collection be tested
//! without a filesystem and reused unchanged in the browser, where
//! `user://` does not exist.
//!
//! Bulk operations are kept as distinct methods rather than loops over the
//! single-item ones. That is not a micro-optimisation: in the GDScript,
//! looping the single-track add produced duplicates and saved N times, and
//! looping the single-index remove shifted positions out from under later
//! removals. Both are recorded in the GUT suite and both are preserved here.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub custom_cover_path: String,
    #[serde(default)]
    pub tracks: Vec<String>,
    #[serde(default)]
    pub folder_id: String,
}

/// An ordered collection of playlists.
///
/// Order is explicit rather than relying on map iteration — the GDScript
/// rebuilt its whole dictionary on create just to get newest-first ordering,
/// which is a symptom of the data structure not matching the requirement.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlaylistStore {
    playlists: Vec<Playlist>,
}

impl PlaylistStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(&self) -> &[Playlist] {
        &self.playlists
    }

    pub fn len(&self) -> usize {
        self.playlists.len()
    }

    pub fn is_empty(&self) -> bool {
        self.playlists.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&Playlist> {
        self.playlists.iter().find(|p| p.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Playlist> {
        self.playlists.iter_mut().find(|p| p.id == id)
    }

    /// Create a playlist with a caller-supplied id, newest first.
    ///
    /// The id is a parameter because the GDScript built it from
    /// `Time.get_ticks_usec()` and `randi()`, neither of which belongs in a
    /// pure library — and both of which make tests non-deterministic.
    pub fn create(&mut self, id: impl Into<String>, name: impl Into<String>) -> &Playlist {
        self.create_in_folder(id, name, "")
    }

    pub fn create_in_folder(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        folder_id: impl Into<String>,
    ) -> &Playlist {
        self.playlists.insert(
            0,
            Playlist {
                id: id.into(),
                name: name.into(),
                custom_cover_path: String::new(),
                tracks: Vec::new(),
                folder_id: folder_id.into(),
            },
        );
        &self.playlists[0]
    }

    /// Snapshot of a library view: the given hrefs, in the given order.
    pub fn create_snapshot(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        hrefs: Vec<String>,
    ) -> &Playlist {
        self.create(id, name);
        self.playlists[0].tracks = hrefs;
        &self.playlists[0]
    }

    /// Returns the removed playlist, so the caller can clean up its cover file.
    pub fn delete(&mut self, id: &str) -> Option<Playlist> {
        let i = self.playlists.iter().position(|p| p.id == id)?;
        Some(self.playlists.remove(i))
    }

    pub fn rename(&mut self, id: &str, new_name: impl Into<String>) -> bool {
        match self.get_mut(id) {
            Some(p) => {
                p.name = new_name.into();
                true
            }
            None => false,
        }
    }

    pub fn add_track(&mut self, id: &str, href: impl Into<String>) -> bool {
        match self.get_mut(id) {
            Some(p) => {
                p.tracks.push(href.into());
                true
            }
            None => false,
        }
    }

    pub fn insert_track(&mut self, id: &str, href: impl Into<String>, index: usize) -> bool {
        match self.get_mut(id) {
            Some(p) if index <= p.tracks.len() => {
                p.tracks.insert(index, href.into());
                true
            }
            _ => false,
        }
    }

    /// Bulk add, skipping anything already present.
    ///
    /// Returns how many were actually added, so the caller can skip saving and
    /// notifying when nothing changed. Deduplication is the behaviour the GUT
    /// suite pins: looping `add_track` instead would create duplicates.
    pub fn add_tracks(&mut self, id: &str, hrefs: &[String]) -> usize {
        let Some(p) = self.get_mut(id) else {
            return 0;
        };
        let mut added = 0;
        for href in hrefs {
            if !p.tracks.iter().any(|t| t == href) {
                p.tracks.push(href.clone());
                added += 1;
            }
        }
        added
    }

    pub fn remove_track(&mut self, id: &str, index: usize) -> bool {
        match self.get_mut(id) {
            Some(p) if index < p.tracks.len() => {
                p.tracks.remove(index);
                true
            }
            _ => false,
        }
    }

    /// Bulk remove by index, highest first.
    ///
    /// Descending order is load-bearing: removing a low index first shifts
    /// every later one down, so ascending removal deletes the wrong tracks.
    /// Returns how many were removed.
    pub fn remove_tracks(&mut self, id: &str, indices: &[usize]) -> usize {
        let Some(p) = self.get_mut(id) else {
            return 0;
        };
        let mut sorted: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|i| *i < p.tracks.len())
            .collect();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        sorted.dedup();
        for i in &sorted {
            p.tracks.remove(*i);
        }
        sorted.len()
    }

    /// Move a track within a playlist.
    pub fn reorder_tracks(&mut self, id: &str, from: usize, to: usize) -> bool {
        match self.get_mut(id) {
            Some(p) if from < p.tracks.len() && to < p.tracks.len() => {
                let track = p.tracks.remove(from);
                p.tracks.insert(to, track);
                true
            }
            _ => false,
        }
    }

    /// Move a playlist within the collection.
    pub fn reorder(&mut self, from: usize, to: usize) -> bool {
        if from >= self.playlists.len() || to >= self.playlists.len() {
            return false;
        }
        let p = self.playlists.remove(from);
        self.playlists.insert(to, p);
        true
    }

    pub fn set_cover(&mut self, id: &str, path: impl Into<String>) -> bool {
        match self.get_mut(id) {
            Some(p) => {
                p.custom_cover_path = path.into();
                true
            }
            None => false,
        }
    }

    pub fn set_folder(&mut self, id: &str, folder_id: impl Into<String>) -> bool {
        match self.get_mut(id) {
            Some(p) => {
                p.folder_id = folder_id.into();
                true
            }
            None => false,
        }
    }

    /// Cover to display: the custom one when set, else the first track, so the
    /// caller can fall back to that track's album art.
    pub fn cover_source(&self, id: &str) -> Option<CoverSource<'_>> {
        let p = self.get(id)?;
        if !p.custom_cover_path.is_empty() {
            return Some(CoverSource::Custom(&p.custom_cover_path));
        }
        p.tracks.first().map(|t| CoverSource::FirstTrack(t))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum CoverSource<'a> {
    Custom(&'a str),
    FirstTrack(&'a str),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with_one() -> (PlaylistStore, String) {
        let mut s = PlaylistStore::new();
        s.create("p1", "Test");
        (s, "p1".to_string())
    }

    /// Ported from `test_create_playlist`.
    #[test]
    fn creates_newest_first() {
        let mut s = PlaylistStore::new();
        s.create("p1", "First");
        s.create("p2", "Second");
        assert_eq!(s.len(), 2);
        assert_eq!(s.all()[0].id, "p2", "newest should come first");
        assert!(s.get("p1").expect("p1 exists").tracks.is_empty());
    }

    /// Ported from `test_delete_playlist`.
    #[test]
    fn deletes_and_returns_the_playlist() {
        let (mut s, id) = store_with_one();
        let removed = s.delete(&id).expect("should return the deleted playlist");
        assert_eq!(removed.id, id);
        assert!(s.get(&id).is_none());
        assert!(s.delete("nonexistent").is_none());
    }

    /// Ported from `test_rename_playlist`.
    #[test]
    fn renames() {
        let (mut s, id) = store_with_one();
        assert!(s.rename(&id, "Renamed"));
        assert_eq!(s.get(&id).unwrap().name, "Renamed");
        assert!(!s.rename("nope", "x"));
    }

    /// Ported from `test_add_and_remove_track`.
    #[test]
    fn adds_and_removes_a_track() {
        let (mut s, id) = store_with_one();
        assert!(s.add_track(&id, "a.mp3"));
        assert_eq!(s.get(&id).unwrap().tracks, vec!["a.mp3"]);
        assert!(s.remove_track(&id, 0));
        assert!(s.get(&id).unwrap().tracks.is_empty());
        assert!(!s.remove_track(&id, 0), "out of range should be refused");
    }

    /// Ported from `test_add_tracks_to_playlist_bulk_dedups_and_saves_once`.
    #[test]
    fn bulk_add_deduplicates() {
        let (mut s, id) = store_with_one();
        s.add_track(&id, "a.mp3");

        let added = s.add_tracks(&id, &["a.mp3".into(), "b.mp3".into(), "c.mp3".into()]);
        assert_eq!(added, 2, "a.mp3 was already present");
        assert_eq!(s.get(&id).unwrap().tracks, vec!["a.mp3", "b.mp3", "c.mp3"]);
    }

    /// Ported from `test_add_tracks_to_playlist_all_already_present_is_a_noop`.
    #[test]
    fn bulk_add_of_existing_tracks_changes_nothing() {
        let (mut s, id) = store_with_one();
        s.add_tracks(&id, &["a.mp3".into(), "b.mp3".into()]);
        let added = s.add_tracks(&id, &["a.mp3".into(), "b.mp3".into()]);
        assert_eq!(added, 0, "caller can skip saving when nothing was added");
        assert_eq!(s.get(&id).unwrap().tracks.len(), 2);
    }

    /// Ported from `test_remove_tracks_from_playlist_bulk_removes_by_index`.
    ///
    /// The property that matters: removing ascending would shift positions and
    /// delete the wrong tracks.
    #[test]
    fn bulk_remove_takes_the_intended_tracks() {
        let (mut s, id) = store_with_one();
        s.add_tracks(
            &id,
            &["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
        );

        let removed = s.remove_tracks(&id, &[0, 2, 4]);
        assert_eq!(removed, 3);
        assert_eq!(
            s.get(&id).unwrap().tracks,
            vec!["b", "d"],
            "indices must be applied highest-first"
        );
    }

    /// Ported from `test_remove_tracks_from_playlist_empty_indices_is_a_noop`.
    #[test]
    fn bulk_remove_with_no_indices_changes_nothing() {
        let (mut s, id) = store_with_one();
        s.add_tracks(&id, &["a".into(), "b".into()]);
        assert_eq!(s.remove_tracks(&id, &[]), 0);
        assert_eq!(s.get(&id).unwrap().tracks.len(), 2);
    }

    #[test]
    fn bulk_remove_ignores_out_of_range_and_duplicate_indices() {
        let (mut s, id) = store_with_one();
        s.add_tracks(&id, &["a".into(), "b".into(), "c".into()]);
        let removed = s.remove_tracks(&id, &[1, 1, 99]);
        assert_eq!(removed, 1);
        assert_eq!(s.get(&id).unwrap().tracks, vec!["a", "c"]);
    }

    /// Ported from `test_reorder_tracks`.
    #[test]
    fn reorders_tracks() {
        let (mut s, id) = store_with_one();
        s.add_tracks(&id, &["a".into(), "b".into(), "c".into()]);
        assert!(s.reorder_tracks(&id, 0, 2));
        assert_eq!(s.get(&id).unwrap().tracks, vec!["b", "c", "a"]);
        assert!(!s.reorder_tracks(&id, 0, 99));
    }

    /// Ported from `test_reorder_playlists`.
    #[test]
    fn reorders_playlists() {
        let mut s = PlaylistStore::new();
        s.create("p1", "One");
        s.create("p2", "Two");
        s.create("p3", "Three");
        // Newest first: p3, p2, p1.
        assert!(s.reorder(0, 2));
        assert_eq!(
            s.all().iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            vec!["p2", "p1", "p3"]
        );
    }

    /// Ported from `test_custom_cover_art` and
    /// `test_first_track_album_art_fallback`.
    #[test]
    fn cover_prefers_custom_then_falls_back_to_the_first_track() {
        let (mut s, id) = store_with_one();
        assert_eq!(s.cover_source(&id), None, "empty playlist has no cover");

        s.add_track(&id, "first.mp3");
        assert_eq!(
            s.cover_source(&id),
            Some(CoverSource::FirstTrack("first.mp3"))
        );

        s.set_cover(&id, "user://playlist_images/x.jpg");
        assert_eq!(
            s.cover_source(&id),
            Some(CoverSource::Custom("user://playlist_images/x.jpg")),
            "a custom cover should win"
        );
    }

    #[test]
    fn snapshot_preserves_the_given_order() {
        let mut s = PlaylistStore::new();
        s.create_snapshot("p1", "Snap", vec!["c".into(), "a".into(), "b".into()]);
        assert_eq!(s.get("p1").unwrap().tracks, vec!["c", "a", "b"]);
    }

    #[test]
    fn insert_places_a_track_at_an_index() {
        let (mut s, id) = store_with_one();
        s.add_tracks(&id, &["a".into(), "c".into()]);
        assert!(s.insert_track(&id, "b", 1));
        assert_eq!(s.get(&id).unwrap().tracks, vec!["a", "b", "c"]);
        assert!(s.insert_track(&id, "d", 3), "appending at len is allowed");
        assert!(!s.insert_track(&id, "e", 99));
    }

    /// The whole store must survive a JSON round trip, since that is how the
    /// shell will persist it.
    #[test]
    fn round_trips_through_json() {
        let (mut s, id) = store_with_one();
        s.add_tracks(&id, &["a".into(), "b".into()]);
        s.set_cover(&id, "cover.jpg");
        s.set_folder(&id, "f1");

        let json = serde_json::to_string(&s).expect("serialise");
        let back: PlaylistStore = serde_json::from_str(&json).expect("deserialise");

        let p = back.get(&id).expect("playlist survived");
        assert_eq!(p.tracks, vec!["a", "b"]);
        assert_eq!(p.custom_cover_path, "cover.jpg");
        assert_eq!(p.folder_id, "f1");
    }
}
