# Handover: open-issues sweep, 2026-11-13

The owner asked a session to work through every open GitHub issue, oldest to newest, until
each is resolved and closed. It was stopped partway at the owner's request. This note says
what was done, what is half-done, and how to carry on.

## How the sweep works

- List: `gh issue list --state open --limit 200 --json number,title,createdAt,comments`,
  sorted by `createdAt`. There were 82 open issues at the start.
- Skip an issue if another session or machine claimed it, if it needs a capture in the
  original game first (observer check), or if it waits on an owner decision. Most of the
  older issues are in these groups (see the triage below).
- For each issue taken: post a claim comment, branch off `origin/main`, fix, run the checks
  CLAUDE.md lists, open a **ready** PR with `Fixes #N`, comment the PR number on the issue.
  **Do not merge**: the PR coordinator merges, and the issue closes on merge.
- At most two agents that build or run smoke at once (8-core machine). Each agent uses its
  own worktree and target dir and builds with `-j 4`.

## Done

| Issue | Result |
|---|---|
| #1640 | Closed: everything was merged in #1782 and #2197. |
| #1512 | PR #2343: the three tile test layouts that copied the game's menu files are rewritten. |
| #1523 | PR #2342: a worn item's own slots push out the Pip-Boy Glove; the raised Pip-Boy hides the body's arms. Two smoke scenarios. |
| #2274 | PR #2332: catalogue parameter types stop at the type word; two rows fixed. |
| #2308 | PR #2400: the repair and recipe menus open over a conversation (no soft-lock). Smoke `repair-from-dialogue`. |
| #2309 | PR #2399: a restrained actor's Dialogue package talks without moving (Hardin's training). Smoke `restrained-dialogue-package`. |
| #2324 | PR #2411: already fixed on main by #2282; the securitron scenarios now also run when `actors.rs` changes. |
| #2269 | No work: the fix is in the open PR #2276 (fork commit `2e6305d06`). Comment left; close it when #2276 merges. |

Check every PR above for coordinator review notes and answer them
(`gh pr view <n> --comments`).

## Half-done (WIP pushed, claims released)

Both were stopped mid-work. Each branch has one WIP commit that was **not built, tested or
smoke-run** after its edits. Start from the branch, finish it, verify it, open the PR.

- **#2311** (B leaving Dead Eye V.A.T.S. also opens the Pip-Boy): branch
  `claude/vats-b-pipboy-2311`. It touches `flashlight.rs` (MenuKeyHold), `vats_mode.rs`,
  `docs/design/vats-mode.md` and a new scenario in `tools/smoke/scenarios.toml`.
- **#2313** (the left stick doesn't change SPECIAL): branch `claude/stick-special-2313`. It
  touches `pad.rs`, `chargen.rs`, `chargen/vigor.rs`, `controller.md`, `vigor-tester.md` and a
  new scenario. Builds on #2171 (D-pad points).
- **#2310** (pointer shows with a controller; a resting pointer takes the Pip-Boy list
  selection): not started. The same agent was to do it after #2311 as its own branch off
  main. The plan: hide the pointer when `Bindings::device` is the controller, and let
  pointer-over take selection only when the pointer moved.

After merging the WIP branches into anything, run `python tools/smoke verify` (both add
scenarios).

## Next workable issues (unclaimed when the sweep stopped)

- #2312: controller hotkey assignment (RB Hotkey on the ITEMS page). Do it after #2310, which
  also touches `pipboy/`.
- #2279: Goodsprings night stalkers try their 23:00 raid route at 08:00.
- #2272, #2270, #2268, #2271, #2273, #2275, #2318, #2326: extender/NVSE items. Most overlap
  the open PRs #2276, #2297, #2299 and #2320. Check those first.

## Skipped in the triage, and why

- **Need an observer capture of the original first:** #973, #986, #1113 (checks 43/45/46/48),
  #1401, #1785, #1787 (trough), #1997, #2030 (pile behaviour), #2083, #2131–#2135 rows.
- **Claimed or with a PR from another session/machine:** #1114, #1115, #1162, #1253, #1277,
  #1387, #1390, #1434, #1443, #1450, #1461, #1797, #1799, #1882, #1891, #1893, #1953, #1956,
  #1992, #2132–#2135, #2167 (#2301), #2169, #2186 (#2327), #2242 (#2323), #2248, #2250,
  #2266, #2286 (#2329), #2298 (#2299), #2317, #2318 (#2320), #2321 (#2322).
- **Owner decision or "do not start":** #1374 (owner-review item 41), #1758 (after all NVSE
  work), #2149 (decided 2026-11-13: smoothing on by default, collision follows it; not yet
  claimed, so it can be taken).
- **Post-1.0 or large features, not started:** #1408, #1420, #1444, #1502, #1503, #1526,
  #1730, #2081, #2082, #2325.
- **Performance, partly done:** #1362, #1404 and the remaining bullets of #1390, #1797,
  #1799 and #1992.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
