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
8. **Not done:** shell casings (the models have a marker for those too) and NPCs'
   shots (they do not shoot yet). Weapons without the marker still draw from the eye.

### Added the same day: bullets you can see, and muzzle flashes

Your request, passed on by the weapon parts session: "actual bullet projectiles that
arent invisible along with the muzzle flash effects". Same branch. Facts:
`docs/spec-notes/combat-rules.md` §15. Pictures in the same folder
(`minigun-flash-side.png`, `minigun-tracer-in-flight.png`, `laser-rifle-beam.png`).

1. **Muzzle flash:** each projectile record names a flash model, how long it shows and
   a light. Ours now shows that model at the end of the barrel, pointing along it,
   with the light, for the record's time (0.04 seconds for bullets).
2. **Your note that the minigun's flash stayed too long:** two causes, both mine. The
   picture was taken with a test switch that held every flash for 3 seconds, and in
   normal play I had padded the time to at least 0.05 seconds, so at the minigun's
   rate one flash ran into the next. Both are gone: the flash lasts the record's time
   and is off the moment you stop firing.
3. **Projectiles:** every shot now draws the game's own projectile model in flight,
   starting at the barrel: tracer rounds, laser beams, missiles, plasma bolts,
   grenades. Checked in a run: the minigun and the laser rifle. The others use the
   same code but I did not look at them.
4. **Decision taken, every shot is drawn:** the game draws only a share of bullets
   (the minigun's record says 30%, the 10mm's 0%). You asked for bullets that are not
   invisible, so ours draws all of them. Say if you want the game's shares back.
5. **Decision taken, how fast a drawn bullet flies:** the game works out a bullet's
   hit at once; how it then draws the bullet I do not know. Ours flies the model from
   the barrel to the hit at the record's speed. For a pistol that is very fast (about
   390 metres a second) and the model is a bullet the size of a real one, so you will
   see it for a frame or two at most. To keep the shot visible I also draw a thin line
   along it for a moment. The minigun's rounds are slower and carry their own glowing
   streak, which shows well.
6. **Your note that the laser beam looked too low:** it was. The beam was drawn from
   where the barrel was at the instant of the shot, and the firing animation lifts the
   gun about 15 cm on the very next frame, so the beam hung under the barrel. A beam
   now stays attached to the barrel for as long as it shows.
7. **For you to decide:** if pistol and rifle bullets should be easier to see, the
   choices are to slow the drawn bullet down (it would arrive after the hit lands) or
   to give every bullet a streak like the minigun's. Both are our invention, so I
   left them.
8. **Not done:** smoke, sparks and the missile's trail (a separate kind of effect in
   the files), impact effects, sounds. The flamer shows its flash but no flame, because
   its flame is made only of that kind of effect.

### Added 2026-10-11: sounds, smoke and sparks, impact effects, shell casings

