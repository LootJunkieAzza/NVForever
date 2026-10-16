//! Messages, time, globals and numbers.

use super::{none, yes, Args, Ctx};
use crate::game::{menus, Game, LineLook, MessageBox};
use crate::trace::EventKind;
use fnv_script::vm::Value;
use fnv_sim::clock;
use rand_core::RngCore;

/// `ShowMessage msg [vars…] [duration]`: traced with the message text; a message with
/// buttons waits for one (runner input `press_button`, read by GetButtonPressed of the
/// script that showed it).
pub fn show_message(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let m = a.form(0)?;
    let def = g.data.messages.get(&m).ok_or_else(|| format!("{} is not a message", g.data.name(m)))?;
    let (text, buttons, is_box) = (def.text.clone(), def.buttons.clone(), def.message_box);
    let hidden = g.hidden_buttons(m, ctx.self_ref);
    if !buttons.is_empty() {
        let owner = Some(MessageBox::owner_of(ctx.inst, &ctx.unit));
        g.message_box = Some(MessageBox { message: m, buttons: buttons.len(), pressed: None, owner });
    }
    g.emit(EventKind::MessageShown { message: m, text, buttons: buttons.clone(), hidden });
    // A message box (MESG flag Message Box, or buttons) is a menu until the player closes
    // it [observed: MenuMode 1001 while the pack plugins' boxes were open,
    // layer5-captures.md §10.1]; other messages are corner notifications.
    if is_box || !buttons.is_empty() {
        g.show_box(m);
    }
    none()
}

/// The chosen button once, then −1 [documented: GECK wiki GetButtonPressed]. Only the
/// script that showed the box receives it; for any other script the box is not there
/// (−1) and stays open [hypothesis, from the GECK wiki's ShowMessage and
/// GetButtonPressed pages; the `--play` integration saw an unrelated script (Doc
/// Mitchell's chemistry set) take every box's button while the button was global].
pub fn get_button_pressed(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let me = MessageBox::owner_of(ctx.inst, &ctx.unit);
    if let Some(m) = g.message_box.as_ref() {
        if let (Some(b), true) = (m.pressed, m.owner.as_ref().is_none_or(|o| *o == me)) {
            g.message_box = None;
            return Ok(Value::Num(b as f64));
        }
    }
    Ok(Value::Num(-1.0))
}

/// Writes to the warnings log only.
pub fn show_warning(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    if let Ok(s) = a.str(0) {
        tracing::info!(warning = %s, "script ShowWarning");
    }
    let _ = g;
    none()
}

/// 0 to 99 from the game's seeded generator (deterministic per seed).
pub fn get_random_percent(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    Ok(Value::Num((g.rng.next_u32() % 100) as f64))
}

/// The resolution of the game clock GetSecondsPassed reads: 1/64 s [observed: every
/// drop in the delay captures is a multiple of 0.015625 s, layer5-captures.md §5.4;
/// hypothesis: the clock, not the frame, is quantised].
pub const CLOCK_TICK: f64 = 1.0 / 64.0;

/// The game clock in 1/64 s steps.
pub fn clock_now(t: f64) -> f64 {
    (t / CLOCK_TICK + 1e-9).floor() * CLOCK_TICK
}

/// The clock time between the start of this instance's previous per-frame run
/// (GameMode, ScriptEffectUpdate) and the start of the current one; 0 on the first run.
/// Every call in one run gives the same value [documented: GECK wiki GetSecondsPassed],
/// and the value does not depend on when the script last called the function
/// [observed: VCG01's countdowns restarted after a pause in its calls ran their full
/// time, layer5-captures.md §10.7; see [`Game::mark_run`]]. Outside a per-frame run (an
/// event block of the same instance), the time since the latest run; result scripts (no
/// instance) get 0.
pub fn get_seconds_passed(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let Some(key) = ctx.inst else { return none() };
    let now = clock_now(g.time);
    let Some(inst) = g.instances.get(&key) else { return none() };
    let v = match (inst.run_last, inst.run_prev) {
        (Some(last), prev) if last == now => prev.map_or(0.0, |p| now - p),
        (Some(last), _) => now - last,
        (None, _) => 0.0,
    };
    Ok(Value::Num(v))
}

