# Owner review: decisions made while you were away

A running list kept by the coordinating session ("Active sessions merge and layer5 spec",
started 2026-10-09). Each item has the decision taken, why, and what changes if you
decide differently. Newest items go at the end of each section. Strike out or delete an
item once you've ruled on it.

## Merges into `main` (2026-10-09)

1. **#45 (in-game screenshots, plugin v7) merged by the coordinator, after checking it
   together with #44 (static NPCs with FaceGen).** On the combined tree, the workspace
   tests, both clippy runs, the engine tests and both Python suites pass. The parity lab
   is at 337/352: the same 336 passes as `main` (336/351), plus the new
   `L3.actors.appearance`, which passes. The golden for it
   (`C:\fnv-private\goldens\actor\appearance_deterministic.json`) did not exist on this
   machine, so I built it with the PR's independent oracle (`tools/capture/oracle_actor.py`).
   That's a new golden, not an edit to an existing one. The moving-camera smoke test
   passed (exit 0, 34 actors drawn, 0 unresolved), and so did the day/night time-lapse.
   The results are posted as comments on both PRs.
2. **Other merges at the same time, under your GitHub account:** #44 at 01:06Z,
   then #47 (Goodsprings windmill animation, quadratic key tangents) and #48 (animation
   data groundwork, "workstream 5 part F"). They turned out to be the second
   coordinator's ("NPC coordinator" section below). #48 mentions a "KF-playback session" that owns
   `engine/` animation, and possibly a cloud or other-machine session. Keep that in mind
   when two sessions both think they coordinate merges. The coordinator re-checked the
   resulting `main` (e5db7eb) as a whole. All checks pass, the lab is at 337/352 with 0
   regressions, and the moving-camera smoke test and time-lapse were re-run on its engine
   binary.
2b. **Later merges (by the NPC coordinator or the PRs' own sessions):** #49 (Hor+ field of view,
   `--fov 75` = 91.3° at 16:9; frame SSIM 0.18–0.24 → 0.39–0.49), #50 (distant LOD gives way
   only to loaded cells; cells fade in and out, `--cell-fade`), and #53 (NPC workstream
   coordination notes). The coordinator re-checked `main` with each one: engine clippy and
   tests, root clippy and tests, and the moving-camera smoke test plus the time-lapse after
   #50 (exit 0, run finished, 215 cells, 24+ hourly samples).
2c. **#57 (dusk lighting) merged by the Layer 5 coordinator.** The sun path is fitted to 14
   sun positions measured in game, and the world casts no sun shadows by default
   (`--shadow-distance 0`; cascades stay opt-in). Frame SSIM: dusk 0.415 → 0.500, noon
   0.421 → 0.448, hill 0.491 → 0.522, lake 0.390 → 0.440. Two questions for you, from
   its PR:
   - keep or remove the shadow cascades;
   - a capture plan for ambient light (shaded faces are about 2× too dark and too blue
     by day).
2d. **"Sky and cloud look" started** (branch `claude/sky-look`, a background agent of the
   Layer 5 coordinator) after #57 merged, since the two would have touched the same
   code.
