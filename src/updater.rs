//! In-place updates from GitHub Releases portable zip assets.

use std::{
    fs,
    io::{Read, Seek, Write, copy},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::StreamExt;
use once_cell::sync::Lazy;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use zip::ZipArchive;

const GITHUB_API_ROOT: &str = "https://api.github.com/repos";
const DEFAULT_UPDATE_REPO: &str = "vertopolkaLF/silence";
const USER_AGENT: &str = concat!("silence-updater/", env!("CARGO_PKG_VERSION"));
const APP_EXE: &str = "silence.exe";
/// Uninstall entry written by the NSIS installer (`bundle.identifier`).
const UNINSTALL_KEY: &str =
    r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\fun.silencemute";

static INSTALLING: AtomicBool = AtomicBool::new(false);
static LAST_PROMPTED_TAG: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateConfig {
    pub owner: String,
    pub repo: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: String,
    pub tag_name: String,
    pub release_url: String,
    pub asset_name: String,
    pub download_url: String,
    pub asset_size: u64,
    /// Lowercase hex SHA-256 reported by the GitHub API, if any.
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateCheck {
    UpToDate,
    Available(UpdateInfo),
}

#[derive(Clone, Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<GitHubAsset>,
}

#[derive(Clone, Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

impl UpdateConfig {
    pub fn from_env() -> Self {
        let repo = std::env::var("SILENCE_UPDATE_REPO")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_UPDATE_REPO.to_string());
        Self::from_repo_slug(&repo).unwrap_or_else(|| Self {
            owner: "vertopolkaLF".to_string(),
            repo: "silence".to_string(),
        })
    }

    fn from_repo_slug(value: &str) -> Option<Self> {
        let mut parts = value.trim().split('/');
        let owner = parts.next()?.trim();
        let repo = parts.next()?.trim();
        if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
            return None;
        }
        Some(Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
        })
    }

    fn latest_release_url(&self) -> String {
        format!(
            "{GITHUB_API_ROOT}/{}/{}/releases/latest",
            self.owner, self.repo
        )
    }
}

pub async fn check_for_update() -> Result<UpdateCheck> {
    check_for_update_with_config(UpdateConfig::from_env()).await
}

pub async fn check_for_update_with_config(config: UpdateConfig) -> Result<UpdateCheck> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .https_only(true)
        .build()
        .context("create update http client")?;
    let release = client
        .get(config.latest_release_url())
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("request latest GitHub release")?
        .error_for_status()
        .context("latest GitHub release response")?
        .json::<GitHubRelease>()
        .await
        .context("parse latest GitHub release")?;

    update_from_release(release, current_arch())
}

pub async fn download_update(
    update: &UpdateInfo,
    mut on_progress: impl FnMut(f32),
) -> Result<DownloadedUpdate> {
    require_standard_update()?;
    let expected = update
        .sha256
        .as_deref()
        .context("release package has no valid SHA-256")?;
    let run = DownloadedUpdate::new()?;
    let part_path = run.directory.join("package.zip");

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .https_only(true)
        .build()
        .context("create download http client")?;
    let response = client
        .get(&update.download_url)
        .send()
        .await
        .context("request update package")?
        .error_for_status()
        .context("update package response")?;
    let total = response
        .content_length()
        .unwrap_or(update.asset_size)
        .max(1);
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&part_path)
        .await
        .context("create partial update package")?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0_u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("download update chunk")?;
        file.write_all(&chunk).await.context("write update chunk")?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;
        on_progress(((downloaded as f32 / total as f32) * 100.0).clamp(0.0, 100.0));
    }

    file.flush().await.context("flush update package")?;
    drop(file);

    anyhow::ensure!(
        downloaded > 0 && (update.asset_size == 0 || downloaded == update.asset_size),
        "downloaded update size mismatch"
    );
    anyhow::ensure!(
        hex(&hasher.finalize()).eq_ignore_ascii_case(expected),
        "update package checksum mismatch"
    );
    on_progress(100.0);
    Ok(run)
}

pub struct DownloadedUpdate {
    directory: PathBuf,
    handed_off: bool,
}

impl DownloadedUpdate {
    fn new() -> Result<Self> {
        let id = unsafe { windows::Win32::System::Com::CoCreateGuid()? };
        let directory = std::env::temp_dir().join(format!("silence-update-{id:?}"));
        fs::create_dir(&directory).context("create update directory")?;
        Ok(Self {
            directory,
            handed_off: false,
        })
    }
}

