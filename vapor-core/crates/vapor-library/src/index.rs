//! The library table: build → filter → sort → group.
//!
//! Port of `track_index.gd`, which was already written as a pure pipeline, so
//! this is the most faithful port in the crate.
//!
//! Two predicates here are deliberately *the* definition of their concept,
//! shared by every view rather than reimplemented per screen:
//!
//! * [`matches_query`] — the library search box and a smart playlist run the
//!   same predicate, so they can never disagree about membership.
//! * [`matches_entity`] — a dynamic group stores entities, never tracks, and
//!   every view of its contents evaluates this against the live library.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::group::{Entity, EntityType};

/// Header shown for rows whose grouping field is unknown.
///
/// A quiet dash rather than a shouty "Unknown Artist": the app is honest about
/// ignorance without making noise about it.
pub const UNKNOWN_HEADER: &str = "—";

/// Where a derived field came from, so the UI can distinguish verified
/// metadata from a guess and from honest ignorance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Source {
    Cache,
    File,
    Folder,
    /// A public service said so — nothing local did.
    ///
    /// The weakest kind of known. A loose track in the library root has no
    /// album in its path and none in its tags, and the only thing that can say
    /// it is track three of *Split The Atom* is Deezer. That is worth showing,
    /// and it is worth being able to tell apart from what the file itself
    /// carries: this app keeps measured, tagged and looked-up facts in separate
    /// stores precisely so a person can be told which is which, and folding a
    /// stranger's answer in as `File` would throw that away at the last step.
    Service,
    #[default]
    Unknown,
}

impl Source {
    pub fn is_known(self) -> bool {
        self != Source::Unknown
    }
}

/// One row of the library table.
// Serialised for two audiences at once: the IPC boundary, where the frontend
// reads camelCase, and the JSON on disk, which was written snake_case before
// this was noticed. `rename_all` fixes the wire; the per-field `alias` keeps
// every existing file readable, so nobody's folders or manual ordering reset
// on upgrade. Removing an alias is a data migration, not a tidy-up.
#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Row {
    pub href: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    #[serde(alias = "artist_source")]
    pub artist_source: Source,
    #[serde(alias = "album_source")]
    pub album_source: Source,
    /// Empty when unknown.
    pub genre: String,
    /// 0.0 when unknown.
    pub bpm: f32,
    /// Camelot key, empty when unknown.
    pub key: String,
    pub year: u32,
    /// Position in a manually ordered playlist.
    #[serde(alias = "manual_pos")]
    pub manual_pos: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupBy {
    None,
    Artist,
    Album,
    Genre,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Title,
    Artist,
    Album,
    Genre,
    Year,
    Bpm,
    Key,
    /// Manual playlist position.
    Order,
}

/// Case-insensitive substring match across title, artist, album and genre.
/// An empty query matches everything.
pub fn matches_query(row: &Row, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    row.title.to_lowercase().contains(&q)
        || row.artist.to_lowercase().contains(&q)
        || row.album.to_lowercase().contains(&q)
        || row.genre.to_lowercase().contains(&q)
}

/// Does this row belong to the given entity?
///
/// An unknown field never matches: a row whose artist is a placeholder is not
/// "by" that placeholder, and treating it as a match would sweep every
/// unidentified track into one group.
pub fn matches_entity(row: &Row, entity: &Entity) -> bool {
    match entity.entity_type {
        EntityType::Artist => row.artist_source.is_known() && row.artist == entity.value,
        EntityType::Album => row.album_source.is_known() && row.album == entity.value,
        EntityType::Genre => !row.genre.is_empty() && row.genre == entity.value,
    }
}

/// Does this row belong to *any* entity in a group?
///
/// A group with several entities is their union, not their intersection:
/// adding "Björk" and "Ambient" means tracks matching either.
pub fn matches_any_entity(row: &Row, entities: &[Entity]) -> bool {
    entities.iter().any(|e| matches_entity(row, e))
}

pub fn filter<'a>(rows: &'a [Row], query: &str) -> Vec<&'a Row> {
    rows.iter().filter(|r| matches_query(r, query)).collect()
}

