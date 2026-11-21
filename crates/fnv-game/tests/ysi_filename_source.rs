//! Synthetic ySI configuration/source/inventory/UI consumers, no original files.
//! Own process and lock: the existing file-reader roots are process-wide.
mod common;
use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    game::{Game, GameConfig, GameSnapshot, RunCtx},
    mod_files::{self, Roots},
    trace::EventKind,
    GameData,
};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use fnv_ui::MemSource;
use std::{
    path::PathBuf,
    sync::{Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};
static LOCK: Mutex<()> = Mutex::new(());
const ITEM: FormId = FormId(0x820);
const PLACED: FormId = FormId(0x900);
struct Files {
    _lock: MutexGuard<'static, ()>,
    root: PathBuf,
    temp: PathBuf,
    previous: Option<Roots>,
}
impl Files {
    fn new() -> Self {
        let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = std::env::temp_dir().canonicalize().unwrap();
        let root = temp.join(format!(
            "nvf-owned-ysi-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let mut out = Self {
            _lock: lock,
            root,
            temp,
            previous: None,
        };
        std::fs::create_dir_all(out.root.join("Data/menus/ySI")).unwrap();
        mod_files::update_roots(|r| {
            out.previous = Some(std::mem::replace(
                r,
                Roots {
                    read: vec![out.root.join("Data")],
                    ..Default::default()
                },
            ))
        });
        out
    }
    fn write(&self, name: &str, text: &str) {
        assert!(!name.contains(['\\', '/', ':']));
        std::fs::write(self.root.join("Data/menus/ySI").join(name), text).unwrap();
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        if let Some(old) = self.previous.take() {
            mod_files::set_roots(old);
        }
        if self.root.parent() == Some(self.temp.as_path())
            && self
                .root
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("nvf-owned-ysi-"))
        {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}
struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}
fn fixture() -> Fixture {
    fixture_with_item_flags(0)
}
fn fixture_with_item_flags(flags: u32) -> Fixture {
    let item = record(
        b"MISC",
        flags,
        ITEM.0,
        &[
            sub(b"EDID", z("OwnYsiItem")),
            sub(b"DATA", [1i32.to_le_bytes(), 1f32.to_le_bytes()].concat()),
        ],
    );
    let placed = record(
        b"REFR",
        0,
        PLACED.0,
        &[
            sub(b"EDID", z("OwnYsiPlaced")),
            sub(b"NAME", ITEM.0.to_le_bytes().to_vec()),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let room = record(b"CELL", 0, 0x200, &[sub(b"EDID", z("OwnYsiRoom"))]);
    let children = group(
        0x200u32.to_le_bytes(),
        6,
        &group(0x200u32.to_le_bytes(), 9, &placed),
    );
    let bytes = [
        tes4(),
        top(b"MISC", &item),
        top(
            b"NPC_",
            &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]),
        ),
        top(
            b"CELL",
            &group(
                0i32.to_le_bytes(),
                2,
                &group(0i32.to_le_bytes(), 3, &[room, children].concat()),
            ),
        ),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "OwnedYsi.esm".into()).unwrap()];
    let order = LoadOrder::new(&plugins).unwrap();
    let schema =
        Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas/script/functions.json");
    let catalogue = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let forms = FormStore::build(&order, &schema);
    let refs = RefTable::build(&order, &schema);
    let data = GameData::build(
        &order,
        &schema,
        FunctionTable::load(&path).unwrap(),
        &catalogue,
        &refs,
    );
    Fixture { data, forms, refs }
}
fn game(f: &Fixture) -> Game<'_> {
    let mut g = Game::new(
        &f.data,
        World::new(&f.forms, &f.refs, SimConfig::default()),
        GameConfig {
            strict: true,
            ..Default::default()
        },
    );
    let source=MemSource::default().with("menus/owned.xml",r#"<menu name="HUDMainMenu"><image name="OwnedIcon"><filename>Owned/before.dds</filename></image><image name="ForeignIcon"><filename>Owned/foreign.dds</filename></image></menu>"#);
    g.ui_tree.load_menu(&source, "menus/owned.xml").unwrap();
    g.ui_tree.update();
    g.ui_files = Some(Box::new(source));
    g
}
fn query(g: &mut Game<'_>, body: &str) -> Result<XVal, String> {
    g.run_query(&format!("ref item\nref empty\nstring_var icon\narray_var pair\nbegin Function {{}}\n{body}\nend"),RunCtx {self_ref:Some(FormId::PLAYER_REF),..Default::default()})
}
fn success(g: &mut Game<'_>, body: &str) -> XVal {
    let before = g.events.len();
    let result = query(g, body).unwrap_or_else(|e| panic!("{body}: {e}"));
    assert!(
        !g.events[before..].iter().any(|e| matches!(
            e.kind,
            EventKind::Stub { .. }
                | EventKind::Unhandled { .. }
                | EventKind::Skipped { .. }
                | EventKind::CallFailed { .. }
                | EventKind::ScriptError { .. }
                | EventKind::ArgUnread { .. }
        )),
        "{:?}",
        &g.events[before..]
    );
    result
}
fn config(tags: serde_json::Value, icons: serde_json::Value) -> String {
    serde_json::json!({"tags":tags,"icons":icons}).to_string()
}
fn certain(filename: &str) -> String {
    config(
        serde_json::json!([{"priority":20,"mod":"OwnedYsi.esm","form":"00000820","tag":"OwnedTag"}]),
        serde_json::json!([{"priority":10,"tag":"OwnedTag","filename":filename}]),
    )
}

#[test]
fn required_filename_calls_reach_real_inventory_and_the_existing_ui_image_trait() {
    let files = Files::new();
    files.write("ySI.json", &certain("Owned/matched.dds"));
    let f = fixture();
    let mut g = game(&f);
    g.world.add_item(FormId::PLAYER_REF, ITEM, 2).unwrap();
    let before = g.world.snapshot();
    assert_eq!(success(&mut g,"let item := OwnYsiItem\nlet icon := ySIGetTrait \"filename\" item\nSetUIString \"HUDMainMenu/OwnedIcon/filename\" (icon)\nSetFunctionValue (GetUIString \"HUDMainMenu/OwnedIcon/filename\")"),XVal::Str("Owned/matched.dds".into()));
    // Plain legacy string arguments keep their literal-token convention; grouped
    // values and explicit conversion pass the variable contents instead.
    assert_eq!(
        success(&mut g, "let icon := ySIGetTrait \"filename\" OwnYsiItem\nSetUIString \"HUDMainMenu/OwnedIcon/filename\" icon\nSetFunctionValue (GetUIString \"HUDMainMenu/OwnedIcon/filename\")"),
        XVal::Str("icon".into())
    );
    assert_eq!(
        success(&mut g, "let icon := ySIGetTrait \"filename\" OwnYsiItem\nSetUIStringAlt (\"HUDMainMenu/OwnedIcon/filename\") $icon\nSetFunctionValue (GetUIString \"HUDMainMenu/OwnedIcon/filename\")"),
        XVal::Str("Owned/matched.dds".into())
    );
    assert_eq!(success(&mut g,"let pair := ar_List OwnYsiItem\nSetFunctionValue (pair[0].ySIGetTrait \"filename\")"),XVal::Str("Owned/matched.dds".into()));
    assert_eq!(
        success(
            &mut g,
            "SetFunctionValue (OwnYsiPlaced.ySIGetTrait \"filename\")"
        ),
        XVal::Str("Owned/matched.dds".into())
    );
    assert_eq!(
        success(
            &mut g,
            "SetFunctionValue (GetUIString \"HUDMainMenu/ForeignIcon/filename\")"
        ),
        XVal::Str("Owned/foreign.dds".into())
    );
    assert_eq!(
        g.world.snapshot(),
        before,
        "getter and explicit UI assignment do not alter inventory/world"
    );
    assert_eq!(g.world.item_count(FormId::PLAYER_REF, ITEM).unwrap(), 2);
}

#[test]
fn multiple_files_use_certain_highest_priority_without_guessing_tie_order() {
    let files = Files::new();
    files.write("ySI.json", &certain("Owned/low.dds"));
    files.write(
        "extra.json",
        &config(
            serde_json::json!([{"priority":30,"formType":31,"tag":"Other"}]),
            serde_json::json!([{"priority":10,"tag":"Other","filename":"Owned/high.dds"}]),
        ),
    );
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(
        success(
            &mut g,
            "SetFunctionValue (ySIGetTrait \"filename\" OwnYsiItem)"
        ),
        XVal::Str("Owned/high.dds".into())
    );
    files.write(
        "tie.json",
        &config(
            serde_json::json!([{"priority":30,"formType":31,"tag":"Conflict"}]),
            serde_json::json!([{"priority":10,"tag":"Conflict","filename":"Owned/conflict.dds"}]),
        ),
    );
    let mut fresh = game(&f);
    assert_eq!(
        query(
            &mut fresh,
            "SetFunctionValue (ySIGetTrait \"filename\" OwnYsiItem)"
        ),
        Err("ambiguous ySI tag priority; native tie ordering remains unqualified".into())
    );
}

#[test]
fn unknown_possible_winners_refuse_but_decisively_false_or_lower_rules_do_not_win() {
    let files = Files::new();
    let f = fixture();
    for (priority, extra, expected) in [
        (30, serde_json::json!({"unknownPredicate":1}), false),
        (10, serde_json::json!({"unknownPredicate":1}), true),
        (
            30,
            serde_json::json!({"formType":40,"unknownWeaponPredicate":1}),
            true,
        ),
    ] {
        let mut rule = extra.as_object().unwrap().clone();
        rule.insert("priority".into(), priority.into());
        rule.insert("tag".into(), "Unknown".into());
        let mut value: serde_json::Value =
            serde_json::from_str(&certain("Owned/certain.dds")).unwrap();
        value["tags"].as_array_mut().unwrap().push(rule.into());
        files.write("ySI.json", &value.to_string());
        let mut g = game(&f);
        let result = query(
            &mut g,
            "SetFunctionValue (ySIGetTrait \"filename\" OwnYsiItem)",
        );
        if expected {
            assert_eq!(result, Ok(XVal::Str("Owned/certain.dds".into())));
        } else {
            assert_eq!(
                result,
                Err("an unsupported ySI predicate could change the highest-priority tag".into())
            );
        }
    }
}

#[test]
fn invalid_receiver_non_item_and_missing_icon_never_fake_a_value_and_tag_icon_stay_intact() {
    let files = Files::new();
    files.write("ySI.json", &certain("Owned/icon.dds"));
    let f = fixture();
    let mut g = game(&f);
    let before = g.world.snapshot();
    for bad in [
        "SetFunctionValue (empty.ySIGetTrait \"filename\")",
        "SetFunctionValue (ySIGetTrait \"filename\" Player)",
    ] {
        assert!(query(&mut g, bad).is_err(), "{bad}");
        assert_eq!(g.world.snapshot(), before);
    }
    // Existing in-memory ySISetTrait / ySIGetTrait "tag" and "icon" behavior is preserved.
    success(
        &mut g,
        "ySISetTrait \"tag\" \"(owned)\" OwnYsiItem\nySISetTrait \"icon\" \"Owned/set.dds\" OwnYsiItem",
    );
    assert_eq!(
        success(&mut g, "SetFunctionValue (ySIGetTrait \"tag\" OwnYsiItem)"),
        XVal::Str("(owned)".into())
    );
    assert_eq!(
        success(&mut g, "SetFunctionValue (ySIGetTrait \"icon\" OwnYsiItem)"),
        XVal::Str("Owned/set.dds".into())
    );
    assert_eq!(
        success(
            &mut g,
            "SetFunctionValue (ySIGetTrait \"filename\" OwnYsiItem)"
        ),
        XVal::Str("Owned/icon.dds".into())
    );
    files.write(
        "ySI.json",
        &config(
            serde_json::json!([{"priority":1,"formType":31,"tag":"NoIcon"}]),
            serde_json::json!([]),
        ),
    );
    let mut fresh = game(&f);
    assert!(query(
        &mut fresh,
        "SetFunctionValue (ySIGetTrait \"filename\" OwnYsiItem)"
    )
    .unwrap_err()
    .contains("missing or ambiguous"));
}

#[test]
fn game_restore_reloads_the_config_snapshot_without_saving_provider_cache() {
    let files = Files::new();
    files.write("ySI.json", &certain("Owned/first.dds"));
    let f = fixture();
    let mut g = game(&f);
    let expression = "SetFunctionValue (ySIGetTrait \"filename\" OwnYsiItem)";
    assert_eq!(
        success(&mut g, expression),
        XVal::Str("Owned/first.dds".into())
    );
    g.world.add_item(FormId::PLAYER_REF, ITEM, 3).unwrap();
    let snapshot: GameSnapshot =
        serde_json::from_value(serde_json::to_value(g.snapshot()).unwrap()).unwrap();
    files.write("ySI.json", &certain("Owned/next.dds"));
    assert_eq!(
        success(&mut g, expression),
        XVal::Str("Owned/first.dds".into()),
        "mapped session snapshot stays stable"
    );
    let world = World::restore(&f.forms, &f.refs, &snapshot.sim).unwrap();
    let mut restored = Game::restore(
        &f.data,
        world,
        GameConfig {
            strict: true,
            ..Default::default()
        },
        &snapshot,
    )
    .unwrap();
    assert_eq!(
        success(&mut restored, expression),
        XVal::Str("Owned/next.dds".into())
    );
    assert_eq!(
        restored.world.item_count(FormId::PLAYER_REF, ITEM).unwrap(),
        3
    );
}

#[test]
fn local_comments_preserve_quoted_content_and_generic_json_stays_strict() {
    let files = Files::new();
    let value = certain("https://owned.invalid/a/*literal*/icon.dds");
    files.write(
        "ySI.json",
        &format!("\u{feff}/* own header */\n// own line\n{value}"),
    );
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(
        success(
            &mut g,
            "SetFunctionValue (ySIGetTrait \"filename\" OwnYsiItem)"
        ),
        XVal::Str("https://owned.invalid/a/*literal*/icon.dds".into())
    );
    // Existing ReadFromJSON is a separate strict reader; this provider does not change it.
    assert!(query(
        &mut g,
        "SetFunctionValue (ReadFromJSON \"Data/menus/ySI/ySI.json\")"
    )
    .unwrap_err()
    .contains("ReadFromJSON: no JSON file Data/menus/ySI/ySI.json"));
    files.write("ySI.json", &value);
    assert!(
        matches!(
            success(
                &mut g,
                "SetFunctionValue (ReadFromJSON \"Data/menus/ySI/ySI.json\")"
            ),
            XVal::Array(_)
        ),
        "same existing JSON reader/path must successfully read plain JSON"
    );
}

#[test]
fn mapped_file_bound_and_malformed_configuration_refuse_explicitly() {
    let files = Files::new();
    files.write("ySI.json", &certain("Owned/base.dds"));
    let f = fixture();
    for n in 0..31 {
        files.write(&format!("owned-{n}.json"), "{}");
    }
    let mut at_bound = game(&f);
    assert_eq!(
        success(
            &mut at_bound,
            "SetFunctionValue (ySIGetTrait \"filename\" OwnYsiItem)"
        ),
        XVal::Str("Owned/base.dds".into())
    );
    files.write("owned-31.json", "{}");
    let mut g = game(&f);
    assert_eq!(
        query(
            &mut g,
            "SetFunctionValue (ySIGetTrait \"filename\" OwnYsiItem)"
        ),
        Err("ySI filename configuration exceeds the mapped file bound".into())
    );
    for n in 0..32 {
        std::fs::remove_file(
            files
                .root
                .join("Data/menus/ySI")
                .join(format!("owned-{n}.json")),
        )
        .unwrap();
    }
    files.write("ySI.json", "/* unfinished");
    let mut fresh = game(&f);
    assert_eq!(
        query(
            &mut fresh,
            "SetFunctionValue (ySIGetTrait \"filename\" OwnYsiItem)"
        ),
        Err("unterminated ySI configuration comment".into())
    );
}

#[test]
fn quest_item_filename_uses_authored_and_saved_world_flags_through_real_source_and_ui() {
    let files = Files::new();
    files.write(
        "ySI.json",
        &config(
            serde_json::json!([
                {"priority":20,"formType":31,"questItem":0,"tag":"Ordinary"},
                {"priority":20,"formType":31,"questItem":1,"tag":"Quest"}
            ]),
            serde_json::json!([
                {"priority":1,"tag":"Ordinary","filename":"Owned/ordinary.dds"},
                {"priority":1,"tag":"Quest","filename":"Owned/quest.dds"}
            ]),
        ),
    );
    let assign = "let item := OwnYsiItem\nlet icon := ySIGetTrait \"filename\" item\nSetUIString \"HUDMainMenu/OwnedIcon/filename\" (icon)\nSetFunctionValue (GetUIString \"HUDMainMenu/OwnedIcon/filename\")";
    for flags in [0, 0x400] {
        let f = fixture_with_item_flags(flags);
        let mut g = game(&f);
        g.world.add_item(FormId::PLAYER_REF, ITEM, 2).unwrap();
        let initial = if flags == 0 {
            "Owned/ordinary.dds"
        } else {
            "Owned/quest.dds"
        };
        assert_eq!(success(&mut g, assign), XVal::Str(initial.into()));
        assert_eq!(g.world.is_quest_item(ITEM), flags != 0);
        success(&mut g, "SetQuestObject OwnYsiItem 1");
        assert!(g.world.is_quest_item(ITEM));
        assert_eq!(success(&mut g, assign), XVal::Str("Owned/quest.dds".into()));
        let saved: GameSnapshot =
            serde_json::from_slice(&serde_json::to_vec(&g.checked_snapshot().unwrap()).unwrap())
                .unwrap();
        let world = World::restore(&f.forms, &f.refs, &saved.sim).unwrap();
        let mut back = Game::restore(&f.data, world, g.cfg, &saved).unwrap();
        assert_eq!(
            success(
                &mut back,
                "SetFunctionValue (OwnYsiPlaced.ySIGetTrait \"filename\")"
            ),
            XVal::Str("Owned/quest.dds".into())
        );
        assert_eq!(back.world.item_count(FormId::PLAYER_REF, ITEM).unwrap(), 2);
        success(&mut back, "SetQuestObject OwnYsiItem 0");
        assert!(
            !back.world.is_quest_item(ITEM),
            "an explicit clear overrides an authored flag too"
        );
        assert_eq!(
            success(
                &mut back,
                "SetFunctionValue (ySIGetTrait \"filename\" OwnYsiItem)"
            ),
            XVal::Str("Owned/ordinary.dds".into())
        );
        assert_eq!(back.world.item_count(FormId::PLAYER_REF, ITEM).unwrap(), 2);
    }
}

#[test]
fn quest_item_false_rules_disqualify_unknowns_but_possible_or_malformed_winners_refuse() {
    let files = Files::new();
    let mut cfg: serde_json::Value = serde_json::from_str(&certain("Owned/certain.dds")).unwrap();
    cfg["tags"].as_array_mut().unwrap().push(
        serde_json::json!({"priority":30,"questItem":1,"unknownPredicate":1,"tag":"Unknown"}),
    );
    files.write("ySI.json", &cfg.to_string());
    let f = fixture();
    let mut g = game(&f);
    let ask = "SetFunctionValue (ySIGetTrait \"filename\" OwnYsiItem)";
    assert_eq!(success(&mut g, ask), XVal::Str("Owned/certain.dds".into()));
    success(&mut g, "SetQuestObject OwnYsiItem 1");
    assert_eq!(
        query(&mut g, ask),
        Err("an unsupported ySI predicate could change the highest-priority tag".into())
    );
    for value in [
        serde_json::json!(2),
        serde_json::json!("true"),
        serde_json::json!([1]),
    ] {
        let mut cfg: serde_json::Value =
            serde_json::from_str(&certain("Owned/certain.dds")).unwrap();
        cfg["tags"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"priority":30,"questItem":value,"tag":"Unknown"}));
        files.write("ySI.json", &cfg.to_string());
        let mut fresh = game(&f);
        assert_eq!(
            query(&mut fresh, ask),
            Err("an unsupported ySI predicate could change the highest-priority tag".into())
        );
    }
}

