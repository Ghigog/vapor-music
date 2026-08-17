//! The credential store on Android.
//!
//! ## Why this is hand-written JNI
//!
//! Android has no OS credential store of the kind `keyring` wraps. There is no
//! "put this password somewhere the system looks after" API. What it has is the
//! **Android Keystore**: a place to keep a *key* that hardware-backs where the
//! device allows it and that never leaves the system process. Everything else —
//! where the ciphertext lives, how it is framed — is the app's problem.
//!
//! So: one AES-256-GCM key in the Keystore under [`KEY_ALIAS`], and the
//! encrypted password in the app's own `SharedPreferences`. The key material is
//! never in this process's memory, which is the property that makes this worth
//! doing rather than writing the password to app-private storage and trusting
//! the sandbox.
//!
//! The encryption happens inside `javax.crypto.Cipher` rather than in Rust,
//! because a Keystore key cannot be exported — using it means calling through
//! to Java. That is what the JNI below is: a transcription of about fifteen
//! lines of Java.
//!
//! ## What is verified and what is not
//!
//! **Compiled**, on every push: the `android (compile)` job in `app.yml` builds
//! this for `aarch64-linux-android`, and the `android-check` feature compiles it
//! on the desktop runners too, so the JNI signatures below are type-checked
//! everywhere rather than only when someone has an NDK.
//!
//! **Not verified**: that it works on a device. Nothing here has run against a
//! real Android runtime — there is no emulator in CI and no NDK on the
//! development machine as of 2026-08-17. JNI is exactly the kind of code where
//! a compiling method signature and a correct one are different things, so
//! treat first-run on hardware as the real test, and read
//! [`record::parse`]'s tests for the part that *is* covered.
//!
//! ## The threat model, stated so it can be argued with
//!
//! This protects the password against another app on the device and against
//! offline inspection of the app's data directory. It does not protect against
//! a rooted device with the app's own UID, because at that point the attacker
//! can ask the Keystore to decrypt exactly as this code does. `setUserAuthenticationRequired`
//! would raise that bar and would also mean the app cannot refresh a library in
//! the background, which is what this app does; that trade is worth revisiting
//! if a device ever holds more than one person's account.

use super::Error;
use jni::objects::{JObject, JString, JValue};
use jni::JavaVM;

/// The Keystore alias holding the app's one wrapping key.
///
/// Versioned in the name. A key cannot be migrated in place — if the parameters
/// ever change, the new key gets a new alias and anything sealed under the old
/// one is unreadable, which must be a deliberate act rather than a surprise.
const KEY_ALIAS: &str = "vapor.secrets.v1";

/// The `SharedPreferences` file the ciphertext lives in.
const PREFS: &str = "vapor.secrets";

/// `Context.MODE_PRIVATE`.
const MODE_PRIVATE: i32 = 0;

/// `KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT`.
const PURPOSE_ENCRYPT_DECRYPT: i32 = 1 | 2;

/// `Cipher.ENCRYPT_MODE` and `Cipher.DECRYPT_MODE`.
const ENCRYPT_MODE: i32 = 1;
const DECRYPT_MODE: i32 = 2;

/// GCM tag length in bits, as `GCMParameterSpec` wants it.
const TAG_BITS: i32 = 128;

/// The stored record: how the nonce and the ciphertext share one string.
///
/// Split out because it is the only part of this file that can be tested
/// without a device, and because a framing bug and a JNI bug look identical
/// from the outside — one of them can at least be ruled out here.
pub mod record {
    use base64::Engine as _;

    const B64: base64::engine::general_purpose::GeneralPurpose =
        base64::engine::general_purpose::STANDARD_NO_PAD;

    /// `v1:<nonce>:<ciphertext>`, both base64.
    ///
    /// A version prefix on a format that has exactly one version, because the
    /// alternative is discovering on the day it changes that old records are
    /// indistinguishable from new ones.
    pub fn format(iv: &[u8], ciphertext: &[u8]) -> String {
        format!("v1:{}:{}", B64.encode(iv), B64.encode(ciphertext))
    }

