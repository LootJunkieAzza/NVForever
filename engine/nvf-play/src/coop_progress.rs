//! Shared co-op progression (private fork, not upstream; owner's asks of 2026-11-04):
//!
//! - **Quest rewards.** When the host completes a quest, each guest gets that quest's XP
//!   and caps (`fnv_game::coop_rewards`: the host's game measures what its scripts gave
//!   for it; a guest takes it with `Game::take_coop_reward`). Item rewards stay the
//!   host's: they belong to a later "synced progression" option, not built.
//! - **Quests ended** (item 205). Each quest the host completes or fails (a Fail Quest
//!   stage, FailQuest) reaches each guest at once and in order ([`Shared::QuestEnded`]),
//!   apart from the completion's reward: its "Quest completed" or "Quest failed" line
//!   ("Quest failed" is NVForever's own words with no sound: the data has neither,
//!   docs/spec-notes/quest-messages.md §5) and a done row in the guest's quest list,
//!   beside the host's active quest (`Game::coop_host_ended`).
//!   Objectives have no failed state in this game (no SetObjectiveFailed among the
//!   functions [observed: schemas/script/functions.json]), so none is sent.
//! - **A save by anyone saves everyone** (anti-exploit: nobody keeps a save the others
//!   lack). A save this game makes is announced ([`Shared::Save`] out), and a save
//!   another player made is followed: this game saves the same kind as soon as saving is
//!   allowed (`save::not_now`). A followed save is not announced again, and an autosave
//!   (or system save) within [`AUTO_QUIET`] seconds of the last shared save is not
//!   announced either, so a trip that autosaves in every game does not set off rounds of
//!   saves. A guest's save is still its own game where it left it, with the character as
//!   it is now (`save::SoloBase`, commit f83a276a).
//! - **Everyone down: back to the last shared save.** When every player is down at once
//!   (the others lost or down: nobody left to revive) for [`DOWN_HOLD`] seconds, the
//!   host tells every game to load its last shared save ([`Shared::Reload`]) and loads
//!   its own [`RELOAD_DELAY`] seconds later, so the message is out before its game starts
//!   again. A load restarts the game with its lobby (nvforever `coop`), so the guests
//!   come back into the host's world as it was at that save. The last shared save's path
//!   is kept in [`SHARED_ENV`] across those restarts. A guest's save is its own game where
//!   it left it, so its new run is taken beside the host ([`REJOIN_ENV`]; item 152).
//!
//! - **A guest down far from everyone: its own last save.** A guest whose health runs out
//!   with no other player up near it (nobody to revive it; nvforever `coop` decides,
//!   [`CoopProgress::alone_down`]) loads its last save after [`ALONE_HOLD`] seconds, as a
//!   player alone does (owner, item 129 Q5). The load restarts its game, which rejoins the
//!   host's session as the same player. Without a save it respawns by another player.
//!
//! nvforever's `coop` moves the messages: it sets [`CoopProgress::role`] and
//! [`CoopProgress::everyone_down`], puts what it heard in `inbox` and sends `outbox`.

use crate::save::{LoadRequest, SaveConfig, SaveDone, SaveRequest};
use crate::script::ScriptState;
use bevy::prelude::*;
use fnv_save::SaveKind;
use fnv_sim::FormId;
use std::path::{Path, PathBuf};

/// An autosave or system save this soon (seconds) after the last shared save is not
/// announced.
pub const AUTO_QUIET: f64 = 20.0;
/// Everyone must stay down this long before the host reloads (a revive in flight).
pub const DOWN_HOLD: f64 = 3.0;
/// The host loads this long after telling the guests to.
pub const RELOAD_DELAY: f64 = 0.5;
/// A guest down with nobody near loads its last save after this long (seconds).
pub const ALONE_HOLD: f64 = 5.0;
/// The last shared save's path, kept across the restart a load makes.
pub const SHARED_ENV: &str = "NVF_COOP_SHARED_SAVE";
/// Set on a guest that loads for everyone being down: its new run goes beside the host
/// (a guest's save is its own game where it left it; nvforever `coop::rejoin`).
pub const REJOIN_ENV: &str = "NVF_COOP_REJOIN_HOST";
/// Set for the run a load starts when that run must hold the syncing screen until it has
/// the host's world (queue item 186: a first join, or a save loaded after going down;
/// nvforever `coop::syncing`). Not set for ordinary loads.
pub const SYNC_ENV: &str = "NVF_COOP_SYNC_SCREEN";

/// This game's part in a co-op session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Role {
    #[default]
    Solo,
    Host,
    Guest,
}

/// Shared progression between the games (nvf-coop `net::Progress`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shared {
    Reward { quest: u32, xp: u32, caps: u32 },
    Save { stamp: u32, kind: SaveKind },
    Reload { stamp: u32 },
    /// A hand-out of the host's scripts (`fnv_game::coop_rewards`, items 112 and 114):
    /// `quest` 0 and `stage` `u32::MAX` for none (`nvf_coop::net::Progress::Gift`).
    Gift { batch: u32, item: u32, count: u32, quest: u32, stage: u32 },
    /// The host's quest completed or failed (`nvf_coop::net::Progress::QuestEnded`, item 205).
    QuestEnded { quest: u32, failed: bool },
    /// Joint progression (`fnv_game::coop_joint`): a guest's join choice and character.
    JointChoice { character: u64, on: bool },
    /// The host set a claimable stage, for the players whose bit is set in `to`.
    Stage { quest: u32, stage: i32, line: u32, line_end: bool, prior: Vec<i32>, to: u8 },
    StageResult { quest: u32, stage: i32, taken: bool },
    /// Kill credit for player `to` (shared-progression plan §4.2).
    KillCredit { to: u8, target: u32, target_base: u32, npc: bool, xp: u32, karma: i32, killer: bool },
    HitCredit { to: u8, target: u32, weapon: u32, part: u32, dealt: u32, critical: bool, crippled: bool },
}

