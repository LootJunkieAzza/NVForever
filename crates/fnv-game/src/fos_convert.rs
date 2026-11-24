//! Clean-room offline `.fos` -> `.nvfsave` save converter (docs/design/save-load.md,
//! docs/spec-notes/save-files.md).
//!
//! The NVForever runtime never reads or writes `.fos` files directly. This module builds a
//! real Layer 4 [`fnv_sim::World`] + Layer 5 [`Game`] from the loaded plugin order and the
//! documented/observed state decoded by [`fnv_save::fos`], validates the resulting
//! [`GameSnapshot`], and emits a standard `NVFSAVE\0` v1 header, optional PNG thumbnail,
//! typed `nvf.save.body/1` body, and a truthful [`ConversionReport`] accounting for every
//! converted and skipped/unsupported `.fos` section.

use crate::actor_values::{CARRY_WEIGHT, HEALTH, LIMB_CONDITIONS, SPECIALS};
use crate::aid::{grows as av_grows, RAD_LEVEL};
use crate::chargen::Look;
use crate::game::{Game, GameConfig, GameSnapshot, InstKey};
use crate::GameData;
use fnv_save::fos::{
    Fos, FosItem, FosItemEntry, FosScriptVars, VarValue, FORM_DEAD, FORM_DELETED, FORM_DISABLED,
    FORM_TALKED_TO_PC, PLAYER_BASE, PLAYER_REF, TYPE_ACHR, TYPE_ACRE, TYPE_CELL, TYPE_CHAL,
    TYPE_CLAS, TYPE_ECZN, TYPE_FACT, TYPE_FLST, TYPE_INFO, TYPE_LVLI, TYPE_NOTE, TYPE_NPC,
    TYPE_QUST, TYPE_REFR, TYPE_REPU,
};
use fnv_save::{EngineSnapshot, Place, PluginEntry, PluginRemap, SaveBody, SaveHeader, SaveKind};
use fnv_script::vm::Value;
use fnv_sim::forms::{ItemKind, LeveledEntry, SPECIAL};
use fnv_sim::inventory::{Extra, Pick};
use fnv_sim::world::LockState;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const REPORT_FORMAT: &str = "fnv.save.fos_convert/1";
pub const INSPECT_FORMAT: &str = "fnv.save.fos_inspect/1";

/// Strength's actor value code; SPECIAL follow in order (5..=11).
const AV_STRENGTH: u16 = 5;
/// Karma actor value code (docs/spec-notes/save-files.md §6.4).
const AV_KARMA: u16 = 23;
/// Experience points actor value code in the player's second AV array (save-files.md §6.4).
const AV_XP: u16 = 24;

/// Options controlling `.fos` -> `.nvfsave` conversion.
#[derive(Debug, Clone)]
pub struct ConvertOptions {
    pub kind: SaveKind,
    /// The loaded plugin list (in load order, with file sizes) that will be stamped into
    /// [`SaveHeader::plugins`] and used to remap `.fos` form IDs.
    pub plugins: Vec<PluginEntry>,
    /// Encode the `.fos` uncompressed RGB screenshot as an 8-bit RGB PNG thumbnail.
    pub include_thumbnail: bool,
    /// Whether an adjacent `.nvse` co-save file was detected beside the `.fos` input.
    pub cosave_present: bool,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            kind: SaveKind::Manual,
            plugins: Vec::new(),
            include_thumbnail: true,
            cosave_present: false,
        }
    }
}

/// Summary of the source `.fos` header and section counts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FosSummary {
    pub save_number: u32,
    pub player_name: String,
    pub karma_title: String,
    pub level: u32,
    pub location: String,
    pub play_time: String,
    pub play_seconds: Option<f64>,
    pub screenshot_width: u32,
    pub screenshot_height: u32,
    pub screenshot_bytes: usize,
    pub plugins: Vec<String>,
    pub form_id_count: usize,
    pub worldspace_count: usize,
    pub changed_form_count: usize,
}

impl FosSummary {
    pub fn from_fos(fos: &Fos) -> Self {
        Self {
            save_number: fos.header.save_number,
            player_name: fos.header.player_name.clone(),
            karma_title: fos.header.karma_title.clone(),
            level: fos.header.level,
            location: fos.header.location.clone(),
            play_time: fos.header.play_time.clone(),
            play_seconds: fos.header.play_seconds(),
            screenshot_width: fos.header.screenshot_width,
            screenshot_height: fos.header.screenshot_height,
            screenshot_bytes: fos.screenshot_rgb.len(),
            plugins: fos.plugins.clone(),
            form_id_count: fos.form_ids.len(),
            worldspace_count: fos.worlds.len(),
            changed_form_count: fos.changed.len(),
        }
    }
}

/// Structured offline inspection report for `fnvtool save inspect-fos`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FosInspectReport {
    pub schema: String,
    pub source: FosSummary,
    pub cosave_present: bool,
    pub global_tables: Vec<(u32, usize)>,
    pub changed_by_kind: BTreeMap<String, usize>,
    pub decoded_counts: BTreeMap<String, usize>,
    pub parse_errors: BTreeMap<String, usize>,
    pub active_quest: Option<String>,
    pub weather_current: Option<String>,
    pub player_has_place: bool,
    pub player_has_special: bool,
    pub player_tag_skill_count: usize,
    pub player_av_parsed: bool,
    pub player_inventory_items: Option<usize>,
}

/// Structured conversion summary emitted alongside the `.nvfsave`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversionReport {
    pub schema: String,
    pub source: FosSummary,
    pub missing_plugins: Vec<String>,
    pub cosave_ignored: bool,
    pub converted: BTreeMap<String, usize>,
    /// Counts of converted fields whose `.fos` decoding or `.nvfsave` mapping relies on
    /// `[hypothesis]` readings in `docs/spec-notes/save-files.md` (§§3–6.6).
    pub hypothesis_fields: BTreeMap<String, usize>,
    pub skipped: BTreeMap<String, usize>,
    pub warnings: Vec<String>,
}

/// The complete converted save ready to write via [`fnv_save::write`].
#[derive(Debug, Clone)]
pub struct ConvertedSave {
    pub header: SaveHeader,
    pub shot: Vec<u8>,
    pub body: SaveBody<GameSnapshot>,
    pub report: ConversionReport,
}

