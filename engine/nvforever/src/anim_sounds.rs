//! The sounds animations and doors name (docs/spec-notes/audio.md §10), in `--play`:
//!
//! - **Clips' text keys.** Every frame, for the player's rig in view (`player_body.rs`:
//!   the first-person arms rig while it is shown, else the body) and every drawn actor
//!   (`locomotion.rs`), the keys each clip passed since the last frame are read
//!   (`fnv_actor::anim_sounds`). Only clips with at least half the weight of their
//!   layer are heard, so two clips crossfading or blended by direction step once
//!   [design]. A clip first seen part-way through is not heard until it next passes a
//!   key, except one just started.
//!   - `Sound: <editor ID>` plays that SOUN: reloads, swings, swimming, sitting down,
//!     an idle's claps.
//!   - `Enum: Left` / `Right` play a footstep for the ground under the feet: the
//!     Havok material of what a ray down from the feet hits (a placed object's
//!     triangle, or the landscape texture showing most at that point: LTEX HNAM),
//!     taken to an `FST<family><gait><side>` record. The gait is the clip's (its
//!     `Armor…`/`Foley…` keys, else its name). Swimming is water. Ground with no
//!     material (a placed object's box or capsule) is concrete; nothing under the
//!     feet, or the default landscape texture, is dirt [design].
//!   - `Enum: ArmorWalk` … `FoleySneak` play the rustle of the armour worn (ARMO SNAM,
//!     its own or its template's), at its chance.
//! - **Doors** that open in place (`nvf_play::doors`): the sound keys their sequences
//!   pass, else the DOOR record's open (SNAM) or close (ANAM) sound as they start.
//!
//! The player's own sounds play at the listener, the others where the actor or door
//! is. Actors more than [`ACTOR_RANGE`] from the player are not heard [design].
//! At exit: `anim sounds summary: …`.

use crate::doors::DoorRig;
use crate::limbs::ActorRig;
use crate::player_body::PlayerBody;
use crate::world::bevy_to_game;
use bevy::prelude::*;
use fnv_actor::anim_sounds::{
    armour_foley, clip_gait, footstep_candidates, footstep_family, foley_for, land_texture_material, parse, passed, worn_foley, FoleySound, KeySound,
};
use fnv_combat::rng::SplitMix;
use fnv_phys::{layers, Filter, Key, Owner};
use fnv_sim::FormId;
use fnv_world::grass::Coverage;
use fnv_world::land::{load_land, LandParts};
use nvf_assets::anim_blend::Mode;
use nvf_assets::anim_play::{BlendPlayer, Clip};
use nvf_play::doors::DoorSound;
use nvf_play::phys::PhysState;
use nvf_play::player::{Player, BodySet};
use nvf_play::sfx::{PlaySound, Sfx};
use nvf_play::sim::SimState;
use nvf_play::{Location, PlayWorld};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// Actors farther than this from the player's feet are not heard, game units [design:
/// most footstep records fade out well within it].
pub const ACTOR_RANGE: f32 = 3000.0;
/// A clip first seen less than this far into its time is heard from its start, seconds.
const JUST_STARTED: f32 = 0.1;
/// The share of its layer a clip needs to be heard.
const HEARD_WEIGHT: f32 = 0.5;
/// The shortest time between two footfalls (or two rustles) of one actor, seconds: two
/// movement clips crossfading can pass their keys a frame or two apart [design].
const MIN_STEP_GAP: f32 = 0.2;

/// Whose clips: the player's rig (true: the first-person one) or an actor's.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Who {
    Player(bool),
    Actor(Entity),
}

/// A clip playing in a role: who, the clip, and the role (player layer tag, or the
/// actor player's slot is not used: a clip moves between slots as the blend goes on).
type Playing = (Who, usize, u32);

