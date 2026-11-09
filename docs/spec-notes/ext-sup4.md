# SUP NVSE batch 4: arrays, delayed calls, saves, near cells and the rest (2026-11-09, #1008)

The last 17 SUP functions (the names `handover.md` of 2026-11-08 called SUP batch 4; the
branch `claude/sup-batch4` claimed for them was never pushed, so they are built here on
`claude/sup-batch4b`). Code: `crates/fnv-game/src/nvse_sup4.rs`, the kept calls in
`crates/fnv-game/src/delayed.rs`, the VM's hand-overs in `fnv_script::vm` (Ar_SetFloatsFrom's
output variables, MarkScriptOnLoad's calling script, the plugins' `ar_` functions passed to
the host), and the engine's save and load names in `engine/nvf-play/src/save.rs` and
`script.rs`. Tests: `crates/fnv-game/tests/sup4.rs` and the unit test of `nvse_sup4.rs`.

Source: the GECK wiki export [documented: the page named], in our own words. Not probed
yet: set `sup4` of `tools/capture/jip_probes.py` (lab `L5.ext.sup4`), questions in §3.

## 1. The functions

| Function | NVForever |
|---|---|
| AllocatePlayer | does nothing, 0 [documented: a joke page, "Two. Can play at that game. (no, they can't)"; decision: there is one player] |
| Ar_GetRandomKey `array` | a random key of the array, a number [documented]; -1 for an empty array [hypothesis] |
| Ar_GetRandomKeyMap `array` | a random key as a string, a number key as its text [documented: the note]; "" for an empty array [hypothesis] |
| Ar_HasInvalidRefs `array [dimension]` | 1 when the array holds an empty reference or one neither the load order nor the world knows [documented: "origin mod is disabled or temporary reference … removed"]; with a dimension the inner array at that key is searched [documented: the second example] |
| Ar_SetFloatsFrom `array f1 … f10` | the array's numbers in key order into the variables, up to 10; the VM writes them (`out_params`) [documented: the example]; an element that is no number gives 0 [hypothesis] |
| CallFunctionNextFrame `udf [frames] [removeOnLoad]` | the UDF that many frames later (1 to 1000, 1 by default) on the calling reference [documented: the notes]; frames count in menus too [hypothesis]; with removeOnLoad 0 it outlasts a load (`Game::take_session`), a function script's only [hypothesis: as the extender's other kept callbacks] |
| GetCurrentQuestObjectiveTeleportLinksAlt | the current objective's compass targets, one array: GetCurrentQuestObjectiveTeleportLinks's [documented: "the current compass marker objects"; hypothesis: the same objects] |
| GetLoadedSaveSize, GetSavedSaveSize `type` | type 0 the save file's bytes; -1 before any load or save this session [documented]; type 1, the .nvse co-save, 0 [decision: NVForever writes the extender's state into its one save file] |
| GetSaveName `type` | 0 the last save written, 1 the last loaded, without the extension; "" before any [documented]. The engine tells the game both (the save writer, the load) and they outlast a load |
| GetNearCells `depth` | the cells within `depth` of the player's (`scan_cells`, as GetFirstRef's depth); indoors the player's cell alone; -1 uGridsToLoad's, 2 [documented: "-1 … uGridsToLoad"; hypothesis: the default 5 cells across] |
| GetNearMapMarkers `depth` | the map marker references (REFR XMRK) in those cells [documented] |
| MarkScriptOnLoad, IsScriptMarkedOnLoad `script` | the calling script marked until the next load, which starts with none [documented]; the VM hands the running script to the host; a query or console line has none: 0 [hypothesis] |
| KillAll2 (KillAllAlt) | KillActor on every living actor whose combat target is the player, not the player's teammates [documented: "kills all current player combat enemies leaving companions … intact"; deprecated] |
| SetCaughtPCPickpocketting `[caught]` | the NPC's caught flag that a failed pickpocket sets (`crate::pickpocket`), set when given; the flag as it is now [documented] |
| TranslitText `text` | the text with Cyrillic letters in Latin ones, returned [hypothesis: the page names no return]; [decision: NVForever's own table, a common Russian romanisation with the Ukrainian and Belarusian letters; an upper-case letter starts its Latin letters in upper case] |

## 2. Left as nothing values

AllocatePlayer (a joke page) and the co-save size (no co-save). No engine follow-ups.

## 3. Probe questions (set sup4, not run)

At Doc Mitchell's house; Doc Mitchell's flag set back; the aux variable erased.

1. `randomkey`: a key in 0-4, a map key of the two, an empty list's (hypothesis `1 1 -1`).
2. `invalidrefs`: valid, an empty reference, an inner array without and with its
   dimension (hypothesis `0 1 0 1`).
3. `setfloats`: the page's example (hypothesis `3 8 5`).
4. `nextframe.set`, `.wait`, `.read`: a UDF two frames later (hypothesis `0` then `1`).
5. `saves`: the loaded save's size, its co-save's, its name, the last save's name before
   any save (hypothesis `1 1 1 0`; NVForever gives 0 for the co-save by decision; the lab's
   candidate loads no save, so it gives -1 sizes there).
6. `near`: cells at depth 0, 2 and -1 indoors, map markers (hypothesis `1 1 1 0`).
7. `markonload`: MarkScriptOnLoad from a query, and Doc Mitchell's script marked
   (hypothesis `0 0`, or the call refused outside a script).
8. `caught`: Doc Mitchell's flag read, set, read (hypothesis `0 1 1`).
9. `translit`: "Привет" (hypothesis `Privet`; the runtime compiler's code page may change
   the letters).
