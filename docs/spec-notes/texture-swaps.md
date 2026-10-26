# Model texture swaps (alternate textures: MODS, MO2S, MO3S, MO4S)

A record can draw a shared mesh with other textures on some of its shapes: a billboard
model per casino, a rusted terminal, a dirty outfit, a beard in an old man's grey. The
textures come from a Texture Set (TXST) the record names per shape. ESM gap audit row 18
(`docs/handoff/esm-gap-audit-2026-10-21.md`). Code: `crates/fnv-world/src/alt_textures.rs`
(reading and the shape rule), `fnv_nif::scene::geometry_order`,
`engine/nvf-assets/src/model.rs` (`ModelCtx::model_swapped`), `actor.rs`
(`ActorCtx::part_swapped`), `nif_spawn.rs` (`Props::use_texture_set`); survey
`fnvtool world alt-textures`.

## 1. The data

- Four subrecords, each beside the model it changes: MODS beside MODL, MO2S beside MOD2,
  MO3S beside MOD3, MO4S beside MOD4 [documented: xEdit `wbDefinitionsFNV.pas` wbMODS in
  wbTexturedModel; the GECK wiki's JohnnyGuitar `GetArmorAltTextures` numbers an armour's
  lists 1 male biped (MODL), 2 female biped (MOD3), 3 male world (MOD2), 4 female world
  (MOD4)].
- Layout: a u32 count, then per entry the shape's name as a u32 byte length and that many
  Windows-1252 bytes (no terminator), the TXST FormID (u32) and the "3D Index" (i32)
  [documented: xEdit field names "3D Name", "New Texture", "3D Index"; observed: all 2,594
  entries in the ten masters parse to the byte, schema type `AlternateTexture` in
  `schemas/fnv/common.json`].
- Who uses them [observed: survey of the ten masters]: STAT 474 records, ARMO (MODS 56,
  MO2S 63, MO3S 54, MO4S 11), DOOR 29, HDPT 26, ACTI 14, TERM 14, ARMA (MODS 4, MO3S 5),
  WEAP 5, ALCH 1. Entries: MODS 2,267, MO3S 241, MO2S 74, MO4S 12. The STATs include the
  first-person weapon models weapons name in WNAM (`1stPerson32CaliberPistol`, …).
- TXST holds up to six files, TX00–TX05, and DNAM flag "No Specular Map" [documented:
  xEdit TXST]. Their meaning, in order: diffuse, normal/gloss, environment mask, glow (or
  skin/hair), height, environment [documented: GECK wiki "Texture Set" map list; observed:
  the files' `_n`, `_m`, `_g`/`_hl` and `_e` suffixes in the masters' TXSTs]. Counts in the
  masters: TX00 619, TX01 603, TX05 163, TX02 147, TX03 73, TX04 11.
- `NullTextureSet` (0x00000028) names no file at all; 29 DOOR entries point at it
  [observed].

## 2. Which shape an entry names

- The 3D Index is the shape's position among the model's geometry blocks (NiTriShape,
  NiTriStrips, BSSegmentedTriShape) counted depth-first from the root, children in their
  stored order [observed: `terminal01.nif`'s nine shapes and `outfitf.nif`'s 22 match
  every entry of `V22HiddenValleyFilterWallTerminal` and `CSArmorRadiationSuit`; in
  FalloutNV.esm 1,679 of 1,838 entries have their named shape at that index, `fnvtool
  world alt-textures`].
- Names are not unique: `outfitf.nif` has two `UpperBody` shapes (12 and 13) and the
  record lists both [observed].
- 70 entries name a shape that exists at another index (`dynamite.nif`: `Dynamite:0` given
  2, it is the fifth shape; the vault gear door `VGearDoor01b:5` given 30, it is at 28) and
  89 name a shape the model does not have (`MS18RangerBattlearmor` `Arms1`) [observed;
  hypothesis: records made against an older mesh].
