//! `fnvtool world terminals`: the terminals of a load order (difficulty, flags, menu
//! items, scripts, password notes, placed references), and `fnvtool world hack`: one
//! hacking game laid out as the screen shows it (docs/spec-notes/hacking.md). The
//! survey prints counts and Editor IDs only; `hack` prints words of the game's word
//! list, for the owner's own console.

use crate::esm_cmd::open_load_order;
use crate::WorldArgs;
use anyhow::Context;
use fnv_esm::decode::formid_str;
use fnv_esm::{LoadOrder, Schema};
use fnv_game::cond::RunOn;
use fnv_game::data::terminal_flag;
use fnv_game::hacking::{self, Dictionary, Difficulty, Puzzle, Settings};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::RefTable;
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// The game's word list in its archives [observed: `Fallout - Misc.bsa`].
pub const DICTIONARY: &str = "menus\\falloutdict.txt";

fn count<K: Ord + ToString>(m: &BTreeMap<K, u64>) -> Value {
    Value::Object(m.iter().map(|(k, v)| (k.to_string(), json!(v))).collect::<Map<_, _>>())
}

pub fn survey(w: &WorldArgs, functions: Option<&std::path::Path>) -> anyhow::Result<Value> {
    let schema = Schema::load_dir(&w.schema).with_context(|| w.schema.display().to_string())?;
    let plugins = open_load_order(&w.lo)?;
    let lo = LoadOrder::new(&plugins)?;
    let tp = functions.map(|p| p.to_path_buf()).unwrap_or_else(|| w.schema.join("..").join("script").join("functions.json"));
    let ftext = std::fs::read_to_string(&tp).with_context(|| tp.display().to_string())?;
    let fjson: Value = serde_json::from_str(&ftext).with_context(|| tp.display().to_string())?;
    let table = FunctionTable::load(&tp).with_context(|| tp.display().to_string())?;
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &fjson, &refs);

    let mut difficulty: BTreeMap<String, u64> = BTreeMap::new();
    let mut flags: BTreeMap<&str, u64> = BTreeMap::new();
    let mut server: BTreeMap<String, u64> = BTreeMap::new();
    let mut run_on: BTreeMap<String, u64> = BTreeMap::new();
    let mut functions_used: BTreeMap<u16, u64> = BTreeMap::new();
    let (mut items, mut scripts, mut notes, mut add_notes, mut submenus, mut conditioned, mut with_password) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let mut submenu_of: BTreeMap<u32, u64> = BTreeMap::new();
    for t in data.terminals.values() {
        *difficulty.entry(Difficulty::from_code(t.difficulty).name().to_string()).or_default() += 1;
        for (bit, name) in [
            (terminal_flag::LEVELED, "Leveled"),
            (terminal_flag::UNLOCKED, "Unlocked"),
            (terminal_flag::ALTERNATE_COLORS, "Alternate Colors"),
            (terminal_flag::HIDE_WELCOME_WITH_IMAGE, "Hide Welcome Text when displaying Image"),
        ] {
            if t.has(bit) {
                *flags.entry(name).or_default() += 1;
            }
        }
        *server.entry(fnv_game::terminal::server_text(t.server)).or_default() += 1;
        with_password += t.password_note.is_some() as u64;
        for it in &t.items {
            items += 1;
            scripts += it.unit.is_some() as u64;
            notes += it.note.is_some() as u64;
            add_notes += (it.flags & 1 != 0) as u64;
            conditioned += !it.conditions.is_empty() as u64;
            if let Some(s) = it.submenu {
                submenus += 1;
                *submenu_of.entry(s.0).or_default() += 1;
            }
            for c in &it.conditions {
                let r = match c.run_on {
                    RunOn::Subject => "subject".to_string(),
                    RunOn::Target => "target".into(),
                    RunOn::Reference(_) => "reference".into(),
                    o => format!("{o:?}"),
                };
                *run_on.entry(r).or_default() += 1;
                *functions_used.entry(c.opcode()).or_default() += 1;
            }
        }
    }
    // Placed terminals: references whose base is a terminal, and how many are sub menus
    // only (never placed).
    let mut placed_by_difficulty: BTreeMap<String, u64> = BTreeMap::new();
    let (mut placed, mut placed_locked) = (0u64, 0u64);
    let mut examples: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for p in refs.refs.values() {
        let Some(t) = data.terminals.get(&p.base) else { continue };
        placed += 1;
        placed_locked += p.lock.is_some() as u64;
        let level = if t.has(terminal_flag::UNLOCKED) { "Unlocked".to_string() } else { Difficulty::from_code(t.difficulty).name().to_string() };
        *placed_by_difficulty.entry(level.clone()).or_default() += 1;
        let ex = examples.entry(level).or_default();
        if ex.len() < 5 {
            let cell = p.cell.and_then(|c| data.edid_of.get(&c).cloned());
            ex.push(json!({"ref": formid_str(p.id.0), "base": data.edid_of.get(&p.base), "cell": cell}));
        }
    }
    let functions_used: BTreeMap<String, u64> = functions_used.into_iter().map(|(k, v)| (format!("0x{k:04X}"), v)).collect();
    Ok(json!({
        "schema": "fnv.world.terminals/1",
        "load_order": plugins.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
        "terminals": data.terminals.len(),
        "difficulty": count(&difficulty),
        "flags": count(&flags),
        "server": count(&server),
        "with_password_note": with_password,
        "items": {"total": items, "with_script": scripts, "with_note": notes, "add_note": add_notes,
                  "with_submenu": submenus, "with_conditions": conditioned,
                  "terminals_used_as_submenu": submenu_of.len()},
        "conditions": {"run_on": count(&run_on), "functions": count(&functions_used)},
        "placed": {"total": placed, "with_lock_data": placed_locked, "by_level": count(&placed_by_difficulty), "examples": examples},
    }))
}

