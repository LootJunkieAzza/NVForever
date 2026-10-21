# ESM data the engine does not act on (audit, 2026-10-21)

The owner asked for this after finding that "Non-Playable" was honoured only on armour
(fixed for weapons and ammo in PR #509). It lists every record field, flag bit and record
type that FNV's masters carry but NVForever ignores or only stores.

**Method.** We flattened every record entry in `schemas/fnv/*.json` (3,911 subrecords,
fields, flag bits and enums) and searched consumer code (outside `fnv-esm`) for each name.
Five audits then read the consumers record by record: items, actors, logic/AI/effects,
world/environment, and base objects/effects. Record types that have no schema entry were
checked against xEdit's public FNV definitions (documentation only). Snapshot:
origin/main c384ae5f.

**Status words.** *Ignored*: decoded, or present in the data, but never read. *Stored*:
read into a struct, but no behaviour uses it. *No schema*: not in our schema, and no
hand-written parser reads it. Impact: **H** means a player would notice in normal play,
**M** means noticeable in places or in sound and visuals, **L** means minor or rare.
"(unverified)" marks claims the audit could not confirm from code or sources. Editor-only
data (EDID, MODT hashes, padding) is left out.

Before you build any line, check it against current main: several of these systems are
under active work.

## Priority list (high impact, across all records)

| # | Gap | Records | What vanilla does |
|---|---|---|---|
| 1 | Encounter zones are not read at all: no ECZN parser, and XEZN is read on no CELL or reference. Levelled actors resolve at a fixed level (`templates.rs:21` `LEVELED_ACTOR_LEVEL = 1`). | ECZN, XEZN, LVLN/LVLC, XLCM | A zone's level is set when first entered, from its minimum level and the player's level. Levelled NPCs, creatures, items and locks resolve at that level. "Never Resets" stops respawn. |
| 2 | ~~PERK combat entry points are never read. `fnv_combat::damage::PerkHooks` has only `NoPerks`, entries with conditions on another tab are skipped (`perks.rs:194`), and only the Set/Add/Multiply functions exist.~~ Built in claude/e2-perk-combat-entry-points ([perk-entry-points](../spec-notes/perk-entry-points.md)): tabs, the other functions, weapon damage, crit chance and damage, DT both ways, spread, VATS AP cost, weapon wear. Left (§2 there): reload, equip and attack speed (need the clips' playback rate), knockdown, Strength requirement, aiming move speed, throwing velocity, explosion radius. | PERK | Most combat perks and traits (crit chance and damage, weapon damage, DT, spread, VATS, reload speed and others). |
| 3 | Challenges never progress except scripted ones. Type, value1–3, SNAM/XNAM and rewards are ignored. | CHAL | Kill, use and craft challenges, and their XP and perk rewards. |
| 4 | Skill books cannot be read: DATA skill is ignored, and "Can't be Taken" is ignored too. | BOOK (+ PERK EP 11) | Reading gives skill points (Comprehension), counts toward the "Books Read" stat, and the item is consumed. |
| 5 | Death items are never added to corpses. | NPC_/CREA INAM | Levelled loot on the body: creature parts, hides, glands. |
| 6 | Vendors trade every item category, because AIDT services are ignored. The package flag "Offers Services" is stored only, and `GetOffersServicesNow` is missing (returns 0). | NPC_/CREA AIDT, PACK | Vendors deal only in their service categories, and only while their package allows it. |
| 7 | The world gives no radiation: REFR XRAD is ignored. Water also does no harm: WATR XNAM, "Causes Damage" and DATA damage are ignored. Explosion radiation (EXPL radiation level and radius) is stored only. | REFR, WATR, EXPL | Radiation hotspots, irradiated water, and nuke fallout. |
| 8 | The weather never varies: the highest-chance WLST entry always wins (`nvforever/sky.rs:394`), and volatility is ignored. REGN RDWT region weathers are never applied. | CLMT, REGN | Weathers are a weighted roll per climate; a region's weather list overrides the climate. |
| 9 | Weather has no sound and no precipitation (WTHR SNAM, precipitation MODL, lightning). | WTHR | Rain loops, wind, thunder and lightning, dust storms. |
| 10 | Weapons have no scopes: MOD3, "Has Scope", "Scope from Mod" and EFSD are ignored. | WEAP | A scope overlay when aiming, and night-vision scopes. |
| 11 | Critical-hit effects are never applied (CRDT effect and the "On Death" flag are stored only). | WEAP | Disintegrate and goo kills from energy weapons; crit effects on unique weapons. |
| 12 | Silencers keep the loud shot sound. WMS1/WMS2 are ignored; the mod changes only the detection sound level. | WEAP | Silenced shot sound. |
| 13 | No spent casings or drained cells: DAT2 consumed_ammo and consumed_percentage are stored only. | AMMO | Feeds the reloading-bench and recharger recipes. |
| 14 | Holotapes never play: NOTE SNAM, and TNAM for Sound and Voice notes, are ignored. | NOTE | Audio logs. |
| 15 | MGEF archetypes Paralysis and Turbo are not implemented. Cure Poison, Concussion and Dispel are missing too. | MGEF | Paralyzing Palm, the Turbo chem's slow time, Antivenom. |
| 16 | Difficulty does not scale damage: no `fDiffMultHP*` game setting is read. | GMST | Damage dealt and taken per difficulty level. |
| 17 | Quest target conditions are not tested, so every alternative marker shows at once. | QUST QSTA + CTDA | Only targets whose conditions pass get a marker. |
| 18 | Model texture swaps are missing: MODS/MO2S/MO3S/MO4S are not in the schema. | WEAP, ARMO, AMMO, STAT… → TXST | Re-skinned variants of shared meshes (how many vanilla records use them: unverified). |

## Medium impact (grouped)

- **AI and packages.** Several PACK fallout behaviours are ignored: Observe Combat, Reaction
  To Player Actions, Allow Idle Chatter, Friendly Fire Comments, Avoid Radiation. Also
  ignored: the package CNAM combat-style override, the begin/end/change TNAM topics, Guard
  "Remain Near Reference", and stay-at packages that place by Object ID, Object Type or
  "at package location" (`fnv-ai/goal.rs:467`). The patrol point's INAM idle, TNAM topic
  and script are not run; only the wait time is. Placed references ignore XIBS (Ignored
  By Sandbox) and header bit 25 No AI Acquire, on both items and actors. DOOR Minimal Use
  is stored but never reaches pathing. CREA "Can't Open Doors" is ignored (rated M–H), so
  animals path through doors. AIDT responsibility is never used by crime reactions.
  Condition Run On "Combat Target" is evaluated with no subject (`game.rs:2918`).
- **Combat style (CSTY).** Dodge chance, directions and timers; rushing attacks; "Ignore
  Damaging Self/Group/Spectators"; "Ignore Threats"; "Melee Alert OK"; max targeting FOV
  and combat radius (stored only).
- **NPC weapon choice.** WEAP "Player Only" and "Not Used In Normal Combat" are ignored.
  The DNAM fields min_range, aim_arc, resist_type and kill_impulse/impulse_dist are stored
  only. Four weapon-mod effects do nothing: Regenerate Ammo, Decrease Equip Time, Increase
  Projectile Speed, VATS Bonus. AMMO "Ignores Normal Weapon Resistance" is not checked.
- **Actor flags (ACBS).** "No VATS Melee", "No Blood Spray/Decal", CREA "Is Ghost" and
  "Allow PC Dialogue" (the engine guesses from greetings) are ignored. NPC_ NAM6 Height
  is stored but never applied to the drawn scale. LVLN/LVLC chance-none is not applied in
  fnv-game. CLAS is never decoded (tag skills, attributes, the Guard flag); the effect on
  auto-calculated NPCs is a hypothesis. FACT "Special Combat" and the XNAM modifier are
  unused. VTYP "Allow Default Dialog" is not parsed.
- **Cells and references.** CELL "Can't Wait" is ignored by the Wait menu. "Off Limits"
  is stored only. "Behave Like Exterior" and XCCM are ignored. REFR "Inaccessible" (header
  bit 8) is ignored. Activate parents (XAPD/XAPR: buttons that drive doors and traps) and
  activation prompts are ignored, both the base ACTI XATO and the REFR XATO. "Leveled Lock"
  is stored only. TERM "Leveled" is stored only.
- **Sound.** Several sounds never play: MSTT SNAM, STAT RNAM and BRUS (bush rustle), LIGH
  SNAM hum, TERM SNAM, ADDN SNAM, the PROJ in-flight sound, WEAP Shoot Dist, NAM6 Block and
  UNAM Idle, WATR SNAM, and MGEF effect sounds. SOUN ignores its attenuation curve, loop
  points, the 2D flag, Mute When Submerged and Start at Random Position. ASPC has no crowd walla
  (its reverb environment is played since claude/q27-reverb). ALOC location and retrigger delays are stored
  only. ARMO "Modulates Voice" is ignored. TACT "No Voice Filter" (header bit 13) is not
  in the schema, and no radio or intercom voice filter exists.
- **Detection.** The sound levels on EXPL, PROJ VNAM, IPCT and CREA NAM5 are unused; only
  the weapon's sound level feeds detection.
- **Visuals.** Projectiles carry no light (PROJ DATA light). EXPL INAM placed impact
  objects are not spawned. Destruction-stage DEBR debris is never spawned. ACTI WNAM water
  activators draw as plain models. Negative lights are skipped instead of darkening, and
  LIGH Spot Shadow and falloff exponent are not applied. EFSH holes and add-on models are
  not drawn. Fog power and clip distance (WTHR, XCLL, LGTM) are stored only, so all fog is
  linear. There is no HDR or bloom pass, so the IMGS HDR block and the IMAD HDR and bloom
  tracks are unused. WTHR transition speed and sun glare are hard-coded. IDLE animation
  group section (upper body or whole body) is stored only.
- **Unparsed record types.** CAMS/CPTH (kill, VATS and death cameras); DOBJ (default
  objects: death and level-up music, the unarmed power-attack spells and others; death
  music is found by the hard-coded EDID `MUSDeath`); RGDL (ragdoll feedback and pose
  matching); REGN RDSD region sounds (no schema; ambient one-shots are not built); CSNO
  slot-machine data (slots are a stub).

## Low impact (short list)

Ignored unless noted:

- **Items.** WEAP burst-length flags; "No Jam After Reload"; iron-sights animation flags;
  "Hide Backpack"; reload and jam times; rumble; EAMT. AMMO QNAM abbreviation (stored).
  ARMO female world model (MOD4) and female icons; BNAM. KEYM/MISC RNAM loop. CMNY value.
  The CSNO chip models are hard-coded, and only the last ICO2 deck back is kept.
- **Actors.** NPC_ AIDT mood (stored); "Not Pushable"; "No Rotating To Head-track";
  "Autocalc Service"; NAM7 weight; KFFZ on NPC_. CREA movement flags (Swims, Walks, No
  Combat in Water); tilt flags; No Head and No Arm flags; No Shadow; WNAM foot weight;
  CNAM impact set; LNAM melee list. RACE skill boosts (non-zero in vanilla: unverified);
  XNAM relations; weight; FaceGen clamps. HAIR "Fixed". FACT ranks and titles, Hidden, and
  Allow Sell. BPTD to-hit chances and IK flags. The LVLI entry owner (COED) is dropped.
- **Logic.** QUST "Allow repeated topics", "Compass Marker Ignores Locks", CNAM log
  entries. INFO PNAM/TPIC order (file order is used instead; matters for mods), and
  "Low/High Intelligence". The INFO speech challenge always passes (3 lines in the
  masters). DIAL INOM/INOA order (mods). The SCPT "Enabled" flag. GLOB FNAM type (Short
  and Long globals keep fractions). MESG "Auto Display" (hard-coded per message). The
  remaining MGEF flags (Painless, No Hit Effect, Gory Visuals: rated M). SPEL SPIT flags.
  ENCH ENIT flags. AMEF "Fatigue Mod".
- **World.** WRLD parent-use flags (stored), ONAM map offset, NAM0/NAM9 bounds, LOD and
  water flags. LAND DATA flags. LTEX friction, restitution and specular. CELL XCLC Hide
  Quad flags (rated L–M), IMPF material swaps, and XNAM noise. REFR XAMT/XAMC (rated L–M),
  XACT, XTRG, XCHG, WMI1 map-marker reputation (rated L–M), room, portal, multibound and
  occlusion data, and placed TXST decals (spawn path not found: unverified). NAVI is used
  only by fnvtool. NAVM edge cover is used only as yes or no per triangle.
  PMIS/PBEA/PFLA/PCBE are never simulated (none found in vanilla). LSCR "Displays In Main
  Menu" header bit (rated M–L). LSCT is not parsed.
- **Objects.** Header bits Visible When Distant, On Local Map, Obstacle and Navmesh
  (LOD is pre-built and the local map is built from collision). ACTI "Dangerous" (rated
  L–M). DOOR "Hidden" and "Sliding" (rated L–M) and "Automatic". GRAS "Vertex Lighting".
  TREE seeds, leaf angles and shadow radius. PWAT reflection and refraction flags.
  The PROJ "Can Be Disabled" and "Can Be Picked Up" flags are stored only (every mine can
  be disarmed). PROJ Supersonic, Pins Limbs and Rotation. EXPL world-orientation and push
  flags. IPCT angle threshold. ADDN particle cap. DEBR collision byte. TERM Alternate Colors
  and "Hide Welcome Text". INGR and COBJ are not item kinds (no CNTO use in vanilla).
  ARMA references are not drawn (vanilla count: unverified).

## Checked and complete

These records showed no gaps: IDLM, FLST, REPU, EYES, HDPT, IMOD, CHIP, CCRD, CDCK,
RCPE/RCCT, AVIF, RADS/DEHY/HUNG/SLPD, IPDS, ANIO, MICN and MUSC. ALCH lacks only its unused
"No Auto-Calc" flag. All "Playable" and "Non-Playable" flags are now honoured: ARMO, WEAP
and AMMO Non-Playable, plus Playable on RACE, HAIR, EYES, HDPT and PERK. CLAS "Playable"
is moot, because FNV has no class choice.
