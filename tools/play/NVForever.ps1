<#
.SYNOPSIS
    Shared launcher for nvforever (dot-sourced by Play.ps1; also usable on its own).

.DESCRIPTION
    Finds the game data and the nvforever build, optionally builds it first, sets the
    NVF_PLAY_TEST_* test aids for the run only, prints the command line and runs the game
    in this window (its log goes to the console and to %LOCALAPPDATA%\NVForever\logs).

    Data folder:  -Data, else $env:NVFOREVER_DATA, else the Steam install.
    Executable:   -Exe, else $env:NVF_EXE, else <repo>\engine\target\release\nvforever.exe.

.EXAMPLE
    . .\tools\play\NVForever.ps1
    Start-NVForever -Arguments '--play','--cell','Goodsprings','--hour','12'
#>

$script:RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$script:SteamData = 'C:\Program Files (x86)\Steam\steamapps\common\Fallout New Vegas\Data'

function Resolve-NVFData {
    param([string]$Data)
    if ($Data) { return $Data }
    if ($env:NVFOREVER_DATA) { return $env:NVFOREVER_DATA }
    if (Test-Path $script:SteamData) { return $script:SteamData }
    throw "Game Data folder not found: pass -Data or set NVFOREVER_DATA."
}

function Resolve-NVFExe {
    param([string]$Exe)
    if ($Exe) { return $Exe }
    if ($env:NVF_EXE) { return $env:NVF_EXE }
    return (Join-Path $script:RepoRoot 'engine\target\release\nvforever.exe')
}

function Build-NVForever {
    Push-Location (Join-Path $script:RepoRoot 'engine')
    try {
        Write-Host 'Building nvforever (release)...' -ForegroundColor Cyan
        cargo build --release -p nvforever
        if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
    } finally {
        Pop-Location
    }
}

function Start-NVForever {
    param(
        [string[]]$Arguments = @(),
        # Test aids for this run only, e.g. @{ NVF_PLAY_TEST_GOD = '1' }.
        [hashtable]$Environment = @{},
        [string]$Data,
        [string]$Exe,
        [switch]$Build
    )
    if ($Build) { Build-NVForever }
    $exePath = Resolve-NVFExe $Exe
    if (-not (Test-Path $exePath)) {
        throw "nvforever not found at $exePath (build it with -Build, or pass -Exe)."
    }
    $dataPath = Resolve-NVFData $Data
    # The schema folders are passed explicitly: a build remembers the folder it was built
    # in, which may be a worktree that no longer exists.
    $schemas = Join-Path $script:RepoRoot 'schemas'
    $all = @('--data', $dataPath, '--esm-schema', (Join-Path $schemas 'fnv'), '--nif-schema', (Join-Path $schemas 'nif')) + $Arguments
    # The script function catalogue the same way (scripts and the residents' AI read it).
    $Environment = $Environment.Clone()
    if (-not $Environment.ContainsKey('NVF_PLAY_SCRIPT_FUNCTIONS') -and -not $env:NVF_PLAY_SCRIPT_FUNCTIONS) {
        $Environment['NVF_PLAY_SCRIPT_FUNCTIONS'] = Join-Path $schemas 'script\functions.json'
    }

    $saved = @{}
    foreach ($k in $Environment.Keys) {
        $saved[$k] = [Environment]::GetEnvironmentVariable($k, 'Process')
        [Environment]::SetEnvironmentVariable($k, [string]$Environment[$k], 'Process')
    }
    try {
        foreach ($k in $Environment.Keys) { Write-Host ("  {0}={1}" -f $k, $Environment[$k]) -ForegroundColor DarkGray }
        Write-Host ("> {0} {1}" -f $exePath, ($all -join ' ')) -ForegroundColor Cyan
        & $exePath @all
        if ($LASTEXITCODE -ne 0) { Write-Host "nvforever exited with code $LASTEXITCODE" -ForegroundColor Yellow }
    } finally {
        foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k], 'Process') }
    }
}
