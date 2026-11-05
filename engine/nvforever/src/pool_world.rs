//! The balls and the cue of a pool game in the world (the fork; nvf-play `pool.rs`): the
//! mod's `Ball00..15.nif` on the cloth where the game has them, turned as they roll, and
//! `PracticeCue.nif` behind the white while someone aims [observed:
//! docs/spec-notes/pool.md §7: the sim-to-table mapping, the balls' height, the cue's tip
//! 18 + 42 power behind the white].
//!
//! - **The stage**: the camera looks down on the table (nvf-play `pool_camera`); the
//!   player is put in third person with the weapon away meanwhile, so no first-person
//!   arms hang in that view.
//! - **The cloth cleared**: the mod pushes the loose things lying on the table (a vanilla
//!   table's rack and balls) off it as a game starts [observed: ClearTable.txt: what is
//!   within 125 by 72 table units of its middle, 18 to 100 above it]; NVForever puts them
//!   out of sight while the table is played and shows them again after [design: nothing
//!   is moved for good].
//! - **Watchers**: a co-op player near a table another player plays sees its balls too, as
//!   replayed from each shot's strike (`nvf_play::pool::PoolWatch`).

use crate::world::WorldRoot;
use bevy::prelude::*;
use fnv_game::pool;
use nvf_assets::model::{ModelCtx, PreparedModel};
use nvf_play::control::{PlayerControl, Pov};
use nvf_play::pool::{PoolScreen, PoolWatch, TableFrame};
use std::sync::Arc;

/// The cue's tip height, table units [observed: Cue3D.txt].
const CUE_Z: f32 = 28.1;
/// How far the cue's butt is lifted, degrees: clear of the cushion [design; the mod lifts
/// it by what is behind the white, at most 25 degrees, CueLift.txt].
const CUE_LIFT: f32 = 9.0;
/// Model 16 is the cue.
const CUE: usize = 16;

fn model_path(i: usize) -> String {
    if i == CUE { "meshes\\minigames\\pool\\practicecue.nif".into() } else { format!("meshes\\minigames\\pool\\ball{i:02}.nif") }
}

#[derive(Component)]
struct PoolHolder;

#[derive(Resource, Default)]
struct PoolModels {
    models: Vec<Option<Arc<PreparedModel>>>,
    tried: bool,
    /// Each model's holders, one per table it is drawn on.
    holders: Vec<Vec<Entity>>,
}

#[derive(Resource, Default)]
struct Stage {
    pov: Option<Pov>,
}

pub struct PoolWorldPlugin;

impl Plugin for PoolWorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PoolModels>()
            .init_resource::<Stage>()
            .add_systems(Update, stage.before(nvf_play::player::BodySet).before(crate::idles::drive))
            .add_systems(PostUpdate, place.before(bevy::transform::TransformSystems::Propagate))
            .add_systems(Update, clear_cloth);
    }
}

/// Third person with the weapon away while a pool table is open.
fn stage(mut st: ResMut<Stage>, screen: Res<PoolScreen>, mut control: ResMut<PlayerControl>) {
    if screen.0.as_ref().filter(|p| p.frame.is_some()).is_none() {
        if let Some(pov) = st.pov.take() {
            control.pov = pov;
            info!("pool stage: the view is the player's again");
        }
        return;
    }
    if st.pov.is_none() {
        st.pov = Some(control.pov);
        info!("pool stage: the camera over the table, the weapon away");
    }
    control.pov = Pov::Third;
    control.weapon_out = false;
}

/// A ball on the cloth: at its point, turned as it has rolled, the table's scale.
fn ball_at(f: &TableFrame, b: &pool::Ball) -> Transform {
    let [w, x, y, z] = b.rot;
    Transform { translation: Vec3::from(f.sim_to_world(b.pos, pool::BALL_Z)), rotation: f.rotation() * Quat::from_xyzw(x, y, z, w).normalize(), scale: Vec3::splat(f.k) }
}