/// Encode uncompressed 8-bit RGB pixels (`w * h * 3` bytes) as a PNG thumbnail.
pub fn encode_rgb_png(rgb: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    if w == 0 || h == 0 {
        return None;
    }
    let expected = (w as usize).checked_mul(h as usize)?.checked_mul(3)?;
    if rgb.len() != expected {
        return None;
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().ok()?;
    writer.write_image_data(rgb).ok()?;
    writer.finish().ok()?;
    Some(out)
}

/// Inspect a parsed `.fos` file without needing game plugins.
pub fn inspect_fos(fos: &Fos, cosave_present: bool) -> FosInspectReport {
    let mut changed_by_kind: BTreeMap<String, usize> = BTreeMap::new();
    for c in &fos.changed {
        let label = format!("{}({})", fnv_save::fos::kind_name(c.kind), c.kind);
        *changed_by_kind.entry(label).or_default() += 1;
    }
    let mut decoded_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut parse_errors: BTreeMap<String, usize> = BTreeMap::new();

    match fos.global_variables() {
        Ok(v) => {
            decoded_counts.insert("globals".into(), v.len());
        }
        Err(_) => {
            *parse_errors.entry("globals".into()).or_default() += 1;
        }
    }
    match fos.misc_stats() {
        Ok(v) => {
            decoded_counts.insert("misc_stats".into(), v.len());
        }
        Err(_) => {
            *parse_errors.entry("misc_stats".into()).or_default() += 1;
        }
    }
    match fos.dead_counts() {
        Ok(v) => {
            decoded_counts.insert("dead_counts".into(), v.len());
        }
        Err(_) => {
            *parse_errors.entry("dead_counts".into()).or_default() += 1;
        }
    }

    let mut count_res = |name: &str, ok: usize, err: usize| {
        decoded_counts.insert(name.into(), ok);
        if err > 0 {
            parse_errors.insert(name.into(), err);
        }
    };
    let (q_ok, q_err) = partition_results(fos.quests());
    count_res("quests", q_ok, q_err);
    let (r_ok, r_err) = partition_results(fos.reference_changes());
    count_res("reference_changes", r_ok, r_err);
    let (ci_ok, ci_err) = partition_results(fos.container_inventories());
    count_res("container_inventories", ci_ok, ci_err);
    let (mm_ok, mm_err) = partition_results(fos.map_markers());
    count_res("map_markers", mm_ok, mm_err);
    let (cr_ok, cr_err) = partition_results(fos.created_refs());
    count_res("created_refs", cr_ok, cr_err);
    let (f_ok, f_err) = partition_results(fos.faction_relations());
    count_res("factions", f_ok, f_err);
    let (fl_ok, fl_err) = partition_results(fos.form_lists());
    count_res("form_lists", fl_ok, fl_err);
    let (lv_ok, lv_err) = partition_results(fos.leveled_lists());
    count_res("leveled_lists", lv_ok, lv_err);
    let (rp_ok, rp_err) = partition_results(fos.reputations());
    count_res("reputations", rp_ok, rp_err);
    let (ch_ok, ch_err) = partition_results(fos.challenges());
    count_res("challenges", ch_ok, ch_err);
    let (ez_ok, ez_err) = partition_results(fos.encounter_zones());
    count_res("encounter_zones", ez_ok, ez_err);
    let (cl_ok, cl_err) = partition_results(fos.cells());
    count_res("cells", cl_ok, cl_err);
    decoded_counts.insert("ref_extras".into(), fos.ref_extras().len());
    decoded_counts.insert("actor_changes".into(), fos.actor_changes().len());
    decoded_counts.insert("notes".into(), fos.notes().len());
    decoded_counts.insert("said_infos".into(), fos.said_infos().len());
    match fos.radio_state() {
        Ok(Some(r)) => {
            decoded_counts.insert("radio_on".into(), usize::from(r.on));
        }
        Ok(None) => {}
        Err(_) => {
            *parse_errors.entry("radio".into()).or_default() += 1;
        }
    }

    let weather_current = match fos.weather() {
        Ok(Some(w)) => w.current.map(|id| format!("0x{id:08X}")),
        Ok(None) => None,
        Err(_) => {
            *parse_errors.entry("weather".into()).or_default() += 1;
            None
        }
    };
    let active_quest = fos.active_quest().map(|id| format!("0x{id:08X}"));

    let (player_has_place, player_has_special, player_tag_skill_count, player_av_parsed, player_inventory_items) =
        match fos.player() {
            Ok(p) => {
                decoded_counts.insert("player_base_av_overrides".into(), p.base_av_overrides.len());
                decoded_counts.insert("player_skill_points".into(), p.skill_points.len());
                decoded_counts.insert("player_perks".into(), p.perks.len());
                decoded_counts.insert("player_teammate_perks".into(), p.teammate_perks.len());
                decoded_counts.insert("player_teammates".into(), p.teammates.len());
                decoded_counts.insert("player_factions".into(), p.factions.len());
                decoded_counts.insert("player_spells".into(), p.spells.len());
                if let Some(rounds) = p.active_weapon_magazine {
                    decoded_counts.insert("player_active_weapon_magazine".into(), rounds as usize);
                }
                (
                    p.place.is_some(),
                    p.special.is_some(),
                    p.tag_skills.len(),
                    p.av_parsed,
                    p.inventory.as_ref().map(Vec::len),
                )
            }
            Err(_) => {
                *parse_errors.entry("player".into()).or_default() += 1;
                (false, false, 0, false, None)
            }
        };

    FosInspectReport {
        schema: INSPECT_FORMAT.into(),
        source: FosSummary::from_fos(fos),
        cosave_present,
        global_tables: fos.globals_data.iter().map(|(t, d)| (*t, d.len())).collect(),
        changed_by_kind,
        decoded_counts,
        parse_errors,
        active_quest,
        weather_current,
        player_has_place,
        player_has_special,
        player_tag_skill_count,
        player_av_parsed,
        player_inventory_items,
    }
}

fn partition_results<T, E>(v: Vec<Result<T, E>>) -> (usize, usize) {
    let mut ok = 0;
    let mut err = 0;
    for item in v {
        if item.is_ok() {
            ok += 1;
        } else {
            err += 1;
        }
    }
    (ok, err)
}

/// Convert a parsed `.fos` save into an NVForever `.nvfsave` payload over a loaded plugin order.
pub fn convert_fos(
    fos: &Fos,
    data: &GameData,
    forms: &FormStore,
    refs: &RefTable,
    opts: &ConvertOptions,
) -> Result<ConvertedSave, String> {
    let saved_plugins: Vec<PluginEntry> = fos
        .plugins
        .iter()
        .map(|name| PluginEntry { name: name.clone(), size: 0 })
        .collect();
    let current_names: Vec<String> = opts.plugins.iter().map(|p| p.name.clone()).collect();
    let remap = PluginRemap::from_names(&saved_plugins, &current_names);
    let missing_plugins = remap.missing();

    let map_id = |id: u32| -> Result<FormId, &'static str> {
        if id == 0 {
            return Err("zero form ID");
        }
        if id >> 24 == 0xFF {
            return Err("created form (0xFF)");
        }
        remap.map(id).map(FormId).ok_or("form of a plugin not loaded")
    };

    let mut converted: BTreeMap<String, usize> = BTreeMap::new();
    let mut hypothesis_fields: BTreeMap<String, usize> = BTreeMap::new();
    let mut skipped: BTreeMap<String, usize> = BTreeMap::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut inc_ok = |k: &str| {
        *converted.entry(k.to_string()).or_default() += 1;
    };
    let mut inc_hyp = |k: &str| {
        *hypothesis_fields.entry(k.to_string()).or_default() += 1;
    };
    let mut inc_skip = |what: &str, why: &str| {
        *skipped.entry(format!("{what}: {why}")).or_default() += 1;
    };

    if opts.cosave_present {
        warnings.push("adjacent .nvse co-save detected and ignored (NVForever does not import xNVSE co-saves)".into());
    }
    if !missing_plugins.is_empty() {
        warnings.push(format!(
            "save references {} plugin(s) not in the loaded order: {}",
            missing_plugins.len(),
            missing_plugins.join(", ")
        ));
    }

    let world = World::new(forms, refs, SimConfig::default());
    let mut g = Game::new(data, world, GameConfig::default());

    if let Some(secs) = fos.header.play_seconds().filter(|s| s.is_finite() && *s >= 0.0) {
        g.time = secs;
    }

    // 1. Global table 1 type 3: global variables (including GameYear/Month/Day/Hour/DaysPassed/TimeScale).
    match fos.global_variables() {
        Ok(globals) => {
            for (raw_id, val) in globals {
                if !val.is_finite() {
                    inc_skip("global", "non-finite value");
                    continue;
                }
                match map_id(raw_id) {
                    Ok(id) if forms.globals.contains_key(&id) => {
                        if g.world.set_global(id, val).is_ok() {
                            inc_ok("globals");
                        } else {
                            inc_skip("global", "set_global rejected");
                        }
                    }
                    Ok(_) => inc_skip("global", "no GLOB record"),
                    Err(why) => inc_skip("global", why),
                }
            }
        }
        Err(_) => inc_skip("global_table", "type 3 (globals) malformed"),
    }

    // 2. Global table 1 type 0: misc statistics (43 entries in code order; [observed]).
    match fos.misc_stats() {
        Ok(stats) => {
            for (code, val) in stats.into_iter().enumerate() {
                if val > 0 {
                    if let Ok(code) = u16::try_from(code) {
                        g.player.misc_stats.insert(code, f64::from(val));
                        inc_ok("misc_stats");
                    }
                }
            }
        }
        Err(_) => inc_skip("global_table", "type 0 (misc stats) malformed"),
    }

    // 2b. Global table 1 type 2 prefix: per-base-form death counts ([observed]; 105B tail unmapped).
    match fos.dead_counts_with_tail() {
        Ok((counts, trailing)) => {
            for (raw_form, count) in counts {
                if count == 0 {
                    continue;
                }
                match raw_form.ok_or("not in save form ID array").and_then(&map_id) {
                    Ok(id) if matches!(data.sig(id).as_ref(), Some(b"NPC_") | Some(b"CREA")) => {
                        g.player.dead_count.insert(id, u32::from(count));
                        inc_ok("dead_counts");
                    }
                    Ok(_) => inc_skip("dead_count", "not an NPC_ or CREA record"),
                    Err(why) => inc_skip("dead_count", why),
                }
            }
            if trailing > 0 {
                inc_skip("global_table", "type 2 trailing bytes unmapped");
            }
        }
        Err(_) => inc_skip("global_table", "type 2 (dead counts) malformed"),
    }

    // 3. Global table 1 type 8: sky/weather ([observed]).
    let mut header_weather: Option<String> = None;
    let mut engine = EngineSnapshot::new();
    match fos.weather() {
        Ok(Some(w)) => {
            if let Some(raw_w) = w.current {
                match map_id(raw_w) {
                    Ok(id) if data.sig(id).as_ref() == Some(b"WTHR") => {
                        g.set_shown_weather(id, false);
                        let edid = data.edid_of.get(&id).cloned();
                        if let Some(ref e) = edid {
                            engine.extra.insert("sky".into(), serde_json::json!({ "weather": e }));
                        }
                        header_weather = edid;
                        inc_ok("weather");
                    }
                    Ok(_) => inc_skip("weather", "not a WTHR record"),
                    Err(why) => inc_skip("weather", why),
                }
            }
        }
        Ok(None) => {}
        Err(_) => inc_skip("global_table", "type 8 (weather) malformed"),
    }

    // 3b. Global table 1 type 10: Pip-Boy radio playback state ([observed] for active-station list 2;
    // [hypothesis] for header fallback).
    let mut radio_decoded = false;
    match fos.radio_state() {
        Ok(Some(radio)) => {
            radio_decoded = true;
            g.radio.pipboy.on = radio.on;
            inc_ok("radio_state");
            if !radio.from_active_list {
                inc_hyp("radio_state (global type 10 header fallback) [hypothesis]");
            }
            if let Some(raw_st) = radio.station {
                match map_id(raw_st) {
                    Ok(sid) if refs.get(sid).is_some() => {
                        g.radio.pipboy.station = Some(sid);
                        inc_ok("radio_station");
                        if !radio.from_active_list {
                            inc_hyp("radio_station (global type 10 header fallback) [hypothesis]");
                        }
                    }
                    Ok(_) => inc_skip("radio_station", "not a placed reference of load order"),
                    Err(why) => inc_skip("radio_station", why),
                }
            }
        }
        Ok(None) => {}
        Err(_) => inc_skip("global_table", "type 10 (radio) malformed"),
    }

    // Count unmapped global data table entries truthfully.
    for &(ty, _) in &fos.globals_data {
        if !matches!(ty, 0 | 1 | 2 | 3 | 8 | 1000) && !(ty == 10 && radio_decoded) {
            inc_skip("global_table", &format!("type {ty} layout unmapped"));
        }
    }

    // 4. Quests (flags, delay, stages, objectives, script variables).
    for qres in fos.quests() {
        let Ok(q) = qres else {
            inc_skip("quest", "record layout not understood");
            continue;
        };
        let qid = match map_id(q.form) {
            Ok(id) if data.quests.contains_key(&id) => id,
            Ok(_) => {
                inc_skip("quest", "no QUST record");
                continue;
            }
            Err(why) => {
                inc_skip("quest", why);
                continue;
            }
        };
        let script_id = data.quests[&qid].script;
        for (idx, val) in &q.vars {
            let Some(sid) = script_id else {
                inc_skip("quest_variable", "quest has no script");
                continue;
            };
            let var_idx = u16::try_from(*idx).ok();
            let declared = var_idx.and_then(|i| data.script(sid)?.var_name(i)).is_some();
            if !declared {
                inc_skip("quest_variable", "not declared in script");
                continue;
            }
            let i = var_idx.expect("checked");
            match val {
                VarValue::Num(x) => {
                    if x.is_finite() {
                        g.store(InstKey::Quest { quest: qid }, Some(sid), i, Value::Num(*x), "fos_convert", false);
                        inc_ok("quest_vars");
                    } else {
                        inc_skip("quest_variable", "non-finite value");
                    }
                }
                VarValue::Ref(None) => {
                    g.store(InstKey::Quest { quest: qid }, Some(sid), i, Value::Ref(0), "fos_convert", false);
                    inc_ok("quest_vars");
                }
                VarValue::Ref(Some(raw_r)) => match map_id(*raw_r) {
                    Ok(r) => {
                        g.store(InstKey::Quest { quest: qid }, Some(sid), i, Value::Ref(r.0), "fos_convert", false);
                        inc_ok("quest_vars");
                    }
                    Err(why) => inc_skip("quest_ref_variable", why),
                },
                VarValue::Unresolved => inc_skip("quest_ref_variable", "not in save form ID array"),
            }
        }
        let qs = g.quest(qid);
        if let Some(r) = q.running() {
            qs.running = r;
        }
        if let Some(c) = q.completed() {
            qs.completed = c;
            if !c {
                qs.failed = false;
            }
        }
        if let Some(delay) = q.delay {
            if delay.is_finite() && delay >= 0.0 {
                qs.delay = Some(f64::from(delay));
            } else if !delay.is_finite() {
                inc_skip("quest_delay", "non-finite value");
            }
        }
        for &(stage, done) in &q.stages {
            if done {
                qs.stages_done.insert(i32::from(stage));
            } else {
                qs.stages_done.remove(&i32::from(stage));
            }
        }
        for &(index, state) in &q.objectives {
            let entry = qs.objectives.entry(index as i32).or_default();
            entry.displayed = state & 1 != 0;
            entry.completed = state & 2 != 0;
        }
        inc_ok("quests");
    }

    // 5. Active quest (global table 1 type 1; [observed]).
    let loc_global = fos.player_location_global();
    if let Some(raw_q) = loc_global.as_ref().and_then(|l| l.active_quest) {
        match map_id(raw_q) {
            Ok(qid) if data.quests.contains_key(&qid) => {
                g.ui.active_quest = Some(qid);
                engine.active_quest = Some(qid.0);
                inc_ok("active_quest");
            }
            Ok(_) => inc_skip("active_quest", "no QUST record"),
            Err(why) => inc_skip("active_quest", why),
        }
    }

    // 6. Player identity, level, SPECIAL, tag skills, XP/Karma, inventory, and place.
    if !fos.header.player_name.is_empty() {
        g.player.name = Some(fos.header.player_name.clone());
        g.name_overrides.insert(FormId(PLAYER_BASE), fos.header.player_name.clone());
    }
    if fos.header.level >= 1 {
        g.player.level = Some(fos.header.level as i32);
    }

    let mut header_place = Place::default();
    match fos.player() {
        Ok(p) => {
            if let Some(female) = p.female {
                g.player.female = Some(female);
                inc_ok("player_sex");
                if !p.female_from_bit24 {
                    inc_hyp("player_sex (NPC_ bit 1 ACBS) [hypothesis]");
                }
            }
            for raw_sp in &p.spells {
                match raw_sp.ok_or("not in save form ID array").and_then(&map_id) {
                    Ok(sid) if data.sig(sid).as_ref() == Some(b"SPEL") => {
                        let list = &mut g.extra.entry(FormId::PLAYER_REF).or_default().spells;
                        if !list.contains(&sid) {
                            list.push(sid);
                        }
                        inc_ok("player_spells");
                    }
                    Ok(_) => inc_skip("player_spell", "not a SPEL record"),
                    Err(why) => inc_skip("player_spell", why),
                }
            }
            if let Some(special) = p.special {
                for (i, &val) in special.iter().enumerate() {
                    let code = AV_STRENGTH + i as u16;
                    g.extra.entry(FormId::PLAYER_REF).or_default().actor_values.insert(code, f64::from(val));
                    let _ = g.world.set_actor_value(FormId::PLAYER_REF, SPECIAL[i], f32::from(val));
                    inc_ok("player_special");
                }
            }
            if let Some(ref app) = p.appearance {
                let morphs_finite = app.fggs.iter().all(|v| v.is_finite())
                    && app.fgga.iter().all(|v| v.is_finite())
                    && app.fgts.iter().all(|v| v.is_finite())
                    && app.hair_length.is_finite();
                if !morphs_finite {
                    inc_skip("player_appearance", "non-finite FaceGen morph");
                } else if let Some(race) = data
                    .actors
                    .get(&FormId(PLAYER_BASE))
                    .and_then(|a| a.race)
                    .or_else(|| (data.sig(FormId(0x19)).as_ref() == Some(b"RACE")).then_some(FormId(0x19)))
                {
                    let hair = app.hair.and_then(|id| map_id(id).ok()).filter(|&id| data.sig(id).as_ref() == Some(b"HAIR"));
                    let eyes = app.eyes.and_then(|id| map_id(id).ok()).filter(|&id| data.sig(id).as_ref() == Some(b"EYES"));
                    let head_parts = app
                        .head_parts
                        .iter()
                        .filter_map(|opt| (*opt).and_then(|id| map_id(id).ok()))
                        .filter(|&id| data.sig(id).as_ref() == Some(b"HDPT"))
                        .collect();
                    g.player.look = Some(Look {
                        race,
                        female: p.female.unwrap_or(false),
                        hair,
                        hair_color: Some(app.hair_color),
                        eyes,
                        head_parts,
                        fggs: app.fggs.clone(),
                        fgga: app.fgga.clone(),
                        fgts: app.fgts.clone(),
                    });
                    inc_ok("player_appearance");
                }
            }
            if !p.tag_skills.is_empty() {
                for (slot, &raw_code) in p.tag_skills.iter().enumerate() {
                    if let Ok(code) = u16::try_from(raw_code) {
                        g.player.tag_skills.insert(code);
                        if let Ok(s) = u8::try_from(slot) {
                            g.player.tag_slots.insert(s, code);
                        }
                        inc_ok("player_tag_skills");
                    }
                }
            }
            // Active effect AV1 modifiers come from equipped items/effects; do not double-count.
            for _ in &p.av_modifiers {
                inc_skip("player_av_modifier", "active-effect modifier (re-applied by worn gear)");
            }
            if p.av_parsed {
                let mut sync_carry_needed = false;
                for &(code, val) in &p.av_base {
                    if !val.is_finite() {
                        inc_skip("player_av", "non-finite value");
                        continue;
                    }
                    let code_u16 = code as u16;
                    match code_u16 {
                        AV_XP if val >= 0.0 => {
                            g.player.xp = f64::from(val);
                            inc_ok("player_xp");
                        }
                        AV_KARMA => {
                            g.extra
                                .entry(FormId::PLAYER_REF)
                                .or_default()
                                .actor_values
                                .insert(AV_KARMA, f64::from(val));
                            inc_ok("player_karma");
                        }
                        c if av_grows(c) => {
                            // Vanilla ModAV writes radiation (54) and hardcore needs (73..=75)
                            // into AV2 (`av_base`) [observed: vanilla 1.4.0.525 A/B save pairs].
                            if val > 0.0 {
                                *g.extra
                                    .entry(FormId::PLAYER_REF)
                                    .or_default()
                                    .actor_values
                                    .entry(c)
                                    .or_insert(0.0) += f64::from(val);
                                if c == RAD_LEVEL {
                                    inc_ok("player_rads");
                                } else {
                                    inc_ok("player_hardcore_needs");
                                }
                            }
                        }
                        c if c < 77 => {
                            g.extra
                                .entry(FormId::PLAYER_REF)
                                .or_default()
                                .av_permanent
                                .insert(c, f64::from(val));
                            if SPECIALS.contains(&c) {
                                let i = (c - AV_STRENGTH) as usize;
                                let base_sp = p.special.map_or(5.0, |s| f32::from(s[i]));
                                let _ = g.world.set_actor_value(FormId::PLAYER_REF, SPECIAL[i], base_sp + val);
                                sync_carry_needed = true;
                            } else if c == CARRY_WEIGHT {
                                sync_carry_needed = true;
                            }
                            inc_ok("player_av_permanent");
                        }
                        _ => {}
                    }
                }
                if sync_carry_needed {
                    crate::functions::actor::sync_carry(&mut g, FormId::PLAYER_REF, CARRY_WEIGHT);
                }
                // AV3: current damage deltas (negative for AP/limbs/etc.) [observed] and any
                // positive growing rads/needs fallback [hypothesis].
                for &(code, val) in &p.av_damage {
                    if !val.is_finite() {
                        inc_skip("player_av_damage", "non-finite value");
                        continue;
                    }
                    let code_u16 = code as u16;
                    if av_grows(code_u16) {
                        if val > 0.0 {
                            *g.extra
                                .entry(FormId::PLAYER_REF)
                                .or_default()
                                .actor_values
                                .entry(code_u16)
                                .or_insert(0.0) += f64::from(val);
                            if code_u16 == RAD_LEVEL {
                                inc_ok("player_rads");
                                inc_hyp("player_rads (AV3 code 54) [hypothesis]");
                            } else {
                                inc_ok("player_hardcore_needs");
                                inc_hyp("player_hardcore_needs (AV3 codes 73-75) [hypothesis]");
                            }
                        }
                    } else if val < 0.0 {
                        g.extra
                            .entry(FormId::PLAYER_REF)
                            .or_default()
                            .av_damage
                            .insert(code_u16, f64::from(-val));
                        if LIMB_CONDITIONS.contains(&code_u16) {
                            inc_ok("player_limb_damage");
                        } else {
                            inc_ok("player_av_damage");
                        }
                    }
                }
                // AV entry 231: player Health damage delta (negative when injured) [observed].
                if let Some(hd) = p.health_damage {
                    if !hd.is_finite() {
                        inc_skip("player_health_damage", "non-finite value");
                    } else if hd < 0.0 {
                        g.extra
                            .entry(FormId::PLAYER_REF)
                            .or_default()
                            .av_damage
                            .insert(HEALTH, f64::from(-hd));
                        inc_ok("player_health_damage");
                    }
                }
            } else if fos.changed.iter().any(|c| c.form == PLAYER_REF && c.kind == TYPE_ACHR) {
                inc_skip("player_av_array", "player record layout after place not recognized");
                warnings.push("player AV/inventory block had an unmapped change-flag layout and was skipped".into());
            }

            if let Some(sc) = p.scale {
                if sc.is_finite() && sc > 0.0 && (sc - 1.0).abs() > 1e-5 {
                    if let Ok(st) = g.world.get(FormId::PLAYER_REF) {
                        st.scale = Some(sc);
                        inc_ok("player_scale");
                    }
                }
            }

            for &(raw_fac, rank) in &p.factions {
                match raw_fac.ok_or("not in save form ID array").and_then(&map_id) {
                    Ok(fid) if data.sig(fid).as_ref() == Some(b"FACT") => {
                        g.extra.entry(FormId::PLAYER_REF).or_default().factions.insert(fid, rank);
                        inc_ok("player_factions");
                    }
                    Ok(_) => inc_skip("player_faction", "not a FACT record"),
                    Err(why) => inc_skip("player_faction", why),
                }
            }

            for &raw_tm in &p.teammates {
                match raw_tm.ok_or("not in save form ID array").and_then(&map_id) {
                    Ok(tid) if refs.get(tid).is_some() => {
                        g.extra.entry(tid).or_default().teammate = true;
                        inc_ok("player_teammates");
                    }
                    Ok(_) => inc_skip("player_teammate", "not a placed reference of load order"),
                    Err(why) => inc_skip("player_teammate", why),
                }
            }

            let mut bit22_carry_sync = false;
            for &(code, val) in &p.base_av_overrides {
                if !val.is_finite() {
                    inc_skip("player_base_av_override", "non-finite value");
                    continue;
                }
                let code_u16 = u16::from(code);
                if (32..=45).contains(&code) {
                    if val >= 0.0 {
                        g.player.skill_points.insert(code_u16, f64::from(val));
                        inc_ok("player_skill_points");
                    }
                } else if code_u16 < 77 {
                    g.extra
                        .entry(FormId::PLAYER_REF)
                        .or_default()
                        .actor_values
                        .insert(code_u16, f64::from(val));
                    if SPECIALS.contains(&code_u16) {
                        let i = (code_u16 - AV_STRENGTH) as usize;
                        let perm = g
                            .extra
                            .get(&FormId::PLAYER_REF)
                            .and_then(|e| e.av_permanent.get(&code_u16))
                            .copied()
                            .unwrap_or(0.0) as f32;
                        let _ = g.world.set_actor_value(FormId::PLAYER_REF, SPECIAL[i], val + perm);
                        bit22_carry_sync = true;
                    } else if code_u16 == CARRY_WEIGHT {
                        bit22_carry_sync = true;
                    }
                    inc_ok("player_base_av_overrides");
                }
            }
            if bit22_carry_sync {
                crate::functions::actor::sync_carry(&mut g, FormId::PLAYER_REF, CARRY_WEIGHT);
            }

            for &(raw_perk, rank) in &p.perks {
                if rank == 0 {
                    continue;
                }
                match raw_perk.ok_or("not in save form ID array").and_then(&map_id) {
                    Ok(pid) if data.perks.contains_key(&pid) || data.sig(pid).as_ref() == Some(b"PERK") => {
                        g.player.perks.insert(pid, rank);
                        inc_ok("player_perks");
                    }
                    Ok(_) => inc_skip("player_perk", "not a PERK record"),
                    Err(why) => inc_skip("player_perk", why),
                }
            }

            for &(raw_perk, rank) in &p.teammate_perks {
                if rank == 0 {
                    continue;
                }
                match raw_perk.ok_or("not in save form ID array").and_then(&map_id) {
                    Ok(pid) if data.perks.contains_key(&pid) || data.sig(pid).as_ref() == Some(b"PERK") => {
                        g.player.teammate_perks.insert(pid);
                        inc_ok("player_teammate_perks");
                    }
                    Ok(_) => inc_skip("player_teammate_perk", "not a PERK record"),
                    Err(why) => inc_skip("player_teammate_perk", why),
                }
            }

            match &p.inventory {
                Some(items) => {
                    apply_player_inventory(
                        &mut g,
                        &mut engine,
                        forms,
                        items,
                        p.active_weapon_magazine,
                        &map_id,
                        &mut inc_ok,
                        &mut inc_hyp,
                        &mut inc_skip,
                    );
                }
                None => {
                    inc_skip("player_inventory", "not read (missing player record or unmapped layout)");
                }
            }

            // Player place: prefer PLAYER_REF place (has rotation), else global type 1 fallback.
            let chosen_place = p.place.or_else(|| {
                loc_global
                    .as_ref()
                    .and_then(|l| l.cell_or_world.map(|at| (at, l.pos, [0.0; 3])))
            });
            if let Some((raw_at, pos, rot)) = chosen_place {
                if !finite_transform(pos, rot) {
                    inc_skip("player_place", "non-finite transform");
                } else {
                    match map_id(raw_at) {
                        Ok(at) => {
                            if let Some(cell) = resolve_cell(refs, at, pos) {
                                if let Ok(st) = g.world.get(FormId::PLAYER_REF) {
                                    st.cell = Some(cell);
                                    st.pos = pos;
                                    st.rot = rot;
                                    g.player_entered(Some(cell));
                                    header_place = build_header_place(refs, loc_global.as_ref(), &map_id, cell, pos, rot);
                                    inc_ok("player_place");
                                } else {
                                    inc_skip("player_place", "player reference missing in world");
                                }
                            } else {
                                inc_skip("player_place", "cell or worldspace not present in load order");
                            }
                        }
                        Err(why) => inc_skip("player_place", why),
                    }
                }
            }
        }
        Err(_) => {
            inc_skip("player", "player record malformed");
            warnings.push("failed to decode player record".into());
        }
    }

    // 7. Placed reference changes (REFR, ACHR, ACRE with known flags 0..=2 layouts).
    for rres in fos.reference_changes() {
        let Ok(r) = rres else {
            inc_skip("reference", "change flags of unmapped layout");
            continue;
        };
        let rid = match map_id(r.form) {
            Ok(id) if refs.get(id).is_some() => id,
            Ok(_) => {
                inc_skip("reference", "not a placed reference of load order");
                continue;
            }
            Err(why) => {
                inc_skip("reference", why);
                continue;
            }
        };
        if let Some(flags) = r.form_flags {
            let _ = g.world.set_enabled(rid, flags & FORM_DISABLED == 0);
            let _ = g.world.set_deleted(rid, flags & FORM_DELETED != 0);
            if r.kind != TYPE_REFR {
                if flags & FORM_TALKED_TO_PC != 0 {
                    g.extra.entry(rid).or_default().talked_to_pc = true;
                }
                if flags & FORM_DEAD != 0 {
                    g.extra.entry(rid).or_default().dead = true;
                    inc_ok("actor_dead");
                }
            }
            inc_ok("references");
        }
        if let Some((raw_at, pos, rot)) = r.place {
            if !finite_transform(pos, rot) {
                inc_skip("reference_place", "non-finite transform");
            } else {
                match map_id(raw_at) {
                    Ok(at) => {
                        if let Some(cell) = resolve_cell(refs, at, pos) {
                            if let Ok(st) = g.world.get(rid) {
                                st.cell = Some(cell);
                                st.pos = pos;
                                st.rot = rot;
                                inc_ok("reference_places");
                            }
                        } else {
                            inc_skip("reference_place", "cell or worldspace not present in load order");
                        }
                    }
                    Err(why) => inc_skip("reference_place", why),
                }
            }
        }
    }

    // 7b. Container REFR inventories (bits 5/27, optional bit 28 tail; [hypothesis]).
    let mut handled_containers: BTreeSet<u32> = BTreeSet::new();
    for cres in fos.container_inventories() {
        let Ok(cont) = cres else {
            inc_skip("container", "inventory layout not understood");
            continue;
        };
        let rid = match map_id(cont.form) {
            Ok(id) if refs.get(id).is_some() => id,
            Ok(_) => {
                inc_skip("container", "not a placed reference of load order");
                continue;
            }
            Err(why) => {
                inc_skip("container", why);
                continue;
            }
        };
        let applied = apply_container_inventory(
            &mut g,
            forms,
            rid,
            &cont.items,
            &map_id,
            &mut inc_ok,
            &mut inc_hyp,
            &mut inc_skip,
        );
        let is_cont = refs.get(rid).is_some_and(|r| forms.sig(r.base).as_ref() == Some(b"CONT"));
        if applied || (is_cont && cont.items.iter().all(|it| it.count == 0)) {
            handled_containers.insert(cont.form);
            inc_ok("containers");
        }
    }

    // 7b2. Non-player actor state (flags/dead, scale, script vars 0x0D, factions 0x5E, inventory/worn gear).
    let mut handled_actors: BTreeSet<u32> = BTreeSet::new();
    for ac in fos.actor_changes() {
        let rid = match map_id(ac.form) {
            Ok(id) if refs.get(id).is_some() => id,
            Ok(_) => continue,
            Err(_) => continue,
        };
        let mut applied_any = false;
        if let Some(sc) = ac.scale {
            if !sc.is_finite() {
                inc_skip("actor_scale", "non-finite value");
            } else if sc > 0.0 && (sc - 1.0).abs() > 1e-5 {
                if let Ok(st) = g.world.get(rid) {
                    st.scale = Some(sc);
                    inc_ok("actor_scales");
                    applied_any = true;
                }
            }
        }
        for &(raw_fac, rank) in &ac.factions {
            match raw_fac.ok_or("not in save form ID array").and_then(&map_id) {
                Ok(fid) if data.sig(fid).as_ref() == Some(b"FACT") => {
                    g.extra.entry(rid).or_default().factions.insert(fid, rank);
                    inc_ok("actor_factions");
                    applied_any = true;
                }
                Ok(_) => inc_skip("actor_faction", "not a FACT record"),
                Err(why) => inc_skip("actor_faction", why),
            }
        }
        if let Some(ref sv) = ac.script_vars {
            if apply_ref_script_vars(
                &mut g,
                data,
                refs,
                rid,
                sv,
                &map_id,
                "actor_script_vars",
                "actor_script_vars (actor extra 0x0D) [hypothesis]",
                "actor_script_var",
                &mut inc_ok,
                &mut inc_hyp,
                &mut inc_skip,
            ) {
                applied_any = true;
            }
        }
        if let Some(ref items) = ac.inventory {
            if apply_actor_inventory(
                &mut g,
                forms,
                rid,
                items,
                &map_id,
                &mut inc_ok,
                &mut inc_hyp,
                &mut inc_skip,
            ) {
                inc_ok("actor_inventories");
                applied_any = true;
            }
        }
        if applied_any {
            handled_actors.insert(ac.form);
        }
    }

    // 7b3. Placed REFR extras: locks (0x2A), door open state (bit 23/28), owners (0x21, bit 6/31),
    // script variables (0x0D), and container inventories combined with extra lists.
    let mut handled_ref_extras: BTreeSet<u32> = BTreeSet::new();
    for re in fos.ref_extras() {
        let rid = match map_id(re.form) {
            Ok(id) if refs.get(id).is_some() => id,
            Ok(_) => continue,
            Err(_) => continue,
        };
        let mut applied_any = false;
        let base_sig = refs.get(rid).and_then(|r| forms.sig(r.base));
        if let Some(sc) = re.scale {
            if !sc.is_finite() {
                inc_skip("ref_scale", "non-finite value");
            } else if sc > 0.0 && (sc - 1.0).abs() > 1e-5 {
                if let Ok(st) = g.world.get(rid) {
                    st.scale = Some(sc);
                    inc_ok("ref_scales");
                    applied_any = true;
                }
            }
        }
        if let Some(lk) = re.lock {
            if matches!(base_sig.as_ref(), Some(b"DOOR" | b"CONT")) {
                let mapped_key = match lk.key {
                    Some(raw_k) => match map_id(raw_k) {
                        Ok(kid) if forms.item(kid).is_some() => Some(kid),
                        Ok(_) => {
                            inc_skip("lock_key", "not an inventory item");
                            None
                        }
                        Err(why) => {
                            inc_skip("lock_key", why);
                            None
                        }
                    },
                    None => None,
                };
                if let Ok(st) = g.world.get(rid) {
                    st.lock = Some(LockState { level: lk.level, key: mapped_key, locked: lk.is_locked() });
                    inc_ok("locks");
                    applied_any = true;
                }
            }
        }
        if let Some(open) = re.door_open {
            if base_sig.as_ref() == Some(b"DOOR") {
                if let Ok(st) = g.world.get(rid) {
                    st.open = open;
                    inc_ok("doors_open_state");
                    applied_any = true;
                }
            }
        }
        if let Some(raw_owner) = re.owner {
            match raw_owner.ok_or("not in save form ID array").and_then(&map_id) {
                Ok(oid) if matches!(data.sig(oid).as_ref(), Some(b"NPC_" | b"FACT")) => {
                    if let Ok(st) = g.world.get(rid) {
                        st.extra.owner = Some(oid);
                        inc_ok("ref_owners");
                        applied_any = true;
                    }
                }
                Ok(_) => inc_skip("ref_owner", "not an NPC_ or FACT record"),
                Err(why) => inc_skip("ref_owner", why),
            }
        }
        if let Some(ref sv) = re.script_vars {
            if apply_ref_script_vars(
                &mut g,
                data,
                refs,
                rid,
                sv,
                &map_id,
                "ref_script_vars",
                "ref_script_vars (REFR extra 0x0D) [hypothesis]",
                "ref_script_var",
                &mut inc_ok,
                &mut inc_hyp,
                &mut inc_skip,
            ) {
                applied_any = true;
            }
        }
        if let Some(ref items) = re.inventory {
            if !handled_containers.contains(&re.form) {
                let inv_ok = apply_container_inventory(
                    &mut g,
                    forms,
                    rid,
                    items,
                    &map_id,
                    &mut inc_ok,
                    &mut inc_hyp,
                    &mut inc_skip,
                );
                if inv_ok || (base_sig.as_ref() == Some(b"CONT") && items.iter().all(|it| it.count == 0)) {
                    handled_containers.insert(re.form);
                    inc_ok("containers");
                    applied_any = true;
                }
            }
        }
        if applied_any {
            handled_ref_extras.insert(re.form);
        }
    }

    // 7c. Map marker REFR discovery/visibility flags (bit 31 extra 0x2C; [observed]).
    let mut handled_map_markers: BTreeSet<u32> = BTreeSet::new();
    for mres in fos.map_markers() {
        let Ok(mm) = mres else {
            inc_skip("map_marker", "record layout not understood");
            continue;
        };
        handled_map_markers.insert(mm.form);
        let rid = match map_id(mm.form) {
            Ok(id) if refs.get(id).is_some_and(|r| r.map_marker.is_some() || r.base == FormId(0x10)) => id,
            Ok(_) => {
                inc_skip("map_marker", "not a MapMarker reference of load order");
                continue;
            }
            Err(why) => {
                inc_skip("map_marker", why);
                continue;
            }
        };
        if mm.flags & 0x01 != 0 {
            g.player.map_markers.insert(rid);
        }
        if mm.flags & 0x02 != 0 {
            g.player.travel_markers.insert(rid);
            g.player.found_markers.insert(rid);
        }
        inc_ok("map_markers");
    }
    if !g.player.found_markers.is_empty() {
        let found_ids: Vec<u32> = g.player.found_markers.iter().map(|id| id.0).collect();
        engine.extra.insert("map_markers_found".into(), serde_json::json!(found_ids));
    }

    // 7d. Created 0xFF REFR records (32-byte header -> World::place; [observed]).
    let mut handled_created_refs: BTreeSet<u32> = BTreeSet::new();
    for cr_res in fos.created_refs() {
        let Ok(cr) = cr_res else {
            inc_skip("created_ref", "32-byte header not understood");
            continue;
        };
        let base_id = match cr.base.ok_or("not in save form ID array").and_then(&map_id) {
            Ok(id) => id,
            Err(why) => {
                inc_skip("created_ref", why);
                continue;
            }
        };
        let Some((raw_at, pos, rot)) = cr.place else {
            inc_skip("created_ref", "missing cell or worldspace");
            continue;
        };
        if !finite_transform(pos, rot) {
            inc_skip("created_ref", "non-finite transform");
            continue;
        }
        let at = match map_id(raw_at) {
            Ok(id) => id,
            Err(why) => {
                inc_skip("created_ref", why);
                continue;
            }
        };
        let Some(cell) = resolve_cell(refs, at, pos) else {
            inc_skip("created_ref", "cell or worldspace not present in load order");
            continue;
        };
        if g.world.place(base_id, Some(cell), pos, rot, 1).is_ok() {
            handled_created_refs.insert(cr.form);
            inc_ok("created_refs");
        } else {
            inc_skip("created_ref", "base form not placeable in load order");
        }
    }

    // 8. Faction relations (FACT flag 2; [observed]).
    let mut factions_changed = false;
    for fres in fos.faction_relations() {
        let Ok(f) = fres else {
            inc_skip("faction", "record not understood");
            continue;
        };
        let fid = match map_id(f.form) {
            Ok(id) => id,
            Err(why) => {
                inc_skip("faction", why);
                continue;
            }
        };
        for (other, _modifier, reaction) in f.relations {
            match other.ok_or("not in save form ID array").and_then(&map_id) {
                Ok(oid) if reaction <= 3 => {
                    g.forms.faction_reactions.entry(fid).or_default().insert(oid, i64::from(reaction));
                    factions_changed = true;
                    inc_ok("faction_relations");
                }
                Ok(_) => inc_skip("faction_relation", "reaction outside 0..=3"),
                Err(why) => inc_skip("faction_relation", why),
            }
        }
    }
    if factions_changed {
        crate::functions::actor::forget_relations(&g);
    }

    // 9. Form lists (FLST flag 31; [observed]).
    for flres in fos.form_lists() {
        let Ok(fl) = flres else {
            inc_skip("form_list", "record not understood");
            continue;
        };
        let fl_id = match map_id(fl.form) {
            Ok(id) if data.sig(id).as_ref() == Some(b"FLST") => id,
            Ok(_) => {
                inc_skip("form_list", "no FLST record");
                continue;
            }
            Err(why) => {
                inc_skip("form_list", why);
                continue;
            }
        };
        let mut members = Vec::new();
        for entry in fl.forms {
            match entry.ok_or("not in save form ID array").and_then(&map_id) {
                Ok(id) => members.push(id),
                Err(why) => inc_skip("form_list_entry", why),
            }
        }
        if !members.is_empty() {
            g.form_lists.insert(fl_id, members);
            inc_ok("form_lists");
        }
    }

    // 10. Leveled item lists (LVLI flag 31; [observed]).
    for lvres in fos.leveled_lists() {
        let Ok(lv) = lvres else {
            inc_skip("leveled_list", "record not understood");
            continue;
        };
        let lv_id = match map_id(lv.form) {
            Ok(id) if forms.leveled(id).is_some() => id,
            Ok(_) => {
                inc_skip("leveled_list", "no LVLI record");
                continue;
            }
            Err(why) => {
                inc_skip("leveled_list", why);
                continue;
            }
        };
        let mut added_any = false;
        for entry in lv.entries {
            match entry.item.ok_or("not in save form ID array").and_then(&map_id) {
                Ok(item_id) => {
                    if !entry.condition.is_finite() {
                        inc_skip("leveled_list_condition", "non-finite value");
                    }
                    let le = LeveledEntry {
                        level: entry.level,
                        item: item_id,
                        count: entry.count.max(1),
                        condition: entry.condition.is_finite().then(|| entry.condition.clamp(0.0, 1.0)),
                    };
                    if g.world.add_to_leveled_list(lv_id, le).is_ok() {
                        added_any = true;
                        inc_ok("leveled_list_entries");
                    } else {
                        inc_skip("leveled_list_entry", "not a valid item or leveled list");
                    }
                }
                Err(why) => inc_skip("leveled_list_entry", why),
            }
        }
        if added_any {
            inc_ok("leveled_lists");
        }
    }

    // 11. Reputations (REPU flag 1: fame, infamy; [observed]).
    for rres in fos.reputations() {
        let Ok(rep) = rres else {
            inc_skip("reputation", "record not understood");
            continue;
        };
        let rid = match map_id(rep.form) {
            Ok(id) if data.reputations.contains_key(&id) || data.sig(id).as_ref() == Some(b"REPU") => id,
            Ok(_) => {
                inc_skip("reputation", "no REPU record");
                continue;
            }
            Err(why) => {
                inc_skip("reputation", why);
                continue;
            }
        };
        if rep.fame.is_finite() && rep.infamy.is_finite() {
            // PlayerState::reputation stores [infamy, fame] (axis 0 = infamy, axis 1 = fame).
            g.player.reputation.insert(rid, [f64::from(rep.infamy.max(0.0)), f64::from(rep.fame.max(0.0))]);
            inc_ok("reputations");
        } else {
            inc_skip("reputation", "non-finite value");
        }
    }

    // 12. Challenges (CHAL flag 1: progress, flags; [observed]).
    for cres in fos.challenges() {
        let Ok(ch) = cres else {
            inc_skip("challenge", "record not understood");
            continue;
        };
        let cid = match map_id(ch.form) {
            Ok(id) if data.challenges.contains_key(&id) || data.sig(id).as_ref() == Some(b"CHAL") => id,
            Ok(_) => {
                inc_skip("challenge", "no CHAL record");
                continue;
            }
            Err(why) => {
                inc_skip("challenge", why);
                continue;
            }
        };
        let goal = data.challenges.get(&cid).copied().unwrap_or(0).max(1);
        let st = g.player.challenges.entry(cid).or_default();
        st.progress = ch.progress;
        if ch.progress >= goal {
            st.completed = true;
        }
        inc_ok("challenges");
    }

    // 13. Notes: union of observed ACHR 0x14 tail notes list and [hypothesis] TYPE_NOTE bit 31.
    let mut seen_notes: BTreeSet<u32> = BTreeSet::new();
    for raw_note in fos.player_tail_notes() {
        seen_notes.insert(raw_note);
        match map_id(raw_note) {
            Ok(id) if data.notes.contains_key(&id) || data.sig(id).as_ref() == Some(b"NOTE") => {
                g.player.notes.insert(id);
                inc_ok("notes");
            }
            Ok(_) => inc_skip("note", "no NOTE record"),
            Err(why) => inc_skip("note", why),
        }
    }
    for raw_note in fos.note_records() {
        if !seen_notes.insert(raw_note) {
            continue;
        }
        match map_id(raw_note) {
            Ok(id) if data.notes.contains_key(&id) || data.sig(id).as_ref() == Some(b"NOTE") => {
                g.player.notes.insert(id);
                inc_ok("notes");
                inc_hyp("notes (NOTE bit 31) [hypothesis]");
            }
            Ok(_) => inc_skip("note", "no NOTE record"),
            Err(why) => inc_skip("note", why),
        }
    }

    // 14. Dialogue said-once (INFO flag 31 -> actorless FormId(0) entry, checked by Game::said_once; [observed]).
    for raw_info in fos.said_infos() {
        match map_id(raw_info) {
            Ok(id) if data.infos.contains_key(&id) || data.sig(id).as_ref() == Some(b"INFO") => {
                g.said.insert((FormId(0), id));
                inc_ok("said_infos");
            }
            Ok(_) => inc_skip("said_info", "no INFO record"),
            Err(why) => inc_skip("said_info", why),
        }
    }

    // 14b. Encounter zones (ECZN bit 31: locked zone level; [observed]).
    for ezres in fos.encounter_zones() {
        let Ok(ez) = ezres else {
            inc_skip("encounter_zone", "record not understood");
            continue;
        };
        let zid = match map_id(ez.form) {
            Ok(id) if forms.zones.contains_key(&id) || data.sig(id).as_ref() == Some(b"ECZN") => id,
            Ok(_) => {
                inc_skip("encounter_zone", "no ECZN record");
                continue;
            }
            Err(why) => {
                inc_skip("encounter_zone", why);
                continue;
            }
        };
        if let Some(level) = u16::try_from(ez.level).ok().filter(|&l| l >= 1) {
            g.world.set_zone_level(zid, level);
            inc_ok("encounter_zones");
        } else {
            inc_skip("encounter_zone", "zone level out of range");
        }
    }

    // 14c. Cells (CELL bits 3/30/31: visited cells, detach timestamps, cell owners; [observed]).
    let mut handled_cells: BTreeSet<u32> = BTreeSet::new();
    for cres in fos.cells() {
        let Ok(cell) = cres else {
            inc_skip("cell", "record not understood");
            continue;
        };
        let cid = match map_id(cell.form) {
            Ok(id) if data.sig(id).as_ref() == Some(b"CELL") => id,
            Ok(_) => {
                inc_skip("cell", "no CELL record");
                continue;
            }
            Err(why) => {
                inc_skip("cell", why);
                continue;
            }
        };
        let mut applied_any = false;
        if cell.detach_hours.is_some() || cell.has_fog {
            g.forms.visited_cells.insert(cid);
            inc_ok("visited_cells");
            applied_any = true;
        }
        if let Some(hours) = cell.detach_hours {
            let days = (hours as f32) / 24.0;
            g.world.set_cell_detached_at(cid, days);
            inc_ok("detached_cells");
            applied_any = true;
        }
        if let Some(raw_owner) = cell.owner {
            match raw_owner.ok_or("not in save form ID array").and_then(&map_id) {
                Ok(oid) if matches!(data.sig(oid).as_ref(), Some(b"NPC_" | b"FACT")) => {
                    g.forms.cell_owners.insert(cid, oid);
                    inc_ok("cell_owners");
                    applied_any = true;
                }
                Ok(_) => inc_skip("cell_owner", "not an NPC_ or FACT record"),
                Err(why) => inc_skip("cell_owner", why),
            }
        }
        if applied_any {
            handled_cells.insert(cell.form);
        }
    }

    // 15. Truthfully account for all remaining unmapped/unsupported changed-form records.
    for cf in &fos.changed {
        if cf.form >> 24 == 0xFF {
            if cf.kind == TYPE_REFR && handled_created_refs.contains(&cf.form) {
                continue;
            }
            inc_skip("changed_form", &format!("created {} (0xFF) unsupported", fnv_save::fos::kind_name(cf.kind)));
            continue;
        }
        let handled = match cf.kind {
            TYPE_QUST => true,
            TYPE_FACT => cf.flags & (1 << 2) != 0,
            TYPE_FLST | TYPE_LVLI | TYPE_ECZN => cf.flags & (1 << 31) != 0,
            TYPE_INFO | TYPE_NOTE => cf.flags & (1 << 31) != 0 && cf.data.is_empty(),
            TYPE_REPU | TYPE_CHAL => cf.flags & (1 << 1) != 0,
            TYPE_CELL => handled_cells.contains(&cf.form),
            TYPE_NPC => {
                cf.form == PLAYER_BASE
                    && (cf.flags & ((1 << 1) | (1 << 2) | (1 << 4) | (1 << 5) | (1 << 11) | (1 << 24)) != 0)
            }
            TYPE_CLAS => cf.form == fnv_save::fos::PLAYER_CLASS && (cf.flags & (1 << 1) != 0),
            TYPE_REFR => {
                (cf.flags & 0b111 != 0)
                    || ((cf.flags & ((1 << 5) | (1 << 27))) != 0
                        && (cf.flags & !((1 << 5) | (1 << 27) | (1 << 28))) == 0)
                    || handled_map_markers.contains(&cf.form)
                    || handled_ref_extras.contains(&cf.form)
            }
            TYPE_ACHR | TYPE_ACRE => cf.form == PLAYER_REF || (cf.flags & 0b111 != 0) || handled_actors.contains(&cf.form),
            _ => false,
        };
        if !handled {
            inc_skip("changed_form", &format!("{} (kind {}) unmapped", fnv_save::fos::kind_name(cf.kind), cf.kind));
        }
    }

    // Build validated GameSnapshot, SaveBody, SaveHeader, and optional PNG thumbnail.
    let snapshot = g.checked_snapshot()?;
    let mut header = SaveHeader::new(opts.kind);
    header.plugins = opts.plugins.clone();
    header.play_seconds = if snapshot.time.is_finite() { snapshot.time } else { 0.0 };
    if let Ok(cal) = g.world.calendar() {
        header.game_year = if cal.year.is_finite() { cal.year.max(0.0) as u32 } else { 0 };
        header.game_month = if cal.month.is_finite() { cal.month.max(0.0) as u32 } else { 0 };
        header.game_day = if cal.day.is_finite() { cal.day.max(0.0) as u32 } else { 0 };
        if cal.hour.is_finite() {
            header.game_hour = cal.hour;
        }
        if cal.days_passed.is_finite() {
            header.game_days_passed = Some(cal.days_passed);
        }
    }
    header.player_name = snapshot.player.name.clone().unwrap_or_else(|| "Courier".into());
    header.level = g.player_level().max(1.0) as u32;
    header.location = if !fos.header.location.is_empty() {
        fos.header.location.clone()
    } else {
        g.world
            .peek(FormId::PLAYER_REF)
            .and_then(|s| s.cell)
            .map(|c| data.name(c))
            .unwrap_or_default()
    };
    header.place = header_place;
    header.weather = header_weather;

    let shot = if opts.include_thumbnail && !fos.screenshot_rgb.is_empty() {
        match encode_rgb_png(&fos.screenshot_rgb, fos.header.screenshot_width, fos.header.screenshot_height) {
            Some(png) => {
                inc_ok("thumbnail_encoded");
                png
            }
            None => {
                inc_skip("thumbnail", "screenshot dimensions/bytes invalid for PNG encoding");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    if !hypothesis_fields.is_empty() {
        warnings.push(format!(
            "wrote {} [hypothesis] .fos field category/categories into .nvfsave (see hypothesis_fields and docs/spec-notes/save-files.md §6.6)",
            hypothesis_fields.len()
        ));
    }

    let body = SaveBody::new(snapshot, engine);
    let report = ConversionReport {
        schema: REPORT_FORMAT.into(),
        source: FosSummary::from_fos(fos),
        missing_plugins,
        cosave_ignored: opts.cosave_present,
        converted,
        hypothesis_fields,
        skipped,
        warnings,
    };

    Ok(ConvertedSave { header, shot, body, report })
}

fn finite_transform(pos: [f32; 3], rot: [f32; 3]) -> bool {
    pos.iter().all(|v| v.is_finite()) && rot.iter().all(|v| v.is_finite())
}

fn normalize_item_condition(forms: &FormStore, id: FormId, cond_hp: Option<f32>) -> Option<f32> {
    let hp = cond_hp?;
    let max_hp = forms.item(id)?.health.filter(|&h| h > 0)? as f32;
    hp.is_finite().then_some((hp / max_hp).clamp(0.0, 1.0)).filter(|v| v.is_finite())
}

#[allow(clippy::too_many_arguments)]
fn apply_container_inventory(
    g: &mut Game<'_>,
    forms: &FormStore,
    holder: FormId,
    items: &[FosItem],
    map_id: &impl Fn(u32) -> Result<FormId, &'static str>,
    inc_ok: &mut impl FnMut(&str),
    _inc_hyp: &mut impl FnMut(&str),
    inc_skip: &mut impl FnMut(&str, &str),
) -> bool {
    let mut applied_any_item = false;
    for it in items.iter().filter(|it| it.count != 0) {
        match it.item.ok_or("not in save form ID array").and_then(map_id) {
            Ok(id) => {
                if it.count > 0 {
                    let mut remaining = it.count;
                    let mut added_any = false;
                    let mut failed = false;
                    for entry in &it.entries {
                        if remaining <= 0 {
                            break;
                        }
                        let stack_n = entry.count.map(i32::from).filter(|&n| n > 0).unwrap_or(1).min(remaining);
                        if entry.condition.is_some_and(|hp| !hp.is_finite()) {
                            inc_skip("container_item_condition", "non-finite value");
                        }
                        let cond = normalize_item_condition(forms, id, entry.condition);
                        let owner = match entry.owner {
                            Some(Some(raw)) => match map_id(raw) {
                                Ok(oid) => Some(oid),
                                Err(why) => {
                                    inc_skip("container_item_owner", why);
                                    None
                                }
                            },
                            Some(None) => {
                                inc_skip("container_item_owner", "not in save form ID array");
                                None
                            }
                            None => None,
                        };
                        let extra = Extra {
                            condition: cond,
                            owner,
                            equipped: false,
                            mods: entry.weapon_mods,
                            live: false,
                        };
                        if g.world.add_item_extra(holder, id, stack_n, extra).is_ok() {
                            remaining -= stack_n;
                            added_any = true;
                        } else {
                            failed = true;
                            break;
                        }
                    }
                    if !failed && remaining > 0 {
                        if g.world.add_item(holder, id, remaining).is_ok() {
                            added_any = true;
                        } else {
                            failed = true;
                        }
                    }
                    if !failed && added_any {
                        applied_any_item = true;
                        inc_ok("container_items");
                    } else {
                        inc_skip("container_item", "unknown or non-container/non-inventory form");
                    }
                } else if let Some(remove) = it.count.checked_neg() {
                    if g.world.remove_item(holder, id, remove).is_ok() {
                        applied_any_item = true;
                        inc_ok("container_items");
                    } else {
                        inc_skip("container_item", "unknown or non-container/non-inventory form");
                    }
                } else {
                    inc_skip("container_item", "negative item count overflow");
                }
            }
            Err(why) => inc_skip("container_item", why),
        }
    }
    applied_any_item
}

#[allow(clippy::too_many_arguments)]
fn apply_player_inventory(
    g: &mut Game<'_>,
    engine: &mut EngineSnapshot,
    forms: &FormStore,
    items: &[FosItem],
    active_weapon_magazine: Option<u32>,
    map_id: &impl Fn(u32) -> Result<FormId, &'static str>,
    inc_ok: &mut impl FnMut(&str),
    inc_hyp: &mut impl FnMut(&str),
    inc_skip: &mut impl FnMut(&str, &str),
) {
    // Pass 1: count hotkeys (not persisted in NVFSAVE v1) and apply inventory deltas & sub-stacks.
    for it in items {
        for entry in &it.entries {
            if entry.hotkey.is_some() {
                inc_skip("player_hotkey", "not persisted in NVFSAVE v1");
            }
            if !entry.worn && entry.ammo.is_some() {
                inc_skip("player_weapon_magazine", "unequipped weapon magazine not persisted in NVFSAVE v1");
            }
        }
        if it.count == 0 {
            continue;
        }
        match it.item.ok_or("not in save form ID array").and_then(map_id) {
            Ok(id) => {
                if it.count > 0 {
                    let mut remaining = it.count;
                    let mut added_any = false;
                    let mut failed = false;
                    // Process worn entries first so a worn sub-stack always receives its exact
                    // condition/mods/owner before plain or unequipped sub-stacks exhaust `remaining`.
                    let ordered_entries: Vec<&FosItemEntry> = it
                        .entries
                        .iter()
                        .filter(|e| e.worn)
                        .chain(it.entries.iter().filter(|e| !e.worn))
                        .collect();
                    for entry in ordered_entries {
                        if remaining <= 0 {
                            break;
                        }
                        let stack_n = entry.count.map(i32::from).filter(|&n| n > 0).unwrap_or(1).min(remaining);
                        if entry.condition.is_some_and(|hp| !hp.is_finite()) {
                            inc_skip("player_item_condition", "non-finite value");
                        }
                        let cond = normalize_item_condition(forms, id, entry.condition);
                        let owner = match entry.owner {
                            Some(Some(raw)) => match map_id(raw) {
                                Ok(oid) => Some(oid),
                                Err(why) => {
                                    inc_skip("player_item_owner", why);
                                    None
                                }
                            },
                            Some(None) => {
                                inc_skip("player_item_owner", "not in save form ID array");
                                None
                            }
                            None => None,
                        };
                        let extra = Extra {
                            condition: cond,
                            owner,
                            equipped: false,
                            mods: entry.weapon_mods,
                            live: false,
                        };
                        if g.world.add_item_extra(FormId::PLAYER_REF, id, stack_n, extra).is_ok() {
                            remaining -= stack_n;
                            added_any = true;
                        } else {
                            failed = true;
                            break;
                        }
                    }
                    if !failed && remaining > 0 {
                        if g.world.add_item(FormId::PLAYER_REF, id, remaining).is_ok() {
                            added_any = true;
                        } else {
                            failed = true;
                        }
                    }
                    if !failed && added_any {
                        inc_ok("player_items");
                    } else {
                        inc_skip("player_item", "unknown or non-inventory form");
                    }
                } else if let Some(remove) = it.count.checked_neg() {
                    if g.world.remove_item(FormId::PLAYER_REF, id, remove).is_ok() {
                        inc_ok("player_items");
                    } else {
                        inc_skip("player_item", "unknown or non-inventory form");
                    }
                } else {
                    inc_skip("player_item", "negative item count overflow");
                }
            }
            Err(why) => inc_skip("player_item", why),
        }
    }

    // Pass 2: equip worn sub-stacks (after all counts are in place) and wire worn weapon magazine.
    for it in items.iter().filter(|it| it.worn) {
        match it.item.ok_or("not in save form ID array").and_then(map_id) {
            Ok(id) => {
                let worn_entries: Vec<&FosItemEntry> = it.entries.iter().filter(|e| e.worn).collect();
                for entry in worn_entries {
                    let cond = normalize_item_condition(forms, id, entry.condition);
                    let pick = Pick::row(cond, entry.weapon_mods);
                    let already = g
                        .world
                        .peek(FormId::PLAYER_REF)
                        .and_then(|s| s.inventory.as_ref())
                        .is_some_and(|inv| inv.is_equipped(id));
                    let equipped = g.world.equip_at(FormId::PLAYER_REF, id, pick).is_ok()
                        || already
                        || g.world.equip(FormId::PLAYER_REF, id).is_ok();
                    if equipped {
                        inc_ok("player_equipped");
                        if entry.worn_secondary {
                            inc_hyp("player_equipped_secondary (extra 0x17) [hypothesis]");
                        }
                        if let Some((raw_ammo, rounds)) = entry.ammo {
                            if forms.item(id).is_some_and(|item| item.kind == ItemKind::Weapon) {
                                let ammo_id = match raw_ammo {
                                    Some(raw) => match map_id(raw) {
                                        Ok(aid) if forms.item(aid).is_some_and(|item| item.kind == ItemKind::Ammo) => {
                                            Some(aid)
                                        }
                                        Ok(_) => {
                                            inc_skip("player_weapon_magazine_ammo", "not an AMMO record");
                                            None
                                        }
                                        Err(why) => {
                                            inc_skip("player_weapon_magazine_ammo", why);
                                            None
                                        }
                                    },
                                    None => None,
                                };
                                engine.magazine = Some((id.0, rounds));
                                engine.magazine_ammo = ammo_id.map(|a| a.0);
                                inc_ok("player_weapon_magazine");
                                inc_hyp("player_weapon_magazine (extra 0x6E) [hypothesis]");
                            }
                        } else if engine.magazine.is_none() {
                            if let Some(rounds) = active_weapon_magazine {
                                if forms.item(id).is_some_and(|item| {
                                    item.kind == ItemKind::Weapon
                                        && item.weapon_traits.is_some_and(|w| w.ammo.is_some())
                                }) {
                                    engine.magazine = Some((id.0, rounds));
                                    inc_ok("player_weapon_magazine");
                                }
                            }
                        }
                    } else {
                        inc_skip("player_equipped", "cannot equip item");
                    }
                }
            }
            Err(why) => inc_skip("player_equipped", why),
        }
    }
}

fn resolve_cell(refs: &RefTable, at: FormId, pos: [f32; 3]) -> Option<FormId> {
    if let Some(c) = refs.world_cell_at(at, pos) {
        return Some(c);
    }
    if refs.by_cell.contains_key(&at) || refs.world_of(at).is_some() {
        return Some(at);
    }
    None
}

fn build_header_place(
    refs: &RefTable,
    loc_global: Option<&fnv_save::fos::FosLocationGlobal>,
    map_id: &impl Fn(u32) -> Result<FormId, &'static str>,
    cell: FormId,
    pos: [f32; 3],
    rot: [f32; 3],
) -> Place {
    let cell_world = refs.world_of(cell).map(|w| w.0);
    let (world, interior) = match cell_world {
        Some(w) => (Some(w), None),
        None => {
            let last_world = loc_global.and_then(|l| l.world).and_then(|w| map_id(w).ok()).map(|w| w.0);
            (last_world, Some(cell.0))
        }
    };
    Place {
        world,
        interior,
        feet: pos,
        heading_deg: rot[2].to_degrees(),
        pitch_deg: rot[0].to_degrees(),
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_ref_script_vars(
    g: &mut Game<'_>,
    data: &GameData,
    refs: &RefTable,
    rid: FormId,
    sv: &FosScriptVars,
    map_id: &impl Fn(u32) -> Result<FormId, &'static str>,
    ok_key: &str,
    hyp_key: &str,
    skip_key: &str,
    inc_ok: &mut impl FnMut(&str),
    inc_hyp: &mut impl FnMut(&str),
    inc_skip: &mut impl FnMut(&str, &str),
) -> bool {
    let script_id = sv
        .script
        .and_then(|raw| map_id(raw).ok())
        .filter(|&sid| data.script(sid).is_some())
        .or_else(|| refs.get(rid).and_then(|r| data.script_of_base(r.base)).filter(|&sid| data.script(sid).is_some()));
    let Some(sid) = script_id else {
        if !sv.vars.is_empty() {
            inc_skip(skip_key, "reference has no script in load order");
        }
        return false;
    };
    let mut applied_any = false;
    for (idx, val) in &sv.vars {
        let var_idx = u16::try_from(*idx).ok();
        let declared = var_idx.and_then(|i| data.script(sid)?.var_name(i)).is_some();
        if !declared {
            inc_skip(skip_key, "not declared in script");
            continue;
        }
        let i = var_idx.expect("checked");
        match val {
            VarValue::Num(x) => {
                if x.is_finite() {
                    g.store(InstKey::Ref { reference: rid }, Some(sid), i, Value::Num(*x), "fos_convert", false);
                    inc_ok(ok_key);
                    inc_hyp(hyp_key);
                    applied_any = true;
                } else {
                    inc_skip(skip_key, "non-finite value");
                }
            }
            VarValue::Ref(None) => {
                g.store(InstKey::Ref { reference: rid }, Some(sid), i, Value::Ref(0), "fos_convert", false);
                inc_ok(ok_key);
                inc_hyp(hyp_key);
                applied_any = true;
            }
            VarValue::Ref(Some(raw_r)) => match map_id(*raw_r) {
                Ok(r) => {
                    g.store(InstKey::Ref { reference: rid }, Some(sid), i, Value::Ref(r.0), "fos_convert", false);
                    inc_ok(ok_key);
                    inc_hyp(hyp_key);
                    applied_any = true;
                }
                Err(why) => inc_skip(skip_key, why),
            },
            VarValue::Unresolved => inc_skip(skip_key, "not in save form ID array"),
        }
    }
    applied_any
}

#[allow(clippy::too_many_arguments)]
fn apply_actor_inventory(
    g: &mut Game<'_>,
    forms: &FormStore,
    holder: FormId,
    items: &[FosItem],
    map_id: &impl Fn(u32) -> Result<FormId, &'static str>,
    inc_ok: &mut impl FnMut(&str),
    inc_hyp: &mut impl FnMut(&str),
    inc_skip: &mut impl FnMut(&str, &str),
) -> bool {
    let mut applied_any = false;
    for it in items.iter().filter(|it| it.count != 0) {
        match it.item.ok_or("not in save form ID array").and_then(map_id) {
            Ok(id) => {
                if it.count > 0 {
                    let mut remaining = it.count;
                    let mut added_any = false;
                    let mut failed = false;
                    let ordered_entries: Vec<&FosItemEntry> = it
                        .entries
                        .iter()
                        .filter(|e| e.worn)
                        .chain(it.entries.iter().filter(|e| !e.worn))
                        .collect();
                    for entry in ordered_entries {
                        if remaining <= 0 {
                            break;
                        }
                        let stack_n = entry.count.map(i32::from).filter(|&n| n > 0).unwrap_or(1).min(remaining);
                        if entry.condition.is_some_and(|hp| !hp.is_finite()) {
                            inc_skip("actor_item_condition", "non-finite value");
                        }
                        let cond = normalize_item_condition(forms, id, entry.condition);
                        let owner = match entry.owner {
                            Some(Some(raw)) => match map_id(raw) {
                                Ok(oid) => Some(oid),
                                Err(why) => {
                                    inc_skip("actor_item_owner", why);
                                    None
                                }
                            },
                            Some(None) => {
                                inc_skip("actor_item_owner", "not in save form ID array");
                                None
                            }
                            None => None,
                        };
                        let extra = Extra {
                            condition: cond,
                            owner,
                            equipped: false,
                            mods: entry.weapon_mods,
                            live: false,
                        };
                        if g.world.add_item_extra(holder, id, stack_n, extra).is_ok() {
                            remaining -= stack_n;
                            added_any = true;
                        } else {
                            failed = true;
                            break;
                        }
                    }
                    if !failed && remaining > 0 {
                        if g.world.add_item(holder, id, remaining).is_ok() {
                            added_any = true;
                        } else {
                            failed = true;
                        }
                    }
                    if !failed && added_any {
                        applied_any = true;
                        inc_ok("actor_items");
                    } else {
                        inc_skip("actor_item", "unknown or non-inventory form");
                    }
                } else if let Some(remove) = it.count.checked_neg() {
                    if g.world.remove_item(holder, id, remove).is_ok() {
                        applied_any = true;
                        inc_ok("actor_items");
                    } else {
                        inc_skip("actor_item", "unknown or non-inventory form");
                    }
                } else {
                    inc_skip("actor_item", "negative item count overflow");
                }
            }
            Err(why) => inc_skip("actor_item", why),
        }
    }

    let mut equipped_any = false;
    for it in items.iter().filter(|it| it.worn) {
        match it.item.ok_or("not in save form ID array").and_then(map_id) {
            Ok(id) => {
                for entry in it.entries.iter().filter(|e| e.worn) {
                    let cond = normalize_item_condition(forms, id, entry.condition);
                    let pick = Pick::row(cond, entry.weapon_mods);
                    let already = g
                        .world
                        .peek(holder)
                        .and_then(|s| s.inventory.as_ref())
                        .is_some_and(|inv| inv.is_equipped(id));
                    let equipped = g.world.equip_at(holder, id, pick).is_ok()
                        || already
                        || g.world.equip(holder, id).is_ok();
                    if equipped {
                        equipped_any = true;
                        applied_any = true;
                        inc_ok("actor_equipped");
                        if entry.worn_secondary {
                            inc_hyp("actor_equipped_secondary (extra 0x17) [hypothesis]");
                        }
                    } else {
                        inc_skip("actor_equipped", "cannot equip item");
                    }
                }
            }
            Err(why) => inc_skip("actor_equipped", why),
        }
    }
    if equipped_any {
        if let Ok(st) = g.world.get(holder) {
            st.outfitted = true;
        }
    }
    applied_any
}
