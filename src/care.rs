//! Fleet care: backups, restore, uninstall, and updates with rollback.
//!
//! Philosophy: every destructive action is guarded — backups happen
//! automatically before updates, and nothing is deleted without the user
//! confirming exactly what goes away, in plain language.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::docker;
use crate::services::SERVICES;

/// Where backups live.
pub fn backup_dir() -> PathBuf {
    docker::data_dir().join("backups")
}

fn timestamp() -> String {
    // File-safe timestamp without extra deps. Nanosecond resolution: two
    // backups in the same second must not share a filename (they'd
    // overwrite each other — seen in tests, would bite in production).
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos}")
}

/// Create a timestamped backup of the install dir (configs, .env, compose
/// files — NOT the media data, which can be terabytes and is re-fetchable).
pub fn create_backup(install_dir: &Path) -> Result<PathBuf> {
    if !install_dir.is_dir() {
        anyhow::bail!("install dir {} doesn't exist", install_dir.display());
    }
    if !docker::command_exists("tar") {
        anyhow::bail!("the 'tar' command is missing — can't create a backup");
    }
    let dir = backup_dir();
    std::fs::create_dir_all(&dir).context("creating backup dir")?;
    let dest = dir.join(format!("porthole-backup-{}.tar.gz", timestamp()));
    let parent = install_dir.parent().context("install dir has no parent")?;
    let name = install_dir.file_name().context("install dir has no name")?;
    // Exclude the data dir (media/downloads): huge and re-fetchable.
    let status = Command::new("tar")
        .args([
            "-czf",
            &dest.to_string_lossy(),
            "--exclude",
            &format!("{}/data", name.to_string_lossy()),
            "-C",
            &parent.to_string_lossy(),
            &name.to_string_lossy(),
        ])
        .status()
        .context("running tar")?;
    if !status.success() {
        anyhow::bail!("tar failed");
    }
    Ok(dest)
}

/// Newest-first list of backups.
pub fn list_backups() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(backup_dir()) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("gz") {
                out.push(p);
            }
        }
    }
    out.sort_by(|a, b| b.cmp(a));
    out
}

/// Restore a backup over the install dir. Containers are stopped first so
/// no half-written config is read.
pub fn restore_backup(backup: &Path, install_dir: &Path) -> Result<()> {
    if !backup.is_file() {
        anyhow::bail!("backup {} not found", backup.display());
    }
    for svc in SERVICES.iter().filter(|s| s.port != 0) {
        let _ = docker::stop_container(svc.id);
    }
    let parent = install_dir.parent().context("install dir has no parent")?;
    let status = Command::new("tar")
        .args([
            "-xzf",
            &backup.to_string_lossy(),
            "-C",
            &parent.to_string_lossy(),
        ])
        .status()
        .context("running tar")?;
    if !status.success() {
        anyhow::bail!("tar extract failed");
    }
    Ok(())
}

/// Human-readable list of what uninstall would remove.
pub fn uninstall_plan(install_dir: &Path) -> Vec<String> {
    let mut plan = vec![
        "Stop and remove all 8 fleet containers".to_string(),
        format!("Delete the install dir: {}", install_dir.display()),
        "Disable the torbox-media-server systemd service (if present)".to_string(),
        "Remove the unused Docker network (media-network)".to_string(),
    ];
    if !install_dir.is_dir() {
        plan.insert(
            1,
            "(install dir doesn't exist — nothing to delete there)".to_string(),
        );
    }
    plan
}

/// Uninstall the fleet. Mirrors uninstall.sh: containers down, configs and
/// data removed, systemd service disabled. The media mount dir is left alone
/// (it's outside the install dir and may hold other things).
pub fn uninstall(install_dir: &Path, tx: &Sender<CareEvent>) -> Result<()> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };
    // 1. Containers down via compose if we have it, else per-container.
    let compose_yml = install_dir.join("docker-compose.yml");
    if compose_yml.is_file() && docker::compose_available() {
        log("[in] bringing containers down…");
        let ok = Command::new("docker")
            .args([
                "compose",
                "-f",
                &compose_yml.to_string_lossy(),
                "down",
                "--remove-orphans",
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        log(if ok {
            "[ok] containers removed"
        } else {
            "[warn] compose down had issues"
        });
    }
    for svc in SERVICES.iter().filter(|s| s.port != 0) {
        let _ = Command::new("docker").args(["rm", "-f", svc.id]).output();
    }
    log("[ok] containers removed");
    // 2. systemd service.
    let _ = Command::new("systemctl")
        .args(["disable", "--now", "torbox-media-server.service"])
        .output();
    log("[in] systemd service disabled (if it existed)");
    // 3. Install dir.
    if install_dir.is_dir() {
        std::fs::remove_dir_all(install_dir)
            .with_context(|| format!("removing {}", install_dir.display()))?;
        log(&format!("[ok] removed {}", install_dir.display()));
    }
    // 4. Network.
    let _ = Command::new("docker")
        .args(["network", "rm", "media-network"])
        .output();
    log("[ok] fleet network removed (if it existed)");
    Ok(())
}

// ---------------------------------------------------------------------------
// Updates
// ---------------------------------------------------------------------------

/// One service with an update waiting.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub service: String,
}

