# Privacy

**Last updated: 2026-08-23.**

Vapor Music is a music player that runs on your device and plays files you
already own. There is no Vapor account, no Vapor server, and nothing in the app
that reports back to its developer.

This document describes what the shipped code actually does. Every claim in it
is checkable against the source, and the last section says how to check.

---

## The short version

| What | What leaves your device | To whom | On by default? |
|---|---|---|---|
| Playing, analysing, mixing, playlists, tags, the DJ | Nothing | — | Always on, always silent |
| Update check (desktop builds) | An HTTPS request for a release file | GitHub | **Yes**, at every launch. No switch |
| Lyrics and artwork lookup | Artist, album and track names | LRCLIB, Deezer | **No** |
| Your own cloud library | Your username, your password, your music | **The server you chose.** Never the developer | **No** — nothing until you configure a server |
| Sync over Wi-Fi | A device name and a per-install id | Everything on your local network | **No** |

---

## What never leaves, under any setting

- **There is no account.** Nothing to sign up for, no email address collected,
  no password of ours. The only credentials the app holds are for a server you
  chose yourself.
- **There is no telemetry, no analytics, no crash reporting and no advertising
  identifier.** Not disabled by default — absent. The app has no code that
  sends usage, errors or identifiers anywhere.
- **Everything the app knows about your music, it worked out here.** Tempo,
  musical key, energy, loudness, cue points, which track should follow which
  and how to mix it — all of that is measured on this device from the audio
  itself, by code in the app. No cloud analysis, no model calls, nothing sent
  away to be scored.
- **Your listening is not recorded anywhere off the device.** What you played,
  when, how often, what you skipped — the app keeps what it needs locally and
  transmits none of it.
- **File paths and file contents are never sent to a third party.** Your music
  only ever moves between this device and storage you configured yourself.

---

## 1. Lyrics and artwork — off unless you turn it on

**Default: off.** In the code this is `Settings::metadata_lookup_enabled`, and
its default value is `false`
(`vapor-core/crates/vapor-library/src/settings.rs`). Turn it on in
**Settings → Fetch lyrics and artwork**. Until you do, the app makes no
request to either service.

**What is sent.** Three strings: the artist, the album, and the track title, as
the app displays them. Those come from the file's own tags, or — when a file has
no tags — from its folder and filename. They go out URL-encoded in the query
string of an ordinary HTTPS request, along with:

- a `User-Agent` naming the app, its version, and its public repository, and
- your IP address, which any HTTPS request necessarily reveals to the host.

Nothing else. Not the file path, not the file, not your library, not an
identifier for you or this installation. A name the app does not really have —
blank, or `Unknown Artist` / `Unknown Album` / `Unknown Track` — is never sent;
a track with no usable title is not looked up at all, and one with a title but
no artist is searched on the title alone.

**Where it goes.**

| Host | Requested |
|---|---|
| `lrclib.net` | `/api/get?artist_name=…&track_name=…` — the words to a track |
| `api.deezer.com` | `/search/artist`, `/search/track`, `/track/{id}`, `/search/album`, `/album/{id}` — a portrait, a sleeve, a genre, and a published tempo |
| `cdn-images.dzcdn.net` | The image files those searches point at |

Requests time out after 8 seconds and are abandoned.

**When it happens.** Three moments, all of them while the setting is on:

1. **A track starts playing.** It is looked up once, ever. The answer is cached
   on disk, so playing the same record again asks nothing.
2. **You open Liner Notes and ask.** Same request, made deliberately.
3. **The Identify pass.** This one sends the artist and title of **every
   analysed track in your library** to Deezer, one after another. It is the
   largest thing the app ever discloses. It runs when you press Identify, and
   also automatically at the end of a library analysis **if lookups are already
   on**. Its purpose is to settle whether a tempo the app measured at 87 should
   be counted as 174 — Deezer's number is never adopted as the tempo, only used
   to choose between octaves of the one measured here.

**What comes back** — lyrics, a genre, image files, a tempo reference — is
stored in the app's own data folder on this device and nowhere else.

**One thing is not gated by that setting:** the **Find album art** button on an
album. Pressing a button labelled "search for the real cover" is itself the
asking, so it searches Deezer whether or not automatic lookups are on. It sends
the same two strings — artist and album — and nothing more.

**To stop it:** switch the setting off. That stops all future requests
immediately. It does not delete what was already cached;
**Settings → Your data → Delete everything stored here** does that.

---

## 2. Your own cloud — a server we never see

Vapor can play from a WebDAV server: Nextcloud, Koofr, ownCloud, a box in your
own house. That server is **yours**. You supply its address, your username and
your password. The developer of Vapor Music runs no server, is not in the path
of that traffic, and cannot see any of it.

- Nothing here happens until you configure a server. There is no default.
- **Your password is stored in your operating system's credential store** —
  Keychain on macOS and iOS, Credential Manager on Windows, Secret Service on
  Linux, and an Android Keystore-encrypted file on Android. It is never written
  into the app's settings file and never sent anywhere except, as HTTP Basic
  authentication over HTTPS, to the server you named.
- Traffic to that server is: listing folders, downloading tracks you play or
  download, and uploading one small file (below).
- **Deleting all data also deletes the stored password.**

**The file the app writes beside your music.** When you sync, the app keeps one
JSON document, `vapor_metadata.json`, in the same folder as your library on your
server — deliberately in plain sight rather than hidden. It holds your
playlists, your folders, your dynamic groups, your hand-typed tempo
corrections, a record of what you deleted so deletions travel between your own
devices, and the per-install id of the device that wrote it last. It contains no
credentials.