/// The mailbox between this module and nvforever's `coop`.
#[derive(Resource, Debug, Default)]
pub struct CoopProgress {
    pub role: Role,
    /// Every player is down now (the host's view; module notes).
    pub everyone_down: bool,
    /// A guest: this player is down with no other player up near it (module notes).
    pub alone_down: bool,
    /// Heard from player `id`.
    pub inbox: Vec<(u8, Shared)>,
    /// To send.
    pub outbox: Vec<Shared>,
    pub saves: SharedSaves,
    /// A guest: the hand-out batch being taken, and whether it had Caravan cards before it.
    pub gift_batch: Option<(u32, bool)>,
    /// A guest: hand-outs taken, waiting for their corner lines ([`HandOuts`]).
    pub hand_outs: HandOuts,
    /// This player's id in the session (nvforever `coop` sets it).
    pub my_id: Option<u8>,
    /// A guest: its join choice for joint progression (the join popup, plan §3.7);
    /// `None` until chosen. Sent to the host once with the character's ID.
    pub joint_on: Option<bool>,
    pub joint_sent: bool,
    /// The host: each guest's character and choice ([`Shared::JointChoice`]), by id.
    pub joint_peers: std::collections::BTreeMap<u8, (u64, bool)>,
    /// The host: each other player's actor in this world, by id (nvforever `coop`).
    pub peer_actors: std::collections::BTreeMap<u8, FormId>,
}

/// A guest's hand-outs taken and not yet told: their lines ([`gift_lines`]) wait until
/// no hand-out has come for [`HandOuts::HOLD`], so a stack the host's script adds a piece
/// at a time is one line even when the pieces arrive over several frames (b21 duo: Doc
/// Mitchell's 22 single 9mm rounds came in two frames 5 ms apart, "9mm Round (7)" then
/// "9mm Round (15)") [design].
#[derive(Debug, Default)]
pub struct HandOuts {
    pending: Vec<(u32, String, i32)>,
    last: Option<std::time::Instant>,
}

impl HandOuts {
    /// How long the lines wait for more of the same hand-out.
    pub const HOLD: std::time::Duration = std::time::Duration::from_millis(500);

    pub fn add(&mut self, now: std::time::Instant, item: u32, name: String, n: i32) {
        self.pending.push((item, name, n));
        self.last = Some(now);
    }

    /// The lines to show now: all of them once [`Self::HOLD`] has passed since the last.
    pub fn due(&mut self, host: &str, now: std::time::Instant) -> Vec<String> {
        match self.last {
            Some(t) if now.duration_since(t) >= Self::HOLD => {
                self.last = None;
                gift_lines(host, &std::mem::take(&mut self.pending))
            }
            _ => Vec::new(),
        }
    }
}

/// What a game does about its own save just written ([`SharedSaves::saved`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Announce {
    /// Tell the others to save too.
    Tell,
    /// One another player asked for: already shared.
    Followed,
    /// An autosave right after a shared save: not told.
    Quiet,
}

/// What the host's all-down watch asks for ([`SharedSaves::down`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownStep {
    Wait,
    /// Tell the guests to load their last shared save.
    TellReload,
    /// Load this game's last shared save.
    Load(PathBuf),
    /// Everyone is down and there is no shared save to go back to.
    NoSave,
}

/// The shared saves' bookkeeping (module notes); pure, so it is tested alone.
#[derive(Debug, Clone, Default)]
pub struct SharedSaves {
    /// Saves another player asked for, still to be asked of the save system.
    pub waiting: Vec<SaveKind>,
    /// Saves asked for because another player saved: not announced when written.
    following: Vec<PathBuf>,
    /// The last shared save, and when it was written (seconds).
    pub last: Option<PathBuf>,
    last_at: Option<f64>,
    /// The host's all-down watch: since when everyone is down, and when the reload was told.
    down_since: Option<f64>,
    told_at: Option<f64>,
    loaded: bool,
    told_no_save: bool,
    /// A guest's watch while down alone: since when, and whether it loaded or said so.
    alone_since: Option<f64>,
    alone_done: bool,
}

impl SharedSaves {
    /// Another player saved `kind`: this game saves it too (once, however often heard
    /// before it could).
    pub fn heard_save(&mut self, kind: SaveKind) {
        if !self.waiting.contains(&kind) {
            self.waiting.push(kind);
        }
    }

    /// A followed save was asked for, to be written at `path`.
    pub fn following(&mut self, path: PathBuf) {
        self.following.push(path);
    }

    /// This game wrote the save at `path` (`auto`: an autosave or system save) at `now`.
    pub fn saved(&mut self, path: &Path, auto: bool, now: f64) -> Announce {
        let what = if let Some(i) = self.following.iter().position(|p| p == path) {
            self.following.remove(i);
            Announce::Followed
        } else if auto && self.last_at.is_some_and(|t| now - t < AUTO_QUIET) {
            Announce::Quiet
        } else {
            Announce::Tell
        };
        if what != Announce::Quiet {
            self.last = Some(path.to_path_buf());
            self.last_at = Some(now);
        }
        what
    }

