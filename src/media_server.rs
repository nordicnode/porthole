//! Media-server settings for small-disk mode.
//!
//! Plex and Jellyfin both assume a fast local disk by default: they
//! generate preview thumbnails, analyze every file deeply, and scan on a
//! schedule. On a cloud-backed library that means hours of CPU, wasted
//! disk, and — worst of all — a scan during a mount outage deleting
//! library entries. This module applies the expert settings automatically.
//!
//! Plex is fully automatic: the token is read from Preferences.xml (the
//! same place the installer reads it) and settings go through the
//! `PUT /:/prefs` API. Every preference name below was verified against
//! real Plex Preferences.xml files — never guess these.
//!
//! Jellyfin has no API key provisioned by the installer and no
//! unauthenticated way to change settings, so for Jellyfin Shiphand
//! reports exact manual steps instead of pretending.

use std::path::Path;
use std::process::Command;

/// Which media server the fleet uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaServer {
    Plex,
    Jellyfin,
}

/// Detect the media server from the install: explicit profile in .env wins,
/// otherwise whichever container exists.
pub fn detect(install_dir: &Path) -> Option<MediaServer> {
    let map = crate::generate::read_env_file(&install_dir.join(".env"));
    if let Some(profile) = map.get("COMPOSE_PROFILES") {
        if profile.contains("plex") {
            return Some(MediaServer::Plex);
        }
        if profile.contains("jellyfin") {
            return Some(MediaServer::Jellyfin);
        }
    }
    // Fall back to config dirs left by a previous install.
    if install_dir.join("configs/plex").is_dir() {
        return Some(MediaServer::Plex);
    }
    if install_dir.join("configs/jellyfin").is_dir() {
        return Some(MediaServer::Jellyfin);
    }
    None
}

/// Read the Plex token from Preferences.xml. Plex writes it there on claim;
/// the file lives in the bind-mounted config dir.
pub fn plex_token(install_dir: &Path) -> Result<String, String> {
    let prefs = install_dir
        .join("configs/plex/Library/Application Support/Plex Media Server/Preferences.xml");
    let raw = std::fs::read_to_string(&prefs)
        .map_err(|_| format!("no Plex Preferences.xml at {}", prefs.display()))?;
    // PlexOnlineToken="...." — parse without an XML dep; the file is one line.
    let key = "PlexOnlineToken=\"";
    let start = raw.find(key).ok_or(
        "Plex isn't claimed yet (no token in Preferences.xml) — open Plex Web once and claim it",
    )? + key.len();
    let end = raw[start..].find('"').ok_or("could not parse Plex token")?;
    let token = &raw[start..start + end];
    if token.is_empty() {
        return Err("Plex token is empty — claim the server in Plex Web first".to_string());
    }
    Ok(token.to_string())
}

/// Plex preferences for a cloud-backed library. Each name verified against
/// real Preferences.xml files; each value is the "don't churn the disk"
/// choice:
///
/// - `autoEmptyTrash=0`: a scan during a mount outage must NEVER delete
///   entries. This is the single most important setting.
/// - `GenerateBIFBehavior=never`, `GenerateChapterThumbBehavior=never`:
///   preview thumbnails are hours of CPU and GBs of disk for nothing.
/// - `GenerateIntroMarkerBehavior=never`: intro/credits detection, same.
/// - `LoudnessAnalysisBehavior=never`: loudness analysis, same.
/// - `ScheduledLibraryUpdatesEnabled=0`: no periodic full scans against
///   a FUSE mount; the *arrs tell Plex about new files directly.
/// - `ButlerTaskDeepMediaAnalysis=0`: no deep analysis passes.
pub fn plex_cloud_prefs() -> Vec<(&'static str, &'static str)> {
    vec![
        ("autoEmptyTrash", "0"),
        ("GenerateBIFBehavior", "never"),
        ("GenerateChapterThumbBehavior", "never"),
        ("GenerateIntroMarkerBehavior", "never"),
        ("LoudnessAnalysisBehavior", "never"),
        ("ScheduledLibraryUpdatesEnabled", "0"),
        ("ButlerTaskDeepMediaAnalysis", "0"),
    ]
}

