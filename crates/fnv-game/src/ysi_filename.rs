//! Bounded documented ySI configuration → filename (docs/spec-notes/ysi-filename.md).
//! The native string trait, defaults and file-order tie policy remain unsupported.
use crate::{
    functions::{base_of, items::form_type_code},
    game::Game,
    mod_files,
};
use fnv_script::vm::{Value, XVal};
use fnv_sim::FormId;
use serde_json::{Map, Value as Json};
use std::collections::BTreeSet;

// Mapped configuration resource bounds, not native limits.
const MAX_FILES: usize = 32;
const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_RULES: usize = 20_000;

#[derive(Default)]
pub(crate) struct State {
    // Original configuration documentation describes launch-time classification.
    // This session's first query freezes its input snapshot, including a load error.
    // Game restore creates a fresh cache; no result is attached to an item or save.
    book: Option<Result<Book, String>>,
}
struct Rule {
    priority: i64,
    tag: String,
    fields: Map<String, Json>,
}
struct Book {
    tags: Vec<Rule>,
    icons: Vec<Rule>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Match {
    Yes,
    No,
    Unknown,
}

pub(crate) fn handles(name: &str, args: &[XVal]) -> bool {
    name.eq_ignore_ascii_case("ySIGetTrait")
        && args
            .first()
            .is_some_and(|v| matches!(v, XVal::Str(s) if s.eq_ignore_ascii_case("filename")))
}

pub(crate) fn call(
    g: &mut Game<'_>,
    receiver: Option<Value>,
    args: &[XVal],
) -> Result<XVal, String> {
    let (trait_name, item) = match (receiver, args) {
        (Some(value), [XVal::Str(name)]) => (
            name,
            value
                .as_form()
                .map(FormId)
                .ok_or("invalid ySI item receiver")?,
        ),
        (None, [XVal::Str(name), XVal::Ref(item)]) if *item != 0 => (name, FormId(*item)),
        _ => {
            return Err(
                "ySIGetTrait \"filename\" expects a trait and one explicit item or item receiver"
                    .into(),
            )
        }
    };
    if !trait_name.eq_ignore_ascii_case("filename") {
        return Err(
            "ySIGetTrait filename mapper handles only the filename trait".into(),
        );
    }
    let original = g.unclone(item);
    let base = if g.world.forms.item(original).is_some() {
        original
    } else {
        g.unclone(base_of(g, item).ok_or("unknown ySI inventory item")?)
    };
    if g.world.peek(item).is_some_and(|s| s.deleted())
        || g.world.deleted_base(item).is_some()
        || g.world.forms.item(base).is_none()
    {
        return Err("ySIGetTrait \"filename\" requires a live inventory item or item base".into());
    }
    if g.ysi_filename.book.is_none() {
        g.ysi_filename.book = Some(Book::load());
    }
    let book = g
        .ysi_filename
        .book
        .as_ref()
        .expect("loaded")
        .as_ref()
        .map_err(Clone::clone)?;
    Ok(XVal::Str(book.filename(g, base)?))
}

impl Book {
    fn load() -> Result<Self, String> {
        let mut names: Vec<String> = mod_files::data_files("menus\\ySI")
            .into_iter()
            .map(|(name, _)| name)
            .filter(|name| name.to_ascii_lowercase().ends_with(".json"))
            .collect();
        if !names.iter().any(|n| n.eq_ignore_ascii_case("ySI.json")) {
            names.push("ySI.json".into());
        }
        names.sort_by_key(|n| n.to_ascii_lowercase());
        if names.len() > MAX_FILES {
            return Err("ySI filename configuration exceeds the mapped file bound".into());
        }
        let mut book = Self {
            tags: Vec::new(),
            icons: Vec::new(),
        };
        let mut bytes_read = 0usize;
        for name in names {
            let bytes = mod_files::read_data_file(&format!("menus\\ySI\\{name}"))
                .ok_or_else(|| format!("ySI filename configuration unavailable: {name}"))?;
            bytes_read = bytes_read
                .checked_add(bytes.len())
                .ok_or("ySI configuration size overflow")?;
            if bytes_read > MAX_BYTES {
                return Err("ySI filename configuration exceeds the mapped byte bound".into());
            }
            let text = std::str::from_utf8(&bytes).map_err(|_| "ySI configuration is not UTF-8")?;
            let value: Json = serde_json::from_str(&commented_json(text)?)
                .map_err(|_| "invalid ySI commented configuration")?;
            let object = value
                .as_object()
                .ok_or("ySI configuration must be an object")?;
            for (key, rules) in [("tags", &mut book.tags), ("icons", &mut book.icons)] {
                let Some(entries) = object.get(key) else {
                    continue;
                };
                for entry in entries
                    .as_array()
                    .ok_or("ySI rule section must be an array")?
                {
                    let fields = entry
                        .as_object()
                        .ok_or("ySI rule must be an object")?
                        .clone();
                    let priority = fields
                        .get("priority")
                        .and_then(Json::as_i64)
                        .ok_or("ySI rule priority must be an integer")?;
                    let tag = fields
                        .get("tag")
                        .and_then(Json::as_str)
                        .ok_or("ySI rule tag must be a string")?
                        .to_string();
                    rules.push(Rule {
                        priority,
                        tag,
                        fields,
                    });
                }
            }
            if book.tags.len() + book.icons.len() > MAX_RULES {
                return Err("ySI filename configuration exceeds the mapped rule bound".into());
            }
        }
        Ok(book)
    }

