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

/// Milliseconds since the Unix epoch, passed in by the shell.
pub type Millis = u64;

/// What kind of machine a peer is, for the dashboard to draw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    /// Host and port, as the shell resolved it from the datagram's source.
    pub address: String,
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
    WrongPin { attempts_left: u32 },
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedDevice {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: DeviceKind,
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
    pub fn add(&mut self, id: impl Into<String>, name: impl Into<String>, kind: DeviceKind, now: Millis) {
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

    let their_lists: HashMap<&str, &PlaylistRecord> =
        remote.playlists.iter().map(|p| (p.id.as_str(), p)).collect();
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

        assert_eq!(registry.get("device-a").unwrap().address, "192.168.1.55:7677");
    }

    // --- Pairing -----------------------------------------------------------

    #[test]
    fn the_right_pin_from_the_right_peer_pairs() {
        let mut pairing = Pairing::begin("482913", "device-b", 0);

        assert_eq!(pairing.offer("device-b", "482913", 500), PairOutcome::Paired);
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
        assert!(reconcile(&local, &tie).replace.is_empty(), "a tie stays put");

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
