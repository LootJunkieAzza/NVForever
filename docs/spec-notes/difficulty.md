# Game difficulty: damage and kill XP

ESM gap audit row 16 (docs/handoff/esm-gap-audit-2026-10-21.md). Built in
claude/e16-difficulty-damage: `fnv_game::difficulty`, `Game::hit`, `Game::kill_xp`,
the settings page row `difficulty` and the console's `SetGameDifficulty` /
`GetGameDifficulty` (engine `nvf_play::difficulty`).

## 1. The levels

- Five levels, numbered 0 Very Easy, 1 Easy, 2 Normal, 3 Hard, 4 Very Hard
  [documented: GECK wiki GetGameDifficulty and SetGameDifficulty (JIP), "0 for Very Easy,
  to 4 for Very Hard"].
- The game settings name them by the suffixes VE, E, N, H, VH [documented: GECK wiki
  Experience Settings, `fDiffMultXPVE` … `fDiffMultXPVH`].
- Where the original keeps the chosen level (its INI or the save) is not documented here
  [hypothesis: its preferences INI]. NVForever keeps it as a setting of its own, not of the
  save: the pause menu's Gameplay page, kept in `NVForever.ini` `[Settings]` as
  `difficulty=<name>` (`difficulty=Normal`; an older slider part, 0.50 = Normal, still
  loads as the nearest of the five) like every row of that page [design]. A new install
  starts at Normal [hypothesis: the original's default].
- NVForever adds a sixth choice, Custom (§5) [design: #3569].

## 2. What the masters carry

- `fDiffMultXPE`, `fDiffMultXPH`, `fDiffMultXPVE`, `fDiffMultXPVH`: all 1.0 in
  FalloutNV.esm; no `fDiffMultXPN`; no other master sets any of them [observed:
  `fnvtool esm inventory` and `esm decode` over the ten masters, 2026-10-21]. The wiki
  agrees: FalloutNV.esm sets them to 1.0, "so there is no XP bonus for playing at a higher
  combat difficulty" [documented: GECK wiki Experience Settings, "General"].
- **No `fDiffMultHPByPC*` or `fDiffMultHPToPC*` setting is in any master** [observed: same
  survey]; the GECK wiki export has no page or row for them either. Their values are the
  engine's own defaults, inside the encrypted executable, which is not read (charter).

## 3. Rules

1. **Kill XP.** The XP for a kill (`Game::kill_xp`, Experience Settings) is multiplied by
   `fDiffMultXP<level>` [documented: "Settings which affect iXPRewardKillNPC___ and
   iXPRewardKillOpponent___ awards"]. Defaults when the load order lacks one: the wiki's,
   VE 0.5, E 0.75, N 1.0, H 1.5, VH 2.0 [documented]. With the vanilla masters every level
   gives 1.0 (§2), so nothing changes in play. Other XP (locks, terminals, quests) is not
   scaled [documented: the page names only the kill rewards].
2. **Damage the player deals.** A hit whose attacker is the player, on any other actor,
   is multiplied by `fDiffMultHPByPC<level>` before it is applied (health and limb
   damage alike) [hypothesis: the setting's role, from its name ("HP by PC")].
3. **Damage the player takes.** A hit on the player, from anyone (an actor, an explosion,
   the player's own grenade), is multiplied by `fDiffMultHPToPC<level>` [hypothesis: the
   setting's role, from its name; that every source counts].
4. **Effect damage** (burns and poison from hit effects, `Game::effect_health_damage`)
   follows the same two rules by the effect's caster [hypothesis].
5. **Followers and other actors** are not scaled: a companion's hit on an enemy and an
   enemy's hit on a companion do their full damage [hypothesis: the settings name only the
   player (PC); nothing in docs/design/followers.md or the wiki export says otherwise].
6. **Defaults** used while no plugin sets them (always, with the shipped masters):

   | Level | ByPC (dealt) | ToPC (taken) |
   |---|---|---|
   | Very Easy | 2.0 | 0.5 |
   | Easy | 1.5 | 0.75 |
   | Normal | 1.0 | 1.0 |
   | Hard | 0.75 | 1.5 |
   | Very Hard | 0.5 | 2.0 |

   [hypothesis: the commonly reported figures for the Fallout 3 / New Vegas engine; no
   source in the wiki export. A capture can settle them: `GetNumericGameSetting
   fDiffMultHPByPCVH` and the rest in the original, docs/owner-review.md.]
7. A plugin that sets one of the ten settings is honoured (read through the load order
   like every game setting) [design].
8. The scaling happens in `Game::hit` and `Game::effect_health_damage`, so every
   path that applies a hit (shots, blows, explosions, mines, the `NVF_PLAY_TEST_HIT` aid)
   gets it once; the weapon damage formula itself (`fnv_combat::damage::hit`) and
   `fnvtool combat damage` / `sim` stay at Normal [design].

## 4. Console and test aid

- `SetGameDifficulty <0-4>` and `GetGameDifficulty` (JIP functions, which the catalogue
  lists without an opcode) work at the console by name; the level changes as from the
  settings page, at once, but the INI is written only when the page changes a row
  [design]. Log line:
  `difficulty: Very Hard (4): damage by the player x0.50, to the player x2.00, kill XP x1.00`.
- Every hit the difficulty changes (a multiplier other than 1) logs its base and scaled
  damage at debug level, target `fnv_game::difficulty` (`RUST_LOG=info,fnv_game::difficulty=debug`),
  so a run shows real hits (weapons, blows, blasts, falls) as well as the aid's:
  `difficulty: a hit of 20.0 on 00000014 by 00104C80 is 40.0 (x2.00)` (`Game::hit`) [design].
- `NVF_PLAY_TEST_HIT` takes `by=<reference>`: that actor is the attacker instead of the
  player (to hit the player: `player` as the target) [test aid].
- A script's `SetGameDifficulty` (JIP, through the script engine rather than the console's
  by-name word) sets the running game's level and leaves it in `Game::difficulty_request`;
  the engine takes it as the chosen level on its next frame, as the console's, so it is not
  put back (it was before 2026-11-22: the engine handed its own level back every frame). As
  the console's, it is not written to the INI until the page changes a row [design].
  Log line: `difficulty: a script chose Very Easy (0); kept as the chosen level`.
- Smoke `difficulty-damage`: the same 10-point hit at Very Easy and at Very Hard, by the
  player on Easy Pete and by him on the player; `difficulty-script-kept`: a script's
  level holds for later hits; `difficulty-custom-*`: §5.

## 5. Custom [design: #3569]

- The list's sixth choice. Three sliders under the list apply while it is chosen and are
  greyed otherwise (`nvf_settings_layout::NEEDS_CHOICE`, the launcher's page too):
  - **Player Damage** (`difficulty-player-damage`): in place of `fDiffMultHPByPC` (§3.2).
  - **NPC Damage** (`difficulty-npc-damage`): in place of `fDiffMultHPToPC` (§3.3), so
    every source of damage to the player, as at the five levels; hits between NPCs and on
    companions stay unscaled (§3.5).
  - **Kill XP** (`difficulty-kill-xp`): in place of `fDiffMultXP` (§3.1).
- Each is 50% to 300% in steps of 5%, 100% until the player moves it, and kept
  in the INI (as the slider's part: 0.00 = 50%, 0.20 = 100%, 1.00 = 300%). The values stay
  while another level is chosen and apply again when Custom is.
- Load-order game settings do not apply to Custom: the sliders are the multipliers.
- Scripts' `GetGameDifficulty` answers 2 (Normal) under Custom, as scripts know only the
  five levels; `SetGameDifficulty 0-4` leaves Custom for that level.
- The list's note names every level's three multipliers as "dealt/taken/kill XP" percentages
  (`Hard 75/150/100%`): the launcher's are the vanilla masters' (§2, §3.6), the game's the load
  order's own, read as the hits read them [design: #3569:
  "the exact info"]. The sliders' notes give their range, not Normal's.
- Log line: `difficulty: Custom: damage by the player x3.00, to the player x0.50, kill XP
  x2.00` (logged again whenever a slider moves while Custom is chosen).