    /// The inverse. `None` for anything this version does not understand,
    /// rather than an error: an unreadable record and an absent one lead to the
    /// same place — asking for the password again.
    pub fn parse(stored: &str) -> Option<(Vec<u8>, Vec<u8>)> {
        let mut parts = stored.split(':');
        if parts.next()? != "v1" {
            return None;
        }
        let iv = B64.decode(parts.next()?).ok()?;
        let ciphertext = B64.decode(parts.next()?).ok()?;
        if parts.next().is_some() || iv.is_empty() || ciphertext.is_empty() {
            return None;
        }
        Some((iv, ciphertext))
    }

    /// The `SharedPreferences` key for one credential.
    ///
    /// Both halves are base64 rather than concatenated raw. `SharedPreferences`
    /// serialises to XML, a username can hold anything a person can type, and a
    /// stray control character would produce a preferences file the platform
    /// cannot read back — taking every other credential in it down too.
    pub fn key(service: &str, username: &str) -> String {
        format!(
            "{}.{}",
            B64.encode(service.as_bytes()),
            B64.encode(username.as_bytes())
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_record_survives_the_round_trip() {
            let iv = vec![9u8; 12];
            let ct = vec![7u8, 0, 255, 1];
            let (back_iv, back_ct) = parse(&format(&iv, &ct)).expect("parses");
            assert_eq!(back_iv, iv);
            assert_eq!(back_ct, ct);
        }

        /// Every rejection is a case that would otherwise reach `Cipher` as
        /// garbage and come back as an exception rather than as "ask again".
        #[test]
        fn nonsense_is_not_a_record() {
            for bad in [
                "",
                "v1",
                "v1:",
                "v1:only-one-part",
                "v2:AAAA:AAAA",
                "v1:AAAA:AAAA:AAAA",
                "v1:!!!!:AAAA",
                "v1::AAAA",
                "v1:AAAA:",
            ] {
                assert!(parse(bad).is_none(), "{bad:?} was accepted as a record");
            }
        }

        /// A username with an XML control character must not be able to corrupt
        /// the preferences file that every other credential shares.
        #[test]
        fn a_key_is_always_plain_text() {
            let key = key("svc", "some\u{1}one\u{c}@example.com");
            assert!(
                key.chars().all(|c| c.is_ascii_graphic()),
                "a control character reached the preferences key: {key:?}"
            );
        }

        #[test]
        fn different_accounts_get_different_keys() {
            assert_ne!(key("a", "b"), key("b", "a"));
            assert_ne!(key("s", "one@example.com"), key("s", "two@example.com"));
        }
    }
}

/// The JVM and the app `Context`, from whatever set up the Android runtime.
///
/// `wry`'s `android_binding!` initialises `ndk_context` during startup, which
/// is also where `cpal`'s Oboe backend gets its handle — so if this is missing,
/// audio is broken too and the credential store is not the first thing anyone
/// will notice.
fn runtime() -> Result<(JavaVM, JObject<'static>), Error> {
    let ctx = ndk_context::android_context();
    if ctx.vm().is_null() || ctx.context().is_null() {
        return Err(Error(
            "the Android runtime is not initialised yet — no JVM to reach the keystore through"
                .into(),
        ));
    }
    // SAFETY: both pointers come from `ndk_context`, which holds the values
    // handed to it by the Android entry point and outlives every caller here.
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.map_err(jni_error)?;
    let context = unsafe { JObject::from_raw(ctx.context().cast()) };
    Ok((vm, context))
}

fn jni_error(e: jni::errors::Error) -> Error {
    Error(format!("Android keystore: {e}"))
}

/// Run a JNI call, then turn any pending Java exception into an error.
///
/// A macro rather than a plain call so the call is evaluated and bound before
/// `env` is borrowed again: `check_result(env, env.call_method(..))` borrows it
/// twice in one expression and does not compile.
macro_rules! check {
    ($env:expr, $call:expr, $doing:expr $(,)?) => {{
        let outcome = $call;
        check_result($env, outcome, $doing)
    }};
}

/// Clear a pending Java exception and turn it into an error.
///
/// Necessary rather than tidy: with an exception pending, the *next* JNI call
/// aborts the process. Every fallible step below goes through this.
fn check_result<'a, T>(
    env: &mut jni::JNIEnv<'a>,
    result: Result<T, jni::errors::Error>,
    doing: &str,
) -> Result<T, Error> {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
        return Err(Error(format!("Android keystore: {doing} threw")));
    }
    result.map_err(|e| Error(format!("Android keystore: {doing}: {e}")))
}

