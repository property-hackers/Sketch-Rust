# Builds dist\SketchRust-<version>-x64.msi with WiX Toolset 5 (the toolchain the
# ArtCraft Launcher's installer uses).
#
#   tools\installer.ps1 -FetchWix     first run: download WiX into target\wix-tools
#   tools\installer.ps1               build from today's staged package
#   tools\installer.ps1 -Test         also install, check and remove it
#
# WiX needs a .NET runtime (6 or newer), not the SDK. It is found, in order,
# as `wix` on PATH, as $env:WIX_DIR\wix.exe, or under target\wix-tools.
# -FetchWix downloads three NuGet packages from nuget.org and refuses any
# whose SHA-256 differs from the one recorded below.
#
# -Test really installs into a scratch folder with the machine's own Windows
# Installer, so it adds and then removes a Start Menu shortcut, a desktop
# shortcut and an entry under Settings > Apps for this user.
param([switch]$FetchWix, [switch]$Test, [switch]$SkipPackage)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $repoRoot
try {
    $tools = Join-Path $repoRoot 'target\wix-tools'
    $packages = [ordered]@{
        'wix'                    = 'f30ef0c74e2a986126539c5780be93ac24e8136eaf723b1937b26272703ae173'
        'wixtoolset.ui.wixext'   = '5ef2c707614b9f70b6bbadd2d4abcb4124efee215e9b16bfbc80113079a604c7'
        'wixtoolset.util.wixext' = 'dda1cc1b4d3b2305b1246f167c6af6fcded5000be84b3bbff3dda4f442bf4740'
    }
    if ($FetchWix) {
        foreach ($id in $packages.Keys) {
            $zip = Join-Path $tools "dl\$id.nupkg"
            New-Item -ItemType Directory -Path (Split-Path $zip) -Force | Out-Null
            Invoke-WebRequest -Uri "https://api.nuget.org/v3-flatcontainer/$id/5.0.2/$id.5.0.2.nupkg" -OutFile $zip
            $hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($hash -ne $packages[$id]) { Remove-Item -LiteralPath $zip; throw "$id.nupkg has SHA-256 $hash, expected $($packages[$id])." }
            $out = Join-Path $tools "x\$id"
            if (Test-Path -LiteralPath $out) { Remove-Item -LiteralPath $out -Recurse -Force }
            Expand-Archive -LiteralPath $zip -DestinationPath $out -Force
        }
    }
    $wixDll = Join-Path $tools 'x\wix\tools\net6.0\any\wix.dll'
    if (Get-Command wix -ErrorAction SilentlyContinue) { $wix = @('wix') }
    elseif ($env:WIX_DIR -and (Test-Path (Join-Path $env:WIX_DIR 'wix.exe'))) { $wix = @((Join-Path $env:WIX_DIR 'wix.exe')) }
    elseif (Test-Path -LiteralPath $wixDll) { $wix = @('dotnet', $wixDll) }
    else { throw 'WiX is not available. Run tools\installer.ps1 -FetchWix once, or install it with: dotnet tool install --global wix --version 5.0.2' }
    $uiExt = Join-Path $tools 'x\wixtoolset.ui.wixext\wixext5\WixToolset.UI.wixext.dll'
    $utilExt = Join-Path $tools 'x\wixtoolset.util.wixext\wixext5\WixToolset.Util.wixext.dll'
    foreach ($ext in $uiExt, $utilExt) { if (-not (Test-Path -LiteralPath $ext)) { throw "Missing $ext. Run -FetchWix." } }

    $version = [regex]::Match((Get-Content -LiteralPath 'Cargo.toml' -Raw), 'version\s*=\s*"([^"]+)"').Groups[1].Value
    if (-not $SkipPackage) {
        # A release must include the Apex compatibility component; package.ps1
        # builds with SKETCH_REQUIRE_APEX=1 unless it is already set (use '0'
        # for an Apex-less dev package). A failed package build must not fall
        # through to a stale staged folder.
        if (-not $env:SKETCH_REQUIRE_APEX) { $env:SKETCH_REQUIRE_APEX = '1' }
        & (Join-Path $PSScriptRoot 'package.ps1') -NativePreview | Out-Null
    }
    $stage = Get-ChildItem -LiteralPath 'target\package' -Directory -Filter "SketchRust-$version-*" |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $stage) { throw "No staged package for $version under target\package. Run tools\package.ps1 first." }

    # Everything generated for the build goes here.
    $work = Join-Path $repoRoot 'target\msi'
    if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force }
    New-Item -ItemType Directory -Path $work -Force | Out-Null

    # Files.wxs: one component per staged file.
    $lines = @('<?xml version="1.0" encoding="utf-8"?>',
        '<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">', '  <Fragment>',
        '    <ComponentGroup Id="AppFiles" Directory="INSTALLFOLDER">')
    foreach ($file in Get-ChildItem -LiteralPath $stage.FullName -File | Sort-Object Name) {
        $id = 'F_' + ($file.Name -replace '[^A-Za-z0-9]', '_')
        $src = [Security.SecurityElement]::Escape($file.FullName)
        $name = [Security.SecurityElement]::Escape($file.Name)
        $lines += "      <Component Id=`"C$id`"><File Id=`"$id`" Name=`"$name`" Source=`"$src`" /></Component>"
    }
    $lines += '    </ComponentGroup>', '  </Fragment>', '</Wix>'
    Set-Content -LiteralPath (Join-Path $work 'Files.wxs') -Value $lines -Encoding utf8

    # Licence page: the repository's MIT licence as RTF.
    $text = (Get-Content -LiteralPath 'LICENSE' -Raw) -replace '\\', '\\\\' -replace '\{', '\{' -replace '\}', '\}'
    $rtf = '{\rtf1\ansi\deff0{\fonttbl{\f0 Segoe UI;}}\f0\fs20 ' + (($text -split "\r?\n\r?\n") -replace "\r?\n", ' ' -join '\par\par ') + '}'
    Set-Content -LiteralPath (Join-Path $work 'License.rtf') -Value $rtf -Encoding ascii

    # Dialog pictures carrying the app icon.
    Add-Type -AssemblyName System.Drawing
    $icon = [System.Drawing.Image]::FromFile((Join-Path $repoRoot 'assets\app-icon.png'))
    function New-Picture([int]$w, [int]$h, [scriptblock]$paint, [string]$path) {
        $bmp = New-Object System.Drawing.Bitmap $w, $h, ([System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.SmoothingMode = 'HighQuality'; $g.InterpolationMode = 'HighQualityBicubic'; $g.TextRenderingHint = 'ClearTypeGridFit'
        & $paint $g
        $g.Dispose(); $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Bmp); $bmp.Dispose()
    }
    # Banner (top of the inner pages, 493 x 58): white, icon at the right.
    New-Picture 493 58 { param($g) $g.Clear([System.Drawing.Color]::White); $g.DrawImage($icon, 433, 5, 48, 48) } (Join-Path $work 'Banner.bmp')
    # Welcome / finish side panel (493 x 312): icon over a dark panel on the left 164 px.
    New-Picture 493 312 { param($g)
        $g.Clear([System.Drawing.Color]::White)
        $panel = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(30, 30, 30))
        $g.FillRectangle($panel, 0, 0, 164, 312)
        $g.DrawImage($icon, 34, 40, 96, 96)
        $font = New-Object System.Drawing.Font 'Segoe UI', 13, ([System.Drawing.FontStyle]::Bold)
        $g.DrawString('Sketch Rust', $font, [System.Drawing.Brushes]::White, 28, 150)
    } (Join-Path $work 'Dialog.bmp')
    $icon.Dispose()

    $dist = Join-Path $repoRoot 'dist'
    New-Item -ItemType Directory -Path $dist -Force | Out-Null
    $msi = Join-Path $dist "SketchRust-$version-x64.msi"
    $args = @($wix[1..($wix.Count)] | Where-Object { $_ }) + @(
        'build', 'installer\SketchRust.wxs', (Join-Path $work 'Files.wxs'), '-arch', 'x64',
        '-ext', $uiExt, '-ext', $utilExt,
        '-d', "Version=$version", '-d', "IconFile=$(Join-Path $repoRoot 'assets\app-icon.ico')",
        '-d', "LicenseRtf=$(Join-Path $work 'License.rtf')", '-d', "BannerBmp=$(Join-Path $work 'Banner.bmp')",
        '-d', "DialogBmp=$(Join-Path $work 'Dialog.bmp')", '-o', $msi)
    & $wix[0] @args
    if ($LASTEXITCODE -ne 0) { throw 'WiX build failed.' }
    "$((Get-FileHash -LiteralPath $msi -Algorithm SHA256).Hash.ToLowerInvariant())  $([IO.Path]::GetFileName($msi))" |
        Set-Content -LiteralPath "$msi.sha256" -Encoding ascii
    Write-Output "Installer: $msi ($([math]::Round((Get-Item -LiteralPath $msi).Length / 1MB, 1)) MB) from $($stage.Name)"

    if ($Test) {
        $scratch = Join-Path $repoRoot 'target\msi-test'
        if (Test-Path -LiteralPath $scratch) { Remove-Item -LiteralPath $scratch -Recurse -Force }
        # An existing install (the portable zip's or an older installer's) may own the same
        # shortcut names; the MSI replaces and then deletes them, so put them back afterwards.
        $keep = @{}
        foreach ($l in (Join-Path ([Environment]::GetFolderPath('Programs')) 'Sketch Rust.lnk'), (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Sketch Rust.lnk')) {
            if (Test-Path -LiteralPath $l) { $copy = Join-Path $work ([IO.Path]::GetFileName((Split-Path $l)) + '.lnk.bak'); Copy-Item -LiteralPath $l -Destination $copy -Force; $keep[$l] = $copy }
        }
        $log = Join-Path $work 'install.log'
        $p = Start-Process msiexec.exe -ArgumentList "/i `"$msi`" /qn /l*v `"$log`" INSTALLFOLDER=`"$scratch\`"" -Wait -PassThru
        if ($p.ExitCode -ne 0) { throw "msiexec /i exited $($p.ExitCode); see $log" }
        foreach ($f in 'sketch-app.exe', 'symbols.json') { if (-not (Test-Path -LiteralPath (Join-Path $scratch $f))) { throw "$f was not installed" } }
        $startMenu = Join-Path ([Environment]::GetFolderPath('Programs')) 'Sketch Rust.lnk'
        $desktop = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Sketch Rust.lnk'
        foreach ($l in $startMenu, $desktop) { if (-not (Test-Path -LiteralPath $l)) { throw "missing shortcut $l" } }
        # Settings > Apps: this build's product must be registered, with the app icon.
        $installer = New-Object -ComObject WindowsInstaller.Installer
        $view = $installer.OpenDatabase($msi, 0).OpenView("SELECT Value FROM Property WHERE Property='ProductCode'")
        $view.Execute()
        $productCode = $view.Fetch().StringData(1)
        if ($installer.ProductState($productCode) -ne 5) { throw "product $productCode is not installed" }
        if (-not $installer.ProductInfo($productCode, 'ProductIcon')) { throw 'the installed product has no icon' }
        $p = Start-Process msiexec.exe -ArgumentList "/x `"$msi`" /qn /l*v `"$(Join-Path $work 'remove.log')`"" -Wait -PassThru
        if ($p.ExitCode -ne 0) { throw "msiexec /x exited $($p.ExitCode)" }
        if ($installer.ProductState($productCode) -ne -1) { throw 'still registered after uninstall' }
        foreach ($l in $startMenu, $desktop, (Join-Path $scratch 'sketch-app.exe')) { if (Test-Path -LiteralPath $l) { throw "left behind after uninstall: $l" } }
        foreach ($l in $keep.Keys) { Copy-Item -LiteralPath $keep[$l] -Destination $l -Force }
        Write-Output 'Self-test: installed, shortcuts and Apps entry present, uninstalled cleanly.'
    }
} finally { Pop-Location }
