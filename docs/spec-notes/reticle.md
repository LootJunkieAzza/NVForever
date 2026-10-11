# Spec note: the crosshair and the prompt under it

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` marks
what is NVForever's own. Implementation: `engine/nvf-play/src/reticle.rs` (the layout and
the drawing), `activate/outcome.rs` (`prompt`: the two lines' words), `interface.rs`
(fonts, textures and the atlas index shared with the HUD).

## 1. Sources

- **Four screenshots from the owner**, 2026-10-11, 1280 x 720 (kept privately in
  `C:\fnv-private\references\crosshair\`): the original game with nothing under the
  crosshair (1), with Doc Mitchell under it (2), a bottle cap (3) and a gun case (4).
  Every "[observed: screenshot]" is measured on them; one interface unit is 0.75 pixel
  at that size (960 units to the view's height).
- **The game's interface files** in the base archive `Fallout - Misc.bsa`:
  `menus\main\hud_main_menu.xml`, `menus\prefabs\hudtemplates.xml` and
  `menus\prefabs\text_box.xml`. Described here in plain words; nothing of them is
  shipped.
- **Textures** in `Fallout - Textures2.bsa`: `textures\interface\hud\glow_crosshair.dds`,
  `glow_crosshair_filled.dds`, `glow_hud_bottom_info_seperator_divider.dds`, the atlas
  `textures\interface\interfaceshared0.dds` with its index, and font 7.

## 2. What the files say

[documented: `hud_main_menu.xml`] The HUD has a piece `ReticleCenter` (63 x 75) and a
piece `Info` (320 x 80) in the HUD's main colour, with no position: the file says such
parameters are set in the game's code. `Info` holds one centred text box with the glow
font, an Xbox button and an empty keyboard text placed to the left.

[documented: `hudtemplates.xml`] The reticle template is one picture, 64 x 64 at (2, 9),
whose file is `glow_crosshair.dds` and which names two alternatives in its user traits:
`glow_crosshair.dds` and `glow_crosshair_filled.dds`. The template `info_seperator` is
the picture `glow_hud_bottom_info_seperator_divider.dds`, 500 x 136 at y 32, hidden
unless the code shows it. All three are in the interface atlas.

[documented: `text_box.xml`] A text box with the glow set and no font of its own uses
font 7 (`Baked-in_Monofonto_Large.fnt`, Fallout_default.ini `sFontFile_7`). Its box is
the text's width plus 20 units, the text is drawn 4 units left of where it would be
(a comment blames the glow font), and the keyboard text is drawn 5 units left of the
box.

## 3. What the screenshots and textures show

[observed: the textures, decoded]

- `glow_crosshair.dds`: two small arrows pointing at each other, white with a soft
  edge, their middle the picture's middle.
- `glow_crosshair_filled.dds`: the same arrows inside a box open at the top and bottom
  middle, `[> <]`.
- The divider: a long double line (two bright lines of 3 texels with a dimmer one
  between), 300 texels long, with a stem going down from texel 153 to 168 across,
  and a soft glow round it.

[observed: screenshots]

- **The crosshair** is the plain arrows when nothing usable is under it (1) and the
  boxed arrows when something is (2, 3, 4), drawn texel for unit (the box is 26 pixels
  wide, the texture's 40 texels at 0.75) with its middle on the view's middle, in the
  HUD's colour.
- **Two lines under it**, in font 7: the action ("Talk", "Take", "Open") with the key
  before it ("E)"), and under that the target's name. The action's middle is 3 units
  left of the view's middle and the name's 1 unit right of it, so the two lines are
  not centred on the same point [observed: both screenshots, to a pixel]. The key ends
  11 units before the action, the same in (3) and (4) although the names differ in
  width: the key goes with the action's line, not with a box round both. The action's
  line stands 326.5 units below the view's middle, the name's 40 units lower.
- **An item** (3) has a third row: the divider, drawn texel for unit with its stem 4
  units left of the view's middle and its top 8.5 units under the name's line; under
  the double line, "WG" at the left end and the weight right-aligned against the stem,
  "VAL" right of the stem and the value right-aligned at the right end, 50.5 units under
  the divider picture's top. A bottle cap shows "--" for its weight and "1" for its
  value. A container (4) and an actor (2) have no third row.

The constants of `reticle.rs` are these measurements, corrected once against
NVForever's own frame at 1280 x 720 so that the letters land on the screenshots'
pixels (the font's glyphs start a little after their pen position).

## 4. Built

- The plain and filled crosshairs from the atlas; the filled one while
  `activate::Target` holds something.
- The action with "E)" before it, the name under it, and for an item the divider with
  its weight and value from the base record (`fnv_sim::forms::Item`).
- Hidden outside play. A script's DisablePlayerControls hides the crosshair with the
  movement flag and the texts with the rollover text flag, as the stand-in did before
  [documented: GECK wiki DisablePlayerControls].
- Without the game's files: a small square and Bevy's font.

## 5. Guesses and open questions

- **A door that leads elsewhere** shows "Open" and the place it leads to as the name
  [hypothesis: not on the screenshots].
- **Locked things** keep NVForever's own words for the action, "Locked (Average)" and
  "Unlock (key)" [hypothesis]; the original's wording is not on the screenshots.
- **Weights** are written without trailing zeros ("2", "0.5", "0.25") and a weight of
  nothing as "--" (the bottle cap); a value of nothing as "--" as the barter menu does
  [hypothesis].
- **A stack** shows the count after the name, "Stimpak (3)", and the figures of one
  item [hypothesis].
- **The key's text** is "E)" for the Activate control's default key [hypothesis: where
  the game gets the letter is not in the files].
- **Not built**: the crosshair's red colour on a hostile, the sneak meter's
  "[HIDDEN]" and similar words, the extra lines the original adds for some targets
  (lock level, owned items in red, "Steal"), weapon damage and condition in the item
  row, and the HUD's fading.
