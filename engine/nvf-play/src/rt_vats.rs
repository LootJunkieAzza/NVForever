//! Real-time VATS (docs/spec-notes/realtime-vats.md): in a co-op game the VATS key locks
//! on to a hostile instead of slowing the clock, which one player cannot do to the
//! others. The rules and every default are `fnv_combat::rt_vats`'s (the settings file of
//! the RealtimeVatsNV mod the owner asked for, queue item 196); this is the game's side.
//!
//! - **When** ([`RtVats::active`]): in a co-op session (`allowed`, set by the co-op
//!   link, for the host and every guest), or solo with the V.A.T.S. style Real time
//!   (`crate::vats_classic::VatsStyle::RealTime`); `bEnabled` true either way. The old
//!   switch "Real-time VATS in co-op" (`realtime-vats`) is retired: co-op is always real
//!   time, and an old INI's value is read and ignored ([`load_config`]). Otherwise V is VATS mode (`crate::vats_mode`), whose V this takes over
//!   while active [design: the fork owner 2026-11-12, "it should be based on the real time
//!   vats mod but just using the new limb darwing technique instaead of the diamond", and
//!   "only if solo vats mode is selected as real time vats"].
//! - **Held** ([`update`]): the view stays on the locked limb while the player moves
//!   (`crate::vats_classic::turn_onto`), the mouse does not turn it meanwhile
//!   (`crate::player`), and Aim (as bound: the right mouse button, LT) moves to the next
//!   target instead of aiming down the sights (`crate::control`) [design: the owner
//!   2026-11-12, "vats should lock the aim to the currently selected target" and "make
//!   switch targets the ads button"]. V lets go, and B on a controller.
//! - **The limb** is lit with VATS's hatched limb highlight (nvforever `vats_highlight`,
//!   [`RtVats::highlighted`]) in place of the mod's mark [design: the owner, above].
//! - **Lock** ([`read_keys`], [`update`]): V (the VATS control), or X (`iKeyLock`) when a
//!   hostile can be locked (otherwise X is still the fork's Weapon Bash), locks on to the
//!   visible hostile nearest the view within `fMaxRange` and `fMaxAngle` whose hit chance
//!   is at least `fMinChanceToLock`; the same key lets go. C (`iKeyNext`) moves to the
//!   next target, Z (`iKeyLimbNext`) to the next limb the target has (torso, head, arms,
//!   legs), F (`iKeyCrit`) arms the critical once the meter is full. While locked those
//!   keys are the lock's alone (`crate::bindings::FreedKeys`).
//! - **Targets**: the living actors drawn now that are hostile to the player (fighting
//!   it, the host's hostile set on a guest, or would attack it: the compass's rule),
//!   seen along a ray from the eye ([`in_sight`]), looked for every
//!   `fLosInterval`. A locked target out of sight for `fLosLostSeconds` is let go.
//! - **Shots** (`crate::combat::LockLink`): a gun's shot while locked is rolled on the
//!   aimed limb's VATS chance (`fnv_combat::vats`, no higher than `fMaxChance`) and
//!   leaves straight at the limb, or wide of it. Each costs the weapon's VATS cost for a
//!   round times `fShotCostMult`; holding the lock costs `fLockDrainPerSecond`; no
//!   Action Points come back while it is held; at none it drops.
//! - **Kills**: the lock chains to the nearest visible hostile within `fChainMaxRange`
//!   and `fChainMaxAngle`.
//! - **Criticals**: hits on the target charge the meter; armed, the next hit deals
//!   `fCritDamageMult` times its damage. The player's hits are critical no other way.
//! - **The HUD** ([`draw`]): a mark on the aimed limb and a panel beside it with the
//!   target's name, the limb, the chance, the limb's condition and the critical meter,
//!   in the HUD's font and colour.
//! - **Settings**: `[RealtimeVats]` of NVForever.ini under the mod's names (written with
//!   the defaults when the section is missing). Solo picks it as the V.A.T.S. style Real
//!   time; the old switch `realtime-vats` is retired.
//!
//! Each player's lock is their own: a guest's shots and hits go to the host as its other
//! shots do (`crate::combat`'s relayed hits carry the damage dealt), so nothing of the
//! lock is sent.
//!
//! Log lines (the two-window check reads them): `realtime vats: lock on …`,
//! `realtime vats: refused: …`, `realtime vats: target …`, `realtime vats: limb …`,
//! `realtime vats: shot …`, `realtime vats: hit …`, `realtime vats: chain …`,
//! `realtime vats: critical armed`, `realtime vats: lock off (…)`.

use bevy::ecs::system::SystemParam;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_combat::rt_vats::{pick_chain, pick_cycle, pick_lock, Config, CritMeter, Limb, Seen, Sight};
use fnv_combat::vats::{final_chance, raw_chance, HitInputs, SIZE_TORSO, WHOLE_TO_HIT};
use fnv_game::actor_values::ACTION_POINTS;
use fnv_sim::FormId;
use nvf_assets::GameData;

use crate::activate::HudMessage;
use crate::aim_assist::{angle_off, sight_filter, TORSO_ABOVE_FEET};
use crate::bindings::{dik_key, Control, ControlInput, FreedKeys};
use crate::combat::{Combat, LockEvent};
use crate::control::{ControlLocks, PlayerControl};
use crate::hitvol::{ActorParts, HitVolumes};
use crate::interface::{amber, at as box_at, bc_supported, face as font_face, plain_font, set_line, Face};
use crate::phys::PhysState;
use crate::player::Player;
use crate::script::{with_world, ScriptState};
use crate::sfx::{PlaySound, Sfx};
use crate::sim::SimState;
use crate::vats_mode::{player_ap, point_of};
use crate::{space, PlaySet, PlayState, PlayerCamera};

/// The INI section the settings are kept in, under the mod's names.
pub const SECTION: &str = "RealtimeVats";
/// The retired settings page's switch (an old INI may still have it).
pub const SETTING_KEY: &str = "realtime-vats";

/// The lock held now.
#[derive(Debug, Clone)]
struct Lock {
    target: u32,
    name: String,
    limb: Limb,
    /// The hit volume part of the limb (`None`: the whole actor, one without parts).
    part: Option<u8>,
    limb_name: String,
    /// The limb's point now, game space.
    point: [f32; 3],
    /// The chance a shot is rolled on, percent.
    chance: f32,
    /// The limb's condition, 0 to 1 (`None`: the limb has none).
    condition: Option<f32>,
    sight: Sight,
    /// Shots and Action Points since it began (for the log).
    shots: u32,
    spent: f32,
    /// The view is held on the limb (`crate::vats_classic::turn_onto`); a new lock swings
    /// onto it first.
    held: bool,
}

