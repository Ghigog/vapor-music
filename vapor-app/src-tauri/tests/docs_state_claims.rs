//! Documentation may not claim what currently works.
//!
//! `HANDOVER.md` said "he has never confirmed a scan works" for weeks after the
//! scan worked. It was read back to the owner of the library three times as
//! though it were true, and each time it sent the conversation somewhere
//! useless. The file is deleted; this is what stops the next one.
//!
//! The rule is narrow and mechanical, because a rule that needs judgement is
//! one that gets argued with at 2am:
//!
//! * A **measurement** is fine — `60.6% exact on 563 tracks` is true forever.
//! * A **decision** is fine — it records why, and why does not rot.
//! * A claim about **the present state of the software** is not, because it
//!   changes without anyone editing prose, and nothing tells you it has.
//!
//! Status belongs in `docs/workspace/tickets.md`, which is a ticket system and
//! is expected to be edited when a ticket moves.

use std::path::{Path, PathBuf};

/// Phrases that assert something about how the software is right now.
///
/// Each is here because it appeared in a doc and went stale, not because it
/// reads badly.
const BANNED: &[(&str, &str)] = &[
    ("has never", "asserts something has not happened yet"),
    ("have never", "asserts something has not happened yet"),
    ("does not yet", "asserts a present absence"),
    ("do not yet", "asserts a present absence"),
    ("is not yet", "asserts a present absence"),
    ("are not yet", "asserts a present absence"),
    ("not implemented", "asserts a present absence"),
    ("still missing", "asserts a present absence"),
    ("still fails", "asserts a present failure"),
    ("currently", "describes the present rather than a decision"),
    (
        "at the moment",
        "describes the present rather than a decision",
    ),
    (
        "as of today",
        "describes the present rather than a decision",
    ),
    ("right now", "describes the present rather than a decision"),
];

/// Docs allowed to talk about status.
///
/// `FINDINGS.md` states the rule itself and quotes the sentence that caused it,
/// which would otherwise trip on its own check.
const EXEMPT: &[&str] = &["FINDINGS.md", "THIRD_PARTY_NOTICES.md"];

/// Directories that are working notes rather than documentation.
///
/// `docs/workspace/` holds tickets, task lists and session walkthroughs — all
/// of them things that are *supposed* to describe a moment and be edited when
/// the moment passes. Policing them would be policing a notebook.
const EXEMPT_DIRS: &[&str] = &["workspace", "upstream"];

/// Counts of tests, which are stale the moment anyone writes one.
fn claims_a_test_count(line: &str) -> bool {
    let lowered = line.to_lowercase();
    if !lowered.contains("test") {
        return false;
    }
    // "317 tests", "83 component", "39 end-to-end" — a bare number next to the
    // word. Deliberately crude: the fix is always to delete the number and say
    // "run the command", so a false positive costs one edit.
    lowered.split_whitespace().any(|word| {
        word.trim_matches(|c: char| !c.is_ascii_digit())
            .parse::<u32>()
            .is_ok_and(|n| n > 5)
    }) && (lowered.contains(" tests") || lowered.contains("tests:"))
}

/// A changelog entry: a version and a date, which makes it a record.
fn is_changelog_entry(line: &str) -> bool {
    let trimmed = line.trim_start();
    // `> **v1.65 (2026-06-23):** …` and `### 1.65 — 2026-06-23`.
    let looks_versioned = trimmed.starts_with("> **v")
        || trimmed.starts_with("* **v")
        || trimmed.starts_with("- **v");
    looks_versioned && trimmed.contains("20")
}

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is vapor-app/src-tauri.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root")
}

fn markdown_under(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            // Vendored trees and build output are not ours to police.
            if matches!(
                name.as_str(),
                "node_modules" | "target" | ".git" | "godot-cpp" | "addons" | "licenses" | "bin"
            ) || EXEMPT_DIRS.contains(&name.as_str())
            {
                continue;
            }
            markdown_under(&path, found);
        } else if name.ends_with(".md") && !EXEMPT.contains(&name.as_str()) {
            found.push(path);
        }
    }
}

