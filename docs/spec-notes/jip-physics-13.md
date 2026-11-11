# JIP LN NVSE, batch 13: physics and movement (2026-11-09, #1006)

The collision objects' velocities, layers and surface and mass properties, contacts, the
ground under an actor, phantoms, materials, grabbing, collision on and off, the local
gravity, pushes, script-forced travel, moves to a node, the reticle and after a fade,
PlaceAtCell, rotations, turning, and the reticle's node and position, in
`crates/fnv-game/src/nvse_jip13.rs`. Sources: the GECK wiki export, each function's page
[documented: the page named]. The probe set `jip13` (`tools/capture/jip_probes.py`, lab
`L5.ext.jip13`) asks the original what the [hypothesis] values below leave open.

39 functions; JIP 102 → 63 on main, 29 left once batch 12 (#1929) is
merged too [observed: the extender_census test, 2026-11-09].

## 0. How the physics is reached

The engine's physics answers through a view of the references the scripts asked about:
a reference's first ask watches it and finds nothing; from the next frame the engine
tells its bodies, contacts, whether the player could grab it and an actor's ground
(`nvf_play::jip_physics`) [hypothesis: as the node functions' scene mirror, a frame
late]. What the scripts change is a request the engine does the same frame
(`Game::take_phys_requests`). The bodies come from `fnv_phys::script`: each body by the
node its collision object hangs on (`CollisionObject::node`, now kept per body),
velocities, layer, friction, restitution, damping, inertia and mass.

Units: linear velocity in JIP's "0.1 Meter/sec, or 7 game units/sec", angular in "2π
rad/s", that is revolutions a second [documented: GetLinearVelocityAlt,
GetAngularVelocity]; mass in kilograms [documented: GetCollisionObjProperty].

## 1. Velocities, layers, properties

| Function | NVForever |
|---|---|
| GetAngularVelocity, SetAngularVelocity `node axis [v]` | the node's moving body, world axes [hypothesis: the page names no axes for these] |
| GetAngularVelocityAlt, SetAngularVelocityEx `node x y z [global]` | local axes unless the flag [documented]; 0 and the variables left without a moving body [documented] |
| GetLinearVelocityAlt, SetLinearVelocity `node x y z [local]` | world axes unless the flag [documented] |
| GetObjectVelocity `[axis]` | game units a second on the axis, or the speed (the page's "product" read as the vector's length [hypothesis]); a moving body's, else the measured movement GetActorVelocity uses [hypothesis] |
| SetActorVelocity `axis v` | for a frame [documented], as the console's SetVel: up or down, or a short push along the ground |
| GetRigidBodyMass | 0 for an actor [documented]; the heaviest moving body's mass up to 100, 100 for a body that does not move [hypothesis: the page's "100 translates as no movement"], 0 without collision |
| Get/SetCollisionObjLayerType `node [layer]` | the body's layer, 1-42, "*" every body [documented]; a set layer changes what queries and layer filters see [hypothesis: the solver's contacts stay] |
| Get/SetCollisionObjProperty `node id [v]` | 0 friction, 1 restitution, 2 linear and 3 angular damping, 4-6 inertia, 7 mass [documented]; the inertia is read but not set [decision: rapier derives it from the shapes and the mass] |

## 2. Contacts, ground, materials

