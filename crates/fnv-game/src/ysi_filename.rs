//! Bounded documented ySI configuration -> tag/icon/filename (docs/spec-notes/ysi-filename.md).
//! The native string trait, defaults and file-order tie policy remain unsupported.
use crate::{functions::base_of, game::Game, mod_files};
use fnv_script::vm::{form_type_code, Value, XVal};
use fnv_sim::FormId;
use serde_json::{Map, Value as Json};
use std::collections::BTreeSet;

// Mapped configuration resource bounds, not native limits.
const MAX_FILES: usize = 128;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Match {
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

/// Resolves a form's certain JSON-configured ySI tag using the session's cached
/// configuration snapshot (returns `None` when configuration is unavailable,
/// unmatched, ambiguous, or blocked by an unsupported predicate).
pub(crate) fn json_tag(g: &mut Game<'_>, form: FormId) -> Option<String> {
    if g.ysi_filename.book.is_none() {
        g.ysi_filename.book = Some(Book::load());
    }
    let book = g.ysi_filename.book.as_ref()?.as_ref().ok()?;
    book.resolve_tag(g, form).ok()
}

/// Resolves the certain winning JSON icon `filename` for a ySI sorting tag using
/// the session's cached configuration snapshot.
pub(crate) fn json_icon_filename(g: &mut Game<'_>, tag: &str) -> Option<String> {
    json_icon_spec(g, tag).map(|(filename, _)| filename)
}

/// Resolves the certain winning JSON icon `(filename, optional template)` for a ySI
/// sorting tag using the session's cached configuration snapshot.
pub(crate) fn json_icon_spec(g: &mut Game<'_>, tag: &str) -> Option<(String, Option<String>)> {
    if g.ysi_filename.book.is_none() {
        g.ysi_filename.book = Some(Book::load());
    }
    let book = g.ysi_filename.book.as_ref()?.as_ref().ok()?;
    book.icon_spec(tag).ok()
}

/// Whether a valid `menus/ySI/*.json` configuration book is loaded in this session.
pub(crate) fn has_loaded_book(g: &mut Game<'_>) -> bool {
    if g.ysi_filename.book.is_none() {
        g.ysi_filename.book = Some(Book::load());
    }
    g.ysi_filename.book.as_ref().is_some_and(Result::is_ok)
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
            let cleaned = commented_json(text)?;
            if cleaned.trim().is_empty() && !name.eq_ignore_ascii_case("ySI.json") {
                continue;
            }
            let value: Json = serde_json::from_str(&cleaned)
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
        let tag = self.resolve_tag(g, item)?;
        self.icon_filename(&tag)
    }

    fn resolve_tag(&self, g: &Game<'_>, form: FormId) -> Result<String, String> {
        let mut best = None;
        let mut tags = BTreeSet::new();
        let mut unknown = None;
        for rule in &self.tags {
            match rule.matches(g, form) {
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
        Ok((*tags.first().expect("one tag")).to_string())
    }

    fn icon_filename(&self, tag: &str) -> Result<String, String> {
        self.icon_spec(tag).map(|(filename, _)| filename)
    }

    fn icon_spec(&self, tag: &str) -> Result<(String, Option<String>), String> {
        let priority = self
            .icons
            .iter()
            .filter(|r| r.tag == tag)
            .map(|r| r.priority)
            .max();
        let mut filenames = BTreeSet::new();
        let mut templates = BTreeSet::new();
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
            if let Some(tpl) = rule
                .fields
                .get("template")
                .and_then(Json::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                templates.insert(tpl);
            }
        }
        if filenames.len() != 1 {
            return Err(
                "missing or ambiguous ySI icon filename; native defaults/ties remain unqualified"
                    .into(),
            );
        }
        let template = if templates.len() == 1 {
            templates.first().map(|s| (*s).to_string())
        } else {
            None
        };
        Ok(((*filenames.first().expect("one filename")).to_string(), template))
    }
}

impl Rule {
    fn matches(&self, g: &Game<'_>, item: FormId) -> Match {
        let mut answer = Match::Yes;
        let formlist_mode = match self.fields.get("formlist") {
            None => Some(0i64),
            Some(v) => match v.as_i64() {
                Some(n @ (0 | 1)) => Some(n),
                _ => None,
            },
        };
        if formlist_mode.is_none() {
            answer = Match::Unknown;
        }
        for (name, value) in &self.fields {
            let part = match name.as_str() {
                "tag" | "priority" | "mod" | "formlist" => continue,
                "form" => match formlist_mode {
                    Some(0) => self.explicit_form(g, item, value),
                    Some(1) => self.formlist_member(g, item, value),
                    _ => Match::Unknown,
                },
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
                // WEAP / ALCH / ARMO record predicates: every key name below is
                // [documented: Sorting Icons 1.4b package, menus/ySI/ySI.json]; no ySI/yUI
                // DLL or source code was consulted. Uncited keys (including shorthand names
                // such as `weaponAutomatic`/`weaponScope`) fall through to `Match::Unknown`.
                //
                // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; schemas/fnv/objects.json WEAP.DNAM.skill;
                //  observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d): scalar integer only, arrays abort JSON]
                "weaponSkill" => weapon_scalar_int_field(g, item, "DNAM", "skill", value),
                // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; schemas/fnv/objects.json WEAP.DNAM.animation_type;
                //  observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d): scalar and array]
                "weaponType" => weapon_int_field(g, item, "DNAM", "animation_type", value),
                // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; schemas/fnv/objects.json WEAP.DNAM.flags1 bit 1 ("Is Automatic");
                //  observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d)]
                "weaponIsAutomatic" => weapon_flag1(g, item, 1 << 1, value),
                // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; schemas/fnv/objects.json WEAP.DNAM.flags1 bit 2 ("Has Scope");
                //  observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d): 1=true, 0=false, -1 does not match false]
                "weaponHasScope" => weapon_flag1(g, item, 1 << 2, value),
                // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; schemas/fnv/objects.json WEAP.DNAM.flags1 bit 0 ("Ignores Normal Weapon Resistance");
                //  observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d)]
                "weaponIgnoresDTDR" => weapon_flag1(g, item, 1 << 0, value),
                // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; schemas/fnv/objects.json ALCH.ENIT.flags bit 1 (0x02, "Food Item");
                //  observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d): 1=true, -1 does not match false]
                "aidIsFood" => aid_flag(g, item, crate::data::IngestibleDef::FOOD, value),
                // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; schemas/fnv/objects.json ALCH.ENIT.flags bit 2 (0x04, "Medicine");
                //  observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d)]
                "aidIsMedicine" => aid_flag(g, item, crate::data::IngestibleDef::MEDICINE, value),
                // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; schemas/fnv/objects.json ALCH.ENIT.withdrawal_effect; hypothesis: non-null withdrawal form]
                "aidIsAddictive" => aid_addictive(g, item, value),
                // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; GECK wiki GetArmorClass (1 Light, 2 Medium, 3 Heavy);
                //  observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d)]
                "armorClass" => armor_class_match(g, item, value),
                // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; schemas/fnv/objects.json ARMO.BMDT.general_flags bit 5 / GECK wiki IsPowerArmor;
                //  observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d): 1=true, -1 does not match false]
                "armorPower" => armor_power_match(g, item, value),
                // 11 ARMO biped-slot keys, each individually cited in `armor_biped_bit`.
                slot if armor_biped_bit(slot).is_some() => {
                    armor_biped_match(g, item, armor_biped_bit(slot).expect("checked"), value)
                }
                _ => crate::ysi_predicates::eval(g, item, &self.fields, name, value),
            };
            if part == Match::No {
                return Match::No;
            }
            if part == Match::Unknown {
                answer = Match::Unknown;
            }
        }
        // A mod or formlist selector without its explicit form has no established meaning here.
        if (self.fields.contains_key("mod") || self.fields.contains_key("formlist"))
            && !self.fields.contains_key("form")
        {
            return Match::Unknown;
        }
        if crate::ysi_predicates::has_dangling_companions(&self.fields) {
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
            let Some(local) = parse_local_form(v) else {
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

    fn formlist_member(&self, g: &Game<'_>, item: FormId, value: &Json) -> Match {
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
            let Some(local) = parse_local_form(v) else {
                return Match::Unknown;
            };
            if index > 255 {
                return Match::Unknown;
            }
            let list_id = FormId(((index as u32) << 24) | local);
            match g.data.sig(list_id) {
                Some(sig) if &sig == b"FLST" => {
                    if crate::functions::refs::list_members(g, list_id).contains(&item) {
                        Match::Yes
                    } else {
                        Match::No
                    }
                }
                _ => Match::Unknown,
            }
        };
        match value {
            Json::Array(values) => any(values.iter().map(matches)),
            _ => matches(value),
        }
    }
}

pub(crate) fn parse_local_form(v: &Json) -> Option<u32> {
    let text = v.as_str()?;
    u32::from_str_radix(
        text.strip_prefix("0x")
            .or_else(|| text.strip_prefix("0X"))
            .unwrap_or(text),
        16,
    )
    .ok()
    .filter(|n| *n <= 0x00ff_ffff)
}

pub(crate) fn any(values: impl Iterator<Item = Match>) -> Match {
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
pub(crate) fn number_set(value: &Json, number: i64) -> Match {
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

/// Match a boolean ySI predicate: `1` = required true, `0` = required false
/// [observed: FNV 1.4.0.525 + xNVSE 6.4.9 + yUI 140 (v1.4d) on `weaponHasScope`], and
/// `-1` = required false only when `allow_neg_one` is true (armor biped-slot rules
/// [documented: Sorting Icons 1.4b menus/ySI/ySI.json uses `1` and `-1` on armor slot
/// rules such as `"armorHat": 1, "armorMask": -1`]; on non-slot flags `-1` was observed
/// not to match false in `yUI 1.4d` and stays `Match::Unknown`). Any other JSON value
/// remains `Match::Unknown`.
pub(crate) fn bool_pred(value: &Json, actual: bool, allow_neg_one: bool) -> Match {
    match value.as_i64() {
        Some(1) => if actual { Match::Yes } else { Match::No },
        Some(0) => if !actual { Match::Yes } else { Match::No },
        Some(-1) if allow_neg_one => if !actual { Match::Yes } else { Match::No },
        _ => Match::Unknown,
    }
}

fn weapon_scalar_int_field(g: &Game<'_>, item: FormId, sub: &str, path: &str, value: &Json) -> Match {
    if g.data.sig(item) != Some(*b"WEAP") {
        return Match::No;
    }
    let Some(expected) = value.as_i64() else {
        return Match::Unknown;
    };
    match crate::nvse::record_value(g, item, sub, path) {
        Some(XVal::Num(n)) if n.is_finite() => {
            if n.trunc() as i64 == expected {
                Match::Yes
            } else {
                Match::No
            }
        }
        _ => Match::Unknown,
    }
}

fn weapon_int_field(g: &Game<'_>, item: FormId, sub: &str, path: &str, value: &Json) -> Match {
    if g.data.sig(item) != Some(*b"WEAP") {
        return Match::No;
    }
    match crate::nvse::record_value(g, item, sub, path) {
        Some(XVal::Num(n)) if n.is_finite() => number_set(value, n.trunc() as i64),
        _ => Match::Unknown,
    }
}

fn weapon_flag1(g: &Game<'_>, item: FormId, bit: u8, value: &Json) -> Match {
    let Some(flags) = g.weapon_flags1(item) else {
        return Match::No;
    };
    bool_pred(value, flags & bit != 0, false)
}

fn aid_flag(g: &Game<'_>, item: FormId, bit: u8, value: &Json) -> Match {
    if g.data.sig(item) != Some(*b"ALCH") {
        return Match::No;
    }
    let Some(def) = g.ingestible_def(item) else {
        return Match::Unknown;
    };
    bool_pred(value, def.flags & bit != 0, false)
}

fn aid_addictive(g: &Game<'_>, item: FormId, value: &Json) -> Match {
    if g.data.sig(item) != Some(*b"ALCH") {
        return Match::No;
    }
    let Some(def) = g.ingestible_def(item) else {
        return Match::Unknown;
    };
    bool_pred(value, def.withdrawal.is_some(), false)
}

fn armor_class_match(g: &Game<'_>, item: FormId, value: &Json) -> Match {
    if g.data.sig(item) != Some(*b"ARMO") {
        return Match::No;
    }
    number_set(value, i64::from(g.armor_class(item)))
}

fn armor_power_match(g: &Game<'_>, item: FormId, value: &Json) -> Match {
    if g.data.sig(item) != Some(*b"ARMO") {
        return Match::No;
    }
    let is_power = g
        .other
        .power_armor
        .get(&item)
        .copied()
        .unwrap_or_else(|| g.world.effective_power_armor(item));
    bool_pred(value, is_power, false)
}

/// ARMO `BMDT/biped_flags` bit index for each public ySI armor-slot predicate key.
/// Every key below is individually [documented: Sorting Icons 1.4b package,
/// `menus/ySI/ySI.json`]; bit indices are [documented: `schemas/fnv/objects.json`
/// `ARMO.BMDT.biped_flags`]. No ySI/yUI DLL or source code was consulted.
fn armor_biped_bit(name: &str) -> Option<u32> {
    Some(match name {
        // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; ARMO.BMDT.biped_flags bit 0 "Head"]
        "armorHead" => 0,
        // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; ARMO.BMDT.biped_flags bit 1 "Hair"]
        "armorHair" => 1,
        // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; ARMO.BMDT.biped_flags bit 2 "Upper Body"]
        "armorUpperBody" => 2,
        // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; ARMO.BMDT.biped_flags bit 6 "PipBoy"]
        "armorPipBoy" => 6,
        // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; ARMO.BMDT.biped_flags bit 7 "Backpack"]
        "armorBackpack" => 7,
        // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; ARMO.BMDT.biped_flags bit 9 "Headband"]
        "armorHeadband" => 9,
        // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; ARMO.BMDT.biped_flags bit 10 "Hat"]
        "armorHat" => 10,
        // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; ARMO.BMDT.biped_flags bit 11 "Eye Glasses"]
        "armorEyeglasses" => 11,
        // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; ARMO.BMDT.biped_flags bit 14 "Mask"]
        "armorMask" => 14,
        // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; ARMO.BMDT.biped_flags bit 15 "Choker"]
        "armorChoker" => 15,
        // [documented: Sorting Icons 1.4b menus/ySI/ySI.json; ARMO.BMDT.biped_flags bit 16 "Mouth Object"]
        "armorMouthObject" => 16,
        _ => return None,
    })
}

fn armor_biped_match(g: &Game<'_>, item: FormId, bit: u32, value: &Json) -> Match {
    if g.data.sig(item) != Some(*b"ARMO") {
        return Match::No;
    }
    let mask = match crate::nvse::record_value(g, item, "BMDT", "biped_flags") {
        Some(XVal::Num(n)) if n.is_finite() => n.trunc() as i64 as u64,
        _ => 0,
    };
    bool_pred(value, (mask >> bit) & 1 != 0, true)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_local_form_accepts_hex_and_rejects_out_of_range() {
        assert_eq!(parse_local_form(&Json::String("00000820".into())), Some(0x820));
        assert_eq!(parse_local_form(&Json::String("0x12AB".into())), Some(0x12AB));
        assert_eq!(parse_local_form(&Json::String("0X00FFFFFF".into())), Some(0x00FF_FFFF));
        assert_eq!(parse_local_form(&Json::String("01000000".into())), None);
        assert_eq!(parse_local_form(&Json::String("not-hex".into())), None);
        assert_eq!(parse_local_form(&Json::from(0x820)), None);
    }

    #[test]
    fn commented_json_strips_comments_trailing_commas_and_bom() {
        let input = "\u{feff}// lead\n{\"a\": \"http://x/*y*/\", /* mid */ \"b\": [1, 2,],}\n";
        let cleaned = commented_json(input).unwrap();
        let parsed: Json = serde_json::from_str(&cleaned).unwrap();
        assert_eq!(parsed["a"], "http://x/*y*/");
        assert_eq!(parsed["b"], serde_json::json!([1, 2]));
        assert!(commented_json("/* unclosed").is_err());
        assert_eq!(commented_json("// comment only\n/* block */").unwrap().trim(), "");
    }

    #[test]
    fn number_set_handles_scalars_arrays_and_unknowns() {
        assert_eq!(number_set(&Json::from(40), 40), Match::Yes);
        assert_eq!(number_set(&Json::from(40), 31), Match::No);
        assert_eq!(number_set(&serde_json::json!([31, 40]), 40), Match::Yes);
        assert_eq!(number_set(&serde_json::json!([31, "bad"]), 40), Match::Unknown);
        assert_eq!(number_set(&serde_json::json!([31, "bad"]), 31), Match::Yes);
    }

    #[test]
    fn bool_pred_and_biped_bits_handle_documented_values_and_reject_unknowns() {
        assert_eq!(bool_pred(&Json::from(1), true, true), Match::Yes);
        assert_eq!(bool_pred(&Json::from(1), false, true), Match::No);
        assert_eq!(bool_pred(&Json::from(0), false, true), Match::Yes);
        assert_eq!(bool_pred(&Json::from(-1), false, true), Match::Yes);
        assert_eq!(bool_pred(&Json::from(-1), true, true), Match::No);
        assert_eq!(bool_pred(&Json::from(-1), false, false), Match::Unknown);
        assert_eq!(bool_pred(&Json::from(-1), true, false), Match::Unknown);
        assert_eq!(bool_pred(&Json::from(2), false, true), Match::Unknown);
        assert_eq!(bool_pred(&Json::from("1"), true, true), Match::Unknown);
        assert_eq!(armor_biped_bit("armorHead"), Some(0));
        assert_eq!(armor_biped_bit("armorHat"), Some(10));
        assert_eq!(armor_biped_bit("armorMask"), Some(14));
        assert_eq!(armor_biped_bit("armorMouthObject"), Some(16));
        assert_eq!(armor_biped_bit("armorUnknown"), None);
    }
}
