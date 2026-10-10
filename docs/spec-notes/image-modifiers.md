# Spec notes: image space modifiers (screen effects)

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525), **[hypothesis]**
needs verification, **[design]** is our choice where the game's rule is not known.
Counts come from `crates/fnv-world/examples/imad_dump.rs` over `FalloutNV.esm` and from
`fnvtool script decompile` of the same plugin.

An image space modifier is an `IMAD` record played over the picture: a fade to black or
white, blurred or doubled vision, a tint, a change to brightness or contrast. The same
record type grades a weather by time of day (world.md §6). Scripts play one with
`ApplyImageSpaceModifier` (`imod`), end it with `RemoveImageSpaceModifier` (`rimod`) and
ask about it with `IsImageSpaceActive`.

Code: `fnv_world::image_modifier` (the record, its tracks, the effect at a moment, several
at once), `fnv_game` (`Game::image_modifiers`, the three script functions),
`engine/nvforever/src/imagespace.rs` with `imagespace.wgsl` and `imagespace_blur.wgsl`
(the picture).

## 1. The IMAD record

- **DNAM** [documented: xEdit wbDefinitionsFNV.pas IMAD DNAM], little-endian:

  | Bytes | Field |
  |---|---|
  | 0 | "Animatable", u32 used as yes/no |
  | 4 | "Duration", f32, seconds |
  | 8 | 21 pairs of u32 key counts (multiply, add): 14 HDR parameters, 3 bloom, 4 cinematic |
  | 176 | key counts, u32 each: tint colour, blur radius, double vision strength, radial blur strength, radial blur ramp up, radial blur start |
  | 200 | "Radial Blur - Use Target", u32 used as yes/no |
  | 204 | radial blur centre X, Y, f32 each |
  | 212 | key counts, u32 each: depth of field strength, distance, range |
  | 224 | "DoF - Use Target", u8 used as yes/no |
  | 225 | "DoF Flags", u8: bit 0 "Mode - Front", bit 1 "Mode - Back", bit 2 "No Sky" |
  | 226 | 2 unused bytes |
  | 228 | key counts, u32 each: radial blur ramp down, radial blur down start, fade colour, motion blur strength |

  244 bytes in 196 of the 215 records; 240 in 8, 236 in 10 and 188 in 1, the fields
  past the end missing [observed]. Every key count equals the number of keys in its
  subrecord: 11,825 counts checked, none differs [observed].
- **Key lists** [documented: xEdit IMAD wbTimeInterpolators, wbColorInterpolator]: each
  parameter is a subrecord of (time f32, value f32) keys; the tint colour (`TNAM`) and
  the fade colour (`NAM3`) are lists of (time f32, red, green, blue, alpha f32) keys.
  - Multiply/add pairs: `\x00IAD`..`\x14IAD` multiply and the same number + 0x40 add
    (`@IAD`..`TIAD`); the names are in schemas/fnv/sky.json. The cinematic four are
    0x11 saturation, 0x12, 0x13 and 0x14 brightness; 0x12 is read as contrast average
    luminance and 0x13 as contrast, the order world.md §6 argues for [hypothesis].
  - Single lists: `BNAM` blur radius, `VNAM` double vision strength, `RNAM` radial blur
    strength, `SNAM` its ramp up, `UNAM` its start, `NAM1` its ramp down, `NAM2` its down
    start, `WNAM` depth of field strength, `XNAM` its distance, `YNAM` its range, `NAM4`
    motion blur strength.

## 2. What the records hold [observed]

- 215 records: 94 animatable, 121 not. 24 of the latter are the weather adapters
  (names ending in `IS`).
- **Key times run from 0 to 1** in every record but one (the weather adapter
  NVWastelandNightIS has keys up to 2), while the durations of animatable records run up
  to 106 s. So a key's time is a part of the duration, not seconds [hypothesis from this].
  One animatable record has duration 0 (HVBunkerExplosionISFX).
- Records that use each effect: brightness 166, tint 127, 0x12 102, saturation 97, 0x13
  93, fade colour 64, blur 62, radial blur 39, double vision 26, motion blur 7, depth of
  field 6. Largest values: blur radius 5, double vision 1, radial blur strength 10, depth
  of field strength 2, motion blur 0.5.
- Radial blur "use target" is set in 6 records (2 of them with centre 1, 1; all others
  0.5, 0.5); depth of field "use target" in 3, its flags are 1 in 2 records.