/// Sort rows in place.
///
/// Rows with an unknown value always sink to the end regardless of direction.
/// An unknown BPM is not "0 BPM", and letting it sort first would state
/// something false. Ties break on title so ordering is stable and predictable.
pub fn sort_rows(rows: &mut [Row], key: SortKey, ascending: bool) {
    use std::cmp::Ordering;

    let title_of = |r: &Row| r.title.to_lowercase();

    rows.sort_by(|a, b| {
        // `known` first, then the comparison, then title as a tiebreak.
        let ord = match key {
            SortKey::Order => return a.manual_pos.cmp(&b.manual_pos),
            SortKey::Bpm => {
                match (a.bpm > 0.0, b.bpm > 0.0) {
                    (true, false) => return Ordering::Less,
                    (false, true) => return Ordering::Greater,
                    _ => {}
                }
                a.bpm.partial_cmp(&b.bpm).unwrap_or(Ordering::Equal)
            }
            SortKey::Year => {
                match (a.year > 0, b.year > 0) {
                    (true, false) => return Ordering::Less,
                    (false, true) => return Ordering::Greater,
                    _ => {}
                }
                a.year.cmp(&b.year)
            }
            SortKey::Key => {
                match (!a.key.is_empty(), !b.key.is_empty()) {
                    (true, false) => return Ordering::Less,
                    (false, true) => return Ordering::Greater,
                    _ => {}
                }
                camelot_sort_tuple(&a.key).cmp(&camelot_sort_tuple(&b.key))
            }
            SortKey::Artist => {
                match (a.artist_source.is_known(), b.artist_source.is_known()) {
                    (true, false) => return Ordering::Less,
                    (false, true) => return Ordering::Greater,
                    _ => {}
                }
                a.artist.to_lowercase().cmp(&b.artist.to_lowercase())
            }
            SortKey::Album => {
                match (a.album_source.is_known(), b.album_source.is_known()) {
                    (true, false) => return Ordering::Less,
                    (false, true) => return Ordering::Greater,
                    _ => {}
                }
                a.album.to_lowercase().cmp(&b.album.to_lowercase())
            }
            SortKey::Genre => {
                match (!a.genre.is_empty(), !b.genre.is_empty()) {
                    (true, false) => return Ordering::Less,
                    (false, true) => return Ordering::Greater,
                    _ => {}
                }
                a.genre.to_lowercase().cmp(&b.genre.to_lowercase())
            }
            SortKey::Title => title_of(a).cmp(&title_of(b)),
        };

        let ord = if ascending { ord } else { ord.reverse() };
        ord.then_with(|| title_of(a).cmp(&title_of(b)))
    });
}

/// Camelot keys sort musically: number first, then letter, so 8A < 8B < 9A.
/// Plain string ordering would put 10A before 2A.
fn camelot_sort_tuple(key: &str) -> (u8, char) {
    match crate::camelot::CamelotKey::parse(key) {
        Some(k) => (k.number, if k.is_minor { 'A' } else { 'B' }),
        None => (u8::MAX, 'Z'),
    }
}