/// `new String[] { .. }`, for the two builder calls that want one.
fn string_array<'a>(env: &mut jni::JNIEnv<'a>, values: &[&str]) -> Result<JObject<'a>, Error> {
    let first = env
        .new_string(values.first().copied().unwrap_or_default())
        .map_err(jni_error)?;
    let array = check!(
        env,
        env.new_object_array(values.len() as i32, "java/lang/String", &first),
        "allocating a String[]",
    )?;
    for (i, value) in values.iter().enumerate().skip(1) {
        let item = env.new_string(value).map_err(jni_error)?;
        check!(
            env,
            env.set_object_array_element(&array, i as i32, &item),
            "filling a String[]",
        )?;
    }
    Ok(array.into())
}

/// The app's wrapping key, generating it on first use.
///
/// `containsAlias` then `generateKey` is a race in principle — two threads
/// could both find it absent. In practice `KeyGenerator.generateKey` with an
/// existing alias replaces it, which would strand any record sealed under the
/// old key. Credential writes come from command handlers on one runtime, so
/// this is documented rather than locked; if that ever stops being true, this
/// wants a mutex, not a comment.
fn wrapping_key<'a>(env: &mut jni::JNIEnv<'a>) -> Result<JObject<'a>, Error> {
    let store_name = env.new_string("AndroidKeyStore").map_err(jni_error)?;
    let store = check!(
        env,
        env.call_static_method(
            "java/security/KeyStore",
            "getInstance",
            "(Ljava/lang/String;)Ljava/security/KeyStore;",
            &[JValue::Object(&store_name)],
        ),
        "KeyStore.getInstance",
    )?
    .l()
    .map_err(jni_error)?;

    check!(
        env,
        env.call_method(
            &store,
            "load",
            "(Ljava/security/KeyStore$LoadStoreParameter;)V",
            &[JValue::Object(&JObject::null())],
        ),
        "KeyStore.load",
    )?;

    let alias = env.new_string(KEY_ALIAS).map_err(jni_error)?;
    let present = check!(
        env,
        env.call_method(
            &store,
            "containsAlias",
            "(Ljava/lang/String;)Z",
            &[JValue::Object(&alias)],
        ),
        "KeyStore.containsAlias",
    )?
    .z()
    .map_err(jni_error)?;

    if !present {
        generate_key(env)?;
    }

    let alias = env.new_string(KEY_ALIAS).map_err(jni_error)?;
    let key = check!(
        env,
        env.call_method(
            &store,
            "getKey",
            "(Ljava/lang/String;[C)Ljava/security/Key;",
            &[JValue::Object(&alias), JValue::Object(&JObject::null())],
        ),
        "KeyStore.getKey",
    )?
    .l()
    .map_err(jni_error)?;

    if key.is_null() {
        return Err(Error(
            "Android keystore: the key vanished between being created and being read".into(),
        ));
    }
    Ok(key)
}

/// `KeyGenerator.getInstance("AES", "AndroidKeyStore")` with GCM parameters.
fn generate_key(env: &mut jni::JNIEnv<'_>) -> Result<(), Error> {
    let algorithm = env.new_string("AES").map_err(jni_error)?;
    let provider = env.new_string("AndroidKeyStore").map_err(jni_error)?;
    let generator = check!(
        env,
        env.call_static_method(
            "javax/crypto/KeyGenerator",
            "getInstance",
            "(Ljava/lang/String;Ljava/lang/String;)Ljavax/crypto/KeyGenerator;",
            &[JValue::Object(&algorithm), JValue::Object(&provider)],
        ),
        "KeyGenerator.getInstance",
    )?
    .l()
    .map_err(jni_error)?;

    let alias = env.new_string(KEY_ALIAS).map_err(jni_error)?;
    let builder = check!(
        env,
        env.new_object(
            "android/security/keystore/KeyGenParameterSpec$Builder",
            "(Ljava/lang/String;I)V",
            &[JValue::Object(&alias), JValue::Int(PURPOSE_ENCRYPT_DECRYPT),],
        ),
        "KeyGenParameterSpec.Builder",
    )?;

    let modes = string_array(env, &["GCM"])?;
    check!(
        env,
        env.call_method(
            &builder,
            "setBlockModes",
            "([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
            &[JValue::Object(&modes)],
        ),
        "setBlockModes",
    )?;

    let paddings = string_array(env, &["NoPadding"])?;
    check!(
        env,
        env.call_method(
            &builder,
            "setEncryptionPaddings",
            "([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
            &[JValue::Object(&paddings)],
        ),
        "setEncryptionPaddings",
    )?;

    let spec = check!(
        env,
        env.call_method(
            &builder,
            "build",
            "()Landroid/security/keystore/KeyGenParameterSpec;",
            &[],
        ),
        "KeyGenParameterSpec.build",
    )?
    .l()
    .map_err(jni_error)?;

    check!(
        env,
        env.call_method(
            &generator,
            "init",
            "(Ljava/security/spec/AlgorithmParameterSpec;)V",
            &[JValue::Object(&spec)],
        ),
        "KeyGenerator.init",
    )?;
    check!(
        env,
        env.call_method(&generator, "generateKey", "()Ljavax/crypto/SecretKey;", &[]),
        "KeyGenerator.generateKey",
    )?;
    Ok(())
}

