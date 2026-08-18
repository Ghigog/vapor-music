package com.dylangrowcoot.vapormusic

import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
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

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
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
