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
  the armour the simulation has equipped on the player. It stands at the character's
  feet, faces the view's heading, and carries no `RefVisual` (the crosshair never
  targets it). Shown in third person and while the camera circles the player.
- **First person**: the game's arms rig, `characters\_1stperson\skeleton.nif`, with its
  own clips (`characters\_1stperson\`). It draws the hands (their `…1st.nif` meshes) and
  only the `Arms…` shapes of the upper body and of worn upper-body armour (gloves whole;
  no head, hair or other armour; `fnv_actor::first_person`), and the weapon's
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
| Ready Item | R | weapon away: draw it; weapon out: tap = reload (not built), hold 0.5 s = holster |
| Use | left mouse | weapon away: draws it; attacking is not built |
| Block | right mouse, Left Alt | held: guns aim down the sights (`AimIS`), fists and melee block (`blockidle`) |
| Toggle POV | F, middle mouse | tap: first/third person; held while the mouse moves: the camera circles the player (not turning them) and swings back when let go; wheel zooms in third person |
| Activate | E | as before |
| Menu Mode | Tab | the inventory (the Pip-Boy is not built) |
| Rest, Vats, Hotkey 1–8 (2 = Ammo Swap), QuickSave F5, QuickLoad F9, Grab Z | | bound; pressing one shows "… : not available yet" |

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
- The crosshair's ray starts at the eye (`Player::eye`, the `Camera1st` position), so it
  matches the first-person view; in third person it still starts there [engineering
  choice; the game's third-person pick is not known].

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
| Body | `engine/nvforever/src/player_body.rs` (`PlayerBodyPlugin`, with `--play` and actors on) | both rigs, weapon models, clip caches, per-frame clip choice, eye and pivot, which rig is shown |
| Shared part building | `engine/nvforever/src/actors.rs` (`part_uses`) | NPCs and the player build parts the same way |
| Test aids | `engine/nvf-play/src/test_aid.rs` | `NVF_PLAY_TEST_EQUIP`, `NVF_PLAY_TEST_MOVE`, `NVF_PLAY_TEST_CONTROL` (`sneak`, `weapon`, `aim`, `view`, `jump`, `zoom=`, `orbit=<yaw>:<pitch>`, `release`) |

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

## 5. Open questions for the owner

1. Are the bindings in docs/spec-notes/controls.md §2 the game's defaults (they are your
   `FalloutPrefs.ini`'s)? Should NVForever's own config allow rebinding?
2. Toggle POV held in the game: does the camera stay where it was circled to after
   letting go? Does holding it in first person zoom out instead?
3. The Pip-Boy: in first person the game shows `pipboy3000\pipboyarm.nif` on the left
   arm; the simulation does not equip the Pip-Boy at the start, so it is not drawn yet.
4. In-game references would settle the hypotheses of animation.md §9 (captures listed
   in its §7): holster positions, aim poses, sneak speeds sideways and backwards.
