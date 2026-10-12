//! Actors who talk on their own (docs/spec-notes/dialogue.md §5.2): an actor says hello
//! to the player as the player passes, and an actor goes up to another near it and starts
//! a random conversation. What is said is the game's (`Game::say_hello`,
//! `Game::start_actor_conversation`); when, and who, is decided here.
//!
//! Who may: the actor's current package allows it. A package with the general flag
//! "Skip Fallout Behavior" (bit 12) allows only the Fallout behaviours it sets ("Hellos to
//! player", "Random conversations"); a package without it leaves the actor the default,
//! where both are allowed [documented: Category:Packages, "Enable Fallout Behavior": the
//! two behaviours; observed: in FalloutNV.esm the behaviour flags of the 2,478 packages
//! without bit 12 are 0 or always 0x1CD, those of the 1,685 with it vary; owner statement
//! 2026-10-11: Vault 3's Fiends talk a lot, and their packages
//! (DefaultSandboxEditorLocation512, DefaultPatrolCasual) have bit 12 clear and no
//! behaviour flag set; hypothesis: what bit 12 means]. Only Sandbox, Guard, Eat and
//! Patrol packages start random conversations [documented: Category:Packages]; a
//! Sandbox or Wander package with "No Conversation" takes part in none [documented:
//! xEdit, the flag's name]. Nobody talks while asleep (a Sleep package), on its way,
//! talking already, or carrying out a Dialogue package [hypothesis].
//!
//! When:
//! - **Hello**: when the player comes within [`HELLO_DISTANCE`] of the actor; again only
//!   after the player has gone beyond [`HELLO_REARM`] times that distance. One hello is
//!   said at a time [stand-ins: the game's distance and repeat rules are not in the
//!   documentation or the masters].
//! - **Random conversation**: every actor that may start one looks for a partner after
//!   a time drawn between `fAISocialTimerForConversationsMin` and `…Max` seconds (30 and
//!   60 in FalloutNV.esm), and does so with a chance of `fAISocialchanceForConversation`
//!   per cent (`…Interior` indoors; 25 both) [observed: the settings' values in
//!   FalloutNV.esm; hypothesis: what they mean, from their names]. The partner is the
//!   nearest actor that may take part, for whom the starter has a HELLO line (Link From
//!   blank) and to whom it has a way, within `fAISocialRadiusToTriggerConversationInterior`
//!   (1000) indoors and `fAISocialRadiusToTriggerConversation` outdoors (not in the
//!   masters: the indoor value stands in).
//!
//! **The walk** ([`Chat`]): the starter goes up to the partner, who waits where it stands
//! and turns to it, and they talk once the starter is within [`TALK_DISTANCE`] (or as
//! near as the navmesh lets it get) [documented: Sandbox Package, "Conversation -- no
//! duration, the behavior is done when the conversation is finished"; StartConversation:
//! "the calling actor will seek out the target actor"; the distance: the standard
//! Activate Distance of a Dialogue package, iActivatePickLength 150 (Dialogue Package);
//! hypothesis: the same for random conversations, and that the partner waits]. A walk
//! that takes longer than [`CHAT_WALK_SECONDS`], a partner that goes, dies or starts
//! talking to someone else, or a package that changes gives the conversation up
//! [stand-in]. Meanwhile neither does what its package has it do; afterwards each goes
//! back to its package's place, a sandboxing actor going on from where it is
//! (`Ai::back_to_package`, as after a push) [hypothesis].
//!
//! While two actors talk, and while an actor says hello or a SayTo line, they turn to
//! whom they talk to (`Game::facings`, which says who turns; [hypothesis] for hellos and
//! conversations); the engine keeps whoever waits or talks where it stands
//! ([`Ai::holds_for_chat`]).

