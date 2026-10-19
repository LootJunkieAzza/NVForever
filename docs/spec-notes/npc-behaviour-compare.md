# Spec notes: NPC behaviour, the original against our engine (2026-10-17)

What the AI observer (ai-observer.md §9–10) measured in the **original** game, the same
scenes in **our engine** (smoke-style launches with the engine's own log lines), and what
differs. Numbers only; no game text. Every original number is [observed 2026-10-17] from
private captures (`C:\fnv-private\capture`, never committed), analysed with
`tools/capture/observe_map.py`; engine numbers are [observed 2026-10-17] from release
builds of this branch (`nvforever --play`, logs private). Owner's two complaints: NPCs
speak combat lines and chatter far too often, and melee enemies come so close they clip
into their target. Both are confirmed; §2 and §3 give the fixes made on this branch.

Distances are 2D, centre to centre (GetPos: an actor's reference point at its feet), in
game units. Real seconds come from game time at TimeScale 30 (a game minute is two real
seconds).

## 1. Scenes

| Scene | Original (campaign, save) | Ours |
|---|---|---|
| Melee against a still player | `melee_spacing` (nvftest): near Goodsprings where Violet's dogs roam; the two dogs, then an NCR trooper (leveled list) given fists, a knife, a machete, a lead pipe, a sledgehammer or a power fist, then one creature of each of ten kinds, each spawned 700 units away and set on the player (god mode); 30 real s each, every game loop | `cmp-dogs`, `cmp-trooper-*`, `cmp-creature-*`: the same spot (`--pos=-45600,65000`), god mode, `combat:` / `moveto:` / `call:` test aids; 30 s each |
| NCR troopers against Fiends | `npc_battles` session `ncr_fiends` (nvftest), twice: the six NCR troopers of the smoke scenarios moved 600 units north of the player south of Cook-Cook's camp, 120 real s, every 2 game loops; the player in god mode, crouched, Sneak 100, under a Stealth Boy | `battle-ncr-fiends` (`cmp-battle`): the same troopers and offsets, the player unseen (`NVF_PLAY_TEST_UNSEEN`), 120 s |
| Other fights | `npc_battles`: NCR troopers against Legion melee raiders, coyotes against troopers, a three-party fight with giant mantises (spawned) | not run (no spawn aid for our engine's fights; §8) |
| The Goodsprings gunfight | `goodsprings_fight_fast` (owner's save nvfCombatTown), 88 real s, every game loop | no comparable scene (needs the quest state) |
| Conversations | `conversations` (nvftest): the player standing at the arrival marker in Goodsprings (10 game h), Freeside (8), the Strip (2, cut off), Westside (6), Camp McCarran (8), Primm (6), Novac (6), every 30 game loops | `cmp-goodsprings-day` (5 real min from 08:00), `cmp-saloon-evening` (4 real min from 18:00); then Westside and Novac for 3 game hours each from 10:00, from the original's spots and next to the NPCs, old and new rule (§5) |
| Casinos | `casinos` session `gomorrah` only (the owner stopped the other five, 2026-10-17): Gomorrah's main floor for 24 game hours from 12:00, every 60 game loops | none (§6) |

Arrival by `coc` from the early-game save leaves NPCs that a trigger enables disabled
(the owner saw the Strip's crowd missing); the Strip was queued again from the owner's save
`nvfstrip` (campaign `strip`), not yet run.

## 2. Combat lines

| | Original | Ours (before this branch) |
|---|---|---|
| NCR troopers against Fiends | 12 fighters; 7 and 6 combat lines in two runs of 120 s (the fight still going at the end) | 11 fighters; 47 combat lines (26 Hit, 15 Attack, 5 Death, 1 DeathResponse) and 7 Detection barks in a fight over in 32 s |
| Lines per fighter-minute | 0.63 over all NPC battles (13 lines, 1,242 fighter-seconds); 0.31 in the Goodsprings gunfight (4 lines from 18 fighters in 88 s) | about 8 (47 lines, about 350 fighter-seconds) |
| Gap between one fighter's lines | median 20 s (gunfight), 2–3 s for the rare pair in the battles | median 3.2 s, least 0.3 s |
| Fighters speaking at once | 0 for 88–96 % of fight time, 1 for the rest; 2 at once only in the Legion fight (8 % of it) | not measured (two or more often: 54 lines in 32 s) |
| Hits followed by a line | about 1 %: at most 2 Hit-like lines for 217 hits (Health falling by more than 2) | 26 Hit lines for 110 hits (24 %) |
| Attacks followed by a line | about 3 Attack-like lines for some 175 attacks begun (GetAnimAction 2 or 3) | 15 Attack lines (a 5–30 s timer per actor, no chance) |

The topic of a line cannot be read with base-game or xNVSE functions; "Hit-like" and
"Attack-like" are the map step's guess from what happened in the 1.5 s before it
[hypothesis, ai-observer.md §10].

**Fix (this branch, `fnv_game::combat_lines`)**: an NPC or creature says Hit only when the
damage is over `fCombatSpeakHitThreshold` (0.01) **and** a random number is below
`fCombatSpeakHitChance` (0.01) (before: or); and Attack, PowerAttack and FireExplosive only
when a random number is below `fCombatSpeakAttackChance` (0.01, in FalloutNV.esm, unread
before) on top of the 5–30 s timer. The player keeps the old rules (the original player
is silent; its lines are our design, and the `combat-lines` smoke scenario checks them).
Both are [hypothesis fitted to the counts above]: the Combat Tab page names the settings
without saying how they combine. Death, DeathResponse, Crippled and the health marks are
unchanged (each is said once per event, and they were 6 of our 47).

## 3. Melee spacing

Original, a swing being GetAnimAction entering 2 or 3:

| Attacker (target) | Swings | Distance when the swing starts: median (least) | Closest during the swing: median (least) |
|---|---|---|---|
| Violet's dogs, CrNVDogFiend (player) | 33 | 133 (95) | 119 (87) |
| Violetta, her own dog (player) | 9 | 122 (100) | 99 (98) |
| NCR trooper, knife (player) | 17 | 96 (89) | 92 (41) |
| NCR trooper, machete (player) | 13 | 95 (56) | 91 (45) |
| Coyotes (NCR troopers) | 20 | 109 (83) | 83 (81) |
| Giant mantises (troopers, raiders) | 57 | 163 (133) | 161 (113) |
| Giant rat (player) | 2 | 84 (79) | 81 (73) |
| Legion melee raiders, long weapons (troopers) | 4 | 222 (155) | 93 (45) |

Between swings the attackers mostly stood: the median back-off between two swings was
0–26 units. A quarter (dogs, coyotes) to three quarters (mantises) of swings followed the
previous one within 0.6 s (combos).

Not explained: against the still player in god mode, the trooper with a lead pipe, a
sledgehammer or a power fist, and most spawned creatures (dog, coyote, mole rat, mantis,
radscorpion, cazador, young deathclaw, ghoul), stopped 147–177 units away and never swung
in 30 s; with the player's Health raised to 100,000 instead of god mode, every attacker
kept 280–500 units off. The same creature kinds swung normally at NPCs (the mantises
above). [hypothesis: the AI weighs its target's health or invulnerability; follow-up §8].

Ours, before this branch (`npc melee spacing` log line, added here): blows began a median
14–36 units from the target in every scene (dogs 14, troopers 24–27, the battle's Fiends
36) and the closest a melee fighter came was 0–1: attackers stood inside their target.
Cause: with both bodies known, an attacker stopped at a body gap of 6 units and struck
within 24 (`MELEE_STOP_GAP`, `MELEE_HIT_GAP`), about 50 units centre to centre for two
people, whatever its weapon, and the navmesh walkers' push-apart (18 units each) allowed
less.

**Fix (this branch, `npc_combat::melee_spacing`)**: an attacker with bodies to measure
also keeps its reach: it stops at 0.9 × reach plus half the two bodies' radii and strikes
within reach plus half the radii, never nearer than the old gaps. Reach is
`fCombatDistance` 150 × the weapon's Reach × scale (knife and machete 0.5: 75), so two
people with a knife stop at 89.5 and strike within 97, against the original's 96 at the
swing and 92 closest [hypothesis: the rule is fitted to the knife and machete rows; the
creatures' rows are larger than their reach (a dog's 58) gives, follow-up §8]. Results
after the fix: §7.

**Creatures (`npc_combat::creature_spacing`, follow-up §8.2)**: the creatures' rows set
against their records (`fnvtool esm decode`, the template's Attack Reach RNAM and Base
Scale BNAM as our engine reads them) [observed]:

| Creature | RNAM × scale | Original: swing starts, median | RNAM × scale + 59 × scale |
|---|---|---|---|
| Violet's dogs (CrNVDogFiend, template CrDogVicious) | 48 × 1.2 = 58 | 133 | 128 |
| Violetta (scale 1.0 on her record, 1.2 on the template) | 48–58 | 122 | 107–128 |
| Coyote (VCrTier1CoyoteMed, CrCoyote) | 48 × 1.0 = 48 | 109 | 107 |
| Giant mantis (VCrTier2GiantMantisMed) | 100 × 1.0 = 100 | 163 | 159 |
| Giant rat (VCrTier1GiantRatMed, 2 swings) | 25 × 1.1 = 28 | 84 | 92 |

A creature's blows reach a near-constant 55–75 units beyond its RNAM, more for the larger
dog: (RNAM + 59) × scale fits the four kinds within 10 units [hypothesis: the 59 stands
for the creature's body ahead of its centre and the target's; the records cannot say,
the dogs' and coyotes' Object Bounds being 0, so a creature far longer or shorter than
these (a deathclaw, a radroach) is not covered by the rows]. Ours now keeps that distance
centre to centre: it stops at 0.9 of it and bites within it, never nearer than the two
bodies' gaps.

## 4. Encounters

| | Original | Ours |
|---|---|---|
| NCR troopers against Fiends, how long | still fighting after 120 real s in both runs (7 of 12 dead in one) | over in 32 s (5 Fiends dead, 1 trooper) |
| Target changes | 38 among 12 fighters (3.2 each) | not logged per fighter |
| Joined before anyone targeted them | 10 of 12 (troopers and Fiends alike: they saw the fight or were alerted) | `combat group: … alarms …` lines (allies-alerted scenario) |
| Fled | none in the NPC battles; 4 in the Goodsprings gunfight (animals, as on 2026-10-08) | `0 fled` |
| The player targeted | 6 of the 38 target choices in one NCR–Fiend run were the player, despite sneak and a Stealth Boy | never (unseen by the aid) |

Taking cover is not readable in the original (no function reports it).

## 5. Conversations between NPCs

**Correction (2026-10-17, later)**: the first map counted 10 conversations in the towns and
15 in Gomorrah, but `observe_map` took any dialogue package or dialogue target as a partner,
and half of them named a stool, a cook stove, a guard marker or an XMarker (a sandboxing
NPC's dialogue target). With both sides required to be sampled actors (fixed on this
branch), the original has **5** in the towns (Westside 4, Novac 1) and **5** in Gomorrah.
All five in the towns were **960–3,050 units from the player** (the observer standing at
the arrival marker): the original has no rule that the player be near.

Ours was run from the same spots and hours as the original (smoke-style launches of
`nvforever --play --hour 10 --no-voice`, 370 real s each = 3 game hours; logs private):
"next to the player" is our rule as shipped (only with the player within 300 units of the
starter or its partner, the owner's playtest list 5 item 28, the player among Westside's
citizens or in the Novac diner); "anywhere" is a measurement-only build with that limit
lifted, which is what the original's numbers compare with.

| | Original | Ours, old rule (a timer per actor) | Ours, new rule (one timer for the area) |
|---|---|---|---|
| Westside, anywhere | 4 in 6.07 game h (**0.66 an hour**), 36.4 NPC-hours | 10 in 3 game h (**3.3 an hour**) | 2 in 3 game h (**0.67 an hour**), 32.5 NPC-hours |
| Novac, anywhere | 1 in 6.07 game h (0.16 an hour), 84.1 NPC-hours | 2 in 3 game h (0.67 an hour); 22 tries, 6 won, most starters had no line for their partner | 2 in 3 game h (0.67 an hour), 49.1 NPC-hours |
| Goodsprings, Freeside, Camp McCarran, Primm | 0 in 32.4 game h | not run | not run |
| Gomorrah | 5 in 14.1 game h (0.35 an hour), 296 NPC-hours | not run | not run |
| Per NPC per loaded game hour (each conversation counts for both) | 0.032 over the seven towns (Westside 0.22, Novac 0.024); Gomorrah 0.034 | Westside 0.62 | Westside 0.12, Novac 0.08 |
| Westside, next to the player | — | 0 in 3 game h (15 tries, 1 won, no partner line) | 0 in 3 game h (5 tries, 2 won; one walk given up, the partner greeting the player) |
| Novac diner, next to the player | — | 0 in 3 game h (22 tries, 5 won, no partner line) | 1 in 3 game h (7 tries, 2 won) |
| Who | Westside: the four citizens 0x001381A7, 0x001381A8, 0x001393C2, 0x001393C6 | the same four and 0x0015D441 with 0x0015D443 | the same; Novac: AdaStrausREF with two settlers |
| Starter's distance when it starts walking up | median 624 (164–937) | 150–907 | 149–533 |
| Distance when the talk starts | median 120 (82–624) | 150 after a walk, but 7 of 10 started where the starter stood (218–907): the walk ended at once (follow-up 9) | 150, one at 533; with follow-up 9 fixed 148–149 after a walk (2 of 2) |
| Length | median 2.0 game minutes (0.8–5), 1 line each | until a Goodbye or no more links | the same |
| Hours of day | 10:00–13:00 (the runs' hours) | 10:00–13:00 | 10:00–13:00 |

**The timer** [hypothesis, fitted to the counts above]: `fAISocialTimerForConversationsMin`
/ `…Max` (30 / 60) count **seconds of play**, and there is **one timer for the loaded
area**, not one per actor. One try every 45 s on average at 25 % is one conversation per
180 real s, 0.67 per game hour at TimeScale 30: Westside's 4 in 6.07 hours (0.66) match
it, and no town or Gomorrah went above it. A timer per actor (our old rule) gave 3.3 an
hour in Westside, five times the original, and would grow with the crowd; a timer in game
seconds would try every 1–2 real s, a hundred times the original. Game minutes (one
conversation per 6 real min, 0.33 an hour) are not ruled out by four conversations.
Where few pairs have lines for each other (Novac, the four towns with none) the original
stays below the timer's rate; ours shows the same in Novac's old-rule run (6 chances won,
2 conversations).

**Change (this branch, `fnv_ai::social`)**: one timer for the loaded area: when it is due
and some actor that may start a conversation has someone within the radius (one of the two
next to the player), the timer is drawn again (30–60 s) and the chance (25 %) is rolled;
on a win a starter is drawn among those actors, and if it has no line for anyone near, the
next drawn. An actor rests a time drawn the same way after a conversation; one that has
just said hello does not start one. The next-to-the-player rule stays (the owner's), so in
play the player hears at most one conversation per 1.5 game hours, and only where a pair is
next to them. The `social summary` line now gives the tries, the chances won and the
actor-seconds next to the player; smoke scenario `conversation-timer` checks one to three
tries in 90 s of Westside.

Other speech outside combat (hellos, idle comments) in the original: 11 lines in the same
46 game hours (Freeside 4, Westside 4, Novac 3).

The game settings the masters hold for this [observed: FalloutNV.esm, `fnvtool esm
decode`]: `fAISocialTimerForConversationsMin` 30, `…Max` 60,
`fAISocialchanceForConversation` 25, `fAISocialchanceForConversationInterior` 25,
`fAISocialRadiusToTriggerConversationInterior` 1000, `fIdleChatterCommentTimer` 15,
`fIdleChatterCommentTimerMax` 60, `fAIDefaultSpeechMult` 0.15. No exterior radius and no
greeting distance is in the masters (their defaults are in the encrypted executable).

## 6. Casinos (Gomorrah only)

The casino part rests on **Gomorrah alone**: the owner stopped the campaign after its first
session, and that session too was ended early to free the game for other sessions. It
covers Gomorrah's main floor from 12:00 to about 02:00 (14.1 game hours, 1,759 samples,
21 actors in the cell). The Tops, the Ultra-Luxe, the Atomic Wrangler, Vikki & Vance and
Gomorrah's lobby were not observed.

- **Conversations**: 5 between NPCs (first counted 15, ten of them an NPC with an XMarker
  or an idle marker, §5), 0.35 per game hour in the cell, 0.034 per NPC per game hour
  (each counts for both; 0.032 over the seven towns), at 120–190 units when they start
  (median 161) and 120–175 when the talk starts, 0.5–1 game minute long, one line each; at
  12:00–13:00 (3), 14:00 and 21:00.
- **Seats**: 37 sitting edges; one NPC sat in up to five different chairs over the day,
  each for 1–14 game hours; chairs of furniture marker types 12, 14, 15 and 20.
- **What the time went to**, by procedure: PATROL (most), SITTING, GUARD, then TRAVEL and
  GET_UP; DIALOGUE took a fraction of a percent.
- Other speech outside combat: 10 lines in 14 hours. No fight.

Ours: no casino scene was run (the comparison of the casino rules waits for a scene with
the same actors; §8).

## 6a. The interaction map

All sessions together (`observe_map.py` over every output; JSON private, from
`fnv.observe.map/1`): 166 actors; edges: 138 targets (22.4 actor-hours), 71 uses of
markers and furniture as package targets (133.1), 57 sits (121.0), 24 talks to (0.9),
5 follows (10.8). Seats by furniture marker type: 15 (37.4 actor-hours), 14 (29.0), 20
(28.2), 19 (13.0), 12 (11.7), 17 and 11 (under 1.2).

What each package did, by record type (67 of the 80 packages seen have a record; the rest
were created at run time):

| Record type | Packages | Actor-hours | Procedures |
|---|---|---|---|
| Sandbox | 20 | 239.8 | WANDER 39 %, SITTING 38 %, TRAVEL 12 %, USE_IDLE_MARKER 6 %, GET_UP 5 % |
| Patrol | 17 | 168.2 | PATROL 99 %, DONE 1 % |
| Travel | 18 | 64.4 | DONE 79 %, TRAVEL 21 % |
| Guard | 2 | 28.3 | GUARD 100 % |
| Follow | 3 | 16.9 | FOLLOW 100 % |
| Dialogue | 2 | 14.1 | DONE 100 % |
| Eat | 4 | 3.9 | EAT 51 %, TRAVEL 49 % |
| Created at run time | 13 | 12.8 | GET_UP and DIALOGUE / DIALOGUE_ACTIVATE |

Every one of the 67 reported the record's own runtime type code (GetCurrentAIPackage), and
none was in use more than 5 % of its time outside its record's schedule window.

## 7. After the fixes

The same scenes in our engine with this branch's two changes:

| | Before | After | Original |
|---|---|---|---|
| NCR–Fiend battle: combat lines | 47 (26 Hit, 15 Attack) in 32 s | 11 (2 Hit, 2 Attack, 4 Death, 3 DeathResponse) in 46 s | 6–7 in 120 s |
| Melee blows in that battle: start, median (range) | 36 (34–86), closest 33 | 84 (64–90), closest 64 | weapons 95–96, closest 91–92 |
| Violet's dogs on the player: start, median | 14, closest 0 | 54 (52–63), closest 52; with the creature rule 121 (104–128), closest 104 | 133 (95), closest 87 |
| Trooper with a knife or machete scenes: blows, median | 24–25, closest 0 | 55–56, closest 16–44 (most blows there are the dogs') | 95–96 |

Combat lines are now within the original's order of magnitude (about 1.3 per
fighter-minute against 0.3–0.6) and melee people stand where the original's do; creatures
closed in nearer than the original's until the creature rule of §3 (follow-up §8.2,
`combat-dogs` smoke, 2026-10-17). Detection barks were
unchanged (6 in the battle).

## 8. Mismatches and follow-ups

1. **Combat lines** (fixed here, §2). Check the fixed rate against the original's 0.3–0.6
   per fighter-minute in a long fight; Detection barks (7 in our battle) were not looked at.
2. **Melee spacing** (fixed here for weapons, §3; creatures: done, §3, the rule
   (RNAM + 59) × scale fitted to dogs, coyotes, mantises and a rat). Open: creatures far
   longer or shorter than these (deathclaws, radroaches, cazadors) were only seen holding
   off a god-mode player; check them against NPCs.
3. **Fight length**: our NCR–Fiend battle ends in 32 s, the original's runs past 120 s.
   Look at hit chance and damage per shot at range (the original's 217 hits over about
   240 s of two runs against our 110 in 32 s).
4. **Attackers that hold off a god-mode or very healthy player** (§3): measure with a
   normal player (the owner's save, no god mode) before copying it.
5. **Conversation rate** (done, §5): one timer for the loaded area; Westside's rate now
   matches. Gomorrah and the four towns without conversations were not run in ours.
6. **Strip crowd**: run the `strip` campaign from `nvfstrip`.
7. **Spawn aid for our engine's fights**: `combat:placed:<base>` is not accepted by
   `NVF_PLAY_TEST_SCRIPT`, so the creature scenes in our engine only had Violet's dogs.
8. **Smoke scenarios moved by the fixes** (marked `known_failing` with the reason):
   `combat-lines` (the dogs' first bites now land before the player's power attack, which
   falls inside the player's own Hit line; fixed 2026-10-17: an attack's line now cuts
   the attacker's own Hit line, combat-dialogue.md §2, and the mark is gone) and
   `combat-cover` (no cover in 2 of 3 runs, 2 covers in the third; with the creature
   rule of §3 the same, cover in 1 of 3 runs: the mark stays).
9. **A walk up to a partner that ends at once** (§5; fixed, branch
   `claude/chat-walk-up`): in Westside 7 of our 10 old-rule conversations started where
   the starter stood, 218–907 units from its partner (the original's median is 120, but
   one of its five started at 624). **Cause** [observed: unit test
   `a_sandboxer_walking_up_to_a_conversation_partner_walks_the_whole_way` fails without
   the fix]: the walk's "reached" test (`Ai::walk`) still looked at the starter's package
   goal, which the walk to the partner does not replace. These citizens sandbox, and a
   sandbox's goal is its area (`Goal::Near`, the package's radius): a starter already
   inside it "arrived" in the walk's first frame, and `Ai::chats_on` took the walker that
   was no longer Travelling as "as near as the navmesh lets it get". **Fix**: a walk to a
   partner ends only at its path's end (as a sandbox's own walks and searches do), and a
   way that ends still farther than `TALK_DISTANCE` (150) gives the conversation up
   instead of talking across the distance [hypothesis: the original's 624 shows it can
   talk from farther; ours keeps to 150 until that is understood]. Westside at 10:00 with
   conversations allowed anywhere (test aid `NVF_PLAY_TEST_CHATS_ANYWHERE=1`), 5 min
   [observed 2026-10-17]: 2 conversations, both after a walk, 535 to 149 units in 4.5 s
   and 283 to 148, none given up. Smoke scenario `conversation-walk-up` checks that every
   conversation starts within 150 units.

Gomorrah stopped early and the Strip run from `nvfstrip` was dropped when the owner needed
the game for other sessions (2026-10-17).

## 9. Fight length and melee hold-off, measured again (2026-10-19)

AI observer campaign `fights_round` (tools/capture/observe/fights_round.campaign.json,
plugin version 9: every game loop, the fight field set and xNVSE's OnHit / OnDeath /
OnStartCombat events; `tools/capture/fight_stats.py` counts shots, hits and damage), save
`nvftest` [observed].

**NCR troopers against Fiends** (`ncr_fiends`, 1 game hour, 121 real s):

- Only **four** troopers took part. `MoveTo` on 0x000FCEB1 and 0x000FD572 failed ("script
  call failed"): both are non-persistent Camp McCarran sentries (NCRTrooperSentryCM) whose
  cell was not loaded, which the original's scripts cannot reach. Our `battle-ncr-fiends`
  aid moved all six.
- Three Fiends died within 8 s; **Cook-Cook** (FiendCookCookRef 0x000F5704, 215 Health, a
  flamer) killed the four troopers one by one (29.7, 51.5, 52.3 and 99.1 s) and lived
  with 15 Health; a second run (`ncr_fiends_far_resetai`) ended the same way at 89 s.
- Troopers: 36–138 shots each, **0.28–0.49 hits per shot**, a median **2.0–5.5 damage per
  hit** (most 23–27), shots from a median 650–1,500 units; all fighters together 291
  shots, 187 hits on actors (0.64 a shot), a median 2.0 damage a hit.
- **Ours with the same four troopers** (smoke `battle-ncr-fiends-four`, the engine's new
  `npc hits summary` line): over in 38–47 s, Cook-Cook dead. Hits from actors, by target:

  | Target | Original: hits, median damage a hit | Ours: hits, median damage a hit |
  |---|---|---|
  | Cook-Cook | 60, 2.0 (200 in all, of 215 Health) | 46, 6.2 (290 in all, of 290 Health) |
  | Fiends | 7–17 each, 4.2–11.1 | 3–10 each, 8.3–8.8 |
  | NCR troopers | 5–13 each, 0.1–1.9 (most died of burning) | 2–12 each, 1.7–30.4 (three died of 2–3 hits) |

  The hit rates are alike; **the damage a hit is two to fifteen times the original's** in
  ours, most of all a Fiend's gun on the troopers, and the actors' maximum Health differs
  too (Cook-Cook 290 against 215). Settled below ("Why the damage differed").
- The player moved 3,500 units off the fight at 52 s (the second run): the NPCs went on
  fighting with **IsInCombat 1** while 2,500–5,500 units from the player, until the last
  trooper died; then the rest left combat.

**Why the damage differed** (campaign `fight_stats_round`,
tools/capture/observe/fight_stats_round.campaign.json: the same fight every 10 game loops
with each fighter's GetLevel, GetBaseAV Health, Endurance, Agility, Guns, DamageThreshold
and DamageResist and GetEquippedCurrentHealth of its weapon, body armour and headgear;
output `obs_fight_stats_round_01_ncr_fiends_stats.1792409539.jsonl`, private) [observed
unless tagged]:

- The fighters' levels and skills are the record's in both games: troopers level 5,
  Guns 46; Fiend3 level 9, Guns 63; Cook-Cook level 14, Guns 58. The player's level (1)
  plays no part. The skill bonus is as documented.
- **NPC weapons are worn in the original, ours were new.** The troopers' Service Rifles
  had 120–200 of 400 health (30–50 %), the Fiend's Hunting Rifle 135 of 300, Cook-Cook's
  Incinerator 110 of 200. They come from the masters: the troopers' inventory names
  CondNVServiceRiflelLoot, a leveled list whose entries carry a COED item condition of
  0.30 to 0.60 (CondNVTrooperArmor 0.35 and 0.50, CondKnifeLoot 0.45 to 0.75). Our
  leveled-list reader dropped the COED that follows an LVLO, so every rolled item was
  whole. Fixed (fnv-sim `forms::leveled`).
- **Condition's damage penalty.** Raw trooper hits (the damage on Cook-Cook, DT 13,
  divided by the 20 % floor; on Fiends, DT 5, plus 5) were 10.94, 10.50 and 9.18 with
  rifles at 50, 45 and 30 %: 0.832, 0.799 and 0.698 of 18 × the skill bonus 0.73. That is
  `1 − (0.75 − condition) × 0.67` to three places [documented: GECK wiki
  SetConditionDamagePenalty, the hard-coded rule the JIP function overrides; full damage
  from 75 %]. Ours was a [design] line from 50 % at zero. Armour has the matching rule
  below 50 % [documented: SetArmorConditionPenalty]. Both now in fnv-combat.
- **NPC armour does not wear.** Cook-Cook's armour stayed at health 85 and his DT at 13
  through 60 hits; the troopers' and Fiends' armour likewise. Ours took 0.35 of each hit
  from it: his DT fell from 12 to 7 in the fight and the troopers' rifles did 6.5 instead
  of 2.6. Now only the player's armour wears. NPC weapons do wear from firing in the
  original (the Incinerator lost 2.19 a shot); ours do not yet (follow-up).
- **The 20 % floor holds**: Cook-Cook's hits were exactly 0.2 × the raw hit (2.1875 of
  10.9375); a critical added the weapon's full Crit Damage (18) before DT.
- **NPC Health**: the original's GetBaseAV Health is Base Health + (Endurance − 1) × 5,
  plus (level − 1) × 5 for an NPC with "Auto-calc stats"; a hand-set NPC's level adds
  nothing. Fifteen NPCs at full Health fit it (troopers 50, Fiends 65–105, Cook-Cook 215,
  Sunny 90, Doc Mitchell 65, Ringo 120, Trudy 75, Violet 170, …). Ours added Endurance ×
  5 + level × 5 for every NPC: 10 more for most, 75 more for Cook-Cook. Fixed (fnv-game
  `max_health`, combat-rules.md §9).

**After the fix** (smoke `battle-ncr-fiends-four`, 2026-10-19, one run):

| Target | Original: hits, median damage a hit | Ours before | Ours after |
|---|---|---|---|
| Cook-Cook | 60, 2.0 (200 of 215) | 46, 6.2 (290 of 290, dead) | 48, 1.9 (152 of 215, lives) |
| Fiends | 7–17 each, 4.2–11.1 | 3–10 each, 8.3–8.8 | 2–13 each, 5.1–5.2 (one 60.7: two hits from a Fiend's own Hunting Rifle) |
| NCR troopers | 5–13 each, 0.1–1.9 | 2–12 each, 1.7–30.4 | 9–23 each, 1.7–2.3 (one 8.2: the Hunting Rifle) |

Cook-Cook now wins, as in the original: the troopers died 34, 48, 50 and 57 s after the
first fighter engaged (the original's at 29.7, 51.5, 52.3 and 99.1 s), three of them by
his flamer. What is left: the last trooper dies some 40 s sooner than the original's, and
our Fiends live longer (17–52 s against the original's three dead within 8 s); one run
each, so the spread is not known.

**Melee against the player, with and without god mode** (inside GSGasStation; a MoveTo to
the outdoor spot failed, so both are in the same room; 30 s each; "no god mode" is Damage
Threshold 200 and Health restored every 2 s):

| Attacker | God mode | Distance to the player, median (least) | Swings | Hits |
|---|---|---|---|---|
| NCR trooper, knife | no | 97 (56) | 24 | 19 |
| NCR trooper, knife | yes | 95 (41) | 18 | 14 |
| NCR trooper, sledgehammer | no | 119 (77) | 22 | 18 |
| NCR trooper, sledgehammer | yes | 126 (98) | 19 | 16 |
| Dog (VCrTier1DogMed) | no | 71 (65) | 16 | 16 |
| Dog | yes | 70 (68) | 19 | 19 |

God mode changes nothing here: every attacker closed in and swung. The outdoor hold-off of
§3 (147–177 units, no swing) is therefore not god mode's doing [hypothesis left: something
of the outdoor spot near Violet's dogs; not settled]. NVForever's
attackers close in, as these do: no change.

## 10. The Strip's crowd against ours (2026-10-19)

The original [observed]: campaign `strip` (tools/capture/observe/strip.campaign.json),
session `strip_evening` from `nvfstrip`, 6 game hours from 20:14, 1,238 samples every 30
game loops, with xNVSE's events (plugin version 9): 21 actors (the crowd of §1's list and
the securitrons), 5,682 events (2,504 OnPackageStart, 2,496 OnPackageDone, 236
OnPackageChange, 348 OnTriggerEnter, 53 OnActivate). Timeline private
(`C:\fnv-private\capture\observe\strip.timeline.json`).

Ours: `fnvtool ai run --samples` on the same capture: each actor starts where the capture
first saw it, at its hour, and is compared at every sample (smoke `ai-observed-strip`).

**First run: nothing matched**, for two reasons in the comparison, not in the AI:

1. Every crowd package (the travellers' and gamblers' ports, the MPs' patrol and check,
   the securitron messenger's dialogue) has the condition GetQuestVariable
   VStreetFluffNPC 52 == 1, which the game's own scripts set on the Strip; the headless run
   had no quest state. `ai run` now takes `--quest-var QUEST:INDEX=VALUE`.
2. Mr. Holdout and the dancers are placed initially disabled (a trigger enables them in
   the game); the run left them disabled. It now enables an actor the capture saw enabled
   (GetDisabled 0).

**With both** (1,219 samples an actor): the package was the same for 97–100 % of the
samples for Mr. Holdout (VMrHoldoutHangOutSell), the three dancers, the gamblers, the
travellers, MP A and the securitrons; MP B 83 %.

What still differs, and why [observed; the causes from the records]:

- **The NCR troopers A–D and the MPs' escorts**: the original ran the SS2 chain
  (VStripSS2Start1Trooper*Move, VStripSS2TravelEnd*, VStripSS2WallDrunk3,
  VStripSS2StartMP*Escort), which is in none of their package lists: a scene script gives
  it. Ours ran their own lists (VStreetSandbox38*, VStripFluff1HookerCheer*). The headless
  run runs no scripts; not an AI difference.
- **The travellers and gamblers walk back and forth in the original**: OnPackageDone and
  OnPackageStart of their port packages came in pairs 550–590 times per actor in 6 hours
  (bursts about a game second apart), and the timeline has 90–107 segments each. Ours made
  one trip each and stayed. Their packages (VStripFluff1PortRNDToGomPackage, …ToTop…) have
  an **end result script** [observed: the records] that, by the packages' names ("Port …
  RND"), sends the actor on to another port [hypothesis: the script's text was not read];
  the headless run does not run result scripts. Their positions therefore differ (median 630–1,040 units in the
  same cell). To compare the walking itself, the next step is the same span in
  `nvforever --play` with an engine-side sampler, which runs the scripts.
- MP B's 17 % and MP A's 3 %: the SS2 escort, above.

Nothing in the AI itself is shown wrong by this capture; the comparison tool was fixed.
