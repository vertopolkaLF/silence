//! In-place updates from GitHub Releases portable zip assets.
//!
//! Portable copies and installer-based installs update the same way: the
//! package is downloaded and unpacked to a temp folder, then a hidden helper
//! waits for silence! to exit, copies the files over the current install
//! folder and starts it again.

use std::{
    fs,
    io::copy,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use anyhow::{Context, Result};
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
const RELEASES_URL: &str = "https://github.com/vertopolkaLF/silence/releases/latest";
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
) -> Result<PathBuf> {
    let target_dir = update_run_dir();
    if tokio::fs::try_exists(&target_dir).await.unwrap_or(false) {
        let _ = tokio::fs::remove_dir_all(&target_dir).await;
    }
    tokio::fs::create_dir_all(&target_dir)
        .await
        .context("create update temp directory")?;
    let final_path = target_dir.join("package.zip");
    let part_path = final_path.with_extension("zip.part");

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
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
    let mut file = tokio::fs::File::create(&part_path)
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
    if let Some(expected) = &update.sha256 {
        anyhow::ensure!(
            hex(&hasher.finalize()).eq_ignore_ascii_case(expected),
            "update package checksum mismatch"
        );
    }
    tokio::fs::rename(&part_path, &final_path)
        .await
        .context("finalize update package")?;
    on_progress(100.0);
    Ok(final_path)
}

pub fn cleanup_downloads_after_startup() {
    thread::spawn(|| {
        thread::sleep(Duration::from_secs(10));
        let target_dir = update_temp_dir();
        if let Err(err) = fs::remove_dir_all(&target_dir) {
            if err.kind() != std::io::ErrorKind::NotFound {
                eprintln!(
                    "failed to cleanup update downloads {}: {err:?}",
                    target_dir.display()
                );
            }
        }
    });
}

/// Unpacks the downloaded package and hands it to a hidden helper that
/// replaces the install folder once silence! has exited. The caller must exit
/// all silence! processes right after this returns.
pub fn install_update(update: &UpdateInfo, package: &Path) -> Result<()> {
    if INSTALLING.swap(true, Ordering::Relaxed) {
        anyhow::bail!("update installation is already running");
    }
    let result = stage_update(update, package);
    if result.is_err() {
        INSTALLING.store(false, Ordering::Relaxed);
    }
    result
}

fn stage_update(update: &UpdateInfo, package: &Path) -> Result<()> {
    let install_dir = install_dir()?;
    if !dir_writable(&install_dir) {
        let _ = open_release_page(&update.release_url);
        anyhow::bail!(
            "{} is not writable, download the update from the release page",
            install_dir.display()
        );
    }

    let run_dir = package
        .parent()
        .context("update package has no parent directory")?;
    let extracted = run_dir.join("extracted");
    if extracted.exists() {
        fs::remove_dir_all(&extracted).context("clear previous update extraction")?;
    }
    let payload = extract_package(package, &extracted)?;
    anyhow::ensure!(
        payload.join(APP_EXE).is_file(),
        "update package does not contain {APP_EXE}"
    );

    let script_path = run_dir.join("apply-update.ps1");
    fs::write(
        &script_path,
        helper_script(&install_dir, &payload, &update.version),
    )
    .context("write update helper script")?;
    spawn_hidden_helper(&script_path)
}

