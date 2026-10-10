# Spec note: dialogue topics, lines and conversations

Clean-room notes for Layer 6 dialogue, steps D1 and D2 of docs/design/dialogue.md
(2026-10-10). Code: `crates/fnv-game/src/data.rs` (`TopicDef`, `InfoDef`),
`crates/fnv-game/src/dialogue.rs` (conversations), `fnvtool dialogue`.

Sources and tags:
- **[documented: xEdit …]**: the record definitions of xEdit's `wbDefinitionsFNV.pas`
  (DIAL and INFO records, re-read 2026-10-10), as carried by `schemas/fnv/gameplay.json`.
- **[documented: page, owner export]**: a page of the owner's private GECK wiki export
  (`C:\fnv-private\geckwiki\`, never committed). Everything here is our own summary.
- **[owner statement 2026-10-10]**: the owner's ruling from long GECK use; not yet
  captured in game.
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
said next, without a choice by the player [hypothesis].

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
   for SayTo: Doc Mitchell's VCG01Intro lines, layer5-captures.md §10.3]. Random: a line
   the actor qualifies for is put on a stack instead of being said; the stack grows with
   every further qualifying Random line and closes at one that has Random End, or at
   the first qualifying line without Random; then one line of the stack is picked at
   random [documented: Category:Dialogue, flags]. The same page's in-depth example 3
   says instead that a plain line between Random ones is skipped and only Random End
   closes the stack; the flag description is what the runtime follows [hypothesis;
   capture question 4].

Implementation: a stable sort of the stored order by quest priority, highest first,
then the first line the speaker can say (rules 1–3 in one pass). The Random pick uses
the game's seeded generator, and only when the stack holds more than one line.

A speaker **can say** a line when:
- it has not said it, for Say Once; not within the last `fSayOncePerDayInfoTimer` game
  hours (12 when the setting is absent), for Say Once a Day; both per actor
  [documented: Category:Dialogue]. The clock is GameDaysPassed × 24 + GameHour
  [hypothesis];
- it is the line's fixed speaker, if the line has one [documented: Category:Dialogue];
- the line's quest is running [hypothesis: holds in the SayTo captures; not tested with
  a stopped quest] and the quest's conditions pass [documented: Quest Data Tab];
- the line's conditions pass, with the speaker as subject and the listener as target
  [documented: Category:Conditions].

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
- **A line.** The begin result script runs when the line starts, the end result script
  when its last response is over [documented: Category:Dialogue]. Responses follow each
  other in number order; each lasts `Game::response_seconds` (the one place a response
  is timed: the fitted 1.1 s + 1/16 s per character until the voice workstream supplies
  the sound's length). Result scripts run on the actor with the line's quest. With Run
  Immediately both scripts run when the line is picked [documented flag; hypothesis
  that for a line the player asked for "when the conversation is generated" is that
  moment; no Topic line of the masters has the flag].
- **When a line ends**, in this order: end result; the line's Add Topics join the known
  topics [documented: xEdit "Add Topics"; hypothesis on the moment and that they stay
  known afterwards]; a Goodbye line ends the conversation [documented]; else the first
  follow-up the actor can say is said (§3.4) [hypothesis]; else choices are offered;
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
  whole game; every actor that has a line for a known topic offers it [hypothesis: the
  wiki's AddTopic page speaks of adding to "the Add Topic list of a topic", which could
  mean something narrower; capture question 6].
- **Leaving** (`Game::end_dialogue`). The line being said is cut without its end result
  [hypothesis]. When the game lets the player leave is the screen's business (D4).
- **Not modelled:** conversations between two NPCs (the Conversation tab: HELLO, link
  to/from, GOODBYE), radio, combat and detection barks, Low/High Intelligence lines and
  the Dumb Response, the quest flag "Allow Repeated Conversation Topics", the Fallout 3
  speech-challenge roll and the SpeechChallengeFailure topic, lip sync and emotions on
  faces.

## 6. What scripts see

| Function | During a conversation | Source |
|---|---|---|
| `IsTalking` | 1 while the actor says a response; 0 while the player chooses | [documented: IsTalking for "currently talking"; hypothesis for the choosing phase] |
| `MenuMode 1009` | 1 from the greeting to the end | [documented: GetMenuTargetRef names menu 1009's target as the actor spoken to; hypothesis that it covers the whole conversation] |
| `MenuMode` (no code), GameMode blocks, quest timers, `Game::time` | unchanged: **the dialogue menu does not pause the game** | [hypothesis; evidence: the Lucky 38 elevator Securitron's script (SCPT 0x0015725D) waits *in its GameMode block* for `MenuMode 1009` to be over before it shows a message box, which is pointless unless GameMode runs during dialogue] |
| `Begin MenuMode 1009` blocks | not run (one script of the masters has one) | [hypothesis; not modelled] |
| `GetTalkedToPC` | 1 once a conversation with the player has started; SayTo lines do not count | [documented: GetTalkedToPC; hypothesis on SayTo] |
| `GetDialogueEmotion` | the emotion type of the response being said, else 0 | [documented: GetDialogueEmotion for the codes] |
| `StartConversation player [topic]` | starts the conversation at once; ignored while one runs or when the actor has no line; the walk to the player needs AI packages | [documented: StartConversation; stand-in for the walk] |
| `StartConversation <npc>` | recorded, no effect (NPC conversations) | – |
| `AddTopic` | the topic joins the known topics (trace `topic_added`) | [documented: catalogue; hypothesis, §5] |
| `GetIsCurrentPackage` | unchanged (no dialogue package is modelled) | – |

A real menu opened during a conversation (a message box from a result script) stops
game time and therefore the conversation, as it stops SayTo lines (scripts.md §7.7).
Still stubs: `IsActorTalkingThroughActivator` (talking activators), `GetDisposition`,
`SetDisposition`, `Exists`.

## 7. Hypotheses that need in-game captures

1. Greeting choice when lines of several quests qualify: rules 1–3 of §4 "to confirm"
   (owner statement), including equal priorities and the stored order of §3.1.
2. Whether a line of a stopped quest can be said.
3. Whether an actor with no qualifying greeting can be activated at all.
4. Random stacks: does a qualifying plain line between Random lines close the stack
   (flag description) or get skipped (the wiki's example 3)?
5. Which topics are offered and in what order: hidden when the actor has no line; ties
   of equal topic priority; the fallback to the topic list when no choice of a line can
   be said; the conversation ending when nothing can be offered; GOODBYE appended.
6. Known topics: are AddTopic and Add Topics global and permanent, or tied to a topic,
   an actor or a conversation? When is an Add Topics list applied (line start or end)?
7. Follow-up lines: said next without a choice; first qualifying of several.
8. Timing of the begin and end results against the sound; the end result when the
   player skips a line or leaves; Run Immediately on a chosen line.
9. INFC/INFX: does a "shared" line count as a line of the listing topic?
10. Link From in player dialogue: does it add a choice?
11. The skill tag's numbers: taken from the line's condition? What is shown for a perk?
12. What `IsTalking`, bare `MenuMode`, `MenuMode 1009`, GameMode blocks and
    `Begin MenuMode 1009` do during a conversation; whether game time runs.
13. Say Once across two actors sharing a line; Say Once a Day's clock; Say Once and
    Say Once a Day counted from the line's start or end.
14. Response order where numbers and stored order disagree.
15. Whether SayTo and Say obey Random stacks like menu dialogue.

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
[--text]` → `fnv.dialogue.lines/1`**: `summary` (the counts of §2.2 and
`previous_info` of §3.1); `lines`, topics by FormID and within a topic in the order
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

`max_frames` defaults to 7,200. A menu open while waiting is an error (the input has to
close it). The same operations are accepted by `fnvtool script run`.

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
  85 at its end. 15 lines; tag skills Guns, Science, Sneak as before.
- **The farewell.** Stand-in: `talk` for Doc's dialogue package after stage 115 (and 4 s
  for his walk). Then the game's: the greeting that returns the player's things,
  two follow-up lines (the Pip-Boy with its message, the suit by the player's sex), the
  player's one answer, its follow-up, the Goodbye line whose end script starts
  VGenericTimer's event 3 → the hardcore box → VCG01 200. 5 lines.

Stand-ins that remain in the walk-through and why: the menus' OK and the hardcore
answer (the player's); walking into the two triggers (physics sensors); sitting on the
couch (furniture, AI); Doc's dialogue package for the farewell (AI packages); and all
of VCG02/VCG03: Sunny Smiles's and Trudy's lines are still `say_info` / `say_first`
inputs, because between them stand travel packages, combat and the crafting menu, and
because her conversations start from packages and activations that part of the
walk-through does not set up (the player's position, her travel). The real-data test
`crates/fnv-game/tests/dialogue_data.rs` runs the walk-through and checks both
conversations and the end at VCG01 200.
