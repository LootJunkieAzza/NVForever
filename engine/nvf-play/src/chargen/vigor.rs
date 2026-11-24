//! The Vigor Tester's S.P.E.C.I.A.L. menu (menu 1074, `ShowLoveTesterMenuParams`), drawn
//! as the original: the machine's 3D model fills the screen in front of the player's
//! camera, its flip boards turn between pages by the model's own sequences, its bulbs,
//! digits and arrows show the sheet (docs/spec-notes/vigor-tester.md).
//!
//! - The model: `nv_vitomaticvigortester_cabinet.nif` (the cabinet and the bulbs with
//!   their glows) with `nv_vitomaticvigortester_activate.nif` (the label boards, the
//!   posters, the lever, the buttons and the 16 sequences) in one frame [observed: the
//!   two files share it; the placed activator's own model is `…_cabinet02.nif`, with a
//!   fixed poster]. Lit, on a render layer of its own ([`VIGOR_LAYER`]), by a camera and
//!   lights of its own (nvforever `vigor_view.rs`), so the menu looks the same in a dark
//!   room and at noon and the room's lighting does not change; each part's colour is
//!   tinted by `model::shade`. It stays below the main camera, whose field of view is the
//!   original menu's while the menu is open and is given back when it closes ([`view`]);
//!   the view's camera takes that field of view, and the room behind is blurred and
//!   darkened there.
//! - Pages ([`model::Page`]): Attract (the model's stored pose), the seven attributes,
//!   Review. A turn plays the sequence of [`model::sequence`] (about 1.3 s).
//! - Keys [hypothesis: the original's PC keys are not documented; the menu file binds the
//!   pad's left/right to increase/decrease, up/down to the row, RT/LT to the pages and
//!   A/X to exit]: Left/Right or A/D lower/raise the page's attribute (on the review page
//!   the chosen row's), Up/Down or W/S choose the row, Tab or Page Down the next page,
//!   Shift+Tab or Page Up the previous one, Enter or E Done; the → button on the review
//!   page is Done too. A controller's D-pad and left stick are the arrow keys
//!   (`crate::pad::menu_stick`, #2313). Clicks on the model's buttons, found by their outlines on the
//!   screen.
//! - Done only with every point given out (`SpecialSheet::complete`); then the sheet is
//!   written to the player and the menu answered (`chargen::finish`).
//!
//! Test aid `NVF_PLAY_TEST_VIGOR=<seconds>:<op>[,…]` (seconds after the menu opened;
//! steps wait for it to open): `raise[:<attribute>]`, `lower[:<attribute>]`,
//! `select:<attribute>`, `page:next`, `page:prev`, `page:<page>` (turning through the
//! pages between), `done`, `click:<shape>` (a click at the middle of the shape's outline
//! on the screen, e.g. `click:P1_RT_Btn:0`). Attributes by number 0–6, name or letter;
//! pages by name (`attract`, `review`, an attribute) or number 0–8.
//!
//! - **Made ready behind the start's loading screen** in a new game ([`warm`], #2406):
//!   the two models are read, parsed and spawned (hidden, 508 parts, 16 textures) and the
//!   sequences read while the screen that brings the player into Doc Mitchell's house is
//!   up, and the first opening takes them. Built in the frame the menu opened, they made
//!   it 79 ms on the bench machine, 104 ms with the driver's shader cache empty and
//!   319 ms with other programs on 60% of the processor [observed: profile campaign
//!   `new-game-chargen` runs, 2026-11-15, docs/design/streaming-hitches.md §6]. What was
//!   made ready and not used is dropped at the next travel's loading screen.

pub mod model;
pub mod view;

use self::model::{Action, AidOp, Outcome, Page, Vigor, STATS};
use crate::pause::AidTime;
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::ui_sounds::{UiEvent, UiMenu, UiSound};
use crate::{PlayerCamera, PlaySet};
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use fnv_game::chargen::SpecialSheet;
use fnv_game::game::menus;
use nvf_assets::anim_play::{Clip, Playing};
use nvf_assets::{GameData, NifMaterial, SpawnCtx};
use std::collections::HashMap;
use std::sync::Arc;

const CABINET: &str = "meshes\\architecture\\goodsprings\\nv_vitomaticvigortester_cabinet.nif";
const ACTIVATE: &str = "meshes\\architecture\\goodsprings\\nv_vitomaticvigortester_activate.nif";

/// The render layer of the machine, its lights and its camera (nvforever `vigor_view.rs`);
/// no other view has it, so the machine takes no light from the room and gives it none.
pub const VIGOR_LAYER: usize = 14;

pub struct VigorPlugin;