use crate::carry::{Activity, Ai, AiEvent, Talking};
use crate::goal::{self, dist};
use crate::sandbox::Rng;
use fnv_game::game::Game;
use fnv_game::package::{fallout, flags, type_flags, PackageDef, PackageType};
use fnv_nav::{NavGraph, Place, Space};
use fnv_sim::FormId;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// The player is greeted from this close, game units [stand-in].
pub const HELLO_DISTANCE: f32 = 300.0;
/// An actor that said hello greets again once the player has gone this many times
/// [`HELLO_DISTANCE`] away [stand-in].
pub const HELLO_REARM: f32 = 2.0;
/// Two actors start talking this close, game units [the standard Activate Distance of a
/// Dialogue package, iActivatePickLength (Dialogue Package); hypothesis for random
/// conversations].
pub const TALK_DISTANCE: f32 = 150.0;
/// A walk up to a partner that takes longer than this, seconds, is given up [stand-in].
pub const CHAT_WALK_SECONDS: f64 = 30.0;
/// The settings' values when the load order does not set them [the indoor values of
/// FalloutNV.esm].
const TIMER_MIN: f32 = 30.0;
const TIMER_MAX: f32 = 60.0;
const CHANCE: f32 = 25.0;
const RADIUS: f32 = 1000.0;

/// The packages that start random conversations [documented: Category:Packages].
const STARTERS: [PackageType; 4] = [PackageType::Sandbox, PackageType::Guard, PackageType::Eat, PackageType::Patrol];

/// Where an actor is with a random conversation ([`crate::Agent::chat`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "chat", rename_all = "snake_case")]
pub enum Chat {
    /// Walking up to `partner`, since `since` (`Game::time`).
    Going { partner: FormId, since: f64 },
    /// Waiting where it stands for `starter` to come up.
    Waiting { starter: FormId },
    /// In the conversation with `with` (said by the game, `Game::speech`).
    Talking { with: FormId },
}

pub(crate) struct Social {
    /// When (`Game::time`) each actor that may start a conversation next looks for a
    /// partner.
    next_chat: BTreeMap<FormId, f64>,
    /// Actors that said hello and wait for the player to go away.
    greeted: BTreeSet<FormId>,
    pub(crate) rng: Rng,
}

impl Default for Social {
    fn default() -> Self {
        Social { next_chat: BTreeMap::new(), greeted: BTreeSet::new(), rng: Rng(0x5EED_C0DE) }
    }
}

/// Whether a package allows a Fallout behaviour (`fallout::HELLOS_TO_PLAYER`,
/// `fallout::RANDOM_CONVERSATIONS`): see the module notes.
pub fn allows(def: &PackageDef, behaviour: u16) -> bool {
    !def.has(flags::SKIP_FALLOUT_BEHAVIOR) || def.fallout_flags & behaviour != 0
}

/// Whether a package lets its actor take part in random conversations.
pub fn chats(def: &PackageDef) -> bool {
    allows(def, fallout::RANDOM_CONVERSATIONS) && !def.sandbox_flag(type_flags::NO_CONVERSATION)
}

/// An actor that may talk now, with where it is and its package.
struct Free<'a> {
    actor: FormId,
    at: Place,
    def: &'a PackageDef,
    /// On furniture: it may be gone up to, but does not walk.
    seated: bool,
}

fn setting(game: &Game, name: &str, default: f32) -> f32 {
    game.world.forms.setting_f32(name).unwrap_or(default)
}

/// Whether an actor is in the world and alive.
fn here(game: &Game, actor: FormId) -> bool {
    game.present(actor) && !game.extra.get(&actor).is_some_and(|e| e.dead)
}

impl Ai {
    /// Whether the engine should keep `actor` where it stands for a random conversation:
    /// it waits for its partner, or talks.
    pub fn holds_for_chat(&self, actor: FormId) -> bool {
        self.agents.get(&actor).is_some_and(|a| matches!(a.chat, Some(Chat::Waiting { .. } | Chat::Talking { .. })))
    }

