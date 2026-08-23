//! Talking to another copy of Vapor on the same Wi-Fi — SYNC-001 to SYNC-004.
//!
//! `vapor_library::sync` holds every decision this makes; this holds the
//! sockets, the clock and the files. The split is the usual one, and here it
//! earns itself twice over: pairing and reconciliation are the two things that
//! are impossible to test with one machine and easy to test as functions.
//!
//! ## The shape
//!
//! * A **beacon** thread shouts a UDP advert every few seconds and listens for
//!   everyone else's, keeping a registry of who is about.
//! * A **server** thread answers TCP requests: pair, manifest, fetch.
//! * The **client** half runs inside a command, because a sync is something a
//!   person started and can watch.
//!
//! ## What an unpaired peer can do
//!
//! Ask to pair. That is the entire list. Every other request is refused before
//! it is parsed further, by one check in one place — [`Session::authorise`] —
//! rather than by each handler remembering, because a permission check that
//! has to be repeated at every call site is one that eventually is not.
//!
//! ## What is served
//!
//! Only an href that is in this device's library index, and only the bytes of
//! the cached file behind it. The path is never taken from the request: it is
//! looked up. A request naming `../../etc/passwd` does not fail a check — it
//! fails to be in the library, which is the same refusal every unknown track
//! gets and needs no separate rule to stay correct.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use vapor_library::sync::{self, Advert, DeviceKind, Manifest, PairOutcome, PeerRegistry, Trust};

/// The UDP port adverts go out on.
pub const DISCOVERY_PORT: u16 = 7676;
/// The TCP port the sync server listens on.
pub const SYNC_PORT: u16 = 7677;

/// How often a device says it is here.
const BEACON_INTERVAL: Duration = Duration::from_secs(5);

/// How long a sync thread may take to notice it has been asked to stop.
///
/// Both loops block on a socket, so this is the socket timeout *and* the slice
/// a sleep is broken into — it bounds how long turning the switch off blocks
/// the caller. Short enough not to be felt, long enough that an idle machine is
/// not waking four threads a second to learn nothing has changed.
const STOP_GRANULARITY: Duration = Duration::from_millis(250);

/// Refusal to wait forever on a peer that opened a connection and went quiet.
const IO_TIMEOUT: Duration = Duration::from_secs(20);

/// The largest request line accepted.
///
/// A request is a line of JSON naming a track. Without a bound, a peer can
/// hold the connection open and stream until the process runs out of memory —
/// and this listens on a network, so "a peer" includes anything on the café
/// Wi-Fi.
const MAX_REQUEST: u64 = 64 * 1024;

/// The largest reply line accepted.
///
/// Much larger than [`MAX_REQUEST`], and the asymmetry is the point. A request
/// names one track and is always short, so a small bound on what the *server*
/// will buffer is a defence worth having against anything on the network. A
/// reply can be a whole manifest — one `TrackRecord` per track the peer knows,
/// about 208 bytes each once a digest is present — and that is answered by a
/// device this one has deliberately paired with.
///
/// Reading the reply with the request's bound is what broke sync on any real
/// library: 64 KiB holds about 315 records, so at 563 tracks `read_line`
/// stopped mid-document and serde's failure surfaced as "unreadable reply".
/// Pairing was unaffected, its reply being two short strings, so the app found
/// the other device and then refused to sync with it.
///
/// 32 MiB is roughly 150,000 tracks. Still bounded, because a paired device can
/// also be a broken one.
const MAX_REPLY: u64 = 32 * 1024 * 1024;

/// The largest `Reply::Bytes` body accepted, and the largest length that will
/// be believed enough to reserve for.
///
/// [`MAX_REPLY`] bounds the JSON *line*; the body arrives after it and was
/// bounded by nothing. `len` is a `u64` chosen by the peer and it went straight
/// into `Vec::reserve`, which gives the sender a choice of two failures:
/// anything above `isize::MAX` panics with "capacity overflow" (measured), and
/// anything below it that is still large — a terabyte, say — is a real
/// allocation request that ends in the process being killed. Neither needs
/// pairing. Answering is enough, and the person only has to have pressed Pair
/// on something.
///
/// Eight times `sync::CHUNK`, which is what a fetch actually asks for. Honest
/// replies are one chunk, so this is only generous enough that raising the
/// chunk size does not silently start refusing transfers.
const MAX_BODY: u64 = 8 * 1024 * 1024;

/// Milliseconds since the epoch. The one place the wall clock is read.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A six-digit pairing code.
///
/// Generated here rather than in the core, which owns no randomness. Seeded
/// from the OS: a PIN from a time-seeded PRNG is guessable by anyone who knows
/// roughly when the button was pressed, which on a shared network is everyone.
pub fn new_pin() -> String {
    let mut bytes = [0u8; 4];
    if !os_random(&mut bytes) {
        // Loud, and second best. The fallback exists so that a platform
        // without OS randomness cannot make the app unusable; a *silently*
        // weak PIN is worse than no pairing at all, which is why this prints.
        //
        // What survives it: the PIN is one of a million, good for two minutes,
        // and dies after three wrong guesses. What does not: see
        // [`Handshake::begin`], which refuses rather than falling back.
        eprintln!("sync: no OS randomness available; this pairing code is weak");
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        bytes = nanos.to_le_bytes();
    }
    format!("{:06}", u32::from_le_bytes(bytes) % 1_000_000)
}

/// Random bytes from the OS. `false` when there are none to be had.
///
/// The `getrandom` crate rather than reading `/dev/urandom`, which is what
/// this used to do and which does not exist on Windows — so on that target
/// every PIN took the fallback above, and the "no OS randomness" line that was
/// meant to be unreachable was the normal path. The crate is the platform call
/// on each target: `getrandom` on Linux, `getentropy` on macOS and iOS,
/// `ProcessPrng` on Windows.
fn os_random(out: &mut [u8]) -> bool {
    getrandom::fill(out).is_ok()
}

// ---------------------------------------------------------------------------
// AUD-7 — proving a reply came from the device that was paired with
// ---------------------------------------------------------------------------
//
// The hole this closes. A `Reply::Bytes` used to carry the audio and the
// SHA-256 of the audio in one plaintext message. Anyone on the path rewrote
// both together and the digest check then confirmed, precisely, whatever they
// had substituted — so the integrity check was not an integrity check against
// an active attacker, and the bytes it waved through went into the cache and
// on to the decoder.
//
// The fix is a MAC, not a cipher. TD-56 decided that the LAN hop stays in
// clear and that decision stands: this is about *authenticity*, and after it
// the wire is exactly as readable as it was. What changes is that a reply
// nobody holding the pairing key could have produced is thrown away.
//
// Where the key comes from. Pairing runs an ephemeral X25519 exchange, and
// HKDF-SHA256 turns the shared point into the 32 bytes both sides keep. The
// PIN is not the key and could not be: six digits is twenty bits, and it is
// sent in clear in the pair request, so anyone who could read the transfer
// could read the PIN. It keeps the job it already had — deciding *which*
// device gets to finish the exchange, three guesses, two minutes.
//
// What it does not close, said plainly. Someone who is actively between the
// two devices *during the pairing itself* can run the exchange twice, once
// with each side, and hold both keys. Ephemeral X25519 buys the case below it
// — a passive listener recording the subnet while the pairing happens learns
// the PIN and the two public keys and still cannot derive the key — but not
// this one. Closing it needs the PIN to authenticate the exchange rather than
// the device, which is a PAKE and a bigger job than this. Pairing is a few
// seconds the owner chose, with both devices in hand; transfers are unattended
// and repeated forever, and moving the window from the second to the first is
// the point of the change.