    fn filename(&self, g: &Game<'_>, item: FormId) -> Result<String, String> {
        let mut best = None;
        let mut tags = BTreeSet::new();
        let mut unknown = None;
        for rule in &self.tags {
            match rule.matches(g, item) {
                Match::Yes => match best {
                    Some(priority) if priority > rule.priority => {}
                    Some(priority) if priority == rule.priority => {
                        tags.insert(rule.tag.as_str());
                    }
                    _ => {
                        best = Some(rule.priority);
                        tags.clear();
                        tags.insert(rule.tag.as_str());
                    }
                },
                Match::Unknown => {
                    unknown = Some(unknown.map_or(rule.priority, |p: i64| p.max(rule.priority)))
                }
                Match::No => {}
            }
        }
        let Some(priority) = best else {
            return Err(
                "no certain ySI tag; native missing-tag behavior remains unqualified".into(),
            );
        };
        if unknown.is_some_and(|p| p >= priority) {
            return Err(
                "an unsupported ySI predicate could change the highest-priority tag".into(),
            );
        }
        if tags.len() != 1 {
            return Err(
                "ambiguous ySI tag priority; native tie ordering remains unqualified".into(),
            );
        }
        let tag = *tags.first().expect("one tag");
        let priority = self
            .icons
            .iter()
            .filter(|r| r.tag == tag)
            .map(|r| r.priority)
            .max();
        let mut filenames = BTreeSet::new();
        for rule in self
            .icons
            .iter()
            .filter(|r| r.tag == tag && Some(r.priority) == priority)
        {
            let filename = rule
                .fields
                .get("filename")
                .and_then(Json::as_str)
                .ok_or("selected ySI icon has no filename string")?;
            filenames.insert(filename);
        }
        if filenames.len() != 1 {
            return Err(
                "missing or ambiguous ySI icon filename; native defaults/ties remain unqualified"
                    .into(),
            );
        }
        Ok((*filenames.first().expect("one filename")).to_string())
    }
}

impl Rule {
    fn matches(&self, g: &Game<'_>, item: FormId) -> Match {
        let mut answer = Match::Yes;
        for (name, value) in &self.fields {
            let part = match name.as_str() {
                "tag" | "priority" | "mod" => continue,
                "form" => self.explicit_form(g, item, value),
                "formType" => match g.data.sig(item).and_then(|s| form_type_code(&s)) {
                    Some(code) => number_set(value, i64::from(code)),
                    None => Match::Unknown,
                },
                "questItem" => match value.as_i64() {
                    Some(n @ (0 | 1)) => {
                        if g.world.is_quest_item(item) == (n == 1) {
                            Match::Yes
                        } else {
                            Match::No
                        }
                    }
                    _ => Match::Unknown,
                },
                _ => Match::Unknown,
            };
            if part == Match::No {
                return Match::No;
            }
            if part == Match::Unknown {
                answer = Match::Unknown;
            }
        }
        // A mod selector without its explicit form has no established meaning here.
        if self.fields.contains_key("mod") && !self.fields.contains_key("form") {
            return Match::Unknown;
        }
        answer
    }

