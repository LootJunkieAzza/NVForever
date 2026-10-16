# Handoff: the owner's playtest list 5 (2026-10-16)

The owner's fifth list (`C:\fnv-private\todo 5.txt`, items numbered and summarised in §4) is
split between the two coordinator machines below. Lists 1–4 are merged
(`playtest-2026-10-12.md`, `playtest-list-4.md`).

## 1. Rules for this list (owner, 2026-10-16)

- **Concurrency:** the desktop runs at most **5** workstream sessions at once, the laptop at
  most **2** (hardware). Start a queued workstream when a running one has its PR in.
- **Each workstream tests only itself:** targeted `cargo test -p <crate>` while working; once at
  the end engine clippy (root test/clippy only if `crates/` changed) and its own smoke scenarios
  in ONE `python tools/smoke run --only <names>` launch. No full smoke selection, no lab runs, one
  before/after frame per picture case. Then push and open a PR; **never merge**. Stay available:
  the coordinator sends integration failures back to the workstream that caused them.
- **The coordinator integrates:** it merges its machine's finished PR branches into an
  integration branch (`claude/list5-integration-desktop` / `-laptop`) from `origin/main`,
  builds once, runs the full `python tools/smoke run --changed origin/main --checks` plus each
  workstream's new scenarios, sends each failure caused by a new item back to that workstream,
  and merges the PRs to main once the integration run is clean. Each machine merges only its
  own PRs and messages the other when shared files changed.
- One worktree and branch per workstream from `origin/main`; build folders copied to
  `F:/nvf-target/<name>-engine|-root` (desktop) — never shared, never the main checkout's.
- Guesses go in a new `docs/owner-review.md` section per workstream (append conflicts: keep both,
  newest last). The common agent rules: `C:\fnv-private\agent-common-brief.md` (desktop copy).
- The owner's reference pictures are in `C:\fnv-private\<folder>` on the desktop, so every item
  with a picture is in the desktop's queue.

## 2. Desktop queue

| # | Branch | Items (§4 numbers) |
|---|---|---|
| D1 | `claude/l5-hud` | 73 HUD bars cleanup + 75 % condition arrows; 32 target health bar; 7 sneak indicator; 74 dialogue hides HUD and other subtitles |
| D2 | `claude/l5-pipboy` | 2 inventory flash on Tab; 14 hover, sounds, item stats panel and icons; 30 scroll bar; 11 karma title; 52 map drag + local map; 54–56 static, mode and light sounds |
| D3 | `claude/l5-menus` | 8 pause/settings menu + main menu style; 46 main menu hover/box; 19 mouse pointer; 51 wait/sleep colours and font; 72 crafting menus |
| D4 | `claude/l5-combat` | 9 weapon jamming; 16 hit detection on creatures (+ unit tests); 24 melee impacts; 25 radscorpions; 20 enemy projectile line on the player; 49 tgm ammo; 21 encumbrance; 22 convict karma; 61 two-handed run speed; 23 camera pan returns to first person |
| D5 | `claude/l5-render-fx` | 1 dust storm; 48 bullet smoke; 10 explosions + grenade projectile; 6 placed lights; 18 weapon model/texture quality; 70 white cave water; 66–67 flashlight on the body, ghost shadow; 26 white beards; 38 SwapTexture |
| D6 (queued) | `claude/l5-world` | 17 steps; 59 motorcycles; 71 cave skeletons; 65 animated containers; 64 open after lockpick/hack; 63 bobby pin position + wobble; 4 viewer weapon animations |
| D7 (queued, after D1) | `claude/l5-compass` | 57 actor compass markers; 12 map markers + discovery message; 15 dynamic crosshair; 58 red crosshair on hostiles; 13 reputation (popup, tracking, Pip-Boy after D2) |

## 3. Laptop queue

| # | Branch | Items (§4 numbers) |
|---|---|---|
| L1 | `claude/l5-npc-ai` | 3 NPC door fade + sound; 28 idle chatter range; 29 ranger ghost package; 31 friendly alert range; 33 face target before firing; 34 weapon-drawn package flag (all flags); 35 Sunny helps against geckos; 37 + 36 NPC sneaking, Sunny sneaks; 40 Doc Mitchell off the chair |
| L2 | `claude/l5-ui-text-sounds` | 43–45, 50, 53 placeholder white text → corner messages or nothing; 39, 41, 42, 47 Vigor Tester, skills, traits, hacking sounds; 60 Take All closes; 62 TAB) Goodbye; 68 text overlay off by default; 27 no radio subtitles; 69 Fallout 3 music |

