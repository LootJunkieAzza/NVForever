//! Conversations between the player and an actor (docs/design/dialogue.md step D2), with
//! no engine types: which line an actor says, the greeting, the choices offered, choosing
//! one, the responses said in order with the begin and end result scripts, added topics,
//! Say Once, Say Once a Day, Goodbye, Random stacks, Run Immediately, follow-up lines and
//! the speech-challenge tag. The state ([`Conversation`]) lives in [`Game::dialogue`], so
//! snapshots and traces include it.
//!
//! Rules with their sources: docs/spec-notes/dialogue.md §4–§6. Tags used below:
//! `[documented: page]` is a page of the owner's GECK wiki export (2026-10-09) or an xEdit
//! definition; `[owner statement 2026-10-10]` is the owner's ruling from GECK use, not yet
//! captured; `[hypothesis]` is the simplest rule where the documentation is silent, each
//! one a capture question (dialogue.md §7).
//!
//! A conversation is a menu: while one runs the game is in menu mode
//! ([`Game::in_menu`]) [owner statement 2026-10-10], and only the conversation's own
//! lines go on, in real time ([`Game::update_dialogue`]).
//!
//! Conversations between two actors (`StartConversation` with an actor as the target,
//! [`Game::start_actor_conversation`]) are no menu: their lines are said in the world as
//! SayTo lines are ([`Game::speech`]), each line's Link To topics give the next one
//! (dialogue.md §5.1).

use crate::cond::{Operand, RunOn};
use crate::data::{info_flag, InfoDef};
use crate::game::{Game, LineKind, RunCtx, Speech};
use crate::trace::EventKind;
use fnv_sim::{clock, FormId};
use rand_core::RngCore;
use serde::{Deserialize, Serialize};

/// The dialogue menu's code [documented: GECK wiki GetMenuTargetRef, owner export: menu
/// 1009's target is the actor being spoken to; observed: one script of the masters
/// tests `MenuMode 1009`].
pub const DIALOGUE_MENU: u16 = 1009;

/// Hours an actor waits before saying a Say Once a Day line again when the game setting
/// `fSayOncePerDayInfoTimer` is not defined [documented: Category:Dialogue, 12 hours].
pub const SAY_ONCE_A_DAY_HOURS: f64 = 12.0;
pub const SAY_ONCE_A_DAY_SETTING: &str = "fSayOncePerDayInfoTimer";

/// The line an actor is saying in a conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub topic: FormId,
    pub info: FormId,
    /// Position in the line's response order ([`InfoDef::response_order`]) of the
    /// response being said.
    pub response: usize,
    /// Seconds of game time left of that response.
    pub remaining: f64,
}

/// The tag of a choice whose line names an actor value or perk (KNAM): the
/// "[Speech 40]" or "[Speech 37/40]" the player sees [documented: Category:Dialogue,
/// "AV/Perk": the field only controls what is shown; whether the line is available
/// depends on its own conditions].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Challenge {
    /// The actor value (AVIF) or perk (PERK).
    pub form: FormId,
    /// DNAM difficulty code (the Speech Challenge flag's die roll of Fallout 3; three
    /// lines of the masters carry it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<u32>,
    /// The level the line asks for and the player's: the comparison value and the
    /// current value of the line's first GetActorValue-family condition run on the
    /// listener [hypothesis: the game shows the numbers of that condition; observed:
    /// the masters' KNAM lines come in pairs whose conditions are `GetActorValue <skill>
    /// >= N` and `< N` on the target, dialogue.md §3.3].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub have: Option<f64>,
}

/// One entry of the list the player chooses from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub topic: FormId,
    /// The line the actor says when this entry is chosen (picked when the list is built).
    pub info: FormId,
    /// Characters of the text shown ([`Game::choice_text`]).
    pub chars: usize,
    /// The text is the line's prompt (else the topic's name).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub prompt: bool,
    /// The line has the Goodbye flag: choosing it ends the conversation after the line.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub goodbye: bool,
    /// The entry is shaded as already chosen: the actor said this line before and the
    /// line has no Info Refusal flag, or the line has Always Darken
    /// [documented: Category:Dialogue flags].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub darkened: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub challenge: Option<Challenge>,
}

/// A conversation between the player and an actor: the actor is saying a line
/// (`line`), or the player is choosing (`choices`), or, inside a line's result script,
/// neither.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conversation {
    pub actor: FormId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<Line>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<Choice>,
    /// Lines said so far.
    #[serde(default)]
    pub lines: u32,
}

/// Why a conversation ended (trace `dialogue_ended`).
pub mod ended {
    /// A line with the Goodbye flag finished.
    pub const GOODBYE: &str = "goodbye";
    /// The actor had nothing to offer after a line.
    pub const NO_CHOICES: &str = "no_choices";
    /// [`super::Game::end_dialogue`] was called (the player left).
    pub const LEFT: &str = "left";
    /// The actor died, was disabled or deleted.
    pub const ACTOR_GONE: &str = "actor_gone";
    /// Conversations between actors: the line just said links to nothing either side can
    /// say.
    pub const NO_LINKS: &str = "no_links";
    /// Conversations between actors: a line was cut off (its speaker started another
    /// line, or was disabled or died).
    pub const CUT: &str = "cut";
}

