# Spec note: extender values and expressions

Covers the value types an extender expression can hold (number, form, string, array), the `let`/`:=` assignment and the compound operators, `eval`, and the type queries.

Skeleton for task G4 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md)); every behaviour below is **open** until that task fills it.

Wiki pages (topic, not one function): `NVSE Expressions`, `Let`, `Let Macro`, `Inline Expression`.

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

From `schemas/script/functions.json` (origin NVSE or FOSE1): 3.

| Function | Origin | Wiki page | Behaviour |
|---|---|---|---|
| `IsFormValid` | FOSE1 | `IsFormValid` | open |
| `IsReference` | FOSE1 | `IsReference` | open |
| `TypeOf` | NVSE | `TypeOf` | open |
