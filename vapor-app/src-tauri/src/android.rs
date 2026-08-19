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

// Under `--features android-check` this module is compiled on a desktop, to
// prove the JNI code still builds without an NDK. Its callers are all
// `cfg(target_os = "android")` and are therefore *not* compiled there, so
// everything here reads as dead — which is the point of the check, not a fault
// in it.
#![cfg_attr(not(target_os = "android"), allow(dead_code))]

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

/// Whether the handles have already been published this process.
static PUBLISHED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn publish(mut env: JNIEnv<'_>, activity: JObject<'_>) -> Result<(), String> {
    /*
     * Once per process, whatever Android does with the activity.
     *
     * `ndk_context::initialize_android_context` asserts nothing was published
     * before it, and the assert lives in a function that cannot unwind — so a
     * second call does not return an error, it aborts the process.
     *
     * `onCreate` runs again whenever the activity is recreated, which this app
     * now makes ordinary: tapping the playback notification brings
     * `MainActivity` back, and a configuration change does the same. The app
     * died on launch with "core unreachable" because the process was being
     * killed underneath the webview.
     *
     * The handles are process-wide and identical every time, so the second
     * publish had nothing to add even before it was fatal.
     */
    if PUBLISHED.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return Ok(());
    }

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

    /*
     * Find `PlaybackService` now, while we are on a Java thread.
     *
     * `FindClass` resolves against the class loader of the *calling* frame. On
     * a thread the JVM created that is the app's loader and everything is
     * visible; on a thread Rust made and attached with
     * `attach_current_thread` there is no Java frame, so the JVM falls back to
     * the system loader — which knows the platform classes and nothing of this
     * app. Every call to the service was therefore made from the playback
     * supervisor or the analysis pass, on exactly such a thread, and every one
     * of them failed to find the class.
     *
     * That is why the notification never appeared while the pass plainly ran.
     * Cached here, from `onCreate`, where the lookup works.
     */
    match env.find_class(SERVICE_CLASS_NAME) {
        Ok(class) => match env.new_global_ref(class) {
            Ok(global) => {
                let _ = SERVICE_CLASS.set(global);
            }
            Err(e) => eprintln!("android: could not hold on to {SERVICE_CLASS_NAME}: {e}"),
        },
        Err(e) => eprintln!("android: could not find {SERVICE_CLASS_NAME}: {e}"),
    }

    Ok(())
}

const SERVICE_CLASS_NAME: &str = "com/dylangrowcoot/vapormusic/PlaybackService";

/// `PlaybackService`, looked up on a Java thread and kept.
static SERVICE_CLASS: std::sync::OnceLock<jni::objects::GlobalRef> = std::sync::OnceLock::new();

// ---------------------------------------------------------------------------
// The playback service
// ---------------------------------------------------------------------------

/// Tell `PlaybackService` what is playing, starting it if it is not running.
///
/// Android suspends a backgrounded process, and the audio thread is suspended
/// with it — the deck starves, which is heard as stuttering, and then the
/// process is frozen and the music stops. A foreground service is the only
/// thing that changes that. See `PlaybackService.kt`.
///
/// Everything here is best effort. A device that refuses to start the service
/// costs background playback and nothing else, and this is called four times a
/// second from the playback supervisor — a failure that logged every time would
/// be its own problem.
pub fn service_update(title: &str, artist: &str, playing: bool, position: f64, duration: f64) {
    report(with_context(|env, context| {
        let title = env.new_string(title)?;
        let artist = env.new_string(artist)?;
        env.call_static_method(
            service_class(),
            "update",
            "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;ZJJ)V",
            &[
                (&context).into(),
                (&title).into(),
                (&artist).into(),
                playing.into(),
                ((position.max(0.0) * 1000.0) as i64).into(),
                ((duration.max(0.0) * 1000.0) as i64).into(),
            ],
        )?;
        Ok(())
    }));
}

/// Stop the service. Nothing is playing, so nothing needs keeping alive.
pub fn service_stop() {
    report(with_context(|env, context| {
        env.call_static_method(
            service_class(),
            "stop",
            "(Landroid/content/Context;)V",
            &[(&context).into()],
        )?;
        Ok(())
    }));
}