#[derive(Default)]
struct Counts {
    footsteps_player: u32,
    footsteps_actors: u32,
    rustles: u32,
    reload_keys: u32,
    other_keys: u32,
    door_sounds: u32,
    door_record_sounds: u32,
    /// Keys naming a SOUN editor ID there is no record of.
    unknown_keys: u32,
    /// Footsteps by ground family.
    grounds: BTreeMap<&'static str, u32>,
}

#[derive(Resource)]
pub struct AnimSounds {
    by_edid: HashMap<String, FormId>,
    built: bool,
    /// Clip time last frame, by clip playing.
    last: HashMap<Playing, f32>,
    /// Armour sounds by the worn list (player) or the reference (actors).
    worn: HashMap<Vec<FormId>, Arc<Vec<FoleySound>>>,
    actor_worn: HashMap<u32, Arc<Vec<FoleySound>>>,
    /// Landscape: per exterior cell, its texture coverage; per LTEX its material.
    land: HashMap<(u32, i32, i32), Option<Arc<Coverage>>>,
    ltex: HashMap<u32, Option<u32>>,
    unknown: HashSet<String>,
    rng: SplitMix,
    counts: Counts,
    /// Seconds since the start, this frame; the last footfall and rustle of each.
    now: f32,
    steps: HashMap<(Who, bool), f32>,
}

impl Default for AnimSounds {
    fn default() -> Self {
        AnimSounds {
            by_edid: HashMap::new(),
            built: false,
            last: HashMap::new(),
            worn: HashMap::new(),
            actor_worn: HashMap::new(),
            land: HashMap::new(),
            ltex: HashMap::new(),
            unknown: HashSet::new(),
            rng: SplitMix(0x0a11_5ed5),
            counts: Counts::default(),
            now: 0.0,
            steps: HashMap::new(),
        }
    }
}

impl AnimSounds {
    /// Whether a footfall (`foot`) or rustle of `who` may sound now; marks it if so.
    fn step_due(&mut self, who: Who, foot: bool) -> bool {
        if self.steps.get(&(who, foot)).is_some_and(|&t| self.now - t < MIN_STEP_GAP) {
            return false;
        }
        self.steps.insert((who, foot), self.now);
        true
    }

    fn sound(&mut self, edid: &str) -> Option<FormId> {
        let s = self.by_edid.get(&edid.to_ascii_lowercase()).copied();
        if s.is_none() {
            self.counts.unknown_keys += 1;
            if self.unknown.insert(edid.to_string()) {
                debug!("anim sounds: no SOUN {edid}");
            }
        }
        s
    }
}

/// The SOUN editor IDs, once the sound forms are read.
fn build(s: &mut AnimSounds, sfx: &Sfx) {
    if s.built {
        return;
    }
    s.by_edid = sfx.forms.sounds.iter().filter_map(|(&id, snd)| Some((snd.edid.as_deref()?.to_ascii_lowercase(), id))).collect();
    s.built = true;
}

/// One heard key, with what it needs.
struct Heard<'a> {
    who: Who,
    clip: &'a Clip,
    key: KeySound<'a>,
}

/// The keys `clip` passed going to clip time `now` in role `playing`; `fresh` when the
/// clip may have just started. A clip played `once` that went back has been started
/// again (a round-by-round reload restarts its clip at `Hit` for the next round): heard
/// from its start, not round its end.
fn keys_of<'a>(last: &HashMap<Playing, f32>, seen: &mut Vec<(Playing, f32)>, playing: Playing, clip: &'a Clip, now: f32, once: bool) -> Vec<KeySound<'a>> {
    seen.push((playing, now));
    let from = match last.get(&playing) {
        Some(&t) if once && now < t => None,
        Some(&t) => Some(t),
        None if now - clip.start <= JUST_STARTED => None,
        None => return Vec::new(),
    };
    if from == Some(now) {
        return Vec::new();
    }
    passed(&clip.text_keys, clip.start, from, now).filter_map(parse).collect()
}

