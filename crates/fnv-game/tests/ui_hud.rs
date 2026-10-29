//! Synthetic XML and sentinel values [design]; expected HUD values [observed: hud.md §14].

use fnv_game::ui_hud::{self, Shown, Texts};
use fnv_ui::{MemSource, Ui, Value};

mod common;

// Columns are 4:3, 16:10 and 16:9. The half-unit reticle offset follows the
// truncated middle [hypothesis: hud.md §14, its child's 607.5/735.5/820.5].
const PLACES: &[(&str, [f64; 3], f64)] = &[
    ("HitPoints", [40.0, 40.0, 40.0], 803.0),
    ("ActionPoints", [894.0, 1150.0, 1320.0], 803.0),
    ("HardcoreMode", [190.0, 190.0, 190.0], 803.0),
    ("SneakMeter", [640.0, 768.0, 853.0], 30.0),
    ("ReticleCenter", [605.5, 733.5, 818.5], 439.5),
    ("EnemyHealth", [465.0, 593.0, 678.0], 843.0),
    ("QuestReminder", [45.0, 45.0, 45.0], 200.0),
    ("Messages", [25.0, 25.0, 25.0], 55.0),
    ("Subtitles", [240.0, 368.0, 453.0], 660.0),
    ("Info", [480.0, 608.0, 693.0], 820.0),
    ("Hokeys", [909.0, 1165.0, 1335.0], 483.0),
    ("Region_Location", [45.0, 45.0, 45.0], 728.0),
    ("XPMeter", [860.0, 1116.0, 1286.0], 650.0),
    ("RadiationMeter", [863.0, 1119.0, 1289.0], 30.0),
    ("BreathMeter", [640.0, 768.0, 853.0], 110.0),
    ("Explosive_positioning_rect", [640.0, 768.0, 853.0], 480.0),
    ("crippled_limb_indicator", [1050.0, 1306.0, 1476.0], 130.0),
    ("DDTIcon", [414.0, 414.0, 414.0], 852.0),
    ("DDTIconEnemy", [433.0, 561.0, 646.0], 846.0),
    ("DDTIconEnemyAP", [433.0, 561.0, 646.0], 846.0),
    ("AmmoTypeLabel", [1044.0, 1300.0, 1470.0], 883.0),
    ("CNDArrows", [1027.0, 1283.0, 1453.0], 886.0),
];

const VISIBLE: &[(&str, f64)] = &[
    ("ActionPoints", 1.0),
    ("HitPoints", 2.0),
    ("RadiationMeter", 4.0),
    ("QuestReminder", 16.0),
    ("Region_Location", 32.0),
    ("ReticleCenter", 64.0),
    ("SneakMeter", 128.0),
    ("Messages", 256.0),
    ("Info", 512.0),
    ("Subtitles", 1024.0),
    ("XPMeter", 4096.0),
    ("BreathMeter", 8192.0),
    ("crippled_limb_indicator", 32768.0),
    ("HardcoreMode", 65536.0),
    ("EnemyHealth", 1.0),
    ("AmmoTypeLabel", 1.0),
    ("CNDArrows", 1.0),
];

/// The children whose alpha shows their piece [observed: hud.md §14.1].
const CHILDREN: &[(&str, &str)] = &[
    ("SneakMeter", "sneak_nif"),
    ("RadiationMeter", "radiation_text"),
    ("RadiationMeter", "radiation_text_value"),
    ("RadiationMeter", "radiation_pointer"),
    ("RadiationMeter", "radiation_bracket"),
];

fn all_shown() -> Shown {
    Shown {
        root: true,
        aiming: false,
        corners: true,
        sneak: true,
        reticle: true,
        enemy: true,
        quests: true,
        messages: true,
        subtitles: true,
        info: true,
        location: true,
        xp: true,
        radiation: true,
        breath: true,
        limbs: true,
        needs: true,
        ammo_label: true,
        condition: true,
    }
}

// The strings observed in the original [observed: hud.md §14.2].
fn texts() -> Texts {
    Texts {
        radiation_word: "RAD/SEC".into(),
        radiation_value: "+0".into(),
        xp_label: "XP".into(),
        xp_level_up: "LEVEL UP".into(),
        needs: ["H20", "FOD", "SLP", "RAD", "LMB"].map(String::from),
        sneak: None,
        prompt_key: None,
        prompt_verb: None,
        frame: Default::default(),
    }
}

