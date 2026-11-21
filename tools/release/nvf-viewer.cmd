@echo off
rem Runs the asset viewer with the NIF schema shipped beside it.
set "HERE=%~dp0"
"%HERE%nvf-viewer.exe" --schema "%HERE%schemas\nif" %*
