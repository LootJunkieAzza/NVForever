//! The leveled-list functions of JIP LN / LU and ShowOff [documented: their pages;
//! observed: the peer's lu-leveled set in the original game]. Item lists (LVLI) are
//! edited in the world, where the leveled-item resolver reads them, so containers and
//! loot see the change; actor lists (LVLN, LVLC) also feed template selection.
//! Persistent additions: docs/spec-notes/actor-list-additions.md. LU's edits last
//! for the session ("not savebaked"); ShowOff's RemoveFormFromLeveledList, documented
//! as saved, is kept for the session too [decision: the saving is "EXPERIMENTAL" on its
//! page].

use crate::game::Game;
use fnv_script::vm::XVal;
use fnv_sim::forms::LeveledEntry;
use fnv_sim::FormId;

/// One leveled list as the functions see it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LevList {
    pub chance_none: u8,
    pub flags: u8,
    pub global: Option<FormId>,
    /// (level, form, count, condition).
    pub entries: Vec<(u16, FormId, u16, Option<f32>)>,
}

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

fn item_list_flags(l: &fnv_sim::forms::LeveledList) -> u8 {
    u8::from(l.all_levels) | u8::from(l.each) << 1 | u8::from(l.use_all) << 2
}

impl Game<'_> {
    /// A leveled list (item or actor) as the session has it.
    pub fn lev_list(&self, f: FormId) -> Option<LevList> {
        if let Some(l) = self.world.leveled_list(f) {
            return Some(LevList {
                chance_none: l.chance_none,
                flags: self.other.lev_flags.get(&f).copied().unwrap_or_else(|| item_list_flags(&l)),
                global: l.global,
                entries: l.entries.iter().map(|e| (e.level, e.item, e.count, e.condition)).collect(),
            });
        }
        if let Some(l) = self.other.lev_actors.get(&f) {
            return Some(l.clone());
        }
        let d = self.data.leveled_actors.get(&f)?;
        let mut entries: Vec<_> = d.entries.iter().enumerate().map(|(i, (lv, e))| (*lv, *e, d.counts.get(i).copied().unwrap_or(1), None)).collect();
        if let Some(added) = self.forms.actor_list_additions.get(&f) {
            entries.extend(added.iter().map(|&(lv, item)| (lv, item, 1, None)));
        }
        Some(LevList {
            chance_none: d.chance_none,
            flags: d.flags,
            global: None,
            entries,
        })
    }

    /// An edited actor list for the existing seeded template resolver; unchanged
    /// lists take its original borrowed-data path without an allocation.
    pub fn actor_list_override(&self, f: FormId) -> Option<crate::data::LeveledActorDef> {
        if !self.other.lev_actors.contains_key(&f) && !self.forms.actor_list_additions.contains_key(&f) {
            return None;
        }
        let l = self.lev_list(f)?;
        Some(crate::data::LeveledActorDef {
            chance_none: l.chance_none,
            all_levels: l.flags & 1 != 0,
            entries: l.entries.iter().map(|e| (e.0, e.1)).collect(),
            counts: l.entries.iter().map(|e| e.2).collect(),
            flags: l.flags,
        })
    }

    /// Refresh source metadata without generating or replacing inventory contents.
    /// List edits update cold inventories; restoration reconstructs every placed
    /// source after saved additions/rolls are available, including generated ones.
    pub(crate) fn refresh_actor_inventory_sources(&mut self, include_generated: bool) {
        let sources: Vec<_> = self.world.placed.refs.iter()
            .filter(|(_, p)| matches!(&p.sig, b"ACHR" | b"ACRE"))
            .filter(|(r, _)| include_generated || self.world.peek(**r).and_then(|s| s.inventory.as_ref()).is_none())
            .map(|(&r, p)| {
                let source = self.template_source(p.base, r, |d| d.template_inventory).map(|(id, _)| id).filter(|&id| id != p.base);
                (r, source)
            }).collect();
        for (r, source) in sources {
            self.world.set_inventory_source(r, source);
        }
    }

    /// Store an edited list where its readers look.
    fn set_lev_list(&mut self, f: FormId, l: LevList) {
        let flags = l.flags;
        let item = self.world.edit_leveled(f, |w| {
            w.chance_none = l.chance_none;
            w.all_levels = flags & 1 != 0;
            w.each = flags & 2 != 0;
            w.use_all = flags & 4 != 0;
            w.global = l.global;
            w.entries = l.entries.iter().map(|&(level, item, count, condition)| LeveledEntry { level, item, count, condition }).collect();
        });
        if item {
            self.other.lev_flags.insert(f, flags);
        } else {
            self.other.lev_actors.insert(f, l);
            self.refresh_actor_inventory_sources(false);
        }
    }

    /// Whether `target` is in the list or any list inside it.
    fn lev_has_deep(&self, list: FormId, target: FormId, depth: u32) -> bool {
        if depth > 16 {
            return false;
        }
        self.lev_list(list).is_some_and(|l| l.entries.iter().any(|e| e.1 == target || self.lev_has_deep(e.1, target, depth + 1)))
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    if matches!(name, "addnpctoleveledlist" | "addcreaturetoleveledlist") {
        // Source-name admission only: native encoding requires a GECK probe.
        // Contract and invalid-input policy: actor-list-additions.md.
        let (list_sig, actor_sig) = if name == "addnpctoleveledlist" { (b"LVLN", b"NPC_") } else { (b"LVLC", b"CREA") };
        let (Some(list), Some(actor), Some(XVal::Num(level))) = (form(args, 0), form(args, 1), args.get(2)) else { return Some(Ok(zero)) };
        if g.data.sig(list).as_ref() != Some(list_sig) || !g.data.leveled_actors.contains_key(&list)
            || !matches!(g.data.sig(actor), Some(s) if &s == actor_sig || &s == list_sig)
            || !level.is_finite() || level.fract() != 0.0 || !(1.0..=f64::from(u16::MAX)).contains(level)
        {
            return Some(Ok(zero));
        }
        let entry = (*level as u16, actor);
        g.forms.actor_list_additions.entry(list).or_default().push(entry);
        if let Some(l) = g.other.lev_actors.get_mut(&list) {
            l.entries.push((entry.0, actor, 1, None));
        }
        g.refresh_actor_inventory_sources(false);
        return Some(Ok(zero));
    }
    let is_lev = matches!(name,
        "getnumlevitems" | "getnthlevitem" | "getnthlevitemcount" | "getnthlevitemlevel" | "getlevitemindexbylevel"
        | "getlevitemindexbyform" | "leveledlisthasformdeep" | "getchancenone" | "setchancenone" | "setchancenoneglobal"
        | "getleveledlistflags" | "setleveledlistflags" | "leveledlistaddform" | "leveledlistremoveform"
        | "removeformfromleveledlist" | "leveledlistreplaceform" | "leveledlistclear" | "removenthlevitem"
        | "setnthlevitem" | "setnthlevitemcount" | "setnthlevitemlevel" | "dumplevlist");
    if !is_lev {
        return None;
    }
    let Some(f) = form(args, 0) else { return Some(Ok(zero)) };
    // On anything but a leveled list the getters give 0 [observed: the stimpak].
    let Some(mut l) = g.lev_list(f) else {
        return Some(Ok(match name {
            "getnthlevitem" => XVal::Ref(0),
            "getlevitemindexbylevel" | "getlevitemindexbyform" => XVal::Num(-1.0),
            _ => zero,
        }));
    };
    let idx = |k: usize| usize::try_from(num(args, k, -1.0) as i64).ok();
    let n = l.entries.len();
    let v = match name {
        "getnumlevitems" => return Some(Ok(XVal::Num(n as f64))),
        // Past the end: 0 [observed].
        "getnthlevitem" => return Some(Ok(XVal::Ref(idx(1).and_then(|i| l.entries.get(i)).map_or(0, |e| e.1 .0)))),
        "getnthlevitemcount" => return Some(Ok(XVal::Num(idx(1).and_then(|i| l.entries.get(i)).map_or(0.0, |e| f64::from(e.2))))),
        "getnthlevitemlevel" => return Some(Ok(XVal::Num(idx(1).and_then(|i| l.entries.get(i)).map_or(0.0, |e| f64::from(e.0))))),
        // The first entry of exactly that level [observed: level 1 gives -1 on a list
        // whose lowest entry is 33; "<=" would give 0].
        "getlevitemindexbylevel" => {
            let lv = num(args, 1, -1.0) as i64;
            return Some(Ok(XVal::Num(l.entries.iter().position(|e| i64::from(e.0) == lv).map_or(-1.0, |i| i as f64))));
        }
        "getlevitemindexbyform" => {
            let t = form(args, 1);
            return Some(Ok(XVal::Num(l.entries.iter().position(|e| Some(e.1) == t).map_or(-1.0, |i| i as f64))));
        }
        "leveledlisthasformdeep" => {
            let Some(t) = form(args, 1) else { return Some(Ok(zero)) };
            return Some(Ok(XVal::Num(f64::from(u8::from(g.lev_has_deep(f, t, 0))))));
        }
        "getchancenone" => return Some(Ok(XVal::Num(f64::from(l.chance_none)))),
        "getleveledlistflags" => return Some(Ok(XVal::Num(f64::from(l.flags)))),
        "dumplevlist" => {
            for (i, e) in l.entries.iter().enumerate() {
                g.console_out.push(format!("{i}: {} level {} count {}", g.data.name(e.1), e.0, e.2));
            }
            return Some(Ok(zero));
        }
        // Over 100 is refused [observed: 250 left 35].
        "setchancenone" => {
            let c = num(args, 1, -1.0);
            if !(0.0..=100.0).contains(&c) {
                return Some(Ok(zero));
            }
            l.chance_none = c as u8;
            zero
        }
        "setchancenoneglobal" => {
            l.global = form(args, 1);
            zero
        }
        // Kept as given, not masked to the three known bits [observed: 15 reads 15].
        "setleveledlistflags" => {
            l.flags = num(args, 1, 0.0) as i64 as u8;
            zero
        }
        // `list form level count health`: any kind of form; the newest entry first, at
        // index 0; an entry above the player's level is not added [observed:
        // lu-leveled2, the player at level 1: adds at level 1 went in, newest first,
        // levels 2 and 7 did not; hypothesis: the rule is "above the player's level", only
        // level 1 seen]; health 0–1, 0 or 1 meaning whole [documented: "pass in 0.0 as
        // the health argument" for a list].
        "leveledlistaddform" => {
            let Some(item) = form(args, 1) else { return Some(Ok(zero)) };
            let level = num(args, 2, 1.0).clamp(0.0, f64::from(u16::MAX)) as u16;
            if f64::from(level) > g.player_level() {
                return Some(Ok(zero));
            }
            let count = num(args, 3, 1.0).clamp(0.0, f64::from(u16::MAX)) as u16;
            let h = num(args, 4, 0.0);
            let condition = (h > 0.0 && h < 1.0).then_some(h as f32);
            l.entries.insert(0, (level, item, count, condition));
            zero
        }
        // Every entry of the form; gives how many [documented].
        "leveledlistremoveform" | "removeformfromleveledlist" => {
            let t = form(args, 1);
            l.entries.retain(|e| Some(e.1) != t);
            XVal::Num((n - l.entries.len()) as f64)
        }
        "leveledlistreplaceform" => {
            let (Some(old), Some(new)) = (form(args, 1), form(args, 2)) else { return Some(Ok(zero)) };
            for e in l.entries.iter_mut().filter(|e| e.1 == old) {
                e.1 = new;
            }
            zero
        }
        "leveledlistclear" => {
            l.entries.clear();
            zero
        }
        "removenthlevitem" => {
            match idx(1).filter(|&i| i < n) {
                Some(i) => {
                    l.entries.remove(i);
                }
                None => return Some(Ok(zero)),
            }
            zero
        }
        "setnthlevitem" | "setnthlevitemcount" | "setnthlevitemlevel" => {
            let Some(e) = idx(1).and_then(|i| l.entries.get_mut(i)) else { return Some(Ok(zero)) };
            match name {
                "setnthlevitem" => match form(args, 2) {
                    Some(new) => e.1 = new,
                    None => return Some(Ok(zero)),
                },
                "setnthlevitemcount" => e.2 = num(args, 2, 1.0).clamp(0.0, f64::from(u16::MAX)) as u16,
                _ => e.0 = num(args, 2, 1.0).clamp(0.0, f64::from(u16::MAX)) as u16,
            }
            zero
        }
        _ => return None,
    };
    g.set_lev_list(f, l);
    Some(Ok(v))
}

