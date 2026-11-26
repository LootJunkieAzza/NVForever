//! Off-screen projected-UI painter consuming `fnv_game::ui_projection::Requests`
//! (`ProjectUITile`; Part of #2418, docs/spec-notes/held-ui-projection-state.md,
//! docs/spec-notes/local-map-ui-service.md).
//!
//! Zero-work fast path by default: when no `ProjectUITile` requests are active and no
//! projected targets exist, this module performs zero allocations, zero UI walks, zero
//! camera spawns, and zero image allocations. Ordinary HUD and menu drawing remain
//! completely unchanged [design].
//!
//! Each admitted `(reference, block)` target receives one off-screen `Camera2d` and
//! `RenderTarget::Image` painted through `tiles::draw::paint` (reusing `runtime_images::mark`
//! and `runtime_images::present`, never adding a second generic painter [design]).

use super::draw::{self, Brightness, Drawn, FontKit, Nodes, Target as PaintTarget};
use super::local_map_service::SessionEpoch;
use super::vanilla::TileMenus;
use crate::script::ScriptState;
use bevy::asset::RenderAssetUsages;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use fnv_game::ui_projection::{Revision, Target, Viewport};
use fnv_ui::{TileId, Ui, Value};
use std::collections::{BTreeMap, HashSet};

/// Upper bound on active projected UI render targets, matching `ui_projection::Requests` [design].
pub const MAX_TARGETS: usize = 16;
/// Upper bound on aggregate projected render-target pixels across all active targets [design].
pub const MAX_TOTAL_PIXELS: u64 = 8 * 1024 * 1024;
/// Upper bound on one projected render-target edge in texels [design].
pub const MAX_EDGE: u32 = 2048;
/// Off-screen projected UI camera order: runs before world (`0`) and Pip-Boy (`-2`) cameras
/// so projected textures are ready before 3D model surfaces sample them [design].
pub const PROJECTED_CAMERA_ORDER: isize = -3;

/// Live off-screen render output published for one `(reference, block)` projection target.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedOutput {
    pub image: Handle<Image>,
    pub camera: Entity,
    pub source: TileId,
    pub viewport: Viewport,
    pub revision: Revision,
    pub piece_count: usize,
}

/// Distinct per-target projection status (never collapses budget rejection into an empty success).
#[derive(Clone, Debug, PartialEq)]
pub enum ProjectionDiagnostic {
    Absent,
    Active {
        source: TileId,
        revision: Revision,
        viewport: Viewport,
        piece_count: usize,
    },
    RejectedBudget,
}

/// Counters for verifying idle zero-work no-op behavior, lifecycle cleanup, and budget caps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectionCounters {
    pub frames_ticked: u64,
    pub idle_frames: u64,
    pub targets_created: u64,
    pub targets_resized: u64,
    pub targets_released: u64,
    pub budget_rejections: u64,
}

struct ProjectedSlot {
    output: ProjectedOutput,
    drawn: Drawn,
}

/// Budget configuration for [`ProjectedUiTargets`] [design].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectionBudget {
    pub max_targets: usize,
    pub max_edge: u32,
    pub max_total_pixels: u64,
}

impl Default for ProjectionBudget {
    fn default() -> Self {
        Self {
            max_targets: MAX_TARGETS,
            max_edge: MAX_EDGE,
            max_total_pixels: MAX_TOTAL_PIXELS,
        }
    }
}

/// Engine-owned registry of live off-screen projected UI targets and cameras.
#[derive(Resource)]
pub struct ProjectedUiTargets {
    budget: ProjectionBudget,
    epoch: Option<SessionEpoch>,
    slots: BTreeMap<Target, ProjectedSlot>,
    diagnostics: BTreeMap<Target, ProjectionDiagnostic>,
    counters: ProjectionCounters,
}

impl Default for ProjectedUiTargets {
    fn default() -> Self {
        Self::with_budget(ProjectionBudget::default())
    }
}

impl ProjectedUiTargets {
    pub fn with_budget(budget: ProjectionBudget) -> Self {
        Self {
            budget,
            epoch: None,
            slots: BTreeMap::new(),
            diagnostics: BTreeMap::new(),
            counters: ProjectionCounters::default(),
        }
    }

    pub fn budget(&self) -> ProjectionBudget {
        self.budget
    }

