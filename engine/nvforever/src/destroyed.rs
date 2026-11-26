//! Destroyed objects' look (docs/spec-notes/destruction-stages.md §4):
//!
//! - **Replacement models**: a destructible reference whose destruction stage reached
//!   one with a replacement model (DSTD's DMDL: the motorcycle's hulk) is drawn with that
//!   model (`fnv_game::Game::destruction_model`). Its drawn entities are taken away and the
//!   model drawn in their place under its cell, and the streamer keeps the model for it
//!   (`Streamer::model_overrides`), so the cell drawn again later draws the hulk too, as
//!   does a loaded save. ClearDestruction brings the intact model back the same way.
//! - **Shapes below a damage-stage node** (`BSDamageStage`, `nvf_assets::model::
//!   damage_stages_of`) are drawn only while the reference's model damage stage is in the
//!   node's range ([`StagedShape`]) [hypothesis: as the add-on nodes below it,
//!   `particles::StagedAddon`]; a model drawn as a rig of its own (an idle, a door) keeps
//!   them all.
//!
//! Log: `destroyed: <ref> drawn with <model> (destruction stage <n>)`, `… drawn intact
//! again`; `damage stage: <ref> shape <name> shown/hidden at model stage <n>`.

use crate::world::{base_object_with_model, ref_affine, spawn_model, CellEntity, Streamer, WorldData};
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_world::placed::base_object;
use nvf_assets::model::ModelCtx;
use nvf_play::script::ScriptState;
use nvf_play::RefVisual;
use std::collections::HashMap;

/// How often the damaged references' models are looked at, seconds [design].
const LOOK_EVERY: f32 = 0.25;

/// A shape of placed reference `r` drawn only at model damage stages `stages`.
#[derive(Component)]
pub struct StagedShape {
    pub r: u32,
    pub stages: (i32, i32),
    /// Shown at the last look; `None` before the first.
    at: Option<bool>,
}

impl StagedShape {
    pub fn new(r: u32, stages: (i32, i32)) -> StagedShape {
        StagedShape { r, stages, at: None }
    }
}

pub struct DestroyedPlugin;

impl Plugin for DestroyedPlugin {
    fn build(&self, app: &mut App) {
        // Before the full-detail ring gives its cells' new meshes their range.
        app.add_systems(Update, (staged_shapes, swap_models).after(crate::world::stream).before(crate::full_detail::fade_ring));
    }
}

/// Show and hide staged shapes as their references' model damage stages change.
fn staged_shapes(script: Option<Res<ScriptState>>, mut shapes: Query<(&mut StagedShape, &mut Visibility)>) {
    let game = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game);
    for (mut s, mut v) in &mut shapes {
        let stage = game.map_or(0, |g| g.model_damage_stage(fnv_sim::FormId(s.r)) as i32);
        let shown = (s.stages.0..=s.stages.1).contains(&stage);
        if s.at == Some(shown) {
            continue;
        }
        if s.at.is_some() {
            info!("damage stage: {:08X} shapes of model stages {}-{} {} at model stage {stage}", s.r, s.stages.0, s.stages.1, if shown { "shown" } else { "hidden" });
        }
        *v = if shown { Visibility::Inherited } else { Visibility::Hidden };
        s.at = Some(shown);
    }
}

