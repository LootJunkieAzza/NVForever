//! Player functions: experience, perks, notes, tag skills, achievements, controls.

use super::{none, yes, Args, Ctx};
use crate::game::{FastTravel, Game};
use fnv_script::vm::Value;
use fnv_sim::FormId;

pub fn reward_xp(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.xp += a.num(0)?.trunc();
    none()
}

/// Perks are kept for the player only; other actors have none [hypothesis: NPC perk
/// lists are not read yet].
pub fn has_perk(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    yes(r == FormId::PLAYER_REF && g.player.perks.contains_key(&a.form(0)?))
}

pub fn add_perk(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    if ctx.subject_or_player() != FormId::PLAYER_REF {
        return Err("perks of actors other than the player are not modelled".into());
    }
    *g.player.perks.entry(a.form(0)?).or_default() += 1;
    none()
}

pub fn remove_perk(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    if ctx.subject_or_player() == FormId::PLAYER_REF {
        g.player.perks.remove(&a.form(0)?);
    }
    none()
}

pub fn get_has_note(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    yes(g.player.notes.contains(&a.form(0)?))
}

pub fn add_note(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.notes.insert(a.form(0)?);
    none()
}

pub fn is_hardcore(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    yes(g.player.hardcore.unwrap_or(g.world.config().hardcore))
}

pub fn is_player_tag_skill(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    yes(g.player.tag_skills.contains(&a.code(0)?))
}

pub fn set_player_tag_skill(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.tag_skills.insert(a.code(0)?);
    none()
}

pub fn add_achievement(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.achievements.insert(a.int(0)?);
    none()
}

/// The seven control groups: movement, Pip-Boy, fighting, POV, looking, rollover text,
/// sneaking. With no parameters `DisablePlayerControls` disables the first four and
/// `EnablePlayerControls` enables all seven [documented: GECK wiki pages of both].
/// A parameter left out of a short list takes `missing` (docs/spec-notes/controls.md §5.2).
fn controls(a: &Args, default: [bool; 7], missing: bool) -> [bool; 7] {
    if a.is_empty() {
        return default;
    }
    let mut out = [false; 7];
    for (i, o) in out.iter_mut().enumerate() {
        *o = if i < a.len() { a.num_or(i, 0.0) != 0.0 } else { missing };
    }
    out
}

pub fn disable_player_controls(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    // A left-out parameter is 0: the group is left as it is [hypothesis; §5.2 has a
    // capture that does not fit any simple rule].
    let which = controls(a, [true, true, true, true, false, false, false], false);
    for (d, w) in g.player.controls_disabled.iter_mut().zip(which) {
        *d |= w;
    }
    none()
}

pub fn enable_player_controls(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    // A left-out parameter enables its group, as the form without parameters does
    // [hypothesis: the trigger outside Doc Mitchell's door,
    // VCG01RemoveDocCarebearModeTriggerSCRIPT, gives the controls back with
    // `EnablePlayerControls 1 1 1 1 1 1`, and no other script gives sneaking back after
    // the opening turned it off; read with a left-out 0 the player could never sneak
    // again. controls.md §5.2].
    let which = controls(a, [true; 7], true);
    for (d, w) in g.player.controls_disabled.iter_mut().zip(which) {
        if w {
            *d = false;
        }
    }
    none()
}

/// 1 when any of the selected groups is disabled (all groups when none is selected)
/// [documented: catalogue summary].
pub fn get_player_controls_disabled(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let which = controls(a, [true; 7], false);
    yes(g.player.controls_disabled.iter().zip(which).any(|(d, w)| *d && w))
}

/// The player's level: SetLevel's, else the configuration's. Another actor's level is
/// known only after SetLevel (NPC_ levels are not read yet).
pub fn get_level(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    if r != FormId::PLAYER_REF {
        return match g.extra.get(&r).and_then(|e| e.level) {
            Some(l) => Ok(Value::Num(l as f64)),
            None => Err("levels of actors other than the player are not modelled".into()),
        };
    }
    Ok(Value::Num(g.player.level.unwrap_or(g.world.config().player_level as i32) as f64))
}

pub fn remove_note(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.notes.remove(&a.form(0)?);
    none()
}

pub fn reset_xp(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    g.player.xp = 0.0;
    none()
}

/// Axis 0 is infamy, 1 fame [documented: GECK wiki GetReputation].
fn rep_axis(a: &Args) -> Result<usize, String> {
    match a.int(1)? {
        0 => Ok(0),
        1 => Ok(1),
        o => Err(format!("reputation axis {o} is not 0 (infamy) or 1 (fame)")),
    }
}

pub fn get_reputation(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = a.form(0)?;
    let i = rep_axis(a)?;
    Ok(Value::Num(g.player.reputation.get(&r).map(|v| v[i]).unwrap_or(0.0)))
}

/// Sets the amount exactly, with no limit [documented: GECK wiki SetReputation;
/// observed: −3, 2.5 and twice the scale were stored as given, layer5-captures.md
/// §9.3 a].
pub fn set_reputation(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = a.form(0)?;
    let i = rep_axis(a)?;
    super::reputation::store(g, r, i, a.num(2)?);
    none()
}

pub fn set_hardcore(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let on = a.num(0)? != 0.0;
    g.player.hardcore = Some(on);
    g.player.hardcore_was_off |= !on;
    none()
}

