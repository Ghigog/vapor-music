# 🎵 Vapor Music

> *Own your music. Own your vibe.*

A local-first, AI-enhanced music player for desktop and mobile, written in Rust with a React interface. Vapor Music is for people who are done renting their music from corporate algorithms — and who refuse to sacrifice the premium, intelligent listening experience that comes with modern streaming.

---

## Philosophy

The era of streaming has trained people to accept two things as trade-offs: **convenience** and **ownership**. You could have one, but not both.

Vapor Music rejects this entirely.

We believe your music library is a personal artifact — one that should survive licensing collapses, algorithm shifts, internet outages, and corporate pivots. Your collection should feel *alive*, not like a read-only database you pay a monthly fee to access. It should know your mood. It should guide your listening. It should feel like the world's best DJ that happens to live entirely in your pocket or on your desk.

This is not a utility tool. It is an **active audio experience** built around the philosophy that the best music player is one that understands your music as deeply as you do.

---

## The Counter-Culture We Serve

There is a quiet but rapidly growing ownership renaissance among music lovers. They are exhausted by:

- Libraries that disappear overnight when licensing deals expire
- Jarring, mood-killing shuffle transitions between incompatible tracks
- Aggressive data harvesting that treats listening habits as a product to sell
- Dependency on internet connectivity just to hear songs they've loved for decades
- Corporate interfaces that prioritize discovery revenue over the listener's own collection

Vapor Music is built *for* these people.

---

## Core USPs

### 1. 🎛️ Harmonic AI Mixing — *The Local AI DJ*

This is the heart of the app's identity and the feature that separates Vapor from every other local music player.

Traditional players shuffle by picking `Random Song A` and crossfading it into `Random Song B`. This destroys the vibe. Vapor doesn't shuffle — it **conducts**.

**How It Works:**

- **Waveform & Energy Analysis** — At import time, Vapor analyzes every track locally on the user's device or home server, extracting:
  - BPM (Beats Per Minute)
  - Musical key (e.g., C Major → A Minor — using the Camelot Wheel for harmonic compatibility)
  - Spectral density and frequency profile
  - How dynamic the track is, on a 0–1 scale. Vapor measures energy; it does not claim to know that a song is sad.
- **Infinite Blend Shuffle** — When shuffle is engaged, the app dynamically constructs a listening path. A mellow acoustic track will never jump directly into heavy electronic music. Vapor bridges the gap through mid-tempo, harmonically compatible intermediary tracks, gently steering the wavelength of the session.
- **Intelligent Auto-Mixing** — Instead of a generic 5-second volume fade, Vapor identifies the optimal **exit beat** of the outgoing track and the optimal **entry intro** of the incoming track, then time-stretches the incoming one to meet it. Up to ±6% — past that the stretch stops sounding like the record, so Vapor refuses the mix rather than shipping an artefact.

**Key Differentiator:** All analysis and mixing logic runs entirely **on-device**. No cloud AI, no subscriptions, no privacy compromise.

> [!NOTE]
> A critical analysis of the current transition limitations and a technical roadmap for professional-grade mixing can be found in the [AI DJ Refactor Plan](docs/FINDINGS.md).

---

### 2. ☁️ Your Own Cloud — *No Server of Ours, and None to Run*

The primary reason everyday users stay on Spotify is friction. Setting up Navidrome or Plex requires Docker, port forwarding, and a networking degree. Vapor asks for none of that: there is no Vapor server, no account, and nothing to self-host.

What it does ask for today is somewhere to point at — a WebDAV URL. If you have cloud storage that speaks WebDAV, that is a URL and a password. If you do not, that is a real barrier and an honest one to state: playing a folder of local files without configuring anything is wanted and not built.

**The Architecture:**

```
[ Your Music Files ]  ──►  [ Your Cloud Storage ]  ──►  [ Smart Client Apps ]
 (PC / NAS / Drive)         (any WebDAV server)          (Desktop / Mobile)
													   │
													   Analysis runs here
													   (BPM, Key, Energy Mapping)
```

- **Bring Your Own Cloud** — Point Vapor at a WebDAV server you control. No Vapor-operated infrastructure sits between you and your files, because there is none to sit there: the app has no backend. Native Proton Drive, Mega, Google Drive and Dropbox backends are wanted and not built — today they work only if you put a WebDAV layer in front of them.
- **P2P Local Network Sync** — When a phone and PC are on the same Wi-Fi network, they fast-sync directly with each other using local discovery — no internet required, no bandwidth charges, no latency.
- **No account, no server, no telemetry** — Vapor has no servers, so there is nothing for Vapor to see. Your files go straight to storage you control, over that provider's HTTPS. Vapor adds no encryption layer of its own, and LAN sync between your own devices goes over the wire unencrypted — a decision made for a single trusted network, recorded in [`docs/RELEASE.md`](docs/RELEASE.md) and due a revisit before the app goes to strangers.

---

## High-Value Feature Set

