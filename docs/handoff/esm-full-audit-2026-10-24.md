# ESM data audit, every field (2026-10-24)

> **Superseded for status (2026-10-25):** 25 of the 30 gaps below have been built since;
> what is still open is in [esm-audit-2026-10-25](esm-audit-2026-10-25.md).

The owner asked for an audit of all base-game ESM data, like the NIF audit
([nif-node-audit](../spec-notes/nif-node-audit.md)): which record types, subrecords, fields
and flag bits carry data that the engine does not use, and which engine logic stands in for
ESM data with stubs or hard-coded values. The earlier audits
([esm-gap-audit-2026-10-21](esm-gap-audit-2026-10-21.md),
[gap-audit-2026-10-23](gap-audit-2026-10-23.md)) worked from the schema and the code. This
one starts from the data: every value the ten masters actually set. Snapshot: origin/main
9bb10a60. Tags as in CLAUDE.md.

## 1. Method

- **New tool.** `fnvtool esm fields <data> <plugins…> --schema schemas/fnv`
  (`fnv.esm.fields/1`, docs/cli-contract.md) decodes every record of a load order through
  the schema. For each path (`WEAP/DNAM/min_range`, arrays as `[]`) it counts how often the
  field is present and how often it is non-zero, keeps its most common values and which
  plugins set it, and counts each flag bit. Run over FalloutNV.esm, the four story add-ons,
  GunRunnersArsenal.esm and the four packs: 107 record types, 1,168 (record, subrecord)
  pairs, 3,325 field paths, about 1.5 minutes [observed]. The JSON dump stays in the
  session scratchpad (game-derived, not committed).
- **Name search.** Engine consumers read decoded values by schema name (`f(dn,
  "min_range")`, `has_flag(f, "No Specular Map")`, `first(d, b"DNAM")`). A small lexer
  collected every string literal in `crates/` and `engine/`, leaving out `fnv-esm`,
  `fnvtool`, tests and `#[cfg(test)]` items. Every field, flag bit and subrecord that
  carries data and whose name no literal matches became a candidate. Counts: 155
  subrecords, 407 fields, 234 named flag bits and 71 flag bits the schema has no name for
  (867 in all).
- **Verification.** Four reviewers read the code for every candidate, in four groups:
  items, effects, actors and logic, world. Each candidate got a verdict: **used** (read by
  byte offset, through a bit mask or under a name built at run time; about 70 of the
  candidates), **stored only** (read into a struct that no behaviour uses), **ignored** or
  **editor-only**. They also spot-checked the fields the search marked as used for "stored
  only", and searched each record's consumers for stubs and hard-coded values. Rows the
  earlier audits settled are marked "settled" and are not argued again.
- **Ranks**: **H** a player notices it in normal play, **M** it shows in places or in sound
  and visuals, **L** minor or rare, **mod** every master leaves it zero.

**Verdict totals** (one per gap row): ignored about 155, stored only about 34, read from
the wrong bit 2, editor-only about 44. By rank: H 2, M 27, the rest L or mod.