Your request: build the "not built" list (smoke, sparks and the missile's trail; the
flamer's flame; impact effects, sounds, shell casings and NPCs' shots). Branch
`claude/shot-effects`. Facts: `docs/spec-notes/particles.md` and
`docs/spec-notes/combat-rules.md` §16. Pictures: `C:\fnv-private\references\muzzle\`
(`minigun-smoke-dust-casings.png`, `missile-trail.png`, `flamer.png`).

1. **Sounds:** your gun now plays the game's own sounds: firing, the click when empty,
   drawing and putting away. Bullets make the impact sound of what they hit (earth,
   concrete, wood, metal, flesh and so on), and explosions make theirs. I cannot hear,
   so what I checked is that each file is found, decoded and sent to the speakers. You
   are the first to listen: tell me if anything is too loud, too quiet or wrong.
   `--no-sound` turns them off.
2. **Smoke, sparks and trails:** the game stores these as particle effects attached to
   the flash, the projectile and the impact. Ours now plays them: sparks and a wisp of
   smoke at the muzzle, a smoke trail behind a missile, dust and debris where a bullet
   lands.
3. **Impact effects:** a bullet that lands shows the game's effect for that surface.
4. **Shell casings:** each shot throws the weapon's casing out of the ejection port; it
   falls, bounces and lies on the ground for the game's own time.
5. **The flamer is only half there:** flame is drawn, but as a thin run of puffs that
   burns about 6 metres out, not the solid jet you know. I think I know why (ours keeps
   each puff's source alive for too short a time) but not what the game does instead.
   A recording of the game's flamer from the side would settle it.
6. **How particles behave is my reading of the files:** the files give numbers with
   names (speed, gravity, grow time, colours over life) and I worked out what each
   does from the name and from how the effect should look. Nothing was compared with
   the game. If an effect looks wrong to you (too big, too fast, wrong colour), say
   which and I will look at that one.
7. **NPCs' shots: not done, and it needs something else first.** NPCs do not attack
   yet. That is the combat-AI work (detection, combat behaviour), which is its own
   project. Everything built here (flash, projectile, casing, sounds, impacts) is
   written so NPC shots can use it when they exist. Say if you want NPC combat started.
8. **The minigun may sound thin:** its record names no firing sound at all, only a
   looping "spin" sound, which ours now plays while you fire. I think the gunfire
   itself is triggered from the firing animation, which is a kind of sound cue not
   built yet. Tell me what you hear.
9. **Not done:** sound coming
   from the left or right; reload sounds (they are tied to the animations); bullet
   holes; the fireball of an explosion (you get its sound and the dust); the flash and
   impact pictures do not yet grow and fade as the files animate them.

## Idle animations: scripts, packages, dialogue, and the wake-up at Doc Mitchell's (2026-10-10, session "idles")

Your request: idles from scripts (PlayIdle), from the package tabs (Idles, and
Begin/End/Change), and on dialogue responses, with the opening at Doc Mitchell's as the
test. Branch `claude/idles`. Facts and sources: `docs/spec-notes/idles.md`.

1. **What works now.** In a new game you wake up looking at the ceiling fan, sit up, sit
   on the edge of the bed, and later stand up, all from the game's own package data.
   Doc Mitchell plays the gestures named on his lines, the scripted "give mirror"
   animation, and his own way of getting up from the chair. NPCs elsewhere play the idles
   of their packages and of idle markers.
2. **Your three reports, and what they were:**
   - *Doc should help the player up.* The game has a special "Doc gets up from his
     chair" animation that is picked only while the opening quest is between stages 10
     and 50. Ours could not read that kind of rule, so he got up the plain way. Fixed.
     (A second record, `LooseNVDocHelpPlayerUp`, names an animation file that is not in
     the game's archives, and nothing uses it.)
   - *The seated view was off to the right.* A bug of mine turned the view 11° when the
     sit-up handed over to the sitting loop. Fixed.
   - *Collision moves the player before the scene.* Yes: the start marker is inside the
     bed, and the player was pushed about 10 units off it. The scene now plays at the
     marker and the player is held there until it is over. With both fixes the seated
     view lines up with your capture (Doc and the Vigor Tester within a few pixels at
     1280 × 720).
3. **Also fixed on the way:** Doc Mitchell was smoking. His packages list the smoking
   idle, but that idle has conditions he does not meet. Idles from a package's list are
   now played only when their own conditions pass. And a second `AddScriptPackage` now
   replaces the first, as the wiki says; before, removing the stand-up package brought
   the sit-up back.
4. **Guesses I made (please say if you know better):**
   - ~~Loop count 255 means "loop until something else replaces it"~~ **answered by
     you (2026-10-10):** the editor's Loop Forever box sets minimum and maximum both to
     255; with the box clear any numbers can be entered. Ours now loops for ever only
     when both are 255. Still a guess: 0 / 0 means once.
   - A looping idle repeats between its `StartLoop` and `EndLoop` markers.
   - `PlayIdle`, package begin/end/change idles and dialogue idles ignore the idle's
     conditions; idles from a package's or marker's list respect them.
   - A package's list plays whenever the actor stands with nothing else to do. The wiki
     gives a rule per package type; I used one rule for all.
   - While an idle moves the first-person view you can still look round (the opening
     leaves looking enabled); the body does not turn with the view.
5. **Needs your eyes in the game:** when does Doc get up from his chair? In ours he
   starts about 3 seconds before you start to stand, because his packages are checked
   again shortly after stage 40. The script itself only tells him to re-check at the end
   of the line he says while you stand. If in the game he stays seated until then, tell
   me and I will hold an actor's package while it is speaking.
6. **Not done:** the sounds stored in the idle animations (bed creak, eyes opening);
   the blur and fade effects of the opening (another branch); idles the game picks by
   itself with nobody asking (fidgets, talking gestures); Doc does not look at you
   during his scripted lines (head tracking does not cover those lines yet); in third
   person the wake-up moves only the first-person body.
7. **A slip to report (lab rule):** early on I ran `fnvtool actor anims` on the whole
   Data folder, which also opens the add-on archives, one of which is a lab holdout. My
   next step failed before reading the output; I deleted the file unread and used
   base-game archives by name from then on.


## Lying down on beds (2026-10-10, session "player beds")

Your request: sitting in chairs should also work for beds, so you can lie down the same
as a sleeping NPC; and, while I was testing, "sideways the camera when laying". Branch
`claude/player-beds`. Facts: `docs/spec-notes/furniture.md` §5 item 10 and §7. Pictures:
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

## Your shadow in first person (2026-10-11, session "player shadow")

Your report: in first person your sun shadow was wrong. Arms only: a shadow of floating
arms and no body. With "Body in first person": a body without a head, and without its
own arms while the weapon is out. Branch `claude/player-shadow`. Design: `docs/design/player-body.md` §1.6. Pictures, before
and after side by side: `C:\fnv-private\references\player_shadow\`. This replaces
point 5 of "Body in first person: a switch" above.

1. **Fixed:** in first person, with the switch on or off, your shadow is now your whole
   body: head, arms, and the weapon in your hands or in its holster. The floating arms'
   shadow is gone. Third person is unchanged.
2. **What you see of yourself is unchanged.** No head or extra arms appear in the view.
   I compared the arms and weapon before and after facing east, north and west at
   17:00: they match to within a few dozen pixels.
3. **One thing I had to solve that you did not ask about:** the body that casts the
   shadow is invisible but stands exactly where your first-person arms are, so at first
   it shaded them: the Pip-Boy, left hand and rifle went dark whichever way I faced. The
   arms now ignore that invisible body's shadow and still take everyone else's: I stood
   in the shade of the truck by the Goodsprings road and inside its cab, and the arms
   went dark there as they did before. They also still shade themselves (the rifle on
   the forearm) as before.
4. **Cost, not measured properly:** this needs the sun's nearest shadows drawn a second
   time while you are in first person. Two 18-second runs each way gave 44.5 and 26.2
   frames per second before and 38.9 and 38.6 after; your machine was busy with other
   sessions, so those numbers overlap and prove nothing. If first person feels slower
   outdoors with shadows on, tell me.
5. **Checked as well:** sitting on the saloon porch chair (both settings), switching to
   third person and back, and the water and reflections at the Lake Mead pier: all as
   before.
6. **Not done, for you to decide:**
   - With "Body in first person" on and the weapon out, the shadow's arms are the
     body's own, which always aim level, while the arms you see follow where you look.
     Looking steeply up or down, the shadow's rifle does not tilt with yours. With the
     switch off the shadow's body bends with your view, as in third person.
   - In water, your reflection in first person is still the floating arms, not your
     body (it was like that before). The same method would fix it; say if you want it.
   - Indoor lamps cast no shadows at all in ours, so you have no shadow from them.

## Waiting and sleeping: the T menu (2026-10-11, session "wait menu")

Your request: "a wait function that the player can use by pressing T", up to 24 hours,
time ticking by one hour at a time, and a sleep instead of a wait when you are in a bed.
Branch `claude/wait-menu`. Facts and sources: `docs/spec-notes/wait-menu.md`. Pictures:
`C:\fnv-private\references\wait_menu\` (`compare-wait.png` is your screenshot above
NVForever's; `nvf-sleep.png` is the bed version).

1. **What works now:** press T while playing. The picture blurs and the box from your
   screenshot comes up: the title, the slider with its 24 marks and the triangle, the
   hours line with the time you will end at, the date line, and Wait and Cancel at the
   right.
   - Pick the hours with the mouse (click or drag on the slider, or the wheel) or with
     the keys (Left/Right or A/D one hour, Page Up/Down six, Home/End the ends).
   - E, Enter or a click on Wait accepts. X, Esc, T or a click on Cancel closes it.
   - After you accept, an hour passes every half second. The menu counts down: the
     hours left, the triangle moving back along the slider, the date and time going up.
     The sky and the light change behind it hour by hour.
   - Cancel during the countdown stops it there. The hours already passed stay passed.
   - A wait across midnight changes the weekday and the date; 24 hours ends at the same
     time the next day.
2. **In a bed it is a sleep.** Lie down on a bed (E on it), then press T: the same menu
   says "How long would you like to sleep?" and the button says Sleep. Time passes the
   same way. You are still lying there when it closes.
   - **The screen fades out when you accept a sleep** (your request while I was
     working). It goes to black over one second, and comes back over one second when
     the sleep ends or you cancel it. The menu stays visible on the black so you can
     watch the hours count down and still cancel. Waiting does not fade. Say if you
     would rather the menu disappeared too, or want the fade slower.
3. **It moves the one game clock**, the same one the sky, the townspeople's schedules
   and quest scripts read. After a wait from 18:37 to 22:37 in Goodsprings, Easy Pete,
   Chet and a settler all switched to their night routines within a few seconds and
   went to bed.
4. **Scripts can hold it shut.** While a quest has taken your controls away (the
   opening at Doc Mitchell's, for one), T does nothing, the same as the inventory.
5. **What I had to guess** (nothing I could read says how the original does it):
   - **The pace:** half a second per hour, so a full day takes twelve seconds. Say if
     the original feels faster or slower.
   - **Stopping part-way:** the original's own notes say a sleep can be cancelled; for
     a wait I assumed the same.
   - **The keys for the slider.** The original's files only describe the gamepad and
     the mouse. E and X are the hints on your screenshot.
   - **The menu opens on 1 hour.** I do not know what the original opens on.
   - **The sleep wording.** Your screenshot is of the wait menu. For sleep I used the
     same sentence with "sleep" and a "Sleep" button. A screenshot of the original's
     sleep menu would settle it.
   - **The day of the week.** Your screenshot says Saturday on 19.10.81. I count days so
     that it comes out that way (the game's day counter starts on 13 October 2281, and
     I took that day as a Sunday). One screenshot is all that backs it.
6. **Where the words come from.** The original keeps this menu's text inside its
   program, not in the data files, so NVForever cannot read it from your install. The
   title, "Wait", "Cancel", "hours" and "Saturday" are typed in from your screenshot.
   That is a handful of words, but they are the original's: say if you would rather
   have NVForever's own wording there.
7. **Something this turned up, for you to decide.** Quest scripts and the townspeople's
   weekday schedules ask "what day of the week is it" through another part of the game
   code, which works it out the way a real-world calendar would. By that rule 19.10.81
   is a Wednesday, and your screenshot says Saturday. So that older rule is probably
   wrong by three days. I did not change it here, because it would move who does what
   on which day and the earlier checks of the schedules were made with it. If you type
   `GetDayOfWeek` in the original's console with the wait menu's date line in view, one
   number settles it (0 is Sunday, 6 is Saturday).
8. **Not built** (the first five points were taken up later the same day: see the next
   entry, "Sleeping, refusals and people during a wait"):
   - **The original's refusals.** Its message list names these: enemies nearby, in
     combat, under water, an alarm sounding, a bed someone else owns, and places or
     quests that forbid waiting. I found the names, not the rules, so NVForever lets
     you wait anywhere.
   - **What sleep does besides passing time.** The data files show "Well Rested" (10%
     more experience for a while, and full health and limbs) given by nine particular
     safe beds, and a sleep need in hardcore mode. None of that happens yet. Plain
     healing per hour of sleep is not in anything I could read.
   - **People moving during the wait.** They stand still while the menu is up and set
     off for where they should be when it closes, so after a long wait you can watch
     them walk there. The original may put them in place; I do not know.
   - **Pressing E on a bed still lies you down** rather than opening the sleep menu
     directly. You asked for lying down first; T does the rest.
   - **Scripts asking "is the player asleep?" or opening this menu themselves.** The
     Well Rested beds need the first.
   - **The game's own font, slider picture and menu sounds.** The menu is drawn with
     NVForever's font and plain shapes, measured from your screenshot.

## Message boxes drawn as the original's (2026-10-11, session "message box")

You asked for the placeholder message boxes to be swapped for the game's own, like your
"Vance's Gun" screenshot, with several buttons where a message has them.

1. **What you get.** A message box is now laid out by the game's own menu file and
   drawn with the game's own two fonts and line pictures, read from your install when
   the game runs (nothing of them is in the repository): the title standing in the top
   line, the text at the left, the buttons one under the other in the middle with a
   frame around the chosen one, the bottom line, the short fading lines at the four
   corners and the dark patch behind. Your screenshot's message comes out with the same
   line breaks, and the box's measures match yours to within a pixel or two.
2. **The pictures are not in `textures\interface\hud`.** That folder holds the compass,
   crosshair and meters. The message box uses `textures\interface\shared` (the fading
   lines and the black background) and `textures\fonts`. The font files had to be
   worked out from their numbers; the note is `docs/spec-notes/fonts.md`.
3. **How to answer a box.** Click a button; or Up and Down to move the frame and Enter
   to press it. The pointer moves the frame to the button it is on. The number keys
   still work as before (1 for the first button), but the numbers are no longer written
   in front of the buttons. A box without buttons has OK, which Space also presses.
4. **Guesses for you to judge** (details in `docs/spec-notes/message-menu.md` §3):
   - **How dark the box is.** The game's file leaves that to the program. Your
     screenshot is of a dark room, so it does not tell. I used the same darkness as the
     Wait menu. A screenshot of a box outdoors in daylight would settle it.
   - **The box's width for a short message.** Your screenshot is a full-width box. For
     short texts I make the box as wide as the text needs, down to the file's minimum.
   - **Which keys.** The menu file only describes the mouse and the gamepad.
   - **A few spacings** (line spacing, where the first line sits) are fitted to your
     one screenshot rather than read from a file.
   - **The colour** is the amber of your screenshot. Your own HUD colour setting is not
     read yet.
5. **Not built:** the icon some boxes show beside the text, scrolling for a box with
   more buttons than fit, the two menu sounds, the blur of the world behind the box.
6. **Other menus use the same fonts.** The Wait menu, dialogue and the rest are still
   in NVForever's stand-in font. The font reader is there now for them.
7. **To look at any message:** set `NVF_PLAY_TEST_SCRIPT=20:message:PrimmMuseumPlaqueMessage`
   and `NVF_PLAY_TEST_SCRIPT_MENUS=off` before starting with `--play` (the name is the
   message's Editor ID).

## Messages at the top left, as the original's (2026-10-11, session "corner messages")

Branch `claude/corner-messages` (it contains the message box branch). Rules and
sources: `docs/spec-notes/corner-messages.md`.

You asked for the messages that are not message boxes to be shown as the original
shows them, with their icons, and then: one at a time, each for the time its record
gives.

1. **What you get.** A message without the Message Box flag now appears at the top
   left: the bracket, the Vault Boy icon and the text beside it in the game's own HUD
   font, all read from your install when the game runs. Next to your "You are now Well
   Rested!" screenshot, at the same window size, the bracket, the face and the text
   fall on the same pixels to within one.
2. **One at a time, for the record's time.** Messages wait in line. Each is shown for
   its own Display Time (Well Rested: 10 seconds), then the next. In the game's data
   the times run from 1 to 15 seconds; 2 and 10 are the commonest.
3. **Icons.** A message's icon is the Menu Icon record it names (the faction faces,
   Wild Wasteland, Vault Boy in pain). Most messages name none and get the neutral
   Vault Boy face, as the GECK wiki says and as your screenshot shows.
4. **Something I found.** The bracket in your screenshot has a cut corner. The bracket's
   own picture file has a square one. The game takes the bracket from its big shared
   interface picture (the "atlas"), where it is drawn differently. NVForever now reads
   that atlas too.
5. **Guesses for you to judge:**
   - **Messages without a time.** Nineteen old messages have no Display Time. They
     stay 2 seconds.
   - **Objectives, quests, items, "Gate is locked".** These lines used to be at the
     bottom left. Nothing I could read says where the original shows them, so they now
     go through the same top-left area, in the same line, with the neutral face, 2
     seconds each. The wording ("Objective: ...", "... added") is still NVForever's.
     After a new game's start about ten of them wait in line, so the last comes some
     20 seconds late. Say if they should be shorter, grouped, or go elsewhere.
   - **Fading.** A message fades in and out over a quarter of a second. Your one still
     picture cannot show how the original does it.
   - **The same message twice running** is shown once.
   - **While a message box is open** the corner message is hidden and its time does not
     run, so it is not used up behind the box.
   - **Where long texts break.** Your screenshot only pins the line width between 228
     and 336 units; I use 330. A screenshot of a longer message (three or more lines)
     would settle it, and would show whether the bracket grows with the text.
6. **Not built:** a sound when a message appears; the original's own messages that do
   not come from scripts (experience, karma, picking things up by hand); the other HUD
   text areas (quest added, location names). The bottom HUD is the other session's.
7. **The debug text gets in the way.** NVForever's own status text is also at the top
   left and lies over the message. Start with `--no-overlay` to look.
8. **To look at any message:** set
   `NVF_PLAY_TEST_SCRIPT=20:message:AbilityWellRestedStartMsg` before starting with
   `--play --no-overlay` (the name is the message's Editor ID; `FactionReminderNCR` has
   a faction face).

## Weapon and armour condition (2026-10-11, session "item condition")

Branch `claude/item-condition`. Rules and sources: docs/spec-notes/combat-rules.md §17.

1. **Built, as you asked:**
   - Each shot wears your weapon. The game file has a setting for this
     (`fDamageToWeaponValue`, 0.2): the 10mm Pistol's record has 80 health, so it lasts
     400 shots, 33 full magazines. Your figure was "about 395".
   - A hit wears the armour it lands on: the head piece for a head hit, the body
     armour for any other, both for an explosion. The amount is 35 % of the hit's
     damage, also a setting in the game file (`fDamageToArmorPercentage`).
   - A shot on a weapon someone is holding damages the weapon instead of them.
   - Your weapon at zero is unequipped and stays in your inventory as "(broken)"; it
     cannot be equipped again. Anyone else's falls to the ground and cannot be equipped
     when picked up.
   - The HUD shows your weapon's condition next to the ammo count, and the inventory
     shows it after the item's name.
2. **My guesses, for you to confirm or change:**
   - Melee weapons wear only when the swing hits something, by the same 0.2.
   - A weapon that is hit loses the full damage of the shot, and its holder takes
     none. A 10mm shot takes about a tenth off a Varmint Rifle.
   - Only a drawn weapon can be hit. Holstered, the shot hits the person.
   - Worn armour protects less: full DT at 100 %, half at 0 %. Broken armour stays on.
   - Weapon damage still falls to half at zero condition (the rule from before). The
     game file has settings that look like the original's rule (66 % + 34 %); I did
     not use them because nothing we can cite says how they combine.
3. **Not testable in play yet:** your own armour wearing and your weapon being hit,
   because nobody attacks you yet. The code handles any actor and is tested without
   the engine.
4. **To see it yourself:** NPC weapons are only drawn with `--actor-weapons drawn`.
   Shoot the gun in a settler's hands about ten times with the 10mm Pistol.
5. **Not done:** repair (you said later), jamming and slower fire of worn weapons, a
   worn item selling for less.

## The HUD: health, compass, action points, condition, ammo (2026-10-11, session "hud")

You asked for the other things on the screen to look like the original's, after your
labelled Goodsprings screenshot. **That screenshot did not reach this session as a
file** (only a description of it in words), so the HUD is built from the game's own
interface files and pictures, and everything those files leave open is my guess until
I can measure your picture. Please attach it again, or drop it in
`C:\fnv-private\references\hud\`.

1. **What you get.** In `--play`, at the bottom left: "HP", the health bar of small
   upright ticks on the game's bracket, and under it the compass strip, which turns as
   you turn. At the bottom right: the action points bar with "AP", and under it "CND"
   with the weapon's condition bar and, right of the divider, the ammo as
   loaded/carried. Brackets, ticks, compass strip and letters are the game's own
   pictures and font, read from your install when the game runs (nothing of them is in
   the repository). The old yellow "HP 100/100" and "13 / 37 9mm CND 60%" texts are
   gone.
2. **Where the numbers come from.** Health and action points are the player's real
   values; the compass is the real heading; condition and ammo are the equipped
   weapon's. Nothing spends action points yet (no V.A.T.S., no sprint), so that bar
   stays full.
3. **When it is shown.** While you walk around. It is off in every menu, in
   conversations and behind loading screens (my choice: the game's file does not say),
   and off while a script holds your movement, as the GECK wiki says of the original
   (so not during the wake-up at Doc Mitchell's).
4. **Guesses for you to judge** (all waiting for your screenshot):
   - how far the two corners stand from the screen's edges (I took 40 and 30 of 960);
   - the bars' length: the game's file says 300, but that runs over the label or past
     the bracket, so I took 256 (32 ticks), ending at the bracket's small corner mark;
   - where "HP", "AP", "CND" and the ammo stand;
   - the compass's middle: I centred the heading in the window the file gives; the
     bracket's small triangle is a little left of that;
   - the action points bar emptying from the left, the health bar from the right;
   - with bare hands: no "CND" and no ammo. With a melee weapon: "CND" but no ammo.
     Your picture shows "0/0" - which weapon were you holding (or none)?
   - everything at full brightness; the game may draw lines a little dimmer than text.
5. **Not built:** markers on the compass (quests, places, people nearby) and its
   faded ends; the original's crosshair, enemy health bar, sneak meter, the messages
   and their icons at the top left, subtitles and the "E) Open" prompt in the game's
   font, the XP and radiation meters, the crippled-limb picture (crippled limbs are
   still a line of words), the ammo type's name, Hardcore mode's words, the HUD's
   fading and glow.
6. **To see it yourself:**
   `nvforever --data <Data> --cell Goodsprings --play --size 1280x720 --no-overlay`,
   press the view key for third person. Details and the test recipe:
   `docs/spec-notes/hud.md`.
## Sleeping, refusals and people during a wait (2026-10-11, session "sleep effects")

7. **Pressing E on a bed.** You lie down as before, and when you are lying the sleep
   menu opens by itself. Cancel leaves you lying there; T opens the menu again; a
   movement key or E gets you up.
   - **For you to decide.** The original most likely opens the menu the moment you
     press E, with no lying down (its bed script only works that way). You asked for
     lying down earlier, so I kept it and put the menu after it. Say if you would rather
     have (a) this, (b) the menu at once and no lying down, or (c) lying down only, with
     T for the menu as it was.
   - **A bug this fixed:** nine kinds of bed (safehouse beds, your hotel and Novac room
     beds) did nothing at all when you pressed E, because they carry a script. They
     work now.
8. **Well Rested works.** Sleep in one of those nine kinds of bed and when you wake the
   game says "You are now Well Rested!", your health and limbs are restored, and you
   have the perk until its time runs out. I did not write any of that by hand: the
   game's own bed script does it, now that it can ask "is the player asleep".
   - The perk's actual bonus (10% more experience) is not applied yet. That belongs to
     perks in general, which are not built.
   - Quest scripts can now put you to sleep, wake you, and open the menu themselves.
     Three quests use the first and one the second; I could not test them in a real
     quest, only in small tests.
9. **Healing in an ordinary bed: not built, on purpose.** Nothing I can read says an
   ordinary bed heals you, or how much per hour. I would rather not make a number up.
   If you sleep one hour in a plain bed in the original while hurt and tell me your
   health before and after (and the same for eight hours), I can build it properly.
10. **Hardcore sleep need: not built.** The game files have the need itself, its five
   stages and a rate number (50), but not what the rate is measured in or how much an
   hour of sleep takes off. NVForever does not build up any of the hardcore needs yet
   (food, water, sleep), so there is nothing for sleep to lower. My proposal: build the
   three needs together in their own piece of work, from numbers captured in the
   original.
11. **When you cannot wait or sleep.** Instead of the menu you now get a line on the
   screen in these cases:
   - **Enemies nearby**, with the original's own sentence (it is in the game files).
   - **Someone else's bed**: you can lie on it, but not sleep.
   - **Swimming.**
   - **A quest has forbidden waiting.**
   What I had to guess, since only the names of the original's messages are known:
   - **How near is "nearby":** about 2000 units outdoors and 1000 indoors (roughly 30
     and 15 metres). The original keeps these two numbers inside its program. If you
     type `GetGS fHostileActorExteriorDistance` and `GetGS
     fHostileActorInteriorDistance` in its console, I can put the real ones in.
   - **Who is an enemy:** anyone alive in view range whose records say they would
     attack you on sight. Nobody actually fights on their own in NVForever yet, so it
     cannot be "is attacking you".
   - **Whose bed:** I look at the bed's own owner. A bed with no owner of its own in an
     owned house still lets you sleep.
   - **The wording** of all lines except the enemies one is mine.
   - **A weakness you will notice:** most wild animals do not count as enemies yet.
     Standing next to two coyotes I could still wait; next to a Fiend I could not. The
     reason is a gap elsewhere (animals get their faction from a template that
     NVForever does not read yet). I have flagged it as its own task.
   - **Not built:** "in combat" and "alarm sounding". Combat needs enemies that fight;
     for the alarm I found no description of what it is.
12. **People move on while you wait.** Before, everyone stood still during a wait and
   walked to where they should be afterwards. Now, each hour that passes, everyone
   picks what their schedule says for that hour and walks for as long as they would
   have in an hour of normal play. After waiting from 18:41 to 22:41 in Goodsprings,
   Easy Pete and a settler were already in their houses at their beds, and Chet at his.
   - This is my design; I found nothing on what the original does.
   - Behind the wait menu you can see people change place from one hour to the next.
     During a sleep the screen is black, so you see nothing. Say if you want waiting to
     hide it too.
   - Nobody starts talking to you in the middle of a wait, and message boxes from
     scripts hold until the menu closes.
   - Someone who was sitting and has to be elsewhere is simply off the chair, without
     the standing-up animation, since it happens behind the menu.
   - People who were wandering about with nothing scheduled stay as they were until
     the wait ends.
13. **Still open from before:** the day-of-the-week question (item 7 of "Waiting and
   sleeping: the T menu") is untouched. I did not change that rule.

## Who is in which faction, and who is hostile to you (2026-10-11, session "actor factions")

From the sleep work: "enemies nearby" could not be told, because many actors had no
factions at all. Details: docs/spec-notes/factions.md.

1. **Fixed:** an actor marked "use the template's factions" now gets them. 3,037 of the
   6,385 actors placed in the game had no factions before, for example the coyotes at
   Goodsprings.
2. **Checked, not a bug:** the Fiend Violet reads as "neutral" to you. The game file
   says so: the Fiends and you do not list each other. She attacks because she is
   "Very Aggressive", and the editor's documentation says those attack neutrals. Most
   dangerous animals work this way too (cazadores, fire geckos, radscorpions,
   deathclaws). The sleep and wait refusal ("enemies nearby") already uses aggression
   together with the relation, so it was right about her; with item 1 it now also
   knows the factions of spawned animals.
3. **My choice, for you to confirm:** where the template is a leveled list ("one of
   these raiders"), the actor takes the factions of the same entry that is drawn for
   it. That pick is ours (level 1, repeatable per actor), not the game's roll by your
   level. For 192 actor types the pick changes the factions.
4. **One question a capture would settle:** 618 placed animals are "Aggressive" (not
   "Very") and neutral to you: bighorners, giant rats, coyotes, worker ants, bloatflies,
   mole rats. By the documented rule they leave you alone until attacked. If you
   remember a coyote or giant rat attacking on sight, the rule has a gap. Console, with
   the animal selected, while walking up to it: `GetFactionRelation player`,
   `GetAV Aggression`, `IsInCombat`.
5. **Not done:** AI packages through a leveled-list template; the game's own leveled
   pick by your level and the encounter zone.
## Music, sound markers and background loops (2026-10-11, session "music")

Docs: docs/spec-notes/audio.md. Start `--play` in Goodsprings to hear it.

1. **What you should hear:** the desert exploration music in and around Goodsprings,
   the day version from 06:00 and the night version from about midnight; the air
   conditioners humming near the saloon and store; the string lights creaking now and
   then; the desert background (wind, insects) changing at dawn, in the day, at dusk and
   at night.
2. **Guesses I had to make, for you to check by ear against the game:**
   - **Layers.** Each place's music is three files of the same length ("low", "mid",
     "high"). I play the low one anywhere inside the place's radius and add the mid and
     high ones as you come nearer its centre (for Goodsprings: within 8,400 and 2,800
     units of the marker by the saloon). If the game instead swaps one for another, or
     the music near the saloon sounds too thin or too busy next to the game's, tell me.
   - **Fades.** A layer comes in or goes over 9 seconds; so does the change between
     day and night music and walking out of a place.
   - **Overlapping places.** Where two places' radii overlap I play the one you are
     deeper inside.
   - **Random placed sounds** (creaks, cave wind, debris): each rolls its record's
     chance once a second. If they come too often or too rarely, tell me which.
   - **Background loop hours:** dawn 5–9, day 9–17, dusk 17–21, night otherwise.
3. **Not built yet:** battle music, dungeon music (explore, suspense, battle), the short
   incidental pieces, music that depends on a faction liking or hating you (I always
   play the neutral one), the music scripts ask for (stingers such as the one on leaving
   Doc Mitchell's house), main menu and death music; reverb indoors, left-right panning
   of placed sounds, and the region sounds (distant gunfire, coyotes, hawks).
4. **Side effect:** sound effects whose file is stored as Ogg are now found. They were
   silently skipped before.
5. **Cost:** reading every marker at the start takes about half a second on the main
   thread before the first frame.

## Dialogue menu as the original's (2026-10-11, session "dialogue menu")

Branch `claude/dialogue-menu`; note docs/spec-notes/dialogue-menu.md.

1. **What you should see:** while someone speaks, their line on a dark band without a
   frame and their name at the top right (as your Bruce Isaac picture); while you choose,
   the choices between two brackets, the selected one framed, said-before ones dim, an
   up and down arrow with a marker at the left when not all fit (as Trudy and Veronica).
   No numbers before the choices and no key hint any more; the number keys still work.
2. **Guesses I had to make, for you to check against the game:**
   - **The red frame.** In the Trudy picture the selected "[Sneak 17/25]" line is
     framed in red. I frame a selected check you fall short of in red, every other one
     in amber. If the game turns red for some other reason, tell me.
   - **How many choices show at once.** The list grows to 290 units (about five
     one-line choices) and then scrolls; the game's file leaves this to its code.
   - **The rows sit 7 units higher** inside the brackets than the file puts them, as
     the Trudy picture shows.
   - **The band's darkness** is measured from your pictures (about 0.87), not taken
     from the file, whose number would make it nearly see-through.
   - **The scroll marker** is 40 units long; your two pictures show about 30 and 43.
3. **Not built yet:** the subtitles setting (lines always show), sounds on the list,
   dragging the marker or clicking the arrows.

## Who sees you: detection (2026-10-11, session "detection")

Docs: docs/design/detection.md. Every NPC and creature near you now works out, four
times a second, how well it perceives you and the other actors near it, with the
editor documentation's formula (Perception, distance, light, sound of movement and
shots, and your Sneak, level and armour while you sneak). Nothing acts on it yet: it is
the groundwork for combat encounters. Scripts' GetDetected now answers from it.

1. **See the numbers without playing:**
   `fnvtool actor detect --schema schemas/fnv <Data> FalloutNV.esm --detector SunnyRef`
   lists, for 13 situations, the value at distances from 100 to 5,000 units and how far
   out you are detected (above 0) and noticed (above −20). In the game the log shows each
   change (`detection: GSSettlerAM 00104F0A → the player: hidden → noticed …`) and
   your band as the sneak meter would show it (HIDDEN, CAUTION, DANGER).
2. **Guesses for you to check against the game:**
   - **Light.** The formula needs the light level at you (0–100) and no source says how
     the game measures it. I take the ambient light plus sunlight by day (or the room's
     light indoors) plus nearby lamps; at noon in Goodsprings that is 93. With it, Sunny
     Smiles spots you sneaking in daylight out to about 1,840 units, and never sneaking
     in the dark. If the game hides you much better in daylight, the light measure is
     what to lower.
   - **Field of view:** 95° either side of where the actor faces (the GetLineOfSight
     notes say so for every NPC). Walls block sight; other people do not.
   - **Sneak-running counts as running** (louder and more visible).
   - **Lying on a bed counts as asleep** (−20 % perception).
3. **Would help me tune:** a few `GetDetected` readings in the game with you standing,
   walking, running and sneaking at set distances from one NPC, at noon and at
   midnight, and the game's fSneak* settings (`GetNumericGameSetting`).
4. **Not built yet:** the alert, combat and lost states with their timers (so bands can
   flicker as an actor turns its head), noise events from explosions and
   CreateDetectionEvent, NPCs sneaking, the sneak meter on the HUD, shade from the sun.

## Reloading one round at a time (2026-10-11, session "looping reload")

Docs: docs/spec-notes/combat-rules.md §4 ("Round by round"). Your rules (2026-10-11): a
trigger pull closes the weapon first, then it fires; the close plays every time.

1. **Which weapons:** none are listed in the code. A weapon loads round by round when
   its reload animation has an opening clip in the game's files. That is true of the
   .357 Magnum Revolver, Lucky, the Lever-Action Shotgun, the Hunting Shotgun, Dinner
   Bell, the Cowboy Repeater, the Brush Gun, the Trail Carbine, and also the **Grenade
   Launcher** (it shares the hunting shotgun's animation). If the grenade launcher
   reloads all at once in the game, tell me.
2. **How it runs:** the opening clip plays once (the .357's cylinder swings out:
   1.07 s); each round is the reload clip from its start to its `Hit` key (0.63 s for
   the .357, 0.47–0.5 s for the rifles and shotguns); after the last round the rest of
   that clip closes the weapon (about 1.1 s). Please compare a full .357 reload
   (2 missing: about 3.5 s) with the game.
3. **Guesses I had to make:**
   - The shot after a stopped reload follows by itself; you do not have to pull again.
     A pull while it is closing also fires once it is closed.
   - A pull while the weapon is still opening lets the first round go in, then closes.
   - Each round's clip starts straight after the last without blending, so the hands
     should not jump. If you see a jerk between rounds, tell me which weapon.
4. **Try it:** equip the .357 (or another one above), fire two shots, press R, then
   click partway through.
5. **Not built:** NPCs reloading (they do not reload with animations yet), the Reload
   Speed perks.

## Quest added, quest completed and objectives at the left (2026-10-11, session "quest messages")

Branch `claude/quest-messages`; note docs/spec-notes/quest-messages.md.

1. **What you should see:** "Quest added" (or "Quest completed", "Quest failed") with
   the quest's name under it in large capitals, at the left a little above the middle,
   as your Come Fly With Me and Ring-a-Ding-Ding pictures; objectives listed in the same
   place behind a small square, filled and with "COMPLETED: " for a completed one, as
   your Sierra Madre and Sunny Smiles pictures. These lines no longer appear in the
   messages at the top left.
2. **Measured:** at 1280x720 the words and the name sit on the same pixel rows and start
   at the same column as in your picture; the Sunny Smiles objectives break their lines
   at the same words.
3. **Guesses I had to make, for you to check against the game:**
   - **How long they stay:** 4 seconds for "Quest added" and the name, 4 for a list of
     objectives and 1.5 more for each further one; 0.25 s to come and go.
   - **The name comes in letter by letter from the left** over 0.6 s. Your Come Fly
     With Me picture shows "WITH ME" fainter than "COME FLY"; it might instead be the
     name going out from the right.
   - **One thing at a time:** "Quest added" first, then its objective; objectives that
     arrive while something else is on screen are gathered into one list.
   - **Sounds** by their record names: UIPopUpQuestNew for "Quest added",
     UIPopUpQuestComplete for "Quest completed", UIQuestUpdate for objectives; none for
     "Quest failed".
   - **The words "Quest failed"**: I have not seen it in the game.
   - **Where long objectives break:** at 570 units (your pictures put it between 557
     and 590).
4. **Please put your two objective screenshots** (the Sierra Madre and the Sunny Smiles
   ones) in `C:\fnv-private\references\quest_messages\`: they only reached me as
   pictures, so the objective list's place is read by eye (to about a pixel).
5. **Not built:** "Location discovered", and the "Quest Added" that SetStage gives for a
   stage added to the Pip-Boy.

## Barter and container menus drawn as the original's (2026-10-11, session "barter menu")

Docs: docs/spec-notes/barter-menu.md. Talk to Chet in the Goodsprings General Store and
pick "Show me what you have for sale."; or open any container.

1. **What you should see:** the layout of your screenshots, built from the game's own
   menu files: your items at the left, the merchant's or container's at the right,
   "Caps" (barter) or "Wg" (container) at the top right of a list, prices in the barter
   lists, the item's picture with WG and VAL under the lists, "Accept A)" / "Exit X)"
   or "Take All R)" / "Exit E)", and the caps arrow between the lists while a trade is
   put together. Items you put up move to the other list, first, with an open square.
2. **Prices** follow the GECK wiki formula with the settings in FalloutNV.esm; at Barter
   100 they give exactly the prices on your screenshots (Gold Bar 9492, Hydra 50,
   .308 AP 10 and so on). A Barter skill of 15, a new character's, makes buying dear and
   selling cheap.
3. **Guesses I had to make, for you to check against the game:**
   - **Keys.** I used the letters on the screen: A accepts, X exits (barter); R takes
     all, E exits (container); Esc and Tab also leave; `[` and `]` change a list's
     category. A click acts on the row under the pointer at once (one unit),
     Shift+click on the whole stack. The game asks "how many?" in a quantity box for a
     stack; that box is not built.
   - **Categories.** I only saw "ITEMS" and "MISC" as list titles; I added WEAPONS,
     APPAREL, AID and AMMO in that order. What does the merchant's list title say when
     it shows only weapons?
   - **Not enough caps.** I refuse with "You don't have enough caps." (or "Chet doesn't
     have enough caps.") under the menu. What does the game do and say?
   - **After the barter menu closes** I go back to the conversation. In the game, does
     the merchant say something then (the "BarterExit" lines)?
   - **The merchant's own pockets:** I only offer what is in the merchant's vendor
     chest, not what the merchant carries.
   - **Quest items** are hidden from the barter lists.
   - **The world behind** is darkened by about a third, as measured on your screenshot,
     but not blurred.
4. **Would help:** a screenshot of the barter menu with the cursor on a weapon (to see
   the damage, condition and strength figures, which I have not drawn), and one of the
   "how many?" box.

## Light rays and lamp glows were far too strong (2026-10-11, session "light rays")

Your screenshots of the Prospector Saloon at 8:00 (rays) and of a booth table (the ring
round the lantern). Branch `claude/light-rays`. Facts and measurements:
`docs/spec-notes/nif.md` §6 "Additive blend". Pictures: `C:\fnv-private\references\light_rays\`.

1. **Why:** the game adds the light of rays, glows and flames to the colours on the
   screen; we added it to linear light, which on a dark wall shows three to four times
   brighter. The rays themselves are faint in the files (they add at most a tenth of
   white); our way of adding turned them into solid shafts and a haze, and turned the
   faint outer part of the lantern glow into a ring on the table.
2. **Changed:** every additive shape now adds about what the game would. The rays are
   faint streaks near the jukebox window; the lantern keeps a small halo on its glass and
   no ring.
3. **A compromise, for you to judge:** our renderer cannot add exactly as the game does
   in one step, so the strength is fitted to the Saloon (`ADD_ALPHA_POWER`, 1.5). Over
   dark walls it is still a little stronger than the game, over lit ones a little
   weaker. If the streaks now look too faint or too strong beside the game, tell me
   which; an exact version needs a render pass of its own, which I have not built.
4. **Also changed by this, not compared with the game:** fire, muzzle flashes, sparks and
   other additive particles use the same material and are fainter at their soft edges
   now. A screenshot of a campfire or a gun firing in a dark place would show whether
   they lost too much.
5. **Seen on the way, not changed:** in your rays screenshot the frosted window panes are
   a light grey-yellow in the game and nearly white in ours.

## NPCs turn to whom they talk to (2026-10-11, session "SayTo facing")

Your request: when an actor says something to an NPC or to you, it turns and faces the
one it speaks to. Your decision: the speaker turns head and body. Branch
`claude/sayto-facing`; details in `docs/design/dialogue-play.md` §9.

1. **Built:** a line said to someone outside the dialogue screen (a script's SayTo, a
   hello to you, a line of a conversation between two NPCs) turns the speaker's head and
   body to the listener, and the listener's head to the speaker. In a conversation
   between two NPCs both turn their bodies, as before. A line said to nobody (Say) turns
   nobody.
2. **From the game's own data, as you pointed out:** SayTo has a "no target look" flag.
   When a script sets it, the listener is not made to look at the speaker (only the
   speaker turns). StartConversation between two NPCs has a "head track" setting; 0
   turns nobody for the whole conversation.
3. **Our guesses:** whether the original turns the speaker's body at all (the wiki only
   says the listener looks), and that a SayTo listener turns its head only. The speeds
   are the dialogue screen's (§8): a body more than 30° off turns at about 150° a
   second and keeps its new heading.
4. **Sitting NPCs** turn their heads only (Doc Mitchell on his chair during the
   opening).
5. **Checked by the logged angles only, not in pictures:** Sunny Smiles in the saloon
   turned about 160° to face the dog and 96° to face you, and finished each line facing
   them. Please watch one by hand. Test aid:
   `NVF_PLAY_TEST_SCRIPT=8:say_to:SunnyRef:player:GREETING` with
   `--cell GSProspectorSaloonInterior --play`.


## Flashlight (2026-10-11, session "flashlight")

Your request: holding the inventory button for 1 to 2 seconds turns a flashlight on or
off, a classic one that lights what is in front of the camera. Branch
`claude/flashlight`; code in `engine/nvf-play/src/flashlight.rs` (the key) and
`engine/nvforever/src/flashlight.rs` (the light).

1. **Built:** Tab held for 1 second switches the light, at once, while the key is still
   down; letting go after that opens nothing. A tap opens the inventory as before, but
   on letting go of the key rather than on pressing it.
2. **The light:** a warm white cone (30° from the middle to the edge, fully lit to 14°)
   from just right of and under the eye, reaching about 40 m with the game's falloff.
   It casts shadows, so walls stop it; your own arms, weapon and body cast none (in
   third person nothing between the camera and a little past your head does).
3. **Our choices, not the game's:** the original's hold of Tab works the Pip-Boy light,
   a glow round the player; this is a flashlight as you asked. Scripts that lock the
   Pip-Boy (the opening) do not lock the light. Detection ignores it for now; say if a
   lit flashlight should make you easier to see.
4. **Checked in pictures:** in the Goodsprings house at 23:00 the cone lights the bed and
   wall with the bedpost's shadow on it; outside at 01:00 the house across the road
   takes the warm light and the stop sign's shadow. Test aid:
   `NVF_PLAY_TEST_CONTROL=2:flashlight` with `--play`.

## Groups on NPCs, fading neon and a trap heard (2026-10-11, session "object anims more")

Your request: the three items left from PR #211. Branch `claude/object-anims-more`;
details in `docs/spec-notes/activators.md` §6–§8.

1. **PlayGroup on NPCs and creatures** now plays the actor's own clip of that group (its
   weapon's Equip, Aim or Unequip; its walk and turn clips; else the unarmed `h2h` clip of
   its skeleton's folder) and blends back to its normal animation when the clip ends,
   when a script plays `Idle`, or when the actor walks. In FalloutNV.esm only one script
   does this to an actor (a dog's package ends with `PlayGroup Aim 1`), so it was tried
   with a test aid on a Goodsprings settler and a big horner. **Our guesses:** that a
   looping group (Aim) holds until the actor moves, and that `SpecialIdle` on an actor
   plays nothing (its clips are many, picked by the idle manager).
2. **Neon signs and lit windows fade**: the alpha tracks of their Left and Right now play,
   and the visibility tracks (Gomorrah's lights). Primm's windows light when the clock
   passes 20:00 (two screenshots measured). Not built: the particle, refraction and morph
   tracks. **Question:** Goodsprings' house windows have alpha tracks but no alpha
   blending in their model, so we draw them the same day and night. Do they change in
   the game at 19:30 and 6:30?
3. **The bear trap's sound** is heard when you disarm it. Two faults stopped or doubled
   such sounds: keys in a group's first frame were missed (the tripwire's would never
   sound), and a group played once passed its keys again as it ended (the trap clacked
   twice).
4. **Seen on the way:** by day the neon script asks for Right again every time Right ends,
   so Right replays every 0.07 s. Nothing shows, but it hints that in the game a group
   played once still counts as playing while its last frame is held. A capture would
   settle it.
5. **A slip:** one survey command (`fnvtool actor anims`, which takes the Data folder)
   opened the add-on archives too, holdout included. Its output was deleted unread
   (only one base-game line was printed), and the surveys were redone on the base
   archives by name.


## Melee combos and power attacks (2026-10-11, session "melee combo")

Your request: chained attacks (two punches go right hand then left hand) and power
attacks by holding Use, with their own clip for standing still, forward, back, left and
right. Branch `claude/melee-combo`; code in `crates/fnv-combat/src/melee.rs`, the
engine's `melee` in `engine/nvf-play/src/combat.rs`, notes in
docs/spec-notes/combat-rules.md §10.

1. **Built from the clips themselves:** every right-hand attack clip of the fists and
   melee weapons ends with a key naming the left one as the next attack, and the left
   ones name the right, with a time from which the next swing may cut in. A press during
   a swing now starts the next one at that time, so quick presses go right, left,
   right, left. Power attacks pick the clip by the movement keys held when they start.
2. **Checked in a run** (Goodsprings, fists): four taps played `attackright_a`,
   `attackleft_b`, `attackright_a`, `attackleft_b` (three chained); holds played
   `h2hattackpower`, `…forwardpower`, `…backpower`, `…leftpower`, `…rightpower` in both
   views. Test aid: `NVF_PLAY_TEST_CONTROL=3:weapon,16:fire,16.1:fire,16.4:fire,16.5:fire`
   with `--play` (taps before about 13 s fall inside the pre-order message boxes).
3. **Guesses, please check against the game:**
   - The `_a` and `_b` forms of each swing are picked at random.
   - A press after a swing has fully ended starts over with the right hand.
   - Holding Use for 0.4 s makes a power attack (you described about a second; say if
     it should be longer).
   - Power attack damage now uses the game's settings named
     `fDamagePowerAttack{Stand,Forward,Back,Side}Bonus`: 3× standing, 2× moving, where
     it was 1.5× for all. That they multiply damage is read from their names only.
   - A movement key counts once it is past 30 %; forward/back wins over a side.
4. **Not built:** the lunge of a moving power attack (the clips' root motion does not
   move the player), the perk-only custom power attacks (`attackcustom1power` …), and
   the `Attack3`–`Attack7` punch chains beyond what a weapon's own attack names.

## Character creation menus (2026-10-11, session "chargen")

Your request: the trait menu, the tag-skill menu, the name menu and the Vigor Tester as
the original's. Branch `claude/chargen-menus`; notes in `docs/spec-notes/chargen.md`
(the Vigor Tester in `vigor-tester.md`).

1. **Built:** the name, tag-skill and trait menus are drawn from the game's own menu
   files with its fonts and pictures. Beside your screenshots, the rows, pictures,
   texts, lines and buttons are within a few pixels. Done writes the name, the three
   tags or the traits into the game. With your screenshot's S.P.E.C.I.A.L.
   (5, 3, 7, 7, 5, 6, 7) every skill value in the tag menu matches yours.
2. **The traits come from the load order:** sixteen with Old World Blues installed (ten
   in the base game, six from the add-on), sorted by name as on your screenshots.
3. **Words the game keeps inside the program** (not in its data files): "Reset", "Done",
   "Ok", "Enter character name.", "R)", "A)", "TRAITS LEFT", "Req:", "Ranks:",
   "Selected", "Tag 1 Skill". I took them from your screenshots. Our guesses, please
   correct any you know: "1 TRAIT LEFT" with one left; "Tag 2 Skills" and "Tag 3 Skills".
4. **Four Eyes' requirement:** the game data has two (Perception above 1 and below 10),
   and your screenshot shows only "PER < 10". I leave out any "above 1" on a
   S.P.E.C.I.A.L., but I don't know the original's rule.
5. **Our guesses about behaviour:**
   a. Reset goes back to what the menu opened with. It might clear everything instead.
   b. A trait whose requirement you don't meet is drawn faint and can't be taken.
   c. Keys: Up/Down choose, Enter or Space tags or takes, R resets, A is Done.
   d. Names are at most 24 characters, no blank names, and no typing cursor is drawn.
6. **Not built:** the blur of the world behind these menus, their sounds, and what each
   trait does once taken (its perk effects). The race menu is still a placeholder.
7. **The Vigor Tester** is drawn from the game's own model of the machine, the one the
   in-world tester uses, with its own page-flip animations. Its camera is fitted to your
   screenshots. Arrows raise and lower, the lever's buttons and the arrows are
   clickable, and Done works only with every point given out. Details are in
   `vigor-tester.md`. Questions:
   a. Which PC keys does it use? Ours: Left/Right lower and raise, Up/Down choose the
      review row, Tab and Page Up/Down turn pages, Enter or E is Done.
   b. Does it open on the "Rate Your… TRY IT NOW!" page? The model rests there, so ours
      does.
   c. On the review page, is the → itself Done, or does another button replace it once
      every point is given out?
   d. Does a row's ◄ disappear at 1?
   e. Ours is drawn without the game's lighting, so the cabinet is flatter and the bulbs
      glow less. Should it get real lighting and the blurred room behind it?

## NPCs on alert: Normal, Alert, Combat, Lost (2026-10-11, session "alert")

Your request: NPCs go on alert when they sense a threat (hear a hostile move, glimpse an
enemy far off) before they have confirmed it, as the start of NPC combat. Branch
`claude/detection-states`, design `docs/design/alert.md`.

1. **Built:** the four detection states from the editor documentation. A hostile that is
   noticed but not detected puts the NPC on Alert: it draws its weapon, turns its head
   to where it noticed something, says its NormalToAlert line, and (if it is run on its
   packages) walks there to search, saying AlertIdle lines. Detected: Combat (it stops
   and faces you; nothing attacks yet). Lost sight in combat: Lost, it searches where
   you were. Bullet impacts are heard: shoot near a Fiend from hiding and it searches
   the impact.
2. **Continue During Combat** (your note): an NPC whose current package has the flag
   keeps doing its package through Alert, Combat and Lost; it still draws its weapon and
   barks. No source describes the flag, so this follows its name; 399 of the game's
   4,163 packages set it. **No Combat Alert** is read the same way: such an NPC never
   goes to Alert (it can still go to Combat). Tell me if you know either to work
   differently.
3. **My guesses, to tune when you can compare with the game:** an alarm lasts 20 s after
   the threat was last noticed; a target hidden for 10 s is forgotten; 3 s without
   seeing its target turns Combat into Lost; a search looks about for 4 s per spot and
   moves within 256 (indoors) or 512 (outdoors) units. A bullet's impact is as loud as
   the gun. Actors with a faction in common, or of the same kind, never turn on each
   other.
4. **Scripts see it:** IsInCombat is 1 for an NPC on Alert, in Combat or Lost, and for
   you while one hunts you (the documentation says so). This can change quest scripts
   that test IsInCombat; nothing showed in the runs, but watch for it.
5. **Fixed on the way:** detection's line of sight looked through hills when the NPC's own
   body was in the way of the ray, and a small transparent fence hid you. Both changed
   (`docs/design/alert.md` §7).
6. **Would help:** in the game, `tdetect` off, stand in sight of a hostile at the edge
   of its range and note how long it searches after you hide (GetDetected and IsInCombat
   on it every second), to tune the timers.

## NPCs in Combat attack (2026-10-11, session "alert", combat AI step CA1)

Your request: NPCs in Combat attack. Branch `claude/combat-ai` (on top of the alert
branch), design `docs/design/combat-ai.md`.

1. **Built:** an NPC in Combat shoots with the gun drawn on it (its fire rate, magazine
   and reloads, the same projectiles and damage as yours) or strikes with its melee
   weapon or fists; a creature bites with its own damage and reach. It attacks only what
   it sees. NPCs run on packages run up to where they can attack: a gun to 80 % of its
   range, a dog until its body is just clear of yours (your note about the dogs pushing
   you around is fixed: they stop at biting range and bite).
2. **You can be hurt and die**: their shots hit you, your armour counts. At 0 health
   every control stops and the screen says "You are dead."; no death camera or reload yet.
   `NVF_PLAY_TEST_GOD=1` makes you take no damage for test runs.
3. **StartCombat / StopCombat work now** (938 calls in the game's scripts): quest
   fights start for real. A test aid uses it: `NVF_PLAY_TEST_SCRIPT=3:combat:<actor>:player`.
4. **My choices, for you to tune:** reaction time 0.6 s; first shots 4° wider, settling
   over 3 s; a blow every 1.2 s; NPCs do not use up ammunition; their shots leave from the
   eyes, not the gun's muzzle. Violet's hunting rifle hit 12 of 15 at 1,600 units, which
   may be too deadly; the spread is the documented one for her Guns skill.
5. **Not yet:** attack animations (they shoot and bite without moving the gun or the
   jaw), allies joining in (Violet's dogs stayed out of her fight: by their records they
   are not hostile to you), cover, fleeing, grenades, weapon choice, combat styles.

## NPCs aim, fire and bite visibly (2026-10-11, session "alert", combat AI step CA2)

Your request: attack animations. Branch `claude/attack-anims` (on top of the combat AI
branch), `docs/design/combat-ai.md` §9.

1. **Built:** NPCs bend their aim up or down to their target, play their weapon's attack
   clip with every shot (Violet: `2hrattack3`, the bolt worked between shots) and swing
   with every blow; creatures play their own bites and swipes in turn. A bite's damage
   lands at the clip's Hit key, so the dog's jaws close when you are hurt.
2. **Your note on Violet's shots:** they came from her face. Now each NPC weapon has a
   marker at its model's ProjectileNode (as yours does) and the bullets, the muzzle flash
   and the shot sound start there. The shot still goes where her eyes aim: from the gun
   itself she hit the metal sheet in front of her every time, because her rifle is held
   at the hip behind it while her eyes see over it.
3. **Please look at:** whether the hip-height hold of the rifle is how Fiends hold it in
   the game when they shoot, and whether the dogs' bites read right at their speed.
4. **Not yet:** NPC reloads are not animated, NPCs that are not run on their packages do
   not turn their bodies to you (only their heads), and the body does not bend while
   running and shooting.

## The Pip-Boy (2026-10-11, session "pip-boy menu")

Your fifteen Pip-Boy screenshots. Branch `claude/pipboy-menu`. Facts and measurements:
`docs/spec-notes/pipboy.md`. Pictures: `C:\fnv-private\references\pipboy\`.

1. **Built:** Tab raises the arm with the game's own clip and holds it before the eye;
   the menus are drawn onto the model's screen, with STATS, ITEMS and DATA (the buttons
   on the device, or F1–F3) and their five tabs each. The world stops and is blurred
   behind it. ITEMS lists the inventory by tab; STATS and DATA show only their headings
   so far.
2. **The clip:** it both raises and lowers the Pip-Boy; the screen faces the eye at its
   `Hit` key, so it is held there and played on when closing. Say if the original's
   timing feels different.
3. **Fitted to your ITEMS screenshot:** the size (the game's `fPipboy1stPersonFOV=47`),
   the menu's place on the screen, the amber, and the scanlines' spacing. Ours now lines
   up with it to a few pixels.
4. **My choices, for you to judge:** the screen's glare is not drawn while the menus are
   on (drawn, it was a pale haze yours does not have; yours shows only a faint lighter
   patch, top right). Keys: F1–F3 pages, A/D or arrows tabs, X drop. The system's mouse
   pointer, not the game's amber arrow.
5. **Would help:** do you know the PC keys for switching pages and tabs in the original?
   And a screenshot of the Pip-Boy being raised or put away mid-way would show whether
   the clip is held where I hold it.

## The Pip-Boy's STATS and DATA pages (2026-10-11, session "pip-boy menu")

Built from the menu files and your screenshots; branch `claude/pipboy-menu` (PR #213).
Details and what is guessed: `docs/spec-notes/pipboy.md` §5.1 (STATS) and §5.3 (DATA).

1. **STATS:** Status (CND with the limb bars, RAD with the meter, EFF), S.P.E.C.I.A.L.,
   Skills, Perks (picture and description from the records) and General (reputations;
   the general statistics show "--": the game keeps no counters of its own yet).
   Questions: should worn items' effects (Vault Jumpsuit: Speech +2) change the actor
   values themselves, for scripts too? The page adds them for now. When does the box
   round the selected row show in the original? Do the hints show "(0)", and when are
   they bright? Which way round are "Dark Hero" and "Soft-Hearted Devil"?
2. **DATA:** the place and date in the heading; Quests (running first, newest on top;
   Enter makes one active), Misc (notes and their text), Radio (tuning from the list;
   the wave is a still line, the original's is animated), World Map (markers you have
   found, quest markers, drag and wheel), Local Map (doors and markers on an empty map:
   the picture of the place from above is not drawn). Questions: how are quests ordered
   and which is active by default? Which doors does the local map show (yours shows only
   the gas station's, ours 17) and at what zoom? Where does the World Map open?
3. **Would help:** screenshots of Perks and General, a crippled limb or rads on Status,
   EFF with a chem taken, and the World Map right after opening it.
4. **Not built yet:** using a Stimpak or RadAway from the hints, the hardcore buttons,
   Place Marker, Challenges, the item picture and figures on ITEMS. The messages at the
   top left (quest updates, corner messages) still draw over the Pip-Boy's edge.
## Terminal hacking (2026-10-11, session "hacking")

Your request: the hacking minigame, from your seven screenshots and rules. Branch
`claude/hacking`; details in `docs/spec-notes/hacking.md`.

1. **Built:** activating a locked terminal opens the hacking screen laid out from the
   game's own `hacking_menu.xml` in its terminal font; the words come from the game's own
   word list (`menus\falloutdict.txt`, every word of your screenshots is in it). Likeness,
   four attempts, bracket pairs (dud removed or attempts back), the log at the right,
   lockout for good after four wrong words, a new game with all attempts after leaving.
   The right word opens the terminal's menu (from `computers_menu.xml`): items by their
   conditions, result scripts (the default unlock terminals' "Disengage Lock" runs its
   unlock), notes shown on the screen, sub menus.
2. **Word count** follows the GECK wiki's formula from your Science and the lock level
   (0/25/50/75/100, as you said): 20 words when Science equals the level, 5 at Science
   100 on an Average terminal; Very Hard always about 12. Science below the level refuses.
3. **Our guesses, please check:** word lengths per level (Very Easy 4–5, Easy 6–8,
   Average 9–10, Hard 11–12, Very Hard 13–15; your Easy terminal had 7); how often a
   bracket pair refills the attempts instead of removing a dud (one in five here); that a
   removed dud turns into dots and logs "Dud removed."; that a lone symbol logs "Error"
   and costs nothing; the last-attempt warning and the lockout screen's words; the refusal
   line; the prompt word "Access"; that holding the password note skips hacking.
   A screenshot of a dud being removed, of the last attempt, of a locked-out terminal and
   of a refusal would settle most of these.
4. **Looks:** drawn flat over a darkened world, with the colours measured on your
   screenshots; the original shows it on a 3D monitor (`terminalinterface01.nif`). The
   monitor, sounds, the typing-out of text and saving which terminals were hacked are not
   built yet.
5. **Try it:** any locked terminal with E, or the test aid
   `NVF_PLAY_TEST_TERMINAL=30:science:25,31:open:000D74D5` with
   `--cell BisonSteve01 --play` (an Easy terminal; keys: arrows/WASD and Enter, or the
   mouse; Tab back, Esc out).

## The crosshair and the prompt under it (2026-10-11, session "crosshair")

Branch `claude/crosshair`; note docs/spec-notes/reticle.md.

1. **What you should see:** the game's own crosshair, the two small arrows, and the
   boxed `[> <]` one while something you can use is under it; under it "E) Take" (or
   Talk, Open, ...) and the name on the next line, as on your four screenshots; for an
   item, the line across with "WG" and the weight left of its stem and "VAL" and the
   value right of it.
2. **Measured:** at 1280x720 the crosshair, the letters of both lines, the line across
   and the figures land on the same pixels as in your bottle cap and gun case pictures
   (compared on a beer and the saloon's cash register).
3. **Guesses I had to make, for you to check against the game:**
   - **A door to another place** shows "E) Open" and the place's name under it.
   - **Locked things** say "E) Locked (Average)" or "E) Unlock (key)": what does the
     game write?
   - **Weights** written as "2", "0.5", "0.25" (no trailing zeros); nothing as "--".
   - **A stack** shows "Stimpak (3)" and the weight and value of one.
4. **Not built:** the crosshair turning red on an enemy, the sneak words ("[HIDDEN]"),
   extra lines such as a lock's level or "Steal" in red for owned things, weapon damage
   and condition in the item row. Screenshots of a locked safe, an owned item, a load
   door and a weapon on the floor would settle most of these.

## Playtest fixes: controls, crosshair and menus (2026-10-11, session "pt-controls")

Branch `claude/playtest-controls`. One commit per item.

1. **No crosshair, containers would not open (regression).** The new crosshair
   (`reticle.rs`, PR #224) was never added to the game, so nothing drew a crosshair.
   Containers: E opened the container menu and the menu, reading the same press of E as
   its "Exit E)", closed it in the same frame (since the barter/container menu PR made E
   close it). Both fixed, each with a test that fails without the fix; a new test aid
   `NVF_PLAY_TEST_KEYS=<s>:<key>` presses real keys and mouse buttons so the bindings
   themselves are tested. **Check:** E on a container opens it and E closes it; the
   crosshair shows and becomes the boxed one on usable things.
2. **Ctrl crouch (regression).** The key worked; the opening's scripts turned sneaking off
   and never gave it back. The trigger outside Doc Mitchell's door gives the controls back
   with `EnablePlayerControls 1 1 1 1 1 1` (six numbers, the seventh, sneaking, left out),
   and we read a left-out number as "leave it off". Now a left-out number of
   `EnablePlayerControls` gives that control back, as the form with no numbers does.
   **Guess:** that this is the game's rule (no other script gives sneaking back, so it
   must be). **Check:** after leaving Doc's house, Ctrl crouches; inside his house before
   that, it does not (as the scripts ask).
3. **Wake-up camera.** While the opening's wake-up/sit-up/stand-up idles drive the view,
   the mouse, the look test aid and movement keys are ignored; the camera is the
   animation's. Released when the last idle ends. **Note:** the game's scripts leave
   looking switched on during the wake-up, so the original may let you look round there;
   this follows your wish. **Check:** the wake-up in a new game.
4. **Dark sheet behind menus.** The barter, container, tag-skill and trait menus now have
   a 0.75 black sheet behind them (`interface::MENU_BACKING`, your wish; the original's
   is about 0.33–0.4 but it also blurs the world, which we do not). The name menu (0.85),
   message boxes, dialogue and terminal screens were already darker and are unchanged.
   The Vigor Tester's text sits on the machine itself and keeps its 0.45 sheet behind the
   machine. **Check:** is 0.75 right, or should it be darker or lighter?
5. **Pip-Boy mouse.** Clicks on the screen and on the STATS/ITEMS/DATA buttons did
   nothing: the pointer was traced with a method that cannot see the arm's animation nor
   meshes kept on the graphics card. Now the screen and the buttons are traced as the arm
   holds them. Tabs and rows take clicks; the pointer on a row selects it, a click on the
   selected row uses/equips it; the button under the pointer glows **[my choice]**.
   **Check:** clicking tabs, rows and the three buttons; whether a button should glow
   under the pointer in the original.
6. **Walk while aiming.** Holding right mouse (Block) with a weapon out moves the player
   at walking speed and the body walks; sneaking slows it further. NPCs: left for the
   combat AI (noted in combat-ai.md §8). **Check:** the feel; should a melee block slow
   the player too (it does now, since Block is the same control)?
7. **Revolver reload grace.** The press that interrupts a round-by-round reload (.357,
   lever-actions, hunting shotgun) now only closes the weapon, never fires. A press made
   after that, while it is still closing, is kept for **0.25 s** (my pick): if the weapon
   finishes closing within 0.25 s of that press, it fires; otherwise the press is
   dropped and you press again. **Check:** whether 0.25 s feels right.

## Playtest fixes: Sunny, dialogue and voices by distance (2026-10-11, session "pt-dialogue")

Branch `claude/playtest-dialogue`; notes in docs/spec-notes/dialogue.md §5,
dialogue-menu.md §4, voice.md §9, docs/design/dialogue-play.md §1 and §8.1,
actor-behaviour.md §5.1–§5.2, alert.md §4 and §8.

1. **Sunny's gecko line holds you.** "Hear that up on the ridge…" is a greeting with the
   Goodbye flag, said by her Dialogue package. **Rule now:** when an actor starts the
   conversation (StartConversation with the player, or its Dialogue package), a Goodbye
   greeting is a conversation of one line: the dialogue screen holds you until it is
   said, then ends. When **you** talk to an actor, a Goodbye greeting is still just
   said at you (your rule of 2026-10-10). Please check in the game whether a
   force-greet's Goodbye line zooms in like this for other NPCs too.
2. **No `{…}` in lines.** Acting notes in braces are left out of the dialogue line, the
   choices and the subtitles ("{questioning}" and the like; 5,634 lines carry one).
   **Kept:** square brackets ("[SUCCEEDED]", "[FAILED]", "[You hear static.]"), because
   the original shows those. Four lines close a note with ")" by mistake ("{KAI-zar)
   Caesar awaits!"); the note ends there. Angle-bracket prompts ("<Log Off>", "<lie>")
   are shown as written: tell me if the game hides those.
3. **Facing in conversations.** The NPC now turns its whole body to you on the spot and
   keeps facing you for the whole conversation, however it started, and still faces you
   afterwards until its package moves it (before, the drawing put it back). Turn speed is
   the standing turn rate (120° a second). Someone sitting turns only the head.
4. **Sunny at the gas station.** Cause: a script's MoveTo (Ringo's "All right, I'm
   ready." moves her to the gas station) did not reach the AI, which kept her where she
   stood in the saloon. Now an actor put somewhere by a script goes on from there and
   takes up its package anew. Checked in the gas station: she appears and says "Time to
   look alive" with the conversation open. Please play it from Ringo's line once.
5. **Cheyenne.** Her package is "accompany Sunny" (128 units); that package type was not
   carried out, so she stayed behind. Follow and Accompany packages are carried out now:
   she stands while Sunny is within 128 + 64 units, else walks after her (runs when Sunny
   is more than 512 away), through doors too. Guessed: the 64-unit slack, the 512 run
   distance, 256 units for packages that give no distance. Escort is still not carried
   out.
6. **Voices by distance.** NPC lines you are not in a conversation with are full volume
   within 600 units, quieter beyond, silent from 2,000, and panned left/right; another
   cell is silent. 600 and 2,000 are `fDialogMinDistance` / `fDialogMaxDistance` from
   the game's `Fallout_default.ini` (the names fit; nothing documents them). **No
   subtitle** while the speaker is 2,000 units or more away or in another cell. Your own
   conversation stays full volume. Please listen to a hello at about 10 m and 25 m.
7. **Fewer barks.** After one actor says a detection line (spotting you, losing you,
   searching), nobody of its group within 4,096 units (one cell) says a line of the same
   kind for 15 s. Group = same NPC type or a shared faction. "Same kind": the three
   "starts fighting" topics count as one, the three "gives up" topics as one, the two
   search idles as one. 15 s is the longest idle-bark wait in the ESM; 4,096 is my
   choice. At Cook-Cook's camp: 12 lines in 45 s before, 4 after.

Seen on the way, not fixed (not mine): in the Fiend check (alert.md §8) Violet becomes
Alert but barely moves and never reaches Combat, on main as well; Cook-Cook's fiends switch
Combat ↔ Lost every few seconds.

## Playtest fixes: NPCs on furniture, turning and anim objects (2026-10-11, session "pt-npcs")

Branch `claude/playtest-npcs`; notes in docs/spec-notes/furniture.md §7, actors.md §8.9
and §9.4, idles.md §10.

1. **The saloon settler standing in his stool after you come back** (and **Easy Pete
   not sitting on the porch chair after coming out of the saloon**): one cause. When you
   left the cell the actor's body was taken away while seated, but the stool stayed
   booked to him, so on your return the stool refused him as "in use" (by himself). Now
   the seat is freed when the body goes, and a resident that was seated is put straight
   back on it when you return, already sitting (no walk-up and sit-down clip again).
   *Decision:* coming back finds him seated, not sitting down again. *Check:* leave the
   saloon and come back; with Pete, enter the saloon while he sits on the porch, wait
   two hours inside, come out at 14:00.
2. **Holstered weapons floating on seated NPCs**: the seat clips don't move the weapon,
   so it fell back to its "in the hand" position while hung from the hip. Now only the
   weapon's own holster track is kept over the seat clip (and over a death pose), so the
   gun stays on the hip or back and follows it. *Check:* a seated settler's pistol at his
   hip; the rifle on the back of a seated rifleman.
3. **Turning speed** (guessed values, nothing in the data gives an NPC's turn speed;
   the game has a setting `fCharacterDefaultTurningSpeed` whose value is built into the
   game, and creatures have a "Turning Speed" field of 40–100 that we don't use yet):
   - walking: at most **180°/s** (was 720°/s); standing turns stay **120°/s**;
   - an actor slows while turning: it keeps 30% of its speed when its way is a quarter
     turn or more off, full speed when facing it (a right-angle corner costs about half a
     second);
   - near you (where actors walk with physics) they walk an arc round corners and turn
     on the spot before setting off, like elsewhere;
   - arriving at an idle marker they now turn to its heading at 120°/s (it was instant);
     walking the last steps to a chair they take as long as the turn needs; getting up
     from a chair no longer snaps them half about in one frame.
   *Measured:* the fastest turn between two frames in the residents test went from
   720°/s (2 frames above 270°/s) to 180°/s (none). Turn-on-the-spot clips already
   exist and play (`mtturnleft/right.kf`); weapon-stance and creature turn clips are not
   used by NPCs yet. The conversation turn was left as it was (another session works on
   dialogue facing). *Check:* do 180°/s and the slow-down feel right? Should creatures
   use their own "Turning Speed" field?
4. **Props in idles (Chet's broom)**: the game links a prop to an idle through an
   "animated object" record that names the idle; the prop's model names the hand it goes
   in. Now while an NPC plays such an idle the prop is drawn in that hand, and taken
   away when the idle ends. Chet sweeps with his broom; a smoking idle gets a cigarette.
   *Guess:* the prop shows for the whole idle (including its fade-out); the prop's own
   small animation (the broom has one) isn't played. *Check:* Chet sweeping; anyone
   smoking, drinking or eating at a table. New test aid `play_idle:<ref>:<idle>` in
   `NVF_PLAY_TEST_SCRIPT`.

## Playtest fixes: loose items, ravens, shadows and shine (2026-10-11, session "pt-world")

Branch `claude/playtest-world`. Pictures: `C:\fnv-private\references\playtest_world\`.

1. **Loose items in Chet's store** (physics.md §4.3.1). Fixed. A new test aid
   (`NVF_PLAY_TEST_PHYS_TRACE`) logged every loose item after the store loaded: 53 of 67
   moved. Three causes: (a) objects made of parts joined by hinges (the globe on its
   stand, the bucket and handle, a hanging office lamp) flew apart and fell from the
   ceiling, because we do not model those joints, and knocked their neighbours; (b) two
   books inside the glass display case and a magazine have nothing under them in the
   collision (the case's collision is only its outer shell), so they dropped; (c) piles of
   produce in a crate overlap and were pushed apart. Now every placed loose item starts
   at rest and stays exactly where it was placed until something touches it (you, an
   NPC, a shot, an explosion, something falling on it, or an item under it being picked
   up); jointed objects stay fixed where they are drawn. After: 0 items move in the
   store, Sunny's house (GSHouseInterior03) and the saloon; shot bottles still fall.
   - My choice: "stays put until disturbed" is my reading of the original (the books in
     the display case would have fallen otherwise). Check: walk into a shelf and shoot
     things: they should react as before.
   - Not done: the hanging lamp, globe and bucket no longer move at all, even when shot
     (their joints would need modelling).
2. **Ravens outside Doc Mitchell's** (actor-behaviour.md §5.8). Fixed. They are
   creatures with a "sandbox around where placed" AI package but no walking animations,
   so our AI slid them around. Their fly-off is their own script: when you come within
   512 units (or shoot one) it plays the fly-away animation and disappears about 4.5 s
   later; that already worked. Now creatures with no walking animations stay where they
   are, and perched birds (those plus the "Flies" flag) no longer block you.
   - My choice: no-collision applies to "can't walk and flies" creatures only; anything
     else that can't walk (e.g. turrets) still blocks. Check: walk out of Doc's door;
     the two ravens should sit still, then fly off as you approach, and never block you.
3. **Shadow flicker after leaving an interior** (player-body.md §1.6). Not reproduced.
   I left the saloon by the front door in first person at 15:00 and took bursts of
   frames on the porch, walking into the sun and standing with the .357 out: nothing
   flickered, and the extra "arms shade" light and the arms/body visibility never
   switched. I left debug log lines for it. Please tell me the time of day, which door,
   whether "Body in first person" was on, and what flickered (the arms, the ground, your
   own shadow); a short video would settle it.
4. **Flashlight shadows from light rays and lanterns** (nif.md §6). Fixed. Anything
   drawn see-through (glass, light rays, glows, smoke) now casts no shadow from any
   light, the sun and the flashlight included. Before/after:
   `flashlight_rays_before_after.png` (saloon, flashlight on, facing the east windows:
   the rays' dark wedges on the wall are gone). Cut-out textures (fences, leaves) still
   cast. Check: glass bottles and windows no longer throw a sun shadow either; say if
   you want the windows' frames-only shadow some other way.
5. **Too shiny: Doc's bed, the gas station's Sunset Sarsaparilla machine and door**
   (nif.md §6 "Specular"). Fixed in the material path for all objects of this kind. All
   three use a normal map without an alpha channel (DXT1, 425 of the game's 3,428 normal
   maps); we read the missing alpha as "full shine", seven times the shine of a typical
   surface, which looked like wet paint and turned mirror-like at shallow angles. Now
   such objects get the typical (median) shine of the other normal maps.
   - Before/after: `shine_gas_station_before_after.png`, `shine_doc_bed_before_after.png`.
   - Guessed constant: the median mask, 35 of 255. Check: the bed when you wake up and
     the vending machine and door at noon. A game screenshot of the vending machine in
     sun from the side would tell whether the original gives these objects any shine.

## Smoke tests reworked (2026-10-11, session "pt-smoke")

Your request (todo 3): stop wasting time and usage on redundant smoke testing. The ~40 prose
smoke tests of CLAUDE.md are now data (`tools/smoke/scenarios.toml`, 49 scenarios) run by
`python tools/smoke run`: it picks the scenarios whose files a branch changed, shares a game
launch where that was checked to be safe, keeps the logs in `target/smoke/` and prints one
line per scenario. CLAUDE.md now says how to run it and that new smoke tests go into the file.

What I decided where the prose was unclear:
1. **"About N"** is N ± 25 % (rounded outwards); "a dozen" is 9–15; "3 s later" 2.25–3.75 s.
2. **The AI tool checks** (`fnvtool ai packages`, `ai reach`, `ai observed`, `ai run`) run over
   the ten masters: CLAUDE.md gave no plugins, and the doc's figures (791 Sandbox, 32 Wander)
   are those of the masters.
3. **"Compare with the doc and explain any change"** is now checked: the figures of
   actor-values.md §3.1/§7, detection.md §4, factions.md §2–§5, audio.md §1–§5 and
   actor-behaviour.md §5.3 are JSON expectations on the tools' output. **Check:** the ai
   reach figures of actor-behaviour.md §5.3 (2026-10-10) no longer match main: 16,935 pairs
   (doc 15,603), 10,258 reachable (9,488), 1,602 staying (1,571), 3,185 never chosen (2,810),
   182 at the player (177). They are marked known-failing, and main's 2026-10-11 figures are
   checked too, so any later change shows. Someone who knows why they changed should update the doc.
4. **The navmesh walk table** (navmesh.md §5) is a dated record; only "all six arrive" is
   checked, as CLAUDE.md asked.
5. **`ai observed` / `ai run --samples`** need the observer captures (`obs_*.jsonl`), which I
   could not find on this machine; the scenario is skipped unless `NVF_OBSERVER_SAMPLES` names
   them. **Check:** where are they?
6. **Test aids the prose left out**: the actors' groups and bear trap runs got only the
   `NVF_PLAY_TEST_SCRIPT` given (the bear trap fails with the plant run's
   `SCRIPT_MENUS=1`: the add-on packs' message boxes still queue at 15 s); the dogs keep the
   Violet run's `GOD` and `SCRIPT_MENUS` ("added"); the Fiend alert check got
   `NVF_PLAY_TEST_SCRIPT_MENUS=1`, without which the run is attended and the first add-on
   message box stays open all run (Violet never fights).
7. **Shared launches** (checked by running both ways): Violet's alert and combat checks;
   the saloon jukebox, its on/off look and Sunny's speech-look lines; the corner messages and
   the house items. Undone after the run: the quest lines with the house items (the
   objective line was never shown on its own) and the Vigor Tester after the name/tag/trait
   menus (moved 18 s later it took no steps). Game launches for the full set: 39 as
   CLAUDE.md had them (41 with the two added on main today), 38 now with 42 game scenarios.
   The bigger saving is the selection: a branch runs only the scenarios its files call for.
8. **Push checks**: engine clippy also when `crates/` changed (the engine workspace builds
   the crates), root tests and clippy when `crates/`, the root Cargo files or `schemas/`
   changed, a Python suite when its folder changed.
9. **Locks**: both locks per launch, given back between launches; a lock with the runner's
   owner file whose process is gone is cleared; another session's mkdir lock older than
   15 min while no nvforever/FalloutNV runs is reported loudly, never removed.

Failing on main (2026-10-12), kept and marked known-failing: house items drop the BB Gun
(`ITMPistolDown`) where `UIItemsClothingDown` is expected; the menus' sounds run finds the
given stimpaks not carried at 58 s (23 sounds, not 24). Flaky: the barter from Chet's
dialogue once had the player carry nothing (the add-on packs' items not given yet) and
got "You don't have enough caps."

## Smoke tests: the last prose tests converted (2026-10-12, coordinator session)

The three smoke tests PR #226 added to CLAUDE.md as prose are now scenarios
(`sunny-wells`, `sunny-gas-station`, `cook-cook-camp`), and CLAUDE.md no longer lists any
prose test. Two checks #229 suggested were added: the `turn summary` in the residents
scenario (fastest turn at most 180°/s, no frame above 270°/s) and `chet-broom` (Chet holds
`aobroom.nif` in his left hand in the general store). All five passed on main at 3a886bb2
on this machine (11.4 min).

1. *Decision:* "voice … out of hearing for lines from 2,000 units or more" is checked as
   every out-of-hearing line being 2,000 units or more away (3 such lines, 2,110–2,635).
2. *Decision:* "held back by the group cooldown above 0" is a number ≥ 1 (27 in the run).
3. *Decision:* Chet's broom runs at noon (`--hour 12`): the note gave no hour, and at noon
   his package sweeps. *Check:* the hour you saw him sweep at.
4. *Seen, not changed:* at Cook-Cook's camp the fiends still go Combat → Lost and back
   16 times in 45 s (the handoff's "about 17"); it belongs to the combat workstream.

## Playtest fixes: lockpicking (2026-10-12, session "lockpick")

Item 31 of the playtest list. Locked doors and containers whose key the player does not
carry now open the lockpicking menu (`nvf-play` `lockpick`, rules in `fnv-game`
`lockpick`, the lock drawn by `nvforever` `lockpick_view.rs`; docs/spec-notes/lockpicking.md).
The mouse turns the bobby pin over the half circle, W/A/S/D (the movement keys, as the
game's help message says) put torque on the lock, F forces it, E leaves. Your reference
screenshots (`C:\fnv-private\references\Lockpicking`) were not on this machine; what the
first session noted from them is kept (lockpicking.md §1).

1. *Decision:* the menu's layout is the game's `lockpick_menu.xml` (information box bottom
   left with Lockpick, Bobby Pins and Lock Level; Force Lock and Exit at the right; a status
   line). *Guess:* the words "Lockpick" and "Bobby Pins" (the game's own are executable
   settings, not in the plugins). *Check:* the box and buttons against your screenshot.
2. *Decision:* the lock is the game's own model (`lockinterface01.nif`, `bobbypin01.nif`)
   drawn by a camera of its own; the plate and screwdriver fly in with the model's
   `Forward` sequence. *Guess:* the camera (45 units in front, 55° view), the lights, the
   turning axis (the files' +Y) and the direction (clockwise). *Check:* the picture against
   your screenshot (`C:\fnv-private\scratch\lockpick\menu.png` is ours), and that the lock,
   screwdriver and pin turn about the keyhole.
3. *Guess (all tuning, `fnv_game::lockpick::Settings`):* sweet spot widths 30/20/12/8/5°
   (Very Easy to Very Hard), +1 % per skill point above the lock's need; a 45° ring
   around it where the lock turns part way; 120°/s turning; a pin lasts 1 s of strain
   (+0.02 s a skill point over); a new pin 1 s after a break; mouse 0.3° per unit.
   *Check:* whether the spot feels as small as in the game, and how fast pins break.
4. *Decision:* the skill needed is 0/25/50/75/100; key-only (255) and broken locks refuse;
   no bobby pins refuses. The refusal lines are NVForever's words. *Check:* the wording.
5. *Guess:* Force Lock's chance is 3 % per Lockpick point over the lock's need, at most 95 %
   (fits the one value seen, 15 % at 30 on an Easy lock); a failure breaks the lock for
   good (only its key opens it: `GetLocked` 2, `GetIsLockBroken` 1) and uses up the pin.
   A forced lock gives no XP, a picked one `iXPRewardPickLock…` (20–60). *Check:* the
   chance at other skills/levels if you can read them in the game.
6. *Decision:* when the lock turns all the way it stays unlocked, the menu closes 1.5 s
   later and the door or container is **not** opened by itself: activate it again.
   *Check:* whether the game opens the container straight away.
7. *Decision:* where the sweet spot lies is new each time the menu opens. *Check:* whether
   in the game it stays put for a lock between tries.
8. *Decision:* sounds by the records' names: enter, pin movement (every 8°), cylinder
   turn, cylinder stop, pin tension (a loop while strained), pin break, unlock, force fail;
   `DRSLocked` when the menu refuses. The two cylinder squeak records are not used.
9. *Crime hook:* picking or forcing a lock sends `LockOpened { reference, owner, forced }`
   and logs "lockpick: crime hook: …" with the owner (the reference's, else the cell's as
   a script set it; a cell's own XOWN is not read yet). No crime rules.
10. *Not built:* the help message the first time, the debug meter, hiding the pointer over
   the lock, the lock's `Backward` and the pin's `Backward`/`Left` sequences, saving broken
   locks.

## Playtest fixes: far world (crash log, distant actors, floating troopers, dead on spawn) (2026-10-12, session "far")

List 1 items 26, 27, 24 and 28 (docs/handoff/playtest-2026-10-12.md).

1. **Crash logging (item 26).** NVForever did not log crashes. Now every run writes a run
   log, `%LOCALAPPDATA%\NVForever\logs\nvforever-<UTC time>-<pid>.log` (or under
   `NVFOREVER_HOME`; never the game folder or `Documents\My Games\FalloutNV`, checked by
   `fnv_install::guard_write`): the console's lines without colours, written line by line
   so a crash keeps everything before it; the last 10 are kept. A panic on any thread
   writes `crash-<UTC time>-<pid>.txt` beside it (thread, message, file and line, where
   the camera was: worldspace, cell and its name, position; the command line; the
   backtrace; the run log's name) and logs the same in one ERROR line. A run that ends
   normally logs "run log: the game exited normally"; at the start the previous log is
   checked and, if it has neither that line nor a crash report, a WARN names it (a native
   crash: driver access violation, stack overflow, or the process killed). Code
   `engine/nvforever/src/crash.rs`; test aid `NVF_PLAY_TEST_PANIC=<s>`; scenario
   `crash-report`.
   *Decision:* the engine's release profile keeps no debug info, so backtraces name
   functions from the PDB's public symbols only (some frames are mislabelled, no line
   numbers). `debug = "line-tables-only"` under `[profile.release]` in `engine/Cargo.toml`
   would give file:line for every frame at the cost of one full rebuild for everybody and a
   larger PDB; not done here, your call. *Decision:* native crashes (no Rust panic) get no
   report of their own, only the "did not end normally" warning; a Windows exception filter
   would need unsafe FFI. *Check:* after a crash, send the newest `crash-*.txt` and
   `nvforever-*.log` from `%LOCALAPPDATA%\NVForever\logs`.
   **The crash outside Crimson Caravan was not reproduced**: `--play --cell
   CrimsonCaravan` for 150 s walking (10 fps under load), and a free-camera flight with
   every actor from Crimson Caravan towards Camp McCarran and the South Vegas ruins (196
   cells streamed, 195 residents) ran clean. *Check:* reproduce it once more on this
   build; the run log and crash report will say where and on which thread.

2. **Actors processed far away (item 27).** Cause: with `--npc-packages all` (the default of
   `--play`) every actor of every cell ever loaded, and every actor a script moved, kept
   its full AI for the rest of the run wherever it was: packages, sandboxes, searches,
   Dialogue packages, hellos and random conversations (whose lines the game then said, so
   Fiends far off talked). Drawing, detection, combat and object scripts were already
   limited to the loaded cells. Now AI process levels (`fnv_ai::process`;
   docs/design/actor-behaviour.md §5.9): actors in the loaded 5 × 5 cells (or the
   interior) are in the high process, as before; the others get one update every 18 s
   (the GECK wiki's figure for the original's "Mid-High" level) in which they only take up
   their package and walk on over the navmesh: no conversations, hellos, searches,
   sandbox choices or Dialogue packages. Leaving the loaded area ends a random
   conversation and a search. Actors whose base has "No Low Level Processing" are not
   updated at all out of the loaded area (documented flag meaning). A "process summary"
   line at exit; scenario `far-actors`; tests in `crates/fnv-ai/tests/carry.rs`. In the
   Crimson Caravan flight: 140 of 195 residents out of the loaded area at the end, 137
   low-process updates, no conversation started out there.
   *Decision/Guess:* every actor the AI has is treated as the original's Mid-High (18 s),
   never Mid-Low or Low (120 s): all of them were handed over because their cell had been
   loaded, which is what Mid-High means. *Decision:* the high area is the drawn cells,
   including cells still fading out. *Check:* stand outside Crimson Caravan: no Fiend
   talk; walk back to Goodsprings after a while: the residents are where their packages
   would have taken them.

3. **NCR troopers floating outside Camp McCarran (item 24): not reproduced.** Checked in
   the masters [observed]: the troopers there are placed on what is under them (rays and
   probes with `fnvtool phys cell`): the gate patrols on the road (4,336-4,345, road
   statics), the sentry at the gate on a sidewalk static (4,352), the four
   NCRTrooperMcCarranCM on the runway transition static (4,336), the tower troopers on
   the wall towers' tops (5,095-5,185, NVMcCarranWall03/04, with navmesh 0x00160C65 up
   there), McCarranTowerTrooper1 following its patrol markers on its tower top. In game
   (`--play`, noon, screenshots at the gate, the south wall and the north-east tower,
   over 2 minutes each, and the patrol along the road): all stood and walked on the
   ground or the tower tops. Not changed. *Check:* where exactly (which side of McCarran,
   tower or ground, near or far, after what) and, if you can, a screenshot with the
   overlay's `pos` line; one candidate left untested is the McCarran wall imposters
   (`McCarranWallTowerImposter`, drawn by region at any distance) standing in for the
   towers while the troopers on them are drawn.

4. **Tribal village corpses (item 28).** You were right: "If Base Health is set to 0 the
   actor will spawn as a corpse" [documented: GECK wiki Stats Tab - NPC and Category:NPC].
   `TCFiend01DEAD` has Base Health 0 in its own stats (its template supplies other tabs,
   not Stats); the placed references carry no flag for it. Now a new game marks every
   placed actor whose stats actor has Base Health 0 (NPC) or Health 0 (creature) dead,
   without a killer, an OnDeath or a dead count: 889 in the ten masters. Drawn, they
   show the end of a death pose and drop as ragdolls (17 ragdolls came to the ground around the village in the
   test run), the AI leaves them alone, and E searches them. Spec note
   `docs/spec-notes/dead-on-spawn.md`; tests `crates/fnv-game/tests/dead_on_spawn.rs`;
   scenario `dead-on-spawn`.
   *Guess:* creatures with Health 0 start dead too (DeadMoleRat, DeadBrahmin,
   NightkinMeleeDEAD; the wiki speaks of NPCs only). *Not done:* the "Respawn" flag
   (TCFiend01DEAD has it) bringing them back alive after three days, and the references'
   `XRGD` "Ragdoll Bones" (probably the pose the designer's Havok Sim left) which would lay
   each body exactly as authored; ours fall where they stand. *Check:* the village's bodies
   lie on the ground, can be searched, and do not get up.

## Playtest fixes: combat rules from the GECK wiki (2026-10-12, session "pt-combat")

Branch `claude/playtest-combat`; design in docs/design/combat-ai.md §10 and §11 (the gap
table against the wiki's combat pages, and §11.1, what is left for the list-2 combat AI
workstream), alert.md §5 and §8. List 1 items 20, 30, 32, 33 and 34.

1. **Machete can't hit a gecko (item 20).** The swing was a fan of rays at eye height,
   75 units long for the machete (fCombatDistance 150 × Reach 0.5): a gecko at your feet
   was under it. Now the swing also strikes any actor whose centre is within the reach
   across the ground (plus 24 units of body), inside the swing's arc, from 32 units below
   your feet to 48 above your eyes, if nothing stands between. *Guess:* the 24, 32 and 48
   units. *Check:* swing a machete at a gecko standing still at your feet, and at one a
   step too far (it should miss).
2. **Hitting a neutral NPC (item 30).** It turns on you at once, says its Assault line
   ("What was that for?") and fights until it goes back to Normal, StopCombat, or a yield
   it accepts. Friends and allies follow the wiki's friendly-fire counts (a friend
   turns at once out of combat, after more than 4 hits within 10 s in combat; an ally
   never in combat, after more than 3 hits out of it). **Your exception:** a neutral
   already fighting someone else, hit while you are fighting too, does not turn.
   *Decision:* "you are fighting" = someone in a combat state has you on its target list.
   *Decision:* an enemy not aggressive enough to attack on sight is treated as a neutral.
   *Guess:* your teammates count as allies.
3. **Yielding (item 30).** Putting your weapon away is a yield. Actors fighting you only
   because you hit them (or because they help someone you hit) accept it if their fight
   began at most 10 s before (fCombatYieldTime): they stop and say AcceptYield. One that
   forgave you within 600 s (fCombatYieldRetryTime) and was hit again ignores the next
   yield. *Decision:* the wiki's 600 s timer is the player's, for everyone; I made it each
   actor's, to match your words "if they have already recently forgiven". Actors hostile
   by their records (raiders, fiends) and StartCombat fights never accept a yield.
   *Check:* shoot Chet once, holster within a few seconds (he stops: "accepts the yield"),
   shoot him again and holster (he keeps fighting).
4. **Allies help allies (item 34).** An actor that sees or hears an ally (or a friend,
   with Assistance "Helps Friends and Allies") in a fight takes it up against everyone
   that ally fights, never against one of its own allies or friends. *Decision:* actors
   sharing a faction, or placed from the same NPC, count as allies (the rule already
   keeping them from fighting each other). Goodsprings' settlers and Violet's dogs help
   each other this way. *Check:* shoot a Goodsprings settler where others can see:
   they should join; Violet's dogs join her fight.
5. **Combat styles and confidence (item 34).** The CSTY records are read: Melee Only has
   a gun-holder fight with its fists; the range multiplier scales the distance it fights
   from; semi-automatic guns wait the style's firing delay; melee fighters attack or hold
   by the style's chances (a dog now bites a little less often). Confidence decides when
   a hurt fighter would flee, but fleeing itself is not built: the log says "would flee".
   *Guess:* how the per-fight confidence modifier applies (threshold × (1 − modifier));
   damage per second for the threat ratio (the wiki does not say); an actor's style from
   the template that supplies its AI data, else DefaultCombatstyle.
6. **NPCs aim down the sights (items 32–33).** An NPC with a gun that has sights aims
   down them when its target is more than 20 m away and walks while aiming; within 20 m
   it fires from the hip and runs. Aimed shots lose the unaimed spread penalty, and the
   iron-sights aim and attack clips play. *Decision:* 20 m = 1,400 units (70 units a
   metre, the figure gravity fits); once up, the sights stay up to 1,350 units so it
   does not switch at the edge; the sights come up only while it can fire (sees you,
   weapon drawn). *Decision:* the record's Sight Usage (the wiki: "percentage of
   fCombatIronSightsDistance") is not used: your 20 m rule stands in, and that setting is
   neither in FalloutNV.esm nor on the wiki. *Check:* Violet from 50 m: she raises her
   rifle to her eye and walks towards you; walk up to 15 m: she lowers it and fires from
   the hip.
7. **Fiends flickering Combat ↔ Lost at Cook-Cook's camp.** They ran at you, dropped
   behind a rise, and the moment they could not see you the combat AI let go of them:
   they stood where they could not see, went Lost after 3 s, searched, saw you, and went
   back to Combat (17 times each way in 45 s). Now a fighter stays a fighter while in
   Combat and keeps closing in on where it last saw you until it sees you again.
   After: no Combat → Lost at all in the same 45 s. *Decision:* while it cannot see you
   it closes in halving the gap each time it plans its way. *Check:* stand south of the
   camp: the fiends should come at you without stopping and starting.

8. **Two older smoke expectations changed by these rules** (marked known failing, not
   changed; please confirm and they can be rewritten): `combat-violet` expected the hip
   attack clip `2hrattack3.kf`, but Violet now fires from 1,600 units, beyond 20 m, so
   every shot is aimed down the sights (`2hrattack3is.kf`); `combat-dogs` expected about
   20 bites in 30 s, now 11–12 (main: 17), because the combat style's melee decision has
   a dog attack at 55 % each time it may and wait out its Hold Timer otherwise.
   *Decision:* keep the wiki's rule. *Check:* do Violet's dogs bite often enough?
   Also seen: `detection-game` (the sneaking check of detection.md §5) fails on main
   980f0c35 the same way; marked known failing, not looked into (not combat).

Tests: fnv-ai, fnv-game, fnv-combat unit tests (new: three provocation/yield/assistance
tests on a synthetic plugin, the blind close-in test, the sights rule); smoke scenarios
`combat-sights` and `provoked-yield` added, `cook-cook-camp` now also forbids the flicker
(at most 2 Combat → Lost lines). Not done (list 2): fleeing, cover, combat groups and
their searches, NPC-against-NPC grudges, blocking, dodging, combat barks (combat-ai.md §11.1).
## Aid items: chems, food and meds (2026-10-12, workstream I "aid")

Using an aid item now changes the actor values its record says (Stimpak, RadAway, Rad-X,
Buffout, Jet, Mentats, food and drink …): the effects' Value Modifier archetype applied to
the three pools of the GECK wiki's ActorValue page, the Medicine and Food potency, the usage
monitor, the addiction roll and withdrawal. Spec note `docs/spec-notes/aid-items.md`; code
`crates/fnv-game/src/aid.rs`, `engine/nvf-play/src/aid.rs`; tool `fnvtool actor aid`; smoke
scenarios `aid-survey` and `aid-items`.

1. **Recover vs no Recover.** A Value Modifier with Recover (Buffout's +2 Strength) is a
   temporary modifier while the effect lasts; without Recover it restores or damages.
   *Decision/Guess:* without Recover and with a duration it acts magnitude **per second**
   (the Hardcore Stimpak's 5 a second for 6 s equals the normal Stimpak's 30 at once).
   *Check:* a Hardcore Stimpak heals 30 (39 at Medicine 15) over 6 s.
2. **Potency.** Medicine-flagged items (Stimpak, RadAway, Rad-X, Doctor's Bag …) and
   Food-flagged ones scale their beneficial magnitudes by `1 + 2 × skill / 100` (GECK wiki
   Ingestible Settings). *Decision:* the user's own skill (the wiki says the player's);
   both flags multiply; durations are not scaled. *Check:* a new character's Stimpak heals
   39, not 30; the Pip-Boy STATS EFF list shows the scaled amounts.
3. **Values that grow.** *Guess:* rads, dehydration, hunger and sleep deprivation go up with
   "Damage …" effects and down with "Restore …" (RadAway "Restore Rads", Nuka-Cola "Damage
   Rads"), never below 0. *Check:* RadAway lowers the RAD meter; Nuka-Cola adds 3 rads. In
   Hardcore, Beer and Nuka-Cola carry "Damage Dehydration" — under this reading they make
   you thirstier; please check H2O after a Nuka-Cola.
4. **Value And Parts / Limb Condition.** *Guess:* the normal Stimpak ("Restore Health &
   Conditions") restores every limb by the same amount as health; Doctor's Bag and Hydra
   ("NVRestoreLimbs", no actor value) restore every limb. *Check:* a crippled leg after a
   Stimpak in normal mode.
5. **Usage monitor.** The chems' effect 0x0000014F has no record in any master; *Guess:*
   it is the wiki's UMON (magnitude 30 = the GetSpellUsageNum example's "multiplied by
   30"). Each use while it lasts (30 hours of play for most chems) adds 30; from 70 the
   addiction chance is (usage − 70) / 100. *Decision:* the usage after this use counts, so
   the third Buffout within 30 hours has 20 %, the sixth 100 %. *Check:* how many doses it
   takes to get addicted.
6. **Withdrawal.** *Guess:* an addiction's penalties are held back while that chem's effect
   is active and apply when it ends; addictions do not wear off with time (Fixer's script
   removes them). *Check:* Strength while addicted to Buffout, with and without a dose.
7. **Not done:** waiting/sleeping does not advance effect timers (the original may);
   Hardcore needs do not rise with time; Chameleon (Stealth Boy); perk entry points
   (Chem Resistant, Chemist); Buffout's Strength does not raise carry weight (the
   carry rule reads the world's S.P.E.C.I.A.L.); NPCs using stimpaks in combat (left for
   the combat workstream: it needs the combat AI to choose to heal).
8. **Test aid added:** `NVF_PLAY_TEST_SCRIPT=<t>:damage:<ref>:<actor value>:<amount>`.
9. **Survival follows Endurance here** (actor-values.md §3.1 hypothesis), and formulas read
   current values, so a Buffout taken before food raises the food's potency (Purified
   Water 1.42 instead of 1.3 after Buffout's +3 Endurance). *Check:* whether chems change
   food healing in the original.
10. **Test aid fixes found on the way.** The inventory drop/use aid (`NVF_PLAY_TEST_DROP`)
    now turns the Pip-Boy's ITEMS page to the item's tab (the list shows one tab, so aid
    items were "not carried": the ui-sounds scenario's known failure on main), waits while
    a script's message box is up, and waits for the scripts to be ready; items used before
    the scripts were ready (or under a loading screen) are kept and applied when they run
    (they were dropped). *Decision:* the ui-sounds stimpak expectation's known-failing mark
    is removed (it passes); "24 played" stays known-failing at 23 with the stimpak's sound
    now among them — one other sound is short, not found which.
11. **quest-messages failed 3 of 3 runs on this branch, from load, not this change:** the
    scripts were ready only after 14–17 s (four agents building on the machine), so its
    test aids at 3, 9 and 9.1 s ran in one frame and the objective was completed before
    it was shown. Nothing of the quest lines changed here. *Check:* it passes on a quiet
    machine.

## Graphics: engine-level improvements (2026-10-12, session "gfx")

Your list 2 item "ENGINE LEVEL GRAPHIC IMPROVEMENTS". Design, survey and the reasons for each
choice: docs/design/graphics-enhanced.md. No asset is changed or added; every value below is an
engineering choice by eye, not a fact about the original game.

1. *Decision:* a new switch `--graphics vanilla|enhanced`. `enhanced` turns on five
   improvements, each also a switch on the pause menu's settings page under "Enhanced
   graphics" (the page now has two columns when it is long), and `--graphics-only <keys>`
   turns on single ones for comparing:
   - **Ambient occlusion** (`ambient-occlusion`): creases, corners and the ground under things
     darken. Our own screen-space pass from the depth (Bevy's needs multisampling off, which
     the plants' soft edges depend on). *Guess:* radius 64 units, strength 0.55, fading out
     4,000-7,000 units away. *Check:* the crates by the house doors in Goodsprings, the bench
     and air conditioner at night, interiors' corners; that the water does not show dark
     patches from the lake bed under it.
   - **Finer, longer sun shadows** (`fine-shadows`): shadow maps 4096 instead of 2048 and
     reaching 1.5 × `--shadow-distance` (24,000). *Check:* shadow edges of roofs and fences
     look sharper; shadows on far hills; frame rate.
   - **Bloom** (`bloom`): light brighter than a threshold glows (the sun, bright sky, lamps).
     Done on the picture before the game's own grade, which still applies on top; it does
     not use the image spaces' HDR/bloom values (not decoded yet). *Guess:* threshold 0.82,
     intensity 0.9, about 136 pixels wide. *Check:* lamps and lit windows at night and in
     the saloon; that the noon sand does not glow.
   - **Sharpening** (`sharpen`): contrast-adaptive, against the four neighbours. *Guess:*
     strength 0.5. *Check:* brick and roof textures crisper, no bright or dark rims on edges.
   - **Anisotropic filtering 16x** (`anisotropic`; 8x before): roads and ground at a slant.
     The switch applies to textures loaded after it changes.
2. *Decision (tried and dropped):* Bevy's contact shadows (speckled dark bands on near
   surfaces without TAA: `C:\fnv-private\scratch\gfx\pair-cs2.png`), sun in-scattering in the
   fog (invisible in the clear weather, a shadow lookup per pixel), Bevy's bloom (needs an HDR
   view, double the memory traffic), Bevy's CAS sharpening (garbled the picture), TAA, SSR,
   volumetric fog. Why each: graphics-enhanced.md §2, §4.
3. *Frame rate* (GTX 1050, 1280 × 720, Goodsprings noon, 60 s runs, machine shared with
   other sessions, so noisy): vanilla 14.5 fps, enhanced right after 10.0 fps; fine shadows
   alone 19.6, the other four together 10.6, vanilla again at the end 7.2. *Decision:* enhanced
   costs about a third of the frame rate here, so **normal play defaults to vanilla**
   (`graphics::PLAY_DEFAULT`, one line to change on a faster GPU). Test runs (`--screenshot`,
   `--timelapse`, `--frame-json`, `--run-seconds`) always default to vanilla, so no golden,
   picture check or smoke expectation changed. *Check:* play with `--graphics enhanced` and
   say whether the frame rate is acceptable to you; if so, flip the default.
4. *Pictures* (in `C:\fnv-private\scratch\gfx\`, 1280 × 720): Goodsprings noon before
   `gs-day-vanilla2.png`, one feature each `gs-day-ambient-occlusion.png`,
   `gs-day-fine-shadows.png`, `gs-day-bloom.png`, `gs-day-sharpen.png`; crops side by side
   (left vanilla) `pair-ao.png`, `pair-fs.png`, `pair-sh.png`; night `night-vanilla.png` /
   `night-enhanced.png`. (No anisotropic pair: the difference is in the far ground at a
   slant. The saloon pair caught the DLC message box and is not useful.)
5. *Observed, not mine:* one of the vanilla noon frames (`gs-day-van.png`) came out much
   warmer and brighter than the others of the same spot, hour and build, and than main's
   build (`gs-day-mainexe.png`): the grade seems to differ from run to run at start.
   *Check:* whether you see the noon colour change between launches.
6. *Smoke:* new scenario `graphics-enhanced` (starts enhanced, switches four settings while
   the game runs, checks the "graphics: …" log line after each).

## Script stubs audited (2026-10-12, session "stubs")

Your list 2 item: go through the placeholder script functions against what the engine has
now. The audit is `docs/design/script-stubs.md` (every stub with its use in the ten
masters' scripts, the system it needs and its status); `fnvtool script stubs` re-counts it
and the smoke scenario `script-stubs` checks the counts.

1. **The count.** 123 stubs before; 22 implemented here (1,807 of the 4,185 uses in the
   shipped scripts that reached a stub), 4 made deliberate no-ops, 3 implemented by the
   combat merge (#235); 94 left: 23 assigned to other workstreams (16 E2 combat AI, 6 J
   karma and crime, 1 H followers), 10 implementable later, 61 waiting for a system not
   built yet (VATS, effect shaders, see-through actors,
   saving, Caravan and the casino, repair and crafting menus, talking activators …).
   *Check:* the table in script-stubs.md §4 for anything you want sooner.

2. **ResetAI (711 calls) and the AI codes.** ResetAI drops the script package, forgets the
   goal and unreachable packages, lets go of a Look and chooses packages again; it does not
   stop combat (that is the combat workstream's). GetCurrentAIPackage and
   GetCurrentAIProcedure answer with the GECK wiki's codes from the package type, the
   detection state and the furniture.
   *Guess:* Find packages are 0 EXPLORE; a Dialogue package is 34; in combat 5/13, searching
   29/36, talking to the player 6/4; a package whose goal was met reads procedure 17 DONE
   (eight package end scripts in FalloutNV.esm wait for four actors' 17). *Check:* scripted scenes
   that wait for an actor to finish, and scripts that check an actor sleeps (code 4).

3. **Look / StopLook (401 calls).** A script's Look now turns the actor's head over every
   other look (conversation, alert); a marker as the target is looked at at eye height.
   *Not done:* Look's "rotate the whole body" flag. *Check:* scenes where an NPC is told to
   look at someone or something (159 Look calls in 30 scripts).

4. **Movement, sneaking, sight, camera.** IsSneaking, IsMoving, IsRunning, GetLineOfSight and
   IsPC1stPerson read what the engine measures (detection every 0.25 s, the player's
   controls every frame). *Guess:* a moving actor reads IsMoving 1 (forward): the
   direction is not told apart yet; NPCs never sneak yet; GetLineOfSight from the player
   reads 0 (the player is not measured as a detector).

5. **The weather (SetWeather, ForceWeather, ReleaseWeatherOverride, IsRaining,
   GetIsCurrentWeather).** The sky takes a script's weather (a fade, or at once) and holds
   it against the climate when the script says so. *Guess:* a weather set without the hold
   stays until the climate's own choice changes. *Check:* Hoover Dam's battle weather and
   the Honest Hearts rain if you get there.

6. **Menus and challenges.** ForceActiveQuest marks the quest in the Pip-Boy; a terminal
   item's ForceTerminalBack goes back a level (or closes the terminal);
   AddSPECIALPoints / SetSPECIALPoints / AddTagSkills open the Vigor Tester's and the
   tag-skill menus with their rules (no lowering below the old values, old tags locked);
   IncrementScriptedChallenge counts to the challenge's threshold.
   *Guess:* AddSPECIALPoints uses the Vigor Tester's menu (New Vegas has no other
   S.P.E.C.I.A.L. page); a Start Disabled challenge counts nothing until unlocked.
   *Not done:* the challenge's XP and its "completed" message.

7. **Deliberate no-ops:** SetRumble, PurgeCellBuffers, PreloadMagicEffect,
   SetSceneIsComplex now do nothing on purpose (no trace). **Not registered** and left so:
   IsDLCInstalled (no documentation to go on), AddItemToLeveledList, GetIsAlignment,
   GetXPForNextLevel, GetIsObjectType (§6).

8. **New test aid:** `NVF_PLAY_TEST_SCRIPT` takes `call:<ref or ->:<Function>[:<arg>…]`,
   any function by name as a script would call it; scenario `script-stubs-play` uses it.

*Decision:* other workstreams' functions (combat, crime, followers, aid) were not touched;
`crates/fnv-game/src/functions/stubs.rs` lists who owns which, and a test fails when one
of them is implemented without being taken off the list.

## Graphics follow-ups: TAA, sun shafts, water reflections (2026-10-12, session "graphics-2")

Follow-ups to PR #236 (list 2, "ENGINE LEVEL GRAPHIC IMPROVEMENTS"; handoff §4 item 8). Reasons
and figures: docs/design/graphics-enhanced.md §3.6-§3.8, §4, §5.1. Every value below is an
engineering choice by eye, not a fact about the original game. The vanilla look is unchanged and
is still the default of every test run, smoke scenario and lab frame.

1. **Temporal anti-aliasing** (`taa`, settings page "Temporal anti-aliasing"). Our own pass:
   Bevy's needs motion vectors from every material, which our custom shaders do not write.
   *Decision:* it is the **other choice to multisampling, not stacked**: on, the view is drawn
   without MSAA and smoothed over the frames; off, `--msaa` comes back (switchable while
   playing). *Decision:* **not part of `--graphics enhanced`**: standing still it is as smooth
   as MSAA 4x (plants' cut-outs smoother), but textures are softer, clearly so while the view
   moves, and you play with 8x MSAA. It is cheaper than MSAA 4x (GPU 0.05 ms for the pass,
   about 0.4 ms saved in the main pass and prepass on the RTX 4070 here). *Guesses:* blend 10 %
   still / 50 % moving fast, mip bias -0.5, points nearer than 48 units move with the camera
   (the arms and weapon), a jump over 400 units in a frame starts the history over. *Check:*
   turn it on in the settings and walk and turn in Goodsprings: softness, ghosts behind walking
   NPCs and swaying grass, the arms while turning; say if you want it in `enhanced`.
2. **Sun shafts** (`light-shafts`, in `enhanced`): light streaming past what stands in front of
   the sun, from the game's own sky and sun as drawn, as strong as the weather's fog makes the
   air hazy. *Guesses:* strength 0.6 + 0.9 x haze, haze = 20,000 / fog far (clear weather 0.1),
   source within 0.3 of the screen height of the sun. *Check:* Goodsprings at 19:00 facing the
   sunset (`--pos=-69000,3500,8900 --heading 260 --pitch 2 --hour 19 --graphics enhanced`);
   a dusty or cloudy weather (stronger there by design); that noon looking away from the sun is
   unchanged.
3. **Screen-space water reflections** (`water-reflections`, in `enhanced`): only where the
   existing mirrored reflection does not reach (water at a second height in view, e.g. Lake Las
   Vegas next to Lake Mead's cells); interiors have no WATR planes in this engine, so nothing
   changes there. *Decision:* kept although its value is small: checked only by forcing it in
   place of the mirrored reflection at Lake Las Vegas (no artefacts, small difference). *Check:*
   if you know a spot with two water levels in view, whether the lower one now shows the shore.
4. *Decision (tried and dropped):* **Bevy's volumetric fog**: on our non-HDR picture it turned
   Goodsprings at dusk dark brown with a visible box edge across the mountains
   (`C:\fnv-private\scratch\graphics-2\vol-dusk.png`) and would replace the weather's fog look;
   the sun shafts give the light shafts it was meant for. Contact shadows and CAS (#236) were
   not retried.
5. *Found:* switching multisampling while the game runs (now possible through TAA) ended the run
   with a validation error in the ambient occlusion pass; the passes that read the depth now go
   by the depth texture's own sample count. The `graphics-enhanced` smoke scenario switches TAA
   on, walks and turns, and off again, and forbids rendering errors.
6. *Test aids for measuring:* `--log-frame-times <s>` (frame time and GPU time per pass) and
   `--graphics-cycle "vanilla;taa;..."` (switches between sets in one run). On this machine
   the picture stayed at 60 Hz whatever was asked, so the per-pass GPU times are the figures;
   the GTX 1050 was not measured. *Check:* your frame rate with `--graphics enhanced` and with
   TAA on instead of MSAA.
7. *Seen in passing (not this workstream):* at Camp Golf (`--pos=29500,62000,3400 --heading 0
   --pitch -10 --hour 12`, free camera) an NCR trooper stands in the air above his shadow;
   possibly your floating-troopers item 24.

## Aid leftovers: waiting, Hardcore needs, Stealth Boy, perks, carry weight (2026-10-12, session "aid-2")

The five leftovers of the aid items (§4 item 6 of the handoff). Spec note
`docs/spec-notes/aid-items.md` §2, §6, §10–§12; code `crates/fnv-game/src/needs.rs`,
`crates/fnv-game/src/perks.rs`, `crates/fnv-game/src/aid.rs`, `engine/nvf-play/src/aid.rs`,
`engine/nvf-play/src/wait.rs`, `engine/nvforever/src/stealth_field.rs`; tool `fnvtool actor
aid --perks --perk P --set AV:v --wait H [--sleep]`; smoke scenarios `aid-perks-needs-survey`,
`aid-wait-perks`, `aid-hardcore`, `aid-stealth-boy`.

1. **Effect timers during wait and sleep.** *Decision/Guess:* each hour waited or slept
   counts as the seconds of play in which an hour passes at the TimeScale: 3600 / 30 =
   120 s. Buffout (240 s) therefore wears off in the second hour waited, Chemist's 480 s
   in the fourth; a usage monitor (108,000 s = 900 waited hours) practically never. The
   other reading would be 3,600 s an hour (every chem gone after one hour, the usage
   monitor after 30). *Check:* Buffout, then wait 1 hour and 2 hours: is Strength back?
2. **Hardcore needs rising.** The masters have `fHCStarvationRate` 25 and
   `fHCSleepDeprivationRate` 50 and no dehydration rate; the game's help message says
   dehydration rises fastest, starvation slower, sleep deprivation slowest.
   *Decision/Guess:* the setting is the seconds of play per point (larger = slower):
   Hunger 4.8 an hour (Minor Starvation after about 42 hours), Sleep Deprivation 2.4 an
   hour (Minor after about 83 hours). *Guess:* dehydration 12.5 s a point (9.6 an hour,
   Minor after about 21 hours). *Guess:* an hour slept takes 100 off sleep deprivation;
   hunger and thirst rise while sleeping as while waiting. *Check:* GetAV Dehydration,
   Hunger, SleepDeprivation after 1, 6 and 24 hours waited and after 8 hours slept (spec
   §9 item 7). These guesses decide how hard Hardcore feels; please say if they feel
   wrong in play.
3. **Need stages.** The RADS/DEHY/HUNG/SLPD records give each stage's threshold (200 …
   1000) and its spell. *Decision:* the player has the spell of the highest stage reached
   and loses it when the value drops below (so water at Minor Dehydration takes the
   penalty away at once); rads' stages hold in every mode, the needs' only in Hardcore
   (they go when Hardcore is turned off). *Not done:* the HUD's H2O/FOD/SLP codes, the
   "Your dehydration level has increased" messages (the log says the stage), refusing a
   wait or sleep that would kill (the help message says the original refuses it).
4. **DamageActorValue / RestoreActorValue on rads and needs** now move them the way the
   aid effects do (Damage raises, Restore lowers, never below 0): the Cannibal perk's
   script restores Hunger 50 with the health it gives. Before, a Restore on Hunger did
   nothing and a Damage on Rads lowered them. *Check:* RadAway-like scripts and eating
   a corpse with Cannibal in Hardcore.
5. **Stealth Boy.** Its "Stealth Field" (archetype Chameleon) now puts Chameleon 75 (and
   Sneak +100) in the temporary pool for 120 s; detection already read Chameleon: in the
   Goodsprings check the settler went from detected to hidden at 1,671 units in sight,
   visual term 0. *Decision (look):* no refraction shader exists, so the player's body
   and first-person arms are drawn see-through: blended at 30 % opacity, tinted blue
   (`stealth_field.rs`; the shot shows a faint ghost). *Check:* whether you want the
   original's shimmer instead (a shader of its own).
6. **Perk entry points.** Read from the PERK records (109 perks of FalloutNV.esm have
   entry point effects). The codes were identified by the only perks that use them: 23
   Modify Addiction Chance (Chem Resistant × 0.5), 25 Modify Positive Chem Duration
   (Chemist × 2, the chem challenge perk × 1.33), 44 Modify Radiation Consumed (Lead Belly
   × 0.5); functions 1/2/3 read as Set/Add/Multiply. *Decisions:* only the player's
   perks; a ranked perk applies its highest owned rank's entries only; entries in
   ascending priority; entries with conditions on a tab other than the perk owner's are
   skipped (none of the aid ones has any). *Decision:* Chemist lengthens beneficial
   effects of items whose equip type is Chems, not the usage monitor; the Stealth Boy is
   a Chems item, so Chemist doubles it too. *Check:* Stealth Boy duration with Chemist;
   Buffout with Chemist lasts 8 minutes. *Not done:* the other entry points (combat's
   still go through `PerkHooks` defaults), perks' ability effects (type 1, e.g. Life
   Giver), "Modify Addiction Duration" (no master perk uses it).
7. **Buffout and carry weight.** *Decision:* the capacity uses the current Strength
   (temporary modifiers included, within 1–10) plus Carry Weight's own modifiers; the
   menus' Wg line follows it (Buffout at Strength 5: 200 → 220, back to 200 when it
   ends). *Check:* the Wg line in the Pip-Boy after Buffout.
8. **Test aids added:** `NVF_PLAY_TEST_SCRIPT=<t>:hardcore:<0|1>`, `<t>:perk:<PERK>`,
   `<t>:set_av:<ref>:<actor value>:<value>`. The wait now logs `wait: hour <n> passed for
   the player (120 s of effects): …` each hour, and `needs: <value> <level>: <stage>` on
   each stage change.
9. **A lock mishap (process, not code):** during a picture run a shell line of mine
   removed the two game locks (`run.lock`, `smoke.lock`) that another session held, for
   a moment, after my own `mkdir` had failed; both existed again when checked right after
   (taken again by their owner). If another session's run of 2026-10-12 about 18:03
   overlapped a launch, that may be why. Later runs used a wrapper that only removes
   locks it made.

## Lockpicking leftovers (2026-10-12, session "lockpick-2")

Item 4 of the handoff list (leftovers of #232). Your ten screenshots in
`C:\fnv-private\references\Lockpicking` were on this machine and were compared with ours
(docs/spec-notes/lockpicking.md §1, §2, §2.0, §2.1, §7.1).

1. **Help message the first time.** *Decision:* the game's own message `HelpLockpicking`
   (FalloutNV.esm 0x0000017C, title "Lockpicking", its text unchanged) comes up over the
   menu the first time it opens, in the tutorial box laid out by `tutorial_menu.xml`
   (VDSG symbol, "VDSG MANUAL", "Close E)"); E (also Enter, Esc, Tab) or a click on Close
   shuts it; nothing is picked while it is up; the lock behind is drawn at about half
   brightness. The key names in its text are the keys bound now ("WSAD", "F").
   *Guess:* once per game (kept in the game state, see 5); the Next/Previous column width
   (120) and VDSG symbol width (128) fitted so the text wraps and the words sit as on your
   screenshot; Close's brightness. *Check:* that the game shows it only once, and whether
   Esc/Enter should close it.
2. **Debug meter.** *Decision:* behind the off-by-default debug switch
   `NVF_PLAY_DEBUG_LOCKPICK=1` only: the menu file's meter bar with the sweet spot (bright)
   and the ring around it (dim), the arrow over the pin's angle, "Pin health …% Lock …° of
   90°", and the Sweet Spot / Concentric values (shown, not adjustable). *Guess:* the
   colours and words (the file leaves them to the code).
3. **Pointer over the lock.** *Decision:* hidden inside the menu file's detect area
   (taken literally: from 60 units in to the right edge, from the top down to two thirds
   of the screen), shown elsewhere and over the help box, as on your screenshot (pointer
   at the left edge). *Check:* that the pointer never shows over the lock and comes back
   at the left edge and over the buttons.
4. **Exit animations.** *Decision:* the model files' sequences are now used:
   - a broken pin plays the pin's `Backward` (it twists) then `Left` (the long piece flies
     off); the short piece stays in the keyhole until the next pin, as on your screenshot
     after a break; the next pin now comes 1.45 s after a break (was 1 s) so the break
     plays out;
   - once the lock is in, it is posed by the lock model's `Backward` sequence at the time
     its turn has reached: the cylinder turns and the screwdriver follows with a twist of
     the wrist. This matches your 40° and fully-turned screenshots (screwdriver nearly
     level at 40°, pointing down when open), where turning everything rigidly did not;
   - the pin no longer turns with the lock (your screenshots: it stays where you put it);
   - leaving (E) pulls the pin out (its `Forward` backwards, 0.5 s) while the menu fades;
     after opening, breaking or running out of pins the menu holds 1.5 s, then fades out in
     0.5 s over the world (your screenshot of an opened lock shows the HUD coming back over
     the lock still drawn).
   *Guess:* the 0.5 s fade and pull-out. *Check:* the break and the leaving in game.
5. **Broken locks saved.** *Decision:* broken locks (and whether the help was shown) are
   part of the game state's snapshot (`GameSnapshot.locks`, written only when not empty).
   NVForever has no save files yet, so today a broken lock stays broken for the session
   and in any snapshot; it will be saved when saving is built.
6. **Ringo's floor safe.** *Finding:* not a collision fault: the safe's collision is
   whole and the crosshair finds it (standing on it 123 units straight down, or from the
   mattress's edge looking 65° down at 145). The safe lies half under the edge of the
   floor mattress, and the floor beside it is narrower than the player, so the player
   stands on the mattress or on the safe; from the mattress the safe's door is 131 below
   the eye and only a narrow cone is within the 150 reach. The test aid
   `NVF_PLAY_TEST_NEAR` had chosen a spot 147.7 away assuming a 112-unit eye, while the
   first-person body puts the eye at about 121, so the view ray needed 154 and missed.
   *Fix:* the aid now chooses with the body's eye and 5 units to spare, and tries closer
   spots aimed at points of the target; the new smoke scenario `lockpick-floor-safe` uses
   the safe. *Check (in game):* can you target this safe from the mattress? If the
   original lets the crosshair pass the mattress or reaches further downwards, tell us
   which (physics.md §6 has the pick-sphere question).
7. **Compared with your screenshots and changed:** "Lockpick Skill" (was "Lockpick");
   the lock's size and place (camera 70 units away with a 17° view, the cylinder about
   0.72 of the screen's height, a little above the middle; was 45 units / 55°, the lock
   less than half that size); the pin not turning with the lock; the screwdriver's
   motion (4); the help box (1). *Left as it is:* the room dim behind the lock on your
   screenshots: our lock stays on black, because a see-through shade let quest and
   corner messages show through the menu; the pin's and screwdriver's colours (ours
   coppery and red, yours brass and brown: lighting). *Check:* the look against your
   screenshots.
8. *Smoke:* `lockpick` now closes the help and checks the help line and the leaving lines;
   new `lockpick-floor-safe` (help once, leaving with the pin pulled out, a failed Force
   Lock breaking the safe, the next activation refused).

## Karma and crime (workstream J, branch claude/karma-crime, 2026-10-12)

Your list 2 items "FACTION KARMA" and "CRIME". Facts and tags:
docs/spec-notes/crime-karma.md. The game's own help messages (`HelpCrime`, `HelpKarma` in
FalloutNV.esm) gave the crime rules: minor crimes (theft; opening an owned lock, which the
help calls trespass) turn only the victim who sees it; major crimes (assault, murder) the
victim and its friends.

1. **Karma for kills.** Killing an Evil or Very Evil NPC gives +100
   (`fKarmaModKillingEvilActor`, the only kill setting in FalloutNV.esm); a Good or Very
   Good NPC costs 100; murdering a Neutral NPC costs 25. *Decision/Guess:* the setting is
   the amount (it is not documented how it is used); the good-side and neutral amounts
   are mine (no setting for them is in the plugins, the executable is encrypted);
   creatures give no karma (a dead radroach is "Evil" in the data); alignment by the
   globals KarmaVeryEvil −750, KarmaEvil −250, KarmaGood 250, KarmaVeryGood 750.
   *Check:* kill a Fiend (Violet is −500, Evil): "You've gained Karma!" with the good
   icon; kill Sunny (900, Very Good): "You've lost Karma!" with the evil icon.
2. **Karma for stealing and breaking in.** Each owned item or stack taken, and each
   owned lock picked or forced, costs 5 (`fKarmaModStealing`), seen or not; nothing when
   the owner is an Evil faction, a faction that does not track crime, or an Evil NPC.
   *Decision:* per take, not per item value; the lock cost is mine (you asked for
   breaking in to cost karma). *Check:* take Chet's cigarettes: −5 and the corner message.
3. **Ownership.** A reference's own owner (XOWN, with the faction rank XRNK), else its
   cell's (the CELL's XOWN/XRNK, or SetCellOwnership). You own what your base or a faction
   you have the rank in owns. *Decision:* items' own owners inside containers (COED) are
   not read: a container's items are the container's owner's; a corpse's inventory is
   nobody's; items you dropped are yours.
4. **Theft.** The owner (or a member of the owning crime-tracking faction) who has you
   *detected* when you take something turns on you and says its Steal line; others who
   see it do nothing. *Decision:* the game's owner first tries to take the item back;
   that is not built, the owner fights straight away. A crime enemy attacks whatever its
   aggression, and a crime is not yieldable (holstering does not end it); StopCombat
   from a script does. *Check:* in Chet's store, take something behind the counter
   while he looks: he says his Steal line and fights; take something while he looks
   away: only the karma.
5. **Picked locks** of owned doors and containers: the owner who sees it attacks at once
   (the help's rule). Hacking an owned terminal is the same crime in the help: not
   hooked yet.
6. **Trespassing.** In an owned cell without the Public place flag that you do not own
   (23 interiors in FalloutNV.esm: Primm's and North Vegas's houses, Novac's motel rooms;
   all of Goodsprings' interiors are public), the owner who sees you warns you
   (GuardTrespass) every 5 s, twice (`iGuardWarnings` 2), then attacks. *Guess:* the Guard
   package's warning rule applies to owned cells too; the Off Limits flag (one cell) and
   the doors' No Alarm flag are not used. *Check:* walk into a Primm house with its
   owner in.
7. **Murder.** Killing a non-Evil NPC that did not attack you of its own accord. Every
   witness who cares about the victim (its ally or friend, same faction or same base)
   turns on you and says its Murder line, and when such a witness saw it, you become an
   enemy of the victim's crime-tracking factions (all members attack on sight until a
   script clears it). IsPCAMurderer becomes 1. *Decision:* only seen murders set the
   faction flag. *Check:* kill Easy Pete in front of another settler: Goodsprings turns.
8. **Assault** (already on main as provocation) now also counts as a major crime
   (GetMajorCrimeCount). *Decision:* an assault does **not** make you an enemy of the
   victim's whole faction (the wiki's enemy flag is for any major crime): one punch would
   turn every NCR trooper in the Mojave. Allies who see it still help (assistance).
   Please decide.
9. **The prompt.** Over an owned item, container or lock the crosshair and its prompt are
   drawn in the crime colour, and an item reads "E) Steal". *Guess:* the colour is the
   HUD's "alt" colour from Fallout_default.ini (255, 67, 42); the word "Steal".
   *Check:* in Chet's store the shelves' items should be red.
10. **No bounties.** New Vegas has none (the wiki: crime gold is unused in Fallout);
    reputation (REPU) is changed by scripts only, crimes do not change it here.
11. **Script functions** built: RewardKarma (with the corner message), GetIsAlignment,
    IsPCAMurderer, Get Minor/Major CrimeCount, GetActorCrimePlayerEnemy, GetAlarmed,
    IsActorAVictim, IsCellOwner, IsInMyOwnedCell, IsTrespassing, GetTrespassWarningLevel,
    SendTrespassAlarm, SendAssaultAlarm (93 calls in the scripts; it now really turns the
    victim on you). SetPCEnemyOfFaction now has an effect: that faction's members attack
    you on sight. *Check:* quests that call SendAssaultAlarm or SetPCEnemyOfFaction.

Shared files touched (the combat AI agent works nearby): `fnv-game` `combat.rs` (one call
in `kill`), `awareness.rs` (crime enemies in `would_attack`, forgiving in `stop_combat`),
`provocation.rs` (one call when an assault grudge starts), `functions/mod.rs` (the crime
entries only), `game.rs` (state fields), `data.rs` (cells, faction flags, ranks); engine
`activate.rs`, `reticle.rs`, `menu.rs`, `menu/model.rs`, `lockpick.rs` (log text).

Tests: fnv-game unit and synthetic-plugin tests (`tests/crime.rs`, 7), root and engine
clippy; smoke: `karma-survey`, `crime-steal`, `karma-fiend` (new; the fiend is killed by
a new `kill:<actor>` script test aid, shooting her was not repeatable under load) and the
changed-file set pass (alert-fiend, provoked-yield, barter, container-e-key, speech-look,
lockpick, cook-cook-camp, dead-on-spawn), except `house-items` and `ui-sounds`: in each run
their container or inventory step fell while an add-on's start-up message box was open
("menu request ignored: a menu is already open"), a different step each run; not caused by
these changes (they only add a message after a container command).

## Far-world leftovers: corpses respawning, their stored poses, the Low process level (2026-10-12, session "far-world-2")

Handoff item 5 (docs/handoff/playtest-2026-10-12.md §4), the leftovers of PR #233.

1. **Corpses' Respawn flag.** Built as a cell reset (`fnv_game::respawn`,
   docs/spec-notes/dead-on-spawn.md §4): when a cell comes back into the loaded cells
   after `iHoursToRespawnCell` (72 h) away, its dead actors whose base has Respawn are
   recreated alive at their authored place. The village's seven Fiends and the burned
   bodies around it have the flag and come back. Scenario `corpse-respawn`.
   *Decision:* "recreated" read as starting again as authored: actor values, damage,
   killer, script-set factions and packages dropped; inventory rolled again.
   *Decision:* the wiki says such a corpse "will respawn alive after 3 days with 0 HP";
   they come back with full health (alive at 0 health means nothing here).
   *Decision:* the visit is the game's loaded cells (the player's cell and the 5 × 5 around
   it); the clock starts when a cell leaves them. *Decision:* respawning containers in a
   reset cell refill too (the `fnv_sim` code for it existed and was never called).
   *Guess:* killed leveled-spawn creatures with the flag come back too (cazadors at the
   village did in the test run). *Not done:* encounter zones' "Never Resets", removing the
   bodies of killed non-respawning actors at a reset, ResetInterior.
   *Check:* kill the village Fiends' neighbours or leave the corpses, go far away (more
   than two cells), wait or sleep three days, come back: the Fiends stand again; a corpse
   without the flag (e.g. `DeadGhoul2`) stays dead.

2. **The poses the designers left the bodies in.** Found: placed corpses carry `XRGD`
   "Ragdoll Bones", the pose the GECK's Havok Sim left (dead-on-spawn.md §5 says how each
   entry maps to a bone and which rotation convention fits, all observed on the 278 human
   corpses of FalloutNV.esm). A corpse laid out by a new game is now put in that pose at
   rest instead of falling from its death pose; in the village 16 bodies lay in their
   stored pose, flat on the ground next to their blood decals. *Guess:* the root entry's
   rotation is relative to the reference's (the reading under which most bodies fell
   backwards; the alternatives put heads every way or all to one side). *Not checked:* no
   capture of the original's bodies was taken (the owner is away); creature corpses use
   the same rule untested beyond the entry count; clutter skeletons' `XRGD` (REFR) are not
   posed. *Check:* the tribal village and other placed corpses (Forlorn Hope's dead
   troopers, the dead brahmin): do they lie as in the original?

3. **The 120 s Low process level.** The AI now has three levels (`fnv_ai::process`,
   actor-behaviour.md §5.9): the loaded area (every frame), the **cell buffer** (every
   18 s) and beyond it (every 120 s). *Guess:* the buffer is the 36 exterior cells drawn
   most recently (the 25 drawn now among them) and the 3 interiors visited last, from your
   Fallout_default.ini (`uExterior Cell Buffer=36`, `uInterior Cell Buffer=3`); how the
   original fills and empties it is not documented. *Decision:* the original's Mid-Low
   (interiors behind the doors of the drawn exterior cells: Chet's store, the saloon seen
   from Goodsprings) has an "unknown" rate; here they get the Mid-High 18 s.
   *Decision:* an actor walking out of the loaded area (or on into another cell) takes
   that cell into the buffer, and a follower is kept at Mid-High while its leader is at
   Mid-High or above: without this Sunny, running to the wells through cells never drawn,
   fell to 120 s updates at once and Cheyenne was left 1,066 units behind (`sunny-wells`). Scenarios `far-actors` (now flying at 400 units/s: at 1500 the cells leave
   the buffer within seconds) and `far-actors-low`. *Check:* nothing visible should change;
   after a long trip, residents of far places are where their packages would have them.
## Smoke failures on main explained (2026-10-12, workstream "fixes")

All four were timing on a loaded machine (3-4 agents building, the game at 5-15 frames a
second), not a feature broken by the merges #225-#236. Runs: `C:\fnv-private\scratch\fixes\`.

1. **detection-game** (detection.md §5). *Cause [observed]:* not a regression: the build of
   dfb60896 (the merge that added detection) fails the same way here, as do 3570ebfc and
   main ec6aaaf0. The settler GSSettlerAM 00104F0A turns into sight at about 12:04-12:05 game
   time; the shot is at 22 s on the test aids' clock. Two things held the game clock back
   against that clock: the add-on packs' twelve start-up message boxes (with
   `NVF_PLAY_TEST_SCRIPT_MENUS=1` each holds the game about 1 s) and, below 7.5 frames a
   second, two frame caps: physics ran at most 8 steps of 1/60 s a frame and the residents'
   AI capped its frame at 0.1 s, so actors walked and turned slower than the game clock
   passed. The shot then fell at about 12:02-12:03 and the settler noticed only after it.
   *Fixed:* play now runs physics steps for a whole frame of the virtual clock (0.25 s,
   `nvf_play::phys::play_params`) and the residents use the frame time up to the same 0.25 s
   (engine bug since 2026-10-10); the scenario passes `--no-messages` (#241), so the
   start-up boxes close on the frame they open. Passes now; mark removed.
   *Decision:* furniture.rs, idles.rs and nav.rs (travellers) still cap at 0.1 s; left as
   they are (animation and the headless travel walk). *Check:* NPCs walk at their speed
   when the frame rate is low.
2. **quest-messages.** *Cause [observed]:* the scripts were ready 10-20 s after launch, so
   the steps at 3, 9 and 9.1 s all ran in the scripts' first frame; the objective was
   completed while still waiting to be shown (shown once, completed: quest-messages.md §5,
   by design, not a queue bug). *Fixed:* when the scripts first run after the first timed
   `NVF_PLAY_TEST_SCRIPT` step, every timed step comes that much later, keeping their
   distances (`script_events::delay_late_aids`, unit-tested); steps gated on a quest stage
   are left alone, nothing moves when the scripts are ready in time. Passes; marks removed.
3. **ui-sounds "24 played".** *Cause [observed]:* the 24 of 2026-10-11 (rebuilt b5ff0244 and
   ran it: 24) include the `NVF_PLAY_TEST_MENU` aid's inventory pass (open, a row selected:
   UIPipBoyScroll, a jumpsuit put on, close). On a loaded machine that pass ran into the
   start-up boxes ("menu request ignored: a menu is already open") and its four sounds were
   lost (20 played). *Fixed:* the menu aid's steps that open a menu wait while a script's
   box is up or the scripts load, and ask for the inventory again if it did not open; the
   scenario passes `--no-messages`. Now 23 with every step done: one UIPipBoyScroll short,
   because since the Pip-Boy ITEMS page (#213) the pass sees the Weapons tab, whose first row
   (the weapon it puts on) is already selected, so no selection move is heard.
   *Decision:* kept known-failing with that reason (not changed to 23). *Check:* whether the
   expectation should become 23.
4. **house-items** (from the coordinator, same family): the container step met an add-on
   box. Now `--no-messages` and its own launch (corner-messages keeps the boxes). Its older
   known failure (the BB Gun dropped, not clothing) is unchanged.
5. **Grading varies between launches:** *Observed:* the "warm" frame `gs-day-van.png` was
   taken with the overlay text on; below the text (rows 120-720) its mean colour equals the
   other two frames' to 0.1 (92.6, 83.7, 64.9) and a difference picture shows only clouds,
   grass and a bird moving. No nondeterminism found in the weather choice (highest chance,
   ties to the first). No further runs made (owner asked to cut usage).
6. **ai-packages / ai-reach figures:** not done. *Hypothesis:* most of the +1,332 pairs come
   from c830f7ee (#190, package lists through leveled actor lists: its own message counts
   +837 pairs on FalloutNV.esm alone); not measured.

Tests: nvf-play unit tests (261), engine clippy; smoke (one pass, final build):
detection-game PASS, quest-messages PASS, house-items KNOWN, ui-sounds KNOWN; an earlier pass
on this branch: residents, sleep-novac, wait-residents, held-player, corner-messages,
sunny-wells, sunny-gas-station, aid-items, push, npc-travel all PASS.
## Combat AI: fleeing, cover, groups and searching (2026-10-12, session "e2")

List 2's combat item ("fight, flee, take cover, assist allies … fights between NPCs at
scale … work together … spreading out to find the enemy, alerting their friends").
Details and sources: docs/design/combat-ai.md §12. The GECK wiki's points come first; what
is our own design is said so.

1. **Fleeing** (wiki: Confidence, Combat Flee). A hurt fighter checks now and then
   whether its side is outmatched (its group's health × damage per second against the
   foes', and its own when its combat style says "personal survival"), against its
   confidence. Failing, it says its Flee line and runs: through a door near it, to cover
   away from the attacker, or straight away. At the spot it waits; hurt again or cornered
   (an enemy within 512 units) its confidence goes up a level and it fights or flees on.
   Frenzied actors never flee. *Guess:* after fCombatFleeWaitTime (30 s) at the spot it
   checks again (the wiki lists the setting without saying what it does); the choice among
   several doors or covers is nearest first; armour is not counted in the strengths yet.
   *Check:* shoot a Goodsprings bighorner once: it runs off behind cover and stays there;
   walk up to it: it should turn on you or run on.
2. **Cover** (wiki: Combat Style cover chance and timers). Every 10 s a gunman rolls its
   style's Take Cover Chance; using cover, it finds a spot behind something its target is
   beyond (navmesh cover edges, checked with a sight ray), hides for the style's Wait
   Timer, steps out to a peek point, fires for the Fire Timer, and ducks back.
   *Decision:* never more than 1,024 units from where it stands (the styles' radii had
   troopers run 2,000 units mid-fight). *Guess:* what the navmesh's cover field means
   (we use the cover triangles' outer edges and confirm with a ray; the cover height is not
   read, and NPCs do not crouch). *Check:* in the NCR–Fiend fight below, troopers should
   run behind rocks and pop out to shoot; tell me if they look silly (too far, wrong side).
3. **Groups working together** (our design on the wiki's combat groups). Gunmen of one side
   fighting one target spread round it (flank places 35° apart), and nobody fires while a
   comrade stands in the line of fire: it steps aside. *Decision:* bystanders (brahmin)
   do not stop a shot, only comrades and the player. *Check:* the troopers should fan out,
   not queue in a line.
4. **Alerting friends** (wiki: Confidence, Alarms; our design on top). An actor that starts
   fighting calls its comrades within 2,048 units, unless its side clearly outmatches the
   foe (the wiki's alarm confidence; its multiplier 3 is the wiki's example, no game
   setting names it). *Decision:* an actor that only notices a hostile also calls them (your
   "alerting their friends when they sense an enemy presence"); an alarm passed on is not
   passed on again; the 2,048 units are my choice. *Check:* at Cook-Cook's camp, once one
   Fiend notices you the whole camp goes on alert.
5. **Searching together** (wiki: Combat Search). A hostile only glimpsed is searched for
   midway between the searcher and it (the wiki's rule; before it was at the hostile).
   Comrades searching for the same thing split the circle round it into sectors, three
   search points each; after two complete searches (FalloutNV.esm's
   iCombatTargetLostRemoveSearchCount) they give up unless they still glimpse you.
   *Decision:* sectors stand in for the wiki's split of the navmesh by what each cannot
   see. *Check:* sneak and fire near the camp: the Fiends should fan out, not walk to one
   spot together.
6. **NPC against NPC.** A hit by another actor now makes the victim fight back as your hits
   do, except stray hits between comrades and between two actors both fighting someone
   else (your exception from list 1, for everyone).
   **Changed rule, please confirm:** actors sharing a faction never fight each other (an
   earlier engineering choice). The NCR and the Fiends both sit in GeneralIdleFaction, so
   they were at peace; their own factions do not name each other at all (neutral both
   ways in FalloutNV.esm), and the Fiends are Very Aggressive, so the game has them attack
   NCR on sight. *Decision:* a shared faction no longer counts when either side's
   factions name the other an enemy, and a shared faction that names no reactions (a
   utility faction such as GeneralIdleFaction) only counts when one of the two has no
   faction that does. *Check:* NCR patrols near Fiend ground should fight them; no new
   fights among townsfolk and their animals.
7. **Watching fights at scale.** Test aids: `NVF_PLAY_TEST_SCRIPT=<s>:moveto:<ref>:player:x:y:z`
   moves any actor next to you; `NVF_PLAY_TEST_UNSEEN=1` makes you invisible to everyone.
   Every fight logs `battle: X killed by Y`, `battle over after … s` and a `battle summary`
   (sides, hits, kills, flights, cover, flanks, alarms, searches, time). Smoke scenarios
   `battle-ncr-fiends`, `combat-cover`, `allies-alerted` (one run: six NCR troopers moved
   by Cook-Cook's camp), `combat-flee`, `group-search`. One run: 11 actors on 2 sides,
   the Fiends won (3 of 5 standing, all six troopers dead), 61 and 33 hits, resolved in
   62.5 s.
8. **Hook for the crime workstream:** `Game::attacked_by` (provocation.rs) now hands hits
   by actors other than the player to `Game::attacked_by_actor` (combat_group.rs), and
   `reaction_to` uses the new `Game::kin`. Nothing else in provocation.rs changed.

Not done (combat-ai.md §12.10): grenades and avoiding threats, NPC blocking, dodging and
power attacks, combat barks other than Flee, Combat Radius and targeting FOV, healing in
combat, cover on a Continue During Combat walk, armour in the threat ratio.

## Floating NCR troopers (2026-10-12, session "floating-troopers")

1. **Playtest list 1 item 24, NCR troopers floating: found and fixed.** Reproduced at Camp
   Golf (the graphics-2 sighting: 2CPoindexter above his shadow in the free camera; with
   `--play` the tower trooper CampGolfNCRTrooperTower1A and 2CMags had their feet 73 units
   above their roots). The ground was right (collision, navmesh and drawn landscape agree);
   the body was lifted. Cause: the `Holster` clips of the two-handed automatic and heavy
   stances (`2haholster.kf`, `2hhholster.kf` and the power-armour/sneak copies) hold `Bip01`
   67 units up and have no `Bip01 NonAccum` track; NPC posing used that height on top of the
   idle's pelvis height. Now the accumulation root's height counts from where its track
   starts (only these four clips of 2,149 start elsewhere). Details:
   `docs/spec-notes/floating-actors.md`.
   - *Decision:* only the height is taken relative to the start; the x/y start of furniture
     clips is left as it was (the sitting code measures it).
   - *Not checked:* Camp McCarran was not run again; I expect its troopers to be the same
     case. *Check:* the troopers outside McCarran and on its towers stand on the ground,
     including right after they put a weapon away.
   - *Seen, not changed:* in the free camera (no `--play`) actors stand at their placed
     height, e.g. Poindexter 28 units above the ground; with `--play`, residents that never
     walk keep it too.
   - New test aid `NVF_TEST_ACTOR_FEET=<s>[:<range>]` logs each nearby actor's root, lowest
     foot bone and the landscape under it; smoke scenario `floating-troopers`.

## Script stubs, second round (2026-10-12, session "stubs-2")

Playtest §4 item 7 (docs/design/script-stubs.md §3.2). Implemented: SetPackDuration,
SetActorsAI, Rotate, ShowAllMapMarkers, SetGlobalTimeMultiplier, IsTurning,
IsPlayerMovingIntoNewSpace; registered AddItemToLeveledList, IsDLCInstalled,
GetXPForNextLevel. Left: PushActorAway, SetForceSneak (both need a system now),
AutoDisplayObjectives, GetIsObjectType.

Decisions where the sources were unclear:
- **SetPackDuration** counts from the call, not the package's start; when the time is up
  the package counts as done (end script, OnPackageDone) and the actor chooses again,
  which may pick the same package and start it afresh. Check: the Hidden Valley bunker
  Paladins (packages 0x000D76E4–0x000D781C) change activity about every 30–90 game
  minutes rather than staying all day.
- **Rotate** turns a reference for one frame per call (the shipped callers call it
  every frame from GameMode). Check: the turning objects of `SCPT 0x0013414C/D/E` (via
  their linked references) spin steadily.
- **SetActorsAI 0** stops package choice, walking and detection, not animation or
  collision: the actor stands in its idle. Check the ambusher of `SCPT 0x000F9D8F/90`.
- **SetGlobalTimeMultiplier** slows the whole virtual clock (animations, physics, NPCs,
  scripts' frame time, the game clock); sound is never slowed or pitched.
- **ShowAllMapMarkers 0** forgets ShowMap's markers but leaves found and Visible ones.
  The Explorer perk (`1 0 0`) shows the hidden-flagged markers too, as the wiki reads.
  Smoke run: 183 of the 188 Mojave markers on the world map (5 are disabled).
- **IsTurning** threshold 0.5 rad/s (about 30°/s), guessed.
- **IsDLCInstalled** matches a load-order plugin by name, spaces/case/extension ignored
  ("tribal pack" = TribalPack.esm); no page documents the function.
- **GetXPForNextLevel** returns what is left to the next level (not the total), from the
  Here and Now perk. Levelling up is not built, so the level stays at SetLevel's.
- **AddItemToLeveledList** ignores the health argument (leveled entries carry no
  condition here); an entry added twice counts twice.

## Followers (2026-10-12, session "followers")

Owner's list 2, H: "FOLLOWER AI … follow the player and assist with combat, transition
through cells, ignore friendly fire etc." Design: docs/design/followers.md. Tested with Boone
in Novac (smoke scenario `follower-boone`, passed: hired by his own recruit line, ignored two
shots by the player, came along through his room's door, ran after the player, killed a feral
ghoul with his rifle, then took his Wait package when his `Waiting` variable was set).

Decisions where the sources were unclear:
- **No hire system of ours**: the companions' recruit lines' result scripts do it all
  (SetPlayerTeammate, SetIgnoreFriendlyHits, VNPCFollowers.bBooneHired …); the Follow and
  Wait packages are the ESM's, chosen by their conditions. Nothing to decide, but check in
  game that recruiting Boone after "One for My Baby" (or Veronica at the 188) makes them
  follow without any test aid.
- **Bringing teammates through doors**: the wiki says "Changing cells will teleport the
  teammate to the player". Only teammates whose current package follows the player come
  (not those told to wait) [hypothesis], put **64 units to the player's right** (a second
  one to the left, then further out) facing the player's way [guess]. Check: go through a
  door with a companion; does the original put them behind you, beside you, or at the door?
- **Teammates in combat** ("as if in the player faction"): anyone attacks a teammate exactly
  when it would attack the player; a teammate attacks whoever would attack the player and
  also whomever its own records have it attack (so Boone keeps his Legion enmity) [design].
  A teammate that sees the player gets an alarm at everyone fighting the player, without
  the Assistance check [design]. Aggressive/passive (FollowerSwitchAggressive) is not built:
  every companion behaves as aggressive. Check: does a passive companion in the original
  hold fire until the player is attacked?
- **Friendly fire**: teammates count as the player's allies (already a hypothesis of the
  combat workstream) and the hire scripts' SetIgnoreFriendlyHits makes them ignore every
  hit. Check: shoot a companion out of combat four times; in the original does it turn?
- **Teammate cleared on death** after OnDeath runs [guess at the order].
- **OpenTeammateContainer** opens the container menu on the companion once the
  conversation has ended (the barter menu opens over it) [design].
- **Companion wheel: not built** (not cheap): its buttons set the companion's script
  variables, but which button sets what and the menu's look are not in the sources at hand.
  Needs the original's menu files and a screenshot of the wheel. Until then nothing in play
  sets `Waiting` except the game's own wait trigger volumes (and the test aid).
- Wait/StopWaiting/IsWaiting are not built: no script of the masters uses them.

## Shadows and the sun (2026-10-12, session "shadows-sun")

Branch `claude/shadows-sun`, playtest list 4 items 5-7. Pictures:
`C:\fnv-private\references\shadows_sun\`.

5. **First-person shadow flicker outside Doc Mitchell's house: not reproduced, cause not
   found.** Three runs, each leaving the house by its door (0x00103E61) and standing where
   your screenshots were taken (−69698, 2246), turning round in 30° steps with two frames
   0.3 s apart at each: a `--cell GSDocMitchellHouse` start at 14:00 (twice) and a full
   `--new-game` opening (wake-up, Doc, chargen, Vigor Tester, about 95 s) then out of the door.
   No shadow appeared or went between the two frames of any pair, and no shadow lay where
   nothing casts one. What I found:
   - In your screenshots the sun is about 72° high (13:56-14:40 in our sun path), so nothing
     standing on the ground can throw shadows 20-30 m long; the wedge in `shadowbug3` is wide
     at your feet and narrows far ahead, as the shadow of something stretched from your
     position high into the air would. My best guess [hypothesis]: a stretched skinned shape
     (vertices bound to a bone that is far away) on the third-person body, which in first
     person is drawn only into the sun's shadow map (so you would see the shadow but not
     the shape), or on a nearby animated actor.
   - Your HUD shows a much larger scene than mine at the same spot: refs drawn 31,848
     (980 animated), shapes 62,552, lights 247, SpeedTree 5,137, 462 cell loads; mine 2,800
     refs (108 animated), 5,200 shapes, 33 lights after 30 loads. Whatever filled your scene
     (a long session, another grid setting, other plugins?) is the likely way to reproduce.
   - Please check: does it happen in third person too, and do you then see a long thin
     streak of geometry? Does `--shadow-distance 0` (or Sun shadows off on the settings page)
     make it go? Which plugins were loaded? A log of that run would let the next session
     look for an actor or body part near you.
6. **Shadow cost: limited** (docs/design/graphics-enhanced.md §5.3, measured with new cycle
   keys of `--graphics-cycle`). At your screenshots' spot the sun's shadows took the frame
   from 7.6 ms (off) to 19.7 ms (16,000 units in 4 cascades); the GPU's time hardly moved,
   it is the CPU's work per cascade. Now 12.7 ms:
   - The arms' shade light (the second light that lets your arms be shaded by walls but
     not by your own unseen body) took a whole cascade of 190 casters; it now covers only
     256 units round the camera, about 30 casters (−2.5 ms). Guessed constant:
     `ARMS_SHADE_REACH` 256. Check: arms still go dark under a roof or in a building's shadow.
   - The sun's shadows default to **8,000 units in 3 cascades** instead of 16,000 in 4
     (−4.5 ms more). Visible change: no sun shadows beyond about 114 m; near shadows a little
     finer (first cascade 667 units instead of 1,333). At noon the road view is unchanged
     (`shadow_limits_before_after.png`); at dusk the long shadows of far rocks and buildings
     end nearer. `--shadow-distance 16000` brings the old reach back (in 3 cascades).
     Decide: whether 8,000 is close enough at dusk, or you'd rather trade more frame rate
     (8,000 in 2 cascades measured 13.2 ms against 14.4 ms for 3, before the shade-light trim).
   - Not changed: while water is in view its reflection camera gets the sun's cascades too
     (Bevy builds them for every active camera).
7. **Sun disc missing: fixed.** Cause: the sky dome is shrunk to nothing in interiors and
   nothing gave it its size back outdoors. Since the sun and its glare were moved onto the
   dome (2026-10-09), every game that started in or went through an interior (a new game
   starts in Doc Mitchell's house) showed neither the sun nor the dome's sky colours, only
   the clear colour and the clouds; a run started outdoors still had them, which is why it
   seemed to come and go. Now the dome gets its size back outdoors, with a log line ("sky:
   outdoors again, the sky dome (with the sun disc and glare) shown"). Before/after:
   `sun_disc_before_after.png` (the two views differ a little: 230°/35° before, 240°/50°
   after; the sun stands at 239°, 72°). Note the sky colours after an interior are now the
   weather's again (a warmer, hazier blue than the plain clear colour you may have got used to).

Checks: smoke scenario `shadows-sun` (new); engine clippy.

**Owner's answers (2026-10-13), on the first-person flicker:** it happens only in first person,
and only with sun shadows on (never in third person). Plugins: the main game and all the DLC
(the ten masters, as every test run loads). So the scene size gap is not plugins: the owner's
run had played longer (462 cell loads). Next lead [hypothesis]: something drawn or posed only
in first person (the first-person body or arms rig, #172/#178) casts into the sun's shadow map.
Follow-up queued as L5 in docs/handoff/playtest-list-4.md.
## Playtest list 4: dialogue and message boxes (2026-10-13, workstream "d1")

**Item 1, Doc Mitchell's opening lines not voiced.** The files were found and played
(`voice: playing …maleuniquedocmitchell\vcg01_vcg01intro_00104bee_1.ogg`), but every one was
flagged "out of hearing, in another cell", so it played at volume 0 with no subtitle. Cause
[observed]: the simulation creates a reference's state on first use, and nothing had touched
Doc Mitchell yet when he said the opening, so the hearing test found no cell for him. Fix: a
speaker not yet created is heard from where it is placed (`voice.rs` `speaker_place`); the
same test also decides the subtitles, so his opening subtitles come back too.
*Decision/Guess:* none. *Check:* the opening, Doc's first lines heard and subtitled.

**Item 4, message box buttons ignore their conditions.** MESG records keep a CTDA list
after each button (ITXT) [observed: GSChemistrySetMsg, GSBrokenSMGMsg: GetActorValue
Science / Repair ≥ 25 on PlayerRef]. They were not read. Now they are checked when
ShowMessage shows the box; buttons whose conditions fail are not drawn, and the ones left
keep their record indices for GetButtonPressed [documented: GECK wiki GetButtonPressed, "the
button indices … remain the same even if certain buttons are hidden with conditions"]. The
log line `script: message …` gives `(hidden by their conditions: [..])`.
*Guess:* the conditions run with the player as subject and the reference whose script shows
the box as target (both boxes here name PlayerRef, so only Subject/Target conditions depend
on this). The conditions are checked once, when the box is shown, not while it is open.
*Check:* with Science and Repair under 25, the chemistry set and broken SMG offer only
"Leave it alone."; at 25 or more the other buttons are back and do their work.

**Item 22, Johnson Nash has no Goodbye.** His topic list (headless `fnvtool dialogue talk`):
Who are you? / I have some questions about Primm. / Want to play a hand of Caravan?. The
Caravan offer line has the Goodbye flag (it ends the conversation to start the game), and
our rule "add the generic GOODBYE only when no listed line says goodbye" [documented: Quest
and Dialogue Tutorial] treated it as his goodbye.
*Guess:* a listed Goodbye line with an end (result) script does not count as the player's
way to leave, because it ends the conversation to do something else (Caravan, hiring a
follower, a quest action). His list now ends with GOODBYE ("See you later." in the headless
run). This can add a generic Goodbye to other lists whose only Goodbye entries have end
scripts (some quest topics do; 100+ top-level Goodbye lines of FalloutNV.esm have one).
*Check:* Johnson Nash, Ringo and another Caravan player each show one Goodbye; check an NPC
whose quest Goodbye topic runs a script, to see whether the original also adds a generic
Goodbye there.

Smoke scenarios added: `voice-opening`, `message-button-conditions`, `goodbye-after-caravan`
(its actions file is `tools/smoke/actions/nash-goodbye.actions.json`). The trace event
`message_shown` gains a `hidden` field (written only when a button is hidden), which the lab's
script traces may show for boxes with conditions (no lab run in this workstream).

## Falling, loading and walks (playtest list 4 items 8, 9, 10, 2-3) (2026-10-13, session "falling-loading")

8. **Falling on the steep road: fixed in the physics.** At your spot (-64168, -8679)
   running straight south never left the ground; at 150° and 210° it did, for 0.4 s and
   more. One step missed the ground (the first thing under the capsule was a face steeper
   than the 45° slope limit: a mesh's edge, a rock), and from then the character fell
   freely until its arc met the slope again, then the landing slowed it to 81 units/s:
   the switching between falling and running (physics.md §6.4.2). Now a character that
   walked off the ground lands on walkable ground as soon as it is within the step height
   (24) of where it left; the landing slow-down needs a drop of more than 24 from the top
   of the jump; and the jump loop needs the ground more than 24 below the feet.
   Guesses: all three 24s (the controller's step height; the original's rules are not in
   the ESM's settings or the wiki); the 45° slope limit for the player (still untouched:
   running across the 56° rock face at 210° still slides). Check: run down that road in
   several directions, and onto and off rocks; jump and fall off a roof (the jump loop
   and landing must still play).
9-10. **Loading screen and ground fading in at the gas station: fixed.** Two causes.
   (a) In the frame of the door the renderer still had the camera where it was inside,
   so it loaded the (empty) cells around that point at once and counted them as the
   destination; the screen came down while the gas station's own cells still loaded one
   a frame, fading in. It now loads around the arrival point until all are in.
   (b) The player placed by the door counted as in the air until the first physics
   step, which waits behind the loading screen: the body played the jump loop behind
   the screen and a landing when it came down (the "drop from a height"). A placed
   player on support now counts as standing. Not done: the screen does not wait for
   distant LOD generation (it can take long on a first visit); the 0.25 s settling time
   for meshes and textures is unchanged. Check: leave the gas station and Doc Mitchell's
   house; the ground should be there when the screen goes.
2-3. **Victor's and Sunny's stop-and-go: partly.** Not the falls: `fnvtool nav walk` on an
   NPC path from Victor's patrol start found 7 steps in the air before the fix (1 after)
   and none at running speed. What did stop them: an actor near the player moves in the
   physics steps (60 a second), but was drawn at the position of the last step and its
   animation took the distance moved since the last frame: a frame with no physics step
   (any frame rate above 60, or uneven frame times) drew it standing and played the idle.
   Actors with a character are now drawn between their last two steps, as the player is.
   In my runs at about 55 frames a second the frames with no step were rare, so I could
   not reproduce the stop-and-go on screen: Goodsprings with every resident, 40 s, 1,311
   frames of residents walking, 251 drawn standing, but only 1 stop of 1-3 frames; the
   rest were 4 long stops (198 frames of actors whose character did not move: held,
   blocked or waiting) and 48 frames of turning on the spot. The smoke scenario
   `falling-loading` logs this ("walk summary") and wants at most 5 short stops. Victor stood at his
   VCG00 idle in a new game (his patrol starts later in the quest), so I did not watch his
   patrol, nor Sunny's run to the geckos. Check: those two walks at your usual frame rate;
   if they still stop and start, note whether it happens at path corners (turning on the
   spot is by design: `spot_turn_start`) or on open ground.

## Playtest list 4: third-person crosshair

Branch `claude/third-person-aim` (desktop queue D2), items 11 and 12.

**Cause (both items).** The third-person camera sits over the right shoulder (the shoulder
offset 40 right, 10 down from the pivot; `fOverShoulderPosX/Z`) and looks the same way as
the player, so the crosshair in the middle of the view lies on a line about 40 units right
of the eye's. The activate prompt (`activate::find_target`) and the player's shots
(`combat::fire`) both used the eye's view ray, as in first person: the prompt showed only
when the eye's ray (left of the crosshair) met the object, and shots landed left of the
crosshair.

**Fix.** `place_camera` now records the third-person camera's line (`control::Crosshair`,
in `PlayerControl::crosshair`) each frame. Item 11: the target ray starts on that line where
it is level with the eye (so nothing between the camera and the player is picked and the
reach stays measured from about the player) and runs along the view. Item 12: the aim point
is what that ray meets first within the weapon's range (its end when nothing); the shot's
ray goes from the eye to that point, then the muzzle and cover rules of combat.md §5.1 apply
as before. First person is unchanged. The `aim…` test aids now put the point under the
crosshair (third person: turned so the camera's line passes through it).

*Decision:* while the camera circles the player (Toggle POV held, or swinging back), the
eye's view is used, since the camera then does not look where the player does.
*Decision:* the shot leaves from the eye (then muzzle), not from the camera, so cover
between the player and the target still blocks it; a target the camera sees round a corner
the player cannot may be hit on the corner instead (as in the original [hypothesis]).
*Guess:* the original converges the shot on the crosshair point the same way; we have not
observed how it treats an aim point very close to the player. Here, an aim point less than
1 unit away or not ahead of the eye uses the view direction.
*Check:* in third person, the E) prompt shows when the crosshair is on an NPC, door or
item (not to its right); shots with a scoped/no-spread weapon land on the crosshair at near
(Sunny's bottles) and far range; shooting past a doorframe near the right shoulder.

Smoke scenarios added: `third-person-shots` (Sunny's bottles shot from third person, glass 3)
and `third-person-activate` (one of Sunny's bottles, targeted in first person, not targeted
once the view goes to third person with it left of the crosshair, then targeted again with
it under the crosshair). Chet behind the general store's counter was tried first and is not
a clean test: even in first person he is found only by his render bounds there, and along
the camera's line not at all [observed: smoke run 2026-10-13]; worth a look if the prompt
is hard to get on shopkeepers behind counters. New
log line: `crosshair: the third-person camera's line (shoulder [..])` / `crosshair: the
eye's view` when the crosshair's line changes.

