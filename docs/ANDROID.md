# Android

What is done, what is not, and what to run when the NDK is installed.

## What this covers

Everything here is a decision or an instruction. **Status — what is built, what
is verified, what is not — lives in `docs/workspace/tickets.md` under AND-1**,
because a status table in a guide is stale the first time someone acts on the
guide.

One thing is worth stating flatly and does not go stale, because it is a claim
about a kind of evidence rather than about a date: **no part of this has been
run on Android hardware or an emulator.** The verification is a compile, and
`app.yml` is where it happens.

## The machine this was written on

macOS, Android Studio installed, SDK platform 36, build-tools 37 — and **no
NDK**, and no `cmdline-tools`, so no `sdkmanager` to install one with. Nothing
compiles for Android without it: every C dependency (`ring` first) is built by
the `cc` crate, which looks for `aarch64-linux-android<api>-clang` and stops.

That is why the verification lives in CI. GitHub's Ubuntu runners ship an NDK at
`$ANDROID_NDK_LATEST_HOME`, so the compile is checked on every push by a machine
that has one, and the JNI is type-checked on all three desktop runners besides —
see `--features android-check` below.

## Installing the NDK

In Android Studio: **Settings → Languages & Frameworks → Android SDK → SDK
Tools**, tick **NDK (Side by side)** and **Android SDK Command-line Tools**.

Then, per shell:

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

## Checks that do not need a device

The whole Rust half, on any machine:

```bash
cd vapor-app/src-tauri && cargo check --features android-check
```

That compiles `src/secrets/android.rs` on the host. It is not a runtime feature
— nothing on a desktop calls into the module — it exists so the JNI method
signatures are type-checked where they were written rather than only where they
run. Its record-format tests run with the ordinary suite.

With an NDK, the real thing:

```bash
cd vapor-app/src-tauri && cargo check --target aarch64-linux-android
```

## What to distrust first

In rough order of how likely each is to be wrong on the first device:

1. **The JNI in `src/secrets/android.rs`.** A method signature that compiles and
   a method signature that resolves are different things; a wrong one throws
   `NoSuchMethodError` at runtime. Every call goes through `check!`, which
   clears the pending exception and reports which step threw, so the first
   failure should name itself rather than abort the process.
2. **`ndk_context` not being initialised.** `runtime()` reports this rather than
   dereferencing null. If it fires, audio is broken too — `cpal`'s Oboe backend
   takes its handle from the same place.
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
