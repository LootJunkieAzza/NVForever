//! AI package choice (Layer 6): which package of its list an actor runs, and when it
//! chooses again. Facts and their sources: docs/spec-notes/packages.md §3 (documented)
//! and §4 (observed in the game); the design: docs/design/actor-behaviour.md.
//!
//! - An actor's packages are tried top to bottom; the first valid one runs
//!   ([`Game::select_package`], [`Game::check_package`]).
//! - A script choosing again (`EvaluatePackage`) is [`Game::evaluate_package`]; the
//!   game's own periodic choice is [`Game::update_package`], which lets a Must Complete
//!   package finish first ([`Game::package_goal_met`]).
//! - With [`crate::game::GameConfig::package_interval`] set, [`Game::tick`] makes the
//!   periodic choice for the actors around the player and runs the packages' begin,
//!   end and change scripts. What a package then does (travel, sandbox, …) is the next
//!   layer's work.

use crate::functions;
use crate::game::Game;
use crate::package::{flags, Clock, Day, PackageCheck, PackageDef, TargetKind};
use crate::trace::EventKind;
use fnv_sim::FormId;
use std::collections::BTreeSet;

/// The least time between two scans for actors whose choice is due, in seconds.
const SCAN_STEP: f64 = 0.25;

impl Game<'_> {
    /// The AI package list of an actor reference: its base form's `PKID` list, or its
    /// template's when the template supplies the AI packages, followed through
    /// templates and leveled actor lists as the actor's factions are
    /// ([`GameData::base_package_list`]) [documented: xEdit template flags; hypothesis:
    /// which entry of a leveled list stands for the actor, `crate::templates::Picks`].
    ///
    /// [`GameData::base_package_list`]: crate::data::GameData::base_package_list
    pub fn package_list(&self, actor: FormId) -> Vec<FormId> {
        match functions::base_of(self, actor) {
            Some(base) => self.data.base_package_list(base, actor),
            None => Vec::new(),
        }
    }

    /// The time schedules are checked against: GameHour and the calendar day (the
    /// weekday as GetDayOfWeek counts it). `None` when the load order has no time
    /// globals; schedules are then not applied.
    pub fn package_clock(&self) -> Option<Clock> {
        let c = self.world.calendar().ok()?;
        let (year, month, day) = (c.year as i64, (c.month as i64).clamp(0, 11), (c.day as i64).max(1));
        let dow = functions::misc::weekday(year, month + 1, day);
        let today = Day { day_of_week: dow as u8, month: month as u8, day: day as u8 };
        let (ym, yd) = if day > 1 {
            (month, day - 1)
        } else {
            let m = (month + 11) % 12;
            (m, fnv_sim::clock::days_in_month(if month == 0 { year - 1 } else { year }, m))
        };
        let yesterday = Day { day_of_week: ((dow + 6) % 7) as u8, month: ym as u8, day: yd as u8 };
        Some(Clock { hour: (c.hour as f64).rem_euclid(24.0), today, yesterday })
    }

    fn linked_ref(&self, actor: FormId) -> Option<FormId> {
        self.world.placed.get(actor).and_then(|p| p.linked)
    }

    /// The reference a package's conditions run on with "Run on Target": the
    /// package's first target when it names a reference, or the actor's linked
    /// reference when the target is "linked reference" [documented: GECK wiki,
    /// Category:Packages; observed: the game reported exactly that reference as the
    /// package target, 21,704 samples, packages.md §4].
    pub fn package_target(&self, actor: FormId, def: &PackageDef) -> Option<FormId> {
        match def.target.map(|t| t.kind) {
            Some(TargetKind::Reference(f)) => f,
            Some(TargetKind::LinkedReference) => self.linked_ref(actor),
            _ => None,
        }
    }

    /// Whether `actor` may choose `package` now, and if not, the first reason. Checked
    /// in this order:
    /// 1. the schedule (`Schedule::allows`) [documented: the schedule is an implicit
    ///    condition and the conditions count only while it holds; observed];
    /// 2. a package located at, or aimed at, the actor's linked reference needs one
    ///    [documented for Patrol and Ambush; observed: two coyotes without one ran
    ///    their sandbox past DefaultPatrolCasual, 15,518 samples; hypothesis for the
    ///    other types];
    /// 3. a package whose location the actor found unreachable is left out for a while
    ///    ([`Game::block_package`]) [hypothesis: what the game does with a package it
    ///    cannot carry out is not documented];
    /// 4. a Once Per Day package is not chosen again within 24 game hours of the
    ///    moment the actor chose it [documented]; one that is still running may go on
    ///    [hypothesis];
    /// 5. the conditions, with the actor as the subject.
    pub fn check_package(&mut self, actor: FormId, package: FormId) -> PackageCheck {
        let data = self.data;
        let Some(def) = data.package_defs.get(&package) else { return PackageCheck::Missing };
        if self.package_clock().is_some_and(|c| !def.schedule.allows(&c)) {
            return PackageCheck::OutOfSchedule;
        }
        if def.uses_linked_ref() && self.linked_ref(actor).is_none() {
            return PackageCheck::NoLinkedRef;
        }
        let now = self.world.calendar().map(|c| c.days_passed).unwrap_or(0.0);
        if self.extra.get(&actor).and_then(|e| e.package_blocked.get(&package)).is_some_and(|until| now < *until) {
            return PackageCheck::Unreachable;
        }
        if def.has(flags::ONCE_PER_DAY) {
            let e = self.extra.get(&actor);
            let running = e.is_some_and(|e| e.current_package == Some(package));
            let chosen = e.and_then(|e| e.package_chosen_at.get(&package)).copied();
            if !running && chosen.is_some_and(|t| now - t < 1.0) {
                return PackageCheck::OncePerDay;
            }
        }
        let target = self.package_target(actor, def);
        if self.conditions_pass(&def.conditions, Some(actor), target, &format!("PACK {package}")) {
            PackageCheck::Valid
        } else {
            PackageCheck::ConditionsFail
        }
    }

    /// The package an actor runs when it evaluates its packages.
    ///
    /// - A script package (AddScriptPackage) overrides the list [documented: catalogue
    ///   summary of AddScriptPackage; observed: RingoRef and SunnyRef ran packages that
    ///   are in no list of theirs].
    /// - Otherwise the first package of the list that is valid
    ///   ([`Game::check_package`]) [documented: GECK wiki, How Are Packages Evaluated;
    ///   observed: `L5.quest.condition_groups`, and 232,755 of 234,335 observer samples
    ///   show a package with no earlier unconditional package in its window; the rest
    ///   are the minutes a switch takes and the next rule, packages.md §4].
    /// - With no valid package the actor keeps the list package it runs [observed:
    ///   GoodspringsSettler01Ref kept GSSettlerAAMSandbox6x16 through the night, 1,466
    ///   samples outside its window, when no other package of its list applied;
    ///   hypothesis: that this is "keep the current one" rather than "the last of the
    ///   list", which are the same package there].
    pub fn select_package(&mut self, actor: FormId) -> Option<FormId> {
        if let Some(p) = self.extra.get(&actor).and_then(|e| e.packages.last()).copied() {
            return Some(p);
        }
        let list = self.package_list(actor);
        for &p in &list {
            if self.check_package(actor, p) == PackageCheck::Valid {
                return Some(p);
            }
        }
        self.extra.get(&actor).and_then(|e| e.current_package).filter(|c| list.contains(c))
    }

    /// Whether the AI runs packages itself (their begin, end and change scripts and the
    /// periodic choice) rather than leaving them to the runner's inputs.
    pub(crate) fn runs_packages(&self) -> bool {
        self.cfg.package_interval.is_some()
    }

    /// Make `package` the actor's current package. A change is traced
    /// (`package_chosen`), starts the Once Per Day clock of the new package and, when
    /// the AI runs packages, runs the old package's change script and OnPackageChange
    /// (unless it had finished) and the new one's begin script and OnPackageStart
    /// [documented: GECK wiki, Category:Packages, Begin / End / Change; hypothesis: no
    /// change event after the end event].
    pub(crate) fn set_package(&mut self, actor: FormId, package: Option<FormId>) {
        let data = self.data;
        let e = self.extra.entry(actor).or_default();
        let old = e.current_package;
        if old == package {
            return;
        }
        let finished = e.package_goal_met;
        e.current_package = package;
        e.package_goal_met = false;
        if let Some(p) = package.filter(|p| data.package_defs.get(p).is_some_and(|d| d.has(flags::ONCE_PER_DAY))) {
            let now = self.world.calendar().map(|c| c.days_passed).unwrap_or(0.0);
            self.extra.entry(actor).or_default().package_chosen_at.insert(p, now);
        }
        self.emit(EventKind::PackageChosen { actor, package });
        if self.runs_packages() {
            if let Some(o) = old.filter(|_| !finished) {
                self.package_event(actor, o, 2);
            }
            if let Some(p) = package {
                self.package_event(actor, p, 0);
            }
            // The events' scripts may have chosen again; if not, the choice stands
            // (`package_event` makes its package current, the old one included).
            let e = self.extra.entry(actor).or_default();
            if e.current_package == old {
                e.current_package = package;
            }
        }
    }

    /// `EvaluatePackage`: the actor picks its package again ([`Game::select_package`])
    /// and keeps it as its current package, whatever it was running: even a Must
    /// Complete package is dropped [documented: GECK wiki, EvaluatePackage]. Without
    /// [`crate::game::GameConfig::package_interval`] the package is not started (its
    /// begin result script, OnPackageStart); the runner's `package` input stands in.
    pub fn evaluate_package(&mut self, actor: FormId) -> Option<FormId> {
        let p = self.select_package(actor);
        self.set_package(actor, p);
        p
    }

    /// The actor's current package: the one last chosen, or chosen now when it never
    /// evaluated its packages (an actor always runs a package once loaded [hypothesis]).
    pub fn current_package(&mut self, actor: FormId) -> Option<FormId> {
        match self.extra.get(&actor).and_then(|e| e.current_package) {
            Some(p) => Some(p),
            None => self.evaluate_package(actor),
        }
    }

    /// The layer that carries packages out reports that the actor's current package
    /// reached its goal (the destination, the target found, the patrol's last point):
    /// a Must Complete or Must Reach Location package may now be replaced, and, when
    /// the AI runs packages, its end script and OnPackageDone run [documented: GECK
    /// wiki, Category:Packages].
    pub fn package_goal_met(&mut self, actor: FormId) {
        let Some(e) = self.extra.get_mut(&actor) else { return };
        let Some(p) = e.current_package.filter(|_| !e.package_goal_met) else { return };
        e.package_goal_met = true;
        if self.runs_packages() {
            self.package_event(actor, p, 1);
        }
    }

    /// The layer that carries packages out found no way to `package`'s location: the
    /// actor does not choose it for the next `hours` game hours, and chooses again now
    /// (the next valid package of its list) [hypothesis: the game's behaviour is not
    /// documented; an actor standing still for ever would be the alternative].
    pub fn block_package(&mut self, actor: FormId, package: FormId, hours: f32) -> Option<FormId> {
        let now = self.world.calendar().map(|c| c.days_passed).unwrap_or(0.0);
        let e = self.extra.entry(actor).or_default();
        e.package_blocked.retain(|_, until| *until > now);
        e.package_blocked.insert(package, now + hours / 24.0);
        // The blocked package must not be kept as "the current one" either.
        if e.current_package == Some(package) && e.packages.last() != Some(&package) {
            self.set_package(actor, None);
        }
        self.evaluate_package(actor)
    }

    /// Put an actor somewhere without a trace event: the layer that moves actors calls
    /// this as they walk (a script's MoveTo is traced; a step is not).
    pub fn set_actor_place(&mut self, actor: FormId, cell: Option<FormId>, pos: [f32; 3], heading: f32) -> Result<(), String> {
        let s = self.world.get(actor).map_err(|e| e.to_string())?;
        s.cell = cell;
        s.pos = pos;
        s.rot[2] = heading;
        if self.ref_script(actor).is_some() {
            self.moved.insert(actor);
        }
        Ok(())
    }

    /// Why the game's own periodic choice would leave the current package alone, if
    /// it would: the package is Must Complete or Must Reach Location and has not met
    /// its goal, or it is Continue If PC Near and the player is in the actor's cell
    /// [documented: GECK wiki, Category:Packages flags].
    pub fn package_held(&self, actor: FormId) -> Option<&'static str> {
        let e = self.extra.get(&actor)?;
        let def = self.data.package_defs.get(&e.current_package?)?;
        if def.has(flags::MUST_COMPLETE | flags::MUST_REACH_LOCATION) && !e.package_goal_met {
            return Some(if def.has(flags::MUST_COMPLETE) { "must complete" } else { "must reach location" });
        }
        let cell = self.cell_of(actor);
        if def.has(flags::CONTINUE_IF_PC_NEAR) && cell.is_some() && cell == self.cell_of(FormId::PLAYER_REF) {
            return Some("continue if PC near");
        }
        None
    }

    /// The periodic choice the game makes for an actor (about every 10 seconds
    /// [documented: "seem to be", GECK wiki; observed: actors switched 0.4 to 26 real
    /// seconds after a schedule boundary]): choose again unless the current package is
    /// held ([`Game::package_held`]).
    pub fn update_package(&mut self, actor: FormId) -> Option<FormId> {
        if self.package_held(actor).is_some() {
            return self.extra.get(&actor).and_then(|e| e.current_package);
        }
        self.evaluate_package(actor)
    }

    /// Placed actors whose packages the periodic choice covers: those in loaded cells
    /// and those that already run a package [hypothesis: the game goes on choosing for
    /// actors it has started; observed: actors in unloaded interiors changed package at
    /// their schedule, together, ai-observer.md §7].
    fn package_actors(&self) -> BTreeSet<FormId> {
        let placed = self.world.placed;
        let is_actor = |r: &FormId| placed.get(*r).is_some_and(|p| matches!(&p.sig, b"ACHR" | b"ACRE"));
        let mut out: BTreeSet<FormId> = BTreeSet::new();
        for c in &self.loaded_cells {
            out.extend(placed.by_cell.get(c).into_iter().flatten().filter(|r| is_actor(r)));
        }
        out.extend(self.moved.iter().filter(|r| is_actor(r)));
        out.retain(|r| self.cell_of(*r).is_some_and(|c| self.loaded_cells.contains(&c)));
        out.extend(self.extra.iter().filter(|(r, e)| e.current_package.is_some() && is_actor(r)).map(|(r, _)| *r));
        out
    }

    /// Every actor the periodic choice covers chooses now, as at its turn
    /// ([`Game::update_package`]), with the packages' begin, end and change scripts; its
    /// turns in the interval stay as they were. For game time that passes while a menu
    /// holds the game (an hour of a wait or a sleep): the periodic choice goes by
    /// [`Game::time`], which stands still there. [engineering choice: what the original's
    /// actors do while the player waits is not described by any source read;
    /// docs/spec-notes/wait-menu.md §7.]
    pub fn choose_packages_now(&mut self) {
        if !self.runs_packages() {
            return;
        }
        for a in self.package_actors() {
            if self.present(a) && !self.extra.get(&a).is_some_and(|e| e.dead) {
                self.update_package(a);
            }
        }
    }

    /// The periodic package choice, from [`Game::tick`]: every actor chooses when it is
    /// first seen and then every `package_interval` seconds. Actors do not all choose
    /// in the same frame: each has a fixed phase within the interval, from its FormID
    /// [our choice; observed: switch delays after a boundary are spread, not equal].
    pub(crate) fn tick_packages(&mut self) {
        let Some(interval) = self.cfg.package_interval.filter(|i| *i > 0.0) else { return };
        if self.time < self.package_scan_due {
            return;
        }
        let now = self.time;
        let mut next = now + interval;
        for a in self.package_actors() {
            if !self.present(a) || self.extra.get(&a).is_some_and(|e| e.dead) {
                continue;
            }
            let due = match self.extra.get(&a).and_then(|e| e.package_due) {
                Some(d) => d,
                None => {
                    // First sight: choose now, then keep to the actor's phase.
                    let phase = (a.0.wrapping_mul(2_654_435_761) >> 22) as f64 / 1024.0;
                    self.update_package(a);
                    now + interval * phase
                }
            };
            let due = if now >= due {
                self.update_package(a);
                due + interval * ((now - due) / interval).floor().max(0.0) + interval
            } else {
                due
            };
            self.extra.entry(a).or_default().package_due = Some(due);
            next = next.min(due);
        }
        self.package_scan_due = next.max(now + SCAN_STEP);
    }
}
