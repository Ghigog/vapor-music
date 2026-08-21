//! Dynamic groups and playlist folders.
//!
//! Port of `dynamic_group_service.gd` and `playlist_folder_service.gd`.
//!
//! A dynamic group is a saved set of *entities* — artists, albums, genres —
//! rather than a fixed list of tracks, so it stays current as the library
//! grows. A folder is a plain container for organising playlists.
//!
//! As with [`crate::playlist`], these own data only: persistence, signals and
//! cover-image lookup stay in the shell. The GDScript reached into
//! `MetadataService` from inside `delete_group` to clear a cover override,
//! which is the kind of cross-service coupling that makes either side
//! untestable alone — `delete` here returns the id so the caller can do it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What a dynamic group can contain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum EntityType {
    Artist,
    Album,
    Genre,
}

impl EntityType {
    pub fn as_str(self) -> &'static str {
        match self {
            EntityType::Artist => "artist",
            EntityType::Album => "album",
            EntityType::Genre => "genre",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "artist" => Some(EntityType::Artist),
            "album" => Some(EntityType::Album),
            "genre" => Some(EntityType::Genre),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Entity {
    // `entityType` on the wire, because that is what the frontend reads —
    // `SmartGroup.tsx` keys its chips on `e.entityType`. It was `type`, which
    // is neither what TypeScript expects nor worth keeping for its own sake.
    // The alias keeps groups written by earlier builds loading.
    #[serde(rename = "entityType", alias = "type")]
    pub entity_type: EntityType,
    pub value: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DynamicGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub entities: Vec<Entity>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GroupStore {
    groups: Vec<DynamicGroup>,
}

impl GroupStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(&self) -> &[DynamicGroup] {
        &self.groups
    }

    pub fn len(&self) -> usize {
        self.groups.len()
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&DynamicGroup> {
        self.groups.iter().find(|g| g.id == id)
    }

    fn get_mut(&mut self, id: &str) -> Option<&mut DynamicGroup> {
        self.groups.iter_mut().find(|g| g.id == id)
    }

    /// Create a group, newest first. Id supplied by the caller, as with
    /// playlists.
    pub fn create(&mut self, id: impl Into<String>, name: impl Into<String>) -> &DynamicGroup {
        self.groups.insert(
            0,
            DynamicGroup {
                id: id.into(),
                name: name.into(),
                entities: Vec::new(),
            },
        );
        &self.groups[0]
    }

    /// Returns the removed group.
    ///
    /// The caller is responsible for clearing any cover override keyed to this
    /// id — otherwise a future group reusing the id inherits a stale image,
    /// which is what the GDScript's inline `clear_custom_image` call prevented.
    pub fn delete(&mut self, id: &str) -> Option<DynamicGroup> {
        let i = self.groups.iter().position(|g| g.id == id)?;
        Some(self.groups.remove(i))
    }

    pub fn rename(&mut self, id: &str, new_name: impl Into<String>) -> bool {
        match self.get_mut(id) {
            Some(g) => {
                g.name = new_name.into();
                true
            }
            None => false,
        }
    }

    pub fn has_entity(&self, id: &str, entity_type: EntityType, value: &str) -> bool {
        self.get(id).is_some_and(|g| {
            g.entities
                .iter()
                .any(|e| e.entity_type == entity_type && e.value == value)
        })
    }

    /// Add an entity. Idempotent: dragging the same artist onto a group twice
    /// is a no-op, not a duplicate row.
    ///
    /// Returns whether anything changed.
    pub fn add_entity(&mut self, id: &str, entity_type: EntityType, value: &str) -> bool {
        if value.is_empty() || self.has_entity(id, entity_type, value) {
            return false;
        }
        match self.get_mut(id) {
            Some(g) => {
                g.entities.push(Entity {
                    entity_type,
                    value: value.to_string(),
                });
                true
            }
            None => false,
        }
    }

    pub fn remove_entity(&mut self, id: &str, entity_type: EntityType, value: &str) -> bool {
        let Some(g) = self.get_mut(id) else {
            return false;
        };
        let before = g.entities.len();
        g.entities
            .retain(|e| !(e.entity_type == entity_type && e.value == value));
        g.entities.len() != before
    }

    pub fn reorder(&mut self, from: usize, to: usize) -> bool {
        if from >= self.groups.len() || to >= self.groups.len() {
            return false;
        }
        let g = self.groups.remove(from);
        self.groups.insert(to, g);
        true
    }

    /// First artist or album entity, which the shell can use to borrow a cover
    /// image. Genres have no natural image and are skipped.
    pub fn cover_entity(&self, id: &str) -> Option<&Entity> {
        self.get(id)?
            .entities
            .iter()
            .find(|e| matches!(e.entity_type, EntityType::Artist | EntityType::Album))
    }
}

// `PartialEq` so the shared document (SYNC-006) can be compared — a round
// trip that cannot be asserted is a serialisation format nobody checked.
// Serialised for two audiences at once: the IPC boundary, where the frontend
// reads camelCase, and the JSON on disk, which was written snake_case before
// this was noticed. `rename_all` fixes the wire; the per-field `alias` keeps
// every existing file readable, so nobody's folders or manual ordering reset
// on upgrade. Removing an alias is a data migration, not a tidy-up.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Folder {
    pub id: String,
    pub name: String,
    #[serde(default)]
    #[serde(alias = "parent_id")]
    pub parent_id: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FolderStore {
    folders: Vec<Folder>,
}

impl FolderStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(&self) -> &[Folder] {
        &self.folders
    }

    pub fn get(&self, id: &str) -> Option<&Folder> {
        self.folders.iter().find(|f| f.id == id)
    }

    pub fn create(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        parent_id: impl Into<String>,
    ) -> &Folder {
        self.folders.insert(
            0,
            Folder {
                id: id.into(),
                name: name.into(),
                parent_id: parent_id.into(),
            },
        );
        &self.folders[0]
    }

    /// Delete a folder, returning the ids of any folders that were nested
    /// inside it and are now orphaned.
    ///
    /// The caller must reassign playlists that pointed at any returned id.
    /// Returning them rather than cascading keeps this free of assumptions
    /// about what else references a folder.
    pub fn delete(&mut self, id: &str) -> Vec<String> {
        let Some(i) = self.folders.iter().position(|f| f.id == id) else {
            return Vec::new();
        };
        self.folders.remove(i);

        let orphaned: Vec<String> = self
            .folders
            .iter()
            .filter(|f| f.parent_id == id)
            .map(|f| f.id.clone())
            .collect();
        for f in self.folders.iter_mut() {
            if f.parent_id == id {
                f.parent_id.clear();
            }
        }
        orphaned
    }

    pub fn rename(&mut self, id: &str, new_name: impl Into<String>) -> bool {
        match self.folders.iter_mut().find(|f| f.id == id) {
            Some(f) => {
                f.name = new_name.into();
                true
            }
            None => false,
        }
    }

    /// Folders whose parent is `parent_id`; pass `""` for the top level.
    pub fn children_of(&self, parent_id: &str) -> Vec<&Folder> {
        self.folders
            .iter()
            .filter(|f| f.parent_id == parent_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> GroupStore {
        let mut s = GroupStore::new();
        s.create("g1", "Chill");
        s
    }

    #[test]
    fn creates_newest_first_and_deletes() {
        let mut s = GroupStore::new();
        s.create("g1", "One");
        s.create("g2", "Two");
        assert_eq!(s.all()[0].id, "g2");
        assert_eq!(s.delete("g1").expect("returns the group").name, "One");
        assert_eq!(s.len(), 1);
        assert!(s.delete("nope").is_none());
    }

    #[test]
    fn renames() {
        let mut s = store();
        assert!(s.rename("g1", "Renamed"));
        assert_eq!(s.get("g1").unwrap().name, "Renamed");
        assert!(!s.rename("nope", "x"));
    }

    /// The behaviour the GDScript comment calls out: dragging the same artist
    /// on twice must not create a duplicate row.
    #[test]
    fn adding_an_entity_is_idempotent() {
        let mut s = store();
        assert!(s.add_entity("g1", EntityType::Artist, "Aphex Twin"));
        assert!(!s.add_entity("g1", EntityType::Artist, "Aphex Twin"));
        assert_eq!(s.get("g1").unwrap().entities.len(), 1);
    }

    /// Type is part of the identity: an artist and a genre with the same name
    /// are different entities.
    #[test]
    fn entities_are_distinguished_by_type_as_well_as_value() {
        let mut s = store();
        assert!(s.add_entity("g1", EntityType::Artist, "Jungle"));
        assert!(s.add_entity("g1", EntityType::Genre, "Jungle"));
        assert_eq!(s.get("g1").unwrap().entities.len(), 2);
        assert!(s.has_entity("g1", EntityType::Artist, "Jungle"));
        assert!(s.has_entity("g1", EntityType::Genre, "Jungle"));
    }

    #[test]
    fn empty_values_are_rejected() {
        let mut s = store();
        assert!(!s.add_entity("g1", EntityType::Artist, ""));
        assert!(s.get("g1").unwrap().entities.is_empty());
    }

    #[test]
    fn removes_only_the_matching_entity() {
        let mut s = store();
        s.add_entity("g1", EntityType::Artist, "A");
        s.add_entity("g1", EntityType::Artist, "B");
        assert!(s.remove_entity("g1", EntityType::Artist, "A"));
        assert_eq!(s.get("g1").unwrap().entities.len(), 1);
        assert!(
            !s.remove_entity("g1", EntityType::Artist, "A"),
            "already gone"
        );
    }

    /// Genres have no artwork, so a cover must be borrowed from an artist or
    /// album entity instead.
    #[test]
    fn cover_entity_skips_genres() {
        let mut s = store();
        s.add_entity("g1", EntityType::Genre, "Techno");
        assert!(s.cover_entity("g1").is_none());

        s.add_entity("g1", EntityType::Artist, "Surgeon");
        assert_eq!(
            s.cover_entity("g1").map(|e| e.value.as_str()),
            Some("Surgeon")
        );
    }

    #[test]
    fn groups_round_trip_through_json() {
        let mut s = store();
        s.add_entity("g1", EntityType::Album, "Selected Ambient Works");
        let json = serde_json::to_string(&s).expect("serialise");
        let back: GroupStore = serde_json::from_str(&json).expect("deserialise");
        assert!(back.has_entity("g1", EntityType::Album, "Selected Ambient Works"));
    }

    #[test]
    fn entity_type_parses_the_stored_strings() {
        assert_eq!(EntityType::parse("artist"), Some(EntityType::Artist));
        assert_eq!(EntityType::parse("ALBUM"), Some(EntityType::Album));
        assert_eq!(EntityType::parse("nonsense"), None);
    }

    /// Deleting a folder must report what it orphaned, so the caller can
    /// reassign anything pointing at it rather than leaving dangling ids.
    #[test]
    fn deleting_a_folder_reports_orphaned_children() {
        let mut f = FolderStore::new();
        f.create("parent", "Parent", "");
        f.create("child", "Child", "parent");
        f.create("other", "Other", "");

        let orphaned = f.delete("parent");
        assert_eq!(orphaned, vec!["child"]);
        assert_eq!(
            f.get("child").expect("child survives").parent_id,
            "",
            "the child should be promoted to the top level"
        );
        assert!(f.get("parent").is_none());
    }

    #[test]
    fn lists_children_of_a_folder() {
        let mut f = FolderStore::new();
        f.create("p", "Parent", "");
        f.create("c1", "One", "p");
        f.create("c2", "Two", "p");

        assert_eq!(f.children_of("p").len(), 2);
        assert_eq!(f.children_of("").len(), 1, "only the parent is top level");
    }
}