use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};

type HmacSha256 = Hmac<Sha256>;

/// Salt for the pairing HKDF, and the version of this exchange.
///
/// Fixed rather than random, and deliberately: the freshness in this
/// construction is the ephemeral key pair, which is 32 OS-random bytes per
/// pairing and never reused. A random salt would have to travel on the wire to
/// be agreed on and would add nothing the ephemeral keys have not already
/// given. The string is here for domain separation — so that a key derived for
/// pairing can never coincide with one derived for anything else this app
/// grows later — and carries `v1` so a future change to the exchange is a
/// different key rather than a silently different meaning for the same one.
const PAIR_SALT: &[u8] = b"vapor-sync/pair/v1";

/// Domain separator for the transfer MAC.
const TRANSFER_LABEL: &[u8] = b"vapor-sync/bytes/v1";

fn encode(bytes: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
}

fn decode32(text: &str) -> Option<[u8; 32]> {
    let bytes =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, text.as_bytes()).ok()?;
    bytes.try_into().ok()
}

/// One side of the pairing key exchange.
///
/// Held across the two halves of a pairing: the public key goes out, the
/// peer's comes back, and [`Handshake::finish`] turns the pair into the secret
/// that every later reply is signed with.
pub struct Handshake {
    secret: StaticSecret,
}

impl Handshake {
    /// A fresh key pair, or `None` when the OS has no randomness.
    ///
    /// `None` refuses the pairing. It does **not** fall back the way
    /// [`new_pin`] does, and the difference is the lifetime of what is being
    /// made: a weak PIN is wrong for two minutes and then gone, while a key
    /// derived from a nanosecond counter is guessable for as long as the two
    /// devices stay paired — which is forever, and silently. The policy this
    /// follows is the one already written above `new_pin`: never silently
    /// weak. Here the only way to keep that promise is to stop.
    pub fn begin() -> Option<Handshake> {
        let mut seed = [0u8; 32];
        if !os_random(&mut seed) {
            eprintln!("sync: no OS randomness available; refusing to pair rather than derive a guessable key");
            return None;
        }
        Some(Handshake {
            secret: StaticSecret::from(seed),
        })
    }

    /// What the other device needs, base64.
    pub fn public_key(&self) -> String {
        encode(PublicKey::from(&self.secret).as_bytes())
    }

    /// The shared key, from the peer's public half.
    ///
    /// The two device ids go into the derivation sorted, so both sides compute
    /// the same value without either having to know which of them was the one
    /// that asked. `None` when the peer sent something that is not a public
    /// key, or sent one that forces a known result.
    pub fn finish(&self, their_public: &str, my_id: &str, their_id: &str) -> Option<[u8; 32]> {
        let theirs = PublicKey::from(decode32(their_public)?);
        let shared = self.secret.diffie_hellman(&theirs);
        // A low-order point makes the shared secret all zeroes whatever this
        // side contributed — so a peer that sends one has chosen the "shared"
        // key alone, and every MAC under it is forgeable by anyone who knows
        // the trick. Curve25519 does not fail such an exchange, it just
        // produces that value, so refusing it is this side's job.
        if !shared.was_contributory() {
            return None;
        }

        let mut ids = [my_id, their_id];
        ids.sort_unstable();
        let mut key = [0u8; 32];
        Hkdf::<Sha256>::new(Some(PAIR_SALT), shared.as_bytes())
            .expand_multi_info(&[ids[0].as_bytes(), b"\x00", ids[1].as_bytes()], &mut key)
            .ok()?;
        Some(key)
    }
}

/// Everything a `Reply::Bytes` MAC commits to.
///
/// Each field length-prefixed rather than run together. An href may hold any
/// character a filename may hold, separators included, so a MAC over the
/// concatenation is one that two different (href, digest) pairs can share —
/// and a MAC two messages can share is a MAC that moves a genuine chunk of one
/// track onto another.
///
/// `href` and `offset` are in here because without them a signed chunk is
/// portable: the peer's own bytes for track A, replayed as the answer to a
/// request for track B, would verify. With them a reply is only ever valid as
/// the answer to the exact question that was asked.
fn transfer_transcript(
    href: &str,
    offset: u64,
    len: u64,
    total: u64,
    digest: &str,
    body: &[u8],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + href.len() + digest.len() + 64);
    let mut field = |bytes: &[u8]| {
        out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        out.extend_from_slice(bytes);
    };
    field(TRANSFER_LABEL);
    field(href.as_bytes());
    field(&offset.to_le_bytes());
    field(&len.to_le_bytes());
    field(&total.to_le_bytes());
    field(digest.as_bytes());
    field(body);
    out
}

/// Sign a chunk with the key the pairing derived.
fn transfer_mac(
    key: &[u8; 32],
    href: &str,
    offset: u64,
    len: u64,
    total: u64,
    digest: &str,
    body: &[u8],
) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC takes a key of any length");
    mac.update(&transfer_transcript(href, offset, len, total, digest, body));
    encode(&mac.finalize().into_bytes())
}

// ---------------------------------------------------------------------------
// The protocol
// ---------------------------------------------------------------------------

/// One request, as a line of JSON.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Request {
    /// The only thing an unpaired device may send.
    Pair {
        device_id: String,
        name: String,
        #[serde(default)]
        device_kind: DeviceKind,
        pin: String,
        /// This device's half of the key exchange, base64 (AUD-7).
        ///
        /// `default` so a request from a build older than AUD-7 parses rather
        /// than being reported as "not a request this version understands" —
        /// it arrives empty, and an empty half cannot complete an exchange, so
        /// the pairing is refused with a reason that says which version is the
        /// old one.
        #[serde(default)]
        public_key: String,
    },
    /// What the other device knows.
    Manifest { device_id: String },
    /// Bytes of one track, from `offset`, at most `len`.
    Fetch {
        device_id: String,
        href: String,
        offset: u64,
        len: u64,
    },
}

impl Request {
    /// Who is asking. Every request carries it, because every request is
    /// authorised.
    pub fn device_id(&self) -> &str {
        match self {
            Request::Pair { device_id, .. }
            | Request::Manifest { device_id, .. }
            | Request::Fetch { device_id, .. } => device_id,
        }
    }

    /// Whether this may be sent by a device that has not paired.
    pub fn is_pairing(&self) -> bool {
        matches!(self, Request::Pair { .. })
    }
}

/// The header of a reply. A `Fetch` follows it with `len` raw bytes.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Reply {
    Paired {
        device_id: String,
        name: String,
        /// The answering device's half of the key exchange, base64 (AUD-7).
        #[serde(default)]
        public_key: String,
    },
    Refused {
        reason: String,
    },
    Manifest(Box<Manifest>),
    /// `len` bytes follow on the wire, and `digest` covers the whole file, not
    /// this chunk — so a resumed transfer is verified once, at the end.
    ///
    /// `digest` on its own proves nothing about who sent the bytes: it travels
    /// in the same plaintext message they do, and anyone on the path rewrites
    /// the pair together. `mac` is what makes it mean something — HMAC-SHA256
    /// over the chunk and the request it answers, under the key the pairing
    /// derived. See [`accept_bytes`], which is the only way this variant is
    /// read.
    Bytes {
        len: u64,
        total: u64,
        digest: String,
        #[serde(default)]
        mac: String,
    },
    Error {
        reason: String,
    },
}

/// Decide whether a request may proceed.
///
/// Free function, and the only place the rule lives. `None` means yes.
pub fn authorise(trust: &Trust, request: &Request) -> Option<Reply> {
    if request.is_pairing() || trust.allows(request.device_id()) {
        return None;
    }
    // Deliberately says nothing about whether the device is known, what the
    // library holds, or which of the two it got wrong.
    Some(Reply::Refused {
        reason: "not paired with this device".to_string(),
    })
}

