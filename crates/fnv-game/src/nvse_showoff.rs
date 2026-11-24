//! ShowOff functions the mod lists call that no earlier file built
//! (docs/handoff/modlist-function-counts.json; docs/spec-notes/ext-showoff.md)
//! [documented: their pages; observed: showoff1 in the original game, 2026-10-27].
//! ShowOff:OnAdd fires where items reach a container (`Game::item_event`),
//! ShowOff:OnExplosionHit where the engine's blast reaches an actor
//! ([`Game::explosion_hit`]), and the aux timers' stop handlers where a timer runs out
//! (`Game::tick`) or is stopped with its event.

use crate::game::{Game, RunCtx};
use fnv_script::vm::XVal;
use fnv_sim::FormId;

/// The events' names as SetEventHandler takes them, lower case.
pub const ON_ADD: &str = "showoff:onadd";
pub const ON_EXPLOSION_HIT: &str = "showoff:onexplosionhit";
pub const ON_PRE_SCRIPTED_ACTIVATE: &str = "showoff:onprescriptedactivate";
pub const ON_PRE_ACTIVATE_INVENTORY_ITEM: &str = "showoff:onpreactivateinventoryitem";
pub const ON_PRE_ACTIVATE_INVENTORY_ITEM_ALT: &str = "showoff:onpreactivateinventoryitemalt";
pub const ON_POV_CHANGE: &str = "showoff:onpovchange";
pub const ON_COMPLETE_OBJECTIVE: &str = "showoff:oncompleteobjective";
pub const ON_DISPLAY_OBJECTIVE: &str = "showoff:ondisplayobjective";
pub const ON_WEAPON_UNHOLSTER: &str = "showoff:onweaponunholster";

#[derive(Debug, Default)]
pub struct ShowOffState {
    /// Batch 5's state: the level-up and pause menu asks, live explosions, the shot
    /// switches (`nvse_showoff5`).
    pub b5: crate::nvse_showoff5::State,
    /// The item a ShowOff:OnAdd handler is running for (GetAddedItemRefShowOff).
    pub added: Option<FormId>,
    /// The damage of the explosion a ShowOff:OnExplosionHit handler is running for.
    pub explosion_damage: Option<f64>,
    /// SetOnAuxTimerStopHandler's handlers: (the function, the timer's name lower case,
    /// its owner).
    pub timer_stops: Vec<(XVal, String, FormId)>,
    /// SetOnAuxTimerStartHandler's and SetOnAuxTimerUpdateHandler's handlers, as
    /// `timer_stops` (`nvse_showoff3`).
    pub timer_starts: Vec<(XVal, String, FormId)>,
    pub timer_updates: Vec<(XVal, String, FormId)>,
    /// Start and update handlers set or removed this frame, applied at the next frame's
    /// aux timer update: (start, set, the function, the name lower case, the owner)
    /// (`nvse_showoff3`).
    pub pending_handlers: Vec<(bool, bool, XVal, String, FormId)>,
    /// Each running aux timer's countdown and flags as AuxTimerStart gave them, by owner
    /// and name lower case (AuxTimerTimeToCountdown; the update flag 256).
    pub countdowns: std::collections::HashMap<(FormId, String), (f64, u32)>,
    /// The game time of the last aux timer update (`Game::aux_timer_updates`).
    pub last_update: Option<f64>,
    /// The seeded randomizer's seeds, by the calling plugin lower case.
    pub seeds: std::collections::HashMap<String, crate::nvse_showoff3::Seeded>,
    /// SetAllyTemp's and SetEnemyTemp's reactions: (faction, other) to 0 neutral, 1 enemy,
    /// 2 ally, 3 friend (`functions::actor::reaction`).
    pub temp_reactions: std::collections::HashMap<(FormId, FormId), i64>,
    /// SetRadiationExtraData's levels, by reference (`Game::radiation_markers_in`),
    /// carried across same-run loads in `Game::Session`.
    pub radiation: std::collections::HashMap<FormId, f32>,
    /// The plugin (lower case) that started each aux timer, by owner and name
    /// (ClearShowoffSavedData, `nvse_showoff4`).
    pub timer_mods: std::collections::HashMap<(FormId, String), String>,
    /// SetShowOffOnCornerMessageEventHandler's functions, and the corner messages queued
    /// since the last frame for them: (text, icon path, seconds) (`nvse_showoff4`).
    pub corner_handlers: Vec<XVal>,
    pub corner_pending: Vec<(String, String, f64)>,
    /// SetItemHotkeyIconPath's paths, by item base.
    pub hotkey_icons: std::collections::HashMap<FormId, String>,
    /// FreezeAmmoRegen: no regenerating weapon gains rounds (`fnv_combat::attack::Cycle`).
    pub ammo_regen_frozen: bool,
    /// SetOwnershipTemp's targets (a reference or a cell) and the owner each had before:
    /// a save writes that one (`Game::snapshot`); SetOwnership or SetCellOwnership on the
    /// target drops the entry [documented: SetOwnershipTemp, session persistent].
    pub temp_owners: std::collections::HashMap<FormId, Option<FormId>>,
    /// Re-entrancy guard while `ShowOff:OnPreActivateInventoryItem[Alt]` handlers run,
    /// so a handler calling `EquipItem` / `UseAidItem` does not recurse infinitely [design].
    pub in_pre_activate_item: bool,
    /// Actors whose draw-start `ShowOff:OnWeaponUnholster` has already fired while
    /// `weapon_out` is still `false`, preventing duplicate firing when draw finishes.
    /// Value is `None` while the draw is active (`AnimAction::Equip` or scripted
    /// `SetWeaponOut 1` pending in `weapon_due`), or `Some(frame)` if `Equip` ended
    /// before `weapon_out` became `true` so same-frame completion still deduplicates
    /// while a later-frame cancelled draw clears cleanly [design].
    pub unholster_pending: std::collections::HashMap<FormId, Option<u64>>,
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

impl Game<'_> {
    /// ShowOff:OnAdd, after an item reached a container: the handlers get the item's base
    /// and the container, and GetAddedItemRefShowOff gives an inventory reference to it
    /// [observed: showoff1, a stimpak AddItem'd to the player: a reference whose base is
    /// type 47; the two-argument handler got the base (47) and the player; a handler
    /// taking no arguments runs too].
    pub(crate) fn showoff_on_add(&mut self, to: FormId, item: FormId) {
        if !self.handled(ON_ADD) {
            return;
        }
        let r = self.temp_ref(to, item);
        let before = self.showoff.added.replace(r);
        self.dispatch_event("ShowOff:OnAdd", vec![XVal::Ref(item.0), XVal::Ref(to.0)], Some(to));
        self.showoff.added = before;
    }

