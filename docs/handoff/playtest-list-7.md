# Handoff: the owner's playtest list 7 (2026-10-23)

The owner's seventh list (`C:\fnv-private\todo 7.txt`, items numbered and summarised in §3)
is taken by the desktop session "todo 7". The owner is away and asked for autonomous work:
decide, build, and write every guess down in `docs/owner-review.md` (one section per
workstream). The owner also asked that each PR is merged into main once GitHub CI passes; the
coordinator does that merge (workers never merge).

## 1. Rules

- At most 4 workstreams at once, each one worker in its own worktree and branch from
  `origin/main`, build folders copied to `F:/nvf-target/<name>-engine|-root`.
- Each workstream tests only itself: targeted `cargo test -p <crate>` while working; once at
  the end the CLAUDE.md checks its files call for and its own smoke scenarios in ONE
  `python tools/smoke run --only <names>` launch. No lab runs.
- The coordinator merges a PR when its CI is green and it merges cleanly with main.

## 2. Workstreams

| # | Branch | Items (§3 numbers) |
|---|---|---|
| W1 | `claude/l7-menus` | 1 rebuild keeps skills/perks; 2 menus on top + darker backgrounds; 7 currency value; 10 [AVSpeech]; 11 no mouse auto-scroll; 16 repair menu stays open; 25 barter/container item stats |
| W2 | `claude/l7-pipboy` | 5 aid effect lines; 6 keyring + caravan deck; 8 caravan cards not counted; 15 last inventory item; 20 HP/DT top bar; 33 skill books in Aid |
| W3 | `claude/l7-explosives` | 19 mine damage; 21 fall damage; 22 thrown weapon timing; 23 disarm XP; 29 incinerator; 26 head stays after explosion; 27 blood decals |
| W4 | `claude/l7-combat-ai` | 12 Trudy unarmed; 13 no moving while staggered; 36 ghoul speed + creature lunges; 39 nightkin club; 40 minimum firing distance; 43 strafe into cover; 45 recorded AI fight |
| W5 | `claude/l7-physics` | 9 step-height flicker; 14 corpses glide; 35 tiny mantis collision; 37 car weight |
| W6 | `claude/l7-ai-world` | 24 AI waits for the loading screen; 30 Beagle/strip furniture markers; 31 Beagle weapon drawn; 32 convicts hostile; 34 crucified gangers; 38 wake a sleeper on talk; 41–42 forced dialogue through walls/doors |
| W7 | `claude/l7-visuals-sound` | 3 leather armour hand; 17 bloatfly wings; 28 Lucky texture; 46 creature sounds audit |
| W8 | `claude/l7-perf` | 4 mantis egg spawn; 47 REPCONN frame rate |
| W9 | `claude/l7-audit` | 44 ESM / stubs / NIF gap audit (last, after the others merge) |

Order: W1, W3, W5, W6 first; then W2, W4, W7, W8; W9 last.

## 3. The list (summarised)

1. A character rebuild after leaving Goodsprings loses skills and perks; keep perks and skill
   levels, re-apply only the three tag skills.
2. Tag skills, trait, repair, crafting and level-up menus drawn on top of everything, with
   darker backgrounds so the text reads.
3. Light leather armour (DLC): right wrist mesh missing, hand floats.
4. Shooting the mantis egg in the Goodsprings schoolhouse drops the frame rate to single digits.
5. Pip-Boy aid effects: Programmer's Digest lists Science +10 twice and +20 twice (one +10,
   doubled by the perk); Blood Pack lists HP +4 and HP +1; same for stimpaks.
6. Misc tab: a keyring item holding all keys, and a caravan deck holding all cards, each opening
   a list with a way back.
7. Currency (NCR dollars) has no value in the barter menu.
8. Caravan: "you need at least 30 cards (you have 0)" after Ringo's starter deck.
9. The character jumps instantly over small road bumps.
10. Speech checks show `[AVSpeech]`; should be `[Speech]`.
11. Moving the mouse to the last item in a barter/container menu scrolls the list; only the wheel
    should.
