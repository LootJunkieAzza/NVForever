//! Template copies beside the existing drawing [design: ui-tile-tree.md §4.1].

use super::{Shown, Texts};
use fnv_ui::{MemSource, TileId, Ui, Value};

/// Kinds supplied by the drawing, including its unused player marker [design].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    Actor,
    Hostile,
    Place,
    Found,
    Quest,
    Player,
}

/// A mark in compass-window units [design]. `heading` is the tile's left edge,
/// equal to `_Heading` [observed: hud.md §14.4–§14.5].
#[derive(Clone, Debug)]
pub struct CompassMark {
    pub kind: MarkKind,
    pub heading: f64,
    pub distance: f64,
}

/// Prompt details shared with the drawing [design]. Height 42/80 gives the two
/// captured subtitle positions; other heights follow the same rule [hypothesis:
/// hud.md §14.3]. Width measurement is deferred [design].
#[derive(Clone, Debug)]
pub struct Prompt {
    pub name: String,
    pub weight: String,
    pub value: String,
    pub empty: bool,
    pub locked: bool,
    pub height: f64,
}

/// Frame values independent of the engine [design]. Shares are bounded to 0..1;
/// defaults are full health/AP, no weapon, prompt or marks [design].
#[derive(Clone, Debug)]
pub struct Frame {
    pub health: f64,
    pub action_points: f64,
    pub condition: Option<f64>,
    pub ammo: String,
    pub radiation: f64,
    pub prompt: Option<Prompt>,
    pub marks: Vec<CompassMark>,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            health: 1.0,
            action_points: 1.0,
            condition: None,
            ammo: String::new(),
            radiation: 0.0,
            prompt: None,
            marks: Vec::new(),
        }
    }
}

/// All HUD code-copy creation goes here [design]. After the parallel tile-origin
/// change merges, marking the returned tile `Code` is a one-line change here.
/// Includes are already expanded in loaded templates [documented:
/// crates/fnv-ui/src/tree.rs, Ui::add_from_template / expand_includes].
fn code_copy(ui: &mut Ui, at: &str, template: &str, name: &str) -> Option<TileId> {
    let tile = ui.add_from_template(&MemSource::default(), at, template, Some(name))?;
    // Keep separate pools even when templates are missing or share names [design].
    ui.set_trait(tile, "_NVFHudTemplate", Value::Str(template.into()));
    Some(tile)
}

fn num(ui: &mut Ui, tile: TileId, name: &str, value: f64) {
    ui.set_trait(tile, name, Value::Num(value));
}

fn string(ui: &mut Ui, tile: TileId, value: &str) {
    ui.set_trait(tile, "string", Value::Str(value.into()));
}

fn find(ui: &Ui, path: &str) -> Option<TileId> {
    ui.find_tile(&format!("HUDMainMenu/{path}")).filter(|id| ui.tile(*id).get("_NVFHudTemplate").is_some())
}

