//! The quest pilot (issue #2365): a headless walkthrough chain
//! (`crates/fnv-game/walkthrough/*.actions.json`, `fnv_game::runner`) played in the game
//! by the player character, to find what the headless run stands in for and a player
//! trips over. All of it is [design].
//!
//! `--pilot <chain> [--pilot-expect <trace>] [--pilot-out <report>]`, with `--play`.
//! Both may be given several times: the chains are played one after the other in the one
//! game, each compared with the trace at its place in the list.
//!
//! - **A campaign** (`--pilot-campaign`, with `--new-game`): the chains go on from the
//!   game's own state, as a player's do, instead of each from its `given` state. A
//!   chain's `given` is then a precondition: a quest entry is compared with the game and
//!   a difference kept as a `precondition` finding, never set; what a player would have
//!   had to earn or carry (actor values, items, skill points, tag skills, reputations,
//!   globals) is given, so the chain's own checks can pass. The first chain's `given` is
//!   left out altogether: the game is where New Game put it. A chain started from a thin
//!   `given` gave false alarms: Doc Mitchell, activated with his opening only marked
//!   done, started the opening again.
//! - **A line the headless run says for an actor** (`say_info` with a speaker) is, in a
//!   campaign, the player bringing that line about: the chains use it for what an actor
//!   says of its own accord, often at the end of a walk (Sunny goes behind the saloon
//!   before her line about the bottles: 2,774 units), and for the answer to the
//!   player's own pick (all of Boulder City Showdown is written so: Monroe's and
//!   Jessup's lines are what the player's choices get). In a conversation with the
//!   speaker the pilot picks the choice that has the actor say the line; when the line
//!   is further down, the choice from which the fewest picks lead to the line's topic
//!   (`choice_toward`, over the lines' Choices and Add Topics in the data; the lines
//!   between are said and their scripts run, as for a player who asks his way there).
//!   The game's rule for unattended runs, which leaves a conversation nobody scripted by
//!   its Goodbye after a second, is off while the pilot plays
//!   (`nvf_play::dialogue::set_piloted`): it left Monroe's before any pick. The player stays within
//!   [`NEAR_TARGET`] of the actor wherever it goes; once the actor has stood still for
//!   [`STILL_SECS`] s and said nothing, the player talks to it, once. The game picks
//!   the line. When the line named has not been begun [`LINE_SECS`] s on, a
//!   `line_not_said` finding is kept and the line said as the headless run says it.
//!   `say_first` and `say_to_done` are left to the game. A `trigger_enter` of another
//!   actor is the AI's to bring about, not the player's, and so is a `call` that moves
//!   another actor (`X.MoveTo`).
//! - **A save after each chain** of a campaign, `pilot-after-<chain>`, in NVForever's
//!   saves folder (run the pilot with `NVFOREVER_HOME` set to a folder of its own: the
//!   game's autosaves after travel go there too, and a run in the player's home
//!   overwrote the player's autosave slot). A later run goes on from one with `--load
//!   pilot-after-<chain> --pilot <the next chains> --pilot-campaign`, so the chains
//!   before it need not be played again. The next chain begins [`SAVE_SECS`] s later.
//! - Menus: `wait_menu`, `get_menus`, `close_menus`, `press_button` and
//!   `dismiss_message_boxes` act on the game's menu as the headless run does. A message
//!   box no step is waiting for is answered after [`BOX_SECS`] s (headless such a box
//!   holds nothing up): one with a single button is closed, as a player presses OK; of
//!   several buttons the last one shown is picked, which is the way on in the boxes
//!   seen so far (leaving Goodsprings: "Edit Name", "Rebuild Character", "Finished -
//!   Travel Onward"; the first button there opened the name menu again and again).
//!
//! - **The player is driven like an NPC and acts directly.** It walks the navmesh paths
//!   the NPCs walk (`fnv_nav`, through doors) on its own movement controls, so the
//!   script's control locks hold it as they hold the keys. It talks, activates and
//!   chooses through the calls the E key and the dialogue screen end in, with no key
//!   pressed [owner decision 2026-11-13].
//! - **Could a real player have done it?** Before it talks to or activates something it
//!   stands where a player would, turns the view onto it and asks the game's own
//!   crosshair (`nvf_play::activate::Target`). When the crosshair does not take it, the
//!   step goes on and a `could_not_target` finding is kept [owner decision 2026-11-13].
//! - **What the headless run stands in for is left to the game.** `package`,
//!   `patrol_point` and `dialogue_package` (an actor's package made to arrive) are not
//!   carried out: the AI has to do it. `wait_talk` with `start` waits for the actor to
//!   come and greet.
//! - **The headless run is the expected result.** With `--pilot-expect` (the trace
//!   `fnvtool script run --actions <chain>` prints) every labelled read is compared with
//!   the headless one of the same step; a `get_quest` that differs is read again for
//!   [`MILESTONE_SECS`] (the game needs real time for what the headless run does in a
//!   frame) and then kept as a divergence.
//! - **The player is never put where it could not go** [owner, 2026-11-13: the pilot
//!   had put itself beside Chris Haversam behind REPCONN's locked door, and so past the
//!   intercom that unlocks it]. With no way to a place (a locked door the player has no
//!   key for, no navmesh), or a door that does not take the player through, the step
//!   fails with an `unreachable` finding and the player stays where it is: a later step
//!   walks from there, by when the game may have opened the way. The paths know the
//!   doors scripts have locked and unlocked, the keys the player carries and the doors
//!   standing open. Only a walk that stops within [`PUT_WITHIN`] units of its goal, in
//!   the same space, ends with the player set on the goal (the pilot's own clumsiness
//!   round furniture, a `stuck` finding).
//! - **Up to a locked door, and what happens there.** When the only way is through a
//!   locked door, the player walks as far as it can, to that door, and tries it, as a
//!   player does: REPCONN's research labs are opened by a second intercom at the door
//!   itself, whose trigger the player walks into there; no step of the headless chain
//!   goes near it. A conversation that begins on such a way is heard out and left by its
//!   Goodbye, and the walk goes on with a new path (the door may be open now). A door
//!   that stays locked [`LOCKED_SECS`] s after it was tried is left, and the next way
//!   round is tried (the shortest way to REPCONN's labs is through a door with a lock to
//!   pick, which the pilot does not; the stairs lead to the intercom's door). With no
//!   way left the step ends with an `unreachable` finding that names each door tried,
//!   its lock level and its key.
//! - A long way out of doors is cut short: when a path is over [`WALK_MAX`] units, the
//!   player starts [`WALK_TAIL`] units before the end of the path's last stretch out of
//!   doors (a stand-in for fast travel, logged). What lies indoors after that, and every
//!   door on the way, is walked: no interior is skipped into.
//! - **A conversation is left as a player leaves it** [owner, 2026-11-13; #2157: a
//!   Goodbye line holds the player until it is said, and a player walking off mid-line
//!   broke result scripts]. The pilot never breaks a conversation off (it did, with
//!   `Game::end_dialogue`, and REPCONN's intercom never ran the end script that unlocks
//!   the door). To leave, it hears the line out, picks the Goodbye choice and hears that
//!   line out too; on a topic list with no Goodbye it leaves as Tab does (#1595). A
//!   conversation offering choices, none of them a Goodbye, and not a topic list, has no
//!   way out for a player: a `no_way_out` finding, and only then is it broken off so
//!   that the chain goes on. `end_talk` is this too.
//! - A conversation that begins while the player walks: where the steps that follow
//!   deal with it (a `trigger_enter`, an actor kept company, a walk followed by
//!   `wait_talk`, `choose` or `talk`), the walk ends there and they hear it out
//!   (REPCONN's lobby intercom, as its trigger is entered). Anywhere else nothing in the
//!   chain answers it: the player hears it out, leaves by its Goodbye and walks on.
//! - Lines are skipped after [`SKIP_AFTER`] s, as a player in a hurry does. A conversation
//!   still open when the next step is a walk is left first, unless the walk is to the
//!   actor talking; an actor that came and spoke first is the conversation a `talk`
//!   step asks for.
//! - **`kill` is a fight** [owner decision 2026-11-13]: the player walks to within
//!   [`FIGHT_RANGE`] of the actor, draws ([`PILOT_WEAPON`], given once with its ammo),
//!   keeps the view on the actor's trunk and pulls the trigger until it is dead, closer
//!   after [`FIGHT_CLOSER_SECS`]. The shots are the player's own (`nvf_play::combat`):
//!   the weapon's cycle, spread, damage and the target's armour decide. An actor still
//!   alive after [`FIGHT_SECS`] is a `fight_not_won` finding and is killed as the
//!   headless run kills it, so the chain goes on. The line of fire is the NPCs' own
//!   judgement (`combat_tactics::shot_clear`: the eye to the target's trunk, else its
//!   head): with none for [`NO_SHOT_SECS`] s the player goes nearer, by a third each
//!   time, instead of firing into what is in the way. Not yet: the NPCs' reach by
//!   weapon and their cover (`npc_combat::gun_reach`, `combat_tactics::find_cover`),
//!   melee.
//! - **The player defends itself and whoever it is with.** An actor in combat whose
//!   target is the player, or the actor the step walks to or keeps company, within
//!   [`THREAT_RANGE`], is fought first, as a `kill` step fights, and the step goes on
//!   afterwards with its time not counted. Without it Sunny, kept company to the second
//!   well, was beaten to death by three geckos while the player stood by and waited for
//!   her next line. A threat alive after [`DEFEND_SECS`] is a `fight_not_won` finding.
//! - **The player levels up.** The level-up menu holds the game until it is answered
//!   (in the first Primm run it came up on arrival and every step behind it timed out):
//!   after [`BOX_SECS`] the pilot spends the level's skill points evenly down the list,
//!   takes the first perk on offer at a perk level, and presses Done
//!   (`fnv_game::levelup`, `nvf_play::chargen::finish`). Any other menu the scripts
//!   opened that no step waits for and that is still up after [`MENU_SECS`] is closed,
//!   with a `menu_closed` finding.
//! - **The weapon is kept in repair.** God mode keeps the magazine full but not the
//!   weapon whole: some 700 shots into a campaign the pistol broke, the game unequipped
//!   it and the player fought on with its fists. Every [`WEAPON_SECS`] s of a fight the
//!   pilot sets the condition of what the player holds to full, and a player found
//!   without the pilot's weapon in hand gets a new one and equips it (unless
//!   `--pilot-no-god`: then wear is part of what the run asks).
//! - **God mode is on** unless `--pilot-no-god` [owner decision 2026-11-13]: the player
//!   takes no damage and its magazine stays full (`nvf_play::combat::set_god_mode`). The
//!   pilot looks for quests that cannot go on, not for fights that cannot be won; a run
//!   without it says whether a fight can be.
//! - Not yet: `hit` (the script call of the headless run) and the samplers
//!   (`not_carried_out` findings).
//!
//! **Limits.** No step lasts longer than [`STEP_SECS`]: it is given up with a
//! `step_timed_out` finding and the chain goes on. A chain with [`FAILED_IN_A_ROW`]
//! failed steps one after the other is abandoned (`chain_abandoned`): what is left of it
//! would only fail the same way; a campaign goes on with the next chain. The pilot's
//! clock runs only while it can act (in play, in a conversation, in a menu the scripts
//! opened); when it has not been able to for [`HELD_SECS`] s of real time (a menu of the
//! engine's own, a loading screen that does not end), the run ends with a `pilot_held`
//! finding and its report, as it does at `--run-seconds`.
//!
//! Log: `pilot: step <n> <op> ...`, `pilot: finding <kind>: ...`, `pilot: divergence at
//! step <n> ...`, and at the end `pilot: done: <n> steps, <d> divergences, <f> findings,
//! <s> soft locks`, after which the game exits. The report (`fnv.pilot.report/1`) holds
//! the same.

