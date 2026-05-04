use std::{
    fs,
    path::PathBuf,
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
use tokio::io::AsyncWriteExt;

const GITHUB_API_ROOT: &str = "https://api.github.com/repos";
const DEFAULT_UPDATE_REPO: &str = "vertopolkaLF/silence";
const USER_AGENT: &str = concat!("silence-updater/", env!("CARGO_PKG_VERSION"));

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
    let target_dir = update_temp_dir();
    tokio::fs::create_dir_all(&target_dir)
        .await
        .context("create update temp directory")?;
    let final_path = target_dir.join(safe_installer_name(update));
    let part_path = final_path.with_extension("exe.part");
    if tokio::fs::try_exists(&part_path).await.unwrap_or(false) {
        let _ = tokio::fs::remove_file(&part_path).await;
    }

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .context("create download http client")?;
    let response = client
        .get(&update.download_url)
        .send()
        .await
        .context("request update installer")?
        .error_for_status()
        .context("update installer response")?;
    let total = response
        .content_length()
        .unwrap_or(update.asset_size)
        .max(1);
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(&part_path)
        .await
        .context("create partial update installer")?;
    let mut downloaded = 0_u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("download update chunk")?;
        file.write_all(&chunk).await.context("write update chunk")?;
        downloaded += chunk.len() as u64;
        on_progress(((downloaded as f32 / total as f32) * 100.0).clamp(0.0, 100.0));
    }

    file.flush().await.context("flush update installer")?;
    drop(file);

    let metadata = tokio::fs::metadata(&part_path)
        .await
        .context("read downloaded installer metadata")?;
    anyhow::ensure!(
        metadata.len() > 0 && (update.asset_size == 0 || metadata.len() == update.asset_size),
        "downloaded installer size mismatch"
    );
    if tokio::fs::try_exists(&final_path).await.unwrap_or(false) {
        let _ = tokio::fs::remove_file(&final_path).await;
    }
    tokio::fs::rename(&part_path, &final_path)
        .await
        .context("finalize update installer")?;
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

pub fn install_update(installer: PathBuf) -> Result<()> {
    if INSTALLING.swap(true, Ordering::Relaxed) {
        anyhow::bail!("update installation is already running");
    }
    std::process::Command::new(&installer)
        .arg("/S")
        .spawn()
        .with_context(|| format!("launch update installer {}", installer.display()))?;
    Ok(())
}

fn update_temp_dir() -> PathBuf {
    std::env::temp_dir().join("silence-updater")
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
    let asset = select_installer_asset(&release.assets, arch)
        .with_context(|| format!("find windows {arch} setup installer asset"))?;
    Ok(UpdateCheck::Available(UpdateInfo {
        version: format!("v{latest}"),
        tag_name: release.tag_name,
        release_url: release.html_url,
        asset_name: asset.name,
        download_url: asset.browser_download_url,
        asset_size: asset.size,
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

fn select_installer_asset(assets: &[GitHubAsset], arch: &str) -> Option<GitHubAsset> {
    let arch_marker = format!("-windows-{arch}-");
    assets
        .iter()
        .find(|asset| {
            let name = asset.name.to_ascii_lowercase();
            name.contains(&arch_marker)
                && name.ends_with("-setup.exe")
                && !name.ends_with("-portable.zip")
        })
        .cloned()
}

fn safe_installer_name(update: &UpdateInfo) -> String {
    let name = update
        .asset_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    if name.to_ascii_lowercase().ends_with(".exe") {
        name
    } else {
        format!("silence-{}-{}-setup.exe", update.version, current_arch())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> GitHubAsset {
        GitHubAsset {
            name: name.to_string(),
            browser_download_url: format!("https://example.invalid/{name}"),
            size: 42,
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
    fn selects_setup_for_arch_not_portable() {
        let assets = vec![
            asset("silence-2.1.0-windows-x64-portable.zip"),
            asset("silence-2.1.0-windows-x64-setup.exe"),
            asset("silence-2.1.0-windows-x86-setup.exe"),
        ];
        let selected = select_installer_asset(&assets, "x64").unwrap();
        assert_eq!(selected.name, "silence-2.1.0-windows-x64-setup.exe");
    }

    #[test]
    fn missing_installer_is_none() {
        let assets = vec![asset("silence-2.1.0-windows-x64-portable.zip")];
        assert!(select_installer_asset(&assets, "x64").is_none());
    }

    #[test]
    fn newer_release_is_available() {
        let release = GitHubRelease {
            tag_name: "v2.1".to_string(),
            html_url: "https://github.com/example/releases/tag/v2.1".to_string(),
            prerelease: false,
            draft: false,
            assets: vec![asset("silence-2.1.0-windows-x64-setup.exe")],
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
            assets: vec![asset("silence-2.0.0-windows-x64-setup.exe")],
        };
        let check = update_from_release(release, "x64").unwrap();
        assert_eq!(check, UpdateCheck::UpToDate);
    }

    #[test]
    fn newer_release_without_installer_errors() {
        let release = GitHubRelease {
            tag_name: "v2.1".to_string(),
            html_url: "https://github.com/example/releases/tag/v2.1".to_string(),
            prerelease: false,
            draft: false,
            assets: vec![asset("silence-2.1.0-windows-x64-portable.zip")],
        };
        let err = update_from_release(release, "x64").unwrap_err();
        assert!(err.to_string().contains("find windows x64 setup"));
    }
}
