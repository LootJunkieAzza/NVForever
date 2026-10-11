# Spec notes: activators' animations and on/off looks

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525), **[hypothesis]**
needs verification, **[design]** is our choice where the game's rule is not known.
Counts come from `fnvtool sim activator-models` (fnv.sim.activator_models/1) over
FalloutNV.esm alone, models from the default archive order; single models were read with
`fnvtool nif decode`. Sources: the owner's GECK wiki export (pages Activator, PlayGroup,
IsAnimPlaying, Radio Stations; 2026-10-09), niftools nif.xml (documentation only).

Code: `fnv_nif::anim` (`Track::controller_id`, `ValueTrack`), `nvf_assets::anim_play`
(`MaterialTrack`), `nvf_assets::material_anim`, nvforever `activators.rs`. Related:
radio.md (receivers on and off), doors.md (Open/Close on doors), nif.md §6 (emissive).

## 1. What the documents say

- "Non scripted activators can serve as drinkable water sources, radios that play radio
  stations or simple objects that play animations when activated" [documented:
  Activator]. Which animation group an activation plays is not said.
- "Random Anim Start: … this object kicks off its idle animation at a random start time"
  [documented: Activator]: activators play an idle animation by themselves.
- `PlayGroup <group> <flag>` plays an animation group on a reference; flags 0 normal,
  1 immediate start, 2 immediate loop [documented: PlayGroup]. `IsAnimPlaying [group]`
  is used on activators (`myActivator.isAnimPlaying forward`) [documented: IsAnimPlaying].
- Radios "will automatically play the radio station when turned on" [documented: Radio
  Stations]. Nothing documents how a radio or jukebox looks when on or off.

## 2. What the models hold [observed]

Activator sequences are NiControllerSequences embedded in the model. Besides transforms
they drive material members; a controlled block names the shape (`node_name`), the
property and controller types, and a controller ID saying which member:

- `NiMaterialColorController`, controller ID `SELF_ILLUM` (nif.xml TC_SELF_ILLUM
  [documented]), `NiPoint3Interpolator` with `NiPosData` keys: the emissive colour.
  Also `SPEC` on a few.
- `NiTextureTransformController`, controller ID `0-0-TT_TRANSLATE_U` / `_V` / `_SCALE_U` /
  `_SCALE_V` / `_ROTATE` (nif.xml TransformMember [documented]); every one in the
  activator models begins `0-0-`, and the controllers carry texture slot 0 (base) and
  shader map false: the ID reads `<shader map>-<slot>-<member>` [hypothesis].
- `BSMaterialEmittanceMultController` (`NiFloatInterpolator`): the emissive multiplier
  [hypothesis from the name].
- Also `NiAlphaController`, `NiGeomMorpherController`, `NiVisController`,
  `BSRefraction*Controller` and particle-system controllers.

**Radio receivers** (24 bases, ACTI with RNAM, radio.md §2) by their model's sequences:

| Sequences | Bases | Refs | Models |
|---|---|---|---|
| none | 12 | 107 | loudspeakers (`vloudspeakersr01.nif`), `vcontrolpanel02.nif`, `marker_audio.nif` |
| Open, Close | 9 | 107 | `vintageradio01.nif` (8 bases), `jukeboxdirty01.nif` |
| Idle | 3 | 6 | `jukeboxdirtynv.nif` (the saloon jukebox and one more), `jukeboxclean01.nif` |

- `vintageradio01.nif`: the shape `VintageRadioBody:0` stores emissive **black** with a
  glow map (`VintageRadio01_g.dds`). "Open" (0.67 s, clamp) keys SELF_ILLUM 0, 0, 1, 0,
  0, 1, 1 at 0, 0.13, 0.2, 0.27, 0.4, 0.5, 0.67 s: the dial flickers and lights. "Close"
  (0.37 s, clamp) keys 1, 0, 1, 1, 0: it flickers and goes dark. No sound keys.
- `jukeboxdirty01.nif`: three shapes store black; "Open" and "Close" are the same
  2.67 s flicker that ends dark (0 at 2.67 s) — a broken jukebox that does not stay lit.