#[test]
fn aggregate_byte_limit_accepts_exact_valid_input_and_refuses_one_more_byte_without_side_effects() {
    let files = Files::new();
    let primary = certain("Owned/byte-limit.dds");
    // Both files are valid JSON. Only their aggregate size crosses the bound.
    let extra = format!("{{}}{}", " ".repeat(4 * 1024 * 1024 - primary.len() - 2));
    assert_eq!(primary.len() + extra.len(), 4 * 1024 * 1024);
    files.write("ySI.json", &primary);
    files.write("zz-extra.json", &extra);
    let f = fixture();
    let mut allowed = game(&f);
    allowed.world.add_item(FormId::PLAYER_REF, ITEM, 2).unwrap();
    let before = allowed.world.snapshot();
    let assign = "let icon := ySIGetTrait \"filename\" OwnYsiItem\nSetUIString \"HUDMainMenu/OwnedIcon/filename\" (icon)\nSetFunctionValue (GetUIString \"HUDMainMenu/OwnedIcon/filename\")";
    assert_eq!(
        success(&mut allowed, assign),
        XVal::Str("Owned/byte-limit.dds".into())
    );
    assert_eq!(allowed.world.snapshot(), before);

    files.write("zz-extra.json", &format!("{extra} "));
    assert_eq!(primary.len() + extra.len() + 1, 4 * 1024 * 1024 + 1);
    let mut refused = game(&f);
    refused.world.add_item(FormId::PLAYER_REF, ITEM, 2).unwrap();
    let before = refused.world.snapshot();
    let expected = Err("ySI filename configuration exceeds the mapped byte bound".into());
    assert_eq!(query(&mut refused, assign), expected);
    assert_eq!(refused.world.snapshot(), before);
    assert_eq!(
        success(
            &mut refused,
            "SetFunctionValue (GetUIString \"HUDMainMenu/OwnedIcon/filename\")"
        ),
        XVal::Str("Owned/before.dds".into())
    );
    assert_eq!(
        success(
            &mut refused,
            "SetFunctionValue (GetUIString \"HUDMainMenu/ForeignIcon/filename\")"
        ),
        XVal::Str("Owned/foreign.dds".into())
    );
    // An already accepted snapshot is not invalidated by later file growth.
    assert_eq!(
        success(&mut allowed, assign),
        XVal::Str("Owned/byte-limit.dds".into())
    );
    // A refused snapshot remains refused; only a fresh Game rereads corrected files.
    files.write("zz-extra.json", &extra);
    assert_eq!(query(&mut refused, assign), expected);
    let mut fresh = game(&f);
    assert_eq!(
        success(&mut fresh, assign),
        XVal::Str("Owned/byte-limit.dds".into())
    );
}

