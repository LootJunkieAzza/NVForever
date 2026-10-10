# Dialogue design: topics, lines, conversations, voice

Status 2026-10-10: **D1 and D2 done** (branch `claude/dialogue-core`: records,
`fnvtool dialogue`, headless conversations; facts and rules in
docs/spec-notes/dialogue.md); D3 in progress (`claude/dialogue-voice`); D4 and D5 open. Owner request 2026-10-10: "now that quests have been done are we
able to work on dialogue lines".

Where it sits: dialogue is Layer 6 (README; layer5.md §1 lists it with AI packages and
combat). Unlike combat (docs/design/combat.md §1), dialogue **is a parity target**: which
line an actor says, which topics the player is offered and when the result scripts run
decide how every quest moves. The screen's look is Layer 7 and can be plain for now.

## 1. What exists (main, 95f1a44)

- **Records.** `schemas/fnv/gameplay.json` describes DIAL and INFO field by field
  [documented: xEdit definitions, cited per field in the schema]. `fnv-game::data` reads
  from them only what Layer 5 needed: a line's quest, fixed speaker, conditions, begin and
  end result scripts, topic, Say Once, Goodbye and the length of each response text
  (`InfoDef`), plus each topic's lines in order (`GameData::topics`).
- **Lines said by scripts.** `SayTo` and `Say` pick the first line of a topic the speaker
  can say, run its begin result, hold `IsTalking` for the line's length and then run the
  end result and the speaker's SayToDone blocks (`Game::say`, scripts.md §7.7)
  [observed: the round 4 intro captures]. The length is fitted to the text
  (1.1 s + 1/16 s per character) because voice files are not read (owner-review item 28c).
- **In the game.** `nvforever --play` shows such lines as subtitles
  (`engine/nvf-play/src/script_hud.rs`). There is no sound playback of any kind.
- **Documented rules.** docs/spec-notes/quests-dialogue.md §4: top-level topics and
  priority, prompts, speech challenges, the info flags, Random stacks, the previous-info
  order, GREETING and the conversation topics.
- **Stubs.** `StartConversation` and the other dialogue functions marked
  `Stub("dialogue")` in `crates/fnv-game/src/functions/mod.rs`; `AddTopic` only records
  the topic.

## 2. What's missing

1. The rest of the two records: topic name, type, priority and flags; a line's type,
   flags, prompt, responses (text, emotion, sound), choices, link-from topics, follow-up
   topics, added topics and speech challenge.
2. A conversation: the player activates an actor, the actor greets, the player is offered
   topics, picks one, the actor answers, results run, new topics appear, a Goodbye line
   ends it.
3. Voice: where a line's sound file lives, how long it is, and playing it.
4. The dialogue screen in `--play`.
5. In-game captures of what the documentation leaves open (§6).

## 3. Rules to implement

Each rule carries its source. Anything marked [hypothesis] goes to §6 as a capture
question and is listed in docs/owner-review.md until a capture settles it.

- **Who can say a line.** A line's conditions run with the speaker as subject and the
  listener as target [documented: quests-dialogue.md §3, Run On]; a fixed speaker acts as
  a GetIsID condition [documented: §4]; the line's quest must be running
  [hypothesis: holds for the SayTo captures; not tested with a stopped quest].
- **Which line of a topic** (greetings, chosen topics, SayTo and Say alike;
  docs/spec-notes/dialogue.md §4):
  1. When lines of several quests are eligible, quest priority decides: the
     higher-priority quest's line wins [documented: Quest Data Tab, owner export; owner
     statement 2026-10-10].
  2. Between quests of the same priority, the first in stored order wins
     [owner statement 2026-10-10, from GECK use; not yet captured].
  3. Inside one quest the game takes the first eligible line from the top of the
     editor's list, unless lines carry the Random flag; then qualifying Random lines form
     a stack from which one is picked [owner statement 2026-10-10, from GECK use; not yet
     captured; the Random stack is documented: §4; observed for SayTo: scripts.md §7.7].
  The runtime does this with a stable sort of the topic's stored order by quest
  priority, highest first, and takes the first line the speaker can say. Stored order
  is file order, which agrees with the previous-info links in 5,677 of the 5,686 lines
  that have one (dialogue.md §3.1). Only 102 topics of the masters have lines of several
  quests; in 76 of them one actor could say lines of two quests of equal priority
  (GREETING foremost), which is what rule 2 decides (dialogue.md §3.2). AI packages are
  chosen the same way, first eligible from the top of the actor's list [owner statement
  2026-10-10], which `Game::select_package` already does.
- **Greeting.** Activating an actor who has something to say starts with a line of the
  GREETING topic [documented: Topics Tab]. An actor with no qualifying greeting cannot be
  talked to [hypothesis].