### 📂 Playlist Management & Interactive Curation
Vapor Music features a local-first, drag-and-drop playlist curation system that natively integrates with our Vibe DJ transitions:
* **Collapsible Sidebar Hub**: The navigation sidebar houses a collapsible **Playlists** section. Create new playlists on-the-fly using the inline `+` button, and rename existing entries by double-clicking them to expose dynamic text inputs.
* **Fluid Drag-and-Drop Workflow**:
  * Drag tracks from the main **Library** browser and drop them onto sidebar playlist items to add them instantly.
  * Drop tracks anywhere in the active **Playlist Screen** to append them, or drop them directly onto a track row to insert them at that specific index.
* **Drag-to-Reorder Mechanics**: Arrange your vibe by grabbing track row drag handles (`☰`) in the playlist view and dragging them to reorder.
* **Living Custom Covers & Metadata**: Customize playlist cover art using the visual `Pencil` icon overlay. Select files using the built-in `FileDialog` or drag-and-drop cover art files directly from your operating system explorer. If no cover art is defined, the system automatically falls back to displaying the cached album art from the playlist's first track.
* **Vibe DJ Integration**: Playlists aren't static lists; they feed into the Vibe DJ's engine. Toggle **Smart Mixing** to dynamically generate transition effects (Standard Crossfade, Bass Swap, or Filter Sweep) based on BPM and musical key differences between adjacent tracks. Engage **Harmonic Shuffle** to calculate an optimized, smooth blend transition sequence through your entire playlist.

### 🎨 Digital Liner Notes & Living Metadata
Streaming services stripped away the beautiful context of music — the album art booklets, production credits, session notes, and lyrics. Vapor restores it.

- A dedicated **Liner Notes** screen acts as a premium digital vinyl sleeve
- Everything the app worked out about the track from the audio itself — tempo, key, energy, loudness — presented as what it is: measured on your device, not fetched. Written notes and production credits exist in no file anyone ships, so Vapor shows what it actually knows instead of mocking them up.
- High-resolution artwork, and optional lyrics from LRCLIB. Lyrics are off until asked for, because looking them up sends the artist and title to a server you have no relationship with.
- Vapor **reads** your tags and never rewrites your files. Editing them is a job for a tagger.

### 🖼️ Visual Aesthetic Customization
Vapor's UI is built for focused, distraction-free listening.

- **Apple-inspired glassmorphism** — frosted-glass panels, a minimal 3-colour palette, and seamless adaptation to any wallpaper or desktop. Two built-in themes: **Daylight** (warm paper and sky, `#007AFF` accent) and **Lamplight** (warm umber under one lamp, `#EC992F` accent), or follow the machine.
- Fully themeable with community-shareable colour schemes (see the [Theme System Developer Guide](docs/theme_system.md) and [Design Language](docs/DESIGN_LANGUAGE.md) for details on creating custom visual presets)

---

## Competitive Positioning

| Feature | Corporate Streaming | Standard Local Players | **Vapor Music** |
|---|---|---|---|
| **Ownership** | ✗ Renting only | ✓ 100% Owned | ✓ **100% Owned** |
| **Library Sync** | ✓ Seamless | ✗ Manual file transfers | ✓ **Automated Cloud / P2P** |
| **The Vibe** | ✗ Jarring shuffles & ads | ✗ Basic crossfades | ✓ **Harmonic AI DJ Transitions** |
| **Privacy** | ✗ Aggressive data tracking | ✓ High privacy | ✓ **Private & Decentralized** |
| **Metadata & Art** | ✗ Minimal / controlled | ✗ Basic tag editors | ✓ **Living Liner Notes** |
| **Audio Fidelity** | ✗ Lossy compression | ✓ Lossless support | ✓ **Lossless + HW Calibration** |
| **Server Required** | N/A | ✗ Sometimes | ✓ **Never — zero config** |

---

## Ideal Architecture

### Technology Stack
- **Shell:** Tauri 2 — a Rust backend and a React 19 frontend in one desktop binary
- **Target Platforms:** macOS, Windows, Linux (Desktop) · Android (Mobile)
- **Audio Backend:** `cpal` for the device, with mixing, crossfade, time-stretching and limiting in `vapor-engine`
- **Local Analysis:** `vapor-dsp` — tempo, key, energy and loudness computed in-process, with no external toolchain and no Homebrew tail.
- **Cloud Sync Layer:** A WebDAV client, plus peer-to-peer transfer between paired devices on a local network.
- **Storage:** Plain JSON, one file per concern, in a single directory the Your Data screen will show you the path of. Every write is atomic and durable, and every file carries a shape version so a later release can migrate it rather than treat it as damage.

### Module Breakdown