    /// The host's watch, each frame: `all_down` now, at `now` (seconds).
    pub fn down(&mut self, all_down: bool, now: f64) -> DownStep {
        if !all_down {
            self.down_since = None;
            self.told_at = None;
            self.loaded = false;
            self.told_no_save = false;
            return DownStep::Wait;
        }
        let since = *self.down_since.get_or_insert(now);
        if self.loaded || now - since < DOWN_HOLD {
            return DownStep::Wait;
        }
        let Some(path) = self.last.clone() else {
            if self.told_no_save {
                return DownStep::Wait;
            }
            self.told_no_save = true;
            return DownStep::NoSave;
        };
        match self.told_at {
            None => {
                self.told_at = Some(now);
                DownStep::TellReload
            }
            Some(t) if now - t >= RELOAD_DELAY => {
                self.loaded = true;
                DownStep::Load(path)
            }
            Some(_) => DownStep::Wait,
        }
    }
}

impl SharedSaves {
    /// A guest's watch, each frame: down with nobody near (`alone`) at `now`.
    pub fn alone(&mut self, alone: bool, now: f64) -> DownStep {
        if !alone {
            self.alone_since = None;
            self.alone_done = false;
            return DownStep::Wait;
        }
        let since = *self.alone_since.get_or_insert(now);
        if self.alone_done || now - since < ALONE_HOLD {
            return DownStep::Wait;
        }
        self.alone_done = true;
        match self.last.clone() {
            Some(path) => DownStep::Load(path),
            None => DownStep::NoSave,
        }
    }
}

/// Every player is down: this one, and each other one down or lost (nobody left to
/// revive); `others` is (down, lost) per other player, and there must be one.
pub fn everyone_down(me_down: bool, others: impl IntoIterator<Item = (bool, bool)>) -> bool {
    let mut any = false;
    for (down, lost) in others {
        any = true;
        if !down && !lost {
            return false;
        }
    }
    me_down && any
}

/// The save request a followed save of `kind` makes (a manual save gets a new name).
fn follow_request(kind: SaveKind, cfg: &SaveConfig) -> SaveRequest {
    let name = match kind {
        SaveKind::Quick => fnv_save::QUICK_NAME.to_string(),
        SaveKind::Auto => fnv_save::AUTO_NAME.to_string(),
        SaveKind::System => fnv_save::SYSTEM_NAME.to_string(),
        SaveKind::Manual => crate::pause::new_save_name(cfg),
    };
    SaveRequest { name, kind }
}

fn stamp() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u32)
}

pub struct CoopProgressPlugin;

impl Plugin for CoopProgressPlugin {
    fn build(&self, app: &mut App) {
        let mut p = CoopProgress::default();
        p.saves.last = std::env::var_os(SHARED_ENV).map(PathBuf::from).filter(|p| p.exists());
        app.insert_resource(p).add_systems(Update, (take_inbox, rewards, announce_saves, all_down, alone_down).chain().in_set(crate::PlaySet::Ui)).add_systems(Update, follow_saves.after(crate::PlaySet::Ui));
    }
}

/// The name a hand-out's line shows: the item's FULL, as the player sees it elsewhere;
/// its Editor ID only when it has none (b19: "Host handed out WeapNV9mmPistol").
fn item_label(full_names: &std::collections::HashMap<FormId, String>, item: FormId, edid: impl FnOnce() -> String) -> String {
    full_names.get(&item).filter(|n| !n.is_empty()).cloned().unwrap_or_else(edid)
}

fn say(corner: &mut Option<ResMut<crate::corner_messages::CornerMessages>>, n: String) {
    info!("co-op: {n}");
    if let Some(c) = corner.as_mut() {
        c.push(n.into());
    }
}

