param(
    [Parameter(Mandatory=$false)]
    [string]$Version = "1.4"
)

$ErrorActionPreference = 'Stop'

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Building Chocolatey Package for silence! v$Version" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Get checksums from GitHub release
$baseUrl = "https://github.com/vertopolkaLF/silence/releases/download/v$Version"
Write-Host "[1/4] Downloading installers to calculate checksums..." -ForegroundColor Yellow

try {
    $tempX86 = Join-Path $env:TEMP "silence-v1.4-x86-temp.exe"
    $tempX64 = Join-Path $env:TEMP "silence-v1.4-x64-temp.exe"
    
    Write-Host "  - Downloading x86 installer..." -ForegroundColor Gray
    Invoke-WebRequest -Uri "$baseUrl/silence-v1.4-x86-setup.exe" -OutFile $tempX86 -UseBasicParsing
    
    Write-Host "  - Downloading x64 installer..." -ForegroundColor Gray
    Invoke-WebRequest -Uri "$baseUrl/silence-v1.4-x64-setup.exe" -OutFile $tempX64 -UseBasicParsing
    
    Write-Host "[2/4] Calculating checksums..." -ForegroundColor Yellow
    $checksum32 = (Get-FileHash -Algorithm SHA256 $tempX86).Hash
    $checksum64 = (Get-FileHash -Algorithm SHA256 $tempX64).Hash
    
    Write-Host "  - x86 Checksum: $checksum32" -ForegroundColor Green
    Write-Host "  - x64 Checksum: $checksum64" -ForegroundColor Green
    
    Remove-Item $tempX86, $tempX64 -ErrorAction SilentlyContinue
    
} catch {
    Write-Host "Error downloading installers: $_" -ForegroundColor Red
    Write-Host "Make sure the release v$Version exists on GitHub" -ForegroundColor Red
    exit 1
}

# Update chocolateyinstall.ps1 with checksums
Write-Host "[3/4] Updating chocolateyinstall.ps1 with checksums..." -ForegroundColor Yellow
$installScript = Get-Content "tools\chocolateyinstall.ps1" -Raw
$installScript = $installScript -replace "checksum\s*=\s*'[^']*'", "checksum      = '$checksum32'"
$installScript = $installScript -replace "checksum64\s*=\s*'[^']*'", "checksum64    = '$checksum64'"
$installScript = $installScript -replace "v[\d\.]+/", "v$Version/"
Set-Content "tools\chocolateyinstall.ps1" -Value $installScript -NoNewline

# Update nuspec version
$nuspecPath = "silence!.nuspec"
$nuspec = [xml](Get-Content $nuspecPath)
$nuspec.package.metadata.version = "$Version.0"
$nuspec.package.metadata.releaseNotes = "https://github.com/vertopolkaLF/silence/releases/tag/v$Version"
$nuspec.Save($nuspecPath)

Write-Host "  - Updated version to $Version.0" -ForegroundColor Green
Write-Host "  - Updated checksums" -ForegroundColor Green

# Build package
Write-Host "[4/4] Building Chocolatey package..." -ForegroundColor Yellow
choco pack

if ($LASTEXITCODE -eq 0) {
    Write-Host ""
    Write-Host "========================================" -ForegroundColor Green
    Write-Host "SUCCESS! Package built: silence.$Version.0.nupkg" -ForegroundColor Green
    Write-Host "========================================" -ForegroundColor Green
    Write-Host ""
    Write-Host "Next steps:" -ForegroundColor Cyan
    Write-Host "  1. Test locally:  choco install silence -s . -y" -ForegroundColor White
    Write-Host "  2. Uninstall:     choco uninstall silence -y" -ForegroundColor White
    Write-Host "  3. Push to repo:  choco push silence.$Version.0.nupkg --source https://push.chocolatey.org/" -ForegroundColor White
    Write-Host ""
} else {
    Write-Host "Failed to build package!" -ForegroundColor Red
    exit 1
}
