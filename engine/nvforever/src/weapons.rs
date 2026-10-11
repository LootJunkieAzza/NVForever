//! Weapons in NPCs' hands (workstream 5 part H, docs/spec-notes/actors.md §9): the weapon
//! an actor carries is drawn on its skeleton's `Weapon` node; the stance's holster, equip,
//! aim and unequip clips play over its locomotion by track priority, and the `Weapon` node
//! moves between holster bone and hand at the clips' `prn:` keys. Nothing decides when to
//! draw yet (Layer 6); `--actor-weapons drawn` and the test aid `--actor-weapon-test` do.

use bevy::prelude::*;
use fnv_actor::weapon::{clip_path, mount_bones, EquipClip, StanceFacts, WeaponAnim};
use nvf_assets::actor::{bind_clip, ActorCtx, PreparedPart, PreparedSkeleton, SpawnedActor};
use nvf_assets::anim_play::{BlendPlayer, BoundClip, Layer};
use nvf_play::combat::WeaponBroken;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// How NPCs' weapons are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum WeaponMode {
    /// Not drawn.
    None,
    /// Holstered (at the pelvis or on the back, actors.md §9.2).
    #[default]
    Holstered,
    /// In the hand, at the ready (the stance's aim clip).
    Drawn,
}

/// The skeleton node the weapon model hangs from [observed: `skeleton.nif`, under
/// `Bip01 R Hand`].
pub const WEAPON_NODE: &str = "Weapon";

/// The clips of one weapon stance bound to one skeleton, shared by the actors using them.
pub struct Stance {
    /// The stance prefix (`2hr`, `1hp`, …).
    pub prefix: String,
    pub clips: BTreeMap<EquipClip, Arc<BoundClip>>,
    pub facts: StanceFacts,
    pub holster: String,
    pub drawn: String,
    /// Skeleton node indices of `Weapon` and of every bone the weapon can hang from.
    pub weapon_node: usize,
    pub bones: HashMap<String, usize>,
}

impl Stance {
    /// The stance `prefix` for the skeleton at `skeleton` (archive path), or why not; also
    /// a report line.
    pub fn load(ctx: &mut ActorCtx, skeleton: &str, skel: &PreparedSkeleton, prefix: &str) -> Result<(Stance, String), String> {
        let weapon_node = *skel.by_name.get(WEAPON_NODE).ok_or("the skeleton has no Weapon node")?;
        let s = fnv_vfs::normalize(skeleton);
        let folder = s.rfind('\\').map_or("", |i| &s[..i]).to_string();
        let mut clips = BTreeMap::new();
        let mut facts = StanceFacts::new();
        for c in EquipClip::ALL {
            let path = clip_path(&folder, prefix, c);
            if let Some(clip) = ctx.clip(&path) {
                facts.insert(c, fnv_actor::weapon::ClipFacts::from_parts(clip.start, clip.stop, &clip.text_keys));
                clips.insert(c, Arc::new(bind_clip(skel, clip)));
            }
        }
        if !clips.contains_key(&EquipClip::Holster) && !clips.contains_key(&EquipClip::Aim) {
            return Err(format!("no {prefix} holster or aim clip in {folder}"));
        }
        let parent = skel.parents[weapon_node].and_then(|p| skel.names[p].clone()).unwrap_or_default();
        let (holster, drawn) = mount_bones(&facts, &parent);
        let mut bones = HashMap::new();
        let names = facts.values().flat_map(|f| f.prn.iter().map(|(_, b)| b.clone())).chain([holster.clone(), drawn.clone()]);
        for n in names {
            if let Some(&i) = skel.by_name.get(&n) {
                bones.insert(n, i);
            }
        }
        let line = format!(
            "{folder} {prefix}: clips {:?}, holstered at {holster}, drawn at {drawn}, bones found {}",
            clips.keys().collect::<Vec<_>>(),
            bones.len()
        );
        Ok((Stance { prefix: prefix.to_string(), clips, facts, holster, drawn, weapon_node, bones }, line))
    }
}