- **Topics offered.** After a line: its choices, if it has any, and nothing else;
  otherwise the top-level topics plus the topics added so far (by lines' Add Topic lists
  and by `AddTopic`), each shown only if it has a line the actor can say now
  [documented: Category:Dialogue for top-level and choices; the "has a line" filter is
  hypothesis]. Order: topic priority (DIAL PNAM, not the quest's), then for a line's
  choices their order in the record [documented: Category:Dialogue, sorting order of
  links] and for the topic list [hypothesis] record order. The shown text is the line's
  prompt if it has one, else the topic's name [documented: §4].
- **Saying a line.** Responses are said one after another in response-number order. The
  begin result runs when the line starts and the end result when it finishes
  [documented: §4; observed for SayTo]. Run Immediately moves the results to when the
  list is built [documented: §4].
- **Say Once, Say Once a Day, Goodbye, Info Refusal, Always Darken.** As documented in
  quests-dialogue.md §4. Say Once is per actor [documented].
- **Speech challenges.** The choice shows the skill and the needed level; picking it
  below the level leads to the failure line [documented: §4; the pass/fail routing is to
  be read from the records and captured].
- **StartConversation and forced greetings.** An actor walks up and starts dialogue
  with a named topic [documented: function catalogue]. The walking part needs AI
  packages; until then the conversation starts at once [stand-in].
- **Voice files.** A response's sound is looked up from the plugin, the speaker's voice
  type, the quest, the topic, the line's form ID and the response number
  [observed: docs/spec-notes/voice.md §2; 97.3 % of the masters' responses have a sound].
  A response lasts as long as its sound; without a sound the fitted length stays
  (`fnv_game::voice`, voice.md §8). Done in step D3 (`crates/fnv-voice`, `fnvtool voice`,
  playback in `engine/nvf-play/src/voice.rs`); the captured spans are 0.3 to 0.4 s per
  line longer than the sounds, which is a capture question (§6, 4).

## 4. Where the code goes

| Part | Place |
|---|---|
| Record facts | `docs/spec-notes/dialogue.md` (new); voice paths and file formats in `docs/spec-notes/voice.md` (new) |
| Data | `crates/fnv-game/src/data.rs`: `TopicDef`, the rest of `InfoDef` |
| Conversation | `crates/fnv-game/src/dialogue.rs` (new): `Game::start_dialogue`, `Game::dialogue_choices`, `Game::choose`, `Game::end_dialogue`; state in `Game` so saves and traces see it |
| Tool | `fnvtool dialogue topics|lines|talk` (list, inspect, run a scripted conversation headless) |
| Trace | `fnv.dialogue.trace/1`: lines said, choices offered, results run, in order |
| Voice | a small reader giving a line's sound path and its length, usable headless and by the engine; playback in `engine/nvf-play` |
| Screen | `engine/nvf-play/src/dialogue.rs` (new): activation of an actor opens it; subtitles, the list of choices, keyboard and mouse; test aid `NVF_PLAY_TEST_TALK` |

The conversation lives in `fnv-game` with no engine types, like quests and menus, so the
lab can run it headless.

## 5. Steps

| Step | What | Depends on | Check |
|---|---|---|---|
| D1 | Spec note and full DIAL/INFO reading; `fnvtool dialogue topics|lines` | – | counts over the ten masters; every field of every record read without residue |
| D2 | Headless conversations; `fnvtool dialogue talk`; the dialogue functions un-stubbed; the intro walk-through's dialogue stand-ins replaced by real choices | D1 | synthetic tests per rule; the L5 scenarios still pass; the intro still ends at VCG01 200 |
| D3 | Voice: path pattern, sound length replaces the fitted length, lines are heard in `--play` | – | fraction of lines of the masters with a sound found; intro stage timings against the capture |
| D4 | Dialogue screen in `--play` | D2 | unattended test-aid conversation with Doc Mitchell and Sunny Smiles; played by hand |
| D5 | In-game captures for §6, then `L6.dialogue.*` scenarios with blind holdouts | capture time on the owner's desktop | lab |

D1+D2 are one workstream and D3 another; they run in parallel. D4 follows D2. D5 needs
the original game running on the owner's desktop and waits for the owner to say when.

Holdout rule as in Layer 5: runtime work never reads `C:\fnv-private\goldens\quest*`
or `intro`; capture work never reads `crates/fnv-game/src/dialogue.rs`.

## 6. Capture questions (D5)

1. Which greeting an actor gives when several qualify: **to confirm** the rules of §3
   (quest priority; equal priorities by stored order; first eligible line from the top
   inside a quest) [owner statement 2026-10-10].
2. The list of topics offered and its order, for a few actors at known quest states.
3. Whether a topic with no line the actor can say is hidden.
4. When the begin and end results run relative to the sound, and what happens to the end
   result when the player skips the line.
5. Whether a line of a stopped quest can be said.
6. Say Once across two actors sharing a line; Say Once a Day.
7. Speech challenge pass and fail routing, and the text shown.
8. What `IsTalking`, `GetIsCurrentPackage` and `MenuMode 1009` read during a conversation
   [the menu code is a hypothesis].

The full list of what D1 and D2 left as hypotheses (15 items, these among them) is
docs/spec-notes/dialogue.md §7.

How to observe 2 and 3 without reading the screen is open: the capture plugin can log
the lines said (it logs `SayTo` today) but has no list of offered topics. Screenshots of
the dialogue menu are the fallback.

## 7. Not in this plan

Lip sync and facial emotion (the `.lip` files and the emotion fields are read into the
data but not used), the dialogue camera, barter and repair services, companion wheels,
radio stations, random conversations between NPCs and combat barks. The last three need
AI packages and detection.