impl Drop for DownloadedUpdate {
    fn drop(&mut self) {
        if !self.handed_off {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }
}

fn require_standard_update() -> Result<()> {
    anyhow::ensure!(
        crate::in_app_updates_allowed()?,
        "Use View release to update when silence! is running as administrator or has a legacy elevated startup task."
    );
    Ok(())
}

/// Unpacks the downloaded package and hands it to a hidden helper that
/// replaces the install folder once silence! has exited. The caller must exit
/// all silence! processes right after this returns.
pub fn install_update(update: &UpdateInfo, mut package: DownloadedUpdate) -> Result<()> {
    require_standard_update()?;
    if INSTALLING.swap(true, Ordering::Relaxed) {
        anyhow::bail!("update installation is already running");
    }
    let result = stage_update(update, &package);
    if result.is_ok() {
        package.handed_off = true;
    } else {
        INSTALLING.store(false, Ordering::Relaxed);
    }
    result
}

fn stage_update(update: &UpdateInfo, package: &DownloadedUpdate) -> Result<()> {
    let install_dir = install_dir()?;
    let expected = update
        .sha256
        .as_deref()
        .context("release package has no valid SHA-256")?;
    // Deny writes and replacement while checking and reading the same archive.
    let mut file = fs::OpenOptions::new()
        .read(true)
        .share_mode(1) // FILE_SHARE_READ
        .open(package.directory.join("package.zip"))
        .context("open downloaded update")?;
    let mut hasher = Sha256::new();
    copy(&mut Read::by_ref(&mut file), &mut hasher).context("verify downloaded update")?;
    anyhow::ensure!(
        hex(&hasher.finalize()).eq_ignore_ascii_case(expected),
        "update package checksum mismatch"
    );
    file.rewind().context("rewind verified update")?;

    let staged = package.directory.join(APP_EXE);
    let executable_hash = extract_executable(file, &staged)?;
    let marker = serde_json::to_string(&crate::LastUpdateNotification {
        version: update.version.clone(),
        release_url: update.release_url.clone(),
        shown: false,
    })?;
    let script = helper_script(
        &install_dir,
        &package.directory,
        &update.version,
        &marker,
        &executable_hash,
    );
    spawn_hidden_helper(&script)
}

fn helper_script(
    install_dir: &Path,
    source_dir: &Path,
    version: &str,
    marker: &str,
    executable_hash: &str,
) -> String {
    format!(
        r#"$ErrorActionPreference = 'Stop'
$install = '{install}'
$source = '{source}'
$exe = Join-Path $install '{APP_EXE}'
$version = '{version}'
$markerJson = '{marker}'
$expectedHash = '{executable_hash}'
$next = Join-Path $install ('.silence-update-' + [guid]::NewGuid() + '.exe')
$backup = $next + '.bak'
$lock = [System.Threading.Mutex]::new($false, 'Local\SilenceV2Update')
$owned = $false
$exited = $false
try {{
    try {{ $owned = $lock.WaitOne(0) }} catch [System.Threading.AbandonedMutexException] {{ $owned = $true }}
    if (!$owned) {{ return }}

    $deadline = (Get-Date).AddSeconds(20)
    while (@(Get-Process -Name 'silence' -ErrorAction SilentlyContinue |
        Where-Object {{ $_.Path -and ($_.Path -ieq $exe) }}).Count -gt 0) {{
        if ((Get-Date) -gt $deadline) {{ throw 'silence! did not exit before the update timeout' }}
        Start-Sleep -Milliseconds 250
    }}
    $exited = $true

    [IO.File]::Copy((Join-Path $source '{APP_EXE}'), $next)
    if ((Get-FileHash -LiteralPath $next -Algorithm SHA256).Hash -ine $expectedHash) {{
        throw 'Staged executable checksum mismatch'
    }}
    for ($attempt = 1; ; $attempt++) {{
        try {{
            [IO.File]::Replace($next, $exe, $backup)
            break
        }} catch [System.IO.IOException] {{
            if ($attempt -ge 10 -or !(Test-Path -LiteralPath $next) -or
                !(Test-Path -LiteralPath $exe) -or (Test-Path -LiteralPath $backup)) {{ throw }}
            Start-Sleep -Milliseconds 250
        }}
    }}

    $stateDir = Join-Path $env:APPDATA 'SilenceV2'
    [IO.Directory]::CreateDirectory($stateDir) | Out-Null
    [IO.File]::WriteAllText((Join-Path $stateDir 'last_update.json'), $markerJson)

    $key = '{UNINSTALL_KEY}'
    $entry = Get-ItemProperty -LiteralPath $key -ErrorAction SilentlyContinue
    if ($entry -and $entry.InstallLocation -and
        ($entry.InstallLocation.TrimEnd('\') -ieq $install.TrimEnd('\'))) {{
        Set-ItemProperty -LiteralPath $key -Name DisplayVersion -Value $version
    }}
}} catch {{
    # ReplaceFile can leave the original at the backup path on a partial failure.
    if (!(Test-Path -LiteralPath $exe) -and (Test-Path -LiteralPath $backup)) {{
        [IO.File]::Move($backup, $exe)
    }}
    $log = Join-Path $env:TEMP 'silence-update-error.log'
    "$(Get-Date -Format o) $($_ | Out-String)" | Add-Content -LiteralPath $log -Encoding utf8
}} finally {{
    Remove-Item -LiteralPath $next -Force -ErrorAction SilentlyContinue
    if ($exited -and (Test-Path -LiteralPath $exe)) {{
        Remove-Item -LiteralPath $backup -Force -ErrorAction SilentlyContinue
        Start-Process -FilePath $exe -WorkingDirectory $install
    }}
    if ($owned) {{ $lock.ReleaseMutex() }}
    $lock.Dispose()
    Remove-Item -LiteralPath $source -Recurse -Force -ErrorAction SilentlyContinue
}}
"#,
        install = ps_quote(&install_dir.display().to_string()),
        source = ps_quote(&source_dir.display().to_string()),
        version = ps_quote(version.trim_start_matches(['v', 'V'])),
        marker = ps_quote(marker),
    )
}

#[cfg(windows)]
fn spawn_hidden_helper(script: &str) -> Result<()> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let utf16le: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    Command::new(crate::system_exe(r"WindowsPowerShell\v1.0\powershell.exe"))
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-EncodedCommand",
        ])
        .arg(STANDARD.encode(utf16le))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .context("spawn update helper")
}

