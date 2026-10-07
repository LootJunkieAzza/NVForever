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
  `MineNVPowderChargeProjectile`, a C4 projectile [observed]. All 297 decode completely
  with the REFR subrecord layout [observed]. Each projectile is fired by a weapon (WEAP
  DNAM `projectile` [documented: xEdit]); for 290 placed explosives exactly one weapon
  fires the base projectile — Frag Mine 219, Powder Charge 33, Satchel Charge 25, Plasma
  Mine 10, Bottlecap Mine 3, all with the Land Mine or Mine Drop animation type — and 7
  use projectiles that several weapons fire [observed]. They are armed mines and charges;
  activating one disarms it and yields that weapon [hypothesis; the Explosives check and
  the chance to set it off are Layer 6].
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
  DATA, weight in DAT2; ALCH weight is DATA (f32), value in ENIT; LIGH DATA; IMOD DATA
  (value u32, weight f32); CCRD DATA (value u32, no weight). NOTE and CHIP have no value
  or weight field; CMNY has only DATA "Absolute Value" u32 (byte offsets: §7 `sim
  items`). CCRD also holds two INTV u32, suit then card value (xEdit enums: suit 1 Hearts,
  2 Spades, 3 Diamonds, 4 Clubs, 5 Joker; value 1 Ace, 2–10, 12 Jack, 13 Queen, 14 King,
  15 Joker) [documented: xEdit]; `CardClubs2Tops` ("2 of Clubs") has INTV 4 then 2
  [observed], consistent with that reading. Schema layouts for IMOD, CHIP, CCRD, CMNY were
  added 2026-10-07 and decode every instance in the ten masters (`fnvtool esm coverage`)
  [observed]. Five AMMO records, all flagged Non-Playable creature ammunition
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
- **Respawn** [hypothesis: cell reset as commonly described for this engine generation]:
  when a cell is loaded again after staying unloaded for at least the respawn delay, its
  containers whose base has the Respawns flag return to their authored state (new
  leveled roll, lock restored, anything left inside lost). No game setting holds the
  delay in the masters (§5b); the simulation uses 72 game hours by default
  (`SimConfig.respawn_hours`), a figure to be captured. Other references are not reset
  yet. The engine reports unloading/loading (`detach_cell` / `attach_cell` ops).
- **Inventory model**: one stack per base item, split into groups of identical extra data
  (condition, owner, equipped). Removing more than is held removes what is there and
  reports that number [hypothesis: GECK `RemoveItem` behaviour as commonly described; to
  be captured]. Removal takes plain items first, then unequipped items with extra data in
  arrival order, equipped items last; items leaving an inventory are unequipped; extra
  data travels with transferred and dropped items, and a drop makes one reference per
  group [hypothesis].
- **Per-item extra data in the data** [observed, six masters]: COED (owner FormID, a u32
  whose meaning depends on the owner type, condition f32 [documented: xEdit]) appears
  1,661 times, always directly after a CNTO (1,419 on NPC_, 122 on CONT, 120 on CREA);
  its condition values are fractions between 0 and 1 (most common 0.5, 0.85, 0.65); only 2
  set an owner. Placed weapons and armour carry **XHLP** "Health" (460 WEAP, 217 ARMO
  references in FalloutNV.esm, also fractions), never other types. Condition is therefore
  a fraction of the item's full health (WEAP/ARMO DATA `health`) [hypothesis]. A COED on
  an entry that names a leveled list applies to whatever the list yields [hypothesis].
- **Equipping** [hypothesis: GECK wiki `EquipItem`; to be captured]: weapons replace the
  equipped weapon; armour replaces equipped armour sharing any body slot (ARMO BMDT biped
  flags [documented: xEdit]); aid items (ALCH) are consumed; other items cannot be
  equipped.
