[CmdletBinding()]
param(
    [string] $GamePath,
    [string] $InstallDir = (Join-Path $env:LOCALAPPDATA 'UPI Launcher'),
    [string] $ShortcutDirectory = [Environment]::GetFolderPath('Programs')
)

$ErrorActionPreference = 'Stop'
# The launcher's name (Cyrillic "UPI Launcher") from code points: Windows
# PowerShell 5.1 reads a script without a byte order mark in the system code
# page, which would garble Cyrillic letters written here. Keep this file ASCII.
$AppName = -join [char[]](0x042E, 0x041F, 0x0418, 0x20, 0x041B, 0x0430, 0x0443, 0x043D, 0x0447, 0x0435, 0x0440)

if ($env:OS -ne 'Windows_NT' -or ![Environment]::Is64BitOperatingSystem) {
    throw "$AppName requires 64-bit Windows."
}
if (!(Get-Command rustup -ErrorAction SilentlyContinue) -or !(Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw 'Install Rust for Windows from https://rustup.rs, then reopen Setup.cmd.'
}

$InstallDir = [IO.Path]::GetFullPath($InstallDir)
for ($parent = [IO.DirectoryInfo]::new($InstallDir); $parent; $parent = $parent.Parent) {
    if (Test-Path -LiteralPath (Join-Path $parent.FullName '.victory-root')) {
        throw 'Choose an install directory outside the source checkout so the launcher can write saves and shader caches.'
    }
}

# An install made before the rename (DexNom Runtime) moves here whole:
# saves (state), shader caches, logs and the saved game path.
$oldInstall = Join-Path $env:LOCALAPPDATA 'DexNom Runtime'
if (!$PSBoundParameters.ContainsKey('InstallDir') -and
    (Test-Path -LiteralPath $oldInstall -PathType Container) -and
    !(Test-Path -LiteralPath $InstallDir)) {
    try {
        Move-Item -LiteralPath $oldInstall -Destination $InstallDir
    } catch {
        throw "Could not move $oldInstall to $InstallDir. Close the game and the launcher, then run Setup.cmd again. $($_.Exception.Message)"
    }
    Write-Host "Moved $oldInstall to $InstallDir (saves, shader caches and logs kept)."
}

$config = Join-Path $InstallDir 'game-path.txt'
if (!$GamePath -and (Test-Path -LiteralPath $config -PathType Leaf)) {
    $saved = [IO.File]::ReadAllText($config).Trim()
    if ($saved -and (Test-Path -LiteralPath $saved -PathType Leaf)) { $GamePath = $saved }
}
if (!$GamePath) {
    Add-Type -AssemblyName System.Windows.Forms
    $picker = [Windows.Forms.OpenFileDialog]::new()
    try {
        $picker.Title = 'Select the Series X game executable from your extracted game installation'
        $picker.Filter = 'Game executable (*.exe)|*.exe'
        if ($picker.ShowDialog() -ne [Windows.Forms.DialogResult]::OK) {
            Write-Host 'Setup cancelled.'
            return
        }
        $GamePath = $picker.FileName
    } finally { $picker.Dispose() }
}
$game = Get-Item -LiteralPath $GamePath
if ($game.PSIsContainer -or $game.Extension -ine '.exe') {
    throw 'Select the game executable in its original installation directory.'
}

Push-Location -LiteralPath $PSScriptRoot
try {
    # The BCPack format tables are in this tree (docs/asset-policy.md).
    if (!(Test-Path -LiteralPath (Join-Path $PSScriptRoot 'crates/victory-formats/src/bcpack-tables.bin'))) {
        throw 'crates/victory-formats/src/bcpack-tables.bin is missing from the checkout. Run git pull again.'
    }
    $targetDir = Join-Path $PSScriptRoot 'target'
    $target = 'x86_64-pc-windows-msvc'
    # The plain release build, as codec-d/cool_fox and the GTX 1660 runs
    # were measured with (the frame statistics line is in it).
    & cargo build --locked --release -p victory --target $target --target-dir $targetDir
    if ($LASTEXITCODE -ne 0) {
        throw 'Build failed. Check the compiler output and the C++ workload and Windows SDK listed in the README.'
    }
    $release = Join-Path $targetDir "$target/release"
    & (Join-Path $release 'victory.exe') inspect $game.FullName | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'The runtime could not read the selected game executable.' }
} finally { Pop-Location }

