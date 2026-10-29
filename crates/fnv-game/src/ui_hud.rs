//! HUD places and visibility beside the current drawing [design: ui-tile-tree.md §6.2].

use fnv_ui::{Ui, Value};

/// What the engine's HUD currently shows; no drawing or engine types here [design].
/// The pieces' `visible` does not follow these; the children it is observed on do
/// (`write`), the rest wait for their captures [observed: hud.md §14.1].
#[derive(Clone, Copy, Debug, Default)]
pub struct Shown {
    pub root: bool,
    /// Block held with a gun, the view zooming to its Sight FOV.
    pub aiming: bool,
    pub corners: bool,
    pub sneak: bool,
    pub reticle: bool,
    pub enemy: bool,
    pub quests: bool,
    pub messages: bool,
    pub subtitles: bool,
    pub info: bool,
    pub location: bool,
    pub xp: bool,
    pub radiation: bool,
    pub breath: bool,
    pub limbs: bool,
    pub needs: bool,
    pub ammo_label: bool,
    pub condition: bool,
}

/// Called after menu lifetime sync, before scripts [design: ui-tile-tree.md §4.1].
pub fn write(ui: &mut Ui, shown: Shown) {
    let Some(root) = ui.find_tile("HUDMainMenu") else { return };
    // A script's SetUIFloat lasts until the next frame's write [hypothesis:
    // menu-xml.md §6, ui-tile-tree.md §4.2; original overwrite frequency unobserved].
    // Root shown 1 [observed: hud.md §14], hidden 0 [hypothesis].
    ui.set_trait(root, "visible", Value::Num(f64::from(shown.root)));
    // Leave root code values alone pending hud.md §14 "Open" captures [design].
    let screen = ui.tile(ui.screen());
    let size = screen.get("width").zip(screen.get("height")).map(|(w, h)| (w.num(), h.num()));
    if let Some((width, height)) = size.filter(|(w, h)| w.is_finite() && h.is_finite() && *w > 0.0 && *h > 0.0) {
        // The view is 960 units high [documented: hud.md §2/§14].
        let w = 960.0 * width / height;
        // Truncate W and W/2 before offsets [hypothesis: hud.md §14: at 16:9,
        // W - 386 = 1320.67 reads 1320; truncating the middle before the half-unit
        // offset also reproduces the child's 820.5. Nearest would give 1321].
        let (right, middle) = (w.floor(), (w / 2.0).floor());
        // Every place below [observed: hud.md §14, every piece's place table].
        for (name, x, y) in [
            ("HitPoints", 40.0, 803.0),
            ("ActionPoints", right - 386.0, 803.0),
            ("HardcoreMode", 190.0, 803.0),
            ("SneakMeter", middle, 30.0),
            ("ReticleCenter", middle - 34.5, 439.5),
            ("EnemyHealth", middle - 175.0, 843.0),
            ("QuestReminder", 45.0, 200.0),
            ("Messages", 25.0, 55.0),
            ("Subtitles", middle - 400.0, 660.0),
            ("Info", middle - 160.0, 820.0),
            ("Hokeys", right - 371.0, 483.0),
            ("Region_Location", 45.0, 728.0),
            ("XPMeter", right - 420.0, 650.0),
            ("RadiationMeter", right - 417.0, 30.0),
            ("BreathMeter", middle, 110.0),
            ("Explosive_positioning_rect", middle, 480.0),
            ("crippled_limb_indicator", right - 230.0, 130.0),
            ("DDTIcon", 414.0, 852.0),
            ("DDTIconEnemy", middle - 207.0, 846.0),
            ("DDTIconEnemyAP", middle - 207.0, 846.0),
            ("AmmoTypeLabel", right - 236.0, 883.0),
            ("CNDArrows", right - 253.0, 886.0),
        ] {
            if let Some(tile) = ui.find_tile(&format!("HUDMainMenu/{name}")) {
                ui.set_trait(tile, "x", Value::Num(x));
                ui.set_trait(tile, "y", Value::Num(y));
            }
        }
    }
    if let Some(tile) = ui.find_tile("HUDMainMenu/CNDArrows") {
        ui.set_trait(tile, "height", Value::Num(20.0)); // [observed: hud.md §14].
    }
    // A piece's visible keeps its value whether or not the piece shows: SneakMeter,
    // Info and RadiationMeter kept theirs out of sneaking, with no prompt and out of
    // radiation, as BreathMeter, XPMeter and the limb indicator when not shown. Only
    // aiming changed one: ReticleCenter 0, EnemyHealth 8 [observed: hud.md §14.1].
    // The corners, AmmoTypeLabel and CNDArrows were seen shown only [hypothesis: the
    // same]. With the root hidden they read 0 [hypothesis: unobserved]. Pieces we do
    // not draw keep the file's visible [design].
    let reticle = if shown.reticle && !shown.aiming { 64.0 } else { 0.0 };
    let enemy = if shown.aiming { 8.0 } else { 1.0 };
    for (name, value) in [
        ("ActionPoints", 1.0),
        ("HitPoints", 2.0),
        ("RadiationMeter", 4.0),
        ("QuestReminder", 16.0),
        ("Region_Location", 32.0),
        ("ReticleCenter", reticle),
        ("SneakMeter", 128.0),
        ("Messages", 256.0),
        ("Info", 512.0),
        ("Subtitles", 1024.0),
        ("XPMeter", 4096.0),
        ("BreathMeter", 8192.0),
        ("crippled_limb_indicator", 32768.0),
        ("HardcoreMode", 65536.0),
        ("EnemyHealth", enemy),
        ("AmmoTypeLabel", 1.0),
        ("CNDArrows", 1.0),
    ] {
        if let Some(tile) = ui.find_tile(&format!("HUDMainMenu/{name}")) {
            ui.set_trait(tile, "visible", Value::Num(if shown.root { value } else { 0.0 }));
        }
    }
    // What shows is the children's alpha: the sneak word while sneaking, the radiation
    // meter's four pieces in radiation, 255 shown and 0 not [observed: hud.md §14.1].
    for (path, on) in [
        ("SneakMeter/sneak_nif", shown.sneak),
        ("RadiationMeter/radiation_text", shown.radiation),
        ("RadiationMeter/radiation_text_value", shown.radiation),
        ("RadiationMeter/radiation_pointer", shown.radiation),
        ("RadiationMeter/radiation_bracket", shown.radiation),
    ] {
        if let Some(tile) = ui.find_tile(&format!("HUDMainMenu/{path}")) {
            ui.set_trait(tile, "alpha", Value::Num(if on { 255.0 } else { 0.0 }));
        }
    }
}
