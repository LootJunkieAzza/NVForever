# Real-time VATS (co-op)

Queue item 196 (owner, 2026-11-10): "replacing how vats works in co-op with this real time
vats system", with the archive of the mod RealtimeVatsNV 1.0.0.

## 1. Sources and what was not read

- **Read:** the mod's settings file `RealtimeVatsNV.toml` (every setting, with its comment).
  Every figure and rule tagged [documented: toml] below comes from it.
- **Not read:** the mod's DLL and PDB (GPLv3, no source in the archive), and its HUD file
  `RealtimeVatsNV.xml`. The behaviour is built from the settings file; the HUD panel is
  NVForever's own.
- Where the settings file leaves a detail open, the choice is tagged [hypothesis] (a guess
  at what the mod does) or [design] (ours, where the fork differs).

## 2. What it replaces

- Solo, V (`Control::Vats`) is NVForever's VATS mode (`nvf-play` `vats_mode`,
  docs/design/vats-mode.md): the clock slows to 0.05 while marks are made and to 0.5 while
  they fire. It is not the original's VATS [design: combat is not a parity target].
- Before this change VATS mode had no co-op rule at all: in a session V slowed the clock
  of the player's own window only (`script::time_multiplier`), while the other players and
  the host's world went on at full speed.
- Now, in a session (`RtVats::allowed`, set by the co-op link for the host and every
  guest), V is the lock's and VATS mode cannot be entered. Solo is unchanged. The two share
  only helpers (the hit chance, the Action Points, the limb's point), so keeping solo as it
  is costs nothing.

## 3. Lock-on

- V, or X (`iKeyLock`), locks on to a hostile; the same key lets go [documented: toml, X].
  - V always tries, and says why it cannot [design: the queue item asks that V start it].
  - X starts a lock only when a hostile can be locked now; otherwise it is still the
    fork's Weapon Bash, which is bound to X [design: the mod's X is free in the original].
- The target: a living actor drawn now, hostile to the player, seen from the eye, within
  `fMaxRange` 2500 units and `fMaxAngle` 45° of the view, with a hit chance of at least
  `fMinChanceToLock` 10 % [documented: toml].
  - Among several, the one nearest the view direction [hypothesis].
  - Hostile: fighting the player, in the host's hostile set (on a guest), or would attack
    the player: the compass's rule [design].
- C (`iKeyNext`) moves to the next target: the lockable ones ordered by their angle off the
  view, going round [hypothesis: the order]. `iKeyPrevious` (off by default) goes back.
- Sight: looked for every `fLosInterval` 0.2 s; a locked target out of sight for
  `fLosLostSeconds` 1.5 s is let go [documented: toml].
- A lock needs a gun in hand and out [hypothesis: the settings speak of shots only]. Melee,
  unarmed and thrown weapons are open (§9).
- The lock also ends when a menu opens, a conversation begins, the player dies, a script
  disables fighting, or the weapon is put away [design: as VATS mode].

## 4. Limbs and shots

- Z (`iKeyLimbNext`) cycles the aimed limb: torso, head, left arm, right arm, left leg,
  right leg [documented: toml, the order and the `iLimb` numbers 0, 1, 3, 5, 7, 10]; limbs
  the target's body part data does not have are skipped [hypothesis]. A new lock starts on
  `iLimb` (the torso), a chained or cycled one keeps the limb.
- The limb's part is the first body part of the target that lowers the limb's condition
  (Perception, Endurance, Left/Right Attack, Left/Right Mobility Condition) [design].
- A gun's shot while locked is rolled on the limb's VATS hit chance (vats-mode.md §17: the
  part's To Hit Chance, the range, the gun's VATS spread, the to-hit perks), no higher than
  `fMaxChance` 95 % [documented: toml]. A hit leaves straight at the limb; a miss goes wide
  of it (VATS mode's `miss_point`) [design].
- Such a shot has no spread but still kicks the view [hypothesis].
- No time stop, no slow motion [documented: toml, "real time"].

## 5. Action Points

- Holding the lock drains `fLockDrainPerSecond` 1.5 AP a second [documented: toml].
- Each shot costs the weapon's VATS attack cost times `fShotCostMult` 0.4 [documented:
  toml]. For an automatic weapon, whose VATS cost is for a burst, a round costs the burst's
  share [hypothesis].
- No Action Points come back while a lock is held (`bNoRegenWhileLocked`) [documented].
- The lock drops at 0 AP and needs `fMinApToLock` 5 AP to start [documented: toml].

## 6. Chaining and criticals

- When the locked target dies the lock moves to the nearest visible hostile within
  `fChainMaxRange` 1500 units and `fChainMaxAngle` 90° of the view, on the same limb
  [documented: toml; hypothesis: the limb is kept]. With none, the lock ends.
- Every hit of the player on the locked target charges the meter by `fCritChargePerHit`
  0.2 [documented: toml]; each projectile that lands counts [hypothesis].
- With a full meter F (`iKeyCrit`) arms the next hit: it deals `fCritDamageMult` 3 times
  its damage and counts as a critical hit (the weapon's critical effect applies)
  [documented: the multiplier; hypothesis: that it multiplies the damage dealt after armour
  and gives the critical effect]. The same shot's other projectiles landing that frame are
  critical too; the critical itself charges nothing [hypothesis].
- With `bNoNaturalCrits` the player's hits are critical no other way: no rolled critical
  and no sneak attack critical [documented: toml]. The sneak attack's damage multiplier
  stays [hypothesis]. This holds whenever real-time VATS is active, locked or not
  [hypothesis].
- The meter is kept between locks [hypothesis].

## 7. Keys and the HUD

- While locked, the game's own bindings of X, C, Z and F are held back (`bFreeGameKeys`):
  Weapon Bash, Ping, Grab and Toggle POV [documented: toml; the fork's two added].
- The HUD: a mark on the aimed limb, and beside it (`fPanelGap` 60 units) a panel with the
  target's name, the limb and the chance, the limb's condition meter and the critical meter
  ("F) CRITICAL" when full, "CRITICAL ARMED"), in the HUD's font and colour [design: our
  own panel; the mod highlights the limb with the game's VATS body part effect, which
  NVForever does not have].
- Sounds: the base game's `UIVATS*` records, as VATS mode [design].

## 8. Co-op

- Each player's lock is their own; the other players see nothing of it.
- A guest's lock-on shots and hits go to the host as its other shots do: the guest works
  out the hit and the damage (the critical's ×3 included) and the relayed hit carries what
  was dealt. No new message, no protocol change.
- Action Points and the critical meter are each player's own.

## 9. Settings

- `[RealtimeVats]` of NVForever.ini holds every value under the mod's names; a missing
  section is written with the defaults at start.
- The settings page: Co-op → "Real-time VATS in co-op" (`realtime-vats`, on).

## 10. Open

- Controller bindings (the lock's keys are keyboard only).
- Melee, unarmed and thrown weapons under a lock.
- The mod's limb highlight effect.
