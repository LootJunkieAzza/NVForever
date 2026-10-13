<#
.SYNOPSIS
    Run NVForever at a named place or scenario.

.DESCRIPTION
    Each place is a set of nvforever arguments (and sometimes test aids) taken from the
    smoke scenarios and the playtest notes. Places play as the player (--play) unless
    -FreeCam is given or the place is a free-camera view. The add-on packs' start-up
    message boxes are closed at once (--no-messages) unless -Messages is given.

    .\tools\play\Play.ps1 -List            every place with what it is for
    .\tools\play\Play.ps1 Saloon           the Prospector Saloon
    .\tools\play\Play.ps1 Primm -Hour 21 -Graphics enhanced
    .\tools\play\Play.ps1 Battle -God      NCR troopers against Cook-Cook's Fiends
    .\tools\play\Play.ps1 Goodsprings -Build -Extra '--time-scale','1.2'

.PARAMETER Place
    The place or scenario (tab completes). Default: Goodsprings.
.PARAMETER Hour
    Time of day (0-24); overrides the place's own hour.
.PARAMETER FreeCam
    Fly the free camera instead of playing (no --play).
.PARAMETER Graphics
    vanilla (default) or enhanced.
.PARAMETER God
    The player takes no damage (NVF_PLAY_TEST_GOD=1).
.PARAMETER NoVoice
    No dialogue voice (lines still last as long as their sound).
.PARAMETER Messages
    Keep the start-up message boxes (they are closed at once by default).
.PARAMETER Size
    Window size, e.g. 1920x1080.
.PARAMETER Extra
    More nvforever arguments, passed as they are.
.PARAMETER Build
    Build nvforever (release) first.
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateSet(
        'NewGame', 'Goodsprings', 'GoodspringsAerial', 'Saloon', 'GeneralStore', 'GasStation',
        'DocMitchell', 'SunnysHouse', 'BighornerPen', 'CookCookCamp', 'FiendViolet', 'Battle',
        'GeckoHunters', 'Coyotes', 'TribalVillage', 'CampfireSmoke', 'GoodspringsSource',
        'Primm', 'PrimmNCRBase', 'BisonSteve', 'Nipton', 'MojaveOutpost', 'Sloan',
        'QuarryJunction', 'Novac', 'NovacRoom', 'HeliosOne', 'BlackMountain', 'BoulderCity',
        'TradingPost188', 'CampGolf', 'CampMcCarran', 'CrimsonCaravan', 'Westside',
        'Jacobstown', 'RedRockCanyon', 'HooverDam', 'CottonwoodCove', 'BitterSprings',
        'FlyOver'
    )]
    [string]$Place = 'Goodsprings',
    [switch]$List,
    [double]$Hour = -1,
    [switch]$FreeCam,
    [ValidateSet('vanilla', 'enhanced')]
    [string]$Graphics,
    [switch]$God,
    [switch]$NoVoice,
    [switch]$Messages,
    [string]$Size,
    [string[]]$Extra = @(),
    [switch]$Build,
    [string]$Data,
    [string]$Exe
)

. (Join-Path $PSScriptRoot 'NVForever.ps1')

