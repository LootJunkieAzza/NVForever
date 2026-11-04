@echo off
rem Co-op prototype: host a game on UDP port 47960 (private fork). Extra arguments pass on,
rem e.g. host.cmd --cell Primm. Its own home, so host and guests on one PC keep apart.
set "HERE=%~dp0"
if not defined NVF_DATA set "NVF_DATA=C:\Modding\GOG-Base Installs\Fallout New Vegas\Data"
set "NVFOREVER_HOME=%HERE%homes\host"
set "NVF_PLAY_SCRIPT_FUNCTIONS=%HERE%..\schemas\script\functions.json"
rem Steam's DLL beside the build (co-op over Steam), from the build's copy of the SDK.
if not exist "%HERE%..\engine\target\release\steam_api64.dll" for /d %%B in ("%HERE%..\engine\target\release\build\steamworks-sys-*") do if exist "%%B\out\steam_api64.dll" copy /Y "%%B\out\steam_api64.dll" "%HERE%..\engine\target\release\" >nul
"%HERE%..\engine\target\release\nvforever.exe" --play --data "%NVF_DATA%" --host 47960 --coop-name Host %*
