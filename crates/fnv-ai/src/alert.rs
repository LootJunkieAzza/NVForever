//! Actors in a combat state (`fnv_game::awareness`; docs/design/alert.md §4): what the
//! AI has them do instead of their package.
//!
//! - **Alert and Lost**: the actor searches [documented: GECK wiki Category:Detection,
//!   "Alert and Lost actors search … if their package allows"]. It gets off furniture,
//!   walks to where its latest alarm is (what it noticed, heard or last saw), looks
//!   that way for a few seconds, then searches on: at the alarm again if it has moved
//!   (the hostile was noticed somewhere else), else at a random point around it within
//!   the first search radius. It walks, it does not run [engineering choice]. The Combat
//!   Search page that describes the original's search is not in the owner's export; the
//!   radii are FalloutNV.esm's fCombatSearchInteriorMinRadius and
//!   fCombatSearchExteriorMinRadius [observed: 256 and 512; hypothesis: the radius of
//!   the nearest search ring].
//! - **Combat**: the actor closes in on its target until it is within the distance it
//!   attacks from (`Awarenesses::engage`, which the engine's combat AI sets from its
//!   weapon), running, the way planned again as the target moves; there it stops and
//!   faces it. Without such a distance it holds its ground [design]. The attacks
//!   themselves are the engine's (nvforever `npc_combat`). A target it does not detect
//!   now it closes in on whatever the distance, halving the gap each time the way is
//!   planned, until it sees it again [design]. While it aims down its sights
//!   (`Awarenesses::aiming`, set by the combat AI) it walks instead of running, as the
//!   player does while aiming [design: the owner's wish, playtest list 2026-10-11, "the
//!   player and NPCs should not be able to move at full speed whilst aiming"].
//! - **Continue During Combat**: an actor whose current package has the flag carries
//!   its package out as if nothing had happened (`Game::continues_during_combat`).
//! - Back in **Normal**, the actor goes back to its package's place, as after a random
//!   conversation.

use crate::carry::{Activity, AiEvent};
use crate::goal;
use crate::Ai;
use fnv_game::awareness::State;
use fnv_game::game::Game;
use fnv_nav::{NavGraph, Place, Space};
use fnv_sim::FormId;
use serde::Serialize;

/// Seconds an actor looks about where it searched before it searches on
/// [engineering choice].
pub const LOOK_SECONDS: f64 = 4.0;
/// A walk to a search point that has not arrived after this long ends where it is
/// [engineering choice].
pub const SEARCH_WALK_SECONDS: f64 = 30.0;
/// An alarm that moved this far from the point last searched is searched at again
/// [engineering choice].
pub const RECENTRE: f32 = 64.0;
/// Closing in on a target: the way is planned again when the target has moved this
/// far from where it was planned to, or after this long [design].
pub const CLOSE_REPLAN_DISTANCE: f32 = 96.0;
pub const CLOSE_REPLAN_SECONDS: f64 = 1.5;
/// Closing in ends this share of the stop distance short of the target, inside it
/// [design].
pub const STOP_SHORT: f32 = 0.9;
/// In Combat, a target it does not detect now: it closes in to this share of the
/// distance, again each time the way is planned [design].
pub const BLIND_CLOSE_SHARE: f32 = 0.5;

/// Where an actor in a combat state is ([`crate::Agent::search`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "search", rename_all = "snake_case")]
pub enum Search {
    /// Walking to a point to search, since `since` (`Game::time`).
    Going { to: [f32; 3], since: f64 },
    /// Looking about at `at` until `until`.
    Looking { at: [f32; 3], until: f64 },
    /// In Combat: standing, facing its target.
    Holding { target: FormId },
    /// In Combat: running up to its target, last planned for where it stood (`to`) at
    /// `since`.
    Closing { target: FormId, to: [f32; 3], since: f64 },
}

fn setting(game: &Game, name: &str, default: f32) -> f32 {
    game.world.forms.setting_f32(name).unwrap_or(default)
}