/// Called once after building each HUD menu, including save-load rebuilds [design].
/// Ordering follows the captured `*:N` paths [observed: hud.md §14–§14.4].
/// Uncaptured copy names retain template names [hypothesis: hud.md §14.6];
/// template identifiers/names [documented: menus/prefabs/hudtemplates.xml].
pub fn build(ui: &mut Ui) {
    for (parent, template, name) in [
        ("HitPoints", "template_compass_window", "compass_window"),
        ("HitPoints", "template_justify_left_text", "justify_left_text"),
        ("HitPoints", "template_meter", "meter"),
        ("HitPoints", "template_left_bracket", "left_bracket"),
        ("ActionPoints", "template_meter_background", "MeterBackground"),
        ("ActionPoints", "template_meter", "meter"),
        ("ActionPoints", "template_justify_right_text", "justify_right_text"),
        ("ActionPoints", "template_justify_right_text", "justify_right_text"),
        ("ActionPoints", "template_justify_right_text", "justify_right_text"),
        ("ActionPoints", "template_meter", "meter"),
        ("ActionPoints", "template_right_bracket", "right_bracket"),
        ("ReticleCenter", "template_reticle_center", "reticle_center"),
    ] {
        code_copy(ui, &format!("HUDMainMenu/{parent}"), template, name);
    }
    for _ in 0..6 {
        code_copy(ui, "HUDMainMenu/Info", "template_justify_left_text", "justify_left_text");
    }
    for _ in 0..2 {
        code_copy(ui, "HUDMainMenu/Info", "template_justify_center_text", "justify_center_text");
    }
    let window = "HUDMainMenu/HitPoints/compass_window";
    if let Some(group) = code_copy(ui, window, "template_compass_npc_icon_group", "compass_npc_icon_group") {
        num(ui, group, "y", 10.0); // [observed: hud.md §14.4].
        for _ in 0..10 {
            if let Some(tick) = code_copy(ui, &ui.path_of(group), "compass_npc_icon", "compass_npc_icon") {
                num(ui, tick, "visible", 0.0); // Unused slots hidden [design].
            }
        }
    }
    for _ in 0..2 {
        if let Some(group) = code_copy(ui, window, "template_compass_icon_group", "compass_icon_group") {
            num(ui, group, "y", 30.0); // [observed: hud.md §14.4].
        }
    }
    // Player/quest in group 1, places in group 2 [hypothesis: hud.md §14.6].
    if let Some(player) = code_copy(ui, &format!("{window}/compass_icon_group:0"), "template_compass_icon_player", "compass_icon") {
        num(ui, player, "visible", 0.0);
    }
    // Only captured code overrides; all other traits come from loaded templates
    // [design]. Coordinates, strings and widths [observed: hud.md §14–§14.4].
    for (path, x, y) in [
        ("HitPoints/meter", 20.0, 41.0),
        ("ActionPoints/meter:0", 90.0, 83.0),
        ("ActionPoints/MeterBackground", 90.0, 83.0),
        ("ActionPoints/meter:1", 30.0, 41.0),
    ] {
        if let Some(tile) = find(ui, path) {
            num(ui, tile, "x", x);
            num(ui, tile, "y", y);
        }
    }
    if let Some(tile) = find(ui, "ActionPoints/meter:0") {
        ui.set_trait(tile, "filename", Value::Str("Interface\\VATS\\vats_bar.dds".into()));
    }
    if let Some(tile) = find(ui, "ReticleCenter/reticle_center") {
        num(ui, tile, "depth", 1.0);
    }
    for (path, text) in [
        ("HitPoints/justify_left_text", "HP"),
        ("ActionPoints/justify_right_text:0", "CND"),
        ("ActionPoints/justify_right_text:2", "AP"),
        ("Info/justify_left_text:0", "VAL"),
        ("Info/justify_left_text:1", "--"),
        ("Info/justify_left_text:2", "WG"),
        ("Info/justify_left_text:3", "--"),
        ("Info/justify_left_text:4", "[EMPTY]"),
        ("Info/justify_left_text:5", "Locked"),
    ] {
        if let Some(tile) = find(ui, path) {
            string(ui, tile, text);
        }
    }
    if let Some(tile) = find(ui, "HitPoints/justify_left_text") {
        // The left-text template starts hidden [documented: hudtemplates.xml];
        // showing this label through alpha/visible is a hypothesis [hypothesis: hud.md §14.6].
        num(ui, tile, "visible", 1.0);
        num(ui, tile, "alpha", 255.0);
    }
    if let Some(tile) = find(ui, "Info/justify_center_text:0") {
        num(ui, tile, "y", 2.0); // [observed: hud.md §14.1].
    }
}

