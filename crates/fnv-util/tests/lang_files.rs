//! The translations NVForever ships in `lang/` (#1948): each file reads, each line's
//! English is a text NVForever still writes (a string literal in the sources, so a
//! renamed English text does not leave a dead line behind), and each translation keeps
//! its English's `{name}` places. The other way round (#3146): each text the sources
//! give to `tr(` or `trf(` as a literal has a line in every shipped file, so a new line
//! of NVForever's cannot be left English by forgetting the files.

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

/// Each Rust source of the root workspace and the engine by itself (as [`sources`] reads
/// them), for the checks that go file by file.
fn source_files(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name();
        if p.is_dir() {
            if name != "target" && name != "vendor" && !name.to_string_lossy().starts_with('.') {
                source_files(&p, out);
            }
        } else if p.extension().is_some_and(|x| x == "rs") {
            let text = std::fs::read_to_string(&p).unwrap_or_default();
            out.push((p, text));
        }
    }
}

/// The texts a source gives to `tr(` or `trf(` as a string literal, unescaped. A call
/// with anything else as its first argument (a name, a table's entry) is not listed:
/// those texts are checked from the files' side only.
fn tr_keys(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for call in ["tr(", "trf("] {
        let mut from = 0;
        while let Some(i) = src[from..].find(call) {
            let at = from + i;
            from = at + call.len();
            // The end of a longer name is not a call of these (`str(`, `attr(`).
            if src[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let Some(body) = src[from..].trim_start().strip_prefix('"') else { continue };
            let mut key = String::new();
            let mut chars = body.chars();
            let mut closed = false;
            while let Some(c) = chars.next() {
                match c {
                    '"' => {
                        closed = true;
                        break;
                    }
                    '\\' => match chars.next() {
                        Some('n') => key.push('\n'),
                        Some(c) => key.push(c),
                        None => break,
                    },
                    c => key.push(c),
                }
            }
            if closed {
                out.push(key);
            }
        }
    }
    out
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

/// #3146: a text NVForever writes through `tr(` / `trf(` has a line in every shipped
/// language file. Only the files that use `fnv_util::words` are read (other crates have
/// functions of the same name), and not this file, whose own tests hold such calls.
#[test]
fn every_translated_text_has_a_line_in_each_shipped_file() {
    let mut files = Vec::new();
    for d in ["crates", "engine"] {
        source_files(&repo().join(d), &mut files);
    }
    let mut keys: Vec<(String, PathBuf)> = Vec::new();
    for (path, text) in &files {
        if !text.contains("words::") || path.file_name().is_some_and(|n| n == "lang_files.rs") {
            continue;
        }
        keys.extend(tr_keys(text).into_iter().map(|k| (k, path.clone())));
    }
    assert!(keys.len() > 50, "only {} texts found from {}", keys.len(), repo().display());
    // The four files #3146 was raised for stay among them.
    for name in ["lockpick.rs", "blackjack.rs", "roulette.rs", "slots.rs"] {
        assert!(keys.iter().any(|(_, p)| p.file_name().is_some_and(|n| n == name)), "no translated text found in {name}");
    }
    let dir = repo().join("lang");
    let mut langs: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect();
    langs.sort();
    assert!(!langs.is_empty());
    let mut problems = Vec::new();
    for path in &langs {
        let id = path.file_stem().unwrap().to_string_lossy().to_string();
        let Ok(table) = Table::parse(&id, &std::fs::read_to_string(path).unwrap()) else { continue };
        let have: BTreeSet<&str> = table.pairs().map(|(en, _)| en).collect();
        for (key, from) in &keys {
            if !have.contains(key.as_str()) {
                problems.push(format!("{id}: no line for {key:?} ({})", from.file_name().unwrap().to_string_lossy()));
            }
        }
    }
    problems.sort();
    problems.dedup();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn translated_texts_are_read_from_a_source() {
    let src = "let a = tr(\"Lock picked\"); x.attr(\"no\"); str(\"no\"); words::trf(\n    \"{n} of \\\"{len}\\\"\", &[]); tr(name); tr(\"open";
    assert_eq!(tr_keys(src), ["Lock picked", "{n} of \"{len}\""]);
}

#[test]
fn places_are_names_in_braces() {
    assert_eq!(places("{n} of {len} right"), BTreeSet::from(["n", "len"]));
    assert_eq!(places("no places {} { x }"), BTreeSet::new());
}