impl Ai {
    /// One update of an actor in a combat state (module notes). Returns whether the state
    /// holds the actor: its package's own doings wait meanwhile.
    pub(crate) fn alert_step(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, dt: f32) -> bool {
        let state = game.awareness.state(actor);
        if !state.in_combat() || game.continues_during_combat(actor) {
            if self.agents[&actor].search.is_some() {
                let why = if state.in_combat() { "its package continues during combat" } else { "back to Normal" };
                self.search_over(game, nav, actor, why);
            }
            return false;
        }
        // Off furniture first: the engine gets it up, and its walk waits until it has.
        if self.agents[&actor].using.is_some() {
            self.set_using(game, actor, None);
            self.log(game, actor, AiEvent::Furniture { furniture: None });
        }
        let Some(aw) = game.awareness.get(actor) else { return true };
        let now = game.time;
        let a = &self.agents[&actor];
        let space = a.walker.space;
        if state == State::Combat {
            // The target it detects; just after losing it, the one detected last.
            let latest = aw.targets.iter().filter(|(_, t)| t.detected.is_some()).max_by(|x, y| x.1.detected.unwrap_or(0.0).total_cmp(&y.1.detected.unwrap_or(0.0)));
            let Some((target, at)) = aw.combat_target().or(latest.map(|(id, t)| (*id, t))).map(|(id, t)| (id, t.at)) else { return true };
            let away = goal::dist(a.walker.feet, at);
            // It stops within its distance and, standing, sets off again only when the
            // target is beyond what it can attack from, so it does not stop and start at
            // the edge.
            let holding = a.search == Some(Search::Holding { target });
            let engage = game.awareness.engage.get(&actor).copied();
            // A target it does not detect now (out of sight behind a rock, say) it closes
            // in on, whatever the distance, halving the gap, until it sees it again
            // [design: an actor that stood where it could not see its target went Lost
            // and back to Combat every few seconds at Cook-Cook's camp].
            let sees = aw.combat_target().is_some();
            let close_in = (!sees && engage.is_some()) || engage.is_some_and(|(stop, resume)| away > if holding { resume } else { stop });
            let stop = if sees { engage.map_or(0.0, |e| e.0) } else { away * BLIND_CLOSE_SHARE };
            let going = a.activity == Activity::Travelling;
            match (close_in, a.search) {
                // On its way, and the way still leads to the target.
                (true, Some(Search::Closing { target: t, to, since })) if t == target && going && goal::dist(to, at) < CLOSE_REPLAN_DISTANCE && now - since < CLOSE_REPLAN_SECONDS => {}
                (true, _) => self.close_in(game, nav, actor, target, at, away, stop),
                (false, Some(Search::Holding { target: t })) if t == target => {}
                (false, _) => {
                    let a = self.agents.get_mut(&actor).expect("listed");
                    a.walker.follower = None;
                    a.activity = Activity::There;
                    a.search = Some(Search::Holding { target });
                    self.log(game, actor, AiEvent::Hold { target, away });
                }
            }
            // Aiming down the sights slows it to a walk, also on a way already planned.
            let aiming = game.awareness.aiming.contains(&actor);
            let (walk, run) = (self.cfg.walk_speed, self.cfg.run_speed);
            let a = self.agents.get_mut(&actor).expect("listed");
            if let Some(Search::Closing { target, .. }) = a.search {
                let speed = if aiming { walk } else { run };
                if a.walker.speed != speed {
                    a.walker.speed = speed;
                    self.log(game, actor, AiEvent::Pace { target, aiming });
                }
            }
            if self.agents[&actor].activity != Activity::Travelling {
                self.face(game, actor, Place { space, pos: at }, dt);
            }
            return true;
        }
        let Some(alarm) = aw.search_alarm().copied() else { return true };
        match a.search {
            None | Some(Search::Holding { .. } | Search::Closing { .. }) => self.search_at(game, nav, actor, alarm.at, alarm.owner),
            Some(Search::Going { to, since }) => {
                if a.activity != Activity::Travelling || now - since > SEARCH_WALK_SECONDS {
                    let a = self.agents.get_mut(&actor).expect("listed");
                    a.walker.follower = None;
                    a.activity = Activity::There;
                    a.search = Some(Search::Looking { at: to, until: now + LOOK_SECONDS });
                    self.log(game, actor, AiEvent::SearchLook { at: to, seconds: LOOK_SECONDS as f32 });
                }
            }
            Some(Search::Looking { at, until }) => {
                self.face(game, actor, Place { space, pos: alarm.at }, dt);
                if now >= until {
                    let to = if goal::dist(at, alarm.at) > RECENTRE {
                        alarm.at
                    } else {
                        let indoors = matches!(space, Space::Interior(_));
                        let r = if indoors { setting(game, "fCombatSearchInteriorMinRadius", 256.0) } else { setting(game, "fCombatSearchExteriorMinRadius", 512.0) };
                        let angle = self.social.rng.range(0.0, std::f32::consts::TAU);
                        let d = self.social.rng.range(0.25, 1.0) * r;
                        [alarm.at[0] + d * angle.cos(), alarm.at[1] + d * angle.sin(), alarm.at[2]]
                    };
                    self.search_at(game, nav, actor, to, alarm.owner);
                }
            }
        }
        true
    }

