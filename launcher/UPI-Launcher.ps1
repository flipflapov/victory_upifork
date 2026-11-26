# UPI Launcher: starts the title with this fork's settings for an NVIDIA card
# of 12 GB and a PC of 16 GB of RAM. Setup.cmd installs it beside victory.exe
# in %LOCALAPPDATA%\UPI Launcher; its shortcuts pass -SeriesX or -Safe.
#   (none)    Series S, codec-d/cool_fox's profile as its launcher runs Series
#             S, the GTX 1660 settings: the title's own lower settings and
#             about 7.6 GB of title memory instead of 13.5
#   -SeriesX  the Series X profile, with the renderer ports 1 and 2 as well
#             (made on Series X; on Series S the picture stayed black: port 2
#             missed 931 descriptor records written by stores it does not
#             patch, title+0x18849c above all, and port 1 found the heap not
#             one stretch of a section)
#   -Safe     Series S without the newest threading
#             (the first thing to try when something is wrong)
# A VICTORY_* variable set before this script runs wins over its setting.
# Keep this file ASCII: Windows PowerShell 5.1 reads a script without a byte
# order mark in the system code page.
param([switch]$SeriesX, [switch]$Safe)
$ErrorActionPreference = 'Stop'

$here = $PSScriptRoot
$runtime = Join-Path $here 'victory.exe'
$config = Join-Path $here 'game-path.txt'
if (!(Test-Path -LiteralPath $runtime -PathType Leaf) -or !(Test-Path -LiteralPath $config -PathType Leaf)) {
    Write-Host "victory.exe or game-path.txt is missing in $here. Run Setup.cmd again."
    Read-Host 'Press Enter to close' | Out-Null
    exit 2
}
$game = ([IO.File]::ReadAllText($config)).Trim()
if (!(Test-Path -LiteralPath $game -PathType Leaf)) {
    Write-Host "The game executable is not where Setup saw it: $game. Run Setup.cmd again."
    Read-Host 'Press Enter to close' | Out-Null
    exit 2
}

# codec-d/cool_fox's launcher profile ("pareto-residency", RTX-tested), the
# three settings that took a GTX 1660 6 GB from ~280 to ~100 ms a frame
# (Impre-visible/victory-gtx1660), sized for 12 GB here, and caps for 16 GB.
$settings = [ordered]@{
    # cool_fox profile
    VICTORY_TEXTURE_NO_WAIT     = '1'
    VICTORY_RELAY               = '2'
    VICTORY_RELAY_COPY          = '1'
    VICTORY_WIDE_BACK           = '1'
    VICTORY_CARD_REHASH_RUNS    = '1'
    VICTORY_JOIN_COPIES         = '1'
    VICTORY_INDIRECT_WINDOW     = '1'
    VICTORY_INDIRECT_BATCH      = '2'
    VICTORY_CARD_STEPS          = '2'
    VICTORY_MAP_THREADS         = '1'
    VICTORY_CARD_AWAKE          = '1'
    VICTORY_SHOW_AFTER          = '1'
    VICTORY_AMBIENT             = '0.8'
    VICTORY_AMBIENT_AIR         = '1.5'
    VICTORY_THIN                = '1'
    VICTORY_JOIN_PARTS          = '8'
    VICTORY_UPLOAD_AHEAD        = '1'
    VICTORY_CARD_BUFFERS_MIN_KB = '64'
    # victory-gtx1660: buffer copies and memory windows in the card's memory
    # instead of the PC's, texture copies started levels down (2 on 12 GB;
    # 3 is faster and softer), textures not made level by level
    VICTORY_CARD_BUFFERS_MB     = '1024'
    VICTORY_CARD_WINDOWS_MB     = '768'
    VICTORY_TEXTURE_MIP_DROP    = '2'
    VICTORY_LEVELS_ON_DEMAND    = '0'
    # 16 GB of RAM: loose copies of title windows capped while RAM is short
    VICTORY_WINDOW_BUDGET_MB    = '1024'
    # the frame statistics line (fps, ms/frame) in the log
    VICTORY_LOG                 = 'info'
}
if ($Safe) {
    foreach ($k in 'VICTORY_THIN', 'VICTORY_JOIN_PARTS', 'VICTORY_UPLOAD_AHEAD') {
        $settings.Remove($k)
    }
}
if ($SeriesX -and !$Safe) {
    # avatarmanz/VIctory-7900XTX renderer ports 1 and 2 (byte-checked
    # against this executable; made on Series X): the title's 32-bit
    # graphics heap used in place, and its descriptor stores marking their
    # slots
    $settings['VICTORY_PORT_CONSTANTS'] = '1'
    $settings['VICTORY_PORT_DESCRIPTORS'] = '1'
}
foreach ($k in $settings.Keys) {
    if (!(Test-Path "Env:$k")) { Set-Item "Env:$k" $settings[$k] }
}
if (!$env:VICTORY_DATA_DIR) { $env:VICTORY_DATA_DIR = Join-Path $here 'state' }
if (!$env:VICTORY_CACHE_DIR) { $env:VICTORY_CACHE_DIR = Join-Path $here 'cache-coolfox' }
$console = if ($SeriesX) { 'series-x' } else { 'series-s' }

$logs = Join-Path $here 'logs'
[IO.Directory]::CreateDirectory($logs) | Out-Null
$log = Join-Path $logs ('run-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '.log')
[IO.File]::WriteAllText((Join-Path $logs 'latest.txt'), $log)
$header = @("Runtime $runtime", "Game $game", "console $console", "safe $([bool]$Safe)")
$header += Get-ChildItem Env:VICTORY_* | Sort-Object Name | ForEach-Object { "$($_.Name)=$($_.Value)" }
[IO.File]::WriteAllLines($log, [string[]]$header)

Write-Host "Starting the game ($console$(if ($Safe) { ', safe' })). The first start translates shaders and takes long."
Write-Host "Log: $log"
# Through cmd: the runtime's output goes to the log as it writes it, in one
# file, without PowerShell rewrapping its lines.
$cmd = Join-Path $env:SystemRoot 'System32\cmd.exe'
$line = "/c `"`"$runtime`" run `"$game`" --execute --console $console >> `"$log`" 2>&1`""
$process = Start-Process -FilePath $cmd -ArgumentList $line -WorkingDirectory $here -NoNewWindow -Wait -PassThru
Write-Host "The game ended (exit code $($process.ExitCode)). Log: $log"
if ($process.ExitCode -ne 0) {
    Read-Host 'Press Enter to close' | Out-Null
}