```
vapor-music/
├── vapor-core/crates/
│   ├── vapor-dsp/           # Tempo, key, energy, loudness. No I/O, compiles to wasm
│   ├── vapor-engine/        # Mixer, transitions, time-stretch, audio device
│   └── vapor-library/       # Index, playlists, folders, groups, sync model
├── vapor-app/
│   ├── src-tauri/src/       # The shell: the only place with a filesystem
│   │   ├── lib.rs           # Command surface and app state
│   │   ├── audio.rs         # Playback thread and its realtime guarantees
│   │   ├── analysis.rs      # Import-time analysis, off the UI thread
│   │   ├── peers.rs         # LAN discovery, pairing, transfer
│   │   ├── sync.rs webdav.rs remote_source.rs
│   │   ├── store.rs cache.rs covers.rs  # Persistence
│   │   └── secrets/         # OS keychain; Android Keystore over JNI
│   └── src/                 # React 19
│       ├── screens/         # One file per screen
│       ├── components/      # Shared widgets
│       └── lib/generated/   # Types derived from the Rust by ts-rs — never edited by hand
└── docs/                    # Decisions, findings, testing, release
```

### Data Flow

```
 Import Track
	  │
	  ▼
 [Analyzer] ─── BPM, Key, Energy, Spectral Profile
	  │
	  ▼
 [JSON Library Cache] ─── Indexed, searchable, portable
	  │
	  ▼
 [AI DJ Module] ─── Constructs harmonic listening paths
	  │
	  ▼
 [Audio Engine] ─── Intelligent crossfade, pitch correction, EQ
	  │
	  ▼
 [Output] ─── Calibrated by headphone profile
```

---

## How it is built

Two pieces, and one of them is being retired.

**`vapor-core/`** — three Rust crates with no I/O and no platform code.
`vapor-dsp` decodes, and finds tempo, key and cue points. `vapor-engine` is two
decks, the EQ and filter chain, and the six transitions. `vapor-library` is
playlists, grouping, the queue, the Camelot pathfinder and device sync. Because
none of it touches a socket or a file, all of it is testable without an app,
and it compiles to wasm as well as native.

**`vapor-app/`** — a Tauri shell: a React frontend and a Rust backend holding
everything the core deliberately does not. The audio device (`cpal`), WebDAV,
the keychain, the filesystem cache, media keys, and the network. Commands are
the only way in, and `tests/command_bindings.rs` fails the build if one has no
frontend binding.

The original Godot build and its C++ `AudioDSP` GDExtension (Essentia, Rubber
Band) were removed from the tree on 2026-08-21, once everything they did lived
in the Rust core — which is what removed the Homebrew dependency tail and the
macOS-only DSP. The tag `godot-final-v1.78` holds that build in full, and the
version history below is its history.

The full account of what was measured and decided along the way is in
[docs/FINDINGS.md](docs/FINDINGS.md).

---

## Design Principles

1. **Local-First** — Every feature must work with zero internet connectivity.
2. **Nothing to Run** — No Vapor server, no account, no Docker, no port forwarding. If a normal person can't set it up in 5 minutes, it's not ready — and by that standard the WebDAV requirement is not there yet.
3. **Privacy by Architecture** — Vapor cannot see user data by design, not just by policy.
4. **The Vibe is Sacred** — No jarring transitions. Ever. The listening experience is a first-class citizen.
5. **Own Your Data** — Library metadata, listening history, and analysis results are stored in open, portable formats the user can inspect, move, and back up themselves.

---


## Status

**Vapor Music 2.0** — a Tauri 2 shell over three Rust crates (`vapor-dsp`,
`vapor-engine`, `vapor-library`) with a React 19 front end. Builds for macOS,
Windows, Linux and Android.

Nothing has been released yet. What has to be true before a build reaches
anyone who is not the author — signing, notarisation, the Android keystore, the
updater — is enumerated in [`docs/RELEASE.md`](docs/RELEASE.md), and the
decisions behind the shape of the release are in
[`docs/DECISIONS.md`](docs/DECISIONS.md).

The original Godot application was deleted on 2026-08-21. Its eighty release
notes moved to [`docs/CHANGELOG-godot.md`](docs/CHANGELOG-godot.md); its code
is on the tag `godot-final-v1.78`.

---

## License

Vapor Music is **proprietary** — © 2026 Dylan Growcoot, all rights reserved.
The full notice is in [`LICENSE`](LICENSE).

It was AGPL-3.0 until 2026-08-20, and that was a consequence rather than a
choice: the Godot build linked **Essentia** (AGPL-3.0) for BPM and key detection
and the **Rubber Band Library** (GPL-2.0-or-later) for time-stretching, both
strong copyleft. Neither has shipped since the Rust rewrite — analysis is
`vapor-dsp`, and the stretcher is Signalsmith Stretch (MIT) — so nothing in the
dependency tree compels a licence any more.

The reserved position is a starting point, not a settled one. It is taken first
because the directions are not equally reversible: rights not yet granted can be
granted later, and rights already granted cannot be withdrawn from work already
distributed. The route to an open licence, and what would have to change, is in
[`docs/LICENSING.md`](docs/LICENSING.md).

Third-party components keep their own licences and are unaffected by any of
this.

- Third-party components and what they are used for: [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)
- Bundled license texts: [`licenses/`](licenses/)
- Reasoning, obligations and alternatives considered: [`docs/LICENSING.md`](docs/LICENSING.md)

Source: <https://github.com/Ghigog/vapor-music>

---

*Built with Rust and Tauri · Designed for music lovers who remember owning things.*
