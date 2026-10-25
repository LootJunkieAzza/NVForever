//! How fast creatures move, and the stance they move in (docs/spec-notes/actors.md §8.10;
//! the owner's playtest 2026-10-16: cazadors moved far too slowly and their walk stopped
//! and started).
//!
//! - **Speed**: a creature walks at the root motion of its `Forward` clip and runs at
//!   that of its `FastForward` clip, times its scale, its Speed Mult (CREA ACBS) and
//!   its skeleton's share ([`skeleton_share`], [`CREATURE_SPEED_FIT`] for most). The AI
//!   moves it at those speeds (`fnv_ai::Ai::speeds`).
//!   People walk and run at `fnv_ai::NPC_WALK` and `NPC_RUN` times their body armour's
//!   share ([`person_pace`]), looked at again every second. Before this every actor
//!   walked at 86.8 and ran at 344 units/s: a cazador's walk clip (217 units/s) played
//!   at its slowest rate (0.4), and 344 is the point between its walk and run clips
//!   (the geometric mean of 217 and 546), so a small slow-down swapped the two clips back
//!   and forth: the stop-and-start.
//! - **Clip rate**: the clips play `Speed Mult × the walk share` faster
//!   (`Locomotion::pace`), as the movement comes from them, and the move is chosen for the
//!   velocity divided by that: a medium cazador walking at 379 units/s plays its walk clip
//!   at 1.75, not its run clip at 0.7.
//! - **Turning**: a creature turns at [`CREATURE_TURN_RATE`] × its scale, walking and
//!   standing, and turns on the spot only when its way is more than 60 degrees off; the
//!   people's 120 degrees a second and 20 degrees made a fast cazador stop at the start of
//!   every short walk.
//! - **Stance**: a creature whose folder has `h2h` movement clips (the cazador, the dog,
//!   the mirelurk king) moves on them while it is not in the Normal detection state
//!   (Alert, Combat, Lost: `locomotion::Locomotion::set_alert`), on its `mt` clips
//!   otherwise; the cazador scurries along the ground while calm and flies when alert
//!   [observed: the owner, 2026-10-16]. Its speeds follow the stance's clips.
//!
//! Speeds are given once the creature is drawn (its clips are then loaded), so one walking
//! off-screen before that moves at the people's speeds.

use crate::limbs::ActorRig;
use crate::locomotion::{ClipSet, Locomotion};
use crate::residents::Residents;
use bevy::prelude::*;
use fnv_actor::locomotion::Move;
use fnv_ai::{ActorSpeeds, Speeds};
use fnv_game::awareness::State;
use fnv_sim::FormId;
use nvf_play::script::ScriptState;

/// The share of the clip pace (root motion × scale × Speed Mult) a creature keeps when
/// its skeleton has none of its own ([`skeleton_share`]) [observed: the AI observer in
/// the original game, 2026-10-25, creatures spawned by PlaceAtMe in a flat test room and
/// placed ones, walking (docs/spec-notes/actors.md §8.10): dogs 0.98 (VCrTier1DogMed,
/// 45.1 against 57.4 × 0.8), coyotes 0.96–0.98, geckos 1.00 (49.9 against 62.5 × 0.8),
/// giant mantises 1.00, a bark scorpion 0.99 (65 against 74.9 × 0.35 × 2.5), an NCR dog
/// 0.91–1.15; leveled geckos charging at 230–290 fit a run of 1 too (578.6 at scale 0.5).
/// Hypothesis: the run keeps the walk's share where no run was measured].
pub const CREATURE_SPEED_FIT: f32 = 1.0;

/// A skeleton's shares of its clips' pace, walking and running.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Share {
    pub walk: f32,
    pub run: f32,
}

