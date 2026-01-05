# silence! Chocolatey Package Builder
# Creates a Chocolatey package from a GitHub release
# Requires: choco CLI

param(
    [string]$Version,        # Version to package (e.g., "1.4")
    [switch]$SkipPush,       # Skip pushing to Chocolatey.org
    [switch]$TestInstall     # Test install locally after building
)

$ErrorActionPreference = "Stop"

# ============================================
# Version Detection
# ============================================
if (-not $Version) {
    $csprojPath = "Silence!.csproj"
    if (Test-Path $csprojPath) {
        [xml]$csproj = Get-Content $csprojPath
        $Version = $csproj.Project.PropertyGroup.Version | Where-Object { $_ } | Select-Object -First 1
        if (-not $Version) { $Version = "1.0" }
    } else {
        Write-Host "ERROR: Could not detect version. Specify with -Version parameter." -ForegroundColor Red
        exit 1
    }
}

Write-Host ""
Write-Host "=======================================================" -ForegroundColor Magenta
Write-Host "    silence! Chocolatey Package Builder v$Version" -ForegroundColor Magenta
Write-Host "=======================================================" -ForegroundColor Magenta
Write-Host ""

# Check if choco is installed
$chocoExists = Get-Command choco -ErrorAction SilentlyContinue
if (-not $chocoExists) {
    Write-Host "ERROR: Chocolatey CLI (choco) not found!" -ForegroundColor Red
    Write-Host "Install from: https://chocolatey.org/install" -ForegroundColor Yellow
    exit 1
}

# Navigate to Chocolatey folder
$chocoDir = Join-Path $PSScriptRoot "Chocolatey"
if (-not (Test-Path $chocoDir)) {
    Write-Host "ERROR: Chocolatey folder not found at $chocoDir" -ForegroundColor Red
    exit 1
}

Push-Location $chocoDir