/// GetActorValue, GetBaseActorValue, GetPermanentActorValue.
const ACTOR_VALUE_OPCODES: [u16; 3] = [0x100E, 0x1115, 0x11EF];

impl<'a> Game<'a> {
    // ---- which line ---------------------------------------------------------------

    /// The game clock in hours (GameDaysPassed × 24 + GameHour), for Say Once a Day
    /// [hypothesis: the timer runs on game time].
    fn game_hours(&self) -> f64 {
        let global = |name: &str| self.world.forms.global_by_edid(name).and_then(|(id, _)| self.world.global(id).ok()).unwrap_or(0.0) as f64;
        global(clock::GAME_DAYS_PASSED) * 24.0 + global(clock::GAME_HOUR)
    }

    fn say_once_a_day_hours(&self) -> f64 {
        self.world.forms.setting_f32(SAY_ONCE_A_DAY_SETTING).map(|h| h as f64).unwrap_or(SAY_ONCE_A_DAY_HOURS)
    }

    /// Whether `speaker` can say the line `info` to `listener` now:
    /// - a Say Once line the speaker has not said, a Say Once a Day line it has not said
    ///   in the last `fSayOncePerDayInfoTimer` (12) game hours [documented:
    ///   Category:Dialogue: both are per actor];
    /// - the speaker is the line's fixed speaker, if it has one (a reference of that
    ///   base form) [documented: Category:Dialogue: "Speaker" acts like GetIsID];
    /// - the line's quest is running and the quest's own conditions pass [owner
    ///   statement 2026-10-10: every actor has the line as long as the quest is active
    ///   and the conditions are met; documented: Quest Data Tab: quest conditions gate
    ///   all of the quest's dialogue, checked first]. The quest's conditions run like
    ///   the line's, with the speaker as subject and the listener as target
    ///   [hypothesis: the page does not say on whom];
    /// - the line's conditions pass with the speaker as subject and the listener as
    ///   target [documented: Category:Conditions, Run On].
    pub fn can_say(&mut self, speaker: FormId, listener: Option<FormId>, info: FormId) -> bool {
        let data = self.data;
        let Some(def) = data.infos.get(&info) else { return false };
        if def.say_once && self.said.contains(&(speaker, info)) {
            return false;
        }
        if def.has(info_flag::SAY_ONCE_A_DAY) {
            if let Some(&at) = self.said_day.get(&speaker).and_then(|m| m.get(&info)) {
                if self.game_hours() - at < self.say_once_a_day_hours() {
                    return false;
                }
            }
        }
        if let Some(s) = def.speaker {
            if s != speaker && Some(s) != crate::functions::base_of(self, speaker) {
                return false;
            }
        }
        if let Some(q) = def.quest {
            if !self.quests.get(&q).is_some_and(|s| s.running) {
                return false;
            }
            if let Some(c) = data.quests.get(&q).map(|d| &d.conditions).filter(|c| !c.is_empty()) {
                if !self.conditions_pass(c, Some(speaker), listener, &format!("QUST {q} dialogue")) {
                    return false;
                }
            }
        }
        self.conditions_pass(&def.conditions, Some(speaker), listener, &format!("INFO {info}"))
    }

    /// The line of `topic` that `speaker` says to `listener` now, for greetings, chosen
    /// topics, SayTo and Say alike:
    /// 1. The topic's lines, its own and those it shares from other topics
    ///    ([`GameData::lines_tried`]), are taken with the lines of higher-priority quests first
    ///    [documented: Quest Data Tab: priority decides whose dialogue wins, higher
    ///    wins; owner statement 2026-10-10].
    /// 2. Lines of quests of the same priority, and lines of one quest, keep their
    ///    stored order, top to bottom as the editor lists them ([`GameData::topics`]:
    ///    the file order of the plugin that first defines each line), and the first
    ///    line the speaker can say ([`Game::can_say`]) is the one
    ///    [owner statement 2026-10-10, from GECK use; not yet captured; observed for
    ///    SayTo: Doc Mitchell's VCG01Intro lines, layer5-captures.md §10.3].
    /// 3. Unless lines carry the Random flag. Random lines that sit together in that
    ///    order form a pool; a line without the flag between two groups of Random lines
    ///    splits them into two pools, whether or not that line can be said
    ///    [owner statement 2026-10-10; not yet captured]. A Random End line closes its
    ///    pool at that line [documented: Category:Dialogue, flag Random End; hypothesis
    ///    that it does so even when it cannot be said itself]. Going down the list, a
    ///    plain line is taken if the speaker can say it; a pool is taken if the speaker
    ///    can say any of its lines, and then one of those is picked at random
    ///    [documented: Category:Dialogue, flag Random, for the pick].
    ///
    /// The pick uses the game's seeded generator, and only when more than one line of
    /// the pool can be said.
    pub fn select_line(&mut self, speaker: FormId, listener: Option<FormId>, topic: FormId) -> Option<FormId> {
        self.select_line_where(speaker, listener, topic, |_| true)
    }

