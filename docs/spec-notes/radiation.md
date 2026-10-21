# Environment radiation

Radiation the player takes from the world: placed radiation markers and irradiated water,
cut by Rad Resistance, the Geiger counter's clicks, the HUD's radiation meter and
GetRadiationLevel. Rads from food, drink and chems, RadAway, Rad-X and the radiation
sickness stages were built before (aid-items.md §3, §11) and are only used here.
Code: `crates/fnv-game/src/radiation.rs` (the rule), `engine/nvf-play/src/radiation.rs`
(water, Geiger counter, meter, log). Queue item 26, 2026-10-21.

## 1. Sources

- GECK wiki export (`C:\fnv-private\geckwiki`): GetRadiationLevel ("a radiation level of 3
  would give you 3 rads/sec"; the actor's own rads are GetAV RadiationRads),
  GetRadiationLevelAlt (the vanilla function "would only return a value when called on
  the player"), Get/SetRadiationExtraData ("mostly used for Radiation Markers, where it
  determines the amount of rads gained while standing in the marker's radius"), the
  Settings page's "Radiation Settings" (fRadiationAccumulationRate,
  fSwimRadiationDamageMult, fWadeRadiationDamageMult). The wiki's own Radiation and
  Radiation Marker pages are not in the export.
- xEdit wbDefinitionsFNV.pas: REFR XRAD "Radiation" (f32), XRDS "Radius" (f32); WATR XNAM
  "Actor Effect".
- FalloutNV.esm [observed, read with a private script over the plugin]: the records below.

## 2. Radiation markers

- **The data** [observed]: 119 references carry XRAD, every one a reference of the static
  `RadiationMarker`. XRAD runs from 0.2 to 720 (1, 5, 10, 3, 2 most often); XRDS from 10
  to 10000 (512, 256 and 1000 most often). 69 stand in exterior cells of the Mojave, the
  rest in interiors (Vault 34, the Searchlight buildings, Techatticup Mine, the nuclear
  test site's shack, Rat Cave …).
- **Level in the radius** [hypothesis]: inside a marker's radius (3-D distance from its
  position) the marker's whole XRAD holds; outside it nothing. Where markers overlap the
  strongest counts; they do not add. Reason: the masters put markers of rising level and
  falling radius on one spot — Clark Field 3/2000 with three 720/10 spots, the crater by
  Camp Searchlight 5/10000, 10/6000 and 18/2000 on one point, Black Mountain's crater
  1/3500, 10/1500, 5/500 — which makes a stepped gradient only if each marker is flat and
  they do not add. The wiki's "standing in the marker's radius" reads the same way.
- A disabled marker (an enable parent, `Disable`) gives nothing [hypothesis].
- The markers are kept by the space they stand in (the worldspace for exterior cells, else
  the interior); every marker of the player's space is measured each frame (119 in all,
  no culling by loaded cells).

## 3. Irradiated water

- **The data** [observed]: no FalloutNV.esm water has a non-zero WATR damage (DATA's u16,
  all 0). Every water but three names an actor effect in XNAM: WaterHeal1Purified (no
  rads), WaterHeal2Good (4 rads), WaterHeal3Average (6), WaterHeal4Bad (10),
  WaterHeal5Terrible (20), WaterHeal1Rads500 (500); these are the effects of drinking
  the water (a Damage Radiation Level effect, plus health and dehydration).
- **The rule** [hypothesis]: swimming in a water gives its effect's rads a second times
  `fSwimRadiationDamageMult`; wading (the feet under the surface, not swimming) times
  `fWadeRadiationDamageMult`. Both are 0.1 in FalloutNV.esm [observed], so the Mojave's
  river and lake waters (WaterHeal4Bad) give 1 rad a second.
- **What the engine tells** (`tell_water`): swimming is the physics character's state;
  wading is the feet under the water surface the physics has at the player's x, y. The
  water type is the cell's XCWT, else the worldspace's NAM2 [hypothesis: a placed water
  volume is taken as its cell's water type; interiors have physics water only through
  their placed volumes].

## 4. The level and the rads taken

