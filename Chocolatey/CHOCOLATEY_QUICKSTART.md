# 🍫 Chocolatey Quick Start Guide

## TL;DR - Fast Track

```powershell
# 1. Build package (automatically gets checksums)
.\build-chocolatey.ps1

# 2. Test locally
choco install silence -s . -y

# 3. Test the app, then uninstall
choco uninstall silence -y

# 4. Set your API key (one time only)
choco apikey --key YOUR-API-KEY --source https://push.chocolatey.org/

# 5. Push to Chocolatey
choco push silence.1.4.0.nupkg --source https://push.chocolatey.org/
```

## 📋 Detailed Steps

### 1️⃣ Build the Package

The automated script does everything:

```powershell
.\build-chocolatey.ps1
```

This will:

- Download installers from GitHub v1.4 release
- Calculate SHA256 checksums
- Update all files with correct checksums
- Build the `.nupkg` file

**For a different version:**

```powershell
.\build-chocolatey.ps1 -Version "1.5"
```

### 2️⃣ Test Locally

**IMPORTANT:** Always test before publishing!

```powershell
# Install from local directory
choco install silence -s . -y

# The app should:
# ✓ Install to Program Files
# ✓ Appear in Add/Remove Programs as "silence!"
# ✓ Launch and work correctly

# Uninstall to test cleanup
choco uninstall silence -y

# Verify:
# ✓ App is removed from Program Files
# ✓ No longer in Add/Remove Programs
```

### 3️⃣ Get Your API Key

1. Go to https://community.chocolatey.org/
2. Sign in or create account
3. Click your username → Account
4. Copy your API Key

### 4️⃣ Set API Key (One Time)

```powershell
choco apikey --key YOUR-API-KEY-HERE --source https://push.chocolatey.org/
```

This saves the key locally, you only need to do this once.

### 5️⃣ Push to Chocolatey

```powershell
choco push silence.1.4.0.nupkg --source https://push.chocolatey.org/
```

### 6️⃣ Wait for Moderation

- **First submission:** Manual review (2-7 days typically)
- Check your email for feedback from moderators
- They might ask for changes
- Once approved, future updates are usually automatic

### 7️⃣ Check Status

Go to: https://community.chocolatey.org/packages/silence

You'll see:

- Package status (pending/approved/rejected)
- Download stats
- User reviews

## 🔄 Updating for New Releases

When you release v1.5:

```powershell
# Build with new version
.\build-chocolatey.ps1 -Version "1.5"

# Test
choco install silence -s . -y
choco uninstall silence -y

# Push
choco push silence.1.5.0.nupkg --source https://push.chocolatey.org/
```

After first approval, updates are usually automatic (no manual review).

## ❓ Troubleshooting

### "Package already exists"

You can't overwrite a published package. Increment the version.

### "Checksum mismatch"

The installer file changed. Re-run `build-chocolatey.ps1` to get new checksums.

### "Invalid package"

Check `choco pack` output for errors. Common issues:

- Invalid XML in `.nuspec`
- Missing required fields
- Invalid version format

### "Installer not found on GitHub"

Make sure the release exists: https://github.com/vertopolkaLF/silence/releases/tag/v1.4

## 📞 Support

- Chocolatey Docs: https://docs.chocolatey.org/
- Chocolatey Community: https://community.chocolatey.org/
- Package Issues: https://github.com/vertopolkaLF/silence/issues

---

**That's it! Your package will be available via:**

```powershell
choco install silence
```

🎉 Congrats on publishing to Chocolatey!
