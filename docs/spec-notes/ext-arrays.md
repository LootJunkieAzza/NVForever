# Spec note: extender array variables

Covers `array_var` locals (arrays, maps, string maps), the `ar_*` functions and `foreach`.

Skeleton for task G4 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md)); every behaviour below is **open** until that task fills it.

Wiki pages (topic, not one function): `NVSE Expressions`, `ForEach`.

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

From `schemas/script/functions.json` (origin NVSE or FOSE1): 39.

| Function | Origin | Wiki page | Behaviour |
|---|---|---|---|
| `ar_All` | NVSE | `Ar All` | open |
| `ar_Any` | NVSE | `Ar Any` | open |
| `Ar_Append` | NVSE | `Ar Append` | open |
| `Ar_BadNumericIndex` | NVSE | `Ar BadNumericIndex` | open |
| `Ar_BadStringIndex` | NVSE | `Ar BadStringIndex` | open |
| `Ar_Construct` | NVSE | `Ar Construct` | open |
| `Ar_Copy` | NVSE | `Ar Copy` | open |
| `Ar_CustomSort` | NVSE | `Ar CustomSort` | open |
| `Ar_DeepCopy` | NVSE | `Ar DeepCopy` | open |
| `Ar_DeepEquals` | NVSE | `Ar DeepEquals` | open |
| `Ar_Dump` | NVSE | `Ar Dump` | open |
| `Ar_DumpF` | NVSE | `Ar DumpF` | open |
| `Ar_DumpID` | NVSE | `Ar DumpID` | open |
| `Ar_Erase` | NVSE | `Ar Erase` | open |
| `Ar_Filter` | NVSE | `Ar Filter` | open |
| `Ar_Find` | NVSE | `Ar Find` | open |
| `Ar_FindWhere` | NVSE | `Ar FindWhere` | open |
| `Ar_First` | NVSE | `Ar First` | open |
| `Ar_ForEach` | NVSE | `Ar ForEach` | open |
| `Ar_Generate` | NVSE | `Ar Generate` | open |
| `Ar_HasKey` | NVSE | `Ar HasKey` | open |
| `ar_Init` | NVSE | `Ar Init` | open |
| `Ar_Insert` | NVSE | `Ar Insert` | open |
| `Ar_InsertRange` | NVSE | `Ar InsertRange` | open |
| `Ar_Keys` | NVSE | `Ar Keys` | open |
| `Ar_Last` | NVSE | `Ar Last` | open |
| `Ar_List` | NVSE | `Ar List` | open |
| `Ar_Map` | NVSE | `Ar Map` | open |
| `Ar_MapTo` | NVSE | `Ar MapTo` | open |
| `Ar_Next` | NVSE | `Ar Next` | open |
| `Ar_Null` | NVSE | `Ar Null` | open |
| `Ar_Packed` | NVSE | `Ar Packed` | open |
| `Ar_Prev` | NVSE | `Ar Prev` | open |
| `Ar_Range` | NVSE | `Ar Range` | open |
| `Ar_Resize` | NVSE | `Ar Resize` | open |
| `Ar_Size` | NVSE | `Ar Size` | open |
| `Ar_Sort` | NVSE | `Ar Sort` | open |
| `Ar_SortAlpha` | NVSE | `Ar SortAlpha` | open |
| `Ar_Unique` | NVSE | `Ar Unique` | open |
