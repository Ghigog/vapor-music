//! Device-to-device sync on a local network — SYNC-001 to SYNC-004.
//!
//! Two devices on the same Wi-Fi find each other, prove they belong to the
//! same person, work out what each is missing, and move the files directly.
//! No cloud in the middle, which is the whole point: the library already lives
//! in the owner's own storage, and a phone that wants a copy should not have
//! to pull it down through somebody else's server.
//!
//! ## What is here and what is not
//!
//! Everything in this module is a decision; none of it is a socket. The shell
//! owns the UDP broadcaster, the TCP server, the filesystem and the clock —
//! same split as the rest of the core, and it is what lets the interesting
//! parts be tested without two machines.
//!
//! Two consequences worth stating, because both look like omissions:
//!
//! * **No randomness.** The PIN is generated in the shell and handed in.
//!   `randi()` inside a library is what made the GDScript's mood path
//!   untestable, and a pairing code nobody can fix in a test is worse.
//! * **No wall clock.** Every function that cares about time takes the time as
//!   a parameter. Expiry is the only security property this module has that a
//!   test can actually check, and it cannot check it against `now()`.
//!
//! ## Trust
//!
//! A peer that has not been paired can do exactly one thing: ask to pair.
//! It cannot list the library, read a manifest or fetch a byte. That is
//! enforced by [`Trust::allows`] and asserted here, rather than left to each
//! call site in the shell to remember — a per-request check that has to be
//! repeated is one that eventually is not.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

/// Milliseconds since the Unix epoch, passed in by the shell.
pub type Millis = u64;

/// What kind of machine a peer is, for the dashboard to draw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum DeviceKind {
    Desktop,
    Phone,
    Tablet,
    #[default]
    Unknown,
}

// ---------------------------------------------------------------------------
// SYNC-001 — discovery
// ---------------------------------------------------------------------------

/// The wire version. Bumped when [`Advert`] or the handshake changes shape.
///
/// Checked on receipt so a newer build on the same subnet is ignored rather
/// than half-understood — the failure mode of a version-free protocol is two
/// devices that discover each other and then disagree about everything after.
pub const PROTOCOL: u32 = 1;

/// Prefix on every datagram.
///
/// A broadcast port is shared with whatever else is shouting on the subnet, so
/// the first thing a listener does is establish that a packet was meant for
/// it. Without this the parser is exposed to every stray UDP payload on the
/// network.
pub const MAGIC: &str = "vapor-sync/1";

/// What a device shouts on the subnet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Advert {
    /// Stable per installation, and **not** derived from anything about the
    /// person or the machine — see `DeviceId` in the shell.
    pub id: String,
    /// What the dashboard shows. Chosen by the owner.
    pub name: String,
    #[serde(default)]
    pub kind: DeviceKind,
    /// Where to reach this device's sync server.
    pub port: u16,
    pub protocol: u32,
}

impl Advert {
    /// The datagram to broadcast.
    pub fn encode(&self) -> String {
        format!(
            "{MAGIC} {}",
            serde_json::to_string(self).unwrap_or_default()
        )
    }

    /// Read a datagram, or `None` if it was not one of ours.
    ///
    /// Also rejects an advert with no id or a port of zero: both are
    /// unusable, and letting them into the registry means the dashboard
    /// offers a device that cannot be reached.
    pub fn decode(datagram: &str) -> Option<Advert> {
        let body = datagram.strip_prefix(MAGIC)?.trim_start();
        let advert: Advert = serde_json::from_str(body).ok()?;
        if advert.protocol != PROTOCOL || advert.id.trim().is_empty() || advert.port == 0 {
            return None;
        }
        Some(advert)
    }
}

/// A device seen on the network.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    /// Host and port, as the shell resolved it from the datagram's source.
    pub address: String,
    // A JSON number over IPC, not a `bigint`: serde_json writes u64 as a
    // plain number and the webview parses it as one. Values here are byte
    // counts and millisecond timestamps, far below 2^53.
    #[ts(type = "number")]
    pub last_seen: Millis,
}

/// How long a peer stays listed after its last advert.
///
/// Three missed broadcasts at the shell's five-second cadence. A device that
/// has walked out of the building should leave the list on its own, and one
/// that dropped a single packet should not.
pub const PEER_TTL: Millis = 15_000;

/// Who is on the network right now.
#[derive(Clone, Debug, Default)]
pub struct PeerRegistry {
    peers: Vec<Peer>,
}

impl PeerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an advert. Returns true when this is a device not seen before.
    ///
    /// The address comes from the datagram's source rather than from the
    /// advert's contents, so a device cannot name someone else's address and
    /// have the dashboard point at it.
    pub fn saw(&mut self, advert: &Advert, address: &str, now: Millis) -> bool {
        if let Some(existing) = self.peers.iter_mut().find(|p| p.id == advert.id) {
            existing.name = advert.name.clone();
            existing.kind = advert.kind;
            existing.address = address.to_string();
            existing.last_seen = now;
            return false;
        }
        self.peers.push(Peer {
            id: advert.id.clone(),
            name: advert.name.clone(),
            kind: advert.kind,
            address: address.to_string(),
            last_seen: now,
        });
        true
    }

    /// Peers heard from recently, and forget the rest.
    pub fn live(&mut self, now: Millis) -> &[Peer] {
        self.peers
            .retain(|p| now.saturating_sub(p.last_seen) <= PEER_TTL);
        &self.peers
    }

    pub fn get(&self, id: &str) -> Option<&Peer> {
        self.peers.iter().find(|p| p.id == id)
    }
}

// ---------------------------------------------------------------------------
// SYNC-002 — pairing
// ---------------------------------------------------------------------------

/// How long a displayed PIN is good for.
pub const PAIRING_WINDOW: Millis = 120_000;

/// How many wrong PINs before the attempt is abandoned.
///
/// A six-digit PIN is a million guesses; three tries makes brute force
/// pointless without making a typo fatal. Without a limit the code length is
/// decoration — an attacker on the subnet simply asks a million times.
pub const MAX_PIN_ATTEMPTS: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairOutcome {
    Paired,
    WrongPin {
        attempts_left: u32,
    },
    /// Too many wrong guesses, or the window closed. Either way this pairing
    /// is over and a fresh PIN is needed.
    Refused,
}

/// A pairing in progress: one PIN, one peer, one short window.
#[derive(Clone, Debug)]
pub struct Pairing {
    pin: String,
    peer_id: String,
    started: Millis,
    attempts: u32,
}

