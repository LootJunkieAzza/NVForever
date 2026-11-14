# Terminal and companion touch adapters

[design] These input and accessibility choices are NVForever additions. They do not
change puzzle attempts, password/bracket outcomes, companion eligibility or scripts.

[documented: engine/nvf-play/src/terminal.rs and crates/fnv-game/src/hacking.rs]
TerminalInput::Point moves selection without choosing; Select reaches Puzzle::choose
inside the existing terminal apply system. Terminal touch defaults to selecting a
whole word/bracket span and a separate explicit Guess button. A selected-token
magnifier, bounded text zoom and vertical row panning make the same live dump readable;
a cursor-pad alternative still emits the ordinary Step commands. Optional immediate
tap entry is explicit and occurs only on a valid un-dragged release. Opening, hovering,
panning, settings, cancellation and contact loss never emit Select.

[design] Android uses a flat terminal backdrop. Hacking has its own live, readable
character grid and token preview; other terminal pages retain their flat canvas and
existing rules. Keyboard/controller commands continue through existing terminal keys.
Raw terminal touches are owned by the adapter while its screen is open; the parent HUD
bridge must suppress and release the generic menu overlay using TerminalScreen.0.

[documented: engine/nvf-play/src/companion_wheel.rs and
crates/fnv-game/src/companion_wheel.rs] WheelInput::Use reaches Game::wheel_command in
the existing apply system. Direct sector taps and an optional drag/release scheme
share its eligibility and outcomes. The hub cancels, disabled selections show the
actual reason derived from the same game state, and one frame can execute at most one
command even if more than one input producer submits. Talk/inventory transitions keep
their existing routes. The parent HUD captures generic touches using WheelScreen.0.

[design] Both adapters ignore the opening-frame gesture, require fresh contacts after
focus/suspend/session changes, cancel multi-touch ambiguity and never activate on
hover or cancellation. A touch while saved input is Controller only restores device
selection through the parent's controller bridge; the next touch acts. Options are
persisted in separate owned JSON files under NVForever's engine home, with desktop
defaults disabled. These are source-only changes pending compilation, execution and
rendered physical-device validation. No shipped game data is included in fixtures.

## Review revision 2

[design] Terminal row targets are at least 48 logical pixels tall; the action toolbar
wraps into additional rows rather than shrinking below 48 by 48. Characters keep
24–32 logical-pixel columns. Narrow or zoomed views show one dump column at a time
with an explicit 48-pixel column selector, and oversized columns pan horizontally.
Usable word/bracket spans share the rectangles used by drawing and input; partial
spans at scroll edges are padded to at least 48 by 48 inside the visible column.
Punctuation remains available through cursor navigation, and direct junk taps only
start panning. The separate Guess action remains the default. These dimensions are
source geometry assertions, not a substitute for rendered phone acceptance.

[design] Both owned option files use a version-1 envelope with exactly `version` and
`settings`, read with a 4 KiB bound. Missing/unknown versions, unrecognized fields,
malformed values or oversized files disable writing for that loaded document, so
later option interactions cannot overwrite the working file with defaults. No
unversioned document is silently migrated. Known-v1 values use bounded defaults and
normalization. Rejected-file status is shown when a user tries to save options.
The original source-only revision is preserved separately; its unversioned files
were never claimed as a shipped format.
