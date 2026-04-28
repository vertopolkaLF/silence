Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-Step {
    param([string]$Message)
    Write-Host ""
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Get-CargoPackageMetadata {
    param([string]$CargoTomlPath)

    $content = Get-Content $CargoTomlPath -Raw
    $packageBlock = [regex]::Match(
        $content,
        '(?ms)^\[package\]\s*(?<body>.*?)(?:^\[|\z)'
    )

    if (-not $packageBlock.Success) {
        throw "Could not find [package] section in $CargoTomlPath"
    }

    $body = $packageBlock.Groups["body"].Value
    $nameMatch = [regex]::Match($body, '(?m)^\s*name\s*=\s*"(?<value>[^"]+)"')
    $versionMatch = [regex]::Match($body, '(?m)^\s*version\s*=\s*"(?<value>[^"]+)"')

    if (-not $nameMatch.Success) {
        throw "Could not find package name in $CargoTomlPath"
    }
    if (-not $versionMatch.Success) {
        throw "Could not find package version in $CargoTomlPath"
    }

    [pscustomobject]@{
        Name = $nameMatch.Groups["value"].Value
        Version = $versionMatch.Groups["value"].Value
    }
}

function Assert-CommandExists {
    param([string]$Name)

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command '$Name' was not found in PATH."
    }
}

function Assert-NoRunningRepoBuilds {
    param([string]$RepoRoot)

    $repoPrefix = [System.IO.Path]::GetFullPath($RepoRoot).TrimEnd('\') + '\'
    $running = Get-Process -Name silence -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -and $_.Path.StartsWith($repoPrefix, [System.StringComparison]::OrdinalIgnoreCase) }

    if ($running) {
        $paths = $running | ForEach-Object { " - $($_.Path) [pid $($_.Id)]" }
        throw "Close running silence.exe instances before packaging to avoid file locks:`n$($paths -join "`n")"
    }
}

function Invoke-Dx {
    param(
        [string]$RepoRoot,
        [string[]]$Arguments
    )

    & dx @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "dx $($Arguments -join ' ') failed with exit code $LASTEXITCODE"
    }
}

function Copy-PortableApp {
    param(
        [string]$SourceAppDir,
        [string]$PortableDir,
        [string]$ZipPath
    )

    if (Test-Path $PortableDir) {
        Remove-Item $PortableDir -Recurse -Force
    }
    New-Item -ItemType Directory -Path $PortableDir -Force | Out-Null
    Copy-Item (Join-Path $SourceAppDir '*') $PortableDir -Recurse -Force

    if (Test-Path $ZipPath) {
        Remove-Item $ZipPath -Force
    }
    Compress-Archive -Path (Join-Path $PortableDir '*') -DestinationPath $ZipPath -Force
}

function Collect-MsiArtifact {
    param(
        [string]$StageDir,
        [string]$FinalPath
    )

    $msi = Get-ChildItem $StageDir -Filter *.msi -File -Recurse | Select-Object -First 1
    if (-not $msi) {
        throw "MSI bundle was not produced in $StageDir"
    }

    if (Test-Path $FinalPath) {
        Remove-Item $FinalPath -Force
    }
    Move-Item $msi.FullName $FinalPath
}

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$cargoTomlPath = Join-Path $repoRoot "Cargo.toml"
$package = Get-CargoPackageMetadata -CargoTomlPath $cargoTomlPath
$versionRoot = Join-Path $repoRoot ("dist\" + $package.Version)
$stagingRoot = Join-Path $versionRoot "_staging"
$appRoot = Join-Path $repoRoot "target\dx\silence\release\windows\app"
$safeName = ($package.Name.ToLowerInvariant() -replace '[^a-z0-9]+', '-').Trim('-')

$targets = @(
    @{ Triple = "x86_64-pc-windows-msvc"; Arch = "x64" },
    @{ Triple = "i686-pc-windows-msvc"; Arch = "x86" },
    @{ Triple = "aarch64-pc-windows-msvc"; Arch = "arm64" }
)

Assert-CommandExists -Name "dx"
Assert-CommandExists -Name "rustup"
Assert-NoRunningRepoBuilds -RepoRoot $repoRoot

Write-Step "Preparing dist\$($package.Version)"
New-Item -ItemType Directory -Path $versionRoot -Force | Out-Null
if (Test-Path $stagingRoot) {
    Remove-Item $stagingRoot -Recurse -Force
}
foreach ($legacyDir in @("portable", "msi")) {
    $legacyPath = Join-Path $versionRoot $legacyDir
    if (Test-Path $legacyPath) {
        Remove-Item $legacyPath -Recurse -Force
    }
}
Get-ChildItem $versionRoot -File -ErrorAction SilentlyContinue |
    Where-Object { $_.Extension -in @(".zip", ".msi") } |
    Remove-Item -Force
New-Item -ItemType Directory -Path $stagingRoot -Force | Out-Null

Write-Step "Ensuring Rust targets are installed"
& rustup target add ($targets | ForEach-Object { $_.Triple })
if ($LASTEXITCODE -ne 0) {
    throw "rustup target add failed with exit code $LASTEXITCODE"
}

$failures = New-Object System.Collections.Generic.List[string]

Push-Location $repoRoot
try {
    foreach ($target in $targets) {
        $triple = $target.Triple
        $arch = $target.Arch
        $portableDir = Join-Path $stagingRoot "portable-$arch"
        $zipPath = Join-Path $versionRoot "$safeName-$($package.Version)-windows-$arch-portable.zip"
        $bundleStageDir = Join-Path $stagingRoot "bundle-$arch"
        $finalMsiPath = Join-Path $versionRoot "$safeName-$($package.Version)-windows-$arch-installer.msi"

        try {
            Write-Step "Building portable app for $arch ($triple)"
            Invoke-Dx -RepoRoot $repoRoot -Arguments @(
                "build",
                "--platform", "windows",
                "--release",
                "--target", $triple
            )

            if (-not (Test-Path $appRoot)) {
                throw "Expected built app folder was not found: $appRoot"
            }

            Copy-PortableApp -SourceAppDir $appRoot -PortableDir $portableDir -ZipPath $zipPath

            Write-Step "Bundling MSI for $arch ($triple)"
            if (Test-Path $bundleStageDir) {
                Remove-Item $bundleStageDir -Recurse -Force
            }
            New-Item -ItemType Directory -Path $bundleStageDir -Force | Out-Null

            Invoke-Dx -RepoRoot $repoRoot -Arguments @(
                "bundle",
                "--platform", "windows",
                "--release",
                "--target", $triple,
                "--package-types", "msi",
                "--out-dir", $bundleStageDir
            )

            Collect-MsiArtifact -StageDir $bundleStageDir -FinalPath $finalMsiPath
        }
        catch {
            $failures.Add("${arch}: $($_.Exception.Message)")
            Write-Warning "Build for $arch failed: $($_.Exception.Message)"
        }
    }
}
finally {
    Pop-Location
}

if ($failures.Count -gt 0) {
    Write-Host ""
    Write-Host "Completed with failures:" -ForegroundColor Yellow
    $failures | ForEach-Object { Write-Host " - $_" -ForegroundColor Yellow }
    exit 1
}

if (Test-Path $stagingRoot) {
    Remove-Item $stagingRoot -Recurse -Force
}

Write-Host ""
Write-Host "Build outputs are ready in $versionRoot" -ForegroundColor Green