use bevy::prelude::*;
use fnv_game::game::Game;
use fnv_game::runner::{self, Action, Applier};
use fnv_nav::follow::{heading_delta, heading_of, turn_towards};
use fnv_nav::{Follower, Place, Space, Steer};
use fnv_sim::FormId;
use nvf_play::activate::Target;
use nvf_play::control::{ControlLocks, Crosshair, PlayerControl};
use nvf_play::loading::{Loading, TravelRequest};
use nvf_play::phys::PhysState;
use nvf_play::player::{MoveIntent, Player};
use nvf_play::script::{with_world, ByHand, ScriptState};
use nvf_play::sim::SimState;
use nvf_play::{Location, PlaySet, PlayState, Travel};
use serde_json::{json, Map, Value as Json};
use std::path::PathBuf;

pub const REPORT_FORMAT: &str = "fnv.pilot.report/1";
/// A path longer than this is cut to its last [`WALK_TAIL`] units.
pub const WALK_MAX: f32 = 12_000.0;
pub const WALK_TAIL: f32 = 3_000.0;
/// Seconds a differing `get_quest` is read again before it is a divergence.
pub const MILESTONE_SECS: f64 = 45.0;
/// How many picks down a conversation the pilot looks for a line.
pub const PICKS_DEEP: usize = 5;
/// Seconds a conversation with no way out is waited on before the finding.
pub const NO_WAY_OUT_SECS: f64 = 5.0;
/// Seconds of a line the pilot hears before it skips it.
pub const SKIP_AFTER: f64 = 0.4;
/// Seconds before the first step: the world has loaded and settled.
const SETTLE_SECS: f64 = 3.0;
/// How near the player stands to what it talks to or activates, and to a marker.
const NEAR_TARGET: f32 = 100.0;
const NEAR_MARKER: f32 = 48.0;
/// Seconds a locked door on the way is waited at, after the player tried it.
pub const LOCKED_SECS: f64 = 8.0;
/// How near its goal a walk that got stuck may end with the player set on the goal.
pub const PUT_WITHIN: f32 = 400.0;
/// Seconds without [`STUCK_UNITS`] of movement on a walk before a new path is planned.
const STUCK_SECS: f64 = 4.0;
const STUCK_UNITS: f32 = 24.0;
const REPLANS: u8 = 3;
/// The heights above a reference's origin the view is turned to, one after the other,
/// to find it under the crosshair.
const AIM_HEIGHTS: [f32; 4] = [80.0, 40.0, 110.0, 8.0];
const AIM_SECS: f64 = 0.25;
/// The longest any one step lasts, seconds: then it is given up, whatever it waits for.
pub const STEP_SECS: f64 = 240.0;
/// Failed steps one after the other that end a chain.
pub const FAILED_IN_A_ROW: u32 = 12;
/// Real seconds without the pilot being able to act that end the run.
pub const HELD_SECS: f64 = 120.0;
/// The longest any one wait lasts, seconds.
const WAIT_CAP: f64 = 120.0;
const ADVANCE_CAP: f64 = 20.0;
const TURN_RATE: f32 = 6.0;
/// Seconds a message box nobody waits for stays up before the pilot closes it.
pub const BOX_SECS: f64 = 1.5;
/// Seconds the next chain waits for the save after the one before it.
pub const SAVE_SECS: f64 = 3.0;
/// Seconds a menu the pilot has no answer for stays up before it is closed.
pub const MENU_SECS: f64 = 20.0;
/// Seconds in which the line a `say_info` names must be begun, the player keeping its
/// speaker company.
pub const LINE_SECS: f64 = 120.0;
/// Seconds the speaker has stood still, silent, before the player talks to it.
pub const STILL_SECS: f64 = 6.0;
/// The weapon and ammo the pilot fights with (Editor IDs of FalloutNV.esm's records).
pub const PILOT_WEAPON: &str = "WeapNV9mmPistol";
pub const PILOT_AMMO: &str = "Ammo9mm";
/// How near the player goes to shoot, then nearer when the target still lives.
pub const FIGHT_RANGE: f32 = 700.0;
pub const FIGHT_NEAR: f32 = 220.0;
pub const FIGHT_CLOSER_SECS: f64 = 12.0;
pub const FIGHT_SECS: f64 = 75.0;
/// Seconds between looks at the weapon in a fight.
pub const WEAPON_SECS: f64 = 2.0;
/// Seconds without a clear line of fire before the player goes nearer, and how near at
/// the least.
pub const NO_SHOT_SECS: f64 = 1.5;
pub const FIGHT_NEAREST: f32 = 120.0;
/// How far off an attacker is fought, and for how long at most.
pub const THREAT_RANGE: f32 = 2500.0;
pub const DEFEND_SECS: f64 = 60.0;

pub struct PilotPlugin {
    pub chains: Vec<PathBuf>,
    pub expect: Vec<PathBuf>,
    pub out: Option<PathBuf>,
    pub campaign: bool,
    pub god: bool,
}

impl Plugin for PilotPlugin {
    fn build(&self, app: &mut App) {
        let loaded: Result<Vec<Chain>, String> = self.chains.iter().enumerate().map(|(k, c)| Chain::load(c, self.expect.get(k).map(PathBuf::as_path)).map_err(|e| format!("{}: {e}", c.display()))).collect();
        match loaded {
            Ok(chains) if !chains.is_empty() => {
                for c in &chains {
                    info!("pilot: chain {}: {} steps{}", c.name, c.actions.len(), if c.expect.is_empty() { "" } else { ", compared with a headless trace" });
                }
                if self.god {
                    nvf_play::combat::set_god_mode(true);
                    info!("pilot: god mode on: the player takes no damage and its magazine stays full (--pilot-no-god for a run without)");
                }
                if self.campaign {
                    info!("pilot: a campaign: the chains go on from the game's own state; their given states are preconditions");
                }
                nvf_play::dialogue::set_piloted(true);
                let mut pilot = Pilot::new(chains, self.out.clone(), self.campaign);
                pilot.god = self.god;
                app.insert_resource(pilot).add_systems(Update, (drive.in_set(PlaySet::Input).after(nvf_play::control::read_controls).before(nvf_play::control::enforce_locks), held));
            }
            Ok(_) => error!("pilot: no chain"),
            Err(e) => error!("pilot: {e}"),
        }
    }
}

/// One chain to play.
struct Chain {
    name: String,
    title: String,
    given: Vec<Json>,
    actions: Vec<Action>,
    ops: Vec<String>,
    expect: std::collections::HashMap<usize, Json>,
}

impl Chain {
    fn load(chain: &std::path::Path, expect: Option<&std::path::Path>) -> Result<Chain, String> {
        let text = std::fs::read_to_string(chain).map_err(|e| e.to_string())?;
        let raw: Json = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let input: runner::Input = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
        if input.format != runner::ACTIONS_FORMAT {
            return Err(format!("format {:?}, want {:?}", input.format, runner::ACTIONS_FORMAT));
        }
        let ops = raw["actions"].as_array().map(|a| a.iter().map(|x| x["op"].as_str().unwrap_or("?").to_string()).collect()).unwrap_or_default();
        let mut expected = std::collections::HashMap::new();
        if let Some(p) = expect {
            let t: Json = serde_json::from_str(&std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?).map_err(|e| format!("{}: {e}", p.display()))?;
            for o in t["observations"].as_array().into_iter().flatten() {
                if let Some(step) = o["step"].as_u64() {
                    expected.insert(step as usize, o.clone());
                }
            }
        }
        let name = chain.file_name().map(|n| n.to_string_lossy().trim_end_matches(".actions.json").to_string()).unwrap_or_default();
        Ok(Chain { name, title: input.title.clone().unwrap_or_default(), given: raw["given"].as_array().cloned().unwrap_or_default(), actions: input.actions, ops, expect: expected })
    }
}

/// A walk to a place.
struct Walk {
    goal: Place,
    /// The reference walked to: a new path when it has moved.
    target: Option<FormId>,
    radius: f32,
    follower: Option<Follower>,
    /// Since when the door at the end of the leg, or the one in the way, is waited for.
    door_since: Option<f64>,
    /// Where the player was, and when, for the stuck rule.
    seen: (f64, [f32; 3]),
    replans: u8,
    cut: bool,
    /// Whether a conversation was open when the walk began (`None` until its first frame).
    talking_at_start: Option<bool>,
    /// A conversation that begins on the way ends the walk: the steps after it deal with
    /// it. Else it is left by its Goodbye and the walk goes on.
    keep_talk: bool,
    /// The path goes through a locked door the player cannot open: walked up to it.
    locked_way: bool,
    /// Locked doors tried and left: the next path keeps off them. With what each said.
    tried: Vec<(u32, String)>,
    /// Said that the actor walked to is talking (once a conversation).
    told_talking: bool,
    /// Stepping aside (to this side, until then) round what the player walked into.
    dodge: Option<(f64, f32)>,
    /// The end of the planned path: the goal on the navmesh, where a put goes.
    snapped: Option<[f32; 3]>,
}

/// Turning the view onto a reference and asking the crosshair for it.
struct Aim {
    target: FormId,
    height: usize,
    since: f64,
    /// What the crosshair had instead, last.
    had: Option<String>,
}

/// A fight with an actor.
struct Fight {
    target: FormId,
    /// Since when the player has been shooting from where it stands.
    shooting: Option<f64>,
    near: bool,
    /// Since when there has been no clear line of fire from where the player stands.
    blocked: Option<f64>,
}

enum Phase {
    /// The next step has not begun.
    Begin,
    Fight(Fight, Walk),
    Walk(Walk),
    Aim(Aim),
    /// Until `until` (seconds), or sooner by the step's own test.
    Wait { until: f64 },
}

#[derive(Resource)]
struct Pilot {
    /// The chains still to play, the one being played and how many are done.
    queue: std::collections::VecDeque<Chain>,
    chain: String,
    chains_done: usize,
    campaign: bool,
    /// Since when a message box nobody waits for has been up.
    box_since: Option<f64>,
    /// A button of that box was picked and waits to be read.
    box_picked: bool,
    /// Failed steps one after the other, and the real time the pilot last could act.
    failed_in_a_row: u32,
    acted_at: Option<f64>,
    /// Since when the open conversation has offered no way out.
    stuck_talk: Option<f64>,
    /// God mode is on, and when the weapon is looked at next.
    god: bool,
    weapon_check: f64,
    /// The chain's save was asked for, and when the next chain may begin.
    saved: bool,
    resume_at: f64,
    /// The other menu up, and since when.
    menu_since: Option<(u16, f64)>,
    /// For a `say_info` kept company: how many lines had been begun when the chain (in
    /// a campaign, the first chain) began, and since when the speaker has stood where it stands.
    mark: u64,
    still: (f64, [f32; 3]),
    /// An attacker being fought before the step goes on, since when, and when the
    /// attackers are looked for next.
    defend: Option<(Fight, Walk)>,
    defend_from: f64,
    threat_check: f64,
    title: String,
    given: Vec<Json>,
    actions: Vec<Action>,
    ops: Vec<String>,
    /// The headless observation of each step.
    expect: std::collections::HashMap<usize, Json>,
    out: Option<PathBuf>,
    applier: Applier,
    given_applied: bool,
    armed: bool,
    i: usize,
    phase: Phase,
    /// The step's part: 0 getting there, 1 aimed, 2 acted.
    part: u8,
    /// Seconds the game has run under the pilot, and when the step and the line began.
    now: f64,
    step_from: f64,
    line: Option<(u64, f64)>,
    steps: Vec<Json>,
    observations: Vec<Json>,
    divergences: Vec<Json>,
    findings: Vec<Json>,
    done: bool,
}