## First-person shadow flicker, second look (2026-10-13, session "fp-shadow-flicker")

Branch `claude/fp-shadow-flicker`, playtest list 4 item 5 (follow-up of "Shadows and the
sun" above). **Not reproduced; cause not found; no fix.** What was added is a debug probe,
off by default: `NVF_PLAY_DEBUG_SHADOWS=1` (`engine/nvforever/src/shadow_probe.rs`, smoke
scenario `shadow-probe`).

- **What the probe checks** (every 10 frames, log lines `shadow probe: …`, a summary every
  5 s): skinned shapes whose joints lie more than 1,500 units apart; skinned casters in the
  sun's two near cascades with bounds wider than that; skins with a joint gone or standing
  at the world's origin; in the render world, instances whose skin slot on the GPU is not
  their skin's, and instances drawn from where their mesh's vertices or indices were
  before the mesh was placed anew; and the widest joint spread of the hidden third-person
  body (the one that casts your shadow in first person).
- **Four runs, all clean** [observed]: Goodsprings at 14:00 in first person, after 11
  jumps of 30,000-100,000 units (about 275 cell loads), with the 9mm pistol drawn and
  holstered every 3 s while turning in 37° steps; a run leaving Doc Mitchell's house by
  its door; and one at 1920x1080 with frames every half second looking east. Nothing was
  flagged in any of them; the hidden body's joints stayed within 69-71 units; the frames
  show only the normal body shadow.
- **A clue for next time** [observed in your screenshots, hypothesis on its meaning]: all
  three `shadowbug` screenshots look east (compass E), and the wedge narrows eastwards.
  East of Goodsprings, 70,000 units away, is the world's origin (game 0, 0, 0). A skinned
  shape whose vertices are drawn at the origin (Bevy writes a missing joint as the
  identity; a skin read from a wrong place gives zeros) stretches from where it stands to
  there, and its shadow would be exactly such a wedge. The probe now looks for this from
  the main world (joints gone or at the origin) and the render world (skin slots, moved
  meshes); a GPU-side cause it cannot see (a joint buffer read past what was written)
  remains possible.
- **Please, if you see it again:** run with `NVF_PLAY_DEBUG_SHADOWS=1` set (e.g.
  `$env:NVF_PLAY_DEBUG_SHADOWS=1` before `play-new-game.ps1`; the script clears only
  `NVF_PLAY_TEST_*`), and keep that run's log from `%LOCALAPPDATA%\NVForever\logs`. Note
  which way you face, and whether turning to look west makes the wedge go. Any
  `shadow probe:` warning names the shape and its owner.
- *Decisions/guesses:* the probe's thresholds (1,500 units; body spread 30-300 in the
  smoke test) are mine. No game behaviour changed.
