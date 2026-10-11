# Spec note: dialogue topics, lines and conversations

Clean-room notes for Layer 6 dialogue, steps D1 and D2 of docs/design/dialogue.md
(2026-10-10). Code: `crates/fnv-game/src/data.rs` (`TopicDef`, `InfoDef`),
`crates/fnv-game/src/dialogue.rs` (conversations), `fnvtool dialogue`.

Sources and tags:
- **[documented: xEdit …]**: the record definitions of xEdit's `wbDefinitionsFNV.pas`
  (DIAL and INFO records, re-read 2026-10-10), as carried by `schemas/fnv/gameplay.json`.
- **[documented: page, owner export]**: a page of the owner's private GECK wiki export
  (`C:\fnv-private\geckwiki\`, never committed). Everything here is our own summary.
- **[owner statement 2026-10-10]**: the owner's rulings from long GECK use and play
  (which line is picked; then, the same day, that a conversation pauses the game, who
  can say a line, follow-ups and Random pools); not yet captured in game.
- **[observed: …]**: read from the ten masters (FalloutNV, the four add-ons,
  GunRunnersArsenal and the four packs), with the command that shows it. "The summary"
  means the `summary` of `fnvtool dialogue topics <Data> <ten masters> --schema
  schemas/fnv` or of `fnvtool dialogue lines …`. "Scratch parse" means a throw-away
  script over the DIAL groups of the same files (2026-10-10), not a committed tool.
- **[hypothesis]**: none of the above; each one is a capture question in §7.

Earlier notes this builds on: quests-dialogue.md §3–4 (the documented rules),
scripts.md §7.7 (SayTo and Say lines).

## 1. The topic record (DIAL)

A topic is a DIAL record. Its lines are the INFO records of the "topic children" group
(group type 7, labelled with the topic's FormID) that follows it
[documented: xEdit group types; observed: every INFO of the masters sits in such a
group, the summary's `with_topic` = `lines`].

### 1.1 Subrecords

| Sub | xEdit name | Layout | Meaning | Read into |
|---|---|---|---|---|
| EDID | Editor ID | string | e.g. `GREETING` | `TopicDef::edid` |
| QSTI | Added Quest → Quest | FormID (QUST), repeated | a quest that has lines in this topic [documented: xEdit DIAL "Added Quests"] | `quests` |
| INFC | Shared Info → Info Connection | FormID (INFO), after a QSTI | see §1.2 | `shared_infos` (raw) |
| INFX | Shared Info → Info Index | signed 32-bit, after its INFC | see §1.2 | `shared_infos` (raw) |
| QSTR | Removed Quest | FormID (QUST) | xEdit notes that New Vegas has none; none in the masters [observed: the summary's `subrecords`] | `removed_quests` |
| FULL | Name | string | the text the player sees for the topic when the line has no prompt [documented: Category:Dialogue, "Prompt"] | `name` |
| PNAM | Priority | float, xEdit default 50 | the topic's place in the list offered to the player; higher is nearer the top [documented: Category:Dialogue, "Priority"] | `priority` |
| TDUM | Dumb Response | string | not described in our sources beyond its name; not used | `dumb_response` |
| DATA | Data | type (u8), flags (u8, optional) | type: 0 Topic, 1 Conversation, 2 Combat, 3 Persuasion, 4 Detection, 5 Service, 6 Miscellaneous, 7 Radio (the quest window's tabs [documented: Category:Dialogue]); flags: bit 0 Rumors ("not used" [documented: Category:Dialogue]), bit 1 Top-level | `topic_type`, `flags` |
| INOM, INOA | INFO Order | FormID arrays | xEdit's own bookkeeping; absent in the masters | ignored |

[documented: xEdit wbDefinitionsFNV.pas DIAL record, for every row's name and layout.]
A topic defined by several plugins is read from its winning record (the last plugin's);
115 topics are defined more than once [observed: the summary's
`defined_in_several_plugins`]. `TopicDef::order` is where the topic is first defined
(plugin, record ordinal).

### 1.2 INFC and INFX ("Shared Infos")

xEdit lists INFC/INFX pairs under each added quest as "Shared Infos" ("Info
Connection", "Info Index") and marks them as not mattering for conflicts; it also notes
a few records that carry such pairs with no quest and absent FormIDs, "probably error in
GECK" [documented: xEdit DIAL record]. What the game does with them is **not
documented** in our sources. What the data shows:

- 2,435 topics carry 3,224 pairs; 18 pairs follow no QSTI; 3,206 name an existing line,
  and 3,031 of those name a line **stored under another topic** [observed: the
  summary's `shared_infos`].
- The line a pair names belongs to the quest the pair is listed under in all but one
  case; it is not explained by the line's choices, link-from or follow-up links (63
  of 3,906 pairs coincide with a choice or link-from link, 7 with a follow-up)
  [observed: scratch parse].
- Example: GREETING lists four lines that are stored under the topic `VMS07Greeting`,
  with indices 11–14 [observed: scratch parse].

The runtime keeps the pairs raw and does not use them. Whether a line named this way
also counts as a line of the listing topic (which would change who greets with what) is
[hypothesis: no] and capture question 9.

### 1.3 Counts [observed: the summary]

- 21,298 topics. By type: Topic 19,285, Conversation 1,724, Radio 219, Combat 43,
  Detection 12, Service 9, Miscellaneous 3, Persuasion 3.
- Top-level: 2,538 (2,536 of type Topic, 2 of type Conversation); the runtime offers
  the 2,536. Rumors: 1.
- With a name: 21,261. With a Dumb Response: 135.
- Priority: 20,073 at 50; the rest between 5 and 100 (190 at 100, 127 at 90, 97 at 80,
  95 at 60, 86 at 95).
- 14,608 topics list quests (15,853 entries). 9,304 topics have no line at all; 9,679
  have one, 1,460 two, 36 more than fifty. GREETING has 5,579 lines.
- Subrecords: of 21,560 DIAL records in the ten files every subrecord signature is read
  (`subrecords.DIAL.unread` is empty).

## 2. The line record (INFO)

### 2.1 Subrecords, in the order they appear in a record

| Sub | xEdit name | Layout | Meaning | Read into |
|---|---|---|---|---|
| DATA | Data | type (u8), next speaker (u8), flags 1 (u8), flags 2 (u8, optional) | type as DIAL's; next speaker 0 Target, 1 Self, 2 Either; flags below | `info_type`, `next_speaker`, `flags` |
| QSTI | Quest | FormID (QUST) | the quest the line belongs to | `quest` |
| TPIC | Previous Topic | FormID (DIAL) | xEdit: ignored by the editor for master files; absent in the masters | `previous_topic` |
| PNAM | Previous INFO | FormID (INFO) or 0 | the line above this one in the editor's list [documented: Category:Dialogue, "Edit Response Order"]; see §3.1 | `has_previous`, `previous` |
| NAME | Add Topics → Topic | FormID (DIAL), repeated | topics the line adds | `add_topics` |
| TRDT | Response Data | emotion type (u32), emotion value (i32), 4 unused, response number (u8), 3 unused, sound (FormID SOUN, optional), use emotion animation (u8, optional), 3 unused | one per response; emotion 0 Neutral, 1 Anger, 2 Disgust, 3 Fear, 4 Sad, 5 Happy, 6 Surprise, 7 Pained; the sound plays instead of the voice [documented: Category:Dialogue, "Sound"] | `Response` |
| NAM1 | Response Text | string | the subtitle; text in curly brackets is an acting note and is not shown [documented: Category:Dialogue, Notes] | `Response::text`, `response_chars` |
| NAM2 | Script Notes | string | acting notes | `Response::notes` |
| NAM3 | Edits | string | an editor comment; empty in all 37,447 cases | ignored |
| SNAM, LNAM | Speaker / Listener Animation | FormID (IDLE) | idles played during the response | `speaker_idle`, `listener_idle` |
| CTDA | Conditions | common condition | who can say the line and when (quests-dialogue.md §3) | `conditions` |
| TCLT | Link To → Topic | FormID (DIAL), repeated | the editor's "Choices": topics offered after this line | `choices` |
| TCLF | Link From → Topic | FormID (DIAL), repeated | see §3.5 | `link_from` |
| TCFU | Follow Up → Info | FormID (INFO), repeated | see §3.4 | `follow_ups` |
| SCHR … | Script (Begin) | embedded script | the begin result script | `begin` |
| NEXT | Marker | empty | separates the two scripts | – |
| SCHR … | Script (End) | embedded script | the end result script | `end` |
| SNDD | Unused | FormID (SOUN) | absent in the masters | ignored |
| RNAM | Prompt | string | shown to the player instead of the topic's name [documented: Category:Dialogue] | `prompt` |
| ANAM | Speaker | FormID (NPC_, CREA) | only this actor says the line; works like a GetIsID condition [documented: Category:Dialogue] | `speaker` |
| KNAM | ActorValue/Perk | FormID (AVIF, PERK) | the skill or perk shown with the choice; see §3.3 | `challenge_form` |
| DNAM | Speech Challenge | u32: 0 none, 1 Very Easy … 5 Very Hard | the difficulty of a Fallout 3 style speech challenge | `challenge_difficulty` |

[documented: xEdit wbDefinitionsFNV.pas INFO record, for names, layouts and order;
observed: the masters' records follow that order, scratch parse.] A line defined by
several plugins (37 lines) is read from its winning record and keeps the place of its
first definition.

Flags (`InfoDef::flags`: flags 1 in bits 0–7, flags 2 in bits 8–15, the combined value
the wiki's GetDialogueInfoFlags page lists) [documented: xEdit for the names;
Category:Dialogue for the meanings]:

| Bit | Name | Meaning | Lines |
|---|---|---|---|
| 0x0001 | Goodbye | the actor leaves dialogue after the line | 9,828 |
| 0x0002 | Random | §4 rule 3 | 8,001 |
| 0x0004 | Say Once | an actor says the line only once; another actor may still say it | 4,434 |
| 0x0008 | Run Immediately | the results run when the conversation is put together, not when the line is spoken | 765 |
| 0x0010 | Info Refusal | the player's prompt is not shaded after use | 80 |
| 0x0020 | Random End | closes a Random stack | 1,143 |
| 0x0040 | Run for Rumors | "not used" | 0 |
| 0x0080 | Speech Challenge | a speech challenge (§3.3) | 3 |
| 0x0100 | Say Once a Day | once per actor per `fSayOncePerDayInfoTimer` (12 game hours) | 194 |
| 0x0200 | Always Darken | the prompt is always shaded | 6 |
| 0x0400, 0x0800 | Unknown 2, 3 | not documented | 0 |
| 0x1000 | Low Intelligence | named by xEdit only; meaning not documented, not used | 33 |
| 0x2000 | High Intelligence | as above | 22 |

### 2.2 Counts [observed: the summary of `fnvtool dialogue lines`]

- 28,896 lines (28,933 INFO records in the ten files; 37 are overridden). By the plugin
  that first defines them: FalloutNV 23,247, DeadMoney 2,126, OldWorldBlues 1,620,
  HonestHearts 1,236, LonesomeRoad 667.
- By type: Topic 21,831, Conversation 2,603, Combat 2,300, Detection 1,349, Radio 769,
  Miscellaneous 39, Service 5. 954 lines have another type than their topic (613 Topic
  lines in Conversation topics, 167 Combat lines in Detection topics, 74 Conversation
  lines in Topic topics, 42 Topic and 40 Conversation lines in Radio topics, 16
  Detection lines in Combat topics, 2 Radio lines in Topic topics).
- Every line names a quest. 15,683 have a fixed speaker; 27,813 have conditions
  (69,277 in all); 3,336 have a begin script with code and 5,303 an end script.
- Responses: 37,403. Per line: none 9, one 23,531, two 3,205, three 1,578, four 375,
  five 127, six to nine 54, ten or more 17. 228 responses name a sound, 1,684 a speaker
  idle, 1 a listener idle, 1,747 have acting notes, 40 an empty text. Emotions: Neutral
  28,608, Anger 2,660, Happy 2,158, Disgust 973, Surprise 947, Sad 859, Fear 773,
  Pained 425.
- Response numbers: 874 lines are not numbered 1…n, and 583 store their responses out
  of ascending number order. The runtime says responses in number order
  [documented: xEdit "Response number" for the field; hypothesis that the number
  decides where it differs from the stored order].
- 1,707 lines have a prompt. 8,114 have choices (28,215 links); 924 have link-from
  topics (1,293 links); 1,759 have follow-ups (2,477 links); 472 add topics (827
  links). 15 lines have both follow-ups and choices.
- 1,499 lines name an actor value or perk (1,391 AVIF, 108 PERK); 3 have a difficulty
  (all "Hard"), the same 3 that have the Speech Challenge flag.
- Next speaker: Target 28,685, Self 211.
- Record header flag 0x2000 is set on 7,631 lines and nothing else is; what it means on
  an INFO is not documented, and it is not used.
- Of 28,933 INFO records every subrecord signature is read or deliberately ignored
  (NAM3); `subrecords.INFO.unread` is empty.

## 3. What the data shows about structure

### 3.1 The stored order of a topic's lines

The editor keeps a topic's lines in a list and evaluates them from the top; moving a
line writes its "Previous INFO" (PNAM) [documented: Category:Dialogue, "Edit Response
Order"].

- FalloutNV.esm's 23,247 lines have **no PNAM**; every line of the add-ons has one
  [observed: the summary's `previous_info`: `absent` 23,210 winning lines; scratch
  parse per plugin].
- Where PNAM exists it describes the order **within one quest** inside the topic: 3,039
  lines have PNAM 0 and are the first line of their quest in the topic; 2,638 name the
  line stored just before them among their quest's lines; 9 differ (6 have 0 without
  being first, 2 name another line of their quest, 1 names a line elsewhere)
  [observed: the summary's `previous_info`].
- Two add-on lines that override FalloutNV lines name as previous the line stored before
  them in FalloutNV.esm [observed: scratch parse, lines 0x00120F26 and 0x0014F170].

So file order agrees with the previous-info links in 5,677 of 5,686 cases, and is the
only order FalloutNV.esm has. **The runtime's stored order of a topic** is: the lines in
the file order of the plugin that first defines each, plugins in load order
(`GameData::topics`) [observed as above for one quest's lines; hypothesis for the 9
exceptions and for how lines of different quests interleave in the editor's list,
which §4 makes mostly irrelevant].

### 3.2 Lines of several quests in one topic

Only 102 topics have lines of more than one quest. In 99 of them one actor could in
principle say lines of quests with different priorities (same fixed speaker, or no
fixed speaker on one side), and in 76 of quests with the same priority (74 both)
[observed: the summary's `quest_mix`]. GREETING alone has lines of 228 quests; the
others are mostly the player-action topics (PLAYERFIREWEAPON and the like) and shared
service topics. So rule 2 of §4 (equal priority) matters for 76 topics, GREETING
foremost.

### 3.3 Skill and perk lines (KNAM)

- The Speech Challenge flag with a difficulty appears on 3 lines (one Fallout 3
  left-over topic). New Vegas's skill checks are the 1,499 lines with KNAM and no flag
  [observed: the summary].
- They come as pairs in one topic: of the topics that have such lines, 488 have exactly
  two lines, both with KNAM, and 380 have one line; the two lines of a pair differ in
  one condition, `GetActorValue <skill> >= N` against `< N`, run on the target (the
  player) [observed: scratch parse; e.g. topic 0x00100A60].
- The wiki says the field controls only what is shown; whether the line is available
  is up to its conditions; New Vegas shows the needed level, or "have/need" when the
  player falls short and a second line exists for that [documented: Category:Dialogue,
  "AV/Perk"].

Runtime: the pass or fail line is simply the line of the topic the actor can say
(§4); the choice carries the tag (`Challenge`: the form, the difficulty, and the needed
and current level taken from the line's first GetActorValue-family condition run on
the target) [hypothesis for where the game takes the two numbers from].

### 3.4 Follow-up lines (TCFU)

Not described in our sources beyond xEdit's name "Follow Up" and its target type INFO.
The data: a line with follow-ups has no choices (15 exceptions in 1,759); follow-up
targets have no prompt (25 exceptions in 2,477 links) and are stored under the same
topic (1,029) or another (1,448) [observed: the summary; scratch parse]. Doc Mitchell's
farewell is a chain of them: the greeting → the Pip-Boy line → one of two suit lines
(they differ by a condition) → choices → answer → the Goodbye line [observed:
`fnvtool dialogue lines … --topic VCG01DocMitchellTopic076`].

Runtime: when a line with follow-ups ends, the first follow-up the actor can say is
said next, without a choice by the player [owner statement 2026-10-10: follow-ups have
no player response, as read here from Doc Mitchell's lines; not yet captured;
hypothesis: the first sayable one when a line lists several].

### 3.5 Link From (TCLF)

The wiki: lets many topics link to this one [documented: Category:Dialogue, "Edit Topic
Links"]; for conversations between NPCs it chains HELLO → … → GOODBYE [documented:
Conversation Tab]. In topics of type Topic, 170 of 185 link-from links repeat a choice
link the other way round (the topic named already lists this one as a choice)
[observed: scratch parse]. Runtime: read, not used [hypothesis: in player dialogue the
choices alone decide; capture question 10].

### 3.6 Run Immediately

All 765 lines with the flag are Radio (673) or Conversation (92) lines; no Topic line
has it [observed: scratch parse; the real-data test asserts it].

## 4. Which line an actor says

One rule for greetings, chosen topics and the script functions SayTo and Say (`Game::select_line`, `Game::can_say`):

1. **Quest priority decides between quests.** The topic's lines are taken with the lines
   of higher-priority quests first [documented: Quest Data Tab, owner export: priority
   decides whose dialogue wins, higher wins; owner statement 2026-10-10].
2. **Equal priority: stored order.** Lines of quests with the same priority keep the
   stored order of §3.1, "the first in the array" [owner statement 2026-10-10, from GECK
   use; not yet captured].
3. **Inside one quest: first eligible line from the top**, unless lines carry the
   Random flag [owner statement 2026-10-10, from GECK use; not yet captured; observed
   for SayTo: Doc Mitchell's VCG01Intro lines, layer5-captures.md §10.3].
4. **Random pools.** Random lines that sit together in the list form a pool. A line
   without the flag between two groups of Random lines splits them into two pools,
   whether or not that line can be said [owner statement 2026-10-10: "random pools have
   to be together; a non random line between 2 groups of randoms will split the pool
   into 2 separates"; not yet captured]. A Random End line closes its pool at that line
   [documented: Category:Dialogue, flag Random End; hypothesis that it also does when
   the actor cannot say that line]. Going down the list, a pool is taken if the actor
   can say any of its lines, and one of those is then picked at random [documented:
   Category:Dialogue, flag Random, for the pick]. This settles the wiki's contradiction
   (its flag description lets a qualifying plain line end the pool, its in-depth
   example 3 lets the pool run across a plain line) in favour of neither: the pool is a
   matter of where the lines sit, not of which qualify. Before this ruling the runtime
   skipped a plain line the actor could not say and let the pool run on across it.

Implementation: a stable sort of the stored order by quest priority, highest first;
then down the list over items, an item being a plain line or a pool; the first item
the speaker can say. A pool is a run in that sorted order, so lines of two quests of
equal priority that sit together can share a pool [hypothesis]. The Random pick uses
the game's seeded generator, and only when more than one line of the pool can be
said.

A speaker **can say** a line when:
- it has not said it, for Say Once; not within the last `fSayOncePerDayInfoTimer` game
  hours (12 when the setting is absent), for Say Once a Day; both per actor
  [documented: Category:Dialogue]. The clock is GameDaysPassed × 24 + GameHour
  [hypothesis];
- it is the line's fixed speaker, if the line has one [documented: Category:Dialogue];
- the line's quest is running **and the quest's own conditions pass**
  [owner statement 2026-10-10: "every actor has the line as long as the quest is active
  and the conditions are met on both the topic and the quest itself"; not yet captured;
  documented: Quest Data Tab: a quest's conditions apply to all of its dialogue and are
  checked before the lines' own]. A line of a stopped quest cannot be said. The
  runtime evaluated the quest's conditions (the QUST's CTDA list before its stages)
  from the start; they run like the line's, speaker as subject and listener as target
  [hypothesis: the page does not say on whom]. 113 of the 640 quests carry conditions
  (289 in all); 101 of them have lines, 8,151 lines in all [observed: the lines
  summary's `quest_conditions`];
- the line's conditions pass, with the speaker as subject and the listener as target
  [documented: Category:Conditions]. "Conditions on the topic" are these: the editor
  lists a line's conditions under its topic, and a DIAL record has no condition
  subrecords of its own [observed: no CTDA among the DIAL subrecords, the topics
  summary's `subrecords`].

Pointer for the AI package work: the owner states that an actor's package is chosen the
same way, the first eligible one from the top of its list [owner statement 2026-10-10];
`Game::select_package` already does that (scripts.md §7.5).

## 5. Conversations (`crate::dialogue`)

State: `Game::dialogue` (`Conversation`: the actor, the line being said with the
response and its remaining time, or the choices offered), `Game::said` (lines said, per
actor), `Game::said_day`, `PlayerState::topics` (known topics), `RefExtra::talked_to_pc`.
All of it is in `GameSnapshot`.

- **Start** (`Game::start_dialogue(actor, topic)`). The actor says the line of GREETING
  it can say (§4), or of the named topic [documented: Topics Tab: GREETING is what an
  actor says when spoken to; StartConversation: a named topic replaces the greeting].
  An actor with no such line cannot be talked to [hypothesis]; nor a dead, disabled or
  deleted one. One conversation at a time [hypothesis].
- **A greeting with the Goodbye flag starts no conversation.** When the line picked from
  GREETING carries the flag, the actor says it at the player and that is all: no
  dialogue menu, no pause, nothing to choose or to leave, and the player is not held
  [owner statement 2026-10-10: a GREETING line marked Goodbye "does not draw the player
  into a dialogue exchange, it just is spoken at the player without locking them in";
  not yet captured]. 3,023 of GREETING's 5,579 lines carry the flag (2,972 of them
  first defined in FalloutNV), so this is the usual answer of an actor with nothing
  particular to say; 73 of them have a begin result, 150 an end result, 3 an Add Topics
  list, 99 Say Once, none a follow-up [observed: `fnvtool dialogue lines … --topic
  GREETING`, counted over `lines`]. In the runtime the line is a `Speech`
  (`Speech::greeting`), said like a SayTo line to the player (`Game::say_line`):
  `Game::dialogue` stays empty, the begin result runs at once and the end result when
  the line is over, in game time, and the trace has `line_started` / `line_ended` but
  no `dialogue_started`. What a conversation of that one line did is kept: the Add
  Topics list is applied when the line ends and the actor counts as having talked to
  the player (GetTalkedToPC); no SayToDone block runs [hypothesis for the three].
  Talking to the actor again while it speaks cuts the line (no end result) and starts
  the greeting afresh [hypothesis, as for a second SayTo]. A Goodbye line of a topic
  named by StartConversation still opens a conversation that ends with it [hypothesis:
  the statement is about GREETING]. StartConversation without a topic is the greeting,
  so the rule applies to it.
- **A conversation pauses the game** behind it; lines spoken outside one (SayTo, Say)
  do not [owner statement 2026-10-10: Doc Mitchell's Rorschach test, "a back and forth
  dialogue with chosen options from the player", is what makes "the game … pause in
  the background", unlike his earlier spoken lines; not yet captured]. §6 says what
  that means for scripts. The conversation's own lines go on in real time.
- **A line.** The begin result script runs when the line starts, the end result script
  when its last response is over [documented: Category:Dialogue]. Responses follow each
  other in number order; each lasts `Game::response_seconds` of real time (the one place a response
  is timed: the length of its voice file in the actor's voice, or the fitted 1.1 s +
  1/16 s per character when it has none or the files are not given,
  docs/spec-notes/voice.md §8). Result scripts run on the actor with the line's quest. With Run
  Immediately both scripts run when the line is picked [documented flag; hypothesis
  that for a line the player asked for "when the conversation is generated" is that
  moment; no Topic line of the masters has the flag].
- **When a line ends**, in this order: end result; the line's Add Topics join the known
  topics [documented: xEdit "Add Topics"; hypothesis on the moment and that they stay
  known afterwards]; a Goodbye line ends the conversation [documented]; else the first
  follow-up the actor can say is said (§3.4) [owner statement]; else choices are offered;
  with nothing to offer the conversation ends [hypothesis].
- **Choices** (`Game::build_choices`). After a line with choices: those topics and
  nothing else. Otherwise the top-level topics plus the known topics
  [documented: Category:Dialogue, "Top-level"; Quest and Dialogue Tutorial]. A topic is
  offered only if the actor has a line of it to say now, and that line is the one said
  if it is chosen [hypothesis; the editor's filtered dialogue window lists only what an
  actor qualifies to say]. If none of a line's choices can be said, the topic list is
  offered instead [hypothesis]. Order: topic priority (DIAL PNAM), highest first
  [documented]; then, for a line's choices, their order in the record [documented:
  "Sorting order of links"], and for the topic list the topics' record order
  [hypothesis]. Quest priority plays no part in the order of the list.
- **The text of a choice** is the line's prompt, else the topic's name [documented].
  It is shaded when the actor has said that line before, unless the line has Info
  Refusal; always with Always Darken [documented: Category:Dialogue flags].
- **GOODBYE.** When the topic list holds no Goodbye line, the actor's line of the
  GOODBYE topic is offered last, if it has one [documented: Quest and Dialogue Tutorial;
  Conversation Tab].
- **Known topics.** `AddTopic` and lines' Add Topics lists fill one set, kept for the
  whole game; every actor that can say a line of a known topic offers it [owner
  statement 2026-10-10: every actor has the line while its quest is active and the
  conditions are met; not yet captured]. When an Add Topics list is applied (here: when
  the line ends) stays a [hypothesis].
- **Leaving** (`Game::end_dialogue`). The line being said is cut without its end result
  [hypothesis]. When the game lets the player leave is the screen's business (D4:
  docs/design/dialogue-play.md §2).
- **Skipping** (`Game::skip_response`, added for the screen). The response being said
  ends at once: the next response of the line starts, and after the last one the line
  ends exactly as a line said to the end (end result, Add Topics, Goodbye, follow-ups,
  choices) [hypothesis: skipping only shortens a line; the other reading, a skipped
  line running no end script as a cut line does, would stop every quest whose stages
  are set by end scripts whenever the player clicks through; capture question §7.8].
  Nothing is skipped while a menu on top of the conversation holds it. No trace event
  of its own.
- **Not modelled:** IdleChatter (hellos and random conversations are §5.2, scripted
  conversations between actors §5.1), radio, combat and detection barks, the reaction
  topics (PLAYERFIREWEAPON and the rest), Low/High Intelligence lines and
  the Dumb Response, the quest flag "Allow Repeated Conversation Topics", the Fallout 3
  speech-challenge roll and the SpeechChallengeFailure topic, lip sync and emotions on
  faces.

### 5.1 Conversations between two actors (`Game::start_actor_conversation`)

`speaker.StartConversation target [topic]` with an actor as the target makes the two
talk to each other [documented: GECK wiki StartConversation: it "forces a conversation
between two NPCs, optionally starting with a specified topic"]. The masters call it 104
times: 83 with the player as the target, 21 (in 16 script units) with another actor
[observed: `fnvtool script decompile` over FalloutNV.esm]. One of them is Goodsprings'
Joe Cobb and Trudy (below).

- **What a conversation is**: "a series of infos that are played between two actors";
  each line's Link To topics (TCLT, the choices of player dialogue) give the next one,
  and with several Link To topics "the game [selects] one … at random"; a line with the
  Goodbye flag ends it; "infos in conversations cannot have more than one response.
  Extra responses are ignored" [documented: Conversation Tab].
- **Start.** The speaker says the line of `topic` it can say with the target as the
  conditions' target (§4); HELLO without a topic [hypothesis, from the Conversation
  Tab: random conversations start with HELLO]. Nothing starts when either side is
  disabled [documented: StartConversation], dead or deleted, already in a conversation
  (with the player or another actor), or when the speaker has no line [hypothesis for
  these]. In the game the speaker first goes to the target ("no matter where the two
  NPCs are currently"); the runtime starts at once [stand-in, as for
  `StartConversation player`]. For Joe Cobb and Trudy no walk is needed: Joe Cobb
  starts next to Trudy in all base-game footage the owner has seen [owner statement
  2026-10-11]. The location references, head-track and allow-movement parameters are
  not read.
- **The lines** are said in the world as SayTo lines are (`Game::speech`,
  `Speech::conversation`): no menu, no pause, the player keeps control and overhears
  them [hypothesis]; the listener is the other actor (its listener idle plays); begin
  result at the start, end result at the end, in game time; only the first response in
  response order is said and timed [documented, above]; Say Once is kept as for SayTo.
  No SayToDone block runs [hypothesis].
- **The next line** (`Game::next_conversation_line`), when a line has ended and its end
  result has run: a Goodbye line ends the conversation [documented]; so does either
  side being gone [hypothesis]. Otherwise the line's next speaker says the next line:
  Target is the other actor, Self the same one, Either the other and else the same one
  [documented: xEdit wbNextSpeaker, the names; hypothesis: what they mean]. It says the
  first follow-up (TCFU) it can say to the other [hypothesis, as in player dialogue],
  else a line of one of the Link To topics, at random among the topics it has a line of
  [documented: the random pick; hypothesis: topics with no line are passed over]. With
  nothing to say the conversation ends; no GOODBYE line is added [owner statement
  2026-10-11: no goodbye is heard after Trudy's line, which links nowhere and has no
  Goodbye flag, in the footage of Joe Cobb and Trudy]. The Conversation Tab says
  "all random conversations will end with a GOODBYE unless they are ended by another
  info with the 'Goodbye' flag"; its own example reaches GOODBYE through a Link To, and
  the scene above is scripted, not random. In FalloutNV.esm 9 Conversation lines link
  to GOODBYE, and of the 1,965 Conversation lines that link nowhere (no Link To, no
  follow-up) 959 carry the Goodbye flag [observed: `fnvtool dialogue lines --all`]. So
  whether random conversations (not modelled) get a GOODBYE added stays open.
- **Cut.** A line cut off (its speaker starts another line, is disabled or dies) ends
  the conversation without its end result [hypothesis, as for SayTo].
- **Trace**: `actor_conversation_started` {speaker, target, topic}, then `line_started`
  (with `target`) / `line_ended` per line, and `actor_conversation_ended` {speaker,
  target (of the last line), reason `goodbye` | `no_links` | `cut` | `actor_gone`,
  lines}.

**Joe Cobb and Trudy** [observed: FalloutNV.esm, `fnvtool script decompile`, `fnvtool
dialogue lines --topic GSJoeCobbIntro`, `fnvtool esm decode`]. The end result of
INFO 0015F8B6 enables TrudyRef (00104C6D), GSJoeCobbRef (00104C68) and the trigger
volume 0010521E in GSProspectorSaloonInterior (three trigger scripts do the same in
other orders of events). The trigger's script GSJoeCobbTriggerScript (SCPT 0010521F),
on the player's OnTriggerEnter while Joe Cobb should not attack the player, calls
`GSJoeCobbRef.StartConversation TrudyRef, GSJoeCobbIntro GSJoeCobbRef TrudyRef 0 0`, sets
VMS16.bMetJoeCobb and disables and deletes itself. GSJoeCobbIntro (DIAL 00104DE7, type
Conversation, quest VFreeformGoodsprings) has one line (INFO 00105219: Joe Cobb to
Trudy, one response, next speaker Target, Link To GSTrudyToJoe); GSTrudyToJoe has one
line (INFO 00105218: Trudy, one response, no links) whose end result sets
VFreeformGoodsprings.bArgumentOver and re-evaluates Joe Cobb's packages; the quest's
script then gives him script package 0010663E. Headless (`fnvtool script run`: the
three references enabled, the trigger entered) the runtime plays this through: Joe
Cobb's line (8.80 s, its voice file's length), Trudy's answer (3.85 s), bArgumentOver 1,
package 0010663E chosen, the conversation ended `no_links` after 2 lines. Before this
section StartConversation with an actor was a stub, so the argument was never heard.

### 5.2 Actors who talk on their own (`fnv_ai::social`)

Two behaviours, each allowed by the actor's current package ("Enable Fallout Behavior":
"Hellos to player: Allow the actor to say hello to the player"; "Random conversations:
Allow the actor to participate in random conversations. (The packages that can initiate
random conversations are Sandbox, Guard, Eat, and Patrol.)" [documented:
Category:Packages]). Which packages allow them: packages.md §1, general bit 12 — without
it the default allows both [hypothesis, from the data and the owner's Vault 3
statement].

**The topic is HELLO, not GREETING.** GREETING is "the default greeting an actor will use
when spoken to" [documented: Topics Tab]; HELLO is the Conversation-tab topic that
starts conversations between NPCs [documented: Conversation Tab; Quest and Dialogue
Tutorial]. FalloutNV.esm's HELLO holds lines for both listeners [observed: `fnvtool
dialogue lines --all`]: 1,074 lines (569 of type Topic, 505 Conversation; 1,014 Random,
659 Goodbye; conditions mostly GetIsVoiceType, GetInFaction, GetGlobalValue, GetIsID; on
the target GetInFaction 89, GetIsID 108, GetIsRace 32, GetIsSex 26). Some are for the
player (Victor's "Howdy" line in VFreeformGoodsprings; Raul's lines with `target.GetIsID
Player`), some for another actor. 1,047 are one line; 27 link on (Link To) into
exchanges of 2 to 6 lines, e.g. GenericFiend's (two to four lines between Fiends) and
a five-line exchange at Nellis. 1,065 have Link From blank, so can start one
[documented: "The starting HELLO must have 'Link From' left blank"].

- **A hello to the player** (`Game::say_hello`): the actor's HELLO line for the player as
  the conditions' target, Link From blank, said in the world (`LineKind::Hello`): no
  menu, no SayToDone, no Add Topics [hypothesis for these]. Lines that open an exchange
  between actors are said to the player too, with no reply: "these lines can be spoken
  at the player in base game just with no reply if it doesn't hook into a back and
  forth; the fiend just goes back to what he was doing" [owner statement 2026-10-11].
  In the engine in Vault 3 (Vault3a, 200 s, the player put by a Fiend) 6 hellos were
  said, among them GenericFiend's openers 00175369 and 0017536A, and one random
  conversation started (a one-line remark, 00175374, which links nowhere). When: the player within 300 units; again only
  after the player has gone beyond 600; one hello at a time [stand-ins: no distance or
  timer for hellos is documented, and none is set in the masters].
- **A random conversation** (`Game::start_actor_conversation` with HELLO, then §5.1):
  an actor whose package may start one (Sandbox, Guard, Eat, Patrol; the behaviour
  allowed; not "No Conversation") looks for a partner after a time drawn between
  `fAISocialTimerForConversationsMin` and `…Max` (30 and 60 s), and starts with a chance
  of `fAISocialchanceForConversation` per cent (`…Interior` indoors; 25 both)
  [observed: the values in FalloutNV.esm; hypothesis: what the settings mean, from
  their names, which the wiki only lists]. The partner is the nearest actor whose
  package lets it take part and to whom the starter has a HELLO line: within
  `fAISocialRadiusToTriggerConversationInterior` (1000) indoors,
  `fAISocialRadiusToTriggerConversation` outdoors (not in the masters: the indoor value
  stands in), and within 384 units [stand-in: in the game they go up to each other,
  "Conversation -- no duration, the behavior is done when the conversation is finished"
  (Sandbox Package); here both must already stand close]. While they talk neither walks
  on (the engine holds both) [hypothesis].
- **Nobody talks** while asleep (a Sleep package), on its way, already talking, carrying
  out a Dialogue package, dead or disabled, or while a menu or a conversation with the
  player holds the game [hypothesis].
- **Turning**: the one saying a hello turns to the player, and both of a conversation
  between actors to each other, unless on furniture or on their way [hypothesis].
- **Log**: `ai [hh:mm] <actor>: says hello to the player from N units, line …`,
  `… starts a conversation with <partner> from N units, line …`, and at exit
  `social summary: N hellos said to the player, M random conversations started`.

## 6. What scripts see

| Function | During a conversation | Source |
|---|---|---|
| `IsTalking` | 1 while the actor says a response; 0 while the player chooses | [documented: IsTalking for "currently talking"; hypothesis for the choosing phase] |
| `MenuMode`, `MenuMode 1009` | 1 from the greeting to the end (0 throughout for a greeting with the Goodbye flag: no conversation, §5) | [owner statement 2026-10-10: the conversation pauses the game; documented: GetMenuTargetRef names menu 1009's target as the actor spoken to] |
| GameMode blocks, game time (`Game::time`, GetSecondsPassed), GameMode quest delays | stopped: **the conversation is menu mode** (`Game::in_menu`), by the rules observed for the other menus (scripts.md §7.2, §7.7) | [owner statement for the pause; the details are the existing menu rules] |
| `Begin MenuMode` and `Begin MenuMode 1009` blocks | run, with the MenuMode quest delay counter | [the existing menu rules; hypothesis for this menu] |
| Magic effects, SayTo and Say lines of other actors | stand still, as in any menu | [the existing menu rules; hypothesis] |
| A GameMode script that starts the conversation (StartConversation) | the rest of that frame's GameMode scripts wait until it is over | [the existing rule for a menu opened during GameMode; hypothesis] |
| `GetTalkedToPC` | 1 once a conversation with the player has started; SayTo lines do not count | [documented: GetTalkedToPC; hypothesis on SayTo] |
| `GetDialogueEmotion` | the emotion type of the response being said, else 0 | [documented: GetDialogueEmotion for the codes] |
| `StartConversation player [topic]` | starts the conversation at once; ignored while one runs or when the actor has no line; the walk to the player needs AI packages | [documented: StartConversation; stand-in for the walk] |
| `StartConversation <npc> [topic]` | the two actors talk to each other in the world (§5.1): no menu, the player overhears; `IsTalking` is 1 for the one saying a line | [documented: StartConversation, Conversation Tab; hypotheses in §5.1] |
| `AddTopic` | the topic joins the known topics (trace `topic_added`) | [documented: catalogue; hypothesis, §5] |
| `GetIsCurrentPackage` | unchanged. Dialogue packages are carried out by `fnv_ai` for actors handed to it (docs/design/actor-behaviour.md §5.6): the actor goes up to the player and the conversation starts as with `StartConversation player` | [documented: GECK wiki, Dialogue Package] |

`Game::menu` holds the other menus and stays empty for a conversation: the conversation
itself is the menu (`Game::in_menu()`, `Game::open_menu_code()` 1009,
`Game::menu_open(code)`). A menu opened during a conversation (a message box from a
result script) opens on top of it at once and holds the conversation's lines until it
is closed; `MenuMode 1009` stays 1 under it [hypothesis].

**The Lucky 38 elevator Securitron (SCPT 0x0015725D)**, which the first version of this
note read as evidence against a pause, fits a pausing menu better. Its OnActivate adds
a dialogue package and sets a counter to 1; a `Begin MenuMode 1009` block moves the
counter from 1 to 2; its GameMode block, when the counter is 2 and `MenuMode 1009` is
not 1, shows the floor menu and resets the counter [observed: the script's compiled
blocks, `fnvtool script decompile --form 0015725D`]. So the script marks "the
conversation is on" from menu mode and acts in GameMode once it is over, which is what
a script does when GameMode stands still during dialogue. With a pause the extra
`MenuMode 1009` test inside GameMode can never be false; why it is there remains
unexplained (a guard for the frame in which the menu closes, or plain caution). In the
runtime both blocks now run as that reading expects.
Still stubs: `IsActorTalkingThroughActivator` (talking activators), `GetDisposition`,
`SetDisposition`, `Exists`.

## 7. What needs in-game captures

**To confirm** (owner statements of 2026-10-10, the rules the runtime follows):

- A. Greeting choice when lines of several quests qualify: rules 1–3 of §4, including
  equal priorities and the stored order of §3.1.
- B. A conversation with the player pauses the game (GameMode, game time); SayTo and Say
  lines do not (§5, §6).
- C. A line needs its quest running and the quest's conditions as well as its own; a
  line of a stopped quest cannot be said (§4).
- D. Known topics are global: every actor that can say a line of one offers it (§5).
- E. Follow-up lines are said next with no player choice (§3.4).
- F. Random pools are runs of Random lines that sit together; a plain line between two
  groups splits them, sayable or not (§4 rule 4).
- G. A GREETING line with the Goodbye flag is said at the player without a conversation
  (§5): no dialogue menu, the player keeps control, `MenuMode 1009` stays 0.

**Open hypotheses** (numbers kept from the first version of this note; 2 and 4 moved
up to C and F):

1. (moved to A)
2. (moved to C)
3. Whether an actor with no qualifying greeting can be activated at all.
4. Random pools, what the ruling leaves open: a Random End line the actor cannot say
   (does it still close the pool?), and a pool made of lines of two quests.
5. Which topics are offered and in what order: hidden when the actor has no line; ties
   of equal topic priority; the fallback to the topic list when no choice of a line can
   be said; the conversation ending when nothing can be offered; GOODBYE appended.
6. Known topics: permanent for the whole game? When is an Add Topics list applied
   (line start or end)? (That they are offered by every actor: D.)
7. Follow-up lines: the first qualifying of several (that they need no choice: E).
8. Timing of the begin and end results against the sound; the end result when the
   player skips a line or leaves; Run Immediately on a chosen line.
9. INFC/INFX: does a "shared" line count as a line of the listing topic?
10. Link From in player dialogue: does it add a choice?
11. The skill tag's numbers: taken from the line's condition? What is shown for a perk?
12. Inside the pause (B): what `IsTalking` reads while the player chooses;
    `Begin MenuMode 1009` blocks and the MenuMode quest delay; whether magic effects
    and other actors' SayTo lines stand still; a message box over a conversation; the
    quest conditions' subject and target (C).
13. A greeting with the Goodbye flag (G), what the statement leaves open: GetTalkedToPC
    afterwards; whether its Add Topics list is applied; SayToDone; activating the actor
    again while it speaks (cut and restart, or ignored); and whether a Goodbye line of
    a topic named by StartConversation opens the menu.
13. Say Once across two actors sharing a line; Say Once a Day's clock; Say Once and
    Say Once a Day counted from the line's start or end.
14. Response order where numbers and stored order disagree.
15. Whether SayTo and Say obey Random stacks like menu dialogue.
16. Conversations between actors (§5.1): the walk to the target before the first line
    (not needed for Joe Cobb and Trudy, owner statement); whether the player is free
    and the game runs on during them; what Target, Self and Either mean; follow-ups in
    them; whether random conversations get a GOODBYE line added (a scripted one does
    not, owner statement); HELLO when no topic is named; SayToDone after their lines;
    refusal while either actor is talking.
17. Hellos and random conversations (§5.2): the hello distance and when an actor says
    hello again; whether the actor turns to the player; the meaning of the
    fAISocial… settings (timer per actor? chance per cent?) and the outdoor radius;
    the walk of the two towards each other; whether the player's presence matters for
    random conversations; general bit 12 as the switch between the default and the
    package's own Fallout behaviour (packages.md §1).

## 8. Tool contracts

`fnvtool dialogue <cmd> <Data> <plugins…> --schema schemas/fnv [--functions <file>]`.
Texts are game data: listings give lengths; `--text` adds the texts for reading at
home (output with texts is never committed).

**`dialogue topics [--quest Q] [--topic T] [--speaker A] [--top-level] [--type T]
[--all] [--text]` → `fnv.dialogue.topics/1`**: `load_order`; `summary` (the counts of
§1.3, `quest_mix` of §3.2, `subrecords`: per record type the signatures read, ignored
and unread over every plugin); `listed`; `topics`: for each selected topic `form`,
`edid`, `type`, `flags`, `priority`, `name_chars` (`name` with `--text`), `quests`,
`lines`, `line_quests`, `shared_infos`, `definitions`; with `--topic` also
`line_forms` (the order lines are tried in), `shared_info_list` and `removed_quests`.
Without a filter or `--all` only the summary is printed. `--quest` and `--speaker`
select topics that have such lines.

**`dialogue lines [--quest Q] [--topic T] [--speaker A] [--info <hex>] [--all]
[--text]` → `fnv.dialogue.lines/1`**: `summary` (the counts of §2.2,
`previous_info` of §3.1 and `quest_conditions` of §4); `lines`, topics by FormID and within a topic in the order
lines are tried: `form`, `plugin`, `definitions`, `topic`, `position`, `quest`,
`quest_priority`, `type`, `next_speaker`, `flags`, `record_flags`, `speaker`,
`prompt_chars` (`prompt`), `responses` in the order said (`number`, `chars`, `emotion`,
`emotion_value`, `sound`, idles, `seconds_fitted`; `text`, `notes`), `conditions`
(function, arguments, comparison, value, OR, run-on), `begin` / `end` (unit keys),
`choices`, `link_from`, `follow_ups`, `add_topics`, `challenge`, `previous`
(`"absent"`, null or a FormID). `--speaker` takes an actor base or a placed reference
and selects lines whose fixed speaker it is.

**`dialogue talk --actions <file> [--text]`: `fnv.dialogue.actions/1` →
`fnv.dialogue.trace/1`.** The input has the fields of `fnv.script.actions/1`
(scripts.md §8): `seed`, `frame_seconds`, `new_game`, `given` (the game state the
conversation runs on) and `actions`, any of that contract's operations plus:

| op | does |
|---|---|
| `talk {actor, topic?, label?}` | `Game::start_dialogue`: the player activating the actor, or the actor's dialogue package; observation `info` |
| `choose {topic \| prompt \| index, max_frames?, label?}` | waits until the actor has finished speaking, then picks the entry by topic (Editor ID or FormID), by its text (ignoring case; a part of it if no entry matches whole) or by 0-based index; observation `info` |
| `wait_talk {start?, max_frames?, label?}` | waits until choices are offered or the conversation is over; with `start`, first until one is running (a script's StartConversation); observations `state` (`choosing`, `ended`), `choices`, `actor` |
| `end_talk {}` | `Game::end_dialogue` |

`max_frames` defaults to 7,200. Another menu open while waiting (`Game::menu`, a message
box over the conversation) is an error (the input has to close it). During a
conversation `get_menus` and `wait_menu` read menu 1009 as open, and the stopwatch
(script time) stands still. The same operations are accepted by `fnvtool script run`.

The trace: `format`, `seed`, `frame_seconds`, `title`, `observations`, `steps`,
`given_errors` as in `fnv.script.trace/1`; `events`, each with its `frame`, of these
kinds only, in the order they happened:

| kind | fields |
|---|---|
| `dialogue_started` | `actor`, `topic`, `topic_edid` |
| `line_started` | `speaker`, `target`, `topic`, `topic_edid`, `info`, `seconds`, `chars` (all responses), `responses`, `flags` (also for SayTo and Say lines) |
| `response_said` | `speaker`, `info`, `number`, `chars`, `emotion`, `seconds` (`text` with `--text`); conversation lines only |
| `result_script` | `unit` (`… INFO 0x… begin` / `end`), `on` |
| `line_ended` | `speaker`, `info`, `cut` |
| `choices_offered` | `actor`, `choices`: `topic`, `topic_edid`, `info`, `chars`, `prompt`, `goodbye`, `darkened`, `challenge` {`form`, `difficulty`, `needed`, `have`} (`text` with `--text`) |
| `choice_made` | `actor`, `index`, `topic`, `topic_edid`, `info` |
| `topic_added` | `topic`, `topic_edid`, `by` (the line; absent for AddTopic) |
| `dialogue_ended` | `actor`, `reason` (`goodbye`, `no_choices`, `left`, `actor_gone`), `lines` |
| `no_line` | `speaker`, `topic` |
| `stage_set`, `menu_opened`, `menu_closed` | as in `fnv.script.trace/1` |

and `final`: `frames`, `dialogue` (the conversation still running, or null), `topics`
(known topics), `quests`. The same events appear in `fnv.script.trace/1`, whose `final`
gains `dialogue`.

## 9. The intro walk-through's conversations

`crates/fnv-game/walkthrough/intro.actions.json` (scripts.md §9) now talks:

- **The psych test.** Stand-ins: objective 40 completed and the couch trigger's
  `bRunTimer` set (its OnTrigger needs IsCurrentFurnitureRef: sitting is AI). From
  there the trigger's own script counts 2 s, sets `bGiveTest` and calls
  `StartConversation player`; Doc greets with the test's opening line (begin: the test
  has begun; end: `bGiveTest` 0), and fourteen `choose` steps answer six word
  associations, five statements and three inkblots; each answer's begin script adds to
  the scores; the last line (Goodbye) sets `bDoFinalCalculation` at its start and stage
  85 at its end. 15 lines; tag skills Guns, Science, Sneak as before. The whole test
  happens in paused time (real time passes for the lines, script time does not). One script used to lean on GameMode running meanwhile: VCG01Test's quest
  script does the tag-skill calculation in GameMode once `bDoFinalCalculation` is 1,
  which the last line's begin script sets. It ran while that line was being said, as long as
  conversations did not pause; now it runs in the first GameMode frames after the
  Goodbye line, still well before stage 90 opens the tag-skill menu 1 s later. The
  result is the same.
- **The farewell.** Stand-in: `talk` for Doc's dialogue package after stage 115 (and 4 s
  for his walk). Then the game's: the greeting that returns the player's things,
  two follow-up lines (the Pip-Boy with its message, the suit by the player's sex), the
  player's one answer, its follow-up, the Goodbye line whose end script starts
  VGenericTimer's event 3 → the hardcore box → VCG01 200. 5 lines, in paused time;
  nothing in it waits on GameMode.

Stand-ins that remain in the walk-through and why: the menus' OK and the hardcore
answer (the player's); walking into the two triggers (physics sensors); sitting on the
couch (furniture, AI); Doc's dialogue package for the farewell (AI packages); and all
of VCG02/VCG03: Sunny Smiles's and Trudy's lines are still `say_info` / `say_first`
inputs, because between them stand travel packages, combat and the crafting menu, and
because her conversations start from packages and activations that part of the
walk-through does not set up (the player's position, her travel). The real-data test
`crates/fnv-game/tests/dialogue_data.rs` runs the walk-through and checks both
conversations and the end at VCG01 200.
