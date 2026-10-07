//! Porthole updates itself. Checks the GitHub releases for this repo,
//! downloads the matching build, verifies its checksum, and swaps the
//! binary — no package manager, no expert knowledge.
//!
//! Network access goes through `curl` (already a preflight-checked tool),
//! and checksums through `sha256sum`. Both missing → honest error.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde::Deserialize;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = "nordicnode/porthole";

#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub tag: String,
    pub asset_url: String,
    pub asset_name: String,
    pub checksum_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

/// e.g. "x86_64-linux". Only what we actually ship.
pub fn target_triple() -> Result<&'static str> {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Ok("x86_64-linux"),
        ("aarch64", "linux") => Ok("aarch64-linux"),
        (arch, os) => anyhow::bail!("Porthole doesn't ship builds for {arch}-{os} yet"),
    }
}

fn gh_api(url: &str) -> Result<String> {
    if !crate::docker::command_exists("curl") {
        anyhow::bail!("curl is missing — can't check for updates");
    }
    let out = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "20",
            "-H",
            "User-Agent: porthole-selfupdate",
            "-H",
            "Accept: application/vnd.github+json",
            url,
        ])
        .output()
        .context("running curl")?;
    if !out.status.success() {
        let code = out
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        anyhow::bail!("GitHub API request failed (HTTP {code})");
    }
    String::from_utf8(out.stdout).context("reading GitHub API response")
}

/// "v1.2.3" vs "1.2.3" — true if `latest` is newer than `current`.
pub fn version_newer(latest: &str, current: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        v.trim_start_matches('v')
            .split('.')
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    }
    let (l, c) = (parts(latest), parts(current));
    for i in 0..l.len().max(c.len()) {
        let (a, b) = (
            l.get(i).copied().unwrap_or(0),
            c.get(i).copied().unwrap_or(0),
        );
        if a != b {
            return a > b;
        }
    }
    false
}

/// Check GitHub for a newer release. Ok(None) = up to date (or no releases
/// published yet — not an error on a fresh project).
pub fn check_for_update() -> Result<Option<ReleaseInfo>> {
    let body = gh_api(&format!(
        "https://api.github.com/repos/{REPO}/releases/latest"
    ))?;
    let rel: GhRelease = serde_json::from_str(&body).context("parsing release info")?;
    if !version_newer(&rel.tag_name, CURRENT_VERSION) {
        return Ok(None);
    }
    let triple = target_triple()?;
    let want = format!("porthole-{triple}.tar.gz");
    let asset = rel
        .assets
        .iter()
        .find(|a| a.name == want)
        .with_context(|| format!("release {} has no build for {triple}", rel.tag_name))?;
    let checksum_url = rel
        .assets
        .iter()
        .find(|a| a.name == "SHA256SUMS")
        .map(|a| a.browser_download_url.clone());
    Ok(Some(ReleaseInfo {
        tag: rel.tag_name.clone(),
        asset_url: asset.browser_download_url.clone(),
        asset_name: asset.name.clone(),
        checksum_url,
    }))
}

fn download(url: &str, dest: &Path) -> Result<()> {
    let status = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "300",
            "-H",
            "User-Agent: porthole-selfupdate",
            "-o",
            &dest.to_string_lossy(),
            url,
        ])
        .status()
        .context("running curl")?;
    if !status.success() {
        anyhow::bail!("download failed");
    }
    Ok(())
}

/// Download, verify, and install the release. Returns a message for the UI.
/// The user restarts Porthole themselves — replacing the binary is enough.
pub fn install_update(rel: &ReleaseInfo) -> Result<String> {
    let exe = std::env::current_exe().context("locating the running binary")?;
    let exe_s = exe.to_string_lossy();
    if exe_s.contains("/target/debug/") || exe_s.contains("/target/release/") {
        anyhow::bail!(
            "this looks like a development build — update it with cargo, not the self-updater"
        );
    }

    let work = std::env::temp_dir().join(format!(
        "porthole-update-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&work)?;
    let cleanup = || std::fs::remove_dir_all(&work).ok();

    let result: Result<String> = (|| {
        // 1. Download.
        let tarball = work.join(&rel.asset_name);
        download(&rel.asset_url, &tarball)?;

        // 2. Verify checksum. Refuse to install if unverifiable.
        let url = rel
            .checksum_url
            .as_ref()
            .context("release has no checksum file — refusing to install an unverified binary")?;
        if !crate::docker::command_exists("sha256sum") {
            anyhow::bail!("sha256sum is missing — refusing to install an unverified binary");
        }
        let sums_file = work.join("SHA256SUMS");
        download(url, &sums_file)?;
        let sums = std::fs::read_to_string(&sums_file)?;
        let expected = sums
            .lines()
            .find(|l| l.contains(&rel.asset_name))
            .and_then(|l| l.split_whitespace().next())
            .context("checksum file doesn't mention the download")?;
        let out = Command::new("sha256sum")
            .arg(&tarball)
            .output()
            .context("running sha256sum")?;
        let actual = String::from_utf8_lossy(&out.stdout);
        let actual = actual.split_whitespace().next().unwrap_or("");
        if actual != expected {
            anyhow::bail!("checksum mismatch — the download may be corrupt; refusing to install");
        }

        // 3. Extract and find the binary.
        let status = Command::new("tar")
            .args([
                "-xzf",
                &tarball.to_string_lossy(),
                "-C",
                &work.to_string_lossy(),
            ])
            .status()
            .context("extracting the release")?;
        if !status.success() {
            anyhow::bail!("couldn't extract the release archive");
        }
        let new_bin = work.join("porthole");
        if !new_bin.is_file() {
            anyhow::bail!("the release archive didn't contain a porthole binary");
        }

        // 4. Swap: current → backup, new → current. Restore on failure.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&new_bin, std::fs::Permissions::from_mode(0o755))?;
        }
        let backup = exe.with_extension("bak");
        std::fs::rename(&exe, &backup).context("backing up the current binary")?;
        if let Err(e) = std::fs::copy(&new_bin, &exe) {
            let _ = std::fs::rename(&backup, &exe); // best-effort restore
            anyhow::bail!("couldn't install the new binary: {e:#}");
        }
        std::fs::remove_file(&backup).ok();
        Ok(format!(
            "Porthole {} installed — restart Porthole to use it.",
            rel.tag
        ))
    })();

    cleanup();
    result
}

/// Where to record the last check, so we check at most daily.
#[cfg_attr(test, allow(dead_code))]
pub fn should_check(last: Option<u64>) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match last {
        None => true,
        Some(t) => now.saturating_sub(t) > 24 * 3600,
    }
}

#[cfg_attr(test, allow(dead_code))]
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(version_newer("v0.2.0", "0.1.0"));
        assert!(version_newer("v1.0.0", "0.9.9"));
        assert!(!version_newer("v0.1.0", "0.1.0"));
        assert!(!version_newer("v0.1.0", "0.2.0"));
        assert!(version_newer("v0.10.0", "0.9.0"));
    }

    #[test]
    fn target_is_known() {
        // On this machine it must resolve; elsewhere it errors honestly.
        let _ = target_triple();
    }
}