[IO.Directory]::CreateDirectory((Join-Path $InstallDir 'state')) | Out-Null
$files = @(
    @{ From = (Join-Path $release 'victory.exe'); To = 'victory.exe' }
    @{ From = (Join-Path $PSScriptRoot 'launcher/UPI-Launcher.ps1'); To = 'UPI-Launcher.ps1' }
)
foreach ($file in $files) {
    $destination = Join-Path $InstallDir $file.To
    $temp = Join-Path $InstallDir ([IO.Path]::GetRandomFileName())
    try {
        [IO.File]::Copy($file.From, $temp)
        if ([IO.File]::Exists($destination)) {
            try {
                [IO.File]::Replace($temp, $destination, [System.Management.Automation.Language.NullString]::Value)
            } catch [IO.IOException] {
                throw "Could not replace $destination. Close the game and run Setup.cmd again. $($_.Exception.Message)"
            }
        } else {
            [IO.File]::Move($temp, $destination)
        }
    } finally { if ([IO.File]::Exists($temp)) { [IO.File]::Delete($temp) } }
}
[IO.File]::WriteAllText($config, $game.FullName, [Text.UTF8Encoding]::new($false))
# The DexNom-based runtime's own launcher and preloader are not this tree's.
foreach ($stale in 'victory-play.exe', 'victory-warm.exe') {
    $path = Join-Path $InstallDir $stale
    if (Test-Path -LiteralPath $path -PathType Leaf) { Remove-Item -LiteralPath $path }
}

Write-Host "Installed $AppName in $InstallDir"

# The shortcuts' icon: the Roman numeral VI, white serif letters on a dark
# blue rounded square, drawn here (the repository holds text only). Sizes
# 256, 48, 32 and 16, each a PNG inside one .ico. Without it the shortcuts
# keep the launcher's own icon.
function New-UpiIcon([string] $Path) {
    Add-Type -AssemblyName System.Drawing
    $images = foreach ($size in 256, 48, 32, 16) {
        $bitmap = [Drawing.Bitmap]::new($size, $size, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
            $graphics.TextRenderingHint = [Drawing.Text.TextRenderingHint]::AntiAlias
            $graphics.Clear([Drawing.Color]::Transparent)
            $corner = [Math]::Max(2, [int]($size * 0.36))
            $edge = $size - 1
            $shape = [Drawing.Drawing2D.GraphicsPath]::new()
            $shape.AddArc(0, 0, $corner, $corner, 180, 90)
            $shape.AddArc($edge - $corner, 0, $corner, $corner, 270, 90)
            $shape.AddArc($edge - $corner, $edge - $corner, $corner, $corner, 0, 90)
            $shape.AddArc(0, $edge - $corner, $corner, $corner, 90, 90)
            $shape.CloseFigure()
            $fill = [Drawing.Drawing2D.LinearGradientBrush]::new(
                [Drawing.Point]::new(0, 0), [Drawing.Point]::new(0, $size),
                [Drawing.Color]::FromArgb(255, 28, 36, 72), [Drawing.Color]::FromArgb(255, 14, 82, 112))
            $graphics.FillPath($fill, $shape)
            $fill.Dispose()
            $shape.Dispose()
            $font = [Drawing.Font]::new('Times New Roman', [float]($size * 0.6), [Drawing.FontStyle]::Bold, [Drawing.GraphicsUnit]::Pixel)
            $layout = [Drawing.StringFormat]::new()
            $layout.Alignment = [Drawing.StringAlignment]::Center
            $layout.LineAlignment = [Drawing.StringAlignment]::Center
            $box = [Drawing.RectangleF]::new(0, 0, $size, $size)
            $graphics.DrawString('VI', $font, [Drawing.Brushes]::White, $box, $layout)
            $font.Dispose()
            $layout.Dispose()
        } finally { $graphics.Dispose() }
        $png = [IO.MemoryStream]::new()
        $bitmap.Save($png, [Drawing.Imaging.ImageFormat]::Png)
        $bitmap.Dispose()
        , @($size, $png.ToArray())
    }
    $file = [IO.MemoryStream]::new()
    $writer = [IO.BinaryWriter]::new($file)
    $writer.Write([UInt16] 0)
    $writer.Write([UInt16] 1)
    $writer.Write([UInt16] $images.Count)
    $offset = 6 + 16 * $images.Count
    foreach ($image in $images) {
        $side = if ($image[0] -ge 256) { 0 } else { $image[0] }
        $writer.Write([byte] $side)
        $writer.Write([byte] $side)
        $writer.Write([byte] 0)
        $writer.Write([byte] 0)
        $writer.Write([UInt16] 1)
        $writer.Write([UInt16] 32)
        $writer.Write([UInt32] $image[1].Length)
        $writer.Write([UInt32] $offset)
        $offset += $image[1].Length
    }
    foreach ($image in $images) { $writer.Write([byte[]] $image[1]) }
    $writer.Flush()
    [IO.File]::WriteAllBytes($Path, $file.ToArray())
    $writer.Dispose()
}