/// What was pressed this frame.
#[derive(Debug, Clone, Copy, Default)]
struct Keys {
    /// V: lock, or let go.
    vats: bool,
    /// X: lock when a target can be, or let go.
    lock: bool,
    next: bool,
    previous: bool,
    limb_next: bool,
    limb_previous: bool,
    crit: bool,
}

/// What the run did (logged at exit).
#[derive(Debug, Clone, Copy, Default)]
pub struct Stats {
    pub locks: u32,
    pub refused: u32,
    pub chains: u32,
    pub shots: u32,
    pub hits: u32,
    pub crits: u32,
    pub ap_spent: f32,
}

/// The real-time VATS lock.
#[derive(Resource)]
pub struct RtVats {
    pub cfg: Config,
    /// In a co-op session: set by the co-op link.
    pub allowed: bool,
    /// Solo with the V.A.T.S. style Real time (`crate::vats_classic::VatsStyle::RealTime`).
    pub solo: bool,
    /// The other players' bodies (set by the co-op link): never a target.
    pub fellow_players: Vec<u32>,
    lock: Option<Lock>,
    crit: CritMeter,
    keys: Keys,
    /// The hostiles seen at the last look, on the limb a new lock would aim at.
    seen: Vec<Seen>,
    /// Seconds to the next look.
    look_in: f32,
    /// The sound FormIDs by Editor ID.
    sounds: Option<std::collections::HashMap<String, FormId>>,
    pub stats: Stats,
}

impl Default for RtVats {
    fn default() -> Self {
        RtVats { cfg: Config::default(), allowed: false, solo: false, fellow_players: Vec::new(), lock: None, crit: CritMeter::default(), keys: Keys::default(), seen: Vec::new(), look_in: 0.0, sounds: None, stats: Stats::default() }
    }
}

impl RtVats {
    /// Whether the VATS key is the lock's now.
    pub fn active(&self) -> bool {
        self.cfg.enabled && (self.allowed || self.solo)
    }

    /// The limb VATS's hatched highlight lights (nvforever `vats_highlight`): the locked
    /// target and its limb's body part (`None`: the whole of a target without parts), with
    /// `bLimbHighlight` on.
    pub fn highlighted(&self) -> Option<(u32, Option<u8>)> {
        self.lock.as_ref().filter(|_| self.cfg.limb_highlight).map(|l| (l.target, l.part))
    }

    /// The locked actor.
    pub fn locked(&self) -> Option<u32> {
        self.lock.as_ref().map(|l| l.target)
    }

    /// Locked on `target`'s torso, for other modules' tests (`player::mouse_look`).
    #[cfg(test)]
    pub(crate) fn lock_for_test(&mut self, target: u32) {
        self.lock = Some(Lock { target, name: String::new(), limb: Limb::Torso, part: None, limb_name: String::new(), point: [0.0; 3], chance: 50.0, condition: None, sight: Sight::default(), shots: 0, spent: 0.0, held: false });
    }

    /// Whether the Action Points' regeneration is held: a lock is held and
    /// `bNoRegenWhileLocked` is on.
    pub fn holds_regen(&self) -> bool {
        self.lock.is_some() && self.cfg.no_regen_while_locked
    }

    /// The critical meter, 0 to 1, and whether the next hit is armed.
    pub fn crit(&self) -> (f32, bool) {
        (self.crit.charge, self.crit.armed)
    }

    fn sound(&mut self, sfx: Option<&Sfx>, edid: &str) -> Option<FormId> {
        let sfx = sfx?;
        let by = self.sounds.get_or_insert_with(|| sfx.forms.sounds.iter().filter_map(|(&id, s)| s.edid.clone().map(|e| (e, id))).collect());
        by.get(edid).copied()
    }
}

/// The interface sounds (the base game's `UIVATS*` records, as VATS mode's).
#[derive(SystemParam)]
struct Sounds<'w> {
    sfx: Option<Res<'w, Sfx>>,
    play: MessageWriter<'w, PlaySound>,
}

impl Sounds<'_> {
    fn play(&mut self, rt: &mut RtVats, edid: &str) {
        if let Some(sound) = rt.sound(self.sfx.as_deref(), edid) {
            self.play.write(PlaySound { sound, at: None });
        }
    }
}

/// The keys the lock has taken from the game's controls this frame: all of its keys
/// while it is held (`bFreeGameKeys`), and the lock key on the press that will start a
/// lock (so X does not also bash). Before the frame's input is read.
fn free_keys(rt: Res<RtVats>, keys: Res<ButtonInput<KeyCode>>, mut freed: ResMut<FreedKeys>) {
    let c = &rt.cfg;
    let mut want = Vec::new();
    if rt.active() && c.free_game_keys {
        if rt.lock.is_some() {
            want.extend([c.key_lock, c.key_next, c.key_previous, c.key_limb_next, c.key_limb_previous, c.key_crit].into_iter().filter(|k| *k != 0).filter_map(dik_key));
        } else if let Some(k) = dik_key(c.key_lock).filter(|k| !rt.seen.is_empty() && keys.just_pressed(*k)) {
            want.push(k);
        }
    }
    if freed.0 != want {
        freed.0 = want;
    }
}

/// The controls' part of the lock's keys this frame (`pressed`: the control was pressed,
/// as bound): Aim moves to the next target while a lock is held; V locks or lets go.
pub fn lock_presses(locked: bool, pressed: impl Fn(Control) -> bool) -> (bool, bool) {
    (locked && pressed(Control::Block), pressed(Control::Vats))
}

/// What the player pressed for the lock this frame.
fn read_keys(input: ControlInput, state: Res<State<PlayState>>, mut rt: ResMut<RtVats>, style: Option<Res<crate::vats_classic::VatsStyle>>) {
    let solo = style.is_some_and(|s| *s == crate::vats_classic::VatsStyle::RealTime);
    if rt.solo != solo {
        rt.solo = solo;
        info!("realtime vats: {}", if solo { "the V.A.T.S. style is Real time: V is the lock" } else { "the V.A.T.S. style is not Real time" });
    }
    let playing = *state.get() == PlayState::Playing;
    let (aim_next, vats) = lock_presses(rt.lock.is_some(), |c| playing && input.just_pressed(c));
    // A controller's B lets go of a held lock.
    let pad_back = playing && rt.lock.is_some() && input.controller() && input.pads.iter().any(|g| g.just_pressed(GamepadButton::East));
    let live = *state.get() == PlayState::Playing && !input.suppressed() && !input.hold.as_ref().is_some_and(|h| h.all) && !input.controller();
    let raw = |code: u8| live && code != 0 && dik_key(code).is_some_and(|k| input.keys.just_pressed(k) && !input.blocked.as_ref().is_some_and(|b| b.keys.contains(&k)));
    let c = &rt.cfg;
    let keys = if rt.active() {
        Keys {
            // (Read past the lock's own hold on the keys: V is never one of them.)
            vats: vats || pad_back,
            lock: raw(c.key_lock),
            next: raw(c.key_next) || aim_next,
            previous: raw(c.key_previous),
            limb_next: raw(c.key_limb_next),
            limb_previous: raw(c.key_limb_previous),
            crit: raw(c.key_crit),
        }
    } else {
        Keys::default()
    };
    rt.keys = keys;
}