/// What the other players sent: rewards taken, saves to follow, the reload.
fn take_inbox(
    mut p: ResMut<CoopProgress>,
    script: Option<ResMut<ScriptState>>,
    mut corner: Option<ResMut<crate::corner_messages::CornerMessages>>,
    mut load: MessageWriter<LoadRequest>,
    mates: Option<Res<crate::coop_social::SocialMates>>,
    mut quest_lines: Option<ResMut<crate::quest_messages::QuestMessages>>,
    mut sim: Option<ResMut<crate::sim::SimState>>,
    solo: Option<Res<crate::save::SoloBase>>,
) {
    let p = &mut *p;
    let mut script = script;
    let now = std::time::Instant::now();
    for (id, what) in std::mem::take(&mut p.inbox) {
        match what {
            Shared::Reward { quest, xp, caps } if p.role == Role::Guest && id == 0 => {
                let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else {
                    warn!("co-op: a quest reward came before the scripts run: {xp} XP, {caps} caps lost");
                    continue;
                };
                let name = rt.game.data.name(FormId(quest));
                let host = mates.as_deref().and_then(|m| m.get(0)).map_or("The host", |m| m.name.as_str()).to_string();
                // Completed in this player's own game: its reward was this player's then
                // (`fnv_game::coop_rewards::REPEAT_DONE_REWARDS`).
                if rt.game.coop_reward_done_here(FormId(quest)) {
                    info!("co-op: the host completed {name} ({xp} XP, {caps} caps): this player completed it in its own game");
                    continue;
                }
                // Into the played world (`crate::script::with_world`): outside it the
                // runtime holds a stand-in, whose caps no menu shows (a guest's +50 caps
                // never reached its Pip-Boy, 2026-11-14).
                let taken = match sim.as_deref_mut() {
                    Some(sim) => crate::script::with_world(rt, sim, |rt| rt.game.take_coop_reward(xp as f64, caps as i32)),
                    None => Err("no simulation world".to_string()),
                };
                match taken {
                    // Script XP no completion claimed (`fnv_game::coop_rewards::SCRIPT_XP`).
                    Ok(gained) if FormId(quest) == fnv_game::coop_rewards::SCRIPT_XP => say(&mut corner, format!("+{gained} XP, earned with {host}")),
                    // The host by its character's name (`coop_social::SocialMates`).
                    // The quest's COMPLETED line came with its own message ([`Shared::QuestEnded`]).
                    Ok(gained) => say(&mut corner, format!("{host} completed {name}: +{gained} XP{}", if caps > 0 { format!(", +{caps} caps") } else { String::new() })),
                    Err(e) => warn!("co-op: the reward for {name} could not be given: {e}"),
                }
            }
            Shared::Gift { batch, item, count, quest, stage } if p.role == Role::Guest && id == 0 => {
                let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else {
                    warn!("co-op: a hand-out came before the scripts run: {count} of {} lost", FormId(item));
                    continue;
                };
                let name = item_label(&rt.game.data.full_names, FormId(item), || rt.game.data.name(FormId(item)));
                // Caravan cards (Ringo's deck: 54 of them) join the player's cards; one
                // corner line for the first, not one per card.
                let card = rt.game.data.caravan_cards.contains_key(&FormId(item));
                let first_card = card && rt.game.player.caravan.cards.is_empty();
                let gift = fnv_game::coop_rewards::Gift { batch, item: FormId(item), count: count as i32, quest: (quest != 0).then_some(FormId(quest)), stage: (stage != u32::MAX).then_some(stage as i32) };
                let done_here = rt.game.coop_gift_done_here(&gift);
                // Into the played world, as the reward above.
                let taken = match sim.as_deref_mut() {
                    Some(sim) => crate::script::with_world(rt, sim, |rt| rt.game.take_coop_gift(&gift, &mut p.gift_batch)),
                    None => Err("no simulation world".to_string()),
                };
                match taken {
                    Ok(0) if done_here => info!("co-op: the host's hand-out {name} ({count}): this player did {} in its own game", rt.game.data.name(FormId(quest))),
                    Ok(0) => info!("co-op: the host's hand-out {name} ({count}): this player has its own"),
                    Ok(_) if card && !first_card => {}
                    Ok(_) if card => say(&mut corner, format!("{} handed out Caravan cards", mates.as_deref().and_then(|m| m.get(0)).map_or("The host's quest", |m| m.name.as_str()))),
                    Ok(n) => {
                        debug!("co-op: the host's hand-out {name}: {n} taken (batch {batch})");
                        p.hand_outs.add(now, item, name, n);
                    }
                    Err(e) => warn!("co-op: the hand-out {name} could not be given: {e}"),
                }
            }
            // The host's quest completed or failed: listed as done, and its line, in the
            // order the host's quests ended (item 205; sweep-duo-b2: the host showed 10
            // failed quests, the guest none; duo b4: VMS39, completed and failed a second
            // apart, showed neither). Item 180: Come Fly With Me, not the host's active
            // quest, showed no COMPLETED line on the guest before such lines were sent.
            Shared::QuestEnded { quest, failed } if p.role == Role::Guest && id == 0 => {
                let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else {
                    warn!("co-op: the host's quest {} ended before the scripts run", FormId(quest));
                    continue;
                };
                let name = rt.game.data.name(FormId(quest));
                let how = if failed { "failed" } else { "completed" };
                if !rt.game.coop_host_quest_ended(FormId(quest), failed) {
                    info!("co-op: the host {how} {name}: told already");
                    continue;
                }
                let quest_name = rt.game.data.quests.get(&FormId(quest)).and_then(|q| q.name.clone()).filter(|n| !n.trim().is_empty());
                match (quest_lines.as_deref_mut(), quest_name) {
                    (Some(lines), Some(qn)) => {
                        let n = if failed { crate::script_events::QuestNotice::Failed(qn) } else { crate::script_events::QuestNotice::Completed(qn) };
                        if !lines.holds(&n) {
                            info!("co-op: the host's quest: {}", crate::quest_messages::describe(&n));
                            lines.push(n);
                        }
                    }
                    // A quest the Pip-Boy does not list (no name) shows no line in the
                    // host's game either (docs/spec-notes/quest-messages.md §5).
                    _ => info!("co-op: the host {how} {name}: no quest line"),
                }
            }
            Shared::Save { kind, .. } if p.role != Role::Solo => {
                info!("co-op: player {} saved ({kind:?}): this game saves too", id + 1);
                p.saves.heard_save(kind);
            }
            Shared::Reload { .. } if p.role == Role::Guest && id == 0 => match p.saves.last.clone() {
                Some(path) => {
                    say(&mut corner, "Everyone is down: back to the last shared save".into());
                    // The save is this game's own spot: the new run goes beside the host
                    // (nvforever `coop::rejoin`; item 152).
                    std::env::set_var(REJOIN_ENV, "1");
                    std::env::set_var(SYNC_ENV, "1");
                    load.write(LoadRequest { path });
                }
                None => say(&mut corner, "Everyone is down, and this game has no shared save to go back to".into()),
            },
            // Joint progression (`fnv_game::coop_joint`; plan §3).
            Shared::JointChoice { character, on } if p.role == Role::Host && id != 0 => {
                info!("co-op: player {} shares quest progress: {} (character {character:016x})", id + 1, if on { "yes" } else { "no" });
                p.joint_peers.insert(id, (character, on));
            }
            Shared::StageResult { quest, stage, taken } if p.role == Role::Host => {
                info!("co-op: player {} {} {} stage {stage}", id + 1, if taken { "claimed" } else { "turned down" }, FormId(quest));
            }
            Shared::Stage { quest, stage, line, line_end, prior, to } if p.role == Role::Guest && id == 0 => {
                if !p.my_id.is_some_and(|m| m < 8 && to & (1 << m) != 0) || p.joint_on != Some(true) {
                    continue;
                }
                let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { continue };
                // The guest's own game as it was at join (`save::SoloBase`): its quests decide.
                let Some(own) = solo.as_deref().and_then(|s| s.0.as_deref()).map(|s| &s.game.quests) else {
                    warn!("co-op: the host set {} stage {stage}: this game's own save is not kept, so nothing is claimed", FormId(quest));
                    continue;
                };
                let q = FormId(quest);
                let offer = fnv_game::coop_joint::StageOffer { quest: q, stage, prior, line: (line != 0).then_some((FormId(line), line_end)), to: Vec::new() };
                let name = rt.game.data.quests.get(&q).and_then(|d| d.name.clone()).filter(|n| !n.trim().is_empty());
                let taken = rt.game.take_joint_offer(own, &offer);
                match &taken {
                    Ok(()) => {
                        info!("co-op: claimed {} stage {stage} for this character's own game", rt.game.data.name(q));
                        let first = rt.game.player.claims.stages.get(&q).is_some_and(|s| s.len() == 1);
                        if let (true, Some(n)) = (first, name) {
                            say(&mut corner, format!("Shared quest progress: {n}"));
                        }
                    }
                    Err(why) => info!("co-op: the host set {} stage {stage}: not claimed ({why})", rt.game.data.name(q)),
                }
                p.outbox.push(Shared::StageResult { quest, stage, taken: taken.is_ok() });
            }
            Shared::KillCredit { to, target, target_base, npc, xp, karma, killer } if p.role == Role::Guest && id == 0 && p.my_id == Some(to) => {
                let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { continue };
                let c = fnv_game::coop_kills::KillCredit {
                    player: FormId::PLAYER_REF,
                    target: FormId(target),
                    target_base: (target_base != 0).then_some(FormId(target_base)),
                    npc,
                    xp: f64::from(xp),
                    karma: f64::from(karma),
                    killer,
                };
                let got = rt.game.take_coop_kill_credit(&c);
                info!("co-op: kill credit for {}: +{got} XP, karma {karma:+}{}", FormId(target), if killer { " (the kill)" } else { "" });
                if got > 0.0 {
                    say(&mut corner, format!("+{got} XP"));
                }
            }
            Shared::HitCredit { to, target, weapon, part, dealt, critical, crippled } if p.role == Role::Guest && id == 0 && p.my_id == Some(to) => {
                let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { continue };
                let h = fnv_game::coop_kills::HitCredit {
                    player: FormId::PLAYER_REF,
                    target: FormId(target),
                    weapon: (weapon != 0).then_some(FormId(weapon)),
                    part: (part != u32::MAX).then_some(part),
                    dealt: f64::from(dealt),
                    critical,
                    crippled,
                };
                rt.game.take_coop_hit_credit(&h);
            }
            other => debug!("co-op: dropped {other:?} from player {}", id + 1),
        }
    }
    let host = mates.as_deref().and_then(|m| m.get(0)).map_or("The host's quest", |m| m.name.as_str());
    for line in p.hand_outs.due(host, now) {
        say(&mut corner, line);
    }
}

