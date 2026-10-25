# Handoff: the owner's playtest list 4 (2026-10-12, night)

The owner's fourth list (`C:\fnv-private\todo 4.txt`, copied in §4) is split between the two
coordinator machines below. The previous lists are all merged (`playtest-2026-10-12.md`).

## 1. Rules for this list

- **One agent at a time on each machine** (owner, 2026-10-12: avoid CPU congestion). Each machine
  works through its queue in order: start the next workstream only when the previous PR is
  merged.
- The usage cut applies: medium effort; engine clippy once; root checks only if `crates/`
  changed; only the workstream's own smoke scenarios, in one `python tools/smoke run --only …`
  launch; no lab runs; one before/after frame per case.
- One worktree and branch per workstream, from `origin/main`. Each machine merges its own PRs
  and messages the other machine on merge when shared files changed.
- Guesses go in a new `docs/owner-review.md` section per workstream (keep both sides on
  append conflicts, newest last).
- The owner's screenshots for this list are only on the laptop (`C:\fnv-private\<folder>`), so
  every item that comes with a picture is in the laptop's queue.

## 2. Laptop queue (the machine with `C:\fnv-private`)

| # | Branch | Items (§4 numbers) |
|---|---|---|
| L1 | `claude/shadows-sun` | 5 first-person shadow flicker outside Doc Mitchell's house (`shadowbug`); 6 shadow cost (draw distance / quality limits); 7 the sun no longer drawn |
| L2 | `claude/falling-loading` | 8 falling at too shallow a drop on a steep road (`runningdownhill`); 9 loading screen removed before the world is drawn; 10 ground fading in and the player dropping after leaving the gas station (`Worldloading`); 2–3 Victor's and Sunny's walk/run stopping and starting (the owner thinks it may be linked to 8) |
| L3 | `claude/smoke-grass-fx` | 16 smoke effects not animated and far too visible (`campfiresmokenotanimated`); 17 stray pixels in some grass (`grasspixels`) |
| L4 (moved to the desktop, 2026-10-13) | `claude/creature-data` | 20 names from template data (`actornameshowingasID`: "VSpawnTier2GeckoMed" shown, "Gecko" wanted; check every template-inherited field); 18 non-hostile geckos attacking at close range (`nonhostilegeckos`); 19 no "E) Talk" on creatures or hostiles, name only; 14 Goodsprings bighorners joining the fight though cowardly and unaggressive |
| L5 | `claude/fp-shadow-flicker` | 5 again: L1 (#254) could not reproduce it. Owner (2026-10-13): first person only, sun shadows on only, all DLC loaded; their run had played a long time (462 cell loads). Look at what casts into the sun shadow map only in first person (first-person body, arms rig, the arms' shade light) |

## 3. Desktop queue

| # | Branch | Items (§4 numbers) |
|---|---|---|
| D1 | `claude/dialogue-list-4` | 1 Doc Mitchell's opening lines not voiced; 4 message box buttons ignore their conditions (chemistry set, broken 9mm SMG in Doc's house); 22 Johnson Nash has no Goodbye |
| D2 | `claude/third-person-aim` | 11 third-person crosshair not used for the activate prompt; 12 bullets go left of the third-person crosshair (the shot still comes from the first-person screen centre) |
| D3 | `claude/combat-list-4` | 13 Ringo didn't follow out of the gas station to the Powder Ganger fight (appeared after it ended); 15 taking a dead Powder Ganger's armour/weapon leaves it drawn on the body; 21 actors in a crippled stagger attack before the stagger ends |

## 4. The owner's list (todo 4.txt, copied 2026-10-12)

1. Doc Mitchell's opening lines are not voiced.
2. Victor's move-forward animation stops and starts while on his patrol package from his shack
   to the water tower; it must be continuous. Check all creatures for this (may be linked to
   falling from a short distance, item 8).
3. The same with Sunny while she runs to clear the geckos after the bottle-shooting tutorial:
   inconsistent run start/stop animations.
4. Message box buttons have conditions that we don't apply: the chemistry set and the broken
   9mm submachine gun in Doc Mitchell's house let me mix or repair with too low skills. Buttons
   whose conditions fail should not be shown.