/// How far off the view an actor that is not hostile may be for V to lock on to it,
/// degrees [design: about a body's width at talking distance].
pub const CROSSHAIR_DEG: f32 = 4.0;

/// A hostile in view and where a limb of it is.
#[derive(Debug, Clone, Copy)]
struct Aimed {
    part: Option<u8>,
    point: [f32; 3],
    /// The part's BPTD To Hit Chance and its size against a torso's.
    base: f32,
    size: f32,
}

/// The hit volume part of `limb` on `actor` and where it is: the first posed part whose
/// body part lowers the limb's condition. A torso the actor's parts do not name is the
/// whole actor.
fn limb_of(combat: &Combat, parts: &ActorParts, phys: &PhysState, actor: u32, limb: Limb, feet: Option<[f32; 3]>) -> Option<Aimed> {
    let posed = parts.by_ref.get(&actor);
    let mut tried = Vec::new();
    for s in posed.map(|p| p.segments.as_slice()).unwrap_or_default() {
        if s.part == crate::hitvol::WEAPON_PART || tried.contains(&s.part) {
            continue;
        }
        tried.push(s.part);
        let Some(bp) = combat.vats_part(parts, actor, Some(s.part)) else { continue };
        if bp.actor_value.as_deref().and_then(fnv_game::combat::limb_condition) == Some(limb.condition()) {
            let point = point_of(phys, actor, Some(s.part)).unwrap_or([(s.a[0] + s.b[0]) / 2.0, (s.a[1] + s.b[1]) / 2.0, (s.a[2] + s.b[2]) / 2.0]);
            return Some(Aimed { part: Some(s.part), point, base: bp.to_hit_chance as f32, size: fnv_combat::vats::part_size(bp.actor_value.as_deref()) });
        }
    }
    if limb != Limb::Torso {
        return None;
    }
    let point = point_of(phys, actor, None).or(feet.map(|f| [f[0], f[1], f[2] + TORSO_ABOVE_FEET]))?;
    Some(Aimed { part: None, point, base: WHOLE_TO_HIT, size: SIZE_TORSO })
}

/// The chance to hit `aimed` from `eye`, percent, before the lock's cap: the VATS chance
/// of the part (`fnv_combat::vats`), with `perks` (the player's to-hit perks) on it.
fn vats_chance(combat: &Combat, skill: f32, eye: [f32; 3], aimed: &Aimed, perks: impl Fn(f32) -> f32) -> f32 {
    let distance = crate::combat::distance(eye, aimed.point);
    let hit = &combat.rules.vats_hit;
    let raw = raw_chance(&HitInputs { base: aimed.base, size: aimed.size, skill, distance, cone_deg: Some(combat.vats_cone.unwrap_or(0.0)) }, hit);
    final_chance(raw, perks, None, hit)
}

/// The world the lock looks at.
#[derive(SystemParam)]
struct Look<'w> {
    combat: Option<ResMut<'w, Combat>>,
    parts: Res<'w, ActorParts>,
    vols: Option<Res<'w, HitVolumes>>,
    sim: ResMut<'w, SimState>,
    script: Option<ResMut<'w, ScriptState>>,
    phys: ResMut<'w, PhysState>,
    player: ResMut<'w, Player>,
}

/// The living hostiles seen from the eye now, each on `limb` (the torso for those
/// without it), with the chance a shot at it would have.
///
/// A living actor that is not hostile is among them, marked so (`false`), only while it
/// is under the crosshair (within [`CROSSHAIR_DEG`] of the view): V locks on to it, as
/// VATS may target anyone; X, the target key and a chain never take it. The actor a lock
/// is held on (`held`) is looked for wherever it stands.
fn look_round(look: &mut Look, cfg: &Config, limb: Limb, skill: f32, held: Option<u32>, fellows: &[u32]) -> Vec<(Seen, Aimed, bool)> {
    let Some(combat) = look.combat.as_deref() else { return Vec::new() };
    let eye = look.player.eye();
    let dir = look.player.look_dir();
    let reach = cfg.max_range.max(cfg.chain_max_range);
    let widest = cfg.max_angle.max(cfg.chain_max_angle).to_radians();
    let near: Vec<(u32, Aimed, f32, f32)> = look
        .vols
        .as_deref()
        .map(|v| v.living.as_slice())
        .unwrap_or_default()
        .iter()
        .filter(|(id, _)| !fellows.contains(id))
        .filter_map(|&(id, feet)| {
            let aimed = limb_of(combat, &look.parts, &look.phys, id, limb, Some(feet)).or_else(|| limb_of(combat, &look.parts, &look.phys, id, Limb::Torso, Some(feet)))?;
            let (angle, distance) = angle_off(eye, dir, aimed.point);
            (held == Some(id) || (distance <= reach && angle <= widest)).then_some((id, aimed, angle, distance))
        })
        .collect();
    if near.is_empty() {
        return Vec::new();
    }
    // Hostile and alive: the compass's rule (a guest has the host's hostile set).
    let sim = &mut *look.sim;
    let foes: Vec<(u32, Aimed, f32, f32, bool)> = match look.script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
        Some(rt) => with_world(rt, sim, |rt| {
            near.into_iter()
                .filter_map(|(id, aimed, angle, distance)| {
                    let (r, g) = (FormId(id), &mut rt.game);
                    let fighting = g.awareness.get(r).and_then(|x| x.combat_target()).is_some_and(|(t, _)| t == FormId::PLAYER_REF);
                    let hostile = fighting || g.coop_hostile.contains(&r) || g.would_attack(r, FormId::PLAYER_REF);
                    (!g.is_dead(r) && (hostile || held == Some(id) || angle <= CROSSHAIR_DEG.to_radians())).then_some((id, aimed, angle, distance, hostile))
                })
                .collect()
        }),
        None => Vec::new(),
    };
    let p = &mut *look.phys;
    let filter = sight_filter(p.player.as_ref().map(|c| c.key()), p.npcs.keys().copied());
    let q = p.world.queries();
    foes.into_iter()
        .filter(|(_, aimed, _, distance, _)| !cfg.line_of_sight || in_sight(&q, &filter, eye, aimed.point, *distance))
        .map(|(id, aimed, angle, distance, hostile)| {
            let chance = cfg.shot_chance(vats_chance(combat, skill, eye, &aimed, |c| c));
            (Seen { id, distance, angle: angle.to_degrees(), chance }, aimed, hostile)
        })
        .collect()
}