/// The corner lines of hand-outs taken together (item, name, count given; [`HandOuts`]):
/// one per item, its counts added up, in the order the items first came. A script that adds a
/// stack a round at a time (Doc Mitchell's 9mm rounds: 25 AddItem calls of 1, b20 duo)
/// was one uncounted "9mm Round" line. The count in brackets after the name, as this
/// game's own item lines have it (`script_events`) [design: the original's wording for a
/// counted hand-out is not in the data files; observer-checks.md].
fn gift_lines(host: &str, handed: &[(u32, String, i32)]) -> Vec<String> {
    let mut order: Vec<(u32, &str, i32)> = Vec::new();
    for (item, name, n) in handed {
        match order.iter_mut().find(|(i, _, _)| i == item) {
            Some(o) => o.2 += n,
            None => order.push((*item, name, *n)),
        }
    }
    order.into_iter().map(|(_, name, n)| if n > 1 { format!("{host} handed out {name} ({n})") } else { format!("{host} handed out {name}") }).collect()
}

/// The host: rewards of the quests it completed, for the guests.
fn rewards(mut p: ResMut<CoopProgress>, script: Option<ResMut<ScriptState>>) {
    let Some(mut st) = script else { return };
    let Some(rt) = st.rt.as_mut() else { return };
    let host = p.role == Role::Host;
    if rt.game.coop_rewards.on != host {
        rt.game.coop_rewards = fnv_game::coop_rewards::RewardLedger::default();
        rt.game.coop_rewards.on = host;
    }
    // Joint progression (`fnv_game::coop_joint`): a guest tells the host its choice and
    // character once; the host keeps the game's guest list current and sends its offers.
    if p.role != Role::Guest {
        p.joint_sent = false;
    }
    if p.role != Role::Host {
        p.joint_peers.clear();
    }
    // The join popup (plan §3.7) is not built yet: until it is, the choice is yes unless
    // NVF_COOP_JOINT=0 [design, interim].
    if p.role == Role::Guest && p.joint_on.is_none() {
        p.joint_on = Some(std::env::var("NVF_COOP_JOINT").map_or(true, |v| v != "0"));
    }
    if p.role == Role::Guest && !p.joint_sent && p.my_id.is_some() {
        if let Some(on) = p.joint_on {
            let character = rt.game.ensure_character_id();
            p.outbox.push(Shared::JointChoice { character, on });
            p.joint_sent = true;
        }
    }
    if !host {
        return;
    }
    rt.game.coop_joint_guests = p
        .joint_peers
        .iter()
        .filter_map(|(id, &(character, on))| p.peer_actors.get(id).map(|a| (*a, fnv_game::coop_joint::JointGuest { character, on })))
        .collect();
    for o in rt.game.take_joint_offers() {
        let to = o.to.iter().filter_map(|a| p.peer_actors.iter().find(|(_, x)| *x == a).map(|(id, _)| *id)).filter(|id| *id < 8).fold(0u8, |m, id| m | (1 << id));
        info!("co-op: {} stage {}: offered to the players sharing quest progress", rt.game.data.name(o.quest), o.stage);
        let (line, line_end) = o.line.map_or((0, false), |(i, e)| (i.0, e));
        p.outbox.push(Shared::Stage { quest: o.quest.0, stage: o.stage, line, line_end, prior: o.prior, to });
    }
    // Kill and hit credit for the guests (`fnv_game::coop_kills`, plan §4.2).
    let id_of = |a: FormId| p.peer_actors.iter().find(|(_, x)| **x == a).map(|(id, _)| *id);
    let mut credits = Vec::new();
    for c in rt.game.take_coop_kill_credits() {
        let Some(to) = id_of(c.player) else { continue };
        credits.push(Shared::KillCredit { to, target: c.target.0, target_base: c.target_base.map_or(0, |b| b.0), npc: c.npc, xp: c.xp.max(0.0).round() as u32, karma: c.karma.round() as i32, killer: c.killer });
    }
    for h in rt.game.take_coop_hit_credits() {
        let Some(to) = id_of(h.player) else { continue };
        credits.push(Shared::HitCredit { to, target: h.target.0, weapon: h.weapon.map_or(0, |w| w.0), part: h.part.unwrap_or(u32::MAX), dealt: h.dealt.max(0.0).round() as u32, critical: h.critical, crippled: h.crippled });
    }
    p.outbox.extend(credits);
    for g in std::mem::take(&mut rt.game.coop_rewards.gifts) {
        let of = match (g.quest, g.stage) {
            (Some(q), Some(s)) => format!(" by {} stage {s}", rt.game.data.name(q)),
            (Some(q), None) => format!(" by {}", rt.game.data.name(q)),
            _ => String::new(),
        };
        info!("co-op: a script handed the player {} ({}){of}: to the guests too", rt.game.data.name(g.item), g.count);
        p.outbox.push(Shared::Gift { batch: g.batch, item: g.item.0, count: g.count.max(0) as u32, quest: g.quest.map_or(0, |q| q.0), stage: g.stage.map_or(u32::MAX, |s| s as u32) });
    }
    for (q, failed) in std::mem::take(&mut rt.game.coop_rewards.ended) {
        info!("co-op: {} {}: to the guests", rt.game.data.name(q), if failed { "failed" } else { "completed" });
        p.outbox.push(Shared::QuestEnded { quest: q.0, failed });
    }
    for r in rt.game.coop_rewards.due(rt.game.frame) {
        if r.quest == fnv_game::coop_rewards::SCRIPT_XP {
            info!("co-op: {} XP from the host's scripts, no quest completed: to the guests", r.xp);
        } else {
            info!("co-op: {} completed: {} XP and {} caps to the guests", rt.game.data.name(r.quest), r.xp, r.caps);
        }
        p.outbox.push(Shared::Reward { quest: r.quest.0, xp: r.xp.max(0.0) as u32, caps: r.caps.max(0) as u32 });
    }
}