- `jukeboxdirtynv.nif` (`JukeBoxDirtyNV`, `JukeBoxDirtyNVCountry`; the Prospector
  Saloon's `00107D73`): one sequence, **Idle**, 8 s, loop (cycle type 0), six tracks:
  SELF_ILLUM on `Computer Screen` (stored 0.62; brief dips to 0.15–0.37), on
  `JukeBox01:0` (the body, stored 0.86 with glow map `JukeBoxClean_G.dds`; down to 0.35
  at 2 s and 6 s, flickers to 0.1 near 2.8 s and 4.07 s), on `JukeBox01:1` (the light
  tubes, unlit shader); TT_TRANSLATE_U on `Music` (notes, 2 → 0 in 8 s), `Bubbles`
  (0 → 2) and `JukeBox01:1` (0 → 1). The stored look is the lit one; there is no Off
  or Close sequence. `jukeboxclean01.nif` has the same Idle.
- No receiver model has a light node (NiLight); the "light" is emissive colour, glow map
  and unlit textures.

**Other activators** (1,119 ACTI bases that are not receivers; 274 models with
sequences among all ACTI and TACT, counted once each): sequence-name sets by bases
(placed refs): none 184 (2,370), no model 714 (1,234),
Backward+Forward 30 (1,627), Left+Right 53 (145), Close+Open 36 (107), Forward 15 (69),
SpecialIdle 12 (34), Idle 6 (12), Idle+Off 2 (18: the Gomorrah and Tops slot machines,
"Off" a 3.33 s loop), and 20 rarer sets. Cycle types: Open and Close always clamp (16
models each); Idle loop on 20 of 21; Forward/Backward/Left/Right both. Of the TACTs, 25
bases use `terminal01.nif`/`terminal02.nif` with Idle (0.07 s), Open and Close.
Time controllers outside any sequence (no ManagerControlled flag, nif.xml
TimeControllerFlags bit 5 [documented]): 44 in the activator models (29 transform, 7
texture transform, 6 material colour controllers).

The scripts: `VintageRadioStartOnSCRIPT` only activates its reference (radio.md §4).
PlayGroup is used 864 times in the masters' scripts (scripts.md §2); §5 surveys which
groups FalloutNV.esm's scripts play on which objects.

## 3. Behaviour implemented

- **Material tracks are played** [design, from §2]: a clip's SELF_ILLUM, emittance
  multiplier and base-texture TT_TRANSLATE_U/V tracks pose the shape's material: drawn
  emissive = colour × multiplier (× glow map), a track replacing the member it drives
  [hypothesis, as nif.md §6]; a texture offset moves the texture by its value (`uv +
  offset`) [hypothesis: the direction is not documented]. Each reference whose clip
  drives a material gets its own copy of it. A clip that drives only materials now
  counts as animating its model, so models with such an **Idle** (or SpecialIdle when
  there is no Idle) play it from load like transform idles (nif.md §6), whatever their
  record type: jukeboxes, `vmonitorbank.nif`, consoles, slot machines. A shape whose
  emissive is driven loads its glow map even when its stored colour is black.
- **Receivers' on and off looks** follow the runtime's switch (`fnv_game` `radio_on`,
  off when destroyed):
  - Open/Close models: switched on plays "Open" and holds its end, switched off "Close"
    [hypothesis: as doors; consistent with the radios' stored dark dial, an Open ending
    lit and a Close ending dark]; never switched: the stored look. Timing, holding and
    turning back halfway are the doors' (`DoorMotion`, doors.md §2).
  - Idle-only models (the jukeboxes): the idle plays while on; while off it stands
    still and the emissive tracks' shapes are drawn dark [hypothesis: by analogy with the
    radios; the model has no off look, and the owner expects the jukebox to light up
    when on]. Unlit shapes (the jukebox's tubes, notes, bubbles) keep their texture: the
    unlit shader ignores emissive (nif.md §6).
  - Without `--play` (no runtime) nothing is driven: idles play, Open/Close models
    keep their stored look.
- Log: `activator: <ref> (<base>) on|off: plays Open (0.67 s) | plays its idle | holds
  its idle, lights dark | keeps its stored look`, and at exit `activator summary: N
  receivers drawn with an on/off look; switched on N times, off N times while drawn`.
- Smoke test (CLAUDE.md, the radio test): in the Prospector Saloon the jukebox is
  switched on by its OnLoad ("on: plays its idle"), off at 20 s ("holds its idle,
  lights dark"), on at 28 s; the broken radio `00109087` "keeps its stored look".
  Screenshot pair from the test spot (`NVF_PLAY_TEST_NEAR=00107D73`), crop of the
  jukebox body: mean (R, G, B) 29.4, 34.5, 20.3 on and 24.8, 31.4, 19.8 off; 42,363 of
  304,000 pixels differ by more than 10 [observed in NVForever, 2026-10-11].

## 4. Not built, and questions for captures

- Not built: activation of a non-receiver activator playing a group (which group the
  "simple objects that play animations when activated" play is not documented);
  `LoopGroup` (§5); the slot machines' "Off"; `NiAlphaController`, `NiVisController`,
  `SPEC` colour, texture scale and rotate, morph and particle controllers in sequences;
  controllers outside sequences; the Open/Close sound keys (none on the receivers).
- Capture questions for the owner: the saloon jukebox switched off in game — does it
  go dark, freeze lit, or keep animating? A vintage radio switched on and off — does
  the dial flicker on and off as Open and Close say? Which way the jukebox's music notes
  scroll.

## 5. Scripts' animation groups: PlayGroup and IsAnimPlaying

Survey: `fnvtool sim playgroup-calls --schema schemas/fnv <Data> FalloutNV.esm`
(fnv.sim.playgroup_calls/1): every compiled script unit decoded with fnv-script, each
PlayGroup (0x1013) and IsAnimPlaying (0x1128) call in statements and expressions, the
object it runs on (the explicit reference's base, else every base that carries the
script), and whether that base's model has a sequence named like the group.

### 5.1 What the documents say

- `PlayGroup <AnimGroup> <InitFlag>` plays an animation group on the calling reference.
  InitFlag 0 "Normal": the current animation finishes its full cycle, then the new one
  starts from its beginning; 1 "Immediate Start": the current one stops at once and the
  new one starts from its beginning; 2 "Immediate Loop": the current one stops at once
  and the new one starts at the beginning of its loop cycle [documented: GECK wiki
  PlayGroup, owner export]. `PlayGroup Idle 1` returns an actor to normal [documented].
- `IsAnimPlaying [AnimGroup]` is 1 while the calling object plays an animation; with a
  group, only an animation of that group's "sequence type"; the page's example is an
  activator, `myActivator.isAnimPlaying forward` [documented: GECK wiki IsAnimPlaying].
- `LoopGroup <group> [flags]` loops a group a number of times on an actor, with the same
  flags [documented: GECK wiki LoopGroup]; no opcode is known for it and no shipped
  script calls it [observed: script-functions-catalogue.md has no opcode for it].
- The group is compiled as a bare code [observed: crates/fnv-script/data/encodings.json
  `anim_group`]; the names shipped source uses for the codes it uses: 0 Idle, 2
  SpecialIdle, 3 Forward, 4 Backward, 5 Left, 6 Right, 7 FastForward, 8 FastBackward,
  9 FastLeft, 10 FastRight, 15 TurnLeft, 17 Aim, 24 Equip, 25 Unequip [observed].

### 5.2 What FalloutNV.esm's scripts do [observed]

589 calls: PlayGroup 520, IsAnimPlaying 69 (17 without a group).

| Group | flag 0 | flag 1 | flag 2 | other |
|---|---|---|---|---|
| Forward | 63 | 168 | 1 | 1 (flag 5) |
| Backward | 32 | 44 | 10 | |
| Left | 21 | 51 | | 2 (flag 5) |
| Right | 23 | 17 | 20 | |
| SpecialIdle | 21 | 3 | 22 | |
| Idle | 2 | 1 | | |
| FastForward, FastBackward, FastLeft, FastRight | 8 | 4 | | |
| Aim, Equip, TurnLeft, Unequip | | 6 | | |

- On what (call × target base): ACTI 555, STAT 131, CONT 86 (82 are `IsAnimPlaying`
  without a group in the vendor containers' script), MSTT 27, TACT 7, FURN 1; 14 calls
  run on a reference variable, 75 sit in scripts no base carries, 1 in a package result.
  STATs and MSTTs are reached through explicit references (`0x000D7D2E.PlayGroup Forward
  1`: Helios One's 75 solar reflector rows; the CG04 klaxons; boulders).
- The model has a sequence of the group's name for 543 of the 555 ACTI targets and for
  every STAT, MSTT and named CONT target; 4 ACTI targets (a roulette table, a laser
  tripwire) and the FURN marker have no sequences. Names match only case-insensitively
  in places (`nvpinto_fruited.nif` has `backward`).
- No activator sequence has a loop-start text key: their keys are start, end, `Sound:`
  (251) and a few others; flag 2 has nothing else to start from.
- Typical uses: harvestable plants (`NevadaAgavePickable` 151 refs,
  `BarrelCactusPickable` 189, `MaizePickable` 220 …): OnLoad plays Backward (not picked)
  or Forward (picked); OnActivate gives the item, plays Forward with flag 1 and calls
  `SetDestroyed 1`. Neon and window lights: script `0x0011B039` (the Strip's signs, the
  Prospector Saloon's) every frame, at night, plays Left with flag 0 if `IsAnimPlaying
  Left` is not 1, by day the same with Right; script `0x001759FC` (house windows) Left
  after 19:30, Right after 6:30. Traps: the bear trap's Forward carries `Sound:
  TRPBearFire`; tripwires, pressure plates. One-off scenes: B-29s, vertibirds, the Hoover
  Dam guns, Repconn's dome.
- Script `0x0017B074` (The Tops' and Vault 21's sign lights) tests `IsAnimPlaying
  Forward == 1 && IsAnimPlaying Backward != 1` before playing Backward: its author
  expected IsAnimPlaying to tell Forward and Backward apart.

### 5.3 Behaviour implemented

- **Rules** (fnv-game `anims`): `PlayGroup` queues a request (reference, group code, its
  name, start flag) for the engine and traces `anim_asked`, once per reference until the
  group or the flag changes [design: scripts that repeat it every frame]. `IsAnimPlaying`
  answers from what the engine reports: with a group, whether that very group plays
  [hypothesis: the documented "sequence type" may be broader; script 0x0017B074 needs
  Forward and Backward apart]; without one, whether any group asked by PlayGroup plays,
  an object's own idle not counted [hypothesis]. With an engine attached, a group counts
  as playing from the call on, until the engine reports it ended or unplayable [design:
  scripts test it right after asking]. Headless (no engine) nothing plays [design, as
  IsIdlePlaying]. Flags other than 0–2 (the three flag-5 calls) are taken as 0 [design].
- **Engine** (nvforever `object_anims.rs`): every drawn object whose model has a named
  sequence that animates it gets its own node hierarchy and a `GroupRig` [design: a
  script may name any reference; the cost is a node hierarchy per such reference,
  counted as "with named sequences" in the cells-loaded line]. A request plays the
  sequence of the group's name [hypothesis: the names match; 543 of 555 activator
  targets have it]:
  - a clamped sequence (cycle type 2) plays once, holds its end and is then reported
    ended; a looping one loops until another group replaces it [hypothesis: as the NIF
    cycle types];
  - flags 1 and 2 start it at once from its beginning (2 as 1: no loop-start key, above);
    flag 0 waits for what plays: a group played once until it ends, a loop (the object's
    idle included) until its cycle ends [documented flag meanings; that an object's idle
    counts as "the current animation" is a hypothesis]; a loop asked for again while it
    plays goes on [design];
  - an object not drawn keeps the latest request and shows it when drawn: a group played
    once at its end, a loop from its start [design]; meanwhile it is reported not
    playing;
  - the group's material tracks (emissive, emittance, texture offset; §3) play with it;
    a track on the model's root node is not applied, the root carrying the placement
    [design]; visibility, alpha, particle and other controllers are not played;
  - `Sound: <SOUN>` keys the group passes play where the object is (anim_sounds.rs).
- Log: `playgroup: <ref> plays <group> (once|loop, <s> s), flag <n>`; `… not drawn, shown
  when it is`; `… drawn, shows <group> as asked while it was not`; `… its model has no
  sequence of that name`; at exit `playgroup summary: …`; the `anim sounds summary`
  counts "object group sounds".
- Smoke test (CLAUDE.md): Goodsprings at 21:00. On load the plants' OnLoad blocks play
  Backward or Forward, the house windows Left, the Prospector Saloon's lights
  (`0x0016B5E8`, Left: a 0.07 s loop of emittance and SELF_ILLUM) Left; at 12 s the test
  aid activates the Nevada agave `0x0016AF20`, whose OnActivate plays "Forward (once,
  0.47 s), flag 1" [observed in NVForever, 2026-10-11: 50 asked (Backward 22, Forward 11,
  Left 17), 0 without a sequence of that name].

### 5.4 Not built, and questions for captures

- Not built: `LoopGroup` (no opcode); PlayGroup on actors (an actor reference has no
  `GroupRig`, so its requests are logged as not drawn; actors' idles are idles.md);
  visibility (`NiVisController`), alpha, particle and refraction controllers in the
  groups (the neon signs' alpha and visibility tracks); IsAnimPlaying's "sequence type"
  grouping, if it is broader than the group; the bear trap's sound keys were not heard
  in a run (no test case reaches a trap).
- Capture questions: does a picked agave keep its picked pose after a save and reload
  (we show Forward's end through OnLoad)? Does the saloon's sign flicker at night and
  stay steady by day? After a group played once, does an activator return to its idle?
