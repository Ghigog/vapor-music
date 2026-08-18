//! Publishing the Android runtime handles that the rest of the app needs.
//!
//! ## The bug this exists to fix
//!
//! `ndk_context` is a tiny crate holding two pointers — the `JavaVM` and the
//! app `Context` — in a process-wide static, so that library code deep in a
//! dependency tree can reach the Android runtime without every caller threading
//! a JNI handle through. `cpal`'s Android backend reads it to open an audio
//! stream through Oboe, and [`crate::secrets::android`] reads it to reach the
//! Keystore.
//!
//! Something has to *put* the pointers there, and in this app nothing did.
//! Neither `tauri` nor `wry` depends on `ndk_context` at all — they carry their
//! own JNI plumbing — so on the first launch on a real device the audio thread
//! panicked at `ndk-context-0.1.1/src/lib.rs:72` with "android context was not
//! initialized", the panic was reported as "the audio thread stopped before
//! reporting", and the app came up perfectly with no sound and no way to save a
//! password.
//!
//! It compiled, on four platforms and in CI, for a day before that. Nothing
//! about it is visible from a build; the whole failure lives in what is missing
//! at runtime.
//!
//! ## How it is fixed
//!
//! `MainActivity.onCreate` calls straight down into [`Java_com_dylangrowcoot_vapormusic_MainActivity_setupNdkContext`],
//! which takes the `JavaVM` and the *application* context from the JNI
//! environment it was handed and publishes both. It runs before Tauri builds
//! anything, so every later caller finds the handles already in place.

use jni::objects::JObject;
use jni::JNIEnv;

/// Publish the `JavaVM` and application `Context` for `ndk_context`.
///
/// Called from `MainActivity.onCreate` in `gen/android`. The name is the JNI
/// mangling of `com.dylangrowcoot.vapormusic.MainActivity.setupNdkContext`, so
/// **renaming the package or the method breaks this silently**: the Kotlin side
/// throws `UnsatisfiedLinkError` at startup, and audio and the credential store
/// go back to being quietly unavailable.
///
/// # Safety
///
/// Called by the JVM with a valid environment and `this`. It publishes raw
/// pointers into a process-wide static, where they must outlive every reader —
/// which is why the context is a leaked global reference rather than a local
/// one. A local reference is freed when this returns, and the static would then
/// hold a dangling pointer that the audio thread dereferences.
#[no_mangle]
pub extern "system" fn Java_com_dylangrowcoot_vapormusic_MainActivity_setupNdkContext(
    env: JNIEnv<'_>,
    activity: JObject<'_>,
) {
    if let Err(e) = publish(env, activity) {
        // Not a panic. This runs on the UI thread inside `onCreate`, and an
        // unwind through a JNI boundary aborts the process — the app would fail
        // to start at all rather than start without sound.
        eprintln!("android: could not publish the runtime handles: {e}");
    }
}

fn publish(mut env: JNIEnv<'_>, activity: JObject<'_>) -> Result<(), String> {
    let vm = env.get_java_vm().map_err(|e| e.to_string())?;

    // The *application* context, not the activity. An activity is destroyed and
    // recreated on every rotation, and a global reference to one would both
    // leak it and leave the static pointing at a dead object; the application
    // outlives the process's use of it by definition.
    let context = env
        .call_method(
            &activity,
            "getApplicationContext",
            "()Landroid/content/Context;",
            &[],
        )
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;

    let context = env.new_global_ref(context).map_err(|e| e.to_string())?;
    let raw = context.as_raw();

    // SAFETY: both pointers outlive every reader. The `JavaVM` is
    // process-wide, and the global reference is deliberately never dropped —
    // see below.
    unsafe {
        ndk_context::initialize_android_context(vm.get_java_vm_pointer().cast(), raw.cast());
    }

    // Leaked on purpose. Dropping the `GlobalRef` tells the JVM the reference
    // is finished with, and `ndk_context` would be left holding a pointer to a
    // collected object. There is exactly one of these per process.
    std::mem::forget(context);
    Ok(())
}
