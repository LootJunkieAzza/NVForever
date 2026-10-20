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

- **The gun**: the use's weapon if it is a gun, else the gun drawn on it; a melee or
  unarmed weapon, or none, swings instead (§3.1).
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

### 3.1 Melee (claude/q5-melee-useweapon, 2026-10-20)

The page counts "how many times to use the weapon" [documented: GECK wiki UseWeapon
(Function)]; nothing documents how a melee weapon is used. Ours, built on the combat AI's
melee fighters (docs/design/combat-ai.md §14.4, §15; `npc_combat`):

- **What it uses**: the weapon named when the game knows it as a weapon, else the one
  drawn on the actor; a gun (thrown weapons included: they fire a projectile) fires as
  above; any other weapon swings; with none, a creature uses its own attack and a person
  its fists [design]. A use that names a melee weapon now swings it even when the actor
  holds a gun (before, it fired the gun).
- **Each shot is a blow**: the use's count (volleys and their sizes for a package) counts
  blows begun (`WeaponUse::shot`); the use ends once the last blow has landed or missed.
- **Reach**: the engine works out the actor's spacing as `npc_combat` does (the weapon's
  reach and the two bodies' radii, `melee_spacing`; a creature's `creature_spacing`) and
  puts it on the use (`WeaponUse::reach`: stop, strike). In place, a **script's** use closes
  in on its target as a fighter does (`fnv_ai` `weapon_step` → `close_in`: runs to within
  the stop distance, stands and faces it, sets off again beyond the strike distance), so
  where to stand is where it sets out from [hypothesis: a blow from a stand out of reach
  would never land; the shipped calls name the actor itself as the stand in most cases].
  A **package's** melee use swings from where its package stands it, when the target is
  within reach [design: the package's own walking is not interrupted].
- **Swings**: facing the target (`FIRE_FACING`), its weapon drawn, within reach and within
  128 units of height, every `STRIKE_SECONDS` over the weapon's attack speed (Attack Mult),
  sooner at the last clip's `a:` key, which picks the next swing's group (a combo, as
  #353); no block or attack-chance roll, the script asks for the blows [hypothesis]. A blow
  lands at its clip's `Hit` key (`attack_anims` times) as an `ActorStrike` (damage, the
  target's block, impacts and sounds as any NPC blow).
- **Always Hit**: a blow lands even when the target stepped out of reach by its `Hit` key
  [hypothesis]. **Do No Damage**: the swing plays and the blow is counted, but nothing is
  struck: no damage, no sound, no impact [design: `ActorStrike` has no harmless form yet;
  left out to keep clear of the open combat PRs]. **Hold Fire When Blocked** and **target
  should be here** hold the swings as they hold shots; **Crouch To Reload** has nothing to
  do.
- No power attacks (#503, people's power attacks, is open; a use's blow is never one).

## 4. Not built

- Melee: harmless blows' sound and impact; power attacks; a package's melee use walking
  up to its target.
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
- Unit test `a_melee_use_closes_in_on_its_target_within_its_reach` (`crates/fnv-ai/tests/carry.rs`):
  a script's use with a reach runs to its target, stops within it, and drops the closing in
  when the use is over.
- Smoke scenario `use-weapon-melee`: Violet uses a knife three times on the player 400 units
  away, no damage: she runs up, swings three blows and goes back to her package; the
  player is not hurt.