    /// ShowOff:OnExplosionHit, before a blast's damage reaches an actor: the handlers get
    /// the actor and none, read the damage (GetExplosionHitDamage) and may change it
    /// (SetExplosionHitDamage). Returns the damage to deal [observed: showoff1, a
    /// MineFragExplosion at the player read 100, and set to 5 the player lost 5 Health;
    /// hypothesis: the damage is the blast's before armour, as the engine's `b.damage`].
    pub fn explosion_hit(&mut self, target: FormId, damage: f64) -> f64 {
        if !self.handled(ON_EXPLOSION_HIT) {
            return damage;
        }
        let before = self.showoff.explosion_damage.replace(damage);
        self.dispatch_event("ShowOff:OnExplosionHit", vec![XVal::Ref(target.0), XVal::Ref(0)], Some(target));
        let out = self.showoff.explosion_damage.unwrap_or(damage);
        self.showoff.explosion_damage = before;
        out
    }

    /// Resolve an `EXPL` form's live damage (`Game::explosion_traits`, including
    /// `SetExplosionTraitNumeric` trait 1) and run `ShowOff:OnExplosionHit` on `target`.
    pub fn explosion_hit_damage(&mut self, explosion: FormId, target: FormId) -> Option<f64> {
        let damage = f64::from(self.explosion_traits(explosion)?.damage);
        Some(self.explosion_hit(target, damage))
    }

    /// Whether any `ShowOff:OnPreActivateInventoryItem` or
    /// `ShowOff:OnPreActivateInventoryItemAlt` handler is registered.
    pub fn has_pre_activate_inventory_handlers(&self) -> bool {
        self.handled(ON_PRE_ACTIVATE_INVENTORY_ITEM)
            || self.handled(ON_PRE_ACTIVATE_INVENTORY_ITEM_ALT)
    }