    /// Set off to search at `to`, the navmesh point nearest it; without a way there the
    /// actor looks about where it stands.
    fn search_at(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, to: [f32; 3], about: FormId) {
        let params = self.path_params(game, actor);
        let speed = self.cfg.walk_speed;
        let now = game.time;
        let a = self.agents.get_mut(&actor).expect("listed");
        // A sandbox's own walk is given up; it chooses again after the search.
        if let Some(s) = a.sandbox.as_mut() {
            s.pending = None;
        }
        a.wait_until = None;
        let space = a.walker.space;
        let from = a.walker.feet;
        let point = nav.nearest(space, to, 512.0).map(|(_, p)| p);
        a.walker.speed = speed;
        let path = point.and_then(|p| a.walker.go_to(nav, Place { space, pos: p }, &params).ok()).map(|path| path.length);
        match (point, path) {
            (Some(p), Some(length)) => {
                a.activity = Activity::Travelling;
                a.trip_started = now;
                a.trip_from = a.walker.travelled;
                a.search = Some(Search::Going { to: p, since: now });
                self.log(game, actor, AiEvent::Search { about, to: p, away: goal::dist(from, p), length });
            }
            _ => {
                a.walker.follower = None;
                a.activity = Activity::There;
                a.search = Some(Search::Looking { at: from, until: now + LOOK_SECONDS });
                self.log(game, actor, AiEvent::SearchLook { at: from, seconds: LOOK_SECONDS as f32 });
            }
        }
    }

    /// Run towards `target`, at `at`: to the point `stop` units short of it on the line
    /// from the target to the actor (the navmesh point nearest that), so that it ends
    /// within attacking distance and not against the target's body [design; owner
    /// 2026-10-11: a dog gets into range and bites, it does not push the player around].
    /// Without a way there the actor holds its ground.
    #[allow(clippy::too_many_arguments)]
    fn close_in(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, target: FormId, at: [f32; 3], away: f32, stop: f32) {
        let params = self.path_params(game, actor);
        let aiming = game.awareness.aiming.contains(&actor);
        let speed = if aiming { self.cfg.walk_speed } else { self.cfg.run_speed };
        let now = game.time;
        let a = self.agents.get_mut(&actor).expect("listed");
        let space = a.walker.space;
        let back = [a.walker.feet[0] - at[0], a.walker.feet[1] - at[1]];
        let len = back[0].hypot(back[1]).max(1e-3);
        let short = (stop * STOP_SHORT).min(len);
        let goal = [at[0] + back[0] / len * short, at[1] + back[1] / len * short, at[2]];
        let point = nav.nearest(space, goal, 512.0).map(|(_, p)| p);
        a.walker.speed = speed;
        let path = point.and_then(|p| a.walker.go_to(nav, Place { space, pos: p }, &params).ok()).map(|path| path.length);
        let first = !matches!(a.search, Some(Search::Closing { target: t, .. }) if t == target);
        match (point, path) {
            (Some(p), Some(length)) => {
                a.activity = Activity::Travelling;
                a.trip_started = now;
                a.trip_from = a.walker.travelled;
                a.search = Some(Search::Closing { target, to: at, since: now });
                if first {
                    self.log(game, actor, AiEvent::Close { target, to: p, away, length, aiming });
                }
            }
            _ => {
                a.walker.follower = None;
                a.activity = Activity::There;
                if a.search != Some(Search::Holding { target }) {
                    a.search = Some(Search::Holding { target });
                    self.log(game, actor, AiEvent::Hold { target, away });
                }
            }
        }
    }

    /// The search or the stand ends: the actor goes back to its package.
    fn search_over(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, why: &'static str) {
        let a = self.agents.get_mut(&actor).expect("listed");
        a.search = None;
        if a.activity == Activity::Travelling {
            a.walker.follower = None;
            a.activity = Activity::There;
        }
        self.log(game, actor, AiEvent::SearchOver { why });
        self.back_to_package(game, nav, actor, |package, away| AiEvent::SearchBack { package, away });
    }
}
