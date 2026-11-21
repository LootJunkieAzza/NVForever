//! The translations NVForever ships in `lang/` (#1948): each file reads, each line's
//! English is a text NVForever still writes (a string literal in the sources, so a
//! renamed English text does not leave a dead line behind), and each translation keeps
//! its English's `{name}` places.

use fnv_util::words::Table;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The Rust sources of the root workspace and the engine, without vendored crates and
/// build output.
fn sources(dir: &Path, out: &mut String) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name();
        if p.is_dir() {
            if name != "target" && name != "vendor" && !name.to_string_lossy().starts_with('.') {
                sources(&p, out);
            }
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
            out.push('\n');
        }
    }
}

/// The `{name}` places of a text.
fn places(s: &str) -> BTreeSet<&str> {
    let mut out = BTreeSet::new();
    let mut rest = s;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        let name = &after[..close];
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            out.insert(name);
        }
        rest = &after[close + 1..];
    }
    out
}

#[test]
fn shipped_language_files_match_the_words_nvforever_writes() {
    let dir = repo().join("lang");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect();
    files.sort();
    let mut src = String::new();
    for d in ["crates", "engine"] {
        sources(&repo().join(d), &mut src);
    }
    assert!(src.len() > 100_000, "the sources were not found from {}", repo().display());
    let mut problems = Vec::new();
    for path in &files {
        let id = path.file_stem().unwrap().to_string_lossy().to_string();
        let text = std::fs::read_to_string(path).unwrap();
        let table = match Table::parse(&id, &text) {
            Ok(t) => t,
            Err(e) => {
                problems.push(format!("{id}: does not read: {e}"));
                continue;
            }
        };
        for (en, tr) in table.pairs() {
            let literal = format!("\"{}\"", en.replace('\\', "\\\\").replace('"', "\\\""));
            if !src.contains(&literal) {
                problems.push(format!("{id}: no text {literal} in the sources"));
            }
            if places(en) != places(tr) {
                problems.push(format!("{id}: {en:?} has places {:?}, its translation {:?}", places(en), places(tr)));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn places_are_names_in_braces() {
    assert_eq!(places("{n} of {len} right"), BTreeSet::from(["n", "len"]));
    assert_eq!(places("no places {} { x }"), BTreeSet::new());
}