/// This game's saves: announced to the others unless followed or quiet.
fn announce_saves(mut p: ResMut<CoopProgress>, mut done: MessageReader<SaveDone>, cfg: Res<SaveConfig>, time: Res<Time>) {
    let now = time.elapsed_secs_f64();
    for d in done.read() {
        if d.result.is_err() || p.role == Role::Solo {
            continue;
        }
        let slot = |n: &str| cfg.path_of(n).ok();
        let kind = if Some(&d.path) == slot(fnv_save::QUICK_NAME).as_ref() {
            SaveKind::Quick
        } else if Some(&d.path) == slot(fnv_save::AUTO_NAME).as_ref() {
            SaveKind::Auto
        } else if Some(&d.path) == slot(fnv_save::SYSTEM_NAME).as_ref() {
            SaveKind::System
        } else {
            SaveKind::Manual
        };
        let what = p.saves.saved(&d.path, matches!(kind, SaveKind::Auto | SaveKind::System), now);
        if what != Announce::Quiet {
            std::env::set_var(SHARED_ENV, &d.path);
        }
        if what == Announce::Tell {
            let stamp = stamp();
            info!("co-op: saved {} ({kind:?}): every game saves too (shared save {stamp:08x})", d.path.display());
            p.outbox.push(Shared::Save { stamp, kind });
        }
    }
}