#[allow(clippy::too_many_arguments)]
fn swap_models(
    mut commands: Commands,
    time: Res<Time>,
    mut wait: Local<f32>,
    script: Option<Res<ScriptState>>,
    world: Res<WorldData>,
    mut streamer: ResMut<Streamer>,
    mut models: ModelCtx,
    mut emittance: ResMut<crate::emittance::EmittanceState>,
    drawn: Query<(Entity, &RefVisual, &ChildOf), crate::lights::NotLightSource>,
    cells: Query<&Transform, With<CellEntity>>,
    (ring_cells, mut ring): (Query<(&Transform, &crate::full_detail::RingCell)>, Option<ResMut<crate::full_detail::FullDetailRing>>),
) {
    *wait -= time.delta_secs();
    if *wait > 0.0 {
        return;
    }
    *wait = LOOK_EVERY;
    let Some(game) = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    let mut want: HashMap<u32, String> = game.damaged_refs().filter_map(|r| game.destruction_model(r).map(|m| (r.0, m.to_string()))).collect();
    // SetRefrModelPath's models (`fnv_game::nvse_jip14`), before a destruction stage's.
    for (r, m) in game.ref_models() {
        if let Some(m) = m {
            want.insert(r.0, m.clone());
        }
    }
    let changed: Vec<u32> = want
        .keys()
        .chain(streamer.model_overrides.keys())
        .copied()
        .filter(|r| want.get(r) != streamer.model_overrides.get(r))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if !changed.is_empty() {
        streamer.model_overrides = want.clone();
    }
    for &r in &changed {
        // The full-detail ring's copies are redrawn below, once their model is prepared.
        if let Some(ring) = ring.as_deref_mut().filter(|ring| ring.radius > 0 || !ring.cells.is_empty()) {
            ring.pending_swaps.insert(r);
        }
        // Its entities directly under a cell (a static model's shapes, a rig's root).
        let mine: Vec<(Entity, Entity)> = drawn.iter().filter(|(_, v, c)| v.0 == r && cells.contains(c.parent())).map(|(e, _, c)| (e, c.parent())).collect();
        let Some(&(_, cell_e)) = mine.first() else { continue };
        let (Ok(cell), Some(p)) = (cells.get(cell_e), world.index.placement(world.lo, world.schema, r)) else { continue };
        let Some(base) = p.base.and_then(|b| base_object(world.lo, world.schema, b)) else { continue };
        for (e, _) in &mine {
            commands.entity(*e).try_despawn();
        }
        let base = match want.get(&r) {
            Some(m) => {
                info!("destroyed: {r:08X} drawn with {m} (model damage stage {})", game.model_damage_stage(fnv_sim::FormId(r)));
                base_object_with_model(base, m)
            }
            None => {
                info!("destroyed: {r:08X} drawn intact again");
                base
            }
        };
        let rt = Affine3A::from_translation(-cell.translation) * ref_affine(&p);
        let Streamer { hide, stats, .. } = &mut *streamer;
        spawn_model(&mut commands, cell_e, &world, &p, r, &base, rt, &mut models, &mut emittance, hide, stats, crate::world::SpawnMode::Full);
    }
    // The ring's copies (full_detail.rs, #2701): drawn only, never preparing a model on this
    // thread. A reference whose new model is not prepared stays pending (asked of the worker
    // threads, tried again at the next look); a new model that draws nothing (missing,
    // empty) leaves the old copy in place (`full_detail::swap_outcome`).
    let Some(ring) = ring.as_deref_mut().filter(|r| !r.pending_swaps.is_empty()) else { return };
    let started = std::time::Instant::now();
    for r in std::mem::take(&mut ring.pending_swaps) {
        let Some(p) = world.index.placement(world.lo, world.schema, r) else { continue };
        let Some(base) = p.base.and_then(|b| base_object(world.lo, world.schema, b)) else { continue };
        let base = match streamer.model_overrides.get(&r) {
            Some(m) => base_object_with_model(base, m),
            None => base,
        };
        // Its copies by ring cell (drawn or still spawning, never a retiring one), and the
        // ring cells where its model spawned no shape before. A reference of a type the
        // ring never draws (an actor) has neither: a swap never draws it there.
        let mut targets: HashMap<Entity, Vec<Entity>> = HashMap::new();
        for (e, _, c) in drawn.iter().filter(|(_, v, c)| v.0 == r && ring_cells.contains(c.parent()) && ring.owns(c.parent())) {
            targets.entry(c.parent()).or_default().push(e);
        }
        for e in ring.retry_holders(r) {
            targets.entry(e).or_default();
        }
        if targets.is_empty() {
            // No ring copy now: a ring cell drawn later takes the model as it is then.
            continue;
        }
        let ready = base.world_model_path().is_none_or(|path| models.models_ready([(path.as_str(), base.world_alt_textures.as_slice())]));
        if !ready {
            ring.pending_swaps.insert(r);
            continue;
        }
        let flagged = crate::full_detail::flagged(world.lo, &p);
        for (cell_e, old) in targets {
            let Ok((cell, _)) = ring_cells.get(cell_e) else { continue };
            let rt = Affine3A::from_translation(-cell.translation) * ref_affine(&p);
            let drew = spawn_model(&mut commands, cell_e, &world, &p, r, &base, rt, &mut models, &mut emittance, &streamer.hide, ring.redraw_stats(), crate::world::SpawnMode::Scenery);
            if crate::full_detail::swap_outcome(true, Some(drew)) == crate::full_detail::SwapOutcome::Replaced {
                for e in &old {
                    commands.entity(*e).try_despawn();
                }
            }
            // The cell's record for it changes only when it now draws, or had nothing drawn.
            if drew || old.is_empty() {
                ring.drew(cell_e, r, flagged, drew);
            }
            ring.redrawn(cell_e);
        }
    }
    ring.stats.ms += started.elapsed().as_secs_f64() * 1000.0;
}
