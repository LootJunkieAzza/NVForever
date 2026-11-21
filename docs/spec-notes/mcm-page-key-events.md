# Key events while the Mods page holds the world

## Contract

- [documented: GECK wiki, `SetOnKeyDownEventHandler` and
  `SetOnKeyUpEventHandler`, also `jip-events.md` §2] The registered function receives
  the physical key's scan code when that key is pressed or released. Disabling a key
  does not prevent its event.
- [documented: `ext-input.md` §1] Wheel directions use codes 264 and 265.
- [observed: installed MCM Extender `OnRestart.gek` and `OnKeyDown.gek`, local demand
  inventory, 2026-11-16] MCM Extender registers wheel and keyboard handlers. Its wheel
  handler reads the open start menu and chooses the list under the cursor; its search
  input also arrives through key events. The files remain private.
- [design: `ui-tile-tree.md` §7; `mcm.md` §2.2] The Mods page stands in for menu 1013
  while the world is held. Its event handlers must see that menu context, and an
  existing message box must remain open.

## Engine boundary

[design] Each Mods-page frame advances the existing JIP physical-key edge tracker
before its menu script blocks. The engine uses the same dispatcher and held-key
state as an ordinary frame. A held key produces one down event, its release one up
event, and returning to play must not replay a page's already-delivered edge. This
does not run world updates, advance game time, or dispatch unrelated queued events.
The surrounding page menu context is restored after the frame.

[documented: `jip-ui-9.md` §1, `GetCursorPos` / `SetCursorPos`] XML cursor queries
use the tile tree's pointer in a menu, and cursor writes are admitted in menu mode
or while the HUD cursor is enabled. [design] The held StartMenu page has the same
cursor admission. Its XML coordinates use the already transformed, hit-tested
pointer; raw window pixels still serve non-XML queries. Plain-play behavior and
ordinary menu behavior keep their existing paths. [observed: installed MCM
Extender `OnKeyDown.gek`, local demand inventory, 2026-11-16] Its wheel callback
selects the list by querying XML cursor coordinates and the list's bounds.

[observed: `menu-xml.md` §10.1, original-game pause-menu capture] The active menu
getter returns 1013 while `MenuMode 1013` holds there.
[observed: installed MCM Extender `InputHandler.gek`, `MenuSearchInput.gek`
and `OnButtonDown.gek`, local demand inventory, 2026-11-16] These input handlers
require active menu 1013. [design] When no ordinary menu overlays the held StartMenu
page, that getter returns StartMenu. An existing message menu keeps precedence;
the page is a context for its blocks, not a replacement for the overlay.

[hypothesis] Exact ordering against the original game's menu scripts still needs a
capture. Dispatch before menu blocks preserves NVForever's ordinary-frame ordering.

## StartMenu cursor-warp consumer

[design: `ui-tile-tree.md` §7; `interface::menu_frame` / `from_menu_frame`]
The tile input's menu coordinates invert the drawing's frame at its menu scale.
A StartMenu XML cursor warp therefore applies the drawing's existing inverse
(`to_menu_frame`) after converting 960-high interface units to window pixels.
The setter retains the StartMenu identity with the pending request because the
page restores its temporary script context before the engine consumes that request.
Raw pixel requests and HUD-cursor requests retain their existing coordinate policy.

[design] A focused authored control runs the real window warp consumer, feeds the
result through the same coordinate conversion and tile-input step used by StartMenu,
and routes a subsequent physical key callback to the MCM list. Cover 50%, 75% and
100% thread-local scale scopes; no global setting mutation or separate test binary.
This controls a Bevy window resource and production conversion/consumer functions;
OS cursor interaction, GPU drawing and original installed-mod execution remain open.

## Validation and limits

[design] Authored regressions must register handlers through the source compiler,
feed physical wheel/keyboard input through the engine input publisher, and call the
same Mods-page tick as the pause page. Check a list trait written by the handler,
menu context, held/released keys, disabled keys, repeated entry, and frozen game
time. A separate source-registered wheel callback must route a list update using
distinct raw and hit-tested cursor positions and request a cursor warp in that
same page context. A getter-only test cannot establish this boundary.

[hypothesis] Full installed MCM 1.5.1 + MCM Extender + Vanilla UI Plus acceptance
remains open until their original initialization, menu registration, list scrolling,
option changes and save/load run on the selected installed profile. The authored
tests do not establish native parity or complete compatibility for those packages.

## Executed consumer checks

[validation: Windows CPU libtest, integrated source b8da4c3d41e9a950a6e43bdb4d95b0e3c60d0a1e]
The selected consumer controls below passed with zero failures and zero ignored.
This combined source contains the focused MCM/control/light changes and fresh
neighboring inventory work. It is not an isolated standalone-head run. Both
libtest targets compiled; all selected jobs ended and the build source was restored.
No pristine baseline failure, playable client, physical OS/device acquisition,
GPU/rendered, native or original installed-mod acceptance is established by this run.

`script_input::mcm_page_tests`:4 passed. Adjacent `script_input::wheel_tests`:4
passed; `tiles::mods::tests`:2 passed. The authored MCM controls exercise source
callbacks, list routing, held/released/disabled state, page/menu restoration and
the actual window/menu-transform consumer, within the limits above.
