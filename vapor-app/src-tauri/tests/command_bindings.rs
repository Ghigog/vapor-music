//! Every registered command must be callable from the frontend (TD-37).
//!
//! Nine commands once had no binding at all — scan, analyse, the cache and the
//! data-deletion pair among them — so the app could index a library, listen to
//! it and delete it, and a person could do none of those things. Nothing
//! noticed, because the two sides are separate languages in separate
//! directories and only ever meet at run time.
//!
//! This is the cheapest possible check: read the `generate_handler!` list, read
//! `core.ts`, and require that every name in the first appears in the second.
//! It cannot tell whether the *arguments* match — that would need real type
//! extraction — but it catches the failure that actually happened, which is a
//! command being added on one side and forgotten on the other.
//!
//! Deliberately not the reverse direction: a binding for a command that does
//! not exist fails loudly the first time it is called, so it needs no test.

/// The files are read at compile time, so this cannot silently pass because a
/// path went stale.
const LIB_RS: &str = include_str!("../src/lib.rs");
const CORE_TS: &str = include_str!("../../src/lib/core.ts");

/// Command names inside `invoke_handler(tauri::generate_handler![ ... ])`.
fn registered_commands() -> Vec<String> {
    let start = LIB_RS
        .find("generate_handler![")
        .expect("no generate_handler! in lib.rs — has the shell been restructured?");
    let rest = &LIB_RS[start..];
    let end = rest.find(']').expect("unterminated generate_handler!");

    rest[..end]
        .lines()
        .skip(1) // the `generate_handler![` line itself
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .map(|l| l.trim_end_matches(',').to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

#[test]
fn every_command_has_a_frontend_binding() {
    let commands = registered_commands();

    // A guard against the parser quietly finding nothing and the test passing
    // by measuring an empty list.
    assert!(
        commands.len() > 15,
        "only found {} commands, so the parser is probably broken: {commands:?}",
        commands.len()
    );

    let missing: Vec<&String> = commands
        .iter()
        // The binding is what the frontend calls: `invoke<T>("name"`.
        .filter(|c| !CORE_TS.contains(&format!("\"{c}\"")))
        .collect();

    assert!(
        missing.is_empty(),
        "these commands are registered but cannot be called from the UI, \
         so whatever they do is unreachable: {missing:?}\n\
         Add a wrapper to vapor-app/src/lib/core.ts."
    );
}

/// Names must be snake_case on the Rust side. A camelCase one would be
/// registered and then never match what Tauri dispatches.
#[test]
fn command_names_are_well_formed() {
    for command in registered_commands() {
        assert!(
            command
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "'{command}' is not a snake_case command name — the parser has \
             probably picked up something that is not a command"
        );
    }
}
