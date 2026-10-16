# Scripted weapon use (UseWeapon, Use Weapon packages)

Sources and open questions: docs/spec-notes/use-weapon.md. Combat mechanics are not a
parity target (owner, 2026-10-09; docs/design/combat.md): what follows is what scripts and
packages need to see happen, built on the combat AI's shooting (docs/design/combat-ai.md).

## 1. One weapon use per actor

`fnv_game::use_weapon::WeaponUse` holds what an actor is to do: the weapon (or none: what
it holds), where to stand (a reference, or none), the target, volleys (`None`: on and on),
shots per volley (min, max), the pause between volleys (`None`: none), the four flags, the
"target should be here" reference, and its progress (in place, shots fired, volleys done,
crouched, done). `Game::weapon_uses` keeps one per actor; it is not saved.

| Source | Weapon | Stand | Shots | Pause |
|---|---|---|---|---|
| `UseWeapon` | its weapon parameter (equipped when carried) | its stand reference; none when it is the actor itself | one volley of `numberOfShots` | none |
| Use Weapon package | target 1 when it is a WEAP, else none | the package's location 1 (`fnv_ai::goal`) | Number of Bursts volleys, or on and on (Repeat Fire); each volley a random size between Shots Per Volley min and max | Volley Fire: random between the min and max pause; Auto Fire: none [hypothesis] |

The function's defaults are the page's: crouch to reload and hold fire on, always hit and
no damage off.

## 2. Getting there (fnv-ai)

- **Package**: Use Weapon resolves like Travel (go to location 1, stay). On arrival the
  use is put in place instead of the package's goal being met.
- **Script**: `Ai::weapon_step` sees a new use (by its number) and walks the actor to its
  stand reference (at walking pace), or puts it in place at once when it is there, has
  no stand, or has no way there. While a script's use lasts it holds the actor: its
  package's doings, its search and its fight wait (`alert_step` is skipped). When the
  use is gone, the actor takes up its package again (`AiEvent::WeaponUseOver`).
- **In place** the actor turns to its target at the standing turn rate (a package's use
  only out of combat).

## 3. Firing (nvforever `use_weapon.rs`)

For each actor whose use is in place and still to be carried out (`Game::active_weapon_use`:
not done; a package's not while the actor is in combat):

- **The gun**: the use's weapon if it is a gun, else the gun drawn on it; none ends the use.
- **Holds fire** while restrained, while the target is more than 256 units from the
  "target should be here" reference, and, with Hold Fire When Blocked, while anyone else
  stands in the line of fire (`combat_tactics::in_line_of_fire`); also until its weapon
  is drawn (`alert.rs` draws it for a use) and it faces the target (`npc_combat::FIRE_FACING`).
- **Shots**: the weapon's own cycle (`fnv_combat::attack::Cycle`), as the combat AI's
  gunmen: automatic weapons hold the trigger, others wait their semi-auto delay; aimed
  from the eyes at the chest, drawn from the muzzle. Each shot is counted on the use
  (`WeaponUse::shot`): more, a pause, or done.
- **Always Hit**: a cone of 0 (straight at the chest; cover between can still stop it).
- **Do No Damage**: `ActorFire::harmless` → `By::harmless`: the projectile flies, strikes,
  shows its impact and plays its sound, but no damage, no hit events and no explosion.
- **Crouch To Reload**: `WeaponUse::crouched` while the cycle reloads; `Game::sneaks` reads it.
- **The end**: all shots fired, or the target dead or gone: `Game::end_weapon_use` removes a
  script's use (the AI sends the actor back to its package) or meets a package's goal (its
  end script runs; the use stays, done, until the package changes).
- A script's use comes before the combat AI (`npc_combat` skips the actor), as the shipped
  scripts call StartCombat with UseWeapon (spec note §2). ResetAI drops the use.

## 4. Not built

- Melee weapons: a use whose weapon is not a gun and an actor with no gun drawn end at once.
- Swapping the drawn model: a use naming a weapon other than the one drawn fires that
  weapon's shots from the drawn model.
- Grabbing a weapon the actor does not carry.
- Walls as "blocked"; ammunition (unlimited, as the combat AI's).
- Location 2 of the 9 packages that have one.

## 5. Checks

- Unit tests: `fnv_game::use_weapon` (volleys and pauses), `crates/fnv-ai/tests/carry.rs`
  (a package walks to its place, faces its target, meets its goal when spent; the function
  walks the actor to its stand reference and back to its package after).
- Smoke scenario `use-weapon` (tools/smoke/scenarios.toml): Violet fires three always-hit,
  harmless shots at the player from her own place; all three land, the player is not hurt,
  she goes back to her package.
