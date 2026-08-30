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
    /// Every genre this track is filed under, most specific first. Empty when
    /// unknown.
    ///
    /// A list since 2026-08-29. It was one `String`, and a track that is
    /// genuinely both Liquid Funk and Jazz had nowhere to put the second — the
    /// granular sources added in the same week routinely name several.
    #[serde(alias = "genre", default, deserialize_with = "genres_field")]
    pub genres: Vec<String>,
    /// 0.0 when unknown.
    pub bpm: f32,
    /// Camelot key, empty when unknown.
    pub key: String,
    pub year: u32,
    /// Position in a manually ordered playlist.
    #[serde(alias = "manual_pos")]
    pub manual_pos: usize,
}

/// Read `genres` from every shape this field has ever been written in.
///
/// Three of them, and all three are on disk in somebody's library right now:
///
/// * `"genres": ["Liquid Funk", "Jazz"]` — what is written from now on.
/// * `"genre": "Electronic"` — the original, a single genre as a string.
/// * `"genre": "Electronic / Dance"` — the same field after AUD-24 taught the
///   Deezer parser to keep every genre an album names, joined with `/`.
///
/// The third is why this cannot be a plain `#[serde(alias)]` on a `Vec<String>`:
/// a joined string is not a JSON array, and serde would fail the whole `Row`
/// rather than the field — which for a library index means the file will not
/// load and every folder, manual ordering and correction in it is gone. The
/// splitting is [`crate::genre::split_genres`], so a value that arrives joined
/// becomes the same list it would have been had it been written as one.
///
/// `null` and a missing field both read as no genres, which is what `default`
/// is for. Nothing here can fail, deliberately: this runs against files written
/// by builds that no longer exist, and refusing to load a library is a far
/// worse outcome than dropping a genre.
fn genres_field<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
        Nothing,
    }

    Ok(match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(joined) => crate::genre::split_genres(&joined),
        // Split on the way in as well: a list is not a promise that nobody put
        // a joined string inside it, and `["Electronic / Dance"]` is a shape a
        // hand-edited file can hold.
        OneOrMany::Many(list) => list
            .iter()
            .flat_map(|g| crate::genre::split_genres(g))
            .collect(),
        OneOrMany::Nothing => Vec::new(),
    })
}

impl Row {
    /// The genres as one string, for a caller that can only show one thing.
    ///
    /// Joined the way this field was stored before it was a list, so a table
    /// cell, a sort key and a search haystack all read exactly as they did.
    pub fn genre_label(&self) -> String {
        self.genres.join(" / ")
    }

    /// Whether this row is filed under `genre`, case-insensitively.
    ///
    /// Case-insensitive because the sources disagree and always have: Deezer
    /// title-cases, Last.fm lower-cases, and a file's own tag is whatever
    /// somebody typed. An exact comparison here builds two tiles for one genre.
    pub fn has_genre(&self, genre: &str) -> bool {
        let wanted = genre.trim();
        self.genres.iter().any(|g| g.eq_ignore_ascii_case(wanted))
    }
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
        || row.genres.iter().any(|g| g.to_lowercase().contains(&q))
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
        EntityType::Genre => row.has_genre(&entity.value),
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
                // Ungenred rows sink, whatever the direction — the same rule as
                // before, now asked of the list rather than the string.
                match (!a.genres.is_empty(), !b.genres.is_empty()) {
                    (true, false) => return Ordering::Less,
                    (false, true) => return Ordering::Greater,
                    _ => {}
                }
                a.genre_label()
                    .to_lowercase()
                    .cmp(&b.genre_label().to_lowercase())
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
            // The *first* genre, not the joined label: grouping on the label
            // would make "Liquid Funk / Jazz" its own heading beside "Liquid
            // Funk", which is the fragmentation a list exists to end. A row
            // belongs under one heading here; the Genres *tab* is the surface
            // that shows a track under each of its genres, and that is
            // `matches_entity`.
            GroupBy::Genre if !row.genres.is_empty() => row.genres[0].clone(),
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
            genres: crate::genre::split_genres(genre),
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

