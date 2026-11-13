# Movement camera overlays and position holds

## Contract and scope

[documented: `fnv_actor::idle::SECTIONS`, `crates/fnv-actor/src/idle.rs`]
IDLE section 1 is Movement. Other sections include Idle (0) and SpecialIdle (7).
The section is record metadata; a clip's filename or looping mode does not identify
whether it owns the player's position.

[design] A first-person clip with a `Camera1st` track in the Movement section may
animate the view while physics moves the player. It does not acquire a position
hold. Camera clips in other sections keep the existing position hold, including
unknown or unavailable section metadata (the existing default is section 0).
Third-person clips and NPC hold policy are unchanged.

[design] A holding first-person idle acquires the hold immediately, including its
zero-weight first frame. A holding outgoing idle keeps the hold while its blend
weight is positive. Once no holding idle is active or contributes to the blend,
the existing player-body release path restores the free position and clears the
seat hold. Thus a Movement replacement cannot release a still-visible cinematic
early, and a looping Movement overlay cannot retain a finished cinematic's hold.

## Evidence and checks

[observed: supplied dev7 source/evidence manifest, 2026-11-13] The primary worker's
bounded same-input replay reported 0 physics steps before the change and 463 steps,
763.6797 travel units after it. Its candidate aggregate diff was
`eaccfa4622117b5e0bff307f0b4d8aec63459e7536051c196a30a859b5f18cbe`.
This is reported evidence for that separate runtime, not a live check of this port
or original-game parity. No saved game, frame, animation asset or raw capture is
included here. It does not establish a general falling, sprint-mod or FPS fix.

[design: owned CPU regressions] Two behavioral ECS cases use synthetic IDLE records,
KF clips, skeleton nodes and a flat physical floor. They call the existing idle and
player-body systems, then the player plugin's update with directly supplied movement
intent. Assertions read `PlayerSeat.hold`, character feet, fixed-step
and controller-move counts. They cover a looping Movement overlay, cinematic
zero-weight starts and repeated requests, and a cinematic-to-Movement fade/release.
The behavioral fixture uses existing interfaces on unchanged main; it does not call
the new hold predicates or write the per-playing policy flag. Three additional
candidate-only slot tests cover classification and empty/third-person negatives.
All five tests are authored. This port is based on main `19c0f08a`; no tests or metadata compilation have been run on this port. Behavioral execution remains pending. The ECS
fixture does not verify the full application's plugin ordering. Release tests,
strict Clippy and selected engine smoke remain required before acceptance.

[hypothesis: unmeasured boundary] This is a positional-hold correction only.
`pose_rig` still derives the camera transform from a `Camera1st` contribution.
Mouse-look behavior during such an overlay has not been accepted; the regressions
make no assertion about view ownership or rotational/animation policy.