/// An armed actor's weapon state (on the actor's root, beside its `Locomotion`).
#[derive(Component)]
pub struct WeaponHold {
    pub stance: Arc<Stance>,
    pub anim: WeaponAnim,
    /// Whether the weapon should be drawn.
    pub drawn: bool,
    /// Keep the pose (`--actor-pose-time`).
    pub hold: bool,
    /// The node the `Weapon` node hangs from now.
    pub parent: usize,
    /// Test aid: draw and holster every `period` seconds; the time into the cycle.
    pub test: Option<(f32, f32)>,
}

impl WeaponHold {
    pub fn new(stance: Arc<Stance>, drawn: bool, skel_parent: usize) -> WeaponHold {
        WeaponHold { stance, anim: WeaponAnim::new(drawn), drawn, hold: false, parent: skel_parent, test: None }
    }

    /// Advance the test aid and the draw cycle by `dt`.
    pub fn step(&mut self, dt: f32) {
        if let Some((period, t)) = self.test.as_mut() {
            *t += dt;
            // Holstered for the first period, drawn for the second, and so on.
            self.drawn = (*t / period.max(0.1)).floor() as i64 % 2 == 1;
        }
        self.anim.step(self.drawn, dt, &self.stance.facts);
    }

    /// Run `secs` ahead in fixed 1/60 s steps (`--actor-pose-time`: a repeatable pose).
    pub fn simulate(&mut self, secs: f32) {
        const DT: f32 = 1.0 / 60.0;
        let mut left = secs;
        while left > 0.0 {
            let dt = left.min(DT);
            self.step(dt);
            left -= dt;
        }
    }

    /// The weapon clips as player layers 2 and 3 (0 and 1 are locomotion's).
    pub fn layers(&self, player: &mut BlendPlayer) {
        let st = &self.stance;
        let [a, b] = self.anim.layers(&st.facts);
        let layer = |l: Option<(EquipClip, f32, f32)>| l.and_then(|(c, phase, weight)| Some(Layer { clip: st.clips.get(&c)?.clone(), phase, weight }));
        player.layers[2] = layer(a);
        player.layers[3] = layer(b);
    }

    /// The node the weapon should hang from now, if the skeleton has that bone.
    pub fn parent_node(&self) -> Option<usize> {
        let st = &self.stance;
        st.bones.get(self.anim.parent(&st.holster, &st.drawn, &st.facts)).copied()
    }
}

/// On each drawn shape of the weapon model in an actor's hand or holster: whose it is
/// and which WEAP (the hit volume of a drawn weapon, `limbs.rs`).
#[derive(Component, Clone, Copy, Debug)]
pub struct HeldWeapon {
    pub reference: u32,
    pub weapon: fnv_sim::FormId,
}

/// The muzzle of the weapon drawn on an actor: an empty entity under the actor's `Weapon`
/// node, where the weapon model's `ProjectileNode` is (the node shots are drawn from,
/// animation.md §9.9, as the player's), so it moves with the weapon. Its node's place
/// in the model at rest [design: the clips that move a weapon's `##` parts are not
/// applied to it].
#[derive(Component, Clone, Copy, Debug)]
pub struct Muzzle {
    pub reference: u32,
}

/// The nodes shots leave from, by either of their names (animation.md §9.9).
pub const MUZZLE_NODES: [&str; 2] = ["ProjectileNode", "##ProjectileNode"];

/// Put a [`Muzzle`] under the actor's `Weapon` node where the weapon model's
/// `ProjectileNode` is; nothing when the model has none.
pub fn spawn_muzzle(commands: &mut Commands, actor: &SpawnedActor, weapon_node: usize, model: &nvf_assets::model::PreparedModel, reference: u32) -> Option<Entity> {
    let path = nvf_assets::model::node_path(&model.nodes, &MUZZLE_NODES)?;
    let at = path.iter().fold(model.root, |m, &i| m * model.nodes[i].local);
    let e = commands.spawn((Name::new("Muzzle"), Muzzle { reference }, Transform::from_matrix(Mat4::from(at)), Visibility::Inherited)).id();
    commands.entity(actor.nodes[weapon_node]).add_child(e);
    Some(e)
}

