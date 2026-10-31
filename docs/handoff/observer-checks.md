# Observer checks: gameplay guesses the original game can settle

Guesses about how the original behaves that a capture can settle without the owner.
The observer can run scripts, take screenshots, record video, and tap keys, so it can
also drive menus. Sorted by how much of the game each check moves, most first.
Taken from the old owner-review log on 2026-10-24 (full log:
`git show 85dcaace:docs/owner-review.md`); decisions that need the owner are in
[owner-review.md](../owner-review.md).

Tools: [ai-observer](../spec-notes/ai-observer.md) (campaigns, fight field set, events,
timed actions, shots), [xnvse-capture](../spec-notes/xnvse-capture.md) (jobs and short
scripts), [owner-decision-captures](../spec-notes/owner-decision-captures.md) (what
earlier rounds settled). Start from the `nvfstrip` save unless a check needs an early
game state. When a check is settled, write the answer into the linked spec, fix the
engine if needed, and delete the item here.

## What the open checks need (2026-10-31)

- **Unattended, from the saves we have** (a session can run them): 1, 2, the rest of 3,
  5 (weapon health around scripted shots), 6 (nvftest, Sunny), the rest of 8, 11 (72 h at a raised
  TimeScale), 13, 15, 20, 21, 22 (dialogue events), the rest of 33, 35, 36, 52, 53, and
  #1076 (melee reach).
- **A save from the owner** (quest or character state no save has yet):
  - Hardcore on (4; the Hardcore Stimpak in 9): SetHardcore does not work from scripts.
  - Boone, Rex and ED-E hired (19, 22's complaints, 18's wheel).
  - Benny's invitation to the Tops taken (47).
  - Old World Blues just started in the Think Tank (43, 37); X-8 done, EM pulse module
    (38). Honest Hearts at Flight from Zion with Daniel (49).
  - VMS46 at Anders' thanks (39); Stone on his night route, VMS31 (50); HELIOS One before
    the west terminal (46, 25 at Intelligence 1); Oliver on House's path (48); a finished
    game at the end slides (28, 29).
- **A save made inside the capture** (allowed, probe-saves rule): 24 (save half-way along
  a patrol, reload), 41 (the emergency radio, timed).
- **Screenshots or video** (window in front, both locks): 7 (casino menus by key taps),
  12 (Ringo, Nash, Red Lucy, Doc Mitchell: nvftest/nvfdoc have them), 16, 17, 26, 27, 54.

## P1: changes many fights or the whole run

1. **Which weapon NPCs carry and draw.** Ours ranks weapons by base damage plus
   explosion damage, so about 40 NPCs now draw grenade rifles or launchers. Any AddItem
   (even caps) makes an NPC equip its best weapon and armour again. Melee Only / Ranged
   Only styles switch weapons only in combat. To check: read GetEquippedObject 5 on
   those 40 NPCs and on a sample of armed NPCs at first load, and again after AddItem
   and after a combat start. ([npc-behaviour-compare §9.1](../spec-notes/npc-behaviour-compare.md),
   [combat-ai §18](../design/combat-ai.md), [companion-wheel §4](../spec-notes/companion-wheel.md))
2. **Who attacks you, and who joins a fight.** Ours:
   - 618 placed Aggressive (not Very Aggressive) animals ignore you until attacked.
   - Inside an aggro radius, an actor warns twice 5 s apart, then attacks.
   - Actors of a leveled template take the factions of our level-1 entry (192 types).
   - Sharing a faction or base NPC makes actors allies.
   - Onlookers who would be attacked by the attacker join at once.
   - Cowardly actors never help.
   - An escort fights its charge's attackers.
   - Anyone hostile to you is hostile to your teammates, and teammates ignore your hits.
   - People on Find with Allow Killing attack only actors carrying the item (creatures
     hunt anything; already captured).
   To check: run campaigns near coyotes, rats and bighorners at Goodsprings, the Primm
   ambushers, a gecko attack on a settler, an escort, and shots at Boone, with the fight
   field set and OnStartCombat events. ([factions](../spec-notes/factions.md),
   [combat-ai §10, §12, §13](../design/combat-ai.md), [followers](../design/followers.md),
   [packages §3.1](../spec-notes/packages.md))
3. **Damage numbers.** Ours:
   - Power attacks do 3× standing and 2× moving.
   - A damage type uses its resistance in place of DR (capped at 85 %, after DT).
   - Rippers, Chainsaw and Thermic Lance ignore DT/DR.
   - `fCombatPlayerLimbDamageMult` halves damage to the player's limbs.
   - Difficulty multipliers are 2/1.5/1/0.75/0.5 dealt and the inverse taken, including
     limb, burn and poison damage and your own grenades.
   - Your own explosives hurt you.
   - A second burn or poison hit restarts the timer rather than stacking.
   - Every weapon's Crit Effect applies.
   - Fall damage is 0.0252 × (h − 600)^1.65, with no leg crippling — answered
     2026-10-31 (o6_00): the formula holds within 1 %, but each fall also hurts one leg
     (+700: 24.9 of 100, half the Health lost; +1000 and +1300: crippled). Ours never
     does (physics.md §6.6, the falls into and beside the lake).
   To check: OnHit events plus Health and limb values for each case.
   ([combat-rules §10](../spec-notes/combat-rules.md),
   [weapon-data-fields](../spec-notes/weapon-data-fields.md),
   [crippled-limbs](../spec-notes/crippled-limbs.md), [difficulty](../spec-notes/difficulty.md),
   [hit-effects](../spec-notes/hit-effects.md), [physics §6.12](../spec-notes/physics.md))