#[test]
fn no_document_claims_what_currently_works() {
    let root = repo_root();
    let mut docs = Vec::new();
    // Only the directories this project writes prose in. `README.md` at the
    // root is included; vendored READMEs are not.
    markdown_under(&root.join("docs"), &mut docs);
    for top in ["README.md", "AGENTS.md"] {
        let path = root.join(top);
        if path.is_file() {
            docs.push(path);
        }
    }

    let mut complaints: Vec<String> = Vec::new();

    for doc in &docs {
        let Ok(text) = std::fs::read_to_string(doc) else {
            continue;
        };
        let shown = doc.strip_prefix(&root).unwrap_or(doc).display().to_string();

        for (number, line) in text.lines().enumerate() {
            // Two kinds of line are records rather than claims.
            //
            // A struck-through row says something *was* true. And a changelog
            // entry stamped with a version and a date is history by
            // construction — "v1.65 (2026-06-23) … all 175 tests passing" is
            // a true statement about v1.65 and always will be. What the rule
            // is for is undated prose in the present tense.
            let historical = line.contains("~~") || is_changelog_entry(line);
            let lowered = line.to_lowercase();

            for (phrase, why) in BANNED {
                if lowered.contains(phrase) && !historical {
                    complaints.push(format!(
                        "{shown}:{} — \"{phrase}\" {why}\n    {}",
                        number + 1,
                        line.trim()
                    ));
                }
            }
            if claims_a_test_count(line) && !historical {
                complaints.push(format!(
                    "{shown}:{} — states a test count, which is stale the moment anyone writes one\n    {}",
                    number + 1,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        complaints.is_empty(),
        "documentation claims the present state of the software in {} place(s).\n\n{}\n\n\
         Record the measurement or the decision instead, and leave status to \
         docs/workspace/tickets.md. See docs/FINDINGS.md.",
        complaints.len(),
        complaints.join("\n")
    );
}

/// Every `docs/…md` a source file points at has to exist.
///
/// The same failure wearing a different hat: seven docs were deleted in one
/// commit and seventeen files went on citing them. A reference to a document
/// that is not there is worse than no reference, because it reads as though
/// there is more explanation somewhere and sends the reader looking for it.
#[test]
fn nothing_points_at_a_document_that_is_gone() {
    let root = repo_root();
    let mut sources = Vec::new();

    fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if matches!(
                    name.as_str(),
                    "node_modules" | "target" | ".git" | "godot-cpp" | "addons" | ".godot" | "bin"
                ) {
                    continue;
                }
                walk(&path, found);
            } else if name.ends_with(".rs")
                || name.ends_with(".ts")
                || name.ends_with(".tsx")
                || name.ends_with(".md")
            {
                found.push(path);
            }
        }
    }
    walk(&root.join("docs"), &mut sources);
    walk(&root.join("vapor-app/src"), &mut sources);
    walk(&root.join("vapor-app/src-tauri/src"), &mut sources);
    walk(&root.join("vapor-app/src-tauri/tests"), &mut sources);
    walk(&root.join("vapor-app/e2e"), &mut sources);
    walk(&root.join("vapor-core"), &mut sources);
    for top in ["README.md", "THIRD_PARTY_NOTICES.md"] {
        let path = root.join(top);
        if path.is_file() {
            sources.push(path);
        }
    }

    let mut dangling = Vec::new();
    for source in &sources {
        let Ok(text) = std::fs::read_to_string(source) else {
            continue;
        };
        let shown = source
            .strip_prefix(&root)
            .unwrap_or(source)
            .display()
            .to_string();

        for (number, line) in text.lines().enumerate() {
            for token in line.split(|c: char| c.is_whitespace() || "()[]`\"'<>,;".contains(c)) {
                let Some(at) = token.find("docs/") else {
                    continue;
                };
                let referenced = &token[at..];
                if !referenced.ends_with(".md") {
                    continue;
                }
                // This very file names the deleted document on purpose, in the
                // sentence explaining why the rule exists.
                if shown.ends_with("docs_state_claims.rs") {
                    continue;
                }
                if !root.join(referenced).is_file() {
                    dangling.push(format!("{shown}:{} — {referenced}", number + 1));
                }
            }
        }
    }

    assert!(
        dangling.is_empty(),
        "{} reference(s) point at a document that does not exist:\n\n{}",
        dangling.len(),
        dangling.join("\n")
    );
}
