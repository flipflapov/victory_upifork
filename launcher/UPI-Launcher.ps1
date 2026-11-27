# UPI Launcher: starts the title with this fork's settings for an NVIDIA card
# of 12 GB and a PC of 16 GB of RAM. Setup.cmd installs it beside victory.exe
# in %LOCALAPPDATA%\UPI Launcher; its shortcuts pass -Turbo, -SeriesX or -Safe.
#   (none)    Series S, codec-d/cool_fox's profile as its launcher runs Series
#             S, the GTX 1660 settings: the title's own lower settings and
#             about 7.6 GB of title memory instead of 13.5
#   -Turbo    the same with renderer ports of avatarmanz/VIctory-7900XTX:
#             1 (the title's graphics heap in place: faces are drawn with
#             it), 2 (its descriptor stores mark their slots) and 6 (the
#             write guards' lock shared), and 2 GB of buffer copies in the
#             card's memory instead of 1. Port 7 is left out: on 6 cores its
#             heaps' records kept the submitting thread waiting ~40 ms a
#             frame (run 2026-11-26 19:02); port 5 too: it translates every
#             shader again and was measured on AMD only. Also the title's
#             committed graphics memory used in place (VICTORY_INPLACE=1):
#             its windows were 3.1-3.6 GB of copies in the PC's 16 GB.
#   -SeriesX  the Series X profile, without the ports (13.5 GB of title
#             memory: slow on a PC of 16 GB, it pages)
#   -Safe     Series S without the newest threading
#             (the first thing to try when something is wrong)
#   -Diag     the diagnostics build (Setup.cmd -Diag makes it): a timeline
#             of 3 frames at 4 and at 5.5 minutes (logs\timeline-*.json and
#             .txt: the chain of work and waits each frame came out of) and
#             the memory lines by category
#   -NoPrep   leaves the PC as it is (by default, while the game runs: the
#             high performance power plan, Game DVR off, and programs in the
#             background at below normal CPU and low memory priority with
#             their RAM handed back; all undone when the game ends; after
#             neptune-turbo.ps1 of Dolberthi/neptune-dolberth-nvidia-8gb)
# A VICTORY_* variable set before this script runs wins over its setting.
# Keep this file ASCII: Windows PowerShell 5.1 reads a script without a byte
# order mark in the system code page.
param([switch]$Turbo, [switch]$SeriesX, [switch]$Safe, [switch]$NoPrep, [switch]$Diag)
$ErrorActionPreference = 'Stop'