---

## 3. Sync over Wi-Fi — off unless you turn it on

**Default: off.** In the code this is `Settings::sync_enabled`, and its default
value is `false` (`vapor-core/crates/vapor-library/src/settings.rs`). While it
is off, no socket is bound and nothing is broadcast — which is also why you get
no firewall prompt.

**When you turn it on**, this device does two things:

- **It shouts.** Every 5 seconds it sends one UDP datagram to the broadcast
  address on port 7676. Anything on the same network can read it. The datagram
  contains a per-installation id, a display name, whether this is a phone or a
  desktop, the port to reach it on, and a protocol version. **It does not
  contain your music, your library, or anything about what you listen to.**
  - The **id** is generated once per installation from the clock. It says
    nothing about you, your account or your hardware.
  - The **display name** is a real disclosure and worth knowing about: on macOS
    it is your Sharing name, which is often *"<Your Name>'s MacBook"*; on
    Android it is the device model, such as "Pixel 9". It goes out in clear on
    whatever network you are joined to, including a café's, for as long as sync
    is on.
- **It listens**, on TCP port 7677. A device that has not been paired with this
  one can do exactly one thing: ask to pair, which requires a PIN this device
  shows you. Every other request is refused before it is read any further.

**Once you have paired two of your own devices**, they exchange a manifest —
the library paths, file sizes and content hashes each one knows — and then the
bytes of the tracks being copied. This is device-to-device on your own network.
It does not pass through any server. The app refuses to connect to any peer
whose address is not on a private network.

**To stop it:** switch it off. The sockets are closed and the threads stopped
before the switch returns.

---

## 4. Update checks — desktop only, and currently not optional

Every time a **desktop** build starts, it asks GitHub whether there is a newer
release, by fetching
`https://github.com/Ghigog/vapor-music/releases/latest/download/latest.json`.
GitHub sees what any web server sees: your IP address and the request. Nothing
about you or your library is included.

If a newer signed release exists, it is downloaded and installed silently, and
runs from the next launch. Updates are verified against a public key compiled
into the app.

**There is no in-app setting to turn this off.** If you do not want it, block
the app at your firewall. Android builds do not do this at all — they have no
updater.

---

## 5. What is stored on your device, and where

Everything the app remembers lives in one folder — your operating system's
application-data folder for `com.dylangrowcoot.vapormusic`. **Settings → Your
data**, at the bottom of the Settings screen, names the exact path, itemises
what is in it with sizes, and will open it in your file manager. The files are
plain JSON; you can read them.

The main ones:

| | |
|---|---|
| `analysis.json` | The library catalogue: tempo, key, energy, cue points |
| `tags.json` | Track tags, as edited or as read from the files |
| `playlists.json`, `folders.json`, `groups.json` | How you organised the library |
| `plays.json`, `skips.json` | **What you have played and skipped.** This is how the DJ learns what you like. It stays here; nothing reads it but this app |
| `settings.json` | Settings. **Never any password** |
| `metadata.json` | What lyric and artwork lookups returned, if you turned them on |
| `trust.json` | Which of your own devices you have paired with, if you turned Wi-Fi sync on |
| Offline cache | Tracks pulled from your server so they play without it |
| Downloads | Tracks you asked to keep |
| Cover art | Artwork, from your files and from lookups |

There are a few more — the search index, a list of files that failed to analyse,
playback stalls, records of what you deleted. Rather than trust this list, press
**Settings → Your data → Open folder** and look. Everything in there is plain
JSON.

**Settings → Your data → Delete everything stored here** removes all of it,
plus the WebDAV password from the OS credential store. It does not touch your
music files themselves.

---

## 6. Checking this yourself

The repository is public. These are the searches behind the claims above:

- **Every host the app can contact:** `grep -rn "https\?://" --include='*.rs'
  vapor-app/src-tauri/src`. The only hosts actually requested are
  `lrclib.net`, `api.deezer.com`, `cdn-images.dzcdn.net` and the WebDAV address
  you typed. That search also returns two strings that are not requests, and
  they are named here so the list can be checked without a puzzle:
  `github.com` appears inside the `User-Agent` the app sends, and
  `app.koofr.net` is an example in the help text beside the server field. The
  rest are test fixtures — `example.com`, `example.invalid` and similar. The
  updater's endpoint is in `vapor-app/src-tauri/tauri.conf.json`.
- **No telemetry:** searching the whole of `vapor-app/src`,
  `vapor-app/src-tauri/src`, `vapor-core`, `package.json` and `Cargo.toml` for
  `sentry`, `posthog`, `mixpanel`, `amplitude`, `segment`, `analytics`,
  `telemetry`, `crashlytics`, `bugsnag`, `firebase`, `google-analytics`,
  `gtag`, `datadog` and `opentelemetry` returns no dependency and no call site.
  The only occurrences of the word "telemetry" are two comments explaining that
  there is none.
- **No account:** searching for `oauth` returns nothing; every hit for
  `account`, `login` and `sign in` is about the WebDAV server you configured.
- **The defaults:** `metadata_lookup_enabled: false` and `sync_enabled: false`
  in the `Default` implementation in
  `vapor-core/crates/vapor-library/src/settings.rs`.
- **The webview cannot open connections of its own:** the content security
  policy in `vapor-app/src-tauri/tauri.conf.json` is `default-src 'self'` with
  no remote origin permitted anywhere in it.

---

## Changes

This file is versioned in the repository, so every change to it is in the
commit history with a date and a reason.

## Reporting a problem

See [SUPPORT.md](SUPPORT.md).
