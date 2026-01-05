# silence! Chocolatey Package

This directory contains the Chocolatey package definition for **silence!** - a microphone mute toggle application for Windows.

## 📦 Package Structure

```
silence!\
├── silence!.nuspec          # Package metadata
├── tools\
│   ├── chocolateyinstall.ps1       # Installation script
│   ├── chocolateyuninstall.ps1     # Uninstallation script
│   ├── chocolateybeforemodify.ps1  # Pre-upgrade/uninstall script
│   ├── LICENSE.txt                 # License information
│   └── VERIFICATION.txt            # Verification instructions
└── ReadMe.md                # This file
```

## 🚀 Building the Package

### Prerequisites

1. Install Chocolatey: https://chocolatey.org/install
2. Ensure you have PowerShell 5.0 or later

### Step 1: Get Checksums

Before building, you need to get SHA256 checksums for the installers:

```powershell
# Download installers from GitHub release
$version = "1.4"
$baseUrl = "https://github.com/vertopolkaLF/silence/releases/download/v$version"

# Download x86 installer
Invoke-WebRequest -Uri "$baseUrl/silence-x86-installer.exe" -OutFile "silence-x86-installer.exe"
$checksum32 = (Get-FileHash -Algorithm SHA256 "silence-x86-installer.exe").Hash
Write-Host "x86 Checksum: $checksum32"

# Download x64 installer
Invoke-WebRequest -Uri "$baseUrl/silence-x64-installer.exe" -OutFile "silence-x64-installer.exe"
$checksum64 = (Get-FileHash -Algorithm SHA256 "silence-x64-installer.exe").Hash
Write-Host "x64 Checksum: $checksum64"

# Clean up
Remove-Item "silence-x86-installer.exe", "silence-x64-installer.exe"
```

### Step 2: Update Checksums

Edit `tools\chocolateyinstall.ps1` and replace the empty checksum values:

```powershell
checksum      = 'YOUR_X86_CHECKSUM_HERE'
checksum64    = 'YOUR_X64_CHECKSUM_HERE'
```

Also update `tools\VERIFICATION.txt` with the checksums.

### Step 3: Build Package

```powershell
# Navigate to this directory
cd C:\Users\leo20\silence!

# Build the package
choco pack

# This creates: silence.1.4.0.nupkg
```

## 🧪 Testing Locally

Before publishing, test the package locally:

```powershell
# Install from local package
choco install silence -s . -y

# Test the application
# - Check if it installed correctly
# - Verify it appears in Add/Remove Programs
# - Test the application functionality

# Uninstall
choco uninstall silence -y
```

## 📤 Publishing to Chocolatey Community

### Step 1: Create Account

1. Go to https://community.chocolatey.org/
2. Create an account or sign in
3. Go to your account settings to get your API key

### Step 2: Set API Key

```powershell
choco apikey --key YOUR-API-KEY-HERE --source https://push.chocolatey.org/
```

### Step 3: Push Package

```powershell
choco push silence.1.4.0.nupkg --source https://push.chocolatey.org/
```

### Step 4: Wait for Moderation

- First package submission goes through manual moderation
- Can take several days
- Check your email for moderation feedback
- Once approved, updates are usually automatic

## 🔄 Updating for New Versions

When releasing a new version:

1. Update version in `silence!.nuspec` (e.g., `1.5.0`)
2. Update URLs in `tools\chocolateyinstall.ps1` to point to new release
3. Get new checksums (see Step 1 above)
4. Update checksums in `tools\chocolateyinstall.ps1` and `tools\VERIFICATION.txt`
5. Update `releaseNotes` URL in `silence!.nuspec`
6. Build and test
7. Push to Chocolatey

## 📋 Important Notes

### Package ID

- Package ID is `silence` (without the exclamation mark)
- Chocolatey doesn't allow special characters in package IDs
- The display name is still "silence!" with the exclamation mark

### Installer Requirements

- Installers must support silent installation
- Using Inno Setup with `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-` flags
- Must not require user interaction

### Community Repository Guidelines

- Package must be under 200MB (we download installers at runtime, so we're good)
- Must include checksums for downloaded files
- Must include LICENSE.txt and VERIFICATION.txt
- Description should be clear and helpful
- Tags help users find the package

## 🛠️ Automation (Optional)

You can automate package creation with a script:

```powershell
# build-chocolatey.ps1
param(
    [Parameter(Mandatory=$true)]
    [string]$Version
)

$ErrorActionPreference = 'Stop'

# Update version in nuspec
$nuspecPath = "silence!.nuspec"
$nuspec = [xml](Get-Content $nuspecPath)
$nuspec.package.metadata.version = $Version
$nuspec.Save($nuspecPath)

# Get checksums
$baseUrl = "https://github.com/vertopolkaLF/silence/releases/download/v$Version"
Write-Host "Downloading installers to calculate checksums..."

Invoke-WebRequest -Uri "$baseUrl/silence-x86-installer.exe" -OutFile "temp-x86.exe"
Invoke-WebRequest -Uri "$baseUrl/silence-x64-installer.exe" -OutFile "temp-x64.exe"

$checksum32 = (Get-FileHash -Algorithm SHA256 "temp-x86.exe").Hash
$checksum64 = (Get-FileHash -Algorithm SHA256 "temp-x64.exe").Hash

Remove-Item "temp-x86.exe", "temp-x64.exe"

Write-Host "x86 Checksum: $checksum32"
Write-Host "x64 Checksum: $checksum64"
Write-Host ""
Write-Host "Update these checksums in tools\chocolateyinstall.ps1 and tools\VERIFICATION.txt"
Write-Host ""

# Build package
choco pack

Write-Host "Package built: silence.$Version.nupkg"
```

## 📚 Resources

- [Chocolatey Package Creation Docs](https://docs.chocolatey.org/en-us/create/create-packages)
- [Chocolatey Functions Reference](https://docs.chocolatey.org/en-us/create/functions)
- [Chocolatey Community Repository Guidelines](https://docs.chocolatey.org/en-us/community-repository/moderation/)

## 🤝 Support

If you encounter issues with the Chocolatey package:

- Open an issue on GitHub: https://github.com/vertopolkaLF/silence/issues
- Tag it with `chocolatey` label