- **Encumbrance**: capacity = `fAVDCarryWeightsBase` + `fAVDCarryWeightMult` × Strength
  (150 + 10 × Strength in the masters); carried weight includes equipped items;
  ammunition weighs nothing outside Hardcore mode — all three [observed in game,
  2026-10-07, `L4.sim.trace.goodsprings_weight_and_time`: GetAV CarryWeight 200 at
  Strength 5 and 160 at Strength 1; GetAV InventoryWeight 18.5 for a 9mm pistol (1.5),
  leather armour (15), brahmin-skin outfit (2) and 100 9mm rounds (0.029 each), unchanged
  by equipping]. Over-encumbered when weight exceeds capacity [hypothesis]; Hardcore
  ammunition weight not captured yet. Strength comes from NPC_ DATA attributes
  (S.P.E.C.I.A.L. as seven u8 in that order [documented: xEdit]); the player base form
  authors 5 for every attribute [observed]. NPCs that take stats from a template are not
  followed yet.

## 4. Leveled item lists (LVLI)

Data [documented: xEdit]: LVLD chance none (u8, percent), LVLF flags (bit 0 "Calculate
from all levels <= player's level", bit 1 "Calculate for each item in count", bit 2
"Use All"), optional LVLG global, repeated LVLO entries (level u16, item FormID, count
u16 — the count is optional in the layout but present in all 16,950 entries [observed]).
3,479 lists; 7,871 entries name another list; flags: all-levels 1,526, each 2,042,
use-all 723; 41 lists have LVLG [observed].