#[cfg(test)]
mod actor_addition_tests {
    use super::*;
    use crate::common::*;
    use crate::{game::GameConfig, GameData};
    use fnv_esm::{LoadOrder, Plugin, Schema};
    use fnv_script::FunctionTable;
    use fnv_sim::{FormStore, RefTable, SimConfig, World};

    struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

    fn fixture() -> Fixture {
        let actor = |sig: &[u8; 4], id: u32, name: &str, carried: u32| record(sig, 0, id, &[
            sub(b"EDID", z(name)), sub(b"CNTO", [carried.to_le_bytes(), 1i32.to_le_bytes()].concat()),
        ]);
        let list = |sig: &[u8; 4], id: u32, name: &str, item: u32| record(sig, 0, id, &[
            sub(b"EDID", z(name)), sub(b"LVLD", vec![0]), sub(b"LVLF", vec![0]),
            sub(b"LVLO", [1u16.to_le_bytes().to_vec(), vec![0; 2], item.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), vec![0; 2]].concat()),
        ]);
        let placed = |id: u32| record(b"ACRE", 0, id, &[sub(b"NAME", 0x831u32.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
        let placed_npc = record(b"ACHR", 0, 0xA21, &[sub(b"NAME", 0x821u32.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
        let cell = [record(b"CELL", 0, 0xA00, &[sub(b"DATA", vec![1])]),
            group(0xA00u32.to_le_bytes(), 6, &group(0xA00u32.to_le_bytes(), 9, &[placed(0xA11), placed(0xA12), placed(0xA13), placed_npc].concat()))].concat();
        let bytes = [tes4(),
            top(b"NPC_", &[actor(b"NPC_", 0x801, "InitialNPC", 0x901), actor(b"NPC_", 0x802, "AddedNPC", 0x902)].concat()),
            top(b"CREA", &[actor(b"CREA", 0x811, "InitialCreature", 0x901), actor(b"CREA", 0x812, "AddedCreature", 0x902)].concat()),
            top(b"LVLN", &[list(b"LVLN", 0x821, "NPCTiers", 0x801), list(b"LVLN", 0x822, "NestedNPC", 0x802)].concat()),
            top(b"LVLC", &[list(b"LVLC", 0x831, "CreatureTiers", 0x811), list(b"LVLC", 0x832, "NestedCreature", 0x812)].concat()),
            top(b"MISC", &[record(b"MISC", 0, 0x901, &[sub(b"EDID", z("InitialItem"))]), record(b"MISC", 0, 0x902, &[sub(b"EDID", z("AddedItem"))])].concat()),
            top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
        ].concat();
        let plugins = [Plugin::parse(&bytes, "ActorAdditions.esm".into()).unwrap()];
        let lo = LoadOrder::new(&plugins).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
        let schema = Schema::load_dir(root.join("fnv")).unwrap();
        let path = root.join("script/functions.json");
        let catalogue = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let forms = FormStore::build(&lo, &schema);
        let refs = RefTable::build(&lo, &schema);
        let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
        Fixture { data, forms, refs }
    }

    fn game(f: &Fixture) -> Game<'_> {
        Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
    }

    fn source(g: &mut Game, text: &str) {
        let (unit, _) = g.compile_statements(text, "ActorAdditions.esm", "SCPT").unwrap();
        assert!(g.run_statements(unit, Default::default()));
    }

    fn selected(g: &Game, list: u32, level: u16) -> Option<FormId> {
        g.runtime_template_source(FormId(list), FormId(0xA01), fnv_sim::zones::LevelAt::plain(level), |_| false).map(|(id, _)| id)
    }

    #[test]
    fn mod_source_additions_reach_level_selection_nested_lists_and_created_inventory() {
        let f = fixture();
        for (command, list, initial, added, nested) in [
            ("AddNPCToLeveledList", 0x821, 0x801, 0x802, "NestedNPC"),
            ("AddCreatureToLeveledList", 0x831, 0x811, 0x812, "NestedCreature"),
        ] {
            let mut g = game(&f);
            assert_eq!(selected(&g, list, 5), Some(FormId(initial)));
            let name = if list == 0x821 { "NPCTiers" } else { "CreatureTiers" };
            source(&mut g, &format!("{command} {name} {nested} 5\n"));
            assert_eq!(selected(&g, list, 4), Some(FormId(initial)));
            assert_eq!(selected(&g, list, 5), Some(FormId(added)));
            g.player.level = Some(5);
            let reference = g.create_at(FormId(list), None, [0.0; 3], [0.0; 3]).unwrap();
            assert_eq!(g.world.inventory_mut(reference).unwrap().count(FormId(0x902)), 1);
            assert_eq!(g.world.inventory_mut(reference).unwrap().count(FormId(0x901)), 0);
            assert_eq!(g.lev_list(FormId(list)).unwrap().entries.len(), 2);
        }
    }

    #[test]
    fn invalid_forms_levels_and_missing_arguments_leave_lists_unchanged() {
        let f = fixture(); let mut g = game(&f);
        for name in ["addnpctoleveledlist", "addcreaturetoleveledlist"] {
            let list = if name == "addnpctoleveledlist" { 0x821 } else { 0x831 };
            let actor = if name == "addnpctoleveledlist" { 0x802 } else { 0x812 };
            for bad in [0.0, -1.0, 1.5, 65536.0, f64::NAN, f64::INFINITY] {
                call(&mut g, name, &[XVal::Ref(list), XVal::Ref(actor), XVal::Num(bad)]).unwrap().unwrap();
            }
            for (bad_list, bad_actor) in [(0, actor), (0x901, actor), (list, 0), (list, 0x901), (list, if actor == 0x802 { 0x812 } else { 0x802 })] {
                call(&mut g, name, &[XVal::Ref(bad_list), XVal::Ref(bad_actor), XVal::Num(5.0)]).unwrap().unwrap();
            }
            call(&mut g, name, &[XVal::Ref(list), XVal::Ref(actor)]).unwrap().unwrap();
        }
        assert!(g.forms.actor_list_additions.is_empty());
        assert!(g.other.lev_actors.is_empty());
    }

    #[test]
    fn saved_additions_survive_roundtrip_without_saving_transient_list_edits() {
        let f = fixture(); let mut g = game(&f);
        source(&mut g, "AddNPCToLeveledList NPCTiers AddedNPC 5\nAddCreatureToLeveledList CreatureTiers AddedCreature 5\n");
        call(&mut g, "setchancenone", &[XVal::Ref(0x821), XVal::Num(100.0)]).unwrap().unwrap();
        assert_eq!(selected(&g, 0x821, 5), None);
        let bytes = serde_json::to_vec(&g.checked_snapshot().unwrap()).unwrap();
        let saved: crate::game::GameSnapshot = serde_json::from_slice(&bytes).unwrap();
        let world = World::restore(&f.forms, &f.refs, &saved.sim).unwrap();
        let back = Game::restore(&f.data, world, g.cfg, &saved).unwrap();
        assert_eq!(selected(&back, 0x821, 5), Some(FormId(0x802)));
        assert_eq!(selected(&back, 0x831, 5), Some(FormId(0x812)));
        assert_eq!(back.lev_list(FormId(0x821)).unwrap().chance_none, 0);
        assert!(back.other.lev_actors.is_empty());
        assert_eq!(selected(&game(&f), 0x821, 5), Some(FormId(0x801)));
    }

    #[test]
    fn cold_placed_inventories_follow_additions_while_generated_contents_survive() {
        let f = fixture(); let mut g = game(&f);
        g.player.level = Some(5);
        assert_eq!(g.world.item_count(FormId(0xA11), FormId(0x901)).unwrap(), 1);
        source(&mut g, "AddCreatureToLeveledList CreatureTiers AddedCreature 5\n");
        assert_eq!(g.world.item_count(FormId(0xA11), FormId(0x901)).unwrap(), 1);
        assert_eq!(g.world.item_count(FormId(0xA12), FormId(0x902)).unwrap(), 1);
        assert_eq!(g.world.item_count(FormId(0xA12), FormId(0x901)).unwrap(), 0);
        let saved = g.checked_snapshot().unwrap();
        let world = World::restore(&f.forms, &f.refs, &saved.sim).unwrap();
        let mut back = Game::restore(&f.data, world, g.cfg, &saved).unwrap();
        assert_eq!(back.world.item_count(FormId(0xA11), FormId(0x901)).unwrap(), 1);
        assert_eq!(back.world.item_count(FormId(0xA13), FormId(0x902)).unwrap(), 1);
    }

    #[test]
    fn permanent_append_does_not_save_an_earlier_session_clear() {
        let f = fixture(); let mut g = game(&f);
        call(&mut g, "leveledlistclear", &[XVal::Ref(0x821)]).unwrap().unwrap();
        source(&mut g, "AddNPCToLeveledList NPCTiers AddedNPC 5\n");
        assert_eq!(g.lev_list(FormId(0x821)).unwrap().entries.len(), 1);
        let saved = g.checked_snapshot().unwrap();
        let world = World::restore(&f.forms, &f.refs, &saved.sim).unwrap();
        let back = Game::restore(&f.data, world, g.cfg, &saved).unwrap();
        assert_eq!(back.lev_list(FormId(0x821)).unwrap().entries.len(), 2);
        assert_eq!(selected(&back, 0x821, 1), Some(FormId(0x801)));
        assert_eq!(selected(&back, 0x821, 5), Some(FormId(0x802)));
    }

    #[test]
    fn unrelated_actor_list_edits_preserve_an_ordinary_actors_carried_extras_on_resurrection() {
        let f = fixture();
        for edit in [None, Some("AddNPCToLeveledList NPCTiers AddedNPC 5\n"), Some("AddCreatureToLeveledList CreatureTiers AddedCreature 5\n")] {
            let mut g = game(&f);
            let actor = g.create_at(FormId(0x801), None, [0.0; 3], [0.0; 3]).unwrap();
            g.world.inventory_mut(actor).unwrap().add(FormId(0x902), 3);
            assert_eq!(g.world.item_count(actor, FormId(0x901)).unwrap(), 1);
            assert_eq!(g.world.item_count(actor, FormId(0x902)).unwrap(), 3);
            if let Some(edit) = edit { source(&mut g, edit); }
            g.extra.entry(actor).or_default().dead = true;
            g.resurrect(actor, false);
            assert_eq!(g.world.item_count(actor, FormId(0x901)).unwrap(), 1, "{edit:?}");
            assert_eq!(g.world.item_count(actor, FormId(0x902)).unwrap(), 3, "{edit:?}");
            assert!(!g.is_dead(actor));
        }
    }

    #[test]
    fn an_edited_list_in_the_actors_inventory_chain_still_regenerates_its_selected_inventory() {
        let f = fixture();
        for (list, edit) in [(0x821, "AddNPCToLeveledList NPCTiers AddedNPC 5\n"), (0x831, "AddCreatureToLeveledList CreatureTiers AddedCreature 5\n")] {
            let mut g = game(&f);
            g.player.level = Some(5);
            let actor = g.create_at(FormId(list), None, [0.0; 3], [0.0; 3]).unwrap();
            g.world.inventory_mut(actor).unwrap().add(FormId(0x902), 3);
            assert_eq!(g.world.item_count(actor, FormId(0x901)).unwrap(), 1);
            source(&mut g, edit);
            g.extra.entry(actor).or_default().dead = true;
            g.resurrect(actor, false);
            assert_eq!(g.world.item_count(actor, FormId(0x901)).unwrap(), 0);
            assert_eq!(g.world.item_count(actor, FormId(0x902)).unwrap(), 1);
        }
    }

    #[test]
    fn rerolled_actor_inventory_sources_follow_reference_lifetime_through_save_and_reset() {
        let f = fixture();
        for (list, placed, selected, edit) in [
            (0x821, 0xA21, 0x802, "AddNPCToLeveledList NPCTiers AddedNPC 5\n"),
            (0x831, 0xA11, 0x812, "AddCreatureToLeveledList CreatureTiers AddedCreature 5\n"),
        ] {
            for created in [false, true] {
                let mut g = game(&f);
                g.player.level = Some(5);
                let actor = if created {
                    g.create_at(FormId(list), None, [0.0; 3], [0.0; 3]).unwrap()
                } else { FormId(placed) };
                assert_eq!(g.world.item_count(actor, FormId(0x901)).unwrap(), 1);
                g.world.inventory_mut(actor).unwrap().add(FormId(0x902), 3);
                source(&mut g, edit);
                g.extra.entry(actor).or_default().dead = true;
                g.resurrect(actor, false);
                assert_eq!(g.world.item_count(actor, FormId(0x901)).unwrap(), 0, "created={created}");
                assert_eq!(g.world.item_count(actor, FormId(0x902)).unwrap(), 1, "created={created}");
                let saved = g.checked_snapshot().unwrap();
                assert_eq!(saved.sim.created_inventory_sources.get(&actor).copied(), created.then_some(FormId(selected)));
                let world = World::restore(&f.forms, &f.refs, &saved.sim).unwrap();
                let mut back = Game::restore(&f.data, world, g.cfg, &saved).unwrap();
                assert!(back.world.reset_inventory(actor).unwrap());
                assert_eq!(back.world.item_count(actor, FormId(0x901)).unwrap(), 0, "created={created}");
                assert_eq!(back.world.item_count(actor, FormId(0x902)).unwrap(), 1, "created={created}");
            }
        }
    }

    #[test]
    fn restored_placed_source_metadata_tracks_saved_edits_without_regenerating_existing_contents() {
        let f = fixture();
        for (list, placed, edit) in [
            (0x821, 0xA21, "AddNPCToLeveledList NPCTiers AddedNPC 5\n"),
            (0x831, 0xA11, "AddCreatureToLeveledList CreatureTiers AddedCreature 5\n"),
        ] {
            for created in [false, true] {
                let mut g = game(&f);
                g.player.level = Some(5);
                let actor = if created { g.create_at(FormId(list), None, [0.0; 3], [0.0; 3]).unwrap() }
                    else { FormId(placed) };
                assert_eq!(g.world.item_count(actor, FormId(0x901)).unwrap(), 1);
                g.world.inventory_mut(actor).unwrap().add(FormId(0x902), 3);
                source(&mut g, edit);
                let saved = g.checked_snapshot().unwrap();
                let world = World::restore(&f.forms, &f.refs, &saved.sim).unwrap();
                let mut back = Game::restore(&f.data, world, g.cfg, &saved).unwrap();
                assert_eq!(back.world.item_count(actor, FormId(0x901)).unwrap(), 1, "contents survive restore: created={created}");
                assert_eq!(back.world.item_count(actor, FormId(0x902)).unwrap(), 3, "extras survive restore: created={created}");
                assert!(back.world.reset_inventory(actor).unwrap());
                assert_eq!(back.world.item_count(actor, FormId(0x901)).unwrap(), i32::from(created), "created={created}");
                assert_eq!(back.world.item_count(actor, FormId(0x902)).unwrap(), i32::from(!created), "created={created}");
            }
        }
    }
}
