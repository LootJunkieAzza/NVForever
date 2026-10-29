//! T12a: the vanilla HUD drawn from its tiles, behind a switch [design: ui-tile-tree.md
//! §6.2 step 3; hud-xml.md step 5].
//!
//! - **The switch**: `[UI] TileMenus=` in the home's `NVForever.ini`, a comma-separated
//!   list of menu names, empty by default; only `HUDMainMenu` is acted on for now. It is
//!   read once at start, from the INI the settings page uses
//!   (`crate::settings::SettingsFile`: an unattended run in the player's own home reads
//!   none). The test aid `NVF_PLAY_TEST_TILE_MENUS=<list>` sets it instead, so a smoke
//!   scenario never writes the player's INI. [`TileMenus`] holds it.
//! - **With the HUD a tile menu**, `super::draw` draws the whole `HUDMainMenu` tree (the
//!   file's tiles, the code-made copies, the mods'; `super::draw::Select::Whole`) and
//!   the hand-built pieces that have a tile-drawn equivalent are hidden ([`DrawnByTiles`],
//!   [`hide_hand_built`]). The pieces whose tiles the engine does not fill yet stay
//!   hand-built and their file and code tiles are not drawn ([`KEPT_PIECES`]); the mapping
//!   is in docs/spec-notes/hud.md §14.7.
//! - [`write_corners`] gives the corner labels' code-made copies the places the
//!   hand-built HUD sets them at (the captures give none), the ammo type's label its
//!   abbreviation, and the condition row its showing rule, only while the switch is on
//!   [design: the tree's values for mods stay T4's when it is off].

use crate::settings::SettingsFile;
use bevy::prelude::*;
use fnv_install::ini_get;
use fnv_ui::{TileId, Ui, Value};

/// The INI section and key [design: ui-tile-tree.md §6.2].
pub const SECTION: &str = "UI";
pub const KEY: &str = "TileMenus";
/// The test aid that sets the list instead of the INI.
pub const AID: &str = "NVF_PLAY_TEST_TILE_MENUS";

/// Which menus are drawn from their tiles.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TileMenus {
    /// `HUDMainMenu`: the vanilla HUD from tiles, the hand-built one hidden where a tile
    /// draws its piece.
    pub hud: bool,
}

impl TileMenus {
    /// A comma-separated list of menu names (case does not matter, blanks around a name
    /// neither): the switches, and the names not acted on.
    pub fn parse(list: &str) -> (TileMenus, Vec<String>) {
        let mut menus = TileMenus::default();
        let mut other = Vec::new();
        for name in list.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            if name.eq_ignore_ascii_case(super::draw::HUD_MENU) {
                menus.hud = true;
            } else {
                other.push(name.to_string());
            }
        }
        (menus, other)
    }

    /// The list for this run and where it was read: the test aid, else the INI's
    /// `[UI] TileMenus=`; `None` when neither gives one.
    pub fn list_for_run(aid: Option<String>, ini: &SettingsFile) -> Option<(String, String)> {
        if let Some(list) = aid {
            return Some((list, format!("test aid {AID}")));
        }
        let path = ini.0.as_ref()?;
        let text = std::fs::read_to_string(path).ok()?;
        ini_get(&text, SECTION, KEY).map(|list| (list, path.display().to_string()))
    }
}

/// Read the switch once at start and tell it.
pub(crate) fn setup(app: &mut App) {
    let unattended = crate::test_aid::Unattended::from_env().0;
    let list = TileMenus::list_for_run(std::env::var(AID).ok(), &SettingsFile::for_run(unattended));
    let (menus, other) = list.as_ref().map_or_else(|| (TileMenus::default(), Vec::new()), |(l, _)| TileMenus::parse(l));
    app.insert_resource(menus);
    app.add_systems(Startup, move || {
        let Some((list, from)) = &list else { return };
        info!("tiles: [{SECTION}] {KEY}={list} (from {from}): {}", if menus.hud { "HUDMainMenu drawn from tiles" } else { "no menu drawn from tiles" });
        if !other.is_empty() {
            warn!("tiles: [{SECTION}] {KEY}: {} not drawn from tiles yet (only HUDMainMenu is)", other.join(", "));
        }
    });
}

/// The HUD's pieces NVForever keeps drawing by hand while the HUD is drawn from tiles,
/// because the engine does not fill their tiles yet [design: hud.md §14.7]: the enemy's
/// health (its children are not built, hud.md §14.6), the prompt (its code-made texts
/// have no captured places and its divider is not built), the Hardcore words (no places,
/// the piece's alpha not written), the XP meter and the limbs (their children's
/// visibility and alpha not written), the breath meter (a `nif` tile, not drawn), the
/// corner and quest messages and the subtitles (their texts not built). The file's and the
/// code's tiles under them are not drawn; a mod's still are.
pub const KEPT_PIECES: [&str; 10] =
    ["EnemyHealth", "Info", "HardcoreMode", "XPMeter", "crippled_limb_indicator", "BreathMeter", "Messages", "QuestReminder", "Region_Location", "Subtitles"];