/// A weapon that broke leaves its actor's hand (docs/spec-notes/combat-rules.md §17):
/// its model is taken off the skeleton and the weapon clips stop. The weapon itself now
/// lies on the ground as a dropped reference (`nvf_play::dropped` draws it).
pub fn drop_broken(
    mut commands: Commands,
    mut broken: MessageReader<WeaponBroken>,
    held: Query<(Entity, &HeldWeapon)>,
    mut holds: Query<(Entity, &crate::limbs::ActorRig, &mut BlendPlayer), With<WeaponHold>>,
) {
    for b in broken.read() {
        for (e, _) in held.iter().filter(|(_, h)| h.reference == b.form_id) {
            commands.entity(e).despawn();
        }
        for (e, _, mut player) in holds.iter_mut().filter(|(_, rig, _)| rig.reference == b.form_id) {
            commands.entity(e).remove::<WeaponHold>();
            (player.layers[2], player.layers[3]) = (None, None);
        }
    }
}

/// Spawn the weapon model's shapes under the actor's `Weapon` node (rigid, in the node's
/// own frame [hypothesis, actors.md §9.2]); returns the entities.
pub fn spawn_model(commands: &mut Commands, actor: &SpawnedActor, weapon_node: usize, part: &PreparedPart) -> Vec<Entity> {
    let mut out = Vec::new();
    for s in part.shapes.iter().filter(|s| !s.cap) {
        let tr = Transform::from_matrix(Mat4::from(part.root * s.transform));
        let e = commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), tr)).id();
        commands.entity(actor.nodes[weapon_node]).add_child(e);
        out.push(e);
    }
    out
}

/// Hang the `Weapon` node of every armed actor from the bone its draw cycle names (the
/// draw cycle itself advances in `locomotion::drive`).
pub fn reparent(mut commands: Commands, mut q: Query<(&mut WeaponHold, &BlendPlayer)>) {
    for (mut w, player) in &mut q {
        let Some(p) = w.parent_node() else { continue };
        if p != w.parent {
            commands.entity(player.nodes[p]).add_child(player.nodes[w.stance.weapon_node]);
            w.parent = p;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_actor::weapon::{ClipFacts, Stage};

    fn stance() -> Arc<Stance> {
        let f = |duration: f32, bone: &str| ClipFacts { duration, blend_time: 0.2, prn: vec![(0.5, bone.to_string())] };
        let facts = StanceFacts::from([
            (EquipClip::Holster, f(0.1, "Bip01 Pelvis")),
            (EquipClip::Equip, f(0.4, "Bip01 R Hand")),
            (EquipClip::Aim, f(2.0, "Bip01 R Hand")),
            (EquipClip::Unequip, f(0.5, "Bip01 Pelvis")),
        ]);
        let bones = HashMap::from([("Bip01 Pelvis".to_string(), 3), ("Bip01 R Hand".to_string(), 7)]);
        Arc::new(Stance { prefix: "1hp".into(), clips: BTreeMap::new(), facts, holster: "Bip01 Pelvis".into(), drawn: "Bip01 R Hand".into(), weapon_node: 8, bones })
    }

    #[test]
    fn test_aid_draws_and_holsters() {
        let mut w = WeaponHold::new(stance(), false, 7);
        w.test = Some((1.0, 0.0));
        assert_eq!(w.parent_node(), Some(3));
        let mut stages = vec![w.anim.stage];
        for _ in 0..240 {
            w.simulate(1.0 / 60.0);
            if stages.last() != Some(&w.anim.stage) {
                stages.push(w.anim.stage);
            }
        }
        // Four seconds: holstered for a second, drawn for one (the equip clip first), holstered
        // (after the unequip clip), drawn again.
        assert_eq!(stages, [Stage::Holstered, Stage::Equipping, Stage::Drawn, Stage::Unequipping, Stage::Holstered, Stage::Equipping, Stage::Drawn]);
        // Repeatable: the same time gives the same state.
        let (mut a, mut b) = (WeaponHold::new(stance(), false, 7), WeaponHold::new(stance(), false, 7));
        a.test = Some((1.0, 0.0));
        b.test = Some((1.0, 0.0));
        a.simulate(1.1);
        b.simulate(1.1);
        assert_eq!(a.anim, b.anim);
        // Early in the draw the weapon is still at the hip; past the clip's `prn:` key (half
        // way) it is in the hand.
        assert_eq!(a.anim.stage, Stage::Equipping);
        assert_eq!(a.parent_node(), Some(3));
        a.simulate(0.2);
        assert_eq!(a.anim.stage, Stage::Equipping);
        assert_eq!(a.parent_node(), Some(7));
    }
}