/// What is underfoot at `feet` (game space): the footstep family, stairs, and a name for
/// the report.
fn ground(s: &mut AnimSounds, phys: &mut PhysState, d: &PlayWorld, world: u32, feet: [f32; 3], exclude: Vec<Key>) -> (&'static str, bool) {
    let filter = Filter { exclude, exclude_layers: layers::character_ignores(), ..Default::default() };
    let from = [feet[0], feet[1], feet[2] + 30.0];
    let Some(hit) = phys.world.queries().ray(from, [0.0, 0.0, -1.0], 90.0, &filter) else { return ("Dirt", false) };
    match hit.owner {
        Owner::Terrain { x, y } => {
            let code = terrain_material(s, d, world, x, y, hit.point);
            code.map_or(("Dirt", false), footstep_family)
        }
        owner => {
            trace!("anim sounds: underfoot {owner:?} material {:?}", hit.material);
            hit.material.map_or(("ConcSolid", false), footstep_family)
        }
    }
}

/// The Havok material of the landscape texture showing most at `p` in exterior cell
/// (`x`, `y`); `None` for the default texture.
fn terrain_material(s: &mut AnimSounds, d: &PlayWorld, world: u32, x: i32, y: i32, p: [f32; 3]) -> Option<u32> {
    let cover = s
        .land
        .entry((world, x, y))
        .or_insert_with(|| {
            let cell = d.index.worldspaces.get(&world)?.grid.get(&(x, y))?;
            let land = d.index.cells.get(cell)?.land?;
            let parts = LandParts { heights: false, normals: false, colors: false, layers: true };
            load_land(d.lo, d.schema, land, parts).map(|l| Arc::new(Coverage::new(&l)))
        })
        .clone()?;
    let size = fnv_world::CELL_SIZE;
    let (lx, ly) = (p[0] - x as f32 * size, p[1] - y as f32 * size);
    let (best, share) = cover.textures().into_iter().map(|t| (t, cover.at(t, lx, ly))).fold((None, 0.0f32), |b, (t, a)| if a > b.1 { (Some(t), a) } else { b });
    // The default texture shows where the named ones leave more than any one of them.
    let best = best.filter(|_| share >= 1.0 - share)?;
    *s.ltex.entry(best).or_insert_with(|| land_texture_material(d.lo, d.schema, best))
}

/// The armour sounds of a worn list.
fn foley_of(lo: &fnv_esm::LoadOrder, schema: &fnv_esm::Schema, worn: &[FormId]) -> Vec<FoleySound> {
    worn_foley(worn.iter().map(|a| armour_foley(lo, schema, a.0)).collect())
}

#[allow(clippy::too_many_arguments)]
fn sound_heard(
    s: &mut AnimSounds,
    out: &mut MessageWriter<PlaySound>,
    phys: &mut Option<ResMut<PhysState>>,
    d: &PlayWorld,
    world: u32,
    h: Heard,
    at: Option<[f32; 3]>,
    feet: [f32; 3],
    exclude: Vec<Key>,
    foley: &[FoleySound],
    swimming: bool,
) {
    match h.key {
        KeySound::Sound(edid) => {
            if h.clip.name.to_ascii_lowercase().starts_with("reload") {
                s.counts.reload_keys += 1;
            } else {
                s.counts.other_keys += 1;
            }
            if let Some(sound) = s.sound(edid) {
                out.write(PlaySound { sound, at });
            }
        }
        KeySound::Foot(side) => {
            if !s.step_due(h.who, true) {
                return;
            }
            let gait = clip_gait(&h.clip.text_keys, &h.clip.name);
            let (family, stairs) = match (swimming, phys.as_deref_mut()) {
                (true, _) => ("Water", false),
                (false, Some(p)) => ground(s, p, d, world, feet, exclude),
                (false, None) => ("Dirt", false),
            };
            let Some(sound) = footstep_candidates(family, stairs, gait, side).iter().find_map(|n| s.by_edid.get(&n.to_ascii_lowercase()).copied()) else {
                s.counts.unknown_keys += 1;
                return;
            };
            match h.who {
                Who::Player(_) => s.counts.footsteps_player += 1,
                Who::Actor(_) => s.counts.footsteps_actors += 1,
            }
            *s.counts.grounds.entry(family).or_default() += 1;
            debug!("anim sounds: {:?} footstep {family}{} {:?} {side:?}", h.who, if stairs { " stairs" } else { "" }, gait);
            out.write(PlaySound { sound, at });
        }
        KeySound::Foley { armor, gait } => {
            let Some(f) = foley_for(foley, armor, gait) else { return };
            if !s.step_due(h.who, false) {
                return;
            }
            if f.chance < 100 && s.rng.next_f32() * 100.0 >= f32::from(f.chance) {
                return;
            }
            s.counts.rustles += 1;
            debug!("anim sounds: {:?} rustle {} {gait:?}", h.who, if armor { "armour" } else { "cloth" });
            out.write(PlaySound { sound: FormId(f.sound), at });
        }
    }
}

