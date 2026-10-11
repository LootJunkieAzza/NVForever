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
  with the menu that was measured (`message-menu.md` §3).

## 4. Font numbers

[documented: `Fallout_default.ini`] 1 `Glow_Monofonto_Large`, 2 `Monofonto_Large`,
3 `Glow_Monofonto_Medium`, 4 `Monofonto_VeryLarge02_Dialogs2`,
5 `Fixedsys_Comp_uniform_width`, 6 `Glow_Monofonto_VL_dialogs`,
7 `Baked-in_Monofonto_Large`, 8 `Glow_Futura_Caps_Large`, 9 `NVFont_Test`.
NVForever uses these defaults; it does not read the player's own INI for them yet.
