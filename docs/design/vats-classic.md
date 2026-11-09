# The classic VATS style

Status: being built on branch `vats` (2026-11-09). Sections say what is built and what
is planned. The other style, "Dead Eye" (docs/design/vats-mode.md), is unchanged and
selectable; this one is the default.

Tags as everywhere: [documented] cites a public page, [observed] the game's data files,
[hypothesis] an inference, [design] NVForever's own choice. Combat is not a parity
target the project measures, so where the public pages give no number the choice is
[design] and says so.

Sources read 2026-11-09 (public pages only; no executable was read):

- fallout.wiki, "Fallout: New Vegas V.A.T.S.", "Fallout 3 V.A.T.S." (the game settings
  table; Fallout 3's defaults), "Action Points (Fallout: New Vegas)", "Special Attack
  (Fallout: New Vegas)", "Fallout: New Vegas perks", "Fallout: New Vegas traits".
- fallout.fandom.com "VATS" and wikihow.com "Use V.A.T.S. in Fallout 3 and New Vegas"
  refused automated reading; nothing here rests on them.

## 1. The owner's decisions (fork owner, 2026-11-09)

- The original's flow replaces Dead Eye as what V starts; Dead Eye stays as a style the
  settings page offers.
- A slider sets how far the world slows while targets are chosen: 50 % to 100 %, 100 %
  a full stop and the default, as the original.
- The slider acts on choosing targets only. The attacks then play as before.
- Slowed rather than stopped: chances are worked out every frame, an attack queued on a
  target that dies or leaves sight is dropped with its AP back, and the player is not
  protected while choosing.
- While the queued attacks play the player takes the original's share of a hit.
- Controls as the original's; where the pages do not give them, ours, rebindable.
- The rules (hit chance, AP, perks, helpers) are the ones VATS mode already has.
- V with nothing to target does nothing but play the refusal sound.

## 2. Time

- Choosing targets, the clock runs at `1 - slowdown` (`vats_classic::clock_scale`):
  0 at the default, at once, with no ease [documented: the game is paused while targets
  are chosen; design: the range, owner].
- At 0 the virtual clock is not paused, it runs at no speed, so nothing takes the
  picture and menus still open over it. The clocks that must not stop with the world
  count real time then (`pause::stands_still`): the test aids' clock and
  `--run-seconds`. The scripts' frame time stays the world's, none.
- The mode's own timing (the turn of the view, the scans) is in real seconds.
- Playing the attacks: `FIRE_SCALE`, as Dead Eye [design, until §8].
- Sounds follow the clock as they do for Dead Eye (`audio_out::slowed_sound`, floored).

## 3. Targets

Every living actor with a hit volume that is

- within the weapon's reach and `fVATSMaxEngageDistance` (5000) [documented:
  fallout.wiki "Fallout 3 V.A.T.S.", settings table];
- within `CLASSIC_FRONT` (55 degrees) of the line the player looked along on entering
  [design: about what the view shows];
- in sight from the eye (the world in the way hides it, another actor does not);
- not barred as for a Dead Eye mark: dead, cloaked without Enhanced Sensors, No VATS
  Melee with a melee weapon.

Hostile or not [documented: any actor can be targeted]. They are ordered left to right
across the entering view; the one nearest the crosshair is selected first [documented
for the first; design for the order]. With none, V is refused (`UIVATSEnterFail`).

The list is made again every 0.25 real seconds. A queued attack whose target is no
longer in the list is dropped (its AP was only reserved). When the selected target is
gone the selection moves to the one nearest the entering line; with none left the mode
ends.

## 4. Choosing and queueing

- Slide Left / Slide Right: the target before or after, round the ends.
- Forward / Back: the part before or after on the selected target.
- Attack (Use): queue an attack on the selected part. Each reserves the weapon's AP and
  rounds exactly as a Dead Eye mark (`MarkQueue`).
- Block: take the last queued attack back; with none queued, leave.
- Activate: play the queue. V: leave.
- Any other action leaves, as Dead Eye (§11.8 there).

[design: the pages give V, the accept key and the mouse; the rest is ours until the
game's own menu file is driven, §9.]

A new target starts on the part with the best chance [design]. Each part's label shows
its name and chance, the selected one bracketed, with "xN" when N attacks are queued on
it; with Concentrated Fire the label shows the chance the next attack on it would have.

## 5. The player and the view

While targets are chosen and while the attacks play the player does not move and the
mouse does not turn the view [documented: the player cannot move or dodge]. The view
turns onto the selected target in real time (`TURN_RATE`). Left without firing, the view
goes back to where it was on entering; after firing it stays on the last target.

Planned: the field of view narrowing to frame the target (§5 of the plan, on the
dialogue camera's framing).

## 6. What it shares with Dead Eye

Entering (weapon, AP and perk checks), the queue, each part's chance, the firing
sequence (shots, bursts, throws, melee runs), rolls and misses, the helpers, Grim
Reaper's Sprint, the kill cameras, the sounds, what the scripts see (GetVATSMode,
GetVATSValue). One set of rules for both styles.

## 7. Damage while the attacks play

A hit on the player is multiplied by `fVATSPlayerDamageMult` while the classic style's
attacks play: 0.75 without the setting [documented: fallout.wiki "Fallout: New Vegas
V.A.T.S.", 75 % in New Vegas, 10 % in Fallout 3]. Not while targets are chosen
[design: owner]. Dead Eye does not apply it, as before.

## 8. Planned on this branch

- The zoom onto the target (§5).
- Playback cameras from the game's camera records, shot by shot.
- The target screen driven by the game's own VATS menu file, the engine-drawn labels
  as the fallback.
- Melee and unarmed special attacks; mines as targets.

## 9. Test aids and smoke

- `NVF_PLAY_TEST_VATS_STYLE=classic|dead-eye`, `NVF_PLAY_TEST_VATS_SLOWDOWN=0.5..1`.
- Every Dead Eye scenario sets the first to `dead-eye`; their expectations are
  untouched.
- The classic scenarios are named `vats-classic-*`.
