# JohnnyGuitar NVSE, batch 1 (2026-10-30, #1007)

Record getters and setters of books, factions, creatures, races, weapons' world models,
music types, containers' sounds and lighting templates, in `crates/fnv-game/src/
nvse_jg1.rs`. Sources: the GECK wiki export [documented: the page named]; the probe set
`jg1` (`tools/capture/jip_probes.py`, run in the original from the owner's save; lab
`L5.ext.jg1`) [observed]: all 68 rows match. Each setter step saves the value, sets it,
reads it and puts it back, so the run leaves the save as it found it. Setters change the record for the session,
read by the getters.

[observed] The two legacy JIP getter names below are a separate compatibility addition; their eleven-row control is not part of the original 68-row `jg1` batch.

| Function | NVForever |
|---|---|
| GetBookFlags, SetBookFlags `book [flags]` | DATA flags: 1 scroll, 2 can't be taken [documented]; one byte, the setter keeps the low 8 bits [documented: xEdit BOOK DATA] |
| GetBookSkill, SetBookSkill `book [code]` | the skill's actor value code: the record keeps its index from Barter, the code is 32 more; -1 for none; the setter takes 32-46 or -1 (whole numbers) and ignores anything else [documented; observed: BookSkillSurvival 44; the guidebook -1, set to 33, 5 refused, -1 back] |
| GetFactionFlags, SetFactionFlags `faction [flags]` | DATA's two flag bytes as one mask, the second from bit 8 (track crime 256, allow sell 512) [documented: the page's table; observed: Lucky38Faction 256] |
| GetCreatureCombatSkill `creature` | DATA's combat skill [documented; observed: the dog 40] |
| GetBaseScale (GetBasedScale) `[actor]` | a creature's BNAM, an NPC's height (NAM6, 0 reading 1), 0 for anything else [documented: "If called on an NPC, will return their base height"; observed: the dog 0.8, Doc 1, the 10mm pistol 0] |
| Sign `x` | 1, -1, or 0 for 0 [documented: 5 gives 1; observed: -2, 0, 0.5] |
| GetRaceHeadModelPath, GetRaceBodyModelPath `race part female` | the race's head (NAM0) or body (NAM1) part list, male (MNAM) or female (FNAM), the part's INDX then its MODL [documented: the parts' numbers; the layout: xEdit RACE; observed: Caucasian and CaucasianOldAged] |
| GetWeaponWorldModelPath, SetWeaponWorldModelPath `weapon [path]` | the record's MOD4, "" when it has none [observed: the Shishkebab's ShishkebabGO.NIF, "" for the 10mm pistol, the combat knife and the nail board] |
| GetMusicTypePath `music` | FNAM under `Data\Music\` [documented: "Data/Music/Special/Success.mp3"; observed: "Data\Music\special\success.mp3"] |
| GetMusicTypeDB, SetMusicTypeDB `music [dB]` | ANAM, -6 when the record has none [documented: DefaultSuccess -6; observed: DefaultSuccess has no ANAM and reads -6]. The page's argument table lacks the dB its example passes; the catalogue takes it from the generator's `PARAM_FIXES` |
| GetContainerSound, SetContainerSound `container which [sound]` | 0 open (SNAM), 1 close (QNAM), 2 looping (RNAM) [documented: the numbers; the subrecords: xEdit CONT]; the setter takes a sound form only [documented: "Sets a Sound form"], and setting no form leaves the sound as it was [observed: the dresser's open sound set to its close sound, none, and back] |
| GetContainerOpenSound `[container]` (legacy JIP name) | Opening sound (SNAM); optional explicit container takes precedence over the receiver, including explicit null [observed: separate eleven-row control below] |
| GetContainerCloseSound `[container]` (legacy JIP name) | Closing sound (QNAM), through the same container sound reader and session overrides [observed: separate eleven-row control below] |
| GetLightingTemplateTraitNumeric, SetLightingTemplateTraitNumeric `template trait [value]` | traits 1-16: ambient, directional and fog colour RGB, fog near and far, directional XY and Z, directional fade, fog clip distance, fog power (LGTM DATA) [documented: the page's table; observed: HiddenValleyInterior]; 0 for another trait |

## Legacy JIP container getters

[observed] Owned public-interface control `native_prep_container_46`, 2026-11-12,
xNVSE 6.4.9/JIP 57.30/JohnnyGuitar 528: an authored CONT with distinct opening and
closing SOUN links returns the corresponding form through GetContainerOpenSound and
GetContainerCloseSound. A placed reference with no explicit argument uses its base.
An explicit container overrides that receiver; explicit null or a wrong-kind form
returns no sound even with a valid receiver. A missing sound, an omitted form without
a receiver and explicit null without a receiver return no sound. Updating the opening
sound with SetContainerSound is visible to the legacy getter. All eleven rows
completed; the native process subsequently faulted during automatic shutdown. This
is behavior evidence, not native client stability acceptance.

[documented] [Container](https://geckwiki.com/index.php?title=Container), the existing
GetContainerSound/SetContainerSound entry above, and the CONT schema describe the
SNAM/QNAM sound slots. [observed] The source-call implementation reuses that reader
and its session overrides; it adds no native bytecode adapter or mod overlays.

[observed] Owned public-command reflection, native API repair jobs 19/19b, 2026-11-12:
both getters have an optional form argument and form result; their reflected opcodes
are 0x2305 and 0x2307. These values are provenance, not installed engine bytecode opcodes.

[observed] In the [maintainer's PR2300 review](https://github.com/LootJunkieAzza/NVForever/pull/2300#issuecomment-6848311718), LootJunkieAzza reports
independent `nvse.log` confirmation of GetContainerOpenSound 0x2305 and
GetContainerCloseSound 0x2307 from two mod-list installs. He identifies them in JIP LN's
block beginning at GetLNVersion 0x2200 and reports one calling unit for each getter in
a 211-plugin mod list. These are his installation observations, separate from the owned
eleven-row control above. He also identifies neighboring native registrations
SetContainerOpenSound 0x2306 and SetContainerCloseSound 0x2308; those setters are outside
the getter-only change and no setter behavior is inferred from registration alone.
