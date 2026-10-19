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
| `headshot_unaware_crit` | true | a headshot on an unaware target is a critical |
| `bloom_per_shot`, `bloom_recovery`, `bloom_max` | 0.03 × Min Spread, 1.0 /s, 0.25 | spread added per shot (spread units) |
| `jump_spread` | 0.3 | actor-spread penalty while airborne |
| `recoil_base_deg`, `recoil_per_damage_deg`, `recoil_recovery_deg` | 0.4°, 0.03°/damage, 12°/s | view kick |
| `melee_window`, `melee_rays`, `melee_arc_deg` | 0.1 s, 5, 35° | melee swing |
| `explosion_edge` | 0.25 | blast damage at the edge of the radius |
| `default_bounce` | 0.3 | lobbers whose PROJ has no bounce value |
| `auto_reload`, `fallback_reload` | true, 2 s | empty-trigger reload; reload without a clip length |
| `stagger_cooldown` | 3 s | after a hit reaction ends, how long hits on a crippled part start no other (§12) |
| `guard_break` | 1 s | how long the player cannot block after a power attack broke its block (§12) |

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
- Weapon wear per attack moved to `condition.rs` and is now the masters' setting (§17).
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
  NiTextKeyExtraData]. Automatic weapons fire at once, except those that spin up.
- **Spin-up** [owner decision 2026-10-10]. An automatic weapon whose attack clip has
  an internal loop (`Fire` and `Loop` keys: a minigun's `AttackSpin`) fires its first
  shot at the clip's `Fire` key [documented: NiTextKeyExtraData, `Fire` works like
  `Hit`], `spin_up` seconds after the pull, then at its rate while the trigger stays
  held. Letting go during the spin-up fires nothing; every new pull, and firing on
  after a reload, spins up again [design]. `Event::SpinUp` marks the start, for the
  body's clip (docs/spec-notes/animation.md §9.8).
- **Reload.** Takes the reload clip's length; the animation multiplier does not speed
  it up [documented]. Without a clip length, `fallback_reload`.
- **Round by round** (`Timing::rounds`, `RoundReload`). Some reloads load one round at
  a time and can be stopped:
  - The clips [observed: the base meshes archive, 2026-10-11]. Three reload
    animations have an opening clip beside the reload clip, in both the first-person and
    the third-person folders (with sneak forms): `ReloadX` (`1hp` and `2hr`), `ReloadY`
    and `ReloadZ` (`2hr`). No other reload has one.

    | clip | length | text keys (s) |
    |---|---|---|
    | `1hpreloadxstart.kf` | 1.067 | sound `WPN357RevolverReloadStart` 0, `WPN357RevolverReloadClick` 0.667 |
    | `1hpreloadx.kf` | 1.767 | sound `WPN357RevolverReloadLoop` 0, `hit` 0.633, `WPN357RevolverReloadClick` 0.917, `WPN357RevolverCock` 1.467 |
    | `2hrreloadxstart.kf` | 0.933 | sound `WPNLeverActShotgunReloadStart` 0.033 |
    | `2hrreloadx.kf` | 1.900 | sound `WPNLeverActShotgunReload` 0.033, `Hit` 0.5, `WPNLeverActShotgunReloadEnd` 0.667 |
    | `2hrreloadystart.kf` | 0.667 | sound `WPNHuntingSGReloadStart` 0.015 |
    | `2hrreloady.kf` | 1.533 | sound `WPNHuntingSGReload` 0.097, `Hit` 0.467, `WPNHuntingSGReloadEnd` 0.502 |
    | `2hrreloadzstart.kf` | 0.333 | sound `WPNCowboyReloadStart` 0.033 |
    | `2hrreloadz.kf` | 1.667 | sound `WPNCowboyReload` 0.033, `Hit` 0.5, `WPNCowboyReloadEnd` 0.6 |

    (First-person files; the third-person ones have the same lengths, `2hrreloadx.kf`
    its end sound at 0.533, and two openings a `Blend:` key.)
  - The weapons [observed: FalloutNV.esm WEAP reload animation, 2026-10-11]: `ReloadX`
    the .357 Magnum Revolver, Lucky and the Lever-Action Shotgun; `ReloadY` the Hunting
    Shotgun, Dinner Bell and the Grenade Launcher; `ReloadZ` the Cowboy Repeater, the
    Brush Gun and the Trail Carbine. The owner's list (the hunting shotgun, the
    lever-actions, the cowboy repeater, the brush gun, the trail carbine and the .357)
    agrees; the Grenade Launcher and the two uniques come with their animations. No
    list of weapons is kept: the clips decide.
  - The rule [hypothesis: the reload clip's `Hit` "also ends looping reloads",
    documented: GECK wiki NiTextKeyExtraData; the sounds after `Hit` are the
    `…ReloadEnd`, click and cock sounds]: the opening clip plays once; the reload
    clip from its start to `Hit` loads one round, and repeats while rounds are missing
    and carried; after the last round the clip plays on from `Hit` to its end, which
    closes the weapon. The close plays every time [owner decision 2026-10-11].
  - Stopping: a fresh pull of the trigger stops it after the round in hand and the
    weapon closes [owner decision 2026-10-11]. That pull fires nothing (owner,
    playtest of 2026-10-11, replacing "closes first, then fires" of the same morning).
    A later pull while it is stopped or closing is kept for 0.25 s
    (`fnv_combat::attack::SHOT_GRACE`) [design: the owner asked for a grace period; the
    length is chosen]: closed within that time of the pull, the shot follows without
    another pull; else the pull is dropped. A pull while it opens still lets the first
    round go in [design]. The pull that starts a reload on an empty magazine does not
    stop it.
  - Events: `ReloadStarted` (the opening clip), `ReloadRound` (the reload clip from its
    start), `RoundLoaded`, `ReloadClosing { stopped }`, `Reloaded { taken }`.
  - **Checked** [observed 2026-10-11, `nvforever --cell GSHouseInterior03 --play`, exit
    0, "run finished"]: the .357 revolver with 2 missing opened, loaded one round
    0.63 s after the other and closed (`reloading round by round (opens 1.07 s, 0.63 s
    a round, closes 1.13 s)`, `a round in, magazine 5`, `… 6`, `closing`, `reloaded 2`);
    with 3 missing and the trigger pulled with the second round in hand, that round
    went in, it closed (`closing (the trigger was pulled)`) and fired 1.2 s later,
    magazine 3 → 5 → 4. The Cowboy Repeater (0.33 / 0.50 / 1.17 s) and, in third
    person, the Hunting Shotgun (0.67 / 0.47 / 1.07 s) loaded the same way. Screenshots
    during the revolver's reload show the cylinder out, the left hand at it, then
    closed.
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
  - The body gives the attack clip's length and `Hit` key (and whether a `Loop`
    key follows it: the spin-up time) and the reload clip's length, from the rig in
    view. The attack clip is `{prefix}{attack}{is}.kf` from the
    WEAP attack animation; the reload clip is `{prefix}reload{x}.kf`
    (`fnv_actor::player_clips`).
  - The body plays the clip once each time combat counts an attack or a reload; for a
    round-by-round reload the opening clip (`{prefix}reload{x}start.kf`) on the reload
    count and the reload clip from its start on each round's count (§4).
