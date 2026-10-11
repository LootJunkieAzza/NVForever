# Spec notes: music, sound markers and background loops

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525), **[hypothesis]**
needs verification, **[design]** is our choice where the game's rule is not known.
Counts come from `fnvtool world audio` (fnv.world.audio/1, §8) over FalloutNV.esm alone.

Code: `fnv_world::audio` (records, markers, the choice of music at a place),
`nvf-play` `music.rs` (playback of music) and `ambient.rs` (placed sounds, background
loops). Related: combat-rules.md §16 (SOUN records and how a sound effect is played),
voice.md (dialogue voice).

## 1. The records

Layouts [documented: xEdit `wbDefinitionsFNV.pas`, branch dev-4.1.6; `schemas/fnv/audio.json`];
all four record types decode over FalloutNV.esm with no bytes left over [observed:
`fnvtool esm coverage`].

- **MUSC, music type** (20): `FNAM` a file or a folder, `ANAM` a float xEdit names
  "dB (positive = Loop)". 13 name a file, 6 a folder, `1NoMusic` nothing [observed].
- **MSET, media set** (140: 81 location, 32 battle, 23 dungeon, 4 incidental): `NAM1`
  the type; `NAM2`–`NAM7` six file names; `NAM8`, `NAM9`, `NAM0`, `ANAM`, `BNAM`, `CNAM`
  a decibel value for each; `JNAM`–`ONAM` a "boundary %" for each; `PNAM` one enable bit
  per file; `DNAM`, `ENAM`, `FNAM`, `GNAM` times; `HNAM`, `INAM` an intro and an outro
  sound (SOUN). For a location set the six files are day outer, middle, inner and night
  outer, middle, inner, and the times are "minimum time on", "looping/random crossfade
  overlap" and "layer crossfade time"; a battle set uses the first file as its loop; a
  dungeon set the first three as battle, explore and suspense [documented: the field
  names].
- **ALOC, media location controller** (89): `NAM5` and `NAM6` the day's and the night's
  start (the stored number over 256 is the fraction of a day [documented: xEdit
  `wbAlocTime`]; 64 and 255, i.e. 06:00 and 23:54, on the ones looked at [observed]);
  `NAM4` location delay, `NAM7` retrigger delay; lists of media sets: `HNAM` neutral,
  `ZNAM` ally, `XNAM` friend, `YNAM` enemy, `LNAM` location, `GNAM` battle; `RNAM` a
  "conditional faction". `NAM1` (flags), `NAM2`, `NAM3` and `FNAM` are not understood.
  83 have location sets (29 more than one), 8 a faction (4 of them location sets too),
  2 no set at all (one is `musCtrlAAAntiMusic`) [observed].
- **ASPC, acoustic space** (113): five `SNAM` in a row, by position the looping sound of
  dawn (or the default loop), afternoon, dusk and night, and a "walla" sound; `WNAM`
  walla trigger count; `RDAT` "use sound from region (interiors only)"; `ANAM` a reverb
  environment by name; `INAM` is interior.
- **On cells and worldspaces**: CELL `XCMO` a music type (297 cells), CELL `XCAS` an
  acoustic space (17,485 cells), WRLD `ZNAM` a music type (3 worldspaces).
- **On references**: the audio marker data `MMRK` (empty), `FULL` a location name,
  `CNAM` the controller, `BNAM` bit 0 "use controller values", `MNAM` and `NNAM` "layer
  2 / layer 3 trigger %"; `XRDS` the radius.

## 2. The files