| Function | NVForever |
|---|---|
| GetContactRefs, GetHasContact `ref/list`, GetHasContactBase `base/list`, GetHasContactType `type` | the references the reference's colliders touch (contact distance 0.5 or less) [documented]; type codes GetType's [documented: Form Type IDs] |
| GetHasPhantom | 0 [observed: Doc Mitchell, against the page's "always return 1 when called on an actor"; decision for objects: NVForever keeps no phantom shapes but water's] |
| GetGroundMaterial | -1 for a non-actor or in the air [documented]; else a ray 24 units down from the feet: its material in the page's table, 0 stone to 9 hollow metal, by the impact material the collision code stands for (`fnv_combat::fx::impact_material`) [hypothesis]; the terrain dirt [design] |
| GetObjectUnderFeet | what that ray meets, none for the terrain [documented] |
| GetRayCastMaterial `[flag] [node]` | a ray from the reference (64 above an actor's feet) along its facing; -1 for no hit [documented]; the flag and node are not used [decision: NVForever's casts meet everything but the player] |
| IsGrabbable | a moving body of 100 kg or less, not an actor [hypothesis: the grab's walking limit, `nvf_play::grab`] |
| ToggleObjectCollision `on` | every collider of the reference off or back on [documented] |
| SetLocalGravityVector `x y z` | the moving bodies' gravity, game units a second squared [documented]; for the exteriors, or for the interior the player is in until the player leaves it [documented: "reset once the cell is reloaded"]; the characters keep their own fall [decision] |

## 3. Pushes and travel

| Function | NVForever |
|---|---|
| PushActorAwayAlt `force [x y z]` | knocked down and thrown away from (negative: toward) the actor's place moved by the offsets [documented]; not the dead [documented]; PushActorAway's speed for the force, 30 units a second per point [hypothesis] |
| PushActorNoRagdoll `force angle [ref]` | shoved along the Z angle, or away from the reference, without a fall [documented]; the same speed, slowing to rest over SetVel's 0.5 s [hypothesis] |
| TravelToRef `ref [flags]` | a Travel package to near the reference, made for the script and run as its script package [documented: "as in a Travel AI package"]; 1 run, 2 sneak [documented] |
| MoveAwayFrom `ref distance [flags]` | the same to an XMarker made the distance away, straight away from the reference [hypothesis: the page says "a valid position within the specified distance"] |

## 4. Moves and rotations

| Function | NVForever |
|---|---|
| MoveToNode `ref node [x y z]` | to the node's place in the target's 3D plus the offsets [documented]; 0 while the target's tree is not known yet (its first ask) [hypothesis] |
| MoveToReticle `[range] [x y z]` | to the reticle's point within the range, 10,000 by default, plus the offsets [documented]; 0 beyond it [hypothesis] |
| MoveToFadeDelay `marker seconds` | MoveToMarkerWithFade after the delay [documented] |
| PlaceAtCell `base count cellOrWorld x y z` | `count` references at the position: a worldspace's cell at that grid square, or the cell given [documented]; the last made |
| RotateAroundPoint `x y z ax ay az` | the place and angles both turned about the point by the angles [documented]; the angles taken as the references keep theirs [hypothesis] |
| SetAngleEx `x y z [method]` | 0 global angles, 1 local, 2 turn by local, 3 by global, 4 face the point given [documented] |
| FaceObject `ref`, Turn `degrees` | the actor turns on the spot to face the object or by the degrees, right positive, playing its turn clip [documented] (`nvforever::locomotion::script_turns`) |
| SynchronizePosition `[ref] [rot] [z] [node]` | the reference kept frame after frame at the target's node (its root without one), its angles too, raised by the offset; without a target it is let go [hypothesis: the page says only "moves the calling reference to a specified node"] |

## 5. The reticle

| Function | NVForever |
|---|---|
| GetReticleNode `[range]` | the node of the collision object the crosshair's ray meets within the range [documented]; empty for none |
| GetReticlePosAlt `x y z` | 1 and the point, else 0 [documented] |

The output-variable getters have no probe rows: the original's runtime compiler refused
GetLinearVelocityAlt and GetReticlePosAlt both in a `let` and as statements [observed: the
jip13 runs, 2026-11-09], as with SUP's output-variable getters [hypothesis: the same cause];
GetAngularVelocityAlt is left out with them [hypothesis: it is refused the same
way].
Five rows answer from the engine only (ground.player, underfeet, raycast, reticlenode,
contacts), so the lab's candidate, which runs no engine, differs there [observed: 12 of 17
rows match at 4f1fce74f].

## 6. Follow-ups

- FaceObject and Turn: the game's angle follows the drawn turn (the AI may face the actor
  back while it turns).
- Phantom shapes besides water, for GetHasPhantom.
- GetRayCastMaterial's collision flag and node; GetCollisionObjProperty's inertia setter.
- A layer change's effect on the solver's contacts.