4. **Hardcore needs and waiting.** Needs a save with Hardcore already on: SetHardcore 1
   from a script or the console left IsHardcore 0 in the original (o6_00-01, 2026-10-31),
   and the needs stay 0 without it. The sleep value's name is `SleepDeprevation`. Ours:
   - Hunger rises 4.8/h and sleep deprivation 2.4/h. Dehydration is guessed at 9.6/h
     (Minor at about 21 h).
   - An hour slept takes 100 off sleep deprivation. Needs rise while asleep.
   - Beer and Nuka-Cola make you thirstier (their "Damage Dehydration" read literally).
   - An hour waited counts as 120 s of effect time.
   - Plain beds don't heal, sleeping heals limbs only outside Hardcore, and waiting is
     refused while you are being irradiated.
   To check: read the need actor values over waits and sleeps, after drinks, and with
   Buffout active. ([aid-items §2, §6, §11](../spec-notes/aid-items.md),
   [wait-menu](../spec-notes/wait-menu.md))
5. **Weapon wear.** Ours:
   - A melee swing wears the weapon only when it lands (once per swing).
   - A record's Damage-to-Weapon Mult replaces 0.2 rather than multiplying it, so the
     Flamer, Incinerator, chainsaw and Rippers now wear 5× faster.
   - NPC weapons wear like the player's.
   To check: weapon health before and after N swings and shots, per weapon.
   ([combat-rules §17–§18](../spec-notes/combat-rules.md))
6. **Detection.** Ours:
   - Light level is ambient + sun + lamps; sunlight counts only where unshaded.
   - FOV is 95° each side, and sneak-running counts as running.
   - A lit flashlight doesn't raise your light.
   - Impacts and explosions alert at their own sound levels, and creature footsteps
     count ×2 Loud / ×0.2 Silent.
   - Lying on a bed counts as asleep.
   To check: GetDetected player while walking up to Sunny at noon and at night, with the
   flashlight on, on a porch, and with a missed shot near a sleeper.
   ([detection §8](../design/detection.md))
7. **Casino payouts.**
   - Slots: ours pays back about 1.35 per chip, so max winnings come fast. Check the
     reading of "= BET × N", the cherry rule and the middle row.
   - Blackjack: the dealer doesn't peek, split aces play on, one split only, surrender
     returns half rounded up, comps come at ¼ / ½ / ¾ of max winnings.
   To check: drive many hands and spins by key taps and read caps and chips.
   ([slots](../spec-notes/slots.md), [blackjack](../spec-notes/blackjack.md))
8. **Package timing.**
   - SetPackDuration 60 switched every 10–70 game minutes in the original. What does
     the number count?
   - A schedule duration with no start hour ends only when the goal is met.
   - Sandbox sit and stand times are our own numbers.
   - An unreachable package is set aside for 1 h.
   - MP A gives up her escort half-way in the original.
   - Random NPC chats start only with you within 300 units.
   - Force greets need a clear line, but facing doesn't matter.
   - A woken sleeper talks within 8 s and stays up 30 game-seconds.
   - People during a wait walk an hour's worth each hour.
   - **A far actor heading for you, past what was captured** (Malcolm Holmes, settled
     in part 2026-10-24: hourly jumps along a route, [ai-observer §13](../spec-notes/ai-observer.md)).
     Answered 2026-10-31 (`holmes_far_long`, 14 h after the player moved): he follows
     the old route to its end (ten more hourly jumps), then heads for where the player
     is now, as ours plans again then.
   To check: observer timelines, which already record packages.
   ([actor-behaviour §3, §5.5–§5.6, §5.9](../design/actor-behaviour.md),
   [owner-decision-captures §7](../spec-notes/owner-decision-captures.md))

## P2: rules met often