5. Still the first-person shadow flicker, noticeable when looking around outside Doc Mitchell's
   house after leaving for the first time; screenshots with the flickering shadows marked in red
   in `C:\fnv-private\shadowbug`.
6. Shadow rendering seriously affects performance; maybe limits on draw distance or quality.
7. We used to have a visible sun; not seen at any time of day (may be fixed by the latest
   changes).
8. The character starts the falling animation at too shallow a drop while walking down a steep
   road (`C:\fnv-private\runningdownhill`, an exact spot: run forward for 5 s). The player gets
   stuck between falling and running; maybe a longer grace distance; check the physics.
9. The world hasn't finished drawing before the loading screen is removed.
10. Ground in front of the Goodsprings gas station fades in, and the character seems to drop from
    a height for a split second after leaving the gas station (`C:\fnv-private\Worldloading`).
11. The third-person crosshair is not used for the activate prompt: you have to look to the right
    of the object before the talk/activate prompt shows.
12. The same for shots: bullets go left of the crosshair, because the first-person screen centre
    is offset by the over-the-shoulder third-person camera.
13. Ringo did not follow out of the gas station to help fight the Powder Gangers; I left first
    (AI processing stopped when no longer in the same cell?). He appeared after the fight ended.
14. The bighorners in Goodsprings helped fight the Powder Gangers. Their records say cowardly and
    unaggressive, so they should flee; this should override the helps-allies combat join. Confirm
    aggression and confidence effects with the GECK wiki's actor pages.
15. Taking a dead Powder Ganger's equipment doesn't remove the equipped armour and weapon from the
    body's model.
16. Campfire smoke and the smoke in the Bison Steve hotel entrance are not animated, and far too
    visible (our render vs the game's, where you can barely see it:
    `C:\fnv-private\campfiresmokenotanimated`). Applies to all placed smoke effects.
17. Some grass textures have floating pixels that are not part of the grass
    (`C:\fnv-private\grasspixels`).
18. Non-hostile geckos attack when the player gets too close (a guard radius on the package or the
    actor record?) (`C:\fnv-private\nonhostilegeckos`).
19. Looking at creatures should show no "E) Talk", only the name; the same for hostile targets.
20. The editor ID is shown as the name: a dead gecko shows "E) Search VSpawnTier2GeckoMed"; it
    should be inherited from the template (leveled list VEncTier2GeckoMed, base record named
    "Gecko"). Check that every actor value flagged as from the template is inherited correctly
    (`C:\fnv-private\actornameshowingasID`).
21. NPCs/creatures playing a crippled stagger should not attack until it is complete.
22. Johnson Nash has no Goodbye in dialogue (missing goodbye topics?); you can Tab out, but
    Goodbye / "I need to get going" should be shown.

## 5. Outcome (2026-10-13)

Every queue item is merged: L1 #254, L2 #258, L5 #260, L3 #263 (laptop); D1 #257, D2 #259,
D3 #261, L4 #264 (desktop); CI fix #255 (main was red since #242). Details and guesses are in the
owner-review sections of each.

Left open:
- Item 5, first-person shadow flicker: not reproduced twice. Debug probe `NVF_PLAY_DEBUG_SHADOWS=1`
  (#260): the owner runs with it and keeps the log; best lead is a skinned vertex drawn at the
  world origin (east of Goodsprings).
- Items 2–3, Victor's and Sunny's stop-start: one cause fixed (#258), not reproduced; needs the
  owner's eyes on those two walks.
- Item 16: smoke built from particle systems inside ordinary models (about 66 placements, e.g.
  Primm smoke, HELIOS steam) is still not drawn.
- Item 18: the owner's two geckos at cell -17,-6 still don't attack: their first package
  VCrGeckoFindFood forbids aggro and our once-per-day Find package never ends. Follow-up L6 below.
- Wider effect of #264: about 467 placed creatures and 14 NPCs now attack inside their aggro
  radius (two warnings 5 s apart).

| # | Branch | Machine | Item |
|---|---|---|---|
| L6 | `claude/package-duration` | laptop | once-per-day packages end after their duration, so the next package (with aggro) takes over (item 18) |