/// Saves another player made, written here as soon as saving is allowed.
fn follow_saves(world: &mut World) {
    if world.get_resource::<CoopProgress>().is_none_or(|p| p.saves.waiting.is_empty()) || world.contains_resource::<crate::save::PendingEngine>() {
        return;
    }
    if crate::save::not_now(world).is_some() {
        return;
    }
    let cfg = world.resource::<SaveConfig>().clone();
    let kinds = std::mem::take(&mut world.resource_mut::<CoopProgress>().saves.waiting);
    for kind in kinds {
        let r = follow_request(kind, &cfg);
        match cfg.path_of(&r.name) {
            Ok(path) => {
                info!("save: a co-op save: {:?} because another player saved", r.name);
                world.resource_mut::<CoopProgress>().saves.following(path);
                world.write_message(r);
            }
            Err(e) => warn!("save: a co-op save cannot be made: {e}"),
        }
    }
}

/// The host: everyone down for [`DOWN_HOLD`]: every game back to its last shared save.
fn all_down(mut p: ResMut<CoopProgress>, time: Res<Time>, mut corner: Option<ResMut<crate::corner_messages::CornerMessages>>, mut load: MessageWriter<LoadRequest>) {
    let p = &mut *p;
    if p.role != Role::Host {
        return;
    }
    match p.saves.down(p.everyone_down, time.elapsed_secs_f64()) {
        DownStep::Wait => {}
        DownStep::TellReload => {
            say(&mut corner, "Everyone is down: back to the last shared save".into());
            p.outbox.push(Shared::Reload { stamp: stamp() });
        }
        DownStep::Load(path) => {
            info!("co-op: everyone down: loading the last shared save {}", path.display());
            load.write(LoadRequest { path });
        }
        DownStep::NoSave => say(&mut corner, "Everyone is down, and there is no shared save to go back to".into()),
    }
}