**Record types the engine never loads**: CAMS and CPTH (kill and death cameras), RGDL
(ragdoll feedback and pose matching; decoded only), LGTM (lighting templates: read through
CELL LTMP, so nothing is lost [observed: lighting.rs]), LSCT (loading-screen layout;
NVForever's own design).

## 2. Ranked gaps

| # | Gap | Records | In masters | Engine | Rank |
|---|---|---|---|---|---|
| 1 | **The creature flag bits 28–31 are named one bit off in the schema.** `schemas/fnv/gameplay.json` follows an xEdit list that names bit 27 twice ("Not Pushable / Allow Pickpocket"), so every later name moves down one. The GECK wiki's ActorHasBaseFlag table gives 27 Not Pushable, 28 Allow Pickpocket, 29 Is Ghost, 30 No Rotating To Head-track, 31 Invulnerable [documented: owner's export, functions.xml]. The data agrees: bit 31 is set on Mr. House, VHDYesMan, Festus, ARCHIMEDES, the Dead Money holograms and the OWB brain tank [observed]. As a result `data.rs:2561` makes the three tumbleweeds and CG02Andy invulnerable, and leaves the 34 creatures that should be invulnerable open to damage. Is Ghost (bit 29, 14 creatures) is read as No Rotating To Head-track (`data.rs:2572`). | CREA ACBS | 34 / 14 / 4 / 136 | read from the wrong bit | **M** |
| 2 | About 35 **perk entry points** have no consumer. Among them: Mysterious Stranger and Miss Fortune (28, 41), Grim Reaper's Sprint and Nerves of Steel (35, 39), Light Step (4), Silent Running, Computer Whiz, Infiltrator, Pack Rat and Hoarder, Long Haul, run speed, equip and attack speed (38, 43), Super Slam, the unarmed moves (61–67), Splash Damage, Meltdown, Living Anatomy, Jury Rigging, Better Healing. This matches perk-audit-2026-10-23; nothing has been wired since. | PERK | 259 perks | ignored | **H** |
| 3 | **Area effects are not built.** EFIT `area` is read but never used (`fnv-game data.rs:2705`, `fnv-combat data.rs:1155`), and `cast_immediate_on_self` only affects the caster. Lost: Glowing One and critter radiation auras, the Legion Explorer aura, Rex's sonar, the EvaluatePackage pulses and the knockdown spells. The 24 actor-effect SPLO entries are never cast either (radiation.md §8). | SPEL, ENCH | 10 spells | stored only | **M** |
| 4 | **NPC Speed Mult does not change movement.** `creature_pace.rs:141` applies it to creatures only. 233 NPCs differ from 100: Legion raiders 130–150, White Legs 125, Lanius 180 [observed]. | NPC_ ACBS | 233 | stored only (for movement) | **M** |
| 5 | **Thrown weapons cannot be picked up again.** PROJ "Can Be Picked Up" is not read (`forms.rs:310` reads Can Be Disabled only) [documented: GECK wiki Projectile]. Affects the throwing knife, hatchet, spear and tomahawk. | PROJ | 19 | ignored | **M** |
| 6 | **Pickpocketing is not built**: Allow Pickpocket (gap 1), `fPickPocket*`, GetPlantedExplosive. | CREA, GMST | 136 | ignored | **M** |
| 7 | **Sound reverb send and loudness curve.** SOUN Reverb Attenuation Control is not read (`reverb.rs:312` sends every sound fully) [documented: GECK wiki Sound: 100 means no reverb]. The five-point attenuation curve is replaced by the engine's own min÷distance rule (`sfx.rs:16`); the wiki gives it as volume % from the inner to the outer radius. audio.md §15.3 called both undocumented, but the local export documents them. | SOUN | 2,352 / 3,540 | ignored | **M** |
| 8 | **The HDR tracks of scripted image-space modifiers are not read** (`image_modifier.rs:279-280`, "not read yet"). Lost: the flash-bang whiteout, the eye adaptation on entering caves, bright-scale fades. IMGS is mostly stored only too (clamps, luminance ramps, grass and tree dimmers, Eye Adapt Speed; `eye_adapt.rs:121` uses a fixed 3 EV/s). | IMAD, IMGS | 134 / 91 | ignored / stored only | **M** |
| 9 | **IMAD RDSD/RDSI sounds** are not in the schema and never play: poison sting, Stealth Suit on/off, Battle Brew heartbeat, cave wind. That they are the start and end sounds is a [hypothesis]. | IMAD | 21 | ignored | **M** |
| 10 | **No VATS kill cameras or death cameras**: CAMS/CPTH are never loaded. | CAMS, CPTH | 276 / 418 | ignored | **M** |
| 11 | **Special VATS attacks.** WEAP `VATS` (AP, damage multiplier, Mod Required) and VANM (attack name, e.g. Machete "Back Slash") are not read. VATS here is NVForever's own design (vats.rs:1). | WEAP | 88 / 86 | ignored | **M** |
| 12 | **Power-attack clip override** (DNAM `power_attack_animation_override`) is not read; power attacks pick their clip by direction only. | WEAP | 105 | ignored | **M** |
| 13 | **Third-person grip** (DNAM `grip_animation`, HandGrip1–3) is not read; only the Land Mine's grip is hard-coded (`player_body.rs:1395`). | WEAP | 32 | ignored | **M** |
| 14 | **Embedded-weapon node** (WEAP NNAM) is not read. Shots from creatures without a ProjectileNode leave from the eyes (`npc_combat.rs:47`). | WEAP | 103 | ignored | **M** |
| 15 | **Power armour voice filter** (ARMO "Modulates Voice"; TACT has its own "No Voice Filter" flag). No voice filter exists. | ARMO | 44 | ignored | **M** |
| 16 | **Map-marker reputation** (REFR WMI1) is never shown on world-map hover [documented: GECK wiki Reference]. | REFR | 36 | ignored | **M** |
| 17 | **Bullet whiz** (PROJ Supersonic: the sound comes from the point of the path nearest the listener) [documented]. | PROJ | 48 | ignored | **M** |
| 18 | **The Pip-Boy light record** (MGEF light → PipLight) is not used; the light is a design cone (`flashlight.rs`, owner request). The critter radiation glow light is unused too. | MGEF | 2 | ignored | **M** |
| 19 | **Four package behaviours** are ignored: Observe Combat, Reaction To Player Actions, Allow Idle Chatter, Avoid Radiation (settled in packages.md §7.7). | PACK | 806 / 474 / 529 / 675 | ignored | **M** |
| 20 | **Next Quest** (QUST NAM0) is stored only (`quest.rs:33`). The wiki says that quest becomes the active one on completion [documented: Quest Stages]; the Pip-Boy picks the newest running quest instead. | QUST | 9 | stored only | L |
| 21 | **NPCs may not drop their weapon on death**: `iDeathDropWeaponChance` (100) is not read and no death drop was found [unverified]. | GMST | 1 | ignored | L–M |
| 22 | **Header bit 25 Obstacle** (cuts the navmesh at run time [documented: GECK wiki]) is not used; fnv-nav has no dynamic cutting. object-fields.md §4 gave LOD as the reason; the reason is wrong. | STAT, MSTT, ACTI, SCOL | 307 / 24 / 20 / 7 | ignored | L |
| 23 | **NPC fall damage everywhere**: WRLD DATA bit 6 "Don't Allow NPC Fall Damage" (Dead Money's West Town N/S) is not read (`falls.rs:18` says no such setting exists). | WRLD | 2 | ignored | L |
| 24 | **Leveled locks.** REFR XLOC "Leveled Lock" is stored only. cells-refs.md says none of 426 locks use it; that holds for FalloutNV.esm only. Dead Money has 22, OWB 3, HH 1. | REFR | 26 | stored only | L |
| 25 | **Main-menu screens.** LSCR "Displays In Main Menu" is not read: `main_menu.rs:257` uses the 196 screens with no location, not the 33 flagged ones. | LSCR | 33 | ignored | L |
| 26 | **Hidden From Local Map** (REFR header bit 6) is not read; `pipboy/data/read.rs:168` still lists 4 Dead Money teleport doors and ignores whether a door is enabled. | REFR | 17 | ignored | L |
| 27 | **Water depth falloff** (shoreline alpha, [documented: GECK wiki WaterType]) is replaced by a 200-unit ramp tuned by eye. Also unused: distortion, light radius and brightness; ANAM opacity and sun power are stored only. | WATR | 74 | ignored | L |
| 28 | **Explosions are always upright**: EXPL Always Uses World Orientation (aligned with the ground) is not read (`shot_fx.rs:43`). | EXPL | 112 | ignored | L |
| 29 | **"Hide Backpack"** is not read. item-fields.md §1 says there is nothing to hide, but 5 ARMO and 7 ARMA use the Backpack slot. | WEAP | 16 | ignored | L |
| 30 | **Small flags, all L.** XTEL No Alarm (5 doors), DOOR Hidden (3; object-fields.md says none), TXST No Specular Map on texture swaps (12), PROJ Pins Limbs (5) and rotation (15), IPCT angle threshold, BPTD To Hit Chance and IK data, MGEF Painless / No Hit Effect / Gory Visuals, the MGEF display flags (No Duration, No Magnitude, Display Name Only: how the effect line reads), ENCH Hide Effect, IDLE No Attacking, FACT Hidden and Allow Sell, DIAL TDUM, INFO Low/High Intelligence and Speech Challenge, GLOB Constant, MESG Auto Display, the MICO message icons on every item type, WEAP Minor Crime and Long Burst, the embedded weapon's actor value, Don't Use 1st/3rd IS Animations, Sight Usage (replaced by the owner's hip-fire rule), CSNO winnings quest, the TERM result-script Enabled flag, CREA Flies, Swims, Walks and Tilt, RACE default hair colour and weights. | — | — | ignored / stored only | L |

## 3. Stubs and hard-coded stand-ins for ESM data

Found in the consumers of these records (file:line on 9bb10a60):

- `crates/fnv-game/src/levelup.rs:36`: the skill points per level are hard-coded, and
  `iLevelUpSkillPointsBase` / `iLevelUpSkillPointsInterval` are not read. **mod**
- `crates/fnv-sim/src/zones.rs:69`: the leveled actor percentages 50/75/125/200 are
  hard-coded, not read from `fLeveledActorMult*`. **mod**
- `crates/fnv-ai/src/goal.rs:227`: the flee distance is 2048, while the masters'
  `fCombatFleeNormalDistance` is 1400; `fFleeDistanceExterior/Interior` are not read. **L**
- Of the masters' 649 numeric game settings, 334 are named nowhere in the engine. Most
  belong to systems that are not built (VATS camera, auto-aim, Action Point costs, the
  condition tables for rate of fire and spread). Where the engine does read a setting, its
  fallback matches vanilla.
- Found by EDID or fixed FormID instead of DOBJ: `MUSDeath` (`music.rs:274`),
  `NightScopeISFX` (`scope_view.rs:49`), `PARALYZING_PALM` (`magic_archetypes.rs:46`,
  should be DOBJ place 5), `IMAGE_SPACE_CONCUSSION` 0x164, the companion wheel's Stimpak
  (`companion_wheel.rs:264`), the Pip-Boy hint items (`read.rs:157`), and
  `HeadCrippleEffect` given to every NPC (`abilities.rs:51`). Of DOBJ's 31 entries, only
  11, 13 and 20 are read.
- Design values or values tuned by eye where the ESM has a field: `sky.rs:99` (cloud
  scroll, a "placeholder rate" for WTHR ONAM), `water.rs` / `ripples.rs` (shoreline and
  ripple constants), `tree.rs:149`, `eye_adapt.rs:121` (3 EV/s), `shot_fx.rs:156-157`
  (explosion and explosion-light times), `fx.rs:528` (collision material → impact
  material, a hypothesis).
- `fnv-world/src/lighting.rs` `emittance()`: takes a region's highest-chance weather, not
  the weather it is showing now.
- `nvf-play/src/main_menu.rs:257`, `falls.rs:18`, `pipboy/data/read.rs:168`: gaps 23, 25
  and 26 above.
- The Land Mine grip (`player_body.rs:1395`), the blackjack chip models
  (`blackjack_view.rs:51`), and `HIP_FIRE_WITHIN` (`nvf-play/src/combat.rs:437`, the
  owner's rule in place of Sight Usage).
- Script functions that are still stubs: SetVatsTarget, GetVATSTargetHeight,
  GetPlantedExplosive (`functions/mod.rs:520-523`); no `Cast` (only mods would call it).
- Corpses that do not respawn are never removed at a cell reset (`respawn.rs:31`), so
  NPC_ header bit 10 (Quest Item: the body is kept) has nothing to act on.

## 4. Mod risk: zero in every master and not read

ESPs can set these, and NVForever would ignore them:

- **Items:** the LVLI and CONT entry owner global or rank (COED); ARMA value, weight and DT; ALCH and INGR EFIT area.
- **Actors and logic:** barter gold, training level and teaches on NPC_, CREA and CLAS; RACE skill boosts; ECZN owner and rank; LVLO count > 1 and "Calculate for each item"; GLOB Constant and type; QUST "Allow repeated topics"; INFO and DIAL order (PNAM, INOM).
- **Effects:**
  - MGEF: cost, counter effects, area sound, the Hidden bit.
  - SPEL: cost and level.
  - Other records: ENCH EFIT range, PROJ light, EFSH Affect Skin Only and the z-test and blend variants, ADDN particle cap, BPTD gore rotation.
  - Audio: ALOC NAM2–NAM4 and FNAM, the ASPC walla sound, MSET DATA, SOUN SNDX start/stop time.
  - Other: IMAD IIAD/JIAD, DOBJ's Every Actor Ability and unarmed attacks.
- **World:** WATR related waters (GNAM) and material (MNAM), the WRLD canopy shadow (NNAM) and No LOD Noise, REFR FNAM "Show All Hidden", XORD, the patrol INAM idle, MSTT debris, DOOR Automatic, LIGH value and weight.
- **Record types:** a record type FNV accepts but no master uses (the placed projectile types PMIS, PBEA, PFLA, PCBE) loads without error but is never placed.

## 5. Not decoded by the schema

These subrecords show as "unknown" in `esm coverage`. Most have hand-written readers:

- **Read by hand:** NAVM and NAVI (fnv-nav); PERK DATA and EPFD; RCPE; AVIF ANAM; CDCK; DEBR; RADS, DEHY, HUNG and SLPD; VTYP DNAM; CREA NIFT (editor hashes).
- **Not read by anything:** IMAD RDSD and RDSI (gap 9; played since #671), REGN RDOT (RDMP
  is read, `pipboy/data/read.rs`), RGDL RAFB, RAFD, RAPS and ANAM, LSCT DATA, INGR (one record).
- **Fails to decode:** QUST DATA, on 2 records [observed: `esm coverage`].

## 6. Checked, no gaps

BOOK, NOTE, CHIP, CCRD, CDCK, INGR, RCPE, RCCT, LVLI, LVLN, LVLC, FLST, REPU, CHAL, AVIF,
RADS, DEHY, HUNG, SLPD, HAIR, EYES, HDPT, SCPT, IDLM, VTYP, ANIO, ECZN, ACHR, ACRE, IPDS,
AMEF, CSTY, ASPC, MUSC, MICN, SCOL, CLMT, LGTM (through CELL), GRAS. MISC, KEYM, IMOD and
CMNY lack only their message icons. Placed references are fully read apart from the rows
above: activate and enable parents, locks, teleports, ownership, counts, radiation,
emittance, linked references, encounter zones, primitives, audio markers, radio, patrols.

## 7. Earlier notes this audit corrects

- cells-refs.md: "Leveled Lock: none of 426" holds for FalloutNV.esm only (gap 24).
- object-fields.md §3: DOOR Hidden is set on 3 doors. §4: Obstacle is about navmesh
  cutting, not LOD (gap 22).
- item-fields.md §1: Hide Backpack has something to hide (gap 29).
- audio.md §15.3: the SOUN attenuation curve and reverb control are documented in the
  local GECK wiki export (gap 7).
- actor-flags.md §3: "Is Ghost" was looked for on the wrong bit (gap 1).
- `fnv-sim/src/refs.rs:25-29` tags Persistent and Initially Disabled as hypotheses, but
  the GECK wiki documents both.
