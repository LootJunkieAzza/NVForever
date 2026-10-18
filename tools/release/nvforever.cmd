@echo off
rem Runs NVForever with the schemas shipped beside it (nvforever.exe also finds them itself
rem when double-clicked). With no arguments it opens on the main menu. Game data: asked for at
rem the first start and kept in Documents\My Games\NVForever\NVForever.ini; --data "..." or
rem NVFOREVER_DATA overrides it.
rem Examples: nvforever.cmd                         (the main menu)
rem           nvforever.cmd --play --cell Primm     (playing, straight in Primm)
rem           nvforever.cmd --explore               (the free-camera explorer)
set "HERE=%~dp0"
if not defined NVF_PLAY_SCRIPT_FUNCTIONS set "NVF_PLAY_SCRIPT_FUNCTIONS=%HERE%schemas\script\functions.json"
"%HERE%nvforever.exe" --esm-schema "%HERE%schemas\fnv" --nif-schema "%HERE%schemas\nif" %*