/// A skeleton's own shares of its clips' pace ([`CREATURE_SPEED_FIT`] for the others),
/// by its folder [observed: the AI observer in the original game]:
/// - cazadors (`nvcazadores`) 0.70 walking and running (2026-10-16: young ones at the
///   Tribal Village walked 228 against a clip pace of 325, medium ones 380 against 542;
///   charging, 576 and 974 against 818 and 1364);
/// - brahmin 1.4 (2026-10-24: the Novac pack brahmin 001650BD, scale 1.25, walked a
///   median 78 and ran about 330, twice what 0.70 gave; hypothesis: every brahmin);
/// - mole rats 0.80 walking, 0.96 running (2026-10-25: VCrTier1MoleRatMed walked 34.1
///   against 42.7; the placed mole rat 000BAC90 charged at a median 324 against 338.7);
/// - giant rats (`nvgiantrat`) 0.76 (VCrTier1GiantRatMed, scale 1.1 by its own record,
///   walked 46.9 against 56.0 × 1.1; hypothesis: the run the same);
/// - deathclaws 0.70 as before (VCrTier3DeathClawSmall, scale 0.75, walked about 0.59 of
///   120 × 0.75, unsteady: kept; hypothesis);
/// - feral ghouls (`ghoul`) 0.70 as before, under [`type_pace`]'s 1.25 (not measured: the
///   observer's ghoul jumped instead of walking).
pub fn skeleton_share(skeleton: &str) -> Share {
    let path = fnv_vfs::normalize(skeleton);
    let has = |folder: &str| path.contains(&format!("creatures\\{folder}\\"));
    let both = |v: f32| Share { walk: v, run: v };
    if has("nvcazadores") || has("deathclaw") || has("ghoul") {
        both(0.70)
    } else if has("brahmin") {
        both(1.4)
    } else if has("molerat") {
        Share { walk: 0.80, run: 0.96 }
    } else if has("nvgiantrat") {
        both(0.76)
    } else {
        both(CREATURE_SPEED_FIT)
    }
}

/// CREA DATA creature type code of feral ghouls [documented: xEdit wbCreatureTypeEnum,
/// 5 Feral Ghoul].
pub const FERAL_GHOUL: i64 = 5;

/// A creature type's own share on top of its skeleton's ([`skeleton_share`]): feral ghouls 1.25
/// [design: the owner's playtest list 7 item 36, "feral ghouls run too slow from a guess
/// want to move 25% faster"; the original could not be watched. With the fit a feral
/// ghoul (`creatures\ghoul`, Speed Mult 100, scale 1) ran 269 units/s, below the
/// player's 344, so a running player left every ghoul behind; with this share it runs
/// 336, about the player's pace (observed in NVForever: smoke `combat-ghoul-lunge`,
/// 2026-10-23)]; every other type 1.
pub fn type_pace(creature_type: Option<i64>) -> f32 {
    if creature_type == Some(FERAL_GHOUL) {
        1.25
    } else {
        1.0
    }
}

/// A creature's turn rate at scale 1, degrees per second, walking and standing; it
/// scales with the creature's size [observed: the same observations' headings, 0.16 s
/// apart: medium cazadors (scale 1) turned at up to 304 degrees a second, young ones
/// (scale 0.5) at up to 137 (300 × 0.5 = 150); hypothesis: the rule and other
/// creatures; CREA TNAM (720 and 90 on these records) does not match it].
pub const CREATURE_TURN_RATE: f32 = 300.0;

/// How far off its way a standing creature may be before it turns on the spot instead
/// of turning as it sets off, radians [design: the cazadors made 85-89% of their fast
/// turns while moving].
pub const CREATURE_SPOT_TURN_START: f32 = 60.0 * std::f32::consts::PI / 180.0;

/// Changes between walk and run clips logged, the first ones only.
const SWAPS_LOGGED: usize = 20;

pub struct CreaturePacePlugin;

impl Plugin for CreaturePacePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PaceStats>()
            .add_systems(Update, (give_speeds.before(crate::residents::drive), people_speeds.after(give_speeds).before(crate::residents::drive), stance.after(nvf_play::PlaySet::Scripts).before(crate::locomotion::walk_test)))
            .add_systems(Last, report);
    }
}

/// Marks an actor [`give_speeds`] has seen, and a creature among them.
#[derive(Component)]
struct Paced;
#[derive(Component)]
struct PacedCreature;

#[derive(Resource, Default)]
struct PaceStats {
    paced: usize,
    /// People whose Speed Mult is not 100 (among `paced`).
    npcs: usize,
    /// Speeds given to people for their armour.
    people: usize,
    alerted: usize,
    calmed: usize,
    /// Frames of paced creatures playing a movement clip, and changes between a walk
    /// and a run clip among them (the stop-and-start of a speed on the switch point).
    moving_frames: usize,
    swaps: usize,
    last: std::collections::HashMap<u32, bool>,
}