/// Whether `point` is seen from `eye`: nothing of the world lies on the ray to it. Another
/// actor's body in the way does not hide it [design: in a pack the one behind is still
/// seen past the others; the aim assist's rule, which wants the target struck first, let
/// go of a coyote in a pack of twelve after 1.6 s].
fn in_sight(q: &fnv_phys::Queries<'_>, filter: &fnv_phys::Filter, eye: [f32; 3], point: [f32; 3], distance: f32) -> bool {
    let to = [point[0] - eye[0], point[1] - eye[1], point[2] - eye[2]];
    match q.ray(eye, to, distance, filter) {
        Some(h) => matches!(h.owner, fnv_phys::Owner::Actor { .. } | fnv_phys::Owner::Character { .. }),
        None => true,
    }
}

/// The Action Points a round of the weapon in hand costs under the lock, and its name;
/// `Err` says why the lock cannot be used with what is in hand.
fn weapon_cost(look: &mut Look, cfg: &Config) -> Result<(f32, String), &'static str> {
    const NEEDS_GUN: &str = "Real-time VATS needs a gun in hand.";
    let combat = look.combat.as_deref().ok_or(NEEDS_GUN)?;
    if combat.gun().is_none() {
        return Err(NEEDS_GUN);
    }
    let w = combat.vats_weapon(&look.sim, look.phys.world.params.gravity).map_err(|_| NEEDS_GUN)?;
    if matches!(w.style, fnv_combat::vats::AttackStyle::Thrown | fnv_combat::vats::AttackStyle::Melee) {
        return Err(NEEDS_GUN);
    }
    // The Action Point Cost perks, as VATS mode's marks (perk-entry-points.md §2).
    let (cost, _) = crate::combat::player_weapon_perk(look.script.as_deref_mut(), &mut look.sim, fnv_game::perks::ACTION_POINT_COST, w.id, w.cost);
    Ok((cfg.shot_cost(cost.max(0.0) / w.rounds_per_shot.max(1) as f32), w.edid))
}

fn spend(look: &mut Look, amount: f32) {
    if amount <= 0.0 {
        return;
    }
    let Some(rt) = look.script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    with_world(rt, &mut look.sim, |rt| fnv_game::functions::add_actor_value_damage(&mut rt.game, FormId::PLAYER_REF, ACTION_POINTS, f64::from(amount)));
}

/// A new lock on `seen`, aimed at `aimed` (already the limb `limb`, or the torso).
fn lock_on(look: &mut Look, seen: Seen, aimed: Aimed, limb: Limb) -> Lock {
    let name = look.sim.base_of(seen.id).and_then(|b| look.sim.name(b).map(str::to_string)).unwrap_or_else(|| "?".into());
    let limb = if aimed.part.is_none() { Limb::Torso } else { limb };
    let mut lock = Lock { target: seen.id, name, limb, part: aimed.part, limb_name: limb.name().to_string(), point: aimed.point, chance: seen.chance, condition: None, sight: Sight::default(), shots: 0, spent: 0.0, held: false };
    name_limb(look, &mut lock);
    lock
}

/// The limb's own name (the target's body part data's) and whether the limb the lock
/// holds is really the one its part is (a torso fallback).
fn name_limb(look: &Look, lock: &mut Lock) {
    let named = look.combat.as_deref().and_then(|c| c.vats_part(&look.parts, lock.target, lock.part)).and_then(|b| b.name);
    lock.limb_name = named.unwrap_or_else(|| lock.limb.name().to_string());
}