    /// `ShowOff:OnPreActivateInventoryItem` followed by
    /// `ShowOff:OnPreActivateInventoryItemAlt` before a player inventory, hotkey,
    /// quick-use, or stats-page item activation. An explicit numeric `SetFunctionValue 0`
    /// prevents vanilla activation (sticky across handlers). Free when no handlers are
    /// registered. See `docs/spec-notes/preactivation-events.md`.
    pub fn pre_activate_inventory_item(
        &mut self,
        actor: FormId,
        item: FormId,
        key: i32,
        equipping: bool,
    ) -> bool {
        if self.showoff.in_pre_activate_item || !self.has_pre_activate_inventory_handlers() {
            return true;
        }
        self.showoff.in_pre_activate_item = true;
        let inv_ref = self.temp_ref(actor, item);
        let mut allowed = true;
        if self.handled(ON_PRE_ACTIVATE_INVENTORY_ITEM) {
            allowed = self.dispatch_pre_event(
                "ShowOff:OnPreActivateInventoryItem",
                Some(actor),
                Some(actor),
                allowed,
                |allowed| {
                    vec![
                        XVal::Ref(item.0),
                        XVal::Ref(inv_ref.0),
                        XVal::Num(if allowed { 1.0 } else { 0.0 }),
                        XVal::Num(f64::from(key)),
                    ]
                },
            );
        }
        if self.handled(ON_PRE_ACTIVATE_INVENTORY_ITEM_ALT) {
            allowed = self.dispatch_pre_event(
                "ShowOff:OnPreActivateInventoryItemAlt",
                Some(actor),
                Some(actor),
                allowed,
                |allowed| {
                    vec![
                        XVal::Ref(item.0),
                        XVal::Ref(inv_ref.0),
                        XVal::Num(if allowed { 1.0 } else { 0.0 }),
                        XVal::Num(f64::from(key)),
                        XVal::Num(if equipping { 1.0 } else { 0.0 }),
                    ]
                },
            );
        }
        self.showoff.in_pre_activate_item = false;
        allowed
    }

    /// `ShowOff:OnPreActivateInventoryItemAlt` for scripted `EquipItem` / `UseAidItem`.
    /// Free when no `ShowOff:OnPreActivateInventoryItemAlt` handler is registered.
    pub(crate) fn pre_activate_inventory_item_alt(
        &mut self,
        actor: FormId,
        item: FormId,
        key: i32,
        equipping: bool,
    ) -> bool {
        if self.showoff.in_pre_activate_item || !self.handled(ON_PRE_ACTIVATE_INVENTORY_ITEM_ALT) {
            return true;
        }
        self.showoff.in_pre_activate_item = true;
        let inv_ref = self.temp_ref(actor, item);
        let allowed = self.dispatch_pre_event(
            "ShowOff:OnPreActivateInventoryItemAlt",
            Some(actor),
            Some(actor),
            true,
            |allowed| {
                vec![
                    XVal::Ref(item.0),
                    XVal::Ref(inv_ref.0),
                    XVal::Num(if allowed { 1.0 } else { 0.0 }),
                    XVal::Num(f64::from(key)),
                    XVal::Num(if equipping { 1.0 } else { 0.0 }),
                ]
            },
        );
        self.showoff.in_pre_activate_item = false;
        allowed
    }

    /// `ShowOff:OnPreScriptedActivate` before a scripted `Activate` runs its block or
    /// default action. Calling reference is `target`; arguments are `(actor,
    /// run_on_activate_block, allowed)` [observed 2026-11-23: Fallout: New Vegas
    /// 1.4.0.525, xNVSE 6.4.9, ShowOffNVSE 184, 10 official base masters,
    /// `so_31_pre_scripted_activate_args_veto` / `so_31b_pre_scripted_activate_noveto`:
    /// `Activate Player 0` passes `(0, 1)`, `Activate Player 1` passes `(1, 1)`, and
    /// `SetFunctionValue 0` vetoes pickup; no DLL or source consulted].
    /// See `docs/spec-notes/preactivation-events.md`.
    pub(crate) fn pre_scripted_activation_allows(
        &mut self,
        actor: FormId,
        target: FormId,
        run_block: bool,
    ) -> bool {
        if !self.handled(ON_PRE_SCRIPTED_ACTIVATE) {
            return true;
        }
        self.dispatch_pre_event(
            "ShowOff:OnPreScriptedActivate",
            Some(target),
            Some(actor),
            true,
            |allowed| {
                vec![
                    XVal::Ref(actor.0),
                    XVal::Num(if run_block { 1.0 } else { 0.0 }),
                    XVal::Num(if allowed { 1.0 } else { 0.0 }),
                ]
            },
        )
    }

    /// `ShowOff:OnPOVChange`, fired when the player's camera perspective transitions
    /// between first-person (`1`) and third-person (`0`) [documented: GECK wiki
    /// ShowOff:OnPOVChange].
    pub(crate) fn showoff_on_pov_change(&mut self, first_person: bool) {
        if !self.handled(ON_POV_CHANGE) {
            return;
        }
        self.dispatch_event(
            "ShowOff:OnPOVChange",
            vec![XVal::Num(if first_person { 1.0 } else { 0.0 })],
            None,
        );
    }