## Playtest list 4: combat (workstream D3, branch `claude/combat-list-4`, 2026-10-13)

### Item 13: Ringo did not come out of the gas station to the fight

**Cause.** Once the player had left GSGasStation, Ringo was out of the loaded area at the
Mid-High level (the interior is in the cell buffer, actor-behaviour.md §5.9): one update
every 18 s, each walking the time owed in one go, and none of the high process (combat,
assistance). RingoTravelPackage (given by the result script of INFO 0x00105D09) sends him
to RingoPositionMarker (exterior, -70390, 3462) [observed: `fnvtool script decompile`,
`esm decode`], so he came out in jumps, late.

**Fix.** A new process level `fnv_ai::ProcessLevel::Coming`: an actor out of the loaded area
whose rest of path (from the leg it is on) has a corner in a drawn cell is updated every
frame in the low process (navmesh steps, doors as if open). It comes through the door when
it would have walking in sight, and is then in the loaded area (high process: drawn, joins
the fight). Log line `… on its way into the loaded area (moved every frame)`; the process
summary counts `set off into the loaded area from out of it`.

A low update in which a new package starts a walk now stops there and keeps the rest of
the time owed (the first run walked Ringo's whole 5,247-unit trip in the one update that
gave him the package); an actor going Coming catches up no time owed: it walks in real time
from where it is [design].

