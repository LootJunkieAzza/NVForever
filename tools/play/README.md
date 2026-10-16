# tools/play: run the game at a place

PowerShell scripts for playing NVForever at named places and scenarios, for playtests.
They run in Windows PowerShell 5.1 and in PowerShell 7.

```powershell
.\tools\play\Play.ps1 -List                       # every place and what it is for
.\tools\play\Play.ps1                             # Goodsprings at noon, playing
.\tools\play\Play.ps1 NewGame                     # the opening in Doc Mitchell's house
.\tools\play\Play.ps1 Saloon -NoVoice
.\tools\play\Play.ps1 Primm -Hour 21 -Graphics enhanced
.\tools\play\Play.ps1 Battle -God                 # NCR troopers against Cook-Cook's Fiends
.\tools\play\Play.ps1 GoodspringsAerial           # free camera above Goodsprings
.\tools\play\Play.ps1 Novac -FreeCam              # any place with the free camera
.\tools\play\Play.ps1 Goodsprings -Build          # build nvforever (release) first
.\tools\play\Play.ps1 Goodsprings -Extra '--time-scale','1.2','--first-person-body'
```

- `-Place` tab-completes. Places with a position come from the smoke scenarios
  (`tools/smoke/scenarios.toml`) and the playtest notes; the named exterior cells (Primm,
  Novac, Camp McCarran …) start above the cell's centre, so the player may land on a roof.
- Playing (`--play`) is the default; `-FreeCam` flies the free camera instead. The add-on
  packs' start-up message boxes are closed at once (`--no-messages`); `-Messages` keeps them.
- `-God` sets the test aid `NVF_PLAY_TEST_GOD=1` (no damage to the player); places such as
  `Battle` set their own test aids. Test aids are set for that run only.
- The game data comes from `-Data`, `NVFOREVER_DATA` or the Steam install; the game from
  `-Exe`, `NVF_EXE` or `engine\target\release\nvforever.exe`. The schema folders and the
  script function catalogue are always taken from this checkout, so a build made in another
  worktree still runs.
- The main menu: run nvforever with `--play --menu` (no place): Continue, New Game, Load
  Game, Settings, Quit; pass it with `-Extra '--menu'` here.
- The log goes to the console and to `Documents\My Games\NVForever\logs` (crash reports too).

To add a place, add a line to the `$places` table in `Play.ps1` and its name to the
`ValidateSet` above it.

If PowerShell refuses to run scripts, allow local ones once with
`Set-ExecutionPolicy -Scope CurrentUser RemoteSigned`.