impl Plugin for VigorPlugin {
    fn build(&self, app: &mut App) {
        let aid = match std::env::var("NVF_PLAY_TEST_VIGOR").map(|v| model::parse_aid(&v)) {
            Ok(Ok(a)) => a,
            Ok(Err(e)) => {
                warn!("NVF_PLAY_TEST_VIGOR: {e}");
                Vec::new()
            }
            Err(_) => Vec::new(),
        };
        app.init_resource::<VigorMenu>()
            .init_resource::<VigorStats>()
            .insert_resource(VigorAid(aid))
            .add_systems(
                Update,
                (warm, open_close, input).chain().in_set(PlaySet::Scripts).before(crate::script::run_scripts).run_if(resource_exists::<GameData>),
            )
            .add_systems(Update, draw.in_set(PlaySet::Ui).run_if(resource_exists::<GameData>))
            .add_systems(Last, report);
    }
}

/// The open menu, if any.
#[derive(Resource, Default)]
pub struct VigorMenu {
    open: Option<OpenMenu>,
    /// The activate model's sequences by name, read on the first opening (or by [`warm`]).
    clips: Option<Arc<HashMap<String, Clip>>>,
    /// The machine spawned hidden ahead of its first opening ([`warm`]).
    ready: Option<Machine>,
    /// [`warm`] has run in this run (once at most).
    warmed: bool,
    /// [design] The current `LOVE_TESTER` opening already failed to draw the 3D machine,
    /// so later frames while the menu stays open do not retry or repeat the warning.
    open_failed: bool,
}

/// The two models, spawned and hidden: their roots and named objects (the activate
/// model's first).
struct Machine {
    roots: [Entity; 2],
    nodes: HashMap<String, Entity>,
}

impl VigorMenu {
    /// [design, #2403 / file refs] Page, review row and attribute allocation;
    /// excludes animation playback, textures and aid clocks in `OpenMenu`.
    pub fn progress(&self) -> Option<&Vigor> {
        self.open.as_ref().map(|o| &o.vigor)
    }

    /// [design, #2403] Synthetic logical transitions using the machine's own actions.
    #[cfg(feature = "report-test-support")]
    #[doc(hidden)]
    pub fn report_progress_fixture(step: usize) -> Self {
        let mut vigor = Vigor::new(SpecialSheet::new([5; 7]));
        if step >= 1 {
            vigor.act(Action::Next);
        }
        if step >= 2 {
            vigor.act(Action::Raise(None));
        }
        if step >= 3 {
            vigor.page = Page::Review;
            vigor.act(Action::Select(1));
        }
        Self {
            open: Some(OpenMenu {
                vigor,
                roots: [Entity::PLACEHOLDER; 2],
                nodes: HashMap::new(),
                flip: None,
                materials: HashMap::new(),
                shown: HashMap::new(),
                keyed_row: step >= 4,
                fov_before: None,
                aid_from: Some(step as f32),
            }),
            clips: None,
            ready: None,
            warmed: false,
            open_failed: false,
        }
    }
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }
}

struct OpenMenu {
    vigor: Vigor,
    /// The two models' roots (children of the camera).
    roots: [Entity; 2],
    /// Named objects of both models (the activate model's first).
    nodes: HashMap<String, Entity>,
    /// The turn playing, or the last one (held at its end).
    flip: Option<Playing>,
    /// Shapes given their own material, by entity.
    materials: HashMap<Entity, Handle<NifMaterial>>,
    /// The texture each changed shape shows now.
    shown: HashMap<Entity, String>,
    /// Up/Down chose a review row: its arrows show it.
    keyed_row: bool,
    /// The camera's field of view before the menu set the original's.
    fov_before: Option<f32>,
    /// When the test aid's clock started: the first frame the menu could take input.
    aid_from: Option<f32>,
}

#[derive(Resource, Default)]
struct VigorStats {
    opened: u32,
    changes: u32,
    turns: u32,
    clicks: u32,
    done: u32,
}

#[derive(Resource, Default)]
struct VigorAid(Vec<(f32, AidOp)>);

/// The machine's parts below the camera.
#[derive(Component)]
struct VigorPart;