$here = $PSScriptRoot
$runtime = Join-Path $here $(if ($Diag) { 'victory-diag.exe' } else { 'victory.exe' })
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
# settings that took a GTX 1660 6 GB from ~280 to ~100 ms a frame
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
    # victory-gtx1660: buffer copies in the card's memory instead of the
    # PC's, and what shaders read of memory windows (1536 MB: its advice for
    # 8 GB or more; the CPU's side of a window stays in the PC's memory),
    # texture copies started a level down
    # (2; 3 is softer and saved no time here: textures cost ~1 ms a frame),
    # textures not made level by level
    VICTORY_CARD_BUFFERS_MB     = '1024'
    VICTORY_CARD_WINDOWS_MB     = '1536'
    VICTORY_TEXTURE_MIP_DROP    = '2'
    VICTORY_LEVELS_ON_DEMAND    = '0'
    # 16 GB of RAM: loose copies of title windows capped while RAM is short
    VICTORY_WINDOW_BUDGET_MB    = '1024'
    # every ray a miss while the ray-tracing structure builder is off (after
    # edsuuu/VI-BR-PORT 7d7b9e49 and the passes after it): the ray-only
    # passes are left out, lights reach what they face (faces were black in
    # cutscenes). Part of the shader translation: set in every mode, or each
    # change of mode would translate every shader again.
    VICTORY_RT_MISS             = '1'
    # the title's output at 1920x1080 (the screen here), not 3840x2160: it
    # renders at 2/3 of its output, so 1280x720 instead of 2560x1440, four
    # times fewer pixels, and its 4K picture was scaled down to the window
    # anyway (run 2026-11-28 00:42: one water draw 31-41 ms of the card, the
    # card 130-160 ms a frame). As edsuuu/VI-BR-PORT and VIctory-NVIDIA run.
    VICTORY_OUTPUT              = '1920x1080'
    VICTORY_DISPLAY_MODE        = '1920x1080'
    # the frame statistics line (fps, ms/frame) and the memory lines in the
    # log (VICTORY_BLOBS_LOG=1 adds the shaders that do not translate and
    # what the pipeline methods 0x1c8 and 0x218 are given, as base64)
    VICTORY_LOG                 = 'info'
}
if ($Safe) {
    foreach ($k in 'VICTORY_THIN', 'VICTORY_JOIN_PARTS', 'VICTORY_UPLOAD_AHEAD') {
        $settings.Remove($k)
    }
}
if ($Turbo -and !$Safe) {
    # avatarmanz/VIctory-7900XTX renderer ports (byte-checked against this
    # executable; 45 ms a frame on Series S there with 5 and 7 as well).
    # Not port 1 (VICTORY_PORT_CONSTANTS=1) since run 2026-11-27 23:11: the
    # title's 185 MB constant heap read in place is read by the card across
    # the bus (PCIe 3.0 here) by nearly every draw: the card took 90-160 ms a
    # frame spread over ~1,000 of the title's pipelines. edsuuu/VI-BR-PORT
    # measured its copy in the card's memory faster on NVIDIA (23.0 -> 25.4
    # and 23.8 -> 27.7 fps). The faces it brought here are now every ray a
    # miss's (VICTORY_RT_MISS) and the bake's programs known by name.
    $settings['VICTORY_PORT_DESCRIPTORS'] = '1'
    $settings['VICTORY_PORT_GUARD'] = '1'
    # buffer copies in the card's memory, not the PC's 16 GB (they stop
    # going there while less than a fifth of the card's budget is free)
    $settings['VICTORY_CARD_BUFFERS_MB'] = '2048'
    # windows of the title's committed graphics memory (its vertex, index
    # and constant pools, ~3.9 GB) read by the card where the title has
    # them: no copy, no refresh, nothing sent up (stage 1 of crate::inplace;
    # their pages stay in RAM while in place, at most 4 GB)
    $settings['VICTORY_INPLACE'] = '1'
    # (Not VICTORY_WINDOW_KB=1024 with VICTORY_WINDOW_TAILS=1, Neptune's 16 GB
    # profile: in run 2026-11-26 22:58 the windows went from 628 to 2,000-3,000
    # and their copies from 2.6-3.2 to 4.5-5 GB, the same memory standing in
    # several windows, and the card's share of the PC's memory overflowed.)
    # Windows of title memory in the card's own memory, filled by the CPU
    # through the bus, with no copy in the PC's memory, where the PC and the
    # driver offer it (a resizable BAR); else as before. They stop going
    # there at 80 % of the card's budget.
    $settings['VICTORY_WINDOWS_BAR_MB'] = '2048'
    # A queue's wait for a fence counts as met once the line has given the
    # card its signal (the title's queues all go to the host's one queue, in
    # order), not only once the card ran it and the relay landed it: run
    # 2026-11-27 00:00 had every party waiting for another, the card half
    # busy and the title 330 ms a frame at presents.
    $settings['VICTORY_GPU_ORDERED_WAITS'] = '1'
    # Memory, from DoubleGhosty/cool_fox (Esteban Viera's switches): idle host
    # lists none took let go after 10 s (a D3D12 allocator keeps the most its
    # commands ever took; ~1,500 lists stand here), and at most 256 MB of send
    # buffers kept spare instead of 768 (they count against the card's share
    # of the PC's memory, 7.5 GB here)
    $settings['VICTORY_LIST_IDLE_TRIM_S'] = '10'
    # (64 since run 2026-11-27 23:11: the card's share of the PC's memory ran
    # out as the heist began, 7.6 of 7.5 GB)
    $settings['VICTORY_SEND_SPARE_MB'] = '64'
    # what changed on the CPU's side of copies in the card's memory goes up
    # on a copy queue of its own while the card still draws (run 2026-11-27
    # 22:25: the drawing queue spent 22-50 ms a frame bringing those copies
    # up to date, ~50 MB a frame across the bus, doing nothing else)
    $settings['VICTORY_UP_QUEUE'] = '1'
    # the CPU's sides of copies in the card's memory in plain memory of ours,
    # which can be paged out, not in buffers the card could read (it never
    # reads them with the sends from each submission's own memory): run
    # 2026-11-27 23:50 had 2.4 GB of them in the card's share of the PC's
    # memory, which ran out as the heist began (7.7 of 7.5 GB)
    $settings['VICTORY_CARD_PLAIN'] = '1'
    # (VICTORY_LAND_THREADS=6 was tried in run 2026-11-26 23:30: the landings
    # took as long as with the default 3, waiting on title memory that pages.)
    # (Neptune by Dolberth's smart copy-back, VICTORY_SMART_COPY_BACK=1, is
    # not set: in run 2026-11-26 22:10 it cut copying back from ~50 to ~7 ms
    # a frame and the frame rate not at all, and faces came out crumpled:
    # passes read what shaders wrote before it came back.)
}
foreach ($k in $settings.Keys) {
    if (!(Test-Path "Env:$k")) { Set-Item "Env:$k" $settings[$k] }
}
if (!$env:VICTORY_DATA_DIR) { $env:VICTORY_DATA_DIR = Join-Path $here 'state' }
if (!$env:VICTORY_CACHE_DIR) { $env:VICTORY_CACHE_DIR = Join-Path $here 'cache-coolfox' }
$console = if ($SeriesX) { 'series-x' } else { 'series-s' }
if ($Diag) {
    # Diagnostics: the timeline (Chrome trace and its text summary) and the
    # memory lines by category.
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $env:VICTORY_TIMELINE = '240+330,3,' + (Join-Path (Join-Path $here 'logs') "timeline-$stamp.json")
    $env:VICTORY_MEMORY_CATEGORIES = '1'
}
$mode = @($console)
if ($Turbo -and !$Safe) { $mode += 'turbo' }
if ($Safe) { $mode += 'safe' }
if ($Diag) { $mode += 'diag' }

