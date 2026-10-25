# Spec notes: the objects' minor fields (ESM audit low-impact list, "Objects")

From the low-impact list of docs/handoff/esm-gap-audit-2026-10-21.md, "Objects". Field
names [documented: xEdit wbDefinitionsFNV.pas PROJ, DOOR, TERM, PWAT, GRAS, TREE, EXPL,
IPCT, ADDN, DEBR, ARMA]. Counts are FalloutNV.esm's [observed 2026-10-23, private survey,
not committed].

## 1. PROJ "Can Be Disabled" (built)

A placed explosive is disarmed by the player's activation into the weapon that fires it
(mines.rs, simulation.md). That now needs the projectile's DATA flag "Can Be Disabled"
[documented: the flag]; a projectile without DATA (synthetic tests) counts as having it.
Every master mine projectile has both Can Be Disabled and Can Be Picked Up: C4, the
bottlecap, frag, powder-charge, plasma and pulse mines [observed], so nothing in the
master changes; a mod's live explosive without the flag shows no Disarm prompt.
"Can Be Picked Up" (11 projectiles) is not used: a disarmed mine is a loose item taken
like any other (owner 2026-10-19).

## 2. Done on main already

- **ARMA** addons are drawn with their armour (799e5a1c).
- **EXPL "Knock Down - Always / By Formula"** and "Ignore LOS Check" are used (combat).

## 3. Nothing to build: no master record uses the field

- TERM "Hide Welcome Text when displaying Image": 5 terminals, none placed.
- DOOR "Automatic": none.
- DOOR "Hidden" was listed here as unused; it is set on 3 doors [observed:
  docs/handoff/esm-full-audit-2026-10-24.md gap 30], so it belongs in §4 (see gap 30 of
  esm-full-audit-2026-10-24).
- PMIS/PBEA/PFLA/PCBE (world-fields.md).

## 4. Not built, and why

- **DOOR "Sliding"** (14 doors, 37 placed) and **"Minimal Use"** (3): a door opens by
  its model's own animation; what the flags add is not documented.
- **PWAT reflection and refraction flags** (3 of 29 refract without reflecting): the
  water material reflects by its type's settings; how each flag maps to the passes is not
  documented.
- **Header bits Visible When Distant, On Local Map, Navmesh**: LOD is built from the
  shipped LOD files and the local map from collision.
- **Header bit 25 Obstacle** (307 STAT, 24 MSTT, 20 ACTI, 7 SCOL [observed: esm-full-audit
  gap 22]): not a LOD matter, as this list once said. An Obstacle object "will dynamically
  cut the navmesh in game preventing actors from walking into it", and only with a model of
  the right collision layer [documented: GECK wiki Activator and Container, "Obstacle"].
  fnv-nav cuts no navmesh at run time; see gap 22 of esm-full-audit-2026-10-24.
- **ACTI "Dangerous"**, **GRAS "Vertex Lighting"**, **TREE seeds, leaf angles, shadow
  radius**, **PROJ Supersonic, Pins Limbs, Rotation**, **EXPL world orientation and push
  flags**, **IPCT angle threshold**, **ADDN particle cap**, **DEBR collision**, **TERM
  Alternate Colors** (1 terminal): no consumer, or what they change is not documented.
- **INGR and COBJ as items**: no CNTO names them in the master.