/// The compass window, whose picture is the strip the heading moves [observed: hud.md
/// §14.4, the name read; §14.5].
pub const STRIP_TILE: &str = "HUDMainMenu/HitPoints/compass_window";

/// The [`KEPT_PIECES`] the tree has.
pub fn kept_pieces(ui: &Ui) -> Vec<TileId> {
    KEPT_PIECES.iter().filter_map(|p| ui.find_tile(&format!("{}/{p}", super::draw::HUD_MENU))).collect()
}

/// A hand-built HUD node whose piece the tiles draw while the HUD is a tile menu: the
/// health and action points corners (with the compass, the condition row and the ammo),
/// the sneak word, the crosshair, the radiation meter.
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct DrawnByTiles;

/// Hide [`DrawnByTiles`] nodes while the HUD is a tile menu and the tree has it (without
/// the game's menu files the hand-built HUD stays): after every drawing system has set its
/// visibility, before it is passed down [design: no drawer changes].
pub(crate) fn hide_hand_built(menus: Res<TileMenus>, script: Res<crate::script::ScriptState>, mut nodes: Query<&mut Visibility, With<DrawnByTiles>>) {
    let loaded = script.rt.as_ref().is_some_and(|rt| rt.game.ui_files.is_some() && rt.game.ui_tree.menu(super::draw::HUD_MENU).is_some());
    if !menus.hud || !loaded {
        return;
    }
    for mut v in &mut nodes {
        v.set_if_neq(Visibility::Hidden);
    }
}

/// What [`write_corners`] writes, in the corner pieces' units.
#[derive(Clone, Debug, PartialEq)]
pub struct Corner {
    /// "HP"'s left and top; the right ends and tops of "AP", "CND" and the ammo counter
    /// (right-justified copies).
    pub hp: (f64, f64),
    pub ap: (f64, f64),
    pub cnd: (f64, f64),
    pub ammo: (f64, f64),
    /// The right bracket's x in `ActionPoints`.
    pub right_bracket: f64,
    /// The ammo type's abbreviation, empty without one.
    pub ammo_label: String,
    /// A weapon with a condition is equipped.
    pub condition: bool,
}

fn set(ui: &mut Ui, path: &str, name: &str, value: Value) {
    if let Some(tile) = ui.find_tile(&format!("{}/{path}", super::draw::HUD_MENU)) {
        ui.set_trait(tile, name, value);
    }
}

/// The condition ground's alpha: a quarter of the fill's [design: the hand-built HUD's
/// ground, `hud::GROUND`, a hypothesis after `meter.xml`'s; the template's own, none,
/// would hide the fill on a ground of the same colour].
pub const GROUND_ALPHA: f64 = 64.0;