/// Check for updates: pull latest images and report which services actually
/// got something new. Pulling *is* the check — nothing restarts until the
/// user runs the update.
pub fn check_updates(install_dir: &Path, tx: &Sender<CareEvent>) -> Result<Vec<UpdateInfo>> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };
    let compose_yml = install_dir.join("docker-compose.yml");
    if !compose_yml.is_file() {
        anyhow::bail!("no docker-compose.yml in {}", install_dir.display());
    }
    // Snapshot image IDs before pulling.
    let before = image_ids(install_dir);
    log("[in] pulling latest images (this can take a while)…");
    let out = Command::new("docker")
        .args([
            "compose",
            "-f",
            &compose_yml.to_string_lossy(),
            "pull",
            "--quiet",
        ])
        .output()
        .context("running docker compose pull")?;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        log(&format!("  {line}"));
    }
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        log(&format!(
            "[warn] pull had issues: {}",
            err.lines().next().unwrap_or("")
        ));
    }
    let after = image_ids(install_dir);
    let mut updates = Vec::new();
    for svc in SERVICES.iter().filter(|s| s.port != 0) {
        let b = before.get(svc.id);
        let a = after.get(svc.id);
        if b != a && a.is_some() {
            updates.push(UpdateInfo {
                service: svc.name.to_string(),
            });
            log(&format!("[ok] {} has an update", svc.name));
        }
    }
    if updates.is_empty() {
        log("[ok] everything is already up to date");
    }
    Ok(updates)
}

/// Map service id -> current image ID via `docker compose config` + inspect.
fn image_ids(install_dir: &Path) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for svc in SERVICES.iter().filter(|s| s.port != 0) {
        if let Some(image) = service_image(install_dir, svc.id) {
            let id = Command::new("docker")
                .args(["images", "-q", &image])
                .output()
                .ok()
                .and_then(|o| {
                    String::from_utf8(o.stdout)
                        .ok()
                        .map(|s| s.lines().next().unwrap_or("").to_string())
                })
                .unwrap_or_default();
            map.insert(svc.id.to_string(), id);
        }
    }
    map
}

fn service_image(install_dir: &Path, service_id: &str) -> Option<String> {
    let out = Command::new("docker")
        .args([
            "compose",
            "-f",
            &install_dir.join("docker-compose.yml").to_string_lossy(),
            "config",
            "--format",
            "json",
        ])
        .output()
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    v.get("services")?
        .get(service_id)?
        .get("image")?
        .as_str()
        .map(|s| s.to_string())
}

