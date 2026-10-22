//! Sounds of things that are drawn here: add-on nodes, projectiles in flight and the
//! weapons in actors' hands (docs/spec-notes/audio.md §15.2).
//!
//! - **Add-on nodes** [hypothesis: the fire add-ons' `FXFireLP` is the fire's crackle]:
//!   an add-on node in the world whose ADDN names a sound (`SNAM`) is a [`SoundSource`]
//!   where it is while it shows; the ambient plays it as it plays a sound marker
//!   (sounds with neither Loop nor Play At Random, the bullet-impact add-ons', are not:
//!   the impact plays its IPCT sound).
//! - **Projectiles** (PROJ DATA Sound): one whose sound has the Loop flag sounds it
//!   where it flies [hypothesis: a missile's fly-by loop]; any other plays once, where
//!   it is, when it first comes within the sound's maximum distance of the listener
//!   having been farther [hypothesis: the bullet whiz is a fly-by; design: when], so a
//!   shot that starts within reach (the player's own) never whizzes.
//! - **Idle** (WEAP `UNAM`): an actor with its weapon out sounds the weapon's idle sound
//!   at the actor, going round whatever its flags [hypothesis: 12 of the 16 have neither
//!   flag, their files are loops by name]. The player's is `nvf_play::combat`'s.
//!
//! An `own sounds summary` line at the end.

use crate::particles::AddonEmitter;
use crate::weapons::HeldWeapon;
use crate::world::WorldRoot;
use bevy::prelude::*;
use nvf_play::combat::ShotVisuals;
use nvf_play::player::Player;
use nvf_play::script::ScriptState;
use nvf_play::sfx::{PlaySound, Sfx, SoundSource};
use nvf_play::sim::SimState;
use fnv_sim::FormId;
use std::collections::{HashMap, HashSet};

/// The [`SoundSource::key`] kinds used here.
const ADDON_KIND: u8 = 10;
const PROJECTILE_KIND: u8 = 11;
const IDLE_KIND: u8 = 12;

#[derive(Resource, Default)]
struct OwnSounds {
    /// Projectiles seen, by their number: whether they may still whiz (they were out of
    /// reach and have not whizzed yet).
    shots: HashMap<u64, bool>,
    addons: HashSet<Entity>,
    projectile_loops: HashSet<u64>,
    whizzes: u32,
    idles: HashSet<u32>,
}

fn distance2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Add-on nodes' sounds (see the module notes).
fn addons(
    mut st: ResMut<OwnSounds>,
    sfx: Option<Res<Sfx>>,
    root: Query<Entity, With<WorldRoot>>,
    emitters: Query<(Entity, &AddonEmitter, Option<&InheritedVisibility>)>,
    transforms: Query<&Transform>,
    parents: Query<&ChildOf>,
    mut out: MessageWriter<SoundSource>,
) {
    let (Some(sfx), Ok(root)) = (sfx, root.single()) else { return };
    for (e, em, v) in &emitters {
        if v.is_some_and(|v| !v.get()) {
            continue;
        }
        let Some(sound) = sfx.forms.addons.get(&em.index).and_then(|a| a.sound) else { continue };
        let at = crate::particles::game_affine(e, &transforms, &parents, root).translation;
        st.addons.insert(e);
        out.write(SoundSource { key: SoundSource::key(ADDON_KIND, e.to_bits() as u32), sound, at: at.to_array(), looping: false });
    }
}

/// Projectiles' sounds in flight (see the module notes).
fn projectiles(mut st: ResMut<OwnSounds>, sfx: Option<Res<Sfx>>, visuals: Res<ShotVisuals>, player: Option<Res<Player>>, mut out: MessageWriter<SoundSource>, mut sounds: MessageWriter<PlaySound>) {
    let (Some(sfx), Some(player)) = (sfx, player) else { return };
    let st = &mut *st;
    let eye = player.eye();
    let mut now: HashSet<u64> = HashSet::new();
    for v in &visuals.flying {
        now.insert(v.id);
        let Some(sound) = sfx.forms.projectiles.get(&v.projectile).and_then(|p| p.sound) else { continue };
        let Some(s) = sfx.forms.sounds.get(&sound) else { continue };
        if s.has("Loop") {
            st.projectile_loops.insert(v.id);
            out.write(SoundSource { key: SoundSource::key(PROJECTILE_KIND, v.id as u32), sound, at: v.pos, looping: false });
            continue;
        }
        let inside = distance2(v.pos, eye) < s.max_distance * s.max_distance;
        let may = st.shots.entry(v.id).or_insert(!inside);
        if *may && inside {
            *may = false;
            st.whizzes += 1;
            debug!("own sounds: projectile {} whizzes by: {}", v.id, s.edid.as_deref().unwrap_or("?"));
            sounds.write(PlaySound { sound, at: Some(v.pos) });
        }
    }
    st.shots.retain(|id, _| now.contains(id));
}

/// Actors' weapons' idle sounds (see the module notes).
fn idles(mut st: ResMut<OwnSounds>, sfx: Option<Res<Sfx>>, script: Option<Res<ScriptState>>, sim: Res<SimState>, held: Query<&HeldWeapon>, mut out: MessageWriter<SoundSource>) {
    let (Some(sfx), Some(rt)) = (sfx, script.as_deref().and_then(|s| s.rt.as_ref())) else { return };
    let mut done: HashSet<u32> = HashSet::new();
    for h in &held {
        if h.reference == FormId::PLAYER_REF.0 || !done.insert(h.reference) {
            continue;
        }
        let Some(sound) = sfx.forms.weapons.get(&h.weapon).and_then(|w| w.idle) else { continue };
        let me = FormId(h.reference);
        if !rt.game.extra.get(&me).is_some_and(|e| e.weapon_out) || rt.game.is_dead(me) {
            continue;
        }
        let Some(at) = sim.world.peek(me).map(|s| s.pos) else { continue };
        st.idles.insert(h.reference);
        out.write(SoundSource { key: SoundSource::key(IDLE_KIND, h.reference), sound, at, looping: true });
    }
}

fn report(mut exit: MessageReader<AppExit>, st: Res<OwnSounds>) {
    if exit.read().last().is_some() {
        info!(
            "own sounds summary: {} add-on nodes with a sound shown; {} projectiles with a looping sound, {} whizzed by; {} actors' weapons idled",
            st.addons.len(),
            st.projectile_loops.len(),
            st.whizzes,
            st.idles.len()
        );
    }
}

pub struct OwnSoundsPlugin;

impl Plugin for OwnSoundsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OwnSounds>().add_systems(Update, (addons, projectiles, idles)).add_systems(Last, report);
    }
}
