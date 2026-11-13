//! The drinking contest in the world (NVForever's own mini-game [design: owner
//! 2026-11-11]): the bottle on the bar counter, where the drinkers stand, their drinks and
//! their passing out. The prop is `fnv_game::drink_table`; the contest and its screen are
//! `crate::drink_contest` (drink-a); the stake page and co-op are `crate::drink_coop`
//! (drink-b).
//!
//! - **Activation**: E on the bottle ("Drink Whiskey", `crate::activate`, never picked
//!   up) sends [`BottleUsed`]; here it becomes drink-b's
//!   `DrinkActivate`, which opens the stake page and, once everyone has staked, sends
//!   `DrinkStart` on every machine.
//! - **The marks**: on `DrinkStart` for one of the bottles, this player (when a drinker)
//!   steps onto the mark of its slot (`fnv_game::drink_table::mark`), facing the counter,
//!   the weapon away, and the view goes outside (third person), so the body and its
//!   drinks are seen [design]. Another player's body is put on its mark by its own game
//!   and comes here as its puppet does.
//! - **A drink at each shrink**: this player's `Shrink` plays the drink the game plays for
//!   whiskey used from the inventory ([`ContestDrink`], nvforever `consume_anims`: the
//!   standing drinking idle `DrinkingStandingA.kf`, IDLE `BeerDrinkingStanding`, with
//!   its bottle in the hand), on the body only, nothing used up and no effect. The
//!   others see it on this player's puppet the same way (the consume animations' co-op
//!   cue) [observed: FalloutNV.esm IDLE 0x00003BEA and ANIO `aoDrinkingStandingA`].
//! - **Passing out**: this player's `PassedOut` lays the body down where it stands as a
//!   knockdown does (`fnv_game::Game::knock_down`, nvforever `player_knockdown`: the
//!   ragdoll, the controls held), tipped backwards away from the counter, and holds it down
//!   (`RefExtra::unconscious`, as SetUnconscious holds a lying actor) until the contest
//!   is over. The others see the puppet fall by co-op's own rule for a player lying in
//!   its own game.
//! - **The winner dances** ([`WIN_DANCE`], the tap dance) from its `Winner` until the
//!   contest's end, then stands as before. A drink still playing ends first
//!   ([`ContestDrinkStop`]), so the dance is not under its last seconds. A player who wins
//!   steps off its mark onto the bar's stage (`fnv_game::drink_table::stage`: open floor
//!   in front of the counter, facing the camera) and dances there; its puppet goes there on the
//!   others' screens as it goes anywhere.
//! - **The win shot**: when a player wins, every drinker at the table (standing, or lying
//!   passed out) sees the dance through the stage's one camera, on the room's side of the
//!   winner with the counter behind it, above the head and looking down at it, with a soft fill light by the
//!   camera faded up on the winner's front ([`DrinkWorld::stage`], drawn by nvforever
//!   `drink_stage`), until the contest's end [design: drink-b's co-op frames, 2026-11-12:
//!   from behind, the dance read as a lean on the table; from the front against the bright
//!   windows the winner was a silhouette, and on the loser's screen its puppet was a dark
//!   shape behind the bottle. One fixed shot shows the same lit frame on every screen].
//!   The camera is pulled in to the first thing between it and the winner (a ray). It
//!   stands back and high enough that the winner's head is under the contest screen's
//!   banner and board (the top third). The shot and the dance hold until the winner's own
//!   contest screen closes ([`stage_seconds`]), on every screen alike: a loser's screen,
//!   which closes sooner, keeps the shot until then.
//! - **Waking up**: when the contest ends (`DrinkEnded`, or its screen gone), the hold
//!   is let go: the body gets up by the game's get-up clip (`GetUpFaceUp` /
//!   `GetUpFaceDown`), the view and the controls come back with it, and the view the
//!   player had before the contest is put back once they stand.
//!
//! Test aid `NVF_PLAY_TEST_DRINK_TABLE=<seconds>:<seed>[:<skill>[:<NPCs>]]`: that long
//! after the start (world aid seconds), a contest at the first bar's bottle with this
//! player, its steering held to the centre this well (0..[`MAX_AID_SKILL`],
//! `DrinkLink::aid_skill`: 1 is the aid's usual pull, more pulls harder; 0 drifts out and
//! passes out), against the nearest NPCs of the loaded cells where they
//! stand (none by default; alone the contest runs until this player passes out, with
//! NPCs it has a winner). A test aid; not a game behaviour.
//!
//! Log lines start `drink world:`.

