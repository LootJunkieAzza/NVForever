# Spec note: items moved by extender functions, and notes in containers (2026-11-15)

Why this exists: the owner's playtest of 2026-11-14 (Beyond the Beef, VMS18). Just
Assorted Mods' loot menu (JLM, a script mod with no DLL) would not take quest items, so
they needed the full container menu. Taking one there has to reach the scripts the way the
container menu's take does.

## 1. What the loot menu does [observed: NVForever desktop probes jlm4 to jlm9, 2026-11-15]

These were read from the engine's own log and frames, not from the mod's scripts.

- The menu lists a container's items, quest items included: a crate holding the Platinum
  Chip duplicate (MISC 0x0011678E), the Matchbook (NOTE 0x00112015, VMS18) and caps showed
  all of them.
- Its take key (the player's Activate key, sent as a key event while the menu holds the
  game's own Activate) takes the selected item with `container.RemoveItemTarget item player`
  (log line `script items: RemoveItemTarget: ...`).
- For the Matchbook it made no call at all while the container's `GetItemCount` of the note
  read 0.

## 2. Rules

- **Moves with the game's effects.** RemoveItemTarget, RemoveMeIR and RemoveMeIRAlt move the
  items out of their container. With a target they move into it as the game's own moves do:
  the removed and added events (`fnv.script.trace`'s item lines) and the item's OnAdd in its
  new holder, once per call as AddItem runs it. A note that reaches the player is filed as
  one of the player's notes on the next frame (`Game::file_carried_notes`), which is what
  GetHasNote and quest conditions read. [documented: the functions' pages say the items move
  (RemoveItemTarget, RemoveMeIRAlt); hypothesis: that OnAdd runs, as AddItem's does (GECK
  wiki OnAdd, "runs when the object is added to a container")]. Implemented in
  `fnv_game::functions::items::script_move_items`, which also logs every move with the
  function's name. The messages are hidden [hypothesis].
- **GetItemCount of a note.** On the player it reads 0: a note leaves the player's inventory
  for the Pip-Boy's list [documented: GECK wiki GetItemCount, "does not work with Notes";
  observed: capture `fa_batch3_actions`, 0 for a holotape just added to the player]. In any
  other container a note is one of its items and counts [observed: observer check 125 on
  the original, 2026-11-15: a filing cabinet in GSGeneralStore read 1 for the Matchbook;
  taken with the container menu, the game showed its "note added" message, and then
  `player.getitemcount` and the cabinet both read 0. With the rule the loot menu takes the
  Matchbook (jlm9), without it it never asks to (jlm8)].

## 3. Open

- Taking from an owned container through the loot menu records no theft in NVForever (the
  menu labels the key "Steal"; the crime summary counted 0). What the original does there is
  not observed.
- Observer check 125 (docs/handoff/observer-checks.md) is answered: the rule above is
  confirmed. The original's console echoes nothing for GetHasNote without NVSE, so that
  part was read from the "note added" message only.
