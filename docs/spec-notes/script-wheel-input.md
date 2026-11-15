# Physical wheel input for scripts

[documented: GECK Wiki DirectX Scancodes,
https://geckwiki.com/index.php?title=DirectX_Scancodes, and IsKeyPressed,
https://geckwiki.com/index.php/IsKeyPressed, read 2026-11-14] Codes 264 and 265
mean mouse wheel up and down; physical mouse buttons occupy 256 through 263.
IsKeyPressed reads a pressed scancode. These pages do not settle the Bevy
message-to-frame or per-notch callback policy proposed here.

[documented: NVForever public source
576d39630287845c598914d17ddffd71eebbef28,
engine/nvf-play/src/script_input.rs] The bridge rebuilds script-held keyboard
and mouse-button state after physical device input, but never reads MouseWheel.
The existing script getter/disabled policy and Game::tick JIP key-edge dispatch
are in crates/fnv-game/src/script_input.rs and jip_events.rs.

[observed: private installed script metadata inspection, no script text copied]
A UI consumer polls 264/265 to perform its own pane-sensitive scrolling. This
identifies a missing physical-input producer; it does not establish actual MCM
navigation, left-list selection, pane focus, Up/Down behavior or native parity.

[hypothesis: engineering input policy] Drain MouseWheel messages after physical
device input. Only finite vertical deltas for the primary window publish their
corresponding code in ScriptInput.held for that frame. Line and pixel events use
direction, without a guessed pixel-to-notch conversion. Same-direction events
coalesce as a boolean, opposite directions can both appear, and consecutive
frames with the same direction retain the existing held/edge semantics. An empty
frame clears only the wheel directions. Horizontal, zero, nonfinite and
secondary-window deltas are ignored. Drain without a runtime or primary window
so startup input cannot replay after admission.

[hypothesis: scope] Existing IsKeyPressed disable/ignore semantics and real
Game::tick key edges remain consumers. This adds no MCM selection model, scroll
offset override, pointer movement, gameplay consumption, wheel remapping or
plugin-version response. Tap/Hold wheel synthesis and native per-notch parity
remain outside this change.

[observed: qualified authored Windows CPU run, result SHA256 prefix 66880e2e]
The exact R2 bridge and four controls were compiled in a separate historical
normal26e+sixUI host. All four selected cases passed, each with no failure or
ignored case and 1724 other tests filtered. The controls use synthetic Bevy
messages, the published input API and real Game::tick edge dispatch, not OS
input, a real window or GPU. The R2 fixture-only correction supplies the required
TouchPhase::Moved field; it leaves runtime and all expectations unchanged.
No pristine-main baseline comparison was executed or inferred.

[observed: current public-port review state] The current-main port's Rust build,
strict Clippy, canonical CI and opt-in manual wheel smoke are unrun. Its Rust
bridge and four control bodies are exact R2, with this separately qualified note.
No installed MCM behavior, physical device/window delivery or whole-mod fix has
been accepted by these CPU results.