use crate::control::{PlayerControl, Pov};
use crate::drink_contest::{DrinkEnded, DrinkEvent, DrinkLink, DrinkScreen, DrinkStart};
use crate::phys::PhysState;
use crate::player::Player;
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;
use crate::{PlaySet, PlayState};
use bevy::prelude::*;
use fnv_game::drink_contest::{Event, Who};
use fnv_game::drink_table::{self as dt, Bar};
use fnv_sim::FormId;

/// E on a drinking contest's bottle (`crate::activate`).
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BottleUsed {
    pub bottle: u32,
}

/// This player takes a drink of `item` in the contest: the body plays the drink of that
/// item used from the inventory, nothing is used up (nvforever `consume_anims`).
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContestDrink {
    pub item: u32,
}

/// This player's contest drink still playing ends now (nvforever `consume_anims`): the
/// winner's dance starts at once instead of under the end of the last drink.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContestDrinkStop;

/// The test aid's strongest steering ([`NVF_PLAY_TEST_DRINK_TABLE`](self)'s skill): 1 is
/// drink-a's bot pull, which loses to some seeds against a strong drinker; up to this
/// much more, for a run that has to win (the tap dance's).
pub const MAX_AID_SKILL: f32 = 4.0;

/// The speed a passing-out body is tipped away from the table with (game units a
/// second): it falls where it stands, not onto the table [design: a slump, far under a
/// push's 300 a point of force, `fnv_game::unconscious::PUSH_SPEED`].
pub const TIP_SPEED: f32 = 70.0;
/// ... and upwards [design: none, it drops].
pub const TIP_RISE: f32 = 0.0;
/// The winner's dance, by its emote key (`crate::emotes::CATALOG`): the tap dance
/// [design: owner 2026-11-11, "it should be tap dance or something silly", not the
/// cheer; observed: FalloutNV.esm IDLE 0x0012CF9D `SpecialIdleNVTapDance`,
/// `SpecialIdle_NVTapDance.kf`, danced in place]. Played as the emote, so the others
/// see it on this player's puppet through the emotes' co-op channel.
pub const WIN_DANCE: &str = "tap-dance";

/// This player at a contest's table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtTable {
    pub bottle: u32,
    pub bar: &'static Bar,
    pub slot: usize,
    /// The view before the contest, put back once the player stands again.
    pub pov: Pov,
    /// Passed out and held down.
    pub held: bool,
    /// The contest is over: waiting for the player to be up to give the view back.
    pub over: bool,
    /// Won: dancing until the contest's end.
    pub dancing: bool,
    /// The contest's slots that are players (bit `i` for slot `i`): a player's win is
    /// danced on the stage.
    pub players: u8,
}

/// The win shot while a player's win is danced: the camera every drinker at the table
/// sees through, the point it looks at and the fill light's place (nvforever
/// `drink_stage`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageView {
    pub camera: [f32; 3],
    pub look_at: [f32; 3],
    pub light: [f32; 3],
}

/// The contest this player drinks in, if any.
#[derive(Resource, Default, Debug)]
pub struct DrinkWorld {
    pub at: Option<AtTable>,
    /// The win shot, from a player's `Winner` until [`DrinkWorld::stage_until`].
    pub stage: Option<StageView>,
    /// When the win shot lets go (the app's elapsed seconds): the winner's own contest
    /// screen's close, the same on every screen ([`stage_seconds`]).
    pub stage_until: f32,
    /// Counts for the exit line: contests joined, drinks, pass-outs, wake-ups.
    pub joined: u32,
    pub drinks: u32,
    pub passed_out: u32,
    pub woke: u32,
    pub dances: u32,
}

pub struct DrinkWorldPlugin;

impl Plugin for DrinkWorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BottleUsed>()
            .add_message::<ContestDrink>()
            .add_message::<ContestDrinkStop>()
            .init_resource::<DrinkWorld>()
            .add_systems(Update, (test_aid, activated, started, events, ended).chain().after(PlaySet::Activation))
            .add_systems(Last, report);
    }
}