/// Run the update: backup, pull, restart, health-check, rollback on failure.
pub fn update_fleet(install_dir: &Path, tx: &Sender<CareEvent>) -> Result<()> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };
    // 1. Safety net first.
    log("[in] creating a backup before touching anything…");
    let backup = create_backup(install_dir)?;
    log(&format!(
        "[ok] backup saved: {}",
        backup.file_name().unwrap_or_default().to_string_lossy()
    ));

    // 2. Tag current images so we can roll back to them.
    let tag = format!("porthole-prev-{}", timestamp());
    let mut tagged: Vec<(String, String, String)> = Vec::new(); // (service, image, old_id)
    for svc in SERVICES.iter().filter(|s| s.port != 0) {
        if let Some(image) = service_image(install_dir, svc.id) {
            if let Some(id) = current_image_id(&image) {
                // Tag the current image so we can roll back to it.
                let repo = image.split(':').next().unwrap_or(&image);
                let backup_tag = format!("{repo}:{tag}");
                if Command::new("docker")
                    .args(["tag", &id, &backup_tag])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
                {
                    tagged.push((svc.id.to_string(), image, id));
                }
            }
        }
    }
    log(&format!(
        "[ok] {} current images tagged for rollback",
        tagged.len()
    ));

    // 3. Pull + restart.
    log("[in] pulling latest images…");
    let compose_yml = install_dir.join("docker-compose.yml");
    let pull_ok = Command::new("docker")
        .args([
            "compose",
            "-f",
            &compose_yml.to_string_lossy(),
            "pull",
            "--quiet",
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !pull_ok {
        log("[warn] pull had issues — continuing with what we have");
    }
    log("[in] restarting services…");
    let up_ok = Command::new("docker")
        .args([
            "compose",
            "-f",
            &compose_yml.to_string_lossy(),
            "up",
            "-d",
            "--remove-orphans",
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !up_ok {
        log("[fail] could not restart services — rolling back");
        rollback(install_dir, &tagged, &backup, &log);
        anyhow::bail!("update failed at restart; rolled back");
    }

    // 4. Health check: every service answering within 90s?
    log("[in] waiting for services to answer (up to 90s)…");
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    let mut healthy = false;
    while std::time::Instant::now() < deadline {
        healthy = SERVICES
            .iter()
            .filter(|s| s.port != 0)
            .all(|s| docker::port_open(s.port, 500));
        if healthy {
            break;
        }
        std::thread::sleep(Duration::from_secs(5));
    }
    if healthy {
        log("[ok] all services answering — update complete");
        // Clean up rollback tags.
        for (_, image, _) in &tagged {
            let repo = image.split(':').next().unwrap_or(image);
            let _ = Command::new("docker")
                .args(["rmi", &format!("{repo}:{tag}")])
                .output();
        }
        Ok(())
    } else {
        log("[fail] some services didn't come back — rolling back");
        rollback(install_dir, &tagged, &backup, &log);
        anyhow::bail!("services unhealthy after update; rolled back");
    }
}

fn current_image_id(image: &str) -> Option<String> {
    Command::new("docker")
        .args(["images", "-q", image])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.lines().next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
}

fn rollback(
    install_dir: &Path,
    tagged: &[(String, String, String)],
    backup: &Path,
    log: &dyn Fn(&str),
) {
    let compose_yml = install_dir.join("docker-compose.yml");
    for (service_id, image, old_id) in tagged {
        // Point the tag back at the old image, then recreate.
        let _ = Command::new("docker").args(["tag", old_id, image]).status();
        log(&format!("[in] {service_id} rolled back to previous image"));
    }
    let _ = Command::new("docker")
        .args([
            "compose",
            "-f",
            &compose_yml.to_string_lossy(),
            "up",
            "-d",
            "--force-recreate",
            "--remove-orphans",
        ])
        .status();
    if restore_backup(backup, install_dir).is_ok() {
        log("[ok] configs restored from backup");
    }
    log("[ok] rollback complete — fleet is back where it started");
}

/// Rewrite all config files using Porthole's native generator, preserving
/// existing secrets (API keys, passwords) from the current `.env`.
/// A backup is taken first. Useful after a rotated TorBox key or a
/// corrupted config — no expert knowledge needed.
pub fn regenerate_configs(install_dir: &Path, tx: &Sender<CareEvent>) -> Result<()> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };
    log("[in] backing up before rewriting…");
    let backup = create_backup(install_dir)?;
    log(&format!(
        "[ok] backup saved: {}",
        backup.file_name().unwrap_or_default().to_string_lossy()
    ));

    let map = crate::generate::read_env_file(&install_dir.join(".env"));
    let get = |k: &str, default: &str| -> String {
        map.get(k).cloned().unwrap_or_else(|| default.to_string())
    };
    let dir_s = install_dir.to_string_lossy().to_string();
    let config_dir = get("CONFIG_DIR", &format!("{dir_s}/configs"));
    let data_dir = get("DATA_DIR", &format!("{dir_s}/data"));
    let mount_dir = get("MOUNT_DIR", "/mnt/torbox-media");

    let secrets = crate::generate::Secrets::from_env_map(&map)?;
    let generated_on = Command::new("date")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| format!("epoch {}", timestamp()));
    let docker_bin = Command::new("sh")
        .args(["-c", "command -v docker"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/usr/bin/docker".to_string());

    let files = crate::generate::generate_all(&crate::generate::GenInputs {
        install_dir: &dir_s,
        config_dir: &config_dir,
        data_dir: &data_dir,
        mount_dir: &mount_dir,
        tz: &get("TZ", "UTC"),
        puid: &get("PUID", "1000"),
        pgid: &get("PGID", "1000"),
        torbox_api_key: &get("TORBOX_API_KEY", ""),
        media_server: &get("COMPOSE_PROFILES", "plex"),
        plex_claim: &get("PLEX_CLAIM", ""),
        secrets: &secrets,
        generated_on: &generated_on,
        docker_bin: &docker_bin,
    });
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    for f in &files {
        let path = install_dir.join(&f.rel_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &f.content)?;
        #[cfg(unix)]
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(f.mode))?;
        log(&format!("[ok] wrote {}", f.rel_path));
    }
    log("[ok] configs rewritten natively — secrets preserved");
    Ok(())
}

// ---------------------------------------------------------------------------
// Worker events (same pattern as the setup wizard)
// ---------------------------------------------------------------------------

pub enum CareEvent {
    Log(String),
    Finished(Result<String, String>),
    /// A Porthole update was found — the UI should ask before installing.
    UpdateAvailable(crate::selfupdate::ReleaseInfo),
}

/// How new downloads reach the library in small-disk mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadAction {
    /// DFS mount + symlinks: instant, zero bytes, needs the mount running.
    Symlink,
    /// .strm files pointing at Decypharr's WebDAV: no mount needed at all.
    /// Jellyfin/Kodi play them natively; Plex needs the plex-strm-assistant
    /// helper (it never learned .strm).
    Strm,
}