/// The player's rig in view.
#[allow(clippy::too_many_arguments)]
fn player_keys(
    time: Res<Time>,
    mut s: ResMut<AnimSounds>,
    sfx: Option<Res<Sfx>>,
    body: Option<Res<PlayerBody>>,
    player: Option<Res<Player>>,
    mut phys: Option<ResMut<PhysState>>,
    d: Res<PlayWorld>,
    location: Res<Location>,
    mut out: MessageWriter<PlaySound>,
    mut seen: Local<Vec<(Playing, f32)>>,
) {
    let (Some(sfx), Some(body), Some(player)) = (sfx, body, player) else { return };
    let s = &mut *s;
    build(s, &sfx);
    s.now = time.elapsed_secs();
    let Some((a, first, worn)) = body.heard() else { return };
    let foley = match s.worn.get(worn) {
        Some(f) => f.clone(),
        None => {
            let f = Arc::new(foley_of(d.lo, d.schema, worn));
            s.worn.insert(worn.to_vec(), f.clone());
            f
        }
    };
    let who = Who::Player(first);
    seen.clear();
    let mut heard: Vec<(&Clip, KeySound)> = Vec::new();
    for l in a.layers.iter().filter(|l| l.weight * l.ease >= HEARD_WEIGHT) {
        let playing = (who, Arc::as_ptr(&l.clip) as usize, l.tag);
        for k in keys_of(&s.last, &mut seen, playing, &l.clip, l.sample_time(a.phase), l.mode == Mode::Once) {
            heard.push((&l.clip, k));
        }
    }
    s.last.retain(|(w, _, _), _| !matches!(w, Who::Player(_)));
    s.last.extend(seen.drain(..));
    let swimming = phys.as_deref().and_then(|p| p.player.as_ref()).is_some_and(|c| c.swimming);
    let exclude: Vec<Key> = phys.as_deref().and_then(|p| p.player.as_ref()).map(|c| c.key()).into_iter().collect();
    let mut stepped = HashSet::new();
    for (clip, key) in heard {
        // One footfall a side a frame, whatever the clips.
        if let KeySound::Foot(side) = key {
            if !stepped.insert(side) {
                continue;
            }
        }
        sound_heard(s, &mut out, &mut phys, &d, location.world, Heard { who, clip, key }, None, player.feet, exclude.clone(), &foley, swimming);
    }
}

