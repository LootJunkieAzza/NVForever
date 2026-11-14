# Lockpicking touch and controller adapter

[design, #2436] This increment supplies a dedicated lockpicking input adapter and two
original touch layouts. It does not change lock difficulty, skill checks, sweet spots,
pin health/wear/break rules, force chance, unlock effects or crime. No game artwork or
copied screen words are added; the underlying lock view and existing game-data labels
remain supplied by the existing menu.

## Existing path

[documented: `engine/nvf-play/src/lockpick.rs`, `crates/fnv-game/src/lockpick.rs`]
`LockpickInput::Turn` goes through `Picking::set_pin`; torque goes through `Picking::step`.
The rules refuse moving a pin while the cylinder is turned or torque is held. The adapter
must use those paths, rather than directly rewriting pin position or advancing the puzzle.
The existing Force and Exit commands and their buttons remain in charge of those actions.

[documented: `engine/nvf-play/src/pad/device.rs`, `bindings.rs`]
`ActionPads` and `ControlInput::controller()` respect current device choice and the input
switch filter. The adapter uses these public views, not raw platform or Gamepad state.

## User settings and layout

[design] Android Controls settings offer **Lockpicking touch controls**: Off,
**Arc + turn** (default), **Precision slider + turn**. **Lockpicking control size** offers
75%, 100% (default), 125%, 150%. Choices use the normal NVForever.ini settings routes.
Desktop touch remains Off by default and these Android settings are not registered there;
controller operation uses the same existing selected-controller route on either platform.

[design] Left thumb drags the visible half-circle to choose the angle. The alternative
slider uses a relative, fine horizontal drag, with a visible angle marker; pressing it
alone does not jump the pin. Right thumb holds the separate Turn target to apply torque.
The angle, hold feedback and a short original instruction line are visible while picking.
No new touch shortcut forces a lock. The original Force/Exit and help remain available.
The general Android menu-direction overlay yields to this dedicated layout while it is
active; the placement editor and its gear/help stay outside this input path.

[design] One geometry function computes logical-pixel rectangles for both drawing and hit
testing. A shared factor applies the chosen size and fits narrow/short windows. Nodes
convert those logical coordinates by the current UiScale; Bevy applies display density.
The touch hit target and visible backing use the same rectangle through resize, rotation
and UI-scale changes. The entire gesture is cancelled when geometry/settings change.
Final device comfort and appearance are pending rendered feedback, not locked by source.

## Ownership, transitions and controller

[design] Each angle/turn role has one owning finger ID. Other fingers cannot steal it.
An angle finger can move outside and clamp to the range; a Turn finger leaving its target
cancels the hold until a fresh press. End/cancel releases immediately. Touches already down
when the menu/help closes, or the layout changes, do not become new actions. The first
active frame consumes opening events; a new Started event is required afterwards.
Background/focus loss, missing window, leaving/help/end, gamepad disconnect and input
switching clear all holds. Resuming requires fresh touch starts and neutral controller.

[design] A selected controller uses filtered left-stick X to turn the pin and the standard
right trigger (RightTrigger2) held for torque. Existing West/Force and East/Exit menu routes
remain. It must pass through neutral after opening or interruption before analog/trigger
input takes effect; this prevents an opening hold from straining a pin. Controller movement
bindings no longer double as torque in this menu, so angle input and torque stay separate.
Keyboard/mouse torque and test aids keep their existing routes. Touch and controller torque
are aggregated, with release of one unable to cancel another active source.

## Qualification

[design] Source-authored tests must cover settings routes and INI round trips, ownership,
opening/help transition suppression, precision press-without-jump, arc endpoints, held
turn/cancel/drag-out, scale/DPI/UI sizing, focus/background/disconnect and controller neutral
gating. Action-outcome tests run the adapter through the same Turn/rules paths and verify
angle, torque, wear, break/open outcomes match the unchanged puzzle rules. These tests are
initially UNRUN. No native, Cargo, phone, save or public-service actions are admitted by
this source task. Compilation, clippy and actual touch/controller/device presentation
remain later qualification.

[documented: Bevy 0.20.1 local `bevy_winit/src/state.rs` Touch event branch and
`converters.rs::convert_touch_input`] Winit physical touch locations are converted to
logical positions with the window scale factor before TouchInput is emitted. The adapter
therefore compares TouchInput directly with Window logical-pixel geometry and does not
apply device density again. UiScale affects drawing conversion only.