    /// [`Game::select_line`] among the lines `keep` accepts: the others count as lines
    /// the speaker cannot say.
    pub(crate) fn select_line_where(&mut self, speaker: FormId, listener: Option<FormId>, topic: FormId, keep: impl Fn(&InfoDef) -> bool) -> Option<FormId> {
        let data = self.data;
        let lines = data.lines_tried(topic);
        let flag = |i: &FormId, f: u16| data.infos.get(i).is_some_and(|d| d.has(f));
        let kept = |i: &FormId| data.infos.get(i).is_some_and(&keep);
        let mut at = 0;
        while at < lines.len() {
            if !flag(&lines[at], info_flag::RANDOM) {
                if kept(&lines[at]) && self.can_say(speaker, listener, lines[at]) {
                    return Some(lines[at]);
                }
                at += 1;
                continue;
            }
            // A pool: the Random lines from here to the first line without the flag, or
            // through the first one with Random End.
            let mut end = at;
            while end < lines.len() && flag(&lines[end], info_flag::RANDOM) {
                end += 1;
                if flag(&lines[end - 1], info_flag::RANDOM_END) {
                    break;
                }
            }
            let pool: Vec<FormId> = lines[at..end].to_vec();
            let sayable: Vec<FormId> = pool.into_iter().filter(|i| kept(i) && self.can_say(speaker, listener, *i)).collect();
            match sayable.len() {
                0 => {}
                1 => return Some(sayable[0]),
                n => return Some(sayable[self.rng.next_u32() as usize % n]),
            }
            at = end;
        }
        None
    }

    /// The speaker said the line: Say Once and Say Once a Day remember it per actor
    /// [documented: Category:Dialogue]. `always` remembers every line (conversations:
    /// the shading of choices already made).
    pub(crate) fn mark_said(&mut self, speaker: FormId, info: FormId, always: bool) {
        let Some(def) = self.data.infos.get(&info) else { return };
        if def.say_once || always {
            self.said.insert((speaker, info));
        }
        if def.has(info_flag::SAY_ONCE_A_DAY) {
            let now = self.game_hours();
            self.said_day.entry(speaker).or_default().insert(info, now);
        }
    }

    // ---- how long a response lasts ---------------------------------------------------

    /// Seconds one response of a line lasts when `speaker` says it. **The one place a
    /// response is timed**: SayTo, Say and conversations all come here. It is the length
    /// of the response's voice file in the speaker's voice when the game was given the
    /// files ([`Game::voice`], `crate::voice`, docs/spec-notes/voice.md §8); without the
    /// file it is the length fitted to the response text
    /// ([`crate::game::fitted_seconds`], scripts.md §7.7).
    pub fn response_seconds(&self, speaker: FormId, info: FormId, response: usize) -> f64 {
        self.response_seconds_as(crate::functions::base_of(self, speaker), info, response)
    }

    /// Seconds a whole line lasts: its responses one after another; a line without
    /// responses lasts as long as an empty response.
    pub fn line_seconds(&self, speaker: FormId, info: FormId) -> f64 {
        let n = self.data.infos.get(&info).map_or(0, |d| d.responses.len());
        if n == 0 {
            return self.response_seconds(speaker, info, 0);
        }
        (0..n).map(|r| self.response_seconds(speaker, info, r)).sum()
    }

    // ---- conversations -------------------------------------------------------------