/// Read the game's word list from the load order's archives.
pub fn dictionary(data_dir: &std::path::Path) -> anyhow::Result<Dictionary> {
    let vfs = fnv_vfs::Vfs::open_default(data_dir).with_context(|| data_dir.display().to_string())?;
    let bytes = vfs.read(DICTIONARY).context("menus\\falloutdict.txt not in the archives")??;
    Ok(Dictionary::parse(&String::from_utf8_lossy(&bytes)))
}

/// Lay out one game; `picks` are dump indices chosen in order.
pub fn hack(data_dir: &std::path::Path, difficulty: u8, science: f32, seed: u64, picks: &[usize]) -> anyhow::Result<Value> {
    let dict = dictionary(data_dir)?;
    let s = Settings::default();
    let diff = Difficulty::from_code(difficulty);
    let refusal = hacking::may_hack(science, diff, &s).err().map(|r| format!("{r:?}"));
    let words = hacking::word_count(science, diff, &s);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut p = Puzzle::new(&dict, diff, words, &mut rng).context("too few words for this difficulty")?;
    let rows: Vec<String> = (0..hacking::ROWS)
        .map(|r| format!("0x{:04X} {}  0x{:04X} {}", p.row_address(0, r), p.row_text(0, r), p.row_address(1, r), p.row_text(1, r)))
        .collect();
    let mut outcomes = Vec::new();
    for &i in picks {
        let o = p.choose(i, &mut rng);
        outcomes.push(json!({"pick": i, "outcome": format!("{o:?}"), "attempts": p.attempts}));
    }
    let likeness: Vec<Value> = p.words.iter().map(|w| json!([w.text, hacking::likeness(&w.text, p.password())])).collect();
    Ok(json!({
        "schema": "fnv.world.hack/1",
        "dictionary_words": dict.len(),
        "difficulty": diff.name(),
        "science": science,
        "refused": refusal,
        "word_count": words,
        "password": p.password(),
        "words": likeness,
        "pairs": p.pairs().len(),
        "rows": rows,
        "picks": outcomes,
        "log": p.log,
    }))
}