- **The opening's records:**

  | Record | Animatable, duration | Keys (time: value) |
  |---|---|---|
  | WhiteISFX 0x000AC03C | no (1 s) | fade colour white, amount 0: 1, 1: 0 |
  | MQ08AllBlackedOutISFX 0x00026370 | no (1 s) | brightness multiply 0: 0, 1: 1 |
  | VCG01FadeInFromBlackISFX 0x00120E95 | yes, 7 s | fade colour **white**, amount 0: 1, 0.71: 0; blur radius 0: 5, 0.5: 3, 0.8: 4, 1: 0; double vision 0: 0.1, 0.5: 0, 0.8: 0.05, 1: 0 |
  | VCG01PlayerStrainISFX 0x00120E96 | yes, 10 s | 0x12 multiply 0: 1, 0.17: 2, 1: 1; brightness multiply 0: 1, 0.25: 2, 1: 1 and add 0: 0, 0.02: −0.75, 0.17: 1, 1: 0; blur radius 0: 0, 0.02: 2, 0.17: 3, 1: 0; double vision up to 0.01; radial blur strength 0.17: 0.3, ramp up 0.17: 0.5 |
  | FadeToBlackISFX 0x000269C4 | yes, 4 s | fade colour black, amount 0: 0, 0.33: 1, 0.67: 1, 1: 0 |

  The other records VCG00's graveyard stages play are FadeInFromBlack2sISFX,
  FadeToBlack2sISFX (both 1 s, despite the names) and Addiction01ISFX (6 s).

## 3. Playing one

- **ApplyImageSpaceModifier** plays the modifier, **RemoveImageSpaceModifier** removes
  it from the screen, **IsImageSpaceActive** returns 1 while it is active [documented:
  GECK wiki pages of the three functions, owner export].
- **Animatable** [documented: GECK wiki "ImageSpace Modifiers"]: "If checked, the
  modifier has a duration and is animated. If this is not checked, the duration box is
  ignored."
- **A record that is not animatable stays at its first keys until it is removed**
  [hypothesis]. Evidence [observed]: MQ08AllBlackedOutISFX's brightness multiplier is 0
  at time 0 and 1 at time 1, so only its first key blacks the screen out; WhiteISFX is
  white only at its first key; in the capture of the new-game save the screen stayed
  white for as long as the session was left alone (layer5-captures.md §10.2); and every
  one of the 9 such records a script applies is also removed by a script.
- **An animatable record plays its keys over its duration and then ends by itself**
  [hypothesis]. Evidence [observed]: 29 script lines wait until `IsImageSpaceActive`
  reads 0 on FadeToBlackISFX before going on, and no script removes it; its own keys
  bring the picture back by time 1. Of the 38 animatable records scripts apply, scripts remove 5.
- Between keys the value moves in a straight line; before the first and after the last
  key it holds [hypothesis: xEdit calls the lists interpolators, the curve is not
  documented].
- **Applying one that is already playing starts it again** (one instance per record)
  [hypothesis; the game may stack instances. VCG00 applies Addiction01ISFX at stages 27
  and 30, half a second apart].
- **A modifier's time passes with the game outside menus and loading screens,
  conversations included** [hypothesis: `Game::tick` ages modifiers where it advances a
  conversation's lines]. Not known: whether an animated modifier goes on behind a
  message box or the pause menu in the game.
- A record with animatable set and duration 0 ends in the frame after it is applied
  [design].

Trace events (`fnv.script.trace/1`): `image_modifier_applied {modifier}` and
`image_modifier_removed {modifier, ended}` (`ended` when an animated one ran out).

## 4. Several at once

- **Tint** [documented: GECK wiki "ImageSpace Modifiers"]: the colour is the average of
  the active modifiers' tint colours weighted by their alpha; the alpha is the highest of
  them.
- **Blur radius and double vision** [documented: the same page]: the highest value among
  the active modifiers.
- **Multiply/add parameters** [documented: the same page]: the parameter × multiply +
  add; "to simply set a parameter to a target value, multiply by zero and add the target
  value". With several modifiers the multipliers are multiplied and the adds summed
  [hypothesis].
- Fade colour: as the tint. Radial blur and depth of field: those of the modifier with
  the greatest strength. Motion blur: the highest [hypothesis, all three].
- The image space's own tint (with the weather's adapter, world.md §6) counts as one more
  tint in the average [hypothesis].