/// Where the tip sends a body standing on a mark facing `heading` (the counter): straight
/// backwards, flat.
pub fn tip(heading: f32) -> [f32; 3] {
    // Heading h faces (sin h, cos h).
    let (s, c) = heading.sin_cos();
    [-s * TIP_SPEED, -c * TIP_SPEED, TIP_RISE]
}

/// How far short of what it hits the win camera is pulled in, and where the fill light
/// hangs: this far from the point the camera looks at, towards the camera, and this much
/// above that line (game units) [design: the light as near the dancer as before the
/// camera stood back, 2026-11-12].
pub const STAGE_CLEAR: f32 = 8.0;
pub const STAGE_LIGHT_OUT: f32 = 110.0;
pub const STAGE_LIGHT_UP: f32 = 30.0;

/// The engine lock held with the win shot, locking no control: the corner and quest
/// messages stay held while it is up (`crate::corner_messages::in_world_game`, as during
/// the contest's screen), so a loser's held messages do not come back over the dance
/// [design: drink-b's pair, 2026-11-12, the compass hint over P1's held shot].
pub const STAGE_LOCK_KEY: &str = "nvforever:drink_stage";

/// How long the win shot holds after the `Winner`: until the winner's contest screen
/// closes (drink-a's highlight, then its leaderboard's hold), on every screen, so the
/// losers' screens, which close sooner, let go of it with the winner's [design: parity,
/// 2026-11-12: both screens close together].
pub fn stage_seconds() -> f32 {
    crate::drink_contest::board_after(true) + crate::drink_contest::RESULT_HOLD
}