impl Pairing {
    /// Begin. The PIN is generated by the shell — the core owns no randomness.
    pub fn begin(pin: impl Into<String>, peer_id: impl Into<String>, now: Millis) -> Self {
        Pairing {
            pin: pin.into(),
            peer_id: peer_id.into(),
            started: now,
            attempts: 0,
        }
    }

    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }

    pub fn expired(&self, now: Millis) -> bool {
        now.saturating_sub(self.started) > PAIRING_WINDOW
    }

    /// Judge an offered PIN from `peer_id`.
    ///
    /// The peer is checked as well as the code: a PIN shown for one device is
    /// not an invitation to any device on the subnet that happens to see it.
    pub fn offer(&mut self, peer_id: &str, pin: &str, now: Millis) -> PairOutcome {
        if self.expired(now) || self.attempts >= MAX_PIN_ATTEMPTS {
            return PairOutcome::Refused;
        }
        if peer_id != self.peer_id || !constant_time_eq(&self.pin, pin) {
            self.attempts += 1;
            let attempts_left = MAX_PIN_ATTEMPTS.saturating_sub(self.attempts);
            return if attempts_left == 0 {
                PairOutcome::Refused
            } else {
                PairOutcome::WrongPin { attempts_left }
            };
        }
        PairOutcome::Paired
    }
}

/// Compare without giving the answer away in the timing.
///
/// A PIN is short and the attempt limit already makes brute force hopeless, so
/// this is belt and braces — but `==` on a string returns early at the first
/// wrong byte, and a comparison that leaks its progress is the kind of thing
/// that is free to avoid and awkward to retrofit.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A device this one has agreed to sync with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrustedDevice {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: DeviceKind,
    // A JSON number over IPC, not a `bigint`: serde_json writes u64 as a
    // plain number and the webview parses it as one. Values here are byte
    // counts and millisecond timestamps, far below 2^53.
    #[ts(type = "number")]
    pub paired_at: Millis,
}

/// Who this device has paired with. Persisted by the shell.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Trust {
    #[serde(default)]
    devices: Vec<TrustedDevice>,
}

impl Trust {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(&self) -> &[TrustedDevice] {
        &self.devices
    }

    /// **The gate.** Everything a peer can do beyond asking to pair goes
    /// through this.
    pub fn allows(&self, peer_id: &str) -> bool {
        !peer_id.trim().is_empty() && self.devices.iter().any(|d| d.id == peer_id)
    }

    /// Trust a device. Pairing again updates the name rather than listing it
    /// twice.
    pub fn add(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        kind: DeviceKind,
        now: Millis,
    ) {
        let id = id.into();
        let name = name.into();
        match self.devices.iter_mut().find(|d| d.id == id) {
            Some(existing) => {
                existing.name = name;
                existing.kind = kind;
            }
            None => self.devices.push(TrustedDevice {
                id,
                name,
                kind,
                paired_at: now,
            }),
        }
    }

    pub fn forget(&mut self, id: &str) -> bool {
        let before = self.devices.len();
        self.devices.retain(|d| d.id != id);
        before != self.devices.len()
    }
}

// ---------------------------------------------------------------------------
// SYNC-003 — reconciliation
// ---------------------------------------------------------------------------

/// One track, as the other device needs to understand it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackRecord {
    /// The library path. The identity of a track across devices.
    pub href: String,
    pub size: u64,
    /// SHA-256 of the file's content, when it has been read.
    ///
    /// Empty for a track this device knows of but has never held — a
    /// cloud-first library is mostly that, and a record with no digest is
    /// still worth exchanging because it says the track *exists*.
    #[serde(default)]
    pub digest: String,
    /// When this device last changed its mind about the track — a tempo
    /// correction, a re-analysis. Decides who wins on a conflict.
    #[serde(default)]
    pub updated: Millis,
}

/// What one device knows, for another to compare against.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub device_id: String,
    #[serde(default)]
    pub tracks: Vec<TrackRecord>,
    /// Playlists as (id, digest of contents, updated).
    #[serde(default)]
    pub playlists: Vec<PlaylistRecord>,
    #[serde(default)]
    pub generated: Millis,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistRecord {
    pub id: String,
    pub name: String,
    pub digest: String,
    #[serde(default)]
    pub updated: Millis,
}

/// The work a sync implies, from this device's point of view.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Delta {
    /// Tracks the other device has and this one does not.
    pub fetch: Vec<String>,
    /// Tracks this device has and the other does not.
    pub offer: Vec<String>,
    /// Present on both, with different content. The other device's copy is
    /// newer, so it wins.
    pub replace: Vec<String>,
    /// Playlists to take from the other device — absent here, or newer there.
    pub take_playlists: Vec<String>,
    /// Playlists to hand over.
    pub give_playlists: Vec<String>,
}

impl Delta {
    pub fn is_empty(&self) -> bool {
        self.fetch.is_empty()
            && self.offer.is_empty()
            && self.replace.is_empty()
            && self.take_playlists.is_empty()
            && self.give_playlists.is_empty()
    }

    /// How many separate things this sync will move.
    pub fn len(&self) -> usize {
        self.fetch.len() + self.replace.len() + self.take_playlists.len()
    }
}

/// Work out what a sync between these two would move.
///
/// Deliberately *not* symmetric in its handling of conflicts: where both
/// devices hold a track with different content, the **newer** record wins and
/// a tie stays put. A tie that moved would flip on every sync — two devices
/// each pulling the other's copy forever, which is the classic way a
/// reconciler becomes an infinite loop that looks like it is working.
pub fn reconcile(local: &Manifest, remote: &Manifest) -> Delta {
    use std::collections::HashMap;

    let theirs: HashMap<&str, &TrackRecord> =
        remote.tracks.iter().map(|t| (t.href.as_str(), t)).collect();
    let ours: HashMap<&str, &TrackRecord> =
        local.tracks.iter().map(|t| (t.href.as_str(), t)).collect();

    let mut delta = Delta::default();

    for track in &remote.tracks {
        match ours.get(track.href.as_str()) {
            None => delta.fetch.push(track.href.clone()),
            Some(mine) => {
                // A missing digest on either side means one of them has never
                // held the file. That is not a disagreement about content, so
                // it is not a reason to move bytes.
                let both_known = !mine.digest.is_empty() && !track.digest.is_empty();
                if both_known && mine.digest != track.digest && track.updated > mine.updated {
                    delta.replace.push(track.href.clone());
                }
            }
        }
    }

    for track in &local.tracks {
        if !theirs.contains_key(track.href.as_str()) {
            delta.offer.push(track.href.clone());
        }
    }

    let their_lists: HashMap<&str, &PlaylistRecord> = remote
        .playlists
        .iter()
        .map(|p| (p.id.as_str(), p))
        .collect();
    let our_lists: HashMap<&str, &PlaylistRecord> =
        local.playlists.iter().map(|p| (p.id.as_str(), p)).collect();

    for list in &remote.playlists {
        match our_lists.get(list.id.as_str()) {
            None => delta.take_playlists.push(list.id.clone()),
            Some(mine) if mine.digest != list.digest && list.updated > mine.updated => {
                delta.take_playlists.push(list.id.clone())
            }
            Some(_) => {}
        }
    }
    for list in &local.playlists {
        match their_lists.get(list.id.as_str()) {
            None => delta.give_playlists.push(list.id.clone()),
            Some(theirs) if theirs.digest != list.digest && list.updated > theirs.updated => {
                delta.give_playlists.push(list.id.clone())
            }
            Some(_) => {}
        }
    }

    // Stable output, so a sync plan reads the same twice and a test can assert
    // on it without sorting at every call site.
    delta.fetch.sort();
    delta.offer.sort();
    delta.replace.sort();
    delta.take_playlists.sort();
    delta.give_playlists.sort();
    delta
}