#[allow(clippy::too_many_arguments)]
fn open_close(
    mut menu: ResMut<VigorMenu>,
    mut script: ResMut<ScriptState>,
    mut sim: ResMut<SimState>,
    mut ctx: SpawnCtx,
    mut camera: Query<(Entity, &mut Projection), With<PlayerCamera>>,
    mut stats: ResMut<VigorStats>,
    tweaks: Option<Res<crate::tweaks::TweakSettings>>,
) {
    let code = crate::chargen::open_code(&script);
    if code != Some(menus::LOVE_TESTER) {
        menu.open_failed = false;
        if let Some(o) = menu.open.take() {
            for e in o.roots {
                ctx.commands.entity(e).despawn();
            }
            if let (Some(fov), Ok((_, mut proj))) = (o.fov_before, camera.single_mut()) {
                if let Projection::Perspective(p) = proj.as_mut() {
                    p.fov = fov;
                }
            }
            info!("vigor tester: closed");
        }
        return;
    }
    if menu.open.is_some() || menu.open_failed {
        return;
    }
    let Ok((cam, _)) = camera.single() else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    let sheet = crate::script::with_world(rt, &mut sim, |rt| SpecialSheet::for_menu(&mut rt.game));
    if menu.clips.is_none() {
        menu.clips = Some(Arc::new(read_clips(&ctx.data)));
    }
    let warmed = menu.ready.is_some();
    let Some(Machine { roots: [ra, rc], nodes }) = menu.ready.take().or_else(|| spawn_machine(&mut ctx)) else {
        menu.open_failed = true;
        return;
    };
    if warmed {
        info!("vigor tester: the machine made ready behind the loading screen is used");
    }
    for r in [ra, rc] {
        ctx.commands.entity(cam).add_child(r);
    }
    let mut vigor = Vigor::new(sheet);
    // The Attract page: the stored pose, as the end of the turn back to it holds it.
    // The player's tweak opens on the review page instead, in the end pose of the turn
    // to it [design: docs/design/tweaks.md].
    let clips = menu.clips.clone().unwrap_or_default();
    let mut pose = model::ATTRACT_POSE;
    if tweaks.is_some_and(|t| t.vigor_review_first) {
        if let Some(seq) = model::sequence(Page::Stat(6), Page::Review).filter(|s| clips.contains_key(*s)) {
            vigor.page = Page::Review;
            pose = seq;
            info!("vigor tester: opens on the review page (tweak)");
        }
    }
    let flip = clips.get(pose).map(|c| Playing::new(ACTIVATE, c, |n| nodes.get(n).copied()).at_phase(1.0));
    info!(
        "vigor tester: open, S.P.E.C.I.A.L. {}, {} points remaining ({} sequences, {} parts)",
        vigor.values(),
        vigor.sheet.remaining(),
        clips.len(),
        nodes.len()
    );
    stats.opened += 1;
    menu.open = Some(OpenMenu { vigor, roots: [ra, rc], nodes, flip, materials: HashMap::new(), shown: HashMap::new(), keyed_row: false, fov_before: None, aid_from: None });
}

/// [design] Whether both Vigor Tester NIFs exist in the mounted VFS.
fn models_present(data: &GameData) -> bool {
    data.vfs.exists(ACTIVATE) && data.vfs.exists(CABINET)
}

/// Spawn the two models, hidden until `draw` has put the shapes on the machine's layer
/// (or the main camera would draw them for a frame in the room's light), not yet under
/// the camera.
///
/// [design] Checks VFS presence before spawning and despawns any partial root if the
/// second model fails, so a missing or broken NIF never leaks an orphan entity tree.
fn spawn_machine(ctx: &mut SpawnCtx) -> Option<Machine> {
    let missing_act = (!ctx.data.vfs.exists(ACTIVATE)).then(|| format!("{ACTIVATE} not found"));
    let missing_cab = (!ctx.data.vfs.exists(CABINET)).then(|| format!("{CABINET} not found"));
    if missing_act.is_some() || missing_cab.is_some() {
        warn!("vigor tester: model not drawn: {:?} {:?}", missing_act, missing_cab);
        return None;
    }
    let act = match ctx.spawn_nif(ACTIVATE, None) {
        Ok(a) => a,
        Err(e) => {
            warn!("vigor tester: model not drawn: {:?} {:?}", Some(e), Option::<String>::None);
            return None;
        }
    };
    let cab = match ctx.spawn_nif(CABINET, None) {
        Ok(c) => c,
        Err(e) => {
            if let Some(ra) = act.root {
                ctx.commands.entity(ra).despawn();
            }
            warn!("vigor tester: model not drawn: {:?} {:?}", Option::<String>::None, Some(e));
            return None;
        }
    };
    let (Some(ra), Some(rc)) = (act.root, cab.root) else {
        if let Some(ra) = act.root {
            ctx.commands.entity(ra).despawn();
        }
        if let Some(rc) = cab.root {
            ctx.commands.entity(rc).despawn();
        }
        return None;
    };
    let mut nodes = act.nodes;
    for (k, v) in cab.nodes {
        nodes.entry(k).or_insert(v);
    }
    for r in [ra, rc] {
        ctx.commands.entity(r).insert((VigorPart, Visibility::Hidden, view::model_in_camera(1.0)));
    }
    Some(Machine { roots: [ra, rc], nodes })
}

