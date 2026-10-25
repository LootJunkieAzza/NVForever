# Script function catalogue

- `functions.json` (`fnv.script.functions/1`), **generated** by
  `tools/capture/script_functions.py`; do not edit by hand. Every script function known to
  the GECK wiki (base game and script extenders) with parameters, calling reference,
  return type, opcode (observed in shipped bytecode, or from xEdit's condition index) and
  usage counts from the shipped masters. Core xNVSE functions (strings, arrays, user
  functions, events, value types) carry `core_family`, naming the
  `docs/spec-notes/ext-<family>.md` note that covers them.
- `notes.json` (`fnv.script.notes/1`), **hand-maintained**: our own paraphrased summary,
  behaviour notes and original-engine bugs for each base-game function, keyed by wiki
  page. Edit this file, then regenerate.

Conventions, sources, tags and the bytecode observations are in
[`docs/spec-notes/script-functions.md`](../../docs/spec-notes/script-functions.md); the
readable base-game table is
[`docs/spec-notes/script-functions-catalogue.md`](../../docs/spec-notes/script-functions-catalogue.md).

Rules: no wiki prose is copied (paraphrase, link the page); no opcode is guessed (only
`observed` or `xedit_index` evidence); no game content is written (names, opcodes and
counts only).