# Args: nvforever arguments. Env: test aids. Cam: a free-camera view (no --play).
# Positions are game units (x,y,z), from the smoke scenarios (tools/smoke/scenarios.toml)
# and the playtest notes; named exterior cells start above the cell's centre.
$places = [ordered]@{
    NewGame           = @{ Note = 'New game: the opening in Doc Mitchell''s house'; Args = @('--new-game') }
    Goodsprings       = @{ Note = 'Goodsprings at noon'; Args = @('--cell', 'Goodsprings', '--hour', '12') }
    GoodspringsAerial = @{ Note = 'Goodsprings from above (free camera)'; Cam = $true; Args = @('--cell', 'Goodsprings', '--pos=-69500,2500,13400', '--pitch=-89.5', '--hour', '12') }
    Saloon            = @{ Note = 'Prospector Saloon (Sunny, Trudy, the jukebox)'; Args = @('--cell', 'GSProspectorSaloonInterior') }
    GeneralStore      = @{ Note = 'Chet''s general store (barter, his broom)'; Args = @('--cell', 'GSGeneralStore', '--hour', '12') }
    GasStation        = @{ Note = 'Goodsprings gas station (Ringo)'; Args = @('--cell', 'GSGasStation', '--hour', '12') }
    DocMitchell       = @{ Note = 'Doc Mitchell''s house (chemistry set, broken SMG)'; Args = @('--cell', 'GSDocMitchellHouse') }
    SunnysHouse       = @{ Note = 'Sunny''s house (items, fridge)'; Args = @('--cell', 'GSHouseInterior03') }
    BighornerPen      = @{ Note = 'The bighorner pen in Goodsprings'; Args = @('--cell', 'Goodsprings', '--pos=-69850,5700,8650', '--hour', '12') }
    CookCookCamp      = @{ Note = 'Cook-Cook''s Fiend camp'; Args = @('--cell', 'Goodsprings', '--pos=-43200,73000,7000', '--hour', '12') }
    FiendViolet       = @{ Note = 'Violet''s Fiend camp (sniper fights, cover)'; Args = @('--cell', 'Goodsprings', '--pos=-43900,64896,7000', '--hour', '12') }
    Battle            = @{ Note = 'Six NCR troopers brought to Cook-Cook''s camp to fight the Fiends (nobody sees the player)'
                           Args = @('--cell', 'Goodsprings', '--pos=-43200,72000,7000', '--hour', '12')
                           Env = @{ NVF_PLAY_TEST_UNSEEN = '1'; NVF_PLAY_TEST_SCRIPT = '3:moveto:0x000F36F5:player:-400:600:0,3:moveto:0x000F36F6:player:-200:650:0,3:moveto:0x0015CE0A:player:0:600:0,3:moveto:0x0015CE08:player:200:650:0,3:moveto:0x000FCEB1:player:400:600:0,3:moveto:0x000FD572:player:600:550:0' } }
    GeckoHunters      = @{ Note = 'Young gecko hunters south of Goodsprings (aggro radius)'; Args = @('--pos=-65828,-23929,7313', '--hour', '17.7') }
    Coyotes           = @{ Note = 'A coyote pack near Goodsprings (warns, then attacks)'; Args = @('--pos=-59300,-753,9100', '--hour', '12') }
    TribalVillage     = @{ Note = 'The dead tribal village (corpses spawned dead)'; Args = @('--pos=-73500,29700,8000', '--heading', '300', '--hour', '12') }
    CampfireSmoke     = @{ Note = 'The camp fire smoke south of Goodsprings'; Args = @('--cell=-16,-5', '--pos=-61744,-17983,6483', '--heading', '80', '--pitch=-14', '--hour', '16') }
    GoodspringsSource = @{ Note = 'Goodsprings Source'; Args = @('--cell', 'GoodspringsSource') }
    Primm             = @{ Note = 'Primm'; Args = @('--cell', 'Primm') }
    PrimmNCRBase      = @{ Note = 'The NCR base outside Primm'; Args = @('--cell', 'PrimmNCRBase') }
    BisonSteve        = @{ Note = 'Bison Steve hotel, inside (the Easy terminal)'; Args = @('--cell', 'BisonSteve01') }
    Nipton            = @{ Note = 'Nipton'; Args = @('--cell', 'Nipton') }
    MojaveOutpost     = @{ Note = 'Mojave Outpost'; Args = @('--cell', 'MojaveOutpost') }
    Sloan             = @{ Note = 'Sloan'; Args = @('--cell', 'Sloan') }
    QuarryJunction    = @{ Note = 'Quarry Junction (deathclaws: try -God)'; Args = @('--cell', 'QuarryJunction') }
    Novac             = @{ Note = 'Novac'; Args = @('--cell', 'Novac') }
    NovacRoom         = @{ Note = 'The player''s room in the Novac motel (bed, sleep)'; Args = @('--cell', 'NovacPlayersRoom', '--hour', '22') }
    HeliosOne         = @{ Note = 'HELIOS One'; Args = @('--cell', 'HeliosOne') }
    BlackMountain     = @{ Note = 'Black Mountain'; Args = @('--cell', 'BlackMountain') }
    BoulderCity       = @{ Note = 'Boulder City'; Args = @('--cell', 'BoulderCity') }
    TradingPost188    = @{ Note = 'The 188 Trading Post'; Args = @('--cell', '188TradingPost') }
    CampGolf          = @{ Note = 'Camp Golf (the trooper that used to float)'; Args = @('--pos=29500,62000,3400', '--heading', '0', '--pitch=-10', '--hour', '12') }
    CampMcCarran      = @{ Note = 'Camp McCarran''s gate'; Args = @('--cell', 'CampMcCarranGate') }
    CrimsonCaravan    = @{ Note = 'Crimson Caravan Company'; Args = @('--cell', 'CrimsonCaravan') }
    Westside          = @{ Note = 'Westside'; Args = @('--cell', 'Westside') }
    Jacobstown        = @{ Note = 'Jacobstown'; Args = @('--cell', 'Jacobstown') }
    RedRockCanyon     = @{ Note = 'Red Rock Canyon'; Args = @('--cell', 'RedRockCanyon') }
    HooverDam         = @{ Note = 'Hoover Dam, west side'; Args = @('--cell', 'HooverDamExtWest') }
    CottonwoodCove    = @{ Note = 'Cottonwood Cove'; Args = @('--cell', 'CottonwoodCove') }
    BitterSprings     = @{ Note = 'Bitter Springs'; Args = @('--cell', 'BitterSprings') }
    FlyOver           = @{ Note = 'Fly north from Goodsprings (cell streaming, LOD; free camera)'; Cam = $true; Args = @('--cell', 'Goodsprings', '--hour', '12', '--auto-fly', '1500') }
}

if ($List) {
    foreach ($name in $places.Keys) { '{0,-18} {1}' -f $name, $places[$name].Note }
    return
}

$p = $places[$Place]
$a = @($p.Args)
$aids = @{}
if ($p.Env) { foreach ($k in $p.Env.Keys) { $aids[$k] = $p.Env[$k] } }

$play = -not ($FreeCam -or $p.Cam)
if ($play) {
    $a = @('--play') + $a
    if (-not $Messages) { $a += '--no-messages' }
}
if ($Hour -ge 0) {
    $i = [array]::IndexOf($a, '--hour')
    if ($i -ge 0) { $a[$i + 1] = "$Hour" } else { $a += @('--hour', "$Hour") }
}
if ($Graphics) { $a += @('--graphics', $Graphics) }
if ($NoVoice) { $a += '--no-voice' }
if ($Size) { $a += @('--size', $Size) }
if ($God) { $aids['NVF_PLAY_TEST_GOD'] = '1' }
$a += $Extra

Write-Host ("{0}: {1}" -f $Place, $p.Note) -ForegroundColor Green
Start-NVForever -Arguments $a -Environment $aids -Data $Data -Exe $Exe -Build:$Build
