# Build installers for all architectures using Inno Setup
# Make sure Inno Setup is installed and iscc.exe is in PATH
# or use full path like: "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"

param(
    [string]$IsccPath = "iscc.exe"
)

$architectures = @("x64", "x86", "arm64")

# Auto-detect version from .csproj
$csprojPath = "silence!.csproj"
if (Test-Path $csprojPath) {
    [xml]$csproj = Get-Content $csprojPath
    $version = $csproj.Project.PropertyGroup.Version | Where-Object { $_ } | Select-Object -First 1
    if (-not $version) { $version = "1.0" }
} else {
    $version = "1.1"
}

Write-Host "Building silence! v$version installers..." -ForegroundColor Cyan
Write-Host ""

# Check if iscc is available
try {
    $null = & $IsccPath /? 2>&1
}
catch {
    # Try common installation paths
    $commonPaths = @(
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
    )
    
    $found = $false
    foreach ($path in $commonPaths) {
        if (Test-Path $path) {
            $IsccPath = $path
            $found = $true
            Write-Host "Found Inno Setup at: $path" -ForegroundColor Green
            break
        }
    }
    
    if (-not $found) {
        Write-Host "ERROR: Inno Setup compiler (iscc.exe) not found!" -ForegroundColor Red
        Write-Host "Please install Inno Setup 6 from: https://jrsoftware.org/isdl.php" -ForegroundColor Yellow
        Write-Host "Or provide path: .\build-installers.ps1 -IsccPath 'C:\path\to\ISCC.exe'" -ForegroundColor Yellow
        exit 1
    }
}

$successCount = 0
$results = @()

foreach ($arch in $architectures) {
    $issFile = "installer\silence-$arch.iss"
    
    Write-Host "Building $arch installer..." -ForegroundColor Yellow
    
    if (-not (Test-Path $issFile)) {
        Write-Host "  SKIPPED - $issFile not found" -ForegroundColor Red
        $results += @{ arch = $arch; status = "SKIPPED"; size = 0 }
        continue
    }
    
    # Check if release folder exists
    $releaseFolder = "releases\silence-v$version-win-$arch"
    if (-not (Test-Path $releaseFolder)) {
        Write-Host "  SKIPPED - Release folder not found: $releaseFolder" -ForegroundColor Red
        $results += @{ arch = $arch; status = "SKIPPED"; size = 0 }
        continue
    }
    
    # Build installer with version parameter
    $output = & $IsccPath "/DMyAppVersion=$version" $issFile 2>&1
    
    if ($LASTEXITCODE -eq 0) {
        $setupFile = "releases\silence-v$version-$arch-setup.exe"
        if (Test-Path $setupFile) {
            $size = [math]::Round((Get-Item $setupFile).Length / 1MB, 2)
            Write-Host "  OK - $size MB" -ForegroundColor Green
            $results += @{ arch = $arch; status = "OK"; size = $size }
            $successCount++
        }
        else {
            Write-Host "  WARNING - Build succeeded but output file not found" -ForegroundColor Yellow
            $results += @{ arch = $arch; status = "WARNING"; size = 0 }
        }
    }
    else {
        Write-Host "  FAILED!" -ForegroundColor Red
        Write-Host $output -ForegroundColor Red
        $results += @{ arch = $arch; status = "FAILED"; size = 0 }
    }
}

# Summary
Write-Host ""
Write-Host "========================================" -ForegroundColor Green
Write-Host "INSTALLER BUILD SUMMARY" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green

foreach ($r in $results) {
    $statusColor = switch ($r.status) {
        "OK" { "Green" }
        "WARNING" { "Yellow" }
        default { "Red" }
    }
    
    if ($r.status -eq "OK") {
        Write-Host "  $($r.arch): $($r.size) MB" -ForegroundColor $statusColor
    }
    else {
        Write-Host "  $($r.arch): $($r.status)" -ForegroundColor $statusColor
    }
}

Write-Host ""
Write-Host "Successful builds: $successCount / $($architectures.Count)" -ForegroundColor $(if ($successCount -eq $architectures.Count) { "Green" } else { "Yellow" })

if ($successCount -gt 0) {
    Write-Host ""
    Write-Host "Installers created in: releases\" -ForegroundColor Cyan
    Get-ChildItem "releases\*-Setup.exe" | ForEach-Object {
        Write-Host "  - $($_.Name)" -ForegroundColor White
    }
}