- **Level** = the markers' level (§2) + the water's (§3): rads a second before
  resistance. GetRadiationLevel returns it for the player and 0 for anyone else
  [documented: §1].
- **Rads taken a second** = level × `fRadiationAccumulationRate` (1.0 [observed]) ×
  (1 − Rad Resistance ÷ 100), Rad Resistance held to 0…85 [hypothesis: the setting's name;
  the resistance as a percentage, as Rad-X's +50 reads; the cap as Damage Resistance's
  85]. They raise the player's Rads actor value (54) every frame of game-mode time (not in
  menus or conversations).
- Rad Resistance is the actor value 20 (the Endurance formula, `(END − 1) × 2`, Rad-X's
  modifier, apparel's enchantments: actor-values and aid-items). Rad-X and RadAway work
  through it and through the Rads value with nothing more here.
- **Radiation sickness** follows the Rads value's stages (RADS records, 200 Minor … 1000
  Fatal; aid-items.md §11) as before.
- `fRadiationDecayRate` (0.01) and `fRadiationResistanceAvoidLevel` (50) are read by
  nothing: what they do is not documented [observed: the values; hypothesis: the second is
  for the AI packages' Avoid Radiation flag, which is not built].
- Only the player takes environment radiation [design]; NPCs and creatures do not.

## 5. Geiger counter

- **The records** [observed]: `UIPipBoyRadiationA`, `…B`, `…C` name the folders
  `fx\ui\pipboy\radiation\a\` … `c\`, three takes each, each about one second (88,2xx
  bytes of 16-bit 44.1 kHz mono); `UIPipBoyRadiationTestLP` is a 11-second loop. Their
  FormIDs follow fRadiationDecayRate's and fRadiationAccumulationRate's (0x235B8 …
  0x235BC).
- **The rule** [hypothesis]: A, B, C are rising strengths; while the level is above 0 one
  burst plays every second, A below 5 rads a second, B below 10, C from 10; a new band
  starts with a burst at once. The bursts play at the listener (interface sounds). The
  test loop is not used.

## 6. The HUD's radiation meter

- **The file** [documented: `menus\main\hud_main_menu.xml` `RadiationMeter`]: a piece 367
  × 60 in the HUD's alternative colour (`&hudalt;`, 255 67 42 [observed:
  Fallout_default.ini]), a bracket picture `glow_messages_radiation_seperator_right.dds`
  (567 × 120 at x −135), a pointer picture, `radiation_text_value` (font 8, right-justified
  at x 237, y 65) and `radiation_text` (font 7, from x 245); the texts are empty and every
  part's alpha 0 in the file: the code fills and shows them.
- **Built** [hypothesis]: while rads are taken the meter shows "+n" (the rads a second
  taken, whole, at least 1) and "RADS" (`sStatsRADAbbrev` is set by no master), with the
  bracket; it stands at the health piece's left margin just over it (the file gives no
  place). The pointer is not drawn (what it points at is not known).

## 7. Log lines (smoke `environment-radiation`)

- `radiation: level <l> rads/s (marker <ref> <l>, swimming in <water> <w>), taking <r>/s
  (Rad Resistance <rr> %), rads <n>` when the source or the level (to a tenth) changes;
  `radiation: out of radiation, rads <n>`.
- `radiation: the player is swimming|wading in water <name>`, `… out of the water`.
- `geiger: UIPipBoyRadiation<A|B|C> (level <l>)`, `geiger: quiet` when the band changes.
- At exit: `radiation summary: <s> s in radiation, highest level <l> rads/s, rads <a> -> <b>
  (+<d>), <k> Geiger bursts`.
- The sickness stage: `needs: Rad Level <n>: Radiation200` (aid.rs); the game's own CGTutorialRadiation box follows.

## 8. Not built

- NPCs and creatures taking rads (feral ghouls' healing in radiation, the Avoid Radiation
  package flag, companions' remarks rest on GetRadiationLevel, which answers for the player).
- Rads from a radioactive actor's aura (Glowing Ones' `vGlowingOneRadAura` is an
  ability with an area effect; area effects are not built).
- The Pip-Boy's own radiation readout beyond the Rads value it lists.
