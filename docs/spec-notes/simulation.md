# Spec notes: core simulation (Layer 4) — forms, references, inventories, activation

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525; load order
FalloutNV, DeadMoney, HonestHearts, OldWorldBlues, LonesomeRoad, GunRunnersArsenal,
ClassicPack, MercenaryPack, TribalPack, CaravanPack — "the ten masters" below) with
`fnvtool sim forms`, **[hypothesis]** needs verification against the original game.

Record layouts come from the Layer 1 schema (`schemas/fnv`, re-expressed from xEdit
`wbDefinitionsFNV.pas` as documentation); this note covers what the simulation does with
them. Behaviour of the running game (what activation does, how leveled lists roll) is
not visible in the data and stays **[hypothesis]** until the in-game captures of §6 exist.

## 1. Forms and references

- A **base form** is a record that describes a kind of thing (a weapon, a container
  type, an NPC). A **reference** (REFR, ACHR for NPCs, ACRE for creatures) places one
  instance of a base form in a cell; its NAME subrecord names the base form, DATA holds
  position (x, y, z) and rotation (radians, x, y, z) [documented: xEdit].
- All FormIDs inside the simulation are global (load-order) FormIDs: the plugin-local
  top byte is replaced by the load-order index of the plugin it designates
  (docs/spec-notes/esm.md "FormIDs and load order"). The last plugin to define a FormID
  wins; a winning record with the Deleted flag (0x20) removes the form [documented: UESP
  "Tes4Mod:Mod File Format" record flags; observed: 67 placed references are deleted by
  their winning definition, no base form is].
- A reference's **cell** is the label of the innermost cell-children group (types 8
  persistent, 9 temporary, 10 visible-distant) that contains it [documented: UESP group
  types]. The simulation takes membership and cell from Layer 3's world index
  (docs/spec-notes/world.md), so both layers apply one rule.
- Besides REFR, ACHR and ACRE, cell groups hold 297 placed explosives (PGRE) across
  FalloutNV, DeadMoney, HonestHearts, OldWorldBlues and LonesomeRoad; their bases are
  projectiles (PROJ), e.g. `MineFragProjectile` "Frag Mine", `MinePlasmaProjectile`,
  `MineNVPowderChargeProjectile`, a C4 projectile [observed]. They are armed mines and
  charges [hypothesis]; the simulation does not model them yet (reported).