/// A clip set's walk and run speeds for a creature: its clips' root motion × `scale` ×
/// `speed_mult` (percent) × its skeleton's `share`; a move without a clip keeps
/// `default`'s speed.
pub fn clip_speeds(set: &ClipSet, scale: f32, speed_mult: f32, share: Share, default: Speeds) -> Speeds {
    let of = |m: Move, k: f32, d: f32| set.info(m).map(|i| i.speed).filter(|v| *v > 1.0).map_or(d, |v| v * scale * speed_mult / 100.0 * k);
    Speeds { walk: of(Move::Forward, share.walk, default.walk), run: of(Move::FastForward, share.run, default.run) }
}

/// The share of the people's speeds (`fnv_ai::NPC_WALK`, `NPC_RUN`) a person keeps for
/// its body armour (`fnv_game::combat::armor_speed`: medium × 0.925, heavy × 0.85
/// [observed, actors.md §8.10]), on top of its Speed Mult's (`npc_speeds`). [observed
/// against the Speed Mult part, one actor: Cook-Cook, Speed Mult 110, ran 255 in metal
/// armour with the Incinerator drawn, 344 × 0.75 = 258; with his Speed Mult 284; asked in
/// docs/owner-review.md.]
pub fn person_pace(game: &fnv_game::Game, world: &fnv_sim::World, id: FormId) -> f32 {
    fnv_game::combat::armor_speed(game, fnv_game::combat::worn_armor_class(world, id)) as f32
}

/// A drawn person and the share of the people's speeds it was last given.
#[derive(Component)]
struct PersonPace(f32);

/// How often a drawn person's armour is looked at again, seconds: scripts and the
/// person's own outfitting change what it wears [design].
const PERSON_PACE_EVERY: f32 = 1.0;

/// Drawn people walk and run at the people's speeds × [`person_pace`].
fn people_speeds(
    time: Res<Time>,
    mut wait: Local<f32>,
    mut script: Option<ResMut<ScriptState>>,
    mut res: Option<ResMut<Residents>>,
    sim: Option<Res<nvf_play::sim::SimState>>,
    mut people: Query<(&ActorRig, &mut PersonPace)>,
    mut stats: ResMut<PaceStats>,
) {
    *wait -= time.delta_secs();
    if *wait > 0.0 {
        return;
    }
    *wait = PERSON_PACE_EVERY;
    let (Some(res), Some(sim)) = (res.as_deref_mut(), sim) else { return };
    let default = res.default_speeds();
    let mut given = Vec::new();
    {
        let game = match script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
            Some(rt) => &mut rt.game,
            None => match res.own_game() {
                Some(g) => g,
                None => return,
            },
        };
        for (rig, mut pace) in &mut people {
            let id = FormId(rig.reference);
            if id == FormId::PLAYER_REF {
                continue;
            }
            let share = person_pace(game, &sim.world, id);
            if (share - pace.0).abs() < 1e-3 {
                continue;
            }
            pace.0 = share;
            // On top of its Speed Mult's share (`npc_speeds`).
            let f = share * speed_mult(game, FormId(rig.base), id) / 100.0;
            let s = Speeds { walk: default.walk * f, run: default.run * f };
            info!("person pace: {} {id} walks {:.0} and runs {:.0} units/s ({:.1}% of the people's speeds for its armour)", game.data.name(FormId(rig.base)), s.walk, s.run, share * 100.0);
            given.push((id, ActorSpeeds { calm: s, alert: s, turn_rate: None, spot_turn_start: None }));
        }
    }
    for (id, s) in given {
        res.set_speeds(id, s);
        stats.people += 1;
    }
}