    /// Every shape this field has been written in, read back. A library index
    /// is the file that holds somebody's folders, manual ordering and hand
    /// corrections, so a `Row` that refuses to deserialize does not lose a
    /// genre — it loses all of that.
    #[test]
    fn a_genre_written_in_any_past_shape_still_loads() {
        // The rest of a real row, so the only thing under test is the genre.
        let read = |field: &str| -> Vec<String> {
            let json = format!(
                r#"{{"href":"h","title":"t","artist":"a","album":"al",
                     "artistSource":"file","albumSource":"file",{field}
                     "bpm":174.0,"key":"8A","year":2010,"manualPos":0}}"#
            );
            serde_json::from_str::<Row>(&json)
                .unwrap_or_else(|e| panic!("{field} did not load: {e}"))
                .genres
        };

        // What is written from now on.
        assert_eq!(
            read(r#""genres":["Liquid Funk","Jazz"],"#),
            ["Liquid Funk", "Jazz"]
        );
        // The original: one genre, as a string.
        assert_eq!(read(r#""genre":"Electronic","#), ["Electronic"]);
        // The same field after AUD-24 taught the Deezer parser to keep every
        // genre an album names. This is the shape a plain serde alias fails on.
        assert_eq!(
            read(r#""genre":"Electronic / Dance","#),
            ["Electronic", "Dance"]
        );
        // A joined string that somehow got inside a list.
        assert_eq!(
            read(r#""genres":["Electronic / Dance"],"#),
            ["Electronic", "Dance"]
        );
        // Absent, null, and empty are all "no genre" and none of them is an
        // error — these files are written by builds that no longer exist.
        assert!(read("").is_empty());
        assert!(read(r#""genre":null,"#).is_empty());
        assert!(read(r#""genre":"","#).is_empty());
        assert!(read(r#""genres":[],"#).is_empty());
    }

    /// The round trip a library does on every save and load.
    #[test]
    fn genres_survive_being_written_and_read_back() {
        let before = row("t", "a", "al", "Liquid Funk / Jazz", 174.0, "8A");
        let json = serde_json::to_string(&before).expect("serialises");
        assert!(
            json.contains(r#""genres":["Liquid Funk","Jazz"]"#),
            "{json}"
        );
        let after: Row = serde_json::from_str(&json).expect("deserialises");
        assert_eq!(after.genres, before.genres);
    }

    /// A track under several genres appears under each of them, which is the
    /// whole point of the list — and the tile is found case-insensitively,
    /// because the three sources disagree about casing and always have.
    #[test]
    fn a_track_belongs_to_every_genre_it_names() {
        let under = |r: &Row, name: &str| {
            matches_entity(
                r,
                &Entity {
                    entity_type: EntityType::Genre,
                    value: name.to_string(),
                },
            )
        };
        let r = row("t", "a", "al", "Liquid Funk / Jazz", 174.0, "8A");
        assert!(under(&r, "Liquid Funk"));
        assert!(
            under(&r, "Jazz"),
            "the second genre had nowhere to go before this"
        );
        assert!(
            under(&r, "liquid funk"),
            "Last.fm lower-cases what Deezer title-cases"
        );
        assert!(!under(&r, "Techno"));
        // An ungenred row belongs to no tile, rather than to an empty one.
        let bare = row("t", "a", "al", "", 0.0, "");
        assert!(!under(&bare, ""));
    }

    /// Grouping keys on the first genre, not the joined label: a heading per
    /// combination is the fragmentation the list exists to end.
    #[test]
    fn grouping_puts_a_multi_genre_row_under_one_heading() {
        let rows = vec![
            row("a", "x", "al", "Liquid Funk / Jazz", 174.0, "8A"),
            row("b", "y", "al", "Liquid Funk", 174.0, "8A"),
        ];
        let groups = group_rows(&rows, GroupBy::Genre);
        let headings: Vec<&str> = groups.iter().map(|g| g.0.as_str()).collect();
        assert_eq!(headings, ["Liquid Funk"], "{headings:?}");
    }
}