pub fn get_global_value(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let gl = a.form(0)?;
    Ok(Value::Num(g.world.global(gl).map_err(|e| e.to_string())? as f64))
}

/// The time of day in hours: the GameHour global.
pub fn get_current_time(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let (id, _) = g.world.forms.global_by_edid(clock::GAME_HOUR).ok_or("no GameHour global")?;
    Ok(Value::Num(g.world.global(id).map_err(|e| e.to_string())? as f64))
}

pub fn get_game_setting(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let name = a.str(0)?;
    let v = g.world.forms.setting_f32(name).ok_or_else(|| format!("game setting {name:?} is not defined"))?;
    Ok(Value::Num(v as f64))
}

pub fn abs(_g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    Ok(Value::Num(a.num(0)?.abs()))
}

/// The day of the week, 0 Sunday … 6 Saturday [documented: GECK wiki GetDayOfWeek],
/// as the Gregorian weekday of the GameYear / GameMonth (from 0) / GameDay globals
/// [hypothesis: the game counts weekdays on that calendar].
pub fn get_day_of_week(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let c = g.world.calendar().map_err(|e| e.to_string())?;
    Ok(Value::Num(weekday(c.year as i64, c.month as i64 + 1, c.day as i64) as f64))
}

/// Sunday = 0 (Sakamoto's method).
pub fn weekday(year: i64, month: i64, day: i64) -> i64 {
    const T: [i64; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    (y + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400) + T[((month - 1).clamp(0, 11)) as usize] + day).rem_euclid(7)
}

pub fn sqrt(_g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    Ok(Value::Num(a.num(0)?.max(0.0).sqrt()))
}

/// 1: the PC version [decision, as IsXBox and IsPS3 read 0].
pub fn one(_g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    Ok(Value::Num(1.0))
}

/// A Layer 6 state with no Layer 6 yet: always 0 (not in combat, not talking).
pub fn zero(_g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    none()
}

/// 1 while a menu is open, or that menu with a menu code [documented: catalogue summary].
/// A conversation with the player is a menu: `MenuMode` and `MenuMode 1009` read 1 from
/// its greeting to its end [owner statement 2026-10-10: such a dialogue pauses the game;
/// documented: GECK wiki GetMenuTargetRef, owner export, for the code 1009].
pub fn menu_mode(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    Ok(Value::bool(match a.num(0).ok().filter(|c| *c != 0.0) {
        None => g.in_menu(),
        Some(c) => g.menu.is_some_and(|m| m as f64 == c) || (c == menus::DIALOGUE as f64 && g.dialogue.is_some()),
    }))
}

/// `speaker.SayTo target topic [forceSubtitleFlag] [noTargetLookFlag]`: the speaker
/// says the topic's first line it can say to the target (`Game::say`,
/// docs/spec-notes/scripts.md §7.7). The two turn to each other while it is said; with
/// `noTargetLookFlag` 1 the target is not made to look at the speaker [documented: GECK
/// wiki SayTo, owner export] and only the speaker turns (`Game::facings`). The
/// subtitle flag is not read (subtitles are always shown).
pub fn say_to(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let speaker = ctx.subject()?;
    let (target, topic) = (a.form(0)?, a.form(1)?);
    if g.say(speaker, Some(target), topic, true).is_some() && a.num_or(3, 0.0) != 0.0 {
        g.set_line_look(speaker, LineLook::Speaker);
    }
    none()
}

/// `speaker.Say topic`: as SayTo, with no listener.
pub fn say(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let speaker = ctx.subject()?;
    let topic = a.form(0)?;
    g.say(speaker, None, topic, false);
    none()
}

