fn main() {
    // Own the Windows application manifest instead of letting tauri-build embed
    // it, so that every binary this crate produces carries it — not just the app.
    //
    // The manifest matters for one reason: it declares a dependency on
    // Microsoft.Windows.Common-Controls 6.0.0.0, and that declaration is what
    // creates the activation context the loader needs to resolve `comctl32.dll`
    // to version 6 in WinSxS. The copy in System32 is 5.82. Only version 6
    // exports `TaskDialogIndirect`, which `rfd` imports by way of
    // `tauri-plugin-dialog` — the folder picker.
    //
    // `tauri_build::build()` embeds that manifest through a Windows resource,
    // and hands the resource to cargo with `cargo:rustc-link-arg-bins=`. `-bins`
    // is the whole problem: `vapor-app.exe` got the manifest and the unit-test
    // binary did not, so `cargo test` produced an executable whose comctl32
    // import could not be bound. It died at load with STATUS_ENTRYPOINT_NOT_FOUND
    // (0xc0000139) before `main`, printing nothing, and cargo reported exit 127.
    // Windows CI had been red that way since 2026-08-20.
    //
    // `cargo:rustc-link-arg-tests=` is not the fix, and this is worth writing
    // down because it looks like it: `-tests` reaches integration targets under
    // `tests/`, and the binary that fails here is the *lib* unit-test harness,
    // which it does not reach. Measured with a throwaway crate — a bogus flag
    // emitted under `-tests` fails `cargo test --test integration` and leaves
    // `cargo test --lib` linking happily. Plain `rustc-link-arg` reaches both.
    //
    // So: tauri embeds no manifest (the resource still carries the icon and the
    // version strings), and the dependency is declared through the linker for
    // every target instead. One source, not two — which also avoids handing
    // link.exe a generated manifest and an RT_MANIFEST resource saying the same
    // thing, and asking it to reconcile them.
    let attributes = tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    tauri_build::try_build(attributes).expect("failed to run tauri-build");

    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if windows && msvc {
        // Identical in effect to the manifest tauri-build used to embed, which
        // contains this dependency and nothing else.
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
    }
}