fn share(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub(super) fn write(ui: &mut Ui, shown: Shown, texts: &Texts) {
    let input = &texts.frame;
    // Whole ticks, AP right anchored; condition nearest unit [observed: hud.md §14.3].
    for (path, value, right) in [("HitPoints/meter", input.health, false), ("ActionPoints/meter:1", input.action_points, true)] {
        if let Some(tile) = find(ui, path) {
            let width = (share(value) * 37.0).floor() * 8.0;
            num(ui, tile, "width", width);
            num(ui, tile, "x", if right { 30.0 + 296.0 - width } else { 20.0 });
        }
    }
    if let Some(tile) = find(ui, "ActionPoints/meter:0") {
        num(ui, tile, "width", (59.0 * share(input.condition.unwrap_or(0.0))).round());
    }
    if let Some(tile) = find(ui, "ActionPoints/justify_right_text:1") {
        string(ui, tile, &input.ammo);
    }
    if let Some(tile) = find(ui, "ReticleCenter/reticle_center") {
        // Keep user0/user1 as the template's strings: their numeric reads are 0
        // [observed: hud.md §14.4]. Select the loaded alternate [design].
        let picture = ui.tile(tile).get(if shown.info { "user1" } else { "user0" }).cloned();
        if let Some(picture) = picture {
            ui.set_trait(tile, "filename", picture);
        }
    }
    if let Some(prompt) = &input.prompt {
        for (path, text) in [
            ("Info/justify_left_text:1", prompt.value.as_str()),
            ("Info/justify_left_text:3", prompt.weight.as_str()),
            ("Info/justify_center_text:0", prompt.name.as_str()),
        ] {
            if let Some(tile) = find(ui, path) {
                string(ui, tile, text);
            }
        }
        if let Some(tile) = find(ui, "Info/justify_center_text:0") {
            num(ui, tile, "height", prompt.height);
        }
    }
    if let (Some(tile), Some(verb)) = (find(ui, "Info/justify_center_text:1"), &texts.prompt_verb) {
        string(ui, tile, verb);
    }
    // Prompt text visibility/alpha follows showing rules [hypothesis: hud.md §14.6].
    // Empty/locked alpha is 0 in the actor capture [observed: hud.md §14.1–§14.2].
    for (name, count) in [("justify_left_text", 6), ("justify_center_text", 2)] {
        for i in 0..count {
            if let Some(tile) = find(ui, &format!("Info/{name}:{i}")) {
                let on = shown.info && input.prompt.is_some();
                let alpha = if name == "justify_left_text" && i >= 4 {
                    on && input.prompt.as_ref().is_some_and(|p| if i == 4 { p.empty } else { p.locked })
                } else {
                    on
                };
                num(ui, tile, "visible", f64::from(on));
                num(ui, tile, "alpha", if alpha { 255.0 } else { 0.0 });
            }
        }
    }
    if let Some(tile) = ui.find_tile("HUDMainMenu/Subtitles") {
        // Two captured heights; immediate return without a prompt is a hypothesis
        // [hypothesis: hud.md §14.1/§14.3; capture prompt removal with subtitles to settle].
        let up = input.prompt.as_ref().filter(|_| shown.info).map_or(0.0, |p| p.height.max(0.0) + 8.0);
        num(ui, tile, "y", 660.0 - up);
    }
    if let Some(tile) = ui.find_tile("HUDMainMenu/RadiationMeter/radiation_pointer") {
        num(ui, tile, "x", radiation_pointer_x(input.radiation));
    }
    write_marks(ui, &input.marks);
}

/// The captured x range is 15..42.9, without paired accumulated-rad readings
/// [observed: hud.md §14.1]. Unit slope from 15 within that range is the minimal
/// provisional interpolation [hypothesis: hud.md §14.6]. A capture setting known
/// accumulated rads and reading x at each step would settle the scale/shape.
/// Outside the recorded range, hold its endpoint [design].
pub(super) fn radiation_pointer_x(rads: f64) -> f64 {
    let rads = if rads.is_finite() { rads.max(0.0) } else { 0.0 };
    (15.0 + rads).min(42.9)
}

fn write_marks(ui: &mut Ui, marks: &[CompassMark]) {
    let window = "HitPoints/compass_window";
    // Reuse slots, hiding the surplus [design]. Ten NPC slots were captured;
    // overflow slots and the other pools grow only as needed [hypothesis: hud.md §14.6].
    for (group, template, name, kinds) in [
        ("compass_npc_icon_group", "compass_npc_icon", "compass_npc_icon", &[MarkKind::Actor, MarkKind::Hostile][..]),
        ("compass_icon_group:0", "template_compass_icon_player", "compass_icon", &[MarkKind::Player][..]),
        ("compass_icon_group:0", "template_compass_icon_quest", "compass_icon", &[MarkKind::Quest][..]),
        (
            "compass_icon_group:1",
            "template_compass_icon_marker",
            "compass_icon",
            &[MarkKind::Place, MarkKind::Found][..],
        ),
    ] {
        let path = format!("HUDMainMenu/{window}/{group}");
        let Some(parent) = find(ui, &format!("{window}/{group}")) else {
            continue;
        };
        let mut slots: Vec<_> = ui
            .tile(parent)
            .children
            .iter()
            .copied()
            .filter(|id| ui.tile(*id).get("_NVFHudTemplate").is_some_and(|v| v.text() == template))
            .collect();
        let incoming: Vec<_> = marks.iter().filter(|m| kinds.contains(&m.kind)).collect();
        while slots.len() < incoming.len() {
            let Some(tile) = code_copy(ui, &path, template, name) else {
                break;
            };
            slots.push(tile);
        }
        for (i, tile) in slots.into_iter().enumerate() {
            let Some(mark) = incoming.get(i) else {
                num(ui, tile, "visible", 0.0);
                continue;
            };
            // x == _Heading, tick distance 1 [observed: hud.md §14.4].
            num(ui, tile, "_Heading", mark.heading);
            num(ui, tile, "x", mark.heading);
            num(
                ui,
                tile,
                "_Distance",
                if matches!(mark.kind, MarkKind::Actor | MarkKind::Hostile) { 1.0 } else { mark.distance },
            );
            // 139 either side of x 139; quest/player band [hypothesis: hud.md §14.5].
            num(ui, tile, "visible", f64::from(mark.heading.is_finite() && (0.0..=278.0).contains(&mark.heading)));
        }
    }
}
