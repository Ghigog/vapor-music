# Android

What is done, what is not, and what to run.

## What this covers

Everything here is a decision or an instruction. **Status — what is built, what
is verified, what is not — lives in `docs/workspace/tickets.md` under AND-1**,
because a status table in a guide is stale the first time someone acts on the
guide.

One thing is worth stating flatly, because it is a claim about a kind of
evidence rather than about a date: **a compile is not a run.** The Android job
in `app.yml` proves the code builds, and the first launch on real hardware still
found a fault no build could have — nothing was publishing the Android runtime
handles, so the app started, rendered, and had no sound (AND-4). Keep the two
columns separate.

## The machine this was written on

macOS, Android Studio installed, SDK platform 36, build-tools 37, and **NDK
30.0.15729638** at `$ANDROID_HOME/ndk/`. A local `--debug --target aarch64`
build was produced and installed on 2026-08-21. This section previously recorded
no NDK, which was true when it was written.

Without one nothing compiles for Android at all: every C dependency (`ring`
first) is built by the `cc` crate, which goes looking for a cross-compiler and
stops when there is none.

CI checks it too, and that is worth keeping rather than treating as a fallback
now that the machine can build: GitHub's Ubuntu runners ship an NDK at
`$ANDROID_NDK_LATEST_HOME`, so the compile is checked on every push by a machine
whose toolchain nobody has adjusted by hand, and the JNI is type-checked on all
three desktop runners besides — see `--features android-check` below.

## The environment

If the NDK is ever missing, Android Studio installs one: **Settings → Languages
& Frameworks → Android SDK → SDK Tools**, tick **NDK (Side by side)** and
**Android SDK Command-line Tools**.

Per shell:

```bash
export ANDROID_HOME="$HOME/Library/Android/sdk"
export NDK_HOME="$ANDROID_HOME/ndk/$(ls "$ANDROID_HOME/ndk" | sort -V | tail -1)"
export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
```

`JAVA_HOME` matters: the system `java` on this machine is 8, and the Android
Gradle Plugin needs 17 or newer. Android Studio bundles 21.

