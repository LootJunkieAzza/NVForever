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
//!   [hypothesis]; a package with "No Warn/Attack Behavior" does not [hypothesis: the
//!   flag's name].
//! - **The warning** [documented: GECK wiki Combat Tab, GuardTrespass "Said when issuing a
//!   warning, either from a Guard package, Aggro Radius behavior, or when the player is
//!   trespassing"; GetTrespassWarningLevel counts the warnings "an actor on a Guard
//!   Package (or responding to trespassing or responding to an aggro radius violation)"
//!   gave]: while it has the player detected (band Detected, measured within the last
//!   second) inside the radius, it says GuardTrespass every fAITrespassWarningTimer
//!   (5 s) up to iGuardWarnings (2) times, then attacks: a grudge ([`Why::Aggro`]) makes
//!   `Game::would_attack` true for the pair [hypothesis: the guard package's rule,
//!   `crate::crime`'s for trespassing; creatures have no GuardTrespass lines and warn
//!   in silence]. The count clears once the actor has gone twice the warning timer
//!   (10 s) without warning, i.e. after leaving the radius or losing the player for
//!   that long; a short loss of detection while it wanders keeps the count [decision,
//!   2026-10-13: clearing it at once let wandering geckos warn forever and never attack].
//!   An actor that has started warning also keeps counting while the player is only in
//!   its alert band (Noticed: it knows where the player is but does not see them, e.g.
//!   walking past with its back turned) [decision, same reason].
//! - **The look** [hypothesis: the idle tree; owner's playtest 2026-10-24, "baby geckos
//!   were not showing the aggressive stay away from me warnings"]: while it warns (from
//!   its first warning until it attacks or the player is out of its radius or unwatched
//!   at an aggro tick) the actor is alerted (`Game::is_alerted`, GetIsAlerted 1): a
//!   person draws its weapon, a creature its unarmed stance; the AI stops it and turns
//!   it to the player (`fnv_ai::alert`); standing alerted, a creature plays its idle
//!   tree's combat idles (the gecko's hiss and side hops under NVGeckoCombatIdles, which
//!   wants IsMoving != 1 and GetIsAlerted 1, or the combat or flee procedure [observed:
//!   FalloutNV.esm IDLE 0x0011D9E9]; the mirelurk's and sentry bot's SpecialIdle_Warning
//!   want GetIsAlerted 1 [observed: MWarning 0x00072DC2, StBWarning 0x00099837]). No
//!   page says what the original's warning looks like; these idles wanting the alerted
//!   state are the evidence. Without the look a creature warned in silence while it
//!   walked on, and the player saw nothing before the attack.
//! - **Guard packages** [documented: GuardTrespass is "said when issuing a warning …
//!   from a Guard package"; fAITrespassWarningTimer and iGuardWarnings are the "Guard
//!   Package Settings" (GECK wiki Settings); the Guard package page, as packages.md §3
//!   took it down: "warn intruders near the guarded reference …, then attack"]: an actor
//!   whose current package is a Guard package without "No Warn/Attack Behavior" warns
//!   and then attacks a player it watches inside the guarded area, by the same timer,
//!   count and look as the aggro radius. The guarded reference is the package's target
//!   (a reference, or the actor's linked reference: vGomorrahGuardPackage's is the door
//!   each Gomorrah guard stands by), the area the target's Count / Distance around it, or
//!   the package location's radius when that is 0 [hypothesis: which field is the
//!   area's radius; capture campaign `warnings`, sessions gomorrah_*]; a package
//!   guarding the player, or with neither radius, warns nobody. The area comes before
//!   the actor's aggro radius; one count serves both (GetTrespassWarningLevel). A grudge
//!   [`Why::Guard`]. In FalloutNV.esm 61 of the 175 Guard packages allow it
//!   [observed: `fnvtool ai show`].
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
use crate::package::{fallout, flags, PackageType};
use crate::provocation::{Grudge, Note, Why, ALLY, FRIEND};
use fnv_sim::FormId;

