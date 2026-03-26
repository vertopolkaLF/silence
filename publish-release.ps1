# silence! Release Build Script - All Architectures

# Auto-detect version from .csproj
$csprojPath = "silence!.csproj"
if (Test-Path $csprojPath) {
    [xml]$csproj = Get-Content $csprojPath
    $version = $csproj.Project.PropertyGroup.Version | Where-Object { $_ } | Select-Object -First 1
    if (-not $version) { $version = "1.0" }
} else {
    Write-Host "ERROR: silence!.csproj not found!" -ForegroundColor Red
    exit 1
}

$architectures = @(
    @{ rid = "win-x64"; platform = "x64" },
    @{ rid = "win-x86"; platform = "x86" },
    @{ rid = "win-arm64"; platform = "ARM64" }
)

$targetFramework = "net8.0-windows10.0.19041.0"

# Keep only these folders (required for WinUI to work)
$keepFolders = @("Assets", "Microsoft.UI.Xaml", "en-us", "ru-ru")

function Resolve-BuildPath {
    param(
        [string]$Platform,
        [string]$Rid
    )

    $candidates = @(
        "bin\$Platform\Release\$targetFramework\$Rid",
        "bin\Release\$targetFramework\$Rid",
        "bin\$Platform\Release\$targetFramework",
        "bin\Release\$targetFramework"
    )

    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }

    $resolved = Get-ChildItem "bin" -Directory -Recurse -ErrorAction SilentlyContinue |
        Where-Object {
            $_.FullName -like "*\$targetFramework\*" -and
            ($_.FullName -like "*\$Rid*" -or $_.FullName -like "*\$Platform*")
        } |
        Sort-Object FullName -Descending |
        Select-Object -First 1

    if ($resolved) {
        return $resolved.FullName
    }

    return $null
}

function Resolve-PublishPath {
    param(
        [string]$Platform,
        [string]$Rid
    )

    $candidates = @(
        "bin\Release\$targetFramework\$Rid\publish",
        "bin\$Platform\Release\$targetFramework\$Rid\publish",
        "bin\Release\$targetFramework\publish",
        "bin\$Platform\Release\$targetFramework\publish"
    )

    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }

    $resolved = Get-ChildItem "bin" -Directory -Recurse -ErrorAction SilentlyContinue |
        Where-Object {
            $_.Name -eq "publish" -and
            $_.FullName -like "*\$targetFramework\*" -and
            ($_.FullName -like "*\$Rid*" -or $_.FullName -like "*\$Platform*")
        } |
        Sort-Object FullName -Descending |
        Select-Object -First 1

    if ($resolved) {
        return $resolved.FullName
    }

    return $null
}

function Invoke-WithRetry {
    param(
        [scriptblock]$Operation,
        [string]$Description,
        [int]$MaxAttempts = 5,
        [int]$DelaySeconds = 2
    )

    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
        try {
            & $Operation
            return $true
        }
        catch {
            if ($attempt -eq $MaxAttempts) {
                Write-Host "  $Description failed after $MaxAttempts attempts: $($_.Exception.Message)" -ForegroundColor Red
                return $false
            }

            Write-Host "  $Description failed on attempt ${attempt}/${MaxAttempts}: $($_.Exception.Message). Retrying..." -ForegroundColor Yellow
            Start-Sleep -Seconds $DelaySeconds
        }
    }

    return $false
}

Write-Host "Building silence! v$version for all architectures..." -ForegroundColor Cyan
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
    
    $publishOutput = & dotnet publish -c Release -r $rid --self-contained true -p:Platform=$platform -p:PublishReadyToRun=true 2>&1
    
    if ($LASTEXITCODE -eq 0) {
        $buildPath = Resolve-BuildPath -Platform $platform -Rid $rid
        $publishPath = Resolve-PublishPath -Platform $platform -Rid $rid
        $releasePath = "releases\silence-v$version-$rid"

        if (-not $publishPath) {
            Write-Host "  FAILED - Publish output folder not found for $rid" -ForegroundColor Red
            $results += @{ arch = $rid; folder = 0; zip = 0; status = "FAILED" }
            continue
        }
        
        # Copy missing WinUI resources from build to publish (.xbf and .pri files)
        Write-Host "  Copying WinUI resources..." -ForegroundColor Gray
        if ($buildPath) {
            Copy-Item (Join-Path $buildPath "*.xbf") -Destination $publishPath -Force -ErrorAction SilentlyContinue
            Copy-Item (Join-Path $buildPath "*.pri") -Destination $publishPath -Force -ErrorAction SilentlyContinue
        }
        
        # Remove unnecessary localization folders from publish
        Write-Host "  Removing localization folders..." -ForegroundColor Gray
        Get-ChildItem $publishPath -Directory | Where-Object { $_.Name -notin $keepFolders } | ForEach-Object {
            Remove-Item $_.FullName -Recurse -Force -ErrorAction SilentlyContinue
        }
        
        # Create release folder
        New-Item -ItemType Directory -Path $releasePath -Force | Out-Null
        
        # Copy published files (including subdirectories)
        Copy-Item "$publishPath\*" -Destination $releasePath -Recurse -Force
        
        $zipName = "silence-v$version-$rid.zip"
        $zipPath = "releases\$zipName"
        $archiveCreated = Invoke-WithRetry -Description "Creating ZIP archive for $rid" -Operation {
            if (Test-Path $zipPath) {
                Remove-Item $zipPath -Force -ErrorAction SilentlyContinue
            }

            Compress-Archive -Path "$releasePath\*" -DestinationPath $zipPath -Force -ErrorAction Stop
        }

        if (-not $archiveCreated -or -not (Test-Path $zipPath)) {
            $results += @{ arch = $rid; folder = 0; zip = 0; status = "FAILED" }
            continue
        }

        # Get sizes
        $zipSize = [math]::Round((Get-Item $zipPath).Length / 1MB, 2)
        $folderSize = [math]::Round((Get-ChildItem $releasePath -Recurse | Measure-Object -Property Length -Sum).Sum / 1MB, 2)
        
        Write-Host "  OK - $folderSize MB (ZIP: $zipSize MB)" -ForegroundColor Green
        $results += @{ arch = $rid; folder = $folderSize; zip = $zipSize; status = "OK" }
        $successCount++
    } else {
        Write-Host "  FAILED!" -ForegroundColor Red
        $publishOutput | ForEach-Object { Write-Host "    $_" -ForegroundColor Red }
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