// ---------------------------------------------------------------------------
// What the server needs from the app
// ---------------------------------------------------------------------------

/// The shell's side of a sync, as the server sees it.
///
/// A trait so the socket layer is not welded to `AppState`: the handler can be
/// driven from a test with a fixture that answers three questions, and the
/// alternative is a server that can only be exercised by starting the app.
pub trait Library: Send + Sync + 'static {
    fn trust(&self) -> Trust;
    /// Judge a pairing request, and — when it pairs — hand back this device's
    /// half of the key exchange.
    ///
    /// The two travel together because they are decided together: the key is
    /// only derived on the branch that pairs, and returning it separately
    /// would allow an implementation to record a device without one, which is
    /// the state [`Trust::allows`] exists to refuse.
    fn pair(&self, request: PairRequest<'_>) -> (PairOutcome, String);
    fn manifest(&self) -> Manifest;
    /// The bytes of a track this device holds, or `None` when it holds none.
    ///
    /// Takes an href and returns bytes — never a path — so the caller cannot
    /// ask for a file by location.
    fn read_track(&self, href: &str) -> Option<Vec<u8>>;
    /// This device's own name and id, for the advert.
    fn identity(&self) -> (String, String, DeviceKind);
}

/// A pairing request, as the app sees it.
///
/// A struct rather than five positional arguments: `device_id`, `name`, `pin`
/// and `public_key` are all `&str`, and four adjacent strings in a signature
/// is a call site that compiles after two of them are swapped.
pub struct PairRequest<'a> {
    pub device_id: &'a str,
    pub name: &'a str,
    pub kind: DeviceKind,
    pub pin: &'a str,
    /// The asking device's half of the key exchange, base64.
    pub public_key: &'a str,
}

/// Answer one request.
///
/// Separated from the socket so every branch — including the refusals, which
/// are the ones that matter — is reachable without a network.
pub fn handle(library: &dyn Library, request: &Request) -> (Reply, Option<Vec<u8>>) {
    let trust = library.trust();
    if let Some(refusal) = authorise(&trust, request) {
        return (refusal, None);
    }

    match request {
        Request::Pair {
            device_id,
            name,
            device_kind,
            pin,
            public_key,
        } => {
            let (outcome, ours) = library.pair(PairRequest {
                device_id,
                name,
                kind: *device_kind,
                pin,
                public_key,
            });
            match outcome {
                // A pairing that produced no key of ours is one the peer could
                // not authenticate afterwards, so it is not a pairing. The
                // implementation refuses before it reaches here; this is the
                // check that makes that impossible to get wrong later.
                PairOutcome::Paired if !ours.is_empty() => {
                    let (id, name, _) = library.identity();
                    (
                        Reply::Paired {
                            device_id: id,
                            name,
                            public_key: ours,
                        },
                        None,
                    )
                }
                PairOutcome::Paired => (
                    Reply::Refused {
                        reason: "that device could not complete the pairing exchange".to_string(),
                    },
                    None,
                ),
                PairOutcome::WrongPin { attempts_left } => (
                    Reply::Refused {
                        reason: format!("wrong code — {attempts_left} left"),
                    },
                    None,
                ),
                PairOutcome::Refused => (
                    Reply::Refused {
                        reason: "pairing is not open on that device".to_string(),
                    },
                    None,
                ),
            }
        }

        Request::Manifest { .. } => (Reply::Manifest(Box::new(library.manifest())), None),

        Request::Fetch {
            device_id,
            href,
            offset,
            len,
        } => {
            // `authorise` already required a key for this device — that is what
            // `Trust::allows` now means. Reading it again here rather than
            // taking it on trust keeps the two from drifting apart: if the gate
            // is ever loosened, this refuses instead of signing with nothing.
            let Some(key) = trust.key(device_id) else {
                return (
                    Reply::Refused {
                        reason: "not paired with this device".to_string(),
                    },
                    None,
                );
            };
            let Some(bytes) = library.read_track(href) else {
                return (
                    Reply::Error {
                        reason: "not in this library".to_string(),
                    },
                    None,
                );
            };
            let total = bytes.len() as u64;
            let digest = sync::digest(&bytes);

            // A range past the end is answered with nothing rather than
            // refused: it is what a peer asks when it already has the file.
            let start = (*offset).min(total) as usize;
            let end = ((*offset).saturating_add(*len).min(total)) as usize;
            let slice = bytes[start..end].to_vec();
            let len = slice.len() as u64;

            let mac = transfer_mac(&key, href, *offset, len, total, &digest, &slice);
            (
                Reply::Bytes {
                    len,
                    total,
                    digest,
                    mac,
                },
                Some(slice),
            )
        }
    }
}

/// Read a `Reply::Bytes`, or refuse to.
///
/// **The only way that variant is unpacked.** `ask` cannot do this itself — it
/// speaks to an address and has no idea which device is on the other end, and
/// the key is per peer — so this is where the check has to live, and taking
/// `reply` by value is what stops a caller verifying and then reading the
/// unverified copy anyway.
///
/// A MAC that does not match is a reply from something that is not the device
/// this one paired with. It gets the same treatment as a corrupt one: nothing
/// comes back, and the caller has nothing to write.
pub fn accept_bytes(
    key: &[u8; 32],
    href: &str,
    offset: u64,
    reply: Reply,
    body: &[u8],
) -> Result<(u64, u64, String), String> {
    let (len, total, digest, mac) = match reply {
        Reply::Bytes {
            len,
            total,
            digest,
            mac,
        } => (len, total, digest, mac),
        Reply::Error { reason } | Reply::Refused { reason } => return Err(reason),
        _ => return Err("that device answered with something else".to_string()),
    };

    // An absent MAC and a wrong one are the same answer. A peer running a
    // build from before AUD-7 sends none, and it is refused for the same
    // reason an attacker is: what it sent cannot be shown to have come from
    // the device that was paired with.
    let mut check = HmacSha256::new_from_slice(key).expect("HMAC takes a key of any length");
    check.update(&transfer_transcript(
        href, offset, len, total, &digest, body,
    ));
    let offered =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, mac.as_bytes())
            .unwrap_or_default();
    // `verify_slice` compares in constant time and refuses a short tag rather
    // than comparing the prefix.
    if check.verify_slice(&offered).is_err() {
        return Err(
            "that reply was not signed by the device this one paired with, so it was \
             thrown away"
                .to_string(),
        );
    }

    Ok((len, total, digest))
}

// ---------------------------------------------------------------------------
// Sockets
// ---------------------------------------------------------------------------

/// Who is on the network, shared between the beacon thread and the commands.
pub type Peers = Arc<Mutex<PeerRegistry>>;

/// Bytes moved since the process started, for the dashboard's rate.
pub static BYTES_MOVED: AtomicU64 = AtomicU64::new(0);

/// A running sync session — the sockets that are bound and the threads on them.
///
/// Held so that turning sync off actually stops it (TD-58). The switch used to
/// gate only what the threads *did*: adverts stopped being acted on, pairings
/// were forgotten and every command was refused, but the beacon carried on
/// broadcasting this machine's name to the network and the listener kept a port
/// bound until the process exited. A machine that had sync on and then turned
/// it off is exactly the case the switch exists for.
pub struct Session {
    stop: Arc<AtomicBool>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

impl Session {
    /// Stop the session and wait for its threads to end.
    ///
    /// Joining rather than detaching, so that the sockets are unbound by the
    /// time this returns. Toggling sync off and straight back on is a normal
    /// thing to do, and a detached stop would race the new session to the same
    /// two ports and lose — reported as "discovery is unavailable" on a machine
    /// where nothing is wrong. Bounded by [`STOP_GRANULARITY`].
    pub fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        for thread in self.threads {
            let _ = thread.join();
        }
    }
}