- **Enable parents** (XESP: parent reference, flag bit 0 "Set Enable State to Opposite of
  Parent" [documented: xEdit]): 5,171 references have one, 759 with the opposite flag;
  162 chains are two deep, none deeper; no chain loops or names a missing reference
  [observed]. The simulation derives a child's enabled state from its parent's
  (inverted for "opposite") and ignores the child's own flag while it has a parent; a
  broken chain falls back to the own flag [hypothesis: GECK wiki "Enable Parent"].
- Record flag **0x400** on a reference coincides exactly with placement in a
  persistent-children group: 20,344 references have both, none has only one [observed].
  It is the "Persistent" flag [documented: UESP].
- Record flag **0x800** ("Initially Disabled" [documented: UESP]) is set on 4,723
  references [observed]. The simulation starts such references disabled [hypothesis].
- The ten masters place 427,022 references: 419,341 REFR, 3,942 ACHR, 3,739 ACRE
  [observed]. 1,492 REFR name base FormIDs that no plugin defines, all in the
  engine-reserved range below 0x800: 0x20 (1,228; every one carries portal data XPOD)
  and 0x17 (264; occlusion-plane data XOCP) [observed]; these are culling primitives
  provided by the engine [hypothesis] and matter to Layer 3 only.

## 2. The player

- The player's base form is NPC_ **0x00000007**, Editor ID `Player` [observed].
- No master places reference **0x00000014**, although 4,566 FormID fields point at it
  (docs/design/layer1.md §2.4) [observed]. It is the player's reference, created by the
  engine [hypothesis]. The simulation creates it on first use from the base form, with
  the base form's carried items, in no cell, at the origin.

## 3. Items, containers, carried items

- **Inventory item** record types: WEAP, ARMO, AMMO, ALCH, BOOK, KEYM, MISC, NOTE, IMOD,
  CHIP, CCRD, CMNY, and LIGH when its DATA flags have bit 1 "Can be Carried"
  [documented: xEdit lists these as valid CNTO targets; flag name from xEdit LIGH DATA].
- **Value and weight** [documented: xEdit layouts]: WEAP, ARMO, BOOK DATA (`value` i32,
  `weight` f32 among other fields); MISC, KEYM DATA (value i32, weight f32); AMMO value in
  DATA, weight in DAT2; ALCH weight is DATA (f32), value in ENIT; LIGH DATA. NOTE has no
  value or weight field; IMOD, CHIP, CCRD, CMNY have no layout in the schema yet
  (work queue). Five AMMO records, all flagged Non-Playable creature ammunition
  (e.g. `AmmoBloatflyDart`), have no DAT2 and therefore no weight [observed].
- **Carried items** (CNTO, repeated: item FormID, count i32) appear on CONT, NPC_ and
  CREA [documented: xEdit]. In the ten masters [observed]: 30,414 entries; 7,365 on
  containers and 15,993 on actors name a leveled item list (LVLI) instead of an item;
  **105 entries have count 0** (none negative; none on containers, so all on NPC_/CREA,
  e.g. the Vault 21 gamblers' templates); 6 entries name non-items (three STAT/MSTT in the
  test container `TestQAMisc`, three uncarriable LIGH on the Gomorrah bouncers).
  The simulation gives nothing for a count-0 or non-item entry [hypothesis].
- **CONT DATA**: flags u8 (bit 1 "Respawns"), weight f32 [documented: xEdit]. 3,417
  container bases; 177 respawn; 845 have no CNTO [observed].
- **Inventory model**: one stack per base item with a count. Removing more than is held
  removes what is there and reports that number [hypothesis: GECK `RemoveItem` behaviour
  as commonly described; to be captured].

## 4. Leveled item lists (LVLI)

Data [documented: xEdit]: LVLD chance none (u8, percent), LVLF flags (bit 0 "Calculate
from all levels <= player's level", bit 1 "Calculate for each item in count", bit 2
"Use All"), optional LVLG global, repeated LVLO entries (level u16, item FormID, count
u16 — the count is optional in the layout but present in all 16,950 entries [observed]).
3,479 lists; 7,871 entries name another list; flags: all-levels 1,526, each 2,042,
use-all 723; 41 lists have LVLG [observed].

Selection rules **[hypothesis, from the flag names only]**, implemented in
`fnv_sim::leveled` and to be checked against sampled in-game outcomes with the planned
distribution comparator:
1. Roll 0–99; below chance none (or the LVLG global's value) yields nothing.
2. Eligible entries have level ≤ the requesting level (the player's level for
   containers [hypothesis]).
3. Use All: every eligible entry. Otherwise one entry uniformly from all eligible entries
   (all-levels flag) or from those of the highest eligible level.
4. "For each item in count": N separate rolls of one; otherwise one roll times N.
5. Nested lists resolve recursively with entry count × multiplier.

## 5. Locks, doors, activation

- **XLOC** on a reference [documented: xEdit]: level u8, key FormID, flags (bit 2
  "Leveled Lock"). Observed only on CONT (321) and DOOR (341) references; levels
  0 (63), 25 (120), 50 (182), 75 (91), 100 (34), 255 (172); 161 name a key, always a
  KEYM; 104 level-255 locks have no key [observed]. Level 255 is "requires key"
  [hypothesis]. A reference with XLOC starts locked [hypothesis].
- **XTEL** (door FormID, position, rotation, flag) makes a door a teleport [documented:
  xEdit]. 1,378 references have XTEL; 1,376 pairs are reciprocal (the destination door's
  XTEL points back); 0x00087A6B → 0x000827BA and 0x000CAEB5 → 0x000CAFA5 point at doors
  that teleport elsewhere [observed]. Every destination exists and is a DOOR reference
  [observed].
- **Default activation by base type** [hypothesis, GECK wiki's description of object
  types; to be captured]: item → the activator picks up the stack (XCNT count, default 1)
  and the reference leaves the world; container → opens its inventory; door → teleports
  the activator to XTEL position/rotation and the destination door's cell, or toggles
  open/closed; NPC/creature → dialogue (Layer 6); activator, talking activator,
  furniture → no default effect beyond the script event. Locked doors and containers
  refuse activation unless the activator carries the key, in which case they unlock
  first. A base form's script (SCRI) receives OnActivate; whether an OnActivate block
  suppresses the default is Layer 5 [hypothesis].
- **Created references** (dropped items) get FormIDs from 0xFF000000 upwards, a load
  slot no plugin can use [hypothesis for the original engine's numbering].

## 5b. Game settings and game time

- **GMST** DATA is a union chosen by the Editor ID's first letter: `f` f32, `i` i32,
  `s` NUL-terminated string [documented: xEdit]. The masters hold 649 settings (459 `f`,
  61 `i`, 130 `s` in FalloutNV.esm alone), all matching the rule [observed]. Examples:
  `fAVDCarryWeightsBase` 150.0, `fAVDCarryWeightMult` 10.0 [observed]. There is no
  respawn-delay setting in the masters (no `iHoursToRespawnCell`) [observed]; the delay
  is an engine default [hypothesis].
- **Time** is kept in globals [observed, FalloutNV.esm]: GameYear 0x35 = 2281, GameMonth
  0x36 = 9, GameDay 0x37 = 18, GameHour 0x38 = 12, GameDaysPassed 0x39 = 5.0, TimeScale
  0x3A = 30. Advancing time [hypothesis]: TimeScale is game seconds per real second;
  h game hours add h to GameHour and h/24 to GameDaysPassed; each wrap past 24 hours
  advances GameDay with Gregorian month lengths, GameMonth counting from 0.
- Globals changed at run time are part of the simulation state (`Snapshot.globals`).

## 6. Capturing the oracle (in-game)

Layer 4 scenarios compare *observations* (item counts, position, cell) that the original
game can report through console functions [documented: GECK wiki function pages
`GetItemCount`, `GetPos`, `GetParentCell`, `AddItem`, `RemoveItem`, `RemoveAllItems`,
`Activate`]. Procedure (to be settled with the owner; the xNVSE logging plugin is an open
lab item, spec §8.1):
1. Load the pinned save at the scenario's start location (Goodsprings: inside Doc
   Mitchell's house).
2. Run the scenario's steps as console commands (`player.removeallitems`,
   `<ref>.activate player`, `player.additem <id> n`, …); record each query's value.
3. Write the values into the golden `sim/<scenario>.trace.json` with the same `label`s.
Until then the scenarios report `error` (missing golden), as Layer 1's xEdit scenarios did.

## 7. CLI contract (Layer 4)

All documents are single JSON objects on stdout; FormIDs are strings `"0x%08X"`
(global); exit codes as for every `fnvtool` command. `<lo>` is
`<data_dir> <plugin>...` (load order) and `--schema <dir>` the Layer 1 schema directory.

### `fnvtool sim containers <lo> --schema <dir>` → `fnv.sim.containers/1`

```json
{"format": "fnv.sim.containers/1", "count": 3417,
 "containers": [
   {"formid": "0x0002FCC3", "edid": "DresserAbandoned", "name": "Dresser",
    "respawns": false, "script": null,
    "entries": [{"item": "0x0016BB08", "count": 1, "class": "leveled_item"}]}]}
```

- One object per winning, non-deleted CONT record, ordered by global FormID.
- `edid` (EDID), `name` (FULL) and `script` (SCRI, global FormID) are `null` when absent.
  `respawns`: bit 1 of the first DATA byte (false without DATA).
- `entries`: the CNTO subrecords in record order, with the item FormID made global and
  the count as stored (i32). Entries with a null item (0) or an item whose master index is
  outside the plugin's master list are omitted.
- `class` of an entry's item: `item` if the item's winning record is one of the item types
  of §3 (LIGH only with "Can be Carried"); `leveled_item` for LVLI; `other:<SIG>` for any
  other existing record type (including uncarriable LIGH); `engine_builtin` for a FormID
  below 0x800 that no plugin defines; `missing` otherwise. A deleted winning record
  counts as not defined.
- Byte layouts used [documented: xEdit]: EDID and FULL are NUL-terminated Windows-1252
  strings; SCRI is a u32 FormID; CNTO is u32 FormID + i32 count (8 bytes); CONT DATA is
  flags u8 + weight f32; LIGH DATA is time i32, radius u32, color (4 bytes), flags u32 at
  byte offset 12 ("Can be Carried" = bit 1), then further fields. All little-endian.

Lab comparator: **keyed** on `formid` over `/containers`, all fields.

### `fnvtool sim run <lo> --schema <dir> --actions <file>` → `fnv.sim.trace/1`

Input `fnv.sim.actions/1` (`parity-lab/inputs/layer4/*.actions.json`): `seed`,
`player_level`, `actions` (ops: `activate`, `add_item`, `remove_item`, `transfer`,
`transfer_all`, `drop_item`, `remove_all_items`, `get_item_count`, `get_pos`,
`get_disabled`, `get_global`, `set_global`, `advance_time`, `lock`, `unlock`, `enable`,
`disable`), optional `report`. Queries (`get_*`) produce observations; `get_disabled`
reports `disabled` (the in-game `GetDisabled`), `get_global` reports `value`. References and forms are named by
global FormID, Editor ID, or `"player"`.

```json
{"format": "fnv.sim.trace/1", "seed": 0, "player_level": 1,
 "steps": [{"step": 0, "op": "activate", "result": {"result": "picked_up", "...": "..."}}],
 "observations": [{"label": "caps after taking five", "step": 10, "count": 5},
                  {"label": "player after leaving", "step": 19, "cell": "0x000846EA",
                   "pos": [-73162.93, 1312.43, 8686.43], "rot": [0.0, 0.0, 1.27]}],
 "final": {"0x00000014": {"base": "0x00000007", "inventory": {"0x0000000F": 12}, "...": "..."}}}
```

Lab comparator: **keyed** on `label` over `/observations` (`count`, `cell`, `pos` with an
absolute tolerance). `steps` and `final` are diagnostics, not compared: their outcome
names are this engine's own.

### `fnvtool sim forms <lo> --schema <dir>` → `fnv.sim.forms/1`

Observational report behind §1–§5 (counts per base type, item value/weight coverage,
carried-item targets, leveled-list statistics, reference flags, locks, teleport
reciprocity, loader issues). Not a scenario.