    /// The player starts talking to `actor` (activation), or the actor starts talking to
    /// the player (StartConversation, a dialogue package): the actor says the line of
    /// `topic` it can say now ([`Game::select_line`]), GREETING when `topic` is none
    /// [documented: Topics Tab: GREETING is what an actor says when spoken to;
    /// StartConversation: the named topic replaces the greeting]. Returns the line.
    ///
    /// **A greeting with the Goodbye flag starts no conversation**: when the line picked
    /// from GREETING carries the flag, the actor just says it at the player, as a SayTo
    /// line is said ([`Game::say_line`]). [`Game::dialogue`] stays empty, so there is no
    /// menu mode, no pause and nothing to choose or leave: the player is not held and
    /// the game goes on while the line is heard [owner statement 2026-10-10: such a line
    /// "does not draw the player into a dialogue exchange, it just is spoken at the
    /// player without locking them in"; not yet captured]. The begin result script runs
    /// at once and the end result script when the line is over, in game time; its Add
    /// Topics are applied then, the actor counts as having talked to the player
    /// (GetTalkedToPC) and no SayToDone block runs [hypothesis for these three: what a
    /// conversation of that one line did]. Talking to the actor again while it speaks
    /// cuts the line and starts the greeting afresh [hypothesis, as for a second
    /// SayTo]. A Goodbye line of a topic named by StartConversation still opens a
    /// conversation that ends with it [hypothesis: the statement is about GREETING;
    /// capture question, dialogue.md §7].
    ///
    /// An actor with no line to say cannot be talked to: nothing starts
    /// [hypothesis]. Neither does a dead, disabled or deleted actor [documented for
    /// StartConversation: nothing happens when either side is disabled]. One
    /// conversation at a time: a second start while one runs is refused [hypothesis].
    ///
    /// While the conversation runs the game is paused behind it, as behind the other
    /// menus: GameMode blocks do not run, game time and GameMode timers stand still,
    /// `MenuMode` and `MenuMode 1009` read 1 and MenuMode blocks run
    /// [owner statement 2026-10-10: a back-and-forth dialogue with the player pauses the
    /// game in the background, unlike lines spoken outside dialogue; not yet captured;
    /// the details follow the menu rules observed for other menus, scripts.md §7.7].
    /// A conversation started from a GameMode block holds the rest of that frame's
    /// GameMode scripts, as any menu does. [`Game::menu`] stays as it is: the
    /// conversation is the menu ([`Game::in_menu`], dialogue.md §6).
    pub fn start_dialogue(&mut self, actor: FormId, topic: Option<FormId>) -> Result<FormId, String> {
        let data = self.data;
        if let Some(c) = &self.dialogue {
            return Err(format!("a conversation with {} is running", data.name(c.actor)));
        }
        if !self.present(actor) || self.extra.get(&actor).is_some_and(|e| e.dead) {
            return Err(format!("{} is not there to talk to", data.name(actor)));
        }
        let topic = topic.or(data.greeting).ok_or("the load order has no GREETING topic")?;
        let listener = Some(FormId::PLAYER_REF);
        let Some(info) = self.select_line(actor, listener, topic) else {
            self.emit(EventKind::NoLine { speaker: actor, topic });
            return Err(format!("{} has no line of {} to say", data.name(actor), data.name(topic)));
        };
        if Some(topic) == data.greeting && data.infos.get(&info).is_some_and(|d| d.goodbye) {
            self.extra.entry(actor).or_default().talked_to_pc = true;
            self.say_line(actor, listener, topic, info, LineKind::Greeting);
            return Ok(info);
        }
        self.dialogue = Some(Conversation { actor, line: None, choices: Vec::new(), lines: 0 });
        self.extra.entry(actor).or_default().talked_to_pc = true;
        self.emit(EventKind::DialogueStarted { actor, topic });
        self.speak(actor, topic, info);
        Ok(info)
    }

    /// The actor of the running conversation starts saying a line: the begin result
    /// script runs now and the end result script when the last response is over
    /// [documented: Category:Dialogue, Result Script (Begin) / (End)]. With Run
    /// Immediately both run now [documented: the results run when the conversation is
    /// generated instead of when the line is spoken; hypothesis: for a line the player
    /// asked for, that moment is when the line is picked; the masters set the flag only
    /// on Radio and Conversation lines].
    fn speak(&mut self, actor: FormId, topic: FormId, info: FormId) {
        let data = self.data;
        let Some(def) = data.infos.get(&info) else { return };
        // A SayTo line the actor was saying stops [hypothesis, as for a second SayTo].
        self.cut_line(actor);
        self.mark_said(actor, info, true);
        let order = def.response_order();
        let seconds = self.line_seconds(actor, info);
        let first = order.first().map_or(seconds, |&r| self.response_seconds(actor, info, r));
        self.emit(EventKind::LineStarted { speaker: actor, target: Some(FormId::PLAYER_REF), topic, info, seconds });
        if let Some(c) = self.dialogue.as_mut() {
            c.line = Some(Line { topic, info, response: 0, remaining: first });
            c.choices.clear();
            c.lines += 1;
        }
        self.emit_response(actor, def, info, 0);
        self.run_line_result(actor, def, false);
        if def.has(info_flag::RUN_IMMEDIATELY) {
            self.run_line_result(actor, def, true);
        }
    }

    fn emit_response(&mut self, actor: FormId, def: &InfoDef, info: FormId, position: usize) {
        let Some(&r) = def.response_order().get(position) else { return };
        let resp = &def.responses[r];
        let seconds = self.response_seconds(actor, info, r);
        self.emit(EventKind::ResponseSaid { speaker: actor, info, number: resp.number, chars: def.response_chars[r], emotion: resp.emotion, seconds });
        // The listener of a conversation's line is the player.
        self.ask_response_idles(actor, Some(FormId::PLAYER_REF), info, r);
    }

    fn run_line_result(&mut self, actor: FormId, def: &InfoDef, end: bool) {
        let data = self.data;
        if let Some(u) = if end { def.end } else { def.begin } {
            self.emit(EventKind::ResultScript { unit: data.unit(u).key(), on: Some(actor) });
            self.run_result(u, RunCtx { self_ref: Some(actor), action_ref: None, quest: def.quest });
        }
    }

    /// One frame of the conversation, in real time: the game is paused behind it but its
    /// own lines go on [owner statement 2026-10-10]; a menu opened on top of it (a
    /// message box from a result script) holds it [hypothesis]. The responses of the
    /// line are said one after another, in
    /// response-number order [documented: xEdit "Response number"]; when the last is
    /// over the line ends ([`Game::finish_line`]). An actor that died, was disabled or
    /// deleted ends the conversation [hypothesis].
    pub(crate) fn update_dialogue(&mut self, dt: f64) {
        let Some(c) = self.dialogue.as_ref() else { return };
        let actor = c.actor;
        if !self.present(actor) || self.extra.get(&actor).is_some_and(|e| e.dead) {
            self.end_dialogue_as(ended::ACTOR_GONE);
            return;
        }
        let data = self.data;
        let Some(mut line) = c.line.clone() else { return };
        let Some(def) = data.infos.get(&line.info) else { return };
        let order = def.response_order();
        line.remaining -= dt;
        while line.remaining <= 1e-9 {
            line.response += 1;
            match order.get(line.response) {
                Some(&r) => {
                    line.remaining += self.response_seconds(actor, line.info, r);
                    self.emit_response(actor, def, line.info, line.response);
                }
                None => {
                    self.finish_line(actor, line.info);
                    return;
                }
            }
        }
        if let Some(c) = self.dialogue.as_mut() {
            c.line = Some(line);
        }
    }

