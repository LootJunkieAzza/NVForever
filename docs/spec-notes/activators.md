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
PlayGroup is used 864 times in the masters' scripts (scripts.md §2); which groups on
which references was not surveyed here.

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
  "simple objects that play animations when activated" play is not documented); `PlayGroup`,
  `LoopGroup` and `IsAnimPlaying` (still stubs) and so the Forward/Backward/Left/Right
  sequences of the 156 activator bases that have them; the slot machines' "Off"; `NiAlphaController`,
  `SPEC` colour, texture scale and rotate, morph and particle controllers in sequences;
  controllers outside sequences; the Open/Close sound keys (none on the receivers).
- Capture questions for the owner: the saloon jukebox switched off in game — does it
  go dark, freeze lit, or keep animating? A vintage radio switched on and off — does
  the dial flicker on and off as Open and Close say? Which way the jukebox's music notes
  scroll.
