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
Random Position, Mute When Submerged, panning.

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
- Not built: creature sound types (`Enum: BackLeft`, `Auxiliary…`, `Idle`, …; CREA
  CSDT), landing and jumping (`FST…Land`/`Jump`), stairs on the landscape, wading
  (ground under water), a door record's looping sound (BNAM), load doors' sounds.
