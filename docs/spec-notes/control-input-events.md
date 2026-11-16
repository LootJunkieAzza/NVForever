# Control transitions through both event interfaces

## Contract

[documented: GECK wiki, [JIP down](https://geckwiki.com/index.php?title=SetOnControlDownEventHandler),
[JIP up](https://geckwiki.com/index.php?title=SetOnControlUpEventHandler)]
`SetOnControlDownEventHandler` and `SetOnControlUpEventHandler` register a callable
for a control ID. The callable receives that ID when the control is pressed or
released. Registration, argument matching and handler context keep the existing
[JIP event contract](jip-events.md).

[design: documented ID matching] Control zero is Forward, not a wildcard. Use
the existing exact-argument filter policy for control down/up; keep other event
families' established wildcard policies unchanged. An unrelated Backward press
must not invoke a Forward registration.

[documented: GECK wiki, [OnControlDown](https://geckwiki.com/index.php?title=OnControlDown),
[OnControlUp](https://geckwiki.com/index.php?title=OnControlUp)]
The alternate interface uses `SetEventHandler` with `OnControlDown:<ID>` or
`OnControlUp:<ID>` and passes one numeric control ID. Down explicitly includes a
control disabled with `DisableControl`. Keep the existing named-event dispatcher,
filters, context restoration and error handling.

[observed: installed Enhanced Movement 1.43 source inventory, 2026-11-16]
Original initialization registers both interfaces for movement, attack, crouch,
auto-move and jump controls. Its handlers test menu state and configured features;
the press/release events are inputs to movement/animation state. Installed files
remain private, and are not authored test fixtures.

## Engine mapping

[design] Publish one bounded 28-control bit mask from the existing selected input
device, current bindings, mouse buttons and controller button/stick readers. Reuse
the existing switching-input filter and controller thresholds. A disabled control
still appears in the event snapshot; gameplay blocking is a separate existing
consumer. Multiple inputs for one control aggregate into one held state, so one
release cannot end a control still held through its other binding.

[design] Compare snapshots once at the existing physical-key event entry point.
Advance history even without subscribers. Dispatch each changed control once to
each interface, retaining each dispatcher's own registration/filter semantics.
Keep physical key callbacks in their existing place before these new callbacks.
Engine-held pages that use this entry point inherit the same edge history. A
headless runtime derives the mask from its existing key/mouse binding snapshot.

[hypothesis] Relative native ordering between the two interfaces, input rebinding
while held, device changes while held, menu/pause delivery and script-injected
input require original-game captures. NVForever follows its published frame
snapshot for these transitions; this is an explicit engine mapping, not a native
timing claim. No raw native argument encoding is inferred.

## Acceptance boundary

[design] Authored controls must use source registration and actual engine input
publication, including selected controller button/stick state, held/release/rebind,
disabled controls, late registration, unrelated filters, and repeated entry.
A handler must reach a real feature consumer rather than only increment a probe:
exercise running/walking through the actual player control/movement consumer.
Native observation and original Enhanced Movement initialization, AP, animation,
prone/slide/mantle, companion speed, MCM and save/load remain separate acceptance.

[design: authored consumer controls] `player::control_event_tests` exercises both interfaces
through original-style independent source handlers. A disabled crouch binding's
press selects walking, its release restores running and unblocks crouch, and the
real character stepping path travels at the configured walk/run speeds. The fixture
uses the existing collision-free movement mode; it does not qualify terrain,
collision, sprint perks or AP. Other controls cover binding aggregation/rebind,
selected controller button/stick state and history before handler registration.

## Executed consumer checks

[validation: Windows CPU libtest, integrated source b8da4c3d41e9a950a6e43bdb4d95b0e3c60d0a1e]
The selected consumer controls below passed with zero failures and zero ignored.
This combined source contains the focused MCM/control/light changes and fresh
neighboring inventory work. It is not an isolated standalone-head run. Both
libtest targets compiled; all selected jobs ended and the build source was restored.
No pristine baseline failure, playable client, physical OS/device acquisition,
GPU/rendered, native or original installed-mod acceptance is established by this run.

`player::control_event_tests`:4 passed, covering source callbacks, actual walk/run
steps in the authored collision-free fixture, selected controller/button/stick
snapshots, binding aggregation, exact filters and edge history. The later controller
query/button/gameplay-blocking successor is separate and remains unrun.