try {
    # ============================================
    # Step 1: Download installers and get checksums
    # ============================================
    Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
    Write-Host "  STEP 1: Downloading installers for checksum" -ForegroundColor Cyan
    Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
    Write-Host ""
    
    $baseUrl = "https://github.com/vertopolkaLF/silence/releases/download/v$Version"
    $tempX86 = Join-Path $env:TEMP "silence-v$Version-x86-temp.exe"
    $tempX64 = Join-Path $env:TEMP "silence-v$Version-x64-temp.exe"
    
    try {
        Write-Host "  Downloading x86 installer..." -ForegroundColor Gray
        Invoke-WebRequest -Uri "$baseUrl/silence-v$Version-x86-setup.exe" -OutFile $tempX86 -UseBasicParsing
        
        Write-Host "  Downloading x64 installer..." -ForegroundColor Gray
        Invoke-WebRequest -Uri "$baseUrl/silence-v$Version-x64-setup.exe" -OutFile $tempX64 -UseBasicParsing
        
        Write-Host ""
        Write-Host "  Calculating checksums..." -ForegroundColor Yellow
        $checksum32 = (Get-FileHash -Algorithm SHA256 $tempX86).Hash
        $checksum64 = (Get-FileHash -Algorithm SHA256 $tempX64).Hash
        
        Write-Host "  x86 SHA256: $checksum32" -ForegroundColor Green
        Write-Host "  x64 SHA256: $checksum64" -ForegroundColor Green
        Write-Host ""
        
        Remove-Item $tempX86, $tempX64 -ErrorAction SilentlyContinue
        
    } catch {
        Write-Host "ERROR: Failed to download installers from GitHub!" -ForegroundColor Red
        Write-Host "Make sure release v$Version exists and is published." -ForegroundColor Yellow
        Write-Host "URL: https://github.com/vertopolkaLF/silence/releases/tag/v$Version" -ForegroundColor Yellow
        exit 1
    }
    
    # ============================================
    # Step 2: Update chocolateyinstall.ps1
    # ============================================
    Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
    Write-Host "  STEP 2: Updating chocolateyinstall.ps1" -ForegroundColor Cyan
    Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
    Write-Host ""
    
    $installScriptPath = "tools\chocolateyinstall.ps1"
    if (-not (Test-Path $installScriptPath)) {
        Write-Host "ERROR: $installScriptPath not found!" -ForegroundColor Red
        exit 1
    }
    
    $installScript = Get-Content $installScriptPath -Raw
    $installScript = $installScript -replace "checksum\s*=\s*'[^']*'", "checksum      = '$checksum32'"
    $installScript = $installScript -replace "checksum64\s*=\s*'[^']*'", "checksum64    = '$checksum64'"
    $installScript = $installScript -replace "v[\d\.]+/", "v$Version/"
    Set-Content $installScriptPath -Value $installScript -NoNewline
    
    Write-Host "  Updated checksums and version URLs" -ForegroundColor Green
    Write-Host ""
    
    # ============================================
    # Step 3: Update nuspec
    # ============================================
    Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
    Write-Host "  STEP 3: Updating nuspec file" -ForegroundColor Cyan
    Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
    Write-Host ""
    
    $nuspecPath = "silence!.nuspec"
    if (-not (Test-Path $nuspecPath)) {
        Write-Host "ERROR: $nuspecPath not found!" -ForegroundColor Red
        exit 1
    }
    
    $nuspec = [xml](Get-Content $nuspecPath)
    $nuspec.package.metadata.version = "$Version"
    $nuspec.package.metadata.releaseNotes = "https://github.com/vertopolkaLF/silence/releases/tag/v$Version"
    $nuspec.Save((Resolve-Path $nuspecPath))
    
    Write-Host "  Updated version to $Version" -ForegroundColor Green
    Write-Host "  Updated release notes URL" -ForegroundColor Green
    Write-Host ""
    
    # ============================================
    # Step 4: Build package
    # ============================================
    Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
    Write-Host "  STEP 4: Building Chocolatey package" -ForegroundColor Cyan
    Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
    Write-Host ""
    
    & choco pack
    
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: Failed to build Chocolatey package!" -ForegroundColor Red
        exit 1
    }
    
    $packageFile = "silence.$Version.nupkg"
    if (-not (Test-Path $packageFile)) {
        Write-Host "ERROR: Package file not created: $packageFile" -ForegroundColor Red
        exit 1
    }
    
    $packageSize = [math]::Round((Get-Item $packageFile).Length / 1KB, 2)
    Write-Host ""
    Write-Host "  Package created: $packageFile ($packageSize KB)" -ForegroundColor Green
    Write-Host ""
    
    # ============================================
    # Step 5: Test install (optional)
    # ============================================
    if ($TestInstall) {
        Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
        Write-Host "  STEP 5: Testing local installation" -ForegroundColor Cyan
        Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
        Write-Host ""
        
        Write-Host "  Installing silence from local package..." -ForegroundColor Yellow
        & choco install silence -s . -y
        
        if ($LASTEXITCODE -eq 0) {
            Write-Host ""
            Write-Host "  Test installation successful!" -ForegroundColor Green
            Write-Host "  To uninstall: choco uninstall silence -y" -ForegroundColor Cyan
            Write-Host ""
        } else {
            Write-Host "  Test installation failed!" -ForegroundColor Red
        }
    }
    
    # ============================================
    # Step 6: Push to Chocolatey.org (optional)
    # ============================================
    if (-not $SkipPush) {
        Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
        Write-Host "  STEP 6: Pushing to Chocolatey.org" -ForegroundColor Cyan
        Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
        Write-Host ""
        
        Write-Host "  Pushing $packageFile to Chocolatey.org..." -ForegroundColor Yellow
        & choco push $packageFile --source https://push.chocolatey.org/
        
        if ($LASTEXITCODE -eq 0) {
            Write-Host ""
            Write-Host "=======================================================" -ForegroundColor Green
            Write-Host "  SUCCESS! Package pushed to Chocolatey.org" -ForegroundColor Green
            Write-Host "=======================================================" -ForegroundColor Green
            Write-Host ""
            Write-Host "  Package will be available after moderation at:" -ForegroundColor Cyan
            Write-Host "  https://community.chocolatey.org/packages/silence" -ForegroundColor White
            Write-Host ""
        } else {
            Write-Host ""
            Write-Host "ERROR: Failed to push package!" -ForegroundColor Red
            Write-Host "Make sure you're authenticated with Chocolatey.org" -ForegroundColor Yellow
            Write-Host "Set API key: choco apikey --key YOUR_KEY --source https://push.chocolatey.org/" -ForegroundColor Yellow
            exit 1
        }
    } else {
        Write-Host ""
        Write-Host "=======================================================" -ForegroundColor Green
        Write-Host "  SUCCESS! Package built: $packageFile" -ForegroundColor Green
        Write-Host "=======================================================" -ForegroundColor Green
        Write-Host ""
        Write-Host "Next steps:" -ForegroundColor Cyan
        Write-Host "  Test locally:  choco install silence -s . -y" -ForegroundColor White
        Write-Host "  Uninstall:     choco uninstall silence -y" -ForegroundColor White
        Write-Host "  Push to repo:  choco push $packageFile --source https://push.chocolatey.org/" -ForegroundColor White
        Write-Host ""
    }
    
} finally {
    Pop-Location
}
