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
  the chance to set it off are Layer 6]. In game [observed 2026-10-08, `bl_placed_mines`, Nipton house 8,
  god mode on]: the four frag mines stayed enabled and did not move for 600 game loops
  after the player arrived, and a scripted `<mine>.Activate player` changed nothing (no
  Frag Mine 0x0000433C in the inventory, the mine still enabled, same position). Whether
  the player's own activation disarms one is not captured (needs someone at the game).
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
  A disabled reference is not drawn, has no collision and cannot be interacted with by
  the player [observed by the owner in game, 2026-10-08] (scripted `Activate` on a disabled item still
  gives the item, §5).
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
  [observed], consistent with that reading. Notes, casino chips, caravan money and
  caravan cards weigh nothing in the inventory, like caps [observed by the owner in game, 2026-10-08];
  carried-weight totals count them as 0 (their `sim items` weight stays `null`, as the
  records have no field). Schema layouts for IMOD, CHIP, CCRD, CMNY were
  added 2026-10-07 and decode every instance in the ten masters (`fnvtool esm coverage`)
  [observed]. Five AMMO records, all flagged Non-Playable creature ammunition
  (e.g. `AmmoBloatflyDart`), have no DAT2 and therefore no weight [observed].
- **Carried items** (CNTO, repeated: item FormID, count i32) appear on CONT, NPC_ and
  CREA [documented: xEdit]. In the ten masters [observed]: 30,414 entries; 7,365 on
  containers and 15,993 on actors name a leveled item list (LVLI) instead of an item;
  **105 entries have count 0** (none negative; none on containers, so all on NPC_/CREA,
  e.g. the Vault 21 gamblers' templates); 6 entries name non-items (three STAT/MSTT in the
  test container `TestQAMisc`, three uncarriable LIGH on the Gomorrah bouncers).
  The simulation gives nothing for a count-0 or non-item entry: NPCs with count-0
  entries carry nothing for them [observed by the owner in game, 2026-10-08]; non-item entries stay
  [hypothesis].
- **CONT DATA**: flags u8 (bit 1 "Respawns"), weight f32 [documented: xEdit]. 3,417
  container bases; 177 respawn; 845 have no CNTO [observed].
