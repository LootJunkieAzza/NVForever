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

## 6. Boots above the ground (owner report 2026-10-16)

The owner's third-person screenshot in Goodsprings showed the player's boots hanging a
little above the terrain, "most places".

- The character controller rests the feet the support's contact shell above the bare
  surface: 3.5 units over terrain, 0.7 over static models (`CharacterParams::rest_gap`,
  physics.md §6.2) [observed: in-game rest heights after door teleports and drops]. The
  reference's position in the game is that height too, so the physics keeps it.
- The bodies (the player's third-person rig, and NPCs that have a character) were drawn
  with their root at the feet. A standing body's soles are at about its root's height
  (ankle bones 6–8 units up, §1), so they hung the rest gap in the air: 3.5 units over
  terrain, which is most of the outdoors; on floors 0.7 hardly shows [observed: the
  probe's root height over the drawn landscape, §4].

Rule (engine): the body is drawn the character's rest gap below its feet (over a
model, on its drawn floor instead: §7) (`nvf_play::phys::drawn_sink`; not while swimming, whose float depth counts from the
feet); the feet, the reference's position, the eye and the camera stay where they were
[hypothesis: the game draws the body on the bare surface, from the owner's report that
it does not float there; not measured in the game]. Furniture places bodies its own
way and is unchanged; residents without a character stand at their placed height (§5).
A ragdoll that starts from the drawn bones is raised by the same gap
(`PhysState::drawn_sink_of`, `ragdoll.rs`): started inside the ground's contact shell,
the player's death ragdoll never came to rest (smoke `death-cam-third` failed 3 of 3;
with the raise it passes as on main, where that rest check is itself flaky: main failed
`death-cam-first` once) [observed].

Before the fix the player's drawn body stood 3.7 units over the drawn landscape on Camp
Golf's open ground (`--pos=29500,62000,3400`) [observed: probe, §4]. The smoke scenario
`feet-on-ground` puts the player in third person there and expects the drawn body's root within 1.5 units of the drawn landscape.

## 7. Easy Pete sunk into the saloon porch (owner report 2026-10-17)

The owner's screenshot showed Easy Pete (EasyPeteRef 0x00104C80) on the Prospector
Saloon's porch with his boots below its boards. Which of the suspected causes it was:

- **Not his placement.** His reference is placed at z 8391.5, 7.9 above the porch's
  collision; a resident that never walks stands there (§5), above the boards, not in
  them. Once his character has walked he stands where it rested [observed: the test
  aid's sim position, 8384 after a walk].
- **The porch's collision lies under its drawn boards.** `nv_prospectorsaloon.nif` (STAT
  0x0010243E, reference 0x001055E0) has one packed-strips collision plane at z 8383.6
  under the whole porch; its drawn shape `NV_ProspectorSaloon:2` draws the boards with
  their top at 8385.3–8387.1 (8386.1 to 8387.1 around Pete's spot) and their underside
  2.6 below the collision [observed: the decoded NIF's render strips and collision
  strips at the reference's pose; `fnvtool phys cell` rays and a dropped probe agree on
  8383.6]. The navmesh there is at 8386.2, on the drawn top [observed: `fnvtool nav
  near`].
- **The sink was right for the collision, wrong for the drawn floor.** A character
  rests 0.7 above that collision (8384.3); the rule of §6 drew the walking body 0.7
  below its feet, on the collision, 2.5 under the boards' top; a resident standing
  after its walk was drawn at its feet, 1.8 under it [observed: probe].
- **The game puts him at the collision too.** Over hours of AI observer captures the
  game reports Pete standing on the porch at z 8384.6 (sitting in his chair, 8387.3)
  [observed: `capture/out/obs_*.jsonl`, private], the rest band of physics.md §6.2 over
  8383.6. So the reference's position is not what lifts him in the game; that the game
  draws him on the boards is the owner's observation, and how it does is not known
  [hypothesis: on the drawn surface, as below].

Rule (engine): the body is drawn on the **drawn ground** under the feet. Over terrain
that is the collision's own surface (the landscape draws the heights it collides with),
so §6 is unchanged there. Over a placed model whose collision stays where it was placed
(not dynamic, not a door that opens in place) it is the highest of the model's drawn
floor-like triangles (facing up or down more than 60° from the side) within 4 units of
the collision under the feet (`DRAWN_FLOOR_WINDOW`); with none there, the collision
(`nvf_play::phys::drawn`, `PhysState::drawn_ground`, `drawn_sink`) [hypothesis: from the
owner's report; the window is a design choice]. Drawn floors are read with a model's
collision, from the shapes the renderer draws for a placed reference (not skinned, not
hidden, not editor helpers, below the root's own transform), and are not looked at for
a reference tilted more than about 5°. The body can then be drawn above its feet (a
negative sink): on the porch 2.8 above. A resident standing without a character is
drawn the same way when its feet are where a character rests (the contact shell above
the collision under them, within 0.5: after a walk, MoveTo onto that height); others
(a placed height, the navmesh's height) stay at their feet
(`PhysState::standing_drawn_feet`). Foot IK (actors.md §13) puts each foot on the same
drawn ground (`PhysState::drawn_over`), or it lowered the raised body back onto the
collision.

The test aid `NVF_TEST_ACTOR_FEET` (§4) chose the first `Camera3d`, and when the water's
reflection camera came first it reported no actor in range (so `floating-troopers` could
pass without looking at anyone); it now leaves that camera out, and with `--play` also
logs the collision and drawn ground under each actor.

Smoke scenarios: `pete-porch` holds Pete (SetRestrained) and puts him by MoveTo at his
placement's x and y at the height the game recorded him at (8384.6), looks at his feet,
and expects his root on the boards' top (8387.1 ± 1) [observed: before the fix 8384,
after it 8387.1; screenshots `pete_porch.png`]. Its twin `pete-terrain` puts him on the
bare landscape in front of the steps at the terrain rest height and expects his root on
the drawn landscape (0.0 above it after the fix; the terrain shots before and after are
alike). `feet-on-ground` and `floating-troopers` pass with the change.
