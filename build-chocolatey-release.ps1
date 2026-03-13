# silence! Chocolatey Package Builder
# Builds a Chocolatey package from local installer files.

param(
    [string]$Version,
    [string]$InstallerX86Path,
    [string]$InstallerX64Path,
    [string]$Repository = "vertopolkaLF/silence",
    [string]$ReleaseTag,
    [string]$OutputDir = "releases",
    [switch]$Push,
    [string]$ApiKey,
    [switch]$TestInstall
)

$ErrorActionPreference = "Stop"

function Get-ProjectVersion {
    $csprojPath = Join-Path $PSScriptRoot "Silence!.csproj"

    if (-not (Test-Path $csprojPath)) {
        throw "Could not detect version. Specify -Version explicitly."
    }

    [xml]$csproj = Get-Content $csprojPath
    $detectedVersion = $csproj.Project.PropertyGroup.Version | Where-Object { $_ } | Select-Object -First 1

    if (-not $detectedVersion) {
        throw "Version not found in Silence!.csproj."
    }

    return $detectedVersion
}

function Convert-ToPackageVersion {
    param(
        [Parameter(Mandatory)]
        [string]$SemanticVersion
    )

    $versionParts = $SemanticVersion.Split(".")

    switch ($versionParts.Count) {
        1 { return "$SemanticVersion.0.0.0" }
        2 { return "$SemanticVersion.0.0" }
        3 { return "$SemanticVersion.0" }
        default { return $SemanticVersion }
    }
}

function Require-File {
    param(
        [Parameter(Mandatory)]
        [string]$Path,
        [Parameter(Mandatory)]
        [string]$Description
    )

    if (-not (Test-Path $Path)) {
        throw "$Description not found: $Path"
    }
}

if (-not $Version) {
    $Version = Get-ProjectVersion
}

if (-not $ReleaseTag) {
    $ReleaseTag = "v$Version"
}

if (-not $InstallerX86Path) {
    $InstallerX86Path = Join-Path $PSScriptRoot "releases\silence-v$Version-x86-setup.exe"
}

if (-not $InstallerX64Path) {
    $InstallerX64Path = Join-Path $PSScriptRoot "releases\silence-v$Version-x64-setup.exe"
}

Require-File -Path $InstallerX86Path -Description "x86 installer"
Require-File -Path $InstallerX64Path -Description "x64 installer"

$packageVersion = Convert-ToPackageVersion -SemanticVersion $Version
$releaseBaseUrl = "https://github.com/$Repository/releases/download/$ReleaseTag"
$releasePageUrl = "https://github.com/$Repository/releases/tag/$ReleaseTag"

$outputPath = if ([System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir
} else {
    Join-Path $PSScriptRoot $OutputDir
}

New-Item -ItemType Directory -Path $outputPath -Force | Out-Null

Write-Host ""
Write-Host "=======================================================" -ForegroundColor Magenta
Write-Host "    silence! Chocolatey Package Builder v$Version" -ForegroundColor Magenta
Write-Host "=======================================================" -ForegroundColor Magenta
Write-Host ""

$chocoExists = Get-Command choco -ErrorAction SilentlyContinue
if (-not $chocoExists) {
    throw "Chocolatey CLI (choco) not found."
}

$checksum32 = (Get-FileHash -Algorithm SHA256 $InstallerX86Path).Hash
$checksum64 = (Get-FileHash -Algorithm SHA256 $InstallerX64Path).Hash

Write-Host "x86 installer: $InstallerX86Path" -ForegroundColor Cyan
Write-Host "x64 installer: $InstallerX64Path" -ForegroundColor Cyan
Write-Host "x86 SHA256: $checksum32" -ForegroundColor Green
Write-Host "x64 SHA256: $checksum64" -ForegroundColor Green
Write-Host ""

$templateDir = Join-Path $PSScriptRoot "Chocolatey"
Require-File -Path $templateDir -Description "Chocolatey template directory"

$packageWorkDir = Join-Path $env:TEMP ("silence-choco-" + [guid]::NewGuid().ToString("N"))
Copy-Item $templateDir $packageWorkDir -Recurse -Force
Get-ChildItem $packageWorkDir -Filter *.nupkg -File -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue

$installScriptPath = Join-Path $packageWorkDir "tools\chocolateyinstall.ps1"
$verificationPath = Join-Path $packageWorkDir "tools\VERIFICATION.txt"
$nuspecPath = Join-Path $packageWorkDir "silence!.nuspec"

$replacements = @{
    "__URL32__"      = "$releaseBaseUrl/silence-v$Version-x86-setup.exe"
    "__URL64__"      = "$releaseBaseUrl/silence-v$Version-x64-setup.exe"
    "__CHECKSUM32__" = $checksum32
    "__CHECKSUM64__" = $checksum64
    "__VERSION__"    = $Version
}

try {
    $installScript = Get-Content $installScriptPath -Raw
    foreach ($token in $replacements.Keys) {
        $installScript = $installScript.Replace($token, $replacements[$token])
    }
    Set-Content -Path $installScriptPath -Value $installScript -NoNewline

    if (Test-Path $verificationPath) {
        $verification = Get-Content $verificationPath -Raw
        foreach ($token in $replacements.Keys) {
            $verification = $verification.Replace($token, $replacements[$token])
        }
        Set-Content -Path $verificationPath -Value $verification -NoNewline
    }

    [xml]$nuspec = Get-Content $nuspecPath
    $nuspec.package.metadata.version = $packageVersion
    $nuspec.package.metadata.releaseNotes = $releasePageUrl
    $nuspec.Save($nuspecPath)

    Push-Location $packageWorkDir

    & choco pack ".\silence!.nuspec"
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to build Chocolatey package."
    }

    $packageName = "silence.$packageVersion.nupkg"
    $packageFile = Join-Path $packageWorkDir $packageName
    Require-File -Path $packageFile -Description "Chocolatey package"

    $finalPackage = Join-Path $outputPath $packageName
    Copy-Item $packageFile $finalPackage -Force

    Write-Host ""
    Write-Host "Package created: $finalPackage" -ForegroundColor Green

    if ($TestInstall) {
        & choco install silence -s $packageWorkDir -y
        if ($LASTEXITCODE -ne 0) {
            throw "Test installation failed."
        }
    }

    if ($Push) {
        if (-not $ApiKey) {
            throw "Chocolatey API key is required when -Push is used."
        }

        & choco push $finalPackage --source https://push.chocolatey.org/ --api-key $ApiKey
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to push Chocolatey package."
        }

        Write-Host "Chocolatey package pushed successfully." -ForegroundColor Green
    }
} finally {
    Pop-Location -ErrorAction SilentlyContinue
    Remove-Item $packageWorkDir -Recurse -Force -ErrorAction SilentlyContinue
}