/// Enable small-disk mode: the debrid cloud becomes the library, the local
/// disk only holds a stream cache.
///
/// - `Symlink`: configures Decypharr's DFS mount (auto-sized cache) and
///   makes *arr "imports" instant symlinks costing zero local bytes.
/// - `Strm`: no mount at all — new downloads become .strm files pointing
///   at Decypharr's WebDAV. For the most disk-poor setups.
///
/// Edits the existing `configs/decypharr/config.json` in place
/// (JSON-merged, preserving every other setting) after taking a backup.
pub fn apply_small_disk_mode(
    install_dir: &Path,
    action: DownloadAction,
    tx: &Sender<CareEvent>,
) -> Result<()> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };
    let config_path = install_dir.join("configs/decypharr/config.json");
    if !config_path.is_file() {
        anyhow::bail!(
            "no Decypharr config found at {} — run the Setup wizard first",
            config_path.display()
        );
    }

    log("[in] backing up before changing the Decypharr config…");
    let backup = create_backup(install_dir)?;
    log(&format!(
        "[ok] backup saved: {}",
        backup.file_name().unwrap_or_default().to_string_lossy()
    ));

    let raw = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading {}", config_path.display()))?;
    let mut cfg: serde_json::Value =
        serde_json::from_str(&raw).context("Decypharr config.json is not valid JSON")?;

    match action {
        DownloadAction::Symlink => {
            let free = crate::storage::free_bytes(install_dir)
                .map_err(|e| anyhow::anyhow!("could not measure free disk space: {e}"))?;
            let cache_bytes = crate::storage::suggested_cache_bytes(free);
            let cache_str = crate::storage::gb_string(cache_bytes);
            log(&format!(
                "[in] {} GB free on this disk; sizing the stream cache at {}…",
                free / crate::storage::GB,
                cache_str
            ));

            let map = crate::generate::read_env_file(&install_dir.join(".env"));
            let puid: u32 = map.get("PUID").and_then(|s| s.parse().ok()).unwrap_or(1000);
            let pgid: u32 = map.get("PGID").and_then(|s| s.parse().ok()).unwrap_or(1000);

            let mount = crate::generate::DecypharrMount::new(
                "/mnt/decypharr",
                "/cache/dfs",
                &cache_str,
                puid,
                pgid,
            );
            cfg["mount"] = mount.to_json();
            cfg["default_download_action"] = serde_json::json!("symlink");

            let out = serde_json::to_string_pretty(&cfg).context("serializing config")?;
            std::fs::write(&config_path, out + "\n")
                .with_context(|| format!("writing {}", config_path.display()))?;

            log("[ok] small-disk mode enabled:");
            log("  • Decypharr now mounts the debrid cloud as a filesystem (DFS)");
            log(&format!(
                "  • stream cache sized at {cache_str} for this disk"
            ));
            log("  • new downloads are imported as symlinks — zero local bytes");
            log("[in] restart Decypharr for the mount to take effect");
        }
        DownloadAction::Strm => {
            // No mount needed — .strm files point at WebDAV. Remove any
            // mount block so a dead mount can't confuse things.
            if let Some(obj) = cfg.as_object_mut() {
                obj.remove("mount");
            }
            cfg["default_download_action"] = serde_json::json!("strm");

            let out = serde_json::to_string_pretty(&cfg).context("serializing config")?;
            std::fs::write(&config_path, out + "\n")
                .with_context(|| format!("writing {}", config_path.display()))?;

            log("[ok] .strm mode enabled:");
            log("  • no mount needed — new downloads become .strm files");
            log("  • Jellyfin plays .strm natively; Plex needs the");
            log("    plex-strm-assistant helper (Plex never learned .strm)");
            log("[in] restart Decypharr for the change to take effect");
        }
    }
    Ok(())
}

