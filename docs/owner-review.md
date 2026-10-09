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
2. **Other sessions merged at the same time, under your GitHub account:** #44 at 01:06Z,
   then #47 (Goodsprings windmill animation, quadratic key tangents) and #48 (animation
   data groundwork, "workstream 5 part F"). These came from sessions this coordinator
   cannot see in the session list. #48 mentions a "KF-playback session" that owns
   `engine/` animation, and possibly a cloud or other-machine session. Keep that in mind
   when two sessions both think they coordinate merges. The coordinator re-checked the
   resulting `main` (e5db7eb) as a whole. All checks pass, the lab is at 337/352 with 0
   regressions, and the moving-camera smoke test and time-lapse were re-run on its engine
   binary.
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
