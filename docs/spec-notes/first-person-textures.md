# First-person weapon textures

How the weapon in the first-person view gets its detailed textures. Implemented in
`engine/nvf-assets/src/texture.rs` (`first_person_texture`) and `model.rs`
(`ModelCtx::model_first_person`), used by `engine/nvforever/src/player_body.rs` for the arms
rig's weapon.

## 1. What the data holds

- The laser rifle has one model, `meshes\weapons\2handrifle\laserrifle.nif`, drawn in the
  world, in third person and in first person (WEAP model fields name it; there is no
  separate first-person mesh in `Fallout - Meshes.bsa`) [observed: `fnvtool bsa list`].
- That model names `textures\weapons\2handrifle\LaserRifle01.dds`, `LaserRifle02.dds`,
  their `_n` normal maps and `MicroFusionCell01.dds` [observed: `fnvtool nif decode`].
  `LaserRifle01.dds` is 128x128 DXT1 [observed: `fnvtool dds list`].
- Beside it lie `1stpersonlaserrifle01.dds` and `1stpersonlaserrifle02.dds` (1024x1024
  DXT5), `1stpersonlaserrifle01_n.dds` and `_02_n.dds` (512x512) [observed: `fnvtool dds
  list` of `Fallout - Textures2.bsa`].
- The archives (`Fallout - Textures.bsa`, `Fallout - Textures2.bsa`) hold 455 files whose
  name starts `1stperson`: 444 under `textures\weapons`, the rest under the DLC folders'
  `weapons`. 413 of them have a twin in the same folder without the prefix; the other 42
  (the C4 detonator, the gatling laser's focus optics glow map, …) have none and are named
  directly by first-person-only models [observed: survey of the two listings].
- No model names the `1stperson` twin of a texture it uses [observed for the laser rifle;
  hypothesis for the rest, from the 413 twins being unreachable otherwise].

## 2. What the game shows

- The owner's screenshot of the game (2026-10-20, Goodsprings, laser rifle drawn in first
  person, 1920x1080) shows the small print on the rifle's focus housing ("Focus 625 nm →
  675 nm") legible. NVForever drawing the model's own 128x128 texture shows it as a smudge
  [observed: side-by-side at the same size].

## 3. The rule NVForever follows

[hypothesis, from §1 and §2] When the player's weapon is drawn in first person, each texture
a shape of its model names (diffuse, normal map, glow map) is replaced by the file of the
same folder whose name is `1stperson` followed by the texture's file name, when that file
exists; otherwise the named texture is used. A name already starting with `1stperson` is
kept. The third-person body and the world keep the model's own textures.

Not known: whether the game applies the rule to other first-person models (the arms, the
Pip-Boy); no `1stperson` textures exist outside the weapons folders, so it makes no
difference with the shipped data.