/// Start the beacon and the server, returning a handle that can stop them.
///
/// Returns `None` when neither could start — a locked-down network, or another
/// copy of the app already holding the ports. There is nothing to stop in that
/// case, and pretending otherwise would make the switch report a session that
/// does not exist.
pub fn start(
    peers: Peers,
    id: String,
    name: String,
    kind: DeviceKind,
    library: Arc<dyn Library>,
) -> Option<Session> {
    let stop = Arc::new(AtomicBool::new(false));
    let mut threads = spawn_beacon(&stop, peers, id, name, kind);
    threads.extend(spawn_server(&stop, library));
    if threads.is_empty() {
        return None;
    }
    Some(Session { stop, threads })
}

/// Sleep in slices, giving up early once `stop` is set.
fn nap(stop: &AtomicBool, total: Duration) {
    let mut left = total;
    while !left.is_zero() && !stop.load(Ordering::Relaxed) {
        let slice = left.min(STOP_GRANULARITY);
        std::thread::sleep(slice);
        left -= slice;
    }
}

/// Shout every few seconds, and listen for everyone else.
///
/// Two threads on one socket, because a broadcaster that cannot hear replies
/// is only half a discovery protocol and binding twice to the same port is not
/// portable.
fn spawn_beacon(
    stop: &Arc<AtomicBool>,
    peers: Peers,
    id: String,
    name: String,
    kind: DeviceKind,
) -> Vec<std::thread::JoinHandle<()>> {
    let socket = match UdpSocket::bind((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT)) {
        Ok(s) => s,
        Err(e) => {
            // Another copy of the app, or a locked-down network. Discovery is
            // a convenience: without it a person can still sync, they just
            // have to be told about it.
            eprintln!("sync: discovery is unavailable ({e})");
            return Vec::new();
        }
    };
    if let Err(e) = socket.set_broadcast(true) {
        eprintln!("sync: cannot broadcast on this network ({e})");
        return Vec::new();
    }
    // Without this the listener blocks in `recv_from` forever and never reaches
    // the stop check. The timeout is what makes the loop interruptible; the
    // packets it wakes up for are usually nothing, which is why a timeout is
    // not an error below.
    if let Err(e) = socket.set_read_timeout(Some(STOP_GRANULARITY)) {
        eprintln!("sync: cannot set a discovery timeout ({e})");
        return Vec::new();
    }

    let advert = Advert {
        id: id.clone(),
        name,
        kind,
        port: SYNC_PORT,
        protocol: sync::PROTOCOL,
    };

    let shouting = match socket.try_clone() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("sync: cannot share the discovery socket ({e})");
            return Vec::new();
        }
    };
    let datagram = advert.encode();
    let mut threads = Vec::new();

    let shout_stop = Arc::clone(stop);
    if let Ok(handle) = std::thread::Builder::new()
        .name("vapor-sync-beacon".into())
        .spawn(move || {
            while !shout_stop.load(Ordering::Relaxed) {
                let _ =
                    shouting.send_to(datagram.as_bytes(), (Ipv4Addr::BROADCAST, DISCOVERY_PORT));
                nap(&shout_stop, BEACON_INTERVAL);
            }
        })
    {
        threads.push(handle);
    }

    let listen_stop = Arc::clone(stop);
    if let Ok(handle) = std::thread::Builder::new()
        .name("vapor-sync-listener".into())
        .spawn(move || {
            let mut buffer = [0u8; 2048];
            while !listen_stop.load(Ordering::Relaxed) {
                // A timeout lands here as an error, and so does a real failure.
                // Both mean "nothing to do", and the loop condition is what
                // decides whether to carry on.
                let Ok((read, from)) = socket.recv_from(&mut buffer) else {
                    continue;
                };
                let Ok(text) = std::str::from_utf8(&buffer[..read]) else {
                    continue;
                };
                let Some(heard) = Advert::decode(text) else {
                    continue;
                };
                // Our own broadcast comes back to us.
                if heard.id == id {
                    continue;
                }
                let address = SocketAddr::new(from.ip(), heard.port).to_string();
                if let Ok(mut registry) = peers.lock() {
                    registry.saw(&heard, &address, now());
                }
            }
        })
    {
        threads.push(handle);
    }

    threads
}

/// Serve requests from other devices.
fn spawn_server(
    stop: &Arc<AtomicBool>,
    library: Arc<dyn Library>,
) -> Vec<std::thread::JoinHandle<()>> {
    let listener = match TcpListener::bind((Ipv4Addr::UNSPECIFIED, SYNC_PORT)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("sync: cannot accept connections ({e})");
            return Vec::new();
        }
    };
    // `accept` blocks, and a blocked accept cannot notice the switch. Polling
    // rather than blocking is what makes the server stoppable; the cost is one
    // wake-up per `STOP_GRANULARITY` on a thread that is otherwise idle.
    if let Err(e) = listener.set_nonblocking(true) {
        eprintln!("sync: cannot poll for connections ({e})");
        return Vec::new();
    }

    let accept_stop = Arc::clone(stop);
    let Ok(handle) = std::thread::Builder::new()
        .name("vapor-sync-server".into())
        .spawn(move || {
            while !accept_stop.load(Ordering::Relaxed) {
                let stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        nap(&accept_stop, STOP_GRANULARITY);
                        continue;
                    }
                    Err(_) => continue,
                };
                // The listener is non-blocking and the accepted socket inherits
                // that on some platforms, which would turn every read in
                // `serve` into an immediate WouldBlock. `serve` sets its own
                // timeouts and expects to block within them.
                if stream.set_nonblocking(false).is_err() {
                    continue;
                }

                let library = Arc::clone(&library);
                // A connection per thread. The expected concurrency is "the
                // other device", and a thread pool for that would be more
                // machinery than the problem.
                //
                // Detached rather than joined on stop: these are bounded by
                // `IO_TIMEOUT` already, and a stop that waited for them would
                // hold the switch for up to twenty seconds. An in-flight
                // request is refused rather than served — turning sync off
                // clears the trust, and `authorise` answers from that.
                let _ = std::thread::Builder::new()
                    .name("vapor-sync-conn".into())
                    .spawn(move || {
                        if let Err(e) = serve(library.as_ref(), stream) {
                            eprintln!("sync: connection ended ({e})");
                        }
                    });
            }
        })
    else {
        return Vec::new();
    };

    vec![handle]
}

fn serve(library: &dyn Library, stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;

    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    let mut line = String::new();
    // Bounded, so a peer cannot stream until the process dies.
    let read = reader.by_ref().take(MAX_REQUEST).read_line(&mut line)?;
    if read == 0 {
        return Ok(());
    }

    let (reply, body) = match serde_json::from_str::<Request>(line.trim()) {
        Ok(request) => handle(library, &request),
        Err(_) => (
            Reply::Error {
                reason: "not a request this version understands".to_string(),
            },
            None,
        ),
    };

    writeln!(
        writer,
        "{}",
        serde_json::to_string(&reply).unwrap_or_default()
    )?;
    if let Some(bytes) = body {
        writer.write_all(&bytes)?;
        BYTES_MOVED.fetch_add(bytes.len() as u64, Ordering::Relaxed);
    }
    writer.flush()
}

