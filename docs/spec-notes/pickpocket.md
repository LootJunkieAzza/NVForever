# Pickpocketing (ESM full audit gap 6, 2026-10-24)

Sources: the GECK wiki export (owner's, `C:\fnv-private\geckwiki`): pages Pickpocket
(GameMechanics.xml), SetGhost, Category:NPC, Combat Tab (Pickpocket topics), Projectile
(notes on planted explosives), GetPlantedExplosive, ActorHasBaseFlag,
SetPlayerCanPickpocketEquippedItems and SetCaughtPCPickpocketting (functions.xml).
FalloutNV.esm's settings [observed: raw read, 2026-10-24]. Code: `fnv_game::pickpocket`
(rules), `nvf_play::pickpocket` (play), the container menu's steal mode
(`nvf_play::menu::model::Steal`).

## 1. Who can be pickpocketed

- Every NPC: NPCs "can all be pickpocketed" [documented: Category:NPC].
- A creature only with ACBS bit 28 Allow Pickpocket [documented: ActorHasBaseFlag]; 124
  in FalloutNV.esm (wild dogs, giant rats, big horners, ED-E's upgrade forms), 2 in Dead
  Money, 5 in Old World Blues, 4 in Honest Hearts, 1 in Lonesome Road [observed]. Read from
  the definition supplying the base data (template flag Base Data) [hypothesis].
- Not a ghost: "Cannot be pickpocketed, despite the prompt still showing up. Instead,
  regular activation (dialog) occurs" [documented: SetGhost notes]. NVForever shows
  Talk on a ghost instead of Steal [design: the original's prompt is a known quirk].
- Not dead (searched instead), not hostile to the player or fighting it (only its name
  shows, as for talking), not unconscious, not a teammate [hypothesis for the last two:
  a companion's inventory is open to the player through the wheel].
- Not one that caught the player before: it "has already caught you." (GMST
  `sNoPickPocketAgain` [observed]; SetCaughtPCPickpocketting "originally is set when
  player fails a pickpocket attempt … player can't pickpocket NPC anymore" [documented]).
  E on it shows that line after its name and opens nothing.
- The player must be sneaking (IsSneaking) [hypothesis: the Combat Tab's
  PLAYERPICKPOCKET topic is "used when the player is sneaking near an actor"; the
  original's E while sneaking opens the pocket].

The prompt reads **Steal** [hypothesis: the original's word is not in the masters' GMSTs].

## 2. The chance

```
chance = (thief Sneak × fPickPocketActorSkillMult + fPickPocketActorSkillBase)
       + (target Sneak × fPickPocketTargetSkillMult + fPickPocketTargetSkillBase)
       + (item value × fPickPocketAmountMult + fPickPocketAmountBase)
```

rounded down, kept within `fPickPocketMinChance` and `fPickPocketMaxChance` [documented:
Pickpocket]. Thief Skill is the player's Sneak, Target Skill the target's Sneak
[documented]. A creature's Sneak is its CREA Stealth skill (as GetActorValue Sneak reads
it, `actor_values`) [hypothesis].

| Setting | Wiki default | FalloutNV.esm |
|---|---|---|
| fPickPocketActorSkillMult | 0.6 | 0.6 |
| fPickPocketActorSkillBase | 40 | 40 |
| fPickPocketTargetSkillMult | −0.6 | −0.6 |
| fPickPocketTargetSkillBase | 0 | not set |
| fPickPocketAmountMult | −0.5 | −0.5 |
| fPickPocketAmountBase | 0 | not set |
| fPickPocketMinChance | 5 | not set |
| fPickPocketMaxChance | 85 | 85 |

- **Item value** is the moved units' value: one unit's value × the count moved
  [hypothesis: the wiki says "Item Value"; taking a stack of 50 caps at once is then much
  harder than taking one]. A key's value "is used to weight the chance of getting caught
  when pickpocketing the key" [documented: Key], which agrees.
- **Each take and each placement** rolls once against its chance [hypothesis: placing is
  judged as taking is]. The menu's line shows the chance of the selected row for one
  unit [design: the original's display is not captured].

## 3. The menu

The container menu on the victim's inventory, its title the victim's name. Equipped items
are not listed ("By default, the player cannot pickpocket equipped items" [documented:
SetPlayerCanPickpocketEquippedItems]; hypothesis: a stack with one unit equipped is hidden
whole). Take All is refused [design: each take is its own roll]. The game clock stops as
in any container menu.

## 4. Success and failure

- **Unseen take**: theft from the victim (its base is the owner): karma
  `fKarmaModStealing` (−5) unless the victim is Evil, as for any theft (crime-karma.md
  §3.1, an unseen theft costs karma too); Pockets Picked +1 per take [hypothesis: per take].
  No witness turns [hypothesis: others watching are not judged].
- **Caught** (the roll fails): nothing moves and the menu closes. The victim says
  **Pickpocket** ("Said by an actor if they notice you picking their pocket" [documented:
  Combat Tab]) and turns on the player as a seen thief does (a crime enemy; the crime
  counts against it as `Kind::Pickpocket`, a minor crime) [hypothesis: as Steal]. One that
  ignores crime says **PickpocketNoCrime** ("doesn't regard it as a crime" [documented])
  and does not turn. It will not be pickpocketed again (§1). No karma for a failed try
  [hypothesis].

## 5. Planted explosives ("exploding pants")

"Grenade and mine-type weapons that use an exploding projectile with the Alt. Trigger flag
can be reverse-pickpocketed onto NPCs. If successful the weapon goes live and explodes in
the NPC's inventory after fProjectileInventoryGrenadeTimer has elapsed, always killing it
regardless of its HP … GetPlantedExplosive will return true on the NPC after
fProjectileInventoryGrenadeFreakoutTime has elapsed" [documented: Projectile notes].
FalloutNV.esm: timer 2.0 s, freak-out 0.1 s [observed].

- A placed weapon whose projectile has Explosion and Alt. Trigger goes live, one per unit.
  The timers run on the game clock [hypothesis], so they stand still while the menu is
  open and start when it closes.
- When the timer is up: the victim loses the explosive and dies (killed by the player),
  Pants Exploded +1, and the projectile's explosion goes off where it stands, its blast
  the player's (it hurts others as a thrown grenade's does) [hypothesis].
- GetPlantedExplosive is 1 from the freak-out time until it goes off.
- Not built: the "Detonates" variant (DetonatePlacedExplosives); the victim's panic idle
  (the masters' only use of GetPlantedExplosive is one condition).

## 6. Checks

Unit tests: `fnv_game::pickpocket::tests::chance_follows_the_wiki`. Smoke:
`pickpocket-steal` (sneaking, E on Goodsprings' Sunny Smiles opens her pocket; one take
is logged as unseen or caught).
