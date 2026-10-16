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
    /// The same clips with only their `Weapon` track: where the weapon hangs while the
    /// body plays something else entirely (a seat's clip, [`WeaponHold::weapon_layers`]).
    pub weapon_only: BTreeMap<EquipClip, Arc<BoundClip>>,
    pub facts: StanceFacts,
    pub holster: String,
    pub drawn: String,
    /// Skeleton node indices of `Weapon` and of every bone the weapon can hang from.
    pub weapon_node: usize,
    pub bones: HashMap<String, usize>,
    /// Where the equip clip leaves the `Weapon` node: its rest pose while in the hand
    /// ([`WeaponHold::rest`]).
    pub weapon_rest: Option<Transform>,
    /// The weapon-model nodes the holster clip switches off (`##FlamerGun`,
    /// `##MinigunGun`): the clips show and hide them ([`gun_shown`]).
    pub holstered_off: Vec<String>,
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
        let weapon_only = clips.iter().map(|(c, b)| (*c, Arc::new(weapon_track_only(b, weapon_node)))).collect();
        let weapon_rest = clips.get(&EquipClip::Equip).and_then(|c| track_end(c, weapon_node, skel.locals[weapon_node]));
        let holstered_off = clips.get(&EquipClip::Holster).map_or_else(Vec::new, |c| holstered_off(&c.clip));
        Ok((Stance { prefix: prefix.to_string(), clips, weapon_only, facts, holster, drawn, weapon_node, bones, weapon_rest, holstered_off }, line))
    }
}

/// The nodes a holster clip's visibility tracks switch off at some time (the `2hh` gun
/// parts, animation.md §9.7).
fn holstered_off(clip: &nvf_assets::anim_play::Clip) -> Vec<String> {
    clip.visibility.iter().filter(|(_, _, b)| b.keys.iter().any(|(_, v)| !v) || b.value == Some(false)).map(|(n, ..)| n.clone()).collect()
}

/// `clip` with only the track of the `Weapon` node (index `weapon_node`).
pub fn weapon_track_only(clip: &BoundClip, weapon_node: usize) -> BoundClip {
    BoundClip { clip: clip.clip.clone(), tracks: clip.tracks.iter().filter(|(i, ..)| *i == weapon_node).cloned().collect(), root: None, root_track: None }
}

/// Where `clip` leaves node `node` at its end (`bind` fills what the track leaves out);
/// `None` when the clip does not drive it.
fn track_end(clip: &BoundClip, node: usize, bind: Transform) -> Option<Transform> {
    let (_, t, _) = clip.tracks.iter().find(|(i, ..)| *i == node)?;
    let time = clip.clip.stop;
    Some(Transform {
        translation: t.translation.sample(time).map_or(bind.translation, Vec3::from_array),
        rotation: t.rotation_at(time).map_or(bind.rotation, |[w, x, y, z]| Quat::from_xyzw(x, y, z, w).normalize()),
        scale: Vec3::splat(t.scale.sample(time).map_or(bind.scale.x, |[s]| s)),
    })
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
    /// The model's shapes under a node the holster clip switches off (the flamer's and
    /// minigun's gun): the shape, that node's name and whether it was last shown.
    pub gun: Vec<(Entity, String, Option<bool>)>,
    /// The model's shapes on a bone of their own (the flamer's tank and the minigun's
    /// backpack on `Bip01 Spine2`): on the body whether the weapon is drawn or not.
    pub back: Vec<Entity>,
}

impl WeaponHold {
    pub fn new(stance: Arc<Stance>, drawn: bool, skel_parent: usize) -> WeaponHold {
        WeaponHold { stance, anim: WeaponAnim::new(drawn), drawn, hold: false, parent: skel_parent, test: None, gun: Vec::new(), back: Vec::new() }
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
        self.rest(player);
    }

    /// In the hand, the `Weapon` node rests where the equip clip leaves it: the aim and
    /// movement clips of some stances (`2hh`: the minigun, the flamer, the gatling laser;
    /// `1hp`) do not drive it, and the equip, attack and reload clips that do agree on
    /// the place [observed: animation.md §9.7; hypothesis: the game leaves the node where
    /// the last clip put it]. Back at its bind transform the weapon turned in the hand
    /// and the left hand missed it [observed in NVForever: owner's playtest 2026-10-16,
    /// Cook-Cook's flamer]. Holstered, the node keeps its bind transform.
    fn rest(&self, player: &mut BlendPlayer) {
        let st = &self.stance;
        let in_hand = self.anim.parent(&st.holster, &st.drawn, &st.facts) == st.drawn;
        player.set_rest(st.weapon_node, st.weapon_rest.filter(|_| in_hand));
    }

