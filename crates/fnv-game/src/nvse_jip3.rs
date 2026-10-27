//! JIP LN, JohnnyGuitar and LU functions the owner's mod lists call that no earlier set
//! built (the third batch, 2026-10-27; docs/handoff/modlist-function-counts.json): ash
//! piles, ragdolls, the compass and nearby hostiles, quests, the player's doors and
//! cells, effect shaders and inventory references moved [documented: their pages in the
//! wiki export; observed where a set is named].

use crate::game::{Game, RunCtx};
use fnv_script::vm::XVal;
use fnv_sim::FormId;
use std::collections::HashMap;

/// This file's session state (not saved).
#[derive(Debug, Default)]
pub struct Jip3State {
    /// The last exterior door the player used (GetPCLastExteriorDoor).
    pub last_exterior_door: Option<FormId>,
    /// The player's cell when each script last asked GetPCMovedCell, by the script's
    /// calling reference and quest.
    pub moved_seen: HashMap<(Option<FormId>, Option<FormId>), Option<FormId>>,
}

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(r) if *r != 0 => Some(FormId(*r)),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(n)) => *n,
        _ => dflt,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(if b { 1.0 } else { 0.0 })
}

impl Game<'_> {
    /// The player used door `door`: kept when it stands outside (GetPCLastExteriorDoor).
    pub(crate) fn note_player_door(&mut self, door: FormId) {
        let outside = self.cell_of(door).is_some_and(|c| self.world.placed.world_of(c).is_some());
        if outside {
            self.gaps.jip3.last_exterior_door = Some(door);
        }
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let v = match name {
        // The actor an ash or goo pile came from [documented: GetAshPileSource].
        "getashpilesource" => XVal::Ref(subject.and_then(|p| g.ash_pile_source(p)).map_or(0, |a| a.0)),
        // Knocked down (unconscious, pushed away) or dead and lying as a body
        // [documented: GetIsRagdolled; hypothesis: a body counts].
        "getisragdolled" => yes(subject.is_some_and(|a| g.knockout(a).is_some() || g.is_dead(a))),
        // JohnnyGuitar: among the compass's hostiles (`nvse_hud`) [documented:
        // IsCompassHostile].
        "iscompasshostile" => yes(subject.is_some_and(|a| g.compass_hostiles().iter().any(|(r, _)| *r == a))),
        // JohnnyGuitar: a hostile within fHostileActorExteriorDistance (interior:
        // ...InteriorDistance) of the player [documented: IsHostilesNearby].
        "ishostilesnearby" => {
            let inside = g.cell_of(FormId::PLAYER_REF).is_some_and(|c| g.world.placed.world_of(c).is_none());
            let key = if inside { "fHostileActorInteriorDistance" } else { "fHostileActorExteriorDistance" };
            let reach = g.world.setting_f32(key).unwrap_or(0.0);
            yes(g.compass_hostiles().iter().any(|(_, d)| *d <= reach))
        }
        // `quest`: failed, as a Fail Quest stage leaves it (also read as completed)
        // [documented: FailQuest; the completed flag as `Game::set_stage` sets it].
        "failquest" => {
            if let Some(q) = form(args, 0) {
                let s = g.quest(q);
                let first = !s.failed;
                s.failed = true;
                s.completed = true;
                if first {
                    g.emit(crate::trace::EventKind::QuestFailed { quest: q });
                }
            }
            zero
        }
        "getpclastexteriordoor" => XVal::Ref(g.gaps.jip3.last_exterior_door.map_or(0, |d| d.0)),
        // 1 when the player's cell changed since this script last asked; the first call
        // reads 0 [documented: GetPCMovedCell; observed: jip3a, 0 then 0 in one cell].
        "getpcmovedcell" => {
            let now = g.cell_of(FormId::PLAYER_REF);
            let key = (ctx.self_ref, ctx.quest);
            let before = g.gaps.jip3.moved_seen.insert(key, now);
            yes(before.is_some_and(|b| b != now))
        }
        // `shader`: how many times it plays on the reference, from the scripts' plays
        // [documented: HasEffectShader; hypothesis: a magic effect's shader not counted].
        "haseffectshader" => {
            let (Some(r), Some(s)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            XVal::Num(g.script_looks.shaders.iter().filter(|p| p.on == r && p.shader == s).count() as f64)
        }
        // JohnnyGuitar: interacting with it is a crime or an attack: an actor hostile to
        // the player, or something someone else owns [documented: IsCrimeOrEnemy, "the
        // same checks that are used to determine red color of the crosshair"].
        "iscrimeorenemy" => {
            let Some(r) = subject else { return Some(Ok(zero)) };
            let enemy = g.crime_enemy(r) || crate::rest::hostile_to_player(g, r);
            yes(enemy || g.owned_by_other(r).is_some())
        }
        // `[count] [clearOwner] [target]`: the inventory reference's items leave their
        // container, to `target` when given [documented: RemoveMeIRAlt; count <= 0 or
        // none is the whole stack].
        "removemeiralt" => {
            let Some((c, item)) = subject.and_then(|r| g.temp_item(r)) else { return Some(Ok(zero)) };
            let have = g.world.inventory_mut(c).map(|inv| inv.count(item)).unwrap_or(0);
            let want = num(args, 0, 0.0) as i32;
            let n = if want <= 0 { have } else { want.min(have) };
            let gone = g.world.remove_item(c, item, n).unwrap_or(0);
            if let (Some(to), true) = (form(args, 2), gone > 0) {
                let _ = g.world.add_item(to, item, gone);
            }
            zero
        }
        // `[count] [clearOwner]`: the inventory reference's items dropped beside their
        // container, as Drop does [documented: DropMeAlt].
        "dropmealt" => {
            let Some((c, item)) = subject.and_then(|r| g.temp_item(r)) else { return Some(Ok(zero)) };
            let have = g.world.inventory_mut(c).map(|inv| inv.count(item)).unwrap_or(0);
            let want = num(args, 0, 0.0) as i32;
            let n = if want <= 0 { have } else { want.min(have) };
            if n > 0 {
                if let Err(e) = crate::functions::console::call_words(g, Some(c), "Drop", &[format!("{:08X}", item.0), n.to_string()], "DropMeAlt") {
                    return Some(Err(e));
                }
            }
            zero
        }
        // `container [clearOwner]`: a placed item goes into the container [documented:
        // MoveToContainer].
        "movetocontainer" => {
            let (Some(r), Some(to)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            let Some(item) = g.world.peek(r).map(|s| s.base) else { return Some(Ok(zero)) };
            let count = g.world.peek(r).map_or(1, |s| s.count.max(1));
            if g.world.add_item(to, item, count).is_ok() {
                if let Ok(s) = g.world.get(r) {
                    s.enabled = false;
                    s.deleted = true;
                }
            }
            zero
        }
        // LU `flag [actorBase]`: a Template Data flag (1 Traits … 512 Script). Given a base
        // as the argument it reads 0, with or without a calling reference; on a reference,
        // its base's ACBS template flags when the base has a template (TPLT) [documented:
        // GetActorTemplateFlag; observed: jip3a, six bases as the argument all 0; jip3b,
        // a placed PowderGangerVault19TEMPLATE reads 942 on the reference (its base is one
        // the game made, not the record, whose flags are 128) and 0 with the base given;
        // hypothesis: references placed from the plugins read their record's flags].
        "getactortemplateflag" => {
            if form(args, 1).is_some() {
                return Some(Ok(zero));
            }
            let Some(b) = subject.map(|r| crate::functions::base_of(g, r).unwrap_or(r)) else { return Some(Ok(zero)) };
            if g.data.actors.get(&b).and_then(|a| a.template).is_none_or(|t| t.0 == 0) {
                return Some(Ok(zero));
            }
            let flags = crate::nvse::record_value(g, b, "ACBS", "template_flags").and_then(|v| v.num().ok()).unwrap_or(0.0) as u64;
            yes(flags & (num(args, 0, 0.0) as u64) != 0)
        }
        // The base's Immobile flag (ACBS bit 23) [documented: GetIsImmobile, "incapable of
        // moving"; the bit, the schema's ACBS flags].
        "getisimmobile" => {
            let Some(b) = form(args, 0).or_else(|| subject.map(|r| crate::functions::base_of(g, r).unwrap_or(r))) else { return Some(Ok(zero)) };
            let flags = crate::nvse::record_value(g, b, "ACBS", "flags").and_then(|v| v.num().ok()).unwrap_or(0.0) as u64;
            yes(flags & (1 << 23) != 0)
        }
        // LU `list`: the calling actor's race is in the form list [documented: IsRaceInList].
        "israceinlist" => {
            let (Some(a), Some(l)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            let race = g.actor_race(a);
            yes(race.is_some_and(|r| crate::functions::list_members(g, l).contains(&r)))
        }
        // `menu`: the reference the open menu is about; the dialogue menu's speaker (1009)
        // [documented: GetMenuTargetRef; hypothesis: the other menus' targets are not kept
        // by the rules and read 0].
        "getmenutargetref" => {
            let code = num(args, 0, 0.0) as u16;
            let r = match code {
                crate::game::menus::DIALOGUE => g.dialogue.as_ref().map(|d| d.actor),
                _ => None,
            };
            XVal::Ref(r.filter(|_| g.menu_open(code)).map_or(0, |r| r.0))
        }
        // `quest seconds`: the quest's script runs after `seconds` more (0: next frame)
        // [documented: SetDelayElapsed; hypothesis: it returns the seconds given].
        "setdelayelapsed" => {
            let Some(q) = form(args, 0) else { return Some(Ok(zero)) };
            let secs = num(args, 1, 0.0).max(0.0);
            let delay = g.quests.get(&q).and_then(|s| s.delay).or_else(|| g.data.quests.get(&q).map(|d| d.script_delay())).unwrap_or(0.0);
            let s = g.quest(q);
            s.since_run = (delay - secs).max(0.0);
            XVal::Num(secs)
        }
        // The open menu's item list is drawn afresh each frame here [design: nothing to
        // refresh] [documented: RefreshItemsList].
        "refreshitemslist" => zero,
        // `mode`: 0 god mode (1 while on), 1 the actors' AI (1 while it runs, 0 after
        // `tai`) [documented: GetDebugModeState; observed: jip3a, 0 then 1 across `tgm`,
        // 1 then 0 then 1 across `tai`; hypothesis: 2 combat AI, 3 collision, 4 borders
        // read 0].
        "getdebugmodestate" => match num(args, 0, -1.0) as i64 {
            0 => yes(g.toggles.god_mode),
            1 => yes(!g.toggles.actors_ai_off),
            _ => zero,
        },
        _ => return None,
    };
    Some(Ok(v))
}
