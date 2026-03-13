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

$scriptPath = Join-Path $PSScriptRoot "..\build-chocolatey-release.ps1"
& $scriptPath @PSBoundParameters
