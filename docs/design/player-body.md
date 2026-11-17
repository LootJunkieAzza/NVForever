# The player's body (`--play`): animated locomotion, weapons, first- and third-person views

Status 2026-10-09: on `claude/player-body` for review. Owner requests 2026-10-09:

1. Using the NPC assembly and animation playback (docs/design/static-actors.md parts B,
   C, E), a player character that moves with the animations, with the camera tied to the
   head; locomotion forwards, backwards and sideways; crouching, and moving while
   crouched; drawing and holstering weapons.
2. Then (owner decisions on the first version's questions): **match all the New Vegas
   controls**; **use the game's separate first-person arms model** ("with armours in
   first person, if the nif is not called arms it's not rendered"); **holding the middle
   mouse button circles the camera around the player** without turning them.

Where it sits: the player's own body is Layer 4 play (`nvf-play` owns the controls and
the camera) drawn with the Layer 3 actor assembly (`nvforever`); choosing clips for an
actor under direct control is the small, player-only part of Layer 6 animation
selection that this needs. NPC animation stays as it was (one idle clip, actors.md §7).
Facts: docs/spec-notes/animation.md §9 (clips, first-person rig), docs/spec-notes/
controls.md (bindings).

## 1. What the player sees

### 1.1 Two rigs

- **Third person**: the player's NPC_ (FormID 7, `fnv_sim::FormId::PLAYER_BASE`)
  resolved like a placed actor (`fnv_actor::Resolver::resolve_wearing`) on
  `characters\_male\skeleton.nif`: race body, head and FaceGen face, hair, eyes, wearing
  the armour the simulation has equipped on the player. With nothing worn the race's
  upper body shows: its skin shape with the tinted skin texture and its underwear shape
  (grey T-shirt, dark shorts) with its own texture (actors.md §3.1, §4.1). It stands at
  the character's
  feet, faces the view's heading, and carries no `RefVisual` (the crosshair never
  targets it). Shown in third person and while the camera circles the player.