/// `speaker.StartConversation target [topic] [speakerLocation] [targetLocation]
/// [headTrack] [allowMovement]`: with the player as the target, the speaker starts a
/// conversation with the named topic, GREETING without one [documented: GECK wiki
/// StartConversation, owner export: a "force greet"; nothing happens when either actor
/// is disabled]. In the game the speaker first walks up to the target; that needs AI
/// packages, so here the conversation starts at once [stand-in]. A call while a
/// conversation is running, or when the speaker has no line to say, does nothing
/// [hypothesis]. With another actor as the target the two talk to each other and the
/// player overhears them (`Game::start_actor_conversation`, docs/spec-notes/dialogue.md
/// §5.1). There `headTrack` 0 turns nobody for the whole conversation [documented:
/// "Enter 0 to disable" head tracking during the conversation; hypothesis: left out, it
/// is on] (`Game::facings`); the location references and allow-movement are not read,
/// nor is head-track in a conversation with the player.
pub fn start_conversation(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let speaker = ctx.subject()?;
    let target = a.form(0)?;
    if target != fnv_sim::FormId::PLAYER_REF {
        // Refusals are not errors of the script, as below.
        if g.start_actor_conversation(speaker, target, a.opt_form(1)).is_ok() && a.num_or(4, 1.0) == 0.0 {
            g.set_line_look(speaker, LineLook::Nobody);
        }
        return none();
    }
    // A disabled player (the speaker's side is checked by `Game::start_dialogue`).
    if g.world.is_enabled(target).is_ok_and(|on| !on) {
        return none();
    }
    // Refusals (a conversation is running, the speaker is gone or has no line) are not
    // errors of the script. The speaker starts it: a force greet, which holds the player
    // also for a Goodbye greeting (`Game::force_greet`).
    let _ = g.force_greet(speaker, a.opt_form(1));
    none()
}

/// 1 once the actor has been in a conversation with the player [documented: GECK wiki
/// GetTalkedToPC, owner export: whether the NPC has ever spoken to the player;
/// hypothesis: SayTo lines do not count; a greeting with the Goodbye flag, said at the
/// player without a conversation (`Game::start_dialogue`), does].
pub fn get_talked_to_pc(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let who = ctx.subject()?;
    Ok(Value::bool(g.extra.get(&who).is_some_and(|e| e.talked_to_pc)))
}

/// The emotion type code (0 Neutral … 7 Pained) of the response the actor is saying
/// (`Game::dialogue_emotion`) [documented: GECK wiki GetDialogueEmotion, owner export].
pub fn get_dialogue_emotion(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let who = ctx.subject()?;
    Ok(Value::Num(g.dialogue_emotion(who) as f64))
}

/// 1 while the actor is saying a line (SayTo, Say, or a line of a conversation with the
/// player; 0 while the player chooses) (`Game::is_talking`).
pub fn is_talking(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let who = ctx.subject()?;
    Ok(Value::bool(g.is_talking(who)))
}

/// The menu each character-creation function opens (`Game::open_menu`) [observed: the
/// code of each was open after the stage whose script calls it, save nvfnew,
/// layer5-captures.md §10.2–10.3]. What the menu does (the name typed, the race chosen,
/// the tag skills and traits picked, the S.P.E.C.I.A.L. points) is the player's, outside
/// the scripts: closing it keeps the state as it is.
fn open(g: &mut Game, code: u16) -> Result<Value, String> {
    g.open_menu(code);
    none()
}

/// GetPlayerName: the name menu.
pub fn get_player_name(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    open(g, menus::NAME)
}

/// ShowRaceMenu: the race menu.
pub fn show_race_menu(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    open(g, menus::RACE)
}

/// SetTagSkills: the tag-skill menu.
pub fn set_tag_skills(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    open(g, menus::TAG_SKILLS)
}

/// ShowTraitMenu: the trait menu.
pub fn show_trait_menu(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    open(g, menus::TRAITS)
}