$icon = Join-Path $InstallDir 'upi.ico'
try {
    New-UpiIcon $icon
} catch {
    Write-Warning "The VI icon was not made ($($_.Exception.Message)); the shortcuts keep the launcher's icon."
    $icon = $null
}

# The shortcuts, in the Start menu and on the desktop: the title as a
# Series S (this fork's default: less memory, the title's lower settings),
# as a Series X, and safe (no renderer ports or newest threading). Earlier
# installs' shortcuts go, the ray tracing one too (it is for Radeon cards).
$powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
if (!(Test-Path -LiteralPath $powershell -PathType Leaf)) { $powershell = 'powershell.exe' }
$script = Join-Path $InstallDir 'UPI-Launcher.ps1'
# " (safe)" in Russian, from code points as the name is.
$safe = ' (' + (-join [char[]](0x0431, 0x0435, 0x0437, 0x043E, 0x043F, 0x0430, 0x0441, 0x043D, 0x044B, 0x0439)) + ')'
$desktop = [Environment]::GetFolderPath('Desktop')
foreach ($stale in 'DexNom Runtime.lnk', 'DexNom Runtime (Experimental RT).lnk', "$AppName (Series S).lnk") {
    foreach ($folder in $ShortcutDirectory, $desktop) {
        $path = Join-Path $folder $stale
        if (Test-Path -LiteralPath $path -PathType Leaf) { Remove-Item -LiteralPath $path }
    }
}
$shell = New-Object -ComObject WScript.Shell
foreach ($folder in $ShortcutDirectory, $desktop) {
    [IO.Directory]::CreateDirectory($folder) | Out-Null
    foreach ($entry in @(
        @{ Name = $AppName; Switch = '' }
        @{ Name = "$AppName (Series X)"; Switch = ' -SeriesX' }
        @{ Name = "$AppName$safe"; Switch = ' -Safe' }
    )) {
        $shortcut = $shell.CreateShortcut((Join-Path $folder ($entry.Name + '.lnk')))
        $shortcut.TargetPath = $powershell
        $shortcut.Arguments = "-NoProfile -ExecutionPolicy Bypass -File `"$script`"$($entry.Switch)"
        $shortcut.WorkingDirectory = $InstallDir
        $shortcut.Description = $entry.Name
        if ($icon) { $shortcut.IconLocation = "$icon,0" }
        $shortcut.Save()
    }
}
Write-Host "Open $AppName from the desktop or the Start menu to play. Logs: $(Join-Path $InstallDir 'logs')"
