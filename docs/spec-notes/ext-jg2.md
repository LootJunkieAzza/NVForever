# JohnnyGuitar NVSE, batch 2 (2026-10-30, #1007)

Cells' acoustic space and interior lighting, references' and cells' encounter zones,
message icons, image space modifiers' Animatable flag and weapons' VATS data, in
`crates/fnv-game/src/nvse_jg2.rs`. Sources: the GECK wiki export [documented: the page
named]; the probe set `jg2` (`tools/capture/jip_probes.py`, run in the original from the
owner's save; lab `L5.ext.jg2`) [observed]: all 36 rows match. Setters change the record
for the session, read by the getters.

| Function | NVForever |
|---|---|
| GetAcousticSpace, SetAcousticSpace `cell [space]` | the cell's XCAS; no space clears it [documented]. Not observed: no form of the call with a cell in a ref variable compiled in the game's runtime compiler (jg2, three tries), though both are in its command table |
| GetInteriorLightingTraitNumeric, SetInteriorLightingTraitNumeric `cell trait [value]` | XCLL: 0-2 ambient RGB, 3-5 directional RGB, 6 directional rotation XY, 7 rotation Z, 8 directional fade, 9-11 fog RGB, 12 fog near, 13 fog far, 14 fog power, 15 fog clip distance; 0 for another trait [documented: the page's table; observed: PrimmNashResidence, every trait] |
| GetRefEncounterZone (GetRefZone), SetRefEncounterZone (SetRefZone) `[zone]` | the reference's encounter zone (XEZN); no zone removes it; 1 when done [documented; observed: Doc Mitchell's reference none, set to Vault11Zone and removed] |
| SetCellEncounterZone (SetCellZone) `cell [zone]` | the cell's zone for the session, read by GetCellEncounterZone (ShowOff); no zone removes it; 1 when done [documented; observed: Doc's house] |
| GetMessageIconPath, SetMessageIconPath `[path] form [female]` | the small icon MICO, an armor's female one MIC2 [documented; the subrecords: xEdit ARMO; observed: Sunset Sarsaparilla, leather armor's male icon, "" female and for the baseball bat] |
| GetIMODAnimatable, SetIMODAnimatable `modifier [value]` | IMAD DNAM's Animatable flag [documented; observed: NVWastelandNightIS 0, set to 1] |
| GetWeaponVATSTraitNumeric, SetWeaponVATSTraitNumeric `weapon trait [value]` | VATS: 1 skill, 2 damage mult, 3 AP, 4 silent, 5 mod required; 0 for another trait [documented: the page's table; observed: the baseball bat 50, 2, 35, 1, 0] |

Not in this batch: GetRegionMapName and GetRecipeCategoryFlags (the schema does not decode
REGN RDMP or RCCT DATA yet), GetWorldspaceEncounterZone and SetWorldspaceEncounterZone,
IsCellVisited and IsCellExpired (no record of the cells the player has seen).