- **Respawn** [hypothesis: cell reset as commonly described for this engine generation]:
  when a cell is loaded again after staying unloaded for at least the respawn delay, its
  containers whose base has the Respawns flag return to their authored state (new
  leveled roll, lock restored, anything left inside lost). No game setting holds the
  delay in the masters (§5b); the simulation uses 72 game hours by default
  (`SimConfig.respawn_hours`), a figure to be captured. Other references are not reset
  yet. The engine reports unloading/loading (`detach_cell` / `attach_cell` ops).
  **In game** [observed 2026-10-07, `L4.sim.trace.goodsprings_respawn`, since retired]: Ringo's cash
  container 0x001629C0 (base CaravanCashRingo, Respawns, 200 caps) in GSGasStation, emptied
  with `RemoveAllItems`, held **200 caps again after the player left, waited 24 hours
  outside and came back**, and again after 96 hours. So the delay for this container is at
  most about 24 game hours (plus the minutes spent walking), well under the 72 h default;
  A follow-up (`aa_respawn_delay`) read **`iHoursToRespawnCell` = 72 in the running game**
  (an engine default; no master defines it) [observed], and the same box stayed empty
  after 1 h and after 6 h away and was full after 12 h away (about 19 h after it was first
  emptied). So this merchant cash box restocks on a rule of its own, faster than the
  72-hour cell respawn [observed]. `aa_vendor_restock` (clock = GameDaysPassed): emptied at
  6.377, the player stayed inside 24 h → full at 7.379 (no cell unload needed); emptied
  at 7.379, 5 h away across midnight (back at 8.095) → empty; 10 h away without
  midnight (back at 9.476) → empty, i.e. empty for 50 h [observed]. Not hours away, not
  hours since emptying, not midnight. `aa_vendor_period` (player inside the whole time,
  never opening a menu; only scripted `GetItemCount` reads): full again at 7.379
  (emptied at 6.377); emptied at 7.379 and one stimpak put in (a sold item); still empty
  with the stimpak at 8.380, 9.382, 9.883; **full (200 caps) and the stimpak gone at
  10.384**, and full at 10.886 and 11.387 [observed]. So a restock **replaces** the
  contents (sold items are lost), needs no barter menu, and the second restock came
  between 2.50 and 3.01 days after the restock observed at 7.379; a fixed 3-day grid from
  day 0 is ruled out (no refill at day 9). Rule consistent with every capture
  [hypothesis]: when a merchant container is accessed and at least 72 game hours
  (`iHoursToRespawnCell`, 72 in game) have passed since its last restock, it is reset to a
  fresh roll of its base contents and the restock time becomes the access time. A save
  carries each container's last restock time, which the plugins do not, so captures that
  start from a save depend on that hidden state (the retired `L4.sim.trace.goodsprings_respawn`'s
  24 h case did; `L4.sim.trace.goodsprings_merchant_restock` avoids it). Merchant containers can be identified from the data: an
  ACHR/ACRE `XMRC` "Merchant Container" names them [documented: xEdit]; Ringo's
  reference 0x00104C7D points at 0x001629C0, and FalloutNV.esm has 77 XMRC links
  [observed].
  **Superseded by the 2026-10-08 captures** (`bl_restock_timing`, `bl_restock_schedule`,
  unattended, `GameHour` and TimeScale substitutes giving the same results; save `nvftest`
  inside GSGasStation, the box's last restock before the captures at midnight of day 11):
  the restock time is **not** the access time. Emptied at 12.388 and left unread for
  100 h, the box was full at 16.556; emptied again at once, it was **full again at
  17.556** (24 h later). Read and emptied every 2 hours, it refilled just after the
  midnights of days 14, 21 and 24 (first reads at 14.063, 21.077, 24.007), and was full at
  the first read (20.658) after 100 unread hours [observed]. Every refill seen in any
  capture (also days 7 and 10 in `aa_vendor_period`, day 17 here) came just after a
  midnight (GameDaysPassed crossing a whole number). Rule consistent with all of them
  [hypothesis]: **at each midnight d, a merchant container whose contents changed since its
  last restock L, with d − L ≥ 3 days, is restocked (contents replaced) and L = d.** Here
  L = 11 → 14 → 17 (in the unread gap) → 20 skipped (untouched and full since 17) → 21
  (emptied at 20.658, 4 days after 17) → 24. The "changed since" condition rests on that
  one skipped midnight; "skipped when full" or "skipped when untouched" fit equally. The
  period matches `iHoursToRespawnCell` (72 h) [hypothesis that it is the source].
  **Refuted the same day; restocks come on fixed days** (later 2026-10-08 captures, all
  unattended from `nvftest` with the player inside and time moved by `GameHour`; jobs from
  `tools/capture/jobs/make_restock_jobs.py` and the scenario
  `L4.sim.trace.goodsprings_merchant_midnight_restock`). The save's calendar: GameDaysPassed
  12.388 is 25 October 2281 (GameMonth 9), day 19 is 1 November [observed,
  `mr_diag_calendar`]. Restocked at day 14 and left full and untouched, the box emptied at
  19.4 was **not** refilled at midnight 20 (6 days after its last restock) but at 21
  (`goodsprings_merchant_midnight_restock`, and again with 2-hourly reads in
  `mr_diag_reads`: reads change nothing). Restocked at 17 and emptied at once, it was not
  refilled at 20 but at 21 (`mr_diag_month_short`, `mr_diag_ringo`), while restocked at 14
  or 21 and emptied at once it refilled 3 days later (17, 24). Restocked at 21 and left
  untouched, it was emptied at 24.4, 25.4 or 26.4 and every time refilled at **28**
  (`mr_diag_untouched_3d`, `_4d`, `mr_diag_long_untouched`). Ringo (0x00104C7D) stayed in
  GSGasStation, enabled, with the same daily AI packages throughout (`mr_diag_ringo`), so
  his state does not explain the skipped midnights. Across every capture of both saves
  (also `bl_restock_timing`, `bl_restock_schedule`, `aa_vendor_restock`,
  `aa_vendor_period`) refills came only just after midnights 7, 10, 14, 17, 21, 24 and 28,
  and an emptied box was passed over at midnights 8, 9, 13, 15, 16, 18, 19, 20, 22, 23, 25,
  26 and 27 [observed]: refills happen exactly on days whose **GameDaysPassed is 0 or 3
  modulo 7** (twice a week, 3 and 4 days apart; equally, the first midnight at or after
  each point 3 + 3.5k), whatever the last restock or access. A held-out run made after the
  rule was stated (`mr_predict_days`: emptied on days 29, 31, 35 and 38, read daily)
  refilled on exactly 31, 35, 38 and 42 and on none of the nine days between [observed].
  Rule [hypothesis, consistent with every capture]: **at each midnight that starts a day d
  with d mod 7 ∈ {0, 3}, every merchant container whose base has the Respawns flag is
  restocked (contents replaced).** No hidden last-restock time takes part, so a capture
  depends on the save only through where its start falls in the 7-day cycle (the earlier
  "save's last restock time" readings were this phase). Open: whether the schedule is
  global (two saves agree; other merchants not captured), whether an untouched box is
  re-rolled (invisible with fixed contents; a merchant with leveled stock would tell), and
  why GameDaysPassed modulo 7 (no week or restock setting appears in the masters;
  `GetGameSetting "iDaysToRespawnVendor"` returned 0 in game [observed], which may only
  mean that no such setting exists). The "changed since the last restock" condition and
  the link to `iHoursToRespawnCell` are dropped: the one skipped midnight they rested on
  (day 20) is not a restock day.
  A merchant box whose base lacks the Respawns flag (Primm, Prospector's Den,
  0x0017409C, base `VLootSackVendorWastelandT2RSPros`; `bl_primm_restock`, both
  substitutes) was emptied and **never refilled in 7 days** of daily reads [observed]; so
  the flag gates merchant restocks too. After its `RemoveAllItems` it reported
  `GetItemCount` of caps 1 with `GetNumItems` 0, every read [observed; unexplained].
  Its contents on arrival differed between two loads of the same save (540 and 380 caps,
  42 stacks each) [observed]: the save did not hold them (cell never visited by that
  character) and they were rolled when the cell first loaded [hypothesis].
  **Cell respawn of ordinary containers** (`bl_container_respawn`, both substitutes;
  General Store, the player leaving and returning with the console's `coc`): the fridge
  0x00105824 (base `VLootFridgeClosedTier1WastelandRS`, Respawns flag), emptied, was still
  empty after 24 h and after 70 h away and held new contents (4–5 stacks) after 74 h away,
  twice; the toolbox 0x001057FF (no Respawns flag) stayed empty throughout [observed]. So
  the 72 h cell respawn (`iHoursToRespawnCell`) applies to containers with the Respawns
  flag, measured from leaving the cell, between 70 and 74 hours [observed]; the exact
  boundary and whether a midnight is involved are not settled.
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
  by equipping]. Weights add exactly (199 teddy bears at 1.0 → 199, plus spoons at 0.5 → 200.5, 201) [observed, `bl_encumbrance`]. Over-encumbered when weight exceeds capacity [hypothesis]: neither `IsPlayerOverencumbered` nor `GetIsPlayerOverencumbered` exists in the runtime's script compiler ("unknown variable", with and without `player.`) [observed], so the threshold is not captured yet. Hardcore
  ammunition: 100 9mm rounds weigh 0 normally and **2.9 with Hardcore on** (0.029 each, the AMMO DAT2 weight), 0 again when switched off [observed, `bl_encumbrance`, `player.SetHardcore`]; `IsHardcore` read 0 throughout, even while the ammunition weighed 2.9 [observed; unexplained]. Strength comes from NPC_ DATA attributes
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
the player's level [observed, same captures].

Also **consistent with the game** for backlog B8 [observed in game, 2026-10-08,
`L4.sim.leveled.b8`, `parity-lab/inputs/layer4/leveled_b8.cases.json`: 300 `AddItem`
draws per case, unattended, save `nvftest`; chi-square p 0.16–1.0 in all ten cases]:
- **LVLG replaces chance none, at the global's current value.** The 41 lists with LVLG
  [observed] use `ScroungerOdds` (0x000645CD), `FortuneFinderOdds` (0x000645CC), both
  authored 100 and 100 in the save, or one-list vendor globals. With the global set from
  the console (`set ScroungerOdds to …`; read back unchanged before and after the
  draws): `AmmoMissilelauncherScrounger` (LVLD 0) at 100 gave nothing in 300/300;
  `Ammo20GaScrounger` (LVLD 100) at 25 gave the ammo in 238/300 (75 % expected);
  `Ammo10mmScrounger` (LVLD 0, each item) ×4 at 50 gave 0–4 rounds 14/78/122/70/16
  (binomial 4, ½: 19/75/113/75/19). So the global overrides LVLD in both directions
  (not a maximum or minimum of the two), and a value set at run time counts. Not
  covered: fractional or out-of-range values (the candidate truncates and clamps to
  0–100 [hypothesis]).
- **All levels, player level between entry levels:** `CapsFortuneFinder` (entries at
  1–6, 8, 8, 9; global set to 0) at level 7 gave only the six entries of levels ≤ 6,
  41–62 each; `VendorArmorRare75` (chance none 25, entries at 5, 5, 5, 7, 7, 12, 12) at
  level 10 gave nothing 69 and the five entries of levels ≤ 7, 36–54 each.
- **Level equal to an entry's level is eligible:** `VendorFlakShrapnelMiniNukes`
  (entries ×1 at 1, ×2 at 5, ×3 at 8, ×3 at 11) at level 8: ×1/×2/×3 = 100/97/103.
- **Count without "each item":** `NVDLC01ChipsChangeFortuneFinder` ×2 (all levels, global
  40) at level 12: nothing 110, 40/50/60 chips 66/71/53 (twice the entries of levels 1, 5,
  10); no mixed sums, so one roll times the count.
- **Nesting:** `VendorBasicMedicalSupplies100` (3 deep; sublists with entries at levels 1
  and 2/3; an entry ×5 naming the "each item" list `LootChemsStimpak75`, chance none 25)
  at level 2 gave 2–5 stimpaks as well as 1 (2/12/16/16 for ×2–×5), so a nested entry's
  count reaches an "each item" sublist as that many rolls; `LootChemsNVAll50` (3 deep,
  chance none 50 at the top and in a sublist; 17 outcomes) and `LL2Tier4ExplosivesMixed50`
  (5 deep, Use All at the top, chance none in sublists; 36 outcomes seen) agree too.
1. Roll 0–99; below chance none (or the LVLG global's current value) yields nothing.
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
  first (a locked container with its key in the inventory opens directly, no prompt
  [observed by the owner in game, 2026-10-08]). A base form's script (SCRI) receives OnActivate; whether an OnActivate block
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
  did take the first one [observed, owner].) `GetParentCell` of a persistent
  exterior reference (door 0x00103E69) asked from an interior returns no cell
  ("00000000"); asked outdoors it returns **0x000DAEBB, the WastelandNV grid cell (−18, 0)
  containing the door's position** (x −73349.5, y 1296.9), not the persistent cell that
  stores the door; the player's own cell outdoors was likewise the grid cell containing
  their position (0x000DAEBD, grid −19, 0, at x −73989.5, y 3750.2) [observed].
- **Created references** (dropped items) get FormIDs from 0xFF000000 upwards, a load
  slot no plugin can use [hypothesis for the original engine's numbering].
- **Crosshair activation in the game** (`engine/nvf-play/src/activate.rs`; values and
  rules to be captured, none observed yet):
  - **Reach**: 150 game units from the eye [hypothesis: placeholder, about two metres at
    ~70 units per metre]. No game setting in the masters names an activation distance
    [observed: FalloutNV.esm has no GMST whose Editor ID contains "Activat" or "Pick"
    other than script and lockpicking-XP settings]; the original's value is presumably
    built into the executable [hypothesis]. Capture idea: with the prompt showing, step
    back until it disappears and log the player-to-reference distance.
  - **Targeting**: the activatable reference nearest along the view within reach.
    Candidates are the reference whose collision the view ray hits first, and references
    whose render bounds (each mesh's box in its own space) the ray enters before the first
    collision hit of any kind, so walls and table tops hide what is behind them; a box the
    eye is inside does not count; ties go to the collision hit [hypothesis: many small
    items have no collision, yet the game lets the player pick them up]. Collision on the
    trigger layer (12) does not block the ray [hypothesis].
  - **What is offered**: references the simulation considers present (not taken, enabled)
    whose base is an item, container, door, actor or placed explosive with a weapon to
    give; activators and furniture only with a display name (FULL) [hypothesis: unnamed
    activators are trigger volumes and markers].
  - **Arrival heading through a door**: the XTEL rotation's z angle in degrees. Reference
    angles are radians and a z rotation turns clockwise seen from above (world.md §4
    [observed]), so z = 0 faces north (+Y) and z is the compass heading [hypothesis: that
    an actor faces its model's +Y; the heading after a teleport is not captured].
  - A door whose destination is in another worldspace is not followed yet (the player's
    simulated position is put back).

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
  **Settled** [observed 2026-10-08, `bl_calendar`: day 27 of every month of 2281 and of
  February 2280, then +24 h five times]: months count from 0 with Gregorian lengths 31,
  28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31 (GameMonth 9 = October); day 31 of month 11
  rolls to day 1, month 0 of the next year; **February 2280 has 28 days** — no leap days,
  although 2280 is a Gregorian leap year.
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
A case with `globals` first sets each one from the console (`set <global> to <value>`)
and reads it back before and after the draws (the golden keeps `globals_read` and
`globals_read_after` next to the counts; a value that drifted means the draws did not
run with the value asked for), then restores it.
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
`AddItem`; `samples` (> 0) the number of independent draws; `globals` (optional) maps
globals, by Editor ID or global FormID, to the values they hold during the case's draws
(as if set from the console first; e.g. a list's LVLG global), the others keeping their
authored values. Output:

```json
{"format": "fnv.sim.samples/1", "seed": 0,
 "cases": [{"label": "stimpak 50 %", "list": "0x00177FAF", "level": 1, "count": 1,
            "samples": 20000,
            "outcomes": {"nothing": 10012, "0x00015169 x1": 9988}}]}
```

- Cases in input order; `list` as a global FormID string; `globals` echoed as given
  (only when the case has any). A name that is not a global is a case error.
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