fn cipher<'a>(env: &mut jni::JNIEnv<'a>) -> Result<JObject<'a>, Error> {
    let transformation = env.new_string("AES/GCM/NoPadding").map_err(jni_error)?;
    check!(
        env,
        env.call_static_method(
            "javax/crypto/Cipher",
            "getInstance",
            "(Ljava/lang/String;)Ljavax/crypto/Cipher;",
            &[JValue::Object(&transformation)],
        ),
        "Cipher.getInstance",
    )?
    .l()
    .map_err(jni_error)
}

/// `context.getSharedPreferences(PREFS, MODE_PRIVATE)`.
fn prefs<'a>(env: &mut jni::JNIEnv<'a>, context: &JObject<'a>) -> Result<JObject<'a>, Error> {
    let name = env.new_string(PREFS).map_err(jni_error)?;
    check!(
        env,
        env.call_method(
            context,
            "getSharedPreferences",
            "(Ljava/lang/String;I)Landroid/content/SharedPreferences;",
            &[JValue::Object(&name), JValue::Int(MODE_PRIVATE)],
        ),
        "getSharedPreferences",
    )?
    .l()
    .map_err(jni_error)
}

pub fn set(service: &str, username: &str, password: &str) -> Result<(), Error> {
    let (vm, context) = runtime()?;
    let mut env = vm.attach_current_thread().map_err(jni_error)?;

    let key = wrapping_key(&mut env)?;
    let cipher = cipher(&mut env)?;
    check!(
        &mut env,
        env.call_method(
            &cipher,
            "init",
            "(ILjava/security/Key;)V",
            &[JValue::Int(ENCRYPT_MODE), JValue::Object(&key)],
        ),
        "Cipher.init (encrypt)",
    )?;

    // The nonce comes from the Cipher rather than from us. GCM is catastrophic
    // on nonce reuse, and the platform's own generator is the one thing here
    // guaranteed not to repeat under this key.
    let iv = check!(
        &mut env,
        env.call_method(&cipher, "getIV", "()[B", &[]),
        "Cipher.getIV",
    )?
    .l()
    .map_err(jni_error)?;
    let iv = env
        .convert_byte_array(unsafe { jni::objects::JByteArray::from_raw(iv.into_raw()) })
        .map_err(jni_error)?;

    let plain = env
        .byte_array_from_slice(password.as_bytes())
        .map_err(jni_error)?;
    let sealed = check!(
        &mut env,
        env.call_method(&cipher, "doFinal", "([B)[B", &[JValue::Object(&plain)]),
        "Cipher.doFinal (encrypt)",
    )?
    .l()
    .map_err(jni_error)?;
    let sealed = env
        .convert_byte_array(unsafe { jni::objects::JByteArray::from_raw(sealed.into_raw()) })
        .map_err(jni_error)?;

    let prefs = prefs(&mut env, &context)?;
    let editor = check!(
        &mut env,
        env.call_method(
            &prefs,
            "edit",
            "()Landroid/content/SharedPreferences$Editor;",
            &[],
        ),
        "SharedPreferences.edit",
    )?
    .l()
    .map_err(jni_error)?;

    let entry_key = env
        .new_string(record::key(service, username))
        .map_err(jni_error)?;
    let value = env
        .new_string(record::format(&iv, &sealed))
        .map_err(jni_error)?;
    check!(
        &mut env,
        env.call_method(
            &editor,
            "putString",
            "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;",
            &[JValue::Object(&entry_key), JValue::Object(&value)],
        ),
        "Editor.putString",
    )?;
    commit(&mut env, &editor)
}