/// A new game's opening has the Vigor Tester: behind the start's loading screen, read its
/// sequences and spawn the machine hidden, so its first opening builds nothing (module
/// notes, #2406). Once a run; what is still unused at the next travel's loading screen
/// is dropped.
fn warm(
    mut menu: ResMut<VigorMenu>,
    mut ctx: SpawnCtx,
    loading: Option<Res<crate::loading::Loading>>,
    new_game: Option<Res<crate::script::NewGame>>,
) {
    let Some(loading) = loading else { return };
    if loading.travelling() {
        if let Some(m) = menu.ready.take() {
            for e in m.roots {
                ctx.commands.entity(e).despawn();
            }
            info!("vigor tester: the machine made ready was not used here; dropped");
        }
        return;
    }
    if menu.warmed || new_game.is_none() || !loading.starting() {
        return;
    }
    menu.warmed = true;
    // [design] Skip speculative pre-warming quietly when either Vigor Tester NIF is absent
    // from the mounted archives/loose files (e.g., FO3/TTW start or modded asset layout):
    // avoids emitting a warning or spawning partial entities when the menu is never opened.
    if !models_present(&ctx.data) {
        debug!("vigor tester: skipping pre-warm; model asset not present in data");
        return;
    }
    let t = std::time::Instant::now();
    if menu.clips.is_none() {
        menu.clips = Some(Arc::new(read_clips(&ctx.data)));
    }
    menu.ready = spawn_machine(&mut ctx);
    if let Some(m) = &menu.ready {
        info!("vigor tester: made ready behind the loading screen in {:.0} ms ({} parts)", t.elapsed().as_secs_f64() * 1000.0, m.nodes.len());
    }
}