/// The lock's frame: the keys, the look round, the Action Points, the kill's chain, and
/// what combat is asked for the frame's shots and hits. Before `crate::combat::fire`.
#[allow(clippy::too_many_arguments)]
fn update(
    time: Res<Time>,
    state: Res<State<PlayState>>,
    mut rt: ResMut<RtVats>,
    mut look: Look,
    control: Res<PlayerControl>,
    locks: Res<ControlLocks>,
    mut hud: ResMut<HudMessage>,
    mut sounds: Sounds,
) {
    let rt = &mut *rt;
    let dt = time.delta_secs();
    let keys = std::mem::take(&mut rt.keys);
    let cfg = rt.cfg.clone();
    // What combat did since the last frame.
    let events = look.combat.as_deref_mut().map(|c| std::mem::take(&mut c.lock.events)).unwrap_or_default();
    if let Some(c) = look.combat.as_deref_mut() {
        c.lock.crit_now = None;
    }
    let mut shots_cost = 0.0;
    let per_shot = if events.is_empty() { 0.0 } else { weapon_cost(&mut look, &cfg).map_or(0.0, |(c, _)| c) };
    for e in events {
        match e {
            LockEvent::Shot { chance, rolled, hit } => {
                rt.stats.shots += 1;
                shots_cost += per_shot;
                if let Some(l) = rt.lock.as_mut() {
                    l.shots += 1;
                    info!("realtime vats: shot {} at {} {:08X} ({}): {chance:.0}% to hit: {} (rolled {rolled:.0}), {per_shot:.1} AP", l.shots, l.name, l.target, l.limb_name, if hit { "hit" } else { "missed" });
                }
            }
            LockEvent::Hit { dealt, critical } => {
                rt.stats.hits += 1;
                match critical {
                    Some(m) => {
                        // (The pellets of one shot share the critical: the meter empties once.)
                        if rt.crit.armed {
                            rt.crit.hit(&cfg);
                            rt.stats.crits += 1;
                        }
                        info!("realtime vats: hit for {dealt:.1}: critical, x{m:.1} damage; the meter is empty");
                    }
                    None => {
                        let before = rt.crit.charge;
                        rt.crit.hit(&cfg);
                        info!("realtime vats: hit for {dealt:.1}; critical meter {:.0}% -> {:.0}%{}", before * 100.0, rt.crit.charge * 100.0, if rt.crit.full() && before < 1.0 - 1e-4 { ": full" } else { "" });
                        if rt.crit.full() && before < 1.0 - 1e-4 {
                            sounds.play(rt, "UIVATSReady");
                        }
                    }
                }
            }
        }
    }
    if !rt.active() {
        if rt.lock.is_some() {
            end(rt, &mut look, "real-time VATS went off", &mut sounds);
        }
        publish(rt, &mut look);
        return;
    }
    let playing = *state.get() == PlayState::Playing;
    let (dead, held) = look.script.as_deref().and_then(|s| s.rt.as_ref()).map_or((false, false), |r| (r.game.is_dead(FormId::PLAYER_REF), r.game.in_menu()));
    let skill = match look.combat.as_deref() {
        Some(c) => c.player_weapon_skill(look.script.as_deref_mut(), &mut look.sim),
        None => 0.0,
    };
    // The look round: every `fLosInterval`, and at once on a key that needs one.
    rt.look_in -= dt;
    let wants_look = keys.vats || keys.lock || keys.next || keys.previous;
    let target_dead = rt.lock.as_ref().is_some_and(|l| crate::combat::is_dead(look.script.as_deref(), l.target));
    let looked = rt.look_in <= 0.0 || wants_look || target_dead;
    let mut found: Vec<(Seen, Aimed, bool)> = Vec::new();
    if looked {
        let since = cfg.los_interval - rt.look_in.min(cfg.los_interval);
        rt.look_in = cfg.los_interval;
        let limb = rt.lock.as_ref().map_or(cfg.limb, |l| l.limb);
        found = if playing && !dead { look_round(&mut look, &cfg, limb, skill, rt.locked(), &rt.fellow_players.clone()) } else { Vec::new() };
        rt.seen = found.iter().filter(|f| f.2).map(|(s, ..)| *s).filter(|s| s.distance <= cfg.max_range && s.angle <= cfg.max_angle && cfg.lockable(s.chance)).collect();
        // The locked target's sight.
        if let Some(l) = rt.lock.as_mut().filter(|_| !target_dead) {
            let seen = found.iter().any(|(s, ..)| s.id == l.target);
            if l.sight.check(seen, since, &cfg) {
                let why = format!("{} out of sight for {:.1} s", l.name, l.sight.unseen);
                end(rt, &mut look, &why, &mut sounds);
            }
        }
    }
    // The hostiles (what X, the target key and a chain choose among), and with them the
    // actor under the crosshair that is not one (V's).
    let all: Vec<Seen> = found.iter().filter(|f| f.2).map(|(s, ..)| *s).collect();
    let any: Vec<Seen> = found.iter().map(|(s, ..)| *s).collect();
    let aimed_of = |id: u32| found.iter().find(|(s, ..)| s.id == id).map(|(_, a, _)| *a);
    // What ends the lock whatever it is doing.
    if rt.lock.is_some() {
        let stop = if !playing {
            Some("a menu opened")
        } else if dead {
            Some("the player died")
        } else if held {
            Some("a conversation or a script's menu began")
        } else if locks.fighting {
            Some("a script disabled fighting")
        } else if !control.weapon_out {
            Some("the weapon was put away")
        } else if look.combat.as_deref().is_none_or(|c| c.gun().is_none()) {
            Some("no gun in hand")
        } else {
            None
        };
        if let Some(why) = stop {
            end(rt, &mut look, why, &mut sounds);
        }
    }
    // The kill: chain to the nearest hostile in view, else the lock ends.
    if target_dead {
        if let Some(l) = rt.lock.clone() {
            match pick_chain(&all, l.target, &cfg).and_then(|s| aimed_of(s.id).map(|a| (s, a))) {
                Some((s, a)) => {
                    let mut next = lock_on(&mut look, s, a, l.limb);
                    (next.shots, next.spent) = (l.shots, l.spent);
                    rt.stats.chains += 1;
                    info!("realtime vats: chain: {} {:08X} killed, lock on {} {:08X} ({}), {:.0} units, {:.0} deg off the view, {:.0}% to hit", l.name, l.target, next.name, next.target, next.limb_name, s.distance, s.angle, s.chance);
                    rt.lock = Some(next);
                    sounds.play(rt, "UIVATSSelectTarget");
                }
                None => {
                    let why = format!("{} killed, no hostile to chain to within {:.0} units and {:.0} deg", l.name, cfg.chain_max_range, cfg.chain_max_angle);
                    end(rt, &mut look, &why, &mut sounds);
                }
            }
        }
    }
    // The keys.
    let ap = player_ap(look.script.as_deref_mut(), &mut look.sim).map_or(f32::INFINITY, |a| a.0);
    match rt.lock.clone() {
        Some(_) if keys.vats || keys.lock => end(rt, &mut look, "the lock key pressed again", &mut sounds),
        Some(l) => {
            if keys.next || keys.previous {
                match pick_cycle(&all, l.target, keys.next, &cfg).and_then(|s| aimed_of(s.id).map(|a| (s, a))) {
                    Some((s, a)) => {
                        let mut next = lock_on(&mut look, s, a, l.limb);
                        (next.shots, next.spent) = (l.shots, l.spent);
                        info!("realtime vats: target {} {:08X} ({}), {:.0} units, {:.0} deg off the view, {:.0}% to hit", next.name, next.target, next.limb_name, s.distance, s.angle, s.chance);
                        rt.lock = Some(next);
                        sounds.play(rt, "UIVATSSelectTarget");
                    }
                    None => info!("realtime vats: no other target to move to"),
                }
            }
            if keys.limb_next || keys.limb_previous {
                if let (Some(lock), Some(combat)) = (rt.lock.as_mut(), look.combat.as_deref()) {
                    let has = |limb: Limb| limb_of(combat, &look.parts, &look.phys, lock.target, limb, None).is_some_and(|a| a.part.is_some() || limb == Limb::Torso);
                    let to = lock.limb.cycled(if keys.limb_next { 1 } else { -1 }, has);
                    if let Some(a) = limb_of(combat, &look.parts, &look.phys, lock.target, to, None).filter(|_| to != lock.limb) {
                        (lock.limb, lock.part, lock.point) = (to, a.part, a.point);
                        name_limb(&look, lock);
                        // The chance now, on the new limb (below).
                        info!("realtime vats: limb {} of {} {:08X}", lock.limb_name, lock.name, lock.target);
                        sounds.play(rt, "UIVATSSelectTargetPart");
                    } else {
                        info!("realtime vats: {} {:08X} has no other limb to aim at", lock.name, lock.target);
                    }
                }
            }
            if keys.crit {
                if rt.crit.arm() {
                    info!("realtime vats: critical armed: the next hit deals x{:.1} damage", cfg.crit_damage_mult);
                    sounds.play(rt, "UIVATSReady");
                } else if !rt.crit.armed {
                    info!("realtime vats: critical not armed: the meter is at {:.0}%", rt.crit.charge * 100.0);
                }
            }
        }
        // (X with nothing to lock is not the lock's: it is the fork's Weapon Bash.)
        None if keys.vats || (keys.lock && !rt.seen.is_empty()) => {
            let refuse = |rt: &mut RtVats, why: &str, hud: &mut HudMessage, sounds: &mut Sounds| {
                info!("realtime vats: refused: {why}");
                hud.show(why.to_string());
                rt.stats.refused += 1;
                sounds.play(rt, "UIVATSEnterFail");
            };
            let weapon = weapon_cost(&mut look, &cfg);
            if !playing || dead || held || locks.fighting {
                refuse(rt, "Fighting is not possible now.", &mut hud, &mut sounds);
            } else if !control.weapon_out {
                refuse(rt, "Real-time VATS needs the weapon out.", &mut hud, &mut sounds);
            } else if let Err(why) = weapon {
                refuse(rt, why, &mut hud, &mut sounds);
            } else if !cfg.can_start(ap) {
                refuse(rt, "Not enough Action Points.", &mut hud, &mut sounds);
            } else {
                match pick_lock(if keys.vats { &any } else { &all }, &cfg).and_then(|s| aimed_of(s.id).map(|a| (s, a))) {
                    Some((s, a)) => {
                        let lock = lock_on(&mut look, s, a, cfg.limb);
                        let (per, edid) = weapon.unwrap_or_default();
                        rt.stats.locks += 1;
                        info!(
                            "realtime vats: lock on {} {:08X} ({}), {:.0} units, {:.0} deg off the view, {:.0}% to hit; {edid}, {per:.1} AP a shot, {:.1} AP a second, {} AP",
                            lock.name,
                            lock.target,
                            lock.limb_name,
                            s.distance,
                            s.angle,
                            s.chance,
                            cfg.hold_cost(1.0),
                            if ap.is_finite() { format!("{ap:.0}") } else { "unlimited".into() }
                        );
                        rt.lock = Some(lock);
                        sounds.play(rt, "UIVATSEnter");
                    }
                    None => {
                        let why = format!("No hostile in view within {:.0} units.", cfg.max_range);
                        refuse(rt, &why, &mut hud, &mut sounds);
                    }
                }
            }
        }
        None => {}
    }
    // The held lock: where the limb is, its chance and condition, and the Action Points.
    if let Some(mut l) = rt.lock.take() {
        let eye = look.player.eye();
        if let Some(p) = point_of(&look.phys, l.target, l.part) {
            l.point = p;
        }
        // The view stays on the limb while the player moves (the module notes).
        if playing {
            let (heading, pitch) = crate::control::Crosshair::aim_at(control.crosshair.as_ref(), eye, l.point);
            let was = l.held;
            let p = &mut *look.player;
            ((p.heading, p.pitch), l.held) = crate::vats_classic::turn_onto((p.heading, p.pitch), (heading, pitch), dt, was);
            if l.held && !was {
                info!("realtime vats: the view is held on {} {:08X} ({})", l.name, l.target, l.limb_name);
            }
        }
        if let Some(combat) = look.combat.as_deref() {
            let aimed = limb_of(combat, &look.parts, &look.phys, l.target, l.limb, None);
            let (weapon, target) = (combat.player_weapon(), FormId(l.target));
            let condition = (l.part.is_some()).then(|| l.limb.condition());
            // The player's to-hit perks see the shot as VATS mode's marks do (§17.3).
            let shot = weapon.map(|w| fnv_game::vats::VatsShot { weapon: w, target, distance: f64::from(crate::combat::distance(eye, l.point)), condition, success: false, height: f64::from(l.point[2] - look.player.feet[2]) });
            let sim = &mut *look.sim;
            let (perks, health) = match (shot, look.script.as_deref_mut().and_then(|s| s.rt.as_mut())) {
                (Some(shot), Some(r)) => with_world(r, sim, |r| (Some(r.game.vats_hit_perks(shot)), condition.and_then(|c| fnv_game::functions::actor_value(&mut r.game, target, c).ok()))),
                _ => (None, None),
            };
            if let Some(a) = aimed {
                l.chance = cfg.shot_chance(vats_chance(combat, skill, eye, &a, |c| perks.as_ref().map_or(c, |p| p.apply(f64::from(c)) as f32)));
            }
            l.condition = health.map(|h| (h as f32 / 100.0).clamp(0.0, 1.0));
        }
        let cost = cfg.hold_cost(dt) + shots_cost;
        spend(&mut look, cost);
        l.spent += cost;
        rt.stats.ap_spent += cost;
        rt.lock = Some(l);
        if cfg.out_of_ap(ap - cost) {
            end(rt, &mut look, "out of Action Points", &mut sounds);
            hud.show("Out of Action Points.".to_string());
        }
    } else if shots_cost > 0.0 {
        // (The last shots of a lock that ended this frame.)
        spend(&mut look, shots_cost);
        rt.stats.ap_spent += shots_cost;
    }
    publish(rt, &mut look);
}