fn helper_script(install_dir: &Path, payload: &Path, version: &str) -> String {
    format!(
        r#"$ErrorActionPreference = 'Stop'
$install = '{install}'
$source = '{source}'
$exe = Join-Path $install '{APP_EXE}'
$version = '{version}'

# Settings and the background app are separate processes of the same exe.
$deadline = (Get-Date).AddSeconds(20)
while ($true) {{
    $running = @(Get-Process -Name 'silence' -ErrorAction SilentlyContinue |
        Where-Object {{ $_.Path -and ($_.Path -ieq $exe) }})
    if ($running.Count -eq 0) {{ break }}
    if ((Get-Date) -gt $deadline) {{
        $running | Stop-Process -Force -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 500
        break
    }}
    Start-Sleep -Milliseconds 250
}}

try {{
    $attempt = 0
    while ($true) {{
        try {{
            Get-ChildItem -LiteralPath $source -Force | ForEach-Object {{
                Copy-Item -LiteralPath $_.FullName -Destination $install -Recurse -Force
            }}
            break
        }} catch {{
            $attempt++
            if ($attempt -ge 10) {{ throw }}
            Start-Sleep -Milliseconds 500
        }}
    }}

    # Bundled asset names carry content hashes; drop the ones this release no longer ships.
    $newAssets = Join-Path $source 'assets'
    $oldAssets = Join-Path $install 'assets'
    if ((Test-Path -LiteralPath $newAssets) -and (Test-Path -LiteralPath $oldAssets)) {{
        $oldRoot = (Get-Item -LiteralPath $oldAssets).FullName.TrimEnd('\')
        Get-ChildItem -LiteralPath $oldRoot -File -Recurse -Force | ForEach-Object {{
            $relative = $_.FullName.Substring($oldRoot.Length).TrimStart('\')
            if (-not (Test-Path -LiteralPath (Join-Path $newAssets $relative))) {{
                Remove-Item -LiteralPath $_.FullName -Force -ErrorAction SilentlyContinue
            }}
        }}
    }}

    # Keep "Installed apps" in sync when this folder is the installer's install.
    $key = '{UNINSTALL_KEY}'
    $entry = Get-ItemProperty -LiteralPath $key -ErrorAction SilentlyContinue
    if ($entry -and $entry.InstallLocation -and
        ($entry.InstallLocation.TrimEnd('\') -ieq $install.TrimEnd('\'))) {{
        Set-ItemProperty -LiteralPath $key -Name DisplayVersion -Value $version
    }}
}} catch {{
    # The "updated" toast must not show for an update that did not apply.
    $marker = Join-Path $env:APPDATA 'SilenceV2\last_update.json'
    Remove-Item -LiteralPath $marker -Force -ErrorAction SilentlyContinue
    $log = Join-Path $env:TEMP 'silence-update-error.log'
    "$(Get-Date -Format o) $($_ | Out-String)" | Add-Content -LiteralPath $log -Encoding utf8
}} finally {{
    Start-Process -FilePath $exe -WorkingDirectory $install
}}
"#,
        install = ps_quote(&install_dir.display().to_string()),
        source = ps_quote(&payload.display().to_string()),
        version = ps_quote(version.trim_start_matches(['v', 'V'])),
    )
}

#[cfg(windows)]
fn spawn_hidden_helper(script_path: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
        ])
        .arg(script_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .context("spawn update helper")
}

#[cfg(not(windows))]
fn spawn_hidden_helper(_script_path: &Path) -> Result<()> {
    anyhow::bail!("in-place updates are only supported on Windows")
}

/// Escapes a value for a single-quoted PowerShell string.
fn ps_quote(value: &str) -> String {
    value.replace('\'', "''")
}

fn open_release_page(url: &str) -> Result<()> {
    let url = if url.trim().is_empty() { RELEASES_URL } else { url };
    Command::new("explorer")
        .arg(url)
        .spawn()
        .map(|_| ())
        .context("open release page")
}

/// Extracts `archive` into `destination` and returns the folder holding the
/// app files (the archive may wrap them in a single top-level folder).
fn extract_package(archive: &Path, destination: &Path) -> Result<PathBuf> {
    let file = fs::File::open(archive).with_context(|| format!("open {}", archive.display()))?;
    let mut archive = ZipArchive::new(file).context("open update package")?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).context("read update package entry")?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let target = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).with_context(|| format!("create {}", target.display()))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let mut output =
            fs::File::create(&target).with_context(|| format!("create {}", target.display()))?;
        copy(&mut entry, &mut output).with_context(|| format!("extract {}", target.display()))?;
    }

    let mut entries = fs::read_dir(destination)
        .with_context(|| format!("read {}", destination.display()))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    if entries.len() == 1 && entries[0].file_type().is_ok_and(|kind| kind.is_dir()) {
        return Ok(entries.remove(0).path());
    }
    Ok(destination.to_path_buf())
}

fn install_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("resolve current executable")?;
    exe.parent()
        .map(Path::to_path_buf)
        .context("executable has no parent directory")
}

fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(".silence-update-write-test");
    match fs::write(&probe, b"") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn update_temp_dir() -> PathBuf {
    std::env::temp_dir().join("silence-updater")
}

fn update_run_dir() -> PathBuf {
    update_temp_dir().join(format!("run-{}", std::process::id()))
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
