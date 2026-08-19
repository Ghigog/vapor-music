package com.dylangrowcoot.vapormusic

import android.os.Build
import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  /**
   * Let the back gesture walk the app's own history.
   *
   * `WryActivity` defaults this to true, and `TauriActivity` turns it off. With
   * it off, back goes straight past the app to the launcher — Settings was a
   * screen you left by killing the app. With it on, back calls
   * `webView.goBack()` while there is anywhere to go and finishes the activity
   * when there is not, which is what the gesture means everywhere else on the
   * phone. The entries it walks are pushed by `App.tsx`.
   */
  override val handleBackNavigation: Boolean = true

  /**
   * Hand the JavaVM and the application Context to `ndk_context`.
   *
   * Neither Tauri nor wry depends on `ndk_context`, so nothing was publishing
   * those handles — and `cpal` reads them to open an audio stream through Oboe,
   * and the credential store reads them to reach the Keystore. Without this the
   * app starts, renders, and has no sound and nowhere to keep a password, with
   * one line in logcat to say why.
   *
   * The Rust symbol is `Java_com_dylangrowcoot_vapormusic_MainActivity_setupNdkContext`
   * in `src/android.rs`. Renaming this method or this package breaks the link
   * silently at build time and loudly at startup.
   */
  private external fun setupNdkContext()

  /**
   * Ask for the notification permission, once, on 33 and up.
   *
   * The foreground service runs either way — which is the half that matters,
   * because it is what stops Android suspending the process and starving the
   * audio thread. Without the permission it simply has nothing to draw, so the
   * transport in the shade and on the lock screen is missing and nothing else
   * is. Asked here rather than at the first play so the answer is already in
   * when a set starts.
   */
  private fun askAboutNotifications() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
    val granted = checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) ==
      android.content.pm.PackageManager.PERMISSION_GRANTED
    if (!granted) {
      requestPermissions(arrayOf(android.Manifest.permission.POST_NOTIFICATIONS), 1)
    }
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    askAboutNotifications()
    // Before `super`, which is where Tauri builds the app and starts the audio
    // device. Anything that reads `ndk_context` after this point finds it set.
    setupNdkContext()
    super.onCreate(savedInstanceState)
  }

  companion object {
    init {
      // TauriActivity loads this too; a second call is a no-op. Named here
      // because `setupNdkContext` above is resolved out of it, and relying on a
      // superclass's static initialiser having already run is a race nobody
      // needs.
      System.loadLibrary("vapor_app_lib")
    }
  }
}