    pub fn counters(&self) -> ProjectionCounters {
        self.counters
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn get(&self, target: &Target) -> Option<&ProjectedOutput> {
        self.slots.get(target).map(|s| &s.output)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Target, &ProjectedOutput)> {
        self.slots.iter().map(|(k, s)| (k, &s.output))
    }

    pub fn diagnostic(&self, target: &Target) -> ProjectionDiagnostic {
        self.diagnostics.get(target).cloned().unwrap_or(ProjectionDiagnostic::Absent)
    }

    /// Test helper: insert a synthetic published output directly so 3D material-binding
    /// controls can exercise the downstream consumer without standing up full UI cameras.
    pub fn insert_synthetic_image_for_test(&mut self, target: Target, image: Handle<Image>, width: u32, height: u32) {
        let mut ui = Ui::new();
        let src = fnv_ui::MemSource::default().with("t.xml", r#"<menu name="T"><rect name="R"/></menu>"#);
        let root = ui.load_menu(&src, "t.xml").expect("synthetic menu");
        ui.update();
        let mut reqs = fnv_game::ui_projection::Requests::default();
        let fnv_game::ui_projection::Accepted::Registered(revision) = reqs
            .set(&ui, target.reference, "T/R", &target.block, [0.0, 0.0, f64::from(width), f64::from(height)])
            .expect("synthetic projection request")
        else {
            unreachable!("non-1x1 request registers");
        };
        let viewport = Viewport { x: 0.0, y: 0.0, width, height };
        let output = ProjectedOutput { image, camera: Entity::PLACEHOLDER, source: root, viewport, revision, piece_count: 1 };
        self.diagnostics.insert(
            target.clone(),
            ProjectionDiagnostic::Active { source: root, revision, viewport, piece_count: 1 },
        );
        self.slots.insert(target, ProjectedSlot { output, drawn: Drawn::default() });
    }

    /// Test helper: remove a synthetic published output.
    pub fn remove_synthetic_output_for_test(&mut self, target: &Target) -> bool {
        self.diagnostics.remove(target);
        self.slots.remove(target).is_some()
    }

    fn clear_all(&mut self, commands: &mut Commands, images: &mut Assets<Image>) {
        let keys: Vec<Target> = self.slots.keys().cloned().collect();
        for key in keys {
            if let Some(mut slot) = self.slots.remove(&key) {
                release_slot(commands, &mut slot, images);
                self.counters.targets_released = self.counters.targets_released.saturating_add(1);
            }
        }
        self.diagnostics.clear();
    }
}

fn release_slot(commands: &mut Commands, slot: &mut ProjectedSlot, images: &mut Assets<Image>) {
    draw::clear(commands, &mut slot.drawn);
    commands.entity(slot.camera()).despawn();
    images.remove(&slot.output.image);
}

impl ProjectedSlot {
    fn camera(&self) -> Entity {
        self.output.camera
    }
}

fn new_target_image(width: u32, height: u32) -> Image {
    let size = Extent3d { width, height, depth_or_array_layers: 1 };
    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    image
}

fn load_subtree_fonts(ui: &Ui, root: TileId, kit: &mut FontKit) {
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let tile = ui.tile(id);
        if !tile.live {
            continue;
        }
        if tile.kind == "text" {
            kit.load(tile.get("font").map_or(0.0, Value::num));
        }
        stack.extend(tile.children.iter().copied());
    }
}

/// Pre-draw reconciliation running after `tiles::update` and before `PlaySet::Ui`.
/// Permanently retires observed stale or index-shadowed projection sources [design].
pub(crate) fn reconcile(mut script: Option<ResMut<ScriptState>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    rt.game.ui_extra.projections.reconcile(&rt.game.ui_tree);
}

/// Paint every admitted `ProjectUITile` subtree into its off-screen `Camera2d` render target
/// through `tiles::draw::paint` (reusing `runtime_images::mark` and `runtime_images::present`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw(
    mut commands: Commands,
    mut targets: ResMut<ProjectedUiTargets>,
    script: Res<ScriptState>,
    menus: Option<Res<TileMenus>>,
    mut kit: FontKit,
    mut nodes: Nodes,
    glyphs: Res<crate::pad::PadGlyphs>,
) {
    let targets = &mut *targets;
    targets.counters.frames_ticked = targets.counters.frames_ticked.saturating_add(1);
    let Some(rt) = script.rt.as_ref() else {
        if !targets.is_empty() {
            targets.clear_all(&mut commands, kit.images_mut());
            targets.epoch = None;
        } else {
            targets.counters.idle_frames = targets.counters.idle_frames.saturating_add(1);
        }
        return;
    };

    let ui = &rt.game.ui_tree;
    let has_requests = rt.game.ui_extra.projections.source_requests(ui).next().is_some();
    if !has_requests && targets.is_empty() {
        targets.diagnostics.clear();
        targets.counters.idle_frames = targets.counters.idle_frames.saturating_add(1);
        return;
    }

    let epoch = SessionEpoch { load_epoch: rt.session_epoch, tree: ui.screen() };
    if targets.epoch != Some(epoch) {
        targets.clear_all(&mut commands, kit.images_mut());
        targets.epoch = Some(epoch);
    }

    let wanted: Vec<(Target, TileId, Viewport, Revision)> = rt
        .game
        .ui_extra
        .projections
        .source_requests(ui)
        .map(|(t, r)| (t.clone(), r.source, r.viewport, r.revision))
        .collect();
    let wanted_keys: HashSet<Target> = wanted.iter().map(|(t, ..)| t.clone()).collect();

    // Release targets whose requests were withdrawn or retired by reconcile.
    let stale_keys: Vec<Target> = targets.slots.keys().filter(|k| !wanted_keys.contains(*k)).cloned().collect();
    for key in stale_keys {
        if let Some(mut slot) = targets.slots.remove(&key) {
            release_slot(&mut commands, &mut slot, kit.images_mut());
            targets.counters.targets_released = targets.counters.targets_released.saturating_add(1);
        }
    }
    targets.diagnostics.clear();

    // Enforce global painter budgets independently (target count, edge cap, total pixels).
    let budget = targets.budget;
    let mut admitted: Vec<(Target, TileId, Viewport, Revision)> = Vec::with_capacity(wanted.len());
    let mut total_pixels: u64 = 0;
    for (target, source, viewport, revision) in wanted {
        let pixels = u64::from(viewport.width).saturating_mul(u64::from(viewport.height));
        let valid_edge = viewport.width >= 1
            && viewport.height >= 1
            && viewport.width <= budget.max_edge
            && viewport.height <= budget.max_edge;
        let fits = valid_edge
            && admitted.len() < budget.max_targets
            && total_pixels.checked_add(pixels).is_some_and(|sum| sum <= budget.max_total_pixels);
        if !fits {
            if let Some(mut slot) = targets.slots.remove(&target) {
                release_slot(&mut commands, &mut slot, kit.images_mut());
                targets.counters.targets_released = targets.counters.targets_released.saturating_add(1);
            }
            targets.counters.budget_rejections = targets.counters.budget_rejections.saturating_add(1);
            targets.diagnostics.insert(target, ProjectionDiagnostic::RejectedBudget);
            continue;
        }
        total_pixels = total_pixels.saturating_add(pixels);
        admitted.push((target, source, viewport, revision));
    }

    let t15 = menus.as_deref().copied().is_some_and(super::interface_screen);

    for (target, source, viewport, revision) in admitted {
        load_subtree_fonts(ui, source, &mut kit);
        let pieces = {
            let measured = |font: f64, text: &str, wrap: Option<f64>| {
                kit.face(font).map_or((0.0, 0.0), |face| draw::measure(&face.font, draw::font_number(font), text, wrap))
            };
            draw::projected_pieces(ui, source, viewport, &measured, t15)
        };
        let piece_count = pieces.len();

        let slot = match targets.slots.get_mut(&target) {
            Some(slot) => {
                if slot.output.viewport.width != viewport.width || slot.output.viewport.height != viewport.height {
                    let old_image = slot.output.image.clone();
                    let new_image = kit.images_mut().add(new_target_image(viewport.width, viewport.height));
                    commands.entity(slot.output.camera).insert(RenderTarget::Image(ImageRenderTarget::from(new_image.clone())));
                    kit.images_mut().remove(&old_image);
                    slot.output.image = new_image;
                    targets.counters.targets_resized = targets.counters.targets_resized.saturating_add(1);
                }
                slot.output.source = source;
                slot.output.viewport = viewport;
                slot.output.revision = revision;
                slot.output.piece_count = piece_count;
                slot
            }
            None => {
                let image = kit.images_mut().add(new_target_image(viewport.width, viewport.height));
                let camera = commands
                    .spawn((
                        Name::new(format!("Projected UI camera {:08X}:{}", target.reference.0, target.block)),
                        Camera2d,
                        Camera {
                            order: PROJECTED_CAMERA_ORDER,
                            is_active: true,
                            clear_color: ClearColorConfig::Custom(Color::NONE),
                            ..default()
                        },
                        RenderTarget::Image(ImageRenderTarget::from(image.clone())),
                    ))
                    .id();
                targets.counters.targets_created = targets.counters.targets_created.saturating_add(1);
                let output = ProjectedOutput { image, camera, source, viewport, revision, piece_count };
                targets.slots.entry(target.clone()).or_insert(ProjectedSlot { output, drawn: Drawn::default() })
            }
        };

        let paint_target = PaintTarget::image(
            slot.output.camera,
            Vec2::new(viewport.width as f32, viewport.height as f32),
            1.0,
            0,
        );
        draw::paint(
            &mut commands,
            &mut slot.drawn,
            pieces,
            paint_target,
            Brightness::File,
            0.0,
            Some(ui),
            &mut kit,
            &mut nodes,
            &glyphs,
        );

        targets.diagnostics.insert(
            target,
            ProjectionDiagnostic::Active { source, revision, viewport, piece_count },
        );
    }
}

#[cfg(test)]
mod tests;
