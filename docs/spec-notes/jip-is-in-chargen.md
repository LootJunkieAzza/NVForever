# JIP LN `IsInCharGen`

- [documented: https://geckwiki.com/index.php?title=IsInCharGen,
  https://geckwiki.com/index.php?title=SetInChargen]
  The GECK wiki describes `IsInCharGen` as taking no arguments and returning `1`
  if the player is currently in character generation, and `0` otherwise.
- [observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + JIP LN 57.30,
  2026-11-24, `GSDocMitchellHouse` (`0x00103DF9`), 10 official base masters,
  clean-room read-only console probes in `tools/capture/nvse/catalogs/wave4c_extenders.json`
  (#3679); no DLL or source consulted]:
  - `w4c_a02_set_in_chargen_menus` (read `IsInCharGen`, call `SetInCharGen 1`,
    re-read same-frame and next-tick, call `SetInCharGen 0`, open `ShowTraitMenu`):
    `flag(c0=0,s1=0,s1n=0,clr=0)|name(0:0)|race(0:0)|love(0:0)|barber(0:0)|trait(1084:1)`
    — `SetInCharGen 1` leaves `IsInCharGen` at `0` both same-frame and on the
    next tick, while `ShowTraitMenu` (`1084`) flips `IsInCharGen` to `1`.
  - `w4c_a02_chargen_all_menus` (open each menu individually via `*np_keepmenu 1`,
    read `GetActiveMenuMode` and `IsInCharGen`, then `CloseActiveMenu`):
    `name(1051:0)|race(1036:0)|love(1074:1)|barber(1036:0)|surg(1036:0)|trait(1084:1)`
    — `ShowLoveTesterMenu` (`1074`) and `ShowTraitMenu` (`1084`) return `1`,
    whereas `ShowNameMenu` (`1051`), `ShowRaceMenu` (`1036`), `ShowBarberMenu`
    (`1036`), and `ShowPlasticSurgeonMenu` (`1036`) all return `0`.
  - `w4c_a02b_set_in_chargen_xp` (`SetInCharGen 1` followed by `RewardXP 500`,
    checked across ticks before and after `SetInCharGen 0`):
    `lvl0=1,cg0=0,cg1=0|whileSet1(mm=0,cg=0)|afterSet0(mm=0,cg=0)` — `IsInCharGen`
    remained `0` throughout.
  - `w4d_01_tagskills_chargen` (`tools/capture/nvse/catalogs/wave4d_extenders.json`,
    #3679; open TagSkills via `SetTagSkills 3` with `*np_keepmenu 1`, read
    `GetActiveMenuMode` and `IsInCharGen`): `mm=1048,cg=1` — `SetTagSkills 3`
    opens `CharGenMenu` / TagSkills mode (`1048`), and `IsInCharGen` returns `1`.
- [design] `Game::is_in_chargen` (`crates/fnv-game/src/nvse_jip2.rs`) returns
  `true` when `menus::TAG_SKILLS` (`1048`), `menus::LOVE_TESTER` (`1074`), or
  `menus::TRAITS` (`1084`) is open, and leaves vanilla `SetInCharGen`
  (`PlayerState::in_chargen`) untouched for vanilla XP/level-up suppression.