    /// A line of the conversation is over. In order:
    /// 1. its end result script runs (unless Run Immediately ran it at the start);
    /// 2. the topics of its Add Topics list join the topics the player knows
    ///    ([`crate::game::PlayerState::topics`], the set `AddTopic` fills)
    ///    [documented: xEdit INFO "Add Topics"; hypothesis: they are added when the
    ///    line ends, and stay known after the conversation];
    /// 3. a Goodbye line ends the conversation [documented: Category:Dialogue];
    /// 4. otherwise, if the line has follow-up lines (TCFU), the first one the actor can
    ///    say is said next, with no choice by the player [owner statement 2026-10-10:
    ///    follow-ups have no player response, as in Doc Mitchell's farewell; not yet
    ///    captured; hypothesis: the first sayable of several, dialogue.md §3.4];
    /// 5. otherwise the player is offered choices ([`Game::build_choices`]); with
    ///    nothing to offer the conversation ends [hypothesis].
    fn finish_line(&mut self, actor: FormId, info: FormId) {
        let data = self.data;
        let Some(def) = data.infos.get(&info) else { return };
        if let Some(c) = self.dialogue.as_mut() {
            c.line = None;
        }
        self.emit(EventKind::LineEnded { speaker: actor, info, cut: false });
        if !def.has(info_flag::RUN_IMMEDIATELY) {
            self.run_line_result(actor, def, true);
        }
        for &t in &def.add_topics {
            if self.player.topics.insert(t) {
                self.emit(EventKind::TopicAdded { topic: t, by: Some(info) });
            }
        }
        // The end script may have ended the conversation.
        if !self.dialogue.as_ref().is_some_and(|c| c.actor == actor && c.line.is_none()) {
            return;
        }
        if def.goodbye {
            self.end_dialogue_as(ended::GOODBYE);
            return;
        }
        let listener = Some(FormId::PLAYER_REF);
        for &f in &def.follow_ups {
            if self.can_say(actor, listener, f) {
                let topic = data.infos.get(&f).and_then(|d| d.topic).unwrap_or(FormId(0));
                self.speak(actor, topic, f);
                return;
            }
        }
        let choices = self.build_choices(actor, def);
        if choices.is_empty() {
            self.end_dialogue_as(ended::NO_CHOICES);
            return;
        }
        self.emit(EventKind::ChoicesOffered { actor, choices: choices.clone() });
        if let Some(c) = self.dialogue.as_mut() {
            c.choices = choices;
        }
    }

    /// The list the player chooses from after the line `def`:
    /// - the line's choices (TCLT), if it has any, and nothing else
    ///   [documented: Category:Dialogue: top-level topics are "the ones that come up when
    ///   an info isn't linked to anything in particular"; Quest and Dialogue Tutorial;
    ///   owner statement 2026-10-11: a line linked by choices offers only the topics given
    ///   as its choices]. When none of them can be said nothing is offered and the
    ///   conversation ends [hypothesis; earlier the topic list stood in, which showed the
    ///   top-level topics after lines whose choices only had shared lines];
    /// - otherwise the top-level topics and the topics the player knows (lines' Add
    ///   Topics lists and `AddTopic`) [documented: the same pages]; a known topic is
    ///   offered by every actor that can say a line of it [owner statement 2026-10-10;
    ///   not yet captured].
    ///
    /// Each topic is offered only if the actor has a line of it to say now, its own or
    /// one it shares ([`GameData::lines_tried`]), and that line is the one chosen later
    /// [hypothesis; the editor's filtered dialogue window lists only what an actor
    /// qualifies to say, Category:Dialogue].
    ///
    /// Order: topic priority (DIAL PNAM), highest first [documented: Category:Dialogue,
    /// "Priority"], then for a line's choices their order in the record [documented:
    /// "Sorting order of links"] and for the topic list the topics' record order
    /// [hypothesis].
    ///
    /// When the topic list holds no line with the Goodbye flag, the actor's line of the
    /// GOODBYE topic is offered last, if it has one [documented: Quest and Dialogue
    /// Tutorial: the generic GOODBYE of the Conversation tab appears when no listed
    /// topic says goodbye; Conversation Tab: the topic is also what NPCs say when the
    /// player leaves the menu].
    pub fn build_choices(&mut self, actor: FormId, def: &InfoDef) -> Vec<Choice> {
        let data = self.data;
        let priority = |t: &FormId| data.topic_defs.get(t).map_or(50.0, |d| d.priority);
        if !def.choices.is_empty() {
            let mut linked = def.choices.clone();
            linked.sort_by(|a, b| priority(b).total_cmp(&priority(a)));
            return self.offer(actor, &linked);
        }
        let mut listed: Vec<FormId> = data.top_level.clone();
        let mut known: Vec<FormId> = self.player.topics.iter().copied().filter(|t| !listed.contains(t)).collect();
        if !known.is_empty() {
            listed.append(&mut known);
            let order = |t: &FormId| data.topic_defs.get(t).map_or((usize::MAX, 0), |d| d.order);
            listed.sort_by(|a, b| priority(b).total_cmp(&priority(a)).then(order(a).cmp(&order(b))));
        }
        let mut out = self.offer(actor, &listed);
        if !out.iter().any(|c| c.goodbye) {
            if let Some(bye) = data.goodbye.filter(|b| !out.iter().any(|c| c.topic == *b)) {
                out.extend(self.offer(actor, &[bye]));
            }
        }
        out
    }

