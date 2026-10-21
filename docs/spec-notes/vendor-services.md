# Spec note: vendor services

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. Implementation:
`crates/fnv-game/src/services.rs` (the rules, `GetOffersServicesNow`),
`engine/nvf-play/src/menu.rs` and `menu/model.rs` (the barter menu),
`crates/fnv-ai/src/errand.rs` (Find's Allow Buying). ESM gap audit row 6
(docs/handoff/esm-gap-audit-2026-10-21.md).

## 1. Sources

- **The GECK wiki** (owner's export, C:\fnv-private\geckwiki): Class (the "Auto-Calc
  Buys/Sells" list and what each flag sells), Category:Packages ("Offers Services"),
  GetOffersServicesNow (its notes), GetClassFlag / SetClassFlag and GetPackageFlag (flag
  numbers), Category:NPC (vendors and owned world items), Barter System Formula. There is
  no "AI Data" or "Services" page in the export.
- **xEdit's record layout** through the schema (`schemas/fnv/gameplay.json`): NPC_/CREA
  AIDT `services` (ServiceFlags) at byte 8, `teaches` and `maximum_training_level`
  after it; CLAS DATA `services` after four tag skills and the class flags; ACBS flag
  bit 23 "Autocalc Service".
- **FalloutNV.esm** [observed, a reader of the masters' bytes in the session's scratch
  folder, nothing committed]: the vendors' service flags and stock, ingestibles' ENIT
  flags and ETYP equip types, the packages' general flags, and where
  GetOffersServicesNow is used.

## 2. Which categories a vendor sells

- **The flags** [documented: xEdit wbServiceFlags; GECK wiki GetClassFlag lists the same
  flags two numbers on]: 0 Weapons, 1 Armor, 2 Alcohol, 3 Books, 4 Food, 5 Chems, 6
  Stimpaks, 10 Miscellaneous, 14 Training, 16 Recharge, 17 Repair.
- **Whose**: an NPC sets "its buys and sells flags directly", unless it is auto-calculated,
  when its class's are used [documented: GECK wiki Class]. NVForever reads the AIDT flags
  (following the template while it supplies the AI data), and the class's (CLAS DATA)
  for an NPC with ACBS "Autocalc Service" [hypothesis: that flag is the auto-calc the page
  means, not "Auto-calc stats"; the class is the one the traits give]. 28 NPCs of
  FalloutNV.esm have the flag, mostly the travelling merchants (class VendorTraveler,
  whose flags match their own) and the Vault dweller spawns [observed].
- **Selling only**: "buys is not used in Fallout 3 or New Vegas and vendors can buy any
  item" [documented: GECK wiki Class]. The player can sell a vendor anything; only the
  vendor's own goods are limited.
- **An item's category**:
  - WEAP Weapons, ARMO Armor, BOOK Books [documented: GECK wiki Class, "Weapons: Can sell
    Weapons", "Armor: Can sell Armors", "Books: Can sell Skill Books"].
  - AMMO and IMOD (weapon mods) Weapons [hypothesis: Mick (Weapons, Armor, Repair) keeps
    loose .308, 12 gauge, 10mm and .22 rounds in his merchant box and has no
    Miscellaneous; observed].
  - An ingestible (ALCH) by its equip type ETYP: 10 Chems, 11 Stimpak, 12 Food, 13
    Alcohol; with another equip type by the wiki's flags, Medicine Stimpaks, Food Food,
    neither Chems [hypothesis. The wiki reads the ENIT flags ("Food: Ingestibles marked
    as food", "Chems: Ingestibles that are unmarked", "Stimpacks: ... marked as
    Medicine") and calls Alcohol unused. The equip types name the same four groups and
    fit the masters' vendors: Nuka-Cola and Sunset Sarsaparilla are unmarked (chems by
    the flags) but of equip type Food, and they are in the "FoodDrinkNoLiquor" lists that
    food-and-drink vendors like Trudy (Alcohol, Food) stock; the High Times chem dealer
    sells Alcohol and Chems, and liquor is marked Food but of equip type Alcohol; observed.
    Counts: 103 Food-marked of type Food, 28 unmarked Chems, 26 unmarked Food, 12
    Food-marked Alcohol, 5 Medicine Stimpak, 5 Medicine Chems (RadAway and the like), 5 of
    type 0].
  - Everything else, MISC and also keys, notes, lights, casino chips, Caravan cards and
    money, Miscellaneous [hypothesis for all but MISC].
  - Caps are the barter menu's money and are never held back.
- **Recharge, Training** are unused in Fallout 3 and New Vegas [documented: GECK wiki
  Class]; NVForever does nothing with them. **Repair** is the vendor's repair skill
  (docs/spec-notes/repair.md); `ShowRepairMenu` is opened by dialogue scripts, so the flag
  is not checked there [hypothesis: the dialogue's conditions decide, as for barter].
- **Hours**: Fallout New Vegas has no vendor hours in AIDT (Oblivion's buy/sell hours are
  not in the FO3/FNV layout [documented: xEdit wbAIDT]); when a vendor serves is its
  packages' "Offers Services" (§3).

## 3. GetOffersServicesNow

- **The rule**: 1 while the actor's current package has the general flag "Offers
  Services" ("If the actor is a service provider, he will offer services so long as this
  package is running. Actors that provide services will also try to keep the player in
  sight if not otherwise engaged in some task" [documented: GECK wiki Category:Packages];
  "Returns true if the calling actor offers services in his current package"
  [documented: GECK wiki GetOffersServicesNow]).
- **Not while** it talks with another actor, nor while it moves to its package's
  destination: "This command will return false if the npc gets into a conversation with
  another npc or while moving to a destination"; a travel package with the flag is "in a
  temporary NOT OFFER SERVICE state while they are moving" [documented: GECK wiki
  GetOffersServicesNow, notes]. "Moving to a destination" is read as the AI layer's
  Travelling phase before the package's goal is met [hypothesis]; an actor the AI does not
  move counts as being there [hypothesis].
- **Not** in combat (combat runs as a package of its own, `GetCurrentAIPackage` 5), nor
  dead, nor for the player [hypothesis]. A conversation with the player does not stop it
  [hypothesis: the function was meant for vendor dialogue conditions].
- **Use in the masters**: none. No condition (CTDA function 255) and no script source of
  FalloutNV.esm names it [observed]; 125 packages have the flag [observed], Trudy's bar
  package `GSTrudyAtBarPackage8x12` among them.
- **It does not gate barter**: `ShowBarterMenu` opens the menu whatever the vendor's
  package [observed: Chet's two packages and Mick's three have no "Offers Services" flag,
  yet their dialogue offers barter (Chet's "for sale" choice runs ShowBarterMenu) and no
  dialogue condition of the masters asks GetOffersServicesNow; hypothesis: the engine
  itself never checks the flag for barter]. The barter menu's log says whether the vendor offers
  services now, for the record.
- The note that the condition can crash New Vegas with a talking-activator vendor (The
  Sink) is not reproduced.

## 4. What NVForever does

- **The barter menu** (barter-menu.md §4): the merchant's rows are those of its stock
  whose category is among its services; the rest are held back (not listed, cannot be
  bought). Items the player puts up are listed on the merchant's side as before; once
  sold they are the merchant's, and held back like its own if out of its categories
  [hypothesis: whether the original lets the player buy them back is not observed]. The
  log names the services and what is held back: `barter: <vendor> sells Alcohol, Food
  (its services); offers services now: yes|no` and `barter: <ref> holds back N item(s)
  outside its services: <item> (<category>), ...`. Without the script data (no
  `GameData`) nothing is held back.
- **Find's Allow Buying** (packages.md §3.1): an actor buys from a vendor only what the
  vendor sells by the same rule [hypothesis: actors buy as the player does]. The vendor's
  package is not checked [hypothesis, as for barter].
- **GetOffersServicesNow** (opcode 0x10FF) by §3.

## 5. Checks

- `cargo test -p fnv-game --test vendor_services` (flags from AIDT, class and template;
  item categories; GetOffersServicesNow by package, on the way, combat) and the unit tests
  of `services.rs`; `cargo test -p fnv-ai --test errands` (the buying test's vendor sells
  Miscellaneous).
- Smoke `vendor-services` (Trudy in the Prospector Saloon: GetOffersServicesNow 1 at the
  bar, 0 after a package without the flag; a pistol put in her merchant box held back as
  Weapons). Its first run, 2026-10-21, showed the pistol held back and her services read
  as Alcohol, Food; GetOffersServicesNow read 0 at 12 s, 9.5 s after she was enabled with
  a 650-unit walk to the bar ahead [hypothesis: still on her way, which §3 counts as not
  offering; the log has no arrival line to tell], and 0 after the evening package; the
  first call was moved to 28 s, not run again (one run allowed).