/// Let go of the lock.
fn end(rt: &mut RtVats, _look: &mut Look, why: &str, sounds: &mut Sounds) {
    let Some(l) = rt.lock.take() else { return };
    info!("realtime vats: lock off ({why}; {} shots, {:.1} AP spent on {} {:08X})", l.shots, l.spent, l.name, l.target);
    sounds.play(rt, "UIVATSExit");
}

/// Tell combat what the frame's shots and hits are asked (`crate::combat::LockLink`).
fn publish(rt: &RtVats, look: &mut Look) {
    let Some(c) = look.combat.as_deref_mut() else { return };
    let lock = rt.lock.as_ref();
    c.lock.target = lock.map(|l| FormId(l.target));
    c.lock.aim = lock.map(|l| (l.point, l.chance));
    c.lock.no_crits = rt.active() && rt.cfg.no_natural_crits;
    c.lock.crit_armed = lock.filter(|_| rt.crit.armed).map(|_| rt.cfg.crit_damage_mult);
}

/// The settings from `[RealtimeVats]` of the INI; a missing section is written with the
/// defaults so they can be found and changed.
fn load_config(file: Option<Res<crate::settings::SettingsFile>>, mut rt: ResMut<RtVats>) {
    let Some(path) = file.and_then(|f| f.0.clone()) else { return };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    // The retired switch "Real-time VATS in co-op": read, told, ignored.
    if let Some(v) = fnv_install::ini_get(&text, crate::settings::SECTION, SETTING_KEY) {
        info!("realtime vats: [{}] {SETTING_KEY}={v} is retired and ignored: co-op is always real time", crate::settings::SECTION);
    }
    let (cfg, bad) = Config::read(|name| fnv_install::ini_get(&text, SECTION, name));
    for name in &bad {
        warn!("realtime vats: [{SECTION}] {name} is not a value of its kind: the default is used");
    }
    if fnv_install::ini_get(&text, SECTION, "bEnabled").is_none() {
        let with = cfg.entries().into_iter().fold(text.clone(), |t, (k, v)| fnv_install::ini_set(&t, SECTION, k, &v));
        match fnv_install::guard_write(&path, None).map_err(|e| e.to_string()).and_then(|()| std::fs::write(&path, with).map_err(|e| e.to_string())) {
            Ok(()) => info!("realtime vats: [{SECTION}] written to {} with the defaults", path.display()),
            Err(e) => warn!("realtime vats: [{SECTION}] not written to {}: {e}", path.display()),
        }
    }
    if cfg != Config::default() {
        info!("realtime vats: settings read from [{SECTION}] of {}", path.display());
    }
    rt.cfg = cfg;
}

// ---- The HUD ----

/// The mark on the aimed limb: a square on its corner.
#[derive(Component)]
struct LimbMark;

