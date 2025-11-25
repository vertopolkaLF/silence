# Silence! Release Build Script

Write-Host "Building Silence! Release..." -ForegroundColor Cyan

# Clean previous builds
Write-Host "`nCleaning previous builds..." -ForegroundColor Yellow
if (Test-Path "bin\Release") {
    Remove-Item "bin\Release" -Recurse -Force
}
if (Test-Path "obj\Release") {
    Remove-Item "obj\Release" -Recurse -Force
}

# Publish x64
Write-Host "`nPublishing x64 release..." -ForegroundColor Yellow
dotnet publish -c Release -r win-x64 --self-contained true -p:PublishReadyToRun=true

if ($LASTEXITCODE -eq 0) {
    $publishPath = "bin\Release\net8.0-windows10.0.19041.0\win-x64\publish"
    $releasePath = "releases\Silence-v1.0-win-x64"
    
    Write-Host "`nCreating release package..." -ForegroundColor Yellow
    
    # Create releases folder
    if (Test-Path "releases") {
        Remove-Item "releases" -Recurse -Force
    }
    New-Item -ItemType Directory -Path $releasePath -Force | Out-Null
    
    # Copy published files
    Copy-Item "$publishPath\*" -Destination $releasePath -Recurse -Force
    
    # Create ZIP archive
    $zipName = "Silence-v1.0-win-x64.zip"
    Write-Host "`nCreating ZIP archive: $zipName" -ForegroundColor Yellow
    Compress-Archive -Path "$releasePath\*" -DestinationPath "releases\$zipName" -Force
    
    # Get size
    $size = (Get-Item "releases\$zipName").Length / 1MB
    $folderSize = (Get-ChildItem $releasePath -Recurse | Measure-Object -Property Length -Sum).Sum / 1MB
    
    Write-Host "`n========================================" -ForegroundColor Green
    Write-Host "Release build completed successfully!" -ForegroundColor Green
    Write-Host "========================================" -ForegroundColor Green
    Write-Host "`nRelease folder: $releasePath" -ForegroundColor Cyan
    Write-Host "Folder size: $([math]::Round($folderSize, 2)) MB" -ForegroundColor Cyan
    Write-Host "`nZIP archive: releases\$zipName" -ForegroundColor Cyan
    Write-Host "ZIP size: $([math]::Round($size, 2)) MB" -ForegroundColor Cyan
    Write-Host "`nTo distribute:" -ForegroundColor Yellow
    Write-Host "  1. Share the ZIP file" -ForegroundColor White
    Write-Host "  2. Users extract and run Silence!.exe" -ForegroundColor White
    Write-Host "  3. No installation required!" -ForegroundColor White
    
} else {
    Write-Host "`nBuild failed!" -ForegroundColor Red
    exit 1
}

