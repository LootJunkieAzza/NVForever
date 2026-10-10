//! Short forms for the scripts in a job (plugin version 8), so that a job can be written
//! by hand: where a step takes a script, the text may be just an expression, a condition
//! or some statements, and forms are named once per job and used as `@name`.
//!
//! The plugin writes the full function script around a short form when the job is read.
//! What it writes is, character for character, what the capture tools generated before
//! and what the game's compiler was seen to accept (docs/spec-notes/xnvse-capture.md §5
//! and §8). A text that holds `Begin Function` is a full script and is left as it is.

use std::collections::BTreeMap;

/// What a short form stands for, i.e. what is written around it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// An expression with a numeric value: `player.GetAV Strength`.
    Number,
    /// An expression whose value is a string: `GetNthModName 0`.
    Text,
    /// A condition; the script returns 1 when it holds, else 0.
    Condition,
    /// Statements, one per line; the script returns 1.
    Action,
}

/// A form named in a job's `forms`: its plugin file and its FormID within that plugin.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub plugin: String,
    /// The low three bytes of the FormID (the first byte is a load-order index).
    pub local: u32,
}

/// `"<plugin file>:<FormID in hex>"`, e.g. `"FalloutNV.esm:0000000F"`.
pub fn parse_form(spec: &str) -> Result<Form, String> {
    let bad = || format!("form {spec:?} must be \"<plugin file>:<FormID in hex>\", e.g. \"FalloutNV.esm:0000000F\"");
    let (plugin, id) = spec.rsplit_once(':').ok_or_else(bad)?;
    let (plugin, id) = (plugin.trim(), id.trim());
    let id = id.strip_prefix("0x").or_else(|| id.strip_prefix("0X")).unwrap_or(id);
    if plugin.is_empty() || plugin.contains(['"', '\n', '\r']) || id.is_empty() || id.len() > 8 {
        return Err(bad());
    }
    let n = u32::from_str_radix(id, 16).map_err(|_| bad())?;
    Ok(Form { plugin: plugin.to_string(), local: n & 0x00FF_FFFF })
}

pub fn is_full_script(text: &str) -> bool {
    text.to_ascii_lowercase().contains("begin function")
}