9. **Aid, perks and stats.** Ours:
   - Medicine/Food potency is 1 + 2 × the user's skill / 100, and Buffout boosts food
     healing.
   - Addiction: +30 usage per use, chance (usage − 70)/100, and it never wears off.
   - The Hardcore Stimpak heals 5/s for 6 s, and a normal Stimpak heals every limb.
   - A second dose doesn't re-run Start.
   - Only the highest rank of a ranked perk applies, and perks are the player's only.
   - Chemist doubles the Stealth Boy.
   - Skill books give 3 points (4 with Comprehension).
   - Worn bonuses (Vault Jumpsuit Speech +2) are not in actor values.
   - GetBaseActorValue includes ModActorValue — answered 2026-10-31: in the original it
     does not (Strength base 5, ModAV 2: GetBaseAV 5, GetAV 7), and neither does ours
     (`get_base_actor_value`, actor-values.md §1); the note here was stale.
   - Calm Heart's +50 Health goes to every teammate.
   - Four Eyes ignores "PER > 1".
   To check: read actor values in scripts around each use.
   ([aid-items](../spec-notes/aid-items.md), [levelup §9](../spec-notes/levelup.md),
   [skill-books](../spec-notes/skill-books.md))
10. **Karma, reputation and crime.** Ours:
    - Killing Very Evil gives +100, Good −100, and murdering a Neutral −25 — partly
      answered 2026-10-31 (o6_02): −500 victims gave +100 and a +900 one −100 (ours gives
      0 for −500, "Evil"); a 0 victim by KillActor 0. The 2026-10-19 convict (−500 only
      through a levelled template) gave 0, so the reward may read the base's own karma
      (crime-karma.md §4.1). Still open: a real murder of a Neutral.
    - Each owned take or lock is −5.
    - A seen theft is +2 infamy and any seen kill +30, including hostile Powder Gangers.
    - An owner who sees a theft attacks at once.
    - Trespass: two warnings 5 s apart, then attack.
    - Crime fights can't be yielded.
    - Templated NPCs take their template's karma.
    - Unclamped AddReputationExact; 15/50/100 % levels for the eight reputations not
      captured.
    To check: read GetAV Karma (already in the header) and GetReputation around each act.
    ([crime-karma](../spec-notes/crime-karma.md))
11. **Vendors and loot.** Ours:
    - Vendors sell only their flagged categories and their vendor chest, but buy
      anything.
    - Restock follows Ringo's days for everyone.
    - Death items are rolled at death; the template's list wins.
    - Spent casings are player only, doubled by Hand Loader.
    - Respawn-flagged corpses return after 72 h at full health.
    - An encounter zone's level is fixed when its cell enters the loaded area (Easy to
      Boss is 50/75/125/200 %).
    - The three placed weapons that name their ammo give it once.
    To check: barter screenshots, inventory dumps, and timelines over 72 h.
    ([vendor-services](../spec-notes/vendor-services.md),
    [death-items](../spec-notes/death-items.md), [encounter-zones](../spec-notes/encounter-zones.md),
    [dead-on-spawn §4](../spec-notes/dead-on-spawn.md))
12. **Dialogue rules.** Ours:
    - Greetings flagged Goodbye are said at you; when an actor starts the conversation
      they hold you for one line.
    - A generic Goodbye is added where the only Goodbye has an end script (100+ lists).
    - Tab leaves only where a Goodbye is offered.
    - A skipped line still runs its end script.
    - Spent Say Once questions vanish.
    - No pause is added between voiced lines (scenes run 0.3–0.4 s short per line).
    - Shared (Info Connection) lines count as the topic's own.
    - Corner and quest messages wait until the conversation ends.
    - Voices without Allow Default Dialog get no generic lines.
    To check: screenshots and video of Ringo, Nash, Red Lucy and Doc Mitchell's stage 40.
    ([dialogue-play](../design/dialogue-play.md), [dialogue](../spec-notes/dialogue.md),
    [voice](../spec-notes/voice.md))
13. **Speeds and turning.** Settled (actors §8.10): people's walk and run with their
    armour and two-handed weapons, and creatures' walks and runs (clip pace × scale ×
    Speed Mult × their skeleton's share). Still open, ours:
    - People turn ≤180°/s walking, 120°/s standing; creatures 300°/s × scale (measured
      on cazadors only); the CREA Turning Speed field is unused.
    - Speed Mult on people: one actor (Cook-Cook, 110) ran as if it were 100 (asked in
      owner-review.md).
    - People walking with a weapon drawn (77–78 seen outside combat, about 100 in
      combat) are not built.
    - Follow distances: stand within distance + 64, run beyond 512.
    - Aiming or blocking slows you to a walk.
    - Swimming starts at 115 units deep.
    - The step-up is 24 units with a 45° slope limit.
    To check: the fight field set at every loop gives positions and headings; flat open
    ground and a cleared area: the `nvfairport` save (campaign `creature_runs`).
    ([actors §8.9–§8.10](../spec-notes/actors.md), [physics §6.4.2](../spec-notes/physics.md))
