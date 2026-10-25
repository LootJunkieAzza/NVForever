# Spec note: extender event handlers

Covers registering a function script for a game event, removing it, dispatching events of one's own, and the handler priority queries.

Skeleton for task G4 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md)); every behaviour below is **open** until that task fills it.

Wiki pages (topic, not one function): `NVSE Event Handling`.

## Per-entry template

Each entry, once filled, has:

- **Syntax**: the line as a script writes it (with the alias, if any).
- **Arguments**: name, type and whether it may be left out, in order.
- **Result**: what it returns, and its type.
- **Edge cases**: bad or missing arguments, empty containers, an invalid form.
- **Persistence**: whether the effect outlives a save and load (ext-save.md).
- **Evidence**: `[documented: <wiki page>]`, `[observed: <capture>]` or `[hypothesis]`.

Behaviour is our own wording of the GECK wiki export (`C:\fnv-private\geckwiki\`,
private); no extender source, no opcode numbers or argument encodings (those come only
from a cited document or from compiling our own test scripts with xNVSE,
[extender-compat.md](../design/extender-compat.md) §4.1).

## Functions

From `schemas/script/functions.json` (origin NVSE or FOSE1): 10.

| Function | Origin | Wiki page | Behaviour |
|---|---|---|---|
| `DispatchEvent` | NVSE | `DispatchEvent` | open |
| `DumpEventHandlers` | NVSE | `DumpEventHandlers` | open |
| `GetEventHandlers` | NVSE | `GetEventHandlers` | open |
| `GetHigherPriorityEventHandlers` | NVSE | `GetHigherPriorityEventHandlers` | open |
| `GetLowerPriorityEventHandlers` | NVSE | `GetLowerPriorityEventHandlers` | open |
| `IsEventHandlerFirst` | NVSE | `IsEventHandlerFirst` | open |
| `IsEventHandlerLast` | NVSE | `IsEventHandlerLast` | open |
| `RemoveEventHandler` | NVSE | `RemoveEventHandler` | open |
| `SetEventHandler` | NVSE | `SetEventHandler` | open |
| `SetEventHandlerAlt` | NVSE | `SetEventHandlerAlt` | open |