fn give_speeds(
    mut commands: Commands,
    mut script: Option<ResMut<ScriptState>>,
    mut res: Option<ResMut<Residents>>,
    mut actors: Query<(Entity, &ActorRig, &mut Locomotion, &GlobalTransform), Without<Paced>>,
    mut stats: ResMut<PaceStats>,
) {
    let Some(res) = res.as_deref_mut() else { return };
    if actors.is_empty() {
        return;
    }
    let default = res.default_speeds();
    let mut given = Vec::new();
    {
        let game = match script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
            Some(rt) => &mut rt.game,
            None => match res.own_game() {
                Some(g) => g,
                None => return,
            },
        };
        for (e, rig, mut lo, global) in &mut actors {
            commands.entity(e).insert(Paced);
            let Some(def) = game.data.actors.get(&FormId(rig.base)) else { continue };
            if &def.sig != b"CREA" {
                if let Some(s) = npc_speeds(game, FormId(rig.base), FormId(rig.reference), default, &mut lo) {
                    given.push((FormId(rig.reference), s));
                    stats.npcs += 1;
                }
                // Its armour's share on top: `people_speeds`.
                commands.entity(e).insert(PersonPace(1.0));
                continue;
            }
            commands.entity(e).insert(PacedCreature);
            let id = FormId(rig.reference);
            // Speed Mult as GetBaseAV gives it: through the template chain (Stats), as the
            // leveled cazadors at the Tribal Village take theirs.
            let mult = speed_mult(game, FormId(rig.base), id);
            let scale = global.affine().transform_vector3(Vec3::Z).length().max(0.01);
            // Its type's share (feral ghouls) as if its Speed Mult were that much higher,
            // and its skeleton's shares. The clips play at the walk's share (a mole rat's
            // run clip plays 1.2 times its own pace).
            let mult = mult * type_pace(game.creature_type(id));
            let share = skeleton_share(&rig.skeleton);
            lo.pace = mult / 100.0 * share.walk;
            let calm = clip_speeds(&lo.stand, scale, mult, share, default);
            let alert = lo.alert.as_deref().map_or(calm, |a| clip_speeds(a, scale, mult, share, default));
            info!(
                "creature pace: {} {id} walks {:.0} and runs {:.0} units/s, turns {:.0} degrees/s{} (scale {scale:.2}, Speed Mult {mult:.0})",
                game.data.name(FormId(rig.base)),
                calm.walk,
                calm.run,
                CREATURE_TURN_RATE * scale,
                if lo.alert.is_some() { format!(", alert {:.0} and {:.0}", alert.walk, alert.run) } else { String::new() }
            );
            let turn = CREATURE_TURN_RATE.to_radians() * scale;
            given.push((id, ActorSpeeds { calm, alert, turn_rate: Some(turn), spot_turn_start: Some(CREATURE_SPOT_TURN_START) }));
        }
    }
    for (id, s) in given {
        res.set_speeds(id, s);
        stats.paced += 1;
    }
}

/// An actor's Speed Mult (percent) as GetBaseAV gives it, through the template chain
/// (Stats). A reference a script made (PlaceAtMe) is not in the game's world outside
/// `with_world`: its base's stats source is read instead.
fn speed_mult(game: &mut fnv_game::Game<'_>, base: FormId, id: FormId) -> f32 {
    let v = if fnv_game::functions::base_of(game, id).is_some() {
        game.base_actor_value(id, fnv_game::actor_values::SPEED_MULTIPLIER).ok()
    } else {
        game.template_source(base, id, |d| d.template_stats).map(|(_, d)| d.speed_mult as f64)
    };
    v.filter(|v| *v > 0.0).map_or(100.0, |v| v as f32)
}

/// A person's speeds: the configured walking and running speeds × its Speed Mult
/// (percent, NPC_ ACBS through the template chain as GetBaseAV gives it): "A Speed actor
/// value is used as a multiplier times a standard movement speed. The Player Character
/// would have a Speed of 100%" [documented: GECK wiki Stats Tab - NPC]. Its clips play
/// that much faster so its feet keep up, as a creature's do [design: the wiki says the
/// original's do not ("the animations only sync up with a speed of 100%"); the owner's
/// pick between the two is asked in docs/owner-review.md]. `None` at 100 (most people):
/// they keep the defaults.
fn npc_speeds(game: &mut fnv_game::Game<'_>, base: FormId, id: FormId, default: Speeds, lo: &mut Locomotion) -> Option<ActorSpeeds> {
    if id == FormId::PLAYER_REF {
        return None;
    }
    let mult = speed_mult(game, base, id);
    if (mult - 100.0).abs() < 0.5 {
        return None;
    }
    let f = mult / 100.0;
    lo.pace = f;
    let calm = Speeds { walk: default.walk * f, run: default.run * f };
    info!("npc pace: {} {id} walks {:.0} and runs {:.0} units/s (Speed Mult {mult:.0})", game.data.name(id), calm.walk, calm.run);
    Some(ActorSpeeds { calm, alert: calm, turn_rate: None, spot_turn_start: None })
}