/// SHA-256 of some bytes, as lowercase hex.
///
/// SHA-256 rather than the MD5 the ticket names. The requirement is integrity
/// — "completed transfers match the source file checksums" — and MD5 has been
/// collision-broken since 2004, so a corrupted or substituted file is exactly
/// what it can no longer be trusted to catch. One hash for fingerprints and
/// transfers rather than two is also one fewer thing to get wrong.
pub fn digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex(&hasher.finalize())
}

/// A digest over a playlist's identity and contents.
pub fn playlist_digest(name: &str, tracks: &[String]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    for href in tracks {
        // Length-prefixed, so ["ab","c"] and ["a","bc"] are different
        // playlists. Concatenating without a separator makes them the same
        // one, and a reconciler that cannot tell them apart never syncs the
        // difference.
        hasher.update((href.len() as u64).to_le_bytes());
        hasher.update(href.as_bytes());
    }
    hex(&hasher.finalize())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

// ---------------------------------------------------------------------------
// SYNC-006 — the document kept on the WebDAV server
// ---------------------------------------------------------------------------

/// What every device agrees on, kept as one file beside the music.
///
/// The library is already in the owner's own storage, so the obvious place for
/// "which playlists exist" is next to it. A device that can read the music can
/// read this; one that cannot has no business with either.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shared {
    /// Bumped when the shape changes, so an older build refuses a document it
    /// would otherwise half-understand.
    pub version: u32,
    /// Which device wrote it last, and when.
    pub written_by: String,
    pub updated: Millis,
    #[serde(default)]
    pub playlists: Vec<crate::playlist::Playlist>,
    #[serde(default)]
    pub folders: Vec<crate::group::Folder>,
    /// Tempo corrections, keyed by href. A person's own claim about a track
    /// (TD-10), and the thing most worth carrying between devices — it is the
    /// one piece of analysis a human typed rather than a machine measured.
    #[serde(default)]
    pub bpm_overrides: std::collections::HashMap<String, f32>,
    /// What has been deleted, so a deletion travels (TD-57).
    #[serde(default)]
    pub deleted: Tombstones,
}

/// The current shape of [`Shared`].
///
/// 2 added [`Tombstones`]. The bump is the point rather than a formality: the
/// field is `#[serde(default)]`, so a version-1 build would read a document,
/// silently drop the tombstones it did not know about, and write back a
/// document in which every deletion had been undone. Refusing to read it is the
/// only safe thing an older build can do, and that refusal is what the version
/// check is for.
///
/// 3 added `Tombstones::tracks`, and the same argument applies unchanged: a
/// version-2 build drops the per-track removals on the floor and writes back a
/// document that puts every one of those tracks back into its playlist.
pub const SHARED_VERSION: u32 = 3;

/// Records of things that were deleted, and when.
///
/// A deletion is the one edit that additive merge cannot carry: everything else
/// is something to add, and there is nothing to add for a record that is gone.
/// So it is written down.
///
/// **Kept forever.** A device that has been off for a year still has the
/// playlist and still has to be told, and there is no moment at which it is
/// safe to say every device has heard. The cost is an id and a timestamp per
/// deletion, which is nothing against the playlists themselves.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Tombstones {
    /// Playlist id → when it was deleted.
    #[serde(default)]
    pub playlists: std::collections::HashMap<String, Millis>,
    /// Folder id → when it was deleted.
    #[serde(default)]
    pub folders: std::collections::HashMap<String, Millis>,
    /// Playlist id → href → when that track was taken out of that playlist.
    ///
    /// The same argument one level down. Deleting a playlist travelled; taking
    /// a track *out* of one did not, because [`merge_shared`] only ever adds
    /// and there is nothing to add for a track that is gone. Remove a track on
    /// the laptop and the phone still has it, writes it back, and the laptop
    /// puts it there again on the next sync — for ever, and silently, which is
    /// worse than the whole-playlist version of this bug because nothing on
    /// screen ever says a merge happened.
    ///
    /// Nested rather than a `"{id}\t{href}"` key so there is no separator to
    /// collide with a path, and so a deleted playlist's removals can be dropped
    /// in one step if that is ever wanted.
    #[serde(default)]
    pub tracks: std::collections::HashMap<String, std::collections::HashMap<String, Millis>>,
}

impl Tombstones {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.playlists.is_empty() && self.folders.is_empty() && self.tracks.is_empty()
    }

    pub fn record_playlist(&mut self, id: impl Into<String>, at: Millis) {
        keep_earliest(&mut self.playlists, id.into(), at);
    }

    pub fn record_folder(&mut self, id: impl Into<String>, at: Millis) {
        keep_earliest(&mut self.folders, id.into(), at);
    }

    /// One track taken out of one playlist.
    pub fn record_track(
        &mut self,
        playlist_id: impl Into<String>,
        href: impl Into<String>,
        at: Millis,
    ) {
        keep_earliest(
            self.tracks.entry(playlist_id.into()).or_default(),
            href.into(),
            at,
        );
    }

    pub fn playlist_deleted(&self, id: &str) -> bool {
        self.playlists.contains_key(id)
    }

    pub fn track_removed(&self, playlist_id: &str, href: &str) -> bool {
        self.tracks
            .get(playlist_id)
            .is_some_and(|hrefs| hrefs.contains_key(href))
    }

    /// Forget the removals for a playlist.
    ///
    /// Called when the playlist itself is deleted: the whole-playlist tombstone
    /// already stops it being recreated, so the per-track records under it can
    /// never be consulted again and would otherwise accumulate for ever.
    pub fn forget_tracks_of(&mut self, playlist_id: &str) {
        self.tracks.remove(playlist_id);
    }

    pub fn folder_deleted(&self, id: &str) -> bool {
        self.folders.contains_key(id)
    }

    /// Take in everything `other` knows about. Returns how many records are new
    /// here, which is how many things this device is about to delete.
    pub fn absorb(&mut self, other: &Tombstones) -> (usize, usize) {
        let before = (self.playlists.len(), self.folders.len());
        for (id, at) in &other.playlists {
            keep_earliest(&mut self.playlists, id.clone(), *at);
        }
        for (id, at) in &other.folders {
            keep_earliest(&mut self.folders, id.clone(), *at);
        }
        for (playlist_id, hrefs) in &other.tracks {
            let mine = self.tracks.entry(playlist_id.clone()).or_default();
            for (href, at) in hrefs {
                keep_earliest(mine, href.clone(), *at);
            }
        }
        (
            self.playlists.len() - before.0,
            self.folders.len() - before.1,
        )
    }
}