/// The win shot of `bar`: its stage camera, pulled in to the first thing between it and
/// the point it looks at (`phys`, when there is one), never nearer than 40% of the way.
pub fn stage_view(bar: &Bar, phys: Option<&mut PhysState>) -> StageView {
    let st = dt::stage(bar);
    let d = [st.camera[0] - st.look_at[0], st.camera[1] - st.look_at[1], st.camera[2] - st.look_at[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-3);
    let dir = [d[0] / len, d[1] / len, d[2] / len];
    let reach = phys.and_then(|p| p.ray(st.look_at, dir, len, crate::table_spots::filter())).map_or(len, |h| (h.distance - STAGE_CLEAR).max(len * 0.4));
    let camera = [st.look_at[0] + dir[0] * reach, st.look_at[1] + dir[1] * reach, st.look_at[2] + dir[2] * reach];
    let out = reach.min(STAGE_LIGHT_OUT);
    let light = [st.look_at[0] + dir[0] * out, st.look_at[1] + dir[1] * out, st.look_at[2] + dir[2] * out + STAGE_LIGHT_UP];
    StageView { camera, look_at: st.look_at, light }
}

/// E on a bottle: drink-b's stake page.
fn activated(mut used: MessageReader<BottleUsed>, mut out: MessageWriter<crate::drink_coop::DrinkActivate>, world: Res<DrinkWorld>) {
    for u in used.read() {
        if world.at.is_some() {
            info!("drink world: bottle {:08X}: already at a contest", u.bottle);
            continue;
        }
        let key = dt::bar_of_bottle(FormId(u.bottle)).map_or("?", |b| b.key);
        info!("drink world: bottle {:08X} ({key}) activated: the stake page", u.bottle);
        out.write(crate::drink_coop::DrinkActivate { bottle: u.bottle, npcs: Vec::new() });
    }
}

/// A contest starts at one of the bottles: this player (a drinker) steps onto its mark.
fn started(
    mut starts: MessageReader<DrinkStart>,
    link: Res<DrinkLink>,
    mut world: ResMut<DrinkWorld>,
    mut phys: Option<ResMut<PhysState>>,
    mut player: ResMut<Player>,
    mut control: ResMut<PlayerControl>,
) {
    for s in starts.read() {
        let Some(bar) = dt::bar_of_bottle(FormId(s.bottle)) else { continue };
        // This player's slot: the co-op link's, else (alone) the first player's.
        let slot = if link.on { link.local } else { s.drinkers.iter().position(|d| matches!(d.who, Who::Player(_))) };
        let Some(slot) = slot else {
            info!("drink world: a contest starts at bottle {:08X} ({}): this player watches", s.bottle, bar.key);
            continue;
        };
        let (feet, heading) = dt::mark(bar, slot);
        match phys.as_deref_mut() {
            Some(p) => crate::table_spots::stand_player(p, &mut player, [feet[0], feet[1]], feet[2], heading),
            None => warn!("drink world: no physics: the player is not moved to the mark"),
        }
        let pov = control.pov;
        control.weapon_out = false;
        control.pov = Pov::Third;
        let players = s.drinkers.iter().enumerate().filter(|(_, d)| matches!(d.who, Who::Player(_))).fold(0u8, |m, (i, _)| m | 1 << i);
        world.at = Some(AtTable { bottle: s.bottle, bar, slot, pov, held: false, over: false, dancing: false, players });
        world.joined += 1;
        info!(
            "drink world: {}: this player drinks in slot {slot} of {}, at mark ({:.0}, {:.0}, {:.0}) facing {:.0} deg; weapon away, view outside",
            bar.key,
            s.drinkers.len(),
            player.feet[0],
            player.feet[1],
            player.feet[2],
            heading.to_degrees()
        );
    }
}

/// The contest's events for this player: a drink at each shrink, the body laid down when
/// passing out.
#[allow(clippy::too_many_arguments)]
fn events(
    mut events: MessageReader<DrinkEvent>,
    mut world: ResMut<DrinkWorld>,
    (mut drinks, mut stop): (MessageWriter<ContestDrink>, MessageWriter<ContestDrinkStop>),
    mut emotes: MessageWriter<crate::emotes::EmoteAsk>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut player: ResMut<Player>,
    (mut phys, mut control): (Option<ResMut<PhysState>>, ResMut<PlayerControl>),
    time: Res<Time>,
) {
    let w = &mut *world;
    for e in events.read() {
        let Some(at) = w.at.as_mut().filter(|a| a.bottle == e.bottle && !a.over) else { continue };
        match e.event {
            Event::Shrink { slot, step, .. } if slot == at.slot && !at.held => {
                let Some(item) = sim.world.forms.by_edid(dt::BOTTLE_EDID) else { continue };
                drinks.write(ContestDrink { item: item.0 });
                w.drinks += 1;
                info!("drink world: {}: drink {step}: this player drinks", at.bar.key);
            }
            Event::PassedOut { slot, why } if slot == at.slot && !at.held => {
                let shove = tip(dt::mark(at.bar, at.slot).1);
                let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { continue };
                let down = with_world(rt, &mut sim, |rt| rt.game.knock_down(FormId::PLAYER_REF, None, shove));
                if down {
                    rt.game.extra.entry(FormId::PLAYER_REF).or_default().unconscious = true;
                    at.held = true;
                    w.passed_out += 1;
                }
                info!(
                    "drink world: {}: this player passes out ({why:?}): {}, tipped ({:.0}, {:.0}, {:.0}) away from the counter",
                    at.bar.key,
                    if down { "laid down and held until the contest is over" } else { "could not be laid down" },
                    shove[0],
                    shove[1],
                    shove[2]
                );
            }
            Event::Winner { slot: Some(winner), .. } if winner < 8 && at.players & (1 << winner) != 0 => {
                // Every drinker at the table sees a player's win through the stage camera.
                let view = stage_view(at.bar, phys.as_deref_mut());
                w.stage = Some(view);
                w.stage_until = time.elapsed_secs() + stage_seconds();
                if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
                    rt.game.other.alt_locks.insert(STAGE_LOCK_KEY.to_string(), [false; 7]);
                }
                let ([cx, cy, cz], [lx, ly, lz]) = (view.camera, view.look_at);
                info!("drink world: {}: player slot {winner} wins: the win camera at ({cx:.0}, {cy:.0}, {cz:.0}) looks at the stage ({lx:.0}, {ly:.0}, {lz:.0}), the fill light over it", at.bar.key);
                if winner != at.slot || at.held {
                    continue;
                }
                let Some(emote) = crate::emotes::by_key(WIN_DANCE) else {
                    warn!("drink world: no emote {WIN_DANCE}: the winner does not dance");
                    continue;
                };
                stop.write(ContestDrinkStop);
                // Off the mark onto the stage, facing the camera.
                let st = dt::stage(at.bar);
                if let Some(p) = phys.as_deref_mut() {
                    crate::table_spots::stand_player(p, &mut player, [st.feet[0], st.feet[1]], st.feet[2], st.heading);
                    info!("drink world: {}: the winner steps onto the stage ({:.0}, {:.0}) facing {:.0} deg, the camera", at.bar.key, st.feet[0], st.feet[1], st.heading.to_degrees());
                }
                control.weapon_out = false;
                emotes.write(crate::emotes::EmoteAsk::Play { emote, cancel_after: None, variant: None });
                at.dancing = true;
                w.dances += 1;
                info!("drink world: {}: this player wins: the drink ends, the {WIN_DANCE} until the contest's end", at.bar.key);
            }
            _ => {}
        }
    }
}

