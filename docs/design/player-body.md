# The player's body (`--play`): animated locomotion, weapons and a head-tied camera

Status 2026-10-09: first version merged for review on `claude/player-body`. Owner request
2026-10-09: using the NPC assembly and animation playback (docs/design/static-actors.md
parts B, C, E), put together a player character model that moves with the animations,
with the camera tied to the head; full locomotion forwards, backwards and sideways;
crouching, and moving while crouched; drawing and holstering weapons.

Where it sits: the player's own body is Layer 4 play (`nvf-play` owns the controls and
the camera) drawn with the Layer 3 actor assembly (`nvforever`); choosing clips for an
actor under direct control is the small, player-only part of Layer 6 animation selection
that this needs. NPC animation stays as it was (one idle clip, actors.md §7). Facts and
readings of the clips: docs/spec-notes/animation.md §8.

## 1. What the player sees

- **Body**: the player's NPC_ (FormID 7, `fnv_sim::FormId::PLAYER_BASE`) resolved like a
  placed actor (`fnv_actor::Resolver::resolve_wearing`): race body, head and FaceGen
  face, hair, eyes, wearing the armour the simulation has equipped on the player (not the
  NPC_'s carried items). An outfit change in the inventory menu rebuilds the body. It
  stands at the character's feet (interpolated like the camera), faces the view's
  heading, and carries no `RefVisual` (the crosshair never targets it).
- **Moving**: W/S/A/D play the forward, backward, left and right clips, walking with
  Shift held and running otherwise; diagonals blend the two clips by the keys' shares.
  The clips play at the speed the character actually moves (sliding along a wall slows
  the legs). Standing still and turning plays the turn clips. In the air the jump loop
  plays, and a fall of 0.35 s or more ends with a landing clip.
- **Crouching**: Left Ctrl toggles sneaking: the crouched idle and crouched movement
  clips, at `fMoveSneakMult` (0.57) of the speed — 196 units/s running, against 197.7
  measured in game (physics.md §6.3). The eye drops from 117.7 to about 64 units above
  the feet.
- **Swimming**: the swimming clips, by the same direction rule.
- **Weapons**: R toggles the weapon out or away. The equipped weapon's DNAM animation
  type selects the clip set (`1hp`, `2hr`, … ; fists `h2h` with no weapon). Drawing plays
  the equip clip; at its `Attach` key the weapon model moves from its holster to the
  right hand; then the Aim pose is held over whatever the legs do, bent up and down with
  the view's pitch. Holstering plays the unequip clip and the weapon returns at `Detach`:
  pistols to the right hip, rifles across the back (the holster clip's `prn:` bone).
  The right mouse button aims down the sights (`AimIS` clips) for weapon types that have
  them. Crouched, the crouched variants of every weapon clip are used where they exist.
- **Camera**: F toggles first and third person.
  - First person: the camera is at the head's eye — the centre of the race's eye parts,
    measured from their meshes, carried by the head bone through the animation — so it
    crouches, bobs and leans with the body. The head, hair and headgear are hidden.
    Looking down, the eye swings forward about the neck (60% of the pitch while no weapon
    is held; with a weapon out the aim-up/down clips bend the body instead), so the
    player sees their chest, legs and feet. Aiming down the sights brings the weapon and
    hands into view.
  - Third person: behind the player at a distance set by the mouse wheel (60–450, 170 to
    start), offset by `fOverShoulderPosX` (40, right) and `fOverShoulderPosZ` (−10, up)
    from a pivot above the feet at eye height; pulled in front of collision between the
    pivot and that spot.
- The crosshair's ray starts at the body's eye (`Player::eye`), so it matches the
  first-person view; in third person it still starts there, 40 units to the left of the
  camera's line of sight [engineering choice; the game's third-person pick is not known].

## 2. Code

| Item | Where | Notes |
|---|---|---|
| Clip choice (headless) | `crates/fnv-actor/src/locomotion.rs` | candidate files per situation in preference order, direction and aim weights, text keys (`ClipKeys`), the draw/holster state machine (`WeaponHands`), WEAP animation type → prefix; unit tests |
| WEAP model and animation type | `crates/fnv-actor/src/records.rs` (`ActorForms::weapons`) | MODL, DNAM animation type |
| Worn-armour override | `crates/fnv-actor/src/resolve.rs` (`resolve_wearing`) | `resolve` unchanged for NPCs |
| Layered playback | `engine/nvf-assets/src/anim_blend.rs` (`Animator`) | priority blending per bone, fades, a shared phase for movement clips, once/loop modes, model-space bone transforms; no allocation per frame once clips are bound; unit tests |
| Clip data | `engine/nvf-assets/src/anim_play.rs` (`Clip`) | now keeps track priorities, text keys, `root_speed()` |
| Controls and camera | `engine/nvf-play/src/control.rs` | `PlayerControl` (sneak, weapon out, aiming, view, distance), `ControlSettings` (GMSTs), `BodyView` (third-person pivot), `place_camera` |
| Player interfaces | `engine/nvf-play/src/player.rs` | `Player::body_eye`, `BodySet` (between the character's move and the camera), sneak speed in `step` |
| Body | `engine/nvforever/src/player_body.rs` (`PlayerBodyPlugin`, added with `--play` when actors are on) | spawn/rebuild, weapon model, clip cache, per-frame clip choice, eye and pivot, first-person hiding |
| Shared part building | `engine/nvforever/src/actors.rs` (`part_uses`) | NPCs and the player build parts the same way |
| Test aids | `engine/nvf-play/src/test_aid.rs` | `NVF_PLAY_TEST_EQUIP`, `NVF_PLAY_TEST_MOVE`, `NVF_PLAY_TEST_CONTROL` |

Interface changes in `nvf-play` (coordinator, 2026-10-09): `Player` gained `body_eye`
(written in `BodySet`; `eye()` returns it when set); `place_camera` moved from
`player.rs` to `control.rs`; new resources `PlayerControl`, `ControlSettings`,
`BodyView`; new set `player::BodySet`. Without actors (`--no-actors`) nothing writes
`body_eye` and the eye stays at `EYE_HEIGHT` above the feet as before.

## 3. Rules and values [hypothesis unless tagged]

- Keys: Left Ctrl sneak, R weapon, right mouse aim, F view, wheel zoom (the game's
  defaults as far as known; the game holds R to holster where this toggles).
- Clip choice and fallbacks: animation.md §8 and `locomotion.rs`. Weapon-type movement
  clips are used while the weapon is out and exist for that direction; otherwise the
  unarmed movement clip with the aim pose over it. Female players use
  `locomotion\female\` (male clips as fallback).
- Blending: priority first, then weight (animation.md §8.4); fades from `Blend:N` keys
  (N/30 s), 0.2 s without one, 0.2 s out. The accumulation root is held at the identity:
  root motion is the character controller's.
- Weapon placement: the `Weapon` node is re-parented to the holster bone or the right
  hand; the weapon model's shapes hang from it as in a placed reference (the node
  replaces the model's root).
- Engineering choices: the eye fallback (9 forward, 7 up from the head bone) when the eye
  parts cannot be measured; jump loop after 0.15 s in the air, landing after 0.35 s;
  turn clips above 1 rad/s; the 60% neck share of the pitch; third-person distances and
  the 8-unit wall margin; aim up/down weights linear to ±90°.

## 4. Validation (2026-10-09, owner's machine, Goodsprings)

Unattended runs with the test aids [observed: logs and screenshots kept privately]:

- The body is built from 12 parts (upper body, hands, head with FaceGen, mouth, teeth,
  tongue, eyes, eyebrows, hair) on `characters\_male\skeleton.nif`; the eye is 117.7
  units above the feet standing.
- `NVF_PLAY_TEST_EQUIP=000E3778` (9mm pistol, `1hp`): R draws (`1hpequip.kf`, weapon to
  `Bip01 R Hand` at the `Attach` key), running forward lays `1hpfastforward.kf` under
  `1hpaim.kf` at clip rate 0.97 (343 units/s), walking right `1hpright.kf`, running back
  `1hpfastbackward.kf` (rate 0.80–0.85 at 258–273 units/s), Ctrl crouches
  (`sneakmtidle.kf`, `sneak1hpaim.kf`; eye 64 above the feet), crouched running
  `sneakmtfastforward.kf` at 190–196 units/s, R holsters (`sneak1hpunequip.kf`, weapon
  back to `Bip01 Pelvis` at `Detach`), the pistol at the right hip.
- `NVF_PLAY_TEST_EQUIP=0007EA24` (varmint rifle, `2hr`): holstered across the back
  (`Bip01 Spine2`), drawn with `2hrequip.kf`, `2hraim.kf` standing, `2hraimis.kf` with
  the right mouse button (rifle and both hands in the first-person view), running
  `locomotion\2hrfastforward.kf`.
- Jumping while running: `mtjumploopforward.kf` in the air, `mtjumplandforward.kf` on
  landing.
- First person looking down 75°: chest, legs and feet below the view.
- Checks: workspace tests and clippy (root and engine), the Python suites, the
  moving-camera smoke test (CLAUDE.md) — see the commit message for the run.

## 5. Open questions for the owner

1. The keys: Left Ctrl, R (toggle), right mouse, F — as in your game? Should R need a
   hold to holster, as in the game?
2. First person: this is a "true first person" view from the body's own head. The game
   draws first person with a separate arms-only skeleton (`characters\_1stperson`, 442
   clips). Keep the body view, offer both (a setting), or switch to the game's way?
3. Third person: the body always faces the view's heading. In the game, does the
   character turn independently while standing (camera orbiting)?
4. In-game references would settle the hypotheses of animation.md §8 (captures listed
   in its §7): holster positions, aim poses, sneak speeds sideways and backwards.