#[cfg(not(windows))]
fn spawn_hidden_helper(_script: &str) -> Result<()> {
    anyhow::bail!("in-place updates are only supported on Windows")
}

/// Escapes a value for a single-quoted PowerShell string.
fn ps_quote(value: &str) -> String {
    value.replace('\'', "''")
}

/// Releases ship an embedded executable, optionally inside one wrapper directory.
fn extract_executable(file: fs::File, target: &Path) -> Result<String> {
    let mut archive = ZipArchive::new(file).context("open update package")?;
    let mut matches = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .context("read update package entry")?;
        if entry.is_file()
            && let Some(path) = entry.enclosed_name()
            && path.components().count() <= 2
            && path
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case(APP_EXE))
        {
            matches.push(index);
        }
    }
    anyhow::ensure!(
        matches.len() == 1,
        "update package must contain exactly one {APP_EXE}"
    );
    let mut entry = archive
        .by_index(matches[0])
        .context("read update executable")?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = entry
            .read(&mut buffer)
            .context("extract update executable")?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .context("write staged executable")?;
        hasher.update(&buffer[..read]);
    }
    output.sync_all().context("flush update executable")?;
    Ok(hex(&hasher.finalize()))
}

fn install_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("resolve current executable")?;
    exe.parent()
        .map(Path::to_path_buf)
        .context("executable has no parent directory")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Extracts the lowercase hex SHA-256 from a GitHub asset `digest` field.
fn parse_sha256_digest(digest: &str) -> Option<String> {
    let hex = digest.trim().strip_prefix("sha256:")?;
    (hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| hex.to_ascii_lowercase())
}

pub fn should_prompt_update(update: &UpdateInfo) -> bool {
    let mut prompted = LAST_PROMPTED_TAG.lock().unwrap();
    if prompted.as_deref() == Some(update.tag_name.as_str()) {
        return false;
    }
    prompted.replace(update.tag_name.clone());
    true
}

pub fn current_version_text() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

fn update_from_release(release: GitHubRelease, arch: &str) -> Result<UpdateCheck> {
    anyhow::ensure!(!release.draft, "latest release is a draft");
    let latest = parse_version(&release.tag_name)
        .with_context(|| format!("parse release version {}", release.tag_name))?;
    let current = parse_version(env!("CARGO_PKG_VERSION")).context("parse current app version")?;
    if release.prerelease || latest <= current {
        return Ok(UpdateCheck::UpToDate);
    }
    let asset = select_portable_asset(&release.assets, arch)
        .with_context(|| format!("find windows {arch} portable zip asset"))?;
    Ok(UpdateCheck::Available(UpdateInfo {
        version: format!("v{latest}"),
        tag_name: release.tag_name,
        release_url: release.html_url,
        asset_name: asset.name,
        download_url: asset.browser_download_url,
        asset_size: asset.size,
        sha256: asset.digest.as_deref().and_then(parse_sha256_digest),
    }))
}