/// The contest is over (its end, or its screen gone): the held body gets up; the view
/// before the contest comes back once the player stands.
fn ended(
    mut ends: MessageReader<DrinkEnded>,
    screen: Res<DrinkScreen>,
    state: Res<State<PlayState>>,
    mut world: ResMut<DrinkWorld>,
    mut script: Option<ResMut<ScriptState>>,
    (mut control, mut emotes): (ResMut<PlayerControl>, MessageWriter<crate::emotes::EmoteAsk>),
    (mut since, time): (Local<u32>, Res<Time>),
) {
    let w = &mut *world;
    let Some(at) = w.at.as_mut() else {
        ends.clear();
        *since = 0;
        return;
    };
    let mut over = ends.read().any(|e| e.bottle == at.bottle);
    let mut gone = false;
    // No screen for a few frames while at a table: the contest went (a load, a
    // disconnect) without its end.
    *since = if screen.0.as_ref().is_some_and(|d| d.bottle == at.bottle) { 0 } else { *since + 1 };
    if !at.over && *since > 5 && *state.get() == PlayState::Playing {
        info!("drink world: {}: the contest's screen is gone: treated as over", at.bar.key);
        over = true;
        gone = true;
    }
    if over && !at.over {
        at.over = true;
        // A contest gone without its end (a load, a disconnect) takes the win shot with it.
        if gone || *state.get() != PlayState::Playing {
            w.stage_until = 0.0;
        }
        if at.held {
            info!("drink world: {}: the contest is over: this player stays down until the win shot lets go", at.bar.key);
        } else {
            info!("drink world: {}: the contest is over: this player was still standing", at.bar.key);
        }
        return;
    }
    if !at.over {
        return;
    }
    // The win shot and the dance hold until the winner's screen closes, then let go on
    // every screen at once; a passed-out player lies where it fell until then (parity,
    // ship17 pvp: the loser got up behind the dancing winner).
    if !wake_due(w.stage.is_some(), time.elapsed_secs(), w.stage_until) {
        return;
    }
    if w.stage.take().is_some() {
        if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
            rt.game.other.alt_locks.remove(STAGE_LOCK_KEY);
        }
        info!("drink world: {}: the contest is over: the win camera lets go, the messages with it", at.bar.key);
    }
    if std::mem::take(&mut at.held) {
        if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
            rt.game.extra.entry(FormId::PLAYER_REF).or_default().unconscious = false;
        }
        w.woke += 1;
        info!("drink world: {}: the contest is over: this player wakes and gets up", at.bar.key);
    }
    if std::mem::take(&mut at.dancing) {
        emotes.write(crate::emotes::EmoteAsk::Cancel);
        info!("drink world: {}: the contest is over: the dance ends, back to standing", at.bar.key);
    }
    // Up again (the knockdown over, its get-up clip played): the view comes back.
    let down = script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.knockout(FormId::PLAYER_REF).is_some());
    if down {
        return;
    }
    control.pov = at.pov;
    info!("drink world: {}: this player stands again; the view is {:?} again", at.bar.key, at.pov);
    w.at = None;
}

/// Whether the contest's end may go on now (the held body wake, the win shot let go):
/// not while a win shot is up and its time (`until`, the app's seconds) not yet come.
fn wake_due(stage_up: bool, now: f32, until: f32) -> bool {
    !stage_up || now >= until
}

