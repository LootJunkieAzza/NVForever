# Spec note: extender string variables

Covers `string_var` locals, the `sv_*` functions and the string conversions.

Skeleton for task G4 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md)); every behaviour below is **open** until that task fills it.

Wiki pages (topic, not one function): `NVSE Expressions`.

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

From `schemas/script/functions.json` (origin NVSE or FOSE1): 21.

| Function | Origin | Wiki page | Behaviour |
|---|---|---|---|
| `GetFormIDString` | NVSE | `GetFormIDString` | open |
| `NumToHex` | NVSE | `NumToHex` | open |
| `Sv_Compare` | NVSE | `Sv Compare` | open |
| `Sv_Construct` | NVSE | `Sv Construct` | open |
| `Sv_Count` | NVSE | `Sv Count` | open |
| `Sv_Destruct` | NVSE | `Sv Destruct` | open |
| `Sv_Erase` | NVSE | `Sv Erase` | open |
| `Sv_Find` | NVSE | `Sv Find` | open |
| `Sv_GetChar` | NVSE | `Sv GetChar` | open |
| `Sv_Insert` | NVSE | `Sv Insert` | open |
| `Sv_Length` | NVSE | `Sv Length` | open |
| `sv_Percentify` | NVSE | `Sv Percentify` | open |
| `Sv_Replace` | NVSE | `Sv Replace` | open |
| `Sv_Set` | NVSE | `Sv Set` | open |
| `Sv_Split` | NVSE | `Sv Split` | open |
| `Sv_SubString` | NVSE | `Sv SubString` | open |
| `sv_ToLower` | NVSE | `Sv ToLower` | open |
| `Sv_ToNumeric` | NVSE | `Sv ToNumeric` | open |
| `sv_ToUpper` | NVSE | `Sv ToUpper` | open |
| `ToNumber` | NVSE | `ToNumber` | open |
| `ToString` | NVSE | `ToString` | open |
