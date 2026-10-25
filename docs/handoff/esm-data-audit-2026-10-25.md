# ESM data the engine does not use (audit 2026-10-25)

Method: every record and subrecord in the 10 official masters (FalloutNV.esm + the four
story DLCs, GRA and the four packs; 630k records, 108 record types) was counted with a
private script, then checked against the code on origin/main (30ab0c8b). Project docs were
not used as evidence; only readers in `crates/` and `engine/` and whether anything outside
tests consumes what they produce. "Unread" = no code reads it; "read, unused" = decoded
into a struct field that nothing consumes. Counts are records across all 10 masters.
All findings are [observed] (code read + data count) unless tagged; the "player sees"
lines are [hypothesis].

## Whole record types never used

| Type | Records | What it is | Note |
|---|---|---|---|
| RGDL | 44 | Ragdoll settings (feedback, foot/look/grab IK, pose matching) | Ragdolls are built from the skeleton NIF; BPTD RAGA links to RGDL are stored but unused too |
| LSCT | 1 | Load screen layout | Load screen layout is the engine's own |
| NAVI | 6 | Navmesh info (islands, cross-cell connections) | fnvtool only; nav builds its own graph |

(CLMT, LGTM and LTEX are never named by type but are read through CELL/WRLD/REGN/LAND links.)

## Biggest gaps, ranked by what a player would notice

1. **Destruction visuals.** DEST logic works (health, stages, explosions, disable). The stage
   replacement model (DMDL; 40 ACTI, 20 MSTT, more on CONT/PROJ) is read but never drawn
   (`engine/nvf-play/src/combat.rs:3331` logs "not drawn yet"), DSTD debris is never spawned.
   Player sees: destroyed objects look unchanged.
2. **Game settings: ~360 of 648 GMSTs never looked up** (estimate ±10%, string matching).
   Unread groups include most fVATS* (30 of 39), fWeaponConditionSpread* (22),
   fActionPoints*, fGunSpread*, fBlockSkill*, fAutoAim*, fFurnitureMarker*. Where those
   behaviours exist they run on engine constants.
3. **Weapon VATS data.** WEAP VATS (490 records: effect, skill, damage mult, AP, silent,
   mod required) and VANM (86, attack name) are never read. Player sees: no weapon-specific
   VATS special attacks or multipliers.
4. **Per-tree variation.** REFR XSED (19,065 refs: SpeedTree seed) and TREE SNAM are unread;
   TREE CNAM leaf curvature/angles and shadow radius decoded, unused. Player sees: every tree
   of a type identical.
5. **Bush rustle.** STAT BRUS (6,307 statics, passthrough sound) unread.
6. **Light shadows.** Every placed light spawns with `shadow_maps_enabled: false`
   (`engine/nvforever/src/lights.rs:553`); LIGH Spot Shadow/Dynamic flags and falloff
   exponent unused.
7. **Creature body data.** CREA TNAM turn speed (2,238) and WNAM foot weight (2,238)
   unread (turn rate is a constant 300°/s), LNAM melee weapon list (829) unread.
   ACBS flags unread: Tilt Front/Back (749), Tilt Left/Right (274), Swims (273),
   Weapon & Shield (27). Player sees: creatures don't tilt to slopes; uniform turning.
8. **AI package behaviour flags.** Observe Combat (806 packages), Allow Idle Chatter (529),
   Avoid Radiation (675), Reaction To Player Actions (474) have no consumers; PKFD follow
   trigger radius (169) stored, unused.
9. **Factions.** XNAM disposition modifiers (165 non-zero relations) unread — disposition
   ignores factions; rank titles MNAM/FNAM (46/32) unread; Hidden from PC / Allow Sell flags
   unused.
10. **Dumb dialogue.** INFO Low Intelligence (33) / High Intelligence (22) flags and DIAL
    TDUM dumb responses (135 topics) unread — low-INT dialogue never changes.
11. **Crowd walla.** ASPC 5th SNAM + WNAM (all 145 acoustic spaces) decoded, never played.
    ASPC OBND (placed acoustic-space boxes) unread; reverb comes only from the cell.
12. **Explosion leftovers.** EXPL INAM placed object read, not built (`combat.rs:3852`).
    Player sees: explosions leave no fire/gas objects.
13. **Ammo armour piercing.** AMMO "Ignores Normal Weapon Resistance" is decoded but
    `crates/fnv-combat/src/damage.rs:371` checks only the weapon's flag.
14. **Magic effect archetypes not handled:** Cure Disease, Light, Lock, Open, Bound Item,
    Summon Creature (counts of records using them not taken). MGEF light, bolt/area sound,
    projectile speed, object display shader unread.
15. **Old-format sounds.** 136 SOUN use SNDX + separate ANAM curve (135) / GNAM reverb (77) /
    HNAM priority (40); none read, so they get the default curve. SNDD priority and loop
    begin/end (3,540) and flags Random Location, 2D Radius, Envelope Fast/Slow, Dialogue
    Sound unread.

## Smaller gaps by area

**World / rendering**
- CELL: XNAM water noise (44,480), XCLC hide-quad flags, DATA "Behave like exterior",
  No LOD Water; XCCM (1).
