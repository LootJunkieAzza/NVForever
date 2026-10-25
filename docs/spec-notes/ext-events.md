# Spec note: extender event handlers

Covers registering a function script for a game event, removing it, dispatching events
of one's own, and the handler priority queries.

Filled for task G4 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md))
from the GECK wiki export, as input to E1–E3. Everything the pages state is tagged
`[documented: <page>]`; what they leave out is **open (to observe)**. Not in the
export: GetCurrentEventName and DispatchEventAlt. (User-Defined Event, Event Handler
Priority System and New xNVSE Event Filtering System arrived on 2026-10-25, `NVSE.xml`.)

Behaviour is our own wording of the GECK wiki export (`C:\fnv-private\geckwiki\`,
private); no extender source, no opcode numbers or argument encodings (those come only
from a cited document or from compiling our own test scripts with xNVSE,
[extender-compat.md](../design/extender-compat.md) §4.1).

## 1. The model

An event handler is a user-defined function (or a lambda) registered for an event by
name; when the event happens, every matching handler is called with the event's
arguments, without any script being attached to the objects involved (NVSE 4.6, ported
from OBSE) [documented: NVSE Event Handling; SetEventHandler]. The handler's parameter
list must match the event's arguments in order; the names are free [documented:
SetEventHandler]. After a handler runs, its variables are cleared as for any
user-defined function [documented: SetEventHandler].

Handlers managed by the extender are the ones set with SetEventHandler and
SetEventHandlerAlt; plugin-specific ones (JIP's "LN" events such as OnCellEnter, and
the older `SetOn…EventHandler` functions) are separate and not covered here
[documented: GetEventHandlers; SetEventHandlerAlt].

## 2. The built-in events

[documented: NVSE Event Handling]. "First" and "second" are the arguments passed, in
order (and the targets of the `"first"`/`"second"` filters, §3).

### 2.1 Game events

| Event | First | Second | Notes |
|---|---|---|---|
| OnActivate | the activated reference | the activating reference | **Does not block activation**, unlike an OnActivate block; runs **before** the object's OnActivate block, and even when activation will be refused (a locked or inaccessible door) |
| OnActorEquip (alias OnEquip) | the actor | the item (base form) | runs before the item counts as equipped, just after whatever held the same slots is unequipped |
| OnActorUnequip (alias OnUnequip) | the actor | the item (base form) | not when equipped ammo is dropped |
| OnAdd | the item (world reference) | the container | only when picking up from the world |
| OnClose, OnOpen | the reference | the same reference | |
| OnCombatEnd | target (actor) | ender (actor) | |
| OnDeath | killed (actor) | killer (actor) | |
| OnDrop | dropper | the dropped world reference | not when moving items between containers in the inventory menu, unlike the OnDrop block |
| OnHit | target (any reference) | attacker: an actor, or the projectile when the target is not an actor and the weapon is not melee/unarmed | runs **before damage**, unlike the OnHit block |
| OnHitWith | target | weapon (base form) | runs before damage |
| OnLoad | the reference | the same | |
| OnMagicEffectHit | target | base effect | |
| OnMurder | murdered (actor) | murderer (actor) | |
| OnPackageStart, OnPackageChange, OnPackageDone | actor | package | |
| OnReset | the reset reference | the same | |
| OnSell | the sold item (base form) | seller (player or vendor) | never fired before xNVSE 6.2.8 (a bug) |
| OnStartCombat | target | starter | not fired again when a save is loaded mid-combat |
| OnTrigger, OnTriggerEnter, OnTriggerLeave | trigger | action reference | |
| SayToDone | speaker | info | |
| OnApplyIMOD, OnRemoveIMOD | — | the imagespace modifier | |
| OnLockBroken, OnLockpickSuccess, OnLockpickBroken | — | the object | |
| OnTerminalHacked, OnTerminalHackFailed | — | the terminal | |
| OnRepair | — | the repaired inventory item; then the repairer and the repair kind (0 normal, 1 vendor) | four arguments |
| OnDisable, OnEnable | — | the reference | |

"—" means the table gives no first argument; how many arguments those handlers take:
open.

### 2.2 Session events (the plugin API's messages)

| Event | Arguments | Notes |
|---|---|---|
| LoadGame | the save file name | not for entering the game from the main menu with CenterOnCell; **runs an extra time** for the missing-plugins warning; runs once the save is mostly loaded (the player's cell is already the new one) |
| PostLoadGame | success flag | not for a new game, not for CenterOnCell from the main menu |
| SaveGame | the save file name | |
| NewGame | — | not for CenterOnCell from the main menu |
| ExitGame, ExitToMainMenu, QQQ | — | |
| RuntimeScriptError | the error message, an int | |
| DeleteGame, RenameGame, RenameNewGame | full file name, its length | |
| DeleteGameName, RenameGameName, RenameNewGameName | relative file name, its length | |

The SetEventHandler page's LoadGame example takes the name and its length as two
parameters [documented: SetEventHandler], while the table lists only the name; the
argument count: open.

### 2.3 User-defined events

Any name may be dispatched with DispatchEvent and listened for with SetEventHandler
(§5) [documented: DispatchEvent; SetEventHandlerAlt]. Names should carry the mod's name
to avoid clashes. Their arguments have no declared types, so SetEventHandlerAlt cannot
type-check filters on them [documented: User-Defined Event].

## 3. SetEventHandler

- **Syntax**: `SetEventHandler "Event" handler ["first"::x] ["second"::y]
  ["priority"::n]`; the pairs in any order [documented: SetEventHandler].
- **Result**: bool, success.
- **Filters**: `"first"::value` filters the first argument (usually what the event
  happens to), `"second"::value` the second (usually its cause). A form list as the value
  matches any of its forms, sublists included; the extender does not guard against two
  lists containing each other. **An array as a filter does not work: the filter is
  ignored.** The player is given as `playerref`. Whether an argument is a reference or a
  base form matters (the filter must be the same kind) [documented: SetEventHandler].
  Filtering in the registration is cheaper than inside the handler.
- **Priority** (xNVSE 6.2.9): default 1; higher runs first; equal priorities run in
  registration order; any number but **0**, which is reserved as "no priority" and
  refused here; negative numbers allowed, from **-9999 (min) to 9999 (max)**; not
  available for plugin (LN) events or the older `SetOn…EventHandler` functions
  [documented: SetEventHandler; Event Handler Priority System].
- **Lambdas** work as handlers, block or one-line [documented: SetEventHandler; Lambda].
- **Session only**: handlers are not saved; they last for the session, so scripts
  register them again when a session starts (GetGameRestarted) [documented:
  SetEventHandler].
- Registering the same handler twice with the same filters: open (for
  SetEventHandlerAlt, duplicates run twice, §4).

## 4. SetEventHandlerAlt (xNVSE 6.2.8)

- **Syntax**: `SetEventHandlerAlt "Event" handler [i::filter …] ["priority"::n]`
  [documented: SetEventHandlerAlt].
- **Filters**: up to 16, each `index::value`: index 0 filters the calling reference
  (what GetSelf returns), 1–15 the event's arguments in order. A value may be a number,
  form, string or array; an array means "any of these" (`1::(Ar_List Player, SunnyREF)`).
- **Cleared on every load**, unlike SetEventHandler's session lifetime: "all" handlers
  set with this function are flushed when a game is loaded [documented:
  SetEventHandlerAlt].
- **Not for plugin (LN) events.** Several registrations of the same function are
  allowed and each runs. Priority as §3.
- **How filters match** (xNVSE 6.2.8) [documented: New xNVSE Event Filtering System]:
  - a number must equal the dispatched number; for a user-defined event a numeric filter
    is a float; a plugin's event may declare an int filter, which floors the value;
  - a form matches the same form (two null forms match); a dispatched reference also
    matches a filter that is its base form; a form list matches any of its forms; a base
    form filter refuses a reference with an error;
  - a string matches without regard to case;
  - an array matches an array of the same kind and size with equal elements (nested
    arrays not supported);
  - an **array of filters** (list, map or string map; keys ignored) matches when any
    element matches. It holds a reference to the array, so changing the array later
    changes the filter.
- **Why flushed on load**: array filters point at savebaked arrays, which a load could
  replace [documented: New xNVSE Event Filtering System].

## 5. RemoveEventHandler and DispatchEvent

**RemoveEventHandler** `RemoveEventHandler "Event" handler [filter1] [filter2]
["priority"::n]`: removes matching handlers; returns 1 when at least one was removed.
With no filters, every handler of that function for that event goes; with one, those
matching it. The default priority is 0, meaning any priority [documented:
RemoveEventHandler]. It also takes SetEventHandlerAlt's numeric indexes [documented:
SetEventHandlerAlt].

**DispatchEvent** `DispatchEvent "Name" [infoMap] [senderName]`: sends a user-defined
event to every handler registered for that name. A handler receives one string map: by
default `"eventName"` (the name) and `"eventSender"` (the dispatching mod's file name);
`infoMap` replaces the whole map, and `senderName` replaces the sender string
[documented: DispatchEvent]. Whether a replaced map still gets `eventName`: open (the
page's example reads both from a custom map, which suggests both are added
[hypothesis]). Whether handlers run at once, inside the dispatching line: open.

## 6. Inspecting handlers (xNVSE 6.2.9)

- **GetEventHandlers** `[eventName, handler, priority]`: with no event name, a string map
  from event names to handler lists; with one, the list for that event. A list sorted by
  priority is a map from priority to handler arrays; with a priority filter, that level
  is skipped. Each handler entry is an array: [0] the script (or, for a plugin handler,
  a string map with "Plugin" and "Handler", possibly empty), [1] a string map of its
  filters [documented: GetEventHandlers].
- **DumpEventHandlers** (same filters): prints them by event and priority, plugin
  handlers included [documented: DumpEventHandlers].
- **GetHigherPriorityEventHandlers / GetLowerPriorityEventHandlers** `event, handler,
  startPriority[, scriptsToIgnore, pluginsToIgnore, pluginHandlersToIgnore]`: a map of
  the handlers at higher-or-equal (lower-or-equal) priority than that handler; Ar_Null
  when the handler is not at that priority [documented: each page].
- **IsEventHandlerFirst / IsEventHandlerLast** (same arguments): 1 or 0; 0 when the
  handler is not at that priority. The pages define First as "nothing at lower or equal
  priority" and Last as "nothing at higher or equal", which reads reversed against
  "higher runs first" and would always count the handler itself [documented: each page;
  the inconsistency is ours to flag]. Real behaviour: open (to observe).

## 7. Order and persistence, collected

- **Before the block**: OnActivate, OnHit and OnHitWith handlers run before the matching
  Begin block or before damage [documented: NVSE Event Handling]. For the other events,
  handler-vs-block order: open.
- **Activation is not blocked** by a handler [documented: NVSE Event Handling].
- **Saves**: SetEventHandler registrations last for the session and are never saved;
  SetEventHandlerAlt ones are dropped on every load [documented: SetEventHandler;
  SetEventHandlerAlt]. Scripts re-register in a GetGameRestarted or GetGameLoaded test
  [documented: SetEventHandler; DispatchEvent examples]. NVForever must copy both
  lifetimes (ext-save.md).
- **Too early**: when an event fires just before the change it reports can be seen, the
  page suggests waiting a few frames with a main-loop callback [documented: NVSE Event
  Handling].

## 8. Open (to observe), collected

Argument counts for the events with no first argument and for LoadGame; handler-vs-block
order for most events; duplicate SetEventHandler registrations; filter matching under
SetEventHandlerAlt; DispatchEvent's map when replaced, and whether handlers run inline;
IsEventHandlerFirst/Last's real meaning.