/// Apply the cloud prefs to a running Plex via `PUT /:/prefs`.
/// Returns the prefs that were confirmed applied.
pub fn apply_plex_cloud_settings(token: &str, log: &dyn Fn(String)) -> Result<Vec<String>, String> {
    if !crate::docker::command_exists("curl") {
        return Err("curl is missing — can't talk to Plex".to_string());
    }
    let mut applied = Vec::new();
    for (name, value) in plex_cloud_prefs() {
        let url = format!("http://localhost:32400/:/prefs?{name}={value}&X-Plex-Token={token}");
        log(format!("[in] setting {name}={value}…"));
        let status = Command::new("curl")
            .args([
                "-sf",
                "--connect-timeout",
                "5",
                "--max-time",
                "15",
                "-X",
                "PUT",
                &url,
            ])
            .status()
            .map_err(|e| format!("could not run curl: {e}"))?;
        if status.success() {
            applied.push(name.to_string());
        } else {
            return Err(format!(
                "Plex refused the {name} setting — is Plex running on port 32400?"
            ));
        }
    }
    Ok(applied)
}

/// Exact manual steps for Jellyfin, since there's no API key to use.
/// Jellyfin's defaults are already FUSE-tolerant (a daily "Scan Media
/// Library" task exists; real-time monitoring just silently doesn't fire
/// on FUSE), so this is confirmation, not repair.
pub fn jellyfin_manual_steps() -> Vec<String> {
    vec![
        "Jellyfin needs no API changes — its defaults already suit a cloud library:".to_string(),
        "  • a daily 'Scan Media Library' task runs on a schedule".to_string(),
        "  • real-time monitoring can't fire on a FUSE mount, so it harmlessly does nothing".to_string(),
        "If you ever see stale entries: Dashboard → Scheduled Tasks → 'Scan Media Library' → run it once.".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_prefs_are_the_verified_set() {
        let prefs = plex_cloud_prefs();
        let names: Vec<_> = prefs.iter().map(|(n, _)| *n).collect();
        // The data-safety critical one must always be present.
        assert!(names.contains(&"autoEmptyTrash"));
        assert_eq!(
            prefs
                .iter()
                .find(|(n, _)| *n == "autoEmptyTrash")
                .unwrap()
                .1,
            "0"
        );
        assert!(names.contains(&"ScheduledLibraryUpdatesEnabled"));
        assert!(names.contains(&"GenerateBIFBehavior"));
        assert!(names.contains(&"LoudnessAnalysisBehavior"));
        // 7 prefs, no duplicates.
        assert_eq!(prefs.len(), 7);
        let mut dedup = names.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(dedup.len(), 7);
    }

    #[test]
    fn parses_plex_token_from_preferences_xml() {
        let dir = std::env::temp_dir().join("shiphand-plex-test");
        let prefs_dir = dir.join("configs/plex/Library/Application Support/Plex Media Server");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&prefs_dir).unwrap();
        std::fs::write(
            prefs_dir.join("Preferences.xml"),
            r#"<?xml version="1.0" encoding="utf-8"?><Preferences MachineIdentifier="abc" PlexOnlineToken="tok_12345" AcceptedEULA="1"/>"#,
        )
        .unwrap();
        assert_eq!(plex_token(&dir).unwrap(), "tok_12345");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_token_is_an_honest_error() {
        let dir = std::env::temp_dir().join("shiphand-plex-missing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(plex_token(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_media_server_from_env() {
        let dir = std::env::temp_dir().join("shiphand-ms-detect");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".env"), "COMPOSE_PROFILES=jellyfin\n").unwrap();
        assert_eq!(detect(&dir), Some(MediaServer::Jellyfin));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