- WRLD: ONAM map offset/scale, NNAM canopy shadow, NAM3/NAM4 LOD water, NAM0/NAM9 bounds,
  several DATA flags.
- REFR: XMBO multibounds (7,377), XOCP/XORD occlusion planes (264/43), XPWR water
  reflection source (3,809), XNDP navmesh door link (1,360), XIBS sandbox-ignore (779),
  XACT activation action (255), XSRD (336), XTRG (3), XCHG (1). ACHR/ACRE XRGB treated as
  constant.
- REGN: RDOT object lists (19 regions) and RCLR map colour (319) unread.
- WATR DNAM sun power, light radius/brightness, reflection HDR mult, distortion; GNAM, MNAM.
  PWAT reflect/refract flags (only water type used).
- IMGS/IMAD: HDR grass/tree/skin dimmers, lum ramp, upper lum clamp, blur and bloom
  radius/alpha read but not rendered. WTHR sun damage and max cloud layers unused.
- LTEX SNAM specular and HNAM friction/restitution; LAND DATA flags read, unused.
- MSTT "On Local Map"; IPCT angle threshold; ADDN particle cap; EFSH explosion wind speed.
- NAVM NVGD (6,139) and NVER unread.
- LSCR WMI1 (283) unread.

**Items / combat**
- WEAP: ETYP (506), BIPL (52), EAMT (12) unread. DNAM reload_time, sight_usage,
  base VATS to-hit, kill impulse / impulse distance, rumble pattern/wavelength unused.
  Flags Short/Long Burst, NPCs Use Ammo, Minor Crime, Don't Use 1st/3rd Person IS
  Animations never tested. Melee swing sounds via SNAM not confirmed.
- PROJ: in-flight light, rotation and "Rotation" flag, "Pins Limbs", NAM2 unread.
- EXPL flags Radius in BS Units, Push Explosion Source Ref Only, Ignore Image Space Swap.
- BPTD: VATS target, gore rotate, "To Hit Chance - Absolute", NAM5 unused.
- ARMO: MOD4 female ground model (126), BNAM (478), BMCT (17), ETYP, ICO2, "Has Backpack".
- BOOK DESC (book text, 27) and "Scroll" flag unread.
- ENCH/SPEL: ENIT flags and most SPIT flags (Immune to Silence, PC Start Effect, Disable
  Absorb/Reflect, Force Touch Explode …), EFIT range.
- MGEF: base cost, school, flags Painless, No Hit Effect, Display Name Only, Gory Visuals.
- INGR (1) DATA/ENIT; MICO small icons (1,058) everywhere.

**Actors / AI / quests**
- CSTY: Cannot Use Stealthboy, Melee Alert OK, acrobatic dodge chance unused.
- AIDT mood (~300 non-neutral actors) only reaches GetActorValue.
- CLAS tag skills and attribute weights unused (NPCs carry stored skills, so harmless).
- RACE default hair colour (CNAM), FaceGen clamps (PNAM/UNAM).
- QUST stage log entries (113 quests) stored, never shown.
- IDLE "No attacking" flag. Guard "Remain Near Reference" (3 packages) has no caller.
- PERK: entry point 3 has no handler (one perk, NVDLC04AintLikeThatNowPerk); all other
  entry points the data uses are handled.
- Scripts: stubs/no-ops the data calls: SetVatsTarget (10), GetVATSTargetHeight (7
  conditions), PurgeCellBuffers (32), SetRumble (17), GetForceHitReaction (2),
  SetSceneIsComplex (2), trapUpdate, IsWaterObject, GetPlantedExplosive,
  PreloadMagicEffect (1 each). GetIsObjectType (0x11B1, 1 condition use) is not registered.

**Audio / activators / misc**
- DOOR FNAM Automatic Door / Minimal Use read, unused; Sliding unread.
- TERM DNAM flags Leveled, Alternate Colors, Hide Welcome Text; ANAM Force Redraw.
- ALOC NAM4/NAM7 delays (fnvtool only), NAM1/NAM2/NAM3/FNAM unread.
- DOBJ: only a handful of default-object slots are looked up; level-up music slot has no
  caller.
- MESG "Auto Display" flag. TES4 header fields beyond masters (harmless).

## Checked and fully used

GRAS, CLMT, LGTM, ECZN, CAMS/CPTH, IPDS, ANIO, DEBR, TXST, AMEF, RCPE/RCCT, CSNO/CDCK,
DEHY/HUNG/SLPD/RADS, LVLI/LVLN/LVLC, MSET, MUSC, MICN, TACT, FURN, NOTE, CONT (bar weight),
MISC/KEYM/IMOD/CMNY/CHIP/CCRD, ALCH, ARMA. Values checked all-zero in the data (no gap):
RACE skill boosts, NPC/CREA barter gold, AIDT teaches/max training level, LVLO count ≠ 1.

## Pure caches, not gaps

MODT/MO2T/MO3T/MO4T/DMDT texture hashes, MODB bound radius, MODD/MOSD FaceGen flags,
DSTF end markers, PKAM/PKED/PUID empty package markers, IDLC counts (the arrays are read),
CREA NIFT (undecoded), OBND on most base types.
