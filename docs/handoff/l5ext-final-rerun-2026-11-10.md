# Final L5.ext rerun on main 3a0b30d81 (2026-11-10)

This is the last check before the owner signs off the NVSE work (#1758): every extender probe set, run against one fnvtool built from main once all NVSE batches were merged (census 0 in every library). Issue #2033.

## How it was run

- **fnvtool:** built from a fresh detached checkout of main 3a0b30d81, in its own target directory.
- **Lab:** `python -m fnvlab run --scenario 'L5.ext.*'`, covering all 106 scenarios of `parity-lab/scenarios/layer5_ext.json`. Nothing was edited: no goldens, scenarios, tolerances or comparators, no baseline, no `--reveal`.
- **Goldens:** the private ones in `C:\fnv-private\goldens`, captured in the original game:
  - xNVSE 6.4.9, JIP LN 57.30, JohnnyGuitar 528, ShowOff, kNVSE 37, SUP, MCM, 1111;
  - the owner's save `nvfairport`, at the airport, loaded read-only and never saved.
- **Sets with no golden:** probed in the original first, under the capture and smoke locks, with `tools/capture` and nvf_capture only (§3).
- **Triage:** each row that doesn't match was classed by reading the probe in `tools/capture/jip_probes.py`, the set's spec note, probe-sets.md, nvse-next-batch.md §7, and the fnvtool code. The classes:
  - **defect:** a wrong value computed from data fnvtool has;
  - **engine:** needs 3D, AI, combat, animation or physics ticks that headless `ext queries` doesn't run;
  - **save:** state from the owner's save that the lab can't see. No L5.ext scenario passes a save; #1023 tracks that;
  - **decided:** a documented decision to differ, including the compiler accepting what the original refuses, where a spec note records it;
  - **known:** listed as known before this run (jg9 save and engine rows, jg10 FaceGen, jip12 GetStringHash, jip14);
  - **unsure:** can't be told without another capture.

## 1. Result

- **Sets:** 106 in all.
  - 27 match every row.
  - 77 differ.
  - 2 have no golden on this machine (pp, jipppln; §3).
- **Rows:** 5582 of 6046 golden rows match. 546 rows differ (changed or extra).

The first run left 10 sets without a golden. The owner then set up the probe game, and eight of them were probed and labbed the same day (§3). Those eight add 377 golden rows, of which 371 match, and 9 rows that differ: save 5, engine 1, decided 3, no defect.

How the 546 rows that differ break down [observed: lab run; classes by reading the probes, docs and code]:

| Class | Rows |
|---|---|
| save (the owner's save or the probe game's setup) | 262 |
| engine | 122 |
| decided | 109 |
| known | 13 |
| unsure | 11 |
| **defect** | **29** |

- **Compared with probe-sets.md:** only three sets match fewer rows than the score it records. Which rows matched then isn't recorded, so these are guesses:
  - gaps7, 83 → 81: its rows that differ include the defects tempeffects.player and terrain.far;
  - jip.fifth, 62 → 61: its rows that differ include additemalt.pistol.equipped, unsure;
  - plugins2, 67 → 65: so.list.last is the probe game's DLC order.
- Every other set that probe-sets.md scores holds or gains: jip.hits now matches 49/49, jip.rgbui 34/34, jip.getters 42/42.
- **Save and setup rows:** of the 257, most are the player's skills and SPECIAL, and the DPS, crit and damage values that follow from them. The rest are inventory, the game hour and the active quest, plus three rows from the probe game's load order (§3).
  - These can't match until the lab starts the sets from the owner's save (#1023).
  - One group looks off: gaps7's `crit.pistol.*` reads a 10mm DPS of 202.31. On the same save, gaps2 and gaps6 read 41.44, and ours is also 41.44. So gaps7's capture ran in a different session state, and it is worth a recapture after #1023.
- **Decided rows:** mostly the compiler accepting what the original's runtime compiler refuses, where a spec note records the decision:
  - output variables (jip-ui-9.md §4);
  - fractional literals for int parameters in jg3, jg4, sup1, sup2 and lutana1;
  - GetZone and GetChanceNoneGlobal.
  - New fact for jip-ui-9.md: posex.inline and nifrot.head.inline show that an inline `(float fP)` declaration is refused as well.

## 2. Real mismatches

Defects (29 rows) and unsure rows (11), each with its issue. Original = the golden [observed]; ours = fnvtool at 3a0b30d81. The guesses are [hypothesis] unless the cell cites a doc or code. Values are as the lab prints them.

- **Two defects confirmed by hand:**
  - #2035: `levelup.rs` `skill_points` gives the half point for an even Intelligence, but the original gives it for an odd one, which also changes real level-ups.
  - #2036: `fnvtool ext queries` prints `script did not compile` for proj.reg and proj.reg2.
- **Borderline calls the triage flagged:**
  - one111 armor.doc.DT (engine): which armour an NPC equips is decided from record data.
  - events2 je.cripple.* (engine): we fire CrippledLimb only from combat hits, not from DamageAV on a limb.
  - knvse2 k2.global and k2.attrs.state (engine): they aren't among the two leftovers ext-knvse.md records.
  - so3.weight.base and .calc print `-0.000` where the original prints `0.000` (in #2038).

### defect

| Set | Probe | Original | Ours | Guess at the cause | Issue |
|---|---|---|---|---|---|
| gaps5 | perk.smallframe.type.0 | "5" | "1" | perk entry order: the game's Small Frame entry 0 is an ability (type 5), ours the entry point 6; nvse_lu3.rs nth_entry_of / nth_entry (the game orders entries differently from record order, e.g. sorted by rank/priority or type) | #2038 |
| gaps5 | perk.smallframe.function.0 | "-1" | "6" | perk entry order: the game's Small Frame entry 0 is an ability (type 5), ours the entry point 6; nvse_lu3.rs nth_entry_of / nth_entry (the game orders entries differently from record order, e.g. sorted by rank/priority or type) | #2038 |
| gaps7 | terrain.far | "-2048" | "-21" | GetTerrainHeight's no-hit fallback is -20.48; the game reads -2048 (a factor of 100); nvse_gaps.rs getterrainheight | #2038 |
| gaps7 | tempeffects.player | "-1" | "0" | GetTempEffects returns an empty array (size 0) where the game returns no array (ar_Size -1, as jip_probes.py's gaps7 comment notes); nvse_jip3.rs gettempeffects | #2038 |
| jip.n7 | karma.set | null | {"id": "karma.set", "value": "37.5"} | compiler too lenient: a fractional literal for SetBaseKarma does not compile in the original (probe-sets.md rules), ours accepts it | #2037 |
| jip.projectile | proj.reg | {"id": "proj.reg", "value": "1"} | null | no candidate row for the 4- and 3-line impact-handler lambdas while proj.reg.first (no GetSelf.) runs: likely our compiler/VM refuses a call result as calling reference (GetSelf.GetProjectileRef*, (GetSelf.X).GetBaseObject) inside a lambda | #2036 |
| jip.projectile | proj.reg2 | {"id": "proj.reg2", "value": "1"} | null | same as proj.reg: GetSelf.GetProjectileRefDamage / (GetSelf.GetProjectileRefWeapon) == rGun lambda produces no row | #2036 |
| jip5 | jip5.txst.dirt.flag.0 | "1" | "0" | GetTextureSetFlag 0 (No Specular Map) reads DNAM bit 0; the original gives 1 for LandscapeDirtNV whose DNAM is 0: rule unknown, open in jip-records-5.md (guess: derived from the texture set's missing map paths) - nvse_jip5.rs | #2039 |
| jip6 | jip6.dest.settrait.1.2.300 | "1 0 3 1" | "1 0 1 1" | model damage stage kept as a raw byte; the original maps out-of-range values (300 reads 3) by an unknown rule (jip-records-6.md, open) - nvse_jip6.rs | #2039 |
| jip6 | jip6.dest.settrait.1.1.3.5 | null | {"id": "jip6.dest.settrait.1.1.3.5", "value": "2 0 3 2"} | compiler too lenient: a fractional literal for this argument does not compile in the original (jip-records-6.md), ours runs it | #2037 |
| jip6b | jip6b.dest.stage.10 | "1 0 1 1" | "1 0 10 1" | model damage stage kept as a raw byte; the original refuses 10 and maps 255/256/300/-1 to 3/4/3/3, rule unknown (jip-records-6.md, open) - nvse_jip6.rs | #2039 |
| jip6b | jip6b.dest.stage.255 | "1 0 3 1" | "1 0 255 1" | model damage stage kept as a raw byte; the original refuses 10 and maps 255/256/300/-1 to 3/4/3/3, rule unknown (jip-records-6.md, open) - nvse_jip6.rs | #2039 |
| jip6b | jip6b.dest.stage.256 | "1 0 4 1" | "1 0 1 1" | model damage stage kept as a raw byte; the original refuses 10 and maps 255/256/300/-1 to 3/4/3/3, rule unknown (jip-records-6.md, open) - nvse_jip6.rs | #2039 |
| jip6b | jip6b.dest.stage.300 | "1 0 3 1" | "1 0 1 1" | model damage stage kept as a raw byte; the original refuses 10 and maps 255/256/300/-1 to 3/4/3/3, rule unknown (jip-records-6.md, open) - nvse_jip6.rs | #2039 |
| jip6b | jip6b.dest.stage.m1 | "1 0 3 1" | "1 0 1 1" | model damage stage kept as a raw byte; the original refuses 10 and maps 255/256/300/-1 to 3/4/3/3, rule unknown (jip-records-6.md, open) - nvse_jip6.rs | #2039 |
| jip7 | jip7.ipct.trait.17 | "2.55321e+08" | "0" | impact data without DODT (BaseBallBatMetalImpact): the original reads colour 255320535, a run-time default like the widths/heights ours already defaults; ours 0 - nvse_jip7.rs trait 17/14 (jip-records-7.md, open) | #2039 |
| jip7 | jip7.ipct.settrait.14.300300300 | "2.55321e+08 0 2.55321e+08 2.55321e+08" | "0 0 0 0" | impact data without DODT (BaseBallBatMetalImpact): the original reads colour 255320535, a run-time default like the widths/heights ours already defaults; ours 0 - nvse_jip7.rs trait 17/14 (jip-records-7.md, open) | #2039 |
| jip7 | jip7.ipct.settrait.14.10020030 | "2.55321e+08 0 1.002e+07 1.002e+07" | "0 0 1.002e+07 0" | impact data without DODT (BaseBallBatMetalImpact): the original reads colour 255320535, a run-time default like the widths/heights ours already defaults; ours 0 - nvse_jip7.rs trait 17/14 (jip-records-7.md, open) | #2039 |
| jip7b | jip7b.ipct.colour.ipct | "255320535" | "0" | impact data without DODT: default colour 255320535 not given (as jip7) - nvse_jip7.rs | #2039 |
| jip7b | jip7b.ipct.setcolour.16777215 | "63063063 0 16009218 63063063" | "63063063 0 16777215 63063063" | colour setter: a channel past 255 carries into the next byte (16\|777\|215: G 777 -> 9, B 215+3 = 218 = 16009218); ours keeps the number - nvse_jip7.rs SetImpactDataTraitNumeric 14 (jip-records-7.md, open) | #2039 |
| jg6 | jg6.callpermobile.high | "20 16" | "16 20" | CallPerMobileObject level 0 calls only actors (ext-jg6.md: original also calls 4 non-actors, not built) and ours calls 16 while our own GetActorsByProcessingLevel 0 lists 20 (nvse_jg6 / nvse_gaps); count part may be save | #2038 |
| jg8 | jg8.shuffle | null | {"id": "jg8.shuffle", "value": "5 15"} | compiler too lenient: still refused after the rewrite (golden is the rerun); likely Ar_Shuffle given an inline (Ar_List ...) or element arithmetic aOut[0]+... | #2037 |
| lutana1 | lu1.perk.entryform.builtto.0 | "00137B00" | "00000000" | GetNthPerkEntryForm: entries 0 and 1 of Built to Last swapped; nvse_lu3::nth_entry sort (rank, priority, reverse record order) wrong for this perk | #2038 |
| lutana1 | lu1.perk.entryform.builtto.1 | "00000000" | "00137B00" | as builtto.0 (nvse_lu3.rs nth_entry) | #2038 |
| nvse.misc | pkg.nth0.location | null | {"id": "pkg.nth0.location", "value": "0015ED61"} | compiler too lenient: GetFormIDString (GetPackageLocation rG) refused by the original (untyped return under GetFormIDString, as RemovePackageAt / GetCurrentFurnitureRef); not documented for this one | #2037 |
| showoff3 | so3.points.skill | "13 13 12 12 13 13" | "12 12 12 12 12 12" | levelup::skill_points (crates/fnv-game/src/levelup.rs) adds the even-level +1 only for even Intelligence; observed 13 at even levels, 12 at odd fits INT 5 (also implied by medicine 17 with Luck 10): odd INT half point should alternate | #2035 |
| showoff3 | so3.ease.badcode | null | {"id": "so3.ease.badcode", "value": "-1 -1 -1"} | compiler too lenient: a fractional literal for an int parameter compiles here, the original refuses it (also jg3/jg4/sup1/sup2/lutana1/so3 rows); here ApplyEasingAlt 0.5 1.5 0 (undocumented) | #2037 |
| showoff3 | so3.jumping | null | {"id": "so3.jumping", "value": "0 0"} | compiler too lenient: rP.IsJumping refused by the original (page marks it broken; maybe not registered or no calling ref); undocumented | #2037 |
| sup4 | sup4.near | "1 20 25 1" | "1 25 25 1" | GetNearCells 2 gives 25 here, 20 in the original; which 5 cells depth 2 leaves out is open (ext-sup4.md §4, scan_cells) | #2039 |

### unsure

| Set | Probe | Original | Ours | Guess at the cause | Issue |
|---|---|---|---|---|---|
| format1 | txst.dirt.dnam | "1 0 0 0" | "0 0 0 0" | GetTextureSetFlag 0 reads 1 in the game though LandscapeDirtNV's DNAM is 0 (fnvtool esm decode); jip-records-5.md s3 marks the rule open | #2039 |
| plugins3 | refname.near.count | "522" | "605" | GetRefs 0 2: ours 605 vs 522; either the save's disabled or moved references (not carried over, s7) or our walk counting references the game skips; needs a per-reference diff |  |
| jip.fifth | additemalt.pistol.equipped | "0" | "1" | ours equips at once (nvse_jip.rs additemalt cites an earlier observation that equip 1 equips); the original reads 0 the same frame: same-frame timing or save state unknown; needs a next-frame read |  |
| jg2 | jg2.refzone.player | "1 000E9081 0000001E 0000001E" | "1 000E9081 00000000 00000000" | player's zone 0000001E in the original, no master has it; ext-jg2.md: open, engine default zone or the save's |  |
| nvse.flags | ref.hidden.16941d | "none" | "73 0" | original: ref not valid (its cell not loaded at the save's place); ours valid '73 0' - load-state vs our records-always-valid; can't tell without the headless place |  |
| nvse.flags | ref.hidden.1690f1 | "none" | "73 0" | as ref.hidden.16941d |  |
| nvse.fourth | ir.removemeir.doc | null | {"id": "ir.removemeir.doc", "value": "1"} | original gave no row yet the next counts show the removal ran; maybe a runtime error at return with a target - not documented |  |
| sup4 | sup4.randomkey | "1 0 0" | "1 1 0" | open (ext-sup4.md §4): which key the map gives |  |
| sup4 | sup4.nextframe.read | "0" | "1" | open (ext-sup4.md §4): whether the lambda runs at the 2nd frame |  |
| sup4 | sup4.caught | "0 0 0" | "0 1 1" | open (ext-sup4.md §4): SetCaughtPCPickpocketting did not read back in the original |  |
| sup5 | sup5.xl.erase | "1 1 0 1 3" | "1 1 0 0 3" | row count after erase: no rule found (ext-sup5.md §1) |  |

## 3. Sets with no golden: owner step

At the first run these sets could not be recorded on this machine: NVFExtProbe.esp and the MCM plugin were inactive, ITR was not installed and the test INIs were not in `Data\Config` [observed 2026-11-10: `plugins.txt` listed CM4A1.esp and FalloutNV.esm only].

The owner then installed ITR, copied the test INIs and had `plugins.txt` list The Mod Configuration Menu.esp and NVFExtProbe.esp. The eight sets below were probed in the original under both locks (`ext_probe_run.py`, then `capture_run.py` for itr, ini_files and nvse_vars), collected into the private goldens and labbed against the same fnvtool [observed 2026-11-10]:

| Set | Matched | Rows that differ |
|---|---|---|
| nvse.strings | 66/67 | tostring.form `[Loot]` vs `[<no name>]`: the save's player name (save). replace.empty_old: an extra row, because Sv_Replace with empty old text crashes the game and NVForever chose a safe answer (ext-strings.md, decided) |
| nvse.expressions | 35/35 | |
| nvse.arrays | 84/84 | range.step0: an extra row, because Ar_Range with step 0 crashes xNVSE (ext-arrays.md, decided) |
| nvse.loops | 21/23 | foreach.container_entries and container_kinds 27 vs 2: the save's inventory (save) |
| nvse.udf | 19/19 | |
| nvse.vars | 18/18 | `HasVariable` now reads 1 on the plugin's quest |
| ini-files | 39/39 | |
| itr | 89/92 | exteriordoor.player and nextdoor.doc: the player stands outdoors in the save and in GSGasStation in the candidate (save). navmesh.player 1 vs 0: no navmesh answerer headless (ext-itr.md, engine). perkblockers.wildwasteland.kind.1: an extra row, because an index past the end is a script error in the game and 0 in our VM (ext-itr.md, decided) |

The run crashed the game twice, on the probes that the spec notes record as crashing xNVSE (strings 05, arrays 07 #4). The ini-files steps wrote into the test INIs. Those were put back from `tools/capture/jip-probes/config` afterwards, and `SUPNVSE.log` was deleted.

**pp and jipppln** are still without a golden here. ppNVSE needs JIP++, and JIP++ replaces JIP LN, which JohnnyGuitar checks for. So both go in only for one run with `bJIPFixes=0`, as ext-jippp.md §3 describes, and never stay installed. Their goldens were captured that way on 2026-11-01 and are on the other machine (`jip/pp.json`, `jip/jipppln.json`). Copying them here is enough.

The old setup also explains three rows counted as save above. These and the NVFExtProbe.esp rows below can be captured again now that the plugins are active; that has not been done yet:
- gaps1 glta.probe: the original returns all 496 weapons for index 255;
- mcm.loaded: 0;
- plugins2 so.list.last: GunRunnersArsenal at index 05, where the lab has 09.

Rows that use NVFExtProbe.esp forms in nvse-fourth, plugin-odds and showoff4 read in the original as if the plugin were missing.

## 4. Every set

Matched = rows equal to the golden, out of the golden's rows. Extra = rows we print that the original did not. The class counts cover every row that differs.

| Scenario | Matched | Extra rows | Not matching, by class |
|---|---|---|---|
| L5.ext.format1 | 14/15 |  | unsure 1 |
| L5.ext.gaps1 | 23/25 |  | save 1, decided 1 |
| L5.ext.gaps10 | 52/57 |  | engine 2, save 3 |
| L5.ext.gaps11 | 36/47 |  | save 11 |
| L5.ext.gaps2 | 44/53 |  | save 9 |
| L5.ext.gaps3 | 53/57 |  | engine 1, save 3 |
| L5.ext.gaps4 | 38/42 |  | engine 2, save 1, decided 1 |
| L5.ext.gaps5 | 91/100 |  | defect 2, engine 5, save 2 |
| L5.ext.gaps6 | 41/55 |  | save 14 |
| L5.ext.gaps7 | 81/110 |  | defect 2, engine 1, save 26 |
| L5.ext.gaps8 | 47/48 |  | engine 1 |
| L5.ext.gaps9 | 62/103 |  | save 41 |
| L5.ext.ini.files | 39/39 |  |  |
| L5.ext.itr | 89/92 | 1 | engine 1, save 2, decided 1 |
| L5.ext.jg1 | 68/68 |  |  |
| L5.ext.jg10 | 4/10 |  | engine 2, save 1, known 3 |
| L5.ext.jg2 | 37/38 |  | unsure 1 |
| L5.ext.jg3 | 184/186 | 1 | decided 3 |
| L5.ext.jg4 | 116/117 | 5 | decided 6 |
| L5.ext.jg5 | 88/88 |  |  |
| L5.ext.jg6 | 27/33 |  | defect 1, save 4, decided 1 |
| L5.ext.jg7 | 14/14 |  |  |
| L5.ext.jg8 | 7/7 | 2 | defect 1, decided 1 |
| L5.ext.jg9 | 7/11 |  | known 4 |
| L5.ext.jgnif | 4/8 |  | engine 4 |
| L5.ext.jip.events | 17/17 |  |  |
| L5.ext.jip.events2 | 34/43 |  | engine 4, save 5 |
| L5.ext.jip.fifth | 61/70 |  | unsure 1, save 8 |
| L5.ext.jip.forms | 24/24 |  |  |
| L5.ext.jip.getters | 42/42 |  |  |
| L5.ext.jip.hits | 49/49 |  |  |
| L5.ext.jip.n7 | 85/89 | 1 | defect 1, save 4 |
| L5.ext.jip.nifblocks | 26/60 | 2 | engine 34, decided 2 |
| L5.ext.jip.playerstate | 31/31 |  |  |
| L5.ext.jip.projectile | 14/33 |  | defect 2, engine 16, save 1 |
| L5.ext.jip.records | 89/89 |  |  |
| L5.ext.jip.rgbui | 34/34 |  |  |
| L5.ext.jip.skysound | 191/193 |  | save 2 |
| L5.ext.jip.stores | 32/32 |  |  |
| L5.ext.jip10 | 16/21 |  | save 5 |
| L5.ext.jip11 | 22/28 |  | engine 3, save 3 |
| L5.ext.jip12 | 31/36 |  | engine 1, decided 3, known 1 |
| L5.ext.jip13 | 12/17 |  | engine 5 |
| L5.ext.jip14 | 9/10 |  | known 1 |
| L5.ext.jip4 | 276/281 |  | save 5 |
| L5.ext.jip5 | 99/100 |  | defect 1 |
| L5.ext.jip6 | 90/91 | 1 | defect 2 |
| L5.ext.jip6b | 32/37 |  | defect 5 |
| L5.ext.jip7 | 105/108 |  | defect 3 |
| L5.ext.jip7b | 44/46 |  | defect 2 |
| L5.ext.jip8 | 281/282 |  | decided 1 |
| L5.ext.jip9 | 42/48 | 10 | engine 1, save 1, decided 14 |
| L5.ext.jipppln | no golden here | | captured 2026-11-01 with JIP++ swapped in; the golden is on the other machine (§3) |
| L5.ext.knvse.anim | 20/32 |  | engine 12 |
| L5.ext.knvse2 | 27/31 |  | engine 4 |
| L5.ext.lu.leveled | 91/94 | 3 | save 3, decided 3 |
| L5.ext.lu.leveled2 | 33/40 |  | save 7 |
| L5.ext.lu2 | 15/15 |  |  |
| L5.ext.lu3 | 5/5 | 4 | known 4 |
| L5.ext.lutana1 | 220/226 | 7 | defect 2, save 1, decided 10 |
| L5.ext.mcm | 12/13 |  | save 1 |
| L5.ext.nvse.actors | 64/64 |  |  |
| L5.ext.nvse.arrays | 84/84 | 1 | decided 1 |
| L5.ext.nvse.controls | 90/90 |  |  |
| L5.ext.nvse.data | 20/20 |  |  |
| L5.ext.nvse.eighth | 56/59 |  | save 3 |
| L5.ext.nvse.events | 81/100 |  | engine 19 |
| L5.ext.nvse.expressions | 35/35 |  |  |
| L5.ext.nvse.fifth | 76/77 |  | save 1 |
| L5.ext.nvse.flags | 30/42 |  | unsure 2, save 10 |
| L5.ext.nvse.forms | 110/110 |  |  |
| L5.ext.nvse.fourth | 62/76 | 2 | unsure 1, save 14, decided 1 |
| L5.ext.nvse.inv | 21/21 |  |  |
| L5.ext.nvse.keynames | 266/266 |  |  |
| L5.ext.nvse.leftovers | 13/16 |  | save 3 |
| L5.ext.nvse.loops | 21/23 |  | save 2 |
| L5.ext.nvse.math | 15/15 | 9 | decided 9 |
| L5.ext.nvse.misc | 33/34 | 1 | defect 1, save 1 |
| L5.ext.nvse.refs | 11/12 |  | save 1 |
| L5.ext.nvse.second | 87/89 |  | save 2 |
| L5.ext.nvse.setters | 10/10 |  |  |
| L5.ext.nvse.seventh | 38/44 | 1 | save 2, decided 5 |
| L5.ext.nvse.sixth | 41/41 | 2 | decided 2 |
| L5.ext.nvse.strings | 66/67 | 1 | save 1, decided 1 |
| L5.ext.nvse.third | 25/25 |  |  |
| L5.ext.nvse.udf | 19/19 |  |  |
| L5.ext.nvse.vars | 18/18 |  |  |
| L5.ext.one111 | 117/118 |  | engine 1 |
| L5.ext.plugin.odds | 42/42 | 2 | decided 2 |
| L5.ext.plugins1 | 67/71 |  | save 4 |
| L5.ext.plugins2 | 65/67 |  | save 2 |
| L5.ext.plugins3 | 19/21 |  | unsure 1, save 1 |
| L5.ext.pp | no golden here | | captured 2026-11-01 with JIP++ swapped in; the golden is on the other machine (§3) |
| L5.ext.showoff2 | 49/50 | 1 | decided 2 |
| L5.ext.showoff3 | 85/104 | 3 | defect 3, engine 1, save 17, decided 1 |
| L5.ext.showoff4 | 38/38 |  |  |
| L5.ext.showoff5 | 25/28 |  | decided 3 |
| L5.ext.sup1 | 76/89 | 3 | save 10, decided 6 |
| L5.ext.sup2 | 32/37 | 3 | save 5, decided 3 |
| L5.ext.sup3 | 28/35 | 11 | save 5, decided 13 |
| L5.ext.sup4 | 4/10 | 1 | defect 1, unsure 3, save 1, decided 2 |
| L5.ext.sup5 | 13/31 | 2 | unsure 1, engine 2, save 13, decided 4 |
| L5.ext.sup6 | 10/12 | 2 | decided 4 |
| L5.ext.sup7 | 11/11 |  |  |
| L5.ext.tmpl2 | 12/12 |  |  |
| L5.ext.tmpl3 | 20/23 |  | decided 3 |