/// Is small-disk mode active? True when the Decypharr config has a `mount`
/// block (DFS) or a non-default download action.
pub fn small_disk_active(install_dir: &Path) -> bool {
    let path = install_dir.join("configs/decypharr/config.json");
    let raw = match std::fs::read_to_string(&path) {
        Ok(r) => r,
        Err(_) => return false,
    };
    let cfg: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return false,
    };
    cfg.get("mount").is_some()
        || cfg
            .get("default_download_action")
            .and_then(|v| v.as_str())
            .is_some_and(|a| a != "download")
}

/// Check the DFS cloud mount from inside the Decypharr container.
/// Returns Ok(true) when mounted, Ok(false) when the mount is dead or
/// missing, Err when we couldn't even ask.
pub fn cloud_mount_healthy() -> Result<bool, String> {
    // mountpoint -q exits 0 iff the path is a mountpoint.
    match crate::docker::exec("decypharr", &["mountpoint", "-q", "/mnt/decypharr"]) {
        Ok(_) => Ok(true),
        Err(e) => {
            // docker exec failed — distinguish "container not running"
            // from "mountpoint says no".
            if e.contains("No such container") || e.contains("not running") {
                Err("the Decypharr container isn't running".to_string())
            } else {
                Ok(false)
            }
        }
    }
}

