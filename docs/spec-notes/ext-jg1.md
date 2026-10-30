# JohnnyGuitar NVSE, batch 1 (2026-10-30, #1007)

Record getters and setters of books, factions, creatures, races, weapons' world models,
music types, containers' sounds and lighting templates, in `crates/fnv-game/src/
nvse_jg1.rs`. Sources: the GECK wiki export [documented: the page named]; the probe set
`jg1` (`tools/capture/jip_probes.py`, run in the original from the owner's save; lab
`L5.ext.jg1`) [observed]: all 71 rows match. Setters change the record for the session,
read by the getters.

| Function | NVForever |
|---|---|
| GetBookFlags, SetBookFlags `book [flags]` | DATA flags: 1 scroll, 2 can't be taken [documented] |
| GetBookSkill, SetBookSkill `book [code]` | the skill's actor value code: the record keeps its index from Barter, the code is 32 more; -1 for none; the setter takes 32-46 or -1 and ignores anything else [documented; observed: BookSkillSurvival 44, SetBookSkill 5 refused] |
| GetFactionFlags, SetFactionFlags `faction [flags]` | DATA's two flag bytes as one mask, the second from bit 8 (track crime 256, allow sell 512) [documented: the page's table; observed: Lucky38Faction 256] |
| GetCreatureCombatSkill `creature` | DATA's combat skill [documented; observed: the dog 40] |
| GetBaseScale (GetBasedScale) `[actor]` | a creature's BNAM, an NPC's height (NAM6, 0 reading 1), 0 for anything else [documented: "If called on an NPC, will return their base height"; observed: the dog 0.8, Doc 1, the 10mm pistol 0] |
| Sign `x` | 1, -1, or 0 for 0 [documented: 5 gives 1; observed: -2, 0, 0.5] |
| GetRaceHeadModelPath, GetRaceBodyModelPath `race part female` | the race's head (NAM0) or body (NAM1) part list, male (MNAM) or female (FNAM), the part's INDX then its MODL [documented: the parts' numbers; the layout: xEdit RACE; observed: Caucasian and CaucasianOldAged] |
| GetWeaponWorldModelPath, SetWeaponWorldModelPath `weapon [path]` | the record's MOD4, "" when it has none [observed: the Shishkebab's ShishkebabGO.NIF, "" for the 10mm pistol, the combat knife and the nail board] |
| GetMusicTypePath `music` | FNAM under `Data\Music\` [documented: "Data/Music/Special/Success.mp3"; observed: "Data\Music\special\success.mp3"] |
| GetMusicTypeDB, SetMusicTypeDB `music [dB]` | ANAM, -6 when the record has none [documented: DefaultSuccess -6; observed: DefaultSuccess has no ANAM and reads -6]. The page's argument table lacks the dB its example passes; the catalogue takes it from the generator's `PARAM_FIXES` |
| GetContainerSound, SetContainerSound `container which [sound]` | 0 open (SNAM), 1 close (QNAM), 2 looping (RNAM) [documented: the numbers; the subrecords: xEdit CONT]; setting no form leaves the sound as it was [observed] |
| GetLightingTemplateTraitNumeric, SetLightingTemplateTraitNumeric `template trait [value]` | traits 1-16: ambient, directional and fog colour RGB, fog near and far, directional XY and Z, directional fade, fog clip distance, fog power (LGTM DATA) [documented: the page's table; observed: HiddenValleyInterior]; 0 for another trait |
