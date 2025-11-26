# silence! Release Build Script - All Architectures

$version = "1.0"
$architectures = @(
    @{ rid = "win-x64"; platform = "x64" },
    @{ rid = "win-x86"; platform = "x86" },
    @{ rid = "win-arm64"; platform = "ARM64" }
)

# Keep only these folders (required for WinUI to work)
$keepFolders = @("Assets", "Microsoft.UI.Xaml", "en-us")

Write-Host "Building silence! Release for all architectures..." -ForegroundColor Cyan
Write-Host "Architectures: x64, x86, ARM64" -ForegroundColor Cyan

# Clean previous builds
Write-Host "`nCleaning previous builds..." -ForegroundColor Yellow
if (Test-Path "bin") { Remove-Item "bin" -Recurse -Force -ErrorAction SilentlyContinue }
if (Test-Path "obj") { Remove-Item "obj" -Recurse -Force -ErrorAction SilentlyContinue }
if (Test-Path "releases") { Remove-Item "releases" -Recurse -Force -ErrorAction SilentlyContinue }

New-Item -ItemType Directory -Path "releases" -Force | Out-Null

$successCount = 0
$results = @()

foreach ($arch in $architectures) {
    $rid = $arch.rid
    $platform = $arch.platform
    
    Write-Host "`n========================================" -ForegroundColor Cyan
    Write-Host "Publishing $rid..." -ForegroundColor Yellow
    Write-Host "========================================" -ForegroundColor Cyan
    
    $publishOutput = dotnet publish -c Release -r $rid --self-contained true -p:Platform=$platform -p:PublishReadyToRun=true 2>&1
    
    if ($LASTEXITCODE -eq 0) {
        $buildPath = "bin\$platform\Release\net8.0-windows10.0.19041.0\$rid"
        $publishPath = "bin\Release\net8.0-windows10.0.19041.0\$rid\publish"
        $releasePath = "releases\silence-v$version-$rid"
        
        # Copy missing WinUI resources from build to publish (.xbf and .pri files)
        Write-Host "  Copying WinUI resources..." -ForegroundColor Gray
        Copy-Item "$buildPath\*.xbf" -Destination $publishPath -Force -ErrorAction SilentlyContinue
        Copy-Item "$buildPath\*.pri" -Destination $publishPath -Force -ErrorAction SilentlyContinue
        
        # Remove unnecessary localization folders from publish
        Write-Host "  Removing localization folders..." -ForegroundColor Gray
        Get-ChildItem $publishPath -Directory | Where-Object { $_.Name -notin $keepFolders } | ForEach-Object {
            Remove-Item $_.FullName -Recurse -Force -ErrorAction SilentlyContinue
        }
        
        # Create release folder
        New-Item -ItemType Directory -Path $releasePath -Force | Out-Null
        
        # Copy published files (including subdirectories)
        Copy-Item "$publishPath\*" -Destination $releasePath -Recurse -Force
        
        # Create ZIP archive
        $zipName = "silence-v$version-$rid.zip"
        Compress-Archive -Path "$releasePath\*" -DestinationPath "releases\$zipName" -Force
        
        # Get sizes
        $zipSize = [math]::Round((Get-Item "releases\$zipName").Length / 1MB, 2)
        $folderSize = [math]::Round((Get-ChildItem $releasePath -Recurse | Measure-Object -Property Length -Sum).Sum / 1MB, 2)
        
        Write-Host "  OK - $folderSize MB (ZIP: $zipSize MB)" -ForegroundColor Green
        $results += @{ arch = $rid; folder = $folderSize; zip = $zipSize; status = "OK" }
        $successCount++
    } else {
        Write-Host "  FAILED!" -ForegroundColor Red
        Write-Host $publishOutput -ForegroundColor Red
        $results += @{ arch = $rid; folder = 0; zip = 0; status = "FAILED" }
    }
}

# Summary
Write-Host "`n========================================" -ForegroundColor Green
Write-Host "BUILD SUMMARY" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green

foreach ($r in $results) {
    if ($r.status -eq "OK") {
        Write-Host "  $($r.arch): $($r.folder) MB (ZIP: $($r.zip) MB)" -ForegroundColor Cyan
    } else {
        Write-Host "  $($r.arch): FAILED" -ForegroundColor Red
    }
}

Write-Host "`nSuccessful builds: $successCount / $($architectures.Count)" -ForegroundColor $(if ($successCount -eq $architectures.Count) { "Green" } else { "Yellow" })
Write-Host "Output folder: releases\" -ForegroundColor Cyan

if ($successCount -gt 0) {
    Write-Host "`nTo distribute:" -ForegroundColor Yellow
    Write-Host "  Share the ZIP files from releases\ folder" -ForegroundColor White
    Write-Host "  Users extract and run silence!.exe" -ForegroundColor White
    Write-Host "  No installation required!" -ForegroundColor White
}