- **First person**: the game's arms rig, `characters\_1stperson\skeleton.nif`, with its
  own clips (`characters\_1stperson\`). It draws the hands (their `…1st.nif` meshes) and
  only the `Arms…` shapes of the upper body and of worn upper-body armour, with the
  outfit's left forearm sleeve, `PipBoyOn…` or without a Pip-Boy `PipBoyOff…` (#2173)
  (gloves and the Pip-Boy whole; no head, hair or other
  armour; skin tinted as in third person; `fnv_actor::first_person`), and the weapon's
  first-person model (WEAP `WNAM`, a STAT). `Bip01 Looking` turns with the view's pitch,
  so arms and weapon stay in front of the camera, which sits at the rig's `Camera1st`
  node: 118 units above the feet, 78 crouched. Shown in first person.
- An outfit change in the inventory menu rebuilds both. Without the first-person
  skeleton the third-person body stands in (its head's eye gives the view and its head
  is hidden), as in the first version.

### 1.2 Controls (the game's, docs/spec-notes/controls.md)

| Control | Default | Here |
|---|---|---|
| Forward / Back / Slide Left / Slide Right | W S A D | the forward, backward, left and right clips; diagonals blend two clips; clips play at the speed the character actually moves |
| Run / Always Run | Left Shift / Caps Lock | running is the default (Always Run on); Shift held walks; Caps Lock swaps the default |
| Auto Move | Q | walks forward until Forward or Back is pressed |
| Jump | Space | jump loop in the air, landing clip after 0.35 s of falling |
| Crouch/Sneak | Left Ctrl | crouched idle and movement at `fMoveSneakMult` (0.57) of the speed |
| Ready Item | R | weapon away: draw it; weapon out: tap = reload (combat C4), hold 0.5 s = holster |
| Use | left mouse | weapon away: draws it; weapon out: the trigger (combat C4: guns, launchers, thrown weapons; melee is C6) |
| Block | right mouse, Left Alt | held: guns aim down the sights (`AimIS`), fists and melee block (`blockidle`) |
| Toggle POV | F, middle mouse | tap: first/third person; held while the mouse moves: the camera circles the player (not turning them) and swings back when let go; wheel zooms in third person |
| Activate | E | as before |
| Menu Mode | Tab | tapped, the Pip-Boy (docs/spec-notes/pipboy.md; the inventory when this was written); held 1 s, the flashlight on or off (`nvf-play` `flashlight.rs`, owner request 2026-10-11) |
| Ammo Swap | 2 | the next carried ammo type down the weapon's list, then a reload (combat C4) |
| Rest | T | the Wait menu, or the Sleep menu while lying on a bed (docs/spec-notes/wait-menu.md) |
| Vats, Hotkeys 1 and 3–8, QuickSave F5, QuickLoad F9 | | bound; pressing one shows "… : not available yet" |

Scripts can turn groups of these controls off and on (`DisablePlayerControls`,
`EnablePlayerControls`): `control::ControlLocks`, docs/spec-notes/controls.md §5.

Mouse look uses the game's `fMouseSensitivity` (0.002 radians per count) and
`bInvertYValues` (off). The F9 test aid that opened the nearest container moved to F10
(F9 is Quick Load).

### 1.3 Animation

Both rigs play layered clips (`nvf_assets::anim_blend`) chosen by
`fnv_actor::player_clips`, each from its own folder: an idle, movement, sneaking, swimming,
jumping and landing, turning on the spot, and the weapon drawn (equip clip; at its
`Attach` key the weapon model moves from its holster to the right hand), held (Aim; in
third person bent up and down with the pitch by `aimup`/`aimdown`), aimed down the
sights or blocking, and holstered (unequip clip; at `Detach` it goes back: third person
to the right hip for pistols and across the back for rifles, first person to
`Bip01 Translate`, out of view). Crouched, the crouched variants are used where they
exist. The draw/holster timing follows the clips of the rig in view.

### 1.4 Camera

- First person: at the first-person rig's `Camera1st` node, looking along the view.
- Third person: behind the player at the wheel's distance (60–450, 170 to start), offset
  by `fOverShoulderPosX` (40, right) and `fOverShoulderPosZ` (−10, up) from a pivot above
  the feet at the third-person head's eye height; pulled in front of collision.
- Circling (Toggle POV held, mouse moving): the third-person camera turned by the
  circling angles, the shoulder offset fading to the centre as it turns away from
  behind; from first person the view shows the third-person body meanwhile. Let go, it
  swings back behind (time constant 0.15 s) and first person returns.
- Detached (the settings page's Gameplay switch "Detached third-person camera", off by
  default; the owner 2026-10-27): in third person with the weapon away the mouse turns
  the camera, kept at its full shoulder offset, and not the player; moving, the player
  turns to the pushed way (a sprung turn that eases in and out, a half turn
  settling in about 0.6 s) as the camera sees it and walks straight ahead, so
  Back runs towards the camera. Drawing the weapon (or first person, or the switch off)
  turns the player to the camera's heading at once. The crosshair follows the camera's
  line meanwhile [design: not in the original; `control::detach_camera`].
- The crosshair's ray starts at the eye (`Player::eye`, the `Camera1st` position), so it
  matches the first-person view; in third person it still starts there [engineering
  choice; the game's third-person pick is not known].

### 1.4a Sway and bob of the arms (owner request 2026-10-20)

The original's first-person arms and weapon lag behind a turn and bob as the player
walks, but none of it is in the files: the first-person movement clips move only the
root (`mt`) or have keyless tracks (`1hp`), there are no `2hr`/`2ha`/`2hh` movement
clips in `_1stperson\locomotion`, and `Camera1st` hangs from `Bip01 Looking` beside the
arms, so a clip moving that chain would move the view too [observed: `fnvtool nif
decode` of the clips and `_1stperson\skeleton.nif`]. The executable names
`fFirstPersonHandFollowMult`, `fFirstPersonHandChaseSeconds` and
`fFirstPersonHandChaseSecondsAttack` [observed: its strings], so the hands chase the
view by code [hypothesis; the values are not known]. NVForever's sway
(`engine/nvforever/src/view_sway.rs`) [design, constants tuned by eye]:

Since the movement-inertia work (owner, 2026-11-13: the gun "moves with the direction
using a ramp curve - slow start, ramps up to a faster movement then smoothly slows") the
weapon hangs from the view on critically damped springs (`nvf_play::ease::Spring`,
solved exactly per frame, so alike at any frame rate): it follows a change slowly at
first, faster, then settles without overshoot, and its own motion never jumps. The view
itself turns exactly as the mouse says.

- Turn lag: the head turns first and the body with the arms follows (the owner's
  playtest of 2026-11-13: "the human body rotates its head first, then the lower body
  catches up"). Driven by the view's turn **rate**, not the angle turned (as Source's
  `CalcViewModelLag` and spring-based first-person rigs: a research pass of the same
  day): each frame's turn asks for a lag of rate × 0.12 s; a first spring averages that,
  a tanh bends the average towards 6° sideways and 4° up or down, and a second spring
  follows it (bending each frame's rate first lost most of an uneven turn: a 90° flick
  traced at 0.7°, now 5.6°); both springs at 0.6 × 0.5 s (0.35 s until the seventh playtest: "slower ramp up and slow smooth ramp down"), one constant rate out and back (Daniel Holden's
  "double spring": the pair also smooths the acceleration, which a lone spring jumps at
  each uneven mouse frame; second research pass, 2026-11-14). A fast turn builds the lag
  up gently, the arms never pass the limit, and when the view stops they start back at
  once and rest on the view without swinging past it; the weapon tilts a little into the
  lag. History: 0.11 s, 5° and 3.5° on an angle-driven spring in the first build
  ("whips" with the view); 0.3 s and 12°/6° in the second (reached the limit at once,
  then "waited"); a rate filter with separate build and release times and separate out
  and back springs in the third and fourth ("transitions between different spring
  values … quick ramps in speed and a delay", the fifth playtest). Since the ninth
  playtest ("there is still a delay between the mouse stop and the player hands
  returning … make the curve continuous so the hand never reach the maximum point"): the
  lag asked for is rate × 0.15 s, the first spring only averages it over 0.06 s, the limit
  is `x / (1 + |x| / limit)` (no flat top: a faster turn always asks a little more), and
  the second spring takes 0.4 s × 1.2 (`view_sway::TURN_WEIGHT`, the owner's "1.2x for
  mouse movement reactions"). The first of the two 0.3 s springs had held up to 14 times
  the limit during a fast flick, which the tanh hid while it drained: about 0.2 s of
  standing still at the limit. The trace after: the arms start back within 0.05 s of the
  view stopping and are on it about 0.5 s later. Since the tenth playtest ("the idle
  weapon/hand animation micro jitters a lot, when I aim down sights it jitters more
  visibly") a slow averaged rate asks for less: the lag is × `r² / (r² + 0.1²)` of the
  averaged rate `r` (radians/s; `view_sway::TURN_KNEE`). A hand resting on the mouse sends
  single counts now and then, and the arms had swum against the view up to 0.05° with them
  (about 1.7 px at 2160 lines: trace `movement-inertia-trace-still-mouse`, the tenth build);
  now 0.001°. A flick is as before; a slow 8°/s turn lags 0.7° instead of 1°. Screenshots
  50 ms apart held still (aimed, Pip-Boy up; with and without DLSS, at Goodsprings'
  x = -71680 and indoors near the origin) moved at most 0.02 px: the drawing itself does
  not shake.
- Bob: a dip at each footfall and one sideways swing a stride, at the phase of the
  movement clips (the footstep keys), growing with the ground speed to a run's
  (300 units/s): 0.9 units down, 0.6 across, 1.2° of tilt. Sneaking 60 %, down the sights
  20 % (the turn lag 35 %), none in the air.
- Movement lag: a sprung hand pushed by the body's acceleration (spring rate 3.8/s; 10/s
  until the fourth playtest, "the gun slides to the sides too quick", 4.5/s until the
  seventh, "make the gun return to the default position smoother"): starting,
  stopping, strafing and turning while running leave the weapon a little behind (a
  sudden change of 344 units/s would peak at 1.7 units; shown at most 1.2) and a steady
  pace brings it back; behind it also drops half a unit per unit and tips the muzzle
  1.2° per unit ("a bit of inertia to front and back movement"); it leans into a strafe
  (1.5° at a run) on a spring settling in 0.6 s.
- The hand's and the head's pushes take the body's velocities through a short spring
  first (0.12 s; `nvf_play::ease::Pushed`), whose rate is the acceleration that pushes:
  a take-off threw the gun 3.5 units within two frames (peak 39 units/s), now 22.
- Jumps and landings: the same hand pushed by a take-off and a landing (4/s; 12/s, 7/s
  and 5.5/s before, softer each time): each dips the weapon about 2.3 units, more the
  harder the landing (shown at most 3.5 units; 2 before the fifth playtest, "jumping is
  not noticeable"); the muzzle tips 1° per unit of dip. Only a take-off (leaving the
  ground going up) and a landing push (`nvf_play::ease::carried_push`): in the air the
  body and the hand fall alike, and with the softer springs the fall's steady push had
  kept growing. Until the tenth playtest gravity's share was taken out of each change of
  speed in the air instead; a fall not quite free (down a slope's face, about 10% under
  gravity) still pushed, and walking off a slope counted its whole speed as a push.
- In the air the muzzle tips down 6° (tenth playtest: "when jumping the muzzle of the gun
  should point more towards the ground, but only the muzzle if possible"), turning about
  the grip (`view_sway::GRIP_PIVOT`, the right hand's place in the view, about 10 right,
  15.7 ahead and 14.6 down) so the gun's back end stays put; from the take-off (rising
  faster than 100 units/s) or once in the air, on two springs of 0.15 s each (× the
  weights), none down the sights.
- The first-person landing clip (it moves `Camera1st`: the view dips about 6 units and
  stands up again, the standing up in 0.4 s at the clip's own pace [observed: the motion
  trace; the head's own landing spring gives 2 of the 8 units]) is played by its
  place along the settling curve over 1.8 × its length
  (`inertia::LAND_STRETCH`): the dip comes as fast as before (0.3 s), the standing up
  takes 0.7 s and slows to its end (seventh playtest: "make landing after jumping
  smoother - the little stand up to normal position after the camera dips"). Since the
  eighth ("fall-scaled landing") the stretch grows with the fall speed, 1.8 up to 350
  units/s (a hop) to 3.2 from 900 (`inertia::land_stretch`), and a hard landing pushes the
  head's and the hand's springs up to 1.6 × as hard (`inertia::hard_landing`; the most
  shown 9 and 6 units, were 5 and 3.5): a hop lands as before, a drop at about 1,000
  units/s dips the view about 13 units and the gun 6 and stands up over 1.3 s (trace,
  smoke `movement-inertia-trace-combat`).
- **The settling curve** (seventh playtest: "we should not have any linear curves -
  everything should be a smooth spring with a slow finish"): `anim_blend::settle`, a
  critically damped spring's step response made to end at 1 (`1 - (1 + 6x)e^(-6x)`,
  normalised): no jolt at the start, more than half the way in the first third of the
  time, the last quarter of the time for the last 5 %. With movement inertia every
  first-person clip fade follows it (`anim_blend::Layer::smooth`; a layer leaving runs it
  mirrored, and one turned back mid-fade carries on from where it is shown), and so do
  the true iron sights, the aim zoom, the draw and holster clips and the landing clip
  (`nvf_play::ease::Settled`, a timeline along the curve that turns back without a jump).
  Standing, walking and running clips cross over 0.45 s (`inertia::MOVE_FADE_SECONDS`;
  their own times are about 0.2 s, straight), the standing and sneaking ones 1.0 s down
  and 1.1 s up.
- No turn clips in first person with movement inertia: starting and ending with the turn
  rate, they cut into the hold and the aim ("aiming down sight during [a turn] … the
  animation resets and kinda skips"); the turn lag above is the arms' turn.
- The calmer shares down the sights and sneaking (bob, lag, breathing, recoil) come and
  go on springs settling in 0.5 s (`view_sway::STANCE_SHARE_SECONDS`), the head's too:
  switched at once they stepped the hand the frame the stance changed (0.37 units at an
  aim's end in the trace).
- Aiming and blocking (sixth playtest: "points the gun and then put it to the eye level
  in 2 different movements"; a motion trace showed the 0.35 s blend ending, the sights
  clip's own rise carrying on for 0.25 s, and the sway's aim lag adding a settle bump):
  the aim and block clips cross over along the settling curve (below; a smootherstep
  until the seventh playtest, which read as "linear") over 0.6 s in and 0.55 s out, the
  leaving clip over the same time, and the true iron sights and the aim zoom
  (`nvf_play::aim_zoom`) move over exactly those times on the same curve, so going down
  the sights and back is one motion each, fast at first and slow to its end; the aim
  adds no lag of its own any more. The clips are not changed.
- Weapon switching: drawing and putting away (6 units, smoothed by a spring settling in
  0.25 s) push a spring (8/s): the gun trails and settles (shown at most 1.5 units); with
  Weapon Handling the draw and holster clips take 1.5 (light) to 2.4 (heavy) times the
  original's time at Agility and Strength 5 (`inertia::draw_weight`; 2.2 to 3.3 times
  until testers found it "like pulling out a concrete slab", 2026-11-15), the attributes
  then speeding them up by the weapon's class (§1.4a.3, Attributes), played by their place
  along the settling curve (`Frame::equip_at`), and the game's readiness waits for them
  (the moment the weapon changes hands is found through the curve's inverse). Over the
  last 65 % of that time the clip hands the arms over to the pose that follows (the hold
  after a draw, the arms down after a holster, which is under the put-away clip all
  through it) along the settling curve, while it still slows (`EQUIP_HANDOVER_FROM`); the
  clip starts at once, the arms easing across the step to its first pose; the trail push
  follows the draw's own progress. Played out first and faded after, the draw's last pose
  (3 units off the hold) made a second movement once the arms had nearly stopped, the
  holster ended in a snap onto the arms-down pose, and the step at the draw's end pushed
  the gun 1.4 units down and back (ninth playtest: "they slow down but then the animation
  speeds up and player character grabs the gun quickly"; the trace). The hold
  clip is asked for through the draw, under the draw clip: asked for only after it, it
  was still fading in while the draw clip faded out, the arms-down idle showed through
  and the gun went down and came up again (seventh playtest: "starts pulling out the
  gun then quickly retracts it and pulls it out quickly again"). An equip clip leaving
  takes the movement clips' 0.45 s.
- Draw hitches (owner, 2026-11-15: "pistol drawing animation is not smooth at all, it
  hitches multiple times"; the trace `movement-inertia-trace-weapons` with its layers' file
  `<trace>.layers`, each frame's layers, fades and seams):
  - The hold fades in under the draw at its own pace and shares a priority with the draw
    clip on the spine and the arms. A draw that let go faster left the two short of the
    whole (98.7 %), and the arms-down idle under them showed through: the arms sagged
    towards it and came back (both hands half a unit, 56-92 units/s, then a second
    movement). The draw clip now keeps whatever the hold has not yet taken, while drawing
    and after it (its own fade-out would drop its last share in one frame), and is gone
    when the hold is whole.
  - The settling curve (`anim_blend::settle`) arrives at rest: the spring's last tenth of
    its speed is taken out over its end, so a fade or a move along it no longer stops
    dead (the hold's fade-in ended at 9 units/s, then 1 in a frame). The aim, the zoom and
    the iron sights run on it too: same times, a softer end (owner, same day).
  - The holstered weapon's clip, cut when the weapon changes hands, is eased across in
    first person (`Animator::smooth_jump`); the flamer's arms stepped from 7 to 86 units/s.
  - A clip's motion (`ClipMotion`) is measured on the arms only, not the `Weapon` node: the
    flamer swinging off the back while the hands held still all but stopped the playback,
    and the hands with it (147 to 19 units/s, then off again).
- Every first-person draw and put-away, one weapon per clip pair (owner, 2026-11-16: "1 test
  for every unique animation"): the eleven clip prefixes, each shared by every weapon of its
  animation type (`h2h` 62 weapons, `1hm` 51, `2hm` 43, `1hp` 108, `2hr` 64, `2ha` 57, `2hh` 22,
  `2hl` 41, `1gt` 34, `1lm` 7, `1md` 7; no weapon has clips of its own; Lonesome Road
  replaces the launcher's). Traces `movement-inertia-trace-weapons` and `-rest` (Assault
  Carbine, Missile Launcher, Frag Mine, Bottlecap Mine), each against its `-original` twin
  (the main switch off: the clips at the original's pace and blending), counting the
  separate movements of each hand:
  - `ClipMotion` measures the hands as the eye sees them: both hands' moves and turns in the
    `Camera1st` node's space, the clip's tracks posed through the skeleton (its bind pose
    where the clip has none). Summed bone by bone, a turn of the upper arm, which swings the
    hand over a long arm, counted as little as one of the wrist, and the playback raced
    through it: the razor's draw made two pushes (390 and 360 units/s) where the original
    makes one, the frag mine's ended with a flick (16, 38, 15 units/s). Now the razor's is one
    push, the flick gone, and the razor's put-away key jump is no longer one (none of the
    clips has a jump of the hands).
  - `anim_blend::Animator::pose` leaves out a layer whose fade shows as nothing: a leaving
    smooth layer's last frame (its fade a hair over 0, the settling curve rounding it to
    none) alone in its priority group shared 0 ÷ 0, and the bones it drove took their bind
    pose for that frame (the left arm flung 45 units out and back during the grenade's
    put-away; it depends on where the frames fall, so it came and went between runs).
  - On screen no clip makes more movements than the original's, several fewer. The ones
    left where the original has fewer happen with the hands out of view (80-140° off the
    line of sight): the fists' put-away (a second push once the hands are below and behind
    the camera), the pistol's put-away left hand, the rebar club's draw (the clip's own dip,
    the original's too), the Bottlecap Mine's left hand (eased where the original snaps).
    The change of holstered weapon moves the arms behind the view in one frame, as the
    original does.
- **Weapon weight in handling** (part of Weapon Handling since 2026-11-15, §1.4a.3:
  `--no-weapon-handling`, key `weapon-handling`; eighth playtest: "weapon weight slows handling"): with
  movement inertia the hand-back after a reload or an attack takes `inertia::handling` of
  the WEAP record's weight times as long: 0.85 for 1 lb and under, 1 for a 3 lb pistol
  (the 10mm pistol the times were tuned with), 1.5 from 20 lb, smoothly between; the draw
  and holster `inertia::draw_weight` (above), the aim (and its zoom) `inertia::aim_weight`
  (0.9 to 1.35, 1 for the pistol), both with the attributes (§1.4a.3). The sway's springs
  keep their own weight part (Weapon Inertia's, below).
- Loop seams of the arms' clips (sixth playtest: "small hitching" in the idle): the trace
  showed the hand turning back in one frame each time the idle looped (every 2.46 s with
  a 10mm pistol). With movement inertia the first-person animator smooths its seams by
  inertialization (`anim_blend` `Seams`, Daniel Holden's technique): at a wrap the pose
  carries on where it was going and the difference dies away on a critically damped
  spring over 0.3 s. The trace after: the hand's worst jolt at
  idle 4 × the background, was 77 ×.
- Recoil on the gun (owner, 2026-11-14): each shot of a gun or launcher (`1hp`, `2hr`,
  `2ha`, `2hh`, `2hl`) sets springs (11/s; 16/s until the ninth playtest, "slower
  ending") moving that peak at 1.2 units back, 1.6° up
  and up to 0.5° aside, for strength 0.7 (light) to 1.3 (20 lb and over); rapid fire
  builds to a plateau (shown at most 3 units, 5°); 60 % down the sights. The view's own
  kick is the game's size, but with soft recoil it comes in over 0.16 s (0.1 s in the
  eighth build;
  `inertia::KICK_EASE_SECONDS`, `combat::ease_kick`): an exponential,
  most of it the first frames, the rest softly, so the view no longer jumps to each
  shot's new point in one frame (eighth playtest: "fast start, slow end … the jump is not
  instant to the next recoil point"), and 40 % of each kick then settles back down on a
  critically damped spring over 0.6 s (`inertia::KICK_BACK_SHARE`,
  `combat::ease_back`): it starts gently while the kick still rises and ends slowly, so the
  gun rises and comes back down a little (ninth playtest: "make the gun smoothly return
  down a bit so the recoil does not just force the whole camera up"); with movement
  inertia the climb is 60 % of the original's. The fire clip starts again
  at once at each shot and the arms ease across the step back to its start
  (`Animator::smooth_jump`, the loop seams' inertialization below): its kick shows whole,
  with no snap between shots.
- Reloads (eighth playtest: "try improving reloads with springs"): with movement inertia
  a reload's, a jam's and each round's clip starts at once and the arms ease across the
  step to it as across a loop seam (the clip's own motion untouched); at its end the arms
  go back to the hold over 0.4 s along the settling curve (an attack's over 0.3 s; both ×
  × the handling below). The trace of a 10mm pistol reload: no step at its start or end,
  the hand settling back onto the hold over about 0.7 s. Since the ninth playtest
  ("reloads - do not see any difference") the sway also leans the gun over for the reload:
  down 1.6 units, in 0.8, the muzzle 5° down and 9° of roll, on two springs in a row of
  0.3 s each (× the weight), out again as the reload ends.
- Walking ahead (ninth playtest: "make the gun point to the ground more when walking
  forward, just a bit"): the muzzle tips down 3° and the gun drops 0.5 units, full from
  150 units/s ahead, on two springs settling in about 0.6 s; none down the sights or
  backing up.
- The gun with the view's pitch (eighth playtest): looking straight down the gun comes up
  1.4 units and back 1.8 towards the eye, its muzzle 2.5° up; looking straight up it drops
  1 unit and goes out 0.6, the muzzle 1.5° down; with the sine of the pitch between, none
  down the sights (the share fades with the sights' spring), so the arms are not bolted
  to the eyes.
- Blows taken (owner: "damage screenshake"): each hit on the player sets the hand's
  springs moving down, back and aside by turns (stronger for a bigger share of health);
  the camera's place gets the same on the head's springs (§1.4a.1); its rotation never.
- Breathing: standing still (below 20 units/s, on the ground) a figure of eight 0.18
  units wide and half as high every 4 s, fading in over 0.8 s; 35 % down the sights and
  sneaking. Out of breath (§1.4a.1) up to 2.2 × faster and larger, moving too.
- Weapon weight (part of Weapon Inertia since 2026-11-15, §1.4a.3; a switch of its own,
  `weapon-weight-sway`, before): a weapon in hand of up to 3 lb sways as above; from
  20 lb its springs are 1.5 × slower and its sway, limits and breathing 1.5 × larger,
  smoothly between (`view_sway::heft`, the WEAP record's weight) [design, the owner].
- It turns and moves `Bip01 Translate` about the eye in the view's frame, after the
  clips are blended, so the camera does not move; it fades out (0.15 s) under the
  Pip-Boy, while an idle drives the view and in third person.
- A switch, off by default since 2026-11-16 (`--weapon-sway` / `--no-weapon-sway`), Weapon Inertia on the settings page (key
  `weapon-inertia`, "Weapon sway" and `weapon-sway` until 2026-11-15). The exit summary gives the most turn lag, the deepest bob and the
  deepest jump or landing dip.

### 1.4a.2 Movement inertia and the eased crouch (owner, 2026-11-13)

The original's player is at full speed from the first loop and stops dead (physics.md
§6.3). With the "Movement inertia" switch (key `movement-inertia`, `--movement-inertia` / `--no-movement-inertia`,
off by default since 2026-11-16) [design]:

- On the ground the player's velocity, walking or running, rides a critically damped
  spring towards the keys' velocity: 95 % of a new, faster speed in 0.5 s, down to 5 % of
  it in 0.25 s (half the start) when the keys are let go (or the brake before a turn
  back), the last of the glide easing out; a run glides about 36 units, half a metre.
  History, all the owner's playtests of 2026-11-13: 0.12 s / 0.2 s (felt like nothing),
  then 0.5 s / 0.9 s (stopping "feels like … sliding on ice"), then the stop at half the
  start
  (`engine/nvf-play/src/inertia.rs`, applied in
  `player::step` before the character controller, so `fnv_phys`, people and the parity
  checks are untouched). The air keeps the game's air control; the landing speed
  profile (physics.md §6.4.1) is left out: a landing lands into the inertia.
- Landing compression (owner, 2026-11-14): a landing faster than 350 units/s takes up to
  60 % of the ground speed (all of it from 900 units/s, smoothly between); the spring
  builds it up again. A hop (about 290) keeps its speed.
- A crouched jump in first person stays crouched: the weapons' jump and landing clips
  stand the body up (only the unarmed ones have sneaking variants), which drew the view
  40 units up mid-jump and back down after ("looks like the character stands and then
  jumps", the fifth playtest). While sneaking with movement inertia on, the first-person
  rig plays no jump or landing clip, and the crouched walk plays on through the air. The
  fifth build kept the sneaking jump clips, but the unarmed one stands the view up too
  (the eye 118 units over the feet instead of 78), and dropping the walk for the idle
  0.15 s into the air lifted it 9 units ("sometimes still", the tenth playtest; trace
  `movement-inertia-trace-crouch-jump`). A drop off a ledge still eases the view down
  over it (physics' step lift, `nvf_play::phys::step_lift`), up to about 10 units behind.
- The Pip-Boy with movement inertia on: its clip is played into its hold at the `Hit` key
  (0.33 s) and out of it along a cubic that keeps the clip's own speed at the far ends and
  has none at the hold (`player_body::pipboy_clip_time`), in the clip's own time. Held at
  the key as it was, the arm stopped dead still moving about 28 units/s and set off at
  speed going down (tenth playtest: "PipBoy jitters a lot also"; trace
  `movement-inertia-trace-pipboy`: held up, the arm does not move at all).
- While the body glides with no key held, or brakes against the keys, the movement
  clips follow the way it slides (walking or running as it last moved); detection,
  power attacks and scripts still read the keys.
- First person: the view sinks into a crouch over 1.0 s and rises over 1.1 s (0.35 s /
  0.4 s at first, then 0.5 s / 0.55 s after "slow down the last bit", then doubled: "make
  the 2x slower crouch and stand up speed the default"), instead of the clips' straight
  0.2 s cross-fade. Since the seventh playtest ("crouching makes the hand with a gun
  hitch") the standing and sneaking clips themselves cross over that long along the
  settling curve (§1.4a), so the view and the arms change stance in one motion; before,
  a separate module moved the arms rig by the difference between an eased eye height and
  the clips' quick cross-fade, and the arms' own change of hold still took 0.2 s.
- The times above are the ones in use; an "Inertia weight" dropdown (0.5, 1, 1.5, 2)
  scaled them, the weapon's springs and the head bob's until the owner's eleventh
  playtest (2026-11-15: "remove the inertia weight option - current default will be the
  one we use"): 1. The switches are §1.4a.3.

### 1.4a.3 Advanced Movement: Gameplay and Visual Only (owner, 2026-11-15)

First "create new section under Settings and name it 'NVF Features' ... add on/off toggle
for all features" (fourteen switches), then "simplify the Advanced Movement menu - too many
option. We should merge some similar toggles together and add 2 categories - toggles for
purely visual change and toggle for changes that affect gameplay", with the names and order
the owner gave. The settings page's NVF Features tab (after the original's four tabs;
settings-page.md) [design]:

| Heading | Key | Switch | What it covers |
|---|---|---|---|
| Advanced Movement | `advanced-movement` | Global Toggle | Over every switch below (`--no-advanced-movement`, `MovementInertia::advanced`): off, none acts, as in the original; each keeps its own choice |
| | `movement-weight` | Movement Weight | A dropdown, 25 % / 50 % / 75 % / 100 % (`MovementInertia::weight`, 100 % by default): Movement Inertia's times (start, stop, crouch, the landing's stand-up and lost speed) at that share, so 25 % is four times as quick; of Weapon Handling's draw, holster, aim and hand-back only the time over the original's is cut, so a weapon is never quicker than the original's at the same attributes (owner, 2026-11-15) |
| *(an empty line)* | | | |
| Gameplay | `movement-inertia` | Movement Inertia | The ground speed's springs (starting, gliding to a stop, braking before turning back; the movement clips following a glide); the first-person stance cross-over (1.0 s / 1.1 s) and crouched jumps staying crouched; the view's take-off and landing dip, deeper for a higher fall (`hard_landing`), the longer landing clip (`land_stretch`) and the speed a hard landing takes (`MovementInertia::land`) (`--no-movement-inertia`) |
| | `weapon-handling` | Weapon Handling | The slower draw and holster (`draw_weight`: 1.5 to 2.4 times the original's) and aim (0.6 s / 0.55 s for a pistol, `aim_weight`), heavier weapons slower still, Agility and Strength by the weapon's class (`attribute_rate`, `aim_rate`); off, the game's own timing with its Agility rate (`--no-weapon-handling`, `MovementInertia::handles`, `equip_rate`, `aim_seconds`) |
| | `advanced-recoil` | Advanced Recoil | The view's kick eased in and 40 % settling back (`nvf_play::combat`), the view's nudge back from a shot (head bob's springs) |
| *(an empty line)* | | | |
| Visual Only | `weapon-inertia` | Weapon Inertia | The arms' lag, steps, bob, jump and landing dip, the muzzle tip in the air, the breathing drift, the gun's recoil springs, the weapon's weight in all of it (§1.4a); `--no-weapon-sway`, a saved `weapon-sway` read as it |
| | `animation-smoothing` | Animation Smoothing | Every first-person clip fade on the settling curve, the draw and holster played by their motion along it, eased starts and slow returns of attacks, reloads and jams, the aim's cross-over, the true iron sights' and the zoom's curve, the loop seams, the Pip-Boy's ease; at Weapon Handling's pace, else at the game's own (the aim at the true iron sights' 0.35 s / 0.3 s), so it changes the look only |
| | `camera-motion` | Camera Motion | The head bob and the head's lag from the body's acceleration (§1.4a.1), the breathing camera, out of breath (the view's and the gun's breath faster and deeper after running), a blow's jolt of the view and the gun; `--no-head-bob`, a saved `head-bob` read as it |

The empty lines are `settings_layout::Section::gap` (`ViewEntry::Gap`), used only here, so
the other tabs' lines are as they were. Smoke: `advanced-movement-menu` (the tab's lines in
this order), `advanced-movement-switches` (every switch off part-way through a draw and on
again while aimed), `advanced-movement-parts` (Camera Motion off: Movement Inertia's glide
and eased crouch stay, the view no longer bobs), `advanced-movement-off` (the Global Toggle
off), `movement-inertia-off`, `weapon-handling-attributes`, `weapon-handling-off-agility`.

**Attributes** (owner, 2026-11-15, after testers' reports: "the weapon draw speed is kinda
brutal ... im at agility 10", "movement speed seems lower"; `MovementInertia::equip_rate`,
`aim_seconds`, kept from the player's current Agility and Strength by `combat::perk_speed`):

- Draws and holsters play at the original's Agility rate, 0.5 + 0.1 × Agility (1 at 5,
  1.5 at 10, 0.6 at 1) [documented: Fallout Wiki, Agility (Fallout: New Vegas), the draw
  and holster times 10 / (Agility + 5), as the reload's; special-skills-audit-2026-10-23
  §2], times the Equip Speed perks; with Advanced Movement off too. Logged `weapon
  handling: draws at x<rate>, aims in <s>s (<lb> lb, Agility <a>, Strength <s>, ...)`.
- With Weapon Handling each point of Agility or Strength from 5 adds or takes the
  original's tenth times its share for the weapon's class (`weight_class`: light to 3 lb,
  medium 8 to 10 lb, heavy from 18 lb, blended between): Agility 1 / 0.6 / 0.3 for light /
  medium / heavy, Strength 0.3 / 0.6 / 1 (`attribute_rate`, never under 0.6, the original's Agility 1 rate, so two low attributes cost no more than one). A light
  weapon at Agility 10 gains as the original's does, a heavy one at Strength 10 as well. The handled time is never shorter than the original's at the same Agility, and Movement Weight cuts only the part over it.
- The aim (and the block's cross-over, which runs on it) takes 40 % of that gain or loss
  (`aim_rate`: 1.26 at most, 0.84 at least), so a light weapon at 10 and 10 still takes
  0.43 s, more than the original's 0.35 s [design: "do not make the min-max difference
  huge"]. Without Weapon Handling the aim is the game's own, as the original's.
- The ground speed is the original's: its run speed does not depend on Agility
  (ext-last-core.md §4), and the inertia reaches the keys' full speed (344 units/s running,
  measured in the smoke runs' `movement inertia: up to` lines); what felt slower was the
  half-second start, which Movement Weight now shortens.

Every weapon type's draw and holster (owner, same day: "some equip animations are not
affected - check that every animation for guns/melee weapons is affected, they can use the
same curve"; trace `movement-inertia-trace-weapons`: the 10mm pistol, the straight razor,
the rebar club, a frag grenade, the flamer, the fists, a swap while drawn):

- A draw or holster clip is played by how far its motion has got, not by its clock
  (`player_body::ClipMotion`: the hands' moves and turns as the eye sees them, below), so the motion as a whole follows the settling curve. The fists' draw has its arm
  up in its first fifth: played by its clock along the curve it still came up at
  980 units/s in 0.1 s; now at most 330. The weapon changes hands where the curve reaches
  the motion done at the clip's own key.
- A key jump inside a clip (a step its keys take at once) is eased across
  (`Animator::smooth_jump`); measured on the hands, none of the first- or third-person draw
  and put-away clips has one (2026-11-16; summed by bone, the razor's put-away had one).
- The hand-over to the hold or the arms down at a draw's or holster's end starts with no
  speed and no pull (`handover_share`, `6u⁵ − 15u⁴ + 10u³` from 35 % of the time): along the
  settling curve's sharp start, the nearly stopped arms set off again at 350 units/s within
  two frames (the fists', the 10mm pistol's holster; now about 150 and 50 at most).
- The last quarter of a draw or a put-away (owner, 2026-11-16: "make the last 25% of the
  equip and holster animations smoother without changing their timing"), same times:
  - The hand-over's speed goes as `u³(1 − u)⁴` (`70u⁴ − 224u⁵ + 280u⁶ − 160u⁷ + 35u⁸`), not
    `6u⁵ − 15u⁴ + 10u³`: it is quickest a little before its middle and has 15 % left at the
    last quarter, not 29 % taken at nine tenths of its top speed and braked there. The 10mm
    pistol's put-away came into its last quarter at 105 units/s and stopped within 0.16 s;
    now 74, tapering off (the Assault Carbine's 124 to 76).
  - The draw's place along its time (`equip_curve`) is the settling curve up to 75 % and
    then a quintic carrying on from it that comes to rest with no speed and no pull: the
    settling curve stops while still braking.
  - The traces `movement-inertia-trace-weapons` and `-rest` give the club, flamer and
    launcher put-aways (2 s and 4 s with Weapon Handling) their whole time: equipping the
    next weapon part-way cut them, a 26 to 114 units/s kick at the cut.
- The blend keeps each clip's rotation of a bone on the side of the sphere it was on last
  frame (`anim_blend::Layer::aligned`): taken from the first clip's side each frame, the
  straight razor's put-away over the idle blended the upper arm the long way round for a
  while and then flipped, a 34-unit jump of the hand. This is in the shared animator, so
  every actor's clips blend this way.

A melee block follows the aim's settling curve too (raised and lowered in about 0.5 s
each; trace `movement-inertia-trace-block`), slower with Weapon Handling.

The shimmer of a held gun down its sights (the owner's twelfth playtest, the Sturdy
Caravan Shotgun) is mostly the drawing, not the motion [observed: traces
`movement-inertia-trace-sights-shotgun` and `-still` at 3840x2160]. With Weapon Inertia
and the breathing camera off the gun does not move against the view at all, yet its
pixels shift about 0.33 px a step at Goodsprings (x = -71680, where single-precision
coordinates step by 0.008 units), 0.14 px at the world's origin and 0.05 px indoors. The
cure would be drawing relative to a point near the player (a render origin); the owner
left it out of this work (2026-11-15).

Switching a part off or on while it acts leaves nothing stuck (smoke
`advanced-movement-switches`): a draw or holster goes on at its rate, the zoom goes on
from where it is, and a placement by the game starts the ground velocity from rest
however near (`MovementInertia::check_game_placed`).

The Global Toggle and Advanced Recoil are on by default; Movement Inertia, Weapon Handling,
Animation Smoothing, Weapon Inertia and Camera Motion are off (each turned on by its own
switch, `--movement-inertia`, `--weapon-handling`, `--animation-smoothing`, `--weapon-sway`,
`--head-bob`, or on the page) [design: the owner, 2026-11-16, "make sure all advanced movement
features are off by default except the improved/advanced recoil"]. A saved choice in the INI
stays. At the start and at each change the log says which of the switches kept in
`MovementInertia` are on (`advanced movement: Global Toggle on, ...`; smoke
`advanced-movement-defaults`). Until 2026-11-16 all were on by default.

### 1.4a.1 The camera's bob: first person yes, third person no (owner, 2026-11-07)

The owner, from a report: "we had headbob in third person, but none in first person?
it should be the other way around". Before: the first-person camera sat on the arms
rig's `Camera1st`, which no first-person movement clip moves (§1.4a), so only the arms
bobbed; the third-person camera's pivot was the walking body's head bone, which rises
and falls with each stride. Now (`engine/nvforever/src/head_bob.rs`) [design, constants
tuned by eye]:

- First person: the whole arms rig (camera node, arms and weapon together) dips 5
  units (about 7.5 cm; 5 until the sixth playtest, "increase headbob by 5%") at each footfall and swings 1.89 across once a stride (1.6 and 0.5
  until the owner's fourth playtest, "there is no headbob even with it on": the log
  showed a 1.5-unit dip, about 2 cm; 3 and 1 in the fourth build, "way too little" at
  the fifth), at the movement clip's
  phase, growing with the ground speed to a run's; sneaking 60 %, down the sights 25 %,
  none in the air, under the Pip-Boy or while an idle drives the view. The arms' own bob
  (§1.4a) comes on top. A switch registered in the settings page's Display group and
  shown under NVF Features › Advanced movement (Gameplay › Camera until 2026-11-13, then
  Gameplay › Advanced Inertia until 2026-11-15; `head-bob`,
  `--head-bob` / `--no-head-bob`; settings-page.md), off by default since 2026-11-16.
- Head inertia (owner's playtest of 2026-11-13: "we should add inertia to it"): the
  stride's bob passes through a critically damped spring settling in 0.12 s, so it
  rounds off; the body's acceleration leaves the head behind as it speeds up and lets it
  lean on as it slows (spring 9/s; a sudden change of 344 units/s would peak at 1 unit,
  shown at most 1.5); take-offs and landings dip it (spring 4/s; 8/s and 5.5/s before, about 2 units for a
  standing jump's landing, more for a harder one, shown at most 5; 1.2 and 3 before the
  fifth playtest). A shot nudges the camera back (12 units/s on that spring per unit of
  strength) and a blow taken jolts it down, back and aside (45 units/s per unit of
  punch; the nudge is Advanced Recoil's, the jolt Camera Motion's, §1.4a.3). Only the camera's
  place moves, never its rotation; halved down the sights. The exit summary gives the deepest take-off or
  landing dip, the most gun recoil and the most out of breath.
- Breathing camera (owner, 2026-11-13: "camera moves on its own to simulate breathing -
  smooth, slow and only a little"; a switch of its own, `breathing-camera`, until it became
  part of Camera Motion on 2026-11-15, §1.4a.3): the whole arms rig rises and falls 1.2 units
  (about 1.7 cm; 0.5, "almost not noticeable", before the fifth playtest) once every
  4.5 s on a sine, with 0.3 units forward and back a quarter breath behind; all of it
  standing still, 40 % moving, half down the sights, none in the air or under the
  Pip-Boy, fading over a second. Position only. The research of 2026-11-14 notes that
  slow whole-view motion at 0.2–0.4 Hz is the band most likely to cause motion sickness
  (Loughborough, `repository.lboro.ac.uk` …9346865): hence the switch.
- Out of breath (owner, 2026-11-14): running above 250 units/s winds the player over
  12 s, standing recovers over 10 s and walking over 20 s, each jump takes 0.06; winded,
  the camera's and the gun's breathing are up to 2.2 × faster and larger, while moving
  too.
- Third person: the pivot's height above the feet is the head's through two
  first-order lags of 0.25 s: a run's stride bob is cut to about an eighth, a crouch
  still lowers the camera (90 % in a second); a change over 40 units in a frame goes
  through at once. The exit summary gives the deepest first-person dip and the
  third-person pivot's frame-to-frame movement against the head's; smoke
  `head-bob-first-person`, `head-bob-third-person`.

### 1.4b The weapon lowered near walls and friends (owner request 2026-10-26)

The original keeps the first-person weapon up whatever is ahead, so it pokes through
walls and companions; its `_1stperson` clips have no lowered pose [observed: the owner's
play; the folder's file list]. NVForever lowers it by code
(`engine/nvforever/src/weapon_lower.rs`) [design, constants tuned by eye with the
owner, 2026-10-27]:

- Two rays ahead of the view, from the eye and from about where the weapon is held. A
  surface within 40 units eases it down, fully at 22; a living actor within 72 units
  that would not attack the player (`fnv_game::rest::hostile_to_player`, by its hit
  volumes) lowers it fully. A hostile actor never does.
- Lower Weapon (H, NVForever's own control, rebindable) lowers or raises it by hand
  (`nvf_play::control`). It was Ready Item tapped twice until 2026-10-27; the owner
  moved it to its own key because waiting 0.25 s for a second tap held every reload
  back, which matters when R is spammed in a fight.
- Aiming, the trigger and an attack bring it up at once and keep it up 0.6 s; a reload
  keeps it up for the reload clip's length. Aiming, the trigger and putting the weapon
  away also end a lowering by hand. Shots still leave from the view; firing is never
  blocked while lowered (owner ruling 2026-10-27).
- The pose turns the arms about the eye (pitched 9° down, turned 10° in, canted 5°), on
  top of the sway, so nothing near the camera rises into the view: turned about the
  hands, the sleeves swung up across it. Rifles take 80 % of it, heavy weapons and
  launchers 35 % (they sit at the foot of the view or on the shoulder). Not for fists,
  under the Pip-Boy, while an idle drives the view or in third person.
- A switch for the automatic part, on by default: `--no-weapon-lower`, "Lower weapon near
  walls" on the settings page (key `weapon-lower`); by hand works either way. The exit
  summary gives how many times it lowered.

Lowering is the cue near walls; it no longer has to keep the weapon out of them: the arms
rig and its weapon are drawn with their depth squeezed in front of the world, so no wall,
door or actor hides them (owner report 2026-10-27; [first-person-depth.md](first-person-depth.md)).

### 1.5 The body in first person (owner request 2026-10-10; not the original's)

A switch, off by default as the original draws the arms rig alone: `--first-person-body`,
and "Body in first person" on the settings page (key `first-person-body`).
[engineering choices throughout; nothing here is a claim about the game]

- On, first person draws the third-person body too, without its head parts (head, hair,
  headgear: the `HEAD_SLOTS` rule of the stand-in body), so looking down shows the
  chest, legs and feet in the outfit worn. The view is unchanged: the camera stays at the
  arms rig's `Camera1st`.
- The body is moved to the view, not the view to the body: each frame it stands where
  its eyes are under the camera and 4 units behind (`OWN_BODY_BACK`), feet on the
  ground, so the open neck stays out of the picture; crouched and moving it follows its
  own head. It stays upright whatever the pitch (the aim up/down clips are not blended
  in), as the arms rig alone follows the view.
- While the arms rig's arms are up (weapon out, being drawn or put away) the body's own
  arms, hands, gloves and Pip-Boy are hidden (the shapes the arms rig draws,
  `fnv_actor::first_person::shape_shows`), and its weapon while it is in the hand. With
  the weapon away the body's arms hang at its sides and its weapon is in its holster.
- On furniture the seated body is drawn without its head and the view is its eye, as
  before, and (owner decision 2026-10-10: "limited to a real person's head movement")
  the view turns at most 80° to either side of the way the body faces (`NECK_TURN`)
  and tilts at most 75° down (`NECK_DOWN`); a view further round when sitting down
  comes round at 4 rad/s, and the body is drawn once it has. The way the body faces is
  its pelvis's (`Bip01 Pelvis`, its turn about the vertical since the bind pose): the
  clips of getting on and off turn the body inside the rig, and the view turns with it
  (owner report 2026-10-10: sitting down, the camera "didn't move in line with the rest
  of the body"). The eyes go round the neck (`Bip01 Head`) with the view, as a turned
  head's do. With the switch off the seated view is as it was: free, not turned by the
  body, and no body.
- While an idle drives the first-person view (its camera node places the view and mouse
  look is ignored: the opening's wake-up in Doc Mitchell's house) the body is not drawn,
  only the arms rig playing the idle, as with the switch off (owner report 2026-10-16:
  with the switch on, loading in showed "the players own body standing over
  themselves"). The body casts its shadow still, from where it stands.
- On a bed (owner requests 2026-10-10: lie down as a sleeping NPC does, "sideways the
  camera when laying") the view is the lying body's, with the switch on or off
  (`lying_view`). That code does not assume an upright body: the frame is the pelvis's
  whole turn since the bind pose (`bone_turn`), not only its turn about the vertical.
  The view keeps its turn and tilt in that frame while the body lies down and gets up,
  so the picture leans with the body (`Player::body_view`: the camera takes a whole
  rotation, not a heading and pitch, as it does for an idle that turns the view) and the mouse moves the
  head in the frame (`PlayerSeat::look`). Limits: the seated 80° and 75° about the
  body's own axes, and no more than 10° below level once lying (`LYING_DOWN`), as lower
  is the pillow. With the switch on the eyes go round the neck about the body's length.
  The player's heading and pitch are set to the level view nearest the leaning one.
  In third person the pivot is lifted up to 40 units (`LYING_PIVOT_UP`) so the camera
  clears the bed. Facts and what was seen: docs/spec-notes/furniture.md §7.
- The shadow is the whole body's, head and arms included: §1.6.

Checked 2026-10-10 on the owner's install [observed in NVForever: screenshots kept
privately], Goodsprings, Vault 21 jumpsuit `00104184` and varmint rifle `0007EA24`:
looking 88° down with the weapon away shows the chest, belt, both hands at the sides
and the boots; with the rifle out, the arms rig over the legs; sneaking and running
forward; seated on the saloon porch chair `00106349`, the lap and forearms ahead, the
shoulder and arm at either limit, a view that starts behind comes round, and through
the sitting-down clip the view turns with the body from facing the chair to facing away
from it.

### 1.6 The player's sun shadow (owner request 2026-10-10; not the original's)

[engineering choices throughout; the original casts no sun shadows from the world]

Before: in first person the arms rig and its weapon cast a shadow of floating arms and
there was no body shadow; with the body in first person (§1.5) the shadow had no head,
and no arms of its own while the weapon was out.

Now, in every view, the third-person body casts the player's shadow whole (head, arms
and hands, weapon), and the arms rig and its weapon cast none on the world. What the
camera sees is as it was.

- **Unseen but casting.** A hidden entity casts no shadow [documented: bevy_light 0.19.1
  `check_dir_light_mesh_visibility` skips entities whose `InheritedVisibility` is false,
  and takes those whose `RenderLayers` meet the light's]. So the parts of the
  third-person body the camera is not to see are no longer hidden: they are drawn on
  render layer 2 (`sky::SHADOW_LAYER`), which the sun casts from (the sun has layers 0
  and 2) and no camera has. The body's root is never hidden. `player_body.rs` `Seen`
  says what the camera sees of the body (all of it in third person; none in first
  person; with §1.5 all but the head, and but the arms and the weapon in the hand while
  the arms rig's are up; seated, as §1.5), and `show_rigs` puts each drawn entity on
  layer 0 or 2 when that changes.
- **The arms rig casts nothing on the world.** Its meshes and its weapon's are on render
  layer 3 (`sky::ARMS_LAYER`), which the main and the reflection camera have and the sun
  does not.
- **The arms rig is not shaded by the unseen body.** That body stands in the middle of
  the arms rig, with its own arms and weapon about where the arms rig's are, and in the
  sun's shadow map it shades them [observed in NVForever, first attempt: the Pip-Boy,
  the left hand and the rifle's stock came out darker at every heading tried]. So the
  arms rig takes its sun shadows from a second light, `ArmsShade`: a child of the sun
  (same direction), with the sun's colour at 1/1024 of its strength
  (`ARMS_SHADE_SHARE`), casting from layers 0 and 3 with one cascade of its own, 256 units around the camera
  (`ARMS_SHADE_REACH`, since #254; before, the sun's first cascade).
  Its shadow map therefore holds exactly what shaded the arms before this change: the
  world, the seen parts of the body, and the arms themselves. The arms rig's materials
  are copies with `NifSurface::shade_light` set; their fragment shader
  (`nif_material.wesl` `with_shade_light`) reads both shadow maps and works the lit
  colour out from two results of Bevy's lighting, with and without shadows (Bevy's
  lighting has no opening for another shadow value; the small share of light is what
  makes the sun's part of the light measurable that way). Since 2026-11-02 (#1374) the light draws only
  the casters whose bounds come within 64 units of the line from the eye 96 units toward
  the sun, and no land (`near_shade_casters`, `terrain::LandMesh`; so steep ground within the
  24-unit body clearance, crouched against a cliff with the sun behind it or the lying
  view, no longer shades the arms [engineering choice]): Bevy keeps every
  caster between a cascade and the light, so at a low sun the whole column toward the
  sun, hills and buildings far off, was drawn into a map as large as the sun's, up to
  3.6 ms a frame at Novac at 07:00; a cell's land box spans its hills and holds the eye
  wherever the player stands. What shades the arms from further off comes from the
  sun's own map (`nif_material.wesl` `shadow_past_body`), read at a depth moved toward
  the sun past the plane through the eye and 24 units more (`BODY_CLEARANCE`), so the
  unseen body round the eye is left out; the arms take the darker of the two. The sun's
  cascades reach 256 units further toward the sun for it (`sky.rs` `SUN_REACH_BACK`,
  `reach_toward_sun`): Bevy puts a cascade's near plane at the view slice's corner
  nearest the sun, just in front of the eye when the sun is behind the camera, and every
  caster before it lands on that plane, the body and a wall behind it alike. Measured
  [observed 2026-11-02, RTX 4070, 1920x1080, finer shadows, Novac 07:00, interleaved
  runs]: main 15.4 ms, now 12.4 ms, without the light 11.4 ms; the light's pass costs
  about 0.75 ms whatever it draws (one caster as three), 0.25 ms when it draws nothing
  (owner-review item 43). The light is there only while
  the arms rig is drawn and the sun casts shadows, and is despawned otherwise, never
  hidden: a hidden directional light keeps its shadow views in Bevy 0.19.1's render
  world and gets a second set when shown again, and the doubled views upset the shadow
  passes' shared indirect draw buffers, so the sun's cascades lost or gained whole
  batch sets from frame to frame [observed 2026-10-26: hillsides flickering between
  lit and shadowed in first person, worse the more often the arms had been hidden;
  gone with GPU culling off, without the light, or with it despawned; the shadow probe
  now counts views sharing a key and the `shadow-probe` smoke scenario switches view
  four times and wants none].
- **Cost.** While the arms rig is drawn: one more shadow cascade to render, one more
  directional light in every fragment's lighting, and a thousandth more sunlight on the
  world. Frame rates of 18 s runs before and after overlap (before 44.5 and 26.2 per
  second, after 38.9 and 38.6, on a machine busy with other work), so the cost was not
  measured.
- Point and spot lights cast no shadows (`lights.rs`). If one ever does it needs the
  sun's layers, or the player casts no shadow from it in first person.

Tried and dropped: (1) leaving the arms rig on the sun's map (the result above); (2) a
material that skips casters within 90 units of the eye: standing in tight shade (inside
the truck cab by the Goodsprings road) the arms stayed sunlit; (3) a shade light without
light of its own and an arms rig that casts no shadow at all: the arms lose the shadows
they cast on themselves.

Not built: in the water's reflection the first-person view still shows the arms rig
rather than the body, as before. With §1.5 on and the weapon out, the shadow's arms are
the body's own (upright, aiming level), not the arms rig's (which follow the pitch).

Checked 2026-10-10/11 on the owner's install [observed in NVForever: screenshots kept
privately, `C:\fnv-private\references\player_shadow\`], Goodsprings at 17:00 facing
east, jumpsuit `00104184` and varmint rifle `0007EA24`, before and after:

| View | Before | After |
|---|---|---|
| First person, weapon away | two arm shadows, no body | a whole body with its head |
| First person, weapon out | arms and rifle floating, no body | a whole body holding the rifle |
| Body in first person, weapon away | a body without a head | a whole body |
| Body in first person, weapon out | headless, armless body and floating arms | a whole body holding the rifle |
| Third person | whole body | the same (under 220 of 1,024,000 pixels differ) |

The arms and weapon themselves, in the part of the picture the shadow does not reach:
4, 49 and 152 pixels differ by more than 24 of 255 facing east, north and west, of
230,400. Standing in the truck's shade the arms are shaded as before. Seated on the
porch chair `00106349`, with and without §1.5, the picture is as before. On the Lake
Mead pier (cell 13,9) the water and its reflections differ only by the ripples.
Switching to third person and back keeps the right parts seen.

**Flicker after leaving an interior** (owner's playtest, 2026-10-11: "strange flickering on
shadows noticed 2 times after exiting interior cell", first person only). Not reproduced
[observed, owner's install, 2026-10-11]: `--play --cell GSProspectorSaloonInterior
--no-npc-packages --hour 15`, `NVF_PLAY_TEST_NEAR=0010618E NVF_PLAY_TEST_ACTIVATE=10`
(the front door), then bursts of 20–40 frames 0.05–0.1 s apart right after the loading
screen: on the porch, walking off it and standing in the sun with the .357 out. Frames of
a still view differ by at most a few hundred pixels (the idle's sway); no shadow comes
and goes. With `RUST_LOG=nvforever::player_body=debug` the arms shade light, the
first-person rig and what the camera sees of the body never switched through the door
(debug lines "arms shade light on/off", "first-person rig shown/hidden", "the camera sees
… of the body", kept for the next look). What the owner should say: the hour, which door,
whether "Body in first person" was on, and what flickered (the arms, the ground, the
body's own shadow).

## 2. Code

| Item | Where | Notes |
|---|---|---|
| Clip choice (headless) | `crates/fnv-actor/src/player_clips.rs` | candidate files per situation, covering the third- and first-person folder layouts; direction and aim weights; block clips; text keys (`ClipKeys`); the draw/holster state machine (`WeaponHands`). NPC locomotion and NPC weapons are `locomotion.rs` and `weapon.rs` (workstream 5 parts G and H); the weapon-type prefix is theirs (`weapon::AnimType::prefix`) |
| First-person parts (headless) | `crates/fnv-actor/src/first_person.rs` | which parts show, `…1st.nif` hands, the `Arms` shape rule |
| WEAP models | `crates/fnv-actor/src/records.rs` (`ActorForms::weapons`, part H's record) | adds WNAM and its STAT's MODL |
| Worn-armour override | `crates/fnv-actor/src/resolve.rs` (`resolve_wearing`) | `resolve` unchanged for NPCs |
| Layered playback | `engine/nvf-assets/src/anim_blend.rs` (`Animator`) | priority blending per bone, fades, a shared phase for movement clips, once/loop modes, model-space bone transforms |
| Clip data | `engine/nvf-assets/src/anim_play.rs` (`Clip`) | track priorities, text keys, `root_speed()` |
| Bindings | `engine/nvf-play/src/bindings.rs` | every `[Controls]` control with the game's default key and mouse button; `ControlInput` |
| Controls and camera | `engine/nvf-play/src/control.rs` | `PlayerControl` (sneak, weapon, Block, Always Run, Auto Move, view, distance, `Orbit`), `ControlSettings` (GMSTs), `BodyView`, `place_camera` |
| Movement keys, mouse look | `engine/nvf-play/src/player.rs` | movement through the bindings; mouse look turns the orbit while Toggle POV is held; `Player::body_eye`, `BodySet` |
| Body | `engine/nvforever/src/player_body.rs` (`PlayerBodyPlugin`, with `--play` and actors on) | both rigs, weapon models, clip caches, per-frame clip choice, eye and pivot, which rig is shown; the body in first person (§1.5: `PlayerBody::first_person_body`, `Seen`, `seated_heading`, `lying_view`); the shadow (§1.6: `Seen`, `show_rigs`, `ArmsShade`) |
| Shade light for the arms | `engine/nvf-assets/src/nif_material.rs` (`NifSurface::shade_light`), `nif_material.wesl` (`with_shade_light`); layers in `engine/nvforever/src/sky.rs` (`SHADOW_LAYER`, `ARMS_LAYER`) | §1.6 |
| Shared part building | `engine/nvforever/src/actors.rs` (`part_uses`) | NPCs and the player build parts the same way |
| Test aids | `engine/nvf-play/src/test_aid.rs` | `NVF_PLAY_TEST_EQUIP`, `NVF_PLAY_TEST_MOVE`, `NVF_PLAY_TEST_CONTROL` (`sneak`, `weapon`, `aim`, `view`, `jump`, `zoom=`, `orbit=<yaw>:<pitch>`, `release`); for a session played by hand, the debug switches `NVF_PLAY_DEBUG_GIVE` and `NVF_PLAY_DEBUG_EQUIP` (same values as the `TEST` ones, the run stays attended) |

Interface changes in `nvf-play` (coordinator, 2026-10-09): `Player::body_eye`
(`eye()` returns it when set); `place_camera` in `control.rs`; resources `Bindings`,
`PlayerControl`, `ControlSettings`, `BodyView`; set `player::BodySet`;
`activate::HudMessage` is public (other systems show lines with it); Activate (E) and
Menu Mode (Tab) are read through the bindings.

## 3. Rules and values [hypothesis unless tagged]

- Clip choice and fallbacks: animation.md §9 and `player_clips.rs`. Weapon-type movement
  clips are used while the weapon is out and exist for that direction; otherwise the
  unarmed movement clip with the aim pose over it. Female players use
  `locomotion\female\` (male clips as fallback).
- Blending: priority first, then weight (animation.md §9.4); fades from `Blend:N` keys
  (N/30 s), 0.2 s without one, 0.2 s out. The accumulation root is held at the identity:
  root motion is the character controller's.
- Aiming up and down (third person, animation.md §9.6): the level Aim clip always plays
  and shares the bones it has in common with `aimup`/`aimdown` by the pitch; an attack
  plays with its own up and down forms at the same weights. Bones only the level clip
  drives (a rifle's arms) stay with it.
- First person: `Bip01 Looking` turned about the actor's right axis by the pitch;
  no aim up/down blend there.
- Weapon placement: the `Weapon` node is re-parented to the holster bone or the right
  hand; the weapon model's shapes hang from it as in a placed reference.
- Engineering choices: jump loop after 0.15 s in the air, landing after 0.35 s; turn
  clips above 1 rad/s; third-person distances and the 8-unit wall margin; the orbit's
  return time and shoulder fade; aim up/down weights linear to ±90°; the 0.5 s holster
  hold.

## 4. Validation (2026-10-09, owner's machine, Goodsprings)

Unattended runs with the test aids [observed: logs and screenshots kept privately]:

- First version (third-person body as the only rig): pistol (`000E3778`) drawn at the
  `Attach` key, `1hpfastforward.kf` under `1hpaim.kf` at clip rate 0.97 (343 units/s),
  strafing, running backwards, crouching (`sneakmtidle.kf`), crouched running at 196
  units/s (197.7 measured in game), holstered to `Bip01 Pelvis` at `Detach`; varmint
  rifle (`0007EA24`) holstered across the back (`Bip01 Spine2`); jump loop and landing.
- Arms rig: built from `upperbody.nif` (`Arms:0`, `Arms:1`), `lefthand1st.nif`,
  `righthand1st.nif` on the 74-node first-person skeleton; the eye 118.0 above the feet
  standing and 78.0 crouched; the 9mm pistol (WNAM `1stPerson9mm`) held in the right hand
  in view, following the view looking down 50°; the varmint rifle held in both hands
  (`_1stperson\2hraim.kf`) and down the sights (`2hraimis.kf`); the rifle on
  `Bip01 Translate` while holstered (not in view).
- Circling: `orbit=150:-10` shows the crouched player from the front-right in third
  person without turning them; after `release` the camera is back behind.
- Checks: workspace tests and clippy (root and engine), the Python suites, the
  moving-camera smoke test (CLAUDE.md) — see the commit messages for the runs.
- The Pip-Boy (2026-10-10, owner report: it "looks off" on the arm, and its screen
  glows): the model hangs from `Bip01 L ForeTwist` by `Prn` and was given the rotation
  that head parts need, which put it on the back of the upper arm; and its light's glow
  shape was always drawn (actors.md §1.2). After the fix [observed: screenshots circling
  the player with `orbit=90:-5`, `-150:-10`, `30:-10`, bare and in a Vault 21 jumpsuit
  `00104184`]: a cuff on the left forearm from below the elbow to the glove, screen
  outwards and dark, the three button lights lit; in first person with the varmint
  rifle up, on the left forearm in view (it was not drawn there before).
- Rifle hands looking up and down (2026-10-10, owner report with a game screenshot:
  with a two-handed weapon the hands drift off it the further up or down the view
  goes, from the hip and down the sights): the blend gave the arms to the idle as the
  level Aim clip's weight fell (animation.md §9.4, §9.6). After the fix [observed:
  screenshots from the side with `orbit=90:<minus the pitch>`, varmint rifle
  `0007EA24`]: both hands on the rifle and the rifle along the view at 45° and 80° up
  and down, down the sights at 60° up and down, and while firing at 60° up and 45° down
  (`2hrattack8.kf` with `2hrattack8up.kf`/`down`). Walking or running with the rifle
  out the body does not bend with the pitch (open question 5).
- Minigun idle (2026-10-10, owner report with screenshots: right while firing at any
  pitch, but idle the left hand is too high, off the handle): `2hhaim.kf` does not
  drive the `Weapon` node and ours put it back at its bind pose (animation.md §9.7).
  After the fix [observed: screenshots from the front-left with `orbit=120:<minus the
  pitch>`, minigun `0000433F` and 9mm pistol `000E3778`]: idle, level and 50° up and
  down, the left hand is on the top handle as while firing, the same after firing
  stops and while running; the pistol is held as before.
- Minigun holstered (2026-10-10, owner question, then game screenshots: holstered only
  the backpack shows, on the back; drawn the gun is in the hands and the backpack
  stays): the weapon model's parts follow the clips' visibility tracks, and the
  `Backpack` node hangs from the bone its `UPB` string names (animation.md §9.7).
  After [observed: screenshots from behind, from the side and from the front with
  `orbit=90:0` and `orbit=150:0`]: as in the game screenshots; the gun appears half way
  through the draw and goes half way through putting it away.
- Moving parts (2026-10-10, owner report: no weapon's parts moved): the weapon model
  was drawn as loose shapes, so the clips' tracks on its `##` nodes had nothing to
  drive. It now keeps its node tree and the clips move its parts by name; an attack
  clip with a `Loop` key keeps going while the trigger is held (animation.md §9.8).
  After [observed: first-person screenshots of the 10mm pistol, and the body's debug
  lines]: the slide goes back at the shot, the magazine leaves the grip in the left
  hand during the reload, and the minigun's barrel turns for as long as it fires.

## 5. Open questions for the owner

1. Are the bindings in docs/spec-notes/controls.md §2 the game's defaults (they are your
   `FalloutPrefs.ini`'s)? Should NVForever's own config allow rebinding?
2. Toggle POV held in the game: does the camera stay where it was circled to after
   letting go? Does holding it in first person zoom out instead?
3. The Pip-Boy (drawn on the left forearm in both views since 2026-10-10, actors.md
   §1.2). Answered 2026-10-10: the three orange button lights are always lit; the
   outfit's `PipBoyOn` sleeve shows in first person too (now drawn). Open: (a) is the
   screen dark with the light off, as drawn now? (b) The light itself was not built then (the
   glow shape was hidden); holding Tab now switches a light (`nvf-play` `flashlight.rs`,
   2026-10-11).
4. In-game references would settle the hypotheses of animation.md §9 (captures listed
   in its §7): holster positions, aim poses, sneak speeds sideways and backwards.
5. Third person with a rifle out, walking and running while looking steeply up or down:
   does the body bend so the rifle follows the view, as it does standing? Ours keeps
   the rifle level while moving (animation.md §9.6, owner-review.md).
