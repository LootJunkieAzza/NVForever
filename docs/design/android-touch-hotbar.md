# Android touch hotbar and item assignment

[design] This is an opt-in touch presentation of the existing eight hotkey slots.
The second slot remains Ammo Swap. Four, six or eight visible slots, size and bar
position are persisted as touch preferences. Pinned consumables reference hotkey
slot indices, not parallel item IDs: existing save/load-order remapping remains
authoritative. Only carried Aid items can be pinned to the optional utility tray.

[documented] `hotkeys::Hotkeys::assign` controls eligible item kinds, mod flags,
duplicate movement and the reserved slot. `weapon_wheel::read`/`act` controls actual
use, holster waits, sounds and `ItemUsed` events. The touch bar sends a bounded
semantic slot request through that path rather than synthesizing hardware keys or
duplicating equip/consume behavior. Bindings and physical ownership stay unchanged.

[design] A tap activates only on release inside the original slot. A hold of450ms
opens a safe context without using anything; release of that same finger never
chooses a context action. A subsequent tap can pin/unpin an Aid slot, clear a slot,
or reposition the bar. Moving, canceling, resizing, mode changes, input-device
handoff, focus/background and platform covers cancel pending use. Other fingers
cannot inherit a contact. Visible rectangles and touch bounds share one geometry.

[design] Pip-Boy row assignment uses the real rendered item row and existing
`RowKey`/mode identity, preserving item/mod selection. Raw touch coordinates feed
the existing flat crop or posed-model ray-to-screen mapping. A touch-only pointer
is distinct from the hardware mouse/custom legacy pointer. Dragging onto a bar
slot assigns; it never consumes the item. Long-held or dragged row contacts
suppress ordinary release clicks. Assignment is refused after the inventory
session or row identity changes, under a platform cover or during controller
switching. Ordinary desktop mouse/controller behavior has no touch resource.

[design] The optional utility tray is opened by an explicit tray button/request;
it does not compete with the bottom swipe that opens Pip-Boy. The parent can send
the tray request from its inventory gesture owner. Configuration and gesture
replays, plus actual App outcomes for assignment/use/refusal and lifecycle gates,
are required before device qualification. No game-derived assets are added.