/// The drawn actors near the player.
#[allow(clippy::too_many_arguments)]
fn actor_keys(
    time: Res<Time>,
    mut s: ResMut<AnimSounds>,
    sfx: Option<Res<Sfx>>,
    player: Option<Res<Player>>,
    mut phys: Option<ResMut<PhysState>>,
    d: Res<PlayWorld>,
    location: Res<Location>,
    actor_world: Option<Res<crate::actors::ActorWorld>>,
    actors: Query<(Entity, &BlendPlayer, &GlobalTransform, Option<&ActorRig>)>,
    mut out: MessageWriter<PlaySound>,
    mut seen: Local<Vec<(Playing, f32)>>,
) {
    let (Some(sfx), Some(player)) = (sfx, player) else { return };
    let s = &mut *s;
    build(s, &sfx);
    s.now = time.elapsed_secs();
    seen.clear();
    let listener = Vec3::from_array(player.feet);
    for (e, bp, gt, rig) in &actors {
        let feet = bevy_to_game(gt.translation());
        if feet.distance(listener) > ACTOR_RANGE {
            continue;
        }
        let who = Who::Actor(e);
        let mut heard: Vec<(&Clip, KeySound)> = Vec::new();
        for l in bp.layers.iter().flatten().filter(|l| l.weight >= HEARD_WEIGHT) {
            let clip = &l.clip.clip;
            let now = clip.start + l.phase.clamp(0.0, 1.0) * clip.length();
            for k in keys_of(&s.last, &mut seen, (who, Arc::as_ptr(clip) as usize, 0), clip, now, false) {
                heard.push((clip, k));
            }
        }
        if heard.is_empty() {
            continue;
        }
        let reference = rig.map(|r| r.reference);
        let foley = match (reference, actor_world.as_deref()) {
            (Some(r), Some(w)) => s
                .actor_worn
                .entry(r)
                .or_insert_with(|| {
                    let worn = rig.and_then(|g| w.resolve(g.base, r).ok()).map(|a| a.worn).unwrap_or_default();
                    Arc::new(foley_of(d.lo, d.schema, &worn))
                })
                .clone(),
            _ => Arc::new(Vec::new()),
        };
        let exclude: Vec<Key> = reference.map(|r| vec![Key::Actor(r), Key::Character(r)]).unwrap_or_default();
        let swimming = reference.and_then(|r| phys.as_deref().and_then(|p| p.npcs.get(&r))).is_some_and(|n| n.ch.swimming);
        let mut stepped = HashSet::new();
        for (clip, key) in heard {
            if let KeySound::Foot(side) = key {
                if !stepped.insert(side) {
                    continue;
                }
            }
            let at = feet.to_array();
            sound_heard(s, &mut out, &mut phys, &d, location.world, Heard { who, clip, key }, Some(at), at, exclude.clone(), &foley, swimming);
        }
    }
    s.last.retain(|(w, _, _), _| matches!(w, Who::Player(_)));
    s.last.extend(seen.drain(..));
}

/// Doors' sounds, where the door is drawn.
fn door_sounds(
    mut s: ResMut<AnimSounds>,
    sfx: Option<Res<Sfx>>,
    mut doors: MessageReader<DoorSound>,
    rigs: Query<(&DoorRig, &GlobalTransform)>,
    sim: Option<Res<SimState>>,
    d: Res<PlayWorld>,
    mut out: MessageWriter<PlaySound>,
) {
    let Some(sfx) = sfx else {
        doors.clear();
        return;
    };
    let s = &mut *s;
    build(s, &sfx);
    for m in doors.read() {
        let at = rigs.iter().find(|(r, _)| r.form_id == m.door).map(|(_, g)| bevy_to_game(g.translation()).to_array());
        let sound = match &m.key {
            Some(edid) => {
                let found = s.sound(edid);
                s.counts.door_sounds += u32::from(found.is_some());
                found
            }
            None => {
                let base = sim.as_deref().and_then(|sim| sim.refs.get(FormId(m.door))).map(|p| p.base.0);
                let found = base.and_then(|b| door_record_sound(&d, b, m.opening));
                s.counts.door_record_sounds += u32::from(found.is_some());
                found
            }
        };
        if let Some(sound) = sound {
            debug!("anim sounds: door 0x{:08X} {} {:?}", m.door, if m.opening { "opens" } else { "closes" }, m.key);
            out.write(PlaySound { sound, at });
        }
    }
}

/// A DOOR record's open (SNAM) or close (ANAM) sound [documented: xEdit
/// wbDefinitionsFNV.pas DOOR, `schemas/fnv/objects.json`].
fn door_record_sound(d: &PlayWorld, base: u32, opening: bool) -> Option<FormId> {
    let (pi, r) = fnv_world::fields::winner(d.lo, base)?;
    let f = fnv_world::fields::Fields::new(r, pi, d.schema).ok()?;
    if &f.sig != b"DOOR" {
        return None;
    }
    f.formid(if opening { b"SNAM" } else { b"ANAM" }, d.lo).map(FormId)
}

