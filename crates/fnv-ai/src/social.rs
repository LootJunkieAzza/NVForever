//! Actors who talk on their own (docs/spec-notes/dialogue.md §5.2): an actor says hello
//! to the player as the player passes, and two actors near each other start a random
//! conversation. What is said is the game's (`Game::say_hello`,
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
//!   60 in FalloutNV.esm), and starts with a chance of `fAISocialchanceForConversation`
//!   per cent (`…Interior` indoors; 25 both) [observed: the settings' values in
//!   FalloutNV.esm; hypothesis: what they mean, from their names]. The partner is the
//!   nearest actor that may take part and for whom the starter has a HELLO line (Link
//!   From blank), within `fAISocialRadiusToTriggerConversationInterior` (1000) indoors
//!   and `fAISocialRadiusToTriggerConversation` outdoors (not in the masters: the indoor
//!   value stands in), and within [`CHAT_REACH`] [stand-in: in the game the two go up to
//!   each other first; here both must already be close and standing].
//!
//! While two actors talk, and while an actor says hello or a SayTo line, they turn to
//! whom they talk to (`Game::facings`, which says who turns; [hypothesis] for hellos and
//! conversations); the engine keeps both of a conversation where they stand.

use crate::carry::{Activity, Ai, AiEvent, Talking};
use crate::goal::{self, dist};
use crate::sandbox::Rng;
use fnv_game::game::Game;
use fnv_game::package::{fallout, flags, type_flags, PackageDef, PackageType};
use fnv_nav::{Place, Space};
use fnv_sim::FormId;
use std::collections::{BTreeMap, BTreeSet};

/// The player is greeted from this close, game units [stand-in].
pub const HELLO_DISTANCE: f32 = 300.0;
/// An actor that said hello greets again once the player has gone this many times
/// [`HELLO_DISTANCE`] away [stand-in].
pub const HELLO_REARM: f32 = 2.0;
/// Two actors start a random conversation only this close, game units [stand-in for the
/// walk up to each other].
pub const CHAT_REACH: f32 = 384.0;
/// The settings' values when the load order does not set them [the indoor values of
/// FalloutNV.esm].
const TIMER_MIN: f32 = 30.0;
const TIMER_MAX: f32 = 60.0;
const CHANCE: f32 = 25.0;
const RADIUS: f32 = 1000.0;

/// The packages that start random conversations [documented: Category:Packages].
const STARTERS: [PackageType; 4] = [PackageType::Sandbox, PackageType::Guard, PackageType::Eat, PackageType::Patrol];

pub(crate) struct Social {
    /// When (`Game::time`) each actor that may start a conversation next looks for a
    /// partner.
    next_chat: BTreeMap<FormId, f64>,
    /// Actors that said hello and wait for the player to go away.
    greeted: BTreeSet<FormId>,
    rng: Rng,
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
}

fn setting(game: &Game, name: &str, default: f32) -> f32 {
    game.world.forms.setting_f32(name).unwrap_or(default)
}

impl Ai {
    /// One frame of hellos and random conversations (module notes). Call after the
    /// update that moved the actors; nothing happens while a menu, a loading screen or a
    /// conversation with the player holds the game.
    pub fn social(&mut self, game: &mut Game, dt: f32) {
        if game.menu.is_some() || game.loading || game.dialogue.is_some() {
            return;
        }
        self.face_listeners(game, dt);
        let data = game.data;
        let now = game.time;
        let mut free: Vec<Free> = Vec::new();
        for (&actor, a) in &self.agents {
            let Some(def) = a.package.and_then(|p| data.package_defs.get(&p)) else { continue };
            let busy = a.activity == Activity::Travelling || a.talk.is_some() || def.ty == PackageType::Sleep;
            if busy || !game.present(actor) || game.extra.get(&actor).is_some_and(|e| e.dead) || game.is_talking(actor) || game.in_actor_conversation(actor) {
                continue;
            }
            free.push(Free { actor, at: Place { space: a.walker.space, pos: a.walker.feet }, def });
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
        // Random conversations.
        for f in &free {
            if !STARTERS.contains(&f.def.ty) || !chats(f.def) {
                continue;
            }
            let indoors = matches!(f.at.space, Space::Interior(_));
            let due = *self.social.next_chat.entry(f.actor).or_insert_with(|| now + self.social.rng.range(setting(game, "fAISocialTimerForConversationsMin", TIMER_MIN), setting(game, "fAISocialTimerForConversationsMax", TIMER_MAX)) as f64);
            if now < due {
                continue;
            }
            let next = self.social.rng.range(setting(game, "fAISocialTimerForConversationsMin", TIMER_MIN), setting(game, "fAISocialTimerForConversationsMax", TIMER_MAX));
            self.social.next_chat.insert(f.actor, now + next as f64);
            let chance = setting(game, if indoors { "fAISocialchanceForConversationInterior" } else { "fAISocialchanceForConversation" }, CHANCE);
            if self.social.rng.next() * 100.0 >= chance || game.is_talking(f.actor) || game.in_actor_conversation(f.actor) {
                continue;
            }
            let interior = setting(game, "fAISocialRadiusToTriggerConversationInterior", RADIUS);
            let radius = if indoors { interior } else { setting(game, "fAISocialRadiusToTriggerConversation", interior) };
            let reach = radius.min(CHAT_REACH);
            let mut near: Vec<(f32, FormId)> = free
                .iter()
                .filter(|o| o.actor != f.actor && o.at.space == f.at.space && chats(o.def))
                .map(|o| (dist(o.at.pos, f.at.pos), o.actor))
                .filter(|(d, _)| *d <= reach)
                .collect();
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (away, partner) in near {
                if let Ok(line) = game.start_actor_conversation(f.actor, partner, None) {
                    events.push((f.actor, AiEvent::Chat { partner, line, away }));
                    break;
                }
            }
        }
        for (actor, e) in events {
            self.log(game, actor, e);
        }
    }

    /// Who says a line to someone turns its body to the listener, and the listener of a
    /// conversation between actors to the speaker (`Game::facings`: SayTo lines, hellos,
    /// conversations; not a line whose script asked for no turning), unless on furniture,
    /// on its way, or turning with its Dialogue package already.
    fn face_listeners(&mut self, game: &mut Game, dt: f32) {
        for f in game.facings().into_iter().filter(|f| f.body) {
            let Some(a) = self.agents.get(&f.who) else { continue };
            if a.using.is_some() || a.activity == Activity::Travelling || a.talk == Some(Talking::Speaking) {
                continue;
            }
            if let Some(at) = goal::place_of_ref(game, f.to) {
                self.face(game, f.who, at, dt);
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