    /// Only where the weapon hangs, as player layers 2 and 3: for an actor whose body plays
    /// a clip of its own over everything (on furniture). The stance's clips hold the
    /// `Weapon` node's place under the bone it hangs from (the holster clip: on the hip
    /// or the back); without them the node went back to its bind transform, which is its
    /// place under the right hand, and the holstered weapon floated off the seated body
    /// at a wrong angle [observed in NVForever, owner's playtest 2026-10-11; the seat
    /// clips have no `Weapon` track: `dynamicidle_chairsit.kf`, `chair_forwardenter.kf`].
    pub fn weapon_layers(&self, player: &mut BlendPlayer) {
        let st = &self.stance;
        let [a, b] = self.anim.layers(&st.facts);
        let layer = |l: Option<(EquipClip, f32, f32)>| l.and_then(|(c, phase, weight)| Some(Layer { clip: st.weapon_only.get(&c)?.clone(), phase, weight }));
        player.layers[2] = layer(a);
        player.layers[3] = layer(b);
        self.rest(player);
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

/// The shapes of a weapon model as spawned on an actor ([`spawn_model`]).
#[derive(Default)]
pub struct SpawnedWeapon {
    /// Every shape.
    pub all: Vec<Entity>,
    /// Those under a node of `gun_nodes`, with its name ([`WeaponHold::gun`]).
    pub gun: Vec<(Entity, String)>,
    /// Those on a bone of their own ([`WeaponHold::back`]).
    pub back: Vec<Entity>,
}

/// On a weapon shape that hangs from a bone of its own (the flamer's tank): not part of
/// the weapon in the hand (its hit volume, `limbs.rs`).
#[derive(Component)]
pub struct OnBack;

/// Spawn the weapon model's shapes under the actor's `Weapon` node (rigid, in the node's
/// own frame [hypothesis, actors.md §9.2]). A shape under a node whose `UPB` string
/// (`bones`, by node name) names a bone of the skeleton hangs from that bone instead,
/// with its place in the whole model (the flamer's tank and the minigun's backpack say
/// `Bip01 Spine2`), as the player body does [hypothesis: that is the string's meaning;
/// animation.md §9.7]. Shapes under a node named in `gun_nodes` are listed apart.
pub fn spawn_model(
    commands: &mut Commands,
    actor: &SpawnedActor,
    skel: &PreparedSkeleton,
    weapon_node: usize,
    part: &PreparedPart,
    bones: &HashMap<String, String>,
    gun_nodes: &[String],
) -> SpawnedWeapon {
    let mut out = SpawnedWeapon::default();
    for s in part.shapes.iter().filter(|s| !s.cap) {
        let names = || s.ancestors.iter().filter_map(|(n, _)| n.as_deref()).chain(s.name.as_deref());
        let bone = names().find_map(|n| skel.by_name.get(bones.get(n)?.trim()).copied());
        let tr = Transform::from_matrix(Mat4::from(part.root * s.transform));
        let e = commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), tr)).id();
        commands.entity(actor.nodes[bone.unwrap_or(weapon_node)]).add_child(e);
        if bone.is_some() {
            commands.entity(e).insert(OnBack);
            out.back.push(e);
        } else if let Some(g) = names().find_map(|n| gun_nodes.iter().find(|g| g.eq_ignore_ascii_case(n))) {
            out.gun.push((e, g.clone()));
        }
        out.all.push(e);
    }
    out
}

/// Whether the gun shapes under node `name` ([`WeaponHold::gun`]) show now: as the
/// current stage's clip's visibility track for that node says at the clip's time (off
/// in `2hhholster.kf`, on from 0.500 s of `2hhequip.kf`, off from 0.500 s of
/// `2hhunequip.kf`; observed, animation.md §9.7); a clip with no track for it (the aim
/// clip) leaves it as it was (`last`), shown at first, as the player body does
/// [hypothesis: animation.md §9.7].
pub fn gun_shown(hold: &WeaponHold, name: &str, last: Option<bool>) -> bool {
    let c = hold.anim.stage.clip();
    let track = hold.stance.clips.get(&c).and_then(|b| {
        let (_, _, t) = b.clip.visibility.iter().find(|(n, ..)| n.eq_ignore_ascii_case(name))?;
        let len = hold.stance.facts.get(&c).map_or(0.0, |f| f.duration);
        t.at(b.clip.start + WeaponAnim::phase(c, hold.anim.time, &hold.stance.facts) * len)
    });
    track.or(last).unwrap_or(true)
}