impl Pilot {
    fn new(chains: Vec<Chain>, out: Option<PathBuf>, campaign: bool) -> Pilot {
        let mut p = Pilot {
            queue: chains.into(),
            chain: String::new(),
            chains_done: 0,
            campaign,
            box_since: None,
            box_picked: false,
            failed_in_a_row: 0,
            acted_at: None,
            stuck_talk: None,
            god: false,
            weapon_check: 0.0,
            saved: false,
            resume_at: 0.0,
            menu_since: None,
            mark: 0,
            still: (0.0, [0.0; 3]),
            defend: None,
            defend_from: 0.0,
            threat_check: 0.0,
            title: String::new(),
            given: Vec::new(),
            actions: Vec::new(),
            ops: Vec::new(),
            expect: Default::default(),
            out,
            applier: Applier::default(),
            given_applied: false,
            armed: false,
            i: 0,
            phase: Phase::Begin,
            part: 0,
            now: 0.0,
            step_from: 0.0,
            line: None,
            steps: Vec::new(),
            observations: Vec::new(),
            divergences: Vec::new(),
            findings: Vec::new(),
            done: false,
        };
        p.next_chain();
        p
    }

    /// Take the next chain of the queue. Whether there was one.
    fn next_chain(&mut self) -> bool {
        let Some(c) = self.queue.pop_front() else { return false };
        (self.chain, self.title, self.given, self.actions, self.ops, self.expect) = (c.name, c.title, c.given, c.actions, c.ops, c.expect);
        (self.i, self.phase, self.part, self.given_applied, self.step_from) = (0, Phase::Begin, 0, false, self.now);
        true
    }

    fn finding(&mut self, kind: &str, text: String) {
        warn!("pilot: finding {kind}: step {} {}: {text}", self.i, self.ops.get(self.i).map_or("", String::as_str));
        self.findings.push(json!({"kind": kind, "chain": self.chain, "step": self.i, "op": self.ops.get(self.i), "text": text}));
    }

    /// The step is over: the next one begins.
    fn next(&mut self, error: Option<String>) {
        let op = self.ops.get(self.i).cloned().unwrap_or_default();
        let secs = self.now - self.step_from;
        match &error {
            Some(e) => warn!("pilot: step {} {op} failed after {secs:.1} s: {e}", self.i),
            None => info!("pilot: step {} {op} done in {secs:.1} s", self.i),
        }
        let mut s = json!({"chain": self.chain, "step": self.i, "op": op, "seconds": (secs * 10.0).round() / 10.0});
        self.failed_in_a_row = if error.is_some() { self.failed_in_a_row + 1 } else { 0 };
        if let Some(e) = error {
            s["error"] = json!(e);
        }
        self.steps.push(s);
        if self.failed_in_a_row >= FAILED_IN_A_ROW {
            self.failed_in_a_row = 0;
            let left = self.actions.len().saturating_sub(self.i + 1);
            self.finding("chain_abandoned", format!("{FAILED_IN_A_ROW} steps failed one after the other: the {left} steps left of the chain are not played"));
            self.i = self.actions.len().saturating_sub(1);
        }
        self.i += 1;
        (self.phase, self.part, self.step_from) = (Phase::Begin, 0, self.now);
    }

    /// A labelled read, against the headless one of the same step. Whether they agree.
    fn observe(&mut self, label: &str, fields: &Map<String, Json>, keep: bool) -> bool {
        let differing: Vec<Json> = match self.expect.get(&self.i) {
            Some(want) => fields.iter().filter(|(k, v)| want.get(k.as_str()).is_some_and(|w| !same(w, v))).map(|(k, v)| json!({"field": k, "headless": want[k.as_str()], "game": v})).collect(),
            None => Vec::new(),
        };
        if !differing.is_empty() && !keep {
            return false;
        }
        let mut o = json!({"chain": self.chain, "label": label, "step": self.i});
        for (k, v) in fields {
            o[k.as_str()] = v.clone();
        }
        self.observations.push(o);
        if differing.is_empty() {
            return true;
        }
        warn!("pilot: divergence at step {} ({label}): {}", self.i, Json::Array(differing.clone()));
        self.divergences.push(json!({"chain": self.chain, "step": self.i, "label": label, "fields": differing}));
        false
    }

    fn report(&self, softlocks: u32) -> Json {
        json!({
            "format": REPORT_FORMAT, "campaign": self.campaign, "chains": self.chains_done, "last_title": self.title, "seconds": self.now.round(),
            "steps": self.steps, "observations": self.observations, "divergences": self.divergences, "findings": self.findings, "softlocks": softlocks,
        })
    }
}

/// Numbers alike within a hair, anything else equal.
fn same(a: &Json, b: &Json) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => (x - y).abs() <= 1e-6 * x.abs().max(1.0),
        _ => a == b,
    }
}

/// What an activation by hand came to, in a word.
fn by_hand(b: &ByHand) -> &'static str {
    match b {
        ByHand::Script => "its script ran",
        ByHand::Default(_) => "its default action",
        ByHand::Talk => "a conversation",
        ByHand::Greeted => "a greeting",
        ByHand::Choices => "a choice of actions",
        ByHand::Wheel => "the companion wheel",
        ByHand::Refused => "refused",
        ByHand::Woken => "woken",
    }
}

fn outcome(r: Option<&Result<ByHand, String>>) -> String {
    match r {
        Some(Ok(b)) => by_hand(b).to_string(),
        Some(Err(e)) => format!("error: {e}"),
        None => "the scripts are not running".to_string(),
    }
}

fn hdist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Where a reference is now: its cell and position.
fn where_is(g: &Game, r: FormId) -> Option<(Option<FormId>, [f32; 3])> {
    g.world.peek(r).map(|s| (s.cell, s.pos)).or_else(|| g.world.placed.get(r).map(|p| (p.cell, p.pos)))
}

fn space_of(loc: &Location) -> Space {
    match loc.interior {
        Some(c) => Space::Interior(c),
        None => Space::World(loc.world),
    }
}

fn travel_to(p: Place, heading_deg: f32) -> Travel {
    match p.space {
        Space::World(w) => Travel { world: Some(w), interior: None, feet: p.pos, heading_deg },
        Space::Interior(c) => Travel { world: None, interior: Some(c), feet: p.pos, heading_deg },
    }
}

/// The point `tail` units before the end of a leg's points.
fn tail_point(points: &[[f32; 3]], tail: f32) -> [f32; 3] {
    let mut left = tail;
    for w in points.windows(2).rev() {
        let d = hdist(w[0], w[1]);
        if d >= left && d > 1e-3 {
            let t = left / d;
            return [w[1][0] + (w[0][0] - w[1][0]) * t, w[1][1] + (w[0][1] - w[1][1]) * t, w[1][2] + (w[0][2] - w[1][2]) * t];
        }
        left -= d;
    }
    points.first().copied().unwrap_or_default()
}

/// What the systems lend the pilot for a frame.
struct Hands<'a, 'w> {
    script: &'a mut ScriptState,
    sim: &'a mut SimState,
    phys: &'a mut PhysState,
    player: &'a mut Player,
    intent: &'a mut MoveIntent,
    control: &'a mut PlayerControl,
    locks: &'a ControlLocks,
    location: &'a Location,
    target: &'a Target,
    nav: Option<&'a crate::nav::NavState>,
    world: &'a crate::world::WorldData,
    doors: &'a nvf_play::doors::Doors,
    /// A menu holds the game this frame: the player cannot move.
    in_menu: bool,
    travel: &'a mut MessageWriter<'w, TravelRequest>,
    dt: f32,
}