    /// `ShowOff:OnDisplayObjective`, fired PRE-update when a quest objective transitions
    /// from hidden to displayed (`GetObjectiveDisplayed` still reads `0` inside the
    /// callback) [documented: GECK wiki ShowOff:OnDisplayObjective; observed: Fallout:
    /// New Vegas 1.4.0.525 + xNVSE 6.4.9 + ShowOffNVSE Plugin 184, 2026-11-23,
    /// GSDocMitchellHouse (0x00103DF9), 10 official base masters, probes
    /// `w1_3656_objectives_unholster_imod` (`StartQuest 0x000E282D`,
    /// `SetObjectiveDisplayed 20 1`/`20 1`/`20 0`/`20 1`/`999 1` + `CompleteQuest`) and
    /// `b3_so_ex_06_on_display_complete_objective` (`SetObjectiveDisplayed 0x00104C1C 10
    /// 1` -> `reg=1,hit=1,dispInside=0`) in #3679]. Arguments are `(quest,
    /// objective_index)`; calling reference is `None`. Free when no handler is
    /// registered.
    pub(crate) fn showoff_on_display_objective(&mut self, quest: FormId, index: i32) {
        if !self.handled(ON_DISPLAY_OBJECTIVE) {
            return;
        }
        self.dispatch_event(
            "ShowOff:OnDisplayObjective",
            vec![XVal::Ref(quest.0), XVal::Num(f64::from(index))],
            None,
        );
    }

    /// `ShowOff:OnCompleteObjective`, fired PRE-update when a currently displayed quest
    /// objective transitions from incomplete to completed (`GetObjectiveCompleted` still
    /// reads `0` inside the callback; hidden objectives and completed quests do not fire)
    /// [documented: GECK wiki ShowOff:OnCompleteObjective; observed: Fallout: New Vegas
    /// 1.4.0.525 + xNVSE 6.4.9 + ShowOffNVSE Plugin 184, 2026-11-23, GSDocMitchellHouse
    /// (0x00103DF9), 10 official base masters, probe `w1_3656_objectives_unholster_imod`
    /// in #3679 (`StartQuest 0x000E282D`, `SetObjectiveCompleted` 20/30 hidden +
    /// `SetObjectiveDisplayed 50 1`/`5 1` + `CompleteAllObjectives` firing 50 then 5 +
    /// `CompleteQuest`)]. Arguments are `(quest, objective_index)`; calling reference is
    /// `None`. Free when no handler is registered.
    pub(crate) fn showoff_on_complete_objective(&mut self, quest: FormId, index: i32) {
        if !self.handled(ON_COMPLETE_OBJECTIVE) {
            return;
        }
        self.dispatch_event(
            "ShowOff:OnCompleteObjective",
            vec![XVal::Ref(quest.0), XVal::Num(f64::from(index))],
            None,
        );
    }

    /// Mark an actor as starting to draw its weapon while `weapon_out == false` and fire
    /// `ShowOff:OnWeaponUnholster` once at draw start (PRE-update, `IsWeaponOut == 0`).
    /// Subsequent draw-completion calls (`set_weapon_out(actor, true)` or `hud_tick`)
    /// consume `unholster_pending` without double-firing [design].
    pub(crate) fn showoff_begin_unholster(&mut self, actor: FormId) {
        if self.extra.get(&actor).is_some_and(|e| e.weapon_out) {
            return;
        }
        // If an earlier Equip ended on a prior frame without completing, treat this as a
        // fresh draw attempt.
        if let Some(Some(ended)) = self.showoff.unholster_pending.get(&actor).copied() {
            if ended < self.frame {
                self.showoff.unholster_pending.remove(&actor);
            }
        }
        if let std::collections::hash_map::Entry::Vacant(e) = self.showoff.unholster_pending.entry(actor) {
            e.insert(None);
            self.showoff_on_weapon_unholster(actor);
        }
    }

