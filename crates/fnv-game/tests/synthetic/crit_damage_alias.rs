//! Public catalogue -> real compiler -> game field/DPS controls; no game assets.
use super::*;
use fnv_esm::decode::{decode_sub, DecodeCtx, SubStatus};
use fnv_esm::encode::encode_sub;
use fnv_game::game::RunCtx;
use fnv_script::compile::{self, Env, FormInfo};
use fnv_script::vm::XVal;

const WEAPON: u32 = 0x800;
const NEIGHBOR: u32 = 0x801;
const SCRIPT: u32 = 0x900;
const WEAPON_REF: u32 = 0x400;

struct Names;
impl Env for Names {
    fn form(&self, name: &str) -> Option<FormInfo> {
        let form = match name.to_ascii_lowercase().as_str() {
            "ownedcritweapon" => WEAPON,
            "ownedcritneighbor" => NEIGHBOR,
            _ => return None,
        };
        Some(FormInfo { form, sig: *b"WEAP" })
    }
    fn info(&self, form: u32) -> Option<FormInfo> {
        [WEAPON, NEIGHBOR].contains(&form).then_some(FormInfo { form, sig: *b"WEAP" })
    }
    fn editor_id(&self, form: u32) -> Option<String> {
        match form { WEAPON => Some("OwnedCritWeapon".into()), NEIGHBOR => Some("OwnedCritNeighbor".into()), _ => None }
    }
    fn local_form(&self, form: u32) -> Option<u32> { Some(form) }
    fn script_vars(&self, _: u32) -> Option<Vec<fnv_script::VarDecl>> { None }
}

fn source(command: &str, result: &str) -> String {
    format!("Begin Function {{}}\n{command}\nSetFunctionValue ({result})\nEnd\n")
}

fn owned_fixture(function: Option<&str>) -> Fixture {
    let schema = schema();
    let (table, catalogue) = catalogue();
    let weapon = |id, name: &str, crit| {
        // Use the real schema reader/writer to set named fields, without guessed offsets.
        let mut ctx = DecodeCtx { schema: &schema, on_formid: None };
        let decoded = decode_sub(b"WEAP", &sub(b"DNAM", vec![0; 204]), &mut ctx);
        assert_eq!(decoded.status, SubStatus::Ok);
        let mut dnam = decoded.value.unwrap();
        dnam["attack_shots_per_sec"] = json!(2.0);
        let data = json!({"value": 1, "health": 100, "weight": 1.0, "base_damage": 10, "clip_size": 1});
        let crdt = json!({"critical_damage": crit, "unused": [0, 0], "crit_percent_mult": 1.5,
            "flags": {"value": 0}, "unused2": [0, 0, 0], "effect": "0x00000000"});
        record(b"WEAP", 0, id, &[sub(b"EDID", z(name)),
            sub(b"DATA", encode_sub(b"WEAP", b"DATA", &data, &schema).unwrap()),
            sub(b"DNAM", encode_sub(b"WEAP", b"DNAM", &dnam, &schema).unwrap()),
            sub(b"CRDT", encode_sub(b"WEAP", b"CRDT", &crdt, &schema).unwrap())])
    };
    let weapons = [weapon(WEAPON, "OwnedCritWeapon", 12), weapon(NEIGHBOR, "OwnedCritNeighbor", 7)].concat();
    let player = record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player")),
        sub(b"DATA", [50i32.to_le_bytes().to_vec(), vec![5; 7]].concat())]);
    let cell = record(b"CELL", 0, 0x500, &[sub(b"EDID", z("OwnedCritRoom")), sub(b"DATA", vec![1])]);
    let placed = record(b"REFR", 0, WEAPON_REF, &[sub(b"EDID", z("OwnedCritWeaponREF")),
        sub(b"NAME", WEAPON.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let children = group(0x500u32.to_le_bytes(), 6, &group(0x500u32.to_le_bytes(), 9, &placed));
    let mut parts = vec![tes4(), top(b"WEAP", &weapons), top(b"NPC_", &player),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat())))];
    if let Some(src) = function {
        // Compile a real mod-runtime function, then run its decoded SCPT through Game.
        let c = compile::compile_mod_runtime(src, &table, &Names);
        assert!(c.errors().next().is_none(), "{:?}", c.diags);
        assert!(c.variables.is_empty(), "the authored function has no locals");
        let refs: Vec<u32> = c.refs.iter().map(|r| match r {
            fnv_script::unit::RefSlot::Form(id) => *id,
            other => panic!("unexpected authored reference {other:?}"),
        }).collect();
        let mut subs = unit(0, &c.bytes, &[], &refs);
        subs.push(sub(b"EDID", z("OwnedCritFunction")));
        subs.push(sub(b"SCTX", src.as_bytes().to_vec()));
        parts.push(top(b"SCPT", &record(b"SCPT", 0, SCRIPT, &subs)));
    }
    let plugins = [Plugin::parse(&parts.concat(), "OwnedCrit.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &catalogue, &refs);
    assert_eq!(data.record_field(FormId(WEAPON), "CRDT", "critical_damage"), Some(&json!(12)));
    assert_eq!(data.record_field(FormId(WEAPON), "DNAM", "attack_shots_per_sec"), Some(&json!(2.0)));
    Fixture { data, forms, refs }
}

fn query(g: &mut Game<'_>, expression: &str) -> f64 {
    match g.run_query(&source("", expression), RunCtx::default()).unwrap() {
        XVal::Num(n) => n,
        value => panic!("expected actual numeric result, got {value:?}"),
    }
}

fn run_function(g: &mut Game<'_>) -> XVal {
    g.run_function(&XVal::Ref(SCRIPT), None, Vec::new(), None, RunCtx::default()).unwrap()
}

#[test]
fn canonical_weapon_critical_damage_setter_keeps_existing_contract() {
    let src = source("SetWeaponCritDamage 31 OwnedCritWeapon", "GetWeaponCritDamage OwnedCritWeapon");
    let f = owned_fixture(Some(&src));
    let mut g = game(&f);
    assert_eq!(run_function(&mut g), XVal::Num(31.0));
    assert_eq!(query(&mut g, "GetCritDam OwnedCritWeapon"), 31.0);
    assert_eq!(query(&mut g, "GetCritDam OwnedCritNeighbor"), 7.0);
    assert_eq!(g.run_query(&source("SetWeaponCritDamage 19 OwnedCritWeapon", "0"), RunCtx::default()), Ok(XVal::Num(0.0)));
    assert_eq!(query(&mut g, "GetWeaponCritDamage OwnedCritWeapon"), 19.0);
    assert_eq!(query(&mut game(&f), "GetCritDam OwnedCritWeapon"), 12.0, "session override is not static data");
}

#[test]
fn short_critical_damage_name_resolves_to_same_command_metadata() {
    let (table, catalogue) = catalogue();
    let canonical = table.by_name("SetWeaponCritDamage").unwrap();
    assert_eq!(canonical, 0x148B);
    for name in ["SetCritDam", "setcritdam", "sEtCrItDaM"] { assert_eq!(table.by_name(name), Some(canonical)); }
    let entries: Vec<_> = catalogue["functions"].as_array().unwrap().iter()
        .filter(|x| x["name"] == "SetWeaponCritDamage").collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["aliases"], json!(["SetCritDam"]));
    assert_eq!(table.get(canonical).unwrap().name, "SetWeaponCritDamage");
    assert_eq!(table.get(canonical).unwrap().params.len(), 2);
    assert_eq!(table.by_name("GetCritDam"), table.by_name("GetWeaponCritDamage"));
    assert_eq!(table.by_name("SetCritDamUnavailable"), None);
}