/// After an aggro attack begins, how long the actor still counts as warning while it is
/// not yet in Combat ([`Game::warning_to`]), seconds [design].
pub const AGGRO_TURN_SECONDS: f64 = 2.0;

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
            // "No Warn/Attack Behavior" turns the warnings and the attack off [hypothesis:
            // the flag's name, xEdit; no page describes it].
            Some(d) if d.has(flags::NO_WARN_ATTACK_BEHAVIOR) => false,
            Some(d) => !d.has(flags::SKIP_FALLOUT_BEHAVIOR) || d.fallout_flags & fallout::AGGRO_RADIUS != 0,
            None => true,
        }
    }

    /// What `actor`'s current Guard package has it guard (module notes, "Guard
    /// packages"): the guarded reference and the radius around it, game units. None for
    /// any other package, one with "No Warn/Attack Behavior", one guarding the player, or
    /// one with no radius.
    pub fn guard_area(&mut self, actor: FormId) -> Option<(FormId, f64)> {
        let data = self.data;
        let d = self.current_package(actor).and_then(|p| data.package_defs.get(&p))?;
        if d.ty != PackageType::Guard || d.has(flags::NO_WARN_ATTACK_BEHAVIOR) {
            return None;
        }
        let guarded = self.package_target(actor, d).filter(|g| *g != FormId::PLAYER_REF)?;
        let radius = d.target.map(|t| t.count_distance).filter(|r| *r > 0).or(d.location.map(|l| l.radius).filter(|r| *r > 0))?;
        Some((guarded, radius as f64))
    }

    /// After a round of detection measurements: aggro radius and Guard package warnings
    /// and attacks (module notes). Returns how many actors turned on the player.
    pub fn aggro_tick(&mut self) -> usize {
        let s = self.crime_settings();
        let now = self.time;
        let Some(player) = self.world.peek(FormId::PLAYER_REF).map(|p| p.pos) else { return 0 };
        let watchers: Vec<FormId> = self
            .detection
            .pairs
            .iter()
            .filter(|((w, t), (d, at))| {
                // One that has started warning keeps counting while it has the player in
                // the alert band (module notes).
                let warning = self.provocations.aggro_warnings.contains_key(w);
                *t == FormId::PLAYER_REF && *w != FormId::PLAYER_REF && (d.band == Band::Detected || (warning && d.band == Band::Noticed)) && now - at <= 1.0
            })
            .map(|((w, _), _)| *w)
            .collect();
        // Actors no longer watching the player inside their radius start counting afresh
        // after a grace of two warning timers (module notes).
        let mut inside = Vec::new();
        let mut turned = 0;
        let at = |g: &Game, r: FormId| g.world.peek(r).map(|p| p.pos).or_else(|| g.world.placed.get(r).map(|p| p.pos));
        let gap = |a: [f32; 3]| (0..3).map(|i| (a[i] - player[i]).powi(2)).sum::<f32>().sqrt() as f64;
        for w in watchers {
            let out = |g: &Game, r: FormId| g.extra.get(&r).is_some_and(|e| e.dead || e.out_cold() || e.restrained || e.teammate);
            if out(self, w) || !self.present(w) || self.provocations.holds(w, FormId::PLAYER_REF) {
                continue;
            }
            // A Guard package's area comes first; else the aggro radius around the actor.
            let guard = self.guard_area(w).and_then(|(g, r)| Some((g, at(self, g)?, r))).map(|(g, p, r)| (Some(g), gap(p), r)).filter(|(_, d, r)| d <= r);
            let aggro = || -> Option<(Option<FormId>, f64, f64)> {
                let r = self.aggro_radius(w)?;
                let d = gap(at(self, w)?);
                (d <= r).then_some((None, d, r))
            };
            let Some((guarded, distance, radius)) = guard.or_else(aggro) else { continue };
            if matches!(self.reaction_to(w, FormId::PLAYER_REF), ALLY | FRIEND) || (guarded.is_none() && !self.package_allows_aggro(w)) {
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
                self.provocations.notes.push(match guarded {
                    Some(guarded) => Note::GuardWarned { actor: w, level: level + 1, guarded, distance, radius },
                    None => Note::AggroWarned { actor: w, level: level + 1, distance, radius },
                });
            } else {
                let why = if guarded.is_some() { Why::Guard } else { Why::Aggro };
                self.provocations.grudges.insert((w, FormId::PLAYER_REF), Grudge { why, since: now });
                self.provocations.notes.push(match guarded {
                    Some(guarded) => Note::GuardAttacks { actor: w, guarded, distance, radius },
                    None => Note::AggroAttacks { actor: w, distance, radius },
                });
                turned += 1;
            }
        }
        let grace = 2.0 * s.warning_timer;
        self.provocations.aggro_warnings.retain(|w, (_, last)| inside.contains(w) || now - *last < grace);
        // One that turned on the player fights now; it no longer warns.
        self.provocations.aggro_inside = inside.into_iter().filter(|w| !self.provocations.holds(*w, FormId::PLAYER_REF)).collect();
        turned
    }

    /// Whom `actor` is warning now (module notes, "The look"): the player while it stands
    /// inside its aggro radius between its first warning and its attack, or while it
    /// sees the player trespass after warning it (`crate::crime`) [hypothesis: the same
    /// look; the owner, 2026-10-24: "guards that are alerted/warning are supposed to draw
    /// their weapon"].
    pub fn warning_to(&self, actor: FormId) -> Option<FormId> {
        // Just turned on the player and not yet in Combat (detection settles a moment
        // later): still the warning, so the stance is not put away and drawn again
        // [design; observed in NVForever: 0.23 s, smoke `aggro-warning-look`].
        let turning = self.provocations.grudges.get(&(actor, FormId::PLAYER_REF)).is_some_and(|g| matches!(g.why, Why::Aggro | Why::Guard) && self.time - g.since < AGGRO_TURN_SECONDS) && !self.awareness.state(actor).in_combat();
        (self.provocations.aggro_inside.contains(&actor) || turning || self.crimes.warning_now.contains(&actor)).then_some(FormId::PLAYER_REF)
    }
}
