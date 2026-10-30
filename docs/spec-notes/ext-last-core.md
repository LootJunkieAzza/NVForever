# The last core xNVSE functions (2026-10-30)

The xNVSE and FOSE functions left after the 2026-10-26 batches, and two JIP/LU getters the
lab asked for. Built in `crates/fnv-game/src/nvse_rest.rs`, `ui_functions.rs`,
`nvse_misc.rs`, `nvse_gaps.rs` and the script VM (`fnv-script/src/vm.rs`). Sources: the
GECK wiki export [documented: the page named], the lab's probe goldens [observed], and
the HUD captures in hud.md §14 [observed]. Every change lasts for the session.

## 1. Setters whose getters were built

| Function | What it changes | Read back by |
|---|---|---|
| SetEyes `eyes` | the NPC base's eyes (ENAM), so every reference of it; 1 when set [documented: "Returns 1 for success", "affects their baseform"] | GetEyes |
| SetHairLength `length [npc]` | the NPC base's LNAM [documented] | GetHairLength |
| SetNPCWeight `weight [npc]` | the NPC base's NAM7 [documented] | GetNPCWeight |
| SetActorBaseFlagsHigh `flags [npc]` | the ACBS flags' high 16 bits, the low ones kept [documented] | GetActorBaseFlagsHigh |
| SetOwningFactionRequiredRank `rank` | the rank a reference's owning faction needs (XRNK) [documented] | GetOwningFactionRequiredRank |
| SetPackageTargetDistance `package value` (alias SetPackageTargetCount) | the package target's count or distance, for every reader, the AI too [documented] | GetPackageTargetDistance |
| SetRefCount `count` | a placed stack's count [documented]; an inventory reference's stack is left alone [hypothesis] | GetRefCount |
| SetCurrentQuest `quest` | the active quest, "identical to ForceActiveQuest" [documented] | GetCurrentQuest |
| SetStringGameSettingEx `"Name\|Value"` | a string game setting, as SetStringSetting [documented; the page calls it broken and its replacement working] | GetStringGameSetting |
| SetAltControl `control button` | the control's mouse button, as SetControl with the mouse; not written to the INI [documented: "saved to the ini"; decision: NVForever never writes the game's INI files] | GetAltControl |

## 2. Input and menus

- MenuHoldKey / MenuReleaseKey (`mhk`, `mrk`) hold and release a key in menu mode
  [documented]; they share the keys HoldKey holds [hypothesis: the menus read the same
  held keys].
- ShowLevelUpMenu opens the level-up menu for the player's level now, without raising it
  [documented: "does NOT cause an increase in level"].
- GetUIFloatInherited `path [ExcludeMenuRoot]`: `x`, `y` and `depth` with the values of
  every parent tile whose `locus` is on added, up to the menu's root, which
  ExcludeMenuRoot leaves out; a tile without the trait counts 0; any other trait as
  GetUIFloatAlt [documented: GetUIFloatInherited; observed: hud.md §14, the page's example
  gives -50, and 0 with ExcludeMenuRoot, though `radiation_text` has no `depth` in the
  file]. Whether a parent without `locus` stops the sum is open [hypothesis: it does not].
- GetFontFile `n`: fonts 1-8 and 10-89 [documented], the INI's `sFontFile_N` (category
  Fonts), else Fallout_default.ini's names for 1-8 [documented: Fallout_default.ini]; 9
  and fonts nothing names give "" [observed: jip-forms fontfile.2, fontfile.9].
- GetNthPerkEntryString `perk n`: an activate choice's button label (EPF2) [documented:
  "Activate Prompt" for type 4]; "" for any other entry [observed: jip-forms perkentry.0].

## 3. Scripts and debugging

- CompileScript `path [forceRecompile]` (alias GetUDFFromFile): the text file
  `Data\NVSE\user_defined_functions\<path>` compiled into a script that Call runs like a
  UDF, cached by path unless forceRecompile [documented]. Its id is NVForever's own, above
  every plugin's FormIDs [decision, as the lambdas', script-compiler.md §9]. A missing
  file or a compile error gives no form [hypothesis].
- PrintVar / DebugPrintVar (`PrintV`, `DPrintV`) print each variable as `name: value` on a
  line of its own, up to 15; DebugPrintVar only with the calling mod's debug mode on
  [documented: PrintVar, "fMyVariable: 2.5" and "aMyVariable: [2, 4]"; DebugPrintVar]. A
  map shows `key: value` pairs in the brackets [hypothesis].
- PrintF, PrintDebugF, Ar_DumpF, DecompileScript, DumpDocs and DumpCommandWikiDoc write
  files (the Fallout folder, DecompiledScripts, nvse.log) [documented]; NVForever writes
  none of them and they do nothing else [decision: NVForever writes nothing beside the
  game, docs/design/install-layout.md]. DecompileScript gives an empty array.

## 4. Found by probing the original (2026-10-30, #1021, #1025)

Probe sets `rules1` and `rules2` (`tools/capture/jip_probes.py`, run from the owner's save
nvfairport; goldens private). They read to eight or ten decimals, more than the lab's sets.

- **SetNthPackage** `ref.SetNthPackage package index` (the page gives no arguments): the
  package at the index of the actor's base list is replaced, for the AI too, and the result
  is the package it replaced; an index past the end changes nothing [observed: Doc's 9
  packages, index 0 15ED62 became 10B756 and back, the count stayed 9; index 99 left 9;
  the result 15ED62]. Only `let x := ref.SetNthPackage p i` compiled in the game; the
  statement forms did not. The catalogue takes its parameters from the generator's
  observed table (`OBSERVED_PARAMS`, tools/capture/script_functions.py).
- **FATan / FATan2** are the game's own approximation, not the exact angle: an odd
  polynomial of degree 11 in 32-bit floats of min(|y|,|x|)/max(|y|,|x|), then π/2 less it
  when |y| is the larger, π less it when x is negative, negated when y is negative; π and
  π/2 are 32-bit floats (FATan2 1 0 = 90.0000025). The coefficients are a least-squares fit
  of the 21 readings in (0, 1] (residual 4.6e-8 radians, the 32-bit noise there), so
  NVForever matches every reading within 0.000014° and at six significant digits, not to
  the last decimal [observed: 29 readings; `fast_atan2` in fnv-script `vm/jip.rs`].
- **GetRunSpeed** (JohnnyGuitar) reads 338.8 for the player and for Doc Mitchell, and kept
  338.8 while Speed Mult (50), Agility (10), `fMoveRunMult` (5) and `fMoveBaseSpeed` (100)
  were changed one at a time, and while sneaking [observed: rules2]. 338.8 is
  `fMoveBaseSpeed` 77 × 4.4, and no human run clip moves at it (`fnvtool actor
  locomotion`); where the number comes from is open [hypothesis: worked out when the actor
  loads, so live setting changes do not reach it]. Not built.
- **The reference walk in Doc Mitchell's house**: the one reference the game counts only
  with taken references included is `VCG01CasualHardcoreMessageREF` (0011F293), a
  persistent reference the opening quest's state in the owner's save has taken away
  [observed: rules1, both lists]. A new game has it, so the count belongs to the save
  (#1023), not to a counting rule. The game lists the cell's references in its own order,
  not by FormID [observed]; NVForever's order is FormID order.

## 5. Not built

- PrintActiveTile: the tile under the pointer; the tile tree has no pointer state.
- `eval` is the if-statement form (ext-syntax.md), built in the compiler.
