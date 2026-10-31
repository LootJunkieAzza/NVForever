# Spec note: the interface's fonts (`.fnt`)

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. Implementation:
`crates/fnv-font`.

## 1. Sources

- **The nine font files** of the base archive `Fallout - Textures2.bsa`
  (`textures\fonts\*.fnt`) and their textures (`textures\fonts\*_0_lod_a.dds`), read
  with `fnvtool bsa extract` and looked at as numbers and as pictures. Nothing of them
  is shipped.
- **`Fallout_default.ini`** in the game folder: the lines `sFontFile_1` to
  `sFontFile_9` name the font file of each font number.
- **The owner's screenshot** of a message box in the original game (2026-10-11, kept
  privately), against which lines set with these numbers were measured
  (`message-menu.md` §3).
- No description of the format was used. Every field meaning below is a reading of the
  numbers, checked as said.

## 2. The file

Every one of the nine files is 14 632 bytes [observed]. Numbers are little-endian.

| Bytes | What | Tag |
|---|---|---|
| 0 to 3 | a float: 31 for `monofonto_large`, 34 for `glow_monofonto_vl_dialogs`, 25 to 52 in the others. The height of one line of text: a one-line button's row in the screenshot is this and the interface file's 20 | [hypothesis, one font checked] |
| 4 to 7 | an integer, 1 in all nine. The number of textures | [hypothesis] |
| 8 to 11 | an integer, 1 in all nine | unknown |
| 12 to 295 | the texture's name, ended by a zero byte, the rest zero: the texture file's name without folder and extension (`Monofonto_Large_0_Lod_A` for `textures\fonts\monofonto_large_0_lod_a.dds`) | [observed] |
| 296 to the end | 256 glyphs of 56 bytes, one per character code | [observed: 256 x 56 is what is left] |

The parser refuses a file of another size, or one whose second number is not 1: where a
second texture's name would stand is not known.

A glyph is 14 floats [observed]:

| Number | What | Tag |
|---|---|---|
| 0 | zero in every glyph of all nine fonts | unknown |
| 1 to 8 | four corners of the glyph's picture on the texture, each a pair from 0 to 1 across and down: top left, top right, bottom left, bottom right | [observed: cutting these rectangles out of the texture gives the letters] |
| 9, 10 | the picture's width and height; equal to the rectangle's size in texels | [observed] |
| 11 | room before the picture. Zero in eight fonts; 0 to 3 in `fixedsys_comp_uniform_width` | [hypothesis, not checked on a picture] |
| 12 | room after the picture. The space has no picture and only this number (7 in `monofonto_large`). In the fonts whose letters have a glow around them it is negative (-3 in `glow_monofonto_vl_dialogs`), so the glows of neighbours overlap | [observed, §3] |
| 13 | how far the picture's top is above the line the letters stand on: 24 for a capital, 17 for a small letter without ascender in `monofonto_large`, where "g" is 25 high and so reaches 8 under the line | [observed: letters set by it stand on one line] |

The codes are those of the game's text, Windows-1252 [hypothesis: the letters at the
codes of plain English text are right; the accented ones were not checked]. About 113
codes have no picture in each font.

The texture of `monofonto_large` is 256 x 256, 16 bits a texel with 4 of them alpha
[observed]; the letters are in the alpha, the colour is a light grey.

## 3. Setting text

- A character takes its room before, its width and its room after; the next starts
  there [observed: the screenshot's widest line, "under some suitcases in the trunk of
  his car during", is 620 units by this sum, which is exactly the interface file's wrap
  width for that text, and its measured width agrees with the box's to 1%; the three
  lines above it agree as well].
- A line is broken between words, before the word that would end beyond the wrap width;
  a line exactly as wide as the wrap width is not broken [observed: the same line].
  The space at the break is not set [design]. What the game does with a word wider
  than the wrap width is not known; NVForever leaves it whole on its own line.
