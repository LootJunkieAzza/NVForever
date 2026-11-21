//! `SAVE_PAIR_FIELDS` names every field of this crate that is a tuple of form IDs alone
//! (#1981): a save's remap cannot tell such a tuple from a list, and shortening one
//! makes the whole save unreadable.

use std::path::Path;

fn sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The name of a field declared on `line` with a type that is a tuple of form IDs alone.
fn pair_field(line: &str) -> Option<&str> {
    let (name, ty) = line.trim().strip_suffix(',')?.split_once(": ")?;
    let ty = ty.strip_prefix("Option<").and_then(|t| t.strip_suffix('>')).unwrap_or(ty);
    let inner = ty.strip_prefix('(')?.strip_suffix(')')?;
    let all_forms = inner.split(", ").all(|part| part == "FormId" || part == "Option<FormId>");
    all_forms.then(|| name.rsplit(' ').next().unwrap_or(name))
}

#[test]
fn every_pair_of_form_ids_is_named_for_the_remap() {
    assert_eq!(pair_field("    pub trespassing: Option<(FormId, FormId)>,"), Some("trespassing"));
    assert_eq!(pair_field("    pub(crate) farewell_then: Option<(FormId, Option<FormId>)>,"), Some("farewell_then"));
    assert_eq!(pair_field("    pub package_ends: Option<(FormId, f64)>,"), None, "told by its shape");
    assert_eq!(pair_field("    pub said: Vec<(FormId, FormId)>,"), None, "list entries go whole");
    let mut files = Vec::new();
    sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../fnv-sim/src"), &mut files);
    assert!(files.len() > 50, "the sources were found");
    let mut found = 0;
    for f in files {
        for line in std::fs::read_to_string(&f).unwrap().lines() {
            if let Some(name) = pair_field(line) {
                found += 1;
                assert!(fnv_game::game::SAVE_PAIR_FIELDS.contains(&name), "{}: add {name:?} to SAVE_PAIR_FIELDS (game.rs)", f.display());
            }
        }
    }
    assert!(found >= fnv_game::game::SAVE_PAIR_FIELDS.len(), "the scan still sees the fields ({found})");
}