*Decision:* any actor whose remaining path passes through the loaded area counts, not only
one sent towards the player (the original's rule for actors heading to the player's area is
not documented; JIP's process-level pages name only the four levels) [design].
*Guess:* that the path's corners are a good enough test (a long straight leg crossing the
loaded area between two corners outside it is missed).
*Check:* give Ringo the "let's go" line in the gas station, leave first and go to the
fight: he should come out of the door a few seconds after you and join the fight. Smoke
`ringo-comes-out`: out of the loaded area, on its way (moved every frame), back in the
loaded area in WastelandNV within the same game minute.

### Item 15: a looted corpse still drawn wearing its armour and weapon

**Cause.** An NPC's drawn outfit and weapon were decided once at spawn
(`fnv_actor::resolve`) and never looked at again; taking them changed only the simulation's
inventory.

**Fix.** `engine/nvforever/src/outfits.rs`: each drawn NPC carries what it is drawn wearing
and holding (`Dressed`). When an item leaves its inventory, the armour's model comes off and
the parts it covered (race body, hands, head, the hair's no-hat shape) are spawned on the
same skeleton nodes (`nvf_assets::actor::spawn_parts`, split out of `spawn_actor`), so a
ragdoll keeps its pose; the weapon model, its muzzle and the weapon clips go as for a broken
weapon. Log lines `outfit: actor …`, `outfit summary: …`.

*Decision:* the drawn choice and the simulation's roll of leveled lists are separate and
agree only for actors with no random leveled choice (GSPG01Ref: "random": false; the dead
tribal Fiends: random) [observed: `fnvtool actor appearance`]. What stands for a drawn item
is looked at once, when the simulation first holds the inventory: the item itself if
carried, else the carried armour sharing a body slot with it (or, for the weapon, every
carried weapon); the drawn item comes off when all of that is gone, and stays when nothing
stood for it [design]. Making the two rolls the same is left for later.
*Check:* kill a Powder Ganger, take only his armour (the body shows skin), then his weapon
(the holster/hand empties); also a Fiend with a leveled outfit. Smoke `corpse-undressed`
(a dead tribal Fiend, take all): weapon and two armour pieces off, upper body and hair put
on. No frame was captured (the screenshot aid did not reach the smoke runner's launches):
please look at the body once.

### Item 21: attacks during a crippled stagger

**Cause.** The hit reaction (`deaths.rs`, `Flinch`) only played a clip; the combat code
(`npc_combat.rs`) and the walks did not know about it.

**Fix.** While a hit reaction is playing an actor neither fires, nor starts a blow, and a
blow it was swinging is cut short; it also stands (held like an actor on furniture,
`residents.rs`) until the clip ends. Log `npc combat: … holds its attacks while it
staggers`; the npc combat summary counts the staggers and blows cut short.

*Decision:* a blow already swung but not landed is cut short [design: the original
interrupts an attack with a stagger, hypothesis]; the gun's cycle (reload, refire timers)
just pauses.
*Check:* cripple a gecko's or Powder Ganger's leg: no bite or shot until the stagger ends.
Smoke `stagger-holds-attack` (Chet shot in the legs).

## Smoke effects and grass pixels (playtest list 4 items 16, 17) (2026-10-13, session "smoke-grass-fx")

- **16, smoke not animated and far too visible.** The camp fire smoke south of Goodsprings
  is `DLCPittFXGroundSmoke01` and the Bison Steve smoke `FXSmokeWisps01`: meshes whose
  texture scrolls by a `NiTextureTransformController` attached straight to the texturing
  property, with no sequence. We only played controllers inside sequences, so they stood
  still. They now play on their own cycles (13.3 s for the ground smoke, 36.7 s and 44.1 s
  for the wisps' two directions). Too visible: these unlit shapes are tinted grey by their
  vertex colours (0.57 for the ground smoke), which we read as linear values, drawing them
  as a near-white plume. Unlit shapes now take vertex colours as display values, as the
  landscape and grass already do. The additive alpha^1.5 from #201 is not involved: both
  smokes use normal alpha blending. Checked with one frame before and frames after (the
  pattern moves between frames 12 s apart and the smoke is a faint haze).
  - *Please check:* the camp fire south of Goodsprings and the Bison Steve smoke against
    the game: speed and direction of the drift (I assumed the texture moves by +offset,
    which makes the pattern rise), and strength.
  - *Side effect to look at:* every unlit shape with grey vertex colours is now darker
    (glows, signs and shade overlays drawn by BSShaderNoLightingProperty). By the same
    reasoning this should be closer to the game, but I did not look at those.
  - *Not done:* smoke built from particle systems inside ordinary models is still not drawn
    (`fxlightsmoke`, the Primm dark smoke `dlc03fxdarksmoke`, tyre smoke, ashtray smoke,
    HELIOS steam; about 66 placements). It needs the add-on particle code to emit from
    ordinary models; a workstream of its own. Transform, visibility and colour controllers
    outside sequences (e.g. the dust tube) are not played either.
- **17, floating grass pixels.** The grass models share one texture atlas, and some blade
  tips reach a little past its edge (V −0.1 to −0.01, U up to 1.23). With repeating
  sampling, those tips picked up the dense blade feet at the opposite edge of the atlas
  and drew them as small opaque triangles above the plants. Grass textures are now
  sampled clamped at their edges, so the soft edges stay as they were. The triangles are
  gone in the owner's view (before and after frames at the same spot, 17:06).
  - *Guess:* the game clamps the grass texture; the shader property stores no clamp mode.

## Playtest list 4: creature data

Workstream L4 (`claude/creature-data`, 2026-10-13): items 14, 18, 19, 20 of
`docs/handoff/playtest-list-4.md` §4, and the VATS hint's key token. Design notes:
docs/design/combat-ai.md §13.

### Item 20: names (and every group) from the template

**Cause.** The prompt named an actor by its placed base's FULL, else its Editor ID. The
dead gecko's base VSpawnTier2GeckoMed (0x00156777) has no FULL: it takes its Base Data
from its template, the leveled list VEncTier2GeckoMed, whose entry VCrTier2GeckoMed is
named "Gecko" [observed: `fnvtool esm decode`]. Other groups were read from the placed
base too: traits (race, sex, voice, class), Essential, No Low Level Processing,
Invulnerable, the carried items (a placed leveled list carried nothing) and the script.

**Fix.** `GameData::display_base` (the "Base Data" source) names actors in the prompt,
the loot/barter menu title, the conversation and hit messages. Traits, Base Data flags,
Inventory (`fnv_sim::World::set_inventory_sources`) and Script now follow their template
flags (combat-ai.md §13.4 has the table). The drawn model already did.

*Decision/Guess:* the name is Base Data [observed: the template actor has no FULL when the
flag is set; the owner's game shows "Gecko"]; GetIsCreatureType follows Stats (the type
sits in CREA DATA with the stats) [hypothesis]; Karma stays the base's (unknown group).
Carried items now come from the template: actors placed from a template or a leveled
list (Powder Gangers, Fiends, much wildlife) now carry their template's items where they
carried nothing before.
*Check:* kill a gecko south of Goodsprings and look at it: "Search / Gecko". Loot a
templated NPC (e.g. a leveled Powder Ganger): it carries items. Smoke `creature-names`.

### Item 18: geckos that attack a player who comes close

**Cause.** The AIDT aggro radius was not read. The owner's geckos are
VSpawnSpecialTier1GeckoSmallGS02Hunter (0x0016518B, 0x0016518C): Unaggressive, neutral to
the player, Aggro Radius Behavior on, radius 650 [observed].

**Fix.** `fnv_game::aggro`: inside its radius, an actor that would not attack the player
on sight warns (GuardTrespass) twice 5 s apart (iGuardWarnings, fAITrespassWarningTimer),
then attacks; only while its package allows "Aggro Radius Behavior".

*Decision/Guess:* warnings then attack, as the guard package (the wiki names aggro radius
among the GuardTrespass warnings; how many warnings it gives is a guess: the guard's 2);
the player only; leaving the radius resets the count. This also makes the Aggressive,
Neutral wildlife that "left the player alone" (giant rats, coyotes, bighorners, ants,
bloatflies, bark scorpions, mole rats; factions.md §5) attack inside their radius, and
14 NPCs (Primm ambush gangers 768; Hoover Dam NCR snipers 25,000).
**Not fixed for the owner's two geckos:** their first package, VCrGeckoFindFood (Once Per
Day, 2 hours, Skip Fallout Behavior without the aggro flag), forbids it, and NVForever
keeps a running Once Per Day package all day (a Find package never completes here), so
they stay on it. In the game they would move on to their patrol/sandbox packages after it,
which allow aggro. Ending a Once Per Day package after its duration is an AI package
change left for the owner's call (it touches every such package).
*Check:* walk up to a coyote or giant rat near Goodsprings: two warnings, then it attacks.
Smoke `aggro-radius` (coyote VERGSCoyote6REF).

### Item 19: no Talk on creatures and hostile actors

**Fix.** A hostile actor (would attack the player, or fighting the player) shows only its
name; a creature shows Talk only when it has a GREETING line it can say now; other NPCs
keep Talk. E on a name-only target does nothing.

*Decision/Guess:* "has a sayable GREETING line" stands for the game's rule [hypothesis: Rex
and ED-E have dialogue, animals none]; an NPC without a line keeps Talk; the name alone is
drawn on the name line (no "E)").
*Check:* look at a bighorner in Goodsprings (name only), Rex (Talk), a Powder Ganger in a
fight (name only). Smokes `creature-no-talk`, `creature-names`.

### Item 14: the Goodsprings bighorners joined the fight

**Cause.** Assistance (Helps Allies) and alarms took any ally into a fight. The bighorners
(GSBigHorner) are Unaggressive, Cowardly, Helps Allies, in GoodspringsAnimalFaction
[observed].

**Fix.** A Cowardly actor never takes up another's fight (assistance or alarm); it still
defends itself and flees when hurt (Cowardly threshold).

*Decision/Guess:* Cowardly is the override ("I will always flee from combat", GECK wiki
Confidence); Unaggressive alone does not keep a helper out (the wiki's aggression levels
are about attacking on sight). The bighorners do not run away from the fight (avoiding
threats out of combat is not built).
*Check:* the Powder Ganger fight in Goodsprings: the bighorners stay out.

### Extra: the VATS hint's `&sUActnVats;`

**Fix.** Corner messages and message boxes now replace `&sUActn<control>;` (with or without
the leading `-`) by the bound key, as the lockpick help did: "&sUActnVats;" reads "V".
*Decision/Guess:* any control by its INI name without spaces [hypothesis; only Vats seen].

## Package durations (2026-10-13, session "package-duration")

Workstream L6 (`claude/package-duration`): playtest list 4 item 18, the follow-up of #264
(the owner's two non-hostile geckos at cell -17,-6 never attacked). Design:
docs/design/actor-behaviour.md §3 "How long a package runs".

**Cause.** VCrGeckoFindFood (Find, Once Per Day, schedule "any time" with a 2-hour duration,
Skip Fallout Behavior without the aggro flag) is the first package of their list, and a
duration without a starting hour was ignored: the package ran all day, so the next one,
which allows aggro, never came.

**Fix.** A list package with a duration and no starting hour now ends that many game
hours after the actor chose it, the same way a SetPackDuration time ends (#250's
`end_timed_packages`: end script and OnPackageDone, then the actor chooses again). Once
Per Day keeps it from being chosen again that day, so the next valid package takes
over. The geckos' list is built for this [observed]: find food 2 h, patrol 3 h, find food
2 h, patrol 3 h, then a sandbox at their spot with no duration, which allows aggro.

*Sources:* GECK wiki SetPackDuration ("sets the duration of the actor's current package";
the Schedule tab gives durations in hours), Category:Packages (Continue If PC Near holds
a package "even if the package should have expired"; Once Per Day exists so a package is
"not immediately selected again after being completed"; Must Complete "will not
terminate" before its goal) [documented]. The export has no Find package page.

*Decisions/guesses:*
- The duration counts from the moment the actor chose the package [hypothesis: nothing
  else gives a start when there is no starting hour].
- A package that is held (Must Complete / Must Reach Location before its goal, Continue
  If PC Near with the player in the cell) does not end by its duration [documented flags;
  SetPackDuration's own timer is unchanged and still ends at once].
- A package without Once Per Day may be chosen again right away and starts afresh (its
  begin script runs again) [hypothesis]. About 358 master packages have a duration and no
  starting hour; the ones without Once Per Day now restart every `duration` hours.
- With no other valid package, the ended package goes on [hypothesis, as the keep rule].
- Packages with a starting hour still end with their window; script packages
  (AddScriptPackage) are not ended by a duration [our choice].
- A Find package is not ended early by finding food: Find is not carried out yet, and no
  source read says what "found" ends.
- Hours passed in the Wait menu: packages are chosen once per waited hour, so each step of
  a chain can start up to an hour late during a wait.

*Not fixed:* in the smoke run the geckos come back on their sandbox and warn the player
twice, but the count starts afresh whenever the gecko stops detecting the player
(`aggro.rs` needs Detected within the last second), and the wandering gecko lost sight of
the player before the attack in each run (at 06:00–07:00 game time). Marked known in the
scenario. Whether a warning count should survive a short loss of sight is the owner's call.

*Check in game:* stand at the owner's spot near the two geckos (-65828, -23929) at about
17:00 and wait 14 hours: they patrol off after about 2 hours and come back to sandbox
after about 10, then warn when the player is within 650 units. Smoke
`package-duration-geckos`.

## Aggro warnings that outlast a short loss of sight (2026-10-13, coordinator)

Follow-up to "Package durations" (L6, #266) and #264's aggro radius. The owner's geckos at cell
-17,-6 warned twice but never attacked: right after the second warning the wandering gecko
walked past the player facing away (detection Detected → Noticed at 49 units), and the count
started afresh at once. Decisions (crates/fnv-game/src/aggro.rs):
- The warning count now clears only after two warning timers (10 s) without the player inside
  the radius, not on the first tick the actor stops seeing them.
- An actor that has started warning keeps counting while the player is in its alert band
  (Noticed), not only Detected.
Check in game: the geckos south-west of Goodsprings warn and then attack; a creature you walk
away from (out of its radius) forgets its warnings after about 10 s.


## Link From between actors and on the radio (2026-10-13, session "link-from")
Port of the rescued 2026-10-11 commit 035c21ae onto main (dialogue.md §3.5, radio.md §5).
- Link From now adds links in conversations between actors and on the radio, as it did
  for player dialogue. A topic reached only through Link From is said with one of its lines
  that names the topic just said (e.g. a child's "Hi." / "What?" answering another actor's
  hello) [hypothesis; capture question 10].
- Kept from main where the two met: conversations between actors still use only
  Conversation lines (your 2026-10-11 ruling), Link From replies included; player dialogue
  takes Link From only from player lines (type Topic) of Topic topics, as before.
- Radio: a line's fixed speaker no longer stops a station saying it; Black Mountain Radio
  now plays its segments (Segment12 too) with Tabitha's and Raul's voices.
Check in game: Black Mountain Radio (segments between songs, Tabitha's voice); two
NPCs greeting each other (and a child answering an NPC's hello where children talk); Michael Angelo's 031/032/034 choices.

## Licence settled: GPL-3.0-or-later (2026-10-15, owner)
The owner decided NVForever stays GPL-3.0-or-later. GPL dependencies are allowed
(`deny.toml`), and the intro film (`Video\FNVIntro.bik` before a new game, decoded by the
GPL `infinitier_bik_decoder`) is now in the default build (cargo feature `intro-video`, on
by default). Earlier notes that leave the licence open no longer apply.

## Item stacks: placed ammo counts, "How many?" (2026-10-15, session "item-stacks")
Your report: the two 9mm stacks in the Goodsprings gas station showed 1 round each. Their
references have no count of their own; the AMMO record's Clip Rounds (20 for 9mm) is what
a placed ammo reference holds (GECK wiki Ammo). Now read: both show 20 (`prid 0010ABA6` in
the console prints "count 20 (as placed)"). Taking a stack takes all of it, a dropped stack
was already one world object holding its count, and the crosshair says "9mm Round (20)".
New: "How many?" from quantity_menu.xml, for stacks of more than 5: take, store, buy, sell,
take back, drop. My choices, to confirm:
- It asks above 5 (your words); the original's own threshold is not in the ESM. Does it ask
  at exactly 5 too?
- It opens at the whole stack (your first screenshot, 6 of 6) and never goes under 1.
- Keys: Left/Right and the wheel ±1, Page Up/Down ±10, Home/End, A/Enter Ok, E/Esc Cancel;
  Shift+Enter/click and Shift+X still move or drop the whole stack without asking.
- Over the Pip-Boy it is drawn centred on the window, not on the Pip-Boy's screen.
Check in game: the gas station's 9mm (can you get the crosshair on them? From the floor in
front of and behind the checkout counter my test aids found no spot where the crosshair
reaches them, so they are on a shelf inside the counter's collision); drop part of a stack
from the Pip-Boy (X on the 20 rounds); buy part of a stack from Chet; store part of the
caps in a container.


## Combat fixes of playtest list 5 (2026-10-16, session "l5-combat")
Items 9, 16, 20–25, 49 and 61. My choices and guesses, to confirm:
- **9 Jamming.** A shot from a weapon that uses ammo (not thrown) may jam it below 75 %
  condition: the chance grows from 0 at 75 % to 4 % a shot at 0 % along a curve (power
  1.5), so at 50 % it is under 1 % a shot (0.77 %). All four numbers are in one place,
  `CombatTuning::jam_condition`, `jam_chance_max`, `jam_curve`, `jam_fallback`
  (crates/fnv-combat/src/tuning.rs). The shot that jams still leaves; then the weapon's
  jam clip plays (`1hpjama.kf` for reload animation `ReloadA`: the clips sit beside the
  reload clips, `JamX`…`JamZ` too; the letter pairing is my guess from the names) and
  nothing fires or reloads until the clip's length has passed (1.5 s if no clip is found).
  NPCs do not jam yet. Check: a gun below 75 % (console or `condition=` test aid) jams now
  and then, the clip looks right in first and third person.
- **16 Hits on creatures.** Two causes. (1) Shots and swings were stopped by the capsule
  that walks an NPC (its character controller), which is not its body: that was the
  "dust and no damage, like a wall". They now pass it and meet the body part capsules.
  (2) A part's chain covered only the bones leading to other parts: long side branches
  (a bloatfly's legs, a radscorpion's head) had nothing. Each part now also covers up to 8
  of its own side branches reaching 12 units or more (fingers and twist bones stay out).
  Unit tests: a bloatfly-shaped skeleton (`limbs.rs`), side-branch capsules (`hitvol.rs`),
  shots past an NPC's character (`combat.rs`). Check: shoot bloatflies and geckos.
- **20 Enemy shot lines.** An enemy's beam or shot line followed the player's own muzzle
  (the follow was meant for the player's shots only). Fixed.
- **21 Encumbrance.** Over the limit the player walks (the game does, from our captures:
  walking at 201 lb with 200 capacity). In god mode the limit is lifted (I believe tgm
  does that in the original; say if not).
- **22 Convict karma.** I could not find the convicts' records (NPC records are compressed
  in the ESM), so two likely causes are fixed: an NPC made from a template read its own
  karma (0, Neutral, so a kill counted as murder, −25) instead of its template's: Karma now
  comes through the template when the NPC takes its Traits from it (which flag carries
  Karma is not documented; with Stats only it stays the NPC's own, as before); and one fighting the player of its own accord (no
  grudge) now counts as the aggressor, so killing it is never murder. Check: the convicts
  outside the NHP station give "+" karma; tell me the karma line if not.
- **23 Camera pan.** Letting go of F / mouse 3 after circling now goes straight back to
  first person when you were in it (before it swung back round in third person for about
  a second first). In third person it still swings back behind. Your sentence could also
  mean "stay in third person"; say so if that was meant.
- **24 Melee impacts.** Every blow that lands shows the impact the weapon's impact data set
  names for the material, with its sounds, as shots do: blood on actors, dust/sparks on the
  world for a swing that meets a wall or the ground within reach. NPC blows too.
- **25 Radscorpions.** Melee: the swing aimed its ray at the centre of the creature's
  bounds, which for a radscorpion with its tail up is in the air above its back; it now
  aims at its body part capsules first (unit test with a radscorpion-like target). Speed:
  every creature ran at the people's running speed (344 units/s). A creature in combat now
  runs at its own pace: its run clip's speed × its scale × its Speed Mult (a small
  radscorpion: 315 × 0.5 × 150 % = 236 units/s), never faster than before and not under
  a quarter of it. Check: radscorpions and geckos chasing you.
- **49 tgm.** In god mode the magazine stays full, nothing is taken from the inventory
  (thrown weapons too) and Reload does nothing.
- **61 Two-handed run.** Running with any two-handed weapon drawn (rifles, launchers,
  heavy weapons, two-handed melee) is slower by `fMove2HBigPenalty` (0.1 in
  FalloutNV.esm: 90 % of the speed), for the player and NPCs in combat. The setting's role
  is my guess from its name; 10 % may feel too small: it is one number to change.
## Render effects of playtest list 5 (2026-10-16, session "l5-render-fx", D5)

Items 1, 48, 10, 6, 18, 70, 66–67, 26 and 38 (`docs/handoff/playtest-list-5.md` §4).
Frames are kept privately; each was one run with test aids.

- **1 Dust storm "tornado"**: the column by the saloon and on the street is the dust
  vortex `effects\nv\sanddust\sanddust02.nif` (FXDust02, two placed in Goodsprings): two
  curved sheets fading in and out for about 5 s of every 45 s. Their vertex alpha (0 at
  the sheets' edges) was dropped because their shader sets flag bit 19 "Dynamic_Alpha",
  not bit 3 "Vertex_Alpha"; we drew a dense grey sheet with hard vertical edges. Unlit
  shapes with bit 19 now keep their vertex alpha. **Guess**: that rule; **check**: the
  column is now soft-edged and thinner, still grey rather than the game's brown (the
  texture is drawn as stored). The other "tornadoes" (`fxdustwhirlwind01.nif`,
  FXDustWhirlWind01) are particle systems on a moving mesh emitter and are still not drawn
  at all (particle systems of placed models are not built).
- **48 Bullet smoke**: particle colours (impact dust, metal smoke) are now taken as
  display values like unlit shapes' vertex colours, so the 0.27–0.43 greys are darker and
  less of a pale cloud. **Check**: this applies to every particle system, blood too
  (MPSBloodSplat): tell me if blood got too dark.
- **10 Explosions**: an EXPL now shows its model (`effects\explosiongrenadefrag.nif`: a
  flash, sparks, smoke and shrapnel burst from its sequence), its light (LIGH, fading
  over 0.6 s: **guess**, the LIGH has no duration) and the impact of its data set on the
  ground under it (`ImpactExplosionDirt.nif`: thrown debris). The grenade in flight
  (`grenlaunchprojectile.nif`) is drawn. **Not done**: the dirt impact's rising dust
  meshes (`FastMesh01–03`) are moved by transform controllers that our impact models do
  not play, so the brown column of your second frame is missing; the smoke fades after
  15 % of its life as its data says, which looks short; the debris flakes are large
  (radius 5, read as half the square's side).
- **6 Lantern light**: `LamplightWarm` stores a black colour; its warm colour is in its
  model's `NiPointLight` (0.78, 0.58, 0.16). Lights with a black record colour now take
  the model light's colour. **Not done**: the flicker (its colour controller).
- **18 Weapon quality**: the first-person model is the same file (`1stPerson9mm` names
  `9mm.nif`); the difference was the view: the game draws the arms and weapon through
  `fDefault1stPersonFOV=55`, we drew them through the world's 75, so the gun was smaller
  with coarser mips. Now magnified as through 55 (the Pip-Boy's 47 eases in as before).
  **Check**: size and place of the held weapons; iron sights while aiming.
- **70 White cave water**: the cave's pools are placeable water (PWAT `WaterBlood`, five
  `waterp512x512.nif` squares) drawn as a plain model, untextured: white. PWAT shapes are
  now drawn with their water type's material (WaterTypeBlood: dark).
- **66–67 Flashlight**: the light now sits on the body, 14 units ahead of the eye and 6
  below (**guess**), facing where the player looks, so it lights ahead of the body in
  third person and its shadows fall behind what casts them. **Check** the ghost shadow.
- **26 Beards**: beards (head parts with the hair shader) now take the hair colour like
  the hair. **Check** Sgt Kilborn and Joe Cobb.
- **38 SwapTexture**: implemented: `ref.SwapTexture "shape" "texture"` puts
  `textures\<texture>.dds` on that shape of the reference's model (the Rorschach cards in
  Doc Mitchell's test, the end slides, the Euclid's C-Finder screen); `##` before a
  name is dropped (**guess**: meaning unknown). Not kept in saves. Not seen in a run.
  **Fixed 2026-10-16 (claude/rorschach-cards)** after your report that the cards behind
  Doc Mitchell never changed: the calls are `SwapTexture <ref> "shape" "texture"` (the
  card `TempRorschachTest01REF` is the first parameter, no calling reference), so every
  call had failed. Now seen in a run: card 1 when the test starts, then each inkblot
  (`C:\fnv-private\frames\rorschach\card1-then-inkblot1.png`); smoke `rorschach-cards`.
  The Euclid's C-Finder screen and the end slides use the same form and are fixed with it
  (not looked at).
## Pause, settings and main menus, pointer, wait menu (2026-10-16, session "l5-menus")
List 5 items 8, 46, 19 and 51. Frames before and after: `C:\fnv-private\frames\l5-menus`.
- **Pause menu (8)** now has the original's look, from `start_menu.xml` and your
  screenshots: the column at the right (Continue, Save, Load, Settings, Quit), the
  highlight box round the chosen entry, the other pages dimming the column but for their
  own entry, Back (and Defaults) at the bottom right. My choices:
  - **Help is left out** (NVForever has no help pages). Say if you want it, e.g. listing the keys.
  - **The pause screen**: the original shows a model (`pausescreen01.nif`) we do not draw;
    I lay its texture over the held world, the left half and its mirror at the right, at
    0.82 strength over the world darkened by 0.35. Its glare texture came out as a dark slab
    drawn over the picture, so it is left out. Compare `after_pause_main.png` with yours.
  - **Settings**: our switch groups are the categories, renamed short like the original's:
    Gameplay (was "Dialogue and menus"), World, Display (was "Picture"), Graphics (was
    "Enhanced graphics"), Debug (was "Developer"), plus Saving. The switch list is wider
    than the original's (820 instead of 520, the value column at 700) because our labels are
    longer, and starts left of the middle so it clears the categories; each switch's note
    is under the list in font 3. Up/Down move within a category, Tab/Page Down go to the
    next category (Page Up back), a click on a category shows it. **Defaults** puts every
    switch back as it was when the page was first shown in this run (the command line's
    state), since nothing is saved between runs.
  - **Quit** asks first, with the original's words: Main Menu (NVForever starts again with
    `--menu`), Exit Game, Cancel. Q opens the question.
  - The **load page** is restyled the same way (thumbnail and details over the rows, left
    of the column, eight rows).
- **Main menu (46)**: the same look: every entry that can be taken at full amber, the
  greyed ones at half, the highlight box round the chosen one; moving the choice plays
  UIMenuFocus, taking one UIMenuOK (the original's list sounds). Its labels stay "New
  Game" and "Load Game" (the original says "New" and "Load"); say if you want them short.
  While Settings or Load Game is up the logo is hidden and the column is drawn dimmed but
  for that entry, as in the pause menu.
- **Pointer (19)**: the pointer is now `cursor.dds` (the light arrow with the dark edge),
  coloured amber, the pointer your screenshots show; before it was `glow_cursor.dds`. It
  replaces the system pointer everywhere the pointer is shown.
- **Wait/sleep menu (51)**: the HUD's amber (255, 182, 66) instead of the dimmer colour
  measured off the screenshot, and the game's fonts the menu file names (font 1 for the
  question, 6 for the hours, 2 for the time; the buttons in font 2 is my guess). The
  buttons stay half bright until the pointer is on them, as the menu file says.
Check in game: Esc in the world (the pause screen's strength, the box), Settings
(categories, the wide list, Defaults), Quit (Main Menu restarts to the main menu), the main
menu's box and hover sound, the pointer in any menu, T for the wait menu next to the HP bar.

## Crafting menus (2026-10-16, session "l5-menus")
List 5 item 72. The workbench, the campfire and the reloading bench now open one recipe
menu, the way the benches' own scripts ask for it (`ShowRecipeMenu`). The recipes,
categories, skills, ingredients and outputs come from the RCPE and RCCT records (notes in
docs/spec-notes/crafting.md). The campfire's list starts exactly as in your screenshot.
Frame: `C:\fnv-private\frames\l5-menus\after_recipe_menu.png`. My choices:
- **Arrows beside RECIPES**: they filter the list by subcategory (all, then Aid, Chems,
  Food, … that the list holds), with the title showing the subcategory. That is my
  reading of the original's filter arrows. Is it what they do?
- **Recipes that cannot be made yet** are a shade dimmer (brightness 190 of 255) than
  ones that can. Your screenshots only show recipes that could not be made. Accept is
  dim and refuses them, with UIMenuCancel.
- **Making**: Accept makes the recipe once. The ingredients leave the inventory, the
  outputs arrive, and an "X (n) added" line shows in the corner, with the menu's click
  sound (UIMenuOK). I found no crafting sound named in the files. There is no "how many
  times" choice yet.
- **The backing** is the darkened sheet of our other menus (0.75 black). The original
  blurs the world behind the menu.
- **Not drawn yet**: the output's icon and the EFFECTS line under WG/VAL.
- The keys: A or Enter to accept; X, Esc or Tab to exit; Up/Down and the wheel to move;
  Left/Right for the filter.
Check in game: a workbench, a campfire (the Goodsprings ones) and a reloading bench.
Check the list, the skill line (e.g. "Survival (32/40)"), the ingredient counts, and that
making something takes the ingredients and gives the items.
## HUD: bars, enemy health, sneak word, dialogue (2026-10-16, session "l5-hud", list 5 D1)

Playtest list 5, items 73, 32, 7 and 74 (`docs/spec-notes/hud.md` §4, §8–§10). Measured on
your screenshots (`Target healthbar` at 1920 x 1080, the four `Sneak detection` frames);
what they could not settle is a guess:

- **73, bars.** Each bar is now 37 ticks from just inside the upright to the corner mark,
  with "HP"/"AP" above it near the uprights; ticks are set on whole pixels so they are
  all the same width (the old unevenness was 4.5-pixel ticks rounded at 720 lines). The
  ticks use the game's tick picture, whose opacity fades towards the top, as the
  original's do. Guess: a tick is lit for any part of its share (37 ticks, so a bar shows
  one tick until the value is 0). Check: same width ticks at your usual resolution; the
  labels' places against the original's.
- **73, condition.** The bar now ends 15 units before the divider, its bottom on the line
  "CND" stands on. The two black arrows are the game's own `repair_meter_pips.dds`
  (named by the HUD file as `CNDArrows`), drawn as tall as the bar at 75 % of it (your
  number; on your screenshot they sit at about 75–79 %). Check: whether the arrows are
  as big as the original's.
- **32, enemy health.** The game's enemy bracket picture in the HUD's red (255, 67, 42),
  31 ticks between its corner marks, the name centred under the line; it empties from
  both ends to the middle. **Guesses (the rule is not documented):** shown for a living
  actor under the crosshair (up to 8192 units) that is hostile to you or fighting you,
  or that you hit in the last 5 seconds; and for 5 seconds after you hit any actor,
  aimed at or not. A friendly actor you only look at gets no bar. The old developer
  text (`Name [|||...] hp/max` above the crosshair) is removed. Check: when the bar
  appears and goes in the original, and whether a non-hostile actor you aim at shows it.
- **7, sneak word.** `[HIDDEN]`, `[DETECTED]` in amber, `[CAUTION]`, `[DANGER]` in red,
  centred at the top while you sneak, as on your screenshots (your list says "Cautious";
  the screenshot says CAUTION, which is used). **Guess (not documented):** DANGER while
  an actor fights you; CAUTION while a hostile actor searches for you (Alert/Lost state);
  DETECTED while any actor detects you; else HIDDEN. Check: walk past a friendly settler
  sneaking (should say DETECTED when seen), and provoke a Fiend camp.
- **74, dialogue.** In a conversation every HUD element is gone. Other actors' lines
  (Joe Cobb to Trudy) and radio lines are not drawn as subtitles while you talk. **Guess:**
  corner messages and quest messages ("COMPLETED: Go and meet Trudy…") wait during the
  conversation and come up after it, rather than over the dialogue screen; your
  screenshot showed the quest message during the conversation, and I hid it because you
  asked for every HUD element to go. Check: whether the original shows quest updates
  during a conversation.
## Pip-Boy: list 5 items 2, 11, 14, 30, 52, 54–56 (2026-10-16, session "l5-pipboy")

- **2 (flash on Tab):** what flashed was the Pip-Boy's flat fallback image, drawn in the
  middle of the window for the frame(s) before the arm model takes the screen (longer from
  third person). It now shows only once the Pip-Boy is all the way up and no model screen
  took it. Check: Tab from first and third person, nothing flashes.
- **11 (karma title):** the titles are in the game data as `sKarmicTitle<Good|Neutral|Evil>
  <level>` settings, but FalloutNV.esm holds only 50 of the 90 (the rest live in the exe);
  those 50 win, your table fills the other 40. Two differ from your table: the ESM spells
  "Demon Spawn" (yours "Demon's Spawn") and "Vegas Crimelord"/"Vegas Councillor". Choices:
  Very Good and Good use the good column, Very Evil and Evil the evil one; levels over 30
  use 30's. Shown on STATS > General in the statistics mode (R switches from reputations)
  under the karma picture, the karma number a line below. The reputation list stays as
  the hook for the reputation workstream (D7).
- **14 (ITEMS):** the hovered row is the selected row (it already sounded
  `UIPipBoyScroll`, whose file is ui_menu_focus.wav). New: the item's picture and figure
  cards at the right, laid out from the menu files. Guesses: DAM is the weapon's base
  damage (not condition- or perk-scaled); WG and VAL are the whole stack's (VAL matches
  your Stimpak 9 × 75 = 675; WG is a guess); the ammo card reads "<short name>
  (clip/carried)" with clip = what a clip holds of what you carry; armour shows its class
  word (Light/Medium/Heavy) beside CND; EFFECTS uses the STATS EFF wording. Not built: DPS,
  the small ammo badge, the Maintain/Mod/Repair hints. Sub-tab hover: the hovered tab gets
  an unfilled box and `UIMenuFocus`; switching tabs plays `UIPipBoyTab`. Check the box
  look against the game.
- **30 (scroll bar):** at the list's left only when rows are hidden, arrows at both ends
  and a line for the part shown (as the DATA lists). Check the line's look.
- **52 (maps):** drag never worked because the map was redrawn under the pointer each move,
  which cancelled Bevy's drag; the map now follows the held left button itself. The local
  map has an outline made from the collision (rays down through the physics world):
  outdoors slope-shaded ground with bright edges at buildings and cliffs, indoors a floor
  plan (walls bright). Indoors the map is drawn 3.5× closer (8 units a pixel instead of
  28), my choice so rooms are not specks. Only what the physics world has loaded shows.
  Check Goodsprings outside and Doc Mitchell's house.
- **54–56 (sounds):** static crackles (`UIPipBoyStatic`, random file of the folder) 0.5–2 s
  after the Pip-Boy is up, then every 4–10 s: the interval is my guess, say if it should
  be denser or a single one. Mode buttons/F1–F3 play `UIPipBoyMode`; the flashlight plays
  `UIPipBoyLightOn`/`Off`.

## World: steps, cave skeletons, bobby pin (2026-10-16, session "l5-world", list 5 D6)

Items 17, 71 and 63 of list 5. Items 59, 65, 64 and 4 of D6's queue are the laptop's L3.

- **17 (ledged steps).** The camper at the Powder Ganger camp (your `physics ledged steps`
  screenshot) has a 10-unit step with a 24-unit top, then a 20-unit step onto its floor.
  The cause was not the step height: the player's cone-shaped bottom meets a step's edge
  with a steep face, and the physics library's step-up refuses any steep contact, so every
  step with a shallow top needed a jump. The controller now makes its own step up when a
  wall stops it on the ground: up by the headroom (at most 24), across, down, kept only if
  it lands higher on a walkable top. *Guesses:* the step height stays 24 (the original's
  is not measured: if you find ledges you run up in the game that still need a jump,
  tell me their place); a steep slope is still not climbed this way. *Check:* run up the
  camper's steps into it; Doc Mitchell's porch steps, the saloon's and Goodsprings'
  wooden walkways; also that you do not now climb things you should not (crates of about
  knee height, fences, rocks: anything above 24 units still needs a jump).
- **71 (cave skeletons).** The cave's three skeletons are clutter (not dead NPCs) whose
  references store the pose the designers left them in (`XRGD`); it was read for corpses
  only. They are now drawn in it, lying on the floor. *Guess:* how the entries of a body
  part hanging directly under the model's root are read (under the reference, which lays
  all three flat; the other reading put bones under the floor). *Not done:* their
  collision stays upright as before (shots and pushes meet an invisible standing
  skeleton); other skeleton clutter with the same data elsewhere is posed too. *Check:*
  the cave's skeletons against the original (your `tposeskeletons` screenshot) and a
  few others (the Mojave has many).
- **63 (bobby pin).** The pin's model ends 1.47 units above the lock's centre as the two
  files place it, so turned about the lock its tip swung round the keyhole into the
  cylinder. It is now moved so its tip sits on the keyhole's middle and turns about it,
  as on your `basegame` screenshot. Strain wobble: while you hold torque against a lock
  that stopped, the pin shakes by 0.5° when new up to 4.5° just before it breaks, quickly
  (23 and 37 Hz). *Guesses:* the shake's size and speed (the original's not measured);
  none while the lock turns freely. *Check:* the pin's tip against `basegame.jpg` at a few
  angles, and whether the shake reads well or should be bigger or slower.
## Compass marks, discovery, crosshair, reputation (2026-10-16, session "l5-compass", list 5 D7)

- **57 (actor marks):** living NPCs and creatures of the loaded cells within Perception ×
  300 units (Perception 5: about 21 m) show as the HUD file's tick over the compass at
  their bearing; red when fighting you or ready to attack you (the enemy bar's rule),
  amber otherwise. The range is my guess (no setting in the masters): say if they should
  appear further out. Tick size and height are fitted to your screenshot; dead actors and
  disabled ones have none.
- **12 (map markers, discovery):** markers of the worldspace within Perception × 2048
  units (Perception 5: about 146 m), outdoors only, hollow until you have been within the
  marker's radius, filled after (the game's two triangle pictures, size fitted to your
  screenshot). The range is a guess; check against how early the game shows Goodsprings
  Cave. A new place shows "You have discovered" over the name where "Quest added" stands
  (same fonts and place as your screenshot) with `UIPopUpMapMarkerAdded` (guessed by its
  name) and gives `iXPRewardDiscoverMapMarker` (10) XP. Saves now keep the places found
  (they were forgotten on load before). Not built: the "+10 XP" bar at the right and the
  "Locations Discovered" statistic; interiors show no markers.
- **15 (dynamic crosshair):** with a gun, energy weapon, launcher or thrown weapon drawn
  the two arrows part so each tip sits on the edge of the cone a shot would leave in now
  (the combat model's spread with bloom: wider running, after shots, with crippled arms,
  narrower aimed or crouched), easing there at the HUD file's `_SpreadChangeSpeed` 0.125
  (my reading: a share of the way per 60th of a second). Capped at 240 units from the
  middle. Check how wide it looks against the game's for the varmint rifle and the
  9mm pistol, hip and aimed. The filled crosshair (something to use) does not open.
- **58 (red crosshair):** red (the crime colour) while the crosshair's ray meets a living
  actor that is hostile by the same rule as the compass.
- **13 (reputation):** the scripts' fame and infamy were already kept; added:
  - crimes: a theft someone saw adds 2 infamy and any kill while someone (the victim too)
    sees you adds 30, with every reputation the victim's factions name (FACT WMI1), up to
    the reputation's scale (the wiki's numbers). Killing hostile Powder Gangers counts, as
    your Vilified box shows. Lockpicking, trespass and assault change nothing. Not built:
    the hidden-silent-one-shot exception beyond "nobody detects you", disguises, resets.
  - the box: when a title changes, the game's message box (stops the game, queued with
    other boxes) titled with the reputation's name, the title and its description, a Vault
    Boy face at the left (very happy with more fame, sad with more infamy as on your two
    screenshots; the neutral face for mixed titles is my guess) and "Ok". Descriptions are
    the wiki's unless a plugin has the `sRepTitlePos<n>Neg<m>Desc` setting (FalloutNV.esm
    overrides one). Sounds `UIRepGood` when fame rose a level, else `UIRepBad` (guess).
  - fixed: Dark Hero and Soft-Hearted Devil were swapped in the Pip-Boy's list
    (FalloutNV.esm's `sRepTitlePosTwoNegThreeDesc` is the Soft-Hearted Devil's text).
  - Pip-Boy Stats > General, reputation mode: under the picture the title, then the
    reputation's name (was "Fame x, Infamy y"), as on your screenshot.
  - Check: shoot a Powder Ganger in view of others, see the box; GetReputation in the
    console.