/// The panel beside the mark; `shown` is what its children now show.
#[derive(Component, Default)]
struct Panel {
    shown: String,
}

/// The HUD's font, as VATS mode's labels (`crate::vats_mode`).
const FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";
/// The mark's side, a share of the window's width, and its bars' thickness, percent of it.
const MARK_SIZE: f32 = 0.022;
const MARK_BAR: f32 = 8.0;
/// The panel's lines' baselines and the bars, interface units from its top.
const NAME_BASE: f32 = 28.0;
const LIMB_BASE: f32 = 62.0;
const BAR_WIDTH: f32 = 170.0;
const BAR_HEIGHT: f32 = 9.0;
const LIMB_BAR_TOP: f32 = 74.0;
const CRIT_BAR_TOP: f32 = 92.0;
const CRIT_BASE: f32 = 134.0;
const HINT_BASE: f32 = 168.0;
/// The panel's height, for centring it on the mark.
const PANEL_HEIGHT: f32 = 174.0;

fn spawn_hud(mut commands: Commands) {
    commands.spawn((Node { position_type: PositionType::Absolute, ..default() }, Visibility::Hidden, GlobalZIndex(5), LimbMark, UiTransform::from_rotation(Rot2::degrees(45.0)))).with_children(|p| {
        let bar = |left: f32, top: f32, width: f32, height: f32| Node { position_type: PositionType::Absolute, left: Val::Percent(left), top: Val::Percent(top), width: Val::Percent(width), height: Val::Percent(height), ..default() };
        for node in [bar(0.0, 0.0, 100.0, MARK_BAR), bar(0.0, 100.0 - MARK_BAR, 100.0, MARK_BAR), bar(0.0, 0.0, MARK_BAR, 100.0), bar(100.0 - MARK_BAR, 0.0, MARK_BAR, 100.0)] {
            p.spawn((node, BackgroundColor(amber(255.0)), MarkBar));
        }
    });
    commands.spawn((Node { position_type: PositionType::Absolute, ..default() }, Visibility::Hidden, GlobalZIndex(5), Panel::default()));
}

/// A bar of the limb mark (recoloured with the HUD).
#[derive(Component)]
struct MarkBar;

/// What the panel shows: the lines and the two meters.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelText {
    pub name: String,
    pub limb: String,
    /// The limb's condition in sixteenths (`None`: no meter).
    pub condition: Option<u8>,
    /// The critical meter in sixteenths, and its line.
    pub crit: u8,
    pub crit_line: String,
    /// The keys: "Target [Right Mouse]  Return [V]", as bound.
    pub hints: String,
}

/// The panel's line of keys: Aim's for the next target, V's to let go, on the device in
/// use, as bound.
pub fn hint_line(b: &crate::bindings::Bindings) -> String {
    use crate::bindings::{key_name, mouse_name, InputDevice};
    let pad = b.device == InputDevice::Controller;
    // A mouse button first (Aim is the right button and Left Alt by default: the button is
    // what the owner names), else the key; a controller's button.
    let name = |c: Control| {
        if pad {
            return b.pad(c).map_or_else(|| "?".to_string(), |p| crate::pad::pad_shown(p).to_string());
        }
        let k = b.get(c);
        k.mouse.map(mouse_name).or_else(|| k.key.and_then(key_name).map(String::from)).unwrap_or_else(|| "?".to_string())
    };
    let back = if pad { crate::pad::pad_shown(crate::pad::PadInput::Button(GamepadButton::East)).to_string() } else { name(Control::Vats) };
    format!("Target [{}]  Return [{back}]", name(Control::Block))
}

/// The panel for a lock: `chance` in percent, `condition` and `charge` 0 to 1.
#[allow(clippy::too_many_arguments)]
pub fn panel_text(name: &str, limb: &str, chance: f32, condition: Option<f32>, charge: f32, armed: bool, crit_key: &str, hints: &str) -> PanelText {
    let sixteenths = |v: f32| (v.clamp(0.0, 1.0) * 16.0).round() as u8;
    let crit_line = if armed {
        "CRITICAL ARMED".to_string()
    } else if charge >= 1.0 - 1e-4 {
        format!("{crit_key}) CRITICAL")
    } else {
        String::new()
    };
    PanelText { name: name.to_string(), limb: format!("{limb} {}", crate::vats_mode::chance_text(chance)), condition: condition.map(sixteenths), crit: sixteenths(charge), crit_line, hints: hints.to_string() }
}

/// Put the mark on the aimed limb and the panel beside it.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    mut commands: Commands,
    rt: Res<RtVats>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut face_: Local<Option<Face>>,
    mut marks: Query<(&mut Node, &mut Visibility), (With<LimbMark>, Without<Panel>)>,
    mut bars: Query<&mut BackgroundColor, With<MarkBar>>,
    mut panels: Query<(Entity, &mut Panel, &mut Node, &mut Visibility), Without<LimbMark>>,
    bindings: Option<Res<crate::bindings::Bindings>>,
) {
    let (width, height) = windows.iter().next().map_or((1280.0, 720.0), |w| (w.width(), w.height()));
    let lock = rt.lock.as_ref();
    let at = lock.zip(cameras.single().ok()).and_then(|(l, (cam, t))| cam.world_to_viewport(t, space::to_bevy(l.point)).ok());
    let size = (MARK_SIZE * width).max(10.0);
    let colour = amber(255.0);
    for (mut node, mut vis) in &mut marks {
        // The limb is lit with VATS's hatched highlight (`RtVats::highlighted`), not marked.
        let want = Visibility::Hidden;
        if *vis != want {
            *vis = want;
        }
        if let Some(at) = at {
            (node.left, node.top, node.width, node.height) = (Val::Px(at.x - size / 2.0), Val::Px(at.y - size / 2.0), Val::Px(size), Val::Px(size));
        }
    }
    for mut bar in &mut bars {
        if bar.0 != colour {
            bar.0 = colour;
        }
    }
    let unit = height / 960.0;
    for (e, mut panel, mut node, mut vis) in &mut panels {
        let want = if at.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        let (Some(at), Some(l)) = (at, lock) else { continue };
        node.left = Val::Px(at.x + size / 2.0 + rt.cfg.panel_gap * unit);
        node.top = Val::Px(at.y - PANEL_HEIGHT / 2.0 * unit);
        let key = dik_key(rt.cfg.key_crit).and_then(crate::bindings::key_name).unwrap_or("?");
        let hints = bindings.as_deref().map(hint_line).unwrap_or_default();
        let text = panel_text(&l.name, &l.limb_name, l.chance, l.condition, rt.crit.charge, rt.crit.armed, key, &hints);
        let shown = format!("{text:?}{colour:?}");
        if panel.shown == shown {
            continue;
        }
        let f = face_.get_or_insert_with(|| font_face(FONT, plain_font(36.0, 15.0), data.as_deref(), &mut images, bc_supported(formats.as_deref())));
        commands.entity(e).despawn_related::<Children>().with_children(|p| {
            for (line, base) in [(&text.name, NAME_BASE), (&text.limb, LIMB_BASE), (&text.crit_line, CRIT_BASE), (&text.hints, HINT_BASE)] {
                if let Some(line) = f.font.lines(line, None).into_iter().next().filter(|_| !line.is_empty()) {
                    set_line(p, f, &line, 0.0, base, colour);
                }
            }
            let mut meter = |top: f32, sixteenths: u8| {
                p.spawn((box_at(0.0, top, BAR_WIDTH, BAR_HEIGHT), BackgroundColor(amber(70.0))));
                p.spawn((box_at(0.0, top, BAR_WIDTH * f32::from(sixteenths) / 16.0, BAR_HEIGHT), BackgroundColor(colour)));
            };
            if let Some(c) = text.condition {
                meter(LIMB_BAR_TOP, c);
            }
            meter(CRIT_BAR_TOP, text.crit);
        });
        panel.shown = shown;
    }
}

