# JIP LN `GetFollowers` and `GetDetectingActors`

## Documented contract

- [documented: GECK wiki, [GetFollowers](https://geckwiki.com/index.php?title=GetFollowers)]
  `reference.GetFollowers` takes no arguments and returns an `array_var` of all
  actors currently following the calling reference.
- [documented: GECK wiki, [GetDetectingActors](https://geckwiki.com/index.php?title=GetDetectingActors)]
  `actorRef.GetDetectingActors` takes no arguments and returns an `array_var` of
  all actors currently detecting the calling actor.
- [observed: `gaps7` capture, `crates/fnv-game/src/nvse_gaps.rs`]
  When no actors match (e.g. player with no followers or no detecting actors),
  both functions return `ar_Null` (`0` / `ar_Size == -1`) rather than an empty
  allocated array.

## NVForever mapping and open questions

- [design] `Game::followers_of(leader)` collects living, present (`Game::present`),
  non-player actors whose currently selected package (`RefExtra::current_package`)
  is `PackageType::Follow` or `PackageType::Accompany` with
  `Game::package_target(actor, def) == Some(leader)` (supporting both specific
  reference and linked-reference targets), sorted deterministically by `FormId`.
  Querying `GetFollowers` is read-only and does not evaluate unevaluated
  packages.
- [design] `Game::detecting_actors(target)` validates that `target` is an actor
  and collects living, present actors (excluding self-pairs) that have `target`
  in `Band::Detected` inside `Game::detection.pairs`, sorted deterministically
  by `FormId`. Both handlers return `ar_Null` (`0`) when empty or when the
  calling reference is omitted.
- [observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + JIP LN 57.30 with the 10
  official base masters, clean-room script probe jip_37_followers_and_detecting_empty
  (GetFollowers/GetDetectingActors): -1,-1; no DLL or source consulted] Both return
  `ar_Null` (`ar_Size == -1`) when empty.
- [observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + JIP LN 57.30, 2026-11-24,
  `GSDocMitchellHouse` (`0x00103DF9`), 10 official base masters,
  `w4d_17_followers_detecting_order` (`tools/capture/nvse/catalogs/wave4d_extenders.json`,
  #3679): `doc=00104C0F,a1=FF00132A,a2=FF00132B|det(1:[FF00132A])|fol(-1:[])`; no DLL or
  source consulted] `GetDetectingActors` returns detecting actors in line-of-sight when
  non-empty, and bare `SetPlayerTeammate 1` without a Follow AI package leaves
  `GetFollowers` as `ar_Null` (`-1`).
- [hypothesis] Multi-element array ordering, whether unloaded or low-process followers are
  included, whether `Escort` packages count as followers, and whether non-actor or stale
  detection pairs are filtered the same way in JIP LN NVSE remain unverified against an
  original-game capture.
