# Spec notes: the world records' minor fields (ESM audit low-impact list, "World")

From the low-impact list of docs/handoff/esm-gap-audit-2026-10-21.md, "World". Field
names [documented: xEdit wbDefinitionsFNV.pas WRLD, LAND, LTEX, CELL, REFR, NAVI, NAVM,
LSCR]. Counts are FalloutNV.esm's [observed 2026-10-23, private survey, not committed].

## 1. REFR XAMT / XAMC: a placed weapon's ammo (built)

Three placed weapons name an ammo type and count: the varmint rifle in Primm's sheriff
station (0x000CDA0D, 5.56mm × 10), a BB gun in Nellis barracks 4 (0x00172FCA, BBs × 40),
a 9mm pistol at Bison Steve (0x000E71BE, 9mm × 10). Taken by hand the first time, the
weapon comes with that ammo [hypothesis: the rounds in it] (`fnv_sim::World::activate`,
`PlacedRef::ammo`). Smoke `placed-weapon-ammo`.

## 2. Not built, and why

- **Placed TXST decals**: 493 texture sets, 2 placed references (`DecalBloodPlaceable1`);
  a placed decal's projection (size, depth, bounds) is not documented for references.
- **CELL XCLC Hide Quad**: one cell sets it; hiding terrain quads needs the landscape mesh
  and its collision cut, for one cell.
- **CELL IMPF, WRLD IMPS/IMPF** (footstep material swaps), **CELL/WRLD XNAM** (water
  noise), **LTEX friction, restitution, specular**, **LAND DATA flags**, **WRLD bounds,
  LOD water and parent-use flags beyond Use Map Data, ONAM map offset**: read or stored
  in places; what each changes is not documented closely enough to build (the map is
  calibrated from its own markers, pipboy.md).
- **REFR XACT, XTRG, XCHG, WMI1, XRMR/XMBO/occlusion planes**: XPOD portals are used for
  room culling (rooms.rs); the rest have no consumer.
- **NAVI, NAVM edge cover**: the AI uses cover as yes or no per triangle; NAVI is read by
  fnvtool only.
- **PMIS, PBEA, PFLA, PCBE**: no master places one.
- **LSCR "Displays In Main Menu", LSCT**: the main menu picks its slides by having no
  location; the base game has one LSCT, the tip type (loading-screens.md).
