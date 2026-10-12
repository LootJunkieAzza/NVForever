//! The Stealth Boy's look on the player (docs/spec-notes/aid-items.md §9): while the
//! player's Chameleon actor value is 1 or more (`fnv_game::aid`, the Stealth Boy's
//! "Stealth Field" effect), the player's body and arms rig are drawn see-through.
//!
//! [design: the original draws a shimmering refraction; NVForever has no such shader yet,
//! so every mesh of the player's rigs gets a copy of its material, blended, at
//! [`ALPHA`] of its opacity and tinted a little towards blue; the copies are dropped and
//! the materials put back when the effect ends.] Detection does not read this: it reads
//! the Chameleon value itself (`fnv_game::detection`).
//!
//! Log lines: `stealth field: on (Chameleon 75): <n> meshes see-through` and
//! `stealth field: off`.

use crate::player_body::PlayerBody;
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_assets::nif_material::NifMaterial;
use nvf_play::script::ScriptState;
use std::collections::HashMap;

/// The Chameleon actor value [documented: xEdit ActorValue, `schemas/fnv/common.json`].
const CHAMELEON: u16 = 49;

/// How opaque the player stays under the field [design].
pub const ALPHA: f32 = 0.3;

/// The tint multiplied into the colour [design].
const TINT: [f32; 3] = [0.6, 0.75, 1.0];

/// The meshes made see-through and their own materials.
#[derive(Resource, Default)]
pub struct StealthField {
    pub on: bool,
    originals: HashMap<Entity, Handle<NifMaterial>>,
}

pub struct StealthFieldPlugin;

impl Plugin for StealthFieldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StealthField>().add_systems(Update, apply.after(nvf_play::player::BodySet));
    }
}

/// The see-through copy of a material.
pub fn see_through(m: &NifMaterial) -> NifMaterial {
    let mut m = m.clone();
    let c = m.base.base_color.to_linear();
    m.base.base_color = Color::linear_rgba(c.red * TINT[0], c.green * TINT[1], c.blue * TINT[2], c.alpha * ALPHA);
    m.base.alpha_mode = AlphaMode::Blend;
    m
}

fn apply(
    script: Option<Res<ScriptState>>,
    body: Res<PlayerBody>,
    mut field: ResMut<StealthField>,
    children: Query<&Children>,
    mut meshes: Query<&mut MeshMaterial3d<NifMaterial>>,
    mut materials: ResMut<Assets<NifMaterial>>,
) {
    let chameleon = script.as_ref().and_then(|s| s.rt.as_ref()).map_or(0.0, |rt| fnv_game::functions::actor_value_temporary(&rt.game, FormId::PLAYER_REF, CHAMELEON));
    let want = chameleon >= 1.0;
    if want {
        // Every frame: a weapon drawn or an outfit changed brings new meshes.
        let mut fresh = 0;
        for root in body.roots() {
            for e in children.iter_descendants(root) {
                if field.originals.contains_key(&e) {
                    continue;
                }
                let Ok(mut handle) = meshes.get_mut(e) else { continue };
                let Some(copy) = materials.get(&handle.0).map(see_through) else { continue };
                let copy = materials.add(copy);
                field.originals.insert(e, std::mem::replace(&mut handle.0, copy));
                fresh += 1;
            }
        }
        if !field.on {
            info!("stealth field: on (Chameleon {chameleon:.0}): {fresh} meshes see-through");
            field.on = true;
        }
    } else if field.on {
        for (e, original) in field.originals.drain() {
            if let Ok(mut handle) = meshes.get_mut(e) {
                materials.remove(&handle.0);
                handle.0 = original;
            }
        }
        field.on = false;
        info!("stealth field: off");
    }
    // Meshes despawned meanwhile.
    field.originals.retain(|e, _| meshes.contains(*e));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_copy_is_blended_and_faint() {
        let m = nvf_assets::nif_material::nif_material(StandardMaterial { base_color: Color::WHITE, ..default() });
        let c = see_through(&m);
        assert!(matches!(c.base.alpha_mode, AlphaMode::Blend));
        assert!((c.base.base_color.alpha() - ALPHA).abs() < 1e-6);
    }
}