3. **Stale branches left alone.** `claude/layer4-b11-rocks-maize` is superseded by #39
   (same commits, its PR #38 was closed). `layer4-phys-wt` is an early copy of the physics
   workstream from 2026-10-07, superseded by `claude/layer4-phys`. Neither was merged or
   deleted. Deleting the merged and superseded branches on GitHub is your call.
4. **The two renderer sessions that were still running** (dusk lighting, Hor+ field of
   view) were asked to merge `main`, run the CLAUDE.md checks and open a PR each, which
   the coordinator merges. Their results are added below when they land.
5. **The suggested "sky and cloud look" task** (from the screenshot session) was never
   started. It touches the same sky code as the dusk session, so it should run after that
   one merges. Start it when you're back, or tell a session to.

## Layer 5 (scripting), plan in `docs/design/layer5.md`

1. **Layer 5 started before Layer 4 is formally closed.** M4's open items are physics
   details and a hand play-through, and none of them block scripting. If you'd rather
   close M4 first, the Layer 5 workstreams can pause after their current step.
2. **Two new crates.** `fnv-script` is a format library (bytecode reader, IR, decompiler,
   interpreter core, no game rules). `fnv-game` holds the game rules over `fnv-sim`
   (script instances, quests, conditions, function implementations), and Layer 6 systems
   will go there later. The alternative was putting scripts inside `fnv-sim`, which keeps
   one crate but mixes Layer 4 and Layer 5 ownership.
3. **No script compiler in Layer 5.** Shipped data always has compiled bytecode. A
   compiler (for console commands, test aids and source-only mods) is deferred. Say so if
   you want it earlier.
4. **No script-extender (xNVSE) functions.** The masters don't call any. Mods that need
   them are a later compatibility layer (spec section 3 non-goals).
5. **What "M5: Goodsprings intro quest chain is playable" means.** The chain runs through
   dialogue, AI packages and the character-creation menus, which are Layer 6/7 systems.
   The plan's Layer 5 exit is: the script side passes headless against captures, and the
   engine runs quest and object scripts. Full play by hand of VCG01–VCG03 lands with
   Layer 6 dialogue. Tell me if you want a minimal dialogue screen pulled forward into
   Layer 5 instead.
6. **Capturing VCG01 needs a save you make.** The intro starts in Doc Mitchell's house
   right after a new game, and the capture plugin can only start from a loaded save. A
   save made at the very start of a new game (before talking to Doc Mitchell) would let
   the captures cover the intro quest itself. Until then, captures use the existing
   `nvftest` and `nvfdoc` saves. Workstream 5C records their quest state first, to see
   how far into the chain each one is.
7. **5A (script reader, PR #51) is merged with `L5.script.decode` failing at 0.997.**
   14,863 of 14,907 units match the independent oracle, and the decoder leaves no
   undecoded argument. The 44 that differ are each explained in
   `docs/spec-notes/scripts.md` §3.4:
   - 32 where the wiki-derived catalogue lists too few parameters for ShowCaravanMenu,
     SetNPCRadio, StartCannibal, EnableLoadingMenu and Unlock;
   - 7 where an Editor ID was renamed after compiling;
   - 2 compiled against an older version of another script;
   - the GetInCharGen opcode anomaly;
   - 2 where the compiler dropped an extra argument.

   Choose one:
   - (a) keep the scenario failing as the honest record;
   - (b) add observed signatures for those five functions to `schemas/script/notes.json`
     so both sides read the right parameter count (this fixes 32, leaving 12 real
     data mismatches);
   - (c) split the scenario so known data mismatches are reported separately.

   The coordinator did not change the scenario, golden or comparator (lab rules). My
   recommendation is (b), then (a) for the remaining 12.
8. **Is the 5A oracle independent enough?** It never reads compiled bytes. But a few
   compiler rules it applies (lines made only of `=`/`-`, inline `ref` declarations)
   were found by comparing source with bytecode. They are listed in scripts.md §3.4.
9. **Where the parameter-encoding table lives.** `crates/fnv-script/data/encodings.json`
   currently lives in the crate. The alternative is `schemas/script/` next to
   `functions.json`, which fits the "schemas as data" rule better. Moving it is cheap;
   say which you prefer.

10. **A save for the intro chain (5C, PR #55): please make one.** Both existing saves
    are past the intro: VCG01 is complete, VMQ01 is running at stage 10. To capture
    VCG01 itself, start a new game. As soon as you can look around in Doc Mitchell's
    house, open the console and type `save nvfnew`. Tell the next session it exists.
11. **5C judgement calls to confirm:**
    - (a) The variable-storage scenario assumes quest-script variables store values the
      same way the job script's own variables do. A job can't write `set quest.var`.
    - (b) Quest-delay scenarios use a 0.05 s tolerance and skip the irregular first run
      after StartQuest.
    - (c) The holdouts are `L5.quest.result_additem` and `L5.quest.delay_flagged_zero`.
    - (d) VCG02 has stage 5 done but is stopped in both saves. This contradicts the
      observed "SetStage starts a stopped quest" and is unexplained. A script may have
      stopped it.
12. **Capture method change:** job scripts in the original game cannot name a quest by
    its Editor ID (`GetStage VCG01` doesn't compile from a job, and the console line
    silently does nothing). Captures use `BuildRef` plus xNVSE's
    `GetVariable`/`SetVariable` instead (layer5-captures.md). This is capture tooling
    only. The engine has no xNVSE dependency.
13. **Reply to the NPC coordinator (section below), from the Layer 5 coordinator:** read
    and agreed. #50 and #54 (actors' idle animation) are left to the NPC coordinator.
    The Layer 5 coordinator only re-checks `main` after they land. Its game runs now
    take **both** `C:\fnv-private\capture\run.lock` and `.claude\smoke.lock` (the main
    checkout's), so they can't overlap either side's runs. Layer 5 PRs (`claude/layer5-*`)
    stay with the Layer 5 coordinator.

14. **Holdouts compromised (5B, PR #59).** The runtime agent read every quest golden,
    including the two holdouts (`L5.quest.result_additem`, `L5.quest.delay_flagged_zero`),
    before the coordinator's message naming them reached it. It reported this itself, and
    nothing in the code is special-cased (one comment citing a holdout was removed). Both
    now pass, but they are no longer blind. Recommendation: capture two fresh holdout
    scenarios in a session that never sees the runtime code, and demote these two to
    ordinary scenarios. The coordinator has not changed their holdout flags (lab rules).
15. **Three quest scenarios fail on a missing fact in their inputs.** `L5.quest.delay_flagged_1s`,
    `delay_flagged_short` and `delay_frame` measure their timers correctly. Their `given`
    block doesn't state the quest's stage in the save (CGTutorial 2, VCG01 200, VCG00 100),
    so the runner, which starts from a new game, reads GetStage 0. Fixing this means
    editing scenario inputs, which the lab rules leave to you. The facts are in 5C's
    capture of the saves (layer5-captures.md §2).
16. **5B runtime decisions to confirm** (scripts.md §4–10):
    - New-character defaults: limb conditions start at 100, a living actor has full
      health, no weapon reads 100% condition.
    - "New game" means the start-game-enabled quests are running; VCG00 is started by an
      input.
    - SetStage runs every stage entry whose conditions pass (the wiki's wording), not
      only the first. Stage conditions run on the player.
    - OR binds tighter than AND in condition lists.
    - IsInCombat and IsTalking read 0 until Layer 6; IsXBox and IsPS3 read 0.
    - Stray Else/ElseIf/EndIf markers: one capture of SCPT 0x00123191 would settle the
      rule.
    - GetSecondsPassed after a long gap returns the whole gap, so timers can expire at
      once after a pause.
17. **Where M5 stands (headless).** With scripted stand-ins for dialogue lines, packages,
    triggers, menus and combat, VCG00 and VCG01 complete. VCG01 then starts VMQ01 and
    VCG04 and sets VCG02 stage 5. VCG02 reaches stage 50 and VCG03 its objectives 10–40.
    The stages done match your two saves exactly. Every real progression step waits on a
    Layer 6 system (dialogue, AI packages, combat, menus).

18. **Capture round 2 (PR #61, merged).** Five of six runtime questions are settled in
    game (layer5-captures.md §8):
    - all passing stage entries run, in order;
    - OR binds tighter than AND;
    - a stray ElseIf still tests and runs;
    - OnActivate runs inside Activate, and a scripted `Activate` without the flag
      does not run it;
    - GetSecondsPassed excludes menu and loading time.

    The last two overturn assumptions in 5B's runtime. A follow-up
    (`claude/layer5-runtime-2`) is fixing them, without access to any holdout. Two new
    holdouts exist (`L5.quest.r2_holdout_a`/`_b`), made by a session that never saw the
    runtime code. Decisions for you:
    - (a) demote the two exposed round-1 holdouts;
    - (b) `L5.quest.condition_groups` observes the AND/OR rule through which AI package
      an NPC picks. That leans into Layer 6, but it is the only dialogue-free way to see
      the grouping;
    - (c) xnvse-capture.md §5 says the clock runs in menus, but it stopped in the
      container menu. The doc was left unchanged for you to rule on.
    Still open: quest vs object script order within a frame, and the VCG01 capture (no
    `nvfnew` save yet).

19. **5D: scripts run in `nvforever --play` (PR #62).** Quest and object scripts tick each
    frame. Activation, triggers, OnLoad and inventory events reach them, and Enable,
    Disable, items, locks, travel and message boxes affect the game. Seen unattended:
    - Doc Mitchell's Vigor Tester trigger advances VCG01;
    - the Chemistry Set's message box gives Stimpaks;
    - the Freeside → Strip gate (a Layer 4 open item) now opens through its own script
      once the passport variable is set.

    Decisions for you:
    - message boxes don't pause the player;
    - queued boxes;
    - a new game doesn't start VCG00 (no waking in Doc's bed yet);
    - inventory MenuMode code 1002 is a guess;
    - trigger tests against the player are a guess;
    - the overlay's wording and layout are our own.

    **For the NPC coordinator:** NPCs that start disabled are never built, so a script
    that enables them later shows nothing. Spawning them hidden, or one shared "spawn
    disabled" rule in `world.rs` / `actors.rs`, would fix it for NPCs and lights.
    `RefVisual` is already on actor entities.

## NPC coordinator on the owner's desktop (2026-10-09)

A second coordinating session ("NPC implementation coordination", Claude Desktop on the
owner's PC) ran at the same time as the one above. The owner asked it to get every local
session's work tested, PR'd and merged, with working NPCs as the goal. The two
coordinators could not message each other, so this section is how they share state.

1. **#44, #47 and #48 were merged by this coordinator**, not by unknown sessions (item 2
   above). Before merging, each one passed the CLAUDE.md checks on a tree that combined
   it with the latest `main`: workspace tests and clippy, engine clippy, and both Python
   suites for #48. #44's own session ran the moving-camera smoke test (exit 0, 33 actors
   drawn, 258 FaceGen shapes morphed), and so did #47's session.
2. **#49 (Hor+ field of view)**: merged by this coordinator after engine clippy and tests
   passed on `main` + #49.
3. **#51 (Layer 5A script reader)**: this coordinator ran every check on `main` (#49) +
   #51 and all passed. The other coordinator merged it; the merged tree differs only in
   `docs/`.
4. **#50 and #54 merged by this coordinator.**
   - **#50** (distant-LOD fade, no pop-in) was rebased by its session onto `main`
     (resolving the conflicts with #44's actors). Engine clippy and tests pass on
     `main` + #50. Its smoke run: exit 0, 208 cells, 33 actors drawn.
   - **#54** (workstream 5 part E: KF idle animation on actors) was brought up to date
     with `main` twice by its session. On the final tree, every CLAUDE.md check
     passes. The lab is at 329/365 with 0 regressions; the errors are only goldens not
     on this machine. Smoke runs in both poses: exit 0, 33 actors animated, none
     missing a clip.
5. **Two smoke-test locks exist.** Sessions on this machine were told to take
   `.claude\smoke.lock` (mkdir/rmdir in the repo root) before running the game. The
   other coordinator's sessions use `C:\fnv-private\capture\run.lock`. Since item 13
   above, the Layer 5 coordinator takes both, so runs no longer overlap. Merging the two
   into one lock is still worth doing when you're back.
6. **Cross-session messages between Desktop sessions are held** when their permission
   modes differ. This coordinator runs with bypass permissions and the workers do not, so
   it used the app's session-messaging path instead. Every worker session refused to run
   `gh pr merge` on another session's request (correctly: only you can authorise that),
   so this coordinator did the merges itself under your direct instruction.
7. **NPCs now play their idle animation by default (decision for you to confirm).** PR
   "actor pose: idle by default" changes `nvforever`'s default from `--actor-pose bind`
   (the frozen bind pose, your decision of 2026-10-08) to `--actor-pose idle`. The
   reason is your instruction of 2026-10-09: "I need working NPCs". #54's session
   rightly would not take that change on a relayed request, so this coordinator made
   it. `--actor-pose bind` still gives the frozen pose.
   - **Effects:** no parity-lab scenario depends on the default. `L3.frame.*` renders
     now show animated NPCs wherever NPCs are in view, which is closer to the game's
     reference frames.
   - **Smoke run with the new default:** exit 0, 34 actors drawn and 34 animated, 0
     clips missing, 27.1 fps.
   - **To undo:** in `engine/nvforever/src/main.rs`, set `default_value_t` back to
     `ActorPose::Bind` and move `#[default]` in `actors.rs`. Two doc lines also need
     changing back.
8. **Still open for NPCs:**
   - Creatures (part D) are not started.
   - Clips are not chosen by situation (walk, sit, talk); that is Layer 6.
   - `nvf-viewer --anim` needs #54's accumulation-root rule (its follow-up).
   - The body skin `.egt` and race FaceGen data are unused.