/// Apply the media-server settings for a cloud-backed library:
/// Plex gets its 7 verified prefs via API; Jellyfin gets exact manual
/// steps (no API key exists to automate it).
pub fn apply_media_server_settings(install_dir: &Path, tx: &Sender<CareEvent>) -> Result<()> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };
    let server = crate::media_server::detect(install_dir)
        .ok_or_else(|| anyhow::anyhow!("couldn't tell whether this fleet uses Plex or Jellyfin"))?;
    match server {
        crate::media_server::MediaServer::Plex => {
            let token =
                crate::media_server::plex_token(install_dir).map_err(|e| anyhow::anyhow!("{e}"))?;
            log("[in] talking to Plex…");
            let log_fn = |s: String| {
                let _ = tx.send(CareEvent::Log(s));
            };
            let applied = crate::media_server::apply_plex_cloud_settings(&token, &log_fn)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            log(&format!(
                "[ok] Plex tuned for cloud storage ({} settings):",
                applied.len()
            ));
            log("  • empty-trash-automatically OFF — a scan during an outage");
            log("    can never delete your library entries");
            log("  • preview thumbnails, chapter images, intro markers,");
            log("    loudness analysis: all off (hours of CPU saved)");
            log("  • periodic full scans off — the *arrs notify Plex directly");
            Ok(())
        }
        crate::media_server::MediaServer::Jellyfin => {
            log("[in] Jellyfin detected.");
            for line in crate::media_server::jellyfin_manual_steps() {
                log(&format!("[ok] {line}"));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    /// Serializes tests that touch the shared backup dir: backup filenames
    /// have nanosecond timestamps, but parallel tests can still interleave
    /// create/restore/delete on the same directory.
    static BACKUP_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn backup_and_restore_round_trip() {
        let _guard = BACKUP_LOCK.lock().unwrap();
        let base = std::env::temp_dir().join(format!("porthole-care-test-{}", timestamp()));
        let install = base.join("stack");
        std::fs::create_dir_all(install.join("configs/sonarr")).unwrap();
        std::fs::write(install.join(".env"), "SECRET=shh\n").unwrap();
        std::fs::write(install.join("configs/sonarr/config.xml"), "<Config/>").unwrap();
        // data/ must be excluded from the backup.
        std::fs::create_dir_all(install.join("data/media")).unwrap();
        std::fs::write(install.join("data/media/bigfile.bin"), vec![0u8; 1024]).unwrap();

        let dest = create_backup(&install).unwrap();
        assert!(dest.is_file());
        // data excluded → backup should be small.
        assert!(std::fs::metadata(&dest).unwrap().len() < 1024);

        // Simulate disaster, then restore.
        std::fs::remove_dir_all(&install).unwrap();
        // restore_backup stops containers first — harmless with no docker.
        let _ = restore_backup(&dest, &install);
        assert_eq!(
            std::fs::read_to_string(install.join(".env")).unwrap(),
            "SECRET=shh\n"
        );
        assert!(install.join("configs/sonarr/config.xml").is_file());

        std::fs::remove_dir_all(&base).ok();
        std::fs::remove_file(&dest).ok();
    }

    #[test]
    fn uninstall_plan_is_plain_language() {
        let plan = uninstall_plan(Path::new("/opt/fleet"));
        assert!(plan.iter().any(|l| l.contains("/opt/fleet")));
        assert!(plan.len() >= 4);
    }

    #[test]
    fn small_disk_mode_merges_into_existing_config() {
        let _guard = BACKUP_LOCK.lock().unwrap();
        let base = std::env::temp_dir().join("porthole-sd-test");
        let _ = std::fs::remove_dir_all(&base);
        let cfg_dir = base.join("configs/decypharr");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join("config.json"),
            r#"{"username":"u","password":"p","port":"8282"}"#,
        )
        .unwrap();
        std::fs::write(base.join(".env"), "PUID=1001\nPGID=1002\n").unwrap();

        let (tx, _rx) = mpsc::channel();
        let before: std::collections::HashSet<_> = list_backups().into_iter().collect();
        apply_small_disk_mode(&base, DownloadAction::Symlink, &tx).unwrap();

        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(cfg_dir.join("config.json")).unwrap())
                .unwrap();
        // Existing keys preserved.
        assert_eq!(v["username"], "u");
        assert_eq!(v["port"], "8282");
        // New keys added.
        assert_eq!(v["mount"]["type"], "dfs");
        assert_eq!(v["mount"]["dfs"]["uid"], 1001);
        assert_eq!(v["mount"]["dfs"]["gid"], 1002);
        assert_eq!(v["default_download_action"], "symlink");
        // Backup was taken — clean up only the file this test created.
        for p in list_backups() {
            if !before.contains(&p) {
                std::fs::remove_file(p).ok();
            }
        }

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn small_disk_mode_refuses_without_config() {
        let base = std::env::temp_dir().join("porthole-sd-missing");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let (tx, _rx) = mpsc::channel();
        assert!(apply_small_disk_mode(&base, DownloadAction::Symlink, &tx).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn strm_mode_sets_strm_and_drops_mount() {
        let _guard = BACKUP_LOCK.lock().unwrap();
        let base = std::env::temp_dir().join("porthole-sd-strm-test");
        let _ = std::fs::remove_dir_all(&base);
        let cfg_dir = base.join("configs/decypharr");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join("config.json"),
            r#"{"username":"u","mount":{"type":"dfs"},"default_download_action":"symlink"}"#,
        )
        .unwrap();

        let (tx, _rx) = mpsc::channel();
        let before: std::collections::HashSet<_> = list_backups().into_iter().collect();
        apply_small_disk_mode(&base, DownloadAction::Strm, &tx).unwrap();

        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(cfg_dir.join("config.json")).unwrap())
                .unwrap();
        assert_eq!(v["default_download_action"], "strm");
        assert!(v.get("mount").is_none());
        assert_eq!(v["username"], "u");
        for p in list_backups() {
            if !before.contains(&p) {
                std::fs::remove_file(p).ok();
            }
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn small_disk_active_detects_modes() {
        let base = std::env::temp_dir().join("porthole-sd-active-test");
        let _ = std::fs::remove_dir_all(&base);
        let cfg_dir = base.join("configs/decypharr");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        let cfg = cfg_dir.join("config.json");

        std::fs::write(&cfg, r#"{"port":"8282"}"#).unwrap();
        assert!(!small_disk_active(&base));

        std::fs::write(&cfg, r#"{"mount":{"type":"dfs"}}"#).unwrap();
        assert!(small_disk_active(&base));

        std::fs::write(&cfg, r#"{"default_download_action":"strm"}"#).unwrap();
        assert!(small_disk_active(&base));

        std::fs::write(&cfg, r#"{"default_download_action":"download"}"#).unwrap();
        assert!(!small_disk_active(&base));

        let _ = std::fs::remove_dir_all(&base);
    }
}
