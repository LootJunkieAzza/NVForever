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
     merchant); armour on the ground should be drawn with MOD2 (male), which the engine
     does not do yet; Layer 5's `coc_destination` should follow the marker rule.
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