    /// One frame of hellos and random conversations (module notes). Call after the
    /// update that moved the actors; nothing happens while a menu, a loading screen or a
    /// conversation with the player holds the game.
    pub fn social(&mut self, game: &mut Game, nav: &NavGraph, dt: f32) {
        if game.menu.is_some() || game.loading || game.dialogue.is_some() {
            return;
        }
        self.chats_on(game, nav);
        self.face_listeners(game, dt);
        let data = game.data;
        let now = game.time;
        let mut free: Vec<Free> = Vec::new();
        for (&actor, a) in &self.agents {
            let Some(def) = a.package.and_then(|p| data.package_defs.get(&p)) else { continue };
            let busy = a.activity == Activity::Travelling || a.talk.is_some() || a.chat.is_some() || a.search.is_some() || def.ty == PackageType::Sleep;
            // Only actors in the loaded area talk (`crate::process`).
            if busy || !self.is_high(actor) || !here(game, actor) || game.is_talking(actor) || game.in_actor_conversation(actor) {
                continue;
            }
            free.push(Free { actor, at: Place { space: a.walker.space, pos: a.walker.feet }, def, seated: a.using.is_some() });
        }
        let player = goal::place_of_ref(game, FormId::PLAYER_REF);
        let mut events: Vec<(FormId, AiEvent)> = Vec::new();
        // Hellos.
        if let Some(p) = player {
            for f in &free {
                let away = if f.at.space == p.space { dist(f.at.pos, p.pos) } else { f32::INFINITY };
                if away > HELLO_DISTANCE * HELLO_REARM {
                    self.social.greeted.remove(&f.actor);
                }
                let saying = game.speech.iter().any(|s| s.hello);
                if saying || away > HELLO_DISTANCE || self.social.greeted.contains(&f.actor) || !allows(f.def, fallout::HELLOS_TO_PLAYER) || game.is_talking(f.actor) {
                    continue;
                }
                self.social.greeted.insert(f.actor);
                if let Some(line) = game.say_hello(f.actor) {
                    events.push((f.actor, AiEvent::Hello { line, away }));
                }
            }
        }
        for (actor, e) in events {
            self.log(game, actor, e);
        }
        // Random conversations.
        let mut taken: BTreeSet<FormId> = BTreeSet::new();
        for f in &free {
            if taken.contains(&f.actor) || !STARTERS.contains(&f.def.ty) || !chats(f.def) {
                continue;
            }
            let (lo, hi) = (setting(game, "fAISocialTimerForConversationsMin", TIMER_MIN), setting(game, "fAISocialTimerForConversationsMax", TIMER_MAX));
            let due = *self.social.next_chat.entry(f.actor).or_insert_with(|| now + self.social.rng.range(lo, hi) as f64);
            if now < due {
                continue;
            }
            let next = self.social.rng.range(lo, hi);
            self.social.next_chat.insert(f.actor, now + next as f64);
            let indoors = matches!(f.at.space, Space::Interior(_));
            let chance = setting(game, if indoors { "fAISocialchanceForConversationInterior" } else { "fAISocialchanceForConversation" }, CHANCE);
            if self.social.rng.next() * 100.0 >= chance || game.is_talking(f.actor) || game.in_actor_conversation(f.actor) {
                continue;
            }
            let interior = setting(game, "fAISocialRadiusToTriggerConversationInterior", RADIUS);
            let radius = if indoors { interior } else { setting(game, "fAISocialRadiusToTriggerConversation", interior) };
            let mut near: Vec<(f32, &Free)> = free
                .iter()
                .filter(|o| o.actor != f.actor && !taken.contains(&o.actor) && o.at.space == f.at.space && chats(o.def))
                .map(|o| (dist(o.at.pos, f.at.pos), o))
                .filter(|(d, _)| *d <= radius)
                .collect();
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (away, o) in near {
                // A seated starter does not get up to go to someone.
                if away > TALK_DISTANCE && f.seated {
                    continue;
                }
                if game.conversation_opener(f.actor, o.actor).is_none() {
                    continue;
                }
                let started = if away <= TALK_DISTANCE { self.chat_start(game, f.actor, o.actor, away) } else { self.chat_set_off(game, nav, f.actor, o.actor, o.at, away) };
                if started {
                    taken.extend([f.actor, o.actor]);
                    break;
                }
            }
        }
    }