/// Map markers made visible (fast travel is Layer 6).
pub fn show_map(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.map_markers.insert(a.form(0)?);
    none()
}

pub fn get_map_marker_visible(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(g.player.map_markers.contains(&r))
}

/// The player's miscellaneous statistics by code (the source names them as strings; the
/// bytes hold the code, scripts.md §2.4). They start at 0 and change through
/// ModPCMiscStat only [hypothesis: the game's own counters are Layer 6].
pub fn get_pc_misc_stat(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    Ok(Value::Num(g.player.misc_stats.get(&a.code(0)?).copied().unwrap_or(0.0)))
}

pub fn mod_pc_misc_stat(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    *g.player.misc_stats.entry(a.code(0)?).or_default() += a.num(1)?;
    none()
}

/// `EnableFastTravel [canTravel canWait keep]`: travel and waiting allowed or not;
/// waiting is forced on whenever travel is allowed; without `keep` an interior →
/// exterior change enables travel again ([`crate::Game::player_entered`])
/// [documented: catalogue notes of EnableFastTravel]. Whether travel is possible at a
/// moment (encumbrance, enemies) is the engine's.
pub fn enable_fast_travel(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let travel = a.num_or(0, 1.0) != 0.0;
    let wait = travel || a.num_or(1, 1.0) != 0.0;
    let keep = a.num_or(2, 0.0) != 0.0;
    g.player.fast_travel = FastTravel { travel_disabled: !travel, wait_disabled: !wait, keep };
    none()
}

fn challenge(g: &Game, a: &Args) -> Result<FormId, String> {
    let c = a.form(0)?;
    if g.data.challenges.contains_key(&c) {
        Ok(c)
    } else {
        Err(format!("{} is not a challenge", g.data.name(c)))
    }
}

/// A challenge flagged Start Disabled becomes able to progress [documented: catalogue
/// summary of UnlockChallenge]. Progress itself is counted by the challenge system
/// (Layer 6).
pub fn unlock_challenge(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let c = challenge(g, a)?;
    g.player.challenges.entry(c).or_default().unlocked = true;
    none()
}

/// 1 once the challenge was completed (a recurring one at least once) [documented:
/// catalogue summary]. Completion is reported by the challenge system (Layer 6) or a
/// runner input.
pub fn get_challenge_completed(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let c = challenge(g, a)?;
    yes(g.player.challenges.get(&c).is_some_and(|s| s.completed))
}

pub fn remove_recurring_from_challenge(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let c = challenge(g, a)?;
    g.player.challenges.entry(c).or_default().not_recurring = true;
    none()
}

/// `SetInCharGen flag` [documented: catalogue summary]; withholding level-ups is the
/// levelling system's (not modelled).
pub fn set_in_chargen(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.in_chargen = a.num(0)? != 0.0;
    none()
}

pub fn get_in_chargen(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    yes(g.player.in_chargen)
}

/// The player's winnings level (0–4) at a casino (CSNO) [documented: catalogue
/// summaries of GetCasinoWinningsLevel and GetCasinoWinningStage]: set by the casino
/// games (Layer 6) or a runner input; 0 until then.
pub fn get_casino_winnings_level(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    Ok(Value::Num(g.player.casino_levels.get(&a.form(0)?).copied().unwrap_or(0) as f64))
}

pub fn set_pc_can_use_power_armor(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.power_armor = a.num(0)? != 0.0;
    none()
}

/// The topic joins the topics the player knows: from then on every actor that can say
/// a line of it offers it in its topic list (`Game::build_choices`)
/// [documented: catalogue summary of AddTopic; owner statement 2026-10-10 for "every
/// actor", not yet captured; docs/spec-notes/dialogue.md §5].
pub fn add_topic(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let topic = a.form(0)?;
    if g.player.topics.insert(topic) {
        g.emit(crate::trace::EventKind::TopicAdded { topic, by: None });
    }
    none()
}

/// 1 when Hardcore mode has been on all game [documented: catalogue summary]: on now
/// and never switched off [hypothesis on what "all game" counts].
pub fn is_always_hardcore(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let on = is_hardcore(g, ctx, a)?.num() != 0.0;
    yes(on && !g.player.hardcore_was_off)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_script::vm::ArgValue;

    fn nums(v: &[f64]) -> Vec<ArgValue> {
        v.iter().map(|&n| ArgValue::Value(Value::Num(n))).collect()
    }

    #[test]
    fn short_parameter_lists() {
        let disable = |v: &[f64]| controls(&Args(&nums(v)), [true, true, true, true, false, false, false], false);
        let enable = |v: &[f64]| controls(&Args(&nums(v)), [true; 7], true);
        assert_eq!(disable(&[]), [true, true, true, true, false, false, false]);
        assert_eq!(disable(&[1.0, 1.0]), [true, true, false, false, false, false, false]);
        assert_eq!(enable(&[]), [true; 7]);
        // The trigger outside Doc Mitchell's door gives sneaking back too.
        assert_eq!(enable(&[1.0; 6]), [true; 7]);
        assert_eq!(enable(&[0.0, 1.0, 1.0]), [false, true, true, true, true, true, true]);
        assert_eq!(enable(&[1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0]), [true, false, false, true, true, true, false]);
    }
}
