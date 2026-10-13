@echo off
rem Runs NVForever with the schemas shipped beside it. Game data: set NVFOREVER_DATA to your
rem Fallout New Vegas Data folder (read-only), or pass --data "...\Data".
rem Example: nvforever.cmd --play --cell Goodsprings
set "HERE=%~dp0"
if not defined NVF_PLAY_SCRIPT_FUNCTIONS set "NVF_PLAY_SCRIPT_FUNCTIONS=%HERE%schemas\script\functions.json"
"%HERE%nvforever.exe" --esm-schema "%HERE%schemas\fnv" --nif-schema "%HERE%schemas\nif" %*
