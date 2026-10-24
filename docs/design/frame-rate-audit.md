# Frame-rate audit: the mantis ootheca and the REPCONN test site

Owner's playtest list 7 (2026-10-23): item 4, shooting the mantis egg in the Goodsprings
schoolhouse drops the frame rate to single digits; item 47, the frame rate drops on the
REPCONN test site's facility floor, "not an intense location". What costs the frames
there, what was changed, the numbers before and after, and what is left.

Every number here is [observed] on the owner's desktop (32 threads) while other sessions
built and ran the game beside these runs; single windows vary by 2× with that load, so
only differences far larger than that are claimed. Logs are private
(`<scratchpad>/l7-perf/runs`).

## 1. Measuring without a sampling profiler

xperf needs elevation (docs/design/streaming-hitches.md §1), so two in-engine aids were
added (`engine/nvforever/src/profile.rs`):

- `NVF_PLAY_TEST_PROFILE=<seconds>`: every window logs `profile: <s> s, <n> frames, frame
  mean <m> ms, cpu <c> ms a frame; per frame ms: <span> <ms>, …`: the 40 costliest
  tracing spans, summed over all threads and divided by the frames (each Bevy system by
  name, the schedules, the render graph's nodes, and the spans of the libraries, rapier's
  character controller among them). The spans exist only in a build with Bevy's `trace`
  feature: `cargo build --release -p nvforever --features nvforever/profile` (keep it in
  its own target folder; it rebuilds Bevy). The `cpu` figure is the process's processor
  time over all threads (`GetProcessTimes`), which other programs on a busy machine do not
  inflate as they do wall times; it is there in a normal build too.
- The same windows log `profile scene: <n> mesh entities (<v> visible, <c> moved this
  frame), <m> distinct meshes, <u> distinct materials in use (<a> material assets)`: what
  the renderer batches by.

A span is entered and left on one thread; spans nested in one another all count (the
`update` span holds `main app`, which holds `Update`, …), so the list is read from the
top down to the first leaf that explains the time. `--log-frame-times <s>` (no vsync)
and `NVF_PLAY_TEST_SAMPLE` (stress_sample.rs, actors and audio counts) were used beside
it.

## 2. Item 4: the mantis ootheca

**What happens** [observed: `fnvtool script decompile --form 000EF406`, `esm decode`]:
the ootheca `0x0010AC54` (`V22MantisOotheca`, a container, health 1) runs
`OnDestructionStageChange`: at destruction stages 1 and 2 it calls
`PlaceAtMe NVCrGiantMantisNymph 3 1 <dir>` for four directions, 24 nymphs in all (stage
1 "Cap Damage, Destroy" with 50 self-damage a second reaches stage 2 a moment later).
PlaceAtMe puts every reference at the caller's own position (the distance and direction
do nothing, catalogue notes; `fnv_game` `place_at_me`): all 24 at the egg's centre,
against the lockers.

**Cause** [observed: `NVF_PLAY_TEST_PROFILE`, schoolhouse, egg broken by
`NVF_PLAY_TEST_SCRIPT=10:call:0x0010AC54:DamageObject:5`]: frames of 0.8–1.3 s for as long
as the nymphs lived, 1 % low 1,162 ms (0.9 fps). Per frame: `player::step` 943 ms, of
which rapier's `move_shape` 853 ms (`detect_grounded_status_and_apply_friction` 618,
`cast_shape` 243, `handle_stairs` 112). Each nymph's character started on or in the egg,
hemmed in by the lockers; every move hit something at once, and the controller ran out
its 20 iterations, each with a ground test against every collider around, for every
nymph and every one of the up to 15 physics steps a slow frame catches up on. Placing
the same 24 nymphs with the same call at a placed nymph on open floor cost 30–50 ms
frames at first, falling to 12 ms as they spread; placing them on the unbroken egg cost
the same seconds-long frames as breaking it. So the place, not the number of actors or
the destruction, was the cause. FaceGen and parts (creatures have none), AI (`residents::
drive` 3–7 ms), detection (`detection::measure` 20–40 ms at 1 s frames, 3 ms after) and
drawing (29 actors drawn in 21 ms over 10 frames) were not.

A second cost showed once the nymphs could walk: their footsteps. 24 nymphs started
about 130 footstep sounds a second, each mixed through its own copy of the room's
reverb; the mixer fell behind real time (13 s mixed in a 25 s run), sounds stopped ending
and 1,800 sound entities piled up.

