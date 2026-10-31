# JIP LN NVSE, batch 5 (2026-10-31, #1006)

Body-part data, land textures, texture sets' decal data and flags, and worldspaces' water,
in `crates/fnv-game/src/nvse_jip5.rs`. Sources: the GECK wiki export [documented: the page
named]; the probe set `jip5` (`tools/capture/jip_probes.py`, run in the original from the
owner's save; lab `L5.ext.jip5`) [observed]: 99 of 100 rows match. Setters change the
record for the session, read by the getters. SetBodyPartFlag, SetBodyPartTraitNumeric,
SetLandTextureTraitNumeric and SetTextureSetTraitNumeric return 0, though the catalogue
names their results "flagValue" and "traitValue" [observed: jip5's `.result` rows].

## 1. Body-part data (BPTD)

The record holds one group of subrecords per body part; the part is the group whose BPND
has that part type (0 torso, 1-2 head, 3-6 arms, 7-12 legs, 13 brain, 14 weapon)
[documented: the pages' part IDs; the layout: xEdit BPTD; observed: NVGecko].

| Function | NVForever |
|---|---|
| GetBodyPartDataHasPart `bpd part` | a group of that part type [observed: NVGecko, every ID 0-15] |
| Get/SetBodyPartDataRagdoll `bpd [ragdoll]` | RAGA |
| Get/SetBodyPartDataSkeleton `bpd [path]` | MODL |
| Get/SetBodyPartTraitNumeric `bpd part trait [value]` | BPND: 0 actor value, 1 damage mult, 2 health percent, 3 to-hit chance, 4 tracking max angle, 5-7 severable debris count / scale / decal count, 8 explosion chance, 9 limb replacement scale, 10-12 explodable debris count / scale / decal count, 13-15 the gore effects' translation, 16-18 their rotation [documented: the page's table; observed: NVGecko's head, traits 0-18, and 19 past the table reading 0] |
| Get/SetBodyPartTraitForm `bpd part trait [form]` | BPND: 0-1 severable and explodable explosion, 2-3 their debris, 4-5 their impact data set [documented; observed] |
| Get/SetBodyPartFlag `bpd part flag [value]` | BPND's flag bits 0-6 [documented: the page's table; observed: the head's flags 0-7] |

## 2. Land textures (LTEX)

| Function | NVForever |
|---|---|
| GetLandTextureNumGrasses, GetLandTextureNthGrass `ltex [index]` | the GNAM grasses in record order [observed: LScrubDirtCanyon01Grass, three] |
| LandTextureAddGrass, LandTextureRemoveGrass `ltex grass` | the session's list; a grass already there is not added twice [observed: three, four, three]; 1 when the list changed, else 0 [observed: add twice 1 0, remove twice 1 0] |
| Get/SetLandTextureTextureSet `ltex [set]` | TNAM |
| Get/SetLandTextureTraitNumeric `ltex trait [value]` | 0 material type, 1 friction, 2 restitution (HNAM), 3 specular exponent (SNAM) [documented; observed] |

## 3. Texture sets (TXST)

| Function | NVForever |
|---|---|
| Get/SetTextureSetTraitNumeric `txst trait [value]` | the decal data (DODT): 0-1 min and max width, 2-3 min and max height, 4 depth, 5 shininess, 6 parallax scale, 7 parallax passes; 8 the colour as R × 1000000 + G × 1000 + B; 0 without decal data [documented: the page's table; observed: DecalOrganicBallisticBB and LandscapeDirtNV] |
| Get/SetTextureSetFlag `txst flag [value]` | 0 No Specular Map (DNAM bit 0), 1 Parallax, 2 Alpha Blending, 3 Alpha Testing (DODT's flag bits 0-2) [documented: the page's table; observed for 1-3] |

Open: flag 0 reads 1 in the game for LandscapeDirtNV, whose DNAM is 0, and 0 for
1stPersonSniperRifleTexture, which has no DNAM [observed: jip5, format1]. The one row that
differs in both sets. What sets it is not known [hypothesis: something about the texture
set's maps, not the record's flag].

## 4. Worldspaces (WRLD)

| Function | NVForever |
|---|---|
| Get/SetWorldspaceNoiseTexture `world [path]` | XNAM, the water noise texture [observed: FreesideWorld, WastelandNV] |
| Get/SetWorldspaceWaterType `world [water]` | NAM2 [observed] |

Not built: GetWorldspacePersistentCell (the world index does not keep a worldspace's
persistent cell yet).

## 5. Formatting found on the way

The texture colour row showed that the game's sv_Construct writes `%g` as the C library
does, not as the wiki's "integer, 1E+006"; the probe set `format1` settled it (ext-format.md
§2).