impl Hands<'_, '_> {
    fn game<R>(&mut self, f: impl FnOnce(&mut Game<'static>) -> R) -> Option<R> {
        let rt = self.script.rt.as_mut()?;
        Some(with_world(rt, self.sim, |rt| rt.player_action(f)))
    }

    fn feet(&self) -> [f32; 3] {
        self.phys.player.as_ref().map_or(self.player.feet, |c| c.feet)
    }

    fn here(&self) -> Place {
        Place { space: space_of(self.location), pos: self.feet() }
    }

    /// The place of a reference (a marker, an actor), by the runner's names.
    fn place_of(&mut self, name: &str) -> Result<(FormId, Place), String> {
        let world = self.world;
        self.game(|g| {
            let r = runner::resolve(g, name)?;
            let (cell, pos) = where_is(g, r).ok_or_else(|| format!("{name}: nowhere in the world"))?;
            let cell = cell.ok_or_else(|| format!("{name}: in no cell"))?;
            let space = crate::nav::space_of_cell(world, cell.0).ok_or_else(|| format!("{name}: its cell {cell} is not indexed"))?;
            Ok((r, Place { space, pos }))
        })
        .unwrap_or_else(|| Err("the scripts are not running".into()))
    }

    /// Put the player at a place without walking (a stand-in, logged by the caller).
    fn put(&mut self, p: Place) {
        if p.space == space_of(self.location) {
            self.phys.place_player(p.pos, 60.0, 1000.0);
            if let Some(c) = self.phys.player.as_ref() {
                self.player.feet = c.feet;
            }
        } else {
            self.travel.write(TravelRequest(travel_to(p, self.player.heading.to_degrees())));
        }
    }
}

enum Walked {
    Going,
    There,
    /// Put there without walking.
    Put,
    /// The player cannot get there: why.
    No(String),
}

/// One frame of a walk.
fn walk(p: &mut Pilot, w: &mut Walk, h: &mut Hands) -> Walked {
    let feet = h.feet();
    let here = h.here();
    // The actor walked to has come and is talking to the player: the player is with it.
    let talker = h.game(|g| g.dialogue.as_ref().map(|c| c.actor)).flatten();
    if talker.is_some() && talker == w.target {
        if !w.told_talking {
            info!("pilot: step {}: the actor walked to is talking to the player: the walk is over", p.i);
        }
        w.told_talking = true;
        return Walked::There;
    }
    w.told_talking = false;
    // A conversation that begins on the way: the steps after the walk hear it, or
    // nothing in the chain answers it and it is left as a player leaves it.
    let began_talking = *w.talking_at_start.get_or_insert(talker.is_some());
    if talker.is_some() && !began_talking {
        if w.keep_talk {
            info!("pilot: step {}: a conversation began on the way: the walk ends here", p.i);
            return Walked::There;
        }
        if !w.dodge.is_some_and(|(t, _)| t < 0.0) {
            info!("pilot: step {}: a conversation began on the way that no step answers: heard out and left by its Goodbye", p.i);
            // (Said once: the marker is a dodge that is over.)
            w.dodge = Some((-1.0, 0.0));
        }
        w.seen = (p.now, feet);
        if leave_talk(p, h) {
            (w.follower, w.door_since, w.dodge) = (None, None, None);
        }
        return Walked::Going;
    }
    // One already open when the walk began holds the player: it is left first, as a
    // player leaves one to go on (`leave_talk`).
    if talker.is_some() {
        w.seen = (p.now, feet);
        if leave_talk(p, h) {
            w.talking_at_start = Some(false);
        }
        return Walked::Going;
    }
    w.talking_at_start = Some(false);
    // The reference walked to has moved: a new path.
    if let Some(now) = w.target.and_then(|t| h.game(|g| where_is(g, t)).flatten()) {
        if hdist(now.1, w.goal.pos) > 96.0 {
            w.goal.pos = now.1;
            if let Some(space) = now.0.and_then(|c| crate::nav::space_of_cell(h.world, c.0)) {
                w.goal.space = space;
            }
            w.follower = None;
        }
    }
    if here.space == w.goal.space && hdist(feet, w.goal.pos) <= w.radius && (feet[2] - w.goal.pos[2]).abs() < 160.0 {
        return Walked::There;
    }
    let Some(nav) = h.nav else {
        p.finding("unreachable", "no navmesh graph in this run (--no-npc-packages?)".into());
        return Walked::No("no navmesh graph".into());
    };
    if w.follower.is_none() {
        // The keys the player carries and the doors standing open, as an NPC's path has
        // them (fnv_ai `path_params`); the graph knows the locks as scripts left them.
        let (keys, open_doors) = h.game(|g| {
            let keys: Vec<u32> = g.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()).map(|inv| inv.iter().map(|(item, _)| item.0).collect()).unwrap_or_default();
            let open: Vec<u32> = g.world.changed().filter(|(_, s)| s.open).map(|(id, _)| id.0).collect();
            (keys, open)
        })
        .unwrap_or_default();
        let mut params = fnv_nav::PathParams { opens_doors: true, ..Default::default() };
        params.keys = keys;
        params.open_doors.extend(open_doors);
        params.refused_doors.extend(w.tried.iter().map(|(d, _)| *d));
        // The last stretch out of doors of a long way: only that is cut short.
        let outdoors = |path: &fnv_nav::Path| path.legs.iter().rposition(|l| matches!(l.space, Space::World(_)));
        match nav.graph.find_path(here, w.goal, &params) {
            Ok(path) if path.length > WALK_MAX && !w.cut && outdoors(&path).is_some() => {
                w.cut = true;
                let leg = &path.legs[outdoors(&path).expect("checked")];
                let at = Place { space: leg.space, pos: tail_point(&leg.points, WALK_TAIL) };
                info!("pilot: step {}: the way is {:.0} units: starts {WALK_TAIL:.0} units before the end of its last stretch out of doors (a stand-in for fast travel)", p.i, path.length);
                h.put(at);
                w.seen = (p.now, at.pos);
                return Walked::Going;
            }
            Ok(path) => {
                info!("pilot: step {}: walks {:.0} units over {} leg(s) to ({:.0}, {:.0}, {:.0})", p.i, path.length, path.legs.len(), w.goal.pos[0], w.goal.pos[1], w.goal.pos[2]);
                w.locked_way = false;
                w.snapped = path.end();
                w.follower = Some(Follower::new(path));
                w.seen = (p.now, feet);
            }
            Err(e) => {
                // Through a locked door, perhaps: then up to that door, as a player goes.
                let mut forced = params.clone();
                forced.open_locked = true;
                match nav.graph.find_path(here, w.goal, &forced) {
                    Ok(path) => {
                        if !w.locked_way {
                            info!("pilot: step {}: the only way to ({:.0}, {:.0}, {:.0}) is through a locked door: the player goes as far as it can", p.i, w.goal.pos[0], w.goal.pos[1], w.goal.pos[2]);
                        }
                        w.locked_way = true;
                        w.snapped = path.end();
                        w.follower = Some(Follower::new(path));
                        w.seen = (p.now, feet);
                    }
                    Err(_) => {
                        let tried = if w.tried.is_empty() { "locked doors or not".to_string() } else { format!("the doors tried: {}", w.tried.iter().map(|(d, l)| format!("{d:08X} ({l})")).collect::<Vec<_>>().join(", ")) };
                        let why = format!("no way from ({:.0}, {:.0}, {:.0}) in {:?} to ({:.0}, {:.0}, {:.0}) in {:?} ({e:?}), {tried}: a player could not get there; the player stays where it is", feet[0], feet[1], feet[2], here.space, w.goal.pos[0], w.goal.pos[1], w.goal.pos[2], w.goal.space);
                        p.finding("unreachable", why.clone());
                        return Walked::No(why);
                    }
                }
            }
        }
    }
    let f = w.follower.as_mut().expect("planned above");
    // In another space than the leg's: through its door, or sent elsewhere.
    if f.space().is_some_and(|s| s != here.space) {
        match f.path.legs.get(f.leg + 1).map(|l| l.space) {
            Some(next) if next == here.space => {
                f.door_used();
                w.door_since = None;
                w.seen = (p.now, feet);
            }
            _ => w.follower = None,
        }
        return Walked::Going;
    }
    match f.steer(feet) {
        Steer::Arrived => return Walked::There,
        Steer::OffPath => {
            w.follower = None;
            w.replans += 1;
        }
        Steer::Walk { dir, .. } => {
            w.door_since = None;
            let want = heading_of(dir);
            h.player.heading = turn_towards(h.player.heading, want, TURN_RATE * h.dt);
            h.player.pitch = turn_towards(h.player.pitch, 0.0, TURN_RATE * h.dt);
            if heading_delta(h.player.heading, want).abs() < 0.7 {
                (h.intent.forward, h.intent.right, h.intent.walk) = (1.0, 0.0, false);
            }
            // Round what it walked into: a step to the side, as a player takes.
            match w.dodge {
                Some((until, side)) if p.now < until => h.intent.right = side,
                _ => w.dodge = None,
            }
            // Held by a script: a wait, and the watchdog's to judge.
            if h.locks.movement || h.in_menu || hdist(feet, w.seen.1) > STUCK_UNITS {
                w.seen = (p.now, feet);
            } else if p.now - w.seen.0 > STUCK_SECS {
                w.seen = (p.now, feet);
                w.follower = None;
                w.replans += 1;
                let side = if w.replans % 2 == 1 { 1.0 } else { -1.0 };
                w.dodge = Some((p.now + 1.2, side));
                h.intent.jump = true;
                info!("pilot: step {}: not moving at ({:.0}, {:.0}, {:.0}): a jump, a step to the {} and a new path ({} of {REPLANS})", p.i, feet[0], feet[1], feet[2], if side > 0.0 { "right" } else { "left" }, w.replans);
            }
        }
        Steer::OpenDoor { door, .. } => {
            if h.doors.is_open(door) == Some(true) {
                f.door_opened();
                w.door_since = None;
            } else if w.door_since.is_none() {
                w.door_since = Some(p.now);
                let r = h.script.rt.as_mut().map(|rt| with_world(rt, h.sim, |rt| rt.activate_as_player(FormId(door))));
                info!("pilot: step {}: opens door {door:08X}: {}", p.i, outcome(r.as_ref()));
            } else if let Some(lock) = lock_words(nav, door).filter(|_| w.door_since.is_some_and(|t| p.now - t > LOCKED_SECS)) {
                info!("pilot: step {}: door {door:08X} on the way is {lock} and nothing opened it in {LOCKED_SECS:.0} s after the player tried it: another way round", p.i);
                w.tried.push((door, lock));
                (w.follower, w.door_since) = (None, None);
                return Walked::Going;
            } else if w.door_since.is_some_and(|t| p.now - t > LOCKED_SECS) {
                p.finding("door", format!("door {door:08X} on the way, not locked, did not open in {LOCKED_SECS:.0} s: walks on as if it had"));
                f.door_opened();
                w.door_since = None;
            }
        }
        Steer::Door(d) => {
            if w.door_since.is_none() {
                w.door_since = Some(p.now);
                let r = h.script.rt.as_mut().map(|rt| with_world(rt, h.sim, |rt| rt.activate_as_player(FormId(d.door))));
                info!("pilot: step {}: goes through door {:08X}: {}", p.i, d.door, outcome(r.as_ref()));
            } else if w.door_since.is_some_and(|t| p.now - t > LOCKED_SECS) {
                // Open now (a script unlocked it meanwhile): tried again on a new path.
                let lock = lock_words(nav, d.door);
                if w.locked_way && lock.is_none() {
                    (w.follower, w.door_since) = (None, None);
                    return Walked::Going;
                }
                let Some(lock) = lock else {
                    let why = format!("door {:08X} is not locked and did not take the player through in {LOCKED_SECS:.0} s after the player tried it (its script refuses the player?): a player could not go on; the player stays in front of it", d.door);
                    p.finding("unreachable", why.clone());
                    return Walked::No(why);
                };
                info!("pilot: step {}: door {:08X} is {lock} and nothing opened it in {LOCKED_SECS:.0} s after the player tried it: another way round", p.i, d.door);
                w.tried.push((d.door, lock));
                (w.follower, w.door_since) = (None, None);
                return Walked::Going;
            }
        }
    }
    if w.replans > REPLANS {
        let end = w.snapped.unwrap_or(w.goal.pos);
        if here.space == w.goal.space && hdist(feet, end) <= PUT_WITHIN {
            p.finding("stuck", format!("could not walk the last {:.0} units to ({:.0}, {:.0}, {:.0}) from ({:.0}, {:.0}, {:.0}) after {REPLANS} new paths: set on the place", hdist(feet, end), end[0], end[1], end[2], feet[0], feet[1], feet[2]));
            h.put(Place { space: w.goal.space, pos: end });
            return Walked::Put;
        }
        let why = format!("could not walk to ({:.0}, {:.0}, {:.0}) from ({:.0}, {:.0}, {:.0}) after {REPLANS} new paths: the player stays where it is", w.goal.pos[0], w.goal.pos[1], w.goal.pos[2], feet[0], feet[1], feet[2]);
        p.finding("unreachable", why.clone());
        return Walked::No(why);
    }
    Walked::Going
}

fn walk_to(goal: Place, target: Option<FormId>, radius: f32, now: f64, feet: [f32; 3]) -> Walk {
    Walk { goal, target, radius, follower: None, door_since: None, seen: (now, feet), replans: 0, cut: false, talking_at_start: None, keep_talk: false, locked_way: false, tried: Vec::new(), told_talking: false, dodge: None, snapped: None }
}

/// A door's lock as the nav graph has it now, in words; `None`: not locked.
fn lock_words(nav: &crate::nav::NavState, door: u32) -> Option<String> {
    nav.graph.door_locks.get(&door).map(|l| match l.key {
        Some(k) => format!("locked, level {}, key {k:08X}", l.level),
        None => format!("locked, level {}, no key", l.level),
    })
}

fn new_walk(goal: Place, target: Option<FormId>, radius: f32, now: f64, feet: [f32; 3]) -> Phase {
    Phase::Walk(walk_to(goal, target, radius, now, feet))
}

/// The nearest living actor in combat whose target is the player or `friend`, within
/// [`THREAT_RANGE`] of `feet`, and where it is.
fn threat(g: &Game, friend: Option<FormId>, feet: [f32; 3]) -> Option<(FormId, Option<FormId>, [f32; 3])> {
    g.awareness
        .actors
        .iter()
        .filter(|(a, _)| **a != FormId::PLAYER_REF && Some(**a) != friend && !g.is_dead(**a) && g.awareness.state(**a) == fnv_game::awareness::State::Combat)
        .filter(|(_, w)| w.combat_target().is_some_and(|(t, _)| t == FormId::PLAYER_REF || Some(t) == friend))
        .filter_map(|(a, _)| where_is(g, *a).map(|(cell, pos)| (*a, cell, pos)))
        .filter(|(_, _, pos)| hdist(*pos, feet) < THREAT_RANGE)
        .min_by(|a, b| hdist(a.2, feet).total_cmp(&hdist(b.2, feet)))
}

/// Answer the level-up menu for the player: the level's skill points spent one at a time
/// down the list, the first perks on offer taken at a perk level. What was done, in words.
fn level_up(g: &mut Game) -> String {
    use fnv_game::levelup::{perk_due, PerkSheet, SkillSheet};
    let level = g.player_level().round() as i32;
    let mut skills = SkillSheet::of_player(g, level);
    let n = skills.skills.len();
    let (mut i, mut refused) = (0, 0);
    while skills.remaining() > 0 && refused < n && n > 0 {
        refused = if skills.raise(i % n) { 0 } else { refused + 1 };
        i += 1;
    }
    let spent = skills.points - skills.remaining();
    skills.apply(g);
    let mut taken = 0;
    if perk_due(g, level) {
        let mut perks = PerkSheet::of_player(g, level);
        for perk in perks.perks.clone() {
            if perks.left() == 0 {
                break;
            }
            if perks.can_take(perk) && perks.toggle(perk) {
                taken += 1;
            }
        }
        perks.apply(g);
    }
    format!("level {level}, {spent} skill points spent, {taken} perk(s) taken")
}

