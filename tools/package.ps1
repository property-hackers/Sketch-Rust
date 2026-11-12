param([switch]$SkipBuild, [switch]$NativePreview, [string]$InstallDirectory)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $repoRoot
try {
    if (-not $SkipBuild) {
        # A release must include the Apex compatibility component. For an
        # Apex-less dev package, set $env:SKETCH_REQUIRE_APEX = '0' first.
        if (-not $env:SKETCH_REQUIRE_APEX) { $env:SKETCH_REQUIRE_APEX = '1' }
        cargo build --release -p sketch-app --offline
        if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    }
    $audit = Get-Content -LiteralPath 'refs/ui-spec/feature-audit.json' -Raw | ConvertFrom-Json
    $version = [regex]::Match((Get-Content -LiteralPath 'Cargo.toml' -Raw), 'version\s*=\s*"([^"]+)"').Groups[1].Value
    $channel = if ($NativePreview) {'-native-preview'} else {''}
    $packageDate = Get-Date -Format "yyyy-MM-dd"
    $packageName = "SketchRust-$version$channel-$packageDate-windows-x64"
    $stage = Join-Path $repoRoot "target\package\$packageName"
    $dist = Join-Path $repoRoot 'dist'
    $install = if ($InstallDirectory) { [IO.Path]::GetFullPath((Join-Path $repoRoot $InstallDirectory)) } else { $dist }
    New-Item -ItemType Directory -Path $stage,$dist,$install -Force | Out-Null
    Copy-Item -LiteralPath 'target/release/sketch-app.exe' -Destination (Join-Path $stage 'sketch-app.exe') -Force
    Copy-Item -LiteralPath 'assets/symbols.json' -Destination (Join-Path $stage 'symbols.json') -Force
    Copy-Item -LiteralPath 'assets/symbols.previous.json' -Destination (Join-Path $stage 'symbols.previous.json') -Force
    Copy-Item -LiteralPath 'docs/SYMBOL_LIBRARY.md' -Destination (Join-Path $stage 'SYMBOL-LIBRARY.md') -Force
    Copy-Item -LiteralPath 'docs/RELEASE_README.txt' -Destination (Join-Path $stage 'README.txt') -Force
    Copy-Item -LiteralPath 'docs/DISTO.md' -Destination (Join-Path $stage 'DISTO.md') -Force
    Copy-Item -LiteralPath 'docs/BUILD_VALIDATION.md' -Destination (Join-Path $stage 'BUILD_VALIDATION.md') -Force
    Copy-Item -LiteralPath 'docs/CONTROL_WIRING_AUDIT.md' -Destination (Join-Path $stage 'CONTROL_WIRING_AUDIT.md') -Force
    Copy-Item -LiteralPath 'docs/APEX_REPLACEMENT.md' -Destination (Join-Path $stage 'APEX-COMPATIBILITY.md') -Force
    Copy-Item -LiteralPath 'refs/ui-spec/PARITY.md' -Destination (Join-Path $stage 'FEATURE-AUDIT.md') -Force
    $packagedAudit = (Get-Content -LiteralPath (Join-Path $stage 'FEATURE-AUDIT.md') -Raw).Replace('../../ACCEPTANCE.md','ACCEPTANCE.md').Replace('docs/DISTO.md','DISTO.md')
    Set-Content -LiteralPath (Join-Path $stage 'FEATURE-AUDIT.md') -Value $packagedAudit -Encoding utf8
    Copy-Item -LiteralPath 'ACCEPTANCE.md' -Destination (Join-Path $stage 'ACCEPTANCE.md') -Force
    Copy-Item -LiteralPath 'refs/ui-spec/feature-audit.json' -Destination (Join-Path $stage 'feature-audit.json') -Force
    Copy-Item -LiteralPath 'refs/ui-spec/feature-inventory.md' -Destination (Join-Path $stage 'feature-inventory.md') -Force
    $sourceFiles = @('Cargo.toml','Cargo.lock') + @(Get-ChildItem -LiteralPath 'crates','assets' -Recurse -File | ForEach-Object { $_.FullName.Substring($repoRoot.Length + 1) })
    $fingerprints = foreach ($sourceFile in ($sourceFiles | Sort-Object)) {
        "$sourceFile $((Get-FileHash -LiteralPath $sourceFile -Algorithm SHA256).Hash)"
    }
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try {
        $sourceHash = [BitConverter]::ToString($hasher.ComputeHash([Text.Encoding]::UTF8.GetBytes(($fingerprints -join "`n")))).Replace('-','').ToLowerInvariant()
    } finally { $hasher.Dispose() }
    $packageFiles = @(Get-ChildItem -LiteralPath $stage -File | Where-Object { $_.Name -ne 'manifest.json' } | ForEach-Object {
        [ordered]@{ name=$_.Name; bytes=$_.Length; sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
    })
    $manifest = [ordered]@{
        version=$version; packagedAtUtc=[DateTime]::UtcNow.ToString('o'); target='x86_64-pc-windows-msvc'
        sourceCommit=(git rev-parse HEAD).Trim(); includesUncommittedChanges=[bool](git status --porcelain)
        sourceSha256=$sourceHash; featureAuditDate=$audit.audit_date; distoHardware='Deferred at user request'
        nativeApexCompatibility='Development preview; see APEX-COMPATIBILITY.md; not a completed universal replacement'
        files=$packageFiles
    }
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $stage 'manifest.json') -Encoding utf8
    foreach ($entry in $manifest.files) {
        if ((Get-FileHash -LiteralPath (Join-Path $stage $entry.name) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256) {
            throw "Package verification failed: $($entry.name)"
        }
    }
    $zip = Join-Path $dist "$packageName.zip"
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip -CompressionLevel Optimal -Force
    "$((Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant())  $([IO.Path]::GetFileName($zip))" |
        Set-Content -LiteralPath "$zip.sha256" -Encoding ascii
    foreach ($file in (Get-ChildItem -LiteralPath $stage -File)) {
        $destination = Join-Path $install $file.Name
        if ((Test-Path -LiteralPath $destination) -and
            (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -eq
            (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash) { continue }
        Copy-Item -LiteralPath $file.FullName -Destination $destination -Force
    }
    Get-FileHash -LiteralPath $zip -Algorithm SHA256 | Format-List
    Write-Output "Working folder: $install"
    Write-Output "Portable archive: $zip"
} finally { Pop-Location }
