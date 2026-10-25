# Spec note: extender user-defined functions

Covers function scripts (`Begin Function {...}`), calling them, their return value, the calling script and lambdas.

Skeleton for task G4 of the extender plan (docs/handoff/extender-core-tasks.md); every behaviour below is **open** until that task fills it.

Wiki pages (topic, not one function): `Lambda`.

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
docs/design/extender-compat.md §4.1).

## Functions

From `schemas/script/functions.json` (origin NVSE or FOSE1): 3.

| Function | Origin | Wiki page | Behaviour |
|---|---|---|---|
| `Call` | NVSE | `Call` | open |
| `GetCallingScript` | NVSE | `GetCallingScript` | open |
| `SetFunctionValue` | NVSE | `SetFunctionValue` | open |