Selection rules, implemented in `fnv_sim::leveled`; first derived from the flag names
only, now **consistent with the game** for the cases below [observed in game,
2026-10-07, `L4.sim.leveled.basic`: 300 `AddItem` draws per case on the owner's install
against 20,000 candidate draws; chi-square p ≥ 0.15 in every case, alpha 0.01, so only
differences of several percentage points would have shown]. Covered: chance none (50,
65), "each item in count" with count 3 (outcomes 0–3 stimpaks: 43/103/125/29 in game,
consistent with three independent rolls), all-levels vs highest eligible level
(`VFactionSquadStimpaks`: always 1 stimpak at player level 10, always 3 at level 30, in
game and candidate), Use All (`VendorChestStimpaksBoomer`), a uniform pick of 18
(`ClutterHouseholdJunkNV`). Console `AddItem` with a leveled list resolves it against
the player's level [observed, same captures]. Not covered yet: LVLG globals, deep
nesting, levels between entries' levels with the all-levels flag set.
1. Roll 0–99; below chance none (or the LVLG global's value) yields nothing.
2. Eligible entries have level ≤ the requesting level (the player's level; for
   containers [hypothesis], for AddItem [observed]).
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
- **In game** [observed 2026-10-07, `L4.sim.trace.goodsprings_doc_mitchell`, scripted
  `Activate` with the player as activator, Doc Mitchell's house]: three stimpak and five
  cap references each gave their item (counts 3 and 5); **activating an already-taken
  stimpak reference (0x00105BF3) again gave a fourth stimpak** — so after a scripted
  pick-up the reference can still be activated (whether it stays enabled in the world is
  not captured yet); `AddItem` 10 then `RemoveItem` 3 caps gave 12 (from 5);
  `RemoveItem` 5 stimpaks with 4 held left 0. The front door
  (`GSDocMitchellHouseIntDoorREF`) put the player at x −73162.92, y 1312.39 (the XTEL
  position within 0.05) but **z 8693.87, 7.44 above the XTEL z 8686.43**, and in cell
  **0x000DAEBB "Goodsprings" (WastelandNV grid −18, 0)**, not in 0x000846EA, the
  WastelandNV persistent cell that holds the destination door: after a teleport into a
  worldspace the actor is in the exterior grid cell containing its position. The z offset
  is presumably the character controller settling the actor [hypothesis; physics
  workstream].
- **Scripted `Activate` on item references** [observed 2026-10-07, two follow-up captures
  in Doc Mitchell's house]: `ref.Activate player` adds the item's stack every time — three
  activations of stimpak 0x00105BF3 gave 1, 2, 3 stimpaks; a stimpak taken by hand
  (0x00105BF5) still gave one more; a cap reference that was never taken but disabled with
  `Disable` (GetDisabled 1) also gave its cap and stayed disabled. `GetDisabled` stays 0
  and `GetParentCell` keeps reporting the original cell after a pick-up by script or by
  hand, so neither shows whether an item was taken. (The player found 2 of the 3
  stimpaks still on the shelf after the scripted activations, i.e. the scripted pick-up
  did take the first one [to be confirmed by the owner].) `GetParentCell` of a persistent
  exterior reference (door 0x00103E69) asked from an interior returns no cell
  ("00000000"); whether it reports the grid cell when asked outdoors is not captured yet.
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
  advances GameDay with Gregorian month lengths, GameMonth counting from 0. In game
  [observed, same scenario]: waiting 24 + 6 hours from day 18, month 9 gives day 19,
  month 9; from day 31, month 9 at 20:00, waiting 6 hours gives day 1, month 10. This
  fits month 9 = October (31 days) but does not yet distinguish it from a rollover of an
  out-of-range day 31 in a 30-day month; a capture across the end of a 30-day month would.
  While the player is in menus the clock keeps running at TimeScale (a first capture read
  GameHour 14.136 for 14), so fractional time is not compared
  (docs/spec-notes/xnvse-capture.md §5).
- Globals changed at run time are part of the simulation state (`Snapshot.globals`).

## 6. Capturing the oracle (in-game)

Layer 4 scenarios compare *observations* (item counts, position, cell) that the original
game can report through console functions [documented: GECK wiki function pages
`GetItemCount`, `GetPos`, `GetParentCell`, `AddItem`, `RemoveItem`, `RemoveAllItems`,
`Activate`]. Procedure by hand (or automated, below):
1. Load the pinned save at the scenario's start location (Goodsprings: inside Doc
   Mitchell's house).
2. Run the scenario's steps as console commands (`player.removeallitems`,
   `<ref>.activate player`, `player.additem <id> n`, …); record each query's value.
3. Write the values into the golden `sim/<scenario>.trace.json` with the same `label`s.
Until then the scenarios report `error` (missing golden), as Layer 1's xEdit scenarios did.

**Automated capture**: the `nvf_capture` xNVSE plugin runs a scenario's steps in the
original game and records the query results (docs/spec-notes/xnvse-capture.md;
tools/capture/README.md §10). `capture_steps.py` writes its jobs from the files in
`parity-lab/inputs/layer4/` using the console equivalents below (ops without one, such as
waiting or changing cells, become prompts) and turns its output into the goldens.

**Console equivalents of the trace ops** [documented: GECK wiki function pages; the
mapping itself is this project's]: `remove_all_items` → `player.removeallitems`;
`add_item` → `player.additem <id> <n>`; `set_av … Strength v` → `player.setav strength
<v>`; `get_weight` → `player.getav InventoryWeight` (`weight`) and `player.getav
CarryWeight` (`capacity`); `equip_item` → `player.equipitem <id>`; `set_global` → `set
<Global> to <v>`; `get_global` → `show <Global>`; `advance_time h` → waiting h hours
(Wait menu, at most 24 at a time); `get_disabled` → `<ref>.getdisabled`; `enable` /
`disable` → `<ref>.enable` / `<ref>.disable`; `detach_cell` / `attach_cell` → leaving
the cell through its door / coming back (wait outside, in another cell, in between);
`get_item_count` on a container → `<ref>.getitemcount <id>`; `remove_all_items` on a
container → `<ref>.removeallitems`. FormIDs in the inputs are load-order
FormIDs of the ten masters in the order of this note, which is also the console's
numbering when the game loads them in that order (Old World Blues = 03). Use a new
level-1 character without perks or implants, so no modifier changes carry weight
(`goodsprings_weight_and_time`).

**Leveled-list samples** (distribution scenarios, golden `sim/<name>.samples.json` in the
`fnv.sim.samples/1` shape of §7, with `outcomes` only). Per case of the scenario's
`fnv.sim.cases/1` file, on a test character whose level is set to the case's `level`
(`player.setlevel <n>`), repeat `samples` times: `player.removeallitems`, then
`player.additem <list> <count>`, then note every item and count the inventory now holds
(the inventory screen, or `player.getitemcount <id>` for the list's possible items).
Record each draw's outcome key (§7) and add up the counts per key. That console
`AddItem` resolves a leveled list, against the player's level, the way containers and
actors do is a [hypothesis] [`AddItem` documented: GECK wiki]; a resolution inside a
spawned container (`placeatme` of a container base holding the list) is the
cross-check. A few hundred draws per case are enough for the comparator to see
differences of a few percentage points; `capture_steps.py cases` makes this a job for the
`nvf_capture` plugin.

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

### `fnvtool sim items <lo> --schema <dir>` → `fnv.sim.items/1`

```json
{"format": "fnv.sim.items/1", "count": 2,
 "items": [
   {"formid": "0x0000000F", "sig": "MISC", "edid": "Caps001", "name": "Bottle Cap",
    "value": 1, "weight": 0.0, "health": null, "biped_flags": null}]}
```

- One object per winning, non-deleted record of an inventory item type (§3: WEAP, ARMO,
  AMMO, ALCH, BOOK, KEYM, MISC, NOTE, IMOD, CHIP, CCRD, CMNY, and LIGH only with "Can be
  Carried"), ordered by global FormID. Winners, deletion and FormID globalisation as for
  `sim containers`.
- `sig`: the record type. `edid`, `name`: EDID, FULL (NUL-terminated Windows-1252),
  `null` when absent.
- `value` (integer), `weight` (f32 widened to a JSON number), `health` (integer) and
  `biped_flags` (integer) are read from these fields [documented: xEdit
  `wbDefinitionsFNV.pas`; byte offsets within the subrecord, little-endian]:

  | Type | value | weight | health | biped_flags |
  |---|---|---|---|---|
  | WEAP | DATA +0 i32 | DATA +8 f32 | DATA +4 i32 | — |
  | ARMO | DATA +0 i32 | DATA +8 f32 | DATA +4 i32 | BMDT +0 u32 |
  | AMMO | DATA +8 i32 | DAT2 +8 f32 | — | — |
  | ALCH | ENIT +0 i32 | DATA +0 f32 | — | — |
  | BOOK | DATA +2 i32 | DATA +6 f32 | — | — |
  | KEYM, MISC | DATA +0 i32 | DATA +4 f32 | — | — |
  | LIGH | DATA +24 u32 | DATA +28 f32 | — | — |
  | IMOD | DATA +0 u32 | DATA +4 f32 | — | — |
  | CCRD | DATA +0 u32 | — | — | — |
  | NOTE, CHIP, CMNY | — | — | — | — |

  "—" means the record format has no such field: the value is `null`. A field is also
  `null` when its subrecord is absent or shorter than offset + 4 bytes, and a weight that
  is not finite (NaN, ±infinity; none occur) is `null`. When a subrecord occurs more than
  once, the first occurrence is used (none does in the masters). A record that cannot be
  decoded is still listed, with every field `null`. CMNY DATA is named "Absolute Value"
  by xEdit and is not treated as the item's value [hypothesis: it is the denomination in
  the Caravan game]. Known difference, not exercised by the data: `fnvtool` reads fields
  through the Layer 1 schema, which decodes a subrecord whole or not at all, so a
  truncated subrecord makes *all* its fields `null` there, not only those past the end
  (no truncated item subrecord exists in the ten masters [observed]).
- In the ten masters [observed, `oracle_sim.py items`]: 3,972 items (NOTE 1,225, MISC 603,
  WEAP 496, ARMO 489, CCRD 324, KEYM 313, ALCH 226, AMMO 145, IMOD 112, BOOK 27, CHIP 6,
  CMNY 6, carriable LIGH 0 — all 728 LIGH lack "Can be Carried"); the only `null` among
  fields the format has are the five AMMO weights of §3; every CCRD has value 2 and every
  IMOD weight 0.0.

Lab comparator: **keyed** on `formid` over `/items`, all fields (`L4.sim.items`, against
the independent oracle).

### `fnvtool sim run <lo> --schema <dir> --actions <file>` → `fnv.sim.trace/1`

Input `fnv.sim.actions/1` (`parity-lab/inputs/layer4/*.actions.json`): `seed`,
`player_level`, `actions` (ops: `activate`, `add_item`, `remove_item`, `transfer`,
`transfer_all`, `drop_item`, `remove_all_items`, `get_item_count`, `get_pos`,
`get_disabled`, `get_global`, `set_global`, `advance_time`, `equip_item`,
`unequip_item`, `get_av`, `set_av`, `get_weight`, `detach_cell`, `attach_cell`, `lock`,
`unlock`, `enable`, `disable`), optional `hardcore` (default false) and `report`.
Queries (`get_*`) produce observations; `get_disabled` reports `disabled` (the in-game
`GetDisabled`), `get_global` and `get_av` report `value` (`get_av` knows the seven
S.P.E.C.I.A.L. attributes), `get_weight` reports `weight`, `capacity` and `over`
(in-game: `GetAV InventoryWeight`, `GetAV CarryWeight` [hypothesis on the actor value
names]). `drop_item` reports `references` (one per group of identical extra data).
`detach_cell {cell}` / `attach_cell {cell}` (cell by global FormID or Editor ID) mark the
cell unloaded / loaded again; `attach_cell` reports `reset`, the references whose
contents respawned (§3 "Respawn"). `activate` on a placed explosive (PGRE) reports
`"result": "disarmed"` with the `item` (weapon) added to the activator (§1). In `final`, a
stack without extra data is a count; otherwise `{"count": n, "groups": [{"count": k,
"condition": c, "owner": id, "equipped": true}]}`. References and forms are named by
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

### `fnvtool sim sample <lo> --schema <dir> --cases <file>` → `fnv.sim.samples/1`

Leveled-list outcome histograms: the candidate side of the distribution scenarios
(spec section 8.1). Input `fnv.sim.cases/1` (`parity-lab/inputs/layer4/*.cases.json`):

```json
{"format": "fnv.sim.cases/1", "seed": 0,
 "cases": [{"label": "stimpak 50 %", "list": "LootChemsStimpak50", "level": 1,
            "count": 1, "samples": 20000}]}
```

(Counts in the examples are illustrative.) `list` is an LVLI named by Editor ID or global FormID (`"0x…"`); `level` (default 1) is
the requesting level; `count` (default 1) the number requested, as in a CNTO entry or
`AddItem`; `samples` (> 0) the number of independent draws. Output:

```json
{"format": "fnv.sim.samples/1", "seed": 0,
 "cases": [{"label": "stimpak 50 %", "list": "0x00177FAF", "level": 1, "count": 1,
            "samples": 20000,
            "outcomes": {"nothing": 10012, "0x00015169 x1": 9988}}]}
```

- Cases in input order; `list` as a global FormID string.
- One draw resolves `count` of `list` at `level` with the leveled-list rules of §4
  (nested lists included). The items drawn are added up per item FormID; the outcome key
  is the items in ascending FormID order, each written `"0x%08X x<n>"`, joined by `", "`;
  a draw that yields no item is `"nothing"`.
- `outcomes` maps each key that occurred to its number of draws (their sum is `samples`).
- Determinism: one RNG stream seeded with `seed`, consumed by the cases in order and the
  draws in order, so the same file gives the same document.
- A case whose `list` is not an LVLI, or whose resolution fails (nesting too deep), gets
  `"error": "<text>"` and no `outcomes`; the other cases still run (exit code 0).
- Optional, not required: `"probabilities": {key: p}` with the exact distribution; when
  present, the comparator tests goodness of fit against it instead of comparing samples.

Lab comparator: **distribution** over `/cases`, keyed on `label`, `field` `outcomes`
(parity-lab/README.md). Goldens are in-game samples (§6).

### `fnvtool sim forms <lo> --schema <dir>` → `fnv.sim.forms/1`

Observational report behind §1–§5 (counts per base type, item value/weight coverage,
carried-item targets, leveled-list statistics, reference flags, locks, teleport
reciprocity, loader issues). Not a scenario.
