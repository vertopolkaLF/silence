# silence! GitHub Release Script
# Builds all architectures, creates installers, and drafts a GitHub release
# Requires: dotnet SDK, Inno Setup 6, GitHub CLI (gh)

param(
    [switch]$SkipBuild,      # Skip dotnet publish (use existing builds)
    [switch]$SkipInstallers, # Skip installer creation
    [switch]$SkipGitHub,     # Skip GitHub release creation
    [switch]$Publish         # Publish release immediately (not draft)
)

$ErrorActionPreference = "Stop"

# ============================================
# Version Detection
# ============================================
$csprojPath = "silence!.csproj"
if (Test-Path $csprojPath) {
    [xml]$csproj = Get-Content $csprojPath
    $version = $csproj.Project.PropertyGroup.Version | Where-Object { $_ } | Select-Object -First 1
    if (-not $version) { $version = "1.0" }
} else {
    Write-Host "ERROR: silence!.csproj not found! Run from project root." -ForegroundColor Red
    exit 1
}

$tagName = "v$version"

Write-Host ""
Write-Host "=======================================================" -ForegroundColor Magenta
Write-Host "         silence! Release Builder v$version" -ForegroundColor Magenta
Write-Host "=======================================================" -ForegroundColor Magenta
Write-Host ""

# ============================================
# Step 1: Build Application
# ============================================
if (-not $SkipBuild) {
    Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
    Write-Host "  STEP 1: Building application for all architectures" -ForegroundColor Cyan
    Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
    Write-Host ""
    
    & "$PSScriptRoot\publish-release.ps1"
    
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: Build failed!" -ForegroundColor Red
        exit 1
    }
    Write-Host ""
} else {
    Write-Host "Skipping build (using existing builds)..." -ForegroundColor Yellow
}

# ============================================
# Step 2: Create Installers
# ============================================
if (-not $SkipInstallers) {
    Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
    Write-Host "  STEP 2: Creating installers" -ForegroundColor Cyan
    Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
    Write-Host ""
    
    & "$PSScriptRoot\build-installers.ps1"
    
    if ($LASTEXITCODE -ne 0) {
        Write-Host "WARNING: Some installers may have failed!" -ForegroundColor Yellow
    }
    Write-Host ""
} else {
    Write-Host "Skipping installers..." -ForegroundColor Yellow
}

# ============================================
# Step 3: Collect Release Assets
# ============================================
Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
Write-Host "  STEP 3: Collecting release assets" -ForegroundColor Cyan
Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
Write-Host ""

$releaseAssets = @()

# Collect ZIP files
$zipFiles = Get-ChildItem "releases\silence-v$version-*.zip" -ErrorAction SilentlyContinue
foreach ($zip in $zipFiles) {
    $releaseAssets += $zip.FullName
    $sizeMB = [math]::Round($zip.Length / 1MB, 2)
    Write-Host "  [ZIP] $($zip.Name) - $sizeMB MB" -ForegroundColor Green
}

# Collect installer files
$setupFiles = Get-ChildItem "releases\silence-v$version-*-setup.exe" -ErrorAction SilentlyContinue
foreach ($setup in $setupFiles) {
    $releaseAssets += $setup.FullName
    $sizeMB = [math]::Round($setup.Length / 1MB, 2)
    Write-Host "  [EXE] $($setup.Name) - $sizeMB MB" -ForegroundColor Green
}