/// Send one request to a peer and read its reply.
///
/// Returns the header and however many bytes the header said would follow.
pub fn ask(address: &str, request: &Request) -> Result<(Reply, Vec<u8>), String> {
    let target: SocketAddr = address
        .parse()
        .map_err(|_| format!("{address} is not an address"))?;
    let stream = TcpStream::connect_timeout(&target, IO_TIMEOUT)
        .map_err(|e| format!("could not reach {address}: {e}"))?;
    stream.set_read_timeout(Some(IO_TIMEOUT)).ok();
    stream.set_write_timeout(Some(IO_TIMEOUT)).ok();

    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    writeln!(
        writer,
        "{}",
        serde_json::to_string(request).map_err(|e| e.to_string())?
    )
    .map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let read = reader
        .by_ref()
        .take(MAX_REPLY)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;

    // Three different failures, told apart before parsing rather than after.
    //
    // All three used to arrive as "unreadable reply", which named the symptom
    // and hid every cause. A reply is one line of JSON, so an ending without a
    // newline is a reply that did not finish — either because it outgrew the
    // bound above or because the peer stopped talking — and neither is serde's
    // to explain.
    if read == 0 {
        return Err("the peer closed the connection without replying".to_string());
    }
    if !line.ends_with('\n') {
        return Err(if read as u64 >= MAX_REPLY {
            "the reply was too large to read".to_string()
        } else {
            "the reply was cut short".to_string()
        });
    }

    let reply: Reply = serde_json::from_str(line.trim()).map_err(|e| {
        // The parse error names the offset and what it expected, which is the
        // difference between "a peer running another version" and a bug here.
        format!("could not understand the reply: {e}")
    })?;

    let mut body = Vec::new();
    if let Reply::Bytes { len, .. } = &reply {
        // Checked before it is believed. See [`MAX_BODY`] — this number comes
        // from the network and the next line hands it to the allocator.
        if *len > MAX_BODY {
            return Err(format!(
                "that device offered a {len}-byte chunk, which is more than any \
                 honest reply sends"
            ));
        }
        body.reserve(*len as usize);
        reader
            .take(*len)
            .read_to_end(&mut body)
            .map_err(|e| e.to_string())?;
        // A short read is a truncated transfer. Accepting it would write a
        // partial chunk and then verify the whole file against a digest it
        // cannot match, which reports the wrong failure.
        if body.len() as u64 != *len {
            return Err("the connection ended mid-track".to_string());
        }
        BYTES_MOVED.fetch_add(body.len() as u64, Ordering::Relaxed);
    }

    Ok((reply, body))
}