## First bring-up

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
```

```bash
cd vapor-app && npm run tauri android init
```

`init` writes the Gradle project to `gen/android`, which is **in the tree** —
`.gitignore` covers `gen/schemas` only. Running `init` again on a checkout that
already has it will overwrite hand-made changes, so read the diff before
accepting one. Two of those changes are decisions rather than defaults:

- **`android:allowBackup="false"`.** The credential store keeps its ciphertext
  in `SharedPreferences`, and the key that opens it is in the Keystore, which is
  never backed up. A restored backup therefore carries an unreadable record —
  handled, since an unparseable record reads as "no password saved" — but
  enrolling a file in a backup it cannot survive is not honest.
- **`minSdk = 24`.** `KeyGenParameterSpec` is API 23, so 24 clears it. Left at
  the template's value; noted because lowering it would break the credential
  store silently.

Then:

```bash
cd vapor-app && npm run tauri android dev
```

Or, to produce an installable APK without a device attached:

```bash
cd vapor-app && npm run build && npm run tauri android build -- --debug --target aarch64
```

It lands at
`src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`,
with an AAB beside it. The debug APK is around 570 MB — unstripped Rust
debuginfo, not a packaging fault — and installs as
`com.dylangrowcoot.vapormusic.debug`, alongside a release build rather than over
it:

```bash
~/Library/Android/sdk/platform-tools/adb install -r <apk>
```

## A release APK

Added 2026-08-24, for the first build that goes to somebody who is not Dylan.
Everything above this section is the debug loop and is unchanged by it.

Two things are different from the debug build, and both matter more than they
look:

* **It is signed with a real keystore**, so it installs as
  `com.dylangrowcoot.vapormusic` rather than `...vapormusic.debug`, and a later
  build installs *over* it instead of beside it. The signing config lives in
  `gen/android/build.gradle.kts` — the **root** file, because the CLI rewrites
  `app/build.gradle.kts` from its template on every build (AND-3) and a signing
  block there would disappear silently, leaving a green build that produced an
  unsigned APK.
* **R8 runs.** `isMinifyEnabled = true` on the release build type, which no
  Android build here has ever hit. `app/proguard-rules.pro` keeps
  `com.dylangrowcoot.vapormusic.**` whole and every `native` method name,
  because `MainActivity` and `PlaybackService` are reached from Rust over JNI by
  string and R8 will happily rename a method on a class it is keeping. If a
  release APK ever starts, renders, and has no sound, that is the first thing
  to look at — it is AND-4's symptom arriving by a different road.

CI builds it: `.github/workflows/release.yml`, `Android (arm64)`, on a
`v*.*.*` tag. **arm64 only** — armv7 roughly doubles the Rust half for devices
nobody testing this has, and x86_64 is emulators.

### R8 is off, and what that cost to learn

`v2.0.0-rc.3` was the first release APK this project ever produced. It
installed and crashed on launch, and the release build type is now
`isMinifyEnabled = false`, set in the **root** `gen/android/build.gradle.kts`
because the CLI rewrites `app/build.gradle.kts` (AND-3).

What was checked against that APK before deciding, because "it must be R8" is a
guess until something looks:

| Ruled out | How |
|---|---|
| JNI symbols broken | All 25 `Java_com_dylangrowcoot_vapormusic_*` exports have a matching class and method in the DEX |
| Classes resolved by `find_class` gone | `app/tauri/plugin/Plugin` and `PluginManager` both present |
| Frontend missing | `/index.html` and `/assets/index-*.js` are embedded in `libvapor_app_lib.so`, as Tauri v2 does |
| Wrong or missing native library | `lib/arm64-v8a/` has `libvapor_app_lib.so` and `libc++_shared.so` |

So the keep rules in `app/proguard-rules.pro` did their job, and the cause is
something they were never going to cover. The likeliest remaining mechanism is
**field** renaming: `app.tauri.plugin.Config`, `CommandData` and
`RegisterListenerArgs` are deserialised reflectively from the `tauri.conf.json`
in the APK's assets at startup, and R8 renames fields on classes it is keeping.

That is not confirmed — confirming it needs `mapping.txt` or logcat from a
device. It is written down because the next person to consider turning
minification back on should know the shape of what they are re-enabling, and
that a keep rule for classes and methods is not sufficient.

**Turning it back on is a task with a phone in someone's hand.** Not a line
changed on the way past.

**Corrected 2026-08-25: it cannot be set from this file at all.** Both attempts
failed, and the note below is kept because the reasoning was right and the
conclusion was wrong twice.

`isMinifyEnabled = true` lives in Tauri's own template, and the CLI rewrites
`app/build.gradle.kts` from that template on every build (AND-3). Assigning the
property from the root script in a `plugins.withId` callback loses to the
template, which runs moments later — that shipped `v2.0.0-rc.4`. Assigning it
from `afterEvaluate` loses to AGP, which registers its own `afterEvaluate` when
the plugin is applied and has created the variants before ours runs — that
failed `v2.0.0-rc.5`'s APK check.

**What works instead:** `-dontshrink`, `-dontoptimize` and `-dontobfuscate` in
`app/proguard-rules.pro`. R8 still runs and does nothing, which is the same
outcome by a route with no dependency on evaluation order. That file is not
regenerated by the CLI and the template already passes it to `proguardFiles`.

The original note, for the record:

**It has to be set from `afterEvaluate`, and the first attempt was not.**
`plugins.withId("com.android.application")` fires the instant the plugin is
applied — the first line of `app/build.gradle.kts` — so an assignment there runs
*before* that file's own `buildTypes { getByName("release") { isMinifyEnabled =
true } }`, and is overwritten by it. Nothing reports this: the build is green and
the APK comes out the same size as before. `v2.0.0-rc.4` shipped that way, with
R8 still on, and was found only by comparing the published APK's byte count
against the previous one. `applicationIdSuffix` above survives the same
lifecycle only because nothing else assigns it.

`verify-apk.mjs` now fails on any obfuscated class name, so the override going
quiet again turns the build red rather than shipping.

### `verify-apk.mjs`, and what it does not do

`vapor-app/scripts/verify-apk.mjs` runs in CI on every release APK. It pulls the
JNI symbol names out of the `.so` and the class and method names out of the DEX
and fails if they disagree — the check that turns "renamed a JNI target" from a
launch crash into a red build.

**It did not catch rc.3**, and it is documented here saying so. It is a floor,
not a gate: the only thing that proves an APK starts is starting it. The gap
that let rc.3 ship is that nothing in this repository has ever launched the
app — `app.yml` runs `cargo check` and `cargo clippy` for the Android target,
which prove the Rust compiles and nothing else.

To build one locally, the keystore has to exist and be described. `keytool`
command and the four CI secret names are in `docs/RELEASE.md` §1; locally it is
`gen/android/keystore.properties`, which `.gitignore` already covers:

```properties
storeFile=/Users/dylangrowcoot/.keys/vapor-upload.jks
storePassword=...
keyAlias=vapor
keyPassword=...
```

```bash
cd vapor-app && npm run build && npm run tauri android build -- --apk --target aarch64
```

Without that file Gradle configures no signing at all and the release build
fails saying so, which is deliberate: a hardcoded fallback is how a key nobody
chose ends up on an APK somebody installed.

## Checks that do not need a device

The whole Rust half, on any machine:

```bash
cd vapor-app/src-tauri && cargo check --features android-check
```

That compiles `src/secrets/android.rs` on the host. It is not a runtime feature
— nothing on a desktop calls into the module — it exists so the JNI method
signatures are type-checked where they were written rather than only where they
run. Its record-format tests run with the ordinary suite.

The real thing needs the toolchain pointed at explicitly. `NDK_HOME` alone is
not enough: `cc` looks for `aarch64-linux-android-clang`, the NDK ships only
API-suffixed names like `aarch64-linux-android24-clang`, and the build stops on
`ring`. The tauri CLI sets this up itself, which is why the APK builds while a
bare `cargo check` does not.

```bash
export PATH="$NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin:$PATH"
export CC_aarch64_linux_android="aarch64-linux-android24-clang"
export AR_aarch64_linux_android="llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="aarch64-linux-android24-clang"
cd vapor-app/src-tauri && cargo check --target aarch64-linux-android
```

24 is `minSdk`, set in `gen/android/app/build.gradle.kts` and noted above.

## What to distrust first

In rough order of how likely each is to be wrong on the first device:

1. **The JNI in `src/secrets/android.rs`.** A method signature that compiles and
   a method signature that resolves are different things; a wrong one throws
   `NoSuchMethodError` at runtime. Every call goes through `check!`, which
   clears the pending exception and reports which step threw, so the first
   failure should name itself rather than abort the process.
2. ~~**`ndk_context` not being initialised.**~~ This one was right, and it
   happened on the first launch: nothing published the handles, so there was no
   audio and there would have been no credential store. `src/android.rs`
   publishes them now (AND-4). If it ever regresses, the symptom is an app that
   starts and renders perfectly with no sound.
3. **Audio.** `cpal` on Android is, in its own words, the least battle-tested
   part of that crate, and this app asks more of it than most: a real-time
   thread that must not allocate. `oboe-shared-stdcxx` is a link-time choice
   that a `cargo check` cannot tell you about.
4. **Peer discovery.** `peers.rs` binds a UDP socket and sends multicast.
   Android needs `INTERNET` and `CHANGE_WIFI_MULTICAST_STATE` permissions and a
   held multicast lock; none of that is in the manifest yet, because there is no
   manifest yet. It is off by default (`syncEnabled`), so it should fail
   quietly rather than at startup.

## What is deliberately not done

**`setUserAuthenticationRequired` on the Keystore key.** It would mean the
password can only be decrypted just after the person unlocked the device, which
would stop the app refreshing a library in the background — which is what this
app does. The trade is worth revisiting if a device ever holds more than one
person's account; the reasoning is in the module's own header so it can be
argued with there.