    /// The entries for the topics of `topics` the actor has a line for, in that order.
    fn offer(&mut self, actor: FormId, topics: &[FormId]) -> Vec<Choice> {
        let data = self.data;
        let listener = Some(FormId::PLAYER_REF);
        let mut out: Vec<Choice> = Vec::new();
        for &topic in topics {
            if out.iter().any(|c| c.topic == topic) {
                continue;
            }
            let Some(info) = self.select_line(actor, listener, topic) else { continue };
            let def = &data.infos[&info];
            let challenge = def.challenge_form.map(|form| {
                let (needed, have) = self.challenge_levels(def);
                Challenge { form, difficulty: def.challenge_difficulty, needed, have }
            });
            let said = self.said.contains(&(actor, info));
            out.push(Choice {
                topic,
                info,
                chars: data.choice_text(topic, info).chars().count(),
                prompt: def.prompt.is_some(),
                goodbye: def.goodbye,
                darkened: def.has(info_flag::ALWAYS_DARKEN) || (said && !def.has(info_flag::INFO_REFUSAL)),
                challenge,
            });
        }
        out
    }

    /// The needed and current level of a KNAM line ([`Challenge::needed`]).
    fn challenge_levels(&mut self, def: &InfoDef) -> (Option<f64>, Option<f64>) {
        let Some(c) = def.conditions.iter().find(|c| c.run_on == RunOn::Target && ACTOR_VALUE_OPCODES.contains(&c.opcode())) else {
            return (None, None);
        };
        let needed = match c.rhs {
            Operand::Value(x) => Some(x),
            Operand::Global(g) => g.and_then(|g| self.world.global(FormId(g)).ok()).map(|v| v as f64),
        };
        let have = self.condition_value(c, Some(FormId::PLAYER_REF), "dialogue challenge");
        (needed, Some(have))
    }

    /// The choices offered now (empty while the actor speaks and outside conversations).
    pub fn dialogue_choices(&self) -> &[Choice] {
        self.dialogue.as_ref().map_or(&[], |c| c.choices.as_slice())
    }