if ($releaseAssets.Count -eq 0) {
    Write-Host "ERROR: No release assets found in releases folder!" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "Total assets: $($releaseAssets.Count)" -ForegroundColor Cyan
Write-Host ""

# ============================================
# Step 4: Create GitHub Release
# ============================================
if (-not $SkipGitHub) {
    Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
    Write-Host "  STEP 4: Creating GitHub release" -ForegroundColor Cyan
    Write-Host "-------------------------------------------------------" -ForegroundColor Cyan
    Write-Host ""
    
    # Check if gh CLI is available
    $ghExists = Get-Command gh -ErrorAction SilentlyContinue
    if (-not $ghExists) {
        Write-Host "ERROR: GitHub CLI (gh) not found!" -ForegroundColor Red
        Write-Host "Install from: https://cli.github.com/" -ForegroundColor Yellow
        Write-Host ""
        Write-Host "Assets are ready in releases folder. Create release manually:" -ForegroundColor Yellow
        foreach ($asset in $releaseAssets) {
            Write-Host "  - $(Split-Path $asset -Leaf)" -ForegroundColor White
        }
        exit 1
    }
    
    # Check if logged in
    $authStatus = gh auth status 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: Not logged into GitHub CLI!" -ForegroundColor Red
        Write-Host "Run: gh auth login" -ForegroundColor Yellow
        exit 1
    }
    
    # Generate release notes (using single quotes to avoid parsing issues)
    $releaseNotes = @'
## silence! {VERSION}

### Downloads

Choose your platform and preferred installation method:

**Windows x64** (most common)
- Installer: silence-{VERSION}-x64-setup.exe
- Portable: silence-{VERSION}-win-x64.zip

**Windows x86** (32-bit)
- Installer: silence-{VERSION}-x86-setup.exe
- Portable: silence-{VERSION}-win-x86.zip

**Windows ARM64**
- Installer: silence-{VERSION}-arm64-setup.exe
- Portable: silence-{VERSION}-win-arm64.zip

### What's New

- TODO: Add your changes here

### Installation

**Installer (recommended):** Download and run the setup.exe for your platform.

**Portable:** Extract the ZIP anywhere and run silence!.exe. No installation required.
'@
    
    # Replace version placeholder
    $releaseNotes = $releaseNotes -replace '\{VERSION\}', $tagName
    
    # Save release notes to temp file
    $notesFile = [System.IO.Path]::GetTempFileName()
    $releaseNotes | Out-File -FilePath $notesFile -Encoding utf8
    
    # Build gh command arguments
    $ghArgs = @("release", "create", $tagName)
    $ghArgs += "--title"
    $ghArgs += "silence! $tagName"
    $ghArgs += "--notes-file"
    $ghArgs += $notesFile
    
    if (-not $Publish) {
        $ghArgs += "--draft"
        Write-Host "Creating DRAFT release $tagName..." -ForegroundColor Yellow
    } else {
        Write-Host "Creating and PUBLISHING release $tagName..." -ForegroundColor Yellow
    }
    
    # Add all assets
    foreach ($asset in $releaseAssets) {
        $ghArgs += $asset
    }
    
    Write-Host ""
    
    # Execute gh release create
    & gh $ghArgs
    $ghResult = $LASTEXITCODE
    
    # Cleanup temp file
    Remove-Item $notesFile -Force -ErrorAction SilentlyContinue
    
    if ($ghResult -eq 0) {
        Write-Host ""
        Write-Host "=======================================================" -ForegroundColor Green
        if (-not $Publish) {
            Write-Host "  SUCCESS! Draft release $tagName created!" -ForegroundColor Green
            Write-Host "  Go to GitHub to edit release notes and publish." -ForegroundColor Cyan
        } else {
            Write-Host "  SUCCESS! Release $tagName published!" -ForegroundColor Green
        }
        Write-Host "=======================================================" -ForegroundColor Green
    } else {
        Write-Host "ERROR: Failed to create GitHub release!" -ForegroundColor Red
        Write-Host "Assets are ready in releases folder." -ForegroundColor Yellow
        exit 1
    }
} else {
    Write-Host "Skipping GitHub release..." -ForegroundColor Yellow
    Write-Host ""
    Write-Host "=======================================================" -ForegroundColor Green
    Write-Host "  BUILD COMPLETE!" -ForegroundColor Green
    Write-Host "=======================================================" -ForegroundColor Green
    Write-Host ""
    Write-Host "Release assets ready in releases folder:" -ForegroundColor Cyan
    foreach ($asset in $releaseAssets) {
        Write-Host "  - $(Split-Path $asset -Leaf)" -ForegroundColor White
    }
}

Write-Host ""