- How far apart the lines are, and where a text's first line stands under its top, is
  with the menu that was measured (`message-menu.md` §3). The interface files' text
  tiles: menu-xml.md §3.1.2, hud.md §14.10 (a line per font, #979).
- [observed: hud_textbase capture 2026-10-31] A text tile's `height`, read in the
  original, is its font's tallest glyph picture: 36 for font 1, 42 for font 7, 64 for
  font 8, the largest number 10 of their files (menu-xml.md §3.1.2).
- [observed: the textures of fonts 1, 7 and 8, 2026-10-31, decoded and measured row by
  row in alpha] Where the ink of a capital or a digit stands against the line (a
  picture set with its top number 13 over it): font 7 (`baked-in_monofonto_large`, a
  capital 30 over, 36 high) from 29 over to 5 under at alpha 16, 25 over to 1 under at
  128, 23 over to 1 over at 192; font 8 (`glow_futura_caps_large`, a capital 47 over,
  58 high) 44 over to 8 under at 16, 39 over to 3 under at 128, 37 over to 1 under at
  240; font 1 (`glow_monofonto_large`, a capital 25 over, 30 high) 24 over to 4 under
  at 16, 22 over to 2 under at 128, 21 over to 1 under at 192. The glow fades as far
  under the letters as over them, so the middle of the ink is the same at every alpha:
  12 over the line in font 7, 18 in font 8 (17.5 for its digits), 10 in font 1.
- For comparison [observed: the files]: the size less the deepest glyph's reach under
  the line (number 10 less number 13) is 21, 24 and 37 for fonts 1, 7 and 8 (31 - 10,
  36 - 12, 52 - 15); the tallest reach over it 29, 34, 50.

### 3.1. Drawing the glyph pictures

- [observed: issue #1221, original and NVForever message-box captures at 1280x720,
  2026-10-31] In the original, the x-height letters in "NVF box without buttons"
  share their top and bottom ink rows. In NVForever, even the two occurrences of
  `u` differ: one is 12 rows high and the other 10. This is a drawing error after
  the font's measurements have been read, not a different ascent for each occurrence.
- [documented: Bevy 0.20.1, `bevy_ui_render/src/lib.rs`, `NodeImageMode::Auto`
  extraction; `bevy_ui/src/layout/mod.rs`, `ui_layout_system`] The default image
  mode fits a texture rectangle inside its node while preserving its aspect ratio.
  Layout rounding can give equal glyphs at different x positions different box
  widths; the fit then changes both their width and height. `Stretch` fills the
  specified box, and `LayoutConfig { use_rounding: false }` retains its fractional
  dimensions and position.
- [observed: issue #1221 temporary Bevy probe, 2026-10-31] The raw font-2 file and
  texture, at 1280×720, were drawn at physical x 532 with a rounded shared baseline
  plus each of 0, 0.125, 0.25, 0.375, 0.5, 0.625 and 0.75 pixels. Ink rows at red > 80
  and green > 30 were: at 0, capitals 17, x-height letters 12 and 11, `t` 14; at
  0.125, capitals 17, all x-height letters 11, `t` 14; at 0.25, capitals 16, all
  x-height letters 11, `t` 14; and at 0.375 through 0.75, capitals 17, all x-height
  letters 12, `t` 15. The 0.25 result is the original capture's row count.
- [design: #1221] Snap one zero-sized line anchor to physical pixels, then draw
  its glyphs without layout rounding, at their fractional horizontal positions and
  their file ascent above the anchor. Give the rounded anchor a fixed quarter-pixel
  sampling phase. The game uses a 1:1 logical-to-physical pixel scale [documented:
  `engine/nvforever/src/main.rs`, `WindowResolution::with_scale_factor_override(1.0)`],
  so the offset is expressed in Bevy's `Px`. Stretch each texture rectangle into its
  measured box. This retains a common baseline, including after resizing, without
  changing the per-font baseline offsets measured in #979. The terminal's coloured
  characters and the character creator's scaled text use the same rule. Stand-in
  text without a game font texture keeps its existing Bevy text path.

## 4. Font numbers

[documented: `Fallout_default.ini`] 1 `Glow_Monofonto_Large`, 2 `Monofonto_Large`,
3 `Glow_Monofonto_Medium`, 4 `Monofonto_VeryLarge02_Dialogs2`,
5 `Fixedsys_Comp_uniform_width`, 6 `Glow_Monofonto_VL_dialogs`,
7 `Baked-in_Monofonto_Large`, 8 `Glow_Futura_Caps_Large`, 9 `NVFont_Test`.
NVForever uses these defaults; it does not read the player's own INI for them yet.