    /// The text of a choice: the line's prompt, else the topic's name
    /// [documented: Category:Dialogue, "Prompt"].
    pub fn choice_text(&self, c: &Choice) -> &'a str {
        self.data.choice_text(c.topic, c.info)
    }

    /// The player picks entry `index` of the offered list: the actor says its line.
    pub fn choose(&mut self, index: usize) -> Result<FormId, String> {
        let c = self.dialogue.as_ref().ok_or("no conversation is running")?;
        if c.line.is_some() {
            return Err("the actor is still speaking".into());
        }
        let actor = c.actor;
        let choice = c.choices.get(index).cloned().ok_or_else(|| format!("no choice {index} ({} offered)", c.choices.len()))?;
        self.emit(EventKind::ChoiceMade { actor, index, topic: choice.topic, info: choice.info });
        self.speak(actor, choice.topic, choice.info);
        Ok(choice.info)
    }

    /// The player skips the response the actor is saying (a click or a key on the
    /// dialogue screen): that response ends now, as if its time had run out. The next
    /// response of the line starts at once; after the last one the line ends as a line
    /// said to the end does ([`Game::finish_line`]): end result script, Add Topics,
    /// Goodbye, follow-ups, choices [hypothesis: skipping only shortens a line. The
    /// other reading, a skipped line counted as cut and running no end script, as a line
    /// cut by leaving does ([`Game::end_dialogue`]), would stop every quest whose stages
    /// are set by end scripts whenever the player clicks through; capture question,
    /// dialogue.md §7 item 8]. No trace event of its own: the next `response_said` or
    /// the `line_ended` (not cut) simply comes early.
    ///
    /// Returns whether a response was skipped: not outside a conversation, not while the
    /// player chooses, and not while a loading screen or a menu opened on top of the
    /// conversation (a message box a result script showed, [`Game::menu`]) holds its
    /// lines ([`Game::tick`]).
    pub fn skip_response(&mut self) -> bool {
        if self.loading || self.menu.is_some() {
            return false;
        }
        let Some(line) = self.dialogue.as_mut().and_then(|c| c.line.as_mut()) else { return false };
        line.remaining = 0.0;
        self.update_dialogue(0.0);
        true
    }

    /// The player leaves the conversation. A line being said is cut off without its end
    /// result script [hypothesis, as for a SayTo line that is cut]. Whether the game
    /// lets the player leave at any point is not modelled: the engine decides when to
    /// call this.
    pub fn end_dialogue(&mut self) -> bool {
        self.end_dialogue_as(ended::LEFT)
    }

    fn end_dialogue_as(&mut self, reason: &'static str) -> bool {
        let Some(c) = self.dialogue.take() else { return false };
        if let Some(l) = c.line {
            self.emit(EventKind::LineEnded { speaker: c.actor, info: l.info, cut: true });
        }
        self.emit(EventKind::DialogueEnded { actor: c.actor, reason, lines: c.lines });
        true
    }

    // ---- conversations between actors (dialogue.md §5.1) --------------------------------

    /// `speaker.StartConversation target [topic]` with an actor as the target: the two
    /// talk to each other and the player overhears them [documented: GECK wiki
    /// StartConversation: it "forces a conversation between two NPCs, optionally starting
    /// with a specified topic"; Conversation Tab: a conversation is "a series of infos
    /// that are played between two actors"]. The speaker says the line of `topic` it can
    /// say to the target ([`Game::select_line`], the target as the conditions' target),
    /// HELLO when `topic` is none [hypothesis, from the Conversation Tab: random
    /// conversations start with HELLO]. Returns the line.
    ///
    /// The lines are said in the world as SayTo lines are ([`Game::say_line`],
    /// [`LineKind::Conversation`]): no menu, nothing paused, the player stays free
    /// [hypothesis: the player is not part of it], and only the first response of each
    /// line is said [documented: Conversation Tab: "Infos in conversations cannot have
    /// more than one response. Extra responses are ignored"]. When a line ends the next
    /// one follows ([`Game::next_conversation_line`]).
    ///
    /// In the game the speaker first goes to the target ("no matter where the two NPCs
    /// are"); that walk needs AI packages, so here the conversation starts at once
    /// [stand-in, as for `StartConversation player`; Joe Cobb starts next to Trudy in the
    /// owner's footage, owner statement 2026-10-11]. Nothing starts when either side is
    /// disabled [documented: StartConversation], dead or deleted [hypothesis], when either
    /// is already in a conversation, with the player or another actor [hypothesis], or
    /// when the speaker has no line to say [hypothesis].
    pub fn start_actor_conversation(&mut self, speaker: FormId, target: FormId, topic: Option<FormId>) -> Result<FormId, String> {
        let data = self.data;
        if speaker == target {
            return Err(format!("{} cannot talk to itself", data.name(speaker)));
        }
        for who in [speaker, target] {
            if !self.present(who) || self.extra.get(&who).is_some_and(|e| e.dead) {
                return Err(format!("{} is not there to talk", data.name(who)));
            }
            if self.dialogue_actor() == Some(who) || self.in_actor_conversation(who) {
                return Err(format!("{} is already in a conversation", data.name(who)));
            }
        }
        let hello = self.hello_topic();
        let topic = topic.or(hello).ok_or("the load order has no HELLO topic")?;
        // A conversation starts only with a HELLO line whose Link From is blank
        // [documented: Conversation Tab].
        let starts = |d: &InfoDef| Some(topic) != hello || d.link_from.is_empty();
        let Some(info) = self.select_line_where(speaker, Some(target), topic, starts) else {
            self.emit(EventKind::NoLine { speaker, topic });
            return Err(format!("{} has no line of {} to say to {}", data.name(speaker), data.name(topic), data.name(target)));
        };
        self.emit(EventKind::ActorConversationStarted { speaker, target, topic });
        self.say_line(speaker, Some(target), topic, info, LineKind::Conversation(1));
        Ok(info)
    }

    /// The HELLO line `speaker` would open a conversation with `target` with now (Link
    /// From blank), without saying it: whether there is anything to go up to `target`
    /// for (`fnv_ai::social`). The line said when they meet is chosen again then.
    pub fn conversation_opener(&mut self, speaker: FormId, target: FormId) -> Option<FormId> {
        let topic = self.hello_topic()?;
        self.select_line_where(speaker, Some(target), topic, |d| d.link_from.is_empty())
    }

    /// The HELLO topic, when the load order has it.
    pub fn hello_topic(&self) -> Option<FormId> {
        let data = self.data;
        data.by_edid("HELLO").filter(|t| data.topic_defs.contains_key(t))
    }

    /// `actor` says hello to the player as the player passes (docs/spec-notes/dialogue.md
    /// §5.2): its line of HELLO for the player as the conditions' target, Link From blank
    /// as for the start of a conversation [documented: Conversation Tab, HELLO; the
    /// Topics-tab GREETING is what an actor says when spoken to, Topics Tab]. The line is
    /// said in the world ([`LineKind::Hello`]): no conversation, no menu. A line that opens
    /// an exchange between actors (Link To) is said to the player all the same, and its
    /// links are not followed: nobody replies and the actor goes back to what it was doing
    /// [owner statement 2026-10-11, of the Fiends' lines in Vault 3]. Returns the line;
    /// none when the actor has no line for the player now. Whether to say hello at all
    /// (the package's "Hellos to player", the distance) is the AI's
    /// (`fnv_ai::social`).
    pub fn say_hello(&mut self, actor: FormId) -> Option<FormId> {
        let topic = self.hello_topic()?;
        let listener = Some(FormId::PLAYER_REF);
        let info = self.select_line_where(actor, listener, topic, |d| d.link_from.is_empty())?;
        self.say_line(actor, listener, topic, info, LineKind::Hello);
        Some(info)
    }

    /// Whether an actor is saying, or listening to, a line of a conversation between
    /// actors.
    pub fn in_actor_conversation(&self, actor: FormId) -> bool {
        self.speech.iter().any(|s| s.conversation.is_some() && (s.speaker == actor || s.target == Some(actor)))
    }

    /// Line `lines` of a conversation between actors (`said`) is over and its end result
    /// script has run: the next line, or the end of the conversation. In order:
    /// 1. a line with the Goodbye flag ends it [documented: Conversation Tab: a
    ///    conversation ends with an info with the Goodbye flag];
    /// 2. so does either side being gone (dead, disabled, deleted) [hypothesis];
    /// 3. the line's next speaker (INFO DATA: Target, the listener; Self, the speaker
    ///    again; Either, the listener and else the speaker) [documented: xEdit
    ///    wbNextSpeaker, the names; hypothesis: what they mean, and the order for Either]
    ///    says the first of the line's follow-ups it can say to the other
    ///    [hypothesis, as in conversations with the player];
    /// 4. else one of the line's Link To topics (the choices, TCLT) of which it has a line
    ///    to say, at random when there are several [documented: Conversation Tab: with
    ///    more than one Link To topic "the game [selects] one of the Link To topics at
    ///    random"; hypothesis: topics with no line it can say are passed over];
    /// 5. else the conversation ends. A GOODBYE line is not added [owner statement
    ///    2026-10-11: no goodbye follows Trudy's answer to Joe Cobb, a line that links
    ///    nowhere; the Conversation Tab's GOODBYE rule is about random conversations,
    ///    dialogue.md §5.1].
    pub(crate) fn next_conversation_line(&mut self, said: &Speech, lines: u32) {
        let data = self.data;
        let (speaker, Some(listener)) = (said.speaker, said.target) else { return };
        let end = |g: &mut Self, reason: &'static str| g.emit(EventKind::ActorConversationEnded { speaker, target: listener, reason, lines });
        let Some(def) = data.infos.get(&said.info) else { return end(self, ended::NO_LINKS) };
        if def.goodbye {
            return end(self, ended::GOODBYE);
        }
        if [speaker, listener].iter().any(|&a| !self.present(a) || self.extra.get(&a).is_some_and(|e| e.dead)) {
            return end(self, ended::ACTOR_GONE);
        }
        let turns: &[(FormId, FormId)] = match def.next_speaker {
            1 => &[(speaker, listener)],
            2 => &[(listener, speaker), (speaker, listener)],
            _ => &[(listener, speaker)],
        };
        let next = LineKind::Conversation(lines + 1);
        for &(who, to) in turns {
            if let Some(&f) = def.follow_ups.iter().find(|&&f| self.can_say(who, Some(to), f)) {
                let topic = data.infos.get(&f).and_then(|d| d.topic).unwrap_or(FormId(0));
                self.say_line(who, Some(to), topic, f, next);
                return;
            }
            let mut linked: Vec<(FormId, FormId)> = Vec::new();
            for &t in &def.choices {
                if !linked.iter().any(|(seen, _)| *seen == t) {
                    if let Some(i) = self.select_line(who, Some(to), t) {
                        linked.push((t, i));
                    }
                }
            }
            let (topic, info) = match linked.len() {
                0 => continue,
                1 => linked[0],
                n => linked[self.rng.next_u32() as usize % n],
            };
            self.say_line(who, Some(to), topic, info, next);
            return;
        }
        end(self, ended::NO_LINKS)
    }

    /// The actor the player is in a conversation with.
    pub fn dialogue_actor(&self) -> Option<FormId> {
        self.dialogue.as_ref().map(|c| c.actor)
    }

    /// The emotion type code of the response an actor is saying (GetDialogueEmotion):
    /// in a conversation, or the first response of a SayTo line; 0 (Neutral) otherwise
    /// [documented: GECK wiki GetDialogueEmotion, the codes; hypothesis: 0 when silent].
    pub fn dialogue_emotion(&self, actor: FormId) -> u8 {
        let data = self.data;
        if let Some(l) = self.dialogue.as_ref().filter(|c| c.actor == actor).and_then(|c| c.line.as_ref()) {
            if let Some(def) = data.infos.get(&l.info) {
                return def.response_order().get(l.response).map_or(0, |&r| def.responses[r].emotion);
            }
        }
        self.speech
            .iter()
            .find(|s| s.speaker == actor)
            .and_then(|s| data.infos.get(&s.info))
            .and_then(|def| def.response_order().first().map(|&r| def.responses[r].emotion))
            .unwrap_or(0)
    }
}