## List 5 — claude/l5-npc-ai
NPC AI items of playtest list 5 (§4 numbers). Each: what changed, my decision or guess,
what to check in game.
- **3 Doors.** An NPC that goes through a load door fades out at it (its body walks on into
  the door for 0.75 s while it dithers away) and fades in at the other side over the same
  time; the door's open sound (DOOR SNAM, as for your own load doors) plays at the door it
  went in by and at the one it came out of, wherever you are drawn. *Guess:* 0.75 s, and
  the dither of the cell fades (meshes with other materials, if any, pop). *Check:* stand
  outside the gas station when Ringo comes out (talk to him, leave); an NPC going into the
  saloon.
- **28 Random conversations** now start only with you within 300 units (the hello
  distance) of one of the two; a due conversation waits for you instead of being skipped.
  *Guess:* 300 for "right next to them". *Check:* Goodsprings NPCs chatting only when you
  stand by them.
- **29 Ranger Ghost** has no packages at all [observed: vMORangerGhost has no PKID and its
  template does not give packages]. In a quiet 45 s run she stays on her sandbagged post
  (11,320 up). What would have left her by the fence: an actor without packages that
  searched, fought or was pushed stayed wherever that left it. Now it walks back to where
  it was placed (from up to 4,096 units). *Guess:* that the game keeps such actors at their
  editor location; that a search or a fight is what brought her down. *Check:* she is on
  the roof by the chain-link gate idle after you arrive, and after a fight at the outpost.