/// The cached `PlaybackService` class.
///
/// Falls back to the name, which will fail on a Rust-made thread for the reason
/// given in `publish` — but failing loudly beats a silent no-op, and if the
/// cache is empty something has already gone wrong at startup.
fn service_class<'a>() -> jni::objects::JClass<'a> {
    match SERVICE_CLASS.get() {
        // SAFETY: the global reference is held for the life of the process, so
        // this borrowed class outlives every use of it. `JClass` does not own
        // the reference and dropping it releases nothing.
        Some(global) => unsafe { jni::objects::JClass::from_raw(global.as_raw()) },
        None => jni::objects::JClass::default(),
    }
}

/// Say when a call into the service failed.
///
/// These used to be `let _ = …`. The notification not appearing while the pass
/// visibly ran was this: every call was failing to find the class and no one
/// was told. Once per process is enough to find that out; repeating it four
/// times a second would be its own fault.
fn report(result: Result<(), jni::errors::Error>) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static SAID: AtomicBool = AtomicBool::new(false);
    if let Err(e) = result {
        if !SAID.swap(true, Ordering::Relaxed) {
            eprintln!("android: the playback service could not be reached: {e}");
        }
    }
}

/// Run `f` with a JNI environment attached to this thread and the app context.
///
/// The supervisor is a Rust thread the JVM has never heard of, so it has to
/// attach before it can call anything — `attach_current_thread` detaches again
/// when the guard drops, which is the behaviour wanted here: this is called
/// from one long-lived thread, not from many.
fn with_context<F>(f: F) -> Result<(), jni::errors::Error>
where
    F: FnOnce(&mut JNIEnv<'_>, JObject<'_>) -> Result<(), jni::errors::Error>,
{
    let ctx = ndk_context::android_context();
    // SAFETY: both pointers were published by `publish` above and are valid for
    // the life of the process.
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }?;
    let context = unsafe { JObject::from_raw(ctx.context().cast()) };
    let mut env = vm.attach_current_thread()?;
    let result = f(&mut env, context);
    // An exception left pending poisons the next JNI call on this thread, which
    // would be the next state update a quarter of a second later.
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    result
}

/// A button on the notification, the lock screen, a headset or a car.
///
/// The integers match `media::Press`. Implemented here rather than in
/// `media.rs` because the JNI name has to mangle to the Kotlin class that
/// declares it — renaming either side breaks this silently.
///
/// # Safety
///
/// Called by the JVM on its main thread with a valid environment.
#[no_mangle]
pub extern "system" fn Java_com_dylangrowcoot_vapormusic_PlaybackService_onMediaButton(
    _env: JNIEnv<'_>,
    _this: JObject<'_>,
    press: i32,
) {
    if let Some(handler) = PRESS_HANDLER.get() {
        handler(press);
    }
}

/// Where a press goes. Set once, when the media controls are attached.
static PRESS_HANDLER: std::sync::OnceLock<Box<dyn Fn(i32) + Send + Sync>> =
    std::sync::OnceLock::new();

/// Route notification and headset presses to `handler`.
pub fn on_press<F>(handler: F)
where
    F: Fn(i32) + Send + Sync + 'static,
{
    let _ = PRESS_HANDLER.set(Box::new(handler));
}

/// Tell the service a pass is running, and how far it has got.
///
/// Analysis is a long, download-bound job, and a backgrounded app is frozen —
/// so before this it stopped the moment someone switched to another app, with
/// nothing playing to keep the service up on playback's behalf.
pub fn service_analysis(done: usize, total: usize, active: bool) {
    report(with_context(|env, context| {
        env.call_static_method(
            service_class(),
            "analysis",
            "(Landroid/content/Context;IIZ)V",
            &[
                (&context).into(),
                (done as i32).into(),
                (total as i32).into(),
                active.into(),
            ],
        )?;
        Ok(())
    }));
}

/// Stop was pressed on the analysis notification.
///
/// # Safety
///
/// Called by the JVM on its main thread with a valid environment.
#[no_mangle]
pub extern "system" fn Java_com_dylangrowcoot_vapormusic_PlaybackService_onStopAnalysis(
    _env: JNIEnv<'_>,
    _this: JObject<'_>,
) {
    if let Some(handler) = STOP_ANALYSIS.get() {
        handler();
    }
}

static STOP_ANALYSIS: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();

/// Route the notification's Stop button to `handler`.
pub fn on_stop_analysis<F>(handler: F)
where
    F: Fn() + Send + Sync + 'static,
{
    let _ = STOP_ANALYSIS.set(Box::new(handler));
}
