//! The frontend and the backend must agree about the IPC surface.
//!
//! `core.ts` calls commands by string name with an object of arguments; Rust
//! registers them in `generate_handler!` and names their parameters in the
//! function signature. Nothing connected the two, so every kind of mismatch was
//! invisible until someone opened the screen that made the call:
//!
//! * a command invoked from TypeScript that was never registered
//! * a command registered but with no `#[tauri::command]` behind it
//! * an argument renamed on one side only
//!
//! None of these is caught by the component tests, which run against a fake
//! backend that answers whatever it is asked, and none is caught by the e2e
//! suite, which serves a browser build against that same fake. Both are the
//! right tools for what they test and neither can see this.
//!
//! So it is checked here, by reading both files. Parsing source with regular
//! expressions is normally a bad idea; it is the right amount of machinery for
//! this because the two shapes involved are mechanical and the failure mode is
//! a test that is too strict, which shows up immediately rather than shipping.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let path: PathBuf = root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Everything between `from` and its matching close, given the opener/closer.
fn balanced(text: &str, from: usize, open: char, close: char) -> Option<&str> {
    let bytes = text.as_bytes();
    if bytes.get(from)? != &(open as u8) {
        return None;
    }
    let mut depth = 0usize;
    for (i, c) in text[from..].char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(&text[from + 1..from + i]);
            }
        }
    }
    None
}