#[test]
fn short_and_long_critical_damage_compile_identically() {
    let (table, _) = catalogue();
    let short = source("SetCritDam 47 OwnedCritWeapon", "GetCritDam OwnedCritWeapon");
    let long = short.replace("SetCritDam", "SetWeaponCritDamage");
    for compiler in [compile::compile_runtime, compile::compile_mod_runtime] {
        let a = compiler(&short, &table, &Names);
        let b = compiler(&long, &table, &Names);
        assert!(a.errors().next().is_none(), "{:?}", a.diags);
        assert!(b.errors().next().is_none(), "{:?}", b.diags);
        assert!(!a.bytes.is_empty());
        assert_eq!(a.bytes, b.bytes, "alias cannot become an unknown named-call fallback");
        assert_eq!(a.refs, b.refs);
        assert_eq!(a.variables, b.variables);
    }
}

#[test]
fn compiled_short_critical_damage_changes_real_getter_state() {
    let src = source("SetCritDam 47 OwnedCritWeapon", "GetWeaponCritDamage OwnedCritWeapon");
    let f = owned_fixture(Some(&src));
    let mut g = game(&f);
    assert_eq!(run_function(&mut g), XVal::Num(47.0));
    assert_eq!(query(&mut g, "GetCritDam OwnedCritWeapon"), 47.0);
    assert_eq!(query(&mut g, "GetCritDam OwnedCritNeighbor"), 7.0);
    // The explicit supplied form still wins over a different calling reference.
    g.run_query(&source("OwnedCritWeaponREF.SetCritDam 23 OwnedCritNeighbor", "0"), RunCtx::default()).unwrap();
    assert_eq!(query(&mut g, "GetCritDam OwnedCritNeighbor"), 23.0);
    assert_eq!(query(&mut g, "GetCritDam OwnedCritWeapon"), 47.0);
    g.run_query(&source("SetWeaponCritDamage 29 OwnedCritWeapon", "0"), RunCtx::default()).unwrap();
    assert_eq!(query(&mut g, "GetCritDam OwnedCritWeapon"), 29.0);
    assert!(!g.events.iter().any(|e| matches!(e.kind, EventKind::CallFailed { .. } | EventKind::ScriptError { .. })));
}

#[test]
fn compiled_short_critical_damage_reaches_existing_dps_consumer() {
    let src = source("SetCritDam 42 OwnedCritWeapon", "GetCalculatedWeaponDPS OwnedCritWeapon");
    let f = owned_fixture(Some(&src));
    let mut g = game(&f);
    g.run_query(&source("PlayerREF.SetAV CriticalChance 25", "0"), RunCtx::default()).unwrap();
    assert_eq!(query(&mut g, "PlayerREF.GetAV CriticalChance"), 25.0);
    assert_eq!(query(&mut g, "GetCritDam OwnedCritWeapon"), 12.0);
    assert_eq!(f.data.record_field(FormId(WEAPON), "CRDT", "crit_percent_mult"), Some(&json!(1.5)));
    let before = query(&mut g, "GetCalculatedWeaponDPS OwnedCritWeapon");
    let neighbor = query(&mut g, "GetCalculatedWeaponDPS OwnedCritNeighbor");
    let XVal::Num(after) = run_function(&mut g) else { panic!("DPS must be numeric") };
    // Same base damage; only crit damage changes by30, with25%, multiplier1.5, shots2.
    assert!((after - before - 22.5).abs() < 1e-9, "DPS delta: {before} -> {after}");
    assert_eq!(query(&mut g, "GetCalculatedWeaponDPS OwnedCritNeighbor"), neighbor);
    assert_eq!(query(&mut g, "GetCritDam OwnedCritWeapon"), 42.0);
    assert_eq!(f.data.record_field(FormId(WEAPON), "CRDT", "critical_damage"), Some(&json!(12)));
}