/// `merchant.ShowBarterMenu [discount]`: the barter menu with the calling actor as the
/// merchant (`Game::barter_requests`; the engine draws it and closes the menu). The
/// discount is clamped to its documented range, -100 to 100 [documented: GECK wiki
/// ShowBarterMenu].
pub fn show_barter_menu(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let merchant = ctx.subject()?;
    let discount = a.num_or(0, 0.0).clamp(-100.0, 100.0) as i32;
    g.barter_requests.push(crate::game::BarterRequest { merchant, discount });
    open(g, menus::BARTER)
}

/// `station.ShowRecipeMenu category` (alias `Craft`): the recipe menu listing the
/// recipes of a category or subcategory (`Game::recipe_requests`; the engine draws it and
/// closes the menu) [documented: GECK wiki ShowRecipeMenu]. A form that is no recipe
/// category is an error of the call.
pub fn show_recipe_menu(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let category = a.form(0)?;
    if !g.data.recipe_categories.contains_key(&category) {
        return Err(format!("{category} is not a recipe category"));
    }
    let station = ctx.subject().ok();
    g.recipe_requests.push(crate::crafting::RecipeRequest { station, category });
    open(g, crate::crafting::RECIPE_MENU)
}

/// ShowLoveTesterMenuParams: the Vigor Tester's S.P.E.C.I.A.L. menu.
pub fn show_love_tester_menu(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    open(g, menus::LOVE_TESTER)
}

/// `ApplyImageSpaceModifier modifier`: the image space modifier starts playing on the
/// screen [documented: GECK wiki ApplyImageSpaceModifier, owner export]
/// (`Game::apply_image_modifier`, docs/spec-notes/image-modifiers.md §3). A form that is
/// not an image space modifier is an error of the call.
pub fn apply_image_space_modifier(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let m = a.form(0)?;
    if !g.data.image_modifiers.contains_key(&m) {
        return Err(format!("{m} is not an image space modifier"));
    }
    g.apply_image_modifier(m);
    none()
}

/// `RemoveImageSpaceModifier modifier`: the modifier leaves the screen [documented: GECK
/// wiki RemoveImageSpaceModifier, owner export]; nothing happens when it is not on it.
pub fn remove_image_space_modifier(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.remove_image_modifier(a.form(0)?);
    none()
}

/// `IsImageSpaceActive modifier`: 1 while the modifier is on the screen [documented:
/// GECK wiki IsImageSpaceActive, owner export].
pub fn is_image_space_active(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    yes(g.image_modifier_active(a.form(0)?))
}

/// `IsDLCInstalled "name"`: 1 when the named add-on is in the load order. No wiki page or
/// catalogue summary describes it; the only caller (`SCPT 0x0017912E`, the pack items'
/// quest script) asks for "tribal pack", "caravan pack", "mercenary pack" and "classic
/// pack" [observed: decoded scripts], which are the pack plugins' names without the
/// space and the extension (TribalPack.esm …). So: a plugin of the load order whose name,
/// without its extension, equals the argument once spaces and case are ignored
/// [hypothesis: how the original finds an add-on by name is not documented].
pub fn is_dlc_installed(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let squash = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_lowercase();
    let want = squash(a.str(0)?);
    yes(!want.is_empty() && g.data.load_order.iter().any(|p| squash(p.rsplit_once('.').map_or(p.as_str(), |(stem, _)| stem)) == want))
}

/// `SetGlobalTimeMultiplier mult [keepSoundUnchanged]`: the speed of the whole game,
/// 1 normal, 0.5 slow motion [documented: GECK wiki SetGlobalTimeMultiplier]. The
/// engine takes it (`UiRequests::time_multiplier`, read by `nvf_play`); sound speed and
/// pitch are not changed either way (the second parameter's "unchanged" is the only
/// behaviour; the shipped slow motion asks for it, `QUST 0x00102037` stage 27 [observed]).
/// 0 "will break the game" [documented]: values at or below 0 are refused.
pub fn set_global_time_multiplier(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let m = a.num(0)?;
    if m <= 0.0 || !m.is_finite() {
        return Err(format!("time multiplier {m} refused (must be above 0)"));
    }
    g.ui.time_multiplier = Some(m);
    none()
}
