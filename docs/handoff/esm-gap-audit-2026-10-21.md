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
| 1 | ~~Encounter zones are not read at all: no ECZN parser, and XEZN is read on no CELL or reference. Levelled actors resolve at a fixed level (`templates.rs:21` `LEVELED_ACTOR_LEVEL = 1`).~~ Built in claude/e1-encounter-zones ([encounter-zones](../spec-notes/encounter-zones.md)); left: leveled locks (REFR XLOC "Leveled", no documented rule), zone owner/rank, GMST level-difference bounds. | ECZN, XEZN, LVLN/LVLC, XLCM | A zone's level is set when first entered, from its minimum level and the player's level. Levelled NPCs, creatures, items and locks resolve at that level. "Never Resets" stops respawn. |
| 2 | ~~PERK combat entry points are never read. `fnv_combat::damage::PerkHooks` has only `NoPerks`, entries with conditions on another tab are skipped (`perks.rs:194`), and only the Set/Add/Multiply functions exist.~~ Built in claude/e2-perk-combat-entry-points ([perk-entry-points](../spec-notes/perk-entry-points.md)): tabs, the other functions, weapon damage, crit chance and damage, DT both ways, spread, VATS AP cost, weapon wear. Left (§2 there): reload, equip and attack speed (need the clips' playback rate), knockdown, Strength requirement, aiming move speed, throwing velocity, explosion radius. | PERK | Most combat perks and traits (crit chance and damage, weapon damage, DT, spread, VATS, reload speed and others). |
| 3 | ~~Challenges never progress except scripted ones. Type, value1–3, SNAM/XNAM and rewards are ignored.~~ Built in claude/e3-challenges ([challenges](../spec-notes/challenges.md)); left: type 7 (use a skill, in no master), the completion sound, the Pip-Boy list's type headline. | CHAL | Kill, use and craft challenges, and their XP and perk rewards. |
| 4 | ~~Skill books cannot be read: DATA skill is ignored, and "Can't be Taken" is ignored too.~~ Built in claude/e4-skill-books ([skill-books](../spec-notes/skill-books.md)): read from the inventory (and EquipItem), fBookPerkBonus points with entry point 11 (Comprehension), the permanent pool, Books Read, a corner line; "Can't be Taken" books stay put. Left: books that teach nothing (OnEquip), the original's message wording. | BOOK (+ PERK EP 11) | Reading gives skill points (Comprehension), counts toward the "Books Read" stat, and the item is consumed. |
| 5 | ~~Death items are never added to corpses.~~ Built in claude/e5-death-items ([death-items](../spec-notes/death-items.md)): INAM through the Traits template, at the reference's level, as the actor dies; placed corpses owed theirs; perk entry point 21 Add Leveled List On Death. Left: JIP GetDeathItem/SetDeathItem. | NPC_/CREA INAM | Levelled loot on the body: creature parts, hides, glands. |
| 6 | ~~Vendors trade every item category, because AIDT services are ignored. The package flag "Offers Services" is stored only, and `GetOffersServicesNow` is missing (returns 0).~~ Built in claude/e6-vendor-services ([vendor-services](../spec-notes/vendor-services.md)): the barter menu and Find's Allow Buying sell only the vendor's service categories (AIDT, or the class's with Autocalc Service); GetOffersServicesNow follows Offers Services. Barter is not gated by the package (Chet's packages lack the flag). Left: Training and Recharge (unused in FNV per the wiki). | NPC_/CREA AIDT, PACK | Vendors deal only in their service categories, and only while their package allows it. |
| 7 | ~~The world gives no radiation: REFR XRAD is ignored. Water also does no harm: WATR XNAM, "Causes Damage" and DATA damage are ignored. Explosion radiation (EXPL radiation level and radius) is stored only.~~ Markers (REFR XRAD/XRDS) and WATR XNAM water rads built in #548; explosions' radiation zones (level, dissipation time, radius, fading) and WATR "Causes Damage" with DATA damage and IsInDangerousWater built in claude/e7-explosion-water-radiation ([radiation](../spec-notes/radiation.md) §2.1, §3.1). Left: NPCs taking rads (decided against, §4 there), zones in saves. | REFR, WATR, EXPL | Radiation hotspots, irradiated water, and nuke fallout. |
| 8 | ~~The weather never varies: the highest-chance WLST entry always wins (`nvforever/sky.rs:394`), and volatility is ignored. REGN RDWT region weathers are never applied.~~ Built in claude/e8-weather-roll ([world](../spec-notes/world.md) §6 "Choosing a weather"): weighted rolls from the climate's list, overriding regions' lists where the camera stands, volatility as a chance per game hour, live gating globals, seeded and saved. Left: the transition time (WTHR "trans delta" unit unknown, 0.5 h placeholder), the vanilla data has volatility 0 so the Mojave only varies by region. | CLMT, REGN | Weathers are a weighted roll per climate; a region's weather list overrides the climate. |
| 9 | ~~Weather has no sound and no precipitation (WTHR SNAM, precipitation MODL, lightning).~~ Built in claude/e9-weather-sound-precip ([world](../spec-notes/world.md) §6 "Weather sound, precipitation and lightning"): SNAM loops at the listener outdoors, MODL drawn about the camera, lightning flashes in the lightning colour with the weather's thunder after them, all within the DATA fade bytes. The masters hold no MODL and no looping sound in a used weather; Honest Hearts' two rain weathers flash and thunder. Zion rain and the Hidden Valley dust storm are drawn by their scripts and placed references. Left: the Snow class, sun damage/glare, wind moving anything, GetWeatherTraitNumeric and the JIP weather setters. | WTHR | Rain loops, wind, thunder and lightning, dust storms. |
| 10 | ~~Weapons have no scopes: MOD3, "Has Scope", "Scope from Mod" and EFSD are ignored.~~ Built in claude/e10-weapon-scopes ([weapon-scopes](../spec-notes/weapon-scopes.md)): the MOD3 picture at full zoom, Scope from Mod by Increase Zoom mods, night-vision scopes (NightScopeISFX, guessed), the sway from `scopewobble.nif` by the player's spread. EFSD stays unused, as the wiki says the original ignores it. Left: the scope in third person, the original's timing. | WEAP | A scope overlay when aiming, and night-vision scopes. |
| 11 | ~~Critical-hit effects are never applied (CRDT effect and the "On Death" flag are stored only).~~ Built in 8375a882 ([ash-piles](../spec-notes/ash-piles.md)): the critical effect after a critical hit's damage, On Death ones only when it kills. | WEAP | Disintegrate and goo kills from energy weapons; crit effects on unique weapons. |
| 12 | ~~Silencers keep the loud shot sound. WMS1/WMS2 are ignored; the mod changes only the detection sound level.~~ Built in claude/e12-silencer-sounds ([weapon-mods](../spec-notes/weapon-mods.md) §7): a Silence or Suppressor mod on the unit plays WMS1 (3D) / WMS2 (2D) for the player and NPCs; inherently silenced weapons already carry quiet SNAM/XNAM sounds; WEAP Shoot Dist built with it. Left: no mod looping sound exists, so a loop stays plain. | WEAP | Silenced shot sound. |
| 13 | ~~No spent casings or drained cells: DAT2 consumed_ammo and consumed_percentage are stored only.~~ Built in claude/e13-spent-casings ([spent-ammo-items](../spec-notes/spent-ammo-items.md)): one roll per player shot, perk entry point 57 (Hand Loader, Vigilant Recycler). Left: NPCs' shots (they carry no inventory ammo). | AMMO | Feeds the reloading-bench and recharger recipes. |
| 14 | ~~Holotapes never play: NOTE SNAM, and TNAM for Sound and Voice notes, are ignored.~~ Built in claude/e14-holotapes ([holotapes](../spec-notes/holotapes.md)): a Voice note used in the Pip-Boy's Notes list plays its topic's line (the one naming the speaker) in the speaker's voice, response after response, one tape at a time, again to stop; a Sound note plays its SOUN once (no master note has one). Left: subtitles, the JIP note functions, GetNoteRead, the original's panel. | NOTE | Audio logs. |
| 15 | ~~MGEF archetypes Paralysis and Turbo are not implemented. Cure Poison, Concussion and Dispel are missing too.~~ Built in claude/e15-mgef-archetypes ([magic-archetypes](../spec-notes/magic-archetypes.md)): Paralysis knocks the target down as a ragdoll while it lasts (GetKnockedState 1), Cure Paralysis, Cure Poison (spells of type Poison) and Dispel end effects, Concussion concusses on a crippled head (GetConcussed, `fConcussionTimer`, ImageSpaceConcussion), the player's Turbo runs the world at 0.5 with the player kept at full speed, the Paralyzing Palm in VATS. The masters use no Dispel, Cure Poison or Turbo archetype: Turbo is a Value Modifier of the actor value Turbo and Antivenom a script that dispels the poisons by name (both work now). Left: the Turbo factor and the concussion's rules are guesses; Cure Disease (no disease in this game's data). | MGEF | Paralyzing Palm, the Turbo chem's slow time, Antivenom. |
| 16 | ~~Difficulty does not scale damage: no `fDiffMultHP*` game setting is read.~~ Built in claude/e16-difficulty-damage ([difficulty](../spec-notes/difficulty.md)): a Difficulty row on the settings page (and SetGameDifficulty/GetGameDifficulty at the console) scales the player's hits by `fDiffMultHPByPC*` and hits on the player by `fDiffMultHPToPC*`, and kill XP by `fDiffMultXP*`. No master carries the ten HP settings, so their values are hypotheses (spec §3.6). | GMST | Damage dealt and taken per difficulty level. |
| 17 | ~~Quest target conditions are not tested, so every alternative marker shows at once.~~ Built in claude/e17-quest-target-conditions ([quests-dialogue](../spec-notes/quests-dialogue.md) §2): each target's conditions run with the target as subject, four times a second for the active quest; the Pip-Boy's maps and the console's ShowQuestTargets / MoveToQuestTarget mark only the targets that pass; "Compass Marker Ignores Locks" is read and picks the door a target indoors is marked at. Left: the compass draws no quest marker yet (hud.md §11); the rate and the one Run-on-Target item are guesses. | QUST QSTA + CTDA | Only targets whose conditions pass get a marker. |
| 18 | ~~Model texture swaps are missing: MODS/MO2S/MO3S/MO4S are not in the schema.~~ Built in claude/e18-texture-swaps ([texture-swaps](../spec-notes/texture-swaps.md)): in the schema (`AlternateTexture`, counted arrays and length-prefixed strings), 2,594 entries in the masters (STAT 474 records, ARMO, DOOR, HDPT, ACTI, TERM, ARMA, WEAP, ALCH; no AMMO), the 3D Index is the depth-first geometry position; applied to placed and dropped objects, worn armour, head parts and held weapons (first person through the WNAM STAT's list). Left: ARMA (not drawn), what an empty TXST slot and a stale shape name do in the original. | WEAP, ARMO, AMMO, STAT… → TXST | Re-skinned variants of shared meshes. |

## Medium impact (grouped)

- **AI and packages.** ~~Several PACK fallout behaviours are ignored: Observe Combat, Reaction
  To Player Actions, Allow Idle Chatter, Friendly Fire Comments, Avoid Radiation. Also
  ignored: the package CNAM combat-style override, the begin/end/change TNAM topics, Guard
  "Remain Near Reference", and stay-at packages that place by Object ID, Object Type or
  "at package location" (`fnv-ai/goal.rs:467`). The patrol point's INAM idle, TNAM topic
  and script are not run; only the wait time is. Placed references ignore XIBS (Ignored
  By Sandbox) and header bit 25 No AI Acquire, on both items and actors. DOOR Minimal Use
  is stored but never reaches pathing. CREA "Can't Open Doors" is ignored (rated M–H), so
  animals path through doors. AIDT responsibility is never used by crime reactions.
  Condition Run On "Combat Target" is evaluated with no subject (`game.rs:2918`).~~
  Built in claude/em1-ai-packages ([packages](../spec-notes/packages.md) §7): CREA "Can't
  Open Doors", Friendly Fire Comments, package CNAM combat style, Begin/End/Change topics,
  patrol points' topic and result script, Run On "Combat Target" (the creatures are kept from
  load doors; hinged doors are left as they were). Left, with the reason
  (§7.7 there): Observe Combat, Reaction To Player Actions and Idle Chatter (no such lines
  said yet), Avoid Radiation (needs radiation on the nav graph), Guard "Remain Near
  Reference" and stay-at by Object ID / Object Type / package location (undocumented),
  patrol INAM (in no master, undocumented), XIBS (the wiki: "appears not to work in New
  Vegas"), No AI Acquire (no actor picks items up), DOOR Minimal Use (undocumented), AIDT
  responsibility in crimes (undocumented).
- ~~**Combat style (CSTY).** Dodge chance, directions and timers; rushing attacks; "Ignore
  Damaging Self/Group/Spectators"; "Ignore Threats"; "Melee Alert OK"; max targeting FOV
  and combat radius (stored only).~~ Built in claude/em2-combat-style
  ([combat-ai](../design/combat-ai.md) §17): the maneuver decision (dodges and idles), rushing
  attacks, the power attack fatigue modifier, Combat Radius, Max Targeting FOV, the three
  Ignore Damaging flags with iAvoidHurtingNonTargetsResponsibility, and running from live
  grenades unless Ignore Threats (with the AvoidThreat line). Melee Alert OK and Cannot Use
  Stealthboy change nothing (§17.8). Left: Acrobatic Dodge (undocumented), threats out of
  combat, mines and exploding cars as threats, AvoidThreatResponse.
- ~~**NPC weapon choice.** WEAP "Player Only" and "Not Used In Normal Combat" are ignored.
  The DNAM fields min_range, aim_arc, resist_type and kill_impulse/impulse_dist are stored
  only. Four weapon-mod effects do nothing: Regenerate Ammo, Decrease Equip Time, Increase
  Projectile Speed, VATS Bonus. AMMO "Ignores Normal Weapon Resistance" is not checked.~~
  Built in claude/em3-npc-weapon-choice ([weapon-data-fields](../spec-notes/weapon-data-fields.md)):
  Player Only and Not Used In Normal Combat kept out of NPCs' weapon choice; Range - Fixed;
  the damage type (Resist) cut by the target's resistance in place of DR; Aim Arc as the
  firing angle; Increase Projectile Speed and both Regenerate Ammo effects. Nothing to
  build: AMMO "Ignores…" ("no longer used"), VATS Bonus ("unused in game"), Min Range.
  Left: Decrease Equip Time (no master mod), Kill Impulse / Impulse Dist (meaning not
  documented).
- ~~**Actor flags (ACBS).** "No VATS Melee", "No Blood Spray/Decal", CREA "Is Ghost" and
  "Allow PC Dialogue" (the engine guesses from greetings) are ignored. NPC_ NAM6 Height
  is stored but never applied to the drawn scale. LVLN/LVLC chance-none is not applied in
  fnv-game. CLAS is never decoded (tag skills, attributes, the Guard flag); the effect on
  auto-calculated NPCs is a hypothesis. FACT "Special Combat" and the XNAM modifier are
  unused. VTYP "Allow Default Dialog" is not parsed.~~ Built in claude/em4-actor-flags
  ([actor-flags](../spec-notes/actor-flags.md)): No VATS Melee, No Blood Spray/Decal,
  Allow PC Dialogue, NAM6 Height, chance none in both actor resolvers (all 0 in the
  master), Special Combat (no alarm), Allow Default Dialog. Not applied: "Is Ghost" (124
  creatures, wild dogs among them, that are killed in normal play). Nothing to build:
  CLAS (auto-calc values are stored). XNAM was already used (faction reactions). Left:
  Special Combat's fSneakSeenMin / iCombatTargetPlayerSoftCap part.
- ~~**Cells and references.** CELL "Can't Wait" is ignored by the Wait menu. "Off Limits"
  is stored only. "Behave Like Exterior" and XCCM are ignored. REFR "Inaccessible" (header
  bit 8) is ignored. Activate parents (XAPD/XAPR: buttons that drive doors and traps) and
  activation prompts are ignored, both the base ACTI XATO and the REFR XATO. "Leveled Lock"
  is stored only. TERM "Leveled" is stored only.~~ Built in claude/em5-cells-refs
  ([cells-refs](../spec-notes/cells-refs.md)): Can't Wait (9 cells), Inaccessible doors (85),
  activation prompts (156 references, 59 activators). Activate parents were built in #547.
  Nothing to build: Behave Like Exterior and XCCM (no master cell uses them), Leveled Lock
  (none of 426 locks), Off Limits (one cell; no documented effect), TERM Leveled (no rule
  documented).
- ~~**Sound.** Several sounds never play: MSTT SNAM, STAT RNAM and BRUS (bush rustle), LIGH
  SNAM hum, TERM SNAM, ADDN SNAM, the PROJ in-flight sound, WEAP Shoot Dist, NAM6 Block and
  UNAM Idle, WATR SNAM, and MGEF effect sounds. SOUN ignores its attenuation curve, loop
  points, the 2D flag, Mute When Submerged and Start at Random Position.~~ Built in
  claude/em6-sound ([audio](../spec-notes/audio.md) §15): MSTT, STAT RNAM, LIGH, TERM, ADDN,
  WATR, PROJ, NAM6, UNAM, the MGEF Hit and Effect sounds, the 2D flag, Start at Random
  Position, Mute When Submerged. Left, with reasons in §15.3: BRUS (no index → sound table),
  the attenuation curve and loop points (units undocumented), the MGEF Bolt and Area sounds,
  and the rest of this item: ASPC has no crowd walla
  (its reverb environment is played since claude/q27-reverb). ALOC location and retrigger delays are stored
  only. ARMO "Modulates Voice" is ignored. TACT "No Voice Filter" (header bit 13) is not
  in the schema, and no radio or intercom voice filter exists.
- ~~**Detection.** The sound levels on EXPL, PROJ VNAM, IPCT and CREA NAM5 are unused; only
  the weapon's sound level feeds detection.~~ Built in claude/em7-detection
  ([detection](../design/detection.md) §8), with the sun's shade in the light measure.
- **Visuals** (claude/em8-visuals: built the drinkable activators, ACTI WNAM, which draw
  no water (activators.md §9); weather Trans Delta and Sun Glare and the interior clip
  distance (world.md §6); nothing to build where no master record uses the field:
  projectile lights, EXPL INAM, negative, spot and spot-shadow lights; left: destruction
  debris (gore pieces belong to actors; 12 containers' stages name debris, the 51
  creatures' stages are not applied), the falloff exponent and fog power (meaning not
  documented), HDR and eye adaptation, the IDLE body section (the clips' track
  priorities already lay an upper-body idle over the legs)). Was: Projectiles carry no light (PROJ DATA light). EXPL INAM placed impact
  objects are not spawned. Destruction-stage DEBR debris is never spawned. ACTI WNAM water
  activators draw as plain models. Negative lights are skipped instead of darkening, and
  LIGH Spot Shadow and falloff exponent are not applied. EFSH holes and add-on models are
  not drawn. Fog power and clip distance (WTHR, XCLL, LGTM) are stored only, so all fog is
  linear. There is no HDR or bloom pass, so the IMGS HDR block and the IMAD HDR and bloom
  tracks are unused. WTHR transition speed and sun glare are hard-coded. IDLE animation
  group section (upper body or whole body) is stored only.
- **Unparsed record types** (claude/em9-records, [default-objects](../spec-notes/default-objects.md): all four now in the schema; DOBJ read, its Death Music and Drug Wears Off Image Space used; REGN RDSD region sounds played; CAMS/CPTH (no kill camera) and RGDL (no feedback or pose matching) decoded only, RGDL's RAFD/RAFB/RAPS/ANAM left raw). Was: CAMS/CPTH (kill, VATS and death cameras); DOBJ (default
  objects: death and level-up music, the unarmed power-attack spells and others; death
  music is found by the hard-coded EDID `MUSDeath`); RGDL (ragdoll feedback and pose
  matching); REGN RDSD region sounds (no schema; ambient one-shots are not built); CSNO
  slot-machine data (slots are a stub).

## Low impact (short list)

Ignored unless noted:

- **Items** (claude/em10-items, [item-fields](../spec-notes/item-fields.md): built No Jam After
  Reload, Jam Time, the weapon's rumble and the MISC/KEYM own sound; the rest left with
  reasons there). Was: WEAP burst-length flags; "No Jam After Reload"; iron-sights animation flags;
  "Hide Backpack"; reload and jam times; rumble; EAMT. AMMO QNAM abbreviation (stored).
  ARMO female world model (MOD4) and female icons; BNAM. KEYM/MISC RNAM loop. CMNY value.
  The CSNO chip models are hard-coded, and only the last ICO2 deck back is kept.
- **Actors** (claude/em11-actors, [actor-fields](../spec-notes/actor-fields.md): built Not Pushable,
  No Rotating To Head-track, No Head/Arm and the CREA CNAM impact set; nothing to build
  for RACE boosts, HAIR Fixed, LVLI owners, No Shadow; the rest left with reasons). Was: NPC_ AIDT mood (stored); "Not Pushable"; "No Rotating To Head-track";
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
- **Objects** (claude/em14-objects, [object-fields](../spec-notes/object-fields.md): built PROJ Can Be
  Disabled; ARMA addons (799e5a1c) and EXPL Knock Down were built before; the rest left with
  counts and reasons). Was: Header bits Visible When Distant, On Local Map, Obstacle and Navmesh
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