    /// `ShowOff:OnWeaponUnholster`, fired at weapon draw start PRE-update so
    /// `IsWeaponOut` still reads `0` inside the callback [documented: GECK wiki
    /// ShowOff:OnWeaponUnholster; observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 +
    /// ShowOffNVSE Plugin 184, 2026-11-23, GSDocMitchellHouse (0x00103DF9), 10 official
    /// base masters, probe `w1_3656_objectives_unholster_imod` in #3679
    /// (`Player.SetWeaponOut 0` / `Player.SetWeaponOut 1` with `Weap10mmPistol`
    /// `0x0000434F` -> `self=0x14, arg1=0x434F, IsWeaponOut=0`; unarmed
    /// `Player.SetWeaponOut 1` -> `self=0x14, arg1=0, IsWeaponOut=0`)].
    /// Calling reference (`GetSelf` / `GetSelfAlt` / `0::`) is `actor`; single argument
    /// (`"first"` / `1::`) is the actor's equipped or engine-held `WEAP` base form, or
    /// `0` when unarmed. Free when no handler is registered.
    pub(crate) fn showoff_on_weapon_unholster(&mut self, actor: FormId) {
        if !self.handled(ON_WEAPON_UNHOLSTER) {
            return;
        }
        let weapon = crate::functions::items::equipped_weapon(self, actor)
            .or_else(|| self.extra.get(&actor).and_then(|e| e.held_weapon))
            .unwrap_or(FormId(0));
        self.dispatch_event("ShowOff:OnWeaponUnholster", vec![XVal::Ref(weapon.0)], Some(actor));
    }

    /// The stop handlers of an aux timer that ran out or was stopped with its event: each
    /// gets the timer's name and its owner [documented: SetOnAuxTimerStopHandler; observed:
    /// showoff1, once when a 0.1 s timer ran out and once on AuxTimerStop 1, the owner the
    /// player].
    pub(crate) fn aux_timer_stopped(&mut self, owner: FormId, name: &str) {
        let hs: Vec<XVal> = self.showoff.timer_stops.iter().filter(|(_, n, o)| n == name && *o == owner).map(|(f, _, _)| f.clone()).collect();
        for f in hs {
            let ctx = RunCtx { self_ref: Some(owner), ..RunCtx::default() };
            if let Err(error) = self.run_function(&f, Some(fnv_script::vm::Value::Ref(owner.0)), vec![XVal::Str(name.to_string()), XVal::Ref(owner.0)], None, ctx) {
                self.emit(crate::trace::EventKind::ScriptError { unit: "aux timer stop handler".into(), error });
            }
        }
    }
}

/// The functions; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let v = match name {
        // Outside a ShowOff:OnAdd handler none [documented: only inside one].
        "getaddeditemrefshowoff" => XVal::Ref(g.showoff.added.map_or(0, |r| r.0)),
        // Outside a ShowOff:OnExplosionHit handler 0 and nothing [documented: unusable
        // outside it].
        "getexplosionhitdamage" => XVal::Num(g.showoff.explosion_damage.unwrap_or(0.0)),
        "setexplosionhitdamage" => {
            if g.showoff.explosion_damage.is_some() {
                g.showoff.explosion_damage = Some(num(args, 0, 0.0));
            }
            XVal::Num(0.0)
        }
        // `SetOnAuxTimerStopHandler 1/0 handler timerName [owner]`, the owner the player when
        // none [documented].
        "setonauxtimerstophandler" | "setontimerstophandler" => {
            let set = num(args, 0, 1.0) != 0.0;
            let Some(f) = args.get(1).cloned() else { return Some(Ok(XVal::Num(0.0))) };
            let timer = match args.get(2) {
                Some(XVal::Str(s)) => s.to_ascii_lowercase(),
                _ => String::new(),
            };
            let owner = match args.get(3) {
                Some(XVal::Ref(r)) if *r != 0 => FormId(*r),
                _ => subject.filter(|_| args.len() < 4).unwrap_or(FormId::PLAYER_REF),
            };
            g.showoff.timer_stops.retain(|(h, n, o)| !(*h == f && *n == timer && *o == owner));
            if set {
                g.showoff.timer_stops.push((f, timer, owner));
            }
            XVal::Num(1.0)
        }
        // 1 when the actor has a weapon out, 0 with it away [observed: showoff1, the 10mm
        // pistol]. A weapon without a jam animation gives 0 [documented]; which weapons have
        // one is not read yet, so every drawn weapon counts [hypothesis], and the engine does
        // not play the animation yet [not built].
        "forceweaponjamanim" | "forcejamanim" => {
            let a = subject.unwrap_or(FormId::PLAYER_REF);
            let out = g.extra.get(&a).is_some_and(|e| e.weapon_out) && crate::functions::items::equipped_weapon(g, a).is_some();
            XVal::Num(f64::from(u8::from(out)))
        }
        _ => return None,
    };
    Some(Ok(v))
}