/// The cue: its tip behind the white along the aim, its shaft (the model's -X) back from
/// there, the butt lifted.
fn cue_at(f: &TableFrame, white: [f32; 2], dir: [f32; 2], power: f32) -> Transform {
    let back = 18.0 + 42.0 * power.clamp(0.0, 1.0);
    let tip = f.sim_to_world([white[0] - dir[0] * back, white[1] - dir[1] * back], CUE_Z);
    // Table units: y is the cloth's -y.
    let yaw = (-dir[1]).atan2(dir[0]);
    let local = Quat::from_rotation_z(yaw) * Quat::from_rotation_y(CUE_LIFT.to_radians());
    Transform { translation: Vec3::from(tip), rotation: f.rotation() * local, scale: Vec3::splat(f.k) }
}

/// Whether a placed thing lies on a table's cloth, as the mod clears it [observed:
/// ClearTable.txt].
fn on_cloth(f: &TableFrame, at: [f32; 3]) -> bool {
    let l = f.to_local(at);
    l[0].abs() <= 125.0 && l[1].abs() <= 72.0 && (18.0..=100.0).contains(&l[2])
}

/// The loose things on each table being played are out of sight (found once as its game
/// starts), and shown again once it is over.
fn clear_cloth(screen: Res<PoolScreen>, watch: Res<PoolWatch>, sim: Option<Res<nvf_play::sim::SimState>>, mut cleared: Local<Vec<(u32, Vec<Entity>)>>, refs: Query<(Entity, &nvf_play::RefVisual, &GlobalTransform)>, mut shown: Query<&mut Visibility>) {
    let mut live: Vec<(u32, TableFrame)> = Vec::new();
    if let Some(p) = screen.0.as_ref().filter(|p| p.page == nvf_play::pool::Page::Play) {
        if let Some(f) = p.frame {
            live.push((p.table_ref, f));
        }
    }
    for r in watch.0.values().filter(|r| r.view.playing) {
        if let (Some(f), false) = (r.frame, live.iter().any(|l| l.0 == r.table_ref)) {
            live.push((r.table_ref, f));
        }
    }
    cleared.retain(|(table, hidden)| {
        if live.iter().any(|l| l.0 == *table) {
            return true;
        }
        for e in hidden {
            if let Ok(mut v) = shown.get_mut(*e) {
                v.set_if_neq(Visibility::Inherited);
            }
        }
        info!("pool world: the things on table 0x{table:08X}'s cloth are back ({})", hidden.len());
        false
    });
    // Placed things only: not people (a player or a resident standing at the rail).
    let loose = |r: u32| {
        sim.as_deref().is_some_and(|sim| {
            sim.refs.get(fnv_sim::FormId(r)).is_some() && !sim.base_of(r).and_then(|b| sim.forms.get(b)).is_some_and(|b| matches!(b.data, fnv_sim::forms::BaseData::Actor(_)))
        })
    };
    for (table, f) in live {
        if !cleared.iter().any(|c| c.0 == table) {
            // Where each thing is placed (its reference, as the mod's GetPos reads it; the
            // drawn entity's origin may sit elsewhere).
            let placed = |r: u32| {
                sim.as_deref().and_then(|sim| {
                    let id = fnv_sim::FormId(r);
                    sim.world.peek(id).map(|x| x.pos).or_else(|| sim.refs.get(id).map(|x| x.pos))
                })
            };
            let mut names: Vec<String> = Vec::new();
            let found: Vec<Entity> = refs
                .iter()
                .filter(|(e, r, _)| r.0 != table && loose(r.0) && placed(r.0).is_some_and(|at| on_cloth(&f, at)) && shown.get(*e).is_ok_and(|v| *v != Visibility::Hidden))
                .map(|(e, r, _)| {
                    let name = sim.as_deref().and_then(|s| s.base_of(r.0).and_then(|b| s.forms.get(b)).and_then(|b| b.edid.clone())).unwrap_or_else(|| format!("0x{:08X}", r.0));
                    if !names.contains(&name) {
                        names.push(name);
                    }
                    e
                })
                .collect();
            info!("pool world: cleared off table 0x{table:08X}'s cloth: {} ({} drawn parts)", names.join(", "), found.len());
            cleared.push((table, found));
        }
    }
    for (_, hidden) in cleared.iter() {
        for e in hidden {
            if let Ok(mut v) = shown.get_mut(*e) {
                v.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

fn place(
    mut commands: Commands,
    mut st: ResMut<PoolModels>,
    mut models: ModelCtx,
    screen: Res<PoolScreen>,
    watch: Res<PoolWatch>,
    root: Query<Entity, With<WorldRoot>>,
    mut holders: Query<(&mut Transform, &mut Visibility), With<PoolHolder>>,
) {
    // What to draw: (model, where).
    let mut want: Vec<(usize, Transform)> = Vec::new();
    let table = |f: &TableFrame, t: &pool::Table, cue: Option<([f32; 2], f32)>, want: &mut Vec<(usize, Transform)>| {
        for (i, b) in t.balls.iter().enumerate() {
            if b.on {
                want.push((i, ball_at(f, b)));
            }
        }
        if let (Some((dir, power)), true) = (cue, t.balls[0].on) {
            want.push((CUE, cue_at(f, t.balls[0].pos, dir, power)));
        }
    };
    let own = screen.0.as_ref().filter(|p| p.page == nvf_play::pool::Page::Play || p.joined.is_some());
    if let Some(p) = own {
        if let Some(f) = p.frame {
            let mut t = p.table.clone();
            // The white from the hand where it goes, while it is put down.
            if p.my_shot() && p.in_hand() {
                match p.hand_spot() {
                    Some(at) => {
                        t.balls[0].pos = at;
                        t.balls[0].on = true;
                    }
                    None => t.balls[0].on = false,
                }
            }
            table(&f, &t, p.cue(p.now), &mut want);
        }
    }
    let own_ref = screen.0.as_ref().map(|p| p.table_ref);
    for r in watch.0.values() {
        if Some(r.table_ref) == own_ref || !r.view.playing {
            continue;
        }
        if let Some(f) = r.frame {
            table(&f, &r.table, r.view.aim.filter(|_| r.view.shot.is_none() && !r.table.moving()), &mut want);
        }
    }
    let shown = st.holders.iter().any(|h| !h.is_empty());
    if want.is_empty() && !shown {
        return;
    }
    if !st.tried {
        st.tried = true;
        st.models = (0..=CUE).map(|i| models.model(&model_path(i))).collect();
        let found = st.models.iter().filter(|m| m.is_some()).count();
        if found == CUE + 1 {
            info!("pool world: the 16 balls and the cue loaded");
        } else {
            warn!("pool world: {found} of the 17 models found; the missing ones are not drawn");
        }
        st.holders = vec![Vec::new(); CUE + 1];
    }
    let Ok(root) = root.single() else { return };
    let mut used = [0usize; CUE + 1];
    for (m, t) in &want {
        let Some(pm) = st.models[*m].clone() else { continue };
        let k = used[*m];
        used[*m] += 1;
        if k == st.holders[*m].len() {
            let h = commands.spawn((PoolHolder, *t, Visibility::Inherited, ChildOf(root))).id();
            for s in &pm.shapes {
                let e = commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), Transform::from_matrix(Mat4::from(s.transform)), Visibility::Inherited)).id();
                commands.entity(h).add_child(e);
            }
            st.holders[*m].push(h);
            continue;
        }
        if let Ok((mut tr, mut vis)) = holders.get_mut(st.holders[*m][k]) {
            *tr = *t;
            vis.set_if_neq(Visibility::Inherited);
        }
    }
    for (m, hs) in st.holders.iter().enumerate() {
        for h in hs.iter().skip(used[m]) {
            if let Ok((_, mut vis)) = holders.get_mut(*h) {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cue_lies_behind_the_white_pointing_at_it() {
        let f = TableFrame::of([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], None);
        let white = [500.0, 250.0];
        let t = cue_at(&f, white, [1.0, 0.0], 0.0);
        let w = Vec3::from(f.sim_to_world(white, CUE_Z));
        // The tip 18 sim units (4 table units) short of the white, along +x.
        assert!((w.x - t.translation.x - 4.0).abs() < 1e-3 && (w.y - t.translation.y).abs() < 1e-3);
        // The shaft (-X) runs back and up.
        let shaft = t.rotation * Vec3::NEG_X;
        assert!(shaft.x < -0.9 && shaft.z > 0.0, "{shaft:?}");
        // Aiming down the cloth (sim +y is the table's -y), the shaft runs back up it.
        let t = cue_at(&f, white, [0.0, 1.0], 0.5);
        let shaft = t.rotation * Vec3::NEG_X;
        assert!(shaft.y > 0.9, "{shaft:?}");
    }
}