/// Give the player the pilot's weapon and ammo, once a run.
fn arm(p: &mut Pilot, h: &mut Hands) {
    if p.armed {
        return;
    }
    p.armed = true;
    let ids = h.game(|g| (runner::resolve(g, PILOT_WEAPON), runner::resolve(g, PILOT_AMMO)));
    match ids {
        Some((Ok(weapon), Ok(ammo))) => {
            let w = &mut h.sim.world;
            let r = w.add_item(FormId::PLAYER_REF, ammo, 500).and_then(|_| w.add_item(FormId::PLAYER_REF, weapon, 1)).and_then(|_| w.equip(FormId::PLAYER_REF, weapon));
            match r {
                Ok(_) => info!("pilot: the player is given {PILOT_WEAPON} and 500 {PILOT_AMMO} to fight with"),
                Err(e) => p.finding("fight", format!("could not arm the player with {PILOT_WEAPON}: {e}")),
            }
        }
        _ => p.finding("fight", format!("this load order has no {PILOT_WEAPON} or {PILOT_AMMO}: the player fights with what it holds")),
    }
}

/// The weapon in the player's hand is whole, and it is the pilot's (module notes).
fn keep_weapon(p: &mut Pilot, h: &mut Hands) {
    if !p.god || p.now < p.weapon_check {
        return;
    }
    p.weapon_check = p.now + WEAPON_SECS;
    let Some(Ok(weapon)) = h.game(|g| runner::resolve(g, PILOT_WEAPON)) else { return };
    let w = &mut h.sim.world;
    let held = w.inventory_mut(FormId::PLAYER_REF).is_ok_and(|inv| inv.equipped().into_iter().any(|i| i == weapon));
    if !held {
        // A broken one stays in the inventory and cannot be equipped: a new one.
        let carried = w.item_count(FormId::PLAYER_REF, weapon).unwrap_or(0);
        let r = (if carried > 0 { w.remove_item(FormId::PLAYER_REF, weapon, carried).map(|_| ()) } else { Ok(()) }).and_then(|_| w.add_item(FormId::PLAYER_REF, weapon, 1)).and_then(|_| w.equip(FormId::PLAYER_REF, weapon));
        match r {
            Ok(_) => info!("pilot: step {}: the player does not hold {PILOT_WEAPON} (broken, or taken): given a new one and equipped", p.i),
            Err(e) => warn!("pilot: step {}: could not give the player a new {PILOT_WEAPON}: {e}", p.i),
        }
    }
    if let Ok(inv) = w.inventory_mut(FormId::PLAYER_REF) {
        for item in inv.equipped() {
            inv.map_equipped(item, |e| fnv_sim::inventory::Extra { condition: Some(1.0), ..e });
        }
    }
}

/// One frame of shooting at an actor: nearer while it is out of range, the view on its
/// trunk and the trigger pulled when in range. `Some(why)`: the actor cannot be reached.
fn engage(p: &mut Pilot, h: &mut Hands, name: &str, f: &mut Fight, w: &mut Walk) -> Option<String> {
    keep_weapon(p, h);
    let target = f.target;
    h.control.weapon_out = true;
    match walk(p, w, h) {
        Walked::No(why) => {
            f.shooting = None;
            h.control.trigger = false;
            return Some(why);
        }
        Walked::Going => {
            f.shooting = None;
            h.control.trigger = false;
        }
        Walked::There | Walked::Put => {
            let since = *f.shooting.get_or_insert(p.now);
            let at = h.phys.world.actor_aim_point(target.0).or_else(|| h.game(|g| where_is(g, target)).flatten().map(|(_, pos)| [pos[0], pos[1], pos[2] + 80.0]));
            // The line of fire, as the NPCs judge it: to the trunk, else the head.
            let eye = h.player.eye();
            let clear = at.and_then(|at| crate::combat_tactics::shot_clear(Some(&mut *h.phys), eye, None, at, [at[0], at[1], at[2] + 40.0]));
            if let Some(at) = clear.or(at) {
                let cross: Option<Crosshair> = h.control.crosshair;
                (h.player.heading, h.player.pitch) = Crosshair::aim_at(cross.as_ref(), eye, at);
            }
            if clear.is_none() && at.is_some() {
                h.control.trigger = false;
                let since = *f.blocked.get_or_insert(p.now);
                if p.now - since > NO_SHOT_SECS && w.radius > FIGHT_NEAREST {
                    w.radius = (w.radius * 0.66).max(FIGHT_NEAREST);
                    (f.blocked, f.shooting, w.follower) = (None, None, None);
                    info!("pilot: step {}: no clear shot at {name} for {NO_SHOT_SECS} s: the player goes nearer (within {:.0} units)", p.i, w.radius);
                }
                return None;
            }
            f.blocked = None;
            // The trigger is pulled and let go, as for a weapon that fires once a pull.
            h.control.trigger = ((p.now - since) * 6.0) as i64 % 2 == 0;
            if !f.near && p.now - since > FIGHT_CLOSER_SECS {
                f.near = true;
                f.shooting = None;
                w.radius = FIGHT_NEAR;
                info!("pilot: step {}: {name} still lives after {FIGHT_CLOSER_SECS:.0} s of shooting: the player goes closer", p.i);
            }
        }
    }
    None
}

/// One frame of turning the view onto a reference. `Some(true)`: the crosshair has it.
/// `Some(false)`: it never did, at any height.
fn aim(p: &Pilot, a: &mut Aim, h: &mut Hands) -> Option<bool> {
    if h.target.0.as_ref().is_some_and(|t| t.form_id == a.target.0) {
        return Some(true);
    }
    // Already talking to it: there is no crosshair in a conversation, and nothing to ask.
    let target = a.target;
    if h.game(|g| g.dialogue.as_ref().is_some_and(|c| c.actor == target)) == Some(true) {
        return Some(true);
    }
    if let Some(t) = h.target.0.as_ref() {
        a.had = Some(format!("{} {:08X} at {:.0} units", t.name, t.form_id, t.distance));
    }
    if p.now - a.since > AIM_SECS {
        a.height += 1;
        a.since = p.now;
    }
    // Every height tried: the crosshair never took it.
    let Some(&up) = AIM_HEIGHTS.get(a.height) else { return Some(false) };
    let pos = h.game(|g| where_is(g, target)).flatten()?.1;
    let cross: Option<Crosshair> = h.control.crosshair;
    (h.player.heading, h.player.pitch) = Crosshair::aim_at(cross.as_ref(), h.player.eye(), [pos[0], pos[1], pos[2] + up]);
    None
}

/// The conversation's lines are skipped after [`SKIP_AFTER`] s each. Whether a line is
/// being said.
fn hear(p: &mut Pilot, h: &mut Hands) -> bool {
    let stamp = h.game(|g| g.dialogue.as_ref().filter(|c| c.line.is_some()).map(|c| fnv_game::dialogue::conversation_stamp(g.dialogue_serial, Some(c)))).flatten();
    let Some(stamp) = stamp else {
        p.line = None;
        return false;
    };
    match p.line {
        Some((s, since)) if s == stamp => {
            if p.now - since > SKIP_AFTER {
                h.game(|g| g.skip_response());
                p.line = Some((stamp, p.now));
            }
        }
        _ => p.line = Some((stamp, p.now)),
    }
    true
}

/// The offered choice to pick so that the actor comes to say `wanted`, and how many picks
/// after it the line's topic is: the choice whose own line it is (0), else the one from
/// whose line the fewest Choices and Add Topics lead to the topic of `wanted`, at most
/// [`PICKS_DEEP`] picks away. A Goodbye is not a way there.
fn choice_toward(g: &Game, wanted: FormId) -> Option<(usize, usize)> {
    use std::collections::{HashMap, HashSet, VecDeque};
    let offered = g.dialogue_choices();
    if let Some(i) = offered.iter().position(|c| c.info == wanted) {
        return Some((i, 0));
    }
    let target = g.data.infos.get(&wanted)?.topic?;
    if let Some(i) = offered.iter().position(|c| c.topic == target && !c.goodbye) {
        return Some((i, 0));
    }
    // Topic to the topics its lines offer or add.
    let mut next: HashMap<FormId, Vec<FormId>> = HashMap::new();
    for d in g.data.infos.values() {
        if let Some(t) = d.topic {
            next.entry(t).or_default().extend(d.choices.iter().chain(&d.add_topics).copied());
        }
    }
    let mut best: Option<(usize, usize)> = None;
    for (i, c) in offered.iter().enumerate().filter(|(_, c)| !c.goodbye) {
        let Some(line) = g.data.infos.get(&c.info) else { continue };
        let mut seen: HashSet<FormId> = HashSet::new();
        let mut queue: VecDeque<(FormId, usize)> = line.choices.iter().chain(&line.add_topics).map(|t| (*t, 1)).collect();
        while let Some((t, d)) = queue.pop_front() {
            if t == target {
                if best.is_none_or(|(_, b)| d < b) {
                    best = Some((i, d));
                }
                break;
            }
            if d >= PICKS_DEEP || !seen.insert(t) {
                continue;
            }
            queue.extend(next.get(&t).into_iter().flatten().map(|n| (*n, d + 1)));
        }
    }
    best
}

/// One frame of leaving the open conversation as a player can (module notes). Whether no
/// conversation is open any more.
fn leave_talk(p: &mut Pilot, h: &mut Hands) -> bool {
    if h.game(|g| g.dialogue.is_none()) != Some(false) {
        p.stuck_talk = None;
        return true;
    }
    // A line is being said: heard out (skipped after a moment, the player's own key).
    if hear(p, h) {
        return false;
    }
    enum Way {
        Goodbye(usize),
        TopicList,
        None,
        NotYet,
    }
    let way = h
        .game(|g| match g.dialogue.as_ref() {
            Some(c) if c.choices.is_empty() => Way::NotYet,
            Some(c) => match c.choices.iter().position(|x| x.goodbye) {
                Some(i) => Way::Goodbye(i),
                None if c.topic_list => Way::TopicList,
                None => Way::None,
            },
            None => Way::NotYet,
        })
        .unwrap_or(Way::NotYet);
    match way {
        Way::Goodbye(i) => {
            let r = h.game(|g| g.choose(i).map(|_| ()));
            info!("pilot: step {}: leaves the conversation by its Goodbye choice{}", p.i, match r {
                Some(Err(e)) => format!(": {e}"),
                _ => String::new(),
            });
            p.stuck_talk = None;
        }
        Way::TopicList => {
            h.game(|g| g.end_dialogue());
            info!("pilot: step {}: leaves the topic list, which offers no Goodbye (as Tab does)", p.i);
        }
        Way::NotYet => {}
        Way::None => {
            let since = *p.stuck_talk.get_or_insert(p.now);
            if p.now - since > NO_WAY_OUT_SECS {
                let offered = h.game(|g| g.dialogue_choices().iter().map(|c| g.data.name(c.topic)).collect::<Vec<_>>().join(", ")).unwrap_or_default();
                p.finding("no_way_out", format!("the conversation offers no Goodbye and is not a topic list (offered: {offered}): a player could not leave it; broken off so that the chain goes on"));
                h.game(|g| g.end_dialogue());
                p.stuck_talk = None;
            }
        }
    }
    false
}

/// Walk to a reference, turn the view onto it and ask the crosshair; then `act`.
/// Returns when the step is over.
fn approach(p: &mut Pilot, h: &mut Hands, name: &str, what: &str) -> Result<Option<FormId>, String> {
    if matches!(p.phase, Phase::Begin) {
        let (r, place) = h.place_of(name)?;
        p.phase = new_walk(place, Some(r), NEAR_TARGET, p.now, h.feet());
    }
    let mut phase = std::mem::replace(&mut p.phase, Phase::Begin);
    let out = match &mut phase {
        Phase::Walk(w) => match walk(p, w, h) {
            Walked::Going => Ok(None),
            Walked::No(why) => Err(why),
            Walked::There | Walked::Put => {
                let target = w.target.expect("an approach walks to a reference");
                phase = Phase::Aim(Aim { target, height: 0, since: p.now, had: None });
                Ok(None)
            }
        },
        Phase::Aim(a) => match aim(p, a, h) {
            None => Ok(None),
            Some(ok) => {
                if !ok {
                    let feet = h.feet();
                    let had = a.had.clone().map_or("nothing".to_string(), |t| format!("{t} instead"));
                    p.finding("could_not_target", format!("standing at ({:.0}, {:.0}, {:.0}) and looking at {name}, the crosshair took {had}: a player could not {what} it from here", feet[0], feet[1], feet[2]));
                }
                Ok(Some(a.target))
            }
        },
        _ => Ok(None),
    };
    p.phase = phase;
    out
}

