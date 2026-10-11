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

## 10. World and script sounds

Owner request 2026-10-11: "the rest of the sounds in game", hits on walls and bottles
first, and shot glass that sounds like glass. Code: `nvf_play::world_sounds` (knocks,
items, containers, activators, message boxes, script sounds, the per-impact log),
`nvf_play::combat` (impacts), `fnv_combat::fx`, `fnv_game::sound`. Counts are of the
owner's load order (FalloutNV.esm and the DLC) [observed, `nvforever --play` start-up
log] unless they say FalloutNV.esm.

### 10.1 Shot impacts: surface → sound

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

### 10.2 Loose objects knocking

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

### 10.3 Items, containers, activators

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

### 10.4 Message boxes and other interface sounds

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

### 10.5 Scripts

`PlaySound sound [SystemSound]` plays a SOUN not placed; `ref.PlaySound3D sound` plays
it at the calling reference [documented: GECK wiki PlaySound, PlaySound3D, owner
export]. The rules queue them (`Game::sound_requests`, traced `sound_asked`); the
engine plays them. Without SystemSound a sound asked for in a menu waits for the game
[documented: it "may be queued and not played until returning to GameMode"]. Seen
[observed]: a DLC start-up script's `PlaySound` (0x03016173). `PlayMusic` is not built
(the music's own controls are music.rs's).

### 10.6 Checks and what is left

Log: `impact: …` (debug: collision material, impact material, IPCT, sounds), `knock: …`
(debug), `world sounds: …` for items, containers, activators, message boxes and
scripts; at the end `world sounds summary: …`. Smoke tests in CLAUDE.md (the bottles,
the house).

Not built: an activator's looping sound (ACTI `SNAM`), a container's random or looping
one (CONT `RNAM`), the casings' PHY sounds, two loose bodies knocking each other told
apart from one on the ground, loudness by how hard a knock is, panning.
