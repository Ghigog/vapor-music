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
use std::sync::atomic::{AtomicU64, Ordering};
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

/// Refusal to wait forever on a peer that opened a connection and went quiet.
const IO_TIMEOUT: Duration = Duration::from_secs(20);

/// The largest request line accepted.
///
/// A request is a line of JSON naming a track. Without a bound, a peer can
/// hold the connection open and stream until the process runs out of memory —
/// and this listens on a network, so "a peer" includes anything on the café
/// Wi-Fi.
const MAX_REQUEST: u64 = 64 * 1024;

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
    getrandom(&mut bytes);
    format!("{:06}", u32::from_le_bytes(bytes) % 1_000_000)
}

/// Random bytes from the OS, falling back to a unique-but-not-secret source.
///
/// The fallback exists so a platform without `/dev/urandom` cannot make the
/// app unusable; it is deliberately loud about being second best, because a
/// silently weak PIN is worse than no pairing at all.
fn getrandom(out: &mut [u8; 4]) {
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(out))
        .is_ok()
    {
        return;
    }
    eprintln!("sync: no OS randomness available; this pairing code is weak");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    *out = nanos.to_le_bytes();
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
    },
    Refused {
        reason: String,
    },
    Manifest(Box<Manifest>),
    /// `len` bytes follow on the wire, and `digest` covers the whole file, not
    /// this chunk — so a resumed transfer is verified once, at the end.
    Bytes {
        len: u64,
        total: u64,
        digest: String,
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
    fn pair(&self, device_id: &str, name: &str, kind: DeviceKind, pin: &str) -> PairOutcome;
    fn manifest(&self) -> Manifest;
    /// The bytes of a track this device holds, or `None` when it holds none.
    ///
    /// Takes an href and returns bytes — never a path — so the caller cannot
    /// ask for a file by location.
    fn read_track(&self, href: &str) -> Option<Vec<u8>>;
    /// This device's own name and id, for the advert.
    fn identity(&self) -> (String, String, DeviceKind);
}

/// Answer one request.
///
/// Separated from the socket so every branch — including the refusals, which
/// are the ones that matter — is reachable without a network.
pub fn handle(library: &dyn Library, request: &Request) -> (Reply, Option<Vec<u8>>) {
    if let Some(refusal) = authorise(&library.trust(), request) {
        return (refusal, None);
    }

    match request {
        Request::Pair {
            device_id,
            name,
            device_kind,
            pin,
        } => match library.pair(device_id, name, *device_kind, pin) {
            PairOutcome::Paired => {
                let (id, name, _) = library.identity();
                (
                    Reply::Paired {
                        device_id: id,
                        name,
                    },
                    None,
                )
            }
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
        },

        Request::Manifest { .. } => (Reply::Manifest(Box::new(library.manifest())), None),

        Request::Fetch {
            href, offset, len, ..
        } => {
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

            (
                Reply::Bytes {
                    len: slice.len() as u64,
                    total,
                    digest,
                },
                Some(slice),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Sockets
// ---------------------------------------------------------------------------

/// Who is on the network, shared between the beacon thread and the commands.
pub type Peers = Arc<Mutex<PeerRegistry>>;

/// Bytes moved since the process started, for the dashboard's rate.
pub static BYTES_MOVED: AtomicU64 = AtomicU64::new(0);

/// Shout every few seconds, and listen for everyone else.
///
/// Two threads on one socket, because a broadcaster that cannot hear replies
/// is only half a discovery protocol and binding twice to the same port is not
/// portable.
pub fn spawn_beacon(peers: Peers, id: String, name: String, kind: DeviceKind) {
    let socket = match UdpSocket::bind((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT)) {
        Ok(s) => s,
        Err(e) => {
            // Another copy of the app, or a locked-down network. Discovery is
            // a convenience: without it a person can still sync, they just
            // have to be told about it.
            eprintln!("sync: discovery is unavailable ({e})");
            return;
        }
    };
    if let Err(e) = socket.set_broadcast(true) {
        eprintln!("sync: cannot broadcast on this network ({e})");
        return;
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
            return;
        }
    };
    let datagram = advert.encode();
    let _ = std::thread::Builder::new()
        .name("vapor-sync-beacon".into())
        .spawn(move || loop {
            let _ = shouting.send_to(datagram.as_bytes(), (Ipv4Addr::BROADCAST, DISCOVERY_PORT));
            std::thread::sleep(BEACON_INTERVAL);
        });

    let _ = std::thread::Builder::new()
        .name("vapor-sync-listener".into())
        .spawn(move || {
            let mut buffer = [0u8; 2048];
            loop {
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
        });
}

/// Serve requests from other devices.
pub fn spawn_server(library: Arc<dyn Library>) {
    let listener = match TcpListener::bind((Ipv4Addr::UNSPECIFIED, SYNC_PORT)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("sync: cannot accept connections ({e})");
            return;
        }
    };

    let _ = std::thread::Builder::new()
        .name("vapor-sync-server".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let library = Arc::clone(&library);
                // A connection per thread. The expected concurrency is "the
                // other device", and a thread pool for that would be more
                // machinery than the problem.
                let _ = std::thread::Builder::new()
                    .name("vapor-sync-conn".into())
                    .spawn(move || {
                        if let Err(e) = serve(library.as_ref(), stream) {
                            eprintln!("sync: connection ended ({e})");
                        }
                    });
            }
        });
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
    reader
        .by_ref()
        .take(MAX_REQUEST)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let reply: Reply =
        serde_json::from_str(line.trim()).map_err(|_| "unreadable reply".to_string())?;

    let mut body = Vec::new();
    if let Reply::Bytes { len, .. } = &reply {
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
    }

    impl Fixture {
        fn new() -> Self {
            Fixture {
                trust: StdMutex::new(Trust::new()),
                pairing: StdMutex::new(None),
                tracks: vec![("/music/a.m4a".to_string(), b"the audio".to_vec())],
            }
        }

        fn trusting(id: &str) -> Self {
            let f = Fixture::new();
            f.trust
                .lock()
                .unwrap()
                .add(id, "Peer", DeviceKind::Phone, 0);
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
        fn pair(&self, id: &str, name: &str, kind: DeviceKind, pin: &str) -> PairOutcome {
            let mut pairing = self.pairing.lock().unwrap();
            let Some(open) = pairing.as_mut() else {
                return PairOutcome::Refused;
            };
            let outcome = open.offer(id, pin, 1);
            if outcome == PairOutcome::Paired {
                self.trust.lock().unwrap().add(id, name, kind, 1);
            }
            outcome
        }
        fn manifest(&self) -> Manifest {
            Manifest {
                device_id: "us".into(),
                ..Default::default()
            }
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
            Reply::Bytes { len, total, digest } => {
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

        let wrong = Request::Pair {
            device_id: "them".into(),
            name: "Phone".into(),
            device_kind: DeviceKind::Phone,
            pin: "000000".into(),
        };
        assert!(matches!(handle(&library, &wrong).0, Reply::Refused { .. }));

        let right = Request::Pair {
            device_id: "them".into(),
            name: "Phone".into(),
            device_kind: DeviceKind::Phone,
            pin: "482913".into(),
        };
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

        let (reply, _) = handle(
            &library,
            &Request::Pair {
                device_id: "them".into(),
                name: "Phone".into(),
                device_kind: DeviceKind::Phone,
                pin: "482913".into(),
            },
        );

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
}