/// The earliest time wins, so two devices deleting the same thing agree on when
/// it happened whichever order they sync in.
fn keep_earliest(map: &mut std::collections::HashMap<String, Millis>, id: String, at: Millis) {
    map.entry(id)
        .and_modify(|t| *t = (*t).min(at))
        .or_insert(at);
}

/// What a merge changed, for the screen to report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergeReport {
    pub playlists_added: usize,
    pub playlists_extended: usize,
    pub folders_added: usize,
    pub tempos_added: usize,
    /// Deleted here because another device deleted them (TD-57).
    pub playlists_deleted: usize,
    pub folders_deleted: usize,
    /// Taken out of a playlist here because another device took them out.
    pub tracks_removed: usize,
}

impl MergeReport {
    pub fn is_empty(&self) -> bool {
        *self == MergeReport::default()
    }
}

/// Fold a document from the server into what this device holds.
///
/// **Additive for everything that exists.** Nothing is overwritten: a playlist
/// absent here is taken, a playlist present here gains any tracks it was
/// missing, and a tempo correction is accepted only where this device has none
/// of its own.
///
/// The alternative — last writer wins per record — needs a modification time
/// on every playlist, and without one it degrades into "whichever device
/// synced most recently is right", which loses work silently. Additive merge
/// cannot lose anything, converges in one pass, and is the same answer
/// whichever order two devices sync in.
///
/// ## Deletions, which are the exception (TD-57)
///
/// A deletion is the one edit an additive merge cannot carry, because there is
/// nothing to add for a record that is gone. Without [`Tombstones`] a playlist
/// removed on the laptop came back the next time the phone wrote the document —
/// not occasionally, but every time.
///
/// So a tombstone applies, and it applies **unconditionally**: an id that has
/// been deleted anywhere is deleted here, whatever the other device's copy
/// still says. That is a real trade and it is worth naming rather than
/// implying. If one device deletes a playlist while another adds tracks to it
/// without having heard, the deletion wins and those additions are lost — the
/// tracks themselves are untouched in the library, but the playlist is gone.
///
/// Refining that needs a modification time on every playlist, so an edit newer
/// than the tombstone could keep the playlist alive. It is not here because the
/// clock lives in the shell rather than in this crate, `get_mut` hands out
/// unguarded mutable access, and a modification time that some mutations forget
/// to set is worse than none — it would make the merge confidently wrong
/// instead of predictably blunt. Weighed against a deletion that presently
/// fails to travel *every single time*, blunt is the better of the two.
///
/// Both directions converge and neither depends on sync order: the tombstone
/// sets are unioned first, and both the incoming and the local list are then
/// filtered against the union.
/// The incoming playlist's tracks, minus anything some device has taken out.
///
/// Without it a removal and an addition are not symmetric: the removal is
/// recorded, and then the next merge puts the track straight back, because a
/// peer that has not heard about it yet still lists it in its copy.
fn wanted_tracks(deleted: &Tombstones, incoming: &crate::playlist::Playlist) -> Vec<String> {
    incoming
        .tracks
        .iter()
        .filter(|href| !deleted.track_removed(&incoming.id, href))
        .cloned()
        .collect()
}

pub fn merge_shared(
    playlists: &mut crate::playlist::PlaylistStore,
    folders: &mut crate::group::FolderStore,
    overrides: &mut std::collections::HashMap<String, f32>,
    deleted: &mut Tombstones,
    remote: &Shared,
) -> MergeReport {
    let mut report = MergeReport::default();

    // The union first, so what follows can be a single filter in both
    // directions rather than two rules that have to agree with each other.
    deleted.absorb(&remote.deleted);

    // Applied to what is already here before anything is taken in — a playlist
    // this device deleted is not re-created below by the copy that still has
    // it, and a playlist another device deleted goes now.
    for id in deleted.playlists.keys() {
        if playlists.delete(id).is_some() {
            report.playlists_deleted += 1;
        }
    }
    for id in deleted.folders.keys() {
        if folders.get(id).is_some() {
            folders.delete(id);
            report.folders_deleted += 1;
            // A folder is organisation, not ownership, so its playlists are
            // rehomed rather than deleted with it — the same rule the shell
            // applies when a folder is deleted here. Without this they keep an
            // id that no longer resolves and disappear from every view that
            // files by folder, which looks exactly like losing them.
            let homeless: Vec<String> = playlists
                .all()
                .iter()
                .filter(|p| p.folder_id == *id)
                .map(|p| p.id.clone())
                .collect();
            for playlist in homeless {
                playlists.set_folder(&playlist, "");
            }
        }
    }

    // The same rule one level down, and for the same reason: a track this
    // device took out is not put back by the copy that still lists it, and a
    // track another device took out goes now. `remove_track` is by index, so
    // the href is resolved first — and the lookup ends before the mutable call
    // rather than being held across it.
    for (playlist_id, hrefs) in &deleted.tracks {
        for href in hrefs.keys() {
            let index = playlists
                .get(playlist_id)
                .and_then(|p| p.tracks.iter().position(|t| t == href));
            if let Some(index) = index {
                if playlists.remove_track(playlist_id, index) {
                    report.tracks_removed += 1;
                }
            }
        }
    }

    for folder in &remote.folders {
        if deleted.folder_deleted(&folder.id) {
            continue;
        }
        if folders.get(&folder.id).is_none() {
            folders.create(
                folder.id.clone(),
                folder.name.clone(),
                folder.parent_id.clone(),
            );
            report.folders_added += 1;
        }
    }

    for incoming in &remote.playlists {
        if deleted.playlist_deleted(&incoming.id) {
            continue;
        }
        match playlists.get(&incoming.id) {
            None => {
                playlists.create_in_folder(
                    incoming.id.clone(),
                    incoming.name.clone(),
                    incoming.folder_id.clone(),
                );
                let added = playlists.add_tracks(&incoming.id, &wanted_tracks(deleted, incoming));
                let _ = added;
                report.playlists_added += 1;
            }
            Some(_) => {
                // `add_tracks` skips what is already there and returns how
                // many actually landed, which is exactly the question here.
                let added = playlists.add_tracks(&incoming.id, &wanted_tracks(deleted, incoming));
                if added > 0 {
                    report.playlists_extended += 1;
                }
            }
        }
    }

    for (href, bpm) in &remote.bpm_overrides {
        // A correction typed on this device is not overruled by one from
        // elsewhere. Two people disagreeing about a tempo is a real thing, and
        // the one sitting in front of this machine wins on this machine.
        if !overrides.contains_key(href) && bpm.is_finite() && *bpm > 0.0 {
            overrides.insert(href.clone(), *bpm);
            report.tempos_added += 1;
        }
    }

    report
}

// ---------------------------------------------------------------------------
// SYNC-004 — transfer
// ---------------------------------------------------------------------------

/// How much is asked for at a time.
///
/// Big enough that the per-request overhead disappears against a gigabit LAN,
/// small enough that an interrupted transfer loses about a quarter of a second
/// of work rather than a whole album track.
pub const CHUNK: u64 = 1024 * 1024;