    fn explicit_form(&self, g: &Game<'_>, item: FormId, value: &Json) -> Match {
        let Some(name) = self.fields.get("mod").and_then(Json::as_str) else {
            return Match::Unknown;
        };
        let Some(index) = g
            .data
            .load_order
            .iter()
            .position(|p| p.eq_ignore_ascii_case(name))
        else {
            return Match::No;
        };
        let matches = |v: &Json| {
            let Some(text) = v.as_str() else {
                return Match::Unknown;
            };
            let Some(local) = u32::from_str_radix(
                text.strip_prefix("0x")
                    .or_else(|| text.strip_prefix("0X"))
                    .unwrap_or(text),
                16,
            )
            .ok()
            .filter(|n| *n <= 0x00ff_ffff) else {
                return Match::Unknown;
            };
            if index > 255 {
                return Match::Unknown;
            }
            if item.0 == (((index as u32) << 24) | local) {
                Match::Yes
            } else {
                Match::No
            }
        };
        match value {
            Json::Array(values) => any(values.iter().map(matches)),
            _ => matches(value),
        }
    }
}

fn any(values: impl Iterator<Item = Match>) -> Match {
    let mut answer = Match::No;
    for v in values {
        if v == Match::Yes {
            return Match::Yes;
        }
        if v == Match::Unknown {
            answer = Match::Unknown;
        }
    }
    answer
}
fn number_set(value: &Json, number: i64) -> Match {
    let one = |v: &Json| match v.as_i64() {
        Some(n) if n == number => Match::Yes,
        Some(_) => Match::No,
        None => Match::Unknown,
    };
    match value {
        Json::Array(values) => any(values.iter().map(one)),
        _ => one(value),
    }
}

/// ySI-local commented configuration, leaving generic strict JSON APIs unchanged.
fn commented_json(text: &str) -> Result<String, String> {
    let bytes = text.trim_start_matches('\u{feff}').as_bytes();
    let mut out = bytes.to_vec();
    let (mut i, mut string, mut escape) = (0, false, false);
    while i < bytes.len() {
        if string {
            if escape {
                escape = false;
            } else if bytes[i] == b'\\' {
                escape = true;
            } else if bytes[i] == b'"' {
                string = false;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b'"' {
            string = true;
            i += 1;
            continue;
        }
        if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                out[i] = b' ';
                i += 1;
            }
        } else if bytes[i..].starts_with(b"/*") {
            out[i] = b' ';
            out[i + 1] = b' ';
            i += 2;
            while i < bytes.len() && !bytes[i..].starts_with(b"*/") {
                if !matches!(bytes[i], b'\r' | b'\n') {
                    out[i] = b' ';
                }
                i += 1;
            }
            if i == bytes.len() {
                return Err("unterminated ySI configuration comment".into());
            }
            out[i] = b' ';
            out[i + 1] = b' ';
            i += 2;
        } else {
            i += 1;
        }
    }
    // Trailing commas are a mapped configuration extension, outside quoted strings.
    let (mut string, mut escape) = (false, false);
    for i in 0..out.len() {
        if string {
            if escape {
                escape = false;
            } else if out[i] == b'\\' {
                escape = true;
            } else if out[i] == b'"' {
                string = false;
            }
            continue;
        }
        if out[i] == b'"' {
            string = true;
            continue;
        }
        if out[i] == b','
            && out[i + 1..]
                .iter()
                .find(|b| !b.is_ascii_whitespace())
                .is_some_and(|b| matches!(*b, b'}' | b']'))
        {
            out[i] = b' ';
        }
    }
    String::from_utf8(out).map_err(|_| "invalid UTF-8 in ySI configuration".into())
}
