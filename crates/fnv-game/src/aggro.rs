//! The aggro radius: an actor that does not attack the player on sight warns a player
//! who comes too close and then attacks (docs/design/combat-ai.md §13; owner's playtest
//! list 4, item 18).
//!
//! - **The records** [documented: xEdit AIDT field names "Aggro Radius Behavior" and
//!   "Aggro Radius"]: the AI data (from the template while the template flag "AI Data"
//!   is set) turns the behaviour on and gives the radius in game units. The current
//!   package must allow it: "Aggro Radius Behavior: Allow aggro radius behavior (if any)"
//!   is one of the package's Fallout behaviour flags [documented: GECK wiki
//!   Category:Packages], read as `fnv_ai::social::allows` reads them (a package without
//!   "Skip Fallout Behavior" allows them all); an actor with no package allows it
//!   [hypothesis].
//! - **The warning** [documented: GECK wiki Combat Tab, GuardTrespass "Said when issuing a
//!   warning, either from a Guard package, Aggro Radius behavior, or when the player is
//!   trespassing"; GetTrespassWarningLevel counts the warnings "an actor on a Guard
//!   Package (or responding to trespassing or responding to an aggro radius violation)"
//!   gave]: while it has the player detected (band Detected, measured within the last
//!   second) inside the radius, it says GuardTrespass every fAITrespassWarningTimer
//!   (5 s) up to iGuardWarnings (2) times, then attacks: a grudge ([`Why::Aggro`]) makes
//!   `Game::would_attack` true for the pair [hypothesis: the guard package's rule,
//!   `crate::crime`'s for trespassing; creatures have no GuardTrespass lines and warn
//!   in silence]. Leaving the radius clears the count [hypothesis].
//! - **Who**: the player only [design: other actors' aggro radii are not measured];
//!   never an actor that is the player's ally or friend or teammate, one already hostile
//!   by its records, a dead, unconscious or restrained one.
//!
//! Example (FalloutNV.esm): the "Young Gecko Hunter" geckos of GS02
//! (VSpawnSpecialTier1GeckoSmallGS02Hunter 0x00165183) are Unaggressive, neutral to the
//! player, with Aggro Radius Behavior on and a radius of 650 [observed: `fnvtool esm
//! decode`]: they ignore the player at a distance and attack within 650 units.

use crate::detection::Band;
use crate::game::Game;
use crate::package::{fallout, flags};
use crate::provocation::{Grudge, Note, Why, ALLY, FRIEND};
use fnv_sim::FormId;

impl Game<'_> {
    /// `actor`'s aggro radius in game units when its AI data turns the behaviour on.
    pub fn aggro_radius(&self, actor: FormId) -> Option<f64> {
        let d = self.def_following(actor, |d| d.template_ai)?;
        (d.aggro_radius_behavior && d.aggro_radius > 0).then_some(d.aggro_radius as f64)
    }

    /// Whether `actor`'s current package allows aggro radius behaviour (module notes).
    fn package_allows_aggro(&mut self, actor: FormId) -> bool {
        let data = self.data;
        match self.current_package(actor).and_then(|p| data.package_defs.get(&p)) {
            Some(d) => !d.has(flags::SKIP_FALLOUT_BEHAVIOR) || d.fallout_flags & fallout::AGGRO_RADIUS != 0,
            None => true,
        }
    }

    /// After a round of detection measurements: aggro radius warnings and attacks (module
    /// notes). Returns how many actors turned on the player.
    pub fn aggro_tick(&mut self) -> usize {
        let s = self.crime_settings();
        let now = self.time;
        let Some(player) = self.world.peek(FormId::PLAYER_REF).map(|p| p.pos) else { return 0 };
        let watchers: Vec<FormId> = self
            .detection
            .pairs
            .iter()
            .filter(|((w, t), (d, at))| *t == FormId::PLAYER_REF && *w != FormId::PLAYER_REF && d.band == Band::Detected && now - at <= 1.0)
            .map(|((w, _), _)| *w)
            .collect();
        // Actors no longer watching the player inside their radius start counting afresh.
        let mut inside = Vec::new();
        let mut turned = 0;
        for w in watchers {
            let Some(radius) = self.aggro_radius(w) else { continue };
            let out = |g: &Game, r: FormId| g.extra.get(&r).is_some_and(|e| e.dead || e.unconscious || e.restrained || e.teammate);
            if out(self, w) || !self.present(w) || self.provocations.holds(w, FormId::PLAYER_REF) {
                continue;
            }
            let Some(pos) = self.world.peek(w).map(|p| p.pos).or_else(|| self.world.placed.get(w).map(|p| p.pos)) else { continue };
            let distance = (0..3).map(|i| (pos[i] - player[i]).powi(2)).sum::<f32>().sqrt() as f64;
            if distance > radius || matches!(self.reaction_to(w, FormId::PLAYER_REF), ALLY | FRIEND) || !self.package_allows_aggro(w) {
                continue;
            }
            if self.would_attack(w, FormId::PLAYER_REF) {
                continue;
            }
            inside.push(w);
            let (level, last) = self.provocations.aggro_warnings.get(&w).copied().unwrap_or((0, f64::NEG_INFINITY));
            if now - last < s.warning_timer {
                continue;
            }
            if (level as f64) < s.guard_warnings {
                self.provocations.aggro_warnings.insert(w, (level + 1, now));
                self.detection_bark(w, Some(FormId::PLAYER_REF), "GuardTrespass");
                self.provocations.notes.push(Note::AggroWarned { actor: w, level: level + 1, distance, radius });
            } else {
                self.provocations.grudges.insert((w, FormId::PLAYER_REF), Grudge { why: Why::Aggro, since: now });
                self.provocations.notes.push(Note::AggroAttacks { actor: w, distance, radius });
                turned += 1;
            }
        }
        self.provocations.aggro_warnings.retain(|w, _| inside.contains(w));
        turned
    }
}
