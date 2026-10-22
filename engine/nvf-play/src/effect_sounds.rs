//! Magic effects' sounds (docs/spec-notes/audio.md §15.2) [documented: xEdit MGEF DATA
//! "Effect Sound", "Hit Sound"].
//!
//! - **Hit Sound** [hypothesis]: an effect starting on an actor plays it once at the
//!   actor (at the listener for the player); one with the Loop flag goes round at the
//!   actor instead, while the effect lasts.
//! - **Effect Sound** [hypothesis: by the name; none in FalloutNV.esm]: goes round at the
//!   actor while the effect lasts.
//! - The effects already running when the game is first seen are taken as started
//!   before: their hit sounds are not played [design].
//!
//! An `effect sounds summary` line at the end.

use crate::player::Player;
use crate::script::ScriptState;
use crate::sfx::{PlaySound, Sfx, SoundSource};
use crate::sim::SimState;
use crate::PlaySet;
use bevy::prelude::*;
use fnv_sim::FormId;
use std::collections::HashSet;

/// The [`SoundSource::key`] kinds of an effect's looping hit sound and its effect sound.
const HIT_LOOP_KIND: u8 = 8;
const EFFECT_LOOP_KIND: u8 = 9;

#[derive(Resource, Default)]
struct EffectSounds {
    /// The effects seen so far, by their id; `None` before the first look.
    seen: Option<HashSet<u32>>,
    hits: u32,
    hit_loops: HashSet<u32>,
    effect_loops: HashSet<u32>,
}

fn play(
    mut st: ResMut<EffectSounds>,
    script: Option<Res<ScriptState>>,
    sim: Res<SimState>,
    player: Option<Res<Player>>,
    sfx: Option<Res<Sfx>>,
    (mut sounds, mut loops): (MessageWriter<PlaySound>, MessageWriter<SoundSource>),
) {
    let (Some(rt), Some(sfx)) = (script.as_deref().and_then(|s| s.rt.as_ref()), sfx) else { return };
    let game = &rt.game;
    let st = &mut *st;
    let first = st.seen.is_none();
    let seen = st.seen.get_or_insert_with(HashSet::new);
    let mut now: HashSet<u32> = HashSet::new();
    for e in &game.effects {
        now.insert(e.id);
        let Some(m) = game.data.magic_effects.get(&e.effect) else { continue };
        let at = if e.target == FormId::PLAYER_REF {
            player.as_deref().map(Player::eye)
        } else {
            sim.world.peek(e.target).map(|s| s.pos).or_else(|| sim.refs.get(e.target).map(|p| p.pos))
        };
        let Some(at) = at else { continue };
        let started = seen.insert(e.id) && !first;
        if let Some(hit) = m.hit_sound {
            if sfx.forms.sounds.get(&hit).is_some_and(|s| s.has("Loop")) {
                st.hit_loops.insert(e.id);
                loops.write(SoundSource { key: SoundSource::key(HIT_LOOP_KIND, e.id), sound: hit, at, looping: false });
            } else if started {
                st.hits += 1;
                debug!("effect sounds: {} on {}: hit sound {hit}", e.effect, e.target);
                sounds.write(PlaySound { sound: hit, at: (e.target != FormId::PLAYER_REF).then_some(at) });
            }
        }
        if let Some(sound) = m.effect_sound {
            st.effect_loops.insert(e.id);
            loops.write(SoundSource { key: SoundSource::key(EFFECT_LOOP_KIND, e.id), sound, at, looping: true });
        }
    }
    seen.retain(|id| now.contains(id));
}

fn report(mut exit: MessageReader<AppExit>, st: Res<EffectSounds>) {
    if exit.read().last().is_some() {
        info!("effect sounds summary: {} hit sounds played; {} effects with a looping hit sound, {} with an effect sound", st.hits, st.hit_loops.len(), st.effect_loops.len());
    }
}

pub struct EffectSoundsPlugin;

impl Plugin for EffectSoundsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EffectSounds>().add_systems(Update, play.in_set(PlaySet::Sync)).add_systems(Last, report);
    }
}