/// A guest down with nobody near: its own last save (module notes).
fn alone_down(mut p: ResMut<CoopProgress>, time: Res<Time>, mut corner: Option<ResMut<crate::corner_messages::CornerMessages>>, mut load: MessageWriter<LoadRequest>) {
    let p = &mut *p;
    let alone = p.role == Role::Guest && p.alone_down;
    match p.saves.alone(alone, time.elapsed_secs_f64()) {
        DownStep::Load(path) => {
            say(&mut corner, "Down with nobody near: back to your last save".into());
            info!("co-op: down far from the other players: loading this game's last save {}", path.display());
            std::env::set_var(SYNC_ENV, "1");
            load.write(LoadRequest { path });
        }
        DownStep::NoSave => say(&mut corner, "Down with nobody near, and no save to go back to: respawn by another player".into()),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn one_line_per_item_handed_out_with_its_count() {
        let mut handed = vec![(0x15, "Stimpak".to_string(), 4), (0x0A, "Bobby Pin".to_string(), 6), (0xE3778, "9mm Pistol".to_string(), 1)];
        // Doc Mitchell's 9mm rounds: one AddItem of 1 each, 25 times.
        handed.extend(std::iter::repeat_n((0x4241, "9mm Round".to_string(), 1), 25));
        assert_eq!(
            gift_lines("Host", &handed),
            ["Host handed out Stimpak (4)", "Host handed out Bobby Pin (6)", "Host handed out 9mm Pistol", "Host handed out 9mm Round (25)"]
        );
        // An item that comes again later in the same inbox joins its first line.
        let again = [(1, "A".to_string(), 1), (2, "B".to_string(), 2), (1, "A".to_string(), 2)];
        assert_eq!(gift_lines("Host", &again), ["Host handed out A (3)", "Host handed out B (2)"]);
        assert!(gift_lines("Host", &[]).is_empty());
    }

    #[test]
    fn a_stack_over_two_frames_is_one_line() {
        let t0 = std::time::Instant::now();
        let ms = |n: u64| t0 + std::time::Duration::from_millis(n);
        let mut h = HandOuts::default();
        h.add(t0, 0xE3778, "9mm Pistol".into(), 1);
        for _ in 0..7 {
            h.add(t0, 0x4241, "9mm Round".into(), 1);
        }
        // The next frame: still waiting for more.
        assert!(h.due("Host", ms(5)).is_empty());
        for _ in 0..15 {
            h.add(ms(5), 0x4241, "9mm Round".into(), 1);
        }
        assert!(h.due("Host", ms(400)).is_empty(), "under HOLD since the last piece");
        assert_eq!(h.due("Host", ms(505)), ["Host handed out 9mm Pistol", "Host handed out 9mm Round (22)"]);
        assert!(h.due("Host", ms(2000)).is_empty(), "told once");
    }

    #[test]
    fn hand_outs_show_the_items_name_not_its_editor_id() {
        let mut full = std::collections::HashMap::new();
        full.insert(FormId(0x000E3778), "9mm Pistol".to_string());
        full.insert(FormId(0x0000000F), String::new());
        assert_eq!(item_label(&full, FormId(0x000E3778), || "WeapNV9mmPistol".into()), "9mm Pistol");
        // No name to show: the Editor ID, as before.
        assert_eq!(item_label(&full, FormId(0x00001234), || "SomeItem".into()), "SomeItem");
        assert_eq!(item_label(&full, FormId(0x0000000F), || "Caps001".into()), "Caps001");
    }

    #[test]
    fn own_saves_are_told_followed_ones_are_not() {
        let mut s = SharedSaves::default();
        assert_eq!(s.saved(&path("a.nvs"), false, 0.0), Announce::Tell);
        assert_eq!(s.last, Some(path("a.nvs")));
        // Another player saved: this game follows, and does not tell again.
        s.heard_save(SaveKind::Quick);
        s.heard_save(SaveKind::Quick);
        assert_eq!(s.waiting, [SaveKind::Quick], "one save however often heard");
        s.following(path("quick.nvs"));
        assert_eq!(s.saved(&path("quick.nvs"), false, 100.0), Announce::Followed);
        assert_eq!(s.last, Some(path("quick.nvs")));
        // The same slot written again by this player's own hand: told.
        assert_eq!(s.saved(&path("quick.nvs"), false, 101.0), Announce::Tell);
    }

    #[test]
    fn autosaves_right_after_a_shared_save_are_quiet() {
        let mut s = SharedSaves::default();
        // The first autosave of a session is told.
        assert_eq!(s.saved(&path("auto.nvs"), true, 0.0), Announce::Tell);
        // The trip's autosave in each game: not another round.
        assert_eq!(s.saved(&path("auto.nvs"), true, 5.0), Announce::Quiet);
        // A manual save is always told.
        assert_eq!(s.saved(&path("Save 3.nvs"), false, 6.0), Announce::Tell);
        assert_eq!(s.last, Some(path("Save 3.nvs")));
        assert_eq!(s.saved(&path("auto.nvs"), true, 6.0 + AUTO_QUIET), Announce::Tell);
    }

    #[test]
    fn a_guest_down_alone_loads_its_last_save_once() {
        let mut s = SharedSaves { last: Some(path("q.nvs")), ..Default::default() };
        assert_eq!(s.alone(true, 0.0), DownStep::Wait);
        assert_eq!(s.alone(true, ALONE_HOLD - 0.1), DownStep::Wait);
        assert_eq!(s.alone(true, ALONE_HOLD), DownStep::Load(path("q.nvs")));
        assert_eq!(s.alone(true, ALONE_HOLD + 1.0), DownStep::Wait, "once");
        // Revived (or someone came near): the watch starts over.
        assert_eq!(s.alone(false, 20.0), DownStep::Wait);
        assert_eq!(s.alone(true, 21.0), DownStep::Wait);
        let mut n = SharedSaves::default();
        n.alone(true, 0.0);
        assert_eq!(n.alone(true, ALONE_HOLD), DownStep::NoSave);
    }

    #[test]
    fn everyone_down_needs_every_player() {
        assert!(everyone_down(true, [(true, false), (false, true)]), "a lost player cannot revive");
        assert!(!everyone_down(true, [(true, false), (false, false)]));
        assert!(!everyone_down(false, [(true, false)]));
        assert!(!everyone_down(true, []), "alone: not a co-op wipe");
    }

    #[test]
    fn all_down_tells_then_loads_once() {
        let mut s = SharedSaves::default();
        s.saved(&path("shared.nvs"), false, 0.0);
        assert_eq!(s.down(true, 10.0), DownStep::Wait);
        assert_eq!(s.down(true, 10.0 + DOWN_HOLD - 0.1), DownStep::Wait);
        // A revive in time: the watch starts again.
        assert_eq!(s.down(false, 12.0), DownStep::Wait);
        assert_eq!(s.down(true, 20.0), DownStep::Wait);
        assert_eq!(s.down(true, 20.0 + DOWN_HOLD), DownStep::TellReload);
        assert_eq!(s.down(true, 20.0 + DOWN_HOLD + 0.1), DownStep::Wait, "the message goes out first");
        assert_eq!(s.down(true, 20.0 + DOWN_HOLD + RELOAD_DELAY), DownStep::Load(path("shared.nvs")));
        assert_eq!(s.down(true, 30.0), DownStep::Wait, "once");
    }

    #[test]
    fn all_down_without_a_shared_save_says_so_once() {
        let mut s = SharedSaves::default();
        s.down(true, 0.0);
        assert_eq!(s.down(true, DOWN_HOLD), DownStep::NoSave);
        assert_eq!(s.down(true, DOWN_HOLD + 1.0), DownStep::Wait);
    }

    #[test]
    fn a_followed_manual_save_gets_a_new_name() {
        let cfg = SaveConfig { dir: None, autosave_travel: true, autosave_fast_travel: true, autosave_wait: true, autosave_sleep: true, autosave_scripts: true };
        assert_eq!(follow_request(SaveKind::Quick, &cfg).name, fnv_save::QUICK_NAME);
        assert_eq!(follow_request(SaveKind::Auto, &cfg).kind, SaveKind::Auto);
        assert_eq!(follow_request(SaveKind::Manual, &cfg).kind, SaveKind::Manual);
    }
}