fn fixture() -> Ui {
    let mut xml = String::from(r#"<menu name="HUDMainMenu"><visible>17</visible>"#);
    for (name, _, _) in PLACES {
        let children = CHILDREN.iter().filter(|(piece, _)| piece == name);
        let mut children: String = children.map(|(_, child)| format!(r#"<image name="{child}"><alpha>17</alpha><string>sentinel</string></image>"#)).collect();
        let texts: &[&str] = match *name {
            "XPMeter" => &["XPLabel", "XPLevelUp"],
            "HardcoreMode" => &["Dehydration", "Hunger", "Sleep", "Rads", "LMBs"],
            _ => &[],
        };
        for text in texts {
            children.push_str(&format!(r#"<text name="{text}"><string>sentinel</string></text>"#));
        }
        if *name == "Info" {
            children.push_str(r#"<rect name="justify_center_hotrect"><string>sentinel</string><text name="PCShortcutLabel"><string>sentinel</string></text><text name="button_text"><string>sentinel</string></text></rect>"#);
        }
        xml.push_str(&format!(r#"<rect name="{name}"><visible>17</visible>{children}</rect>"#));
    }
    xml.push_str(r#"<rect name="DDTIconAP"><visible>17</visible></rect>"#);
    for name in ["_CrosshairVisible", "_SneakVisible", "_SneakLevel", "_RadiationLevel", "_TargetType", "_ActionType", "_SpreadPercentage"] {
        xml.push_str(&format!("<{name}>17</{name}>"));
    }
    xml.push_str("</menu>");
    let source = MemSource::default().with("hud.xml", &xml);
    let mut ui = Ui::new();
    ui.load_menu(&source, "hud.xml").unwrap();
    ui
}

fn screen(ui: &mut Ui, width: f64, height: f64) {
    let tile = ui.screen();
    ui.set_trait(tile, "width", Value::Num(width));
    ui.set_trait(tile, "height", Value::Num(height));
}

#[test]
fn places_at_the_three_observed_aspect_ratios_and_resize() {
    let mut ui = fixture();
    for (column, (width, height)) in [(1024.0, 768.0), (1280.0, 800.0), (1920.0, 1080.0)].into_iter().enumerate() {
        screen(&mut ui, width, height);
        ui_hud::write(&mut ui, all_shown(), &texts());
        ui.update();
        for (name, xs, y) in PLACES {
            assert_eq!(ui.get_float(&format!("HUDMainMenu/{name}/x")), Some(xs[column]), "{name}, {width}x{height}");
            assert_eq!(ui.get_float(&format!("HUDMainMenu/{name}/y")), Some(*y), "{name}");
        }
        assert_eq!(ui.get_float("HUDMainMenu/CNDArrows/height"), Some(20.0));
        assert_eq!(ui.get_float("HUDMainMenu/DDTIconAP/x"), None);
    }
}

#[test]
fn visible_keeps_its_value_shown_or_not_and_untouched_file_traits() {
    let mut ui = fixture();
    // Everything shown, nothing shown (the reticle hidden too) and the root hidden.
    for shown in [all_shown(), Shown { root: true, ..Shown::default() }, Shown { root: false, ..all_shown() }] {
        ui_hud::write(&mut ui, shown, &texts());
        ui.update();
        assert_eq!(ui.get_float("HUDMainMenu/visible"), Some(f64::from(shown.root)));
        for (name, value) in VISIBLE {
            let shows = shown.root && (*name != "ReticleCenter" || shown.reticle);
            let expected = if shows { *value } else { 0.0 };
            assert_eq!(ui.get_float(&format!("HUDMainMenu/{name}/visible")), Some(expected), "{name}");
        }
        for name in ["Hokeys", "Explosive_positioning_rect", "DDTIcon", "DDTIconAP", "DDTIconEnemy", "DDTIconEnemyAP"] {
            assert_eq!(ui.get_float(&format!("HUDMainMenu/{name}/visible")), Some(17.0), "{name}");
        }
        for name in ["_CrosshairVisible", "_SneakVisible", "_SneakLevel", "_RadiationLevel", "_TargetType", "_ActionType", "_SpreadPercentage"] {
            assert_eq!(ui.get_float(&format!("HUDMainMenu/{name}")), Some(17.0), "{name}");
        }
    }
}

#[test]
fn aiming_hides_the_reticle_and_sets_the_enemy_bar_to_8() {
    let mut ui = fixture();
    ui_hud::write(&mut ui, Shown { aiming: true, ..all_shown() }, &texts());
    assert_eq!(ui.get_float("HUDMainMenu/ReticleCenter/visible"), Some(0.0));
    assert_eq!(ui.get_float("HUDMainMenu/EnemyHealth/visible"), Some(8.0));
    ui_hud::write(&mut ui, all_shown(), &texts());
    assert_eq!(ui.get_float("HUDMainMenu/ReticleCenter/visible"), Some(64.0));
    assert_eq!(ui.get_float("HUDMainMenu/EnemyHealth/visible"), Some(1.0));
}

#[test]
fn sneaking_and_radiation_show_through_the_childrens_alpha() {
    let mut ui = fixture();
    for (sneak, radiation) in [(true, false), (false, true), (false, false)] {
        ui_hud::write(&mut ui, Shown { sneak, radiation, ..all_shown() }, &texts());
        for (piece, child) in CHILDREN {
            let on = if *piece == "SneakMeter" { sneak } else { radiation };
            let expected = if on { 255.0 } else { 0.0 };
            assert_eq!(ui.get_float(&format!("HUDMainMenu/{piece}/{child}/alpha")), Some(expected), "{piece}/{child}");
        }
        assert_eq!(ui.get_float("HUDMainMenu/SneakMeter/visible"), Some(128.0));
        assert_eq!(ui.get_float("HUDMainMenu/RadiationMeter/visible"), Some(4.0));
    }
}

#[test]
fn script_values_hold_until_the_next_write() {
    let mut ui = fixture();
    screen(&mut ui, 1280.0, 800.0);
    ui_hud::write(&mut ui, all_shown(), &texts());
    assert!(ui.set_float("HUDMainMenu/ActionPoints/x", 17.0));
    assert!(ui.set_float("HUDMainMenu/HitPoints/visible", 17.0));
    ui.update();
    assert_eq!(ui.get_float("HUDMainMenu/ActionPoints/x"), Some(17.0));
    assert_eq!(ui.get_float("HUDMainMenu/HitPoints/visible"), Some(17.0));
    ui_hud::write(&mut ui, all_shown(), &texts());
    ui.update();
    assert_eq!(ui.get_float("HUDMainMenu/ActionPoints/x"), Some(1150.0));
    assert_eq!(ui.get_float("HUDMainMenu/HitPoints/visible"), Some(2.0));
}

#[test]
fn no_hud_loaded_means_no_writes() {
    let mut ui = Ui::new();
    screen(&mut ui, 1280.0, 800.0);
    let snapshot = |ui: &Ui| [ui.screen(), ui.strings(), ui.globals()].into_iter().chain(ui.menus()).map(|id| format!("{:?}", ui.tile(id))).collect::<Vec<_>>();
    let before = snapshot(&ui);
    ui_hud::write(&mut ui, all_shown(), &texts());
    assert_eq!(before, snapshot(&ui));
    assert!(ui.find_tile("HUDMainMenu").is_none());
}

#[test]
fn every_observed_file_string_is_written_even_when_hidden() {
    let mut ui = fixture();
    let texts = Texts { radiation_value: "+9".into(), sneak: Some("[HIDDEN]".into()), prompt_key: Some("E)".into()), prompt_verb: Some("Talk".into()), ..texts() };
    ui_hud::write(&mut ui, Shown::default(), &texts);
    ui.update();
    // All file paths and strings [observed: hud.md §14.2].
    for (path, value) in [
        ("RadiationMeter/radiation_text", "RAD/SEC"),
        ("RadiationMeter/radiation_text_value", "+9"),
        ("XPMeter/XPLabel", "XP"),
        ("XPMeter/XPLevelUp", "LEVEL UP"),
        ("HardcoreMode/Dehydration", "H20"),
        ("HardcoreMode/Hunger", "FOD"),
        ("HardcoreMode/Sleep", "SLP"),
        ("HardcoreMode/Rads", "RAD"),
        ("HardcoreMode/LMBs", "LMB"),
        ("SneakMeter/sneak_nif", "[HIDDEN]"),
        ("Info/justify_center_hotrect/PCShortcutLabel", "E)"),
        ("Info/justify_center_hotrect", "Talk"),
        ("Info/justify_center_hotrect/button_text", "Talk"),
    ] {
        assert_eq!(ui.get_string(&format!("HUDMainMenu/{path}/string")).as_deref(), Some(value), "{path}");
    }
}

#[test]
fn standing_and_losing_the_prompt_keep_the_last_strings() {
    let mut ui = fixture();
    let current = Texts { sneak: Some("[CAUTION]".into()), prompt_key: Some("E)".into()), prompt_verb: Some("Talk".into()), ..texts() };
    ui_hud::write(&mut ui, all_shown(), &current);
    ui.update();
    ui_hud::write(&mut ui, Shown { root: true, ..Shown::default() }, &texts());
    ui.update();
    for (path, value) in [
        ("SneakMeter/sneak_nif", "[CAUTION]"),
        ("Info/justify_center_hotrect/PCShortcutLabel", "E)"),
        ("Info/justify_center_hotrect", "Talk"),
        ("Info/justify_center_hotrect/button_text", "Talk"),
    ] {
        assert_eq!(ui.get_string(&format!("HUDMainMenu/{path}/string")).as_deref(), Some(value), "{path}");
    }
    assert_eq!(ui.get_float("HUDMainMenu/SneakMeter/sneak_nif/alpha"), Some(0.0));
}

#[test]
fn no_radiation_writes_plus_zero_and_needs_accept_plugin_words() {
    let mut ui = fixture();
    let current = Texts { radiation_value: "+9".into(), ..texts() };
    ui_hud::write(&mut ui, all_shown(), &current);
    // Plugin words are synthetic [design].
    let current = Texts { needs: ["Water", "Food", "Rest", "Rads", "Limbs"].map(String::from), ..texts() };
    ui_hud::write(&mut ui, Shown { radiation: false, ..all_shown() }, &current);
    ui.update();
    assert_eq!(ui.get_string("HUDMainMenu/RadiationMeter/radiation_text_value/string").as_deref(), Some("+0"));
    for (tile, word) in ["Dehydration", "Hunger", "Sleep", "Rads", "LMBs"].into_iter().zip(current.needs) {
        assert_eq!(ui.get_string(&format!("HUDMainMenu/HardcoreMode/{tile}/string")), Some(word));
    }
}

#[test]
fn missing_file_tiles_are_skipped() {
    let source = MemSource::default().with("hud.xml", r#"<menu name="HUDMainMenu"><rect name="RadiationMeter"><text name="radiation_text"/></rect><rect name="Info"/></menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&source, "hud.xml").unwrap();
    let current = Texts { sneak: Some("[HIDDEN]".into()), prompt_key: Some("E)".into()), prompt_verb: Some("Talk".into()), ..texts() };
    ui_hud::write(&mut ui, all_shown(), &current);
    ui.update();
    assert_eq!(ui.get_string("HUDMainMenu/RadiationMeter/radiation_text/string").as_deref(), Some("RAD/SEC"));
    assert!(ui.find_tile("HUDMainMenu/RadiationMeter/radiation_text_value").is_none());
    assert!(ui.find_tile("HUDMainMenu/Info/justify_center_hotrect").is_none());
}

// Hand-written synthetic templates with sentinel values [design]. Identifiers and
// default tile names [documented: menus/prefabs/hudtemplates.xml; hud.md §14.6].
fn copy_files(omit: &[&str]) -> MemSource {
    let mut xml = String::from(r#"<menu name="HUDMainMenu"><class>&hudmainmenu;</class>"#);
    for (template, kind, name, body) in [
        (
            "template_compass_window",
            "image",
            "compass_window",
            "<width>345</width><height>64</height><x>20</x><y>65</y>",
        ),
        ("template_justify_left_text", "text", "justify_left_text", "<visible>0</visible><alpha>0</alpha>"),
        ("template_justify_right_text", "text", "justify_right_text", ""),
        ("template_justify_center_text", "text", "justify_center_text", ""),
        ("template_meter", "image", "meter", "<height>20</height>"),
        ("template_meter_background", "image", "MeterBackground", "<width>60</width><height>20</height>"),
        ("template_left_bracket", "image", "left_bracket", ""),
        ("template_right_bracket", "image", "right_bracket", ""),
        (
            "template_reticle_center",
            "image",
            "reticle_center",
            "<user0>plain.dds</user0><user1>filled.dds</user1><width>64</width><height>64</height><x>2</x><y>9</y>",
        ),
        ("template_compass_npc_icon_group", "rect", "compass_npc_icon_group", "<y>17</y>"),
        ("template_compass_icon_group", "rect", "compass_icon_group", "<y>17</y>"),
        ("compass_npc_icon", "image", "compass_npc_icon", "<y>-10</y><x><copy src=\"me()\" trait=\"_Heading\"/></x>"),
        ("template_compass_icon_player", "image", "compass_icon", ""),
        ("template_compass_icon_quest", "image", "compass_icon", ""),
        ("template_compass_icon_marker", "image", "compass_icon", ""),
    ] {
        if !omit.contains(&template) {
            xml.push_str(&format!(r#"<template name="{template}"><{kind} name="{name}"><depth>17</depth>{body}</{kind}></template>"#));
        }
    }
    xml.push_str(
        r#"<rect name="HitPoints"/><rect name="ActionPoints"/><rect name="ReticleCenter"/>
        <rect name="Info"><rect name="justify_center_hotrect"/></rect>
        <rect name="Subtitles"/><rect name="RadiationMeter"><image name="radiation_pointer"/></rect></menu>"#,
    );
    MemSource::default().with("menus/hud.xml", &xml)
}

fn copy_tree(omit: &[&str]) -> Ui {
    let files = copy_files(omit);
    let mut ui = Ui::new();
    ui.load_menu(&files, "menus/hud.xml").unwrap();
    ui_hud::build(&mut ui);
    ui.update();
    ui
}

// Ordering [observed: hud.md §14–§14.4]; uncaptured names [hypothesis: hud.md §14.6].
fn assert_copy_paths(ui: &Ui) {
    for (parent, names) in [
        ("HitPoints", &["compass_window", "justify_left_text", "meter", "left_bracket"][..]),
        (
            "ActionPoints",
            &[
                "MeterBackground",
                "meter:0",
                "justify_right_text:0",
                "justify_right_text:1",
                "justify_right_text:2",
                "meter:1",
                "right_bracket",
            ][..],
        ),
        ("ReticleCenter", &["reticle_center"][..]),
        (
            "Info",
            &[
                "justify_center_hotrect",
                "justify_left_text:0",
                "justify_left_text:1",
                "justify_left_text:2",
                "justify_left_text:3",
                "justify_left_text:4",
                "justify_left_text:5",
                "justify_center_text:0",
                "justify_center_text:1",
            ][..],
        ),
        ("HitPoints/compass_window", &["compass_npc_icon_group", "compass_icon_group:0", "compass_icon_group:1"][..]),
    ] {
        for (i, name) in names.iter().enumerate() {
            let named = ui.find_tile(&format!("HUDMainMenu/{parent}/{name}"));
            assert!(named.is_some(), "{parent}/{name}");
            assert_eq!(named, ui.find_tile(&format!("HUDMainMenu/{parent}/*:{i}")));
        }
    }
    assert!(ui.find_tile("HUDMainMenu/HitPoints/compass_window/compass_icon_group:0/compass_icon").is_some());
    for (path, expected) in [("HitPoints/compass_window/compass_npc_icon_group/y", 10.0), ("HitPoints/compass_window/compass_icon_group:0/y", 30.0), ("ReticleCenter/reticle_center/depth", 1.0)] {
        assert_eq!(ui.get_float(&format!("HUDMainMenu/{path}")), Some(expected));
    }
    for i in 0..10 {
        assert_eq!(
            ui.find_tile(&format!("HUDMainMenu/HitPoints/compass_window/compass_npc_icon_group/compass_npc_icon:{i}")),
            ui.find_tile(&format!("HUDMainMenu/HitPoints/*:0/*:0/*:{i}"))
        );
    }
}

#[test]
fn copies_exist_by_name_and_index_on_load_and_rebuild() {
    use fnv_esm::{LoadOrder, Plugin, Schema};
    use fnv_game::{ui_loading, Game, GameConfig, GameData};
    use fnv_script::FunctionTable;
    use fnv_sim::{FormStore, RefTable, SimConfig, World};
    let plugins = vec![Plugin::parse(&common::tes4(), "Empty.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let functions = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &functions, &refs);
    let mut game = Game::new(&data, World::new(&forms, &refs, SimConfig::default()), GameConfig::default());
    for _ in 0..2 {
        let files = copy_files(&[]);
        ui_loading::load(&mut game, Box::new(files), ["menus/hud.xml".into()]);
        assert_copy_paths(&game.ui_tree);
        assert_eq!(game.ui_tree.get_string("HUDMainMenu/HitPoints/*:1/string").as_deref(), Some("HP"));
        assert!(game.ui_tree.find_tile("HUDMainMenu/HitPoints/Injected").is_none());
        game.ui_tree.inject_text(&MemSource::default(), "HUDMainMenu/HitPoints", r#"<rect name="Injected"/>"#);
        game.ui_tree.set_string("HUDMainMenu/HitPoints/*:1/string", "changed");
    }
}

#[test]
fn bar_copies_follow_captured_tick_and_condition_widths() {
    let mut ui = copy_tree(&[]);
    let mut input = texts();
    // Health/AP and condition pairs [observed: hud.md §14.3].
    for (health, hp_width) in [(1.0, 296.0), (0.5, 144.0), (0.1, 24.0)] {
        input.frame.health = health;
        for (ap, width) in [(43.68 / 80.0, 160.0), (52.15 / 80.0, 192.0), (69.7 / 80.0, 256.0), (1.0, 296.0)] {
            input.frame.action_points = ap;
            ui_hud::write(&mut ui, all_shown(), &input);
            ui.update();
            assert_eq!(ui.get_float("HUDMainMenu/HitPoints/*:2/width"), Some(hp_width));
            assert_eq!(ui.get_float("HUDMainMenu/HitPoints/meter/x"), Some(20.0));
            assert_eq!(ui.get_float("HUDMainMenu/ActionPoints/*:5/width"), Some(width));
            assert_eq!(ui.get_float("HUDMainMenu/ActionPoints/*:5/x"), Some(326.0 - width));
        }
    }
    for (condition, width) in [(0.999, 59.0), (0.5, 30.0), (0.4987, 29.0), (0.1, 6.0)] {
        input.frame.condition = Some(condition);
        input.frame.ammo = "12/60".into();
        ui_hud::write(&mut ui, all_shown(), &input);
        ui.update();
        assert_eq!(ui.get_float("HUDMainMenu/ActionPoints/*:1/width"), Some(width));
        assert_eq!(ui.get_float("HUDMainMenu/ActionPoints/*:1/x"), Some(90.0));
        assert_eq!(ui.get_float("HUDMainMenu/ActionPoints/*:1/y"), Some(83.0));
        assert_eq!(ui.get_string("HUDMainMenu/ActionPoints/*:3/string").as_deref(), Some("12/60"));
    }
    assert_eq!(ui.get_string("HUDMainMenu/ActionPoints/*:2/string").as_deref(), Some("CND"));
    assert_eq!(ui.get_string("HUDMainMenu/ActionPoints/*:4/string").as_deref(), Some("AP"));
    assert_eq!(ui.get_float("HUDMainMenu/ActionPoints/*:0/width"), Some(60.0));
}

fn prompt(height: f64) -> ui_hud::Prompt {
    ui_hud::Prompt {
        name: "Goodsprings Settler".into(),
        weight: "--".into(),
        value: "--".into(),
        empty: false,
        locked: false,
        height,
    }
}

#[test]
fn prompt_and_reticle_copies_change_and_keep_absent_strings() {
    let mut ui = copy_tree(&[]);
    let mut input = Texts {
        prompt_verb: Some("Talk".into()),
        ..texts()
    };
    input.frame.prompt = Some(prompt(42.0));
    ui_hud::write(&mut ui, all_shown(), &input);
    ui.update();
    for (i, value) in ["VAL", "--", "WG", "--", "[EMPTY]", "Locked", "Goodsprings Settler", "Talk"].iter().enumerate() {
        assert_eq!(ui.get_string(&format!("HUDMainMenu/Info/*:{}/string", i + 1)).as_deref(), Some(*value));
    }
    for i in [5, 6] {
        assert_eq!(ui.get_float(&format!("HUDMainMenu/Info/*:{i}/visible")), Some(1.0));
        assert_eq!(ui.get_float(&format!("HUDMainMenu/Info/*:{i}/alpha")), Some(0.0));
    }
    assert_eq!(ui.get_float("HUDMainMenu/Info/*:7/height"), Some(42.0));
    assert_eq!(ui.get_float("HUDMainMenu/Info/*:7/y"), Some(2.0));
    assert_eq!(ui.get_string("HUDMainMenu/ReticleCenter/reticle_center/filename").as_deref(), Some("filled.dds"));
    assert_eq!(ui.get_float("HUDMainMenu/ReticleCenter/reticle_center/user0"), Some(0.0));
    assert_eq!(ui.get_string("HUDMainMenu/ReticleCenter/reticle_center/user1").as_deref(), Some("filled.dds"));
    assert_eq!(ui.get_float("HUDMainMenu/ReticleCenter/*:0/depth"), Some(1.0));
    ui_hud::write(&mut ui, Shown { info: false, ..all_shown() }, &texts());
    ui.update();
    assert_eq!(ui.get_string("HUDMainMenu/Info/*:7/string").as_deref(), Some("Goodsprings Settler"));
    assert_eq!(ui.get_string("HUDMainMenu/Info/*:8/string").as_deref(), Some("Talk"));
    assert_eq!(ui.get_float("HUDMainMenu/Info/*:7/alpha"), Some(0.0));
    assert_eq!(ui.get_string("HUDMainMenu/ReticleCenter/reticle_center/filename").as_deref(), Some("plain.dds"));
}

#[test]
fn subtitles_follow_both_captured_prompt_heights_and_restore_without_prompt() {
    let mut ui = copy_tree(&[]);
    let mut input = texts();
    for (height, y) in [(42.0, 610.0), (80.0, 572.0)] {
        input.frame.prompt = Some(prompt(height));
        ui_hud::write(&mut ui, all_shown(), &input);
        ui.update();
        assert_eq!(ui.get_float("HUDMainMenu/Subtitles/y"), Some(y));
    }
    ui_hud::write(&mut ui, Shown { info: false, ..all_shown() }, &input);
    assert_eq!(ui.get_float("HUDMainMenu/Subtitles/y"), Some(660.0));
    ui_hud::write(&mut ui, all_shown(), &texts());
    assert_eq!(ui.get_float("HUDMainMenu/Subtitles/y"), Some(660.0));
}

#[test]
fn pointer_replays_recorded_x_values_with_provisional_scale() {
    let mut ui = copy_tree(&[]);
    let mut input = texts();
    // Outputs [observed: hud.md §14.1–§14.2]; input totals are synthetic [design],
    // testing the unit-slope hypothesis, NOT claimed captured rad/x pairs [hypothesis: §14.6].
    for (rads, x) in [(0.0, 15.0), (21.0, 36.0), (27.0, 42.0), (27.9, 42.9), (1000.0, 42.9), (-1.0, 15.0), (f64::NAN, 15.0)] {
        input.frame.radiation = rads;
        ui_hud::write(&mut ui, all_shown(), &input);
        ui.update();
        assert_eq!(ui.get_float("HUDMainMenu/RadiationMeter/radiation_pointer/x"), Some(x));
    }
}

#[test]
fn compass_marks_write_heading_distance_visibility_and_reuse_slots() {
    use ui_hud::{CompassMark, MarkKind};
    let mut ui = copy_tree(&[]);
    let mut input = texts();
    input.frame.marks = vec![
        CompassMark {
            kind: MarkKind::Actor,
            heading: 139.0,
            distance: 99.0,
        },
        CompassMark {
            kind: MarkKind::Hostile,
            heading: 30.1,
            distance: 99.0,
        },
        CompassMark {
            kind: MarkKind::Quest,
            heading: 148.0,
            distance: 12.0,
        },
        CompassMark {
            kind: MarkKind::Place,
            heading: 272.7,
            distance: 13.0,
        },
        CompassMark {
            kind: MarkKind::Found,
            heading: 279.0,
            distance: 14.0,
        },
    ];
    ui_hud::write(&mut ui, all_shown(), &input);
    ui.update();
    for (path, x, distance, visible) in [
        ("compass_npc_icon_group/*:0", 139.0, 1.0, 1.0),
        ("compass_npc_icon_group/*:1", 30.1, 1.0, 1.0),
        ("compass_icon_group:0/*:1", 148.0, 12.0, 1.0),
        ("compass_icon_group:1/*:0", 272.7, 13.0, 1.0),
        ("compass_icon_group:1/*:1", 279.0, 14.0, 0.0),
    ] {
        let path = format!("HUDMainMenu/HitPoints/compass_window/{path}");
        assert_eq!(ui.get_float(&format!("{path}/_Heading")), Some(x));
        assert_eq!(ui.get_float(&format!("{path}/x")), Some(x));
        assert_eq!(ui.get_float(&format!("{path}/_Distance")), Some(distance));
        assert_eq!(ui.get_float(&format!("{path}/visible")), Some(visible));
    }
    let npc = ui.find_tile("HUDMainMenu/HitPoints/compass_window/compass_npc_icon_group").unwrap();
    assert_eq!(ui.tile(npc).children.len(), 10);
    assert_eq!(ui.tile(npc).get("y"), Some(&Value::Num(10.0)));
    let quest = ui.find_tile("HUDMainMenu/HitPoints/compass_window/compass_icon_group:0/*:1").unwrap();
    input.frame.marks.clear();
    ui_hud::write(&mut ui, all_shown(), &input);
    assert_eq!(ui.tile(quest).get("visible"), Some(&Value::Num(0.0)));
    input.frame.marks.push(CompassMark {
        kind: MarkKind::Quest,
        heading: 139.0,
        distance: 1.0,
    });
    ui_hud::write(&mut ui, all_shown(), &input);
    assert_eq!(ui.find_tile("HUDMainMenu/HitPoints/compass_window/compass_icon_group:0/*:1"), Some(quest));
    assert_eq!(ui.tile(quest).get("visible"), Some(&Value::Num(1.0)));
}

#[test]
fn missing_templates_and_parents_skip_quietly_without_cross_writes() {
    ui_hud::build(&mut Ui::new());
    let mut ui = fixture();
    ui_hud::build(&mut ui);
    assert!(ui.warnings.is_empty());
    assert!(ui.find_tile("HUDMainMenu/HitPoints/compass_window").is_none());
    // A mod's own same-named tile is not a code copy [design].
    ui.inject_text(&MemSource::default(), "HUDMainMenu/HitPoints", r#"<image name="meter"><width>17</width></image>"#);
    ui_hud::write(&mut ui, all_shown(), &texts());
    assert_eq!(ui.get_float("HUDMainMenu/HitPoints/meter/width"), Some(17.0));
    for missing in [
        "template_meter",
        "template_justify_left_text",
        "template_compass_npc_icon_group",
        "template_compass_icon_player",
        "template_compass_icon_quest",
    ] {
        let mut ui = copy_tree(&[missing]);
        let mut input = texts();
        input.frame.marks.push(ui_hud::CompassMark {
            kind: ui_hud::MarkKind::Quest,
            heading: 139.0,
            distance: 1.0,
        });
        ui_hud::write(&mut ui, all_shown(), &input);
        ui.update();
        assert!(ui.warnings.is_empty(), "{missing}");
        assert_eq!(ui.get_string("HUDMainMenu/ActionPoints/justify_right_text:0/string").as_deref(), Some("CND"));
        if missing == "template_meter" {
            assert!(ui.find_tile("HUDMainMenu/ActionPoints/meter").is_none());
            assert_eq!(ui.get_float("HUDMainMenu/ActionPoints/MeterBackground/width"), Some(60.0));
        }
        if missing == "template_compass_icon_player" {
            assert_eq!(ui.get_float("HUDMainMenu/HitPoints/compass_window/compass_icon_group:0/*:0/visible"), Some(1.0));
        }
    }
}
