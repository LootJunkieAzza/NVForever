# SMP physics (NV SMP Physics, native)

Item 215 (owner, 2026-11-14): the cloth and hair physics of the NV SMP Physics plugin
(an xNVSE DLL built on FSMP and Bullet), rebuilt in the engine. NVForever loads no DLLs.
Sources: the plugin's README [documented: github.com/Balebandro/NV-SMP-Physics,
README.md, read 2026-11-14; its `src/` was not opened], its shipped INI [observed:
the owner's NV SMP 1.0 download], its Nexus page (newvegas/mods/100482), and the
Vanilla Outfits for Skinned Mesh Physics pack by Zombocrom (newvegas/mods/100489, 80
NIFs, 79 XMLs) [observed]. The DLL and its .pdb were not opened. The code:
`engine/nvforever/src/smp_physics/`, `engine/nvf-assets/src/actor.rs` (`SmpExtra`,
`SmpCandidate`).

## 1. What an SMP item carries

- A string extra data `HDT Skinned Mesh Physics Object` naming its physics XML, a
  Data path (`Meshes\armor\legion\legionveteran_smp.xml`) [documented: README "How it
  works" 1; observed: on every shape of the pack's 80 NIFs]. Read from the shape, else
  from the file's root node [hypothesis: Skyrim items carry it on the root].
- A string extra data `SMP Bones` on each skinned shape: `name|parent;name|parent;...`
  in skin-instance order; the parent is empty for a bone the skeleton has
  [documented: README "The item's own bones"]. Long lists go on in `SMP Bones 2`,
  `SMP Bones 3` [observed: 20 and 1 of the pack's NIFs]; they are read in order and
  joined.
- The item's bones as nodes in the file, skin slots pointing at them [observed: 306
  `SMP ...` nodes in the pack]. The original game points slots the skeleton lacks at
  the skeleton root and drops the nodes [documented: README], so the plugin makes the
  bones itself at their bind pose against the body, carried on their parent. NVForever
  does the same: each made bone rests where the inverse of its skin bind pose puts it,
  on its `SMP Bones` parent (a skin slot, else the skeleton node of that name, else the
  root).
- `SMP Proxy:<shape>` shapes: collision proxies [documented: Nexus page, "proxies,
  which are collisions"; observed: their names in the pack]. They are not drawn.

## 2. The XML

FSMP's format [observed: the pack]: `<system>` holding `<bone name template>` (a bone
with no template and no mass is animation driven), `<bone-default name [extends]>`
templates (mass, inertia, linearDamping, angularDamping, friction, restitution,
wind-factor), `<generic-constraint bodyA bodyB template>` (inside `<constraint-group>`
or not) with `<generic-constraint-default>` templates (frameInA/frameInB,
linear/angular lower and upper limits, stiffness, damping, non-Hookean stiffness and
damping), `<per-vertex-shape>` and `<per-triangle-shape>` (margin, penetration, tag,
no-collide-with-tag, weight-threshold bone), and NV SMP's `<nv-virtual-legs fit scale>`
[documented: README "Virtual legs"]. Names match case-insensitively [documented:
README]. Unknown elements are counted and logged.

## 3. The solver [design]

Not FSMP's Bullet rigid bodies: a position-based solver of our own on the same data.

- A bone with mass is a simulated point; one without follows its parent at rest
  (anchors), and skeleton bones follow the animation.
- Per step: speed kept `(1 - linearDamping)` per second [documented: Bullet's linear
  damping, its public API docs], gravity 9.81 m/s² at 70 units per metre times
  `fGravityScale`, wind; springs; then `iSolverIterations` passes of the constraints and
  collisions.
- A constraint with no linear freedom (warp, link: lower = upper = 0) holds `bodyA` at
  its rest distance from `bodyB`, its direction within the widest x/y angular limit of
  its rest direction in `bodyB`'s frame, and pulls it back with the mean angular
  stiffness. For a simulated `bodyB` the frame is `bodyB` unswung (its parent's turn
  times its rest turn), so the stiffness and limits act as the swing of the joint above.
- A constraint with linear freedom (weft) keeps the bodies within the widest linear
  limit of their rest distance, pulled toward it with the mean linear stiffness.
- A stiffness of 1 is 30/s² of pull per unit of offset [tuned by eye]. A bone's turn is
  its rest direction to its first child turned onto the current one.
- A frame takes `ceil(dt × iMinFps)` steps, at most `iMaxSubSteps`; a longer frame's
  rest is dropped [documented: INI comments]. The driven bones move smoothly between
  the last frame's pose and this one's over the steps.
- A system whose point gets farther than 6 rest lengths from its parent, or not finite,
  starts over at rest.
- Only actors within `fCollisionDistance` of the camera whose sphere (64 units above the
  root, radius 100, times the actor's scale) is in the view are simulated. The others'
  cloth holds its last pose on the actor, as in photo mode, and goes on from there when
  they come back [design: owner's pick 2026-11-17, for the cost of a Strip crowd; NV
  SMP's INI keeps physics without collision beyond that range, so a far actor's cloth
  may differ from the original's, which is not checked against the original game].
- The systems step in parallel, one task each; what a step reads (the driven pose, the
  body capsules, the garments under a poncho) is gathered first and the bones are
  written back after, so the result is the same as stepping them one by one.

## 4. Collision

- Body: capsules on the skeleton (pelvis to neck, neck, thighs, calves, upper and lower
  arms) and a sphere for the skull [design: radii fitted by eye to the vanilla body]. Each
  system shrinks a capsule where its own bones rest closer to it (never below 35%), so
  the rest pose starts clear. `nv-virtual-legs`' scale scales the leg capsules
  [documented: README; the README fits the radii to a named shape, here fixed]. The
  XML's per-triangle proxies are not used. A bone collides with the body unless its
  per-vertex shape has `no-collide-with-tag body`; its radius is that shape's margin
  (1 when none names it).
- Ground: a plane at the actor's feet (its root's height) [documented: README
  "Ground": flat at the feet, approximate on slopes]; off with `bGround=0` or a shape's
  `no-collide-with-tag ground`.
- Only the `iMaxActors` actors nearest the camera within `fCollisionDistance` collide;
  the others keep their physics without collision [documented: INI comments]; here those
  past the range hold still (§3). The player is nearest the camera.
- Not built: cloth against cloth, impact sounds (`<impact-sound>`), the jitter filter.

## 5. Settings: `NVSMPPhysics.ini`

Read from the load order's `nvse\plugins\NVSMPPhysics.ini`, else the shipped values:
`bEnabled`, `fCollisionDistance` 1500, `iMaxActors` 10, `iMinFps` 100, `iMaxSubSteps` 4,
`iSolverIterations` 10, `fGravityScale` 1, `fResetAngle` 120, `bWind` 1, `fWindStrength`
0.08, `bWindInInteriors` 0, `bCollisions`, `bGround` [observed: the shipped INI].

- Wind: the weather's wind as the trees get it (`tree::wind_vector`: WTHR wind speed,
  none indoors) times `fWindStrength / 9.8` of gravity [documented: INI "compared with
  9.8 for gravity"], with a slow gust [design], times each bone's `wind-factor` divided
  by its mass (at least 0.05) [hypothesis: FSMP runs on Bullet, which takes forces per
  body; neither NV SMP's README nor FSMP's wiki says]. At the shipped 0.08 a weather's
  wind barely moves cloth (NVWastelandClearWindy is wind speed 50 of 255; a 0.25-mass
  duster bone gets about 1/150 of gravity there); a stronger wind is the INI's (or the
  plugin's F7 menu's) to set. With no `fWindStrength` in the load order NVForever uses
  1, the owner's choice (2026-11-15) over the plugin's documented 0.08 [design]; an INI
  that sets it still wins.
  `bWindInInteriors` is not built (indoors is calm).
- Resets: an actor whose root moves more than 150 units in a frame (a teleport) or
  turns faster than `fResetAngle` degrees a second starts over in its rest pose
  [documented: INI comment; the distance is design].
- Not built: the F7 menu, the collision threads.

## 6. The generator (items with no SMP data)

Owner decision 2026-11-14 (item 215): CKR's hair and Qwib's accessories come with no
physics bones [observed: the CKR hair NIFs are rigid NiTriShapes with no skin; Qwib's
poncho is skinned to the body's bones only], so their chains are generated at load time
from the owner's own meshes; nothing derived is kept or shipped. No SMP tool's code is
used.

- CKR's hair (rigid hair-shader shapes under `meshes\bale\hair\` [observed: `fnvtool
  bsa list` of CharacterKitRemake-Hair.bsa]; the vanilla hair has no SMP data and stays
  rigid, as with the plugin): vertices more than 5 units below the
  head bone's bind height swing; the crown keeps its shape. 10 sectors around the
  hair's vertical axis; each reaching at least 6 units down gets a chain (links every
  ~4 units, at most 6), the shape is skinned to it with a 4-unit fade, and the links of
  neighbouring sectors are tied by springs [tuned by eye].
- Qwib's poncho (`meshes\qwib\poncho\`): the lower 55% of its height swings from
  `Bip01 Spine2`, apart from what is weighted to the arms, neck and head; 12 sectors,
  links every ~5 units. The bandana, bags and hats keep their shape [design].

## 7. Co-op

Every machine simulates the actors it draws; nothing of the physics is sent. A guest's
copy of the host's player and of NPCs moves by the replicated animation and its cloth
follows locally [design: the README's plugin is per game too].

## 8. Checked in game (2026-11-15)

Goodsprings at noon, third person side-on, walking, running, turning and stopping, with
the owner's Vanilla NV SMP pack, CKR hair and Qwib's Accessory Pack (frames kept
privately) [observed]:

- Sheriff's Duster (the pack's lucassimms outfit, 44 simulated bones): at rest the coat
  hangs straight; running, its tail flares out behind the legs; stopping, it swings
  forward over the shins and settles within about 2 s. With `bEnabled=0` the hem
  follows the thighs as plain skinning does. No stretch, no blow-up, nothing through
  the legs.
- Legion Veteran Armor (2 systems, 78 bones): the skirt strips flap while running and
  settle at rest.
- Qwib's Red Poncho (generated, 49 bones): the side panel streams back while running and
  hangs at rest as the unsimulated poncho does.
- CKR Mullet (generated, 24 bones): the tail at the nape lifts behind while running, the
  side locks swing forward on stopping and settle.
- Wind: at the shipped `fWindStrength` 0.08, NVWastelandClearWindy barely moves cloth;
  at 3 the duster's hem lifts with the gusts.
- Vanilla data rigs nothing (smoke scenario `smp-vanilla-stays-rigid`).
