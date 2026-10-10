# The dialogue screen in `nvforever --play` (step D4)

Status 2026-10-10: built on `claude/dialogue-screen`. The plan is docs/design/dialogue.md
(step D4, and its Pausing section); the rules a conversation follows are the runtime's
(`crates/fnv-game/src/dialogue.rs`, docs/spec-notes/dialogue.md §4–§6). This note is the
engine side: `engine/nvf-play/src/dialogue.rs`.

The screen's layout, wording and keys are NVForever's own and plain. Layer 7 builds the
real interface; nothing here imitates the original's art, and nothing here is a claim
about the original unless it carries a tag.

## 1. What happens

1. **Start.** E on a living actor runs its default action, talking
   (`fnv_sim::Outcome::Talk`): the conversation starts with the actor's greeting
   (`Game::start_dialogue`). The same happens when the actor's OnActivate block calls
   `Activate`, and when any script activates an actor for the player
   (`script::Runtime::talk_after` reads the frame's `activated` events). Scripts also
   start conversations themselves with `StartConversation` (Doc Mitchell's psych test).
   An actor with nothing to say starts nothing: the line "… has nothing to say" shows as
   before and the log says why (`dialogue: no conversation: …`).
2. **The actor speaks.** The response being said is a subtitle, "Speaker: text", in the
   same place as the subtitles of SayTo lines (`script_hud.rs`), for as long as the
   runtime says it (its voice file's length; `voice.rs` plays the file). Under it a hint
   line names the keys.
3. **The player chooses.** When the actor has finished, the offered choices are listed
   in a panel, numbered from 1, at most ten at a time (a longer list scrolls with the
   selection). A choice the actor has answered before is dimmed (`Choice::darkened`:
   the runtime's rule [documented: GECK wiki Category:Dialogue, Info Refusal and Always
   Darken]). A skill or perk line carries its tag, "[Speech 40]", or "[Speech 37/40]"
   when the player falls short [documented: Category:Dialogue "AV/Perk"; where the two
   numbers come from is the runtime's hypothesis, dialogue.md §3.3].
4. **The end.** A Goodbye line ends the conversation when it has been said; so does an
   actor with nothing more to offer (the runtime's rules).

## 2. Keys

| When | Input | Does |
|---|---|---|
| The actor speaks | Space, Enter, Activate (E), left click | skips to the next response (`Game::skip_response`); after the last one the line ends as if it had been said |
| The actor speaks (`living`) | mouse | looks around |
| The player chooses | 1–9 (also the number pad) | picks that choice |
| | Up / Down, W / S, mouse wheel | move the selection |
| | Enter, Space, Activate (E) | pick the selected choice |
| | left click on a choice (hovering selects it) | picks it |
| | Tab | says goodbye: picks the offered Goodbye choice, so the actor says its goodbye line and the conversation ends as the data ends it; refused when no Goodbye choice is offered |
| Any time | Esc | the pause menu (Resume goes back to the conversation) |

- The original leaves a conversation with Tab [hypothesis: the coordinator's brief of
  2026-10-10, from play; when it allows leaving is not captured]. Ours: Tab only where
  the data offers a goodbye, so a forced conversation (the psych test) cannot be left by
  accident.
- Breaking a conversation off (cutting the line, no end script) is not a key of the
  game: it is a debug switch, off by default (§4) [owner decision 2026-10-10].
- The cursor: free look while the actor speaks, the cursor released while the player
  chooses (§3) [owner decision 2026-10-10].
- A message box a result script shows during a conversation takes the keys first: it is
  drawn and answered as any other box (`script_hud.rs`), the conversation's lines wait
  for it (the runtime's rule), and the panel is hidden meanwhile.
- Skipping: the response ends at once and the next one starts; the voice of the
  skipped response stops (`voice.rs` stops a sound whose line moved on). The end result
  script still runs when the last response is skipped [hypothesis:
  `Game::skip_response`, dialogue.md §5; capture question §7.8].

## 3. State and pausing

While a conversation runs the game is in `PlayState::Dialogue` (`dialogue::sync_state`),
a third state beside `Playing` and `Menu`.

- **Held, as in the container and inventory menus and under a message box**: the
  player's movement and actions (activation, weapons, the inventory key), the player's
  character and the physics world (they step only while `Playing`), the game clock
  (`sim.rs`), combat (`combat.rs`), trigger blocks (`script.rs` skips the trigger test,
  so no OnTrigger runs every frame), and travelling NPCs (`--npc-travel`; `WorldHeld`,
  read by `engine/nvforever/src/nav.rs`: they stand and blend to their idle, since they
  are given no velocity). Scripts are in menu mode by the runtime's own rule
  (`Game::in_menu`: no GameMode blocks, game time and quest delays stand still, MenuMode
  blocks run with code 1009) [owner ruling 2026-10-10: a conversation with the player
  pauses the game; lines said outside one do not].
- **Running**: the conversation's own lines (`Game::tick` advances them in real time),
  subtitles and voice.
- **The picture** is one switch for every menu, `--menu-pause living|original`
  (`pause::MenuPause`; `--dialogue-pause` is its old name and still works;
  docs/design/dialogue.md §8):
  - `living` (default): everything that is only presentation runs on (actors' idle
    animation, wind in the trees, water, the sky's clouds, effects), nothing snaps to a
    frozen frame, and the player looks around with the mouse while the actor speaks
    (`dialogue::FreeLook`). When choices are offered the cursor is released for
    clicking and the view holds still; it is grabbed again when the next line starts
    [owner decision 2026-10-10, of the two options offered]. [owner preference
    2026-10-10 for `living`: a deliberate departure from the original's look that keeps
    what scripts see.]
  - `original`: Bevy's virtual clock is paused while the menu is open
    (`pause::freeze_picture`), so animations, wind, water and clouds stand still too;
    the cursor is free throughout. This holds for the dialogue screen, the inventory
    and container menus, message boxes and the menus scripts open alike.
- **The Esc pause menu holds everything in both modes**: the picture (the virtual
  clock), scripts and the conversation (`script.rs` does not tick under it) and the
  voice of the line being said, which is paused and resumed with it (`voice.rs`).
- **What goes on while the picture stands still** reads `pause::AidTime` (the virtual
  clock plus the real time it stood still): the scripts' frame time (so the
  conversation's lines and MenuMode blocks run in real time under an `original` menu),
  every timed test aid and the menus' timers. `--run-seconds` counts real time while the
  virtual clock is paused, so an unattended run always ends.

## 4. Test aid `NVF_PLAY_TEST_TALK`

`NVF_PLAY_TEST_TALK=<when>:<who>[=<step>|<step>…][;<when>:<who>…]`

- `<when>`: seconds after the start, or `<seconds>@<quest>=<stage>`: that long after
  the stage was first seen done (as in `NVF_PLAY_TEST_SCRIPT`, layer5-play.md §5).
- `<who>`: a reference (Editor ID, hex FormID, `placed:<base>`): the player activates it
  then, as E would, wherever the crosshair is. Or `*`: nothing is started, and the steps
  answer the next conversation a script starts.
- Steps, each taken once the actor has finished speaking and the choices have been on
  screen for `NVF_PLAY_TEST_TALK_DELAY` seconds (default 0.25):
  - `<n>`: the n-th offered choice, 0-based (as `index` of `fnvtool dialogue talk`; the
    screen numbers from 1);
  - a topic's Editor ID or hex FormID: that topic's choice;
  - any other text: the choice that reads so, ignoring case; a part of it if exactly one
    choice contains it; quotes force text (`"3"`, `'leave'`);
  - `leave`: as Tab; `skip`: skips the response being said (taken while the actor
    speaks).
- When the steps run out, or a step names nothing that is offered (a warning), the aid
  leaves: by the Goodbye choice if one is offered, else by breaking off (this safety net
  exists only in aid-driven runs and does not need the debug switch below).
- `break`: Shift+Tab as the player would press it, once the response or the choices
  have been on screen a moment: it does something only with the debug switch.

**Debug switch `NVF_PLAY_DEBUG_TALK_BREAK=1`** (unset, empty, `0`, `off`, `false` and
`no` leave it off; it does not make a run unattended): Shift+Tab breaks the conversation
off at any point (`Game::end_dialogue`: the line being said is cut and no end script
runs, so a quest result can be lost). Without the switch Shift+Tab does nothing, and no
on-screen hint names it [owner decision 2026-10-10: "this should not be in the release
version"].

Debug switches (`NVF_PLAY_DEBUG_*`) and test aids (`NVF_PLAY_TEST_*`) are development
tools: they are to be compiled out of packaged releases once packaging exists
[coordinator's note 2026-10-10; no packaging work yet].

**Unattended runs never wait for input** (any `NVF_PLAY_TEST_*` variable, or
`--auto-walk`): a conversation no script answers is left the same way after 1 s of
choosing, and any conversation is broken off after 600 s. Message boxes inside a
conversation are answered by `NVF_PLAY_TEST_SCRIPT_MENUS` as elsewhere.

**Log**, one line each, all starting `dialogue:` (`dialogue::log_event`, from the trace
events, so they are the runtime's account):

```
dialogue: conversation with <actor> started (topic <topic>)
dialogue: <actor> says [<topic>] <line> response k/n (<seconds> s): "<text>"
dialogue: <actor> offers N choices: 0) "<text>" [<topic>]; 1) … (goodbye) (dimmed)
dialogue: choice <index> made: "<text>" [<topic>], <actor> answers with <line>
dialogue: topic <topic> added by line <line>
dialogue: conversation with <actor> ended (<goodbye|no_choices|left|actor_gone>, N lines)
```

The aid's own lines start `test aid NVF_PLAY_TEST_TALK:`. With `NVF_PLAY_SCRIPT_TRACE`
the trace file has the same as trace events (`dialogue_started`, `response_said`,
`choices_offered`, `choice_made`, `dialogue_ended`, dialogue.md §8) and a `talk_input`
line for each thing the player or the aid did (`op`: `activate`, `choose`, `skip`,
`leave`, `break_off`; `by`; `error`).

`NVF_PLAY_TEST_SCRIPT`'s `var:` step now takes names with a colon
(`var:scripted:0x001055B4:bRunTimer:1`).

`NVF_PLAY_TEST_PAUSE=<seconds>:<seconds>[,…]` (`pause.rs`) opens the Esc pause menu at
the first time and resumes at the second, as Esc would.

## 5. The unattended intro with real conversations

`nvforever --data <Data> --play --new-game --no-voice --run-seconds 360` with
`NVF_PLAY_TEST_SCRIPT_MENUS=1` and

```
NVF_PLAY_TEST_SCRIPT=2@VCG01=55:walk:scripted:0x00104C02,12@VCG01=60:activate:VCG01VigorTesterREF,
1@VCG01=80:objective_done:VCG01:40,1@VCG01=80:var:scripted:0x001055B4:bRunTimer:1,
2@VCG01=110:walk:scripted:0x0010722F
NVF_PLAY_TEST_TALK=0@VCG01=80:*=VCG01DocMitchellTopic000|VCG01DocMitchellTopic009|VCG01DocMitchellTopic013|
VCG01DocMitchellTopic024|VCG01DocMitchellTopic029|VCG01DocMitchellTopic032|VCG01DocMitchellTopic040|
VCG01DocMitchellTopic045|VCG01DocMitchellTopic051|VCG01DocMitchellTopic055|VCG01DocMitchellTopic060|
VCG01DocMitchellTopic065|VCG01DocMitchellTopic070|VCG01DocMitchellTopic075;
4@VCG01=115:DocMitchellREF=VCG01DocMitchellTopic083
```

(each on one line). These are the inputs of the headless walk-through
(`crates/fnv-game/walkthrough/intro.actions.json`, dialogue.md §9):

- **The psych test.** Stand-ins: objective 40 completed and the couch trigger's
  `bRunTimer` set (sitting on the couch is furniture, AI). The trigger's own script then
  calls `StartConversation player`; the screen opens; the `*` script answers fourteen
  times by topic; the last answer's Goodbye line sets stage 85 at its end. (dialogue.md
  §9 describes a 2 s count by the trigger's script before the call; in the engine runs
  the conversation started on the frame the variable was set. Reported, not explained.)
- **The farewell.** Stand-in: the aid activates Doc 4 s after stage 115 (his dialogue
  package would walk him to the player). The greeting, its follow-up lines and the
  Pip-Boy message are the game's; the one answer is the aid's; the Goodbye line's
  end script leads to the hardcore box and VCG01 200.

The stand-ins of layer5-play.md §6.2 for the psych test's scores, `bBeganTest`,
`bDoFinalCalculation`, the verdict line and the two farewell lines are gone: the
conversations set them.

## 6. Checks (2026-10-10)

All windowed runs unattended, on main c7ced81 merged in (voice #119, the runtime's menu
mode in conversations #120), `RUST_LOG=info`, each exit 0 with "run finished".

- **The intro** (§5; 360 s asked, done after about 260 s; 58.8 frames per second):
  **VCG01 completed at stage 200** (stopped), VMQ01 10, VCG02 5, CGTutorial 2, VCG00
  0, 90, 95, 100 and VCG01 0, 1, 3, 5, 7, 8, 10, 15, 25, 30, 35, 36, 40, 45, 50, 55, 60,
  65, 70, 75, 76, 79, 80, 85, 90, 95, 98, 100, 102, 105, 110, 115, 200: the stage set of
  layer5-play.md §6.2. Tag skills Science, Guns, Sneak, as the headless walk-through.
  - Psych test: the couch trigger's `StartConversation` opened the screen 1.0 s after
    stage 80 (on the frame after the aid set `bRunTimer`); greeting (two responses), fourteen choices picked by topic
    ("Cat.", "Target.", "Dream.", "Reasonable.", "Inspiration.", "Genes.", four times
    "No opinion." and once "Disagree." for the statements, three inkblots), 15 lines,
    ended by Goodbye; stage 85 on the frame it ended, 90 one second later. 54.7 s of
    real time during which the runtime's game time stood still (trace `gt` 109.53
    before and after).
  - Farewell: the aid's activation started Doc's greeting 4 s after stage 115
    (0x001057E8); two follow-up lines without a choice, one choice offered and picked
    ("Thanks for patching me up, Doc."), its follow-up Goodbye line of four responses,
    5 lines, 62.5 s; the hardcore box opened on the frame the conversation ended and
    VCG01 200 came 0.4 s after it was answered.
  - Trace: 2 `dialogue_started`, 29 `response_said`, 15 `choices_offered`, 15
    `choice_made`, 2 `dialogue_ended`, 16 `talk_input` with no error; one
    `voice: playing …` log line per response (silent with `--no-voice`).
  - No message box opened *during* either conversation (the Pip-Boy line's message
    is a line on screen, not a box), so a box on top of a conversation is covered only by the unit
    tests of the runtime, not by a run.
- **Easy Pete** (`--cell Goodsprings`, no new game, so VCG01 has not run):
  `NVF_PLAY_TEST_TALK="12:EasyPeteRef=VFreeformGoodspringsGSEasyPeteTopic000|victor|leave"`,
  `NVF_PLAY_TEST_TALK_DELAY=2`. Greeting "Howdy. What can Easy Pete do for you?"; offered:
  the people who attacked me / why Easy Pete / Victor / Goodbye. Chosen by topic: why
  Easy Pete; then offered six (the attackers, prospecting, the NCR, the Bighorners,
  Victor, Goodbye); chosen by a part of the text: Victor; then five; `leave` took
  Goodbye, whose line ended the conversation (4 lines). A question that was asked is
  gone from the next list, and nothing was ever dimmed (a rule to capture, §7).
- **With sound** (the same without `--no-voice`, greeting and goodbye): two
  `voice: playing sound\voice\falloutnv.esm\maleold02\…ogg` lines (2.97 s and 9.10 s),
  no audio warning or error in the log. Whether it was audible is the owner's to say.
- **`--dialogue-pause original`** with `12:EasyPeteRef=skip|1|skip|leave|skip`: the
  virtual clock stopped when the conversation started and ran again when it ended; the
  three `skip` steps cut each response after 0.3 s (the lines still ended uncut, the
  conversation by Goodbye); the run counted its 30 s of virtual time and finished.
- **A traveller during a conversation** (`--npc-travel DocMitchellREF=GSChetRef`, the
  conversation started by `NVF_PLAY_TEST_SCRIPT=12:activate:EasyPeteRef`, no talk
  script): Doc had walked 989 units when it started and 989 when it ended 13 s later,
  then walked on. With no script to answer, the unattended rule took Goodbye after 1 s.
- **Goodsprings `--play`, 40 s, no aid**: exit 0 (the first pack box stays open, as
  before).
- **Travel smoke test** of CLAUDE.md (nav.rs changed): "nav summary: 2 of 2 travellers
  arrived".
- Not run: the moving-camera smoke test and the time-lapse (no rendering, sky or
  lighting change).

## 7. Open

- **Played by hand: not done by this session** (the unattended runs cannot hold the
  cursor, so free look and mouse clicks on choices were not exercised; keys and clicks
  go through the same code path as the aid from `Act` on). Owner item.
- No dialogue camera: the view stays where it was, the actor does not turn to the
  player (dialogue.md §7). The mouth moves since step D6 (docs/spec-notes/lipsync.md);
  head turns and eye movement while talking do not.
- The panel does not show the player's own line being said, nor the actor's last line
  while choosing.
- A choice's skill tag names the actor value by display name when the simulation knows
  one, else by Editor ID.
- NPC idle animation is presentation and runs on in `living`; an NPC that was walking
  (`--npc-travel`) stops and stands. AI packages do not exist yet; when they do they
  have to honour `WorldHeld` too.
- Between two message boxes shown one after the other the game is `Playing` for a frame
  or two (the next box opens when the first has closed), so under `original` the
  picture moves by that much between them.
- Barter, repair and the companion wheel are not conversations here (dialogue.md §7).

Seen in the runs of §6, for the rules' owner (nothing was changed in `fnv-game` for
them):

- Every question asked of Easy Pete was gone from the next list, and no choice was ever
  dimmed. The data explains the first: the answer to "Why are you called Easy Pete?"
  (INFO 0x0010515C) has Say Once and no choices, so the topic has no line left and the
  runtime hides it. Whether the original hides such a topic or shows it dimmed is the
  capture question of dialogue.md §7.5; the dimmed look is so far exercised only by the
  runtime's unit tests.
- The psych test opens on the frame `bRunTimer` is set, without the 2 s that
  dialogue.md §9 describes (§5).
- A response skipped by the player still runs the line's end script
  [hypothesis, §2]; every quest result in the runs came from lines said to the end.
- After a held traveller walks on, its 5-second progress line is logged a few times in
  a row (it catches up with the time it stood); cosmetic.
