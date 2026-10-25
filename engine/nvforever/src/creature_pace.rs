//! How fast creatures move, and the stance they move in (docs/spec-notes/actors.md §8.10;
//! the owner's playtest 2026-10-16: cazadors moved far too slowly and their walk stopped
//! and started).
//!
//! - **Speed**: a creature walks at the root motion of its `Forward` clip and runs at
//!   that of its `FastForward` clip, times its scale, its Speed Mult (CREA ACBS) and
//!   [`CREATURE_SPEED_FIT`]. The AI moves it at those speeds (`fnv_ai::Ai::speeds`).
//!   People walk and run at `fnv_ai::NPC_WALK` and `NPC_RUN` times their body armour's
//!   share ([`person_pace`]), looked at again every second. Before this every actor
//!   walked at 86.8 and ran at 344 units/s: a cazador's walk clip (217 units/s) played
//!   at its slowest rate (0.4), and 344 is the point between its walk and run clips
//!   (the geometric mean of 217 and 546), so a small slow-down swapped the two clips back
//!   and forth: the stop-and-start.
//! - **Clip rate**: the clips play `Speed Mult × CREATURE_SPEED_FIT` faster
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

/// The share of the clip pace (root motion × scale × Speed Mult) a creature keeps
/// [observed: the AI observer in the original game, 2026-10-16: non-hostile cazadors at
/// the Tribal Village walked at 228 units/s (young: scale 0.5, Speed Mult 300; clip
/// pace 325) and 380 (scale 1, Speed Mult 250; clip pace 542), 0.70 of the clip pace
/// both; charging, they reached 576 and 974 (run clip pace 818 and 1364, again 0.70);
/// hypothesis: the same share for every creature; why 0.70 is open, actors.md §8.10].
pub const CREATURE_SPEED_FIT: f32 = 0.70;

/// CREA DATA creature type code of feral ghouls [documented: xEdit wbCreatureTypeEnum,
/// 5 Feral Ghoul].
pub const FERAL_GHOUL: i64 = 5;

/// A creature type's own share on top of [`CREATURE_SPEED_FIT`]: feral ghouls 1.25
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

/// A skeleton's own share on top of [`CREATURE_SPEED_FIT`]: brahmin (`creatures\brahmin`)
/// 2.0 [observed: the AI observer in the original game, 2026-10-24, the Novac caravan's
/// pack brahmin 001650BD (scale 1.25, Speed Mult 100) following its merchant on its
/// route: it walked at a median 78 units/s over 2-second windows, and ran once at a
/// median of about 330 to catch up; with the fit alone it walked 39 and ran 155, half
/// of both. Hypothesis: that its walk is its own and not matched to the merchant's 77
/// (the run, matched to nothing, is about twice too slow as well), and the same share
/// for every brahmin]; every other skeleton 1.
pub fn skeleton_pace(skeleton: &str) -> f32 {
    if fnv_vfs::normalize(skeleton).contains("creatures\\brahmin\\") {
        2.0
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
/// `speed_mult` (percent) × [`CREATURE_SPEED_FIT`]; a move without a clip keeps
/// `default`'s speed.
pub fn clip_speeds(set: &ClipSet, scale: f32, speed_mult: f32, default: Speeds) -> Speeds {
    let of = |m: Move, d: f32| set.info(m).map(|i| i.speed).filter(|v| *v > 1.0).map_or(d, |v| v * scale * speed_mult / 100.0 * CREATURE_SPEED_FIT);
    Speeds { walk: of(Move::Forward, default.walk), run: of(Move::FastForward, default.run) }
}

/// The share of the people's speeds (`fnv_ai::NPC_WALK`, `NPC_RUN`) a person keeps: its
/// body armour's (`Game::armor_speed_mult`: medium × 0.925, heavy × 0.85 [observed,
/// actors.md §8.10]). Not its Speed Mult [observed: Cook-Cook, Speed Mult 110, ran 255 in
/// metal armour with the Incinerator drawn, 344 × 0.75 = 258; with his Speed Mult it
/// would be 284].
pub fn person_pace(game: &mut fnv_game::Game, id: FormId) -> f32 {
    game.armor_speed_mult(id) as f32
}

/// A drawn person and the share of the people's speeds it was last given.
#[derive(Component)]
struct PersonPace(f32);

/// How often a drawn person's armour is looked at again, seconds: people put on their
/// outfit when their cell loads, which may come after they are drawn, and scripts change
/// what they wear [design].
const PERSON_PACE_EVERY: f32 = 1.0;

/// Drawn people walk and run at the people's speeds × [`person_pace`].
fn people_speeds(
    time: Res<Time>,
    mut wait: Local<f32>,
    mut script: Option<ResMut<ScriptState>>,
    mut res: Option<ResMut<Residents>>,
    mut people: Query<(&ActorRig, &mut PersonPace)>,
    mut stats: ResMut<PaceStats>,
) {
    *wait -= time.delta_secs();
    if *wait > 0.0 {
        return;
    }
    *wait = PERSON_PACE_EVERY;
    let Some(res) = res.as_deref_mut() else { return };
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
            let share = person_pace(game, id);
            if (share - pace.0).abs() < 1e-3 {
                continue;
            }
            pace.0 = share;
            let s = Speeds { walk: default.walk * share, run: default.run * share };
            info!("person pace: {} {id} walks {:.0} and runs {:.0} units/s ({:.1}% of the people's speeds: its armour)", game.data.name(FormId(rig.base)), s.walk, s.run, share * 100.0);
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
                // A person: `people_speeds` gives its own.
                commands.entity(e).insert(PersonPace(1.0));
                continue;
            }
            commands.entity(e).insert(PacedCreature);
            let id = FormId(rig.reference);
            // Speed Mult as GetBaseAV gives it: through the template chain (Stats), as the
            // leveled cazadors at the Tribal Village take theirs.
            let mult = game.base_actor_value(id, fnv_game::actor_values::SPEED_MULTIPLIER).ok().filter(|v| *v > 0.0).map_or(100.0, |v| v as f32);
            let scale = global.affine().transform_vector3(Vec3::Z).length().max(0.01);
            // Its type's and its skeleton's own shares (feral ghouls, brahmin), as if its
            // Speed Mult were that much higher.
            let mult = mult * type_pace(game.creature_type(id)) * skeleton_pace(&rig.skeleton);
            lo.pace = mult / 100.0 * CREATURE_SPEED_FIT;
            let calm = clip_speeds(&lo.stand, scale, mult, default);
            let alert = lo.alert.as_deref().map_or(calm, |a| clip_speeds(a, scale, mult, default));
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
            "creature pace summary: {} creatures given their own speeds, people given their armour's speeds {} times; alert stance taken {} times and left {} times; {} frames of their moves, {} changes between walk and run clips",
            stats.paced, stats.people, stats.alerted, stats.calmed, stats.moving_frames, stats.swaps
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_actor::locomotion::ClipInfo;

    #[test]
    fn brahmin_go_twice_the_fit() {
        assert_eq!(skeleton_pace("Creatures\\Brahmin\\Skeleton.nif"), 2.0);
        assert_eq!(skeleton_pace("meshes/creatures/brahmin/skeleton.nif"), 2.0);
        assert_eq!(skeleton_pace("creatures\\cazador\\skeleton.nif"), 1.0);
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
        let young = clip_speeds(&caz, 0.5, 300.0, d);
        let medium = clip_speeds(&caz, 1.0, 250.0, d);
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
        lo.pace = 2.5 * CREATURE_SPEED_FIT;
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
        assert_eq!(clip_speeds(&set(100.0, None), 1.0, 100.0, d).run, 344.0);
    }
}