- **Shots.**
  - They leave the eye along the view, inside the cone from §3. The stance is taken
    from the controls and the character: aiming, sneaking, moving, airborne.
  - They are drawn from the muzzle of the weapon in hand (the model's
    `ProjectileNode`, animation.md §9.9; `WeaponAnim::muzzle` from the player body),
    closing on the view ray by the point aimed at. In third person a shot whose
    muzzle has something between it and that point leaves the muzzle instead
    (`fnv_combat::shot::leaving`; docs/design/combat.md §5.1) [design]. The debug log
    gives each shot's eye, aim point, muzzle and where it is drawn from, and each
    tracer's start; the summary at exit counts the shots drawn from the muzzle and
    those from behind cover.
  - The skill, Strength and Luck are the player's actor values
    (docs/spec-notes/actor-values.md): the skill from its attribute and Luck by the
    documented formula, plus anything a script changed.
  - Recoil turns the view's pitch.
- **The world.** Projectiles fly against the physics world (`PhysView`):
  - rays skip the player's capsule and the layers the character passes;
  - actor hit volumes (§7) report the actor;
  - small-transparent colliders are the layers TRANSPARENT_SMALL and
    TRANSPARENT_SMALL_ANIM.
- **Hits.** Hits and blasts are logged with the damage the rules give (target DT 0, no
  body part); applying it is step C5.
