# Touch practice (#2436)

[design] Settings > Controls > Touch offers an optional practice screen. This is a
first-use explanation, not an automatic interruption: read the left-thumb movement and
right-thumb look instructions on separate short pages before pressing Begin. Reset
returns to that guide. Close and Escape return to Settings. No Seen bit or active
session is persisted.

[design] The screen opens only over the paused Settings page, without changing the play
state. It owns FormOver while open and through its closing drain, following the existing
hand-report form contract. The settings choice is an action (not saved); loading any
stored row text cannot open it. It refuses an existing form.

[design] Practice consumes its own raw TouchInput route before live touch contacts,
mouse-look synthesis, aim latches or drawer commands. It only changes local lesson
state: joystick direction/dead-zone display, a pretend look marker and Fire/Aim/Jump
acknowledgements. The per-axis dead zone, floating/fixed explanation, Aim hold/toggle
mode and optional drawer distance are copied from current touch settings on opening.
Its optional drawer exercise only acknowledges a local swipe. It never
presses input buttons or writes gameplay controls, inventory messages, ammo/AP, player
transforms or gameplay saves. Live touch contacts, aim latch, held-sneak and drawer
recognition are cancelled/synced on entry, throughout practice and on closing; hardware
key/mouse ownership remains with the existing merger.

[design] The first opening frame drops the entry gesture. Closing consumes the remaining
raw-touch batch, and two following updates drain trailing input before the form guard is
released. Lost focus, suspension, missing/resized window or leaving the paused Settings
page cancels the practice session. Raw keyboard Escape closes the form while the pause
menu remains guarded. The existing editor-active marker prevents automatic controller
takeover from KeyboardMouse while the form owns input. Real touch activity can still
restore KeyboardMouse from Controller through the normal physical-device selector; it
never creates a live practice action.

[design] Entry is unavailable below 400 x 300 logical screen units, with a
rotation/window-size explanation in Settings. The screen uses original text, geometric
controls, large labelled targets and fixed logical-screen geometry. Text labels use Bevy
Label markers, refreshed when their words change; native screen-reader activation
remains unqualified. The joystick includes a visible dead-zone and knob/direction
feedback; the look area is separate from action targets. The drawer exercise is shown
only when the real optional drawer setting is enabled, but never uses its OpenInventory
bridge. No dependencies or game-derived artwork are added.

[qualification] Rust routing, lifecycle and settings-action tests are authored, not
compiled or executed until admitted separately. Layout/accessibility on a phone, font
fit and merged interaction with other input increments need qualification.

[design, review revision 2] Floating movement admits the left practice panel and
anchors the ring/dead-zone at the owning contact's Started position. That origin
stays fixed until release/cancellation; another movement finger cannot replace it.
Fixed movement retains its demonstration-ring anchor and ring-only
admission. The ring renderer follows the same owned origin used by direction feedback.

[documented: engine/nvforever/src/touch.rs, toggle aim and TouchPhase::Canceled]
The synthetic Aim model mirrors live source timing: toggle at Started once while
no other Aim contact is down, retain through drag and Ended, and clear the toggle
on any Canceled touch. Hold mode stays active while an Aim contact remains. These
changes only update the local guide model; live input synthesis remains blocked.
The corrected Aim route test and added floating/fixed origin test remain uncompiled
and unrun until separately admitted.