# What Windows can commit (RAM and page file): the process commits 19-20 GB
# in the world. With 24 GB in all (a small page file) it had under 1 GB left
# and held texture uploads back (run 2026-11-26 21:17).
$commitGb = 0
try { $commitGb = [math]::Round((Get-CimInstance Win32_OperatingSystem).TotalVirtualMemorySize / 1MB) } catch { }
if ($commitGb -gt 0 -and $commitGb -lt 40) {
    Write-Warning "Windows can commit only $commitGb GB (RAM + page file). Set the page file on C: to 49152 MB: System, About, Advanced system settings, Performance, Advanced, Virtual memory. With less the game slows down and holds textures back."
}

# PC preparation while the game runs (after neptune-turbo.ps1 "Preparar" of
# Dolberthi/neptune-dolberth-nvidia-8gb, GPL-3.0-or-later). On 16 GB the
# game pages: what other programs hold in RAM is what it lacks.
function Start-UpiPrep {
    $ErrorActionPreference = 'Continue'
    if (-not ('UpiPrepNative' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class UpiPrepNative {
    [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool SetProcessInformation(IntPtr h, int cls, ref uint info, uint size);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool GetProcessInformation(IntPtr h, int cls, ref uint info, uint size);
    [DllImport("psapi.dll", SetLastError = true)] static extern bool EmptyWorkingSet(IntPtr h);
    const uint SET_INFORMATION = 0x0200, QUERY_LIMITED = 0x1000, SET_QUOTA = 0x0100, QUERY_INFORMATION = 0x0400;
    const int ProcessMemoryPriority = 0;
    public static int GetMemoryPriority(int pid) {
        IntPtr h = OpenProcess(QUERY_LIMITED, false, pid);
        if (h == IntPtr.Zero) return -1;
        try { uint p = 0; return GetProcessInformation(h, ProcessMemoryPriority, ref p, 4) ? (int)p : -1; }
        finally { CloseHandle(h); }
    }
    public static bool SetMemoryPriority(int pid, uint priority) {
        IntPtr h = OpenProcess(SET_INFORMATION, false, pid);
        if (h == IntPtr.Zero) return false;
        try { uint p = priority; return SetProcessInformation(h, ProcessMemoryPriority, ref p, 4); }
        finally { CloseHandle(h); }
    }
    public static bool Trim(int pid) {
        IntPtr h = OpenProcess(SET_QUOTA | QUERY_INFORMATION, false, pid);
        if (h == IntPtr.Zero) return false;
        try { return EmptyWorkingSet(h); }
        finally { CloseHandle(h); }
    }
}
'@
    }
    $state = @{ plan = $null; dvr = @(); procs = @(); trimmed = 0 }
    $freeBefore = [math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1KB)
    # Power: from Balanced or Power saver to Ultimate or High performance.
    $active = $null
    $line0 = (powercfg /getactivescheme) 2>$null | Select-Object -First 1
    if ($line0 -match '([0-9a-fA-F-]{36})') { $active = $Matches[1].ToLower() }
    $plans = (powercfg /list) 2>$null
    $target = @('e9a42b02-d5df-448d-aa00-03f14749eb61', '8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c') | Where-Object { $plans -match $_ } | Select-Object -First 1
    if ($target -and $active -and (@('381b4222-f694-41f0-9685-ff5bb260df2e', 'a1841308-3541-4fab-bc81-f71556f20b4a') -contains $active)) {
        powercfg /setactive $target 2>$null | Out-Null
        $state.plan = $active
    }
    # Game DVR's background recording off.
    foreach ($k in @(@('HKCU:\System\GameConfigStore', 'GameDVR_Enabled'),
            @('HKCU:\Software\Microsoft\Windows\CurrentVersion\GameDVR', 'AppCaptureEnabled'),
            @('HKCU:\Software\Microsoft\Windows\CurrentVersion\GameDVR', 'HistoricalCaptureEnabled'))) {
        $v = (Get-ItemProperty -Path $k[0] -Name $k[1] -ErrorAction SilentlyContinue).($k[1])
        if ($v -eq 1) {
            Set-ItemProperty -Path $k[0] -Name $k[1] -Value 0 -ErrorAction SilentlyContinue
            $state.dvr += , $k
        }
    }
    # Programs in the background: CPU below normal, memory priority low,
    # their working sets handed back. The system's, the desktop's, sound,
    # consoles and this launcher are left alone.
    $keep = @('idle', 'system', 'registry', 'smss', 'csrss', 'wininit', 'winlogon', 'services', 'lsass', 'lsaiso',
        'svchost', 'fontdrvhost', 'dwm', 'audiodg', 'explorer', 'sihost', 'ctfmon', 'taskhostw', 'runtimebroker',
        'shellexperiencehost', 'startmenuexperiencehost', 'searchapp', 'searchui', 'searchhost', 'textinputhost',
        'applicationframehost', 'lockapp', 'securityhealthsystray', 'securityhealthservice', 'msmpeng', 'conhost',
        'openconsole', 'windowsterminal', 'powershell', 'pwsh', 'cmd', 'victory', 'nvdisplay.container',
        'nvcontainer', 'wudfhost', 'dllhost', 'wmiprvse', 'spoolsv', 'memcompression')
    $session = (Get-Process -Id $PID).SessionId
    foreach ($p in Get-Process) {
        if ($p.Id -le 4 -or $p.Id -eq $PID -or $p.SessionId -ne $session -or $keep -contains $p.ProcessName.ToLower()) { continue }
        $class = $null
        try { $class = $p.PriorityClass } catch { continue }
        if ($null -eq $class) { continue }
        $mem = [UpiPrepNative]::GetMemoryPriority($p.Id)
        $start = $null
        try { $start = $p.StartTime.Ticks } catch { }
        $changed = $false
        if ($class -eq 'Normal' -or $class -eq 'AboveNormal') {
            try { $p.PriorityClass = 'BelowNormal'; $changed = $true } catch { }
        }
        if ($mem -gt 2 -and [UpiPrepNative]::SetMemoryPriority($p.Id, 2)) { $changed = $true }
        if ($changed) { $state.procs += [pscustomobject]@{ id = $p.Id; name = $p.ProcessName; start = $start; class = "$class"; mem = $mem } }
        if ([UpiPrepNative]::Trim($p.Id)) { $state.trimmed++ }
    }
    $freeAfter = [math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1KB)
    Write-Host ("PC prepared (-NoPrep: not): power plan {0}, Game DVR {1}, {2} background programs lowered, {3} gave their RAM back; RAM free {4} -> {5} MB" -f `
        $(if ($state.plan) { 'high performance' } else { 'as it was' }), $(if ($state.dvr.Count) { 'off' } else { 'as it was' }), $state.procs.Count, $state.trimmed, $freeBefore, $freeAfter)
    $state
}

# Undoes Start-UpiPrep.
function Stop-UpiPrep($state) {
    $ErrorActionPreference = 'Continue'
    if ($state.plan) { powercfg /setactive $state.plan 2>$null | Out-Null }
    foreach ($k in $state.dvr) { Set-ItemProperty -Path $k[0] -Name $k[1] -Value 1 -ErrorAction SilentlyContinue }
    foreach ($r in $state.procs) {
        $p = Get-Process -Id $r.id -ErrorAction SilentlyContinue
        if (-not $p -or $p.ProcessName -ne $r.name) { continue }
        try { if ($r.start -and $p.StartTime.Ticks -ne $r.start) { continue } } catch { }
        try { if ($p.PriorityClass -eq 'BelowNormal') { $p.PriorityClass = $r.class } } catch { }
        if ($r.mem -ge 1) { [void][UpiPrepNative]::SetMemoryPriority($r.id, [uint32]$r.mem) }
    }
    Write-Host "PC preparation undone."
}

$logs = Join-Path $here 'logs'
[IO.Directory]::CreateDirectory($logs) | Out-Null
$log = Join-Path $logs ('run-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + ($mode -join '-') + '.log')
[IO.File]::WriteAllText((Join-Path $logs 'latest.txt'), $log)
$header = @("Runtime $runtime", "Game $game", "console $console", "mode $($mode -join ', ')", "commit limit $commitGb GB")
$header += Get-ChildItem Env:VICTORY_* | Sort-Object Name | ForEach-Object { "$($_.Name)=$($_.Value)" }
[IO.File]::WriteAllLines($log, [string[]]$header)

Write-Host "Starting the game ($($mode -join ', ')). The first start translates shaders and takes long."
Write-Host "Log: $log"
# Through cmd: the runtime's output goes to the log as it writes it, in one
# file, without PowerShell rewrapping its lines.
$cmd = Join-Path $env:SystemRoot 'System32\cmd.exe'
$line = "/c `"`"$runtime`" run `"$game`" --execute --console $console >> `"$log`" 2>&1`""
$prep = $null
if (!$NoPrep) {
    try { $prep = Start-UpiPrep } catch { Write-Warning "PC preparation skipped: $($_.Exception.Message)" }
}
try {
    $process = Start-Process -FilePath $cmd -ArgumentList $line -WorkingDirectory $here -NoNewWindow -Wait -PassThru
} finally {
    if ($prep) { try { Stop-UpiPrep $prep } catch { Write-Warning "PC preparation not fully undone: $($_.Exception.Message)" } }
}
Write-Host "The game ended (exit code $($process.ExitCode)). Log: $log"
if ($process.ExitCode -ne 0) {
    Read-Host 'Press Enter to close' | Out-Null
}