    /// The random conversations under way: walks that arrive or are given up, and
    /// conversations that are over.
    fn chats_on(&mut self, game: &mut Game, nav: &NavGraph) {
        let now = game.time;
        let going: Vec<(FormId, Option<Chat>)> =self.agents.iter().filter(|(_, a)| a.chat.is_some()).map(|(id, a)| (*id, a.chat)).collect();
        for (actor, chat) in going {
            // Already settled from the other side in this pass.
            if self.agents[&actor].chat != chat {
                continue;
            }
            match chat {
                Some(Chat::Going { partner, since }) => {
                    let waits = self.agents.get(&partner).is_some_and(|p| p.chat == Some(Chat::Waiting { starter: actor }));
                    let why = if !here(game, actor) || !here(game, partner) {
                        Some("one of them is gone")
                    } else if !waits || game.is_talking(partner) || game.in_actor_conversation(partner) {
                        Some("the partner is busy")
                    } else if now - since > CHAT_WALK_SECONDS {
                        Some("the way took too long")
                    } else {
                        None
                    };
                    if let Some(why) = why {
                        self.chat_give_up(game, nav, actor, partner, why);
                        continue;
                    }
                    let a = &self.agents[&actor];
                    let at = goal::place_of_ref(game, partner);
                    let away = at.filter(|p| p.space == a.walker.space).map_or(f32::INFINITY, |p| dist(p.pos, a.walker.feet));
                    // Near enough, or as near as the navmesh lets it get.
                    if away <= TALK_DISTANCE || a.activity != Activity::Travelling {
                        let a = self.agents.get_mut(&actor).expect("listed");
                        a.walker.follower = None;
                        a.activity = Activity::There;
                        if !self.chat_start(game, actor, partner, away) {
                            self.chat_give_up(game, nav, actor, partner, "it has no line for the partner");
                        }
                    }
                }
                Some(Chat::Waiting { starter }) => {
                    // The starter gave up, or its package changed.
                    let goes = self.agents.get(&starter).is_some_and(|s| matches!(s.chat, Some(Chat::Going { partner: p, .. }) if p == actor));
                    if !goes {
                        self.agents.get_mut(&actor).expect("listed").chat = None;
                    }
                }
                Some(Chat::Talking { with }) if !game.in_actor_conversation(actor) && !game.in_actor_conversation(with) => {
                    self.chat_over(game, nav, actor);
                }
                Some(Chat::Talking { .. }) | None => {}
            }
        }
    }

    /// `starter` and `partner` are close: the conversation starts.
    fn chat_start(&mut self, game: &mut Game, starter: FormId, partner: FormId, away: f32) -> bool {
        let Ok(line) = game.start_actor_conversation(starter, partner, None) else { return false };
        for (who, with) in [(starter, partner), (partner, starter)] {
            if let Some(a) = self.agents.get_mut(&who) {
                a.chat = Some(Chat::Talking { with });
            }
        }
        self.log(game, starter, AiEvent::Chat { partner, line, away });
        true
    }

    /// `starter` sets off to `partner`, who waits. False when there is no way there.
    fn chat_set_off(&mut self, game: &mut Game, nav: &NavGraph, starter: FormId, partner: FormId, to: Place, away: f32) -> bool {
        let params = self.path_params(game, starter);
        let speed = self.cfg.walk_speed;
        let a = self.agents.get_mut(&starter).expect("listed");
        a.walker.speed = speed;
        let Ok(length) = a.walker.go_to(nav, to, &params).map(|p| p.length) else { return false };
        a.trip_started = game.time;
        a.trip_from = a.walker.travelled;
        a.activity = Activity::Travelling;
        a.chat = Some(Chat::Going { partner, since: game.time });
        if let Some(p) = self.agents.get_mut(&partner) {
            p.chat = Some(Chat::Waiting { starter });
        }
        self.log(game, starter, AiEvent::ChatApproach { partner, away, length });
        true
    }