**What was changed**:
- `engine/nvf-play/src/created_spots.rs`: an actor a script creates is moved, before the
  engine first takes it up (`residents.rs` `spread_created`), to the nearest free spot on
  rings round where it was put (0 to 384 units): a walkable floor that is not the caller
  (the reference standing exactly where the actor was put), no more than 24 above it,
  room for a person's capsule (else for a 12 × 40 one), in sight of where it was put, and
  32 units from every other actor. Logged `… created at (…), stands at (…), <d> units away
  (the nearest free spot)`. [engineering choice: the game's own rule is unknown beyond "it
  lands near the caller"]
- `engine/nvf-play/src/sfx.rs`: at most 64 one-shot sound effects play at once and 8 of
  one SOUN; one asked for beyond that is not started (counted, and the most at once
  logged in the `sfx:` exit line). Interface sounds are not limited. [engineering choice]
- `engine/nvforever/src/ragdoll.rs`: a ragdoll still moving 3 s after it started (or was
  roused by a hit or a blast) is set at rest where it lies (`MAX_AWAKE`; logged `ragdoll:
  actor … held at rest still moving after 3 s`). [engineering choice]

A third cost came once the nymphs reached the player: the nymph's script kills it within
50 units of the player (`GetDistance player < 50` → `KillActor player`, stepping on it),
so they died one after another at the player's feet, 23 ragdolls of about 20 bodies each
in one heap that never calmed (the calm rule of ragdoll.md §4 never held for any of them).
From 20 s after the spawn the contact solver (`init_and_solve`) took 160–310 ms a frame,
frames 200–450 ms, until the run ended. With `MAX_AWAKE` at 5 s the worst 3 s window was
84 ms a frame (`init_and_solve` 40 ms) and frames were back to 4 ms once the last corpse
was held; at 3 s the smoke run (vsync on) stayed at 17–18 ms a frame through the die-off.

**After** (same run, profile build, 30 s): the 24 nymphs stood 32–384 units round the
egg, all on the floor; frames 8–11 ms through the fight (one 2 s window at 61 ms and one
at 253 ms right after the spawn in the first version, before the spots kept their
distance from the first batch); 1 % low 55 ms, of which the start-up's 513 ms frame is
most; the sound mixer kept up (29.2 s mixed in 30 s), at most 64 one-shots at once. The
smoke run (`mantis-egg-spawn`, 50 s, vsync on, the die-off included): every 2 s window
17–18 ms a frame, 1 % low 51 ms, all 23 corpses at rest at the exit.

## 3. Item 47: the REPCONN test site (RocketLabMid)

**What it holds** [observed: `fnvtool world refs --cell RocketLabMid`]: 2,095 references
(1,064 statics, 591 items, 88 lights, 105 containers), 17 room markers and 31 portals; in
the engine 4,221 mesh entities of 1,142 distinct meshes and 1,162 distinct materials
(about one material per mesh: nothing is drawn instanced across models).

**Where the time goes** (unseen, standing at the start; the best windows): frames
15–17 ms, of which the render thread about 15 ms: command encoding and submission
(`RenderContextState::apply` 3.4, `queue_submit` 2.1, the `Core3d` graph 4.9) and
preparing the draws (`write_binned_instance_buffers` 3.6, `prepare_preprocess_bind_groups`
1.9). The main thread took about 4.5 ms and the GPU 3–7 ms (`gpu time:` lines). So the
place is bound by the CPU work of drawing ~3,450 meshes one by one, not by lights,
shadows (none indoors), particles or scripts. With the ghouls awake (26 actors, 15 dead
ones as ragdolls) frames rose to 20–29 ms: `player::step` (their characters) 2–5 ms,
`residents::drive` up to 2 ms, and their skinned meshes on the render thread.

**What was changed**: `engine/nvforever/src/rooms.rs`, room and portal culling as the
original's renderer does it (docs/spec-notes/level-design.md §3.1): with the camera in a
room, only the rooms seen through its portals are drawn. Membership by bounds (a mesh
belongs to every room its bounds touch, rooms grown by 32 units), portals followed
through screen rectangles, the camera in no room draws everything; actors, dropped items,
lights and effects are never culled; culled meshes move to a render layer no camera
sees. `--no-room-culling` turns it off; `NVF_PLAY_TEST_ROOM_CULLING_CYCLE=<s>` switches it
off and on in one run for comparisons.

**After**: from the start 11 of 17 rooms and the unbounded space are in view and 1,185
meshes hidden (3,455 → 2,420 visible); from room 0008D44B looking west 4 rooms and 2,321
meshes hidden. Screenshots with and without culling at both places are the same
(1280 × 720: no pixel differs by more than 24 at the second, 5 pixels at the first, a
flickering light) [observed]. The frame time did not measurably change: with the culling
switched every 8 s in one run, frame means 17.9 against 18.1 ms and processor time 37.7
against 34.6 ms a frame, inside the machine's noise. So hiding a third of the meshes
does not remove most of the render thread's work here; what does scale is not settled
(see §4). Smoke: `rooms-repconn`.

## 4. Left to do

- **Batching** (REPCONN, and every interior): about one material asset per mesh, and
  `NifMaterial` was not bindless, so Bevy drew every mesh with its own bind group. Done
  2026-10-23: `NifSurface` is bindless (graphics-enhanced.md §5.6); at Goodsprings the main
  view's batch sets went from 316 to 23. REPCONN not measured again.
- **What the render thread's 15 ms scale with** in REPCONN is not settled: a third fewer
  visible meshes left it unchanged within the noise. A profile on an idle machine (no
  other builds or games) is the next step; xperf when elevation can be granted.
- **Physics steps on slow frames**: a frame of 0.25 s catches up 15 physics steps
  (`phys::play_params`), each moving every character; when something else makes frames
  slow, this makes them slower still. Left as designed (it keeps actors' walking in step
  with the game clock, detection.md §5).
- **A stuck character** still costs its controller's full 20 iterations every step;
  created actors no longer start stuck, but an actor pushed into geometry would. Not
  changed (the character controller is the physics workstream's).
- The voice limit's numbers (64, 8) are guesses; whether the original limits voices is
  not known.
- **Heaps of corpses**: corpses of different actors collide with each other; a heap of
  them is held at rest after 3 s, but for those seconds the solver's cost grows with the
  heap. Letting small creatures' corpses pass through each other (or fewer bodies for
  them) would cut it further; not done (the physics workstream's ground).