14. **Script rules that differ from the wiki.**
    - A call on an empty ref is skipped — partly answered 2026-10-31 (o5_00): in a
      function script run through xNVSE's CallFunction, `rE.GetDisabled` on an unset
      `ref`, alone or as an `if` condition, let the script go on (the next line ran). Ours
      stops the script (game.rs `stop_run`, scripts.md §4.2). Still open: the same in a
      quest or object script run by the game itself, before ours changes.
    - Return ends one block only.
    - Scripted items in other inventories don't run.
    - Message boxes don't freeze the player, and MenuMode 1002 is our guess.
    - The GetWeaponAnimType table (Sunny's tutorial depends on it).
    - tgm lifts carry weight and keeps magazines full.
    - Scripted UseWeapon runs before the combat AI; melee UseWeapon walks up to the
      target.
    - Quest items can be stored in containers.
    To check: short-script jobs for each.
    ([script-language](../spec-notes/script-language.md), [scripts §10](../spec-notes/scripts.md),
    [use-weapon](../design/use-weapon.md))
15. **Survival hazards.**
    - Environment radiation: strongest marker wins, and water gives 1 rad/s (2 if
      radioactive).
    - The Fat Man zone fades from 10 to 0 rads/s over 12 s within 616 units.
    - Turbo slows the world to 0.5.
    - A crippled head means a 40 s concussion.
    - A crippled arm makes an NPC drop its weapon.
    - Knockdowns last 3 s.
    - Only BMDT Power Armor needs training.
    - Traps give 2 XP (your note said 5).
    To check: actor values and events.
    ([radiation](../spec-notes/radiation.md), [magic-archetypes](../spec-notes/magic-archetypes.md),
    [unconscious §5.4](../spec-notes/unconscious.md), [traps](../spec-notes/traps.md))

## P3: smaller or rarer

16. **HUD and menus.**
    - The enemy health bar and red crosshair show only for hostiles, or for anyone hit in
      the last 5 s.
    - Sneak-indicator states.
    - Compass ticks within PER × 300; markers within PER × 2048.
    - "How many?" asks only above 5 (barter) or above 1 (crafting).
    - The crafting filter arrows.
    - Wait pace: 1 hour per 0.5 s, and it can be cancelled.
    - Wait refusals: within 2,048 units outdoors, 1,024 indoors.
    - Hacking word lengths by difficulty; a bracket resets attempts 1 in 5.
    - Holotapes keep playing with the Pip-Boy closed.
    - A second ShowTutorialMenu adds a page.
    - Aid effects are shown as totals.
    - The order of misc stats after #16, and what Barter Amount Traded counts.
    - Caravan currencies: caps first, then the least valued notes.
    - Cannibal and Sandman show a choice box.
    Screenshots. ([hud §8–§10](../spec-notes/hud.md), [wait-menu §9](../spec-notes/wait-menu.md),
    [hacking](../spec-notes/hacking.md), [misc-stats](../spec-notes/misc-stats.md))
17. **Weapons and bodies, by video.**
    - Minigun spin-up on every press and after a reload.
    - The Grenade Launcher reloads round by round.
    - Bodies go limp at once, with no death clip.
    - Shove speeds: shots 140, blows 220, bottles about 3 m/s.
    - Corpses: the player walks through them.
    - Small creatures block the player.
    - Bodies over 250 mass can't be pushed.
    - Loose items rest until touched.
    - Pushed NPCs move at half speed and return after 2 s (the `pc_actor_push` capture is
      ready).
    ([animation §9.8](../spec-notes/animation.md), [ragdoll §4](../spec-notes/ragdoll.md),
    [physics §4.3.1, §6.8, §6.10–§6.11](../spec-notes/physics.md))
18. **NPC odds and ends.**
    - Doors stay open behind NPCs.
    - No ledge links.
    - Locked doors are closed to NPCs unless they carry the key.
    - Idle conditions ignored for PlayIdle.
    - Sitting is instant for the player and holsters the weapon.
    - Followers arrive 160 units to your left (captured) and every teammate follows
      through doors.
    - Package-less actors walk home after a fight.
    - Creatures with Can't Open Doors never use load doors (the Searchlight ghouls
      following Edwards).
    - The companion-wheel details.
    - Challenges count every kill.
    - Quest targets indoors are marked at one door.
    ([navmesh §7](../design/navmesh.md), [idles](../spec-notes/idles.md),
    [furniture §5, §7](../spec-notes/furniture.md), [packages §7](../spec-notes/packages.md),
    [challenges](../spec-notes/challenges.md))

19. **The companion perks' highlights.** Ours: Spotter (Boone) and Search and Mark
    (Rex) glow only while the player aims with a weapon out; Spotter marks the actors
    the compass shows red within its range; Search and Mark marks chems, Stimpaks, guns,
    energy weapons and ammunition within 1,500 units; Enhanced Sensors (ED-E) sets the
    compass's actor range to Perception 10's. To check: with each companion hired, a
    frame before and while aiming near hostiles and near dropped items (a Stimpak, a
    pistol, a melee weapon, food) at a few distances; the compass with ED-E at low
    Perception. ([perk-entry-points §13](../spec-notes/perk-entry-points.md))

## Added 2026-10-25 (from the ESM-gap work)

The numbers above stay put so links to them hold; each item here gives its own priority.

