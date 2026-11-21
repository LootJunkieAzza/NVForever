# Spec notes: default objects, region sounds, camera shots and ragdoll records

ESM audit medium group 8 ("unparsed record types", docs/handoff/esm-gap-audit-2026-10-21.md).
The layouts are xEdit's [documented: xEdit wbDefinitionsFNV.pas DOBJ, CAMS, CPTH, RGDL,
REGN; wbDefinitionsCommon.pas wbRegionSounds], now in the schema (`schemas/fnv/`
gameplay.json DOBJ, effects.json CAMS and CPTH, combat.json RGDL, sky.json REGN `RDSD`).
Counts are FalloutNV.esm alone [observed 2026-10-23, private survey script, not committed].

## 1. DOBJ: the default objects

One record, `DefaultObjectManager`: `DATA` is a list of 34 FormIDs whose place says what
each is: 0 Stimpak, 1 SuperStimpak, 2 RadX, 3 RadAway, 4 Morphine, 5 Perk Paralysis,
6 Player Faction, 7–8 Mysterious Stranger NPC and faction, 9 Default Music, 10 Battle
Music, 11 Death Music, 12 Success Music, 13 Level Up Music, 14–17 player voices, 18 Eat
Package Default Food, 19 Every Actor Ability, 20 Drug Wears Off Image Space, 21 Doctor's
Bag, 22–23 Miss Fortune NPC and faction, 24 Meltdown Explosion, 25–30 Unarmed Forward,
Backward, Left, Right, Crouch and Counter power attacks, 31 Spotter Effect, 32 Item
Detected Effect, 33 Cateye Mobile Effect [documented: xEdit names, in order].

FalloutNV.esm [observed]: Default Music `DefaultExplore`, Battle `DefaultBattle`, Death
`DefaultDeath`, Success and Level Up `DefaultSuccess` (all MUSC); Drug Wears Off
`Addiction01ISFX` (IMAD); Meltdown `MinePlasmaExplosion` (EXPL); Every Actor Ability and
the Forward and Counter power attacks null. `DefaultDeath` names `special\death.mp3` and
`DefaultSuccess` `special\success.mp3`; neither file is shipped (`Data\Music\Special`
holds EndCredits, MainTitle, the end-game slideshow and stinger, and hail-chief music).

As built (`fnv_game::data::GameData::default_object`, `fnv_world::audio::AudioForms::default_objects`):