/// Creatures with an alert stance take it outside the Normal detection state.
fn stance(script: Option<Res<ScriptState>>, mut actors: Query<(&ActorRig, &mut Locomotion), With<PacedCreature>>, mut stats: ResMut<PaceStats>) {
    let Some(game) = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    for (rig, mut lo) in &mut actors {
        let id = FormId(rig.reference);
        let mv = lo.blend.cur.mv;
        if mv.is_moving() {
            stats.moving_frames += 1;
            let fast = matches!(mv, Move::FastForward | Move::FastBackward | Move::FastLeft | Move::FastRight);
            if stats.last.insert(rig.reference, fast).is_some_and(|was| was != fast) {
                stats.swaps += 1;
                if stats.swaps <= SWAPS_LOGGED {
                    let v = lo.velocity[0].hypot(lo.velocity[1]);
                    info!(
                        "creature pace: {} {id} changes to its {} clip at {v:.0} units/s in its own scale ({:.0} at its clips' pace){}",
                        game.data.name(FormId(rig.base)),
                        if fast { "run" } else { "walk" },
                        v / lo.pace.max(0.01),
                        if game.awareness.state(id) != State::Normal { ", alert" } else { "" }
                    );
                }
            }
        }
        if lo.alert.is_none() {
            continue;
        }
        let want = game.awareness.state(id) != State::Normal;
        if lo.set_alert(want) {
            if want {
                stats.alerted += 1;
            } else {
                stats.calmed += 1;
            }
            info!("creature pace: {} {id} {}", game.data.name(FormId(rig.base)), if want { "takes its alert stance" } else { "is calm again" });
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<PaceStats>) {
    if exit.read().next().is_some() {
        info!(
            "creature pace summary: {} creatures given their own speeds ({} people by Speed Mult), people given their armour's speeds {} times; alert stance taken {} times and left {} times; {} frames of their moves, {} changes between walk and run clips",
            stats.paced - stats.npcs, stats.npcs, stats.people, stats.alerted, stats.calmed, stats.moving_frames, stats.swaps
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_actor::locomotion::ClipInfo;

    #[test]
    fn skeletons_keep_their_measured_shares() {
        let s = |p: &str| skeleton_share(p);
        assert_eq!(s("Creatures\\Brahmin\\Skeleton.nif").walk, 1.4);
        assert_eq!(s("meshes/creatures/brahmin/skeleton.nif").run, 1.4);
        assert_eq!(s("creatures\\nvcazadores\\skeleton.nif").walk, 0.70);
        assert_eq!(s("Creatures\\MoleRat\\Skeleton.nif"), Share { walk: 0.80, run: 0.96 });
        assert_eq!(s("creatures\\NVgiantrat\\skeleton.nif").walk, 0.76);
        // Dogs, geckos, mantises, scorpions: the full clip pace.
        assert_eq!(s("Creatures\\Dog\\Skeleton.nif").walk, 1.0);
        assert_eq!(s("creatures\\nvgecko\\skeleton.nif").run, 1.0);
        // Measured in the original: a dog (scale 0.8) walks 45.1, a mole rat runs 324.
        let d = Speeds { walk: 85.3, run: 344.0 };
        let dog = clip_speeds(&set(57.4, Some(333.1)), 0.8, 100.0, s("creatures\\dog\\skeleton.nif"), d);
        assert!((dog.walk - 45.1).abs() < 1.5, "{dog:?}");
        let mole = clip_speeds(&set(42.7, Some(338.7)), 1.0, 100.0, s("creatures\\molerat\\skeleton.nif"), d);
        assert!((mole.walk - 34.1).abs() < 1.0 && (mole.run - 324.0).abs() < 5.0, "{mole:?}");
    }

    #[test]
    fn feral_ghouls_run_a_quarter_faster() {
        assert_eq!(type_pace(Some(FERAL_GHOUL)), 1.25);
        assert_eq!(type_pace(Some(0)), 1.0);
        assert_eq!(type_pace(None), 1.0);
    }
    use nvf_assets::anim_play::{BoundClip, Clip};
    use std::sync::Arc;

    fn set(walk: f32, run: Option<f32>) -> ClipSet {
        let bound = Arc::new(BoundClip {
            clip: Arc::new(Clip {
                name: String::new(),
                start: 0.0,
                stop: 1.0,
                frequency: 1.0,
                cycle_type: 0,
                tracks: Vec::new(),
                priorities: Vec::new(),
                skipped: 0,
                accum_root: None,
                text_keys: Vec::new(),
                visibility: Vec::new(),
                emitters: Vec::new(),
                materials: Vec::new(),
                free: Vec::new(),
                free_visibility: Vec::new(),
                lights: Vec::new(),
            }),
            tracks: Vec::new(),
            root: None,
            root_track: None,
        });
        let clip = |speed: f32| crate::locomotion::SetClip { bound: bound.clone(), info: ClipInfo { duration: 1.0, speed, blend_time: 0.1, foot_phase: 0.0 }, path: String::new() };
        let mut s = ClipSet::default();
        s.clips.insert(Move::Forward, clip(walk));
        if let Some(r) = run {
            s.clips.insert(Move::FastForward, clip(r));
        }
        s
    }

    #[test]
    fn cazadors_move_at_the_observed_speeds() {
        let d = Speeds { walk: 86.8, run: 344.0 };
        // The cazador's mtforward.kf and mtfastforward.kf (216.73 and 545.54 units/s).
        let caz = set(216.73, Some(545.54));
        let share = skeleton_share("creatures\\nvcazadores\\skeleton.nif");
        let young = clip_speeds(&caz, 0.5, 300.0, share, d);
        let medium = clip_speeds(&caz, 1.0, 250.0, share, d);
        // Observed: walks 228 and 380, charges up to 576 and 974.
        assert!((young.walk - 228.0).abs() < 5.0, "{young:?}");
        assert!((medium.walk - 380.0).abs() < 5.0, "{medium:?}");
        assert!((young.run - 576.0).abs() < 10.0, "{young:?}");
        assert!((medium.run - 974.0).abs() < 30.0, "{medium:?}");
        // The medium cazador's walk plays its walk clip, at 1.75 its rate; without the
        // pace it would count as running.
        let mut lo = Locomotion::new(Arc::new(caz.clone()), 0.0);
        lo.velocity = [0.0, medium.walk];
        lo.step(0.016);
        assert_eq!(lo.blend.cur.mv, Move::FastForward);
        let mut lo = Locomotion::new(Arc::new(caz.clone()), 0.0);
        lo.pace = 2.5 * share.walk;
        lo.velocity = [0.0, medium.walk];
        lo.step(0.016);
        assert_eq!(lo.blend.cur.mv, Move::Forward);
        assert!((lo.blend.cur.rate - 1.75).abs() < 0.01, "{}", lo.blend.cur.rate);
        // Near the switch point (343.9) the gait holds: walk until 395, run down to 299.
        let mut lo = Locomotion::new(Arc::new(caz.clone()), 0.0);
        let mut seen = Vec::new();
        for v in [216.0, 360.0, 420.0, 330.0, 250.0] {
            lo.velocity = [0.0, v];
            // Past any cross-fade, so the chosen move is taken.
            for _ in 0..60 {
                lo.step(0.016);
            }
            seen.push(lo.blend.cur.mv);
        }
        assert_eq!(seen, [Move::Forward, Move::Forward, Move::FastForward, Move::FastForward, Move::Forward]);
        // A one-frame spike to three times the walk does not change the gait.
        let mut lo = Locomotion::new(Arc::new(caz.clone()), 0.0);
        let mut gaits = Vec::new();
        for v in [216.0; 30].into_iter().chain([650.0]).chain([216.0; 30]) {
            lo.velocity = [0.0, v];
            lo.step(1.0 / 60.0);
            gaits.push(lo.blend.cur.mv);
        }
        assert!(gaits.iter().all(|m| *m == Move::Forward), "{gaits:?}");
        // No run clip: the default run.
        assert_eq!(clip_speeds(&set(100.0, None), 1.0, 100.0, share, d).run, 344.0);
    }
}