fn report(mut exit: MessageReader<AppExit>, s: Res<AnimSounds>) {
    if exit.read().last().is_none() {
        return;
    }
    let c = &s.counts;
    let grounds: Vec<String> = c.grounds.iter().map(|(k, n)| format!("{k} {n}")).collect();
    let mut unknown: Vec<&String> = s.unknown.iter().collect();
    unknown.sort();
    info!(
        "anim sounds summary: {} footsteps ({} by the player, {} by actors; ground: {}), {} armour rustles, {} reload keys, {} other sound keys, {} door sounds ({} from door records), {} keys without a SOUN{}",
        c.footsteps_player + c.footsteps_actors,
        c.footsteps_player,
        c.footsteps_actors,
        if grounds.is_empty() { "-".to_string() } else { grounds.join(", ") },
        c.rustles,
        c.reload_keys,
        c.other_keys,
        c.door_sounds + c.door_record_sounds,
        c.door_record_sounds,
        c.unknown_keys,
        if unknown.is_empty() { String::new() } else { format!(" ({})", unknown.iter().take(8).map(|u| u.as_str()).collect::<Vec<_>>().join(", ")) }
    );
}

/// Added by nvforever with `--play`.
pub struct AnimSoundsPlugin;

impl Plugin for AnimSoundsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AnimSounds>()
            .add_systems(
                Update,
                (player_keys.after(BodySet), actor_keys.after(crate::locomotion::drive), door_sounds.after(nvf_play::doors::DoorSystems)).chain(),
            )
            .add_systems(Last, report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_actor::anim_sounds::Side;

    fn clip(keys: &[(f32, &str)]) -> Clip {
        Clip {
            name: "Forward".into(),
            start: 0.0,
            stop: 1.0,
            frequency: 1.0,
            cycle_type: 0,
            tracks: Vec::new(),
            priorities: Vec::new(),
            skipped: 0,
            accum_root: None,
            text_keys: keys.iter().map(|&(t, k)| (t, k.to_string())).collect(),
            visibility: Vec::new(),
        }
    }

    #[test]
    fn a_clip_is_heard_from_where_it_was_seen() {
        let c = clip(&[(0.0, "Sound: A"), (0.4, "Enum: Left"), (0.9, "Enum: Right")]);
        let mut seen = Vec::new();
        let p: Playing = (Who::Player(true), 1, 10);
        // Just started: heard from its start.
        let last = HashMap::new();
        assert_eq!(keys_of(&last, &mut seen, p, &c, 0.05, false), vec![KeySound::Sound("A")]);
        // First seen part-way: nothing until the next frame.
        assert!(keys_of(&last, &mut seen, p, &c, 0.5, false).is_empty());
        let last: HashMap<Playing, f32> = [(p, 0.3)].into();
        assert_eq!(keys_of(&last, &mut seen, p, &c, 0.5, false), vec![KeySound::Foot(Side::Left)]);
        // Round the end of a loop.
        let last: HashMap<Playing, f32> = [(p, 0.8)].into();
        assert_eq!(keys_of(&last, &mut seen, p, &c, 0.1, false), vec![KeySound::Sound("A"), KeySound::Foot(Side::Right)]);
        // Held at the end: nothing again.
        let last: HashMap<Playing, f32> = [(p, 1.0)].into();
        assert!(keys_of(&last, &mut seen, p, &c, 1.0, false).is_empty());
        // A clip played once, started again part-way (the next round of a reload): heard
        // from its start, not round its end.
        let last: HashMap<Playing, f32> = [(p, 0.6)].into();
        assert_eq!(keys_of(&last, &mut seen, p, &c, 0.05, true), vec![KeySound::Sound("A")]);
    }
}