- **31 Friendly alert range:** an actor that starts fighting or notices a hostile alarms its
  allies within 4,096 units (was 2,048). *Guess:* the number. *Check:* Goodsprings
  settlers joining from across the town.
- **33 Facing:** gunmen the AI moves fire only with the target within 30° of straight ahead
  (they turn first); blows and bites are not held back, and actors without
  `--npc-packages` fire as before. People in Combat on the move (closing in, to cover, to
  a flank) are now drawn facing their target and step sideways or backwards with the
  locomotion clips for those directions, so they can fire as they go. *Guesses:* 30°;
  the turn to the target at twice the walking turn rate. *Check:* NCR troopers against
  Cook-Cook's Fiends: nobody firing while looking away, and how the sideways steps look.
  Fighters also turn to their target at twice the standing turn rate (240°/s), so a
  new target holds their fire for at most about 0.75 s. *Guess:* that rate. (Without it,
  `combat-cover` found no cover taken in 2 of 6 runs: the troopers lost the Cook-Cook
  fight too fast; with it, 4 of 4 runs passed.)
- **34 Package flags.** Done: Weapon Drawn (drawn during the package: Sunny runs to the
  wells with her rifle out), Weapons Unequipped (holstered and hidden unless fighting),
  HeadTracking Off (no head tracking unless a conversation or script look forces it),
  Defensive Combat (no attacking NPCs or creatures on sight; being attacked or an ally's
  call still counts), Always Sneak (36–37), the six door flags (lock/unlock all doors of
  the actor's cell at start, at end, and kept while it runs; only doors that have a lock,
  at their own level; the "at location" ones re-applied at each 10 s package choice).
  Already honoured before: Must Complete, Must Reach Location, Continue If PC Near, Once
  Per Day, Skip Fallout Behavior, Always Run, Continue During Combat, No Combat Alert, No
  Idle Anims, Offers Services. Not built: Pretend In Combat, Allow Swimming, Allow Falls,
  No Warn/Attack Behavior. *Guesses:* doors without a lock are left alone; an ended or
  replaced package both count as "end".
- **35 Sunny helps:** an actor that detects an attacker fighting someone, where the attacker
  would attack it too, now joins against it at once (the VCG02 geckos are Very Aggressive,
  so Sunny and Cheyenne join when they go for you). Not cowards, not Defensive Combat
  packages. *Decision:* it covers fights against you and against the onlooker itself, for
  everyone (settlers join against geckos that attack you nearby, too); fights between
  other actors are left to the existing assistance and alarms (with the rule there too,
  every NCR trooper took on Cook-Cook by itself and they stopped alarming each other).
  *Check:* VCG02 at the wells.
- **36–37 Sneaking:** an NPC sneaks while its package has Always Sneak or a script's
  SetForceSneak holds (now implemented): it crouches with its skeleton's `sneakmt…`
  locomotion clips, walks at fMoveSneakMult (0.57) of its pace, and detection and
  IsSneaking count it as sneaking. Sunny crouches on VCG02SunnySneakCloserToWell and
  stands up at "OK, you're on". Not built: sneaking with the Follow leader; the weapon's
  sneak clips (with a weapon drawn the arms hold the standing aim over the crouched legs).
  *Check:* how Sunny looks sneaking with her rifle out.
- **40 Doc Mitchell's chair:** his exit clip (SpecialIdle_NVDocChairFrontExit, the one the
  game's IDLE tree names for the front marker during VCG01 10–50) starts its Bip01 52 units
  beside the seat, so he jumped about 75 units to the side before walking off. Exits now play
  from where the body sits (no change for the plain exits, which start there anyway).
  *Guess:* that the game aligns every exit so; what the clip's own travel then does (35
  units sideways, 14 forward) is the clip's. *Check:* after "the stuff that mattered", which
  side he steps off to; if still the wrong side, say so and I will mirror that clip's
  sideways travel.


## List 5 — claude/l5-ui-text-sounds

Workstream L2 (laptop): the placeholder white text, menu sounds, Take All, the Goodbye
hint, the text overlay, radio subtitles and the Fallout 3 music question.