- **Music** is 199 loose MP3 files under `Data\Music` (`LOC`, `BTTL`, `DNGN`, `OLD`,
  `SCR`, `Special`, `Fallout1and2`) [observed]. Record paths are relative to `music\`.
- The three files of a location set's day (and of its night) have the same size to the
  byte in the folders looked at (`LOC\CaesarsLegion`, `LOC\CorporateRuins`) and are
  named `…_1Low`, `…_2Mid`, `…_3High` [observed]: three stems of one piece.
- The folders the default music types name (`explore\`, `public\`, `dungeon\`, `base\`,
  `battle\`) are not in the install [observed].

## 3. Audio markers

- 572 references carry audio marker data, 363 of them in interiors. Every one has a
  controller and a radius (1,000 to 18,000 units); none is initially disabled [observed].
- **Every marker stores 0.33 and 0.67 as its layer 2 and layer 3 triggers**, whether it
  uses its controller's values (426) or not (146) [observed], so the markers' own
  values cannot be told from a default.
- Goodsprings: one marker (00152433) at −70849, 3430, 9501 with radius 14,000 and the
  controller `musCtrlGoodsprings`, whose neutral, ally and friend lists all hold
  `musSetLOCDesExplBothABC` and whose enemy list holds a dungeon set [observed].
- **Cell music types do not silence markers** [observed, indirectly]: 264 of the 372
  cells that hold audio markers also name a music type, 159 cells name `1NoMusic`, and
  the other types' folders are not shipped. Reading: the music type is the older system
  and the markers are what New Vegas plays [hypothesis].

## 4. What plays where (as built)

1. **The marker** [design]: of the enabled markers whose radius reaches the player, the
   one the player is deepest inside (smallest distance over radius); the lower FormID on
   a tie. How the game ranks overlapping markers is not known.
2. **The set** [design]: a controller with a conditional faction plays its neutral sets
   (the player's standing with the faction is taken as neutral), any other its location
   sets. One of the list is picked at random on entering the marker and kept while
   inside. A controller without a set is silence.
3. **Day or night** by the controller's two hours.
4. **Layers** [hypothesis]: the outer file sounds within its boundary percentage of the
   radius (100 on the sets looked at), the middle and inner ones are added inside
   theirs (60 and 20 for the Goodsprings set; 66.6 and 33.3 on others). A marker that
   does not use its controller's values gives the two inner boundaries itself, read as
   the fraction of the way in from the edge (0.33 and 0.67, i.e. 67 and 33 percent of
   the radius). That the layers add up, rather than replace one another, rests on the
   files being stems of equal length (§2).
5. **Levels and fades**: each file at 10^(dB ÷ 20). A layer fades in or out over the
   set's layer crossfade time (9 s on the Goodsprings set) [hypothesis: the unit is
   seconds]; a change of set, or between day and night, fades the old out and the new in
   over the same time [design].
6. **No marker**: the cell's music type, else the worldspace's, if its file exists; as
   one track at 10^(−|ANAM| ÷ 20), looping when `ANAM` is positive [hypothesis].

Not built: battle, dungeon and incidental sets; the ally, friend and enemy lists; the
location and retrigger delays and the minimum time on; intro and outro sounds; the
`PlayMusic` script function; menu and death music.

## 5. Sound markers

- A reference whose base is a SOUN. 1,178 of them (706 in interiors) place 311 sounds
  [observed]. 607 have the Loop flag, 550 Play At Random, 21 neither; 151 Random
  Location; 54 are initially disabled and 91 have an enable parent.
- **Play At Random sounds name a folder** and have a Random Chance (`RNAM`) of 1 to 60
  percent, 4 on 220 of them [observed]. As built: once a second a marker in reach that
  is not already sounding is rolled against its chance [hypothesis: the chance is per
  second].
- **Hours**: SOUN `SNDD` start and stop time, the stored byte over 256 as the fraction
  of a day. 1,150 placed sounds store 0 and 0; 21 store about 20:00 or 21:00 to 06:00,
  one 08:15 to 17:20 [observed]. As built: a sound with different start and stop sounds
  only between them.
- **Reach and loudness** [design]: a marker sounds within its sound's maximum
  attenuation distance, at the loudness combat-rules.md §16 gives a sound at that
  distance.
- **Ogg under a `.wav` name** [observed]: the base sound archive holds 4,233 `.wav` and
  2,232 `.ogg` files; 2,184 of the Ogg files are effects (`sound\fx`), ambient ones
  above all. A record that names `x.wav` is served by `x.ogg` when only that exists
  (`AMBDesertBedDay` names `…\amb_desertdaybed_lp.wav`; the archive has the `.ogg`),
  and folders hold `.ogg` takes.

Not built: markers that neither loop nor play at random, Random Location, Start at
Random Position, Mute When Submerged. Panning: §12.5.

## 6. Background loops

The acoustic space of the cell the player is in names a looping sound per time of day
(the Goodsprings cell: `ExtDesertDefault`, with `AMBDesertBedDawn`, `…Day`, `…Dusk`,
`…Night`, all flagged 2D) [observed]. As built [design]: dawn 5–9, afternoon 9–17, dusk
17–21, night otherwise; a space with only its first sound plays it all day; one loop
gives way to the next over 2 seconds. The hours the game uses are not known.

Not built: the reverb environment, walla, and the region an acoustic space points to
(REGN sound lists, `RDSD`).

## 7. Playback (`nvforever --play`)

- Music and loops are decoded while they play (rodio with symphonia: MP3, Ogg Vorbis,
  WAV); the three files of a set are mixed in one source so they cannot drift apart.
- Log: `music: in marker …`, `music: <set> day|night started, layers …`,
  `music: <set> layers …`, `music: <set> fades out`; at the end `music summary: …` and
  `ambient summary: …`.
- Switches: `--no-music`, `--music-volume`; placed sounds and background loops go with
  `--no-sound`. The settings page has a Music switch.
- Smoke test (CLAUDE.md): standing in Goodsprings, expect the desert exploration set
  with three layers, two placed loops (the air conditioners) and one background loop.

## 8. Tools

`fnvtool world audio <Data> <plugins> --schema schemas/fnv` (fnv.world.audio/1): the
counts above. With `--cell C`: the cell's music type and acoustic space, and every
audio marker, controller and sound marker in earshot (the whole worldspace for an
exterior cell). With `--at x,y,z [--hour H]`: the marker, set, layers and files chosen
there, the background loop and the sound markers in reach.

## 9. Open

- How the game ranks overlapping markers, and picks among several sets.
- Whether layers add up or replace one another, and what "trigger %" on a marker means.
- What a controller's faction lists need (reputation, disguise?) and when battle and
  incidental sets play.
- The per-second reading of Random Chance, and the hours of the four background loops.

## 10. Sounds animations and doors name

Code: `fnv_actor::anim_sounds` (reading the keys, footstep and armour sounds),
nvforever `anim_sounds.rs` (hearing the clips that play, the ground underfoot),
`nvf_play::doors` (door sequences' keys). Survey tools: the examples
`fnv-nif kf_sound_keys` (text keys of every sequence in the archives named) and
`fnv-world footstep_survey` (LTEX materials, ARMO animation sounds, `FST` records,
which key names are SOUN editor IDs). Counts are over `Fallout - Meshes.bsa` (5,171
files with sequences, 6,229 sequences) and FalloutNV.esm alone; the add-on archives
were not read.

### 10.1 What the data says

- **Text keys naming sounds** [observed]: 3,530 keys read `Sound: <name>` (also
  `sound:`), sometimes with a node after the name (`Sound: OBJVertibirdBladesALP
  SoundNodeProps`; 586 of them on doors, doors.md). 842 distinct names: 663 are editor
  IDs of SOUN records in FalloutNV.esm; the 179 others are not records there (83 with
  `DLC` in the name, the rest Fallout 3 content such as `QSTBirthEKGCritical`,
  `NPCTrogFootLeft`) [observed; hypothesis: animations left over from Fallout 3].
- **First-person and third-person weapon clips** name their sounds: each reload clip
  its weapon family's parts, e.g. `1hpreloada.kf` `WPNPistol10mmReloadOut` (0.20 s),
  `WPNPistol10mmReloadN` (0.43 s), `WPNPistol10mmReloadChamber` (0.67 s);
  `2hrreloada.kf` `WPNRifleHuntingReloadPt1`–`Pt4`; `2hareloadb.kf`
  `WPNRiflePlasmaReloadPt1`–`Pt4`; jams, grenade throws (`WPNGrenadeThrowA`/`B`), melee
  swings (`FXSwingKnife`, `FXSwing2hm`, `WPNArmSwing`) and swimming (`NPCHumanSwim`,
  `NPCHumanSwimSurface`) likewise [observed]. Equip clips carry `Enum: Equip` and no
  sound (the weapon's own draw sound plays, combat-rules.md §16) [observed].
- **Footfalls** [observed]: `Enum: Left` (1,038 keys) and `Enum: Right` (966) on the
  movement clips of people and creatures, first and third person (`_male\locomotion\
  male\mtforward.kf`: Left 0.37 s, Right 1.03 s; `_1stperson\locomotion\mtforward.kf`:
  0.33 s, 0.93 s), also on landing clips. Beside them, `Enum: ArmorWalk` (90),
  `ArmorRun` (106), `ArmorSneak` (45), `FoleyWalk` (84), `FoleyRun` (126) and
  `FoleySneak` (48): an `Armor…` key at each footfall, a `Foley…` key between them.
  Creature clips also carry `Enum: BackLeft`/`BackRight`, `Auxiliary1`–`8`, `Idle`,
  `Aware`, `Attack` (the CREA sound types, CSDT) and `m:L`/`m:R` (actors.md §8.3).
- **Footstep records** [observed]: SOUN editor IDs `FST<material><kind>`: the families
  ConcBroken, ConcSolid, Dirt, Grass, MetalHollow, MetalSheet, MetalSolid, Sand, Water
  and Wood have `WalkL`/`WalkR`, `RunL`/`RunR`, mostly `SneakL`/`SneakR`, `Land` and
  `Jump`; Wood, MetalSheet, MetalSolid and ConcSolid also `StairsL`/`StairsR`; Snow has
  `Walk`, `Sneak` and `Land`; there are `FSTArmorL`/`R`, `FSTArmorPowerWalkL`/`R`,
  `FSTWaterSwim` and Fallout 3's baby set. No record links a material to them: the
  link is by name [hypothesis].
- **Ground** [documented: xEdit LTEX HNAM; observed: counts]: each landscape texture
  names a Havok material, numbered like nif.xml's `Fallout3HavokMaterial` (0 Stone, 2
  Dirt, 4 Grass, 18 Sand, 19 Broken Concrete …). Of 89 LTEX: Dirt 47, Grass 12, Broken
  Concrete 10, Stone 10 (`Asphalt02`, `Pavement`), Sand 7 (`GravelRoadWasteland01`),
  Heavy Stone 3. Placed objects' collision triangles carry the same codes, plus 32 for
  platforms and 64/96 for stairs (physics.md, `fnv_combat::fx::impact_material`).
- **Armour rustle** [documented: xEdit ARMO SNAM "Animation Sound" (sound, chance,
  type 17–22: Walk, Sneak, Run, Sneak (Armor), Run (Armor), Walk (Armor)) and TNAM
  "Animation Sounds Template"; observed: the rest]: of 389 ARMO, 5 have SNAM, all
  `…AudioTemplate` records (Light, Medium, Heavy, Plastic, Power), three sounds each,
  at 100%; 228 name one of them as template (Light 134, Medium 63, Heavy 24, Plastic
  6, Power 1); none sets BNAM. Light and Plastic use the plain types (FOLCloth…,
  FOLPlastic…), Medium, Heavy and Power the `(Armor)` types (FOLArmorMedium…, …). The
  type names and the sounds' names disagree consistently: type Walk holds
  `FOLClothSneak`, Sneak `FOLClothWalk`, Run (Armor) `FOLArmorHeavyWalk`, Walk (Armor)
  `FOLArmorHeavyRun` [observed; either xEdit's enum order or the records are off].

### 10.2 What NVForever plays (`--play`)

- **Which clips are heard** [design]: the player's rig in view (the first-person arms
  rig while it is drawn, else the third-person body) and every drawn actor within
  3,000 units of the player. Of a rig's layers, those with at least half their layer's
  weight; a clip first seen part-way is heard from its next key, one just started (0.1
  s) from its start; no actor steps or rustles twice within 0.2 s (crossfading clips).
- `Sound:` keys play the SOUN of that editor ID; the node after the name is not used.
  The player's sounds play at the listener, an actor's at its feet.
- `Enum: Left`/`Right`: a ray down from the feet; on the landscape, the texture showing
  most there (the default texture where none shows on more than half) gives the
  material, on a placed object the triangle's code; the family by
  `fnv_actor::anim_sounds::footstep_family` [hypothesis, from the names: stone, glass
  and heavy stone are ConcSolid; dirt, cloth, organic and skin Dirt; hollow metal,
  vehicle bodies, barrels and carts MetalHollow; sheet metal, cans and lunchboxes
  MetalSheet; other metal MetalSolid]; shapes without per-triangle codes count as
  ConcSolid, nothing underfoot as Dirt, swimming as Water [design]. The record is
  `FST<family>Stairs<side>` on stairs, else `<gait><side>`, falling back to the walk,
  the side-less sound and Dirt's [design]. The gait is the clip's `Armor…`/`Foley…`
  keys' (else from its name: `fast` runs, `sneak` sneaks) [hypothesis].
- `Enum: Armor<gait>` plays the worn armour's `(Armor)` sound for that gait,
  `Foley<gait>` its plain one [hypothesis]; the gait is read from the sound's name, not
  the type (§10.1) [design]; the chance is applied. Of several pieces worn, the first
  with armour sounds counts, else the first with any [design]. No armour, no rustle.
- **Doors**: the keys of the "Open"/"Close" sequence as the door passes them, at the
  door; a sequence without sound keys plays the DOOR record's open (SNAM) or close
  (ANAM) sound as it starts [design: whether the game plays the record's sound as well
  as the keys' is not known].
- Log: `anim sounds summary: N footsteps (… by the player, … by actors; ground: …), N
  armour rustles, N reload keys, N other sound keys, N door sounds (N from door
  records), N keys without a SOUN`; per sound with
  `RUST_LOG=info,nvforever::anim_sounds=debug` (`trace` adds what is underfoot).
- Built since (§12): creature sound types, the player's landing and jumping, wading, a
  door record's looping sound (BNAM), load doors' sounds. Stairs on the landscape: the
  data has none (§12.2).

## 11. World and script sounds

Owner request 2026-10-11: "the rest of the sounds in game", hits on walls and bottles
first, and shot glass that sounds like glass. Code: `nvf_play::world_sounds` (knocks,
items, containers, activators, message boxes, script sounds, the per-impact log),
`nvf_play::combat` (impacts), `fnv_combat::fx`, `fnv_game::sound`. Counts are of the
owner's load order (FalloutNV.esm and the DLC) [observed, `nvforever --play` start-up
log] unless they say FalloutNV.esm.

### 11.1 Shot impacts: surface → sound

A shot's impact takes the IPCT its weapon's impact data set (IPDS) gives for the
surface's impact material, and plays that IPCT's sounds (combat-rules.md §16).

- **Finding** [observed 2026-10-11, the bottles of combat-rules.md §14]: shooting
  Sunny's bottles played `FXBulletImpactEarth`, the dirt sound. A bottle's collision is
  one convex shape, and only triangle meshes kept their Havok material; a hit without a
  material counts as dirt. Every box, sphere, capsule and convex-vertices shape stores
  one material [documented: nif.xml `bhkSphereRepShape`]; it is now read
  (`CollisionObject::material`, the first such shape's when a body has several
  [design]) and given to the body's convex colliders. The same bottles now play
  `FXBulletImpactGlass` ("impacts by material: glass 3").
- Walls were not silent: buildings are triangle meshes whose triangles carry materials
  [observed: Goodsprings building 001055E0, triangles of 0 STONE and 17 SHEET_METAL].
- **Material names.** `fnv_combat::fx::impact_material` maps the 32 Havok materials
  (code mod 32) [documented: nif.xml `Fallout3HavokMaterial`] to the twelve IPDS
  materials by their names [hypothesis]. Checked against the names: stone, heavy stone
  and broken concrete → stone; cloth → cloth; dirt and sand → dirt; glass and bottle →
  glass; grass → grass; organic and skin → organic; water → water; wood and heavy wood
  → wood; hollow metal, sheet metal, vehicle body and hollow parts, barrel, soda can,
  shopping cart, lunchbox → hollow metal; metal, heavy metal, chain, bottle cap,
  elevator, solid vehicle parts, pistol, rifle, baby rattle and rubber ball → metal
  (the last two by default, not by name). Terrain has no code and counts as dirt
  [design].
- **Actors** take their base's Impact Material Type (NPC_ and CREA `NAM4`, the same
  twelve in the same order [documented: xEdit]), organic without one [design]. Read
  [observed, load order]: organic 5,565, metal 487, organic bug 318, hollow metal 40,
  organic glow 34, stone 8, grass 3. Before this every actor counted as organic.
- Destructible and loose objects go through the same path as walls: the impact is
  picked before the hit's damage or push [observed: the code and the bottles' log].

The table: the ballistic impact data set 0x00019083, which the 10mm pistol, the Varmint
rifle and the service rifle name (WEAP `INAM`) [observed: `fnvtool combat fx`]. Sound
folders are under `sound\fx\fx\bullet\impact\`; dB is the SOUN's static attenuation.

| Surface (example) | Havok material | Impact material | IPCT | Sound (folder, dB) |
|---|---|---|---|---|
| Sunset Sarsaparilla bottle (loose) | 24 BOTTLE, convex [observed] | glass | `BallisticGlassImpact` | `FXBulletImpactGlass` (`glass\`, 0) |
| window glass | 3 GLASS | glass | `BallisticGlassImpact` | `FXBulletImpactGlass` |
| concrete or stone wall | 0 STONE [observed: 001055E0] | stone | `BallisticStoneImpact` | `FXBulletImpactConc` (`concrete\`, 15.4) |
| sheet metal wall, car body, barrel, can | 17, 20, 23, 25 [17 observed: 001055E0] | hollow metal | `BallisticMetalHollowImpact` | `FXBulletImpactMetalH` (`metalhollow\`, 7.9) |
| solid metal, guns | 5, 11, 26, 27 | metal | `BallisticMetalImpact` | `FXBulletImpactMetalS` (`metalsolid\`, 10.1) |
| wood | 9, 12 | wood | `BallisticWoodImpact` | `FXBulletImpactWood` (`newood\`, 12.8) |
| terrain, dirt, sand | none, 2, 18 [observed: terrain] | dirt | `BalisticDirtImpact` | `FXBulletImpactEarth` (`earth\`, 8.4) |
| grass | 4 | grass | `BalisticDirtImpact` | `FXBulletImpactEarth` |
| water | 8 | water | `ImpactWaterSplashSm` | `FXBulletImpactWater` (`water\`, 0) |
| cloth | 1 | cloth | none: the set names no IPCT [observed] | none |
| human NPC | `NAM4` Organic [observed: a hit on 00109A39] | organic | `BallisticOrganicImpact` | `FXBulletImpactFlesh` (`flesh\`, 10.1) |
| robot | `NAM4` Metal or Hollow Metal | metal, hollow metal | as metal, hollow metal above | `FXBulletImpactMetalS`, `…MetalH` |
| insect (radscorpion, ant, …) | `NAM4` Organic Bug | organic bug | `BallisticOrganicBugImpact` | `FXBulletImpactFlesh` |
| glowing one | `NAM4` Organic Glow | organic glow | `BallisticOrganicGlowImpact` | `FXBulletImpactFlesh` |

Read [observed, load order]: 147 IPCT in 73 IPDS. The three organic kinds share the
flesh sound; their effects differ.

### 11.2 Loose objects knocking

No record names what a loose body sounds like when it hits something. FalloutNV.esm
has 51 SOUN named `PHY…` (files under `sound\fx\phy\<material>\`), most in pairs `…H`
and `…L`, named after the Havok materials (`PHYBottleH`, `PHYBarrelL`, `PHYCanSodaH`,
`PHYGenericMetalSheetSmall`, `PHYVehicleMetalBodyH`, `PHYChain`, `PHYRubberBall`, …),
and `CWoodSmall`, `CWoodMedium`, `CWoodLarge` whose files are `fx\phy\wood\crate_*`
[observed]. `world_sounds::phy_sound` takes the one named after the body's material
[hypothesis]; H for a hard knock, L for a light one [hypothesis: hard and light];
where a family comes in sizes, hard takes the larger [design]. Dirt, grass, water and
sand have none.

When [design]: a dynamic body (placed clutter, a dropped item) that loses at least 90
units/s of its speed in a frame (`knock_speed`: the speed lost along the way it was
going) knocks; from 300 units/s hard; at most every 0.12 s per body, and not in the
first 1.5 s after its collision came (bodies settling when a cell loads). Gaining speed
(falling, pushed by a shot) is no knock. A knock plays at the body.

Seen [observed 2026-10-11, Goodsprings]: a shot bottle falling off the fence,
`PHYBottleL` at 122 units/s and `PHYBottleH` at 322; tumbleweeds (`TumbleweedNV`,
collision material 30 BABY_RATTLE) rolling, `PHYBabyRattle`. `PHYTumbleweed` exists but
nothing links it to the tumbleweed (its MSTT has no `SNAM`) [observed].

### 11.3 Items, containers, activators

| Record | Fields [documented: xEdit] | Records | Naming a pick-up / drop sound [observed] |
|---|---|---|---|
| WEAP | `YNAM` Sound - Pick Up, `ZNAM` Sound - Drop | 496 | 221 / 221 |
| ARMO | same | 489 | 427 / 425 |
| AMMO | same | 145 | 144 / 144 |
| MISC | same | 603 | 53 / 42 |
| KEYM | same | 313 | 283 / 283 |
| ALCH | same | 226 | 24 / 24 |
| CHIP | same | 6 | 1 / 1 |
| BOOK | same | 27 | 0 / 0 |
| NOTE | same | 1,225 | 0 / 0 |
| IMOD | same | 112 | 0 / 0 |
| CCRD | same | 324 | 0 / 0 |
| CMNY | same | 6 | 0 / 0 |

- Played: the pick-up sound when an item leaves the world for an inventory (activated
  by hand or by a script), the drop sound when a drop creates a reference, both at the
  reference [hypothesis: where]; in a container menu, the pick-up sound for what the
  player takes and the drop sound for what the player puts in, at the listener
  [hypothesis]. A record without one plays `UIItemGenericUp` or `UIItemGenericDown`
  [hypothesis: by the names; no record links them].
- Seen [observed 2026-10-11, GSHouseInterior03]: the BB gun `ITMPistolUp`, an empty
  bottle (no field) `UIItemGenericUp`, five kinds of food taken from the fridge
  `UIItemGenericUp`, 10mm rounds put in `ITMAmmunitionDown`, the jumpsuit dropped
  `UIItemsClothingDown`.
- CONT `SNAM` Sound - Open and `QNAM` Sound - Close [documented: xEdit]: 1,249
  containers name one or both [observed]; played when the container menu opens on the
  container and when it closes, at the container. The fridge 00105658 names neither.
- ACTI `VNAM` Sound - Activation [documented: xEdit]: 116 activators without a radio
  station (`RNAM`) name one [observed]; played when the player activates one, at it.
  Not seen in a run yet.

### 11.4 Message boxes and other interface sounds

- MESG has no sound field [documented: xEdit MESG], and `menus\message_menu.xml` names
  none for opening [observed]. Its buttons include
  `menus\prefabs\list_box_template.xml`, whose `clicksound` is `UIMenuOK` and
  `mouseoversound` `UIMenuFocus` [observed].
- Built: `UIPopUpMessageWindow` when a box opens [design: by its name;
  `UIPopUpMessageGeneral` names the same file, `fx\ui\popup\ui_popup_messagewindow.wav`
  [observed]], `UIMenuOK` when a button is picked (the box goes), `UIMenuFocus` when the
  choice moves.
- Named by the interface files and not built [observed: `clicksound` and
  `mouseoversound` in `menus\*.xml`]: `UIMenuOK`, `UIMenuPrevNext`, `UIMenuCancel`,
  `UIMenuFocus`, `UIPipboySelect`, `UIPipboyScroll`, `UIPipBoyMode`, `&nosound;`, and
  the numbers 1, 2, 3 and 9 (what they stand for is not known). `globals.xml` sets
  `_sound_button_click` to `UIMenuOK`.
- Other UI SOUN in FalloutNV.esm, not built: `UIMenuMode`, `UIMenuTab`, `UIPipBoy…`
  (access up and down, light, tabs, scroll, hum, radiation), `UIPopUpQuestNew`,
  `UIPopUpQuestComplete`, `UIPopUpExperienceUp`, `UILevelUp`, `UIKarmaUp` and `…Down`,
  `UIRepGood` and `…Bad`, `UIItemTakeAll`, and the lockpicking, hacking and VATS sounds.

### 11.5 Scripts

`PlaySound sound [SystemSound]` plays a SOUN not placed; `ref.PlaySound3D sound` plays
it at the calling reference [documented: GECK wiki PlaySound, PlaySound3D, owner
export]. The rules queue them (`Game::sound_requests`, traced `sound_asked`); the
engine plays them. Without SystemSound a sound asked for in a menu waits for the game
[documented: it "may be queued and not played until returning to GameMode"]. Seen
[observed]: a DLC start-up script's `PlaySound` (0x03016173). `PlayMusic` is not built
(the music's own controls are music.rs's).

### 11.6 Checks and what is left

Log: `impact: …` (debug: collision material, impact material, IPCT, sounds), `knock: …`
(debug), `world sounds: …` for items, containers, activators, message boxes and
scripts; at the end `world sounds summary: …`. Smoke tests in CLAUDE.md (the bottles,
the house).

Not built: two loose bodies knocking each other told apart from one on the ground,
loudness by how hard a knock is. Objects' own sounds (ACTI `SNAM`, CONT `RNAM`), the
casings' sounds and panning: §12.

## 12. Creatures, movement, objects' own sounds, placement

Owner request 2026-10-11: the remaining "not built" sounds. Code:
`fnv_actor::anim_sounds` (the creature records, key → type), nvforever `anim_sounds.rs`
(creatures, jumps, landings, wading) and `shot_fx.rs` (casings), `nvf_play::world_sounds`
(objects' own sounds, load doors), `nvf_play::ambient` (sources played like sound
markers), `nvf_play::sfx` (panning). Survey: the example `fnv-actor
creature_sound_survey` (FalloutNV.esm alone; `--show <CREA edid>`, `--near x,y,r`,
`--soun <prefix>`).

### 12.1 Creatures

- **The records** [documented: xEdit wbDefinitionsFNV.pas CREA, wbCSDT,
  wbSoundTypeSounds; `schemas/fnv/gameplay.json`]: `CSDT` a sound type, then pairs of
  `CSDI` a sound and `CSDC` its chance; `CSCR` "Inherits Sounds from" a creature. The
  types: Left Foot, Right Foot, Left Back Foot, Right Back Foot, Idle, Aware, Attack, Hit,
  Death, Weapon, Movement Loop, Conscious Loop, Auxiliary 1–8, then (xEdit lists
  "Auxiliary 8" twice, numbering the rest one higher) Jump and PlayRandom/Loop.
- **Counts** [observed]: 1,578 CREA. 86 list sounds themselves (most named
  `AudioTemplate…`), 999 only inherit them (`CSCR`), none do both, 468 only have a
  template (`TPLT`: 443 a leveled list, 25 a creature), 25 neither; 1,085 have sounds
  after following `CSCR`. Every type a creature lists holds one sound (704 type lists).
  Types listed (creatures): Left/Right Foot 72, Back feet 34, Idle 46, Aware 15, Attack
  40, Hit 32, Death 66, Movement Loop 12, Conscious Loop 37, Auxiliary 1–8 45 to 16,
  type 21 3; Weapon none, type 20 none. Chances: 100 on 541 of the 704, the rest 5–90.
- **Type 21** holds `NPCSecuritronSiren` on the three Securitron templates [observed]:
  a siren, not a jump; read with the observation that type 20 is never used, the game's
  numbering is likely Auxiliary 8 = 19, Jump = 20, PlayRandom/Loop = 21 [hypothesis].
  Not played.
- **Examples** [observed]: `AudioTemplateGecko` Left/Right Foot `NPCGeckoFootLeft`/
  `Right`, back feet `NPCGeckoFootRun…`, Attack `NPCGeckoSnarl` 50%, Hit
  `NPCFeralGhoulInjured`, Death `NPCGeckoDeath`, Auxiliary 1–7 voices, swing, chomp,
  jump; `AudioTemplateBigHorner` (Goodsprings' bighorners) feet `NPCBigHorner…FootL/R`,
  Idle `NPCBrahminIdleBreathing` 50%, Hit and Aware `NPCBigHornerInjured`, Death
  `NPCBighornerDeath`, Conscious Loop `NPCBigHornerConsciousLP` 50% (a folder, flagged
  Play At Random), Auxiliary 1–4; `NVCrRaven` only Auxiliary 1 and 3 (take-off and fly
  away); `AudioTemplateSecuritron` Movement Loop and Conscious Loop (files `…_lp.wav`).
- **The keys** [observed, `kf_sound_keys` over `Fallout - Meshes.bsa`]: creature clips
  carry `Enum: BackLeft` (136), `BackRight` (77), `Idle` (22), `Aware` (9), `Attack` (17,
  and `attack` 11), `Hit` (3, the cazador's hit clips), `Auxiliary1`–`8` (54, 261, 51,
  39, 39, 28, 23, 28; once `Auxiliary 3`), beside `Left`/`Right`. The bighorner's idle
  clip has `BackLeft` at 4.5, 9.1, 11.1 and 14.0 s; its walk `Left`/`Right`; its run the
  back feet and `Auxiliary2`; the raven's fly-away `Auxiliary1` at 0 s and `Auxiliary3`
  at 2.4 s.
- **As built**: the sounds are the creature's own list, else `CSCR`'s (followed up to
  five times) [hypothesis: an own list wins; none has both], of the creature a leveled
  list picked, else of the one placed. An `Enum:` key plays one of its type's sounds at
  its chance (`fnv_actor::anim_sounds::creature_key_type`, `pick_creature_sound`)
  [hypothesis: the keys name the types, one to one by name], at the creature, at most
  once per 0.2 s per type [design]. Hit and Death play when combat reports a hit or a
  death (`ActorHit`) [hypothesis]; `Enum: Hit` keys are not played again [design].
  A living creature within 3,000 units sounds its Conscious Loop (the chance rolled once
  per creature [design]) and while it moves (over 20 units/s) its Movement Loop, as
  sources the ambient plays like sound markers (§12.3). A creature without sounds steps
  like a person (§10.2). Not built: Aware (only through keys: nothing tells a creature
  became aware yet; detection exists but combat AI does not), Attack beyond the keys
  (creatures do not attack yet), Weapon, the type-21 sound.

### 12.2 The player's movement

- **Jumps and landings** [hypothesis: by the names]: `FST<family>Jump` when the player
  leaves the ground going up faster than 100 units/s, `FST<family>Land` on touching
  down after at least 0.4 s in the air [design: the thresholds], of the ground under the
  feet (§10.2), else `FSTDirtJump`/`Land`; at the listener. Not while swimming.
  `FSTDirtJump` names `fx\xxx_placeholder_silence.wav` [observed].
- **Wading** [design]: feet more than 2 units under a water surface (the cell's water or
  a placed water volume) step on Water (`FSTWaterWalkL`…, files `fx\fst\water\…`).
- **Stairs on the landscape** do not exist in the data: the 89 landscape textures' Havok
  materials are 0 (10), 2 (47), 4 (12), 10 (3), 18 (7) and 19 (10), none a stairs code
  (64–127) [observed].
- Not built: actors' jumps and landings (actors do not jump).

### 12.3 Objects' own sounds

- **The fields** [documented: xEdit]: ACTI `SNAM` Sound - Looping, CONT `RNAM` Sound -
  Random/Looping, DOOR `BNAM` Sound - Looping. In FalloutNV.esm [observed]: 216 of 1,143
  activators name one (Play At Random 164, Loop 49, neither 3), 144 of 2,478 containers
  (Loop 86, Play At Random 39, neither 19), 3 of 320 doors (`EMTWoodFence`, Play At
  Random); placed: 1,518 activators, 909 containers, 15 doors. Around Goodsprings: the
  mailboxes and dumpsters (`EMTSandOnMetal`), the picket-fence gates (`EMTWoodFence`),
  the saloon's exit triggers (`EMTWaterPressure`).
- **As built** [design]: each such reference, while enabled, is a source at its place;
  the ambient plays it as it plays a sound marker of that sound (§5: Loop goes round
  within the sound's reach, Play At Random rolls its chance once a second, hours apply).
  Sounds with neither flag are not played.
- **Load doors** [hypothesis]: activating a door that leads elsewhere plays its open sound
  (DOOR `SNAM`) at the listener as the player goes through.

### 12.4 Shell casings

FalloutNV.esm has `PHYCasingPistol`, `PHYCasingRifle` and `PHYCasingShotgun` (folders
`fx\phy\weapon\casing_pistol\` …) and no field links them to a weapon [observed]. 78
weapons name a casing model (WEAP `MOD2`): `assaultrifleshellcasing` 23, `grenadefrag`
13, `45calcasing` 11 (the 10mm pistol's), `762mmcasing` 10, `shotgunshellcasing` 6,
`eccellcasing` 6, others 1 or 2 [observed]. As built: the sound by the model's name
(shotgun → Shotgun; .45, .357, .44, 10mm, 9mm, pistol and energy cells → Pistol; the rest
Rifle) [hypothesis]; a bounce from 60 units/s plays it where the casing lands, up to
three times a casing [design].

### 12.5 Panning

How the game places sounds in stereo is not known. As built [design]
(`nvf_play::sfx::pan_gains`): a sound with a place is mixed to one channel and played in
stereo; the ear facing it hears it whole, the other loses up to 0.7 of it by how far to
the side it is (ahead, behind, above, below: both ears alike). The listener is the
player's eye and heading, followed while the sound plays. Panned: effects at a place
(impacts, knocks, actors' and creatures' sounds, doors, items, `PlaySound3D`), placed
loops and random markers, objects' and creatures' sources, radios. Not panned: sounds at
the listener (the player's own, interface), background loops, music, voices (a
recorded decision: voice plays flat, voice.md).

### 12.6 Reverb (not built)

ASPC `ANAM` is an environment type [documented: xEdit]: None, Default, Generic, Padded
Cell, Room, Bathroom, Livingroom, Stone Room, Auditorium, Concerthall, Cave, Arena,
Hangar, Carpeted Hallway, Hallway, Stone Corridor, Alley, Forest, City, Mountains,
Quarry, Plain, Parkinglot, Sewerpipe, Underwater, Small Room … Large Hall, Plate: the
names of the I3DL2 / EAX reverb presets [hypothesis: the game hands the preset to its
audio library]. Used in FalloutNV.esm [observed]: Small Room 26, Stone Corridor 25,
Auditorium 23, Medium Room 15, Large Room 10, Cave 5, Medium Hall 3, Mountains 2,
Livingroom, Sewerpipe, Stone Room and None 1 each. Not built: no reverb is applied; the
preset values would need a documented source.

### 12.7 Checks

Log: the `anim sounds summary` ends `creatures: N sounds (<type> n, …), N hits, N deaths,
N keys of a type not listed; the player jumped N and landed N times`; the `ambient
summary` ends `things that sound (activators, containers, doors, creatures): N heard, N
loops started, N random sounds played`; the `world sounds summary` ends `N load doors; N
objects' own sounds in reach`; the `shot fx` line ends `N casing knocks heard`. Per
sound with `RUST_LOG=info,nvforever::anim_sounds=debug,nvf_play::ambient=debug`. Seen
[observed 2026-10-11, Goodsprings by the bighorner pen]: the bighorners' idle `Left Back
Foot` (12 in 25 s), their conscious sound now and then, 4 hits and a death of
00109A39 with its Hit and Death sounds, the player's jump and landing on dirt, 10
casing knocks of 5 10mm casings. Smoke test in CLAUDE.md.
