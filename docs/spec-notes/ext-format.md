# Spec note: extender format specifiers

Covers the `%`-codes that string-building functions expand (numbers, forms, names, strings). The wiki's format specifiers page is not in the private dump yet: the owner adds it to the next `Special:Export`.

Skeleton for task G4 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md)); every behaviour below is **open** until that task fills it.

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

No catalogue function belongs here; the note covers language or file behaviour. Open.
