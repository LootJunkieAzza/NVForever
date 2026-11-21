# Spec note: skill books

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading nothing has confirmed yet. Implementation:
`crates/fnv-sim/src/forms.rs` (`Item::teaches`, `Item::cant_be_taken`),
`crates/fnv-sim/src/world.rs` (`equip_at`, `activate_as`), `crates/fnv-game/src/books.rs`
(`Game::read_book`), `engine/nvf-play/src/aid.rs` (`read_book`, the log line),
`engine/nvf-play/src/script_events.rs` (the corner line). ESM gap audit row 4
(docs/handoff/esm-gap-audit-2026-10-21.md).

## 1. The records

- **BOOK DATA** [documented: xEdit wbDefinitionsFNV.pas BOOK, `schemas/fnv/objects.json`]:
  flags (u8: bit 0 "Scroll", bit 1 "Can't be Taken"), skill (i8, wbSkillEnum: -1 None,
  0 Barter … 13 Unarmed), value (i32), weight (f32). The skill is the editor's "Teaches"
  [documented: GECK wiki Book, "Teaches: The Actor Value that is increased"]; the skills'
  actor value codes run in the same order from Barter 32 to Unarmed 45 [documented: xEdit
  ActorValue].
- **What the masters hold** [observed: `fnvtool esm inventory` and `esm decode` of the six
  masters, 2026-10-21]: 26 BOOK records, all in FalloutNV.esm. Fourteen teach a skill,
  one per skill (BookSkillGuns "Guns and Bullets" … BookSkillSurvival "Wasteland Survival
  Guide"), value 50, weight 2, no flags. The rest teach nothing (the quest's Wasteland
  Survival Guide and manuscript, Big Book of Science and Motivational Secrets of MS03,
  Paradise Lost, the DONOTUSE generics, You're SPECIAL!). Two carry "Can't be Taken":
  BabyBookStatic (You're SPECIAL!) and MS03GuideManuscript (no model). The add-ons add no
  BOOK.
- **The setting**: `fBookPerkBonus`, 3.0, is the only game setting of the masters whose
  name has "Book" in it [observed: every GMST of FalloutNV.esm]. No `iBookSkill*` or
  `fBookSkill*` setting exists.
- **The perk**: Comprehension's one entry point is code 11, Add 1 [observed; levelup.md
  §9]; its description: "you gain double the bonus from reading magazines and one
  additional point from skill books" [observed: PERK DESC]. The GECK wiki's entry point
  table: "Adjust Book Skill Points", "Run when the actor reads a book", "Modifies the skill
  points gained", example Comprehension [documented: GECK wiki Perk Entry Points]. The
  magazines' doubled bonus is the magazines' own conditioned effects (ALCH), not this.

## 2. Reading

- **From the inventory**: using a skill book in the inventory menu or the Pip-Boy reads
  it; one is taken from the inventory (`World::equip_at` treats it as used up, as an aid
  item) [documented: GECK wiki Book: "When a book is 'used' in the player's inventory" …;
  the wiki's "Activating the book 'teaches' an actor value"]. A book that teaches nothing
  is not used (its OnEquip block, which the wiki names for such books, is not built).
- **EquipItem** of a skill book reads it as the inventory's use does [hypothesis: the wiki
  says EquipItem treats an item "as though it were used normally"].
- **In the world**: a placed skill book is picked up into the inventory like any item, to
  be read from there [hypothesis: the masters keep skill books in inventories to be given
  away or traded — the prison script's `HanniganTrained`, "Player gave Hannigan a Medicine
  skill book", and Old World Blues' X-13 reward `AddItem BookSkillSneak` [observed: script
  sources]; vendors sell them, "Books: Can sell Skill Books" [documented: GECK wiki AI
  Data]. Whether the original reads a book on the spot when it is picked up is not
  checked; the owner may say].
- **Which tab**: the books stay under MISC in the menus, as before [hypothesis: the
  original's tab is not checked].

## 3. The points

- A book is worth `fBookPerkBonus` (3; 3 when the setting is missing) [hypothesis: the
  setting's name speaks of a perk bonus, but it is the masters' only book setting and the
  perk's own entry is Add 1, so it is read as the book's base], then the actor's entry
  point 11 entries change it (`Game::entry_point`: owner conditions, ranks, priority), so
  Comprehension makes 4.
- The skill's **permanent modifier** rises by the points [documented: GECK wiki Actor
  Value, "Permanent Pool … the in-game sources for this modifier come from skill books"]:
  ModActorValue's pool, kept in saves (`ExtraState::av_permanent`), read by
  GetActorValue and GetPermanentActorValue like ModActorValue's changes.
- Whole points, never past 100 for the permanent value, never negative: a book read at
  98 gives 2, at 100 nothing, and is still used up [hypothesis].

## 4. "Can't be Taken"

The GECK wiki's Book page calls the flag "Unused" [documented]. NVForever honours it by
its name [hypothesis]: a placed reference of such a book is not picked up by hand (the
activation does nothing, `Outcome::NoDefault`, as for a non-playable weapon) and the
crosshair shows no prompt; a script's Activate still gives it, as for non-playable items.
No skill book carries the flag (§1).

## 5. What the player sees and what counts

- A corner message: "<Skill> increased to <value>" with the skill's icon (AVIF ICON)
  [hypothesis: the original's line is not in the plugins — no MESG or GMST of the masters
  names a book read — so wording and icon are NVForever's until the owner shows one].
- **Books Read** (misc statistic 15) counts one per book the player reads [documented:
  GECK wiki GetPCMiscStat lists "Books Read" for New Vegas; hypothesis: one per skill book,
  books that teach nothing do not count]. A challenge on Books Read steps with it once the
  challenges branch (PR #556) is in (`Game::count_stat`).
- Log: `book: the player read <book>: <Skill> a → b (+p[, worth w]); perks: <perk>
  (entry point 11); Books Read n` (`engine/nvf-play/src/aid.rs`), and the event
  `BookRead` in the trace.

## 6. Checks

- Unit tests: `fnv_game::books` (points, the top), `fnv-sim` synthetic tests (a skill book
  used up, a plain book refused, a book that can't be taken stays unless a script
  activates it).
- Smoke test `skill-books` (tools/smoke/scenarios.toml): two Guns and Bullets given, one
  read from the inventory menu, Comprehension added, the second read: Guns +3, then +4
  with the perk, Books Read 1 then 2.

## 7. Not built

Reading books that teach nothing (their OnEquip blocks), NPCs reading books, and the NVSE
book functions (GetBookSkill, GetBookFlags, …).