/// The machine's clips by name. A broken file (a panic reading it) gives none, and a clip
/// that cannot be played ([`Clip::check`]) is left out; each is logged.
fn read_clips(data: &GameData) -> HashMap<String, Clip> {
    let Some(bytes) = data.read(ACTIVATE) else { return HashMap::new() };
    let read = nvf_assets::guard::guarded(|| {
        let Ok(f) = fnv_nif::NifFile::parse(&bytes, Some(&data.schema)) else { return Ok(Vec::new()) };
        let dec = fnv_nif::scene::Decoded::new(&f, &bytes, &data.schema);
        Ok(fnv_nif::anim::sequences(&f, &dec).iter().map(Clip::from_sequence).collect())
    });
    let clips: Vec<Clip> = read.unwrap_or_else(|e| {
        warn!("vigor tester: {ACTIVATE} not read: {e}");
        Vec::new()
    });
    clips
        .into_iter()
        .filter(|c| c.check().map_err(|e| warn!("vigor tester: clip {} not played: {e}", c.name)).is_ok())
        .map(|c| (c.name.clone(), c))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn input(
    keys: crate::pad::ActionButtons<KeyCode>,
    mouse: crate::pad::ActionButtons<MouseButton>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    shapes: Query<(&Aabb, &GlobalTransform, &InheritedVisibility)>,
    time: AidTime,
    mut aid: ResMut<VigorAid>,
    mut menu: ResMut<VigorMenu>,
    mut script: ResMut<ScriptState>,
    mut sim: ResMut<SimState>,
    mut stats: ResMut<VigorStats>,
    mut sounds: MessageWriter<UiSound>,
) {
    let keys = keys.snapshot();
    let mouse = mouse.snapshot();
    use KeyCode::*;
    let clips = menu.clips.clone().unwrap_or_default();
    let Some(o) = menu.open.as_mut() else { return };
    let mut acts: Vec<(Action, &'static str)> = Vec::new();
    let shift = keys.any_pressed([ShiftLeft, ShiftRight]);
    if keys.any_just_pressed([ArrowLeft, KeyA]) {
        acts.push((Action::Lower(None), "key"));
    }
    if keys.any_just_pressed([ArrowRight, KeyD]) {
        acts.push((Action::Raise(None), "key"));
    }
    if keys.any_just_pressed([ArrowUp, KeyW]) {
        acts.push((Action::Step(-1), "key"));
    }
    if keys.any_just_pressed([ArrowDown, KeyS]) {
        acts.push((Action::Step(1), "key"));
    }
    if keys.just_pressed(PageDown) || (keys.just_pressed(Tab) && !shift) {
        acts.push((Action::Next, "key"));
    }
    if keys.just_pressed(PageUp) || (keys.just_pressed(Tab) && shift) {
        acts.push((Action::Prev, "key"));
    }
    if keys.any_just_pressed([Enter, NumpadEnter, KeyE]) {
        acts.push((Action::Done, "key"));
    }
    if acts.iter().any(|(a, _)| matches!(a, Action::Step(_))) {
        o.keyed_row = true;
    }
    let cam = camera.single().ok();
    let pick = |at: Vec2, o: &OpenMenu| -> Option<Action> {
        let (camera, cam_t) = cam?;
        let mut best: Option<(f32, Action)> = None;
        for name in model::live_buttons(o.vigor.page) {
            let Some(&e) = o.nodes.get(&name) else { continue };
            let Some(rect) = screen_rect(e, camera, cam_t, &shapes) else { continue };
            let area = rect.size().x * rect.size().y;
            if rect.contains(at) && best.is_none_or(|(a, _)| area < a) {
                if let Some(a) = model::button_action(&name, o.vigor.page) {
                    best = Some((area, a));
                }
            }
        }
        best.map(|(_, a)| a)
    };
    if mouse.just_pressed(MouseButton::Left) {
        if let Some(a) = windows.single().ok().and_then(|w| w.cursor_position()).and_then(|at| pick(at, o)) {
            acts.push((a, "click"));
            stats.clicks += 1;
        }
    }
    // The test aid's steps due now.
    // Its seconds count from the menu's first frame of input, so that a slow start
    // (cells loading) cannot run every step in the frame the model appears.
    let now = time.elapsed_secs();
    let since = now - *o.aid_from.get_or_insert(now);
    let mut due: Vec<(f32, AidOp)> = aid.0.iter().filter(|(t, _)| *t <= since).cloned().collect();
    due.sort_by(|a, b| a.0.total_cmp(&b.0));
    aid.0.retain(|(t, _)| *t > since);
    for (_, step) in due {
        info!("test aid NVF_PLAY_TEST_VIGOR: {step:?}");
        match step {
            AidOp::Act(a) => acts.push((a, "test aid")),
            AidOp::Page(p) => {
                let (from, to) = (o.vigor.page.index(), p.index());
                let a = if to > from { Action::Next } else { Action::Prev };
                for _ in 0..from.abs_diff(to) {
                    acts.push((a, "test aid"));
                }
            }
            AidOp::Click(name) => {
                let at = cam.and_then(|(c, ct)| o.nodes.get(&name).and_then(|&e| screen_rect(e, c, ct, &shapes))).map(|r| r.center());
                match at.and_then(|at| pick(at, o).map(|a| (at, a))) {
                    Some((at, a)) => {
                        info!("vigor tester: click at ({:.0}, {:.0}) on {name}", at.x, at.y);
                        acts.push((a, "click"));
                        stats.clicks += 1;
                    }
                    None => info!("vigor tester: click on {name}: nothing there to press"),
                }
            }
        }
    }
    let mut sound = |event| {
        sounds.write(UiSound { menu: UiMenu::Vigor, event });
    };
    for (a, how) in acts {
        match o.vigor.act(a) {
            Outcome::Changed { stat, value, remaining } => {
                sound(UiEvent::Value { up: matches!(a, Action::Raise(_)) });
                stats.changes += 1;
                info!("vigor tester: {} {value}, {remaining} points remaining ({how})", STATS[stat]);
            }
            Outcome::Turned { from, to } => {
                sound(UiEvent::Switch);
                stats.turns += 1;
                let seq = model::sequence(from, to).unwrap_or("?");
                info!("vigor tester: page {} (sequence {seq}, {how})", to.name());
                match clips.get(seq) {
                    Some(c) => o.flip = Some(Playing::new(ACTIVATE, c, |n| o.nodes.get(n).copied())),
                    None => warn!("vigor tester: no sequence {seq}: the boards turn at once"),
                }
            }
            Outcome::Selected(i) => {
                sound(UiEvent::Focus);
                info!("vigor tester: row {} chosen ({how})", STATS[i]);
            }
            Outcome::Refused(why) => {
                sound(UiEvent::Refused);
                info!("vigor tester: {a:?} refused: {why} ({how})");
            }
            Outcome::Done => {
                let sheet = o.vigor.sheet;
                let values = o.vigor.values();
                if crate::chargen::finish(&mut script, &mut sim, menus::LOVE_TESTER, how, |g| sheet.apply(g)) {
                    sound(UiEvent::Accepted);
                    stats.done += 1;
                    info!("vigor tester: done, S.P.E.C.I.A.L. {values} ({how})");
                }
                break;
            }
        }
    }
}

/// A shape's outline on the screen (logical pixels), from its bounds; `None` while it is
/// hidden or behind the camera.
fn screen_rect(e: Entity, camera: &Camera, cam_t: &GlobalTransform, shapes: &Query<(&Aabb, &GlobalTransform, &InheritedVisibility)>) -> Option<Rect> {
    let (aabb, t, vis) = shapes.get(e).ok()?;
    if !vis.get() {
        return None;
    }
    let (c, h) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
    let mut rect: Option<Rect> = None;
    for k in 0..8 {
        let s = Vec3::new(if k & 1 == 0 { -1.0 } else { 1.0 }, if k & 2 == 0 { -1.0 } else { 1.0 }, if k & 4 == 0 { -1.0 } else { 1.0 });
        let p = camera.world_to_viewport(cam_t, t.transform_point(c + h * s)).ok()?;
        rect = Some(rect.map_or(Rect::from_corners(p, p), |r| r.union_point(p)));
    }
    rect
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut menu: ResMut<VigorMenu>,
    time: AidTime,
    mut camera: Query<&mut Projection, With<PlayerCamera>>,
    children: Query<&Children>,
    meshes: Query<&MeshMaterial3d<NifMaterial>>,
    names: Query<&Name>,
    mut transforms: Query<&mut Transform>,
    mut visibility: Query<&mut Visibility>,
    mut ctx: SpawnCtx,
    talk: Option<Res<crate::dialogue_zoom::DialogueZoom>>,
) {
    let Some(o) = menu.open.as_mut() else { return };
    // The same place in front of the camera every frame.
    // The original's field of view while the menu is open (the world behind is only a
    // background), given back when it closes.
    let vfov = match camera.single_mut().as_deref_mut() {
        Ok(Projection::Perspective(p)) => {
            let fit = view::FIT_VFOV_DEG.to_radians();
            if o.fov_before.is_none() {
                // Not a conversation's zoom: what lies under it (`crate::dialogue_zoom`).
                o.fov_before = Some(talk.as_deref().map_or(p.fov, |t| t.underlying(p.fov)));
            }
            if p.fov != fit {
                p.fov = fit;
            }
            fit
        }
        _ => view::FIT_VFOV_DEG.to_radians(),
    };
    let place = view::model_in_camera(vfov);
    for r in o.roots {
        if let Ok(mut t) = transforms.get_mut(r) {
            t.set_if_neq(place);
        }
    }
    // Once the shapes exist: each its own material and tint, on the machine's layer,
    // casting no shadow. The model says which shapes are unlit; the glows always are.
    if o.materials.is_empty() {
        for r in o.roots {
            for e in children.iter_descendants(r) {
                let Ok(m) = meshes.get(e) else { continue };
                let Some(mut mat) = ctx.materials.get(&m.0).cloned() else { continue };
                let name = names.get(e).map(|n| n.as_str()).unwrap_or("");
                mat.base.unlit |= model::is_glow(name);
                if mat.base.unlit {
                    // An unlit copy takes no authored set (#2981).
                    nvf_assets::authored::make_legacy(&mut mat);
                }
                let [r, g, b] = model::shade(name, r == o.roots[1]);
                let c = mat.base.base_color.to_srgba();
                mat.base.base_color = Color::srgba(c.red * r, c.green * g, c.blue * b, c.alpha);
                mat.base.fog_enabled = false;
                let h = ctx.materials.add(mat);
                ctx.commands.entity(e).insert((MeshMaterial3d(h.clone()), NotShadowCaster, RenderLayers::layer(VIGOR_LAYER)));
                o.materials.insert(e, h);
            }
        }
        if !o.materials.is_empty() {
            debug!("vigor tester: {} shapes drawn", o.materials.len());
            for r in o.roots {
                if let Ok(mut vis) = visibility.get_mut(r) {
                    vis.set_if_neq(Visibility::Inherited);
                }
            }
        }
    }
    if let Some(f) = o.flip.as_mut() {
        f.step(time.delta_secs_f64() as f32, false, &mut transforms);
    }
    let v = &o.vigor;
    // Bulbs.
    let lit = model::lit_bulbs(v.page, &v.sheet);
    for n in 1..=10 {
        for s in model::glow_shapes(n) {
            if let Some(&e) = o.nodes.get(&s) {
                if let Ok(mut vis) = visibility.get_mut(e) {
                    vis.set_if_neq(if n <= lit { Visibility::Inherited } else { Visibility::Hidden });
                }
            }
        }
    }
    // The review page's arrows and values, the points remaining, the arrow buttons.
    let mut want: Vec<(String, String)> = Vec::new();
    for (i, s) in STATS.iter().enumerate() {
        let (down, up) = model::review_arrows(&v.sheet, i);
        let chosen = o.keyed_row && v.row == i && v.page == Page::Review;
        for (shape, shown, raise) in [(format!("Index_{s}Decrease_Btn:0"), down, false), (format!("Index_{s}Increase_Btn:0"), up, true)] {
            if let Some(&e) = o.nodes.get(&shape) {
                if let Ok(mut vis) = visibility.get_mut(e) {
                    vis.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
                }
            }
            want.push((shape, model::press_texture(raise, chosen).to_string()));
        }
        want.push((format!("Index_{s}PointVal:0"), model::number_texture(v.sheet.values[i])));
    }
    let (tens, ones) = model::two_digits(v.sheet.remaining());
    want.push((model::TENS_SHAPE.into(), model::number_texture(tens)));
    want.push((model::ONES_SHAPE.into(), model::number_texture(ones)));
    want.push(("P1_RT_Btn:0".into(), model::rt_texture(model::next_enabled(v.page, &v.sheet)).into()));
    want.push(("LookInside_Btn:0".into(), model::rt_texture(true).into()));
    want.push(("P1_LT_Btn:0".into(), model::LT_TEXTURE.into()));
    for (shape, tex) in want {
        let Some(&e) = o.nodes.get(&shape) else { continue };
        let Some(h) = o.materials.get(&e) else { continue };
        if o.shown.get(&e) == Some(&tex) {
            continue;
        }
        let img = ctx.textures.get(&tex, true, &ctx.data, &mut ctx.images, ctx.formats.as_deref());
        if let (Some(img), Some(mut m)) = (img, ctx.materials.get_mut(h)) {
            m.base.base_color_texture = Some(img);
            // The new picture's authored set, or the legacy look (#2981).
            nvf_assets::authored::retexture(&mut m, &tex, &mut ctx.textures, &ctx.data, &mut ctx.images, ctx.formats.as_deref());
        }
        o.shown.insert(e, tex);
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<VigorStats>) {
    if exit.read().next().is_none() || stats.opened == 0 {
        return;
    }
    info!(
        "vigor tester summary: opened {} times, {} changes, {} page turns, {} clicks on its buttons, done {} times",
        stats.opened, stats.changes, stats.turns, stats.clicks, stats.done
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loading::{Loading, LoadingPlugin, StartCover};
    use crate::script::NewGame;
    use crate::{Location, PlayState, Travel};
    use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
    use nvf_assets::{NifRoot, TextureCache};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("nvf-vigor-test-{}-{}-{id}", std::process::id(), tag));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Build a minimal valid NIF 20.2.0.7 containing a single named `NiNode`.
    fn minimal_nif(node_name: &str) -> Vec<u8> {
        let words = |out: &mut Vec<u8>, xs: &[u32]| {
            for &x in xs {
                out.extend(x.to_le_bytes());
            }
        };
        let string = |out: &mut Vec<u8>, s: &str| {
            words(out, &[s.len() as u32]);
            out.extend(s.as_bytes());
        };
        // NiNode (BS version 34): name_index (0), num_extra_data (0), controller (-1), flags (14u32),
        // translation (0,0,0), rotation (identity 3x3), scale (1.0),
        // num_properties (0), collision (-1), num_children (0), num_effects (0).
        let mut node = Vec::new();
        words(&mut node, &[0, 0, u32::MAX, 14]);
        for f in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0] {
            node.extend(f.to_le_bytes());
        }
        words(&mut node, &[0, u32::MAX, 0, 0]);

        let mut out = b"Gamebryo File Format, Version 20.2.0.7\n".to_vec();
        words(&mut out, &[0x1402_0007]);
        out.push(1);
        words(&mut out, &[11, 1, 34]);
        out.extend([1, 0, 1, 0, 1, 0]);
        out.extend(1u16.to_le_bytes());
        string(&mut out, "NiNode");
        out.extend(0u16.to_le_bytes());
        words(&mut out, &[node.len() as u32]);
        words(&mut out, &[1, node_name.len() as u32]);
        string(&mut out, node_name);
        words(&mut out, &[0]);
        out.extend(&node);
        words(&mut out, &[1, 0]);
        out
    }

    fn vigor_test_app(tag: &str, files: &[(&str, Vec<u8>)], with_new_game_loading: bool) -> (App, TempDir, Entity) {
        let dir = TempDir::new(tag);
        for (rel, bytes) in files {
            let path = dir.0.join(rel.replace('\\', "/"));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        let schema_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/nif");
        let vfs = fnv_vfs::Vfs::open(&dir.0, &[]).unwrap();
        let schema = fnv_nif::Schema::load_dir(&schema_dir).unwrap();
        let data = GameData::new(Arc::new(vfs), Arc::new(schema));

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_resource(Location { world: 0xDA726, interior: None })
            .init_state::<PlayState>()
            .add_message::<Travel>()
            .configure_sets(Update, (PlaySet::Input, PlaySet::Scripts, PlaySet::Sync).chain())
            .add_plugins(LoadingPlugin)
            .insert_resource(data)
            .insert_resource(ScriptState::over_no_plugins())
            .insert_resource(SimState::over_placed([]))
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<NifMaterial>>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<SkinnedMeshInverseBindposes>>()
            .init_resource::<TextureCache>()
            .init_resource::<VigorMenu>()
            .init_resource::<VigorStats>()
            .add_systems(Update, (warm, open_close).chain().in_set(PlaySet::Scripts));
        if with_new_game_loading {
            app.insert_resource(StartCover).insert_resource(NewGame);
        }
        let cam = app.world_mut().spawn((PlayerCamera, Projection::Perspective(PerspectiveProjection::default()))).id();
        (app, dir, cam)
    }

    fn set_open_menu(app: &mut App, code: Option<u16>) {
        let mut script = app.world_mut().resource_mut::<ScriptState>();
        script.rt.as_mut().unwrap().game.menu = code;
    }

    #[test]
    fn present_nifs_prewarm_behind_loading_screen_and_open_vanilla_machine() {
        let files = [
            (ACTIVATE, minimal_nif("ActivateNode")),
            (CABINET, minimal_nif("CabinetNode")),
        ];
        let (mut app, _dir, cam) = vigor_test_app("present", &files, true);

        // First update brings StartCover up (Loading::starting() becomes true); second runs warm.
        app.update();
        assert!(app.world().resource::<Loading>().starting(), "start loading screen is active");
        app.update();

        {
            let menu = app.world().resource::<VigorMenu>();
            assert!(menu.warmed, "warm ran once behind the loading screen");
            assert!(!menu.open_failed);
            let ready = menu.ready.as_ref().expect("both models pre-warmed behind loading screen");
            assert!(ready.nodes.contains_key("ActivateNode"));
            assert!(ready.nodes.contains_key("CabinetNode"));
            assert!(menu.clips.is_some());
        }
        assert_eq!(app.world_mut().query_filtered::<Entity, With<VigorPart>>().iter(app.world()).count(), 2);
        for vis in app.world_mut().query_filtered::<&Visibility, With<VigorPart>>().iter(app.world()) {
            assert_eq!(*vis, Visibility::Hidden, "pre-warmed parts stay hidden until opened");
        }

        // Opening LOVE_TESTER takes the pre-warmed machine and attaches both roots to PlayerCamera.
        set_open_menu(&mut app, Some(menus::LOVE_TESTER));
        app.update();

        {
            let menu = app.world().resource::<VigorMenu>();
            assert!(menu.is_open());
            assert!(menu.ready.is_none(), "warmed machine consumed on open");
            assert!(!menu.open_failed);
        }
        assert_eq!(app.world().resource::<VigorStats>().opened, 1);
        let children = app.world().get::<Children>(cam).expect("roots parented to PlayerCamera");
        assert_eq!(children.len(), 2);

        // Closing LOVE_TESTER despawns both roots cleanly.
        set_open_menu(&mut app, None);
        app.update();
        assert!(!app.world().resource::<VigorMenu>().is_open());
        assert_eq!(app.world_mut().query_filtered::<Entity, With<VigorPart>>().iter(app.world()).count(), 0);
    }

    #[test]
    fn missing_activate_nif_skips_prewarm_and_guards_open_without_spam_or_leaks() {
        // Only the cabinet NIF is present; the activate NIF is missing (e.g., FO3/TTW or modded layout).
        let files = [(CABINET, minimal_nif("CabinetNode"))];
        let (mut app, _dir, _cam) = vigor_test_app("missing-activate", &files, true);

        app.update();
        assert!(app.world().resource::<Loading>().starting());
        for _ in 0..3 {
            app.update();
        }

        {
            let menu = app.world().resource::<VigorMenu>();
            assert!(menu.warmed, "warm marked complete");
            assert!(menu.ready.is_none(), "no partial machine stored");
            assert!(menu.clips.is_none(), "clips not read when activate NIF is absent");
            assert!(!menu.open_failed);
        }
        assert_eq!(app.world_mut().query_filtered::<Entity, With<NifRoot>>().iter(app.world()).count(), 0, "cabinet NIF must not be spawned/leaked when activate NIF is missing");
        assert_eq!(app.world_mut().query_filtered::<Entity, With<VigorPart>>().iter(app.world()).count(), 0);

        // Even if LOVE_TESTER is opened for multiple frames while the activate NIF is missing,
        // open_close records open_failed on the first frame and does not retry or leak entities.
        set_open_menu(&mut app, Some(menus::LOVE_TESTER));
        for _ in 0..7 {
            app.update();
            let menu = app.world().resource::<VigorMenu>();
            assert!(!menu.is_open());
            assert!(menu.open_failed, "failure latched so later frames do not retry");
        }
        assert_eq!(app.world().resource::<VigorStats>().opened, 0);
        assert_eq!(app.world_mut().query_filtered::<Entity, With<NifRoot>>().iter(app.world()).count(), 0);
        assert_eq!(app.world_mut().query_filtered::<Entity, With<VigorPart>>().iter(app.world()).count(), 0);

        // Closing LOVE_TESTER clears open_failed for any later opening.
        set_open_menu(&mut app, None);
        app.update();
        assert!(!app.world().resource::<VigorMenu>().open_failed);
    }

    #[test]
    fn corrupt_cabinet_nif_cleans_up_spawned_activate_root_and_latches_failure() {
        let files = [
            (ACTIVATE, minimal_nif("ActivateNode")),
            (CABINET, b"not a valid nif".to_vec()),
        ];
        let (mut app, _dir, _cam) = vigor_test_app("corrupt-cabinet", &files, false);

        set_open_menu(&mut app, Some(menus::LOVE_TESTER));
        app.update();
        assert!(!app.world().resource::<VigorMenu>().is_open());
        assert!(app.world().resource::<VigorMenu>().open_failed);
        assert_eq!(app.world_mut().query_filtered::<Entity, With<VigorPart>>().iter(app.world()).count(), 0);
        assert!(
            !app.world_mut().query_filtered::<&Name, With<NifRoot>>().iter(app.world()).any(|n| n.as_str() == ACTIVATE),
            "spawned activate root is despawned when cabinet fails"
        );
        let roots_after_first_frame = app.world_mut().query_filtered::<Entity, With<NifRoot>>().iter(app.world()).count();

        // Subsequent frames while LOVE_TESTER remains open must not re-attempt spawn_nif.
        for _ in 0..6 {
            app.update();
        }
        assert_eq!(
            app.world_mut().query_filtered::<Entity, With<NifRoot>>().iter(app.world()).count(),
            roots_after_first_frame,
            "latched open_failed prevents per-frame retries"
        );
    }
}