/// Write what the tile-drawn corners need beyond T4's writes, before scripts, only while
/// the HUD is a tile menu [design: ui-tile-tree.md §4.1]:
/// - the corner labels' copies (`HitPoints/justify_left_text`,
///   `ActionPoints/justify_right_text:0`-`:2`, "CND", the ammo counter, "AP") where the
///   hand-built HUD sets its labels, and `ActionPoints/right_bracket` where it draws its
///   bracket [design: hud.rs `label_places`, the captures give none of these places,
///   hud.md §14.6; hypothesis: the original's right bracket stands where the hand-built
///   one does on the screen, hud.md §14 "the current HUD's right corner still lands where
///   the original's does"];
/// - `AmmoTypeLabel`'s string, the weapon's ammo abbreviation [hypothesis: by the tile's
///   name and place, the right piece's lower row; its string was not read, hud.md §14.2];
/// - the condition row (the ground, "CND", `CNDArrows`) shown by alpha only with a
///   weapon's condition, as the hand-built HUD hides its row [design].
pub fn write_corners(ui: &mut Ui, c: &Corner) {
    for (path, (x, y)) in [
        ("HitPoints/justify_left_text", c.hp),
        ("ActionPoints/justify_right_text:0", c.cnd),
        ("ActionPoints/justify_right_text:1", c.ammo),
        ("ActionPoints/justify_right_text:2", c.ap),
    ] {
        set(ui, path, "x", Value::Num(x));
        set(ui, path, "y", Value::Num(y));
    }
    set(ui, "ActionPoints/right_bracket", "x", Value::Num(c.right_bracket));
    set(ui, "AmmoTypeLabel", "string", Value::Str(c.ammo_label.clone()));
    let on = |alpha: f64| Value::Num(if c.condition { alpha } else { 0.0 });
    set(ui, "ActionPoints/MeterBackground", "alpha", on(GROUND_ALPHA));
    set(ui, "ActionPoints/justify_right_text:0", "alpha", on(255.0));
    set(ui, "CNDArrows", "alpha", on(255.0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::MemSource;

    #[test]
    fn the_switch_is_a_list_of_menu_names() {
        assert_eq!(TileMenus::parse(""), (TileMenus { hud: false }, Vec::new()));
        assert_eq!(TileMenus::parse("HUDMainMenu"), (TileMenus { hud: true }, Vec::new()));
        assert_eq!(TileMenus::parse(" hudmainmenu , StartMenu,,"), (TileMenus { hud: true }, vec!["StartMenu".to_string()]));
        assert_eq!(TileMenus::parse("StartMenu").0, TileMenus { hud: false });
    }

    #[test]
    fn the_switch_comes_from_the_aid_else_the_ini() {
        let dir = std::env::temp_dir().join(format!("nvf-tile-menus-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("NVForever.ini");
        std::fs::write(&path, "[Game]\nPath=F:\\FNV\n\n[UI]\nTileMenus=HUDMainMenu\n").unwrap();
        let ini = SettingsFile(Some(path.clone()));
        let (list, from) = TileMenus::list_for_run(None, &ini).unwrap();
        assert_eq!((list.as_str(), from), ("HUDMainMenu", path.display().to_string()));
        // The test aid wins, even when empty; no INI and no aid give none.
        assert_eq!(TileMenus::list_for_run(Some(String::new()), &ini).unwrap().0, "");
        assert!(TileMenus::list_for_run(None, &SettingsFile(None)).is_none());
        std::fs::write(&path, "[Settings]\ngrass=1\n").unwrap();
        assert!(TileMenus::list_for_run(None, &ini).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_corners_take_the_hand_built_places_and_the_condition_row_its_rule() {
        let src = MemSource::default().with(
            "hud.xml",
            r#"<menu name="HUDMainMenu"><rect name="HitPoints"/><rect name="ActionPoints"/><image name="CNDArrows"/><text name="AmmoTypeLabel"/>
              <template name="template_justify_left_text"><text name="justify_left_text"><x>0</x></text></template>
              <template name="template_justify_right_text"><text name="justify_right_text"><x>0</x></text></template>
              <template name="template_meter_background"><image name="MeterBackground"/></template>
              <template name="template_right_bracket"><image name="right_bracket"/></template></menu>"#,
        );
        let mut ui = Ui::new();
        ui.load_menu(&src, "hud.xml").unwrap();
        for (at, template) in [
            ("HUDMainMenu/HitPoints", "template_justify_left_text"),
            ("HUDMainMenu/ActionPoints", "template_meter_background"),
            ("HUDMainMenu/ActionPoints", "template_justify_right_text"),
            ("HUDMainMenu/ActionPoints", "template_justify_right_text"),
            ("HUDMainMenu/ActionPoints", "template_justify_right_text"),
            ("HUDMainMenu/ActionPoints", "template_right_bracket"),
        ] {
            ui.add_code_tile_from_template(&src, at, template, None).unwrap();
        }
        let mut c = Corner { hp: (23.0, 10.0), ap: (315.0, 10.0), cnd: (79.0, 83.0), ammo: (317.0, 83.0), right_bracket: -40.0, ammo_label: "9mm".into(), condition: true };
        write_corners(&mut ui, &c);
        let f = |ui: &Ui, p: &str| ui.get_float(&format!("HUDMainMenu/{p}")).unwrap();
        assert_eq!((f(&ui, "HitPoints/justify_left_text/x"), f(&ui, "HitPoints/justify_left_text/y")), (23.0, 10.0));
        assert_eq!(f(&ui, "ActionPoints/justify_right_text:0/x"), 79.0);
        assert_eq!(f(&ui, "ActionPoints/justify_right_text:1/x"), 317.0);
        assert_eq!(f(&ui, "ActionPoints/justify_right_text:2/x"), 315.0);
        assert_eq!(f(&ui, "ActionPoints/right_bracket/x"), -40.0);
        assert_eq!(ui.get_string("HUDMainMenu/AmmoTypeLabel/string").as_deref(), Some("9mm"));
        assert_eq!((f(&ui, "ActionPoints/MeterBackground/alpha"), f(&ui, "CNDArrows/alpha")), (GROUND_ALPHA, 255.0));
        c.condition = false;
        write_corners(&mut ui, &c);
        assert_eq!((f(&ui, "ActionPoints/MeterBackground/alpha"), f(&ui, "ActionPoints/justify_right_text:0/alpha"), f(&ui, "CNDArrows/alpha")), (0.0, 0.0, 0.0));
    }
}