/// Group rows under headers, preserving the order they arrive in.
///
/// Returns `(header, rows)` pairs. Unknown values collapse into a single
/// [`UNKNOWN_HEADER`] group rather than one group per placeholder.
pub fn group_rows(rows: &[Row], by: GroupBy) -> Vec<(String, Vec<&Row>)> {
    if by == GroupBy::None {
        return vec![(String::new(), rows.iter().collect())];
    }

    let mut order: Vec<String> = Vec::new();
    let mut groups: Vec<(String, Vec<&Row>)> = Vec::new();

    for row in rows {
        let header = match by {
            GroupBy::Artist if row.artist_source.is_known() => row.artist.clone(),
            GroupBy::Album if row.album_source.is_known() => row.album.clone(),
            GroupBy::Genre if !row.genre.is_empty() => row.genre.clone(),
            GroupBy::None => unreachable!("handled above"),
            _ => UNKNOWN_HEADER.to_string(),
        };

        match order.iter().position(|h| *h == header) {
            Some(i) => groups[i].1.push(row),
            None => {
                order.push(header.clone());
                groups.push((header, vec![row]));
            }
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(title: &str, artist: &str, album: &str, genre: &str, bpm: f32, key: &str) -> Row {
        Row {
            href: format!("/{title}.mp3"),
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            artist_source: if artist.is_empty() {
                Source::Unknown
            } else {
                Source::Cache
            },
            album_source: if album.is_empty() {
                Source::Unknown
            } else {
                Source::Cache
            },
            genre: genre.into(),
            bpm,
            key: key.into(),
            ..Default::default()
        }
    }

    fn sample() -> Vec<Row> {
        vec![
            row("Beta", "Björk", "Homogenic", "Trip Hop", 90.0, "8A"),
            row("Alpha", "Aphex Twin", "SAW", "Ambient", 0.0, ""),
            row("Gamma", "", "", "", 120.0, "10A"),
            row("Delta", "Björk", "Vespertine", "Trip Hop", 100.0, "2B"),
        ]
    }

    #[test]
    fn an_empty_query_matches_everything() {
        let rows = sample();
        assert_eq!(filter(&rows, "").len(), 4);
        assert_eq!(filter(&rows, "   ").len(), 4);
    }

    #[test]
    fn query_matches_across_every_searchable_field() {
        let rows = sample();
        assert_eq!(filter(&rows, "björk").len(), 2, "artist");
        assert_eq!(filter(&rows, "saw").len(), 1, "album");
        assert_eq!(filter(&rows, "ambient").len(), 1, "genre");
        assert_eq!(filter(&rows, "alpha").len(), 1, "title");
        assert_eq!(filter(&rows, "TRIP").len(), 2, "case-insensitive");
    }

    /// The property that makes unknown-sinking correct: an unknown BPM is not
    /// zero, and must not lead an ascending sort.
    #[test]
    fn unknown_values_sink_regardless_of_direction() {
        for ascending in [true, false] {
            let mut rows = sample();
            sort_rows(&mut rows, SortKey::Bpm, ascending);
            assert_eq!(
                rows.last().expect("non-empty").title,
                "Alpha",
                "the unknown BPM should sink when ascending={ascending}"
            );
        }
    }

    #[test]
    fn bpm_sorts_numerically_in_both_directions() {
        let mut rows = sample();
        sort_rows(&mut rows, SortKey::Bpm, true);
        let known: Vec<f32> = rows.iter().filter(|r| r.bpm > 0.0).map(|r| r.bpm).collect();
        assert_eq!(known, vec![90.0, 100.0, 120.0]);

        sort_rows(&mut rows, SortKey::Bpm, false);
        let known: Vec<f32> = rows.iter().filter(|r| r.bpm > 0.0).map(|r| r.bpm).collect();
        assert_eq!(known, vec![120.0, 100.0, 90.0]);
    }

    /// Plain string ordering would put "10A" before "2A".
    #[test]
    fn camelot_keys_sort_musically_not_alphabetically() {
        let mut rows = vec![
            row("a", "x", "y", "", 120.0, "10A"),
            row("b", "x", "y", "", 120.0, "2A"),
            row("c", "x", "y", "", 120.0, "8B"),
            row("d", "x", "y", "", 120.0, "8A"),
        ];
        sort_rows(&mut rows, SortKey::Key, true);
        let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, vec!["2A", "8A", "8B", "10A"]);
    }

    #[test]
    fn ties_break_on_title_so_ordering_is_stable() {
        let mut rows = vec![
            row("Zebra", "X", "Y", "Rock", 120.0, "8A"),
            row("Apple", "X", "Y", "Rock", 120.0, "8A"),
        ];
        sort_rows(&mut rows, SortKey::Bpm, true);
        assert_eq!(rows[0].title, "Apple");
    }

    /// A group is the union of its entities, not the intersection.
    #[test]
    fn group_membership_is_a_union() {
        let rows = sample();
        let entities = vec![
            Entity {
                entity_type: EntityType::Artist,
                value: "Björk".into(),
            },
            Entity {
                entity_type: EntityType::Genre,
                value: "Ambient".into(),
            },
        ];
        let matched: Vec<&Row> = rows
            .iter()
            .filter(|r| matches_any_entity(r, &entities))
            .collect();
        assert_eq!(matched.len(), 3, "two Björk tracks plus one Ambient");
    }

    /// An unidentified row must not be swept into a group by its placeholder.
    #[test]
    fn unknown_fields_never_match_an_entity() {
        let unknown = row("Gamma", "", "", "", 120.0, "");
        let e = Entity {
            entity_type: EntityType::Artist,
            value: String::new(),
        };
        assert!(!matches_entity(&unknown, &e));
    }

    #[test]
    fn grouping_collapses_unknowns_into_one_header() {
        let rows = sample();
        let groups = group_rows(&rows, GroupBy::Artist);
        let headers: Vec<&str> = groups.iter().map(|(h, _)| h.as_str()).collect();
        assert!(headers.contains(&"Björk"));
        assert!(headers.contains(&UNKNOWN_HEADER));
        let bjork = groups.iter().find(|(h, _)| h == "Björk").expect("present");
        assert_eq!(bjork.1.len(), 2);
    }

    #[test]
    fn grouping_by_none_yields_a_single_ungrouped_list() {
        let rows = sample();
        let groups = group_rows(&rows, GroupBy::None);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1.len(), 4);
    }

    #[test]
    fn manual_order_sorts_by_position() {
        let mut rows = sample();
        for (i, r) in rows.iter_mut().enumerate() {
            r.manual_pos = 3 - i;
        }
        sort_rows(&mut rows, SortKey::Order, true);
        assert_eq!(rows[0].title, "Delta");
    }
}
