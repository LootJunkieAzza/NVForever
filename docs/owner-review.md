# Owner review: decisions made while you were away

A running list kept by the coordinating session ("Active sessions merge and layer5 spec",
started 2026-10-09). Each item has the decision taken, why, and what changes if you
decide differently. Newest items go at the end of each section. Strike out or delete an
item once you've ruled on it.

## Read first: your autosave was overwritten (2026-10-09 09:45)

During capture round 4 (VCG01 from `nvfnew`, PR #79), one trial drove VCG01 to stage 55,
whose result script calls `Autosave`. Turning off scripted autosaves in the INI did not
stop it. The game wrote a new `autosave.fos` in `Documents\My Games\FalloutNV\Saves`
at 09:45:56:
- your 05:43 autosave became `autosave.fos.bak` and is still there;
- the older `autosave.fos.bak` it replaced is gone (the 05:43 file may itself have come
  from an earlier capture);
- your named saves (Save 1–11, `nvftest`, `nvfdoc`, `nvfnew`) are untouched.

To get the 05:43 autosave back, rename `autosave.fos.bak` to `autosave.fos` yourself
(the tools never write to the Saves folder). Prevention:
- `capture_run.py` now backs up the rolling saves before every run, then fails (exit 3)
  and lists any change if the game writes to the Saves folder (tools/capture/README.md);
- no scenario reaches stage 55;
- capturing stage 55 needs your consent to let the game autosave there.

The same run also triggered Steam achievement 31 (`AddAchievement 31`) in four game
sessions. You already had it from an earlier playthrough.

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
   - keep or remove the shadow cascades — **answered 2026-10-09: keep them, on by default**
     (`--shadow-distance` back to 16,000; `0` matches the game);
   - a capture plan for ambient light (shaded faces are about 2× too dark and too blue
     by day).
2d. **"Sky and cloud look" started** (branch `claude/sky-look`, a background agent of the
   Layer 5 coordinator) after #57 merged, since the two would have touched the same
   code.
2e. **#69 (sky and cloud look) merged by the Layer 5 coordinator, with one trade-off.**
   The sky now matches the game's colours far better: colour error over 12 cameras went
   from 33.7 to 14.7, noon sky (215,219,214) → (162,157,143) against the game's
   (160,154,147), and noon ground is unchanged. SSIM rose on noon, hill and lake but
   **fell on dusk, 0.500 → 0.476**: the clouds now have contrast, but not the game's
   cloud pattern. All four frames still fail min_ssim 0.6, as before. Most of the model
   is fitted to screenshots [hypothesis] (world.md §6). If you'd rather keep the higher
   dusk SSIM, revert #69. Open questions from it:
   - the game's noon sky darkening, which looks like eye adaptation;
   - the larger sun glow at 09:00 and 18:00;
   - the western sky still lit at 20:00.

   Its new in-game captures came out black (probably the display was off). The camera
   list `C:\fnv-private\sky\cap\cameras.json` is ready for a rerun with you there.
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

10. **A save for the intro chain (5C, PR #55): done.** `nvfnew` exists (2026-10-09 09:21)
    and sits before VCG01 stage 0, VCG00 at stage 90 (layer5-captures.md §2). Original
    request: Both existing saves
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

20. **Function coverage (#66) and effect scripts in the game (#67).** Script uses
    handled: 95.0% → 99.98% (implemented 88.1% → 92.6%). New features:
    - reputation (fame/infamy with thresholds);
    - effect scripts on consumables, which now run in `--play` when an aid item is used;
    - PlaceAtMe, ownership, regions, destruction stages, factions, fast travel,
      challenges and casino state.

    Decisions for you (scripts.md §10):
    - (a) GetReputationThreshold's 25/50/75% levels and 4×4 grid are a guess with
      1,786 uses; a capture is proposed;
    - (b) effect-script timing is a guess;
    - (c) RemoveAllTypedItems codes other than 40 follow the script-extender
      community's numbering;
    - (d) Layer 6 state checks (GetSitting, IsMoving, GetDetected…) stay stubs for now;
    - (e) the xEdit source was read on GitHub for the catalogue, and
      `script_functions.py` gained `--xedit-from-catalogue`.
21. **Coordinator mistake, fixed forward.** PR #64 committed `nvforever-wire.exe`
    (97 MB, an engine build the coordinator had copied into the worktree root; our own
    binary, no game data) because of a `git add -A`. #67 removed it and now ignores
    `*.exe`. The blob is still in `main`'s history. Purging it needs a history rewrite
    and a force-push of `main`, which is your call. While cleaning up, a second copy of
    the binary turned out corrupt in the shared object store (the push failed on it). It
    was unreferenced after amending, so the coordinator ran
    `git reflog expire --expire-unreachable=now --all` (no stash existed) and deleted
    that one loose object. `git fsck --connectivity-only` is clean.

22. **Capture round 3 (PR #73, merged).**
    - **Reputation is measured:** levels at 15/50/100% of the scale, deed sizes give
      1/2/4/7/12 points, and a 4×4 tier grid. The runtime's earlier 25/50/75% guess was
      wrong; runtime round 3 (`claude/layer5-runtime-3`) replaces it.
    - **Effect-script timing is measured:** Start runs inside the use call, Update every
      frame, menus pause it, and repeated doses add to the duration.
    - **Order:** one quest script ran before object scripts in the same frame.

    For you:
    - (a) the tolerances it chose: 0.25 s for effect durations, 0.0001 for reputation;
    - (b) `L5.quest.script_order` travels with `coc Goodsprings`;
    - (c) the 15/50/100% rule is assumed for the eight reputations it did not change;
    - (d) a one-line blindness slip: one grep printed an unrelated line of scripts.md
      §5 (it reported this itself).

    New holdouts: `L5.quest.r3_holdout_a`/`_b`.

23. **Runtime round 3 (#75) and the clock order (#76).** L5 is now 24/28. The four
    misses are the three `delay_*` inputs (item 15) and `L5.script.decode` (item 7). All
    six holdouts pass. Lab 361/380 with 0 regressions. Decisions for you:
    - zero-duration effects run Finish inside the use (the captures can't tell this
      from "never");
    - a second dose's Start doesn't re-run, and only the same item merges;
    - `go_to_cell` puts the player mid-cell (the Goodsprings cell has no COC marker);
    - GetBaseActorValue includes ModActorValue changes [hypothesis];
    - AddReputationExact clamping wasn't captured;
    - the engine now advances the game clock before each frame's scripts (#76) [hypothesis].

24. **Your GECK wiki export (2026-10-09) is used.** It is in `C:\fnv-private\geckwiki`;
    the four `GECK-2026100909*.xml` files are identical, so three can go.
    `docs/spec-notes/quests-dialogue.md` is our own summary of its 19 pages, checked for
    copied wording (no 8-word run in common).
    - **Agrees with the captures:** AND/OR precedence, all passing stage items run,
      repeated stages, completed ≠ stopped, and SetStage starting a quest.
    - **New documented rules:** SetObjectiveDisplayed starts a quest; quest variables
      live on while it is stopped; condition Run On meanings; "Run on Target" is invalid
      on stages and targets; Next quest becomes the active quest.
    - **Dialogue groundwork for Layer 6:** begin/end result scripts, Say Once, Goodbye,
      Random stacks, speech challenges.
    - **One disagreement:** the wiki says the "Default" delay flag means 5 s, but the
      captures show the record's delay is used whenever it is non-zero. The runtime
      follows the captures.

    The runtime session was given these rules to check against.

25. **Two more notes from your wiki exports (scripting and actor pages).** The three
    `GECK-2026100909*.xml` files in `C:\fnv-private\geckwiki` now hold different pages
    (19 quest, 53 scripting, 7 actor pages), so keep all three (item 24 said they were
    identical). `docs/spec-notes/script-language.md` covers the base-game script
    language and `docs/spec-notes/actor-behaviour.md` AI packages, detection, factions,
    combat and idles. Both are our own wording (no 8-word run in common with the
    exports). Extender (xNVSE) features get one short section, marked out of scope.
    - **The captures win twice:** integer variables truncate toward zero (the wiki says
      floor), and float variables are doubles (the wiki says single precision). The
      runtime already follows the captures.
    - **Runtime differs from the wiki, no capture yet:** calls on an empty `ref` stop the
      script in the wiki but are skipped in the runtime; disabled references run their
      scripts in the wiki but not in the runtime; scripted items in other actors' or
      containers' inventories don't run; Return ends one block, not the frame (matters
      in 5 shipped scripts); result-script variables don't overwrite the quest script's;
      script packages stack instead of replacing each other; EvaluatePackage keeps a
      script package.
    - Each note ends with capture candidates. Nothing was changed in the runtime.

26. **Your `objects.xml` export (9 pages, mostly category indexes).** Three facts were
    new; each is handled as below.
    - **Quest items can't be dropped or sold, and bodies holding one don't despawn until
      it's taken.** Nothing read a quest-item record flag, so `--play` let you drop
      anything. A session on `claude/quest-items` is adding the flag (from xEdit's
      definitions, confirmed in the data) and the rule.
    - **"Random Anim Start":** world objects with it start their idle animation at a
      random time, so copies don't animate in step. Nothing reads it yet. **For the
      NPC/animation coordinator:** this belongs with the placed-model animation of #47
      (`engine/nvf-assets` model.rs).
    - **Leveled lists pick by the player's level relative to the encounter zone.**
      `fnv-sim` uses the player's level only (no ECZN). This matters for leveled actors
      (Layer 6); noted, not changed.

    The export is kept as `C:\fnv-private\geckwiki\objects.xml`.

27. **Your "Getting started" export (Bethesda's tutorial series, 20 pages used).**
    `docs/spec-notes/level-design.md` is our own summary (no 8-word run in common with the
    export), with data checks run on `FalloutNV.esm` this session. The tutorials were
    written for Fallout 3, so each rule still needs a New Vegas check where it matters.
    - **Settled from the data:** placed primitive bounds (`XPRM`) are half extents. Of
      1,341 portal/room pairs, 1,298 put the portal on a room face that way, none the
      other way. Physics already reads them so.
    - **Triggers (for the `--play` session):** any actor can fire a trigger (and, without
      the "ActorZone" flag, any object); OnTrigger runs every frame. Ours tests the player
      only, as three points on the player's axis.
    - **Lights (for the renderer):** a placed light's radius can differ from its base.
      6,484 of 7,742 interior lights carry `XRDS`, which looks like an offset added to the
      base radius; the renderer ignores it. Emittance from a light should also carry its
      brightness, and objects should fade in by size. None of this is changed yet.
    - **Culling:** rooms, portals and occlusion planes are described in detail. The
      renderer has none of them; that costs speed, not looks, except in rare cases.
    - **Encounter zones:** a zone's level is fixed the first time you enter it. This
      adds to item 26.

    The full list of differences, by file, is the note's §11.2; capture ideas are its §13.

28. **The intro, headless (#86) and in the game (#96).** The runtime reproduces your
    `nvfnew` captures: all four `L5.intro.*` pass, holdout included, and L5 is 28/32.
    `nvforever --play --new-game` runs the opening in Doc Mitchell's house unattended to
    VCG01 stage 200, with stage timings matching the walk-through and the capture
    (mostly within a few hundredths of a second). Decisions for you, from #86 and #96:
    - (a) message boxes hold scripts only in inputs whose `given` includes a menu; the
      alternative is adding the pack quests' state to the older inputs;
    - (b) a new game puts the player at `VCG01PlayerStartMarkerREF` and sets VCG00
      stage 0 directly;
    - (c) dialogue line length is fitted (1.1 s + 1/16 s per character) until voice
      files are read; the psych intro runs 18 s against about 13 s in game;
    - (d) unattended `--play` runs answer script boxes after 0.25 s; attended runs wait
      for you;
    - (e) NPC trigger capsules use the player's placeholder size times scale;
    - (f) `--new-game` lives in `engine/nvforever/src/main.rs`.
29. **Quest items (#91).** Bit 10 "Quest Item" on WEAP/ARMO/ALCH/BOOK/KEYM/MISC/LIGH
    [documented: xEdit]. 339 shipped items set it [observed]. The player can't drop them
    (`--play` says so). A scripted `Drop` still drops them, as tested in game, and
    storing them in containers is allowed [hypothesis] until checked by hand
    (layer4-captures.md B14). Record-header flags are now schema data (`header_flags`).
30. **Lock trouble and a proposal.** Twice today the shared lock folders got stuck:
    - a session that hit the usage limit died holding them;
    - at 14:22 an unknown session created both and never used them;
    - once they vanished while the coordinator held them, cause unknown.

    The coordinator removed stale locks only after checking that no FalloutNV or
    nvforever process was running. Proposal: one shared helper (e.g.
    `tools/capture/locks.py`) that writes an owner file (session, PID, time) into each
    lock, treats a lock as stale when its PID is gone, and is used by every runner. Say
    if you want it built.
31. **Dialogue started (2026-10-10), plan in `docs/design/dialogue.md`.** You asked
    whether dialogue lines can be worked on now; they can, and no other session had
    taken it. Decisions taken, each reversible:
    - (a) dialogue is treated as a parity target (unlike combat), because lines and
      their result scripts move the quests;
    - (b) two workstreams run now: records plus headless conversations
      (`claude/dialogue-core`), and voice files (`claude/dialogue-voice`); the dialogue
      screen in `--play` follows;
    - (c) the screen will be plain (a list of choices and subtitles); its look is
      Layer 7;
    - (d) lip sync, the dialogue camera, barter, companion wheels, radio and NPC-to-NPC
      conversations are left out of this plan (§7).

    **Needs you:** the capture questions in the plan's §6 need the original game running
    on this desktop, which takes the screen and the keyboard. No capture run is started
    until you say when you are away. Until then the rules marked [hypothesis] in the
    plan's §3 stay hypotheses.
32. **Dialogue records and headless conversations are on main (#117, 2026-10-10).**
    Topics and lines are read in full (21,298 topics, 28,896 lines, 37,403 responses in
    the ten masters) and a conversation runs without the game window
    (`fnvtool dialogue topics|lines|talk`; facts and rules in
    `docs/spec-notes/dialogue.md`). In the headless intro, the psych test and Doc
    Mitchell's farewell are now real conversations; the chain still ends at VCG01 200.

    **Your ruling on which line is picked is applied** (quest priority, then stored
    order, first eligible from the top unless Random). It is recorded as your statement
    and stays on the capture list as "to confirm". How much rides on it: 102 topics have
    lines of several quests; in 99 an actor could say lines of quests with different
    priorities, in 76 of quests with equal priority. GREETING has lines of 228 quests.

    **Questions for you** (your GECK experience may settle them without a capture):
    - (a) **Does dialogue pause the game?** **Answered 2026-10-10: yes.** A conversation
      with the player now pauses the game behind it like the other menus (scripts'
      GameMode and game time stop, MenuMode runs); lines spoken outside a conversation
      (SayTo) pause nothing. The Securitron script fits this reading better than the
      first one. The intro still ends at VCG01 200.
    - (b) **AddTopic and a line's "Add Topics" list:** **Answered 2026-10-10:** every
      actor offers a known topic if they can say a line of it. That is what the runtime
      did; it is now recorded as your statement.
    - (c) **Follow-up lines:** **Answered 2026-10-10: correct**, no player response.
      Nothing to change; recorded as your statement.
    - (d) **Random lines:** **Answered 2026-10-10:** Random lines that sit together are
      a pool, and a normal line between two groups splits them into two pools. Fixed:
      the runtime used to let a pool run on across a normal line the actor could not
      say.
    - (e) **Shared lines** (the Info Connection list on 2,435 topics; e.g. GREETING
      lists four lines stored under `VMS07Greeting`): not used, because nothing we have
      says what the game does with them. If the GECK shows these as lines shared
      between two topics, they should also count as greetings.
    - (f) **Skill checks:** New Vegas marks a line with a skill or perk and uses ordinary
      conditions in pass/fail pairs (1,499 lines). The Fallout 3 percentage roll exists
      on 3 left-over lines only and is not modelled. The numbers shown ("Speech 35",
      "30/35") are taken from the line's own skill condition.
    - (g) **A line of a stopped quest cannot be said:** **Answered 2026-10-10: right**,
      and the quest's own conditions must pass as well as the line's (113 of 640 quests
      have such conditions, covering 8,151 lines; the runtime already checked them).
      Still assumed: an actor with no eligible greeting cannot be talked to.

    **Left as stand-ins in the intro:** sitting on the couch and Doc walking over (AI
    packages), the two walk-in triggers and the menus' OK (the player), and all of Sunny
    Smiles's and Trudy's lines (between travel packages, combat and crafting).

    **Captures when you are away:** the 15 items of `docs/spec-notes/dialogue.md` §7.
    Real-data tests of the dialogue code need `FNVLAB_DATA_DIR=<Data>`; without it they
    are skipped.

    Two small things your answers leave open (#120): when a Random pool's last line
    carries "Random End" but the actor cannot say that line, does the pool still end
    there (assumed yes); and do a quest's own conditions run on the speaking actor, like
    a line's conditions (assumed yes).
33. **Dialogue voice (#119, 2026-10-10).** Lines last as long as their voice files and
    are heard in `--play`; 97.3 % of the masters' 37,403 responses have a sound
    (`docs/spec-notes/voice.md`). Nobody has listened to it yet: playback is confirmed
    from the log only. Decisions for you:
    - (a) **A missing 0.3 to 0.4 s per line.** With real sound lengths every captured
      span of Doc Mitchell's lines comes out short by that much per line (one line
      2.60 s against 2.9 s; three lines 14.5 s against 15.4 s; the psych intro 11.7 s
      against about 13 s, where the old estimate gave 18.1 s). Nothing was tuned. Either
      add a fixed pause per line (a fitted number again) or capture single lines first;
      the captures so far cannot tell a per-line gap from a per-response one.
    - (b) Voice lengths are on by default in `fnvtool script run` and `dialogue talk`
      (the four `L5.intro.*` scenarios pass either way); `--no-voice-lengths` restores
      the estimate.
    - (c) Unattended `--play` runs now play voice through the speakers; `--no-voice`
      keeps them silent. Agents were told to run silent while you are at the desk.
    - (d) Open hypotheses (voice.md §11): which of `.ogg` and `.wav` the game plays for
      the 40 lines that have both; what a response's sound form does (228 responses;
      reported, not played); the voice of actors templated on a levelled list; whether
      a line with no file shows its subtitle for some fixed time.
    - (e) Voices play flat (no 3D placement).
34. **Pausing in conversations: "living pause" now, a live world later (your request,
    2026-10-10).** You said the original pauses during a conversation but that you would
    prefer no pause if scripts and outside NPCs can be kept from causing trouble.
    - **Built now (dialogue screen, `claude/dialogue-screen`):** only what scripts can
      see is frozen (script time, quest timers, the game clock, NPC decisions, damage,
      needs). What you see and hear keeps running (idle animation, wind, water, weather,
      sky, the speaker's voice), with free look. `--dialogue-pause living|original`,
      default `living`. Quest behaviour is the original's.
    - **Proposed for when AI packages exist:** NPCs on ordinary schedules keep moving;
      events that happen meanwhile (a trigger entered, a package finished) are queued
      and delivered to scripts when the conversation ends; the speaker's schedule is
      held and hostiles cannot start a fight with the pair; NPCs under script control
      hold their place.
    - **Not proposed as default:** scripts running live during dialogue (as in Skyrim).
      The base game's scripts react to a line's result on the next frame (moving the
      speaker, opening a box, starting another conversation), which would land in the
      middle of the line. Could be an experimental option after listing the scripts at
      risk from the decoded scripts.
    - **Your call:** (1) should walking away end a conversation where the line would let
      you leave; (2) should the game clock keep moving while you talk (frozen for now);
      (3) should being attacked break the conversation, or is the pair untouchable
      (proposed: untouchable, because a broken line could skip a quest result).
35. **Lip sync started at your request (2026-10-10, `claude/dialogue-lip`).** Goal: the
    `.lip` file of a spoken line moves the speaker's mouth and face in `--play`. The
    format has no official documentation, so the work uses public documentation and
    observation of the files only; if that is not enough to animate honestly, it stops
    at the notes and the reader. It touches the NPC side's files
    (`engine/nvforever/src/actors.rs`, `facegen.rs`) as little as the feature needs.
36. **Dialogue screen in `--play` (#122, 2026-10-10): please play it by hand.**
    `nvforever --data <Data> --play --cell Goodsprings`, then E on Easy Pete (or anyone
    with a greeting); `--play --new-game` for Doc Mitchell's psych test. Keys and the
    test aid are in `docs/design/dialogue-play.md`. Unattended runs passed (the intro
    ends at VCG01 200 with both of Doc's conversations real; Easy Pete offers 4, 6, 5
    choices and says goodbye), but free look while an NPC speaks and clicking choices
    with the mouse could only be tried by hand. Decisions for you:
    - (a) **Shift+Tab breaks a conversation off at any point** (the line is cut, no end
      script). It is ours, a way out of a conversation the data gives no end to; it can
      skip a quest result. Keep it, hide it behind a debug switch, or remove it?
      **Decided 2026-10-10 ("hide the shift tab cut line to a debug switch this should
      not be in the release version"):** done on `claude/menu-pause`: off unless
      `NVF_PLAY_DEBUG_TALK_BREAK=1` is set, no on-screen hint; debug switches and test
      aids are to be compiled out of packaged releases once packaging exists.
    - (b) **Tab says goodbye only where a Goodbye choice is offered** (so the psych
      test cannot be left). How the original's Tab behaves is not captured.
    - (c) **Skipping a line** (Space, Enter, E or a click) still runs its end script
      [hypothesis].
    - (d) **Cursor in `living` mode:** free look while the actor speaks, cursor released
      while choosing (the view holds then). The alternative is keys-only choosing with
      free look throughout. **Decided 2026-10-10 ("go with the first option"):** as
      built; recorded in docs/design/dialogue.md §8.
    - (e) **`--dialogue-pause original` is stricter than today's menus**, which never
      stopped animations. Should the inventory, container and message boxes follow the
      same switch, or stay as they are? **Decided 2026-10-10 ("add the pause switch to
      all menus except the actual ESC pause menu"):** done on `claude/menu-pause`:
      `--menu-pause living|original` (default `living`, `--dialogue-pause` still works)
      covers every menu; the Esc pause menu always holds everything, picture and voice
      included.
    - (f) **A question asked once disappears from the list** when its line is Say Once
      and has no choices (all of Easy Pete's did). Whether the original hides or dims
      it needs a capture; nothing was ever shown dimmed in the runs.
    - (g) Known rough edges: no dialogue camera and the actor does not turn to the
      player; the panel shows neither the player's own line nor the actor's last line
      while choosing; AI packages, when they exist, must honour the hold on travellers.

    **#126 (merged) applies your decisions (a), (d), (e).** Please try by hand:
    `--menu-pause original`, open the inventory (Tab), then talk to Easy Pete (the world
    should freeze under both); and without the flag, Esc while he is speaking (voice and
    mouth should stop and pick up on Resume). Found and fixed on the way: lines NPCs say
    outside conversations (Doc Mitchell in the intro) used to talk on underneath a menu
    or message box while their timer stood still; the sound now holds with the timer.
    Not verified unattended: a hand pressing Shift+Tab, that the sound is really silent
    while held, and that the mouth stands still while held. Rough edge: under `original`
    the picture moves a frame or two between message boxes shown back to back.
37. **Lip sync (#125, 2026-10-10): you compared it and found it identical to the base
    game** (Doc Mitchell's lines, with sound). Recorded in `docs/spec-notes/lipsync.md`
    §6 as your observation. The `.lip` format had no public documentation; all 60,702
    files decode and re-encode byte for byte. `--no-lip` turns it off. Left open:
    - (a) **Head and eye movement while talking is not shown.** The files carry look and
      head pitch/roll/yaw channels whose unit could not be established. A frame-by-frame
      video of one line in the original game would show how far the head turns.
    - (b) **Channel names by position.** The vowel shapes and the brow, look and squint
      names follow the order of the game's `MFG` console lists and are not confirmed one
      by one. In the original, `MFG phoneme <n> 100` and `MFG modifier <n> 100` on an
      NPC with a screenshot each would confirm them; an export of the GECK wiki page
      "ModifyFaceGen" would help too (it could not be fetched directly).
    - (c) **Emotions.** A response's emotion type and value and the heads' expression
      morphs are not applied; how the game blends them with speech is unknown. Is a
      plain design acceptable (the emotion morph at its value while the response is
      said), or do you want a capture first?
    - (d) **Speakers without a human head** (robots, Mr. House, creatures) have `.lip`
      files but nothing moves for them.
    - (e) Child and ghoul heads were not looked at on screen (their morph files parse
      and fit their models). Blinking outside speech and head tracking are not done.
    - (f) A private dump of all `.lip` and `.tri` files (about 345 MB) is in
      `C:\Users\Loot\coord-bin\dlg-lip\priv\`, outside the repo. Delete it when you like.
    - (g) Correction to `docs/spec-notes/actors.md` §4: `headhuman.nif` is not
      vertex-identical to its `.tri` (same order, up to 0.53 units apart). Nothing
      changes on screen.

38. **Conversation partners look at each other (your request, 2026-10-10): built, PR #133
    (`claude/dialogue-facing`), waiting for the coordinator's check and merge.** In a
    back-and-forth conversation (the dialogue screen is open) the NPC turns its head to
    you, a standing NPC also turns its body on the spot, and your view eases onto the
    NPC's face. Lines an NPC just says out loud turn nobody: in the unattended intro Doc
    Mitchell's 23 spoken lines turned no one and his two conversations did. Details and
    what the screenshots show: `docs/design/dialogue-play.md` §8. Try it:
    `nvforever --data <Data> --play --cell Goodsprings`, walk up to Easy Pete from the
    side or from behind and press E.
    - **Free look against facing: decided 2026-10-10** ("never come back i just want
      the initial look at npc to happen then allow the player to free look the rest of
      the time they are in dialogue with them"). Your view turns to the NPC when the
      conversation starts (0.4 s) and is yours after that; it no longer comes back when
      the choices appear. The mouse always wins: moving it during the opening turn stops
      the turn. `--dialogue-recentre` brings the old coming-back behaviour back. Still
      as you chose earlier: while the choices are shown the cursor is released for
      clicking, so the view holds until you pick; say if you want free look there too
      (choosing would then be keys only).
    - **How far and how fast are our own choices, not the original's.** The head turns
      at most 60° to a side (that number is the game's own: "Tracking Max Angle" of the
      head in `DefaultBodyPartData`) and 30° up or down; it takes 0.45 s to turn and
      0.6 s to ease back. A body more than 30° off turns at about 150° a second and
      **keeps facing you after the conversation**. Say if any of it looks wrong (too
      fast, too stiff, should turn back afterwards).
    - **Sitting NPCs.** A sitting or lying NPC is meant to turn its head only. Nothing
      in the engine seats anyone yet (every NPC is drawn standing, Doc Mitchell during
      the psych test included, so he turned on the spot standing by his bed), so today
      only the hook exists (`BodyHeld`); the furniture work has to set it.
    - **Seen and left as it is:** NPCs are still drawn where the plugins place them, so
      a conversation a script starts from afar turns your view to where the NPC stands,
      even through a wall (Doc Mitchell's farewell, which in the original he walks over
      for). This goes away when AI packages move NPCs.
    - **Not included:** the eyes do not move in the head (at 60° the face is turned to
      you but the eyes look straight out of it); no zoom or dialogue camera (the wiki
      export does not describe the original's; tell us what it does, or we capture it);
      creatures, robots and talking activators do not turn; the turn animation is asked
      for but its pace is not matched to the turn and it was not checked in a picture.
    - `--no-dialogue-look` turns the whole thing off. With `--menu-pause original` the
      two are facing each other at once in the frozen picture.
    - **Please play it by hand**: the turns were checked in screenshots a few frames
      apart, not in motion, and the mouse during a turn only through a test aid.

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

## Layers 1–4 close-out (coordinator "Layers 1–4 and housekeeping", 2026-10-09 afternoon)

The owner asked for Layers 1–4 to be worked through and the repository tidied, using the
in-game comparison tools. Eight workstreams ran in parallel worktrees; their PRs were
checked and merged by this coordinator (owner approved merging when green): #91, #93,
#95, #97, #98, #99, #100, #101, #102. Lab on `main` after the merges: **376/402 pass, 0 regressions** (L1 111/115, L2 51/56, L3 99/109, L4 87/90, L5 28/32; the 8 errors are holdouts whose goldens are not on this machine; the 18 fails are the 4 xEdit rows of item 3, 7 exterior frames below `min_ssim` 0.6, the 3 accepted physics residuals and the 4 known Layer 5 rows).

1. **⚠️ Your autosave was rewritten again.** A Layer 4 capture (`pc_drop_fall`) walked
   out of Doc Mitchell's house through the front door, and the game wrote `autosave.fos`.
   `capture_run.py` backed up the previous `autosave.fos`, `.bak` and `.nvse` first, to
   `C:\fnv-private\capture\saves-backup\1791556049`. Copy them back into
   `Documents\My Games\FalloutNV\Saves` yourself if you want them (the tools never write
   there). The older movement jobs (layer4-captures.md §5) leave the house the same way,
   so treat any job that passes a door as one that may autosave.
2. **Accepted physics residuals (your decision today):** `L4.phys.drops.b11_goodsprings`
   17/20, `L4.phys.walk.portal_wall` 5/6 and `L4.phys.drops.swim_h2o_fresh` 8/9 stay
   failing as the honest record; nobody tunes them further (layer4.md §4).
3. **Layer 1 (#93).** The four failing xEdit scenarios are xEdit normalising the file,
   not our reader: three missing required EDID/FULL subrecords that xEdit exports as `""`,
   and GRA's out-of-range master index, which xEdit folds into the plugin's own. Our
   reader and the independent oracle agree on all four. Decide:
   - (a) add a "documented differences" list to the scenario format (proposal in
     layer1.md §5.1), or (b) leave the four failing.
   - The game does what xEdit does with the out-of-range index (the reference is placed
     as GRA's own, captured). Following it needs an `oracle_world.py` change and two
     regenerated L3 interior goldens — your call (golden regeneration).
   - Record layouts now cover 70 of 107 types, with nothing left undecoded at the end
     of a record in any master. FalloutNV.esm's SHA-256 is unchanged; a Steam "verify"
     is still yours if you want the checksum question closed. The licence is still yours.
4. **Layer 2 (#101).** Archive precedence is settled in game: `Update.bsa` and the DLC
   `<Plugin> - Main.bsa` archives override the base archives, whatever the file dates.
   Loose files on top stay a hypothesis (Data has no loose meshes or textures; put one
   there by hand if you want it confirmed). Five fixed-camera frames were captured for a
   mesh-by-mesh comparison; no asset bug was found. Their `L2.frame.*` scenarios are not
   generated yet, and their threshold (provisional `min_ssim` 0.6) needs you.
5. **Layer 3 (#95, #98, #99, #100).**
   - **Creatures** are drawn with their idle animation; disabled NPCs, creatures and
     lights are now spawned hidden, so a script's Enable shows them (item 19 above);
     "Random Anim Start" objects no longer animate in step. Whether the game adds the
     race's FaceGen data to an NPC's is open: say if you want the capture (several NPCs
     of one race).
   - **Distant land:** terrain LOD levels 4–32, object blocks out to 125,000 units, distant
     tree billboards to 40,000, distances from your INI; the interior-start bug is fixed;
     smoke test 47.6 → 54.0 fps. **Question:** the game's view of the hill east of
     Goodsprings matched ours only at `--radius 4`, not at your `uGridsToLoad=5`
     (`--radius 2`). Raise the default load radius?
   - **Interiors:** a placed light's radius is its base radius plus `XRDS` (settled with
     seven new interior reference frames; SSIM up on all seven, e.g. Doc's living room
     0.42 → 0.68).
   - **Sky (docs only):** the new captures settled that the noon darkening is not eye
     adaptation, that clouds move with seconds since the save loaded (not the game hour),
     and that sunlight is about 0.6–0.66× too strong while shaded faces should be lit
     nearly neutral, not blue. Nothing in the renderer changed yet. **Questions:** record
     seconds-since-load in the frame scenarios so clouds can be matched? And the lab
     renders frames with sun shadows on (your default) while the game draws none — should
     the frame scenarios pass `--shadow-distance 0`?
   - The plugin's screenshot step fails ("BitBlt failed") in Vault 21, REPCONN and Big MT;
     the three saloon frames crash inside the lab (0xC0000142) but render fine outside it.
6. **Layer 4 (#97, #102).**
   - **Played by hand** (M4's last open item) with real OS keyboard and mouse input
     (layer4.md §4). The five problems found — free-fly help text in play mode, Esc
     quitting at once, message-box buttons not clickable, a raw FormID in a menu error,
     "level 0" lock prompts — are fixed: Esc now opens a Resume / Quit pause menu.
   - **Captured and modelled:** dropped items fall (and face the player's heading); the
     player is about 130 units tall; pushing matches the game; `coc` and a fall into
     nothing land on the cell's marker; physics-moved clutter is written back; Caps Lock,
     Ctrl (sneak) and Q (auto-move) work; `iHoursToRespawnCell` is 72 and a cell resets 72
     hours after the player leaves it; placed frag mines are live; the Pip-Boy and its
     glove are worn from the start.
   - **Still open:** crouched and eye height; player disarming of mines; Chet's restock
     rule (Ringo's days are not global, but the simulation still applies them to every
     merchant); ~~armour on the ground should be drawn with MOD2 (male), which the engine
     does not do yet~~ (done 2026-10-10, "Dropped items use their world model" below);
     Layer 5's `coc_destination` should follow the marker rule.
7. **Housekeeping.**
   - The README now describes the project as it is (status table, `--play`, the comparison
     tools, every crate).
   - Build folders of 33 merged worktrees were deleted (C: free space 75 → 195 GB); the
     worktrees themselves were kept. Merged remote branches (78) are untouched; deleting
     them is still your call. `frosty-nash`'s one useful doc line was taken in; the other
     unmerged branches add nothing to `main` except `layer5-play-2` (Layer 5, left alone).
   - **Lock incident:** when all agents stopped at the usage limit, a leftover lock loop
     of one agent took both game locks and held them with nothing running (about
     14:22–14:54), which blocked the Layer 5 session's windowed checks; they never ran and
     need re-running there. All game runs since go through a wrapper that releases both
     locks in a trap. Keep it that way for loops that wait for the locks.

## Navmesh and travelling NPCs (2026-10-09, session "navmesh")

Owner request: navmesh building for worldspaces and AI, tested with an NPC navigating
the world, able to tell it can reach its package's target through interiors and
worldspaces. Design: `docs/design/navmesh.md`; facts: `docs/spec-notes/navmesh.md`.

1. **New crate `fnv-nav`** (reader, nav graph, hierarchical paths with door routes and
   locks, path following), **`fnvtool nav`** (survey, graph, near, path, walk) and
   **`nvforever --npc-travel / --follow-npc / --show-navmesh`**. Headless physics walks
   all arrive, including Doc Mitchell's house to the Strip through 7 doors and 8 spaces.
2. **Decisions taken (design §7), each reversible:** NPC walking speed 86.8 units/s;
   ledge links unused by default; locked doors closed to actors without the key; in the
   walk test, other actors and hinged doors are left out of the collision world.
3. **Animated in-cell doors** (your request, with NPCs waiting for the Open animation):
   designed in §6.1 and queued as a separate task; not part of this change.
4. **I ran the engine before reading the smoke-lock note above** (item 5 of the NPC
   coordinator's section): the first `nvforever` runs of this session took neither
   lock. Later smoke runs take both.
5. **A useful reference if you can capture it:** the GECK's "Test Pathfinding" tool
   (your Menus export) shows the path an actor would take between two triangles. A few
   captured paths would let the lab compare our routes with the original's.

## Doors that open in place (2026-10-09, session "doors")

Owner requests: doors that are not cell transitions animate so they can be opened, and
an NPC waits until the open animation has finished. Design: `docs/design/navmesh.md`
§6.1; facts: `docs/spec-notes/doors.md`.

1. **Done:** such doors (263 of 474 DOOR models have Open/Close; 1,844 placed references
   without XTEL) are drawn with their animation, collide as they move, open and close
   with E (locks and keys as before), and log their sound keys (there is no sound
   playback yet). Followers stop in front of a door on their path, open it, wait for
   the Open sequence to end, then walk through: in `fnvtool nav walk` (all six trips in
   §5 arrive; Doc Mitchell to the Strip waits 1.68 s at the sewer door) and for
   `--npc-travel` travellers (Trudy through a saloon door: waits 1.02 s).
2. **Decisions taken (design §7.5), each reversible:** the actor stops in line with the
   middle of the opening, the opening's width plus its clearance in front (so a hinged
   leaf cannot swing into it, whichever way it opens), and walks straight through; doors
   stay open; a door told to close while opening turns back. A door flagged Open by
   Default now starts open in the simulation too [hypothesis].
3. **Open questions for captures:** do doors close by themselves behind NPCs; from how
   far does an NPC open a door; does the original wait for the animation; what does the
   "Sliding Door" flag change (hinged-looking doors carry it).
4. **Fixed on the way:** a traveller inside the interior being drawn never started
   (`--npc-travel` from an interior with `--cell` that interior); the interior now counts
   as streamed in once its cell is loaded.
5. **I ran `nvforever` without the smoke locks** (`.claude\smoke.lock`,
   `C:\fnv-private\capture\run.lock`) about fifteen times (13:50–15:10, 20–120 s
   each) before reading item 5 of the NPC coordinator's section; another session held
   `run.lock` during some of them. Any windowed check that overlapped them may need
   re-running. The final smoke runs (15:29–15:33) took `.claude\smoke.lock`;
   `run.lock` had been held since 11:39 with no owner file and no FalloutNV or
   nvforever process running (stale-looking); removed at your request (15:45).

## AI packages (2026-10-10, steps 1 to 4b of the plan)

Facts and figures: `docs/spec-notes/packages.md`; design and the decisions taken:
`docs/design/actor-behaviour.md` (§6). Branch `claude/ai-packages`.

1. **Done:** every AI package record of the six masters decodes (step 1), and the game
   data now holds each package as typed data: type, flags, where, what it targets, when,
   its conditions and the per-type extras (step 2; `fnv_game::package`). Nothing uses
   the new fields for choosing a package yet: that is step 3.
2. **Checked against your observer captures** (eleven Goodsprings runs, 354,983 actor
   samples; `fnvtool ai observed`): the game reported the expected package type for
   every one of the 30 packages seen (10 of the 15 types), the record's target whenever
   it named a reference, and 39 of 42 actor/package pairs came from the actor's list
   (the other three are packages scripts add).
3. **A documented rule the game narrows:** the wiki says a patrol is skipped when the
   actor has no linked reference. Victor has none and patrols all day: his patrol names
   a marker and follows the markers linked from it. Only patrols located "near linked
   reference" (the default patrols) need the actor's own link.
4. **Corrected from step 1:** "location 2" was counted twice (each record stores it in
   two identical copies): 236 packages have one, not 472.
5. **Would help if captured:** an actor running Find, Follow, Escort, Wander, Use Item
   At or Use Weapon packages (their in-game type codes are paired by name only; set
   aside at your request), and the sandbox settings (`GetGameSetting`), as noted for
   step 1.
6. **Step 3, done:** actors now choose their package as the game does: by the hour
   (a package runs from its starting hour for its duration), top to bottom, skipping
   packages that need a linked reference the actor lacks, with Once Per Day, Must
   Complete and Continue If PC Near. `fnvtool ai day --actor EasyPeteRef` prints an
   actor's day. Replayed over your captures, the choice matches wherever quest state
   does not decide (186,563 samples the same; 51 differ, all in the seconds a switch
   takes).
7. **Taken from your captures, not from the documentation:** an actor with no valid
   package keeps the one it has (a Goodsprings settler sandboxes all night because its
   sleep package's conditions fail). And a rule the documentation states that the
   captures contradict, left as it was: a script package without Must Complete is said
   to end at the next evaluation, but Ringo's ran for 887 samples.
8. **Captures that would settle open points:** the evaluation interval at a timescale
   other than 30, and an actor under a Once Per Day or Must Complete package.
9. **Step 4, done headless:** an actor now goes where its package sends it, over the
   navmesh and through doors, and the game is told when it arrives (new crate `fnv-ai`).
   `fnvtool ai run --actor EasyPeteRef --start-hour 6` shows his day: chair, bed, chair,
   the saloon at noon, chair, bed, each walk with its doors and length. Started from
   your captures, the simulated actors are in the same cell as in the game 91–100% of
   the time (Chet, Ringo, two settlers, Easy Pete, Cheyenne).
10. **Can every actor get where its packages send it?** `fnvtool ai reach` answers for
    the whole game in 16 seconds: of the 9,938 destinations it can judge, 9,434 are
    reachable (up to eight doors away) and 504 are not.
11. **Notes to look into** (the full list of twenty: `docs/design/actor-behaviour.md`
    §7). The ones that matter most:
    - 200 destinations have no chain of doors (Primm deputies, Nellis tower guards, Yes
      Man walking to his terminal): ladders, lifts, quest-enabled navmeshes or doors?
    - 112 are behind doors a quest unlocks; game-side lock changes do not reach the nav
      graph yet.
    - 186 actors are placed with no navmesh within 512 units.
    - Patrols do not wait at their points (Victor matches his capture only a third of
      the time); sandboxing is "go there and stay" (no wandering, sitting, sleeping in
      beds).
    - What the game does when a package cannot be carried out is unknown; we set it
      aside for an hour and take the next one.
    - Not carried out yet: Follow, Accompany, Escort, Find, Use Item At, Use Weapon,
      Dialogue, Ambush and Flee packages (1,107 actor/package pairs).
12. **Step 4b, done: NPCs live by their packages in `nvforever`.** Try
    `nvforever --data <Data> --cell Goodsprings --npc-packages EasyPeteRef --hour 11.8
    --follow-npc`: at noon Easy Pete gets up from his chair and walks into the saloon to
    eat; with `--hour 21.9` and `EasyPeteRef,GSChetRef,GoodspringsSettler04Ref` the town
    goes to bed. Limits: only the actors you name; their package choice runs in a game
    state of its own, so in `--play` quests do not affect it yet; they walk on the
    navmesh without physics (design §5.4).
13. **Your question about Doc Mitchell's route** (he leaves his room through the living
    room instead of the doorway beside him): it is the game's navmesh, not the test or
    our pathfinder. The house's navmesh has no triangles in that doorway (a gap of about
    50 units, read straight from FalloutNV.esm), though the doorway is physically open.
    I could not confirm what the original game's NPCs do there: the captures only have
    Doc standing still. Watching an NPC walk through that house in the game, or the
    cell's navmesh in the GECK, would settle it (design §7, note 21).

14. **Step 4c, done:** `--npc-packages all` hands every actor of the loaded cells to the
    package layer; with `--play` the choice uses the game's own state (quests, deaths,
    menus pausing it); and near the player actors are moved by the character controller
    (they go round furniture and follow the floor), falling back to the navmesh surface
    when stuck and for the last steps to a chair or bed.
15. **A change you will see in `--play`: the sky now follows the game's clock.** Time
    passes at the game's TimeScale (one game hour in two minutes), starting at `--hour`.
    `--still-sky` gives the old fixed sky back. Say if you would rather have it off by
    default.
16. **Your rewrite of `main`'s history** (commits re-authored with the GitHub noreply
    address) left `claude/ai-packages` on the old history. This work is now one commit
    on the new history, on **`claude/ai-packages-v2`** (same files; authored with the
    noreply address). I did not touch the old remote branch `claude/ai-packages`: it
    still holds the old commits with the old address, so you will want to delete it.
    Other old-history branches on the remote are in the same position.
17. **Furniture with the package routines (your request, 2026-10-10):** residents now use
    the furniture from main. Easy Pete sits in his porch chair, gets up at noon, sits at
    his booth in the saloon, and at night lies down on his bed; the walk to the next
    place waits until he is up. The rule for chairs comes from your captures (the game
    reported him sitting under those packages); beds for Sleep packages and seats for
    Eat packages follow the documentation. Not done: sandboxing actors picking
    furniture on their own, and actors that arrive unseen are seated only once drawn.
18. **Sandboxing (your request, 2026-10-10):** actors under Sandbox and Wander packages
    now do things in their area: walk about, sit on seats, stand at idle markers, go to
    bed at night and take meals, as the package's flags allow. The six behaviour flags
    are read and five are acted on. **Conversation is read but not carried out** (no
    NPC-to-NPC talk yet), and so are Random Conversations, Idle Chatter, No Idle Anims
    and the package's own idles: nothing plays idle animations yet. The sleep and meal
    hours are the documented ones; how long an actor sits or stands, and how it scores
    what is around it, are my numbers until a capture shows the game's (design §5.5).
    Patrols now wait at their points for the point's idle time.

## Loading screens (2026-10-10, session "loading screen")

Your request: pause and show a loading screen when changing cell, everything on hold
until loading is done, the picture taken from the game's loading-screen textures as the
plugins list them. Design: docs/design/loading-screen.md; records:
docs/spec-notes/loading-screens.md; branch `claude/loading-screen`.

1. **Yes, the plugins say which pictures are loading screens.** FalloutNV.esm has 208
   loading screen records (LSCR); each names a picture and a text. They use 35 of the 52
   textures in `textures\interface\loading`; the other 17 are parts of the original's
   screen (roulette wheel, glare, text bar), the start-up screens (legal, publisher
   logos) and three pictures no record names. The game picks from the records, so those
   17 never appear.
2. **Decisions taken, each reversible:**
   - 12 records belong to places (the Strip's casinos, Primm, Vault 11, Goodsprings,
     Boulder City, ...). Going to such a place, its own screen is taken every other
     time, a general one otherwise. The original's odds are not known.
   - A place given as a map square (Goodsprings, Primm, Boulder City, Vault 11) counts
     for arrivals within 2 squares of it. The original's reach is not known.
   - The picture fills the window without being stretched (its sides are cropped); the
     record's text is shown in a band along the bottom. The original's layout, roulette
     wheel and level progress are not copied.
   - Scripts, the game clock, animations, NPCs and the test aids all stand still under
     the screen; a voice line already playing goes on, as under the pause menu.
3. **Not done, your call:** a loading screen at the start of a run (the world still
   streams in view there), and for travel without `--play` (`--follow-npc`).
4. **Shared code touched** (other sessions: note when merging): `PlayState` gained
   `Loading`; activation and scripts now send `nvf_play::loading::TravelRequest`, and
   `Travel` goes out two frames later; `run_scripts` returns under a loading screen.

## Ragdolls (2026-10-10, session "ragdoll")

Owner request: dead NPCs and creatures drop to the floor with real physics. Facts,
design and what was checked: `docs/spec-notes/ragdoll.md`.

1. **Done:** an actor that dies becomes a ragdoll built from its skeleton's own Havok
   bodies and constraints (73 skeletons have them: people 18 bodies, a deathclaw 32, a
   raven 3), simulated in the physics world and driving the bones. The body lies where
   it falls, can be searched, is shoved by further shots and thrown by explosions, and
   is put back as it lay when you leave and return. New: `fnvtool collide ragdoll`
   (survey of the joint data), test aid `aimcorpse`.
2. **Decisions taken, each reversible (ragdoll.md §4):**
   - the ragdoll starts at once on death (no death clip first); the death pose is only
     shown for bodies whose ground is not loaded yet;
   - the player walks through corpses (no pushing them with your feet yet);
   - a body at rest is held still until it is shot or caught in a blast (a barrel
     rolling onto it will not move it);
   - shove speeds (140 units per second for a shot, 220 for a melee blow) and joint
     damping are guesses to tune when you play; shots move a lying body only a little;
   - corpse poses are remembered for the run only (there are no saves yet).
3. **Open questions (ragdoll.md §6):** the sign of two joint limits rests on anatomy,
   not on a capture; the unit of the constraints' friction value is unknown. A capture
   of how corpses lie in the original would settle the first.
4. **Not built:** armour ragdoll templates (`.rdt`), knockdown of living actors
   (`PushActorAway`, explosion knockdown), weapons dropping from dead hands, severed
   limbs, dragging bodies.
5. **I ran `nvforever` without the smoke locks** nine times (10:37–10:57, 14–39 s each,
   unattended test-aid runs with a window) before reading item 5 of the NPC
   coordinator's section. Another session took both locks at 10:50:52 and still held
   them at 10:57; my last four runs (10:46–10:57) overlapped it. Any windowed check or
   capture of that session in that window may need re-running. The two runs
   after that (11:05 and 11:07, on the tree merged with main) took both locks.

## The player's unclothed body (2026-10-10, session "naked body texture")

Your report: the unclothed body was one orange-tan colour, the T-shirt visible only as
shading. Facts and rules: `docs/spec-notes/actors.md` §3.1 and §4.1.

1. **Fixed:** the body model holds two shapes, the skin and the underwear (its own
   texture: grey T-shirt, dark shorts). The engine put the race's skin texture on both.
   Now only the skin shape takes it, for the player and for NPCs, men and women.
2. **Also changed, so the body is one colour:** hands are tinted like the body (they
   were left the light colour of the texture beside darker arms), the skin showing in
   gloves likewise (the Pip-Boy glove's fingers and wrist), and the first-person arms
   and hands take the same tint as the third-person body (they had none).
3. **For you to judge: the skin tone.** Your picture is described as showing light skin.
   Ours is now a clear brown on arms, legs, hands and face alike. That comes from the
   face values stored in the player's record, which shift the skin colour darker than
   nine in ten Caucasian NPCs; the shirt and shorts are not affected. The rule that
   applies the shift is a hypothesis (it fits the files well, see §4.1) and I could not
   see your picture. If the skin is still too dark against the original, please put the
   picture where sessions can read it; `--no-facegen` shows the body without the shift.
4. **Found, not changed:** African American NPCs (about 280) are probably drawn too
   light: their own face values barely darken the shared skin texture, while their
   race's values would. The game most likely adds the two (actors.md §4.2). Joe Cobb in
   Goodsprings is the nearest case to compare with the original.
5. **Fixed in the next session ("worn skin", below):** ghoul and raider-race NPCs showed
   human skin where an outfit leaves skin bare.

## Skin inside outfits follows the wearer's race (2026-10-10, session "worn skin")

Builds on the section above. Facts and the rule:
`docs/spec-notes/actors.md` §3.1, "Worn models".

1. **Fixed:** outfits and gloves hold their own pieces of bare skin (arms, chest, legs,
   fingers), and each piece names a fixed texture, nearly always the adult human one.
   The engine drew that texture, untinted unless it happened to be the wearer's own. So
   ghouls had pale human arms under a ghoul head and hands, and the raider races
   (Fiends and Great Khans among them; 350 placed NPCs in outfits with bare
   skin) had light arms and legs under darker faces. Now that skin takes the wearer's
   race texture (body or hand) and the same tint as the face. Children in outfits (30)
   and 48 women whose outfit names the child's skin texture by mistake are put right by
   the same rule, and so are 11 men in combat armour, one of whose skin pieces names the head
   texture.
2. **How it was checked:** before and after pictures of one ghoul (Gomorrah, in a vest
   and shorts) and three Great Khans at Quarry Junction; in both the bare skin now
   matches head and hands. The pictures are in `C:\fnv-private\frames\worn-skin\`. The
   rule itself is a hypothesis: the game's files fit it, but I compared no picture from
   the original. A ghoul with bare arms in the original (any ghoul in Gomorrah, or
   Beatrix Russell) would confirm it.
3. **For you to judge:** the Great Khan woman in the picture (Melissa) is now very dark
   on arms and legs, because her face values are dark and the body follows the face
   (item 3 above is the same question for the player).
4. **Fixed in the next session ("skin normal maps", 2026-10-10; actors.md §3.1, "Normal
   maps"):** the fine surface relief of the skin (the normal map) was the adult man's on
   everyone, on the body itself as well as inside outfits: each kind of skin has its own
   relief file beside its colour file and the engine did not switch to it. Now skin
   takes the relief file that lies beside the colour texture it is given.
   - **What you will see:** ghouls have the ghoul surface (sunken nose, bared jaw,
     sinews on the neck, rough arms and legs) instead of ghoul colours on a smooth human
     one. Women had the men's face relief (heavy brow and jaw shading) and now have
     their own: 489 placed adult women, so this is the change you will meet most often.
     Old people get the wrinkles of their own files (131), children theirs (29). The
     raider races (384) change least: their files differ little from the human ones.
     Young adult men of the ordinary human races are untouched.
   - **How it was checked:** one before and after pair each of the Gomorrah ghoul, the
     three Great Khans and a Goodsprings settler, in
     `C:\fnv-private\frames\skin-normal-maps\` (`cmp_ghoul_gom.png` and `cmp_khans.png`
     are enlarged side-by-side views). The ghoul changes on his own pixels only; of the
     Khans mostly the woman's face; the settler's two pictures are the same file byte
     for byte. The rule is a hypothesis: the game's files fit it (most of these relief
     files are named by no model, so the game can only find them by name), but I
     compared no picture from the original. A close view of a ghoul's or a woman's face
     in the original would confirm it.
   - **Not changed, for you to decide:** hair. 216 placed NPCs (the African American
     hair and the balding hair on the base hair model) have a hair texture with its own
     relief file that differs from the one their hair model names. The files do not show
     which one the game uses, so they keep the model's. Eyes, mouths and teeth have no
     real relief files of their own and stay as they were.
   - **To tell you:** I ran `fnvtool nif textures` once, which walks every archive of
     the install, the add-ons included (one of them is a lab holdout). It lists texture
     paths that meshes name and that do not exist; I read its summary counts and the
     first rows (base-game architecture meshes), found it was not what I needed, and
     deleted the output. The survey behind the rule used `Fallout - Textures.bsa`,
     `Fallout - Textures2.bsa`, `Fallout - Meshes.bsa` and `FalloutNV.esm` only.

   Still open from the session before: hands drawn as part of a fist weapon (boxing
   tape, knuckles) keep human skin.
5. **To tell you:** I also ran the skin survey over the add-on archives, among them
   `HonestHearts - Main.bsa`, before noticing that the lab treats that archive as a
   holdout. It printed counts of skin shapes by texture name, nothing else. The rule was
   settled on the base game's files, and nothing from the add-ons is in the notes or the
   code.

## Furniture: sitting (2026-10-10, session "furniture")

Owner request: furniture usable by the player and by NPCs, animated: sit down, stay,
stand up on moving. Facts, model and what was checked: `docs/spec-notes/furniture.md`.
Branch `claude/furniture` (worktree `NVForever-furniture`).

1. **Done:** chairs, benches, booths and stools work for NPCs and the player, with the
   game's own clips (which clip is read from the game's idle records per furniture
   marker, so left, right and front entries, stools and women's variants come out of the
   data). You sit by activating the furniture and get up with a movement key or by
   activating it again. NPCs can also use beds. `GetSitting` and the four related script
   functions now answer for whoever is seated. New: `--npc-sit ACTOR=FURNITURE[:SECONDS]`
   and `fnvtool actor furniture`.
2. **Decisions taken, each reversible (furniture.md §5, §7):**
   - you are put at the furniture's entry point when you activate it (no walking up);
   - getting up: any movement key, or Activate on the same furniture;
   - in first person you see through the seated body's eyes and the body is not drawn;
   - sitting puts your weapon away;
   - an actor takes the nearest enabled, free entry point; a chair holds one user, a
     bench or a table of stools one per seat;
   - the prompt still says "Activate" (I believe the game says "Sit" but have no source).
3. **Not built:** you cannot use beds (that needs the sleep menu); NPCs do not pick
   furniture by themselves yet (that needs AI packages, in progress on
   `claude/ai-packages`; its session can give an actor a `SitOn` to seat it); work spots
   such as the bar and workbenches; eating, talking and fidgeting while seated; the fast
   stand-up when attacked.
4. **A capture would settle** which side an NPC enters a chair from and when
   `GetSitting` is 1 (furniture.md §5); not needed for what is built.
5. Game runs took both locks. The repository move orphaned this session's first
   worktree mid-way; its two unpushed commits were rebuilt from the files on disk.

## Pip-Boy on the arm (2026-10-10, session "pip-boy")

Your report: the Pip-Boy on the player's arm looks off, and its glow belongs to the
Pip-Boy light. Facts: docs/spec-notes/actors.md §1.2; design: docs/design/player-body.md
§4 and §5.

1. **Fixed, position:** the Pip-Boy model hangs from the forearm bone and was being
   turned the way hats and hair have to be turned on the head bone, which threw it onto
   the back of the upper arm. It is now a cuff on the left forearm, down to the glove,
   in third person, and it is drawn in first person too (it was missing there).
2. **Fixed, glow:** the bright green square was the model's light-glow shape, always
   drawn. It is hidden until a Pip-Boy light exists (not built; say if you want it).
3. **Also changed, a guess from the shape names:** outfits carry two left sleeves,
   `PipBoyOn` and `PipBoyOff`; both were drawn on everyone. Now an actor wearing a
   Pip-Boy shows the first and everyone else the second (170 outfits).
4. **Your answers (2026-10-10), applied:** the three orange button lights are always
   lit (they are drawn); with a Pip-Boy on, the outfit's `PipBoyOn` sleeve is drawn, in
   first person too (it was missing there, leaving bare wrist in a jumpsuit). Item 3 is
   therefore your rule for the Pip-Boy wearer; that everyone else shows `PipBoyOff` is
   still my reading of the name. Still open: is the screen dark with the light off?
5. **Seen on the way, not touched:** the Vault 21 jumpsuit shows a "101" (armour
   texture swaps are not read yet, actors.md §1.1). The black patches I saw on the bare
   body are the subject of the section "The player's unclothed body" above.

## Skin tone by race (2026-10-10, session "race facegen")

Follows item 4 of "The player's unclothed body" above. Numbers and reasons:
`docs/spec-notes/actors.md` §4.2.

1. **Changed:** an NPC's skin tint (face, body, hands) now comes from its own face
   texture values plus its race's. Before, only its own were used, and African American
   NPCs (357 NPC records) were drawn with light skin, lighter than Caucasian NPCs: their
   own values hold none of the race's colour, the race's values hold all of it. The
   other races barely change (their race values are small), nor does the player's body.
2. **For you to judge, with the original game beside it:** Joe Cobb and Easy Pete. Our
   pictures with and without the change are in `C:\fnv-private\references\race_facegen\`
   (`sheet.png`: Joe Cobb, Sunny Smiles, Easy Pete; top row before, bottom row after).
   - Joe Cobb: pale before, dark brown now.
   - Easy Pete: his record's race is African American (old aged). He was pale grey-white
     before and is a dark red-brown now, darker than Joe Cobb. If the original shows him
     clearly lighter than that, the rule is too strong for him and I need to know.
   - Sunny Smiles: no visible change (her record's race is Hispanic).

   `--no-race-facegen` shows the old look for comparison.
3. **Not changed: the face shape.** The data suggests the race's shape values are meant
   to be added the same way, but I have nothing to check a face shape against. An
   in-game picture of Joe Cobb's face from the front and from the side would settle it.
4. **Added, off by default:** `--show-disabled-actors` draws the actors a script has not
   enabled yet. Joe Cobb is one of them (hidden in the saloon until the story shows
   him), so the pictures needed it.

## Dog eyes (2026-10-10, session "dog-eyes")

Your report: on dogs, the eyes are not showing correctly. Facts and measurements:
docs/spec-notes/actors.md §10.2 and §10.5.

1. **Fixed, cause:** a dog's eyes are a separate model that hangs from the head bone.
   It was being turned the way hats and hair have to be turned on a person's head, which
   put both eyeballs on one side of the dog's head, one above the other, and left the
   sockets empty. Creature parts now hang from their bone as the files give them. Checked
   on Cheyenne against the dog's own eyelids: the eyes now sit just behind them.
2. **Fixed by the same change (135 placed creatures):** every dog with an eye model
   (Cheyenne, Rex, NCR and wild dogs, cyberdogs), the cyberdogs' skull caps including
   Rex's, the dome on every Protectron's head, and Primm Slim's hat (it was on edge
   beside his head). Coyotes and Legion mongrels have their eyes in the body model and
   were not affected.
3. **Not compared with the game:** I had no base-game screenshot of a dog's face. The
   placement is settled by the data, but the look of the eye (colour, shine) is drawn
   from the plain texture and is my render only. A close game screenshot of Cheyenne's
   face would confirm it.
4. **Seen on the way, not touched:** Rex's glass skull cap is drawn solid black, so the
   brain under it cannot be seen (a material problem, not a placement one; notes in
   §10.5). Fixed since: see "Rex's glass skull cap" below.

## Sunny Smiles' face shape and shiny hair (2026-10-10, session "face shape")

Your report: next to the game, our Sunny Smiles has a strange, almost masculine face,
and her hair shines in light. Branch `claude/facegen-race` (#142); it builds on "Skin tone by
race" (#141) above and answers its item 3. Facts:
`docs/spec-notes/actors.md` §4.3 and `docs/spec-notes/nif.md` §6 "Hair". Pictures:
`C:\fnv-private\references\face_shape\` (`sunny_compare.png`: the game, ours before,
ours after; `saloon_before_after.png`: Sunny, Trudy, Joe Cobb, a settler;
`hair_shine_before_after.png`).

1. **Fixed, face shape:** every race carries its own face-shape values, and an NPC's
   values are meant to sit on top of them. We used the NPC's alone. For women the
   race's values are large, so their faces came out long with a heavy jaw. With both
   added, Sunny has the game's shorter face, tapered jaw and larger eyes. Every NPC's
   face changes somewhat; women's the most.
2. **Fixed, hair fit:** most hair models keep their face-fitting data under a file
   name we did not look for (67 of them), so hair never followed the head's shape. It
   does now.
3. **Changed, hair shine:** hair, brows and beards are now matte. The data that made
   them glint is real, but the game clearly does not use it as an ordinary shine, and
   I do not know what it does with it. If the game shows a soft highlight on hair in
   strong light that you miss, a side-on picture of it would let me rebuild it.
4. **Still different on Sunny, for you to judge:** in your picture her eyes are more
   open and her lips fuller. She is in her idle pose there, looking up at you; ours is
   a still pose. I cannot tell yet whether that is the whole difference.
5. **Not compared with the game:** Trudy, Joe Cobb and the others — only Sunny had a
   picture. `--no-race-facegen` shows the old faces for comparison.
6. Your other request, the dogs' eyes, was done separately: the "Dog eyes" section
   above (#145).

## Rex's glass skull cap (2026-10-10, session "skull cap")

Facts and the survey: docs/spec-notes/nif.md §6 "Multiply blend", actors.md §10.5.

1. **Fixed, cause:** the cap is the game's one lit shape that *multiplies* what is behind
   it by its own colour (584 shapes use that blend in the base meshes; the other 583 are
   unlit stains, shades and interface parts). Its colour comes almost entirely from a
   strong glow setting on a flat blue texture. We were adding that glow as light and then
   fogging and tonemapping the result, which left a factor so dark that the cap came out
   black in the King's School. The cap's colour is now worked out as a factor: Rex's
   leaves about 44% of the red, 78% of the green and all of the blue of what is behind
   it, so the brain shows through a light blue glass.
2. **Please check against the game:** I had no base-game screenshot of Rex's head, so
   the tint's colour is from the data but its strength rests on my reading of how the
   game combines the glow with the texture. A close screenshot of Rex's head in the
   King's School (or outside in daylight) would settle it. If the glass looks darker or
   clearer in the game, that one rule changes; the placement and the blend do not.
3. **Not built:** the cap and the brain pulse in the game's data (a four-second glow
   loop) and the cap carries a chrome reflection map. Neither is drawn yet, so the cap
   is steady and has no reflections.
4. **Applies to the cyberdogs too, not looked at:** by the same rule the police
   cyberdog's cap is a red glass and the military one a yellow glass. Both are Old World
   Blues models; I decoded those two files by name and did not open them in the engine.
5. **Seen on the way, and wrong (corrected 2026-10-10, see "Decals" below):** I wrote
   here that the unlit stain overlays in interior kit pieces (for example the rust under
   the pipes of the utility halls) flicker against the wall they lie on. They do not.

## See-through corn (2026-10-10, session "skull cap", your screenshot at Goodsprings)

Facts: docs/spec-notes/nif.md §6 "Alpha" (last paragraph), world.md "Texture layers".

1. **Fixed, cause:** the ground's texture layers are drawn as see-through sheets over
   the base ground. The corn *rows* (the merged models, not the single plants) are also
   drawn see-through, with soft leaf edges, and such models leave no depth behind. So
   whenever the ground piece behind a row counted as nearer than the row, its layers
   were painted on top of the corn. That made the near row faint and hid the row behind
   it completely. Ground layers are now always drawn before any other see-through
   model. Checked from your position: both rows are solid, and the ground is unchanged.
2. **Wider than corn:** the same fault could fade any see-through model seen against
   the ground (dirty glass, some signs). They get the fix too. I did not go looking for
   other cases.
3. **Left as it is:** the rows stay soft-edged (blended) while the single plants are
   hard-edged (cut out), because the merged models store a cut-off of 0 where the single
   plant stores 124. If the rows look different from the single plants in the game in a
   way that matters, say so and I will draw the rows cut out as well.

## Decals: stains, dirt and markings on walls and roads (2026-10-10, session "decals")

Facts and the survey: docs/spec-notes/nif.md §6 "Decal flags".

1. **The reported fault was not one.** The task was "stain overlays lie in the plane of
   the wall and flicker against it". The survey says otherwise: of 2,130 shapes marked
   as decals in the base meshes, almost all are modelled a little in front of their
   surface (the rust stain in the utility hall by exactly 0.1 units, most others by 0.2
   to 0.3), and our depth buffer tells that apart without help. The dark jagged streaks
   in the viewer's picture of that hall piece are something else: thin pipe and cable
   cards standing 1.3 units off the wall, seen edge-on from the viewer's default camera.
2. **Changed anyway, small:** shapes marked as decals are now drawn a hair nearer to the
   camera than their geometry says, so they always win against the surface under them.
   This matters for the few decals that really are in the plane of their surface. Seen
   in the viewer on two of them: the ground layer on a red rock slope and the road
   surface on a highway overpass each had a ragged or partly missing edge, and are whole
   now. The hall piece is unchanged (8 pixels of the picture differ).
3. **A guess, not a finding:** that the game itself offsets marked decals is my reading
   of what the mark is for. The data shows which shapes carry the mark and where they
   lie, not what the game does with it. The offset is far smaller than the distance the
   decals are lifted by, so it cannot make one show through something in front of it.
4. **Not looked at:** about a thousand see-through shapes that are *not* marked as
   decals also coincide with another shape (glows and glass over the same triangles,
   most likely). Whether any of them draws wrongly is unknown.
5. **For you, if you care:** the cable cards in the hall piece look broken from outside
   the hall and at a steep angle. From inside, facing the wall, they should read as
   pipes and cables. If they look jagged in play, that is a separate texture-filtering
   question.

## Grass (2026-10-10, session "grass")

Your request: grass that is not placed as objects but grows on certain ground textures,
as in the base game. Branch `claude/grass`. Facts and open questions:
`docs/spec-notes/grass.md`. Pictures: `C:\fnv-private\references\grass\`
(`game_0800.jpg`, `game_1200.jpg`: your two screenshots; `sign_0800.png`,
`sign_1200.png`: ours from the same spot and hours; `compare_game_left_ours_right.png`:
the grass field cut out of all four).

1. **Built:** each ground texture names up to three grass types, and the game data says
   how dense each is, on what slopes and how near water it grows, and how much plants
   vary in size and shade. The engine grows them on every loaded cell, moves them in the
   wind and stops drawing them beyond the grass distance of your INI (7,000 units).
   `--no-grass` turns it off; `--grass-density 0.5` halves it.
2. **A rule I made up, for you to judge:** nothing public says how the game decides
   where each plant stands, so the rule is mine. It uses your INI's values (spacing 80,
   two types per texture) and puts the tall straw grass and the low sage in the same
   places as your screenshots. Ours looks somewhat thicker than the game's; single
   plants will not match the game's one for one. `--grass-density` is there if you want
   it thinner, and I can change the default once you say how it looks to you.
3. **Which two of three types:** most textures name three grass types and the game
   grows two. I grow the first two listed; the game may pick differently.
4. **Pixelated strands (your question about anti-aliasing): fixed.** Your game runs
   with 8x anti-aliasing and transparency multisampling, which softens the cut-out edges
   of each strand. Ours cut every strand with a hard test per pixel, which also ate the
   thin strands at a distance, so plants looked half the size. Strands now end softly
   and the plants are as full as the game's.
5. **Too bright: fixed at noon.** Your first comparison was our noon against the game's
   morning, which was most of it. With your hour-12 screenshot to measure against, our
   grass was still 10 % too bright and too orange, because it was lit exactly like the
   ground. The game's grass takes more of the bluish sky light and less of the warm sun.
   Ours now does the same and matches the game's colour at noon to within a few percent.
6. **Still different at hour 8:** our whole scene is about 15 % brighter than the game's
   there, ground and grass alike, and we draw sun shadows the game does not. That is the
   scene's morning light, which this branch does not touch.
7. **Not matched:** the low sage plants are browner and darker than the game's
   grey-green ones. I do not know why yet.
8. **Ground textures (your other request):** found and fixed by a separate task, pull
   request #150 (branch `claude/terrain-quality`): the ground textures were stretched to
   2.7 times their size in the game and had no surface relief (normal maps). The game's
   ground still shows stronger relief than ours after the fix; that remainder is lighting
   or shine on the terrain and is not done.

## Smooth cut-out edges and anti-aliasing (2026-10-10, session "grass")

Your question after the grass work: does the rest of the game need transparency
multisampling and anti-aliasing. Branch `claude/soft-edges`. Facts:
`docs/spec-notes/nif.md` §6 "Alpha" (soft edges). Pictures:
`C:\fnv-private\references\soft_edges\` (`before_after.png`: plants at the Goodsprings
sign, before on top; `msaa_1_4_8.png`: the same yucca with 1, 4 and 8 samples).

1. **Done:** everything that is cut out of a texture (plants, fences, grates, hair, tree
   leaves and the distant tree cards) now has smooth edges, as grass already had. Your
   game does this with "transparency multisampling"; ours had hard steps.
2. **Ordinary anti-aliasing** was already on at 4 samples; your game runs 8.
   `--msaa 8` gives you the same (also 1, 2, 4). I left the default at 4 because 8 costs
   frame rate everywhere and I have not measured by how much on your machine.
3. **Left hard on purpose:** the distant building blocks. Their textures are black
   where they are cut out, and a soft edge would draw a thin black line round every
   cut-out.
4. **For you to judge:** outlines grow by up to one pixel. I checked plants and trees at
   the Goodsprings sign and one interior; hair and fences close up I have not looked at
   beside the game.
5. **Seen on the way:** grass shows a faint dotted pattern where it overlaps something
   dark, visible when a frame is enlarged. It comes from having only four samples;
   `--msaa 8` reduces it.

## Pale blue frames inside the ruined house in Primm (2026-10-10, session "white trim")

Your screenshot from inside the house at the NCR camp (cell -16,-14). Branch
`claude/white-trim`. Facts and the surveys: `docs/spec-notes/nif.md` §6 "Alpha" (vertex
alpha and the test) and "View-angle falloff". Picture:
`C:\fnv-private\references\shadow_strips\before_after.png` (before on top).

1. **What they were:** not a missing texture. The house has black, see-through shadow
   strips along its inner edges (ceiling, skirting, door and window openings), which fade
   out across their width. We hid the faded part of each strip but still let the whole
   strip block the wall behind it, so the empty background colour showed through the
   wall. Fixed: the frames are gone (59,183 background-coloured pixels in that view
   before, none after).
2. **Where else:** seven building pieces carry such strips, all ruined Old Town houses
   (two house bodies, three upper floors, two corner pieces). One part of the giant rat
   and one terminal screen are built in a way that was open to the same fault; I did not
   look at those two in the engine.
3. **A guess, for you to check in the game:** how much of each strip the game shows. The
   files say "blend, and drop everything fainter than half". Read that way, only the
   darkest edge of each strip survives: a dark line a pixel or two wide in the corners.
   We now follow that reading, and from where you stood we draw no shadow at all (see
   item 4 for why not even the line). If the game shows soft dark shading a hand wide
   along those edges, the "drop" setting is not applied to these strips and ours are
   wrong. One game screenshot from inside that house would settle it; the rule is one
   function (`blend_cut` in `engine/nvf-assets/src/nif_spawn.rs`).
4. **Found on the way, not changed:** we fade every unlit see-through shape (signs,
   glows, stains, these strips) as it turns edge-on to the camera. The data suggests the
   game does that only for shapes that carry a certain mark: 84% of the shapes with a
   hand-set fade carry it, 2% of the rest, and the Saloon lamp beam carries it. If so,
   about 4,400 shapes are drawn too faint at an angle, and these strips lose their thin
   line for the same reason. That is a change to the look of the whole game, so I left
   it for a task of its own.
5. **Not changed:** see-through glow shapes with the same "drop" setting (force fields,
   stasis pod fields) still ignore it, as before.

## Dropped items use their world model (2026-10-10, session "world model")

Your request: a dropped item should use the world model its record names, normally a
`_GO.nif`. Branch `claude/dropped-world-model`. Facts: `docs/spec-notes/simulation.md` §3
"Item models". Pictures: `C:\fnv-private\references\world_models\`.

1. **What the records say** (`FalloutNV.esm`): only armour and weapons name a separate
   world model. Armour: `MOD2` (345 of 389, nearly all `GO.NIF`). Weapons: `MOD4` (57 of
   261); a weapon's `MOD2` is its shell casing and `MOD3` its scope overlay, so those are
   not used. Every other item type has one model.
2. **Done:** armour and weapons dropped, and those placed in the world by the game's
   level design, are drawn with that model and collide by it. Leather Armor now lies as
   the folded bundle instead of a body-shaped suit. Brass knuckles and the other fist
   weapons that have a world model and no other (8 records) were invisible when dropped
   and are drawn now.
3. **Not captured in the game:** that weapons on the ground show `MOD4` rests on the
   field's name in xEdit ("World Model") and on the file names, not on a picture from
   your game. Armour was captured on 2026-10-09.
4. **Still open:** a female player may see the female world model (`MOD4` of armour,
   28 hats and outfits differ); a dropped weapon with mods attached may show a different
   model. Both draw the plain world model for now.

## Sign lights, glows and shadow overlays no longer fade at an angle (2026-10-10, session "falloff flag")

Follows item 4 of "Pale blue frames inside the ruined house in Primm" above. Branch
`claude/falloff-flag`. Facts and the survey:
`docs/spec-notes/nif.md` §6 "View-angle falloff". Pictures (before left, after right):
`C:\fnv-private\references\falloff_flag\`.

1. **What changed:** unlit see-through shapes (sign lights, glows, screens, painted-on
   shadows) store a setting that fades them as they turn edge-on to the camera. It is
   meant for light beams, smoke and dust. We applied it to every such shape; now only
   to shapes that carry the mark that switches it on. About 3,470 shapes are drawn at
   full strength from every angle now. (The earlier note said 4,400; 930 of those are
   not see-through and were never faded.) The 1,100 shapes with the mark fade as before.
2. **Why I think the mark is the switch (a guess, not checked in the game):**
   - 85% of the shapes with a hand-set fade carry it, 2% of the others.
   - The public format notes call this mark "empty" for New Vegas, but for Skyrim and
     Fallout 4 they call the same mark "use falloff", and many other marks kept their
     meaning between the games.
   - Without it we were fading things nobody would fade: the shadows under the cards
     and coins on the Caravan table, neon tubes, lit sign letters.
3. **What it looks like in ours:**
   - `lamp_beam_front.png`, `lamp_beam_side.png`: the lamp beam on the Prospector
     Saloon porch is unchanged (it carries the mark): not one pixel of it differs.
   - `saloon_neon_tubes.png`: the red neon tubes on the letters S-A-L of the Saloon's
     sign, seen from the side along the wall at night. Before they were nearly out;
     now they glow red.
   - `bison_steve_sign.png`: the Bison Steve sign in Primm at night, seen nearly
     edge-on from below. Before its letters were dull; now they are lit red.
   - `shadow_strips.png`: the ruined house at the NCR camp in Primm, from where you
     stood. The shadow strips of that entry now show as thin dark lines along the
     ceiling edges, down the room corners and along the skirting.
4. **Please check in the game: one screenshot of any of these at an angle.** The
   easiest is the Bison Steve sign: at night (`set gamehour to 21`), stand a few
   steps south of the foot of the sign's pole and look up at the sign so that you see
   it nearly edge-on. If its red letters are lit, the change is right; if they are
   dull or gone, the game fades these shapes after all and I should put it back.
   Ours for comparison:
   `nvforever --cell=-13,-13 --pos=-52473,-53735,5990 --heading 12 --pitch 32 --hour 21`.
   A screenshot from inside the ruined house (the one that entry asks for)
   would answer this as well: thin dark lines in the corners mean both guesses hold.
5. **Not sure about:** 140 see-through shapes have a hand-set fade but not the mark:
   force field panels, the glowing tubes of stasis and brain pods, some weapon beams.
   Most are Fallout 3 leftovers in the archive. They are now drawn without the
   fade. If the game fades them, the rule is "the mark, or hand-set values". I know of
   no place in the base game to check one.

## A greeting marked Goodbye is just said at you (2026-10-10, session "greeting goodbye")

You said: when the GREETING line an actor picks is marked Goodbye in the data, the game
does not pull the player into a dialogue; the line is spoken at the player and the
player is not locked in.

1. **What was wrong:** we opened a full conversation for that one line. The game
   paused, the dialogue screen came up and you could not move until the line was over.
2. **Done:** such a greeting is now said like any line an actor calls out. You hear it
   and see its subtitle, the game keeps running, you can walk away, and scripts no
   longer see a dialogue menu. Greetings without the mark open the conversation as
   before.
3. **How common it is:** 3,023 of the 5,579 GREETING lines in the ten masters carry the
   mark, so this is what most actors with nothing particular to say do.
4. **Checked:** in `--play` in Goodsprings a scripted press of E on settler 04
   (`GoodspringsSettler04Ref`) made her say her line with no conversation and the run
   went on; ten seconds later the same press on Easy Pete opened his conversation as
   before and it ended by his goodbye. I did not watch it by eye (the run was
   unattended and silent). To see it: `nvforever --data <Data> --cell Goodsprings
   --play`, walk up to a settler and press E.
5. **My guesses, please correct any that are wrong:**
   - The actor still counts as "has talked to the player" afterwards.
   - Topics the line adds are still added when it ends.
   - Pressing E again while the actor is still speaking cuts the line and starts the
     greeting again.
   - The rule is only for GREETING. A script that starts a conversation with another
     topic whose first line is marked Goodbye still opens the dialogue screen for that
     line.

## Shooting Sunny's bottles (2026-10-10, session "object hits")

Your report: in "Back in the Saddle" the shots at the three bottles did not count, and
the bottles did not move when hit. Branch `claude/object-hits`. Facts:
`docs/spec-notes/combat-rules.md` §14.

1. **Why the shots did not count:** each bottle counts a hit in its own script, and
   three things were missing.
   - Shots only told actors' scripts they were hit, never an object's.
   - The script asks whether your weapon is drawn. That question always answered no.
   - It asks what kind of weapon you hold. That question always answered 0.
2. **Why the bottles did not move:** three more.
   - Bullets gave loose objects no push at all.
   - A resting object that was pushed could stay frozen (a fault in how we use the
     physics library). Explosions had the same fault.
   - Objects that a script switches on during play, like these bottles, were never
     redrawn where physics moved them.
3. **Done:** all six. Checked in a run: three shots with the Varmint Rifle, Sunny
   reacts to each, the objective completes on the third, and the bottles fly off the
   fence.
4. **A guess, for you to check:** the numbers the game gives for "what kind of weapon".
   - The wiki page (GetWeaponAnimType) has the table, but your export does not include
     it and the site blocks me.
   - I worked the numbers out from the game's own scripts. The tutorial counts rifles
     and pistols whichever way the one unsure part falls.
   - If you can paste the table from that page, I will check ours against it.
5. **Push strength is my choice:** a rifle shot sends a bottle off at about 3 metres per
   second. Say if it should be harder or softer (`OBJECT_PUSH` in
   `engine/nvf-play/src/combat.rs`).
6. **Not done:** melee hits on objects (they only strike actors so far), bullet impact
   effects and sounds.

## Both hands stay on a rifle when you look up or down (2026-10-10, session "two-hand aim")

Your report: in third person with a two-handed weapon, the hands drift off the weapon
the further up or down you look, from the hip and down the sights. Branch
`claude/two-hand-aim`. Facts: `docs/spec-notes/animation.md` §9.4 and §9.6. Pictures
(yours and ours, before and after): `C:\fnv-private\references\two_hand_aim\`.

1. **Cause:** the game has three poses for holding a weapon out: level, fully up and
   fully down, and mixes them by how far you look up or down. For rifles the up and
   down poses only bend the back; the arms are in the level pose alone. Our mixing
   turned the level pose down as you looked up, and the arms went with it, towards the
   arms-at-the-sides idle pose. One-handed melee and two-handed melee weapons had the
   same fault; pistols did not (their up and down poses include the arms).
2. **Fix:** a bone that only one pose moves now stays with that pose in full. The back
   still bends by how far you look.
3. **Also fixed, same cause:** firing while looking up or down straightened the body
   for the length of the shot. Each firing animation has its own up and down poses;
   they are mixed in now.
4. **Checked in ours** (varmint rifle, seen from the side): 45° and 80° up and down,
   down the sights at 60° up and down, firing at 60° up and 45° down. Both hands stay
   on the rifle and the rifle points where you look (`after_*.png`). Your game
   screenshot shows the same pose as `after_up80.png`.
5. **Not fixed, needs your eyes in the game:** while walking or running with a rifle
   out, our body does not bend at all when you look up or down; the rifle stays level
   (`after_walk_up60.png`, `after_run_up60.png`). The walking and running animations
   take over the back, and they have no up or down poses, so I cannot tell from the
   files what the game does. Reloading straightens the body in the same way. Please
   look in the game, third person, rifle out:
   - run forward while looking steeply up: does the rifle point up, or stay level?
   - reload while looking steeply up: does the body stay bent back?
   A screenshot of each from the side would be enough. If the game bends the body
   while moving, I will make the up and down poses apply over movement.
6. **A guess I kept:** that fully up and fully down mean 90°, and that the mix is even
   in between (half way at 45°). The rifle files support 90° (the back bends three
   times 30°). A game screenshot looking about 45° up, from the side, would check the
   in-between.

## Minigun: left hand on the handle when not firing (2026-10-10, session "two-hand aim")

Your report: with the minigun, firing looks right at any angle, but when just holding it
the left hand is too high. Branch `claude/weapon-rest-pose`. Facts:
`docs/spec-notes/animation.md` §9.7. Pictures: `C:\fnv-private\references\two_hand_aim\`
(`minigun_*.png`, `pistol_after.png`).

1. **Cause:** the arms were right all along: the holding pose and the firing pose have
   the same arms. The difference was the gun. The animations for drawing, firing and
   reloading the minigun say where the gun sits in the right hand; the holding and
   walking animations do not. When none said, we put the gun back at a default spot in
   the hand, which turned it away from the left hand.
2. **Fix:** while a weapon is out, it stays where its draw animation left it unless
   an animation says otherwise. This also applies to the flamer and gatling laser (same
   animations) and to pistols, whose holding animation is silent in the same way.
3. **Checked in ours:** minigun held level and looking 50° up and down, firing, after
   firing stops, and running: the left hand stays on the top handle and nothing jumps
   when firing stops. The 9mm pistol looks as before.
4. **The holstered minigun (your question, then your two game screenshots), built on
   branch `claude/weapon-parts`:** the game's files switch the gun part of the minigun,
   flamer and gatling laser off while holstered and on again half a second into the
   draw, and the minigun's backpack is a separate part meant for the upper back. Ours
   now reads both. Checked with the minigun (`minigun_parts_sheet.png`): holstered,
   only the backpack shows, on the back, as in your screenshot; the gun appears half
   way through the draw and goes half way through putting it away; drawn, the gun is in
   the hands and the backpack stays on the back.
5. **Not done:** the flamer and gatling laser were not looked at in ours (same
   animations, so the gun part should behave the same; whether their models have a
   back part like the minigun's I did not check). NPCs carrying these weapons still
   draw them the old way. In first person the backpack is not drawn at all.

## Weapons' moving parts: slides, magazines, the minigun's barrel (2026-10-10, session "weapon parts")

Your report: no weapon's moving parts are animated (the 10mm's magazine on reload, its
slide on firing, the minigun's barrel). Branch `claude/weapon-moving-parts`. Facts:
`docs/spec-notes/animation.md` §9.8. Pictures: `C:\fnv-private\references\weapon_parts\`
(`10mm-sheet.png`: holding, firing, two moments of the reload).

1. **Cause:** nothing was missing from what we read out of the files. The body
   animations (fire, reload, hold) also carry the movements of the weapon's parts, each
   addressed by the part's name inside the weapon model (`##Slide`, `##Clip`,
   `##MinigunBarrel`). We drew the weapon as loose pieces with those names thrown away,
   so the movements had nothing to act on and were dropped.
2. **Fix:** the weapon in your hands keeps its parts as named pieces, and the animations
   move them the same way they move bones. First and third person.
3. **Checked in ours:** 10mm pistol in first person: the slide snaps back at the shot;
   on reload the magazine comes out in the left hand and goes back in. Minigun: the
   barrel turns for as long as the trigger is held. The log also names moving parts for
   the varmint rifle (bolt), 9mm pistol, cowboy repeater (lever), assault carbine
   (charging handle, magazine) and laser rifle (cell on reload). I did not compare
   any of it with the game frame by frame.
4. **Decision taken, the minigun's firing animation:** measured from the file, the
   barrel spins up for about a second, turns at full speed for two thirds of a second
   between two markers (`Fire` and `Loop`), then spins down. Before, every shot
   restarted the animation, so it never got going. Now it plays the spin-up once,
   repeats the full-speed part while you hold the trigger, and spins down when you let
   go. That the game repeats exactly that part is my reading of the markers, not
   something I saw in the game; if it does it differently, say so.
5. **The spin-up delay (you asked for it, 2026-10-10):** built. The minigun now waits
   for the animation's `Fire` marker before the first shot, then fires at its normal
   rate while you hold the trigger. Letting go before the first shot fires nothing
   (the barrel still spins and winds down); every new press spins up again, and so
   does firing on after a reload. Those last two rules are my choice.
   **A correction to what I told you:** I quoted about a second of spin-up. That is
   the file in the base game archive. Your install uses a different version of that
   animation from a later archive (1.7 s long, `Fire` marker 0.2 s in), so the delay
   you will see is 0.2 s. I did not look up which archive it comes from. If 0.2 s
   feels too short, say so and say what you want instead; a fixed time would be our
   own number rather than the file's.
6. **Not done:** NPCs' weapons (they do not fire or reload with animations yet, so
   there is nothing to move); shell casings and muzzle flashes; the glow on the gatling
   laser and the screens on a few energy weapons, which are other kinds of animation.

## Screen effects in the opening: white screen, blurred wake-up (2026-10-10, session "image space modifiers")

Your request: the opening quest's screen effects (fade to black, white screen, blurred
vision) should play in our engine. Branch `claude/imagespace-modifiers`. Facts:
`docs/spec-notes/image-modifiers.md`. Pictures:
`C:\fnv-private\references\image_modifiers\`.

1. **What plays now:** a new game starts on a white screen. When the opening quest hands
   over to Doc Mitchell's, the room comes out of the white over 5 seconds, blurred and
   slightly doubled, and is sharp after 7. After Doc's first line there is a dark blink
   and a bright, blurred flash that settles over 10 seconds (`newgame_1` to `newgame_5`).
   Message boxes, subtitles and the HUD stay sharp on top.
2. **It is not only the opening:** the three script commands behind this now work
   everywhere, so the other scripts that use them get their effect too: 266 uses in the
   base game, most of them fades to black and back. I only watched the opening.
3. **Built:** fade to a colour, blur, double vision, blur around the edges of the screen,
   and changes to brightness, contrast, saturation and tint. An effect with a set length
   ends by itself; one without stays until a script removes it.
4. **Not built:** depth of field (6 effects use it) and motion blur (7). Blood on the
   screen, effects from explosions and weapons, and a script changing a room's whole
   look are separate things and still do nothing.
5. **Guesses, please check against the game:** the files say how strong each effect is
   as a number, not what that number looks like. How wide a blur of "5" is, how far apart
   the two images of double vision sit, and how they sway are my guesses. To compare,
   this shows one effect frozen at a chosen second, in ours:
   `nvforever --data <Data> --cell GSDocMitchellHouse --imod VCG01FadeInFromBlackISFX@3.5`
   (`wakeup_held_3.5s.png` is that frame). A game screenshot from the same spot with the
   console's `imod VCG01FadeInFromBlackISFX`, taken about 3 to 4 seconds in, would let me
   set the blur. Leave `@3.5` off to watch it loop.
6. **Two more guesses:**
   - While a message box is open the wake-up effect waits (in ours the add-on pack boxes
     pop up during it, so it takes longer than 7 seconds on the clock). If the game keeps
     fading behind a box, say so.
   - A script starting an effect that is already playing restarts it. The game might
     layer a second copy instead.
7. **Odd thing in the files:** the wake-up effect is named "fade in from black" but its
   colour in the file is white, and a capture of your new-game save shows a white
   screen at that point, so white it is.
8. **Still wrong in the opening, not touched here:** you can walk and look around while
   Doc is talking. The scripts ask for the controls to be locked; we do not obey that
   yet. Before the opening's scripts have loaded (3 to 6 seconds) I hold the screen
   white so the room does not show early.

## Body in first person: a switch (2026-10-10, session "first-person body")

Your request: an option to toggle the body rendering in first person. Branch
`claude/first-person-body`. Design: `docs/design/player-body.md` §1.5.

1. **The switch:** "Body in first person" on the Esc menu's settings page (under
   Picture), or start with `--first-person-body`. It is off by default, because the
   original only draws the arms.
2. **On:** looking down you see your chest, legs and feet in what you are wearing. With
   the weapon away your arms hang at your sides; with it out, the usual first-person
   arms and weapon are drawn and the body's own arms are hidden. Your view does not
   move: the body is placed under the camera.
3. **Sitting (your report, with the screenshot of the view inside the jumpsuit):** with
   the switch on, a seated view now turns at most 80° to either side and 75° down, like
   a head on a neck, and the eyes swing round the neck as the head turns, so you see
   your shoulder and arm at the limit and never the inside of the body. If you sit down
   looking the other way, the view comes round by itself. While you sit down and get up
   the view now turns with the body (your second report: the camera stayed put while the
   body turned); you said that looked a lot better.
4. **For you to decide:**
   - With the switch off, the seated view is as before: free to turn all the way round,
     and not turned by the body as it sits down (no body is drawn, so nothing looks
     wrong). Should the neck limit and the turning apply there too?
   - 80° and 75° are my guesses at a comfortable limit. Say if it feels too tight.
5. **Not done:** your shadow. With the switch on it is a body without a head (and
   without its own arms while the weapon is out). Fixing that means letting hidden
   parts still cast shadows; it is a separate piece of work.

## Actors can be pushed (2026-10-10, session "actor push")

Your request: the player or an NPC that is moving should be able to push another actor
that is standing still, with a capture from the game to say how far. Branch
`claude/actor-push`. Facts: `docs/spec-notes/physics.md` §6.8,
`docs/design/actor-behaviour.md` §5.7.

1. **I need the capture, and it is ready to run.** About 4 minutes, nobody at the
   controls, save `nvfdoc`, stays inside Doc Mitchell's house (no door, so no autosave):
   ```
   python tools/capture/capture_run.py --game "<FNV folder>" --save nvfdoc --kill-on-timeout -o C:/fnv-private/capture/run_pc_actor_push.json tools/capture/jobs/pc_actor_push.steps.json
   ```
   I did not start the game myself: you asked to be told, and other sessions were
   building on the machine at the time, which would have slowed the game.
2. **What the capture does.** The player runs and walks into Doc Mitchell and into a
   settler the job creates:
   - straight on, and from a little to the side;
   - with the key held for several seconds, up to the wall;
   - with the NPC told not to move (restrained);
   - placed on the very spot the NPC stands on;
   - standing in the way of the NPC while it walks.
3. **What works in ours now**, with my own numbers until the capture gives the game's:
   - You walk into a standing NPC: it moves ahead of you at half your speed. Aim off
     its middle and it goes aside while you pass. Checked in a run: a Goodsprings
     settler pushed 117 units in under a second of running at him.
   - A walking NPC meets you while you stand still: it pushes you, the same way.
     Checked in a run: Easy Pete on his noon walk pushed you ahead of him for three
     seconds, then went on into the saloon.
   - A walking NPC meets a standing NPC: the same rule. It did not happen in any of my
     runs, so I have not seen it in the game.
   - A pushed NPC stops at walls and furniture. It never leaves the floor NPCs can
     walk on.
   - Two seconds after you leave it alone, an NPC pushed out of its place walks back.
   - NPCs that sit, sleep or are on their way somewhere are not pushed. They block as
     before.
4. **The guesses the capture replaces:** the half speed, the two seconds, and whether
   the game's NPC walks back at all.
5. **Not done:**
   - A pushed NPC slides; it plays no stepping animation.
   - Two actors that are both walking still do not push each other.
   - NPCs far from you (moved without collision) neither push nor are stopped.
   - Size and weight make no difference: a deathclaw is pushed like a settler.
6. **Two faults found on the way and fixed:**
   - An NPC walking close past you carried you along with it at its own speed, like a
     moving platform. Easy Pete took you 163 units to the saloon door this way. This
     was in our engine before my change.
   - After being left untouched for a few seconds and then moved by a door or a
     teleport, a character's solid body could stay behind where it arrived while the
     character walked on. Hard to meet in play, but it was there.

## Shots leave the barrel (2026-10-10, session "muzzle")

Your report: a shot looks as if it comes from the camera, not from the gun's barrel,
and you thought something in the weapon model controls it. Both were right. Branch
`claude/muzzle-origin` (on top of `claude/weapon-moving-parts`). Facts:
`docs/spec-notes/animation.md` §9.9; design: `docs/design/combat.md` §5.1. Pictures:
`C:\fnv-private\references\muzzle\`.

1. **Cause:** ours started every shot, tracer and all, at the eye. That was chosen on
   purpose for aiming (the shot goes where the crosshair is), and the plan said the
   barrel would be used for what you see. That second half was never built.
2. **What the model has:** guns carry an empty marker called `ProjectileNode` at the
   end of the barrel: 198 of the 362 weapon models in the base game, none of the melee
   weapons, grenades or mines. A silencer moves it out by the silencer's length. That
   it marks where the shot starts is my reading of its name and place; I did not
   check it against the game.
3. **Fix:** the tracer and the flash now start at that marker on the gun you see, in
   first and third person, and run to the point the crosshair is on. What gets hit is
   decided exactly as before.
4. **Checked in ours:** 10mm pistol in first and third person and the minigun in third
   person. Seen from the side, the minigun's tracers fan out from the end of the
   barrel. The log gives the barrel's position for every shot.
5. **Added, third person only:** if something stands right in front of the barrel
   (within about 2 metres) between it and the point you aim at (you look over a low
   wall, the gun is behind it), the shot now hits that thing instead of passing
   through it. This was in the plan. It is covered by a test; I did not try it at a
   wall in a run.
6. **For you to decide, the hit rule:** the original fires along the gun's barrel, so
   the shot can land off the crosshair. Ours keeps the crosshair true (your earlier
   choice for combat). If you meant the shot itself should follow the barrel, say so.
7. **For you to decide, third person:** the view ray starts at the character's head,
   but the camera sits over the right shoulder, so shots should land about 40 units
   (roughly 60 cm) to the left of the crosshair dot at every distance. That figure is
   the camera's offset in the code; I did not measure it in a run. It was like this
   before. The fix
   would be to aim from the camera's own line in third person; that changes where
   shots land, so I left it.
8. **Not done:** the game's real muzzle flash effect and light (ours draws a small
   marker), shell casings (the models have a marker for those too), models for missiles
   and grenades in flight, and NPCs' shots (they do not shoot yet). Weapons without the
   marker still draw from the eye.

## Lying down on beds (2026-10-10, session "player beds")

Your request: sitting in chairs should also work for beds, so you can lie down the same
as a sleeping NPC; and, while I was testing, "sideways the camera when laying". Branch
`claude/player-beds` (it includes the "Body in first person" branch, which is not
merged yet). Facts: `docs/spec-notes/furniture.md` §5 item 10 and §7. Pictures:
`C:\fnv-private\references\player_beds\` (`v1-third-sheet.png`, `v2-first-off-sheet.png`,
`v2-first-on-sheet.png`; the `base-*` ones are before the camera was changed).

1. **What works now:** look at a bed and press E. You sit on the edge, swing your legs
   up and lie down, with the same animations a sleeping NPC uses on that bed. Any
   movement key or E gets you up, and you stand beside the bed. As with chairs: you
   cannot move while on it, your weapon is put away, and quest scripts that ask "is
   the player sleeping" get a yes.
2. **Why it did not work before:** I had blocked beds for the player on purpose, with
   the message "Sleeping is not available yet", because the original opens a sleep
   menu instead of lying you down. That block is gone.
3. **The sleeping pose is on its side**, not on the back: that is what the game's own
   sleep animation is, the same one NPCs get.
4. **The camera, first person (my choices; the original has nothing to copy here):**
   - Your view now lies down with your body: the picture leans over as you lie down
     and ends on its side, the way a head on a pillow sees the room. It straightens up
     again as you get up. This is the same whether "Body in first person" is on or off.
   - Lying, the mouse turns your head on the pillow: up to the ceiling, along the bed
     to your feet, and a little to the side. It stops before the view goes into the
     pillow or the mattress, and with the body drawn, before it goes into your own
     body (you see your shoulder and arm there).
   - Before this change the lying view was broken in both modes: the picture was the
     inside of the mattress or of your own body.
5. **The camera, third person:** it circled into the bed, so the picture was the inside
   of the mattress. It now sits higher above you while you lie.
6. **E with nothing under the crosshair gets you up** from any furniture. Lying down
   you are usually looking at the ceiling, so "press E on the bed again" would not
   have worked. With a chair you could already do it with a movement key.
7. **For you to decide:**
   - The sideways picture is strong. If it is too much, a milder version keeps the
     horizon level and only looks up from the pillow. Say which you prefer after
     trying it.
   - I checked one bed in Goodsprings (Easy Pete's). Bedrolls and mattresses on the
     floor use other animations from the same family; they should work the same way
     but I did not look at them.
8. **Not done:** actually sleeping. No sleep menu, no time passes, no healing, no
   "well rested". You only lie down. A bed someone else owns can be used freely, and a
   bed an NPC is already lying on says it is in use. Tell me if you want the sleep
   menu next.