## 5. On the picture

Parameter meanings [documented: GECK wiki "ImageSpace Modifiers"]: blur radius 0 none,
7 the most; double vision strength "what percentage of the screen to apply the effect
to", 0 to 1, "animates automatically", 0.15 "jarring but still readable", 0.35
"gibberish"; radial blur strength 2.5 moderate, 5 strong; ramp up 0 never blurs, 1 is at
full strength at once; start is the part of the screen around the centre left sharp (0
all blurred, 1 none); ramp down and down start are the same from the outside in, leaving
a sharp ring at the edge; depth of field distance and range in game units.

The renderer's formulas are ours [hypothesis, every constant; to be set against
screenshots of the game, §7]:

1. **Blur**: a Gaussian blur of the tonemapped picture, in two passes (across, down).
   A radius of 1 reaches 1/270 of the screen height to each side (two standard
   deviations), so radius 5 is about 2% of the height. Reasoning: the wiki gives the HDR
   blur radius as whole numbers from 1 to 7, which reads like a reach in pixels of a
   reduced picture.
2. **Double vision**: two copies of the picture side by side, each at half strength,
   strength × screen width apart at most, swinging between 0.2 and 1 of that with a
   period of about 4 s.
3. **Radial blur**: 12 samples from each point towards the centre, reaching strength ×
   2% of the point's distance from the centre, scaled by the ramps: 0 inside `start`,
   rising by ramp up ÷ (1 − ramp up) per unit of distance (1 = the far corner of a
   centred screen), falling the same way from `down start` by ramp down.
4. **Grade**: the image space's cinematic values with the modifiers' multiply/add, then
   the grade's formula (world.md §6).
5. **Fade colour**: the graded picture mixed towards the colour by its alpha, on display
   values, last. That the fade covers the grade is [observed]: in the new-game save
   WhiteISFX and MQ08AllBlackedOutISFX (brightness × 0) are both active and the screen
   is white (layer5-captures.md §10.2).

Menus, message boxes, subtitles and the HUD are drawn over the result and are not
affected [observed for a message box: the same capture shows the pack box over the white
screen].

Not drawn: depth of field (6 records), motion blur (7), the HDR and bloom parameters
(the renderer has no eye adaptation or bloom pass), radial blur and depth of field "use
target".

## 6. The opening

VCG00 stage 0 applies WhiteISFX and MQ08AllBlackedOutISFX; stage 90 applies the latter
again and stage 100 removes it; VCG01 stage 0 removes WhiteISFX and applies
VCG01FadeInFromBlackISFX; VCG01 stage 5 applies VCG01PlayerStrainISFX [observed in the
masters]. So the player sees white, then over 5 s the room comes out of the white,
blurred and doubled, sharp after 7 s; with Doc's first line over, a dark blink and a
bright, blurred flash that settle over 10 s.

`nvforever --play --new-game` (2026-10-10, 1280×720, boxes answered after 1 s): the log
line "image space modifiers on the screen:" gives WhiteISFX, MQ08AllBlackedOutISFX at
the first script frame; VCG01FadeInFromBlackISFX at VCG01 0 (3.05 s of game time);
VCG01PlayerStrainISFX joins at stage 5 (8.69 s); the fade-in ends at 10.12 s (7.06 s
after it began; the pack boxes held it meanwhile) and the strain at 18.67 s; then none.
Until the scripts have started the new game's screen is white [design: the script data
loads while the room is already drawn].

To look at one modifier: `nvforever ... --imod <Editor ID or 0xFormID>[@seconds]`
(repeatable). An animated one plays again and again, a second apart; `@seconds` holds it
at that time.

## 7. Open

- Captures to set the constants of §5: game screenshots at known moments of
  VCG01FadeInFromBlackISFX and of a record applied from the console (`imod`), e.g.
  blur radius alone and double vision alone, against `--imod <record>@<seconds>` at the
  same window size.
- Whether a second apply restarts or stacks; whether animated modifiers go on behind
  menus (§3).
- Depth of field and motion blur; "use target".
- Other things that play modifiers are not built: explosions and weapons (their records
  name an IMAD), `TriggerScreenSplatter`, `SetCellImageSpace`, the get-hit and
  night-eye blocks of the image space.
- `ApplyImageSpaceModifier` given a form that is not an IMAD fails the call here; the
  game's behaviour is not known.
