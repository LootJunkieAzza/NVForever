//! Actors already at their package's place when the player arrives (issue #1208).
//!
//! After a fast travel or a load door the original shows the destination's actors where
//! their current package has them: Boone at his post by the dinosaur when the player
//! fast travels to Novac at 5am, Cass already drinking at the Mojave Outpost's bar on
//! entering, the Goodsprings settler already seated in the Prospector Saloon [observed,
//! owner, issue #1208]. Ours walked there in front of the player: an actor of a cell
//! that was not loaded joined the AI at its editor place (or where the low process had
//! left it) and only then set off.
//!
//! So here the engine, while the loading screen is up, hands the AI each actor that
//! comes into the loaded area and was not in it before the trip ([`Ai::arrive`]); the
//! AI takes up the actor's current package and, instead of walking, puts it where the
//! walk ends: at the package's place, on the furniture the package or its sandbox gives
//! it, at a patrol's first point. Only walks that start or end in the loaded area
//! ([`crate::Ai::set_loaded`]) are skipped [hypothesis: the original's actors were seen at the
//! place, not part-way; whether it puts them exactly at the end or somewhere within the
//! package's radius is for the observer, docs/handoff/observer-checks.md]. Actors the
//! player could see before the trip are never moved: the engine leaves them out (a
//! short fast travel inside the loaded area moves nobody [design, issue #1208]).
//!
//! Not put anywhere: an actor that is dead, unconscious, restrained, rooted, with its
//! AI off, or not in the world; one following, escorting, searching, fleeing or in a
//! conversation (those walks depend on someone else, and go on as they do now).

use crate::carry::{Activity, Ai, Passed, Talking};
use crate::goal::{self, Goal};
use fnv_game::game::Game;
use fnv_nav::follow::heading_of;
use fnv_nav::{NavGraph, Place};
use fnv_sim::FormId;
use serde::Serialize;

/// At most this many walks are skipped for one actor on arrival: to its package's place,
/// then a sandbox's own walk from there to the furniture or marker it chose [our choice;
/// a bound so that a package that keeps sending its actor on cannot hold the loading].
pub const ARRIVE_WALKS: usize = 3;

/// How far from a walk's end the navmesh is looked for to stand the actor on, game units
/// [our choice: the path's end is on the navmesh already, this only finds its triangle].
const ARRIVE_NAV_REACH: f32 = 64.0;

/// What [`Ai::arrive`] did with an actor.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Arrival {
    /// Its package, where it was and where it is now.
    pub package: FormId,
    pub from: Place,
    pub to: Place,
    /// The walks skipped, and how long they were together (straight lines between the
    /// ends), game units.
    pub walks: usize,
    pub skipped: f32,
    /// The furniture it now uses (the engine seats it at once), if any.
    pub furniture: Option<FormId>,
}

impl Ai {
    /// `actor` comes into the loaded area as the player arrives somewhere (module
    /// notes): it takes up its current package, chosen again for the hour it is now, and
    /// is put where that package's walk would take it, on the furniture it would use
    /// there. Returns what was done, or `None` when it was not put anywhere (already
    /// there, nowhere to go, or one of those left alone).
    pub fn arrive(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId) -> Option<Arrival> {
        let a = self.agents.get(&actor)?;
        let held = game.extra.get(&actor).is_some_and(|e| e.dead || e.out_cold() || e.restrained);
        if a.rooted || held || !game.present(actor) || game.ai_off(actor) {
            return None;
        }
        let from = Place { space: a.walker.space, pos: a.walker.feet };
        // The hour may have moved on in the trip (a fast travel's hours): the package of
        // now, not of the last periodic choice.
        game.update_package(actor);
        let (mut walks, mut skipped) = (0, 0.0);
        for _ in 0..ARRIVE_WALKS {
            // Takes up the package (its goal and walk), as an update in a menu does.
            self.step_actor(game, nav, actor, 0.0, &mut Passed, true);
            let a = &self.agents[&actor];
            // At the place of a sandbox: what it does there, chosen now.
            if a.activity == Activity::There && a.sandbox.as_ref().is_some_and(|s| s.pending.is_none()) {
                self.sandbox_step(game, nav, actor);
            }
            let Some(to) = self.skippable(actor) else { break };
            // Only a walk that would be seen: one that starts or ends in the loaded area
            // (Boone, in his room behind the motel's door, to the dinosaur in the
            // street); one wholly out of it is the low process's ([`crate::process`]).
            let at = Place { space: self.agents[&actor].walker.space, pos: self.agents[&actor].walker.feet };
            if self.loaded.as_ref().is_some_and(|l| !l.holds(at.space, at.pos) && !l.holds(to.space, to.pos)) {
                break;
            }
            let a = self.agents.get_mut(&actor).expect("listed");
            let w = &mut a.walker;
            // Facing the way the walk's last stretch went.
            let path = w.follower.as_ref().and_then(|f| f.path.legs.last()).map(|l| l.points.clone()).unwrap_or_default();
            let heading = match path.as_slice() {
                [.., p, q] if goal::dist(*p, *q) > 1.0 => heading_of([q[0] - p[0], q[1] - p[1]]),
                _ => w.heading,
            };
            skipped += goal::dist(w.feet, to.pos);
            w.space = to.space;
            w.feet = to.pos;
            w.heading = heading;
            w.tri = nav.nearest(to.space, to.pos, ARRIVE_NAV_REACH).map_or(fnv_nav::graph::NONE, |(t, _)| t);
            (w.turning, w.moving) = (false, false);
            let cell = goal::cell_of_place(game.world.placed, &to);
            let _ = game.set_actor_place(actor, cell, to.pos, heading);
            walks += 1;
            let patrol = matches!(a.goal, Some(Goal::Route(_)));
            self.arrived(game, nav, actor);
            // A patrol starts from its first point; it walks on from there in sight.
            if patrol {
                break;
            }
        }
        if walks == 0 {
            return None;
        }
        let a = &self.agents[&actor];
        let to = Place { space: a.walker.space, pos: a.walker.feet };
        Some(Arrival { package: a.package?, from, to, walks, skipped, furniture: a.using })
    }

    /// Where `actor`'s walk ends, when it is one [`Ai::arrive`] skips: a walk of its
    /// package to its place (or a patrol's point), or of its sandbox to what it chose.
    fn skippable(&self, actor: FormId) -> Option<Place> {
        let a = &self.agents[&actor];
        let own = a.package.is_some()
            && a.activity == Activity::Travelling
            && a.chat.is_none()
            && a.search.is_none()
            && a.errand.is_none()
            && a.weapon_order.is_none()
            && a.backing.is_none()
            && matches!(a.talk, None | Some(Talking::ToWait));
        let kind = matches!(a.goal, Some(Goal::Near { .. } | Goal::Inside { .. } | Goal::Route(_) | Goal::Talk(_))) || a.sandbox.as_ref().is_some_and(|s| s.pending.is_some());
        if !own || !kind {
            return None;
        }
        a.walker.follower.as_ref().and_then(|f| f.destination())
    }
}
