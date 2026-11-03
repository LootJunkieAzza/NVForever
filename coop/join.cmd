@echo off
rem Co-op prototype: join a host (private fork). join.cmd [address] [name] [more arguments]
rem The address defaults to this PC (127.0.0.1:47960); each name gets its own home.
rem More arguments pass on as typed, e.g. join.cmd 192.168.1.20 Sam --window-at 960,0
setlocal
set "HERE=%~dp0"
set "ADDR=%~1"
if "%ADDR%"=="" set "ADDR=127.0.0.1:47960"
set "NAME=%~2"
if "%NAME%"=="" set "NAME=Guest"
rem The rest of the line as typed (%3 and on would split "960,0" at the comma).
set "REST="
if not "%~2"=="" (
  set "REST=%*"
  call set "REST=%%REST:*%~1=%%"
)
if not "%~2"=="" call set "REST=%%REST:*%~2=%%"
if not defined NVF_DATA set "NVF_DATA=C:\Modding\GOG-Base Installs\Fallout New Vegas\Data"
set "NVFOREVER_HOME=%HERE%homes\%NAME%"
set "NVF_PLAY_SCRIPT_FUNCTIONS=%HERE%..\schemas\script\functions.json"
"%HERE%..\engine\target\release\nvforever.exe" --play --data "%NVF_DATA%" --join %ADDR% --coop-name %NAME% %REST%
