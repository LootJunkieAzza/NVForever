# Spec note: the new-game start

NVForever's own reading of two public settings. This is not a claim copied from another
program, and no game file was opened for it.

## Quest

A new game's start quest, when `sCharGenQuest` is not set to something else, is form
`00102037`.

- [documented: Fallout Wiki, "Fallout: New Vegas quest IDs"] The row `00102037` is named
  "Welcome To Fabulous New Vegas". The stage table lists the same id with GECK ID `VCG00`,
  including the opening stages (hood removed, pistol whip).
- [documented: Fallout Wiki, "VCG00ScriptNV"] The script's editor ID starts quest `VCG00`.
  The script's own form id is `00102035`, which is not the quest.
- [documented: GECK Wiki, "INI Settings"] `SCharGenQuest` default value is `00102037`.
- [documented: Fallout Wiki quest list] `00104C1C` is "Ain't That a Kick in the Head"
  (`VCG01`), the quest after the opening, not the opening itself.
- [documented: `engine/nvf-play/src/game_start.rs`] `001FFFF8` is a mod start. A base-game
  load order has no quest at that id. Without the key, NVForever starts `VCG00`.

## Film

The intro a new game plays is `Data\Video\FNVIntro.bik`.

- [documented: Fallout Wiki, "Fallout: New Vegas intro"] The opening cinematic plays before
  the first playable part. That page does not name the file.
- [documented: public mod pages that replace the new-game intro] They name the file
  `Data\Video\FNVIntro.bik`.
- [documented: GECK Wiki, "INI Settings"] `SIntroMovie` is listed as
  `Fallout INTRO Vsk.bik`. That is Fallout 3's film. New Vegas does not ship it
  [documented: `engine/nvf-play/src/game_start.rs`].
- [documented: `engine/nvf-play/src/game_start.rs`] With no `sIntroMovie` line, NVForever
  plays `video\fnvintro.bik`. An empty `sIntroMovie=` plays no film.

## What Clear writes

Clear sets the fields to `00102037` and `FNVIntro.bik`, with "No intro film" off. Save
writes those two lines into the home's ini. A missing key still means the same opening,
so the written lines match a fresh game [documented: `GameStart::parse` of those values
is quest `00102037` and film `video\fnvintro.bik`].