## 4. The owner's list (todo 5.txt, summarised 2026-10-16)

1. Dust storm "tornado" effects (outside Doc Mitchell's house, by the saloon sign) far too
   visible, like the smoke was; same for similar effects (`duststorm`).
2. Opening the inventory shows UI for a split second on Tab.
3. NPCs going through a door should fade out/in and play the door sound, like the player.
4. Viewer: step through all weapon animations on an NPC (weapon drawn: moves in all directions,
   attacks, moving attacks; aimed and hip-fire) to list broken ones.
5. (Owner: fixed, reference only — ammo stacks.)
6. The Goodsprings gas station lantern gives no light: every placed light-emitting record must
   light the world (`gasstation lightsource`).
7. Sneak indicator at the top middle while sneaking: [Hidden] [Detected] [Cautious] [Danger]
   (`Sneak detection`).
8. Final pause menu UI and settings layout fitting our new options; then the main menu in the
   same style (`Pause Menu settings layout`).
9. Weapon jamming for ammo weapons (not thrown): chance grows below 75 % condition, plays the
   first/third-person jam animation, no firing until it ends; keep the chance low.
10. No explosions: grenade launcher shows no projectile and no explosion; use the game's
    explosion effects, then the dust kicked up (`Explosions`).
11. Stats/General shows the karma title (game data if there, else the owner's table of titles by
    level and karma).
12. Map markers on the compass (discovered and undiscovered, range by Perception) and the
    "location discovered" message like the quest messages (`Map Markers`).
13. Reputation: track faction reputation from crimes/actions, show the threshold popup and the
    Pip-Boy Stats/General entry (`Faction Reputation`, `fallout fandom`).
14. Pip-Boy inventory: hover on sub-tab buttons and items with sound (`ui_menu_focus`), the
    hovered item's stats, icon and effects at the right; sub-tab switch plays
    `sound\fx\ui\pipboy\ui_pipboy_tab.wav` (`pipboy icons`).
15. Dynamic crosshair showing spread with a projectile weapon drawn.
16. Hits on creatures map badly: shots on a bloatfly hit with dust and no damage, others pass
    through the lower body; add unit tests.
17. Ledged steps the player runs up in the game need a jump here; make small steps and bumps
    traversable everywhere (`physics ledged steps` is the test case).
18. Equipped weapons look worse than the game's (model detail, blurred textures) (`Weapon Quality`).
19. The game's own mouse pointer in menus.
20. Enemies shooting draw a projectile line from the player's weapon.
21. Encumbrance: over the limit (284/200) the player must walk.
22. Killing a convict outside the NCR highway patrol station gave bad karma; should be good.
23. After panning the camera with mouse 3 or F, return to first person if the player was in it.
24. Melee weapons need impact sounds and debris/blood like guns.
25. Radscorpions nearly impossible to hit in melee (like baby geckos) and move too fast.
26. FaceGen: Sgt Kilborn (Mojave Outpost) and Joe Cobb have white beards not matching their hair.
27. Radio stations should show no subtitles (Radio New Vegas).
28. NPC idle conversations trigger too far away: the player must stand next to them.
29. Ranger Ghost should be on top of the building up the ramp, found by the chain-link gate idle.
30. Pip-Boy inventory scroll bar, only when the list is longer than the page (`pipboy inventory scroll`).
31. Friendly AI must be alerted from further away when a friendly is drawn into combat.
32. Target health bar like the game's, between HP and AP at the bottom middle, shrinking to the
    centre from both ends (`Target healthbar`).
33. NPCs must face their target before shooting (seen firing while looking 90° off).
34. Sunny runs to the geckos without her weapon drawn: package weapon-drawn flag; check all flags.
35. Sunny doesn't help clear the geckos (help allies / aggro range); she and her dog should fight
    as soon as the geckos aggro on the player.
36. Sunny should sneak towards the geckos before "OK, you're on".
37. NPCs sneak like the player (Always Sneak package flag and script hooks).
38. The SwapTexture hook.
39. Vigor Tester sounds moving between SPECIAL screens and raising/lowering stats.
40. Doc Mitchell gets off the chair after "the stuff that mattered" to its front right; the
    animation goes to his left.
41. Skill select: hover and select sounds, Accept/Reset too.
42. Trait select: the same.
43. No white text on picking up items or doors ("Stimpak Added", "Gate Opened").
44. "Enemies nearby" (can't sleep) as a top-left corner message.
45. Lockpicking and hacking "skill too low" as top-left corner messages.
46. Main menu hover sounds and a box round the selected option (`mainmenu`).
47. Hacking menu hover and select sounds from the game's assets.
48. Less smoke from bullets hitting walls/objects (blood and glass are fine).
49. tgm: no ammo used and no reloading.
50. Remove "quick save not available" on F5 and every placeholder white text.
51. Wait/sleep menu in the HUD's colour (HP bar) and font; it looks faded.
52. Pip-Boy local and world map: drag with left mouse; the local map draws nothing (needs an
    outline of the current cell).
53. Jukebox shows "scripts are not yet activated": remove the placeholder.
54. With the Pip-Boy inventory open a random sound from `sound\fx\ui\pipboy\static` plays.
55. The three orange mode buttons play `sound\fx\ui\pipboy\ui_pipboy_mode.wav`.
56. Pip-Boy light: `ui_pipboy_light_on.wav` / `ui_pipboy_light_off.wav`.
57. Actor compass markers in close range by Perception: amber non-hostile, red hostile (`Creature Markers`).
58. Crosshair red over a hostile.
59. The motorcycles outside the saloon can be pushed; in the game they don't move (tumbleweeds
    and crates are fine).
60. Container menu closes on Take All.
61. Slower running with any two-handed weapon drawn (NPCs and player).
62. Show "TAB) Goodbye" at the left of the dialogue box when available.
63. Lockpicking bobby pin drawn too high (clips the cylinder); centre it; wobble as its health
    drops (`lockpickmisaligned`).
64. After a successful lockpick or hack, open it as if activated.
65. Animated container NIFs (school safe, lockers, footlocker, toolbox) open before the menu and
    play their sounds; non-animated ones already do.
66. The flashlight shines from the camera and lights the player's back: attach it to the front of the body.
67. The flashlight draws a ghost shadow next to objects.
68. Text overlay developer option off by default.
69. Fallout 3 ambient music plays: a missing condition on music tracks?
70. Goodsprings cave water renders pure white (Camp Golf lake is fine) (`whitewater`).
71. Goodsprings cave skeletons float in a T-pose; in the game they lie posed on the floor (Havok
    data in the NIF?) (`tposeskeletons`).
72. The three crafting menus, one shared layout; recipes, skills and outputs from the records (`Crafting`).
73. HUD: HP/AP labels over the bars, even pips filling both bars, condition bar by the separator
    and level with "CND", the two black 75 % jamming arrows; like the game (`HPAPMisaligned`).
74. In dialogue all HUD elements go; other actors' subtitles (Joe Cobb talking to Trudy) hidden (`DialogueUI`).

## 5. Outcome (2026-10-16)

Every item of the list is merged on main. The laptop also took L3 `claude/l5-world-objects`
(items 59, 65, 64, 4, the D6 items without pictures); D6 kept 17, 71 and 63.

| Queue | PR(s) | Integration fix-backs |
|---|---|---|
| D1 HUD | #289 | — |
| D2 Pip-Boy | #288 | — |
| D3 menus, crafting | #290, #292 | — |
| D4 combat | #286 | #296 (aim point took a leg capsule: bighorner not hit), #298 (bighorner check order-independent; combat-dogs known_failing again: blow count follows frame rate, laptop) |
| D5 render/FX | #291 | script-stubs counts updated for SwapTexture, ShowRecipeMenu, Autosave, OpenTeammateContainer |
| D6 steps, skeletons, pin | #299 | — |
| D7 compass, crosshair, reputation | #297 | #300 (falling-loading runs with `--no-dialogue-look`: a late Ringo greeting left the view off the door) |
| L1 NPC AI | #294 | ncr-fiends cover/alarm (laptop) |
| L2 white text, sounds | #293 | — |
| L3 world objects | #295 | — |

Desktop integration runs: D1–D5 together (62 pass, 2 fail → fixed by their workstreams), then
D6 + D7 on main (32 pass, 1 fail → #300). Laptop: L1–L3 together (63 pass; its failures went
back to L1 and to D4).

Left partly done or unconfirmed, as the workstreams reported (details in each one's
`docs/owner-review.md` section): item 1 dust storm and item 10 explosions partly (D5); item 22
convict karma fixed in the survey but not checked in game (D4); item 71 the skeletons lie posed
but their collision still stands upright (D6). Merges between two appends of
`tools/smoke/scenarios.toml` dropped a closing `]` three times: check the file parses after a merge.