- **Death Music**: when the player dies, the DOBJ's Death Music plays as a script's music
  does, if its file is there; otherwise the SOUN `MUSDeath` as before (audio.md §13)
  [hypothesis: the default object is the death's music]. With the shipped data nothing
  changes; a mod that ships `special\death.mp3` or points the entry elsewhere is heard.
- **Drug Wears Off Image Space**: when the last effect of a chem the player took (ALCH,
  equip type Chems) ends, that image space modifier plays [documented: the entry's name;
  hypothesis: when].
- **Stimpak** (place 0): the companion wheel's Use Stimpak gives the carried stimpak the
  entry names first (`Game::wheel_stimpak`), else the one with Editor ID `Stimpak`
  [observed: FalloutNV.esm names `Stimpak` 0x00015169 there, so nothing changes with the
  shipped data].
- **Perk Paralysis** (place 5): Paralyzing Palm casts the entry's spell
  (`Game::palm_spell`), else `PerkParalyzingPalm` 0x0006B20B [observed: FalloutNV.esm names
  that spell there, `fnvtool esm decode --edid DefaultObjectManager`].
- **Stimpak, RadX, RadAway, Doctor's Bag** (places 0, 2, 3, 21): the Pip-Boy STATS
  page's hints count and use them (`nvf_play::pipboy::stats::read`, its own DOBJ read),
  else the Editor IDs as before [observed: FalloutNV.esm names `Stimpak`, `RadX`,
  `RadAway` and `NVDoctorBag` there, so nothing changes with the shipped data].
- Read and not used: the rest. The level-up's `DefaultSuccess` names no shipped file and
  the level-up keeps `UILevelUp` (xp-meter); the other chems, the factions, the voices. The
  Meltdown explosion (place 24) is used by the Meltdown perk and the unarmed power-attack
  effects (25–30) by the unarmed special moves since 2026-10-24 (nvf-play `combat.rs`,
  perk-entry-points.md §9, §11). Spotter Effect and Item Detected Effect: played while the
  player aims by Spotter and Search and Mark (perk-entry-points.md §13).
- Looked up by Editor ID because no place matches: the SOUN `MUSDeath` (the fallback after
  Death Music, `nvf_play::music`) and the night-vision scope's `NightScopeISFX`
  (`nvforever::scope_view`); the list has no death sound or scope image space.

## 2. REGN RDSD: region sounds

`RDSD` (a region's "Sound" data entry, RDAT type 7) is a list of (sound, weather flags,
chance): flags 1 Pleasant, 2 Cloudy, 4 Rainy, 8 Snowy (the WTHR classification); the
chance is stored × 10,000 [documented: xEdit wbRegionSounds, wbScaledInt4]. In
FalloutNV.esm [observed]: 1,081 entries in 76 regions; most named `AMBRGenericRumble2D6`
(24), `AMBSuperDuperMartRumble` (14), `AMBRMetalRattle`, `AMBVaultVentShaftOS` (13 each),
`AMBDesertWindA01`/`A02` (12 each); chances 40% (231), 30% (188), 20% (141), 50% (131),
60% (112), 10% (105).

The regions holding them are the `Audio…` regions acoustic spaces name (ASPC `RDAT`):
`AudioExtDesertDefault` (67), `AudioExtNorthVegas` (67), `AudioExtBlackMountain` (69),
`AudioIntMitchellHouse` (12), `AudioIntVault` (18) … [observed]; the Goodsprings
exterior cells' own regions (XCLR) hold none.

As built (`nvf_play::ambient::region_sounds`): indoors and out, every 10 s [design] the
sounds of the region the listener's cell's acoustic space names, and of the cell's own
regions (`fnv_world::audio::cell_regions`) [hypothesis: the acoustic space's region is
where the game reads them, audio.md §6], that play in the weather shown are tried in a random order, each at its chance [hypothesis:
the chance is per try], and the first that passes plays [design: one at a time]: at the
listener when the SOUN is 2D, else at a random point between its minimum distance and half
its maximum about the listener [design]. A sound naming no weather plays in any
[hypothesis]. The `ambient summary` counts them ("region sounds played").

## 3. CAMS and CPTH: camera shots and paths

CAMS [documented: xEdit]: a model, `DATA` (action Shoot/Fly/Hit/Zoom; location and
target Attacker/Projectile/Target; flags Position Follows Location, Rotation Follows
Target, Don't Follow Bone, First Person Camera, No Tracer, Start At Time Zero; time
multipliers for the player, the target and the world; max and min time; target %
between actors) and `MNAM` an image space modifier. CPTH: conditions, `ANAM` (parent,
previous sibling), `DATA` camera zoom (Default, Disable, Shot List) and `SNAM` shots.
FalloutNV.esm: 276 CAMS, 418 CPTH [observed]. They are the kill cameras of VATS and the
death cameras. As built (2026-10-24, ESM audit gap 10): both are loaded
(`fnv_game::camera_paths`, `GameData::camera_paths` / `camera_shots`), and VATS mode's
kill camera chooses a path from the tree and plays its shots' models (vats-mode.md §19).
The death camera stays the owner's own (death_cam.rs); the `PlayerDeath` paths are
loaded, not played. `fnvtool esm decode` shows them.

## 4. RGDL: ragdolls

RGDL [documented: xEdit]: `NVER` version, `DATA` (dynamic bone count; Feedback, Foot IK,
Look IK, Grab IK, Pose Matching yes/no, the three IK marked "broken, don't use"), `XNAM`
actor base, `TNAM` body part data, then feedback data (`RAFD`), feedback dynamic bones
(`RAFB`), pose matching (`RAPS`) and the death pose (`ANAM`). FalloutNV.esm: 38 RGDL
[observed]. The schema decodes `NVER`, `DATA`, `XNAM` and `TNAM`; the rest stays raw
(xEdit's field list for them was not read in full). Ragdolls do not use them: feedback
and pose matching (keyframed bones driving the ragdoll) are not built (ragdoll.md).

## 5. Checks

Unit tests: `default_object` places (0, 5, 11, 13, 20), the palm's fallback
(`palm_spell_in`), `RegionSound::plays_in`. Smoke `region-sounds` (Doc Mitchell's house,
AudioIntMitchellHouse: region sounds played) and `death-music-dobj` (the death line names
the SOUN fallback with the shipped data).