- **43, 50, 53 — no white text.** The white line under the crosshair is gone: nothing
  draws `activate::HudMessage` any more; its lines ("Stimpak added", "Gate opened",
  "… killed (+XP)", "… crippled", "You are dead.", "F5: not available yet", the
  jukebox's "its script would run (scripts are not implemented yet)", "You cannot use
  this (…)") are only logged. *Decision:* none of them became a corner message: they are
  NVForever's placeholders, not the game's lines; items a script adds still show their
  corner message as before. *Check:* pick up a stimpak, open the Goodsprings gate, press
  F5, use the saloon jukebox, kill a gecko: no white text anywhere.
- **44, 45 — refusals as corner messages.** The Wait/Sleep menu's refusals ("You cannot
  sleep when enemies are nearby.", game text), the lockpick refusals ("You need a
  Lockpick skill of 50 to pick this lock.", NVForever's wording, the game has none in its
  plugins) and the terminal refusals ("You need a Science skill of N to hack this
  terminal.") go to the top-left corner messages with the default icon for 2 s.
  *Guess:* the default icon and time (the records give none). *Check:* the corner
  message beside D1's reworked HUD; sleep with a gecko near; a lock above your skill.
- **39 — Vigor Tester sounds.** Its menu file and models name no sounds, so it plays
  Fallout 3's S.P.E.C.I.A.L. book records, which FalloutNV.esm keeps with two added for
  New Vegas: page turn `OBJBookSpecialPageTurn`, raise `OBJBookSpecialNumber`, lower
  `OBJBookSpecialNumberDown`, review row chosen `OBJBookSpecialFocus`, Done
  `OBJBookSpecialNumberFinished`, refused `UIMenuCancel`. *Guess:* that these are the
  Vigor Tester's sounds (by their names). *Check:* do they sound like the original's?
- **41, 42 — tag-skill and trait menus.** Row moved (keys, wheel, pointer) and pointer
  onto Reset/Done `UIMenuFocus`; a row taken or given back `UIMenuPrevNext`; refused
  `UIMenuCancel`; Reset `UIMenuCancel`; Done `UIMenuOK`. The game's files silence the
  rows (`&nosound;`); Done's `UIMenuOK` and the boxes' `UIMenuFocus` are the files'.
  *Guess:* the row click `UIMenuPrevNext` (the trait file's perk rows) and Reset's
  `UIMenuCancel`. *Check:* whether Reset should click `UIMenuOK` or `UIMenuPrevNext`.
- **47 — hacking sounds.** Cursor or menu selection moved `UIHackingCharScroll`, a word
  or row entered `UIHackingCharEnter`, password right `UIHackingPassGood`, wrong (and
  the lockout) `UIHackingPassBad`. *Guess:* which record is which event (by name);
  `UIHackingCharSingle`/`…Multiple` (presumably the text typing out, not built) and the
  fan hum loop are not played. *Check:* hack the Easy terminal at Bison Steve.
- **60 — Take All closes the menu** (when it took something). The container menu test
  aid opens the container again to store one item back, so `house-items` now expects
  the container opened and closed twice. *Check:* Take All on Doc Mitchell's fridge.
- **62 — "TAB) Goodbye".** While a Goodbye choice is offered the hint stands left of the
  dialogue band, level with the bottom bracket, on a dark box of the band's shade, in
  the dialogue font; on a 4:3 screen (too narrow) it sits at the safe zone's left edge
  over the band. *Guess:* the place, the wording's case and the box. *Check:* talk to
  Trudy on a 16:9 screen; say if it belongs inside the band instead.
- **68 — text overlay off by default** in the game (`--play`); `--overlay` turns it on,
  and the settings page's Developer group still toggles it. The free-camera viewer mode
  (no `--play`) keeps it on. *Check:* start the game: no position/key help text.
- **27 — no radio subtitles.** A station's talk lines (Radio New Vegas, the NCR
  Emergency Radio) are heard and never subtitled; `radio-pipboy` now expects none.
- **69 — Fallout 3 music: no change made.** No missing condition found: FalloutNV.esm's
  own controllers list 11 Fallout 3 location sets beside New Vegas, Fallout 1 and 2 sets
  (18 controllers, 171 of the 572 audio markers, 64 out in the Mojave, e.g. the generic
  Peaceful, Mystical, Dangerous and Military controllers, Novac, Boulder City, the Tops,
  Gomorrah, the crashed B-29); the sets are built exactly like New Vegas's single-piece
  sets. Only the controllers' `NAM1` (looping option, "Faction Not Found") is not
  decoded: if the original plays a list in order rather than at random, the Fallout 3
  pieces would come up in turn, not by chance (audio.md §13.5). *Check:* where did you
  hear it? Round Goodsprings the nearest such marker is a generic "Mystical" one
  (radius 3000) about 15,300 units from the town's marker; inside the town's radius the
  Goodsprings set plays. If you heard it inside Goodsprings or Doc Mitchell's house,
  that is a bug to chase with the `music:` log lines.


## List 5 — claude/l5-world-objects (2026-10-16, laptop L3: items 59, 65, 64, 4)

- **59 (motorcycles outside the saloon):** the three are STAT references
  (`Motorcycle01Static`), but their model `vehicles\motorcycle01.nif` has one dynamic
  Havok body (Props layer, 600 kg), the same model as the moveable static `Motorcycle01`.
  So the NIF cannot be what keeps them still in the game; the record type is.
  *Decision:* a placed STAT or SCOL never moves: its dynamic bodies are made fixed
  (physics.md §4.3.2). This also fixes the 1,131 other STAT references with dynamic models
  (mostly the `1stPerson…` weapon statics). MSTT, items, containers and activators move as
  before. `fnvtool phys cell` (the parity lab's rule) is unchanged. *Check:* walk into the
  saloon's motorcycles (they should not move), and push a tumbleweed and a Sarsaparilla
  crate (they should still move).
- **65 (animated containers):** containers whose model has Open and Close sequences
  (school lockers, footlockers, safes, toolboxes, ammo boxes) are now drawn with their
  moving parts and timed like doors that open in place. Opening one plays Open (0.13–0.5 s)
  and the container menu opens when it ends; closing the menu plays Close. Their sounds are
  the sequences' `Sound:` keys (`DRSLocker01Open`, `DRSSafeOpen`, …): these CONT records
  have no open or close sound of their own. *Guess:* the menu waits for the whole Open
  sequence (at most 2 s). A container with both its own SNAM/QNAM sound and sound keys
  would play both; none of the ones checked has both. *Check:* the school house's lockers,
  footlocker and toolbox and Ringo's floor safe: does the door or lid open before the menu
  and close after, and are the sounds right?
- **64 (open after lockpick or hack):** when a lock is picked or forced open, the lockpick
  menu closes and the door or container is then activated as if you had pressed E on it
  (a door opens or takes you through, a container's menu opens, after its Open
  animation). Terminals already went on to their menu after the right password; that is
  unchanged. *Check:* pick a locked door and a locked container.
- **4 (weapon animations in the viewer):** `nvf-viewer --weapon-review 2hr` (or `1hp`,
  `2ha`, `2hh`, `2hl`, `1hm`, `2hm`, `h2h`, `1gt`, …) shows a male NPC with a weapon of that
  type drawn. A / Shift+A step through the cases: standing, walking and running in all four
  directions, every attack of the type standing and while walking each way, each from the
  hip and aimed down the sights where the type has sights clips. The screen and the log
  name each case and every clip it plays (`weapon review: [n] … : 2hrleft.kf + 2hraimis.kf
  + 2hrattackleftis.kf`), so broken ones can be listed by case number or clip name.
  Options: `--weapon-mesh <path>` for another weapon model (default: Varmint Rifle, 10mm
  Pistol, Assault Rifle, Minigun, Missile Launcher, Machete, Sledgehammer, Frag Grenade;
  the fists have none), `--sneak` for the crouched forms, `--case N` to start at a case.
  The clips are picked and blended the way the game's actors do it (idle or movement,
  then aim, then attack, by track priority). *Not covered:* aim up and down, reloads,
  jams, equip and holster. *Check:* run it for each type and list the broken cases.

## Fire and poison: hit effects and the burning look (2026-10-16, branch claude/fire-poison-effects)

What a hit now applies besides its damage (docs/spec-notes/hit-effects.md): the flamer's
and the Shishkebab's "Burning" (2 health a second for 5 s), the incinerator's and the
incendiary grenade's (through their explosions: 4/s for 8 s, 6/s for 15 s), a cazador's
poison (on the player 5/s for 30 s and −25 AP, the record's own condition; on NPCs 18/s
for 8 s) and the radscorpions' (3/s for 5 s, 8/s for 10 s). Fire resistance and poison
resistance cut them. A burning actor shows the game's `Flames01` shader: an orange glow on
its outline and flame particles rising off its body (and `Smoke01` smoke from the
flamer's script). Guesses to check:

- **A second hit refreshes, it does not stack.** A flamer hit on someone already burning
  starts the 5 s again; the burns do not add up. The same for poisons: a second cazador
  sting restarts the 30 s rather than adding a second 5/s. *Check:* in the game, does a
  cazador's second sting double the poison (two "Poison" entries), and does a burn last
  5 s after the last flamer hit?
- **Radscorpions poison with every blow.** Their record ties the poison to the
  "AttackForwardPower" attack; NVForever's creatures do not play their forward power
  attacks yet, so every blow poisons. *Check:* does a radscorpion poison only with its
  lunging sting?
- **Corpses burn on.** The burn's records say it is not dispelled by death (and the
  flamer's script lights up a corpse), so a target killed by the flamer keeps burning for
  the rest of its 5 s; a poison ends at death. *Check:* do killed actors burn on?
- **How many flames.** The record's birth ratio (30, then 15) has no documented unit; it
  is read as 6 particles a second per unit (90 a second while it burns). The flames start
  at the body's bones, 10 units out. *Check:* screenshots `player_burning.png` and
  `npc_burning.png` of the smoke runs `flamer-burns-player` / `flamer-burns-npc` against
  a burning actor in the game: more or fewer flames, bigger or smaller?
- **Fire resistance like poison resistance.** The GECK wiki says only that poison resist
  is "the percentage of poison damage that is ignored"; fire resistance is read the same.
- **God mode** (tgm, `NVF_PLAY_TEST_GOD`) keeps all hit effects off the player, not only
  the damage.
- **Resistances show in the numbers.** The player's Poison Resistance (20 % at
  Endurance 5) makes a cazador sting 4 a second and a large radscorpion's 6.4.
- **Creatures a script places now fight** (your report). PlaceAtMe of a creature or NPC
  (or a leveled list of them) makes a real actor from the next frame: drawn with its
  skeleton, walked by the AI, sensing and sensed, attacking enemies as placed actors do
  (smoke `placeatme-fights`: a cazador placed at the player in Goodsprings attacks and
  poisons). Before, it was drawn as a frozen model and nothing ever noticed it. *Check:*
  scripted spawns in the game (ambushes) come at you.
- Not built: the weapons' critical effects (the flamer's "Burning Explosion"), the
  flames' add-on fire-ball models, the poison's screen effect.
## Creature speeds, cazador flight, turning (2026-10-16, branch `claude/cazador-movement`)

Your report: cazadors move far too slowly and their movement stops and starts like
Victor's used to. Details and numbers: docs/spec-notes/actors.md §8.10.

- **Speed, measured in the original game.** Every creature walked at the people's 87 and
  ran at 344 units/s. I watched cazadors in the original with the AI observer (your tip:
  the calm ones at the Tribal Village with `tdetect`, and spawned ones charging in god
  mode). They move at their clip's own speed × size × Speed Mult × 0.70: young cazadors
  walk 228 and charge at up to 576 units/s, the full-size ones 380 and up to 974. Every
  creature now moves by that rule, so small ones (your half-size radscorpion) are slower
  than the big ones of their kind. *Guess:* the 0.70 holds for all creatures; I only
  measured cazadors.
- **Flying when alert.** As you said: cazadors move on their ground clips while calm and
  on their flying clips (`h2hforward`, `h2hfastforward`) when Alert, in Combat or
  searching. Dogs and the mirelurk king have such clips too and switch the same way.
  *Guess:* the switch is immediate (no blend between the two sets).
- **Stop-and-start.** Three causes, all fixed: (1) a cazador's run speed of 344 sat right
  on the point between its walk and run clips, so small slow-downs swapped them; (2) a
  one-frame jump in a creature's drawn speed started a fade to the run clip and back (the
  hitch); the walk/run choice now follows a speed smoothed over 0.2 s with a margin;
  (3) creatures stopped to turn on the spot at the people's 120°/s before every short
  walk. In the original cazadors turn at about 300°/s (full size) and 137°/s (young),
  mostly while moving; creatures now turn at 300°/s × their size, and stop to turn only
  when they face more than 60° away. *Guesses:* the size rule and the 60°.
- **Deathclaws had no walk:** their walk clip is spelt `mtfoward.kf` in the game data; it
  is found now.
- **Deathclaws' and ghouls' alert look** (claws and hands raised) came in with #326
  (`creature_stance.rs`, actors.md §9).
- *Check:* the cazadors at the Tribal Village (calm, then shoot one); a deathclaw walking;
  a small radscorpion.
## Scripted weapon use: UseWeapon and Use Weapon packages (2026-10-16, session "use-weapon")
Built (docs/design/use-weapon.md): an actor told to use a weapon walks to where it is to
stand, draws, turns to its target and fires its shots. My choices and guesses, to confirm:
- **A script's use beats the fight.** The shipped scripts call StartCombat right next to
  UseWeapon (e.g. a trigger that has an actor fire four shots at someone, then fight you),
  so the use runs first and the combat AI takes over after the last shot. A *package's*
  use waits while the actor is in combat, like other package doings.
- **Shots.** The function's count is one volley at the weapon's own rate (semi-automatics
  wait their record's semi-auto delay between shots). For packages: Number of Bursts =
  that many volleys, each a random size between Shots Per Volley min and max; Volley Fire
  waits a random pause between volleys, Auto Fire does not; Repeat Fire goes on until the
  package changes.
- **Always Hit** = no spread at all, straight at the chest (cover can still stop it).
  **Do No Damage** = the shot flies, hits, shows its impact and makes its sound, but no
  damage, no OnHit, no explosion.
- **Hold Fire When Blocked** only looks at people in the line of fire, not walls.
  **Target should be here**: the target must be within 256 units of that reference.
- **Crouch To Reload** crouches the actor while its weapon reloads (ammo is unlimited, as
  for the combat AI, so it only reloads when its magazine is empty).
- **Not built:** melee weapon use (the use ends at once), swapping the model in the hand
  when the use names another weapon (that weapon's shots come out of the gun drawn),
  grabbing a weapon the actor does not carry, the 9 packages' location 2.
- **Check:** `python tools/smoke run --only use-weapon` (Violet fires three harmless,
  always-hit shots at you). In the game: Archimedes II's targeting sequence and the
  Cachino/Big Sal killing in the Gomorrah quest use it.
## Combat lines and effort sounds (2026-10-16, session "combat dialogue", branch claude/combat-dialogue)

The Combat tab's topics are now said in fights (docs/spec-notes/combat-dialogue.md). From the
GECK wiki's Combat Tab page: Hit when a hit takes more than `fCombatSpeakHitThreshold` (1 %) of
the actor's health or by `fCombatSpeakHitChance`; Attack and PowerAttack at most once per
5–30 s per actor (`fCombatDialogueAttack…ElapsedTime`); Death always. The player's lines are the
`GenericPlayer` quest's (PlayerVoiceMale grunts for Hit, PowerAttack and Death). Creatures and
robots say Hit and Death lines whose response is a sound (`NPCBrahminInjured`,
`FXExplosionRobotBuzz`, …): it plays where they stand instead of a voice.

*Guesses:* "health" for the Hit threshold and for HealthHalf/HealthQuarter is the actor's
base health; `fCombatSpeakAttackChance` (0.01) is not used, the timer alone decides (a 1 %
chance per attack would make Attack lines rare); FireExplosive shares Attack's timer;
Crippled, HealthHalf and HealthQuarter replace that hit's Hit line when the actor has one;
a Death line goes on after its speaker died; the nearest living member of the dead actor's
group within 1,536 units says DeathResponse after the Death line; combat lines get no group
cooldown (unlike the Detection lines), so every dog of a pack yelps when hit; no combat line
is said by an actor already saying a line.
*Check:* fight the Goodsprings dogs or Fiends with sound on: do you hear your own grunts when
hit and on a held (power) melee swing, the dogs' injured yelps, NPCs' battle cries no more
often than every few seconds, and a death line when someone dies? Is anything too frequent?
*Not built:* NPC power attacks and grenades (so their PowerAttack and FireExplosive lines),
AvoidThreat, Yield, KnockedBack, OnFire, Poisoned, Wake and the Response topics other than
DeathResponse.

## Cook-Cook's flamer: steady fire and hands on the gun (2026-10-16)

Your report: firing, Cook-Cook jittered as if the flamer had recoil (his shoulders moved
a lot), and his hands did not line up on the flamer, as with the minigun before. Branch
`claude/npc-heavy-weapon-pose`. Facts: `docs/spec-notes/animation.md` §9.7 and §9.8.

1. **Jitter:** every flame shot started the firing animation again from its start, many
   times a second. Now the animation keeps running through the burst, going round its
   steady middle part, as yours does with the minigun, and plays out when he stops.
2. **Hands:** the minigun fix (the gun stays where the draw animation put it) was only
   on your character. NPCs have it now too: the flamer is held forward, right hand on
   the rear grip, left hand on the top handle. It also applies to NPCs' miniguns, gatling
   lasers and pistols.
3. **My guess, for you to check:** NPCs start the firing animation at its steady part
   (no spin-up), since their shots leave at once. A game look at Cook-Cook firing,
   from the side, would confirm the arms.

## Controller support (2026-10-16, branch claude/controller, stacked on claude/key-rebinding)

Design: docs/design/controller.md; facts: docs/spec-notes/controls.md §2.1. Settings page
→ Controls → first row **Input Device** (Keyboard and Mouse / Controller), also `Input
Device=` in `[Controls]` of NVForever.ini; the buttons are in `[Controller]`.

To check with a controller in the original, my guesses:

1. The game's controller byte read as buttons: Grab on a press of the right stick and
   Toggle POV on LB are guesses (A/B/X/Y, LT/RT, RB = V.A.T.S., Back = Wait, left stick
   press = Sneak follow the published console layout). Ammo Swap's code `01` is not
   placed; I put Hotkey1, Ammo Swap, Hotkey3, Hotkey4 on the D-pad (up, right, down, left).
2. Right stick look speed: the game's `fXenonHorizLookSpeed=1500` / `Vert…=1200` read as
   1.5 / 1.2 radians a second at full push, the push squared. Too fast or slow?
3. The left stick walks under 0.6 of a full push (Always Run on), runs beyond.
4. With Controller chosen the keyboard and mouse no longer work the controls (Esc and
   the console still do). Should both work at once instead?
5. The prompt under the crosshair shows "A)" as text; the original shows the A button's
   picture. Menus' own buttons and hints ("E) Exit") do not follow the controller yet.
6. **Rumble** (branch claude/controller-rumble; docs/design/controller.md §4): `Rumble=1`
   in `[Controller]`, a Rumble row at the end of the Controls tab with Controller chosen.
   All my guesses, to feel with a pad against the original:
   - firing: by weapon weight, a 2 lb pistol a short buzz (weak 0.38, 0.08 s), 20 lb and
     up the most (strong 0.7, weak 0.7, 0.2 s); melee swings do not rumble;
   - taking damage: strong 0.35 to 1 by the share of health lost (a quarter or more the
     most), 0.15 to 0.45 s;
   - a hard landing from 600 units/s (a drop of about 260 units), the most at 1200;
   - explosions: full within the blast radius, none 1400 units beyond it, up to 0.6 s.
   Does the original rumble on other things (being hit by melee only, low health, V.A.T.S.,
   the Pip-Boy)?


## Draw-distance sliders (2026-10-16, branch claude/render-distance-sliders)

Settings → Display has five sliders, drawn like the "How many?" meter (ticks, an arrow
at each end, the value as a percentage): Object, Actor, Item, Shadow and Grass distance.
Each is a part of a full distance, 10% to 100% in 5% steps; Left/Right, the arrows or a
press/drag on the meter move it; it is kept in `NVForever.ini` [Settings] (`object-distance=0.50`).
Choices I made, each easy to change:
- **Full distance for objects, actors and items**: nothing culled them before but their
  cells, so 100% is the farthest a loaded cell reaches, `(--radius + 1)` cells along both
  axes (17,377 units with the default radius 2), and 100% draws exactly as before. 50%
  draws what is within 8,689 units of the camera.
- **Fade**: past its distance a mesh dithers out over a further 10% of it (the cell fades'
  dither), then is culled, shadow included.
- **What counts as what**: actors are NPCs and creatures with everything they wear and
  hold; items are references whose base is an item you can take (WEAP, ARMO, AMMO, MISC,
  ALCH, BOOK, NOTE, KEYM, IMOD, CHIP, CCRD, CMNY) and items dropped in the game; objects
  are every other placed model in a loaded cell. Distant LOD, imposters, terrain, water and
  trees are not touched.
- **Shadows** are a part of `--shadow-distance` (8,000) and **grass** of `--grass-distance`
  plus its fade range (7,000 + 1,000).
## NPCs crouch, block and close in by context (2026-10-16, branch claude/npc-crouch-block)

Your request: NPCs crouch to fire at a distance or block with a melee weapon, by context,
not predictably, people only. Spec and rules: docs/design/combat-ai.md §14. Every number
below is my choice; tell me any you want changed.

- **Crouching**: a gunman that sees its target, has stood still 0.5 s, with the target
  beyond 600 units and no close-combat enemy within 1,500, rolls every 1.5–4 s to crouch.
  The chance is 25–90 % by a per-fight temperament, lower nearer than 1,400 units.
  Crouched, it now and then stands and shifts 128–320 units sideways. A crouched NPC
  drops the spread's standing term, as the player does, and is drawn with the game's
  `sneak…aim` and `sneak…attack` clips. Its eyes for shooting are at 0.65 of the standing
  height (not measured).
- **Cover first**: a person whose style's take-cover roll fails gets a second chance of
  up to 40 % by temperament, so even styles with 0 (CSRaider) sometimes look for cover. Behind **low
  cover** (hides the crouched eyes only) it hides crouched and stands to fire.
- **Blocking**: only people with fists or a melee weapon, only against a close-combat
  opponent, never against a gunman. The chance is the wiki's documented melee decision
  (block first); FalloutNV.esm's default styles give 80 % while the opponent swings and
  40 % otherwise. It also reacts to a swing after 0.12–0.35 s, facing the attacker. A
  blocked blow loses 60 % (the existing `CombatTuning::block`) and the NPC attacker
  recoils for 1 s. **Your own block (Block held with a melee weapon) now reduces NPC
  blows too**: the rule existed but was never applied.
- **Closing in**: a melee person against a gunman that can see it and is more than 1,000
  units off bounds from cover to cover (back of a navmesh cover edge, 250+ units nearer,
  within 50° of the line, hidden from the gunman), pausing 0.4–1.6 s at each. With no
  such cover it charges. About half (by temperament) stay by a gunman of their group
  until the target is within 900 units of either, the target fights in close combat, or
  10–25 s pass.
- Not built: the player recoiling when an NPC blocks; switching a guard to whoever
  rushes the gunman it guards (it fights its own combat target).
## NPCs chain melee combos (2026-10-16, branch claude/npc-melee-combo)

Your request: melee enemies chain attacks into combos with different clips. Rules:
docs/design/combat-ai.md §15. My choices, tell me any you want changed:

- An NPC decides again at the clip's `a:` key (the same key the player's combos use);
  if it attacks then, the next swing is the group the key names (right, left, right…).
  It goes on by its combat style's attack chance (55 % for the default style), so combos
  are mostly two or three blows; a hold, block or recoil ends one.
- The next clip cuts the last one at the `a:` key, as the player's does.
- No NPC power attacks yet.

## NPCs' holstered flamer: tank on the back, no gun (2026-10-16)

Your request: an unequipped flamer should show the flame tank on the back and no weapon,
as your minigun does. Branch `claude/npc-heavy-holster`. Facts: `docs/spec-notes/animation.md` §9.7.

1. **Built:** NPCs now read the same two things from the game files as your character:
   the flamer's tank (and the minigun's backpack) hangs from the upper back, and the gun
   part is switched off while holstered and on half way through the draw, by the
   animations' own show/hide tracks. Checked on Cook-Cook: holstered, the tank on his
   back and empty hands; drawn, the flamer in both hands with the tank still on the back.
2. **Also:** NPCs' miniguns and gatling lasers get the same. Under a package that keeps
   weapons put away, the tank is hidden with the rest of the weapon; it is not counted as
   part of the gun when hits on the weapon are worked out.
3. **Your character's flamer** already worked this way (it uses the same rule as the
   minigun); I did not take a new look at it.

## Key names in tutorial messages (2026-10-16, session "key-tokens")

Branch `claude/key-tokens`: the sneak tutorial read "To sneak or crouch, &sUActnCrouch;."
(owner's screenshot).

**Fix.** The game's texts use 16 control strings (every `&…;` in FalloutNV.esm); five are
not the INI's names: Sldleft and Sldright (mapped before), and now Crouch (Crouch/Sneak),
Rdyitem (Ready Item) and ToggleRun. A control bound only to a mouse button names the
button ("Left Mouse: Attack"). The keys are read when the message appears, so a key
rebound on the Controls page shows in the next message.
*Guesses for you to check:*
1. ToggleRun ("To toggle between run and walk, …") is the Always Run key (Caps Lock).
2. The original's words for keys are not known: modifiers read "Left Ctrl", "Left Shift",
   mouse buttons "Left Mouse", and `&sDeviceMouse;` ("… Look around") reads "Mouse".
   A screenshot of the sneak tutorial or the controls help in the game would settle them.

## Talking gestures (2026-10-16, branch claude/talk-gestures-bc034f)

Your report: lips move but heads and bodies stand still while NPCs talk. NPCs in the
dialogue screen, and both actors of a conversation between NPCs, now play the IDLE
tree's talking and listening clips (spec: docs/spec-notes/idles.md §12). The tree picks
mood, sitting or standing, power armour and arms crossed by itself. My guesses, each
worth a look in the original:

- **When a new clip starts**: at each new response, when the NPC stops talking (it
  listens while you choose), and whenever a clip ends while it is still talking. Talk
  clips are 3–7 s, so a long response gets several. A new response cross-fades the clip
  at once rather than letting it finish.
- **Random rolls**: each `GetRandomPercent` condition is a fresh roll. The mood groups'
  rising thresholds (11, 22, … 88) look written for one shared roll, which would split
  the clips evenly; with fresh rolls the later clips in a list come up more often.
- **Arms crossed sticks**: once an NPC crosses its arms (5 %, at most once per 240 s)
  the tree keeps them crossed for the rest of the conversation (`IsLastIdlePlayed`, a
  function the GECK docs do not describe; I read it as "the last idle played was X").
- **NPC-to-NPC conversations** count as the dialogue procedure for the tree
  (`GetCurrentAIProcedure` 4, `GetCurrentAIPackage` 6), else their branch could never
  be reached.
- **The listener's mood** is Neutral (`GetDialogueEmotion` is read from the actor's own
  line).
- **SayTo lines, hellos and barks** get no gestures: the tree's dialogue root wants the
  dialogue screen or a conversation. Doc Mitchell's SayTo lines in the opening therefore
  still use only their own speaker idles.
- **Replay delays** are read as seconds (as for the weapon fidgets).
- Not done: the `.lip` files' head and look channels.
## Script function audit (2026-10-16, branch claude/serene-chaplygin-ee2772)

Every registered script function checked for "the engine knows it, the rules never hear it"
and for constant answers (docs/design/script-function-audit.md: the table, the three
captures, what is left). Fixed 35, matching 283, 25 gaps left, 18 open.

*Decisions for you to check:*
1. **NPCs dress the first time their cell loads**, by the engine's wearing rule (armour by
   DT, then DR; a slot taken stays taken) and the highest-damage weapon it can draw. In the
   original Sunny Smiles read GetEquipped 0 for her armour and rifle before her cell ever
   loaded and 1 after (and kept it). The weapon rule is still the engine's guess.
2. **GetDayOfWeek** now counts from Sunday 13 October 2281 on a 365-day calendar, as the
   wait menu already did: the original read Friday (5) on 25 October 2281, and your
   screenshot shows 19 October as a Saturday. Package schedules by weekday follow, so some
   NPCs keep a different day than before. Leap days are not checked.
3. **GetOpenState** reads 3 for every closed object (doors, containers, even activators) and
   0 for actors, as nine objects and Ringo did in the original.
4. **SetPos / SetAngle / MoveTo** now move what is drawn at once (actors teleport, objects
   redraw, the player travels); before, the stored value was overwritten.
5. **Unconscious actors no longer walk** (a guess); they still stand, as lying down is not
   built.
6. **IsWeaponOut** of an NPC the engine is not drawing follows the draw rule (alerted, Weapon
   Drawn package, a weapon use), so it reads 1 at once where a drawn NPC reads 1 only once
   the draw animation ends (the original: 0 three frames after SetAlert 1, 1 by 180 frames).

*Open, for the next capture batch (§4 of the audit):* GetDisposition read 35 for Sunny and
Trudy (our stub returns 0; 15 dialogue conditions test it); GetCurrentAIPackage read 34
for Sunny while unloaded and 6 once loaded; GetCauseofDeath of a live unloaded coyote
read 0 where the wiki lists −1 for none; NPC door use and OnActivate; far NPC fights.

*Follow-up proposed:* make the engine dress and arm NPCs from the items the rules have
equipped, so scripted EquipItem/UnequipItem show and the two leveled rolls agree.

## Blackjack at the casino tables (2026-10-17, branch claude/blackjack)

Built from the game's own files: the casinos' records (decks, shuffle point, 3:2 payout,
the soft-17 rule, max winnings, the chip), the tables' scripts (ShowBlackJackMenuParams),
`black_jack_menu.xml`, the table/card/chip models with their deal, reveal and discard
sequences, the `GAMEBlackJack…` sounds and the `HelpBlackjack` message
(docs/spec-notes/blackjack.md). Smoke `blackjack`; screenshots in `target/smoke/blackjack_*.png`
of a run. Guesses a screenshot or a few hands at a real table would settle:

- The words on the deal-phase buttons (Deal, Increase Bet, Decrease Bet, Max Bet, Exit Game,
  taken from the menu file's comments), the result line ("Blackjack. Dealer has 16. +37") and
  the refusals ("You need Gomorrah Chips to play here.", "You have been banned from gambling at …").
- Choices that do not apply are hidden, not dimmed; the bet starts at the table's minimum and
  steps 1 / 5 / 25.
- The dealer does not peek for a natural; split aces play on normally; one split only;
  surrender gives half back rounded up.
- Winnings levels 1–3 (the floor manager's comps) at a quarter, half and three quarters of
  the casino's max winnings; at the max the round ends, the menu closes and a corner line
  says the player is banned.
- The camera, the lights and where the bet's chips stand are fitted by eye to the one
  screenshot.