    /// The walk to `partner` ends without a conversation: both go back to their packages.
    fn chat_give_up(&mut self, game: &mut Game, nav: &NavGraph, starter: FormId, partner: FormId, why: &'static str) {
        self.log(game, starter, AiEvent::ChatGaveUp { partner, why });
        if let Some(p) = self.agents.get_mut(&partner) {
            if p.chat == Some(Chat::Waiting { starter }) {
                p.chat = None;
            }
        }
        let a = self.agents.get_mut(&starter).expect("listed");
        a.walker.follower = None;
        a.activity = Activity::There;
        self.chat_over(game, nav, starter);
    }

    /// The conversation is over (or never began): the actor goes back to its package, and
    /// its time to the next one starts over [hypothesis: the timer is the time between an
    /// actor's conversations; without it the same two started again at once].
    fn chat_over(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId) {
        self.agents.get_mut(&actor).expect("listed").chat = None;
        let (lo, hi) = (setting(game, "fAISocialTimerForConversationsMin", TIMER_MIN), setting(game, "fAISocialTimerForConversationsMax", TIMER_MAX));
        let next = game.time + self.social.rng.range(lo, hi) as f64;
        self.social.next_chat.insert(actor, next);
        self.back_to_package(game, nav, actor, |package, away| AiEvent::ChatBack { package, away });
    }

    /// Who says a line to someone turns its body to the listener, and the listener of a
    /// conversation between actors to the speaker (`Game::facings`: SayTo lines, hellos,
    /// conversations; not a line whose script asked for no turning); who waits for a
    /// partner turns to it as it comes. Not on furniture, on its way, or turning with its
    /// Dialogue package already.
    fn face_listeners(&mut self, game: &mut Game, dt: f32) {
        let mut pairs: Vec<(FormId, FormId)> = game.facings().into_iter().filter(|f| f.body).map(|f| (f.who, f.to)).collect();
        pairs.extend(self.agents.iter().filter_map(|(id, a)| match a.chat {
            Some(Chat::Waiting { starter }) => Some((*id, starter)),
            _ => None,
        }));
        for (who, to) in pairs {
            let Some(a) = self.agents.get(&who) else { continue };
            if !self.is_high(who) || a.using.is_some() || a.activity == Activity::Travelling || a.talk == Some(Talking::Speaking) {
                continue;
            }
            if let Some(at) = goal::place_of_ref(game, to) {
                self.face(game, who, at, dt);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(ty: PackageType, general: u32, behaviour: u16, specific: u16) -> PackageDef {
        PackageDef { ty, flags: general, fallout_flags: behaviour, type_flags: specific, ..PackageDef::default() }
    }

    #[test]
    fn the_default_allows_and_a_skip_package_allows_what_it_sets() {
        // Bit 12 clear: the default, whatever the stored behaviour flags (Vault 3's
        // DefaultSandboxEditorLocation512 stores none).
        let plain = def(PackageType::Sandbox, 0, 0, 0);
        assert!(allows(&plain, fallout::HELLOS_TO_PLAYER) && chats(&plain));
        // Bit 12 set: only what it sets (Easy Pete's chair package, 0x1F6).
        let chair = def(PackageType::Travel, flags::SKIP_FALLOUT_BEHAVIOR, 0x1F6, 0);
        assert!(!allows(&chair, fallout::HELLOS_TO_PLAYER) && chats(&chair));
        let none = def(PackageType::Patrol, flags::SKIP_FALLOUT_BEHAVIOR, 0, 0);
        assert!(!allows(&none, fallout::HELLOS_TO_PLAYER) && !chats(&none));
        // No Conversation counts for Sandbox and Wander only.
        assert!(!chats(&def(PackageType::Sandbox, 0, 0, type_flags::NO_CONVERSATION)));
        assert!(chats(&def(PackageType::Guard, 0, 0, type_flags::NO_CONVERSATION)));
    }
}