- The index decides, not the name [observed 2026-10-26, the owner: the blue dress
  NVForever drew on Jeannie May Crawford is wrong. Her `OutfitPrewarCasualwear2Dirty`
  (female model `1950stylecasual01\f\outfit.nif`) lists `arms` 2, `pipboyoff` 7,
  `pipboyon` 8 and `pipboyoff` 9; the model's shape 9 is `UpperBody`, the dress, so only
  the index puts the list's brown `outfit02fold.dds` on it as on her arms. The name-first
  rule built before left the dress its own blue `outfit01f.dds`]. The same stale names
  sit on the dress or body of every `OutfitPrewarCasualwear2*`, the female Vault
  87/92/106/112 suits (`PipboyOn` at 6, their `UpperBody`), the RobCo jumpsuits and the
  1950s suits (`Arms` against `Arms:0`) [observed, `fnvtool world alt-textures`].
- Rule built: the shape at the index, whatever its name; only when no shape sits there
  (the beards' index -1, an index past the last shape) every shape of the entry's name,
  without regard to case; neither: nothing (logged) [hypothesis for the fallback]. Some
  first-person weapon lists are off by one or two against their meshes
  (`32snubnose.nif`, `dynamite.nif`) and now go to the neighbouring shape by index, as
  the rule says the game does [hypothesis: not captured]. Over the ten masters' 2,589
  entries: 2,301 name the shape at their index, 229 go to it by index alone (81 of them
  only a `:0` suffix apart), 14 by name and 45 to nothing [observed, `fnvtool world
  alt-textures`; before, 89 went by name and 199 to nothing].

## 3. What the swap does

- The TXST's files replace the shape's texture-set slots: TX00 → slot 0 (diffuse), TX01 →
  1 (normal), TX03 → 2 (glow/skin/hair), TX04 → 3 (height), TX05 → 4 (environment cube),
  TX02 → 5 (environment mask) [documented: nif.xml BSShaderTextureSet slot order; the
  pairing from the meanings in §1].
- A TXST slot with no file keeps the shape's own texture in that slot [hypothesis: the
  alternative, clearing it, would leave e.g. `NullTextureSet` shapes untextured; which one
  the game does is not captured]. So `NullTextureSet` changes nothing here.
- The glow map follows slot 2 unless the shape uses slot 2 for skin or hair (the rule
  `glow_map` applies to the file's own textures, nif.md §6).
- DNAM "No Specular Map" is read by the schema but not applied [hypothesis: the shape's
  own specular flag stays].

## 4. Where it is applied

- Placed references (render streaming `world.rs` `spawn_model`, imposters) and created or
  dropped references (`nvf-play/src/dropped.rs`): the list beside the model drawn in the
  world, MODS for MODL, MO2S for armour's MOD2, MO4S for a weapon's MOD4
  (`BaseObject::world_alt_textures`).
- Worn armour: MODS with the male model, MO3S when a female wears MOD3 (the male list
  when she falls back to MODL). The armour addons a worn item's BIPL names (ARMA,
  actors.md §2.3) are drawn since 2026-10-23 and take their MODL/MOD3 lists the same way.
- Head parts (HDPT beards): MODS.
- Weapons in the hand: the weapon's MODS in third person (NPCs and the player; MO4S when
  it holds its MOD4, actors.md §9.3); in first
  person the MODS of the STAT WNAM names (its MODL is the model shown). A modded weapon's
  own model (MWD*/WNM*) takes no list [documented: xEdit has none beside them].
- The `1stperson` texture fallback (first-person-textures.md §3) runs after the swap, on the
  swapped files.
- Each retextured model or part is a copy of its own, cached by the list; the log says
  `<model>: texture swap: N entr(ies), M shape(s) retextured: <shape> → <texture>, …`.

## 5. Open

- Whether the game falls back to the name when the index finds no shape, and what a TXST
  slot without a file does (§2, §3): a capture of `VaultSuit108` (index past its shapes) and a door with
  `NullTextureSet` (`REPCONNMidToBasementDoor`) would show it.
- Scripts' SwapTexture (`engine/nvforever/src/texture_swap.rs`) is separate and works on
  drawn materials; it is unchanged.
