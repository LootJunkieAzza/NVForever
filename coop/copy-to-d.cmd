@echo off
rem Refresh the second machine's copy (D:\FNVForever\NVForever by default, or the folder given)
rem from this fork's newest build: the program, its schemas and the licence files; then the
rem mods: new or changed files in this PC's game Data folder go to the copy's game beside it
rem (..\Fallout New Vegas\Data), and this PC's NVForever load order (plugins.txt) and mods
rem folder go to its home. Its saves, settings and logs are left alone. Last, this PC's own
rem co-op install (C:\NVForever\NVForever-coop) gets the same program and schemas.
rem A copy whose drive is not plugged in is skipped; this PC's install is still updated.
rem Build first: cargo build --release --bin nvforever
setlocal
set "HERE=%~dp0"
set "REPO=%HERE%.."
set "TO=%~1"
if "%TO%"=="" set "TO=D:\FNVForever\NVForever"
set "GAME=C:\Modding\GOG-Base Installs\Fallout New Vegas\Data"
set "HOME_FROM=%USERPROFILE%\OneDrive\Documents\My Games\NVForever"
set "MO2_FROM=C:\NVForever"
set "HERE_PC=%MO2_FROM%\NVForever-coop"
if not exist "%REPO%\engine\target\release\nvforever.exe" (
  echo No build yet: run cargo build --release --bin nvforever in %REPO%\engine
  exit /b 1
)
tasklist /FI "IMAGENAME eq nvforever.exe" | "%SystemRoot%\System32\find.exe" /I "nvforever.exe" >nul && (
  echo NVForever is running: close it first, the program cannot be replaced while it runs.
  exit /b 1
)
for %%D in ("%TO%") do if not exist "%%~dD\" (
  echo %%~dD is not plugged in: skipped %TO%.
  goto here_pc
)
if not exist "%TO%" mkdir "%TO%"
copy /Y "%REPO%\engine\target\release\nvforever.exe" "%TO%\" >nul || exit /b 1
robocopy "%REPO%\schemas" "%TO%\schemas" /MIR /NFL /NDL /NJH /NJS /NP >nul
if errorlevel 8 (
  echo Copying the schemas failed.
  exit /b 1
)
copy /Y "%REPO%\LICENSE" "%TO%\" >nul
copy /Y "%REPO%\LEGAL.md" "%TO%\" >nul
if not exist "%TO%\vcruntime140.dll" copy /Y "%SystemRoot%\System32\vcruntime140.dll" "%TO%\" >nul
rem Mods: only files that are missing or differ are copied, nothing is deleted.
if exist "%GAME%\FalloutNV.esm" if exist "%TO%\..\Fallout New Vegas\Data" (
  robocopy "%GAME%" "%TO%\..\Fallout New Vegas\Data" /E /XO /NDL /NJH /NJS /NP
  if errorlevel 8 (
    echo Copying the game's mods failed.
    exit /b 1
  )
)
if not exist "%TO%\home" mkdir "%TO%\home"
if exist "%HOME_FROM%\plugins.txt" copy /Y "%HOME_FROM%\plugins.txt" "%TO%\home\" >nul
if exist "%HOME_FROM%\mods" robocopy "%HOME_FROM%\mods" "%TO%\home\mods" /E /NFL /NDL /NJH /NJS /NP >nul
rem Mod Organizer 2: the copy beside it (..\MO2) gets this PC's MO2 mods (new or changed
rem ones) and profiles (mod list, load order). Its ModOrganizer.ini, which points at the
rem copy's game, is left alone.
if exist "%MO2_FROM%\mods" if exist "%TO%\..\MO2\ModOrganizer.ini" (
  robocopy "%MO2_FROM%\mods" "%TO%\..\MO2\mods" /E /XO /NDL /NJH /NJS /NP
  if errorlevel 8 (
    echo Copying the MO2 mods failed.
    exit /b 1
  )
  robocopy "%MO2_FROM%\profiles" "%TO%\..\MO2\profiles" /E /NFL /NDL /NJH /NJS /NP >nul
)
echo Updated %TO% from this build.
:here_pc
rem This PC's own co-op install, which its MO2 starts as "NVForever co-op".
if exist "%HERE_PC%\nvforever.exe" (
  copy /Y "%REPO%\engine\target\release\nvforever.exe" "%HERE_PC%\" >nul || exit /b 1
  robocopy "%REPO%\schemas" "%HERE_PC%\schemas" /MIR /NFL /NDL /NJH /NJS /NP >nul
  if errorlevel 8 (
    echo Copying the schemas to %HERE_PC% failed.
    exit /b 1
  )
  echo Updated %HERE_PC% from this build.
)