/// Whether an address is on a private network.
///
/// Sync is a local-network feature. Refusing anything routable is what stops a
/// crafted advert pointing this device at a host on the internet and having it
/// open a connection there.
pub fn is_local(address: &str) -> bool {
    let Ok(parsed) = address.parse::<SocketAddr>() else {
        return false;
    };
    match parsed.ip() {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
        // fc00::/7 unique-local, or loopback.
        IpAddr::V6(v6) => v6.is_loopback() || (v6.octets()[0] & 0xfe) == 0xfc,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;
    use vapor_library::sync::Pairing;

    /// A library that answers the three questions, so every branch of the
    /// handler is reachable without a socket.
    struct Fixture {
        trust: StdMutex<Trust>,
        pairing: StdMutex<Option<Pairing>>,
        tracks: Vec<(String, Vec<u8>)>,
        manifest: StdMutex<Manifest>,
    }

    /// The key a `trusting` fixture shares with its peer.
    ///
    /// A constant rather than a real exchange: every test below that is about
    /// the *transfer* wants a key both halves already agree on, and running a
    /// handshake to get one would put the thing being tested downstream of a
    /// second thing that can fail. The exchange has tests of its own.
    const SHARED: [u8; 32] = [0x5a; 32];

    impl Fixture {
        fn new() -> Self {
            Fixture {
                trust: StdMutex::new(Trust::new()),
                pairing: StdMutex::new(None),
                tracks: vec![("/music/a.m4a".to_string(), b"the audio".to_vec())],
                manifest: StdMutex::new(Manifest {
                    device_id: "us".into(),
                    ..Default::default()
                }),
            }
        }

        fn trusting(id: &str) -> Self {
            let f = Fixture::new();
            f.trust
                .lock()
                .unwrap()
                .add(id, "Peer", DeviceKind::Phone, SHARED, 0);
            f
        }

        fn opening(pin: &str, peer: &str) -> Self {
            let f = Fixture::new();
            *f.pairing.lock().unwrap() = Some(Pairing::begin(pin, peer, 0));
            f
        }
    }

    impl Library for Fixture {
        fn trust(&self) -> Trust {
            self.trust.lock().unwrap().clone()
        }
        fn pair(&self, request: PairRequest<'_>) -> (PairOutcome, String) {
            let mut pairing = self.pairing.lock().unwrap();
            let Some(open) = pairing.as_mut() else {
                return (PairOutcome::Refused, String::new());
            };
            let outcome = open.offer(request.device_id, request.pin, 1);
            if outcome != PairOutcome::Paired {
                return (outcome, String::new());
            }
            // The same order the app does it in: no key, no trust.
            let Some(handshake) = Handshake::begin() else {
                return (PairOutcome::Refused, String::new());
            };
            let Some(key) = handshake.finish(request.public_key, "us", request.device_id) else {
                return (PairOutcome::Refused, String::new());
            };
            self.trust
                .lock()
                .unwrap()
                .add(request.device_id, request.name, request.kind, key, 1);
            (PairOutcome::Paired, handshake.public_key())
        }
        fn manifest(&self) -> Manifest {
            self.manifest.lock().unwrap().clone()
        }
        fn read_track(&self, href: &str) -> Option<Vec<u8>> {
            self.tracks
                .iter()
                .find(|(h, _)| h == href)
                .map(|(_, b)| b.clone())
        }
        fn identity(&self) -> (String, String, DeviceKind) {
            ("us".into(), "This Mac".into(), DeviceKind::Desktop)
        }
    }

    fn fetch(href: &str) -> Request {
        Request::Fetch {
            device_id: "them".into(),
            href: href.into(),
            offset: 0,
            len: 1024,
        }
    }

    /// A pair request carrying a real public key, from a handshake the caller
    /// keeps so it can finish the exchange with whatever comes back.
    fn pair_request(peer: &str, pin: &str, handshake: &Handshake) -> Request {
        Request::Pair {
            device_id: peer.into(),
            name: "Phone".into(),
            device_kind: DeviceKind::Phone,
            pin: pin.into(),
            public_key: handshake.public_key(),
        }
    }

    /// The gate. An unpaired device may ask to pair and nothing else.
    #[test]
    fn an_unpaired_device_cannot_read_the_library() {
        let library = Fixture::new();

        let (reply, body) = handle(&library, &fetch("/music/a.m4a"));

        assert!(matches!(reply, Reply::Refused { .. }));
        assert!(body.is_none());
    }

    #[test]
    fn an_unpaired_device_cannot_read_the_manifest_either() {
        let library = Fixture::new();

        let (reply, _) = handle(
            &library,
            &Request::Manifest {
                device_id: "them".into(),
            },
        );

        assert!(matches!(reply, Reply::Refused { .. }));
    }

    /// The refusal says nothing about whether the device is known or whether
    /// the track exists — an unpaired peer should not be able to enumerate a
    /// library by watching which refusals differ.
    #[test]
    fn the_refusal_does_not_say_which_part_was_wrong() {
        let library = Fixture::new();

        let (known, _) = handle(&library, &fetch("/music/a.m4a"));
        let (unknown, _) = handle(&library, &fetch("/not/here.m4a"));

        let text = |r: Reply| match r {
            Reply::Refused { reason } => reason,
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert_eq!(text(known), text(unknown));
    }

    #[test]
    fn a_paired_device_is_served() {
        let library = Fixture::trusting("them");

        let (reply, body) = handle(&library, &fetch("/music/a.m4a"));

        match reply {
            Reply::Bytes {
                len, total, digest, ..
            } => {
                assert_eq!(len, 9);
                assert_eq!(total, 9);
                assert_eq!(digest, sync::digest(b"the audio"));
            }
            other => panic!("expected bytes, got {other:?}"),
        }
        assert_eq!(body.as_deref(), Some(&b"the audio"[..]));
    }

    /// The path is never taken from the request — it is looked up. A traversal
    /// attempt is refused as "not in this library", which is the same answer
    /// every unknown track gets and needs no separate rule to stay correct.
    #[test]
    fn a_path_that_is_not_a_track_in_the_library_is_simply_not_there() {
        let library = Fixture::trusting("them");

        for attempt in [
            "../../../../etc/passwd",
            "/etc/passwd",
            "/music/../../../secrets",
        ] {
            let (reply, body) = handle(&library, &fetch(attempt));
            assert!(
                matches!(reply, Reply::Error { .. }),
                "{attempt} was not refused"
            );
            assert!(body.is_none());
        }
    }

    /// What a peer asks when it already has the whole file.
    #[test]
    fn a_range_past_the_end_returns_nothing_rather_than_an_error() {
        let library = Fixture::trusting("them");

        let (reply, body) = handle(
            &library,
            &Request::Fetch {
                device_id: "them".into(),
                href: "/music/a.m4a".into(),
                offset: 9,
                len: 1024,
            },
        );

        assert!(matches!(reply, Reply::Bytes { len: 0, .. }));
        assert_eq!(body.as_deref(), Some(&b""[..]));
    }

    #[test]
    fn a_partial_range_is_served_from_the_offset() {
        let library = Fixture::trusting("them");

        let (_, body) = handle(
            &library,
            &Request::Fetch {
                device_id: "them".into(),
                href: "/music/a.m4a".into(),
                offset: 4,
                len: 3,
            },
        );

        assert_eq!(body.as_deref(), Some(&b"aud"[..]));
    }

    #[test]
    fn the_right_pin_pairs_and_the_wrong_one_does_not() {
        let library = Fixture::opening("482913", "them");
        let handshake = Handshake::begin().expect("the test machine has randomness");

        let wrong = pair_request("them", "000000", &handshake);
        assert!(matches!(handle(&library, &wrong).0, Reply::Refused { .. }));

        let right = pair_request("them", "482913", &handshake);
        assert!(matches!(handle(&library, &right).0, Reply::Paired { .. }));
        // And now it may read.
        assert!(matches!(
            handle(&library, &fetch("/music/a.m4a")).0,
            Reply::Bytes { .. }
        ));
    }

    /// Pairing with a device that is not offering must not succeed just
    /// because the asker sent a plausible code.
    #[test]
    fn pairing_with_a_device_that_is_not_asking_fails() {
        let library = Fixture::new();

        let handshake = Handshake::begin().expect("the test machine has randomness");
        let (reply, _) = handle(&library, &pair_request("them", "482913", &handshake));

        assert!(matches!(reply, Reply::Refused { .. }));
    }

    /// A crafted advert must not be able to point this device at a host on the
    /// internet and have it open a connection there.
    #[test]
    fn only_a_private_address_counts_as_local() {
        assert!(is_local("192.168.1.20:7677"));
        assert!(is_local("10.0.0.4:7677"));
        assert!(is_local("172.16.5.5:7677"));
        assert!(is_local("127.0.0.1:7677"));

        assert!(!is_local("8.8.8.8:7677"));
        assert!(!is_local("93.184.216.34:443"));
        assert!(!is_local("not an address"));
    }

    #[test]
    fn a_pin_is_six_digits() {
        for _ in 0..50 {
            let pin = new_pin();
            assert_eq!(pin.len(), 6, "{pin}");
            assert!(pin.chars().all(|c| c.is_ascii_digit()), "{pin}");
        }
    }

    /// TD-58: stopping actually stops, and the proof is that it can start again.
    ///
    /// Asserting on the flag would only test that a boolean was set. The thing
    /// that was wrong is that the sockets stayed bound for the life of the
    /// process, and the observable consequence of fixing it is that the same
    /// two ports can be taken a second time — which is also the real sequence,
    /// since a person who turns sync off and back on is not restarting the app
    /// in between.
    ///
    /// If the first start fails there is no network to test on — a locked-down
    /// CI container, or another copy of the app holding the ports — and the
    /// test says so rather than failing for a reason that is not this one.
    #[test]
    fn stopping_releases_the_ports_so_sync_can_start_again() {
        let registry: Peers = Arc::new(Mutex::new(PeerRegistry::new()));
        let begin = || {
            start(
                Arc::clone(&registry),
                "device-a".to_string(),
                "Test".to_string(),
                DeviceKind::Desktop,
                Arc::new(Fixture::new()),
            )
        };

        // Beacon, listener, server. Counting them rather than only asking
        // whether *something* started: `start` succeeds if either half binds,
        // so a leak of one port would otherwise pass this test while leaving
        // the machine announcing itself with the switch off.
        const THREADS: usize = 3;

        let Some(session) = begin() else {
            eprintln!("skipped: the discovery and sync ports could not be bound here");
            return;
        };
        if session.threads.len() != THREADS {
            eprintln!("skipped: only part of the network is available here");
            session.stop();
            return;
        }
        session.stop();

        let again = begin();
        let count = again.as_ref().map(|s| s.threads.len()).unwrap_or(0);
        assert_eq!(
            count, THREADS,
            "a port was still held after stop — a thread outlived it"
        );
        if let Some(session) = again {
            session.stop();
        }
    }

    /// A real library's manifest, over a real socket.
    ///
    /// Every other test in this file calls `handle` directly and gets a `Reply`
    /// value back, which is why this went unseen: the handler was always right.
    /// The failure was on the wire. `ask` read the reply with the bound meant
    /// for *requests* — 64 KiB — and a manifest carries every track the library
    /// knows: about 208 bytes each once a digest is present, so a 563-track
    /// library is roughly 114 KiB. `read_line` stopped at the cap, the JSON was
    /// half a document, and serde's failure was reported as "unreadable reply".
    ///
    /// Pairing kept working throughout, because that reply is two short
    /// strings — which is exactly what "it finds the other device but will not
    /// sync" looks like from the outside.
    #[test]
    fn a_manifest_larger_than_a_request_survives_the_wire() {
        use vapor_library::sync::TrackRecord;

        let fixture = Fixture::trusting("them");
        {
            let mut manifest = fixture.manifest.lock().unwrap();
            manifest.tracks = (0..563)
                .map(|i| TrackRecord {
                    href: format!(
                        "/dav/Koofr/Music/An Artist/An Album With A Long Name/{i:02} A Track.flac"
                    ),
                    size: 41_234_567,
                    digest: "a".repeat(64),
                    updated: 1_755_780_000_000,
                })
                .collect();
        }

        let wire = serde_json::to_string(&Reply::Manifest(Box::new(fixture.manifest())))
            .expect("serialise");
        assert!(
            wire.len() as u64 > MAX_REQUEST,
            "the fixture has to exceed the request bound or it proves nothing: {} bytes",
            wire.len()
        );

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
        let address = listener.local_addr().expect("addr").to_string();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            serve(&fixture, stream).expect("serve");
        });

        let (reply, body) = ask(
            &address,
            &Request::Manifest {
                device_id: "them".into(),
            },
        )
        .expect("the manifest has to arrive whole");
        server.join().expect("server thread");

        assert!(body.is_empty(), "a manifest has no trailing bytes");
        match reply {
            Reply::Manifest(manifest) => {
                assert_eq!(manifest.tracks.len(), 563, "every record has to survive");
            }
            other => panic!("expected a manifest, got {other:?}"),
        }
    }

    /// A reply cut short says it was cut short.
    ///
    /// The old message was "unreadable reply" for both this and genuine
    /// nonsense, so the one failure a size bound can cause was indistinguishable
    /// from the one it cannot. Finding the real cause took arithmetic on the
    /// record size rather than anything the app said.
    #[test]
    fn a_truncated_reply_is_not_reported_as_nonsense() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
        let address = listener.local_addr().expect("addr").to_string();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut sink = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut sink)
                .ok();
            // Valid JSON for as far as it goes, and no newline ever.
            let half = format!("{{\"Manifest\":{{\"tracks\":[{}", "0,".repeat(40 * 1024));
            stream.write_all(half.as_bytes()).ok();
            stream.flush().ok();
            std::thread::sleep(Duration::from_millis(200));
        });

        let err = ask(
            &address,
            &Request::Manifest {
                device_id: "them".into(),
            },
        )
        .expect_err("a reply with no end must not parse");

        assert!(
            err.contains("too large") || err.contains("cut short"),
            "the error has to name the shape of the problem, got {err:?}"
        );
    }

    /// `len` came off the wire and went straight into `Vec::reserve`.
    ///
    /// `u64::MAX` is the cheap end of it — that panics with "capacity
    /// overflow" before any memory is asked for. The expensive end is a length
    /// under `isize::MAX` that is still enormous, which is a genuine allocation
    /// request and takes the process with it. This test uses the cheap end
    /// because it is the one a test can survive asserting on.
    ///
    /// The peer does not need to be paired. Answering is enough.
    #[test]
    fn an_absurd_chunk_length_is_refused_rather_than_reserved() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
        let address = listener.local_addr().expect("addr").to_string();

        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut sink = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut sink)
                .ok();

            // Serialised from the real type: a hand-written line would test the
            // wrong shape the moment the enum's tagging changes.
            let reply = Reply::Bytes {
                len: u64::MAX,
                total: u64::MAX,
                digest: "not reached".into(),
                mac: "not reached either".into(),
            };
            let line = format!("{}\n", serde_json::to_string(&reply).expect("serialise"));
            stream.write_all(line.as_bytes()).ok();
            stream.flush().ok();
            std::thread::sleep(Duration::from_millis(200));
        });

        let err = ask(
            &address,
            &Request::Fetch {
                device_id: "them".into(),
                href: "/music/a.m4a".into(),
                offset: 0,
                len: sync::CHUNK,
            },
        )
        .expect_err("a u64::MAX chunk length must not be believed");

        assert!(
            err.contains("honest"),
            "the error should say the reply was implausible, got {err:?}"
        );
    }

    // --- AUD-7: the reply has to have come from the paired device ----------

    /// The genuine article, straight from the handler, for the tests below to
    /// spoil in one specific way each.
    fn served(href: &str) -> (Reply, Vec<u8>) {
        let library = Fixture::trusting("them");
        let (reply, body) = handle(&library, &fetch(href));
        (reply, body.expect("a served track has a body"))
    }

    #[test]
    fn a_genuine_reply_from_a_paired_device_is_accepted() {
        let (reply, body) = served("/music/a.m4a");

        let (len, total, digest) =
            accept_bytes(&SHARED, "/music/a.m4a", 0, reply, &body).expect("this one is real");

        assert_eq!((len, total), (9, 9));
        assert_eq!(digest, sync::digest(b"the audio"));
    }

    /// **The hole AUD-7 names.** The bytes and the SHA-256 of the bytes used
    /// to travel in one plaintext message, so an attacker on the path rewrote
    /// them together and the digest check confirmed exactly what they had put
    /// there. This is that rewrite, done properly — new audio, new digest, new
    /// length, all consistent — and the only thing it cannot do is produce the
    /// MAC.
    #[test]
    fn a_reply_whose_bytes_were_changed_on_the_way_is_refused() {
        let (reply, _) = served("/music/a.m4a");
        let Reply::Bytes { mac, .. } = reply else {
            panic!("expected bytes");
        };

        let substituted = b"not the audio".to_vec();
        let rewritten = Reply::Bytes {
            len: substituted.len() as u64,
            total: substituted.len() as u64,
            // Recomputed, so the check that used to be the only one passes.
            digest: sync::digest(&substituted),
            mac,
        };

        let err = accept_bytes(&SHARED, "/music/a.m4a", 0, rewritten, &substituted)
            .expect_err("substituted audio must not be accepted");

        assert!(err.contains("not signed"), "got {err:?}");
    }

    /// A peer running a build from before AUD-7 sends no MAC at all. It is
    /// refused for the same reason an attacker is — nothing it sent can be
    /// shown to have come from the device that was paired with — and there is
    /// deliberately no version flag that would let it through.
    #[test]
    fn a_reply_with_a_correct_digest_and_no_mac_is_refused() {
        let unsigned = Reply::Bytes {
            len: 9,
            total: 9,
            digest: sync::digest(b"the audio"),
            mac: String::new(),
        };

        let err = accept_bytes(&SHARED, "/music/a.m4a", 0, unsigned, b"the audio")
            .expect_err("an unsigned reply must not be accepted");

        assert!(err.contains("not signed"), "got {err:?}");
    }

    /// Signed, but by something holding a different key — which is every
    /// device on the network except the one that was paired with.
    #[test]
    fn a_reply_signed_with_the_wrong_key_is_refused() {
        let body = b"the audio";
        let digest = sync::digest(body);
        let theirs = Reply::Bytes {
            len: 9,
            total: 9,
            digest: digest.clone(),
            mac: transfer_mac(&[0x11; 32], "/music/a.m4a", 0, 9, 9, &digest, body),
        };

        let err = accept_bytes(&SHARED, "/music/a.m4a", 0, theirs, body)
            .expect_err("someone else's signature must not be accepted");

        assert!(err.contains("not signed"), "got {err:?}");
    }

    /// The MAC covers the question as well as the answer. Without that, a
    /// genuine signed chunk of one track is a valid reply to a request for
    /// another, and a peer's own bytes become the substitution.
    #[test]
    fn a_signed_chunk_is_not_a_valid_answer_to_a_different_question() {
        let (reply, body) = served("/music/a.m4a");

        let err = accept_bytes(&SHARED, "/music/b.m4a", 0, reply, &body)
            .expect_err("a reply for one track must not answer for another");

        assert!(err.contains("not signed"), "got {err:?}");
    }

    #[test]
    fn a_signed_chunk_is_not_a_valid_answer_at_a_different_offset() {
        let (reply, body) = served("/music/a.m4a");

        let err = accept_bytes(&SHARED, "/music/a.m4a", 4, reply, &body)
            .expect_err("a reply for one offset must not answer for another");

        assert!(err.contains("not signed"), "got {err:?}");
    }

    /// The whole thing over a real socket, with something in the middle doing
    /// what the ticket describes: reading the reply, replacing the audio, and
    /// recomputing the digest so the two still agree.
    ///
    /// Every other test here calls `handle` and `accept_bytes` directly. This
    /// one is worth its length because the mismatch it would catch — a MAC
    /// computed over one framing and checked over another — is invisible to
    /// both halves when they are the same process and the same values.
    #[test]
    fn audio_rewritten_between_the_two_devices_never_reaches_the_caller() {
        let real = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
        let real_address = real.local_addr().expect("addr").to_string();
        std::thread::spawn(move || {
            for stream in real.incoming().take(2) {
                let Ok(stream) = stream else { continue };
                let library = Fixture::trusting("them");
                serve(&library, stream).ok();
            }
        });

        // First, undisturbed, so that a failure below is the rewrite and not
        // the wire.
        let (reply, body) = ask(&real_address, &fetch("/music/a.m4a")).expect("the direct fetch");
        accept_bytes(&SHARED, "/music/a.m4a", 0, reply, &body)
            .expect("an untouched reply has to be accepted, or this proves nothing");

        // Now with someone on the path.
        let middle = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
        let middle_address = middle.local_addr().expect("addr").to_string();
        let upstream = real_address.clone();
        std::thread::spawn(move || {
            let (victim, _) = middle.accept().expect("accept");
            let mut to_victim = victim.try_clone().expect("clone");
            let mut from_victim = BufReader::new(victim);

            let server = TcpStream::connect(&upstream).expect("connect");
            let mut to_server = server.try_clone().expect("clone");
            let mut from_server = BufReader::new(server);

            // Pass the request through untouched.
            let mut request = String::new();
            from_victim.read_line(&mut request).expect("request");
            to_server.write_all(request.as_bytes()).expect("forward");
            to_server.flush().expect("flush");

            let mut header = String::new();
            from_server.read_line(&mut header).expect("reply");
            let Reply::Bytes { len, mac, .. } =
                serde_json::from_str::<Reply>(header.trim()).expect("parse")
            else {
                panic!("expected bytes from the real device");
            };
            let mut genuine = vec![0u8; len as usize];
            from_server.read_exact(&mut genuine).expect("body");

            // Both halves rewritten together, which is exactly what the
            // SHA-256 alone could not tell apart. The MAC is passed straight
            // through, because it is the one field that cannot be recomputed.
            let substituted = b"malware, not music".to_vec();
            let forged = Reply::Bytes {
                len: substituted.len() as u64,
                total: substituted.len() as u64,
                digest: sync::digest(&substituted),
                mac,
            };
            let line = format!("{}\n", serde_json::to_string(&forged).expect("serialise"));
            to_victim.write_all(line.as_bytes()).expect("header");
            to_victim.write_all(&substituted).expect("body");
            to_victim.flush().expect("flush");
        });

        let (reply, body) = ask(&middle_address, &fetch("/music/a.m4a")).expect("the fetch");
        assert_eq!(
            body, b"malware, not music",
            "the rewrite has to have actually happened"
        );

        let err = accept_bytes(&SHARED, "/music/a.m4a", 0, reply, &body)
            .expect_err("substituted audio must never come back from here");
        assert!(err.contains("not signed"), "got {err:?}");
    }

    // --- AUD-7: the exchange that produces the key -------------------------

    #[test]
    fn both_sides_of_a_pairing_derive_the_same_key() {
        let a = Handshake::begin().expect("the test machine has randomness");
        let b = Handshake::begin().expect("the test machine has randomness");

        let theirs = a.finish(&b.public_key(), "device-a", "device-b");
        let ours = b.finish(&a.public_key(), "device-b", "device-a");

        assert!(theirs.is_some());
        assert_eq!(
            theirs, ours,
            "the ids are sorted so the roles do not matter"
        );
    }

    /// Two pairings are two keys. Otherwise every Vapor install on the subnet
    /// could sign for every other one.
    #[test]
    fn a_third_device_derives_a_different_key() {
        let a = Handshake::begin().expect("randomness");
        let b = Handshake::begin().expect("randomness");
        let c = Handshake::begin().expect("randomness");

        let with_b = a.finish(&b.public_key(), "device-a", "device-b");
        let with_c = a.finish(&c.public_key(), "device-a", "device-c");

        assert!(with_b.is_some() && with_c.is_some());
        assert_ne!(with_b, with_c);
    }

    /// The key is bound to who was pairing, not only to the two halves, so a
    /// relayed exchange under a different name does not land on the same
    /// secret.
    #[test]
    fn the_key_is_bound_to_the_two_device_ids() {
        let a = Handshake::begin().expect("randomness");
        let b = Handshake::begin().expect("randomness");

        let honest = a.finish(&b.public_key(), "device-a", "device-b");
        let renamed = a.finish(&b.public_key(), "device-a", "device-z");

        assert_ne!(honest, renamed);
    }

    /// Curve25519 does not fail on a low-order point — it produces an all-zero
    /// shared secret whatever this side contributed, so a peer that sends one
    /// has chosen the "shared" key on its own and anyone who knows the trick
    /// can sign with it. Refusing is this side's job.
    #[test]
    fn a_public_key_that_forces_a_known_secret_is_refused() {
        let mine = Handshake::begin().expect("randomness");
        let all_zero = encode(&[0u8; 32]);

        assert_eq!(mine.finish(&all_zero, "device-a", "device-b"), None);
    }

    #[test]
    fn a_public_key_that_is_not_a_key_is_refused() {
        let mine = Handshake::begin().expect("randomness");

        assert_eq!(mine.finish("", "device-a", "device-b"), None);
        assert_eq!(
            mine.finish("not base64 at all!", "device-a", "device-b"),
            None
        );
        assert_eq!(
            mine.finish(&encode(b"too short"), "device-a", "device-b"),
            None
        );
    }

    /// A build from before AUD-7 sends a pair request with no public key. It
    /// cannot be paired with, because there would be no key to check its
    /// replies against — and it is told so rather than being trusted anyway.
    #[test]
    fn a_pairing_request_that_carries_no_public_key_is_refused() {
        let library = Fixture::opening("482913", "them");

        let (reply, _) = handle(
            &library,
            &Request::Pair {
                device_id: "them".into(),
                name: "Phone".into(),
                device_kind: DeviceKind::Phone,
                pin: "482913".into(),
                public_key: String::new(),
            },
        );

        assert!(matches!(reply, Reply::Refused { .. }), "got {reply:?}");
        assert!(
            !library.trust.lock().unwrap().allows("them"),
            "and nothing was written down"
        );
    }

    /// A device carried over from before AUD-7 is listed but has no key, and
    /// the gate refuses it — the upgrade path is to pair again, not to serve
    /// it unauthenticated.
    #[test]
    fn a_device_paired_before_there_were_keys_is_served_nothing() {
        let library = Fixture::new();
        {
            let mut trust = library.trust.lock().unwrap();
            let old = r#"{"devices":[{"id":"them","name":"Phone","kind":"phone","pairedAt":1}]}"#;
            *trust = serde_json::from_str(old).expect("an old trust file loads");
        }

        let (reply, body) = handle(&library, &fetch("/music/a.m4a"));

        assert!(matches!(reply, Reply::Refused { .. }), "got {reply:?}");
        assert!(body.is_none());
    }

    /// The refusal a stale pairing gets is the one an unknown device gets,
    /// word for word. Anything else would let something on the subnet ask
    /// which device ids this machine has heard of.
    #[test]
    fn a_stale_pairing_is_refused_in_the_same_words_as_a_stranger() {
        let stale = Fixture::new();
        {
            let mut trust = stale.trust.lock().unwrap();
            let old = r#"{"devices":[{"id":"them","name":"Phone","kind":"phone","pairedAt":1}]}"#;
            *trust = serde_json::from_str(old).expect("an old trust file loads");
        }

        let text = |library: &Fixture| match handle(library, &fetch("/music/a.m4a")).0 {
            Reply::Refused { reason } => reason,
            other => panic!("expected a refusal, got {other:?}"),
        };

        assert_eq!(text(&stale), text(&Fixture::new()));
    }
}
