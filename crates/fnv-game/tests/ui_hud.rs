//! Synthetic XML and sentinel values [design]; expected HUD values [observed: hud.md §14].

use fnv_game::ui_hud::{self, Shown, Texts};
use fnv_ui::{MemSource, Ui, Value};

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
