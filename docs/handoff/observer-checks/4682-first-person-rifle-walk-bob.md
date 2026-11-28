# In first person, does a rifle bob as the player walks, as a pistol does?

Priority: P2
Needs: a person at the keyboard, or timed movement keys; video of the first-person view;
any save outdoors on flat ground with the Varmint Rifle and the 9mm Pistol (the
Goodsprings start); the console for the crippled leg
Spec: #4682, [animation.md](../../spec-notes/animation.md) §9.5 the first-person rig;
[player-body.md](../../design/player-body.md) §1.4a

The first-person folder has movement clips for `mt` (unarmed), `1hp` and `1hm` only.
Standing, the `1hp` and `1hm` ones bob `Bip01 Translate` (the arms) and `Camera1st` (the
view); the `mt` ones move nothing but the root. Limping it is the other way: `hurt\mt*`
bob the arms, `hurt\1hp*` and `hurt\1hm*` don't [observed: animation.md §9.5]. Before
#4682, a rifle in first person walked on the `mt` clips, so with Weapon Inertia off it
did not bob at all, while a pistol bobbed from its own clips [observed: the motion trace,
#4682]. Ours since #4682: standing, a two-handed weapon plays the `1hp` clips (`2hm` the
`1hm` ones) before `mt` [hypothesis]; limping and sneaking are as before.

In the original, with no mods, in first person on flat ground, record each of these:
stand 2 s, walk forward 4 s, run forward 4 s, walk left 2 s, walk back 2 s.

1. The 9mm Pistol drawn.
2. The Varmint Rifle drawn.
3. Fists raised (unarmed), for the `mt` clips themselves.
4. The rifle and the pistol with a crippled leg (`player.damageav LeftMobilityCondition
   100`), walking forward only.

For each, report:

- whether the arms and weapon bob, and whether the view itself rises and falls;
- how often (one bob per footstep, or one per stride) and how much, compared with the
  pistol: the same, smaller or larger;
- whether the rifle's bob looks slower or heavier than the pistol's, beyond its size.

The lag behind a turn is not part of this check (Weapon Inertia, §1.4a).

Reading the answer: the rifle bobs as the pistol does → the stand-in (A) is right; it
bobs in another way → a bob made by code (B), to be measured from the video; it does not
bob → the original walks a rifle on `mt` (C), and #4682 goes back to the owner.

## Partial answer, 2026-11-28: items 1, 2 and 4

[observed: two videos of the original by a contributor (xNVSE without plugins, no other
mods), 800×452, the Goodsprings road, about 7 minutes in all. The weapon's place on screen
is tracked frame by frame and the movement read from the ground's motion. Rates come from
counted step dips; sizes are in screen pixels.] Item 3 (fists) is not in them.

Steps a second, one dip of the weapon per footstep:

| | Varmint Rifle | 9mm Pistol |
|---|---|---|
| Standing | no step bob | – |
| Walking forward | 1.64 and 1.67 | 1.94 (42 dips); about 1.6 in a shorter, uneven stretch |
| Walking sideways | 1.85–1.86 | 1.88–2.06 |
| Walking back | 2.00 | 2.18 |
| Running forward | about 2.56 | 2.81–2.91 |
| Left leg crippled, forward | 1.95–2.21 | 2.01–2.12 (a pistol: the 9mm or the 10mm) |

- Every move bobs both weapons, the rifle as the pistol: one dip per footstep and a sway
  side to side per stride. Standing, there's only a slow sway, once in about 1.9 s, which
  goes on under the steps.
- Sizes of the dip: walking forward 7–8 px for the rifle and about 10 for the pistol,
  running 11 and 14–15, sideways 4–5 for both. Pixels are not units: the tracked parts of
  the two weapons are at different distances from the camera.
- With the left leg crippled (`player.damageav LeftMobilityCondition 100`; the Pip-Boy
  shows it), both weapons still bob, one dip per footstep, with no long-short rhythm on
  screen. Ours limps a pistol on `hurt\1hp*_hurt.kf`, which moves nothing (animation.md
  §9.5), so ours keeps it still. That difference is older than #4682.
- Ours since #4682 (motion trace): walking forward, both 1.68 steps a second (87 units/s);
  running, the rifle 2.20 (309 units/s) and the pistol 2.38 (344). The rifle's walk
  matches the original's; both of our runs step slower than the original's.
- Reading so far: not (C). The rifle bobs as the pistol does, and walking at the rate the
  stand-in gives it, which fits (A). But the crippled pistol bobs although its limping
  clips don't, so the original's bob may not come from the files alone (B). Item 3
  decides: the `mt` clips carry no bob, so fists that bob mean a bob made by code.
