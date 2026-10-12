# Floating actors (playtest list 1 item 24, "NCR troopers floating")

Investigated 2026-10-12 (session "floating-troopers"). PR #233 checked the McCarran
troopers' placement, collision and navmesh and found nothing wrong
(`docs/owner-review.md`, far-world section, item 3); this note finds the cause in the
animation, not in the ground.

## 1. What was seen

- At Camp Golf (`--pos=29500,62000,3400 --heading 0 --pitch -10 --hour 12`, free camera)
  2CPoindexter (an NCR Misfit in trooper fatigues) stood well above his shadow
  [observed: frame before the fix].
- With `--play` at the same spot, the test aid `NVF_TEST_ACTOR_FEET` (§4) logged for
  CampGolfNCRTrooperTower1A (on his sandbag tower) and 2CMags a lowest foot bone 73
  units above the actor's root, against 6–8 for every other actor (a standing actor's
  ankle height) [observed].
- The roots themselves were where they belong: the tower trooper's on the tower floor
  (3,705; the tower's collision floor is at 3,707, the landscape 209 below, which is
  why he looks high from afar), the others within 2 units of the drawn landscape
  [observed: `fnvtool phys cell` probes and the test aid's landscape height].

So the drawn ground, the collision and the navmesh agree; the body is lifted off its
own root by about 66 units.

## 2. Cause

- In every male clip under `meshes\characters\_male` but four, the accumulation root
  `Bip01` starts at height 0 and the pelvis height (55–68) lives on `Bip01 NonAccum`
  (actors.md §8.2) [observed: all 2,149 sequences scanned with a scratch reader].
- The four exceptions are `2haholster.kf`, `2hhholster.kf`, `pa2haholster.kf` and
  `pasneak2haholster.kf` (the `Holster` of the two-handed automatic and heavy
  stances): they hold `Bip01` at a constant (0, 0, 67–68) and have no `Bip01
  NonAccum` track [observed].
- NPCs are posed by `nvf_assets::anim_play::BlendPlayer`, which took the root track's
  translation as it is and only removed the travel over a cycle. Blended over an idle
  or walk whose `Bip01 NonAccum` keeps the pelvis at ~65, the holster's 67 on `Bip01`
  stacked on top: the whole body 66 units up while the root (and its shadow-casting
  ground contact) stayed on the ground [observed: 73 − 7 = 66]. Troopers with 2ha
  weapons (assault carbines, marksman rifles) holster them, which is why it showed on
  NCR troopers. The player's body was never affected: its animator holds `Bip01` at the
  identity (`anim_blend`).

## 3. Rule (engine)

The accumulation root's height counts from where its track starts: `BlendPlayer`
subtracts the track's start z as well as the per-cycle travel
[hypothesis: in the game the accumulation root's motion is taken as a change from the
clip's start, so a constant offset moves nothing; consistent with every other clip
starting at height 0]. Only z is treated this way: furniture clips start `Bip01` away
from the origin in x/y on purpose (the getting-on and getting-off paths that
`furniture.rs` measures, e.g. `stoolenter.kf` starts at (20, −41, 0)), and those stay as
they were. Of the 2,149 sequences only the four holsters start away from height 0, so
nothing else changes [observed].

After the fix the same `--play` run logs the tower trooper's and Mags's lowest foot
bone 6 units above the root and the free-camera frame shows Poindexter on his shadow
[observed].

## 4. Test aid and smoke check

`NVF_TEST_ACTOR_FEET=<seconds>[:<range>]` (`engine/nvforever/src/actor_feet.rs`): once,
for every drawn actor within the range of the camera, the root, the lowest foot bone and
the full-detail landscape height under it, then a count of actors with a foot bone more
than 30 units above the root. The smoke scenario `floating-troopers` runs it at Camp
Golf and expects that count to be 0.

## 5. Not checked

- Camp McCarran itself was not run again (budget): the troopers there are expected to be
  the same case (NCR troopers with 2ha weapons) [hypothesis].
- In the free camera (no `--play`) actors stand at their placed height without being
  dropped to the ground; Poindexter's reference is 28 units above the landscape. With
  `--play` the physics puts walking residents on the ground; a resident that never walks
  keeps its placed height [observed in code: `residents.rs` gives physics only to
  walkers]. Not changed.