/// One frame of the current step. `Some(error)` when the step is over.
fn step(p: &mut Pilot, h: &mut Hands) -> Option<Option<String>> {
    let a = p.actions[p.i].clone();
    match &a {
        Action::Note { .. } => Some(None),
        Action::MovePlayer { to } => {
            // The actor walked to came and is talking to the player: the player is with
            // it (Sunny comes to Ringo's with her own package and line; leaving that
            // conversation to walk to her lost her Say Once greeting and the fight).
            if matches!(p.phase, Phase::Begin) {
                let with_it = h.game(|g| runner::resolve(g, to).is_ok_and(|r| g.dialogue.as_ref().is_some_and(|c| c.actor == r)));
                if with_it == Some(true) {
                    info!("pilot: step {}: {to} is talking to the player: no walk", p.i);
                    return Some(None);
                }
            }
            if matches!(p.phase, Phase::Begin) {
                // The steps after the walk deal with a conversation begun on it?
                let answered = p.actions.iter().skip(p.i + 1).find(|a| !matches!(a, Action::Note { .. })).is_some_and(|a| matches!(a, Action::WaitTalk { .. } | Action::Choose { .. } | Action::Talk { .. }));
                match h.place_of(to) {
                    Ok((r, place)) => {
                        let mut w = walk_to(place, Some(r), NEAR_MARKER, p.now, h.feet());
                        w.keep_talk = answered;
                        p.phase = Phase::Walk(w);
                    }
                    Err(e) => return Some(Some(e)),
                }
            }
            walk_phase(p, h)
        }
        Action::GoToCell { cell } => {
            if matches!(p.phase, Phase::Begin) {
                let world = h.world;
                let place = h.game(|g| runner::cell_arrival(g, cell)).unwrap_or_else(|| Err("the scripts are not running".into())).and_then(|(c, pos)| {
                    let space = crate::nav::space_of_cell(world, c.0).ok_or_else(|| format!("{cell}: not indexed"))?;
                    Ok(Place { space, pos })
                });
                match place {
                    Ok(place) => p.phase = new_walk(place, None, 2.0 * NEAR_MARKER, p.now, h.feet()),
                    Err(e) => return Some(Some(e)),
                }
            }
            walk_phase(p, h)
        }
        Action::TriggerEnter { trigger, actor: Some(actor), .. } if actor != "player" => {
            info!("pilot: step {}: left to the AI: {actor} entering {trigger}", p.i);
            Some(None)
        }
        Action::TriggerEnter { trigger, .. } => {
            if matches!(p.phase, Phase::Begin) {
                match h.place_of(trigger) {
                    Ok((_, place)) => {
                        let mut w = walk_to(place, None, 24.0, p.now, h.feet());
                        w.keep_talk = true;
                        p.phase = Phase::Walk(w);
                    }
                    Err(e) => return Some(Some(e)),
                }
            }
            // A second inside it, for its script to see the player.
            match walk_phase(p, h) {
                Some(None) if p.part == 0 => {
                    (p.part, p.phase) = (1, Phase::Wait { until: p.now + 1.0 });
                    None
                }
                other => other,
            }
        }
        Action::Talk { actor, topic, label } => {
            // The actor came and spoke first (a force greet): that is the conversation.
            if p.part == 0 && matches!(p.phase, Phase::Begin) {
                let with_it = h.game(|g| runner::resolve(g, actor).is_ok_and(|r| g.dialogue.as_ref().is_some_and(|c| c.actor == r)));
                if with_it == Some(true) {
                    info!("pilot: step {}: {actor} is already talking to the player", p.i);
                    p.part = 2;
                }
            }
            if p.part == 0 {
                match approach(p, h, actor, "talk to") {
                    Err(e) => return Some(Some(e)),
                    Ok(None) => return None,
                    Ok(Some(target)) => {
                        let topic = topic.clone();
                        let started = match topic {
                            // A named topic is the headless run's own opening: said as it says it.
                            Some(t) => h.game(|g| runner::resolve(g, &t).and_then(|t| g.start_dialogue(target, Some(t))).map(|_| ())),
                            None => h.script.rt.as_mut().map(|rt| {
                                with_world(rt, h.sim, |rt| rt.activate_as_player(target)).and_then(|by| match by {
                                    // A script may start the conversation itself, a moment later.
                                    // (So may its default action: the conversation is waited for.)
                                    ByHand::Talk | ByHand::Greeted | ByHand::Choices | ByHand::Script | ByHand::Default(_) => Ok(()),
                                    other => Err(format!("activating {actor} did not start a conversation: {}", by_hand(&other))),
                                })
                            }),
                        };
                        if let Some(Err(e)) = started {
                            return Some(Some(e));
                        }
                        (p.part, p.phase) = (2, Phase::Wait { until: p.now + 5.0 });
                    }
                }
            }
            if h.game(|g| g.dialogue.is_some()) == Some(true) {
                if let Some(l) = label {
                    p.observe(l, &Map::new(), true);
                }
                return Some(None);
            }
            match p.phase {
                Phase::Wait { until } if p.now < until => None,
                _ => Some(Some(format!("no conversation with {actor} 5 s after talking to it"))),
            }
        }
        Action::PlayerActivate { target, .. } => match approach(p, h, target, "activate") {
            Err(e) => Some(Some(e)),
            Ok(None) => None,
            Ok(Some(r)) => {
                let done = h.script.rt.as_mut().map(|rt| with_world(rt, h.sim, |rt| rt.activate_as_player(r)));
                match done {
                    Some(Ok(by)) => {
                        info!("pilot: step {}: activates {target}: {}", p.i, by_hand(&by));
                        Some(None)
                    }
                    Some(Err(e)) => Some(Some(e)),
                    None => Some(Some("the scripts are not running".into())),
                }
            }
        },
        Action::WaitTalk { max_frames, start, .. } => {
            let limit = (*max_frames as f64 / 60.0).clamp(5.0, WAIT_CAP);
            let talking = h.game(|g| g.dialogue.is_some()) == Some(true);
            if *start && !talking && p.part == 0 {
                if p.now - p.step_from > limit {
                    p.finding("no_conversation", format!("nobody started a conversation with the player in {limit:.0} s (the headless run stands in for the actor's arrival)"));
                    return Some(Some("no conversation started".into()));
                }
                return None;
            }
            p.part = 1;
            if !hear(p, h) {
                return Some(None);
            }
            (p.now - p.step_from > limit).then(|| Some("the line did not end".to_string()))
        }
        Action::Choose { topic, prompt, index, max_frames, label } => {
            let limit = (*max_frames as f64 / 60.0).clamp(5.0, WAIT_CAP);
            if hear(p, h) {
                return (p.now - p.step_from > limit).then(|| Some("the line before the choice did not end".to_string()));
            }
            let picked = h.game(|g| {
                if g.dialogue.is_none() {
                    return Err("no conversation: nothing to choose".to_string());
                }
                if g.dialogue_choices().is_empty() {
                    return Ok(None);
                }
                let i = runner::choice_index(g, topic, prompt, index)?;
                g.choose(i).map(Some)
            });
            match picked {
                Some(Ok(Some(_))) => {
                    if let Some(l) = label {
                        p.observe(l, &Map::new(), true);
                    }
                    Some(None)
                }
                Some(Ok(None)) => (p.now - p.step_from > limit).then(|| Some("no choices were offered".to_string())),
                Some(Err(e)) => Some(Some(e)),
                None => Some(Some("the scripts are not running".into())),
            }
        }
        Action::AdvanceSeconds { seconds } => wait_for(p, seconds.min(ADVANCE_CAP)),
        Action::WaitFrames { frames } => wait_for(p, (*frames as f64 / 60.0).min(ADVANCE_CAP)),
        Action::WaitQuest { quest, field, value, max_frames, label, .. } => {
            let limit = (*max_frames as f64 / 60.0).clamp(10.0, WAIT_CAP);
            match h.game(|g| runner::quest_field(g, quest, field)) {
                Some(Ok(v)) if same(&v, value) => {
                    p.observe(label, &Map::from_iter([(field.clone(), v)]), true);
                    Some(None)
                }
                Some(Ok(v)) if p.now - p.step_from > limit => {
                    p.finding("wait_timed_out", format!("{quest} {field} is {v}, not {value}, after {limit:.0} s"));
                    p.observe(label, &Map::from_iter([(field.clone(), v)]), true);
                    Some(Some(format!("{quest} {field} did not become {value}")))
                }
                Some(Ok(_)) => None,
                Some(Err(e)) => Some(Some(e)),
                None => Some(Some("the scripts are not running".into())),
            }
        }
        Action::EndTalk { .. } => {
            if leave_talk(p, h) {
                return Some(None);
            }
            (p.now - p.step_from > 60.0).then(|| Some("the conversation could not be left in 60 s".to_string()))
        }
        Action::GetQuest { quest, fields, label } => {
            let read = h.game(|g| fields.iter().map(|f| runner::quest_field(g, quest, f).map(|v| (f.clone(), v))).collect::<Result<Map<String, Json>, String>>());
            match read {
                None => Some(Some("the scripts are not running".into())),
                Some(Err(e)) => Some(Some(e)),
                Some(Ok(read)) => {
                    // A read that differs is read again: the game takes real time for
                    // what the headless run does in a frame.
                    let label = label.clone().unwrap_or_else(|| format!("step {}", p.i));
                    let last = p.now - p.step_from > MILESTONE_SECS;
                    (p.observe(&label, &read, last) || last).then_some(None)
                }
            }
        }
        Action::GetRef { fields: Some(fields), of, reference, label } => {
            let Some(name) = reference.as_ref().or(of.as_ref()) else { return Some(Some("get_ref needs 'ref'".into())) };
            let read = h.game(|g| fields.iter().map(|f| runner::reference_field(g, name, f).map(|v| (f.clone(), v))).collect::<Result<Map<String, Json>, String>>());
            match read {
                None => Some(Some("the scripts are not running".into())),
                Some(Err(e)) => Some(Some(e)),
                Some(Ok(read)) => {
                    let label = label.clone().unwrap_or_else(|| format!("step {}", p.i));
                    p.observe(&label, &read, true);
                    Some(None)
                }
            }
        }
        Action::Kill { target, actor, .. } if target.is_some() || actor.is_some() => {
            let name = target.clone().or_else(|| actor.clone()).expect("checked");
            fight(p, h, &name, &a)
        }
        Action::SayInfo { info, speaker: Some(speaker), .. } if p.campaign => {
            // Begun since the chain began, not since this step did: the game does not wait
            // for the chain's steps, and a line said while an earlier step was still being
            // carried out is said (Sunny's lines at the wells came during the fights; the
            // player then stood by her for two minutes a line, asking her what was up).
            if matches!(p.phase, Phase::Begin) {
                let (r, place) = match h.place_of(speaker) {
                    Ok(x) => x,
                    Err(e) => return Some(Some(e)),
                };
                p.still = (p.now, place.pos);
                p.part = 0;
                let mut w = walk_to(place, Some(r), NEAR_TARGET, p.now, h.feet());
                w.keep_talk = true;
                p.phase = Phase::Walk(w);
            }
            let mark = p.mark;
            let begun = h.game(|g| runner::resolve(g, info).is_ok_and(|i| g.softlock_seen.lines_since(mark).any(|l| l == i) || g.dialogue.as_ref().and_then(|c| c.line.as_ref()).is_some_and(|l| l.info == i)));
            if begun == Some(true) {
                info!("pilot: step {}: {speaker} has said {info} ({:.1} s into the step)", p.i, p.now - p.step_from);
                p.phase = Phase::Begin;
                return Some(None);
            }
            if p.now - p.step_from > LINE_SECS {
                p.finding("line_not_said", format!("{speaker} did not say {info} in {LINE_SECS:.0} s with the player beside it: said as the headless run says it"));
                let mut applier = std::mem::take(&mut p.applier);
                let r = h.game(|g| runner::apply_now(g, &mut applier, &a));
                p.applier = applier;
                return Some(r.and_then(|r| r.err()));
            }
            // Its other lines are heard out (and skipped) on the way.
            let saying = hear(p, h);
            let talking = saying || h.game(|g| g.dialogue.is_some()) == Some(true);
            // In a conversation with it, choosing: the pick that brings the line.
            if talking && !saying {
                let picked = h.game(|g| {
                    let wanted = runner::resolve(g, info).ok()?;
                    let with_it = runner::resolve(g, speaker).is_ok_and(|r| g.dialogue.as_ref().is_some_and(|c| c.actor == r));
                    if !with_it || g.dialogue_choices().is_empty() {
                        return None;
                    }
                    let (i, away) = choice_toward(g, wanted)?;
                    let text = g.choice_text(&g.dialogue_choices()[i]).to_string();
                    g.choose(i).ok().map(|_| (text, away))
                });
                if let Some(Some((text, away))) = picked {
                    info!("pilot: step {}: picks {text:?} for the line {info} ({})", p.i, if away == 0 { "its own choice".to_string() } else { format!("{away} pick(s) from its topic") });
                    return None;
                }
            }
            let mut phase = std::mem::replace(&mut p.phase, Phase::Begin);
            if let Phase::Walk(w) = &mut phase {
                let walked = walk(p, w, h);
                if let Walked::No(why) = walked {
                    return Some(Some(why));
                }
                let there = matches!(walked, Walked::There | Walked::Put);
                let at = w.goal.pos;
                if hdist(at, p.still.1) > 16.0 || talking || !there {
                    p.still = (p.now, at);
                } else if p.part == 0 && p.now - p.still.0 > STILL_SECS {
                    p.part = 1;
                    let target = w.target.expect("walks to the speaker");
                    let r = h.script.rt.as_mut().map(|rt| with_world(rt, h.sim, |rt| rt.activate_as_player(target)));
                    info!("pilot: step {}: {speaker} stands silent: the player talks to it for the line {info}: {}", p.i, outcome(r.as_ref()));
                }
                // A walk that ended is begun again: the speaker may go on.
                if there {
                    w.follower = None;
                }
            }
            p.phase = phase;
            None
        }
        // A console line that moves another actor stands in for its walk (Beagle's run
        // back to Primm): the AI's to make.
        Action::Call { line, .. } if p.campaign && line.to_ascii_lowercase().contains(".moveto") && !line.to_ascii_lowercase().starts_with("player") => {
            info!("pilot: step {}: left to the AI: {line}", p.i);
            Some(None)
        }
        Action::SayFirst { .. } | Action::SayToDone { .. } if p.campaign => {
            info!("pilot: step {}: left to the game: a line the headless run says for an actor", p.i);
            Some(None)
        }
        Action::WaitMenu { menu, max_frames, label, .. } => {
            let limit = (*max_frames as f64 / 60.0).clamp(5.0, WAIT_CAP);
            let open = h.game(|g| g.menu_open(*menu)) == Some(true);
            if open || p.now - p.step_from > limit {
                if !open {
                    p.finding("wait_timed_out", format!("menu {menu} did not open in {limit:.0} s"));
                }
                p.observe(label, &Map::from_iter([("found".to_string(), json!(open))]), true);
                return Some((!open).then(|| format!("menu {menu} did not open")));
            }
            None
        }
        Action::GetMenus { menus, label } => {
            let read = h.game(|g| menus.iter().map(|c| (format!("menu.{c}"), json!(g.menu_open(*c)))).collect::<Map<String, Json>>()).unwrap_or_default();
            p.observe(label, &read, true);
            Some(None)
        }
        Action::PressButton { button } => {
            // As the play loop answers a box: the button is set, the box stays a moment so
            // that MenuMode blocks read it, then it closes.
            if p.part == 0 {
                let set = h.game(|g| match g.message_box.as_mut() {
                    Some(m) => m.can_press(*button).map(|_| m.pressed = Some(*button)).map(|_| true),
                    None => Ok(false),
                });
                return match set {
                    Some(Ok(true)) => {
                        (p.part, p.phase) = (1, Phase::Wait { until: p.now + 0.1 });
                        None
                    }
                    Some(Ok(false)) => (p.now - p.step_from > 10.0).then(|| Some("no message box opened in 10 s".to_string())),
                    Some(Err(e)) => Some(Some(e)),
                    None => Some(Some("the scripts are not running".into())),
                };
            }
            match p.phase {
                Phase::Wait { until } if p.now < until => None,
                _ => {
                    h.game(|g| (g.menu == Some(fnv_game::game::menus::MESSAGE)).then(|| g.close_menu()));
                    Some(None)
                }
            }
        }
        Action::DismissMessageBoxes { max, wait_frames, label } => {
            // `part` counts the boxes closed; the wait for each starts again in `phase`.
            let wait = (*wait_frames as f64 / 60.0).clamp(1.0, 30.0);
            if matches!(p.phase, Phase::Begin) {
                p.phase = Phase::Wait { until: p.now + wait };
            }
            if h.game(|g| g.menu == Some(fnv_game::game::menus::MESSAGE) && g.close_menu().is_some()) == Some(true) {
                p.part += 1;
                p.phase = Phase::Wait { until: p.now + wait };
            }
            let over = p.part as u32 >= *max || matches!(p.phase, Phase::Wait { until } if p.now >= until);
            if over {
                if let Some(l) = label {
                    p.observe(l, &Map::from_iter([("closed".to_string(), json!(p.part))]), true);
                }
                return Some(None);
            }
            None
        }
        Action::Package { actor, package, event } => {
            info!("pilot: step {}: left to the AI: {actor}'s package {package} ({event})", p.i);
            Some(None)
        }
        Action::PatrolPoint { actor, point } => {
            info!("pilot: step {}: left to the AI: {actor} reaching {point}", p.i);
            Some(None)
        }
        Action::DialoguePackage { actor, .. } => {
            info!("pilot: step {}: left to the AI: {actor}'s dialogue package", p.i);
            Some(None)
        }
        _ if !runner::applies_now(&a) => {
            p.finding("not_carried_out", "the pilot has no way to do this step yet".into());
            Some(None)
        }
        // Everything else is the headless run's own step on the running game.
        _ => {
            let mut applier = std::mem::take(&mut p.applier);
            let r = h.game(|g| runner::apply_now(g, &mut applier, &a));
            p.applier = applier;
            match r {
                None => Some(Some("the scripts are not running".into())),
                Some(Err(e)) => {
                    if let Some(l) = runner::label_of(&a) {
                        p.observe(&l, &Map::from_iter([("errors".to_string(), json!({"action": e}))]), true);
                    }
                    Some(Some(e))
                }
                Some(Ok(None)) => Some(None),
                Some(Ok(Some((label, fields)))) => {
                    p.observe(&label, &fields, true);
                    Some(None)
                }
            }
        }
    }
}