/// Hang the `Weapon` node of every armed actor from the bone its draw cycle names (the
/// draw cycle itself advances in `locomotion::drive`), and show or hide its gun shapes
/// ([`gun_shown`]).
pub fn reparent(mut commands: Commands, mut q: Query<(&mut WeaponHold, &BlendPlayer)>) {
    for (mut w, player) in &mut q {
        let shown: Vec<bool> = w.gun.iter().map(|(_, name, last)| gun_shown(&w, name, *last)).collect();
        for ((e, _, last), shown) in w.gun.iter_mut().zip(shown) {
            if *last != Some(shown) {
                commands.entity(*e).try_insert(if shown { Visibility::Inherited } else { Visibility::Hidden });
                *last = Some(shown);
            }
        }
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
        let f = |duration: f32, bone: &str| ClipFacts { duration, blend_time: 0.2, prn: vec![(0.5, bone.to_string())], switch: None };
        let facts = StanceFacts::from([
            (EquipClip::Holster, f(0.1, "Bip01 Pelvis")),
            (EquipClip::Equip, f(0.4, "Bip01 R Hand")),
            (EquipClip::Aim, f(2.0, "Bip01 R Hand")),
            (EquipClip::Unequip, f(0.5, "Bip01 Pelvis")),
        ]);
        let bones = HashMap::from([("Bip01 Pelvis".to_string(), 3), ("Bip01 R Hand".to_string(), 7)]);
        Arc::new(Stance { prefix: "1hp".into(), clips: BTreeMap::new(), weapon_only: BTreeMap::new(), facts, holster: "Bip01 Pelvis".into(), drawn: "Bip01 R Hand".into(), weapon_node: 8, bones, weapon_rest: None, holstered_off: Vec::new() })
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

    #[test]
    fn gun_shows_as_the_clips_say() {
        use fnv_nif::anim::BoolTrack;
        // As `2hhholster.kf` / `2hhequip.kf` / `2hhunequip.kf` (animation.md §9.7): off,
        // on from half way through the draw, off from half way through putting it away.
        let clip = |stop: f32, keys: Vec<(f32, bool)>| {
            let mut c = nvf_assets::anim_play::Clip::from_controllers(Vec::new());
            c.stop = stop;
            if !keys.is_empty() {
                c.visibility.push(("##FlamerGun".into(), 0, BoolTrack { value: None, keys }));
            }
            Arc::new(BoundClip { clip: Arc::new(c), tracks: Vec::new(), root: None, root_track: None })
        };
        let mut st = Arc::try_unwrap(stance()).ok().expect("one owner");
        st.clips.insert(EquipClip::Holster, clip(0.1, vec![(0.0, false)]));
        st.clips.insert(EquipClip::Equip, clip(0.4, vec![(0.0, false), (0.2, true)]));
        st.clips.insert(EquipClip::Aim, clip(2.0, Vec::new()));
        st.clips.insert(EquipClip::Unequip, clip(0.5, vec![(0.0, true), (0.25, false)]));
        st.holstered_off = holstered_off(&st.clips[&EquipClip::Holster].clip);
        assert_eq!(st.holstered_off, ["##FlamerGun"]);
        let mut w = WeaponHold::new(Arc::new(st), false, 7);
        let mut last = None;
        let mut seen = Vec::new();
        for i in 0..200 {
            w.drawn = (40..120).contains(&i);
            w.step(1.0 / 60.0);
            let shown = gun_shown(&w, "##FlamerGun", last);
            if last != Some(shown) {
                seen.push((w.anim.stage, shown));
            }
            last = Some(shown);
        }
        // Hidden holstered and early in the draw; shown from half way through it and on
        // through the aim clip, which has no track; hidden again half way through putting
        // it away.
        assert_eq!(seen, [(Stage::Holstered, false), (Stage::Equipping, true), (Stage::Unequipping, false)]);
    }
}