/// [`NVF_PLAY_TEST_DRINK_TABLE`](self): a solo contest at the first bar's bottle.
fn test_aid(time: crate::pause::AidTime, mut done: Local<bool>, mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>, mut start: MessageWriter<DrinkStart>, mut link: ResMut<DrinkLink>, state: Res<State<PlayState>>) {
    if *done || *state.get() != PlayState::Playing {
        return;
    }
    let Some((at, seed, skill, npcs)) = std::env::var("NVF_PLAY_TEST_DRINK_TABLE").ok().and_then(|v| {
        let mut it = v.trim().split(':');
        let at: f32 = it.next()?.trim().parse().ok()?;
        let seed: u64 = it.next()?.trim().parse().ok()?;
        let skill: Option<f32> = it.next().and_then(|s| s.trim().parse().ok());
        let npcs: usize = it.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
        Some((at, seed, skill, npcs))
    }) else {
        *done = true;
        return;
    };
    if time.world_secs().is_none_or(|t| t < at) {
        return;
    }
    let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { return };
    let Some(bottle) = dt::BARS.first().map(|b| b.refs[2]) else { return };
    if sim.world.peek(bottle).is_none() {
        return;
    }
    *done = true;
    let mut drinkers = vec![crate::drink_contest::player_drinker(rt, &mut sim, 0, "You")];
    if npcs > 0 {
        let me = sim.world.peek(FormId::PLAYER_REF).map_or([0.0; 3], |s| s.pos);
        // Only people drink (no creature, child, dead, out cold or fighting actor).
        let near = with_world(rt, &mut sim, |rt| crate::drink_contest::near_with_fit(&rt.game));
        for r in crate::drink_contest::nearest_drinkers(near, me, npcs.min(dt::MARKS - 1)) {
            drinkers.push(crate::drink_contest::npc_drinker(rt, &mut sim, r));
        }
    }
    if let Some(k) = skill {
        link.aid_skill = Some(k.clamp(0.0, MAX_AID_SKILL));
    }
    info!("drink world: test aid starts a contest at bottle {:08X} (seed {seed}, skill {skill:?}, {} NPC(s))", bottle.0, drinkers.len() - 1);
    start.write(DrinkStart { bottle: bottle.0, drinkers, seed, stake: 0 });
}

fn report(mut exit: MessageReader<AppExit>, world: Res<DrinkWorld>) {
    if exit.read().next().is_none() {
        return;
    }
    if world.joined > 0 {
        info!("drink world summary: {} contest(s) joined, {} drink(s), {} pass-out(s), {} wake-up(s), {} win dance(s){}", world.joined, world.drinks, world.passed_out, world.woke, world.dances, if world.at.is_some_and(|a| a.held) { "; still held down at exit" } else { "" });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A passed-out loser never gets up while the win shot is up: only at its release,
    /// or at once when there is no win shot (a draw, a contest gone).
    #[test]
    fn the_loser_stays_down_until_the_win_shot_lets_go() {
        let until = 105.45;
        for now in [100.0, 103.0, 105.0, 105.44] {
            assert!(!wake_due(true, now, until), "{now}");
        }
        assert!(wake_due(true, until, until));
        assert!(wake_due(true, 106.0, until));
        assert!(wake_due(false, 100.0, until));
    }

    #[test]
    fn a_passing_out_body_is_tipped_backwards_away_from_the_counter() {
        // Facing north (+y), the counter ahead: tipped south.
        let v = tip(0.0);
        assert!(v[1] < 0.0 && v[0].abs() < 1e-3);
        assert!(((v[0] * v[0] + v[1] * v[1]).sqrt() - TIP_SPEED).abs() < 1e-3);
        assert_eq!(v[2], TIP_RISE);
        // Every saloon mark: away from the bottle on the counter.
        let bar = &dt::BARS[0];
        for slot in 0..dt::MARKS {
            let (p, h) = dt::mark(bar, slot);
            let v = tip(h);
            assert!(v[0] * (bar.top[0] - p[0]) + v[1] * (bar.top[1] - p[1]) < 0.0, "slot {slot}");
        }
    }
}
