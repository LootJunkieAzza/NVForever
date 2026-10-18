# Saves from scripts and placing at the crosshair

SystemSave, ForceSave and PlaceAtReticle (docs/outstanding.md §1 rows 2 and 12). The
documentation is the GECK wiki (owner's local export, `C:\fnv-private\geckwiki\functions.xml`).
Built in `crates/fnv-game/src/functions/ui.rs`, `objects.rs` and `senses.rs`, and in the
engine's `engine/nvf-play/src/save.rs` and `activate.rs`.

## 1. SystemSave (0x127B)

- Takes no parameters and returns nothing [documented: GECK wiki SystemSave].
- Makes one special save labelled SYS, of which there is only ever one: each later call
  overwrites it [documented: GECK wiki SystemSave].
- The save shows in the Load menu, not in the Save menu [documented: GECK wiki SystemSave].
- The base game calls it only just before the player can start the battle for Hoover Dam
  [documented: GECK wiki SystemSave, Notes]. The coverage table counts 5 calls in shipped
  scripts [observed: docs/design/script-function-coverage.md].

NVForever: the game layer counts the call (`UiRequests::system_saves`); the engine writes
the slot `systemsave` (`fnv_save::SYSTEM_NAME`, kind `system`) at the end of the frame,
keeping the previous file as `.bak` as the other rolling slots do. The pause menu's Save
button always makes a new save, so the slot is never offered for saving over; the load
page lists it with the kind "system". When saving is not allowed at that moment (a
conversation, a menu, a loading screen), the save waits for the first moment it is, as an
autosave does [hypothesis: the wiki says nothing about a SystemSave called at such a
time; the one base-game use is before a battle the player must not lose].

## 2. ForceSave (0x1217)

- Takes no parameters and returns nothing [documented: GECK wiki ForceSave].
- Makes a new save in a new slot, as if the player had saved by hand [documented: GECK wiki
  ForceSave].
- It has the limits of saving by hand: it fails while a message box shows [documented:
  GECK wiki ForceSave, Notes].

NVForever: the game layer counts the call (`UiRequests::force_saves`); the engine names a
new save as the pause menu's Save button does (`Save <n>`, one past the highest such name
in the saves folder) and queues it as a save by hand. When saving is not allowed at the end
of that frame, it is not made, and the console's log says why, as for a save asked for by
hand [documented in spirit: "fails while a message box shows"; that every other time
saving is refused also drops it is a hypothesis].

## 3. The switch

Both follow the settings page's "Save when a script asks" switch (INI key
`autosave-scripts`, on by default), which also covers Autosave [engineering choice: a
player who turns script saves off does not want these either].

## 4. PlaceAtReticle (0x1263)

- `PlaceAtReticle ObjectID count [minDistance] [maxDistance]`: places the object at the spot
  the crosshair points at [documented: GECK wiki PlaceAtReticle].
- minDistance is marked unused; maxDistance takes 1 to 50000 [documented: GECK wiki
  PlaceAtReticle, its argument list]. The examples call it with and without a subject
  (`player.PlaceAtReticle EuclidTarget 1 0 20000`) [documented: same page].
- The wiki does not say what is returned, what happens when the crosshair hits nothing,
  how the new reference is turned, or what maxDistance defaults to.

NVForever:

- The engine casts the crosshair's ray (the activation ray of `activate.rs`, from about
  the player's eye along the view; in third person the camera's line through the
  crosshair) every frame out to 50000 units and tells the game layer where it starts, its
  direction and the distance to the first hit (`Senses::reticle`); trigger volumes do not
  stop it, as they do not stop activation [hypothesis, as for activation].
- The spot is that first hit. When it is farther than maxDistance (clamped to 1–50000; left
  out, 50000 [hypothesis]), or the ray hits nothing, nothing is placed and the function
  returns 0 [hypothesis].
- The reference is made as PlaceAtMe makes them: an item is one stack of `count`, anything
  else `count` references; the last one made is returned [hypothesis: as PlaceAtMe]. It
  goes into the player's cell, turned to the player's heading (no tilt) [hypothesis].
- The subject the call is made on is ignored; the crosshair is always the player's
  [hypothesis].
- A headless run (no engine telling the ray) places nothing.

## 5. Open questions

- Whether the original's SystemSave waits or fails when called during a conversation or a
  menu; whether ForceSave waits for a conversation to end.
- PlaceAtReticle's default maxDistance, its return value, the new reference's heading, and
  whether it is lifted off the surface it lands on (NVForever puts it exactly at the hit).