/// One frame of a `kill` step fought out (module notes).
fn fight(p: &mut Pilot, h: &mut Hands, name: &str, a: &Action) -> Option<Option<String>> {
    if matches!(p.phase, Phase::Begin) {
        let (r, place) = match h.place_of(name) {
            Ok(x) => x,
            Err(e) => return Some(Some(e)),
        };
        arm(p, h);
        let Phase::Walk(w) = new_walk(place, Some(r), FIGHT_RANGE, p.now, h.feet()) else { unreachable!() };
        p.phase = Phase::Fight(Fight { target: r, shooting: None, near: false, blocked: None }, w);
    }
    let Phase::Fight(mut f, mut w) = std::mem::replace(&mut p.phase, Phase::Begin) else { return Some(None) };
    let target = f.target;
    if h.game(|g| g.is_dead(target)) == Some(true) {
        info!("pilot: step {}: {name} is dead after {:.1} s of the fight", p.i, p.now - p.step_from);
        h.control.trigger = false;
        return Some(None);
    }
    if p.now - p.step_from > FIGHT_SECS {
        p.finding("fight_not_won", format!("{name} still lives after {FIGHT_SECS:.0} s of shooting at it with {PILOT_WEAPON}: killed as the headless run kills it"));
        h.control.trigger = false;
        let mut applier = std::mem::take(&mut p.applier);
        let r = h.game(|g| runner::apply_now(g, &mut applier, a));
        p.applier = applier;
        return Some(r.and_then(|r| r.err()));
    }
    if let Some(why) = engage(p, h, name, &mut f, &mut w) {
        return Some(Some(why));
    }
    p.phase = Phase::Fight(f, w);
    None
}

fn wait_for(p: &mut Pilot, seconds: f64) -> Option<Option<String>> {
    (p.now - p.step_from >= seconds).then_some(None)
}

/// A step that is one walk: over when the player is there.
fn walk_phase(p: &mut Pilot, h: &mut Hands) -> Option<Option<String>> {
    let mut phase = std::mem::replace(&mut p.phase, Phase::Begin);
    let out = match &mut phase {
        Phase::Walk(w) => match walk(p, w, h) {
            Walked::Going => None,
            Walked::No(why) => Some(Some(why)),
            Walked::There | Walked::Put => Some(None),
        },
        Phase::Wait { until } => (p.now >= *until).then_some(None),
        _ => Some(None),
    };
    p.phase = phase;
    out
}

/// The run ends when the pilot has not been able to act for [`HELD_SECS`] of real time.
fn held(real: Res<Time<Real>>, mut pilot: ResMut<Pilot>, watch: Option<Res<nvf_play::softlock::SoftlockWatch>>, state: Option<Res<State<PlayState>>>, mut exit: MessageWriter<AppExit>) {
    let p = &mut *pilot;
    let now = real.elapsed_secs_f64();
    let since = *p.acted_at.get_or_insert(now);
    if p.done || now - since < HELD_SECS {
        return;
    }
    p.done = true;
    p.finding("pilot_held", format!("the pilot could not act for {HELD_SECS:.0} s of real time (the game's state: {:?}): the run ends here", state.map(|s| *s.get())));
    let softlocks = watch.map_or(0, |w| w.found);
    if let Some(out) = &p.out {
        if let Err(e) = serde_json::to_string_pretty(&p.report(softlocks)).map_err(|e| e.to_string()).and_then(|t| std::fs::write(out, t).map_err(|e| e.to_string())) {
            error!("pilot: report {}: {e}", out.display());
        }
    }
    info!("pilot: done (held): {} chain(s), {} steps, {} divergences, {} findings, {softlocks} soft locks in {:.0} s", p.chains_done, p.steps.len(), p.divergences.len(), p.findings.len(), p.now);
    exit.write(AppExit::Success);
}

/// Where things are, for [`drive`].
type Places<'w> = (Res<'w, Location>, Res<'w, Target>, Option<Res<'w, crate::nav::NavState>>, Res<'w, crate::world::WorldData>, Res<'w, nvf_play::doors::Doors>);