fn report(mut exit: MessageReader<AppExit>, rt: Res<RtVats>) {
    if exit.read().next().is_none() {
        return;
    }
    let s = rt.stats;
    if s.locks == 0 && s.refused == 0 {
        return;
    }
    info!("realtime vats summary: {} locks, {} chains, {} refused, {} shots, {} hits, {} criticals, {:.1} AP spent", s.locks, s.chains, s.refused, s.shots, s.hits, s.crits, s.ap_spent);
}

pub struct RtVatsPlugin;

impl Plugin for RtVatsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RtVats>()
            .init_resource::<FreedKeys>()
            .add_systems(Startup, (load_config, spawn_hud))
            .add_systems(Update, (free_keys, read_keys).chain().before(PlaySet::Input))
            .add_systems(Update, update.in_set(PlaySet::Activation).after(crate::vats_mode::VatsUpdate).before(crate::combat::fire))
            .add_systems(Update, draw.after(PlaySet::Activation))
            .add_systems(Last, report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lock_takes_the_vats_key_in_co_op_or_with_the_real_time_style() {
        let mut rt = RtVats::default();
        assert!(!rt.active(), "solo keeps VATS mode");
        // Solo with the V.A.T.S. style Real time: the lock.
        rt.solo = true;
        assert!(rt.active(), "solo, the style Real time");
        rt.cfg.enabled = false;
        assert!(!rt.active(), "bEnabled off turns it off everywhere");
        (rt.solo, rt.cfg.enabled) = (false, true);
        rt.allowed = true;
        assert!(rt.active(), "co-op is always real time");
        rt.cfg.enabled = false;
        assert!(!rt.active());
    }

    #[test]
    fn the_panel_names_the_limb_the_chance_and_the_critical_key() {
        let t = panel_text("Powder Ganger", "Head", 61.6, Some(0.5), 0.4, false, "F", "");
        assert_eq!((t.name.as_str(), t.limb.as_str(), t.condition, t.crit, t.crit_line.as_str()), ("Powder Ganger", "Head 62%", Some(8), 6, ""));
        let full = panel_text("Gecko", "Torso", 95.0, None, 1.0, false, "F", "");
        assert_eq!((full.condition, full.crit, full.crit_line.as_str()), (None, 16, "F) CRITICAL"));
        assert_eq!(panel_text("Gecko", "Torso", 95.0, None, 1.0, true, "F", "").crit_line, "CRITICAL ARMED");
    }

    /// The owner, 2026-11-12: "make switch targets the ads button". Aim as bound moves to
    /// the next target while a lock is held; rebound, the switch follows it; with no lock
    /// Aim is the player's own.
    #[test]
    fn aim_switches_targets_as_bound() {
        use crate::bindings::{Bindings, Control, Input};
        fn press(b: &Bindings, m: MouseButton) -> impl Fn(Control) -> bool + '_ {
            move |c: Control| b.get(c).mouse == Some(m)
        }
        let b = Bindings::game_defaults();
        let rmb = b.get(Control::Block).mouse.expect("Aim is on a mouse button by default");
        assert_eq!(lock_presses(true, press(&b, rmb)), (true, false), "Aim: the next target");
        assert_eq!(lock_presses(false, press(&b, rmb)), (false, false), "no lock: Aim aims");
        // Aim moved to the middle button: the switch goes with it, the right button is no longer it.
        let mut moved = Bindings::game_defaults();
        moved.rebind(Control::Block, Input::Mouse(MouseButton::Middle));
        assert!(lock_presses(true, press(&moved, MouseButton::Middle)).0);
        assert!(!lock_presses(true, press(&moved, rmb)).0, "the right button is another control's now");
        // V lets go.
        let v = |c: Control| c == Control::Vats;
        assert_eq!(lock_presses(true, v), (false, true));
    }

    #[test]
    fn the_hints_name_the_keys_as_bound() {
        use crate::bindings::{Bindings, Control, Input, InputDevice};
        let mut b = Bindings::game_defaults();
        let line = hint_line(&b);
        assert_eq!(line, format!("Target [{}]  Return [V]", crate::bindings::mouse_name(MouseButton::Right)), "the right button by default");
        b.rebind(Control::Block, Input::Mouse(MouseButton::Middle));
        assert_eq!(hint_line(&b), format!("Target [{}]  Return [V]", crate::bindings::mouse_name(MouseButton::Middle)));
        b.device = InputDevice::Controller;
        assert!(hint_line(&b).ends_with(&format!("Return [{}]", crate::pad::pad_shown(crate::pad::PadInput::Button(GamepadButton::East)))));
    }

    #[test]
    fn the_hatched_highlight_follows_the_lock() {
        let mut rt = RtVats::default();
        assert_eq!(rt.highlighted(), None);
        rt.lock = Some(Lock { target: 0x1234, name: "Gecko".into(), limb: Limb::Head, part: Some(1), limb_name: "Head".into(), point: [0.0; 3], chance: 50.0, condition: None, sight: Sight::default(), shots: 0, spent: 0.0, held: false });
        assert_eq!(rt.highlighted(), Some((0x1234, Some(1))));
        rt.cfg.limb_highlight = false;
        assert_eq!(rt.highlighted(), None, "bLimbHighlight off");
    }

    #[test]
    fn no_action_points_come_back_while_a_lock_is_held() {
        let mut rt = RtVats::default();
        assert!(!rt.holds_regen());
        rt.lock = Some(Lock { target: 1, name: String::new(), limb: Limb::Torso, part: None, limb_name: String::new(), point: [0.0; 3], chance: 50.0, condition: None, sight: Sight::default(), shots: 0, spent: 0.0, held: false });
        assert!(rt.holds_regen());
        rt.cfg.no_regen_while_locked = false;
        assert!(!rt.holds_regen());
    }
}
