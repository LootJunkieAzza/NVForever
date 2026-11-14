# Touch dialogue accessibility

[design: owner request, Android follow-up to #2436] Dialogue needs wide wrapped
choices, larger touch targets, independent choice/subtitle sizes, scrolling, and a
choice between direct taps and selecting before confirmation. This increment owns
the conversation view, not game dialogue rules or global HUD scaling.

[documented: `engine/nvf-play/src/dialogue.rs`, `dialogue_menu.rs`,
`tiles/dialog_menu.rs`] The runtime supplies ordered eligible choices. Every view
request carries a `ListId`; the conversation input rejects a request from another
conversation, response or choice list. Keyboard/controller confirmation and game
commands already pass through that input. The new view keeps that route and the
same indices, challenge information and said-before state.

[design] A persisted **Touch dialogue** setting enables a separate full-width Bevy
view (on by default for Android, off elsewhere). The ordinary hand-built and tile
views give way while this setting is on. Turning it off restores the existing view.
Choice and conversation subtitle text scales are separate bounded settings, 75–200%.
This view uses Bevy's UI text rather than the tile view's game-font glyph images;
the offered strings and their status still come from the same conversation model.
The view uses Bevy text with word-or-character wrapping, rows at least 56 logical UI pixels high, and a clipped
vertical viewport. The speaker and the offered text come from the existing model.
Scrolling does not select or answer a choice. Keyboard/controller selection brings
the selected row into view.

[design] **Select then confirm** is the touch default: a released tap selects a row,
then a separate confirmation target answers it. Confirmation starts unarmed and
requires that the selected row still matches the released selection. **Direct tap** answers on release.
Real mouse clicks and keyboard/controller choices keep their existing commands.
The conversation's speaking line can be skipped by a fresh released tap, not by
the finger that entered the conversation or ended the previous response.

[design] One fresh contact owns a gesture. A drag past eight logical window pixels,
a second contact, cancellation, focus loss, suspension, a resize, a changed list or
settings change cancels its activation. A release must land on the same visible
target and the same list. Input is ignored under pause, another game menu, character
creation or controller ownership. Touch controls underneath this view are hidden
and do not synthesize menu keys; raw touch remains available to the dialogue view.
The first touch restoring a controller-selected device cannot answer dialogue.

[documented: local Bevy 0.20.1 `ui_pointer.rs` and `ComputedNode`]
Touch positions are logical window pixels; UI hit boxes are physical pixels. Hit
testing uses the window scale and actual computed node transform/clipping. Scroll
offsets use the node's inverse scale factor, rather than guessing device density.

[design] The settings use the existing Settings/INI flow. No saves, quest state,
game data or control bindings are rewritten. Existing list stamps and runtime
eligibility remain authoritative, including while a new list appears during input.

[design] Qualification must cover actual TouchInput start/move/end/cancel routing,
fast taps, entry and list replacement, drag-away-and-back, scrolling, multi-touch,
focus/suspend/resize cancellation, both tap modes, selection/confirmation indices,
and settings persistence. Layout checks must cover wrapped long choices and
independent text sizes. Rust tests and native/device rendering are pending until
the parent admits compilation and execution; source authorship is not a test pass.