#[allow(clippy::too_many_arguments)]
fn drive(
    real: Res<Time<Real>>,
    state: Res<State<PlayState>>,
    (loading, paused): (Res<Loading>, Res<nvf_play::pause::Paused>),
    mut pilot: ResMut<Pilot>,
    (mut script, mut sim, mut phys): (ResMut<ScriptState>, ResMut<SimState>, ResMut<PhysState>),
    (mut player, mut intent, mut control, locks): (ResMut<Player>, ResMut<MoveIntent>, ResMut<PlayerControl>, Res<ControlLocks>),
    (location, target, nav, world, doors): Places,
    watch: Option<Res<nvf_play::softlock::SoftlockWatch>>,
    (mut travel, mut save): (MessageWriter<TravelRequest>, MessageWriter<nvf_play::save::SaveRequest>),
    mut exit: MessageWriter<AppExit>,
) {
    let p = &mut *pilot;
    // In play, in a conversation, and in a menu the scripts opened (the opening's menus,
    // a message box): not behind the pause menu or a loading screen.
    let scripts_menu = *state.get() == PlayState::Menu && !paused.0 && script.rt.as_ref().is_some_and(|rt| rt.game.menu.is_some());
    if p.done || loading.active() || !(matches!(*state.get(), PlayState::Playing | PlayState::Dialogue) || scripts_menu) || script.rt.is_none() {
        return;
    }
    let dt = real.delta_secs().min(0.25);
    p.now += dt as f64;
    p.acted_at = Some(real.elapsed_secs_f64());
    if p.now < SETTLE_SECS {
        return;
    }
    let mut h = Hands {
        script: &mut script,
        sim: &mut sim,
        phys: &mut phys,
        player: &mut player,
        intent: &mut intent,
        control: &mut control,
        locks: &locks,
        location: &location,
        target: &target,
        nav: nav.as_deref(),
        world: &world,
        doors: &doors,
        in_menu: scripts_menu,
        travel: &mut travel,
        dt,
    };
    if !p.given_applied {
        p.given_applied = true;
        let all = p.given.clone();
        // In a campaign the given state is a precondition (module notes).
        let (given, checked): (Vec<Json>, Vec<Json>) = match (p.campaign, p.chains_done) {
            (false, _) => (all, Vec::new()),
            (true, 0) => (Vec::new(), Vec::new()),
            (true, _) => all.into_iter().filter(|e| e["kind"] != "menu" && e["kind"] != "place" && e["kind"] != "ref" && e["kind"] != "disabled").partition(|e| e["kind"] != "quest"),
        };
        for e in &checked {
            let Some(quest) = e["quest"].as_str() else { continue };
            let differing: Vec<String> = ["running", "completed", "stage"]
                .iter()
                .filter(|f| !e[**f].is_null())
                .filter_map(|f| match h.game(|g| runner::quest_field(g, quest, f)) {
                    Some(Ok(v)) if !same(&v, &e[*f]) => Some(format!("{f} is {v}, the chain starts from {}", e[*f])),
                    _ => None,
                })
                .collect();
            if !differing.is_empty() {
                p.finding("precondition", format!("{quest}: {}", differing.join("; ")));
            }
        }
        let errors = h.game(|g| runner::apply_given_entries(g, &given)).unwrap_or_default();
        // In a campaign a line said in an earlier chain is said (ghost-town-gunfight opens
        // with the line intro ends on): the count is taken once, at the first chain.
        if !p.campaign || p.chains_done == 0 {
            p.mark = h.game(|g| g.softlock_seen.lines_begun).unwrap_or(0);
        }
        info!("pilot: chain {}: {} given entries applied, {} checked, {} errors", p.chain, given.len(), checked.len(), errors.len());
        for e in errors {
            p.finding("given", e.to_string());
        }
        p.step_from = p.now;
        return;
    }
    // A message box no step waits for: closed as a player presses OK.
    let waited_for = matches!(p.actions.get(p.i), Some(Action::PressButton { .. } | Action::DismissMessageBoxes { .. } | Action::WaitMenu { .. } | Action::GetMenus { .. } | Action::CloseMenus { .. }));
    let boxed = h.game(|g| g.menu == Some(fnv_game::game::menus::MESSAGE)) == Some(true);
    match (boxed && !waited_for, p.box_since) {
        (true, None) => p.box_since = Some(p.now),
        // The button picked a moment ago has been read by the box's script: it closes.
        (true, Some(t)) if p.box_picked && p.now - t > BOX_SECS + 0.15 => {
            h.game(|g| g.close_menu());
            (p.box_since, p.box_picked) = (None, false);
        }
        (true, Some(t)) if !p.box_picked && p.now - t > BOX_SECS => {
            let picked = h.game(|g| {
                let last = g.message_box.as_ref().filter(|m| m.buttons > 1 && m.waiting()).and_then(|m| (0..m.buttons).rev().find(|b| !m.hidden.contains(b)));
                match (last, g.message_box.as_mut()) {
                    (Some(b), Some(m)) => {
                        m.pressed = Some(b as i32);
                        Some(b)
                    }
                    _ => {
                        g.close_menu();
                        None
                    }
                }
            });
            match picked.flatten() {
                Some(b) => {
                    info!("pilot: step {}: a message box no step waits for has several buttons: picks the last, button {b}", p.i);
                    p.box_picked = true;
                }
                None => {
                    info!("pilot: step {}: closes a message box no step waits for", p.i);
                    p.box_since = None;
                }
            }
        }
        (false, _) => (p.box_since, p.box_picked) = (None, false),
        _ => {}
    }
    // Another menu the scripts opened that no step waits for: the level-up menu is
    // answered as a player answers it, anything else closed after a while.
    // Not one a step soon to come waits for: the game is often ahead of the chain (the
    // opening's name menu was up 20 s before its wait_menu step, was closed here, and
    // the step then waited a minute for it).
    // Nor one the chain itself is about to close.
    let coming = |m: u16| {
        p.actions.iter().skip(p.i).take(24).any(|a| matches!(a, Action::WaitMenu { menu, .. } if *menu == m) || matches!(a, Action::GetMenus { menus, .. } if menus.contains(&m)))
            || p.actions.iter().skip(p.i).take(6).any(|a| matches!(a, Action::CloseMenus { .. }))
    };
    let menu = h.game(|g| g.menu).flatten().filter(|m| *m != fnv_game::game::menus::MESSAGE && !waited_for && !coming(*m));
    match (menu, p.menu_since) {
        (Some(m), Some((was, t))) if m == was => {
            let up = p.now - t;
            if m == fnv_game::game::menus::LEVEL_UP && up > BOX_SECS {
                let mut told = String::new();
                let done = nvf_play::chargen::finish(h.script, h.sim, m, "the pilot's Done", |g| told = level_up(g));
                info!("pilot: step {}: level-up menu answered ({}): {told}", p.i, if done { "done" } else { "not open" });
                p.menu_since = None;
            } else if up > MENU_SECS {
                h.game(|g| g.close_all_menus());
                p.finding("menu_closed", format!("menu {m} was up for {MENU_SECS:.0} s with no step waiting for it: closed"));
                p.menu_since = None;
            }
        }
        (Some(m), _) => p.menu_since = Some((m, p.now)),
        (None, _) => p.menu_since = None,
    }
    // Something attacks the player or the actor it is with: fought first (module notes).
    if *state.get() == PlayState::Playing && !matches!(p.phase, Phase::Fight(..)) {
        if p.defend.is_none() && p.now >= p.threat_check {
            p.threat_check = p.now + 0.5;
            let friend = match &p.phase {
                Phase::Walk(w) => w.target,
                Phase::Aim(a) => Some(a.target),
                _ => None,
            };
            let feet = h.feet();
            let world = h.world;
            if let Some((who, cell, pos)) = h.game(|g| threat(g, friend, feet)).flatten() {
                let space = cell.and_then(|c| crate::nav::space_of_cell(world, c.0)).unwrap_or(space_of(h.location));
                arm(p, &mut h);
                info!("pilot: step {}: {who} attacks {}: fought before the step goes on", p.i, if friend.is_some() { "the player or the actor it is with" } else { "the player" });
                p.defend = Some((Fight { target: who, shooting: None, near: false, blocked: None }, walk_to(Place { space, pos }, Some(who), FIGHT_RANGE, p.now, feet)));
                p.defend_from = p.now;
            }
        }
        if let Some((mut f, mut w)) = p.defend.take() {
            let who = f.target;
            let dead = h.game(|g| g.is_dead(who)) == Some(true);
            if dead || p.now - p.defend_from > DEFEND_SECS {
                h.control.trigger = false;
                if dead {
                    info!("pilot: step {}: the attacker {who} is dead after {:.1} s", p.i, p.now - p.defend_from);
                } else {
                    p.finding("fight_not_won", format!("the attacker {who} still lives after {DEFEND_SECS:.0} s of shooting at it: the step goes on"));
                    p.threat_check = p.now + 20.0;
                }
            } else {
                if engage(p, &mut h, "the attacker", &mut f, &mut w).is_some() {
                    // Out of reach: left alone for a while.
                    p.threat_check = p.now + 20.0;
                    return;
                }
                p.defend = Some((f, w));
                // The step's own time stands still meanwhile.
                p.step_from += dt as f64;
                p.still.0 += dt as f64;
                return;
            }
        }
    }
    // Steps that take no time follow one another within the frame.
    for _ in 0..64 {
        if p.i >= p.actions.len() {
            break;
        }
        match step(p, &mut h) {
            Some(error) => p.next(error),
            // No step holds the run up for good, whatever went wrong in it.
            None if p.now - p.step_from > STEP_SECS => {
                p.finding("step_timed_out", format!("the step was not over after {STEP_SECS:.0} s: given up"));
                h.control.trigger = false;
                p.next(Some("the step timed out".into()));
            }
            None => return,
        }
    }
    if p.i < p.actions.len() {
        return;
    }
    // The save after the chain is asked for once nothing holds the game, then waited for.
    if p.campaign && !p.saved {
        if *state.get() != PlayState::Playing {
            leave_talk(p, &mut h);
            return;
        }
        let name = format!("pilot-after-{}", p.chain);
        info!("pilot: chain {} over: saving {name}", p.chain);
        save.write(nvf_play::save::SaveRequest { name, kind: fnv_save::SaveKind::Manual });
        (p.saved, p.resume_at) = (true, p.now + SAVE_SECS);
        return;
    }
    if p.now < p.resume_at {
        return;
    }
    p.saved = false;
    p.chains_done += 1;
    info!("pilot: chain {} over at {:.0} s: {} divergences and {} findings so far", p.chain, p.now, p.divergences.len(), p.findings.len());
    if p.next_chain() {
        return;
    }
    p.done = true;
    let softlocks = watch.map_or(0, |w| w.found);
    let failed = p.steps.iter().filter(|s| s.get("error").is_some()).count();
    info!("pilot: done: {} chain(s), {} steps ({failed} failed), {} divergences, {} findings, {softlocks} soft locks in {:.0} s", p.chains_done, p.steps.len(), p.divergences.len(), p.findings.len(), p.now);
    if let Some(out) = &p.out {
        match serde_json::to_string_pretty(&p.report(softlocks)).map_err(|e| e.to_string()).and_then(|t| std::fs::write(out, t).map_err(|e| e.to_string())) {
            Ok(()) => info!("pilot: report written to {}", out.display()),
            Err(e) => error!("pilot: report {}: {e}", out.display()),
        }
    }
    exit.write(AppExit::Success);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tail_point_is_that_far_before_the_end() {
        let pts = [[0.0, 0.0, 0.0], [100.0, 0.0, 10.0], [100.0, 50.0, 10.0]];
        assert_eq!(tail_point(&pts, 20.0), [100.0, 30.0, 10.0]);
        let p = tail_point(&pts, 100.0);
        assert!((p[0] - 50.0).abs() < 1e-3 && p[1] == 0.0 && (p[2] - 5.0).abs() < 1e-3, "{p:?}");
        assert_eq!(tail_point(&pts, 1000.0), [0.0, 0.0, 0.0], "a short path starts at its start");
    }

    #[test]
    fn numbers_compare_within_a_hair_and_the_rest_exactly() {
        assert!(same(&json!(10), &json!(10.0)));
        assert!(!same(&json!(10), &json!(20)));
        assert!(same(&json!(true), &json!(true)));
        assert!(!same(&json!("a"), &json!("b")));
    }
}