/// The byte range to ask for next, given how much is already on disk.
///
/// `None` when the file is complete. Returning a range rather than a boolean
/// is what makes resume free: a partial file's length *is* the offset to
/// continue from, so nothing about progress has to be written down separately
/// and there is no progress record to disagree with the file.
pub fn next_chunk(have: u64, total: u64) -> Option<(u64, u64)> {
    if total == 0 || have >= total {
        return None;
    }
    Some((have, (total - have).min(CHUNK)))
}

/// Every chunk of a file, for planning or for a progress bar.
pub fn chunks(total: u64) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some((offset, len)) = next_chunk(at, total) {
        out.push((offset, len));
        at += len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advert() -> Advert {
        Advert {
            id: "device-a".into(),
            name: "Dylan's Mac".into(),
            kind: DeviceKind::Desktop,
            port: 7677,
            protocol: PROTOCOL,
        }
    }

    // --- Discovery ---------------------------------------------------------

    #[test]
    fn an_advert_round_trips() {
        let decoded = Advert::decode(&advert().encode()).expect("ours");
        assert_eq!(decoded, advert());
    }

    /// A broadcast port is shared with whatever else is on the subnet, so the
    /// first job of a listener is to establish that a packet was meant for it.
    #[test]
    fn a_datagram_from_something_else_is_not_ours() {
        assert!(Advert::decode("").is_none());
        assert!(Advert::decode("{\"id\":\"x\"}").is_none());
        assert!(Advert::decode("some other protocol entirely").is_none());
        assert!(Advert::decode(&format!("{MAGIC} not json")).is_none());
    }

    /// A newer build on the same network is ignored rather than
    /// half-understood: two devices that discover each other and then disagree
    /// about everything after is the failure mode of a version-free protocol.
    #[test]
    fn an_advert_from_another_protocol_version_is_ignored() {
        let mut future = advert();
        future.protocol = PROTOCOL + 1;

        assert!(Advert::decode(&future.encode()).is_none());
    }

    /// Both are unusable, and admitting one means the dashboard offers a
    /// device that cannot be reached.
    #[test]
    fn an_advert_with_no_id_or_no_port_is_refused() {
        let mut anonymous = advert();
        anonymous.id = "  ".into();
        assert!(Advert::decode(&anonymous.encode()).is_none());

        let mut unreachable = advert();
        unreachable.port = 0;
        assert!(Advert::decode(&unreachable.encode()).is_none());
    }

    #[test]
    fn a_peer_is_listed_once_however_often_it_shouts() {
        let mut registry = PeerRegistry::new();

        assert!(registry.saw(&advert(), "192.168.1.20:7677", 1_000));
        assert!(!registry.saw(&advert(), "192.168.1.20:7677", 6_000));

        assert_eq!(registry.live(6_000).len(), 1);
    }

    /// A device that has left the building drops off the list on its own; one
    /// that missed a single broadcast does not.
    #[test]
    fn a_peer_that_stops_shouting_is_forgotten() {
        let mut registry = PeerRegistry::new();
        registry.saw(&advert(), "192.168.1.20:7677", 1_000);

        assert_eq!(registry.live(1_000 + PEER_TTL).len(), 1, "one missed beat");
        assert_eq!(registry.live(1_000 + PEER_TTL + 1).len(), 0);
    }

    /// The address comes from the datagram's source, not from its contents —
    /// otherwise a device can name someone else's address and have the
    /// dashboard point at it.
    #[test]
    fn a_moved_peer_is_reachable_at_where_it_actually_shouted_from() {
        let mut registry = PeerRegistry::new();
        registry.saw(&advert(), "192.168.1.20:7677", 1_000);
        registry.saw(&advert(), "192.168.1.55:7677", 2_000);

        assert_eq!(
            registry.get("device-a").unwrap().address,
            "192.168.1.55:7677"
        );
    }

    // --- Pairing -----------------------------------------------------------

    #[test]
    fn the_right_pin_from_the_right_peer_pairs() {
        let mut pairing = Pairing::begin("482913", "device-b", 0);

        assert_eq!(
            pairing.offer("device-b", "482913", 500),
            PairOutcome::Paired
        );
    }

    /// A PIN shown for one device is not an invitation to every device on the
    /// subnet that can see the screen.
    #[test]
    fn the_right_pin_from_another_peer_does_not_pair() {
        let mut pairing = Pairing::begin("482913", "device-b", 0);

        assert!(matches!(
            pairing.offer("someone-else", "482913", 500),
            PairOutcome::WrongPin { .. }
        ));
    }

    /// Without a limit the code length is decoration: an attacker on the
    /// subnet asks a million times.
    #[test]
    fn three_wrong_pins_end_the_attempt() {
        let mut pairing = Pairing::begin("482913", "device-b", 0);

        assert_eq!(
            pairing.offer("device-b", "000000", 1),
            PairOutcome::WrongPin { attempts_left: 2 }
        );
        assert_eq!(
            pairing.offer("device-b", "111111", 2),
            PairOutcome::WrongPin { attempts_left: 1 }
        );
        assert_eq!(pairing.offer("device-b", "222222", 3), PairOutcome::Refused);
        // And the real PIN no longer helps.
        assert_eq!(pairing.offer("device-b", "482913", 4), PairOutcome::Refused);
    }

    #[test]
    fn a_pin_goes_stale() {
        let mut pairing = Pairing::begin("482913", "device-b", 0);

        assert!(!pairing.expired(PAIRING_WINDOW));
        assert_eq!(
            pairing.offer("device-b", "482913", PAIRING_WINDOW + 1),
            PairOutcome::Refused
        );
    }

    // --- Trust -------------------------------------------------------------

    /// The gate. Everything beyond asking to pair goes through it.
    #[test]
    fn an_unpaired_device_is_allowed_nothing() {
        let mut trust = Trust::new();
        assert!(!trust.allows("device-b"));

        trust.add("device-b", "Dylan's Phone", DeviceKind::Phone, 1_000);
        assert!(trust.allows("device-b"));
        assert!(!trust.allows("device-c"));
    }

    /// An empty id must never pass. A peer that sent no id at all would
    /// otherwise be compared against nothing and, on some future refactor,
    /// match a default-constructed record.
    #[test]
    fn an_empty_id_is_never_trusted() {
        let mut trust = Trust::new();
        trust.add("", "", DeviceKind::Unknown, 0);

        assert!(!trust.allows(""));
        assert!(!trust.allows("   "));
    }

    #[test]
    fn pairing_again_updates_rather_than_duplicates() {
        let mut trust = Trust::new();
        trust.add("device-b", "Old Name", DeviceKind::Phone, 1_000);
        trust.add("device-b", "New Name", DeviceKind::Tablet, 2_000);

        assert_eq!(trust.all().len(), 1);
        assert_eq!(trust.all()[0].name, "New Name");
        // The date it was first trusted is not rewritten by a rename.
        assert_eq!(trust.all()[0].paired_at, 1_000);
    }

    #[test]
    fn forgetting_a_device_revokes_it() {
        let mut trust = Trust::new();
        trust.add("device-b", "Phone", DeviceKind::Phone, 0);

        assert!(trust.forget("device-b"));
        assert!(!trust.allows("device-b"));
        assert!(!trust.forget("device-b"), "already gone");
    }

    // --- Reconciliation ----------------------------------------------------

    fn track(href: &str, digest: &str, updated: Millis) -> TrackRecord {
        TrackRecord {
            href: href.into(),
            size: 4_000_000,
            digest: digest.into(),
            updated,
        }
    }

    fn manifest(id: &str, tracks: Vec<TrackRecord>) -> Manifest {
        Manifest {
            device_id: id.into(),
            tracks,
            playlists: Vec::new(),
            generated: 1_000,
        }
    }

    #[test]
    fn what_only_they_have_is_fetched_and_what_only_we_have_is_offered() {
        let local = manifest("a", vec![track("/mine.m4a", "aa", 1)]);
        let remote = manifest("b", vec![track("/theirs.m4a", "bb", 1)]);

        let delta = reconcile(&local, &remote);

        assert_eq!(delta.fetch, ["/theirs.m4a"]);
        assert_eq!(delta.offer, ["/mine.m4a"]);
        assert!(delta.replace.is_empty());
    }

    #[test]
    fn a_track_both_devices_have_unchanged_moves_nothing() {
        let local = manifest("a", vec![track("/same.m4a", "aa", 1)]);
        let remote = manifest("b", vec![track("/same.m4a", "aa", 9)]);

        assert!(reconcile(&local, &remote).is_empty());
    }

    #[test]
    fn a_newer_copy_of_a_changed_track_wins() {
        let local = manifest("a", vec![track("/t.m4a", "old", 1)]);
        let remote = manifest("b", vec![track("/t.m4a", "new", 2)]);

        assert_eq!(reconcile(&local, &remote).replace, ["/t.m4a"]);
    }

    /// The loop-forever case. If a tie moved, each device would pull the
    /// other's copy on every sync, and a reconciler that never converges looks
    /// exactly like one that is working hard.
    #[test]
    fn a_tie_does_not_move_and_an_older_copy_does_not_win() {
        let local = manifest("a", vec![track("/t.m4a", "mine", 5)]);

        let tie = manifest("b", vec![track("/t.m4a", "theirs", 5)]);
        assert!(
            reconcile(&local, &tie).replace.is_empty(),
            "a tie stays put"
        );

        let older = manifest("b", vec![track("/t.m4a", "theirs", 4)]);
        assert!(reconcile(&local, &older).replace.is_empty());
    }

    /// A cloud-first library is mostly tracks this device has never held. That
    /// is not a disagreement about content and must not move bytes.
    #[test]
    fn a_track_neither_device_has_read_is_not_a_conflict() {
        let local = manifest("a", vec![track("/t.m4a", "", 1)]);
        let remote = manifest("b", vec![track("/t.m4a", "theirs", 2)]);

        assert!(reconcile(&local, &remote).replace.is_empty());
    }

    #[test]
    fn a_playlist_they_have_and_we_do_not_is_taken() {
        let mut local = manifest("a", vec![]);
        let mut remote = manifest("b", vec![]);
        remote.playlists.push(PlaylistRecord {
            id: "p1".into(),
            name: "Late Night".into(),
            digest: "d1".into(),
            updated: 5,
        });
        local.playlists.push(PlaylistRecord {
            id: "p2".into(),
            name: "Warm Up".into(),
            digest: "d2".into(),
            updated: 5,
        });

        let delta = reconcile(&local, &remote);
        assert_eq!(delta.take_playlists, ["p1"]);
        assert_eq!(delta.give_playlists, ["p2"]);
    }

    /// Concatenating hrefs without a separator makes ["ab","c"] and ["a","bc"]
    /// the same playlist, and a reconciler that cannot tell them apart never
    /// syncs the difference between them.
    #[test]
    fn two_playlists_that_differ_only_in_where_the_boundary_is_digest_differently() {
        let one = playlist_digest("Set", &["ab".into(), "c".into()]);
        let other = playlist_digest("Set", &["a".into(), "bc".into()]);

        assert_ne!(one, other);
    }

    #[test]
    fn a_renamed_playlist_digests_differently() {
        let tracks = vec!["/a.m4a".to_string()];
        assert_ne!(
            playlist_digest("Before", &tracks),
            playlist_digest("After", &tracks)
        );
    }

    #[test]
    fn a_digest_is_sha256_hex() {
        // The published SHA-256 of the empty input, so this pins the algorithm
        // rather than merely pinning whatever this build happens to compute.
        assert_eq!(
            digest(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    // --- The shared document (SYNC-006) ------------------------------------

    use crate::group::FolderStore;
    use crate::playlist::PlaylistStore;

    fn shared_with(playlists: Vec<(&str, &str, Vec<&str>)>) -> Shared {
        let mut store = PlaylistStore::new();
        for (id, name, tracks) in playlists {
            store.create(id, name);
            let hrefs: Vec<String> = tracks.iter().map(|t| t.to_string()).collect();
            store.add_tracks(id, &hrefs);
        }
        Shared {
            version: SHARED_VERSION,
            written_by: "other-device".into(),
            updated: 10,
            playlists: store.all().to_vec(),
            folders: Vec::new(),
            bpm_overrides: Default::default(),
            deleted: Tombstones::new(),
        }
    }

    #[test]
    fn a_playlist_only_the_server_has_arrives() {
        let mut playlists = PlaylistStore::new();
        let mut folders = FolderStore::new();
        let mut overrides = std::collections::HashMap::new();
        let mut deleted = Tombstones::new();
        let remote = shared_with(vec![("p1", "Late Night", vec!["/a.m4a", "/b.m4a"])]);

        let report = merge_shared(
            &mut playlists,
            &mut folders,
            &mut overrides,
            &mut deleted,
            &remote,
        );

        assert_eq!(report.playlists_added, 1);
        let landed = playlists.get("p1").expect("arrived");
        assert_eq!(landed.name, "Late Night");
        assert_eq!(landed.tracks.len(), 2);
    }

    /// Additive on contents too: a playlist edited on both devices ends up
    /// with both edits rather than one of them.
    #[test]
    fn a_playlist_edited_on_both_devices_keeps_both_edits() {
        let mut playlists = PlaylistStore::new();
        playlists.create("p1", "Late Night");
        playlists.add_tracks("p1", &["/here.m4a".to_string()]);
        let mut folders = FolderStore::new();
        let mut overrides = std::collections::HashMap::new();
        let mut deleted = Tombstones::new();

        let remote = shared_with(vec![("p1", "Late Night", vec!["/there.m4a"])]);
        let report = merge_shared(
            &mut playlists,
            &mut folders,
            &mut overrides,
            &mut deleted,
            &remote,
        );

        assert_eq!(report.playlists_extended, 1);
        let merged = playlists.get("p1").expect("still here");
        assert!(merged.tracks.contains(&"/here.m4a".to_string()));
        assert!(merged.tracks.contains(&"/there.m4a".to_string()));
    }

    /// Merging twice must not keep reporting changes, or the screen tells a
    /// person something happened every time they open the app.
    #[test]
    fn merging_the_same_document_twice_changes_nothing_the_second_time() {
        let mut playlists = PlaylistStore::new();
        let mut folders = FolderStore::new();
        let mut overrides = std::collections::HashMap::new();
        let mut deleted = Tombstones::new();
        let remote = shared_with(vec![("p1", "Late Night", vec!["/a.m4a"])]);

        merge_shared(
            &mut playlists,
            &mut folders,
            &mut overrides,
            &mut deleted,
            &remote,
        );
        let again = merge_shared(
            &mut playlists,
            &mut folders,
            &mut overrides,
            &mut deleted,
            &remote,
        );

        assert!(again.is_empty(), "{again:?}");
        assert_eq!(playlists.all().len(), 1);
        assert_eq!(playlists.get("p1").unwrap().tracks.len(), 1);
    }

    /// Two people disagreeing about a tempo is a real thing, and the one
    /// sitting in front of this machine wins on this machine.
    #[test]
    fn a_local_tempo_correction_is_not_overruled_by_a_remote_one() {
        let mut playlists = PlaylistStore::new();
        let mut folders = FolderStore::new();
        let mut overrides = std::collections::HashMap::new();
        let mut deleted = Tombstones::new();
        overrides.insert("/a.m4a".to_string(), 128.0);

        let mut remote = shared_with(vec![]);
        remote.bpm_overrides.insert("/a.m4a".into(), 64.0);
        remote.bpm_overrides.insert("/b.m4a".into(), 140.0);

        let report = merge_shared(
            &mut playlists,
            &mut folders,
            &mut overrides,
            &mut deleted,
            &remote,
        );

        assert_eq!(overrides["/a.m4a"], 128.0, "mine stands");
        assert_eq!(overrides["/b.m4a"], 140.0, "theirs fills a gap");
        assert_eq!(report.tempos_added, 1);
    }

    /// A corrupt or hostile document must not be able to poison the settings
    /// file — `f32::INFINITY` survives a `> 0.0` check and serialises as
    /// `null`, which loses the whole file on the next read (see MAX_MANUAL_BPM).
    #[test]
    fn a_tempo_that_is_not_a_number_is_not_taken() {
        let mut playlists = PlaylistStore::new();
        let mut folders = FolderStore::new();
        let mut overrides = std::collections::HashMap::new();
        let mut deleted = Tombstones::new();

        let mut remote = shared_with(vec![]);
        remote.bpm_overrides.insert("/a.m4a".into(), f32::INFINITY);
        remote.bpm_overrides.insert("/b.m4a".into(), f32::NAN);
        remote.bpm_overrides.insert("/c.m4a".into(), -5.0);

        merge_shared(
            &mut playlists,
            &mut folders,
            &mut overrides,
            &mut deleted,
            &remote,
        );

        assert!(overrides.is_empty(), "{overrides:?}");
    }

    #[test]
    fn a_folder_only_the_server_has_arrives_once() {
        let mut playlists = PlaylistStore::new();
        let mut folders = FolderStore::new();
        let mut overrides = std::collections::HashMap::new();
        let mut deleted = Tombstones::new();

        let mut remote = shared_with(vec![]);
        remote.folders.push(crate::group::Folder {
            id: "f1".into(),
            name: "Sets".into(),
            parent_id: String::new(),
        });

        assert_eq!(
            merge_shared(
                &mut playlists,
                &mut folders,
                &mut overrides,
                &mut deleted,
                &remote
            )
            .folders_added,
            1
        );
        assert_eq!(
            merge_shared(
                &mut playlists,
                &mut folders,
                &mut overrides,
                &mut deleted,
                &remote
            )
            .folders_added,
            0
        );
        assert_eq!(folders.all().len(), 1);
    }

    // -----------------------------------------------------------------------
    // TD-57 — deletions that travel
    // -----------------------------------------------------------------------

    /// The same bug one level down, and the reason `Tombstones::tracks`
    /// exists. Take a track out here, sync with a device that has not heard,
    /// and the additive merge put it straight back — every time.
    ///
    /// Worse than the whole-playlist version: a playlist reappearing is
    /// visible, a track sliding back into a forty-track playlist is not.
    #[test]
    fn a_track_removed_here_is_not_restored_by_a_device_that_still_lists_it() {
        let mut playlists = PlaylistStore::new();
        let mut folders = FolderStore::new();
        let mut overrides = std::collections::HashMap::new();
        let mut deleted = Tombstones::new();

        // The playlist exists here, with the track already taken out.
        playlists.create("p1", "Late Night");
        playlists.add_tracks("p1", &["/a.m4a".to_string(), "/c.m4a".to_string()]);
        deleted.record_track("p1", "/b.m4a", 100);

        // The peer still has all three.
        let remote = shared_with(vec![(
            "p1",
            "Late Night",
            vec!["/a.m4a", "/b.m4a", "/c.m4a"],
        )]);
        merge_shared(
            &mut playlists,
            &mut folders,
            &mut overrides,
            &mut deleted,
            &remote,
        );

        let tracks = &playlists.get("p1").expect("playlist").tracks;
        assert!(
            !tracks.iter().any(|t| t == "/b.m4a"),
            "the removed track came back: {tracks:?}"
        );
        assert_eq!(tracks.len(), 2);
    }

    /// And the other direction: the removal was made elsewhere, so it has to
    /// be applied here rather than merely not undone.
    #[test]
    fn a_track_removed_elsewhere_is_taken_out_here() {
        let mut playlists = PlaylistStore::new();
        let mut folders = FolderStore::new();
        let mut overrides = std::collections::HashMap::new();
        let mut deleted = Tombstones::new();

        playlists.create("p1", "Late Night");
        playlists.add_tracks(
            "p1",
            &[
                "/a.m4a".to_string(),
                "/b.m4a".to_string(),
                "/c.m4a".to_string(),
            ],
        );

        let mut remote = shared_with(vec![("p1", "Late Night", vec!["/a.m4a", "/c.m4a"])]);
        remote.deleted.record_track("p1", "/b.m4a", 100);

        let report = merge_shared(
            &mut playlists,
            &mut folders,
            &mut overrides,
            &mut deleted,
            &remote,
        );

        assert_eq!(report.tracks_removed, 1);
        let tracks = &playlists.get("p1").expect("playlist").tracks;
        assert_eq!(tracks, &["/a.m4a".to_string(), "/c.m4a".to_string()]);

        // And it stays gone on a second pass, having been absorbed locally.
        merge_shared(
            &mut playlists,
            &mut folders,
            &mut overrides,
            &mut deleted,
            &remote,
        );
        assert_eq!(playlists.get("p1").expect("playlist").tracks.len(), 2);
    }

    /// The bug, stated as a test: this device deleted a playlist, the other
    /// device has not heard and still has it, and the merge must not bring it
    /// back. Before tombstones it did — every time, not occasionally.
    #[test]
    fn a_playlist_deleted_here_is_not_restored_by_a_device_that_still_has_it() {
        let mut playlists = PlaylistStore::new();
        let mut folders = FolderStore::new();
        let mut overrides = std::collections::HashMap::new();
        let mut deleted = Tombstones::new();

        // Deleted locally, and written down.
        deleted.record_playlist("p1", 100);

        let remote = shared_with(vec![("p1", "Late Night", vec!["/a.m4a"])]);
        let report = merge_shared(
            &mut playlists,
            &mut folders,
            &mut overrides,
            &mut deleted,
            &remote,
        );

        assert_eq!(report.playlists_added, 0);
        assert!(
            playlists.get("p1").is_none(),
            "a deleted playlist came back from the other device"
        );
    }

    /// And the other direction: the deletion happened elsewhere, so it has to
    /// arrive and take effect here.
    #[test]
    fn a_playlist_deleted_elsewhere_is_deleted_here() {
        let mut playlists = PlaylistStore::new();
        playlists.create("p1", "Late Night");
        playlists.create("p2", "Kept");
        let mut folders = FolderStore::new();
        let mut overrides = std::collections::HashMap::new();
        let mut deleted = Tombstones::new();

        let mut remote = shared_with(vec![]);
        remote.deleted.record_playlist("p1", 100);

        let report = merge_shared(
            &mut playlists,
            &mut folders,
            &mut overrides,
            &mut deleted,
            &remote,
        );

        assert_eq!(report.playlists_deleted, 1);
        assert!(playlists.get("p1").is_none());
        assert!(playlists.get("p2").is_some(), "took the wrong one");
        // Kept, so the next device to sync also hears about it. A tombstone
        // that is applied and then forgotten resurrects the playlist on the
        // third device.
        assert!(deleted.playlist_deleted("p1"));
    }

    #[test]
    fn a_folder_deleted_elsewhere_is_deleted_here() {
        let mut playlists = PlaylistStore::new();
        let mut folders = FolderStore::new();
        folders.create("f1", "Sets", String::new());
        let mut overrides = std::collections::HashMap::new();
        let mut deleted = Tombstones::new();

        let mut remote = shared_with(vec![]);
        remote.deleted.record_folder("f1", 100);
        // The other device still lists it, since it has not heard.
        remote.folders.push(crate::group::Folder {
            id: "f1".into(),
            name: "Sets".into(),
            parent_id: String::new(),
        });

        let report = merge_shared(
            &mut playlists,
            &mut folders,
            &mut overrides,
            &mut deleted,
            &remote,
        );

        assert_eq!(report.folders_deleted, 1);
        assert!(folders.get("f1").is_none(), "the folder came back");
    }

    /// Deletion is not a special case for convergence: merging twice must
    /// change nothing the second time, and the second merge must not report a
    /// deletion it already performed.
    #[test]
    fn a_deletion_converges_and_is_reported_once() {
        let mut playlists = PlaylistStore::new();
        playlists.create("p1", "Late Night");
        let mut folders = FolderStore::new();
        let mut overrides = std::collections::HashMap::new();
        let mut deleted = Tombstones::new();

        let mut remote = shared_with(vec![]);
        remote.deleted.record_playlist("p1", 100);

        let first = merge_shared(
            &mut playlists,
            &mut folders,
            &mut overrides,
            &mut deleted,
            &remote,
        );
        let second = merge_shared(
            &mut playlists,
            &mut folders,
            &mut overrides,
            &mut deleted,
            &remote,
        );

        assert_eq!(first.playlists_deleted, 1);
        assert!(
            second.is_empty(),
            "the second merge did something: {second:?}"
        );
    }

    /// The trade named in `merge_shared`'s documentation, asserted rather than
    /// only described: a deletion beats a concurrent edit it never saw. This is
    /// a real loss, and a test is the honest place to record that it is the
    /// chosen behaviour rather than an accident.
    #[test]
    fn a_deletion_beats_a_concurrent_edit_and_that_is_deliberate() {
        let mut playlists = PlaylistStore::new();
        let mut folders = FolderStore::new();
        let mut overrides = std::collections::HashMap::new();
        let mut deleted = Tombstones::new();
        deleted.record_playlist("p1", 100);

        // The other device added tracks at 200 — after the deletion, without
        // having heard about it.
        let mut remote = shared_with(vec![("p1", "Late Night", vec!["/a.m4a", "/b.m4a"])]);
        remote.updated = 200;

        merge_shared(
            &mut playlists,
            &mut folders,
            &mut overrides,
            &mut deleted,
            &remote,
        );

        assert!(playlists.get("p1").is_none());
    }

    /// Two devices deleting the same thing agree on when it happened, whichever
    /// order they sync in.
    #[test]
    fn the_earliest_deletion_time_wins_either_way_round() {
        let mut a = Tombstones::new();
        a.record_playlist("p1", 500);
        a.record_playlist("p1", 100);

        let mut b = Tombstones::new();
        b.record_playlist("p1", 100);
        b.record_playlist("p1", 500);

        assert_eq!(a, b);
        assert_eq!(a.playlists.get("p1"), Some(&100));
    }

    #[test]
    fn the_shared_document_round_trips_through_json() {
        let remote = shared_with(vec![("p1", "Late Night", vec!["/a.m4a"])]);
        let text = serde_json::to_string(&remote).expect("write");

        assert_eq!(serde_json::from_str::<Shared>(&text).expect("read"), remote);
    }

    /// A document from an older build, missing every optional field, must
    /// still open — the alternative is a sync that stops working on upgrade.
    #[test]
    fn a_document_with_only_its_header_still_reads() {
        let bare = r#"{"version":1,"writtenBy":"x","updated":5}"#;

        let parsed: Shared = serde_json::from_str(bare).expect("read");
        assert!(parsed.playlists.is_empty());
        assert!(parsed.bpm_overrides.is_empty());
    }

    // --- Transfer ----------------------------------------------------------

    #[test]
    fn a_file_is_asked_for_a_chunk_at_a_time() {
        let ranges = chunks(CHUNK * 2 + 512);

        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0], (0, CHUNK));
        assert_eq!(ranges[2], (CHUNK * 2, 512), "the last one is the remainder");
    }

    /// Resume is free because a partial file's length *is* the offset to
    /// continue from — there is no separate progress record to disagree with
    /// the file on disk.
    #[test]
    fn an_interrupted_transfer_continues_from_what_is_on_disk() {
        assert_eq!(next_chunk(CHUNK, CHUNK * 3), Some((CHUNK, CHUNK)));
        assert_eq!(next_chunk(0, 100), Some((0, 100)));
    }

    #[test]
    fn a_complete_file_asks_for_nothing_more() {
        assert_eq!(next_chunk(500, 500), None);
        assert_eq!(next_chunk(0, 0), None);
        // Longer than expected: still finished, rather than a negative length.
        assert_eq!(next_chunk(900, 500), None);
    }
}