20. **NPC grenades** (P1). Ours: the first throw comes 4–10 s after the NPC sees its
    target, then one every 12–25 s, only between 1.5× the blast radius and the throw's
    reach (at most 1,500 units), and each throw takes one from the inventory.
    To check:
    - When do NPCs throw grenades or dynamite: the distance to the target, how often,
      and how soon into the fight?
    - Do they visibly switch to the grenade and back?
    - Does their count drop?
    Setup: Powder Gangers with dynamite at NCRCF, or a Legion squad.
    ([combat-ai §20](../design/combat-ai.md))
21. **Packages that send NPCs through player-only load doors** (P2). Some door
    scripts' OnActivate blocks call Activate for the player only. `fnvtool ai reach`
    (routes_through_door) still finds FalloutNV.esm packages routed through them:
    - the Strip gates (VStripGate, refs 0016A14D–0016A150), about 420 actor/package
      pairs, among them the securitrons' VStripSecuritronGuardGate*/Patrol*, the NCR
      MPs' VStripMPBPatrolEmb and the Omerta thugs' VMS33ThugLeavePackage;
    - Gomorrah's elevator doors (0010DA76, 0010D6AC, 0010D6AE, 0010DDB4), 15 pairs, e.g.
      VMS21NeroCasinoTravel and VMS21BigSalHomeTravel.

    Ours lets an NPC through a load door whatever the block did (the block still runs)
    and refuses only at doors that open in place.
    To check: does the original move these NPCs through those doors? Does a Strip
    securitron's gate patrol cross the gate? Do Nero and Big Sal go between their rooms
    and the casino floor through the elevator, or never get there?
    ([doors §5.2](../spec-notes/doors.md))
22. **Combat answer lines and companion complaints** (P2). To check:
    - When a group-mate says NormalToAlert or NormalToCombat, does a nearby mate answer
      with NormalToAlertResponse or StartCombatResponse?
    - Are AttackResponse (BoS: "On it!") and FleeResponse (super mutants: "Coward!")
      said after Attack and Flee lines?
    - Does DeathResponse play without JIP? The wiki says it doesn't.
    - When does a companion say WeaponIneffective or ArmorIneffective? Ours says:
      - WeaponIneffective when DT cut the hit to its floor;
      - ArmorIneffective when one hit took 25 % of health with DT stopping 10 % or less;
      - each at most once a minute.

      Test: Boone with a 9mm against a high-DT target, and Boone in light armour taking
      a heavy hit.
    Read dialogue events in the observer timeline, with audio or subtitles.
    ([combat-dialogue §5](../spec-notes/combat-dialogue.md))
23. **Actor Effect spells in NPC and creature spell lists** — answered 2026-10-30 (issue
    #916): the original does not apply the type-0 entries (Tunnelers' DT, Ruthless,
    SporeCarrierRegen read IsSpellTarget 0 and base values; the Ability beside them 1), as
    ours. ([area-effects §3](../spec-notes/area-effects.md))
24. **A patrol after a load** (P3). Ours: an NPC on a non-repeating Patrol package
    goes on to its next point after a reload.
    To check: save while it is half-way along its route, reload, and see whether it
    goes on or starts again from the first point.
    ([save-load §5 item 4](../design/save-load.md))
25. **Dialogue at low Intelligence** (P3; few players run Intelligence 3 or below).
    Ours (PR #743), with the player's Intelligence at or below
    iDialogueDummySpeakThisIntOrBelow (3 in FalloutNV.esm):
    - a topic's Dumb Response (DIAL TDUM) replaces the player's line, e.g. "Pizza." for
      "Peace." in Helios One's ARCHIMEDES conversation;
    - INFO lines flagged Low Intelligence (acting note "{Dummy}") replace their High
      Intelligence twins ("{Non-Dummy}").

    To check: `player.setav intelligence 1`, then screenshots of those conversations.
    Check Intelligence 4 too: is the player dim there?
    ([dialogue §2](../spec-notes/dialogue.md))

## Added 2026-10-26 (UI tile tree)

26. **How the original's menu engine reads its files** (P2: every UI mod and MCM).
    Ours (fnv-ui):
    - `&true;` is 1 and `&false;` 0;
    - other constants are kept by name;
    - an end tag closes the innermost element of its name, else the innermost element;
    - a value a script set holds until what its expression reads changes;
    - SetUIFloat makes no trait that is not there;
    - `name:N` counts from 0 among the children of that name.

    To check: GetUIFloat on traits the files set to constants (`HUDMainMenu/locus`,
    `_CrosshairVisible`, a `justify`), DumpTileInfo on QuestAddedMenu and on MCM's
    StartMenu tree, SetUIFloat on a trait with an expression and on a missing one, and
    SetUIFloat against SetUIFloatAlt on a templated list row (the "children tiles" bug
    MCM depends on). ([menu-xml §8](../spec-notes/menu-xml.md))

## Added 2026-10-27 (first-person depth)

27. **Does the original's first-person weapon go into walls?** (P3: one look in every
    close fight indoors.) Ours draws the arms and weapon in front of the world whatever
    is nearer (first-person-depth.md §1, [hypothesis] that the original does the same).
    To check: in the Prospector Saloon, a varmint rifle drawn nose to the east wall
    (the `first-person-depth` smoke spot, feet at 469, 244, 3457 facing 80°), a screenshot
    hip-fired and one at the sights; then the same against a standing NPC. If the barrel
    is cut by the wall there, the original draws it with the world.
    ([first-person-depth](../design/first-person-depth.md))