#[test]
fn aggregate_valid_rule_limit_reaches_the_final_winner_and_refuses_the_next_rule() {
    let files = Files::new();
    let irrelevant = serde_json::json!({"priority":1,"formType":40,"tag":"NoMatch"});
    let main_tags = vec![irrelevant.clone(); 9_999];
    let primary = config(
        main_tags.into(),
        serde_json::json!([{"priority":1,"tag":"FinalWinner","filename":"Owned/rule-limit.dds"}]),
    );
    let mut extra_tags = vec![irrelevant; 9_999];
    extra_tags.push(serde_json::json!({"priority":20,"mod":"OwnedYsi.esm","form":"00000820","tag":"FinalWinner"}));
    assert_eq!(9_999 + 1 + extra_tags.len(), 20_000);
    let extra = config(extra_tags.clone().into(), serde_json::json!([]));
    assert!(primary.len() + extra.len() < 4 * 1024 * 1024);
    files.write("ySI.json", &primary);
    files.write("zz-extra.json", &extra);
    let f = fixture();
    let mut allowed = game(&f);
    allowed.world.add_item(FormId::PLAYER_REF, ITEM, 3).unwrap();
    let before = allowed.world.snapshot();
    let assign = "let icon := ySIGetTrait \"filename\" OwnYsiItem\nSetUIStringAlt (\"HUDMainMenu/OwnedIcon/filename\") $icon\nSetFunctionValue (GetUIString \"HUDMainMenu/OwnedIcon/filename\")";
    assert_eq!(
        success(&mut allowed, assign),
        XVal::Str("Owned/rule-limit.dds".into())
    );
    assert_eq!(allowed.world.snapshot(), before);
    // This extra entry is valid and matches the existing icon. Without the rule
    // cap it would succeed, so the negative control cannot fail on malformed input.
    extra_tags.push(serde_json::json!({"priority":21,"formType":31,"tag":"FinalWinner"}));
    assert_eq!(9_999 + 1 + extra_tags.len(), 20_001);
    let over = config(extra_tags.into(), serde_json::json!([]));
    assert!(primary.len() + over.len() < 4 * 1024 * 1024);
    files.write("zz-extra.json", &over);
    let mut refused = game(&f);
    refused.world.add_item(FormId::PLAYER_REF, ITEM, 3).unwrap();
    let before = refused.world.snapshot();
    let expected = Err("ySI filename configuration exceeds the mapped rule bound".into());
    assert_eq!(query(&mut refused, assign), expected);
    assert_eq!(refused.world.snapshot(), before);
    assert_eq!(
        success(
            &mut refused,
            "SetFunctionValue (GetUIString \"HUDMainMenu/OwnedIcon/filename\")"
        ),
        XVal::Str("Owned/before.dds".into())
    );
    assert_eq!(
        success(
            &mut refused,
            "SetFunctionValue (GetUIString \"HUDMainMenu/ForeignIcon/filename\")"
        ),
        XVal::Str("Owned/foreign.dds".into())
    );
    assert_eq!(
        success(&mut allowed, assign),
        XVal::Str("Owned/rule-limit.dds".into())
    );
    files.write("zz-extra.json", &extra);
    assert_eq!(query(&mut refused, assign), expected);
    let mut fresh = game(&f);
    assert_eq!(
        success(&mut fresh, assign),
        XVal::Str("Owned/rule-limit.dds".into())
    );
}
