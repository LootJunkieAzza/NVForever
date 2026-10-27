# Main-quest walkthroughs past Novac, headless (2026-10-27)

This is item 3 of project-audit-2026-10-23.md §7: one `fnvtool script run` action chain for
each main-quest chain past Novac, in `crates/fnv-game/walkthrough/`, and a smoke scenario
for each (`mq-*` in tools/smoke/scenarios.toml). Each chain starts from a `given` state
(the earlier quests' stages, globals and items). It drives the quests by the game's own
means where the runner has them: the player's dialogue picks, terminals, activations,
triggers, deaths, AI package events and message box buttons. Then it reads the stages the
game's own scripts set.

**Result: every chain reaches its final stage headlessly** [observed: the eight chains
run with no step error on main at #870 plus this branch; `python tools/smoke run --only mq-…`:
8 passed]. Three hard stalls were found, all in the engine, all small; they are fixed on
this branch (§3). Nothing past Novac is now untested headless. These are still not game
runs: travel, combat and AI arrivals are stand-ins (§4), and the Hoover Dam load test (§7
item 6) is a separate task.

## 1. Chains

| chain file (smoke scenario) | path taken | final stage reached |
|---|---|---|
| boulder-city (`mq-boulder-city`) | VMS20 negotiator: Monroe, Jessup talked round with Speech 45, back to Monroe; Jessup's Benny line | VMS20 100, VMQ01 100, VMQTops 10 |
| tops (`mq-tops`) | Strip gate: the Securitron's 2000-cap check unlocks the north gate; Benny invites the player to the suite (Speech 60); Benny killed there; the chip looted | VMQTops 80 |
| house (`mq-house`) | Victor's door, the elevator's floor box, the chip handed over, the basement demo; House II (Fort upgrade), III (Boomers), IV (Omertas), V (Hidden Valley bunker blown), VI (Kimball saved), VII (El Dorado), VIII (point of no return) | VMQHouse1-7 100, VMQHouse8 80, VHDHouseBattle 10 |
| yesman (`mq-yesman`) | Yes Man at the Tops; the Fort's vault console; House removed at the control-room terminal; the five tribes given as done | VMQYesMan01, 01a, 02, 03 100; VMQYesManFailsafe 100 |
| dam-panel (`mq-dam-panel`) | No Gods, No Masters: the control-center panel (a talking activator in Yes Man's voice, stage 20), the east power-plant console (30), the Legate | VHDIndependentBattle 100 |
| ncr (`mq-ncr`) | Things That Go Boom, Kings' Gambit (only Crocker's Gambit lines set VMQ05 10), For the Republic Part 2 (House unplugged), You'll Know It When It Happens (sniper killed), Moore's box, Eureka! (Lanius talked down) | VMS23 100, VMS29 105, VMQ03a 100, VMQ05 100, vHDBattleController 30 |
| legion (`mq-legion`) | Render Unto Caesar (Cottonwood Cove, the Fort, the vault), Et Tumor Brute? (surgery), Arizona Killer, Veni Vidi Vici | VMQ02, VMS35, VMQ03b 100; VHDLegionBattle 100 (starts VEnding) |
| ending (`mq-ending`) | Eureka! 30 sets the NCR ending; VEnding moves the player to the slide room; the narrator's SayTo chain runs every slide | FadeToCreditsTimer faded out, ExitGame called |
| ending-legion (`mq-ending-legion`) | VHDLegionBattle 100 sets story state 62; the same slide chain (12 slides) | FadeToCreditsTimer faded out, ExitGame called |
| ending-house (`mq-ending-house`) | VHDHouseBattle 100 sets 63 (9 slides) | FadeToCreditsTimer faded out, ExitGame called |
| ending-independent (`mq-ending-independent`) | VHDIndependentBattle 100 sets 64, Securitron vault not opened (65 when it was; not run) (11 slides) | FadeToCreditsTimer faded out, ExitGame called |
| reputation-axes (`mq-reputation-axes`) | Crocker's messenger and Vulpes's Strip greeting, which call SetReputation with axis 2 (item 10) | NCR and Legion infamy 0; VMS23's flags set after the axis-2 call |

Notes [observed]:
- **The Lucky 38 elevator** is an actor with one reference per floor. Activation adds a
  Dialogue package; after the conversation its GameMode shows the floor box (Penthouse is
  button 5), and the button moves the player. It fires headlessly once the package's
  conversation is stood in (§4).
- **The Dam panel** is not a terminal. It is a talking activator (0x00153FCE) that stage 10
  gives Yes Man's voice. All or Nothing uses the same panel step, and it works there too
  (House's line, VHDHouseBattle 20; a probe run, not a committed chain).
- **The Fort's player storage** (NVCCPlayerStorage): VMQ02 110, which its OnOpen sets, is
  the *fail* stage (the Legion turns hostile when the player robs the storage). The
  storage is locked at 255, and the gear comes back when the player leaves the Fort. So
  project-audit §1 item 1's "OnOpen stalls the Legion's turn" should read "OnOpen is how
  the Legion turns hostile". OnOpen fires in the engine since 2026-10-23.
- **Eureka!'s battle controller** counts Legion and NCR respawn pools per zone; no stage
  waits on them. Stage 20 comes from Lanius yielding (or his guards' counts if he dies),
  and 30 from Oliver's greeting.
- **`VMQYesMan01MoveToTerminal` unreachable** (`fnvtool ai reach`) is a false alarm of the
  check, not a stall. `ai reach` judges the package from Yes Man's placed position at the
  Tops, and no door chain joins that to the penthouse. In the game, two scripts move him to
  his penthouse marker 0x00166304 before the package can be chosen. From there it is a
  98-unit walk on one navmesh (`nav path`) [observed].

## 2. Stalls, ranked

**Hard** (a player would stall in the engine too), all fixed on this branch:

1. **The end slides stopped at the fourth slide**: SayTo found no line. VEndingBoomers'
   lines name a Boomer (NellisPete) as fixed speaker (INFO ANAM) and GetIsID Narrator as
   their condition. Our rule "only the fixed speaker says the line" made them unsayable.
   262 lines of FalloutNV.esm name a fixed speaker other than the actor of their own
   GetIsID condition; 167 are end slides, and others are the Lucky 38 elevator Victor,
   Freeside and the Tops. Fix: `Game::can_say_as` (dialogue.rs) lets the line's own
   GetIsID condition decide [hypothesis; documented, GECK wiki Category:Dialogue: Speaker
   "will act like GetIsID when dialogue is exported"]. Observer check 29.
2. **The end slides stopped at the Brotherhood slide**: the narrator's SayToDone chain
   (SCPT 0x00161EB4) SayTos VEndingBrotherhood unconditionally. That topic has no line for
   the NCR ending with the Brotherhood never met, and our SayTo with no line never
   finished. Fix: `Game::say` queues the speaker's SayToDone blocks for the topic in the
   next frame [hypothesis: the chain must go on, as the script calls every topic in turn;
   no document says]. Observer check 28. With both fixes the NCR ending says 11 slides
   (3 topics have no line) and reaches VEndingConclusion, FadeToCreditsTimer and ExitGame.
3. **Message box buttons read in MenuMode 1001 were lost headless**: `press_button` closed
   the box before a MenuMode frame ran. This affects the Fort's hatch and vault consoles
   (House II, Yes Man) and Caesar's surgery box (VMS35); 21-23 scripts read buttons this
   way. The play loop was right (nvf-play `answer`/`finish_answer`); only the runner was
   wrong, so this is hard only for the walkthroughs. Fix: the runner's press sets the
   button, runs one frame with menu 1001 open, then closes the box (runner.rs).

**Soft** (headless only, open; each chain works around it with a stand-in):

4. The runner has no op for reaching a patrol marker, so the marker's script never runs.
   Hit by: Moore at Kimball's podium (REFR 0x00130C75 gives him 0x00145EC3); Yes Man's and
   Oliver's markers. Stand-in: `package {event: done}` on the next package. Fix: a
   `patrol_point {actor, point}` op calling `Game::patrol_point_reached` (small).
5. `package {event: done}` does not mark the goal met, so a script package stays current:
   Kimball's walk back to the vertibird (0x001377CC, then 0x00130C7A). Fix: `done` calls
   `package_goal_met` when the package is the actor's current one (small).
6. Dialogue packages start no conversation headless (fnv-ai's `talk` goal is not in
   fnv-game). Hit by: the Lucky 38 elevator, Cato, Oliver, Lanius. Stand-in:
   `StartConversation` / `talk`. Fix: the runner force-greets with the package's topic
   (small to medium).
7. `trigger_enter` with `inside` is one frame; the Hidden Valley bunker's escape trigger
   counts 1 s inside. Stand-in: the trigger's timer is set. Fix: a `frames: N` field
   (small).
8. Opening a container headless runs no OnOpen (the engine queues it from the container
   menu). Not on any main path (see the storage note in §1). Fix: queue OnOpen/OnClose in
   the runner's container menus (small).
9. `ai reach` reports `VMQYesMan01MoveToTerminal` unreachable (§1). Fix: start the check
   also from the places scripts MoveTo the actor, as its own verdict (about 50-100 lines
   in fnv-ai).

**Possibly hard, unverified** (not on the canonical paths; observer checks 30-32):

10. Reputation axes other than 0/1. FalloutNV.esm calls SetReputation with axis 2
    (Crocker's messenger, Vulpes's and Alerio's Strip greetings), AddReputation with axis 2
    (Boulder City Showdown stage 100, one ending) and 3 (Severus's three tag trades), and
    conditions use GetReputation axis 2 (VMS36, Crocker's topics 013/014) and
    GetReputationThreshold axis 3 (Benny's two greetings) [observed: decompiled scripts,
    INFO conditions]. **Not a stall**: a refused call reads 0 and the script goes on (the
    engine traces `call_failed` and returns 0; only a call on an empty reference stops a
    script), and `reputation-axes` shows the messenger's flags set after the axis-2 call.
    The engine now treats other axes as doing nothing and reading 0 without a call error
    (reputation.rs `axis`) [hypothesis; observer check 30]. One open effect: Benny's
    greeting for a player liked on the Strip needs axis 3 >= 3, so with 0 it is never
    chosen; what the original reads for axis 3 decides it.
11. Say Once per actor: the Yes Man mainframe repeats the Tops Yes Man's first-meeting
    greeting (0x001164F1, shared by GetIsID OR) once (dialogue.md question 13).
12. `GetStage Q N == 1` compiled with an extra argument leaves two values on the stack.
    Three begin scripts (e.g. INFO 0x00160731) stop there; ours gives a script error.
13. A line with both a follow-up and choices (0x00158A3D, Topic056) plays the follow-up and
    never offers Topic068 [observed in the runner; the original's order is not known].

## 3. Stand-ins (what the chains do by hand)

- Travel: `move_player` / `go_to_cell` to the next scene (all chains).
- Walking into triggers: `trigger_enter` (physics sensors).
- Fights: `kill` with the player as killer (Benny, ambushers, the Kimball sniper, the
  Legion targets, the Dam's Legate guard, 20 kills in the Legion chain).
- AI arrivals: `package {event: done}` and `talk` (soft stalls 4-6).
- Side quests outside the main path: in `given` or `set_stage` with a note (How Little We
  Know 50, Volare!, G.I. Blues, the five Yes Man tribe outcomes, VMQ03a 100 in the House
  chain).
- The ending chain sets Eureka!'s stage 30 itself (the NCR chain reaches it), so it can run
  alone.
- The hacking minigame and the crafting/surgery menus: their results.

## 4. What is left

- Soft stalls 4-9: small runner ops; each would remove a stand-in.
- Observer checks 28-32 (docs/handoff/observer-checks.md): the two end-slide fixes are
  hypotheses until the original confirms them.
- The other endings' slides: all four endings now run every slide to ExitGame (2026-10-27,
  second batch); no new stall. The independent ending with the Securitron vault opened
  (story state 65) was not run.
- No add-on quest was walked (DM, HH, OWB, LR).
- Smoke `sunny-wells` fails on main (325cdaf1): Cheyenne runs after Sunny instead of walking. It was rerun alone with this branch's two play changes switched off and still failed, so it is marked `known_failing`; not explained yet.
- Game runs: these chains say the quest flow holds. They do not show that the AI walks
  there, the fights end or the Dam's ~300 actors run at speed (audit §7 item 6).

## 5. Coverage: what the chains reach, what is untouched

`fnvtool script cover <Data> <the ten masters> --schema schemas/fnv --actions
crates/fnv-game/walkthrough [--detail report.json] [--log-dir logs/] [--logs more.json ...]`
runs every chain over the load order it names (`plugins` in the actions file, else
FalloutNV.esm alone: the add-ons' start-up message boxes would hold the base-game chains
that keep menus), records what each reached, merges that with any saved logs (`script run
--coverage <file>` writes one per run; the result-script sweep will write one) and
compares it with the ten masters. The smoke scenario `walkthrough-coverage` checks the
printed summary against `tools/smoke/expected/walkthrough-coverage.toml`: a branch that
adds a chain or reaches further writes the new numbers there; a drop fails the run.

Reading the summary:
- `quests`: quests with stages (`total`), those with at least one stage reached
  (`touched`), those where a stage that completes the quest (QSDT flag Complete Quest) was
  reached (`completed`). `stages`: stages reached of all stages.
- `scripts` / `blocks`: SCPT scripts with blocks, those with a block run, and blocks run of
  all blocks (unit and block, so a script's GameMode and OnActivate count apart).
- `lines` / `actors`: dialogue lines (INFO) started; actors with lines (a line's own
  GetIsID actor, else its fixed speaker) and those with any line said.
- `results`: result scripts with statements, by kind (quest stage, INFO, terminal,
  package), and those run.
- `untouched_quests`: quests with no stage reached, by rank: `main` (FalloutNV.esm's VMQ,
  VHD, VEnding, VCG0* quests), `faction` (another base-game quest whose stage scripts
  change a reputation [hypothesis: a working rule for ranking, not a category in the data]),
  `add-on` (any other plugin's), `other`. The `untouched` list names them in that order.
- `chains`: each chain's step and given errors (they should all be 0).

Picking the next chain: take the first quests of the `untouched` list (the main quest, then
factions, then add-ons); `--detail` gives per quest the stages reached and its completing
stages, per script the blocks run, per actor the lines said, so a quest half walked shows
which stages are left. Totals at the end of the second batch: §1's table.
