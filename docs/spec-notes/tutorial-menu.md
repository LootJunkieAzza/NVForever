# The tutorial menu (ShowTutorialMenu)

Plain-language notes for `nvf_play::tutorial_menu` and `fnv_game::Game::show_tutorial`
(2026-10-18, claude/menu-hooks). Sources: the GECK wiki export (owner's, C:\fnv-private\geckwiki),
the game's `menus\tutorial_menu.xml` (owner's extract, C:\fnv-private\menus-extract) and
the masters' scripts decompiled with `fnvtool script decompile`. No game data is kept here.

## 1. What scripts do with it

- `ShowTutorialMenu <message>` "shows the specified Message Box form, similar to
  ShowMessage, but with slightly different visuals. These are the 'VDSG Manual' styled
  message boxes" [documented: GECK wiki ShowTutorialMenu].
- The masters call it twice [observed]: CGTutorial (QUST 0x00059C85) stage 212, entries 0
  and 1, show `HelpHealingLimbs` (MESG 0x00072358) and `HelpHealingLimbsXBox`
  (0x00072359); the entries' conditions pick the platform. Stage 212 is set from the
  quest script's `Begin MenuMode 1` block when a limb of the player is crippled, so the
  menu opens while the Pip-Boy is open.
- The messages are message boxes (DNAM Message Box) with a title (FULL "Healing Limbs")
  and a text (DESC) marked up with `<div>` at the start and `<p>` between paragraphs
  [observed].

## 2. The menu file [documented: menus\tutorial_menu.xml]

Lengths in the interface's units, from the background's top left.

- A black background (`solid_black.dds`) 0.8 of the screen wide and high, centred, at
  the menus' fill alpha (`_background_fill_alpha`, 32 of 255).
- Top: at x 48, y 96 a line fading down (vertical_fade_line.xml, 60 long) and a line 50
  long to the right; the title (font 6, "replaced in code") 10 after that line, 5 lower
  than centred on it; a line on to the page text "??/??" (font 6, right-justified, 10
  before the right stub; "replaced in code"); when the page text is hidden a line fills
  its place; a stub 50 long ending 48 before the right side, and a line fading down there.
- Text: a clipping column from the title's x, under the top fade lines (y 156), to the
  lower fade lines, ending 64 before the Previous button; the text 5 in and 21 down (the
  file's "font adjustment"), wrapping 5 short of the column, moved up by the scroll bar's
  value. A vertical scroll bar beside the column at the left corner line; a step of 30.
- Buttons (text_box.xml, right-justified, right side 81 before the background's right):
  Next (`sNext`) at the column's top, Previous (`sPrevious`) under it, Close
  (`sCloseButton`) standing on the lower right fade line's top. PC key hints
  `sPCMenuHintA`, `_S`, `_E` after them; the PC keys `_PCButton_A` Next, `_S` Previous,
  `_E` Close.
- Bottom: the corners' lines fading up from 96 above the bottom, a stub 50 long, the
  VDSG symbol (`Interface\Loading\glow_loading_VDSGSymbol.dds`, 64 high, its width less
  36 of dead space), the words `sVDSGManual`, a line on to the right corner.

The strings are game settings that are not in the masters (the executable's defaults).

## 3. What NVForever does

- `Game::show_tutorial`: the message is a page of the tutorial menu, which opens over the
  menu that is open and remembers it (`Game::menu_under`); closing the tutorial makes that
  menu the open one again; a message already shown is not added twice; further messages
  while it is open are more pages [hypothesis]. A message box shown meanwhile waits.
  If the engine closes the menu underneath (the Pip-Boy put away), it is forgotten.
- MenuMode 1059 while it is open [hypothesis: the code as remembered from the GECK wiki's
  menu list, whose template the owner's export lacks]. `MenuMode 1` stays true over the
  Pip-Boy.
- Drawn as §2 says, with these choices: the words are "Next", "Previous", "Close", "A)",
  "S)", "E)" and "VDSG Manual" [hypothesis]; the background as dark as the other menus
  (0.75) instead of the file's alpha 32 [design: readable over the Pip-Boy]; the page
  count only with more than one page; a thin scroll mark instead of the scroll-bar
  prefab; the text in font 1 [hypothesis: a text tile without a font]; `<p>` and `<br>`
  start paragraphs and other tags go.
- Keys: A or Right next, S or Left previous, E, Esc or Enter close, Up, Down and the wheel
  scroll; clicks on the buttons. The Pip-Boy's own keys are held while the menu is over
  it.

## 4. Open

- The code 1059, the words and whether a second call adds a page or replaces it:
  capture job `tools/capture/probes/menu_hooks.toml` (the menu codes while it is open).
