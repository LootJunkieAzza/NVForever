# ShowOff events, explosion damage, aux timer stops and the jam animation

Plain-language notes for `crates/fnv-game/src/nvse_showoff.rs`. Sources: the GECK wiki
pages of each function [documented] and the showoff1 probe set run in the original game on
2026-10-27 [observed] (`tools/capture/jip_probes.py`; goldens stay private). The pages of
the ShowOff events themselves are not in the wiki dump, so their arguments come only from
showoff1: each event had one handler taking no arguments and one taking two refs, and both
ran.

## 1. ShowOff:OnAdd and GetAddedItemRefShowOff

- The event fires when an item reaches a container. With a stimpak AddItem'd to the player,
  a two-argument handler got the item's base form (type 47) first and the player second; a
  handler taking no arguments ran as well [observed].
- Inside the handler, `GetAddedItemRefShowOff` gives a reference whose base is the stimpak
  [observed: IsReference 1, base type 47]; the page says a world item reference or an
  inventory reference [documented]. NVForever gives an inventory reference to the item in
  the container. Outside a handler it gives none.
- NVForever fires it wherever its script OnAdd block would run for an item reaching a
  container (AddItem and its variants, RemoveAllItems into a container, picking up).

## 2. ShowOff:OnExplosionHit and the explosion's damage

- The event fires when an explosion's blast reaches an actor, before the damage lands. A
  MineFragExplosion (FalloutNV.esm 07078B) placed at the player gave a two-argument handler
  the player first and none second [observed].
- `GetExplosionHitDamage` read 100 inside the handler; after `SetExplosionHitDamage 5` the
  player lost 5 Health [observed]. Both are unusable outside the event [documented]; outside
  it NVForever reads 0 and ignores a set.
- NVForever asks for the damage in the engine's blast loop, before armour is taken off, and
  deals what the handlers leave [hypothesis: whether the original's value is before or after
  armour is not measured; the player in nvfairport took exactly the set value].

## 3. SetOnAuxTimerStopHandler

- `SetOnAuxTimerStopHandler 1/0 handler timerName [owner]` adds or removes a handler for an
  aux timer of an owner, the player when none is given; the handler gets the timer's name
  and its owner [documented].
- It runs when the timer runs out: a 0.1 second timer had run out within 30 frames, its time
  left read -1 (gone) and the handler had run once, with the player as owner [observed]. It
  runs again on `AuxTimerStop name 1` [observed]; AuxTimerStop fires the event only when its
  fireEvent argument is non-0 [documented].
- NVForever checks the running timers each frame and erases and reports the ones whose time
  is up. Before this, a timer whose time was up stayed with 0 seconds left.

## 4. ForceWeaponJamAnim

- It returned 0 with the 10mm pistol away and 1 with it drawn [observed]. IsAnimPlayingEx 4
  (reload) never read 1 over the 60 frames after [observed], so the jam is not a reload as
  that function counts them.
- A weapon without a jam animation gives 0 [documented: the .357 revolver cannot jam].
  NVForever does not read which weapons have one yet, so any drawn weapon gives 1
  [hypothesis], and the engine does not play the animation [not built].
