# Spec notes: combat rules (step C2) — damage, spread, attack cycle, projectiles

The rules of `crates/fnv-combat` for docs/design/combat.md step C2. Combat is not a
parity target (owner, 2026-10-09). Each rule is either **[documented]** (a GECK wiki
formula from the owner's exports, summarised in combat.md §4.2) or **[design]**
(NVForever's own, tunable). Record fields are as docs/spec-notes/combat-data.md describes
them.

## 1. Inputs (`tuning.rs`)

- `Rules`: the game settings the documented formulas name, read from the load order
  (`Rules::from_forms`). A setting the masters lack takes the wiki page's default
  [documented default; that an absent setting means its default is a hypothesis,
  combat-data.md §3]. `Rules::defaulted` lists which came from defaults.
  `fCombatDistance` has no documented default; 100 units is a placeholder
  [hypothesis].
- `CombatTuning`: every [design] number below, in one place:

| Value | Default | Shapes |
|---|---|---|
| `dt_floor` | 0.2 | DT never removes more than 80 % of a hit |
| `condition_min_mult` | 0.5 | damage at zero condition (linear to 1 at full) |
| `headshot_unaware_crit` | true | a headshot on an unaware target is a critical |
| `wear_per_attack` | 1.0 | condition points lost per attack |
| `bloom_per_shot`, `bloom_recovery`, `bloom_max` | 0.03 × Min Spread, 1.0 /s, 0.25 | spread added per shot (spread units) |
| `jump_spread` | 0.3 | actor-spread penalty while airborne |
| `recoil_base_deg`, `recoil_per_damage_deg`, `recoil_recovery_deg` | 0.4°, 0.03°/damage, 12°/s | view kick |
| `melee_window`, `melee_rays`, `melee_arc_deg` | 0.1 s, 5, 35° | melee swing |
| `explosion_edge` | 0.25 | blast damage at the edge of the radius |
| `default_bounce` | 0.3 | lobbers whose PROJ has no bounce value |
| `auto_reload`, `fallback_reload` | true, 2 s | empty-trigger reload; reload without a clip length |

The bloom values were lowered after the first `combat sim` runs: at 0.15 × Min Spread
with a cap of 1.0, the 10mm SMG hit 2 of 60 shots at 1000 units from the hip [observed
with `fnvtool combat sim`]. With the current values it hits about 19 %, and every shot
aimed at 600 units.

## 2. Damage (`damage.rs`)

For one projectile (or one swing) that hits [documented unless marked]:

1. **Base.**
   - Guns: weapon damage × `fDamageWeaponMult` × condition × skill bonus ÷ projectiles
     per shot. The skill bonus is `fDamageSkillBase` + `fDamageSkillMult` × skill / 100.
   - Melee: weapon damage × multiplier × condition × skill bonus + MeleeDamage
     (Strength × `fAVDMeleeDamageStrengthMult` + offset) + the crippled-arm term.
   - Unarmed: weapon damage × multiplier × condition + UnarmedDamage
     (`fAVDUnarmedDamageBase` + `fAVDUnarmedDamageMult` × Unarmed) + the crippled-arm
     term.
   - The crippled-arm term is weapon damage × (`fDamageArmConditionBase` − 1 +
     `fDamageArmConditionMult` × arm condition) / 2.
   - The condition curve is ours [design]; the original's is hard-coded.
2. **Miss.** A base of 0 is a miss: no critical, no damage.
3. **Modifiers.** Ammo Damage Mod effects (add, multiply, subtract, in list order), then
   the "Calculate Weapon Damage" perk hook.
4. **Sneak attacks.** × `fCombatDamageBonusSneakingMult` for guns,
   × `fCombatDamageBonusMeleeSneakingMult` for melee and unarmed.
5. **Body part.** × the part's damage multiplier [design: placed after the documented
   steps].
6. **Critical chance.**
   - CritChance = `fAVDCritLuckBase` + `fAVDCritLuckMult` × Luck + bonuses.
   - Times the weapon's crit % multiplier, divided by the fire rate for automatic
     weapons.
   - Through the "Calculate My Critical Hit Chance" hook, + `fVATSCriticalChanceBonus`
     in VATS, then the target's "Modify Enemy Critical Hit Chance" hook.
   - The hit is a critical if a roll in [0, 100) falls under the chance, or if it is a
     headshot on an unaware target [design]. A weapon with crit % multiplier 0 never
     crits.
   - Critical damage (through "Calculate My Critical Hit Damage") is added before
     armour. For shotguns it is shared among the projectiles [design].
7. **Armour.**
   - DT comes from the target, then ammo DT Mod effects, then the attacker's and
     defender's DT perk hooks. A weapon with "Ignores Normal Weapon Resistance" skips
     DT and DR [design: the page says the flag did not work in the original].
   - After DT, at least `dt_floor` of the hit remains [design].
   - DR (after ammo DR Mod effects, capped at `fMaxArmorRating`) then takes its
     percentage.
8. **Child flag.** A target whose race has the Child flag takes nothing.

Also in `damage.rs`:
- Weapon wear per attack [design]: `wear_per_attack` × the damage-to-weapon override ×
  the ammo's condition effects.
- Knockdown chance for explosions [documented: Explosion Knockdown Formula, with both
  health thresholds].
- Melee knockback force [documented: Melee Knockback Formula].
- Blast damage falloff [design].

Perks: the `PerkHooks` trait has one method per combat entry point (weapon damage,
critical chance and damage, enemy critical chance, DT attacker/defender, gun spread),
each the identity by default. Reading PERK records is a later step.

## 3. Spread, bloom, recoil (`spread.rs`)

- **Actor spread** [documented: Gun Spread Formula (New Vegas)]:
  - Sum the penalties: not aiming, walking or running (÷ the "Modify Aiming Move
    Speed" divisor), standing rather than crouching, crippled arms by one- or
    two-handed weapon, and unmet Strength and skill requirements. While airborne,
    add `jump_spread` [design].
  - Multiply by 1 − `fWobbleToSkillConversion` × skill / 100, and keep at least
    `fMinGunSpreadValue`.
  - The page reports the original applies the Strength setting to the skill shortfall,
    a bug. We use the skill setting.
- **Degrees** [documented as the page authors' in-game measurement]:
  (actor + 0.0125·W² + 0.125·W) × 5.955, where W is Min Spread. Ammo Spread Mod effects
  multiply the result.
- **Cone** [design]:
  - The shot leaves inside a cone of that half-angle around the view direction, uniform
    over the cone's disc, so the crosshair is always the centre.
  - Bloom adds to the actor spread per shot and recovers; recoil kicks the view up per
    shot (more for harder-hitting weapons) and recovers.

## 4. Attack cycle (`attack.rs`)

- **Interval.**
  - Automatic weapons: 1 ÷ `fire_rate`.
  - Semi-automatic weapons: the attack clip's length ÷ (attack multiplier × animation
    multiplier) when the engine passes it, else 1 ÷ "Attack Shots/Sec" [documented:
    GECK wiki Weapons].
- **When the shot leaves.** At the attack clip's `Hit` key [documented:
  NiTextKeyExtraData]. Automatic weapons fire at once.
- **Reload.** Takes the reload clip's length; the animation multiplier does not speed
  it up [documented]. Without a clip length, `fallback_reload`.
- **Trigger.** A semi-automatic weapon needs a fresh pull per shot; an automatic one
  fires while the trigger is held.
- **Empty magazine.** Pulling the trigger on an empty magazine reloads when ammo is
  carried (`auto_reload` [design]), else gives one dry click per pull.
- **Ammo.** A shot takes `ammo_use` rounds.
- **Regenerating weapons** gain a round every `regen_rate` seconds and cannot be
  reloaded by hand [documented].
- **No-ammo weapons.** Melee and unarmed weapons cycle without ammo.

## 5. Projectiles, swings, blasts (`projectile.rs`, `shot.rs`)

- **The world.** `CombatWorld` is the engine's side: `ray(from, to, ignore)` and
  `actors_near(point, radius)`.
- **A shot.** `shot::fire` spawns the chain's projectiles per shot, each with its own
  direction in the cone.
- **Flight.**
  - Beams and hitscan projectiles trace at once. The hit is delivered after distance ÷
    speed; beams deliver at once.
  - Missiles and flames are stepped with swept rays under gravity, up to their range.
    A flame goes on through actors, hitting each once.
  - Projectiles flagged to pass small-transparent colliders go through them.
  - Gravity is the PROJ value × the world's 9.81 m/s² at 70 units per metre
    [hypothesis; the owner plans projectile captures to tune this].
- **Explosions.**
  - Without Alt. Trigger, the explosion goes off on impact.
  - With it, a lobber bounces (× its bouncy multiplier, else `default_bounce`) and
    settles. It explodes when its timer runs out, or, when it has a proximity radius,
    once armed (after the timer) when an actor other than the thrower comes within
    the radius.
  - Detonating projectiles wait for `detonate()` (C-4).
  - Anything else ends after 60 s [design].
- **Melee swing.** A fan of `melee_rays` rays over ±`melee_arc_deg` at the eye, as
  long as the reach. Each actor counts once [design]. The reach is `fCombatDistance` ×
  scale × weapon reach [documented formula; placeholder setting].
- **Blast.** Every actor within the radius takes damage, unless a ray from the centre
  is blocked first and the EXPL does not ignore line of sight [documented]. The damage
  falls off [design]; the push is away from the centre with the EXPL force.

## 6. CLI contract

Both commands take `<Data> <plugins…> --schema <dir> --form <weapon>` and the shooter
and target options:
- `--ammo`, `--skill` (50), `--strength` (5), `--luck` (5), `--unarmed` (50),
  `--condition` (1), `--sneaking`;
- `--dt`, `--dr`, `--part-mult` (1), `--head`, `--unaware`.

- `fnvtool combat damage` → `fnv.combat.damage/1`:
  - `hit` (the `Damage` breakdown without a critical) and `critical_hit`;
  - `per_shot` (all projectiles hitting);
  - `explosion` (damage, radius, force);
  - `settings_defaulted`.
- `fnvtool combat sim` → `fnv.combat.sim/1`. It also takes `--distance` (1000),
  `--radius` (30), `--health` (100), `--seconds` (10), `--aim`, `--crouch`,
  `--carried` (1000) and `--seed` (1).
  - The setup: the shooter's eye is at 100 units, aiming at a sphere at the same
    height, on a floor. Semi-automatic weapons are pulled every other tick and
    automatic ones held; explosions use the blast rule. Runs with the same seed
    repeat exactly.
  - Output: `interval`, `actor_spread`, `cone_deg_first_shot`, `shots`, `projectiles`,
    `hits`, `hit_rate`, `criticals`, `reloads`, `damage`, `time_to_kill`.

Examples [observed, seed 1, skill 50, 100 HP target]:

| Weapon | Setup | Hits | Time to kill |
|---|---|---|---|
| 9mm pistol | hip fire at 1000 units | 8 of 9 | 2.7 s |
| 9mm pistol | aimed and crouched | 9 of 9 | 2.7 s |
| sawed-off shotgun | at 400 units | 17 of 28 pellets | 2.5 s (with a reload) |
| 10mm SMG | hip fire at 1000 | 19 % | 6.6 s |
| 10mm SMG | aimed at 600 | 8 of 8 | 0.85 s |
| laser rifle | aimed, DT 12 | 12 of 12 | 3.7 s |
| hunting rifle | skill 75, aimed at 3000 units, DT 6 | 3 of 3 | 2.2 s |

## 7. Hit volumes (step C3)

- **The capsule** (`fnv_phys::actors`, `HitVolume`): one vertical capsule per placed
  actor in the physics world, kinematic, owner `Owner::Actor { form_id, layer, part }`
  (part 255 since C8; per-part capsules are §12).
  - Its layer is BIPED while the actor lives and DEAD_BIP once dead. The layers are the
    documented Havok names; using them for actors is our choice.
  - It takes part in no contacts: it pushes no clutter, and the character controller
    walks through it [design].
  - Setting an unchanged volume does nothing; the shape is rebuilt only when its size
    changes.
- **In play** (`engine/nvf-play/src/hitvol.rs`): every drawn, present ACHR/ACRE gets a
  volume each frame from its render bounds [design]:
  - the union of its drawn parts' world boxes;
  - feet at the bottom centre, height the box height;
  - radius the smaller horizontal half-extent, clamped to 10–150 units.

  An actor that is hidden, removed or no longer drawn loses its volume.
- **Rays.**
  - The crosshair's collision ray targets an actor when it hits its volume.
  - The floor probe, the safe-spot search and the third-person camera skip actor
    volumes (`layers::actor_volumes()`).
- **Checked.**
  - `fnv-phys` tests: shots hit the capsule, follow it when moved and see its layer
    change on death; a filter can skip it; a character walks through it; moving it
    through a dynamic box does not push the box.
  - In the engine: `nvforever --data <Data> --cell Goodsprings --play --run-seconds
    25` exits 0 with "run finished" and reports 13 actor volumes at exit (10 NPCs and
    15 creatures drawn, 12 of them hidden by the simulation). A level probe ray from
    150 units at each capsule's middle hit its own actor for 12 of the 13, and
    something else first for one [observed 2026-10-09].

## 8. The player shooting (step C4)

- **Weapon and ammo** (`engine/nvf-play/src/combat.rs`):
  - The equipped weapon comes from the simulation, with the ammo from
    `CombatForms::ammo_for` (the first carried entry of its list).
  - The magazine is a view of the carried rounds. A shot removes `ammo_use` rounds
    from the inventory; a reload moves nothing.
  - A weapon with no ammo but a projectile (grenades) spends itself.
  - A newly equipped weapon starts loaded with what is carried [design].
- **Controls.** Use is the trigger; the press that draws the weapon does not fire. A
  Ready Item tap reloads, and Ammo Swap takes the next carried ammo, then reloads
  [design].
- **Readiness and clips.** The cycle runs once the player body reports the weapon drawn
  and in hand (`WeaponAnim::ready`; without a body, as soon as it is wanted out).
  - The body gives the attack clip's length and `Hit` key and the reload clip's
    length, from the rig in view. The attack clip is `{prefix}{attack}{is}.kf` from the
    WEAP attack animation; the reload clip is `{prefix}reload{x}.kf`
    (`fnv_actor::player_clips`).
  - The body plays the clip once each time combat counts an attack or a reload.
- **Shots.**
  - They leave the eye along the view, inside the cone from §3. The stance is taken
    from the controls and the character: aiming, sneaking, moving, airborne.
  - The skill is derived from its attribute and Luck (`damage::derived_skill`;
    which attribute governs which skill is a hypothesis), since the simulation has no
    skills yet.
  - Recoil turns the view's pitch.
- **The world.** Projectiles fly against the physics world (`PhysView`):
  - rays skip the player's capsule and the layers the character passes;
  - actor hit volumes (§7) report the actor;
  - small-transparent colliders are the layers TRANSPARENT_SMALL and
    TRANSPARENT_SMALL_ANIM.
- **Hits.** Hits and blasts are logged with the damage the rules give (target DT 0, no
  body part); applying it is step C5.
- **Feedback** [design, until impact effects and audio]:
  - an impact marker for 0.3 s (0.6 s for explosions);
  - tracers at the PROJ tracer chance;
  - an ammo counter (magazine / carried, the ammo's abbreviation).
- **Test aids.** `NVF_PLAY_TEST_CONTROL` presses `fire`, `reload`, `swap` and
  `aimactor` (look at the nearest actor's hit volume).
- **Checked** [observed 2026-10-09, `nvforever --cell Goodsprings --play`, exit 0, "run
  finished"]:
  - **9mm pistol** (50 rounds given): magazine 13/13, one shot per trigger pull. Two
    shots hit a creature 1244 units away for 9.2 each (16 × the skill bonus at Guns
    15). A reload took one round. The screenshot shows the first-person pistol and the
    counter `12 / 37  9mm`.
  - **Missile launcher:** the missile struck the ground about 158 units from the
    creature it was aimed at. The explosion caught it for 176 and another creature at
    501 units for 125 (before armour).

## 9. Damage applied (step C5)

- **Health** (`crates/fnv-game/src/combat.rs`). Health's base (actor value 16) is the
  actor's maximum health whenever no script has set it, so `GetAV Health` and
  `GetHealthPercentage` see combat.
  - NPC: Base Health + Endurance × `fAVDNPCHealthEnduranceMult` (5) + level ×
    `fAVDNPCHealthLevelMult` (5).
  - Creature: its DATA Health.
  - Player: `fAVDHealthEnduranceOffset` + Endurance × 20 + (level − 1) × 5 + Base
    Health.
  - The forms are hypotheses until captured; the settings' names and values are
    observed (combat-data.md §3).
  - Level comes from ACBS, or for "PC Level Mult" the player's level × the multiplier
    within the calculated bounds [documented: Stats Tab pages].
  - A template with the "Stats" flag supplies the stats.
- **A hit** (`Game::hit`):
  - Health's damage part grows (as `DamageAV`).
  - The last hit (location, critical, attacker, weapon) is kept for `GetHitLocation`,
    which is now implemented.
  - OnHit runs with the attacker, then OnHitWith with the weapon.
  - Child races and Invulnerable creatures take nothing.
  - Essential actors (base flag or `SetActorRefEssential`) fall unconscious at 1 health
    instead of dying [design].
  - At 0 health the actor dies as with `KillActor`: dead flag, killer, GetDeadCount,
    OnDeath. `KillActor` now uses the same `Game::kill`.
- **Kill XP** [documented: Experience Settings]. The target's level picks the class
  (the first `iXPLevelKill…` threshold at or above it), which gives the reward. There is
  none when the player did less than `iXPDeathRewardHealthThreshold` % of the damage.
- **Armour.** The target's armour is what it would wear of what it carries
  (`CombatForms::worn_armor`, the owner's wearing rule from actors.md §2.3). DT and DR
  are the sums over the pieces [hypothesis]. The simulation creates an actor's inventory
  on first touch, so the engine reads it through `World::inventory_mut`.
- **In play** (`engine/nvf-play/src/combat.rs`). Every projectile hit and blast goes
  through `Game::hit` inside the script runtime's world swap.
  - Blasts use location −1 and the same armour rule as bullets.
  - An `ActorHit` message goes out with each hit.
  - Without the script runtime, hits are only logged.
- **On screen** (`engine/nvforever/src/deaths.rs`, `locomotion.rs`):
  - A living actor flinches with `idleanims\mt_hittorso.kf` over its other clips.
  - A dead one plays `idleanims\deathpose1.kf` or `deathpose2.kf` (chosen by reference)
    and holds its last frame.
  - An actor already dead when drawn goes straight to that end pose.
  - A skeleton without death poses (creatures) freezes and is laid on its back
    [design, until ragdolls].
  - Dead actors' hit volumes move to DEAD_BIP.
- **Looting.** The crosshair offers **Search** on a dead actor, and activation by hand
  opens its inventory in the container menu (the dead-actor rule of
  `Game::default_activation`, now also on the by-hand path).
- **HUD** [design]:
  - a hit marker (white, red on a kill);
  - the last target's name and health bar for 4 s;
  - the player's health (`HP now/max`);
  - a line per kill with its XP.
- **Test aid:** `aimnpc` (aim at the nearest NPC's hit volume).
- **Checked** [observed 2026-10-09, `nvforever --cell Goodsprings --play`, exit 0, "run
  finished"; screenshots kept privately]:
  - **Raven** (creature, 6 health): killed by one 9mm hit, its OnHit script ran, +1 XP
    (very easy creature). Later hits on the corpse did nothing.
  - **Goodsprings Settler** (55 health, Field Hand Outfit, DT 2): 7 hits (7.2 each, a
    23.2 critical) killed it, +10 XP.
  - **The body:** it lay in the death pose, the crosshair showed "Search", and
    activating it opened its inventory (5.56mm rounds, Field Hand Outfit, Varmint
    Rifle).
  - **HUD:** `HP 200/200` (Endurance 5 × 20 + Base Health 100).
- **Lab.** `fnvlab run --layer 5` gives 28/32, the same four known failures as main
  (`L5.script.decode` and three `delay_*`; layer5.md), so the new Health base changed
  no scenario.

## 10. Melee and unarmed (step C6)

- **Swings** (`engine/nvf-play/src/combat.rs`, `melee`). Melee and unarmed weapons
  don't use the gun cycle.
  - A tap of Use swings on release. Holding Use for `power_hold` (0.4 s) starts a power
    attack: base × `power_damage` (1.5), `power_time` (1.3) × slower. A press made
    during a swing queues the next one [design].
  - The swing takes the attack clip's length ÷ (attack × animation multiplier), or
    1 ÷ "Attack Shots/Sec" without a clip. It strikes once, at the clip's `Hit` key
    [documented: NiTextKeyExtraData], or at 40 % of the swing without one [design].
  - The strike is a fan of `melee_rays` rays over ±`melee_arc_deg` from the eye along
    the view, as long as the reach; each actor is hit once (`projectile::swing`).
- **Reach** (`damage::melee_reach`) = `fCombatDistance` × scale × the WEAP Reach
  [documented formula]. The masters lack `fCombatDistance`, and its placeholder is now
  150 (a knife, reach 0.5, gets 75 units; a sledgehammer, 0.75, 112) [hypothesis,
  tunable]. A Reach of 0 (the Fists) counts as 1 [hypothesis].
- **Damage.** The melee and unarmed forms of §2, with the target's worn armour. The
  attacker's Melee Weapons and Unarmed skills are derived from Strength and Endurance
  (§8).
- **Unarmed.** With no weapon equipped the player fights with the `Fists` WEAP
  [hypothesis: the game's unarmed weapon form]. The Fists do 0 base damage, so a punch is
  the UnarmedDamage actor value (0.5 + 0.05 × Unarmed) [documented].
- **Knockback.** The force is computed with the documented Melee Knockback Formula and
  logged. Actors are not pushed until they move physically (AI and ragdoll work).
- **Blocking.** `damage::blocked` takes `block` (60 %) off a melee hit taken while
  blocking [design]. Nothing attacks the player yet; NPC melee (Layer 6 AI) will use it.
- **The body.** A power attack plays `{prefix}attackpower.kf` (`1hmattackpower.kf`,
  `h2hattackpower.kf`). Normal swings use the weapon's attack animation, falling back to
  its `_a` variant (`1hmattackright_a.kf`).
- **Checked** [observed 2026-10-09, `nvforever --cell Goodsprings --play`, exit 0, "run
  finished"]:
  - **Sledgehammer** against the Goodsprings Settler (55 health, DT 2), standing on its
    open side: three normal hits for 14.3 (24 × the skill bonus at Melee Weapons 15 +
    MeleeDamage 2.5 − DT 2), then a power attack killed it (+10 XP). Knockback forces
    were 100 and 143. The screenshot shows the first-person power swing, the target bar
    at 0/55, the kill line and "Search".
  - **From the fenced side** the swings missed: a ray along the view hit the fence at
    71 units, and melee correctly doesn't reach through it.
  - **Fists:** three taps, one pressed during a swing, gave three hits of 0.2 (Unarmed
    damage 1.25 against DT 2, the 20 % floor).

## 11. Explosives and destructible objects (step C7)

- **Thrown weapons and mines.** They are their own ammo (`self_ammo`): a shot spends one
  and throws its PROJ along the view. Each projectile type behaves as §5 describes:
  - grenades are lobbers on a fuse (Alt. Trigger timer);
  - mines land, arm after their timer and go off when an actor other than the thrower
    comes within their proximity radius;
  - C-4 (Explosion + Detonates, no Alt. Trigger) lands and waits. It no longer explodes
    on impact [documented: GECK wiki Projectile, Detonates].
- **OnFire and detonation.** Every shot runs the weapon's OnFire blocks
  (`Game::weapon_fired`, block 0x24).
  - The detonator's script, `DetonatorOnFire`, calls `player.DetonatePlacedExplosives`
    and has main-quest hooks: the Hoover Dam generators (`vHDGenerator0N.do 50000`) and
    VMQ03b [observed: its SCTX].
  - `DetonatePlacedExplosives` is implemented: it asks the engine
    (`Game::combat_requests`), which sets off that actor's detonating projectiles.
  - Ranged weapons without a projectile (the detonator, binoculars) cycle without ammo.
- **Explosions** (`fly`). A queue of up to 64 per frame takes impacts, detonations and
  destruction stages [design]. Each explosion:
  - damages actors in its radius, with line of sight and armour;
  - rolls knockdown: always with "Knock Down - Always", by the documented formula with
    "By Formula". A knockdown is counted and logged; it is not animated yet;
  - damages destructible objects whose collision overlaps its radius;
  - pushes dynamic bodies with its force (`PhysWorld::push_dynamic`: a speed change of
    the EXPL force in units/s at the centre, falling to 0 at the radius [design]).
  - Radiation, the enchantment and the placed impact object are logged and not built.
- **Destructible objects** (`Game::damage_object`, now also behind `DamageObject` /
  `do`, so scripts and combat share it) [documented: GECK wiki Destructible Object
  Stage]:
  - damage adds up to the object's health; each stage applies once its health
    percentage is reached;
  - Cap Damage stops one hit at its stage;
  - Destroy marks the object destroyed, Disable disables it;
  - a stage's self-damage per second burns it down (`Game::destruction_tick`);
  - OnDestructionStageChange runs;
  - the stages' explosions go off 40 units above the object [design] and can damage
    other objects.

  Bullets damage the destructible they hit with the hit's damage (no armour). The
  stages' damaged models and `BSDamageStage` nodes are not drawn yet; they are logged.
- **Test aids:** `equip=<FormID>` switches weapons during a run.
- **Checked** [observed 2026-10-09, `nvforever --play`, exit 0, "run finished"]:
  - **Truck02** (0x000E7F40, 450 health, 6 stages): one missile's blast took it to stage
    4 (destroyed). Its stage explosions went off (SmallNukeExplosion 0x1CA88 and
    GrenadeFragExplosion), and it then burned down to the last stage on its own. The
    first run also showed stages without an explosion (FormID 0) counted as explosions;
    they now read as none.
  - **Frag grenade** near the Goodsprings Settler: it exploded on its fuse.
  - **C-4:** thrown, it landed and waited. Switching to the detonator and firing ran
    `DetonatorOnFire`, and `DetonatePlacedExplosives` set off the one placed charge
    (C4Explosion). Both explosions went off away from actors: the grenade bounced far
    before its fuse ran out. Throw arcs and bounce are part of the planned projectile
    captures.

## 12. Limb hits (step C8)

- **Which body part data** (`CombatForms::body_parts_for`):
  - a creature's own BPTD (CREA `PNAM`) [documented: xEdit];
  - otherwise the BPTD whose model is the actor's skeleton, which is
    `DefaultBodyPartData` (0x1D) for `characters\_male\skeleton.nif`. The player's
    `PlayerBodyPartData` (0x1C) is skipped [hypothesis, combat.md §5.9].
  - `DefaultBodyPartData` has six parts. The second number is the damage multiplier,
    the third the share of total health [observed: `fnvtool combat form`]:

    | Part | Bone | Damage | Health | Condition |
    |---|---|---|---|---|
    | Head | Bip01 Neck1 | ×2 | 20 % | Perception |
    | Torso | Bip01 | ×1 | 60 % | Endurance |
    | Left Arm | Bip01 L UpperArm | ×1 | 25 % | Left Attack |
    | Left Leg | Bip01 L Thigh | ×1 | 25 % | Left Mobility |
    | Right Leg | Bip01 R Thigh | ×1 | 25 % | Right Mobility |
    | Right Arm | Bip01 R UpperArm | ×1 | 25 % | Right Attack |
- **Part capsules** (`fnv_phys::actors`, `PhysWorld::set_actor_parts`): the actor's
  kinematic body carries one capsule per bone segment. Each collider's owner is
  `Owner::Actor { form_id, layer, part }`, where `part` is the index in the BPTD and 255
  (`actors::WHOLE`) means the single C3 capsule. Shapes are rebuilt only when a size
  changes, as in C3.
- **Bones** (`engine/nvforever/src/limbs.rs`) [design]:
  - A part runs from its bone to an end joint:
    - when its own bones lead on to other parts, the end is the farthest such
      connecting bone (the torso runs up to the neck);
    - otherwise the end is the farthest `Bip01` bone below the part (an arm reaches the
      fingers, the head the top of the head).
  - Chains skip the skeleton's root bone and `Bip01 NonAccum`. `Bip01`, the torso's
    bone, stands at the feet [observed: the posed segments, 2026-10-09]; a torso started
    there enclosed both legs.
  - Joints closer than 4 units to the last one kept are merged.
  - The renderer publishes the posed segments every frame, after transforms propagate
    (`nvf_play::hitvol::ActorParts`).
- **Radii** (`hitvol::limb_radius`) [design]: a share of the actor's size (the largest
  side of its render bounds):

  | Part | Share | At 128 units |
  |---|---|---|
  | Head | 0.065 | 8.3 |
  | Torso | 0.10 | 12.8 |
  | Arm | 0.042 | 5.4 |
  | Leg | 0.058 | 7.4 |

  Each part's chain is pulled in by its radius at both ends, so the caps end at the end
  joints. Without this, the torso's top cap covered the lower head and took head shots
  [observed]. The torso's lower end is not pulled in: pulled, it left a band at the hips
  that no capsule covered [observed].
- **Damage** (`nvf-play` combat; projectiles and melee alike):
  - The hit's part gives the damage multiplier (`BodyPart::hit_part`; head hits count
    for the unaware-headshot critical).
  - The part gives the `GetHitLocation` number (`BodyPart::location`, the BodyLocation
    enum).
  - The limb damage is the damage dealt × the WEAP Limb Dmg Mult. A multiplier of 0
    deals none [hypothesis].
  - Hits on the single capsule stay location 0 (torso); explosions are −1, with no
    limb [design].
- **Limb condition** (`Game::hit`, `Game::damage_limb`):
  - The part's condition actor value (25–31, starting at 100) loses
    `limb damage ÷ (max health × health % ÷ 100) × 100`, and stops at 0.
  - The limb is crippled when it reaches 0 [documented: GECK wiki Body Part Data,
    crippled after that share of total health; hypothesis: the count-down form].
  - `HitResult` reports the condition and whether this hit crippled the limb. The log
    and the HUD name it ("Goodsprings Settler: left leg crippled").
- **The player's crippled limbs** (`PlayerLimbs`, read every frame):
  - crippled arms add the documented crippled-arm spread terms (§3) and lower
    `Attacker::arm_condition` (1, 0.5 with one arm, 0 with both) [design: the share];
  - crippled legs slow walking and running to 0.7 with one leg, 0.5 with both
    [design];
  - the HUD's health line lists crippled limbs.

  Nothing hurts the player yet (no NPC attacks); scripts can (`DamageActorValue`).
- **Hit reactions:** a living actor plays the hit clip for where it was hit:
  `mt_hithead`, `mt_hittorso`, `mt_hitarmleft`/`right` or `mt_hitlegleft`/`right`
  (`idleanims`, the `_male` folder) [observed: the clips exist; design: which location
  plays which].
- **Test aids:**
  - `aimpart=<n>` aims at the nearest NPC's part `n`;
  - `aimnpc=<share>` aims at a share of its height;
  - `aimactor` / `aimnpc` now aim at the torso when the actor has parts. The exit
    probe does the same.
- **Not built:**
  - creature parts are not checked in play (no creature in range of the test);
  - a creature's torso may leave gaps (a tail);
  - crippled NPCs do not limp or drop weapons (no NPC movement or AI);
  - the head's screen effect is not shown;
  - perks (Adjust Limb Damage) are not read;
  - severing and exploding limbs (gore) are later.
- **Checked** [observed 2026-10-09, `nvforever --play`, exit 0, "run finished"]:
  - All 13 actor volumes had body parts. The exit probe at each aim point hit its own
    actor for 11–12 of 13 (12 of 13 with the single capsule in C3).
  - The Goodsprings Settler (55 health, DT 2), shot with the 9mm pistol through
    `aimpart`:
    - Head shots: 16.4 damage (×2), and one hit crippled the head (pool 11).
    - Leg shots: 7.2 damage; a second hit crippled the left leg (pool 13.75), and the
      leg hit reaction played.
    - Arms and torso were hit as aimed. The torso was crippled after five hits (pool
      33).
    - Some shots missed while the settler walked and flinched (a capsule about 15 units
      across, at 150 units with pistol spread).

## 13. Open points

- Every [design] number is a first guess for the owner to tune in play.
- Projectile gravity units and speeds: the owner's projectile capture round.
- `fCombatDistance`, the melee reach base: not in the masters and not documented.
- Not here yet: weapon mods (Decrease Spread and the like), fatigue, reading perks,
  gore. All are later steps (combat.md §8).
- Limb radii, the crippled-leg speeds and the arm-condition share are [design] (§12).
  The owner tunes them in play.
