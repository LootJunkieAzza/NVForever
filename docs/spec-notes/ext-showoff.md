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

## 5. The 2026-10-30 batch (#1010, `nvse_showoff2.rs`)

Checked in the original game with the probe set `showoff2` (`tools/capture/jip_probes.py`,
lab `L5.ext.showoff2`): 49 of 50 rows match.

| Function | NVForever |
|---|---|
| GetEquippedWeapon | the equipped weapon's base, Fists (000001F4) when none [documented] |
| IsEquippedWeaponMelee, IsEquippedWeaponRanged, IsWeaponMelee `[weapon]` | melee is a weapon type (DNAM animation type) of 2 or below [documented: "type is <= 2"]; with no weapon, melee 1 and ranged 0 [documented] |
| GetItemCanHaveHealth `[item]` | a weapon or armor [documented] |
| IsLimbCrippled `[limb] [threshold]`, GetCrippledLimbsAsBitMask `[threshold]`, GetNumCrippledLimbs `[threshold]` | a limb's condition at or below the threshold (0); limbs head 0 … brain 6, bits head 1 … brain 64; -1 or none any limb [documented; observed: DamageAV 200 on the left leg gives 16, 1, 1] |
| GetActorValueDamage (GetAVDamage) `av` | the damage, no more than the value had [observed: DamageAV 200 on a limb of 100 reads 100] |
| GetActorValueName (GetAVName) `code` | the script name: 1 Confidence, 25 PerceptionCondition, 76 DamageThreshold, no spaces [observed]. The game compiles the code only in parentheses, as the page's example has it |
| GetAmmoName `[type] [ammo]`, SetAmmoName `name [type] [ammo]` | 0 FULL, 1 ONAM (the short name), 2 QNAM (the abbreviation) [documented: the page; xEdit AMMO]; a name the record lacks reads "<no name>" [observed: 10mm's abbreviation] |
| GetIngestibleConsumeSound `[aid]`, SetIngestibleConsumeSound `sound [aid]` | ENIT's consume sound [observed: the stimpak's 000296B9]; the setter true when set [documented; the game did not compile it in a `let`, so it is not observed] |
| GetCreatureFootWeight, GetCreatureTurningSpeed `[creature]` | WNAM, TNAM; -1 for no creature [documented; observed: the dog 1 and 90, Doc -1] |
| SetCreatureFootWeight, SetCreatureTurningSpeed, SetCreatureBaseScale, SetCreatureReach `value [creature]` | WNAM, TNAM, BNAM, RNAM for the session; refused below 0; 1 when set [documented; observed: turning speed 45 read back, -1 refused] |
| GetCellEncounterZone (GetCellZone) `cell` | the cell's zone [documented; observed: Doc's house none] |
| GetZoneRespawns `zone` | not "Never Resets" [documented; observed: Vault 11 0, Hoover Dam 1] |
| IsBaseForm `form`, IsReferenceCloned, IsActorAlt `[form]` | a record that is no placed reference; a reference made in play (0xFF); an NPC or creature [documented; observed: the pistol 1, the player and Doc's reference 0] |
| ModNumericGameSetting `setting by` | the setting changed by the amount; 0 for no such setting [documented; observed: fMoveRunMult 4 + 1 = 5] |
| GetShowOffDebugMode, SetShowOffDebugMode | the switch kept; nothing more is printed |

The one row that differs is not ShowOff's: a `string_var` given to SetAmmoName's name
reached it as the variable's name, "sText", in the game [observed: showoff2
ammoname.restore], as SetUIStringAlt's `%z` did in gaps1. The game's runtime compiler
seems to pass a string variable's name where a parameter takes literal text [hypothesis];
NVForever passes the value. A third case: SetWeaponWorldModelPath given a `string_var` set
the path to the variable's name, "sSave" [observed: jg1, before the step was changed to a
literal].