pub fn get(service: &str, username: &str) -> Result<Option<String>, Error> {
    let (vm, context) = runtime()?;
    let mut env = vm.attach_current_thread().map_err(jni_error)?;

    let prefs = prefs(&mut env, &context)?;
    let entry_key = env
        .new_string(record::key(service, username))
        .map_err(jni_error)?;
    let stored = check!(
        &mut env,
        env.call_method(
            &prefs,
            "getString",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(&entry_key), JValue::Object(&JObject::null())],
        ),
        "SharedPreferences.getString",
    )?
    .l()
    .map_err(jni_error)?;

    if stored.is_null() {
        return Ok(None);
    }
    let stored: String = env
        .get_string(&JString::from(stored))
        .map_err(jni_error)?
        .into();
    let Some((iv, sealed)) = record::parse(&stored) else {
        // A record this version cannot read is the same answer as no record:
        // ask for the password again. Returning an error here would put an
        // unfixable message on the Settings screen instead.
        return Ok(None);
    };

    let key = wrapping_key(&mut env)?;
    let cipher = cipher(&mut env)?;
    let iv_array = env.byte_array_from_slice(&iv).map_err(jni_error)?;
    let spec = check!(
        &mut env,
        env.new_object(
            "javax/crypto/spec/GCMParameterSpec",
            "(I[B)V",
            &[JValue::Int(TAG_BITS), JValue::Object(&iv_array)],
        ),
        "GCMParameterSpec",
    )?;
    check!(
        &mut env,
        env.call_method(
            &cipher,
            "init",
            "(ILjava/security/Key;Ljava/security/spec/AlgorithmParameterSpec;)V",
            &[
                JValue::Int(DECRYPT_MODE),
                JValue::Object(&key),
                JValue::Object(&spec),
            ],
        ),
        "Cipher.init (decrypt)",
    )?;

    let sealed_array = env.byte_array_from_slice(&sealed).map_err(jni_error)?;
    let plain = check!(
        &mut env,
        env.call_method(
            &cipher,
            "doFinal",
            "([B)[B",
            &[JValue::Object(&sealed_array)],
        ),
        "Cipher.doFinal (decrypt)",
    )?
    .l()
    .map_err(jni_error)?;
    let plain = env
        .convert_byte_array(unsafe { jni::objects::JByteArray::from_raw(plain.into_raw()) })
        .map_err(jni_error)?;

    String::from_utf8(plain)
        .map(Some)
        .map_err(|_| Error("Android keystore: the stored password is not text".into()))
}

pub fn delete(service: &str, username: &str) -> Result<(), Error> {
    let (vm, context) = runtime()?;
    let mut env = vm.attach_current_thread().map_err(jni_error)?;

    let prefs = prefs(&mut env, &context)?;
    let editor = check!(
        &mut env,
        env.call_method(
            &prefs,
            "edit",
            "()Landroid/content/SharedPreferences$Editor;",
            &[],
        ),
        "SharedPreferences.edit",
    )?
    .l()
    .map_err(jni_error)?;

    let entry_key = env
        .new_string(record::key(service, username))
        .map_err(jni_error)?;
    check!(
        &mut env,
        env.call_method(
            &editor,
            "remove",
            "(Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;",
            &[JValue::Object(&entry_key)],
        ),
        "Editor.remove",
    )?;
    commit(&mut env, &editor)
}

/// `commit()`, not `apply()`.
///
/// `apply()` returns immediately and writes on a background thread, which would
/// make `save_password` return before the password is on disk — and the very
/// next thing this app does after saving is read it back to answer "is a
/// password saved?". That is the TD-50 shape again: a write that reports
/// success before it has happened.
fn commit(env: &mut jni::JNIEnv<'_>, editor: &JObject<'_>) -> Result<(), Error> {
    let committed = check!(env, env.call_method(editor, "commit", "()Z", &[]), "commit")?
        .z()
        .map_err(jni_error)?;
    if committed {
        Ok(())
    } else {
        Err(Error(
            "Android keystore: the preferences file refused the write".into(),
        ))
    }
}