- **Feedback** [design, until impact effects and audio]:
  - developer markers, drawn only when switched on (`--hit-markers`, settings page
    Developer → Shot markers; off by default): an impact marker for 0.3 s (0.6 s for
    explosions) and a line from the muzzle along every hitscan shot for 0.08 s, to the
    hit or to the end of the range (for other projectiles at the PROJ tracer chance);
    the muzzle flash and the projectiles' models are §15;
  - an ammo counter (magazine / carried, the ammo's abbreviation).
- **Test aids.** `NVF_PLAY_TEST_CONTROL` presses `fire`, `reload`, `swap` and
  `aimactor` (look at the nearest actor's hit volume). `NVF_PLAY_TEST_TRACERS=<seconds>`
  gives every projectile a line and keeps the lines and a marker at the muzzle that
  long, so one screenshot shows where shots were drawn from (muzzle flashes and
  projectile models are not held).
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
  - NPC: Base Health + (Endurance − 1) × `fAVDNPCHealthEnduranceMult` (5), plus
    (level − 1) × `fAVDNPCHealthLevelMult` (5) for an NPC with "Auto-calc stats"; a
    hand-set NPC's level adds nothing [observed: the AI observer's captures, fifteen
    NPCs at full Health, all fit: auto-calculated the NCR troopers (level 5, Base 10,
    Endurance 5: 50), Fiends 65–105, Joe Cobb 65, Sunny Smiles 90, Ringo 120, settlers
    45, the Strip crowd 45, Violet 170; hand-set Cook-Cook 215 (level 14, Base 180,
    Endurance 8), Doc Mitchell 65, Trudy 75, the Geckoman 70, Ramos 25;
    npc-behaviour-compare.md §9]. Until 2026-10-19 this was Endurance × 5 + level × 5
    for every NPC, 10 more for most and 75 more for Cook-Cook.
  - Creature: its DATA Health.
  - Player: `fAVDHealthEnduranceOffset` + Endurance × 20 + (level − 1) × 5 + Base
    Health [hypothesis until captured].
  - The settings' names and values are observed (combat-data.md §3).
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
    instead of dying [design]; they get up again (docs/spec-notes/unconscious.md).
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
  - A living actor plays a hit reaction over its other clips when the hit cripples a
    body part or lands on a crippled one (the rule and the clips: §12).
  - A dead one whose skeleton has a ragdoll drops as one (ragdoll.md §4;
    `ragdoll.rs`): the Havok bodies and constraints of its skeleton as jointed rigid
    bodies, shoved by the hit that killed it and by later hits.
  - Until its ragdoll is in the world (its ground is not loaded, or it was dead when
    drawn), and for a skeleton without one: it plays `idleanims\deathpose1.kf` or
    `deathpose2.kf` (chosen by reference) and holds its last frame, or goes straight to
    that end pose when already dead when drawn; a skeleton without death poses freezes
    and is laid on its back [design].
  - Dead actors' hit volumes move to DEAD_BIP; a ragdoll's bodies take their place.
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
  - A tap of Use swings on release (`fnv_combat::melee`). Holding Use for `power_hold`
    (0.4 s) starts a power attack [design].
  - **Combos.** An attack clip's `a:` text key names the attack group to play next,
    and its time is when the clip may be interrupted for it [documented: GECK wiki
    NiTextKeyExtraData: `a:R` plays AttackRight, `a:7` Attack7, a blank key a random
    attack]. In the base meshes archive every `AttackRight` clip of `h2h`, `1hm` and
    `2hm` carries `a:L` and every `AttackLeft` clip `a:R`; `h2hattack3` names 4,
    `h2hattack4` 3, `h2hattack5` itself, `h2hattack6` and `7` R; power clips have no
    `a:` key [observed: both rigs, 2026-10-11]. Some times (third person, seconds):

    | clip | Hit | `a:` | end |
    |---|---|---|---|
    | `h2hattackright_a` | 0.30 | L 0.63 | 1.43 |
    | `h2hattackleft_a` | 0.33 | R 0.63 | 0.90 |
    | `1hmattackright_a` | 0.30 | L 0.43 | 1.13 |
    | `2hmattackright_a` | 0.57 | L 0.63 | 1.53 |

    A press during a swing starts the next one at the `a:` key (or at once, if the key
    has passed), playing the group the key names, so the fists alternate right and left.
    A press after the swing has ended starts again from the weapon's own attack group
    (`AttackRight` by default) [design: when the original resets a run is not known].
  - **Variants.** The melee clips come as `_a` and `_b` (`h2hattackleft_a.kf`,
    `…_b.kf`; `h2hattack5` only `_a`, `h2hattack6`/`7` neither) [observed]; each swing
    picks one at random [hypothesis], falling back to the plain and `_a` forms.
  - **Power attacks** play by the movement keys held when they start: `AttackPower`
    standing, `AttackForwardPower`, `AttackBackPower`, `AttackLeftPower`,
    `AttackRightPower` (an axis past 0.3; forward and back win a tie) [observed: those
    clips for `h2h`, `1hm`, `2hm` in both rigs; owner's account of the original,
    2026-10-11; thresholds design]. Sneaking, only the standing one has a sneak form.
    Damage: base × `fDamagePowerAttackStandBonus` (3.0), `…ForwardBonus`,
    `…BackBonus` or `…SideBonus` (2.0 each) [observed: FalloutNV.esm values;
    hypothesis: that they multiply the damage, from their names]. `fDamagePowerAttackBonus`
    (2.0) is not used. A power attack cannot be cut by a combo; a press during it
    waits for its end.
  - The swing takes its own clip's length ÷ (attack × animation multiplier), or
    1 ÷ "Attack Shots/Sec" without a clip (× `power_time`, 1.3, for a power attack). It
    strikes once, at the clip's `Hit` key [documented: NiTextKeyExtraData], or at 40 %
    of the swing without one [design]. The body reports the timing of the clip it plays
    for the swing (`WeaponAnim::timed`) a frame after the swing starts.
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
  logged. Living actors are not pushed until they move physically (AI work). A dead
  actor's ragdoll is shoved by every shot and blow that lands on it (ragdoll.md §4).
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
  - the enchantment (EXPL `EITM`) applies to every actor damaged (hit-effects.md §1);
    radiation and the placed impact object are logged and not built.
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
    `Attacker::arm_condition` by the weapon (`damage::arm_condition`, for every
    attacker: crippled-limbs.md §3);
  - crippled legs slow walking and running to `fMoveOneCrippledLegSpeedMult` (0.85)
    with one leg and `fMoveTwoCrippledLegsSpeedMult` (0.75) with both, as for every
    actor (crippled-limbs.md §4; 0.7 and 0.5 [design] before 2026-10-18);
  - the HUD's health line lists crippled limbs.
  - What every crippled limb does, to the player and to NPCs: crippled-limbs.md.

  Nothing hurts the player yet (no NPC attacks); scripts can (`DamageActorValue`).
- **Hit reactions** (`damage::staggers`, `deaths.rs`) [design: the owner's rule,
  2026-10-11; before it every hit played one, and fast hits held an actor in it]:
  - The hit that cripples a body part always plays one.
  - A later hit on a part that is already crippled plays one when the actor's last
    reaction ended at least `stagger_cooldown` seconds ago (3 s; one clock per actor,
    whichever part was hit).
  - A power attack that breaks a block (below) always plays one.
  - Any other hit plays none: a hit on a healthy part, or a blast (no part).
  - The clip is the one for where the hit landed: `mt_hithead`, `mt_hittorso`,
    `mt_hitarmleft`/`right` or `mt_hitlegleft`/`right` (`idleanims`, the `_male`
    folder) [observed: the clips exist; design: which location plays which].
  - The log names each one ("hit reaction: … staggers").
- **Power attacks break blocks** (`damage::against_block`) [design: the owner's
  request, 2026-10-16; no page documents the original's rule]: a power attack (the
  player's, or a creature's such as the radscorpion's sting) that meets a blocker from
  its front is not reduced by the block. All of it is dealt, the attacker does not
  recoil, and the blocker does not play its block hit. A blocking NPC staggers instead
  (its torso hit reaction, whatever part was hit); the stagger ends its block and holds
  its attacks while it plays (combat-ai.md §14.3). The player has no stagger clip, so
  a broken block means it cannot block for `guard_break` seconds (1 s). The log says
  "breaks …'s block".
  - Checked [observed 2026-10-11, `nvforever --play`, exit 0, "run finished"; the
    Goodsprings Settler 0x00104F0A, 9mm pistol, `aimpart=0` and `aimnpc`]:
    - Four torso hits and an arm hit on healthy parts played nothing.
    - The fifth torso hit crippled the torso and played `mt_hittorso` (1.4 s).
    - The hit that crippled the head played `mt_hithead` (4.5 s); a second head hit
      1.2 s later played nothing.
    - A head hit 3.4 s after the crippling one played the reaction again. That run
      counted the cooldown from the reaction's start; counting from its end was not
      seen in play (the settler walks behind a wall before it runs out) and rests on
      the rule's test.
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

## 14. Shots on objects (bottles, clutter)

What a shot does to a placed object that is not an actor. Found with the tutorial's
bottles (quest `VCG02`, "Back in the Saddle"), which a shot neither counted nor moved.

- **The bottles' script** [observed: `VCG02TargetSCRIPT` on MISC `VCG02Bottle`, seven
  references on the fence behind the Prospector Saloon, enabled with
  `VCG02BottleMarkerREF` by Sunny's line 0x0010A1E6]:
  - `Begin OnHitWith` without a parameter counts the hit when all of these hold:
    `VCG02` is running, this bottle was not counted before, `player.IsWeaponOut == 1`,
    `player.GetWeaponAnimType` is above 3 and below 9, and the player does not have the
    Codac R9000 camera equipped.
  - A counted hit makes Sunny say `VCG02SunnyShotReaction`, adds 1 to
    `VCG02.nTargetCount` and sets `CGTutorial` stage 62. The third completes objective
    10 and re-evaluates Sunny's package.
- **Hit blocks on objects.**
  - The target of OnHit and OnHitWith is "any reference" [documented: GECK wiki Event
    Handling, the owner's export], and they run before damage is dealt.
  - `Game::hit_object` runs the reference's OnHit blocks with the attacker and its
    OnHitWith blocks with the weapon; nothing else changes. The engine calls it for
    every shot that lands on a placed object.
  - [hypothesis] OnHit's subject is the attacking actor. That page gives a projectile
    reference for a ranged hit on a non-actor; we have no projectile references.
  - Melee blows only strike actors so far (§10), so they run no object's blocks.
- **`IsWeaponOut`** [documented: catalogue]: 1 while the actor has a weapon out; the
  value changes when the weapon is attached to the weapon node. The engine reports the
  player's (`WeaponAnim::ready`: drawn and in hand). No one reports other actors yet:
  they read 0.
- **`GetWeaponAnimType`.**
  - [documented: GECK wiki GetWeaponAnimType] it tells 11 animation types apart and has
    one catch-all value; several of the record's 13 (Fallout 3) or 14 (New Vegas) types
    share a value. The page's table is a template that is not in the owner's export,
    and the site refuses automated readers.
  - [hypothesis] the numbering in `functions::weapon_anim_code`:

    | WEAP DNAM animation type | value |
    |---|---|
    | 0 Hand to Hand | 1 |
    | 1 Melee (1 Hand) | 2 |
    | 2 Melee (2 Hand) | 3 |
    | 3 Pistol - Ballistic, 4 Pistol - Energy | 4 |
    | 5 Rifle - Ballistic, 7 Rifle - Energy | 5 |
    | 6 Rifle - Automatic | 6 |
    | 8 Handle | 7 |
    | 9 Launcher | 8 |
    | 10 Grenade Throw | 9 |
    | 11 Land Mine | 10 |
    | 12 Mine Drop | 11 |
    | 13 Thrown, anything else | 0 |

    No weapon equipped reads 1 (the actor fights with its hands).
  - What it rests on [observed: shipped scripts and records, `FalloutNV.esm`]:
    - `GetWeaponAnimType == 4` is tested together with `GetEquipped NVWeapMS22Camera`
      (SCPT 0x001136AE); that weapon's record type is 3. So the values are not the
      record's.
    - Four weapon-alarm scripts test `IsWeaponOut == 1 && GetWeaponAnimType != 1`:
      raised fists are no alarm, so Hand to Hand is 1.
    - Two scripts treat `>= 2` as armed and `<= 1` as unarmed.
    - The bottles' `> 3 && < 9` is then every gun (4 to 8) and nothing else.
    - 13 types in 11 values needs two pairs to share. The pistols are one (above). The
      rifles as the other is a guess.
  - The bottles count the Varmint Rifle (record type 5) and the 9 mm Pistol (3) whether
    or not the rifle guess holds: both land in 4 to 8 either way.
- **The push** [design]:
  - A shot strikes the loose (dynamic) body it lands on at the point of impact, along
    its flight: `PhysWorld::push_ref`.
  - Impulse: the projectile's Impact Force (PROJ DATA) times `OBJECT_PUSH` (1), read
    as mass × metres per second in the stored collision's units [hypothesis: the
    field's unit is not documented]. The masters' bullets carry 3 to 6.
  - The speed gained is the impulse over the body's mass, at most
    `OBJECT_PUSH_MAX_SPEED` (700 units/s). The turn it gives is bounded as a ragdoll's.
  - A bottle (mass 1) hit by the Varmint Rifle (Impact Force 3) leaves at 210 units/s.
  - Fixed and animated collision is not pushed.
- **Two faults found on the way.**
  - A sleeping body that was struck and then queried before the next physics step never
    moved (physics.md §5, rapier quirk). Explosions' pushes had the same fault.
  - Objects a script enables during play (`script_draw.rs`) were not redrawn where
    physics moved them; they are now (`script_draw::follow`).
- **Test aids:** `NVF_PLAY_TEST_CONTROL` `aimref=<FormID>` aims at a placed object's
  body; `NVF_PLAY_TEST_NO_SPREAD=1` makes shots leave exactly along the view.
- **Checked** [observed 2026-10-10, `nvforever --play`, exit 0, "run finished"]: with
  `VCG02` started and Sunny's line 0x0010A1E6 said, the player drew the Varmint Rifle
  and shot bottles 0x0010A202, 0x0010A203 and 0x0010A204 from 260 units.
  - Each shot ran the bottle's OnHitWith block and Sunny said a reaction line.
  - The first set `CGTutorial` stage 62; the third completed `VCG02` objective 10.
  - Each bottle left the fence at 210 units/s; one came to rest on the ground 94 units
    from where it stood.

  ```
  NVF_PLAY_TEST_NO_SPREAD=1 NVF_PLAY_TEST_GOTO=1:-68375:4800:8600
  NVF_PLAY_TEST_SCRIPT=3:start:VCG02,4:info:0x0010A1E6:SunnyRef
  NVF_PLAY_TEST_CONTROL=7:weapon,10:aimref=0010A202,10.5:fire,10.7:fire,13:aimref=0010A203,13.5:fire,13.7:fire,16:aimref=0010A204,16.5:fire,16.7:fire
  nvforever --data <Data> --cell Goodsprings --play --pos=-68375,4800,8700 --run-seconds 22
  ```
- **Open:** the `GetWeaponAnimType` table (owner-review.md); how hard the game's shots
  push clutter (the planned projectile captures); impact effects and sounds.

## 15. What is drawn of a shot: muzzle flash and projectiles

Owner request 2026-10-10: "actual bullet projectiles that aren't invisible along with
the muzzle flash effects". Where a shot is drawn from is docs/design/combat.md §5.1 and
animation.md §9.9.

- **The records** [documented: xEdit PROJ; GECK wiki Projectile, §4.2 of combat.md]:
  a PROJ has a model, a tracer chance ("how often the projectile is drawn"), a fade
  duration (beams), and with its Muzzle Flash flag a flash model (`NAM1`), a flash
  duration and a flash light (LIGH). Read with `fnvtool combat weapon` [observed]:

  | Weapon | PROJ | Type, flags | Speed | Tracer chance | Model | Flash model, duration |
  |---|---|---|---|---|---|---|
  | 10mm pistol | `10mmBulletProjectile` | Missile, Hitscan | 27,520 | 0 | `projectiles\10mmprojectile.nif` | `handgunmuzzleflash01.nif`, 0.04 s |
  | Minigun | `MinigunTracerProjectile` | Missile, Hitscan | 7,000 | 0.3 | `projectiles\762mmprojectile.nif` | `minigunmuzzleflash01.nif`, 0.04 s |
  | Laser rifle | `BeamLaserProjectile` | Beam | 10,000 | 0 | `projectiles\testlaserbeamsteady.nif` (fade 0.14 s) | `laserriflemuzzleflash.nif`, 0.14 s |
  | Missile launcher | `MissileProjectile` | Missile, Explosion | 1,550 | 0 | `projectiles\mlprojectile.nif` | `assaultriflemuzzleflash.nif`, 0 s |
  | Plasma rifle | `PlasmaProjectile` | Missile | 7,500 | 0 | `projectiles\plasmaprojectile01.nif` | `plasmariflemuzzleflash.nif`, 0.12 s |
  | Flamer | `FlameProjectile` | Flame, Hitscan | 12,000 (range 640) | 0 | `projectiles\flamerflame01.nif` | `flamermuzzleflash01.nif`, 0.2 s |
  | Frag grenade | `GrenadeFragProjectile` | Lobber, Explosion | 1,200 | 0 | the grenade's own model | none |

- **The models** [observed: `crates/fnv-nif/examples/effect_model_survey.rs` on
  `Fallout - Meshes.bsa` and `Update.bsa`; 63 files under `meshes\projectiles`, 16
  under `meshes\effects\muzzleflashes`]:
  - they are built along +y: the 10mm bullet fills y 0 to 1.5 (1 unit across), the
    missile y −55.7 to 12.7 with its flame behind, the laser beam y −2.7 to 2048 with
    a node `BeamEnd` at (0, 2048, 0), the handgun flash y −5 to 31.7 and the minigun's
    y −10.6 to 77;
  - `762mmprojectile.nif` holds a bullet (`Projectile`, 0.8 by 3 units) and a streak
    244 units long and 4 across under a `NiBillboardNode` named `Tracer`;
  - `10mmprojectile.nif` holds one shape, the bullet, under a node `10mm`, beside
    empty nodes named for other calibres;
  - `NiBillboardNode`: in 10 of the 63 projectile models and 13 of the 16 flashes, 31
    nodes in all, with billboard mode 1 (12), 4 (13) and 2 (6). The mode-1 nodes of the
    files looked at hold geometry that is long in y (the tracer streak, the laser
    beam), and the handgun flash's mode-1 node is turned a quarter turn about y in the
    file, the others not at all;
  - they also hold `BSValueNode`s named `AddOnNode<n>` (two in each bullet flash,
    three behind the missile, four in the plasma bolt), and the flash planes have a
    `NiTransformController` with no sequence in the file.
- **Readings** [hypothesis]: +y is the models' forward axis; the flash model is
  attached at the weapon's `ProjectileNode` with the node's rotation (whose y axis lies
  along the barrel, animation.md §9.9); `BeamEnd` marks the beam's far end so the model
  can be stretched to the beam's length; a billboard node shows its +z side and, in the
  "rotate about up" modes, turns about its own y [documented: the mode names, nif.xml
  BillboardMode]. How the game draws a hitscan bullet is not known.
- **Engine** (since 2026-10-10):
  - combat sends a `ShotFired` (the PROJ) for each shot and lists every projectile to
    draw in `ShotVisuals` (PROJ, place, direction; `nvf_play::combat`):
    - a projectile stepped in flight (missile, plasma, grenade): where it is seen
      (from the muzzle, closing on its path);
    - one whose hit is found at once (hitscan) [design]: a bullet flying from the
      muzzle to where it landed at the PROJ's speed, gone on arrival;
    - a beam [design]: from the muzzle to its end, for the PROJ's fade duration. Its
      start stays on the muzzle while it shows, and so does the start of the line
      along a hitscan shot: the attack clip moves the gun right after the shot
      [observed: the laser rifle's `ProjectileNode` is 10.3 units higher and 4.4 to
      the side one frame after it fires, and stays there], and a beam left where the
      muzzle was hung below the barrel (the owner: "that laser beam looks too low");
    - every projectile is listed, not the tracer chance of them
      (`DRAW_EVERY_PROJECTILE`) [design: the owner's request];
  - `nvforever::shot_fx` draws them:
    - the flash model under the `ProjectileNode` entity of the rig in view
      (`PlayerBody::muzzle_entity`), for the PROJ's flash duration and always for the
      frame it is made in, with a point light of the LIGH's colour, fade and radius
      (no shadows);
    - the PROJ's model at each listed projectile, +y along the flight; a beam's model
      scaled along y by its length ÷ `BeamEnd`'s y;
    - billboard nodes turned to the camera each frame (`ModelNode::billboard`);
    - the models of the weapon in use are loaded when it is equipped, so the first
      flash is not lost to loading;
  - a weapon without a `ProjectileNode` (or a run without a player body) shows no
    flash; its projectiles are still drawn.
- **Checked** [observed 2026-10-10, `--play` at Goodsprings, `--hour 12`, third person
  from the side (`orbit=75:5` at the screenshot's time), exit 0 and "run finished";
  screenshots kept privately]:
  - Minigun, trigger held: "46 muzzle flashes (46 with a light), 46 projectile models
    drawn, 0 projectiles without a model". During the burst one flash stands at the end
    of the barrel, pointing along it; 0.4 s after the trigger is let go there is none.
    A frame of another run shows a tracer streak in flight ahead of the barrel.
  - Laser rifle, one shot: "BeamLaserProjectile drawn at (−71672.1, 2091.8, 8437.5),
    a beam 628 units long"; the red beam runs along the shot from the muzzle, and
    0.1 s after the shot still starts at the muzzle, which has risen since.
  - The muzzle's place worked out from the pose (`Rig::muzzle_at`) and the drawn
    node's place a frame later agree to 0.1 unit in every frame logged, standing and
    through a shot.
  - Before the flash time was the record's alone it was held to at least 0.05 s, and
    the test aid held it for seconds: the owner saw the minigun's flash "sticking
    around way too long". Now 0.04 s is one frame at 25 frames a second.
- **Not built:** the controllers inside the flash models, the PROJ's own light, NPCs'
  shots. Add-on nodes are docs/spec-notes/particles.md; sounds, impacts and shell
  casings are §16. Not looked at in a run: the 10mm bullet, plasma bolt and grenade
  models (their PROJs go the same way).
- A model is not shown until it has flown the length it reaches behind its origin
  [design]: the minigun round's streak (220 units behind the bullet) ran back through
  the gun and the shooter in its first frame [observed].
- **Open:** whether the game draws a hitscan bullet at all outside its tracer chance,
  and at what speed; whether a billboard shows +z or −z (the flash glows and the beam
  show with +z); the flame's model has no shape of its own (add-on nodes only): its
  flame is particles (particles.md).

## 16. Sounds, impacts and shell casings

Owner request 2026-10-11: build what §15 left out. Particle effects are
docs/spec-notes/particles.md.

- **The records** [documented: xEdit; schemas/fnv `objects.json`, `combat.json`,
  `placed.json`], read by `fnv_combat::fx` (`fnvtool combat fx --form <weapon>` prints
  a weapon's):
  - WEAP: `SNAM` twice (Shoot 3D, Shoot Dist), `XNAM` Shoot 2D, `NAM7` Shoot 3D
    Looping, `TNAM` Melee Swing / Gun No Ammo, `NAM9` Equip, `NAM8` Unequip, `INAM`
    impact data set, `MOD2` shell casing model (read from the raw subrecord: the WEAP
    layout in the schema does not list it);
  - SOUN: `FNAM` a file or a folder under `sound\`, `SNDD` flags, minimum and maximum
    attenuation distance (stored ÷ 5 and ÷ 100), static attenuation in hundredths of a
    decibel, frequency adjustment in percent;
  - PROJ `DATA` sound; EXPL `DATA` sound 1, sound 2, impact data set;
  - IPDS: an IPCT for each of twelve materials (stone, dirt, grass, glass, metal, wood,
    organic, cloth, water, hollow metal, organic bug, organic glow);
  - IPCT: model, effect duration, effect orientation (Surface Normal, Projectile
    Vector, Projectile Reflection), texture set, sound 1, sound 2.
  - Read [observed, FalloutNV.esm]: 3,190 SOUN, 125 IPCT in 60 IPDS, 37 ADDN.
- **Examples** [observed]: the 10mm pistol's Shoot 2D is `WPN10mmFire2D`, the folder
  `fx\wpn\10mm_pistol\fire_2d\` (flag 2D, static attenuation 13.22 dB); its No Ammo
  `WPNPistol10mmFireDry` is one `.wav`; its casing `Projectiles\45CalCasing.NIF`; its
  impact data set gives `BallisticStoneImpact` (`Effects\ImpactBallisticConcrete01.NIF`,
  0.25 s, Projectile Reflection) for stone, `BalisticDirtImpact` for dirt and grass, and
  `BallisticOrganicImpact` (blood, 0.08 s, Surface Normal) for organic.
  `fGunShellEjectSpeed` is 160.
- **Sounds** (`nvf_play::sfx`):
  - a `PlaySound` names a SOUN and, for a sound with a place, a point. A folder stands
    for the `.wav` files directly in it, one picked at random [hypothesis];
  - WAV is decoded here (PCM of 8, 16 and 24 bits, 32-bit float) and played through
    Bevy's audio as a source of our own; another encoding is reported once;
  - loudness [design]: gain 10^(−static attenuation ÷ 20); with a place, full within
    the minimum distance of the player's eye, then minimum ÷ distance, fading to
    nothing at the maximum distance. Not panned. The record's attenuation curve is not
    used;
  - pitch: the frequency adjustment, and ±5% at random with Random Frequency Shift
    [design: the size];
  - combat plays: Shoot 2D (else Shoot 3D) for each shot of the player's [hypothesis:
    that the 2D sound is the shooter's own]; the Shoot 3D Looping sound going round
    from a weapon's first shot until 0.25 s after its last [hypothesis: when; design:
    the 0.25 s]; No Ammo on an empty trigger; Equip and
    Unequip when the player starts to draw or put away [hypothesis: when]; an impact's
    sounds at the impact; an explosion's at the explosion;
  - `nvforever --no-sound` turns them off.
- **Impacts.** A shot that lands takes the IPCT its weapon's impact data set gives for
  the surface:
  - the collision material of the triangle hit [documented: nif.xml
    `Fallout3HavokMaterial`, 32 materials repeated as platforms and stairs] is mapped
    to one of the twelve by its name [hypothesis: `fnv_combat::fx::impact_material`];
    terrain, which has no code, counts as dirt [design]; an actor takes its base's
    Impact Material Type (NPC_ / CREA `NAM4`) [documented: xEdit], organic without one
    [design]; convex shapes' materials are read too since 2026-10-11 (bottles had
    counted as dirt; audio.md §11.1 has the table by surface);
  - `nvforever::shot_fx` shows the IPCT's model at the point for its effect duration,
    its +z along the orientation the IPCT names [hypothesis: +z, from the impact
    models' add-on nodes standing along +z]. The model's add-on nodes emit dust and
    debris (particles.md).
- **Shell casings.** The player body gives the weapon model's `ShellCasingNode` place
  and its +z axis to combat (`WeaponAnim::casing`). Each shot of a weapon with a casing
  model throws one from there along that axis at `fGunShellEjectSpeed`; it falls under
  the world's gravity, bounces (keeping 0.35 of its speed [design]) and is removed
  after `fGunShellLifetime`.
  - For +z as the way out [observed: `muzzle_node_survey --list`]: in the 59 models
    whose node is turned by no right angle, its z axis points to the side the node sits
    on and up (9mm (−0.38, 0.36, 0.85), service rifle (0.17, 0.59, 0.79)); the cowboy
    repeater's points up (lever actions throw up).
- **Checked** [observed 2026-10-11, `--play` at Goodsprings, exit 0, "run finished"]:
  - 10mm pistol: "sfx: 4 sounds played": `WPNPistol0Equip`, `WPN10mmFire2D` (gain
    0.218), `FXBulletImpactEarth` at 656 units (gain 0.276, `BalisticDirtImpact` on
    terrain), `WPNPistol0EquipUn`. No file failed to decode. Nobody listened: the check
    is that the files are found, decoded and handed to the audio output.
  - Minigun burst: "40 shell casings, 40 impact effects"; impacts named concrete, dirt
    and wood by surface; casings fall to the right of the gun and lie on the ground.
  - The minigun has neither a Shoot 2D nor a Shoot 3D sound [observed: `fnvtool combat
    fx --form WeapMinigun`]: its only shoot sound is the loop `WPNMinigunSpinLPM`
    (`fx\wpn\minigun\wpn_minigun_spin_lpm.wav`). Where its gunfire sound comes from is
    open [hypothesis: the attack clip's text keys].
- **Not built:** the Shoot Dist
  sound, panning and reverb, sounds named by animation text keys (reloads), the PROJ's
  own sound (a bullet passing), impact decals (the IPCT's texture set), the animations
  inside impact models (they grow and fade), explosion models, casings hitting the
  ground with a sound.
- **NPCs' shots** are not built and cannot be on their own: NPCs do not attack yet (no
  combat packages, detection or combat styles; docs/design/combat.md §8). What a shot
  shows and sounds like is ready to be used by them: the messages carry places, not
  the player.

## 17. Item condition: wear, armour, broken weapons

Owner's request, 2026-10-11. Code: `fnv_combat::condition`, `fnv_sim::inventory`,
`engine/nvf-play/src/combat.rs`.

- **What condition is.** A fraction of the item's full health, kept with the carried
  item (`Extra::condition`, simulation.md §3). Full health is the record's: WEAP and
  ARMO `DATA` health [documented: xEdit, combat-data.md §1].
- **Settings in FalloutNV.esm** [observed, `fnvtool esm decode`]:

  | Setting | Value | Used |
  |---|---|---|
  | `fDamageToWeaponValue` | 0.2 | health a weapon loses per attack |
  | `fDamageToArmorPercentage` | 0.35 | share of a hit's damage the armour loses |
  | `fDamageToWeaponGunMult`, `…EnergyMult`, `…LauncherMult`, `…MeleeMult` | 0.03, 0.04, 0.06, 0.05 | no |
  | `fVATSDamageToWeaponMult` | 1.0 | no |
  | `fDamageGunWeapCondBase`, `fDamageGunWeapCondMult` | 0.66, 0.34 | no |
  | `fWeaponConditionRateOfFire1`–`9` | 0.82 to 0.98 in steps of 0.02 | no |
  | `fWeaponConditionJam1`–`4`, `fWeaponConditionSpread1`–`9` | 0 | no |

  What each of them does in the original is not documented anywhere we can cite. The
  two we use are read by `Rules` like the other settings.
- **Weapon wear.** Each attack takes `fDamageToWeaponValue` health points, times the
  record's Damage to Weapon Mult when its override flag is set, with the ammunition's
  Weapon Condition effects applied (`condition::weapon_wear`).
  - [hypothesis: the setting's role.] It fits the published figure the owner quoted:
    "about 395 standard rounds, the equivalent of 33 reloads" for the 10mm Pistol from
    full condition. Its record has health 80 and 12 rounds [observed]: 80 ÷ 0.2 = 400
    shots, 33 full magazines.
  - The figure is 5 shots short of 400, which is exactly one health point
    (79 ÷ 0.2 = 395). [hypothesis, not built: the original may count a weapon with
    less than one point as broken.] Ours breaks at zero.
  - A shot wears the weapon whether it hits or not. A melee or unarmed swing wears it
    only when it lands, once however many it strikes [design]. Thrown weapons are
    spent, not worn. The Fists are not an item and do not wear.
  - The per-class multipliers (gun, energy, launcher, melee) and the weapon's damage
    play no part [hypothesis: with them the 10mm Pistol would not come out near 395].
- **Armour wear.** A hit takes `fDamageToArmorPercentage` of its damage before armour
  from the armour it lands on, as health points (`condition::armor_wear`)
  [hypothesis: the setting's role, from its name].
  - Where it lands [design: the owner's rule]: a hit on a head part (Head 1, Head 2,
    Brain) wears what is worn on the head, any other hit what is worn on the upper
    body, an explosion both (`condition::Zone`, `CombatForms::armor_struck`).
  - "On the head" is any of the biped slots Head, Hair, Headband, Hat, Eye Glasses and
    Mask; "on the upper body" is the Upper Body slot [documented: xEdit's slot names;
    observed: a power armour helmet sets exactly those six, body armour sets Upper
    Body].
  - Pieces struck together share the loss evenly. A head hit on a bare head, or a body
    hit on an actor with no body armour, wears nothing.
  - A target that takes no damage (dead, a child, invulnerable) does not wear.
  - **Only the player's armour wears.** In the original an NPC's armour kept its
    condition through a whole fight: Cook-Cook's stayed at health 85 and his Damage
    Threshold at 13 through 60 hits, the troopers' and Fiends' likewise [observed: AI
    observer campaign `fight_stats_round`, 2026-10-19, npc-behaviour-compare.md §9]. An
    NPC's weapon does wear (Cook-Cook's Incinerator lost 2.19 health a shot) [observed,
    same run].
- **A weapon that is hit** [design: the owner's rule]. An actor holding its weapon
  drawn has a hit volume along the weapon (`hitvol::WEAPON_PART`: one capsule along the
  longest side of the model's bounds, `nvforever::limbs::weapon_segment`).
  - A shot or blow that lands there takes its damage before armour from the weapon's
    health. The actor is not hurt, and its hit blocks do not run [design].
  - A holstered weapon has no volume: a shot at the hip or the back hits the actor.
  - The weapon is the one the renderer put in the hand. The simulation rolls an
    actor's inventory on its own, so when it lacks that weapon one is added first
    [design; the two choices should become one].
- **What a worn item is worth** [documented: GECK wiki SetConditionDamagePenalty and
  SetArmorConditionPenalty (JIP), the hard-coded formulas those functions override]:
  - a weapon deals all its damage at and above 75 % condition; below,
    `1 − (0.75 − condition) × 0.67` of it (`damage::condition_mult`): 83 % at half
    condition, 49.75 % at zero. The original's troopers' Service Rifles at 30, 45 and
    50 % dealt 0.698, 0.799 and 0.832 of the full-condition hit [observed: §9 of
    npc-behaviour-compare.md]. Every actor's shots and blows use its weapon's real
    condition;
  - a piece of armour gives all its DT and DR at and above half condition; below,
    `1 − (0.5 − condition)` of them (`condition::armor_mult`): half at zero. Armour at
    zero stays worn.
  - An NPC's weapons and armour start at the condition its leveled lists give them:
    the COED after an LVLO (the CondNV… lists, e.g. CondNVServiceRiflelLoot: a Service
    Rifle at 30 to 60 %) [documented: GECK wiki Container, "Health %"; observed: the
    original's troopers drew them at 30 to 50 %].
- **A broken weapon** (zero condition) [design: the owner's rule]:
  - the player's is unequipped at once and stays in the inventory. `World::equip`
    refuses it (`SimError::Broken`) until it is repaired; a second, working one of the
    same kind is equipped instead. The player falls back to the Fists;
  - another actor's falls to the ground: its unit leaves the inventory as a dropped
    reference at the point of the hit, keeping condition 0
    (`World::drop_equipped`), and `WeaponBroken` makes the renderer take the model out
    of the hand and stop the weapon clips. Picked up, it cannot be equipped.
- **Which unit wears.** The equipped one. An NPC's weapon and armour are not marked
  equipped in the simulation until they first wear; then one unit is (plain ones
  first) and keeps the wear (`condition::wear_item`) [design].
- **Shown.** The ammo line ends with the weapon's condition ("CND 95%"); the inventory
  menu adds "(CND 46%)" or "(broken)" to a worn weapon or piece of armour; a message
  names a weapon that breaks. The percentage reads 0 only when broken.
- **Scripts.** `GetWeaponHealthPerc`, `SetWeaponHealthPerc`, `ModWeaponHealthPerc` and
  `AddItemHealthPercent` read and write the same value. A script that sets the
  player's weapon to 0 has it unequipped on the next frame; another actor's stays in
  its hand until it is hit [not built].
- **Test aids:** `condition=<percent>` (test_aid.rs) sets the condition of what the
  player has equipped; `aimpart=254` aims at the weapon the nearest NPC holds drawn.
- **Checked** [observed 2026-10-11, `nvforever --play` at Goodsprings, exit 0, "run
  finished"]:
  - 10mm Pistol set to 0.2 %: the first shot broke it ("the player's 10mm Pistol broke:
    unequipped"), the player went to the Fists, and `equip=` was refused ("is broken:
    it cannot be equipped until it is repaired").
  - 21 shots took the 10mm Pistol to "CND 95%" on the HUD (21 × 0.2 of 80).
  - `--actor-weapons drawn`, `aimpart=254`, the Goodsprings Settler 0x00104F0A's
    Varmint Rifle (health 120): ten hits of 12.6 took it 100 → 89.5 → … → 5.7 → 0 %;
    it "broke and fell" as reference 0xFF000000, which the dropped-item code drew.
    The settler's health did not change. Seen in the log only: the settler was too far
    off in the screenshot to see the empty hand.
  - Torso hits on a settler: its Field Hand Outfit lost 4.4 points per hit (0.35 of
    12.5), 100 → 95.6 → 91.3 → 86.9 → 82.6 %.
- **Not built:** repair (the owner: later); NPCs' own shots wearing their weapons and
  hits on the player's armour and weapon (NPCs do not attack yet; the code paths take
  any actor); jamming, slower fire and wider spread of a worn weapon; a worn item's
  lower value; armour worn to zero coming off; the per-class and VATS multipliers.