12. Trudy has no weapon in the Goodsprings gunfight.
13. NPCs, creatures and the player must not move during a cripple or stagger animation.
14. Killed NPCs glide on (their collision keeps the charge's trajectory and pushes the player).
15. The last Pip-Boy inventory item can't be scrolled into view with long lists.
16. Repair menu stays open: closes only when the player closes it, the item is at 100 %, or no
    more repair items.
17. Bloatfly wings flap only while standing still.
18. (not used)
19. Mines must damage the player and anyone nearby.
20. HP and DT on the Pip-Boy top bar (now `--`).
21. Fall damage.
22. Thrown weapons (dynamite, frag grenades, spears) spawn the projectile out of sync with the
    throw animation.
23. Disarming mines and traps gives XP (5 if the data has none).
24. Johnson Nash forces dialogue while the loading screen is up; AI waits for loading to end.
25. Barter/container menus show no item stats; mirror the Pip-Boy item details.
26. An exploded head leaves the head model on the body.
27. Blood decals float off the mesh at wrong angles; and fewer of them.
28. Lucky shows the plain .357 texture in third person.
29. The incinerator does no damage when an NPC fires it.
30. Deputy Beagle doesn't use his captive marker in the Bison Steve; the same with the strip's
    prostitutes.
31. Freed Beagle following the player should keep his weapon drawn.
32. Convicts approaching the Primm police station should be hostile.
33. Skill books show in Misc; should be in Aid.
34. Nipton's crucified Powder Gangers get off their crosses.
35. Tiny mantises in the police station block the player; they should be stepped over (and on).
36. Feral ghouls ~25 % too slow; creatures don't lunge or power-attack forward.
37. Cars have no weight and get pushed around.
38. Talking to a sleeping NPC gets them out of bed.
39. Nightkin used fists instead of his rebar club.
40. No minimum firing distance for firearms (keep it for flamer, missiles, explosives).
41. Davison forces dialogue through the closed REPCONN basement door.
42. Harland forces dialogue through a wall.
43. NPCs strafe sideways into cover (Violet turned to face the cover).
44. Audit ESM records, stubs, scripted logic and NIF fields for gaps.
45. Record a combat fight per NPC to find AI logic gaps.
46. Feral ghouls make no sound; audit creature attack/alert/hurt sounds.
47. Frame rate drops on the REPCONN main floor.

## 4. Outcome (2026-10-23)

Every workstream is merged into main. Each PR's guesses and the things to check in game
are in `docs/owner-review.md`, one section per workstream ("… (2026-10-23, list 7 W#)").

| # | PR | Items done | Partly / not done |
|---|---|---|---|
| W1 menus | #597 | 1, 2, 7, 10, 11, 16, 25 | 10 not run in game (needs Trudy's quest state) |
| W2 Pip-Boy | #603 | 5, 6, 8, 15, 20, 33 | 15 checked by a unit test only |
| W3 explosives | #598 | 19, 21, 22, 23, 26, 27, 29 | 26 not seen in game (the run never hit the path); NPCs don't throw yet |
| W4 combat AI | #605 | 12, 13, 36, 39, 40, 43 | 45 partly: findings in combat-ai.md §18.7 |
| W5 physics | #600 | 9, 14, 35, 37 | 9's feel on roads is for the owner's eye |
| W6 AI in the world | #599 | 24, 30, 34, 38, 41, 42 | 31 no change (data has no Weapon Drawn flag); 32 no missing flag found |
| W7 visuals/sound | #601 | 3, 17, 28, 46 | super mutant / robot combat voices (dialogue topics) |
| W8 frame rate | #606 | 4 | 47 partly: room culling built, no measurable gain; material batching needed |
| W9 audit | #604 | 44: audit + teammate perks, compass quest markers, main smoke fixes | queue in `gap-audit-2026-10-23.md` |

### What the owner should check first

- **Item 1:** the character revision keeps level, perks and spent skill points; this departs
  from the original on purpose.
- **Item 2:** whether the 0.93 black behind the menus is too dark.
- **Item 21:** the fall-damage factor 0.0252 (fitted; `fJumpFallHeightMult` is not in the ESM).
- **Items 31 and 32:** Beagle drawing only outside the hotel and the station convicts not
  attacking match the records; say if the original differs.
- **Item 36:** ghouls ×1.25 is the owner's number, not measured.
- **Item 40:** firearms now fire to their full range; watch for NPCs shooting from too far.
- **Item 12 side effect:** AddItem now re-equips an NPC's best weapon and armour, which could
  undo a scripted EquipItem of a weaker weapon.
- **Item 8 side effect:** RemoveAllItems now runs OnAdd on the moved items.
- **Item 4:** spawned creatures placed on free spots; corpses frozen after 3 s still moving;
  at most 64 sound effects at once.

### Left for later sessions

- Renderer material batching for interiors (item 47).
- Combat: shots held for comrades in the line of fire, melee Fiends' charges (item 45);
  NPCs throwing grenades; creature weapons dropped on death.
- Physics: corpses of small creatures passing through each other; the player stepping onto
  a living small creature.
- The W9 queue: NPCs running scripted doors' OnActivate, teammate attack-speed perks, NIF
  light-colour controllers, compass distance figure, the Ripper's chain teeth, credits.
- Smoke on main: `knockdown-player` (one check marked known_failing, cause not traced),
  `vats-mode-melee` and `anim-sounds` marked with reasons.