/// The function script for `text`: `text` itself if it is a full script, otherwise the
/// short form of the given kind with every `@name` replaced by a ref variable built
/// from the job's `forms`.
pub fn wrap(text: &str, kind: Kind, forms: &BTreeMap<String, Form>) -> Result<String, String> {
    if is_full_script(text) {
        return Ok(text.to_string());
    }
    let text = text.trim();
    if text.is_empty() {
        return Err("empty script".into());
    }
    // Forms in order of first use: the n-th is `rF<n>`, its plugin's index `fM<n>`.
    let mut used: Vec<(&str, &Form)> = Vec::new();
    let mut body = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('@') {
        body.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let len = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(after.len());
        let name = &after[..len];
        let form = forms
            .get(name)
            .ok_or_else(|| format!("@{name} is not in the job's \"forms\" (a text with a literal @ must be a full script)"))?;
        let n = used.iter().position(|(u, _)| *u == name).unwrap_or_else(|| {
            used.push((name, form));
            used.len() - 1
        });
        body.push_str(&format!("rF{}", n + 1));
        rest = &after[len..];
    }
    body.push_str(rest);

    let mut s = String::new();
    for n in 1..=used.len() {
        s.push_str(&format!("float fM{n}\nref rF{n}\n"));
    }
    if matches!(kind, Kind::Number | Kind::Condition) {
        s.push_str("float fV\n");
    }
    s.push_str("Begin Function { }\n");
    for (n, (_, f)) in used.iter().enumerate() {
        let n = n + 1;
        s.push_str(&format!("let fM{n} := GetModIndex \"{}\"\nlet rF{n} := BuildRef fM{n} {}\n", f.plugin, f.local));
    }
    match kind {
        Kind::Number => s.push_str(&format!("let fV := {body}\nSetFunctionValue fV\n")),
        Kind::Text => s.push_str(&format!("SetFunctionValue ({body})\n")),
        Kind::Condition => s.push_str(&format!("if {body}\n  let fV := 1\nendif\nSetFunctionValue fV\n")),
        Kind::Action => s.push_str(&format!("{body}\nSetFunctionValue 1\n")),
    }
    s.push_str("End\n");
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forms(list: &[(&str, &str)]) -> BTreeMap<String, Form> {
        list.iter().map(|(n, s)| (n.to_string(), parse_form(s).unwrap())).collect()
    }

    /// Each expected text below is one that ran in the game, from the named job in
    /// tools/capture/jobs (made by capture_steps.py or by hand).
    #[test]
    fn short_forms_become_the_scripts_that_ran_in_game() {
        let none = BTreeMap::new();
        // aa_crash_test
        assert_eq!(wrap("player.GetAV Strength", Kind::Number, &none).unwrap(),
                   "float fV\nBegin Function { }\nlet fV := player.GetAV Strength\nSetFunctionValue fV\nEnd\n");
        // aa_time_substitutes; %{name} is left for the runner
        assert_eq!(wrap("GameDaysPassed >= %{days3} + 1", Kind::Condition, &none).unwrap(),
                   "float fV\nBegin Function { }\nif GameDaysPassed >= %{days3} + 1\n  let fV := 1\nendif\nSetFunctionValue fV\nEnd\n");
        // ld_lock_levels (0x10B2D4 = 1094356)
        let f = forms(&[("safe", "FalloutNV.esm:0010B2D4"), ("item", "FalloutNV.esm:340BC")]);
        assert_eq!(wrap("@safe.GetLocked", Kind::Number, &f).unwrap(),
                   "float fM1\nref rF1\nfloat fV\nBegin Function { }\nlet fM1 := GetModIndex \"FalloutNV.esm\"\nlet rF1 := BuildRef fM1 1094356\nlet fV := rF1.GetLocked\nSetFunctionValue fV\nEnd\n");
        // bl_encumbrance (0x340BC = 213180)
        assert_eq!(wrap("player.RemoveItem @item 2", Kind::Action, &f).unwrap(),
                   "float fM1\nref rF1\nBegin Function { }\nlet fM1 := GetModIndex \"FalloutNV.esm\"\nlet rF1 := BuildRef fM1 213180\nplayer.RemoveItem rF1 2\nSetFunctionValue 1\nEnd\n");
        // xnvse-capture.md §5: strings are returned directly
        assert_eq!(wrap("GetNthModName 0", Kind::Text, &none).unwrap(), "Begin Function { }\nSetFunctionValue (GetNthModName 0)\nEnd\n");
    }

    #[test]
    fn forms_are_numbered_by_first_use_and_full_scripts_are_untouched() {
        let f = forms(&[("a", "A.esm:1"), ("b", "My Mod.esp:0x0A000ABC")]);
        let s = wrap("@b.AddItem @a 1 1\n@b.AddItem @a 2 1", Kind::Action, &f).unwrap();
        assert!(s.starts_with("float fM1\nref rF1\nfloat fM2\nref rF2\nBegin Function { }\n"));
        assert!(s.contains("let fM1 := GetModIndex \"My Mod.esp\"\nlet rF1 := BuildRef fM1 2748\n"), "load-order byte dropped: {s}");
        assert!(s.contains("let rF2 := BuildRef fM2 1\nrF1.AddItem rF2 1 1\nrF1.AddItem rF2 2 1\nSetFunctionValue 1\nEnd\n"));
        let full = "ref rC\nbegin function { }\nMessageEx \"a@b\"\nEnd\n";
        assert_eq!(wrap(full, Kind::Number, &f).unwrap(), full);
        assert!(wrap("player.GetItemCount @nope", Kind::Number, &f).unwrap_err().contains("@nope"));
        assert!(wrap("  ", Kind::Action, &f).is_err());
    }

    #[test]
    fn form_specs() {
        assert_eq!(parse_form("FalloutNV.esm:0000000F").unwrap(), Form { plugin: "FalloutNV.esm".into(), local: 15 });
        assert_eq!(parse_form(" DeadMoney.esm : 01005819 ").unwrap(), Form { plugin: "DeadMoney.esm".into(), local: 0x5819 });
        for bad in ["FalloutNV.esm", ":F", "A.esm:", "A.esm:xyz", "A.esm:123456789", "A\".esm:1"] {
            assert!(parse_form(bad).is_err(), "{bad}");
        }
    }
}