fn parse_version(value: &str) -> Result<Version> {
    let value = value.trim().trim_start_matches('v').trim_start_matches('V');
    let mut parts = value.split('.').collect::<Vec<_>>();
    while parts.len() < 3 {
        parts.push("0");
    }
    Version::parse(&parts.join(".")).context("parse semver")
}

fn current_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        std::env::consts::ARCH
    }
}

fn select_portable_asset(assets: &[GitHubAsset], arch: &str) -> Option<GitHubAsset> {
    let suffix = format!("-windows-{arch}-portable.zip");
    assets
        .iter()
        .find(|asset| asset.name.to_ascii_lowercase().ends_with(&suffix))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> GitHubAsset {
        GitHubAsset {
            name: name.to_string(),
            browser_download_url: format!("https://example.invalid/{name}"),
            size: 42,
            digest: None,
        }
    }

    #[test]
    fn parses_short_and_prefixed_versions() {
        assert_eq!(parse_version("v2.1").unwrap(), Version::new(2, 1, 0));
        assert_eq!(parse_version("2.1.0").unwrap(), Version::new(2, 1, 0));
    }

    #[test]
    fn prerelease_is_below_stable() {
        assert!(parse_version("2.1.0-alpha.1").unwrap() < Version::new(2, 1, 0));
    }

    #[test]
    fn selects_portable_for_arch_not_setup() {
        let assets = vec![
            asset("silence-2.1.0-windows-x64-setup.exe"),
            asset("silence-2.1.0-windows-x86-portable.zip"),
            asset("silence-2.1.0-windows-x64-portable.zip"),
        ];
        let selected = select_portable_asset(&assets, "x64").unwrap();
        assert_eq!(selected.name, "silence-2.1.0-windows-x64-portable.zip");
    }

    #[test]
    fn missing_portable_is_none() {
        let assets = vec![asset("silence-2.1.0-windows-x64-setup.exe")];
        assert!(select_portable_asset(&assets, "x64").is_none());
    }

    #[test]
    fn parses_sha256_digest() {
        let hex = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(
            parse_sha256_digest(&format!("sha256:{}", hex.to_ascii_uppercase())).as_deref(),
            Some(hex)
        );
        assert_eq!(parse_sha256_digest("sha1:abc"), None);
        assert_eq!(parse_sha256_digest("sha256:zz"), None);
    }

    #[test]
    fn helper_script_quotes_paths_and_strips_version_prefix() {
        let script = helper_script(
            Path::new(r"C:\Users\O'Neil\silence"),
            Path::new(r"C:\Temp\payload"),
            "v2.5.0",
            "{}",
            "digest",
        );
        assert!(script.contains(r"$install = 'C:\Users\O''Neil\silence'"));
        assert!(script.contains("$version = '2.5.0'"));
    }

    #[test]
    fn newer_release_is_available() {
        let release = GitHubRelease {
            tag_name: "v99.0".to_string(),
            html_url: "https://github.com/example/releases/tag/v99.0".to_string(),
            prerelease: false,
            draft: false,
            assets: vec![asset("silence-2.1.0-windows-x64-portable.zip")],
        };
        let check = update_from_release(release, "x64").unwrap();
        assert!(matches!(check, UpdateCheck::Available(_)));
    }

    #[test]
    fn current_release_is_up_to_date() {
        let release = GitHubRelease {
            tag_name: env!("CARGO_PKG_VERSION").to_string(),
            html_url: "https://github.com/example/releases/latest".to_string(),
            prerelease: false,
            draft: false,
            assets: vec![asset("silence-2.0.0-windows-x64-portable.zip")],
        };
        let check = update_from_release(release, "x64").unwrap();
        assert_eq!(check, UpdateCheck::UpToDate);
    }

    #[test]
    fn newer_release_without_portable_errors() {
        let release = GitHubRelease {
            tag_name: "v99.0".to_string(),
            html_url: "https://github.com/example/releases/tag/v99.0".to_string(),
            prerelease: false,
            draft: false,
            assets: vec![asset("silence-2.1.0-windows-x64-setup.exe")],
        };
        let err = update_from_release(release, "x64").unwrap_err();
        assert!(err.to_string().contains("find windows x64 portable"));
    }
}