## Added 2026-10-27 (main-quest walkthroughs)

28. **Does a SayTo that finds no line still end with SayToDone?** (P1: every ending.)
    Ours now runs the speaker's SayToDone blocks for the topic in the next frame
    [hypothesis]: the narrator's end-slide chain (SCPT 0x00161EB4) calls topics that have
    no line for some endings (VEndingBrotherhood for the NCR ending with the Brotherhood
    never met) and must still reach VEndingConclusion. To check: a query script that
    SayTos a topic whose lines all fail, and a SayToDone block that sets a variable; read
    it one and ten frames later. ([mq-walkthrough](mq-walkthrough-2026-10-27.md) §2)
29. **Does a line's fixed speaker (INFO ANAM) gate who says it?** (P2.) 262 lines name a
    fixed speaker other than the actor of their own GetIsID condition (167 end slides:
    a Boomer, Nolan McNamara ... as speaker, GetIsID Narrator). Ours now lets that
    condition decide when the line has one [hypothesis]. To check: the end slides of a
    finished game say the Boomers' slide; for a non-ending case, talk to Victor at the
    Lucky 38 door (0x00165102, speaker the entrance Victor, GetIsID the elevator).
    ([mq-walkthrough](mq-walkthrough-2026-10-27.md) §2)
30. **Reputation axes outside 0-2** — answered 2026-10-31 (o5_00-02, RepNVNCR, scale 80):
    GetReputation axes 2 and 3 read 0; GetReputationThreshold axis 3 read 0 at every value
    tried (so Benny's "axis 3 >= 3" greeting is never chosen in the original either), and
    axes 0-2 matched ours (`reputation::tiers`) at all ten infamy/fame pairs. SetReputation
    with axis 2 or 3 is a failed call that **stops the script** (the line after it did not
    run); ours does nothing and goes on (reputation.rs `axis`). (scripts.md §7.6)
31. **Say Once on a line two actors share** — answered 2026-10-31 (owner, save
    nvf31mainframe, issue #1113): **no repeat**. After the upload and the Mark II upgrade
    the Yes Man mainframe greeted with 0x001164F2 (GREETING position 2424, conditions
    GetIsID YesManMainframe OR VMQTopsYesMan, VMQTops variable 32 < 1), not the Tops
    first-meeting greeting 0x001164F1 (position 2398, Say Once, only the same GetIsID OR)
    that the Tops Yes Man had said [observed]. So Say Once holds per line across speakers
    in the original [hypothesis: one case; the mainframe and the Tops Yes Man may also
    count as one speaker]. Ours keeps it per (speaker, line) (dialogue.rs `said`) and
    would say 0x001164F1 again. (dialogue.md question 13)
32. **An expression that leaves extra values** — answered 2026-10-30 (issue #908): the
    original stops the script at `if GetStage Q N == 1` and runs neither arm, as ours.
    ([scripts §4.2](../spec-notes/scripts.md))

## Added 2026-10-29 (radiation falloff, issue #894; HUD states capture)

33. **Radiation falloff and Rad Resistance** — answered 2026-10-31 (o6_00): linear in
    the 3-D distance (346 units 11.412178, 602 units 9.647502, as ours to 1e-5) and
    GetRadiationLevel × (1 − Rad Resistance ÷ 100) (Rad-X: 6.291849, as ours).
    (radiation.md §2, §4) Still open (P3): overlapping markers (the Camp Searchlight
    crater's three, 1000 units from their point: ours 8.28, summed 20.09) and a Fat Man's
    zone at its centre and 300 units out. MoveTo a marker with offsets moves the player
    where SetPos did not; at two of the four points it left the player in place
    [hypothesis: points it cannot stand the player on].
34. **The corner bars' ticks' brightness** — answered 2026-10-31 (o5_00-01): the full-row
    meters read `brightness` 175 in both bars (`HUDMainMenu/HitPoints/*:2` and
    `ActionPoints/*:2`, each 296 wide and visible; the bars' other visible children 255),
    as the tile HUD writes. Read with a dialogue open over the HUD (the nvfstrip save loads
    into one). (hud.md §14.9)

## Added 2026-10-30 (issues #907, #915)

35. **A condition with a null form parameter** (GetQuestVariable on INFO 0x0014B595-7,
    GetIsID on 0x000FF5BA) — answered 2026-10-30 (issue #907): the original reads 0 for
    both; ours now does without a failed call. Open: other functions given a null form
    in a condition (ours: a failed call, also 0). ([dialogue §4](../spec-notes/dialogue.md))
36. **Which of a creature's turn rates applies when** (P3; issue #915). Steady turns are
    TNAM × 1.25, × 3 or × 0.075 °/s on every skeleton tried, whatever the base scale
    (actors.md §8.10 "TNAM"); × 0.075 is aim tracking within about 6°, but × 1.25 and × 3
    both came on big turns towards a target. To check: GetAnimAction, the movement
    speed, IsInCombat and the combat target's distance against the rate, with more turns
    per creature (e.g. a mongrel set on the player again and again from behind), and
    whether a standing turn without combat (a Travel package to a marker behind it)
    takes × 1.25 or × 3. ([actors §8.10](../spec-notes/actors.md))

## Added 2026-10-30 (add-on walkthroughs, issue #918)

37. **Is a line that another line names as its follow-up (TCFU) said when its topic is
    chosen?** (P2: Old World Blues' Sink.) Six topics of the ten masters store a
    follow-up line first and another line after it, e.g. the Think Tank's "Topic001"
    (`NVDLC03DialogueKleinNVDLC03DoctorOLVLTopic001`): 0x01013555 (the five-line
    technologies chain that Topic000's line 0x0101432E leads into) and 0x01013559
    ("FINE. SO... YES."), whose choice Topic002 is the only way to the Sink's chip.
    Ours now passes over follow-up lines of a topic the player chooses while another line
    of it can be said (greetings and SayTo not: [dialogue §3.4](../spec-notes/dialogue.md))
    [hypothesis]. To check: in the first
    Think Tank talk, after agreeing to help, pick the technologies question and see which
    line answers, and whether "a place to stay" is offered next. The others: Doc
    Mitchell's VCG01DocMitchellTopic076 (never chosen), Mr. House's MrHouseTopic154,
    VMS46 Diane's VRRCDianeTopic003, the Think Tank's OLVLTopic006 (Dala's 0x01011333 now
    leads into Klein's 0x01011335) and Dr. O's OLVLTopic036.

38. **Does a block whose parameter is a form list run for any member?** (P3: Old World
    Blues' force fields.) `Begin OnHitWith NVDLC03SonicWeaponList` (SCPT 0x01016AB4,
    0x01016953, 0x0101571D) names a FLST of the five Sonic Emitters; ours runs the block
    when the hitting weapon is in the list (game.rs `block_accepts`) [hypothesis, fit to
    the data]. To check: with the EM pulse module downloaded (X-8), shoot a Big MT force
    field with the Sonic Emitter: Sonic Emitter Upgrade completes and the field drops.

39. **Can a Goodbye line's end script start a new conversation?** (P2: 8 lines, e.g. Anders'
    thanks in VMS46, INFO 0x00142A9C.) Ours refuses StartConversation while the old
    conversation is still open. In the original, after the line, does the new speaker
    force-greet? (mq-walkthrough §9 stall 1.)
40. **Does re-displaying a completed objective un-complete it?** — answered 2026-10-31
    (o5_00, issue #1108): no. VMS19 65 and VFSAtomicPimp 34: displayed, completed, then
    SetObjectiveDisplayed 1 again: still completed. Only showing a **hidden** objective
    clears the flag (displayed 0, then 1: not completed), which is what the VMS16 capture
    of layer5-captures.md §5.2 saw. Ours clears it on every display, so G.I. Blues can
    stall. (scripts.md §5)
41. **Does ResetQuest keep a SetQuestDelay?** (P3.) Use the NCR emergency radio and close
    the Pip-Boy at once: does the supply drop still come? (§9 stall 3.) No script function
    reads a quest's delay, so this needs a timed watch of the quest's effect.
42. **Does a stage result script's reference-less RemoveSpell act on the player?** —
    answered 2026-10-31 (o5_03 on nvfairport, issue #1110): yes. Addicted to Jet (10 doses;
    IsSpellTarget WithdrawalJet 1 over a game hour), `SetStage VDoctors 20` made it 0 within
    10 frames. The doctors' cure works in the original; ours does nothing.
    (script-language.md §1.2) Traps for the next capture: the first addiction opens a
    message box (menu 1001) and the nvfstrip save loads into a dialogue (menu 1009); both
    pause the game, and effects do not change while it is paused.
43. **Does a newly started quest's script run before its first delay?** (P2.) Old World
    Blues: wake in the Think Tank and talk to Dr. Klein at once. Do the Big Empty's random
    encounters (NVDLC03Enc 10) start? (§10 stall 2.)
44. **What does `RemoveItem <item> 0` do?** — answered 2026-10-31 (o5_00, issue #1113):
    nothing, and the script goes on (12 Stimpaks and 21 caps stayed; the next line ran).
    Ours refuses the call (#1113).
45. **Does a second Nelson explorer stop the player right after the first?** (P3; §10
    stall 4.)
46. **Does the HELIOS One west-terminal password go to the player from Fantastic's line?**
    (P3; §10 stall 5.)
47. **Does a talking activator's scripted `Activate player` start its conversation?** (P1:
    Ring-a-Ding-Ding's ambush at the Tops, and the REPCONN intercom, #906.) Take Benny's
    cash-up-front invitation and enter the presidential suite: does the intercom start
    Benny's conversation by itself about 15 s later? (§11 stall 1.)
48. **Can Oliver be talked into retreating on House's path?** (P3.) After "You will because
    the alternative is worse", which reply does he give (Topic065 has two lines with the same
    conditions, 0x00159932 and 0x001596BD)? (§11 stall 2.)
49. **Does attacking Daniel during Flight from Zion start Chaos in Zion at once?** (P3; a hit
    starts no AI combat headless.) Also: attacked at the Northern Passage, does Jed go down
    unconscious, and does Happy Trails fail? (§11 stall 3.)
50. **Does walking behind Stone on his night route set VMS31 60 as soon as he is in view?**
    (P3; no line of sight headless.) (§11 stall 3.)
51. **Does the script language short-circuit `&&`?** — answered 2026-10-31 (o5_01, issue
    #1112): no. `if 0 && player.ModAV Variable09 10`, `if 1 || …`, `if 0 == 1 && … == 0`,
    `if 0 && 1 && …` and four more forms all ran the ModAV (+10 each time). EDEScript's
    empty-reference call each frame is the original's too; check 14 asks whether it stops
    the script there. (script-language.md §4)

## Added 2026-10-30 (the enemy bar's show rule, issue #935)

52. **How long a neutral's enemy bar lingers, and what ends it** (P3: every hurt actor
    looked at.) The hurt settler's bar was still up 16-18 s after the player turned away
    in hud_enemy5 a1-a3a, where the watch ended; a restrained hostile gecko's went after
    about 5.5 s (b2). Ours lingers 5.5 s for every actor (`hud_target::LINGER_SECONDS`):
    a known divergence for a neutral. To check: hurt a neutral settler by script (DamageAV
    Health), turn the player onto him from 350 units (not the activation target, no
    prompt), then 180 degrees away, and read `EnemyHealth`'s three children every 50 ms
    for 40-60 s with no MoveTo; then the same on a hurt restrained gecko, and on each
    after showing the bar 1 s against 10 s before the turn (hostility or the time shown
    are the candidates). ([hud §8](../spec-notes/hud.md))
53. **Does the player's own hit show the enemy bar, and for how long?** (P2: every fight
    the player starts.) No capture had the weapon out (IsWeaponOut read 0 after the
    script's Ready taps), so no shot hit. Ours keeps its earlier guess: 5 s after the
    player hits an actor it has the bar, aimed at or not and prompt or not, while it is
    hurt (`hud_target::HIT_SECONDS`) [hypothesis]. To check: with a weapon drawn (EquipItem
    and a Ready tap confirmed by IsWeaponOut 1), shoot a neutral settler once from 350
    units with the crosshair on him, turn away at once, and read the three children every
    50 ms for 20 s; then the same with the crosshair off him at the moment of the hit (a
    shot from a script, or a grenade), and at 120 units where he is the activation target.
    ([hud §8](../spec-notes/hud.md))

## Added 2026-10-30 (UI tile tree T8: the pointer and keys)

54. **The pointer on menu tiles** (P2: every menu drawn from its file, MCM's page, 16
    plugins of the modlist scan). Ours (fnv-ui `input`):
    - where two targets overlap, the highest summed `depth` takes the pointer (a tie to
      the one drawn later), and a menu opened later is over an earlier one;
    - `clicked` is 1 for the frame of the press, then 0 (the tutorial), not a count (the
      Clicked page);
    - `mouseover` is written on the tile taking the event only, not on the tiles above;
    - GetActiveUIComponentName/FullName/ID with no menu open give "", "" and -1.

    To check: GetActiveUIComponentName over an overlap of two tiles injected with
    InjectUIComponent at different depths; two injected tiles, one copying `clicked` and
    one adding it to itself, clicked twice with GetUIFloat after each click; GetUIFloat on
    a text_box button's hotrect, text and parent `mouseover` with the cursor on the text;
    the three functions in game mode and over a target without an `<id>`.
    ([menu-xml §3.2, §8 items 7-10](../spec-notes/menu-xml.md))

## Added 2026-10-31 (the Pip-Boy Glove and outfit gloves, issue #1067)

55. **Which left hand the player shows in an outfit with gloves** (P3: the player's own
    hand in every outfit with glove addons). NVForever lets the Pip-Boy Glove give way to
    an outfit's glove addon (leather, metal, raider, ranger armour, power armour) and a
    vault suit's bare-hand addon give way to the Pip-Boy Glove. To check: the player in
    Leather Armor and in T-45d Power Armor, first and third person: is the left hand the
    outfit's glove or the Pip-Boy Glove's hand? ([actors §2.3](../spec-notes/actors.md))
