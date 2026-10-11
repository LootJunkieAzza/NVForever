# Spec note: the message box

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` marks
what is NVForever's own. Implementation: `engine/nvf-play/src/message_menu.rs` (the
box), `script_hud.rs` (what it holds, its keys), `crates/fnv-font` (the fonts,
`fonts.md`).

## 1. Sources

- **Two pictures from the owner**, 2026-10-11 (kept privately): a screenshot of the
  original's box for the message "Vance's Gun" (806 x 694, a cut from a larger screen),
  with a title, thirteen lines of text and an OK button; and a small picture (180 pixels
  wide) of a box without title, with three buttons one under the other. Every
  "[observed: screenshot]" is measured on the first.
- **The game's interface files** in the base archive `Fallout - Misc.bsa`:
  `menus\message_menu.xml` and the shared pieces it includes (`menus\prefabs\line.xml`,
  `vertical_fade_line.xml`, `bottom_bracket.xml`, `list_box.xml`,
  `list_box_template.xml`, `box.xml`, `brightness.xml`, `menus\globals.xml`). Described
  here in plain words; nothing of them is shipped.
- **The textures those files name**, in `Fallout - Textures2.bsa` under
  `textures\interface\shared\`: `line\fade_to_bottom.dds`, `line\fade_to_top.dds`,
  `background\solid_black.dds`, `solid.dds`. (`textures\interface\hud\` holds the
  compass, the crosshair and the meters; the message box uses none of them.)
- **The GECK wiki's "Message" page**, from the owner's export
  (`MiscWithmessage.xml`).

## 2. What the files say

[documented: GECK wiki, Message] A message has a title, a text and up to ten buttons,
each with conditions for showing. With the "Message Box" flag it is a box that stops
the game and waits for an answer; otherwise it is shown briefly at the top left with an
icon. Texts over 1023 characters crash the original.

[documented: `menus\message_menu.xml` and its pieces] In the interface's units:

- The box is at least 200 and at most 700 wide; 40 are kept free at the sides and 20
  above and below. Its width is "set in code". It is centred on the screen.
- At the top: a short line fading downwards at each end (40 high), and a horizontal
  line with the title standing in it. The title is in font 6, 10 above the line,
  starting 60 from the left; the line runs from the left end to 10 before the title and
  from 10 after it to the right end. Without a title the line is whole.
- The text is in font 2, wrapped at 620 (700 less the free 40 on both sides), starting
  40 from the left, and 15 under the title's bottom (the title's height, plus 20, less
  5, counted from the title's top). The file also has a place for a 100 x 100 icon left
  of the text, off unless the code turns it on; then the text is 120 narrower.
- Under the text, 20 lower than its bottom, a list of the buttons one under the other.
  A button's row is as high as its text and 20 more, with the text at the bottom of the
  row, centred; button texts wrap at 560. The row of the chosen button has a box around
  it, one unit less high than the row: a line all round at brightness 200 of 255 and a
  fill at 40 of 255 opacity and half brightness. The list scrolls if it grows over 420.
  Its buttons play `UIMenuOK` when clicked and `UIMenuFocus` when the pointer comes on
  them.
- The bottom line is 20 under the list, with a line at each end fading upwards.
- Behind it all is a black picture with soft upper and lower edges, a tenth wider and
  three tenths taller than the box and a little up and left of it; how opaque is "set
  in code".
- Lines are 2 thick on a PC (times a factor for the screen that the file does not give).
- Every part is in the interface's colour; none sets its own.
- The fonts are named by number; `Fallout_default.ini` gives font 2 as
  `Monofonto_Large` and font 6 as `Glow_Monofonto_VL_dialogs` (`fonts.md` §4).

## 3. What the screenshot adds

The screenshot's box is 763 pixels wide for the 700 units of a box with full-width
text, so one unit is 1.09 pixels there (a screen 1050 high for the interface's 960)
[observed]. In units, measured down from the top line:

| What | Measured | Reading |
|---|---|---|
| Title's letters stand at | 11.5 | 21.5 under the title's top (10 above the line) |
| Top line resumes after the title at | 237.7 from the left | 60 + the title's width by the font file (168) + 10 |
| First text line stands at | 58.0 | see below |
| Text lines apart | 34.8 | 35: the font's 31 and 4 |
| Frame of the button, top to bottom | 513.7 to 563.1 | row 51 high (31 + 20), frame one less |
| "OK" stands at | 548.9 | 35.2 under its row's top: 20 and 15 |
| Frame's width | 70.4 between its lines' middles, 72 outside | the text "OK" is 29 wide: the text and about 42 |
| Frame's middle | 3.7 right of the box's | the file centres the list 10 left and leaves room for a scroll bar at its left; 2.5 right if that room is 30 |
| Bottom line | 585.5 | row's bottom and 20 (584.7) |
| Corner lines, top and bottom | fade out over about 40 | |
| Lines' thickness | 2 pixel rows | 2 units |

From these, all [hypothesis] fitted to this one screenshot:

- A text's first line stands 15 under the text's top in font 2. With that, the button
  and the text agree, and the text starts 43 under the top line, which makes the
  title's height 38 (the title font's file says 34; 38 is its tallest glyph).
- A text of n lines is 31 + 35 (n - 1) high: with thirteen lines that puts the button's
  row at 514, as measured. One line is the font's 31, as the button's row shows.
- The box's width is the text's widest line and the two free 40s, within 200 to 700
  (the screenshot only shows the full 700).
- The background is nearly opaque: the room behind the box can hardly be made out.
  NVForever uses the Wait menu's 0.93 (`wait-menu.md` §2).
- The text is left-aligned, the buttons centred, the title at the left.
- The colour is amber; the brightest letters are near (255, 182, 66).

The small picture agrees where it can be read: no title and a whole top line, the text
at the left, three buttons one under the other about 50 apart, centred.

## 4. What NVForever does

- Draws the box by §2 and §3 with the game's two fonts and three textures, read from
  the owner's archives at run time. Without them (no archives) the same layout is drawn
  with Bevy's font and plain lines.
- The title is the message's name (MESG `FULL`); a box without buttons gets one "OK"
  button [observed: screenshot]. The label is fixed, not yet the game setting's.
- The first button is the chosen one when the box opens [hypothesis: the screenshot's
  OK is framed]. The pointer chooses the button it comes onto [documented: the list's
  rows take the pointer]; a click picks it. Up and Down move the choice and Enter picks
  it; number keys pick a button by its place; Space presses the OK of a box without
  buttons [design: the keys; the original's are not known from these sources].
- The box is made wide enough for its title and for the widest button's frame
  [design].
- The menus scripts open for character creation, which are not built yet, are shown as
  a placeholder in the same box.

Not built: the icon beside the text, the list's scrolling (ten long buttons under a
long text would run off the screen), the two sounds, the blur of the world behind
(the Wait menu note has the same gap), the player's own font and colour settings.

## 5. Looking at one

`NVF_PLAY_TEST_SCRIPT=<seconds>:message:<MESG Editor ID or FormID>` shows a message as
`ShowMessage` would, outside the script that shows it (no script reads its button).
With `NVF_PLAY_TEST_SCRIPT_MENUS=off` it stays until answered.

Checked this way on 2026-10-11: the "Vance's Gun" message drawn in a 1680 x 1050 window
has the screenshot's line breaks, and its box, title, top line, text lines, button frame
and bottom line are each within 2 pixels of the screenshot's when both are measured from
the box's top left corner [observed].
