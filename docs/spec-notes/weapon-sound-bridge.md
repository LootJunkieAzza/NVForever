# Weapon Sound Playback Bridge (`SetWeaponSound` -> Engine Audio)

## 1. Documented contract

[documented] Per the GECK wiki (`https://geckwiki.com/index.php?title=GetWeaponSound`,
`https://geckwiki.com/index.php?title=SetWeaponSound`) and xEdit (`wbDefinitionsFNV.pas`
`WEAP`), JIP LN `SetWeaponSound weapon soundID [newSound]` overrides or clears (when
`newSound` is omitted or `0`) any of the 14 `WEAP` sound slots for the session:

| `soundID` | `WEAP` subrecord | Engine target |
|---:|---|---|
| `0` | `SNAM` (1st) — Shoot 3D | `WeaponFx::shoot_3d` (NPC/3D shot, player fallback when 2D absent, NPC melee swing) |
| `1` | `SNAM` (2nd) — Shoot Dist | `WeaponFx::shoot_dist` (distant 3D shot beyond 3D `max_distance`) |
| `2` | `XNAM` — Shoot 2D | `WeaponFx::shoot_2d` (player 2D shot, NPC fallback when 3D absent) |
| `3` | `NAM7` — Attack Loop | `WeaponFx::shoot_loop` (looping weapon attack sound) |
| `4` | `TNAM` — Attack Fail | `WeaponFx::no_ammo` (dry fire / gun no ammo) |
| `5` | `NAM6` — Melee Block | `WeaponFx::block` (player and NPC weapon block sound) |
| `6` | `UNAM` — Idle | `WeaponFx::idle` (player idle loop in `nvf_play::combat::weapon_sounds` and NPC idle loop in `nvforever::own_sounds::idles`) |
| `7` | `NAM9` — Equip | `WeaponFx::equip` (player draw sound in `nvf_play::combat::weapon_sounds`) |
| `8` | `NAM8` — Unequip | `WeaponFx::unequip` (player holster sound in `nvf_play::combat::weapon_sounds`) |
| `9` | `WMS1` (1st) — Mod Shoot 3D | `WeaponFx::mod_shoot_3d` (silenced/suppressed 3D shot) |
| `10` | `WMS1` (2nd) — Mod Shoot Dist | `WeaponFx::mod_shoot_dist` (silenced/suppressed distant shot) |
| `11` | `WMS2` — Mod Shoot 2D | `WeaponFx::mod_shoot_2d` (silenced/suppressed 2D shot) |
| `12` | `YNAM` — Pickup | `WorldSounds::forms.items[weap].0` (world pickup, container take, UI pickup via `item_sound_of`) |
| `13` | `ZNAM` — Putdown | `WorldSounds::forms.items[weap].1` (world drop, container put, UI putdown via `item_sound_of`) |

[documented] Lutana's `SetPickupSound` and `SetPutdownSound` on `WEAP` records share live
slots `12` and `13` with `SetWeaponSound` (`docs/spec-notes/weapon-mods.md` §9).

## 2. Engine sync design

[design] `nvf_play::world_sounds::script_weapon_sounds` mirrors `Game::weapon_sound_changes()`
into the engine's two live weapon-sound tables using a shared `WeaponSoundSync` resource:

- `Sfx::forms::weapons` (`HashMap<FormId, WeaponFx>`) receives slots `0..=11`, preserving
  every untouched sound slot as well as `shell_casing_model` and `impact_dataset`.
- `WorldSounds::forms::items` (`HashMap<FormId, (Option<FormId>, Option<FormId>)>`)
  receives slots `12..=13` for `WEAP` records.
- `WeaponSoundSync` gates both tables on `(game.weapon_sound_stamp(), weapon_clones_stamp(game))`
  so frames with no session sound or clone edits do O(1) work with zero allocations.
- `WorldSoundsPlugin` runs `script_weapon_sounds` twice in `Update` against the single shared
  `WeaponSoundSync` resource:
  1. `.before(PlaySet::Activation)` so combat systems (`weapon_sounds`, `fire`, `melee`,
     `actor_attacks`, `block_sound`) observe overrides from console commands, save loads, or
     earlier frames immediately without 1-frame lag.
  2. `.after(PlaySet::Scripts).before(PlaySet::Ui)` so UI pickup/putdown (`ui_sounds::play`),
     world pickup/drop (`world_sounds::objects`), and NPC weapon idle loops
     (`nvforever::own_sounds::idles`, ordered `.after(PlaySet::Sync).before(SfxSet)`)
     observe same-frame script edits immediately.
- Authored records are snapshotted once before first mutation and restored in full when an
  override disappears (e.g., on load or new game).
- `TempCloneForm` `WEAP` aliases are materialized from their authored origin's unmutated
  record plus the clone's own (or creation-time snapshotted) `WeaponSoundChange` overrides,
  and pruned cleanly whenever clones disappear or a clone FormID is reused in a fresh session.
  Non-`WEAP` clones are never admitted into either table.

## 3. Hypotheses & unmeasured edge cases

[hypothesis] Clearing a `WEAP`'s `YNAM` or `ZNAM` slot (`SetWeaponSound weap 12 0` or
`13 0`) sets `WorldSounds::forms.items[weap]`'s pickup/putdown entry to `None`, causing
`world_sounds::item_sound` to fall back to `UIItemGenericUp` / `UIItemGenericDown` — the
same fallback already used for authored weapons that omit `YNAM`/`ZNAM` — rather than
muting pickup/putdown completely.

[hypothesis] Clearing or replacing `shoot_loop` (slot `3`) or `idle` (slot `6`) while a
loop is already playing transitions through the existing loop lifecycles (`combat::weapon_sounds`
stops the old idle loop and starts the new one on the next frame; `combat::fire` switches
`c.looping` on the next shot or lets `LOOP_SOUND_HOLD` expire when cleared;
`nvforever::own_sounds::idles` updates or stops its `SoundSource` on the next tick). Exact
mid-burst transition timing in the original game has not been captured.