/// Split on commas that are not nested inside brackets, braces or parens.
fn split_top_level(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    let mut in_string: Option<char> = None;
    for c in s.chars() {
        match in_string {
            Some(q) => {
                current.push(c);
                if c == q {
                    in_string = None;
                }
                continue;
            }
            None => {
                if c == '"' || c == '\'' || c == '`' {
                    in_string = Some(c);
                    current.push(c);
                    continue;
                }
            }
        }
        match c {
            '(' | '[' | '{' | '<' => {
                depth += 1;
                current.push(c);
            }
            ')' | ']' | '}' | '>' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                out.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

/// Command name -> argument names, as TypeScript calls them.
fn typescript_calls() -> BTreeMap<String, BTreeSet<String>> {
    let src = read("../src/lib/core.ts");
    let mut calls: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    let mut cursor = 0usize;
    while let Some(found) = src[cursor..].find("invoke<") {
        let at = cursor + found;
        cursor = at + "invoke<".len();

        // The command name is the first string literal after the generic.
        let Some(open_quote) = src[cursor..].find('"') else {
            break;
        };
        let name_start = cursor + open_quote + 1;
        let Some(close_quote) = src[name_start..].find('"') else {
            break;
        };
        let name = &src[name_start..name_start + close_quote];

        // Reject anything that is not a plain identifier: the generic may
        // itself contain a string type, and a false positive here would fail
        // the test for a command that does not exist.
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
            continue;
        }

        let after = name_start + close_quote + 1;
        let mut args = BTreeSet::new();
        // `, { … }` before the closing paren means arguments were passed.
        let rest = src[after..].trim_start();
        if let Some(stripped) = rest.strip_prefix(',') {
            let brace_at = after + (src[after..].len() - stripped.len()) + 1;
            if let Some(pos) = src[brace_at..].find('{') {
                let abs = brace_at + pos;
                // Only if the brace is on this call, not a later line.
                if src[brace_at..abs].trim().is_empty() {
                    if let Some(body) = balanced(&src, abs, '{', '}') {
                        for field in split_top_level(body) {
                            let key = field.split(':').next().unwrap_or("").trim();
                            let key = key.trim_start_matches("...").trim();
                            if !key.is_empty()
                                && key.chars().all(|c| c.is_alphanumeric() || c == '_')
                            {
                                args.insert(key.to_string());
                            }
                        }
                    }
                }
            }
        }
        calls.entry(name.to_string()).or_insert(args);
    }

    calls
}

/// Command name -> parameter names, as Rust declares them.
///
/// Tauri injects `State`, `AppHandle` and `Window` from the runtime rather than
/// from the request, so they are not part of the contract and are dropped.
fn rust_commands(src: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut out = BTreeMap::new();
    for chunk in src.split("#[tauri::command]").skip(1) {
        let Some(fn_at) = chunk.find("fn ") else {
            continue;
        };
        let after_fn = &chunk[fn_at + 3..];
        let name: String = after_fn
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        let Some(paren) = chunk[fn_at..].find('(') else {
            continue;
        };
        let Some(params) = balanced(chunk, fn_at + paren, '(', ')') else {
            continue;
        };

        let mut args = BTreeSet::new();
        for param in split_top_level(params) {
            let Some((lhs, ty)) = param.split_once(':') else {
                continue;
            };
            let ty = ty.trim();
            if ty.contains("State<") || ty.contains("AppHandle") || ty.contains("Window") {
                continue;
            }
            let arg = lhs.trim().trim_start_matches("mut ").trim();
            if !arg.is_empty() {
                args.insert(arg.to_string());
            }
        }
        out.insert(name, args);
    }
    out
}

/// The names listed in `generate_handler!`.
fn registered(src: &str) -> BTreeSet<String> {
    let at = src
        .find("generate_handler![")
        .expect("an invoke_handler in lib.rs");
    let bracket = at + "generate_handler!".len();
    let body = balanced(src, bracket, '[', ']').expect("a closed handler list");
    body.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.starts_with("//"))
        .map(str::to_string)
        .collect()
}

/// `camelCase` as Rust would spell it. Tauri accepts either on the wire.
fn to_snake(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn every_command_the_frontend_calls_is_registered() {
    let lib = read("src/lib.rs");
    let handlers = registered(&lib);
    let calls = typescript_calls();

    assert!(
        calls.len() > 50,
        "only found {} invoke sites — the parser stopped working, which would \
         make this test pass by finding nothing",
        calls.len()
    );

    let missing: Vec<&String> = calls.keys().filter(|c| !handlers.contains(*c)).collect();
    assert!(
        missing.is_empty(),
        "core.ts calls commands that are not registered in generate_handler!: {missing:?}\n\
         This is the failure that only shows when someone opens the screen."
    );
}

#[test]
fn every_registered_command_exists() {
    let lib = read("src/lib.rs");
    let handlers = registered(&lib);
    let defined = rust_commands(&lib);

    let ghosts: Vec<&String> = handlers
        .iter()
        .filter(|h| !defined.contains_key(*h))
        .collect();
    assert!(
        ghosts.is_empty(),
        "registered in generate_handler! with no #[tauri::command] behind them: {ghosts:?}"
    );
}

/// The one that would have caught renaming an argument on one side only.
#[test]
fn argument_names_agree_on_both_sides() {
    let lib = read("src/lib.rs");
    let defined = rust_commands(&lib);
    let calls = typescript_calls();

    let mut wrong: Vec<String> = Vec::new();
    for (cmd, js_args) in &calls {
        let Some(rust_args) = defined.get(cmd) else {
            continue; // reported by the registration test
        };
        let js_snake: BTreeSet<String> = js_args.iter().map(|a| to_snake(a)).collect();
        if &js_snake != rust_args {
            wrong.push(format!(
                "  {cmd}: TypeScript sends {:?}, Rust expects {:?}",
                js_snake, rust_args
            ));
        }
    }

    assert!(
        wrong.is_empty(),
        "argument names disagree between core.ts and lib.rs:\n{}",
        wrong.join("\n")
    );
}

/// A command nothing calls is not a bug, but it is worth knowing about — dead
/// IPC surface is still surface, and every one of these is reachable from a
/// page that can be told to invoke it.
#[test]
fn no_command_is_registered_and_never_called() {
    let lib = read("src/lib.rs");
    let handlers = registered(&lib);
    let calls = typescript_calls();

    let unused: Vec<&String> = handlers
        .iter()
        .filter(|h| !calls.contains_key(*h))
        .collect();
    assert!(
        unused.is_empty(),
        "registered but never called from core.ts: {unused:?}\n\
         Either wire it up or take it out of generate_handler!."
    );
}

/// The parsers themselves, because a source-reading test that quietly matches
/// nothing is worse than no test — it reports success for the wrong reason.
#[test]
fn the_parsers_actually_parse() {
    let sample = r#"
#[tauri::command]
fn thing(href: String, count: usize, state: State<'_, Shared>) -> Result<()> { }

#[tauri::command]
async fn other(app_handle: tauri::AppHandle, state: State<'_, Shared>) -> Result<()> { }
"#;
    let parsed = rust_commands(sample);
    assert_eq!(parsed.len(), 2);
    assert_eq!(
        parsed["thing"],
        ["count".to_string(), "href".to_string()]
            .into_iter()
            .collect()
    );
    // AppHandle and State are injected, so `other` takes nothing from the wire.
    assert!(parsed["other"].is_empty(), "{:?}", parsed["other"]);

    assert_eq!(to_snake("peerId"), "peer_id");
    assert_eq!(to_snake("href"), "href");

    let handlers = registered("x.invoke_handler(tauri::generate_handler![a, b, c,]).y");
    assert_eq!(handlers.len(), 3);

    assert_eq!(
        split_top_level("a: Foo<X, Y>, b: Bar"),
        vec!["a: Foo<X, Y>".to_string(), "b: Bar".to_string()]
    );
}

/// Guards the guard: the contract test reads two files by relative path, and a
/// move would make it silently read nothing.
#[test]
fn both_sides_of_the_contract_are_where_this_test_thinks() {
    assert!(root().join("src/lib.rs").is_file());
    assert!(root().join("../src/lib/core.ts").is_file());
    assert!(Path::new(&root()).join("../src/lib/core.ts").exists());
}

// ---------------------------------------------------------------------------
// Reply shapes
// ---------------------------------------------------------------------------
/*
 * `reply_field_names_match_the_typescript_that_reads_them` used to live here.
 *
 * It compared each Rust struct's field names against the TypeScript interface
 * that read them, by scanning both files as text and applying serde's rename
 * rules by hand. It earned its place — it is what caught `Settings` crossing as
 * snake_case — but it was a hand-maintained third copy of a contract that is
 * now derived: `ts-rs` generates `src/lib/generated/` from these structs, using
 * the same serde attributes serde itself uses, and `npm run types:check` fails
 * when the committed output drifts. A mismatch is no longer detectable after
 * the fact; it is unrepresentable.
 *
 * What remains in this file is the half `ts-rs` does not cover: the command
 * surface. Types say nothing about whether a command the frontend calls is
 * registered, or whether its argument names agree.
 */

