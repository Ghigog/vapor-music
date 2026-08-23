fn main() {
    tauri_build::build();

    // Give the test binaries the manifest that only the app binary gets.
    //
    // `tauri_build::build()` writes a Windows resource — icon, version strings,
    // and an application manifest that declares a dependency on
    // Microsoft.Windows.Common-Controls 6.0.0.0 — and hands it to cargo through
    // `embed_resource::compile()`. That function emits
    // `cargo:rustc-link-arg-bins=`, and `-bins` means bins: the resource is
    // linked into `vapor-app.exe` and into nothing else. embed-resource has a
    // `compile_for_tests()` that emits `-tests` instead; tauri-build does not
    // call it, so `cargo test` produces an executable with no manifest at all.
    //
    // That was invisible until `tauri-plugin-dialog` arrived for the folder
    // picker. It pulls in `rfd`, which imports `TaskDialogIndirect` from
    // comctl32 — and only comctl32 *version 6* exports it. Version 6 is not the
    // copy in System32; that one is 5.82 and never had the symbol. Version 6
    // lives in WinSxS and the loader only finds it through the activation
    // context the manifest creates. No manifest, no activation context, no
    // TaskDialogIndirect: the test binary died at load with
    // STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139), before `main`, so the harness
    // never printed a line and cargo reported exit code 127.
    //
    // Declaring the same dependency for test targets is enough — the symbol
    // itself is never called there, it only has to bind. The app binary is left
    // alone: it already has tauri's manifest, and a second one would collide.
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if windows && msvc {
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
    }
}
