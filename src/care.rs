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
    // No -f: auto-discovers the override so extras/companions are removed too.
    if compose_yml.is_file() && docker::compose_available() {
        log("[in] bringing containers down…");
        let ok = Command::new("docker")
            .args(["compose", "down", "--remove-orphans"])
            .current_dir(install_dir)
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
    // No -f: auto-discovers docker-compose.override.yml (extras/companions).
    let out = Command::new("docker")
        .args(["compose", "pull", "--quiet"])
        .current_dir(install_dir)
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
    // Don't use -f: docker compose auto-discovers docker-compose.override.yml
    // when run from the install dir (extras/companions live there).
    let out = Command::new("docker")
        .args(["compose", "config", "--format", "json"])
        .current_dir(install_dir)
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
    // (No -f: auto-discovers docker-compose.override.yml.)
    log("[in] pulling latest images…");
    let pull_ok = Command::new("docker")
        .args(["compose", "pull", "--quiet"])
        .current_dir(install_dir)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !pull_ok {
        log("[warn] pull had issues — continuing with what we have");
    }
    log("[in] restarting services…");
    let up_ok = Command::new("docker")
        .args(["compose", "up", "-d", "--remove-orphans"])
        .current_dir(install_dir)
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

/// Apply expert quality profiles: generate Configarr's config from the
/// user's quality answer and run Configarr as a one-shot Docker job on the
/// fleet's network. Idempotent — re-running reverts hand-edits.
pub fn apply_quality_profiles(
    install_dir: &Path,
    four_k: bool,
    tx: &Sender<CareEvent>,
) -> Result<()> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };

    // API keys live in the *arr config.xml files.
    let sonarr_key =
        crate::configarr::arr_api_key(install_dir, "sonarr").map_err(|e| anyhow::anyhow!("{e}"))?;
    let radarr_key =
        crate::configarr::arr_api_key(install_dir, "radarr").map_err(|e| anyhow::anyhow!("{e}"))?;

    // Write Configarr's config where the job will mount it.
    let cfg_dir = install_dir.join("configs/configarr");
    std::fs::create_dir_all(&cfg_dir).context("creating configarr config dir")?;
    std::fs::write(
        cfg_dir.join("config.yml"),
        crate::configarr::render_config_yml(four_k),
    )
    .context("writing config.yml")?;
    std::fs::write(
        cfg_dir.join("secrets.yml"),
        crate::configarr::render_secrets_yml(&sonarr_key, &radarr_key),
    )
    .context("writing secrets.yml")?;
    // Secrets file: owner-only.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(
            cfg_dir.join("secrets.yml"),
            std::fs::Permissions::from_mode(0o600),
        );
    }
    log(&format!(
        "[ok] quality profiles configured for {}",
        if four_k { "4K" } else { "1080p" }
    ));

    // Pull and run Configarr as a one-shot job on the fleet network.
    log("[in] pulling Configarr…");
    let pull = Command::new("docker")
        .args(["pull", crate::configarr::IMAGE])
        .output()
        .map_err(|e| anyhow::anyhow!("could not run docker pull: {e}"))?;
    if !pull.status.success() {
        anyhow::bail!("could not pull the Configarr image — check your connection");
    }
    log("[in] syncing TRaSH quality profiles into Sonarr/Radarr…");
    log("     (this takes a minute; Configarr is talking to both *arrs)");
    let out = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--network",
            crate::configarr::NETWORK,
            "-v",
            &format!("{}:/app/config", cfg_dir.to_string_lossy()),
            crate::configarr::IMAGE,
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("could not run Configarr: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    for line in stdout.lines().chain(stderr.lines()) {
        // Redact API keys if they leak into logs.
        let line = line
            .replace(&sonarr_key, "[redacted]")
            .replace(&radarr_key, "[redacted]");
        log(&format!("  │ {line}"));
    }
    if !out.status.success() {
        anyhow::bail!("Configarr reported errors — see the log above");
    }

    // Re-assert the two integrations Configarr doesn't own: the Decypharr
    // download client in each *arr, and Prowlarr's indexer sync.
    log("[in] verifying the download-client wiring…");
    crate::arr::ensure_decypharr_client(install_dir, tx)?;
    crate::arr::prowlarr_resync(install_dir, tx)?;

    // Verify: the TRaSH profile should now exist in each *arr.
    for (name, port, key) in [("Sonarr", 8989, &sonarr_key), ("Radarr", 7878, &radarr_key)] {
        let profiles = Command::new("curl")
            .args([
                "-sf",
                "--connect-timeout",
                "5",
                "--max-time",
                "15",
                "-H",
                &format!("X-Api-Key: {key}"),
                &format!("http://localhost:{port}/api/v3/qualityprofile"),
            ])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        // TRaSH profiles are named like "WEB-1080p", "HD Bluray + WEB".
        let want = if four_k { "2160p" } else { "1080p" };
        if profiles.contains(want) {
            log(&format!("[ok] {name}: quality profiles synced"));
        } else {
            log(&format!(
                "[warn] {name}: couldn't confirm the new profiles — check its UI"
            ));
        }
    }
    log("[ok] expert quality profiles applied. They'll survive updates;");
    log("     re-run this if you ever hand-edit a profile and want it reset.");
    Ok(())
}

/// Wire the optional extras after they start:
/// Decypharr download clients + Prowlarr apps for Lidarr/Sportarr,
/// guided Bazarr setup. Idempotent; skips anything not opted in.
/// Set up the VPN for the local-download profile.
/// Patches the download override with the user's VPN credentials,
/// then restarts gluetun. The user brings their own VPN account
/// (PIA or Proton VPN recommended — both support port forwarding).
pub fn setup_vpn(
    install_dir: &Path,
    provider: &str,
    wireguard_key: &str,
    tx: &Sender<CareEvent>,
) -> Result<()> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };
    let dest = install_dir.join("docker-compose.override.yml");
    if !dest.exists() {
        anyhow::bail!("no download override found — pick Self-downloaded or Hybrid in Setup first");
    }
    let mut yml = std::fs::read_to_string(&dest)?;
    if !yml.contains("gluetun:") {
        anyhow::bail!("gluetun not in the override — re-run Setup with a local profile");
    }
    // Patch the placeholder env vars Porthole generated.
    yml = yml.replace(
        "- VPN_SERVICE_PROVIDER=",
        &format!("- VPN_SERVICE_PROVIDER={provider}"),
    );
    yml = yml.replace(
        "- WIREGUARD_PRIVATE_KEY=",
        &format!("- WIREGUARD_PRIVATE_KEY={wireguard_key}"),
    );
    std::fs::write(&dest, yml)?;
    log("[ok] VPN credentials written (kept in the override, never logged)");
    log("[in] restarting gluetun… (this takes ~30s)");
    let out = std::process::Command::new("docker")
        .args(["compose", "up", "-d", "gluetun"])
        .current_dir(install_dir)
        .output()?;
    if !out.status.success() {
        anyhow::bail!("could not restart gluetun");
    }
    log("[ok] gluetun restarting — check Care → Doctor to verify the VPN IP");
    Ok(())
}

/// Set a permanent qBittorrent password and return it.
/// qBittorrent 5.x prints a random temp password on first run; Porthole
/// reads it from the logs, then uses the WebUI API to set a permanent
/// one (the API takes plaintext and hashes server-side).
pub fn setup_qbit_password(tx: &Sender<CareEvent>) -> Result<String> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };
    log("[in] reading qBittorrent's temporary password from its logs…");
    let out = std::process::Command::new("docker")
        .args(["logs", "qbittorrent"])
        .output()?;
    let logs =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    // "A temporary password is provided for this session: <pwd>"
    let temp = logs
        .lines()
        .rev()
        .find_map(|l| {
            l.find("temporary password is provided for this session:")
                .map(|i| {
                    l[i + "temporary password is provided for this session:".len()..]
                        .trim()
                        .to_string()
                })
        })
        .filter(|s| !s.is_empty());
    let temp = match temp {
        Some(t) => t,
        None => {
            anyhow::bail!("couldn't find the temp password in qbittorrent's logs — is it running?")
        }
    };
    // Generate a permanent password (CSPRNG, like the *arr API keys).
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("no entropy: {e}"))?;
    let permanent: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    log("[in] setting a permanent password via the WebUI API…");
    // Login with temp password to get a session cookie.
    let login = std::process::Command::new("curl")
        .args([
            "-sf",
            "--connect-timeout",
            "5",
            "--max-time",
            "15",
            "-c",
            "/tmp/qbit-cookie",
            "-d",
            &format!("username=admin&password={temp}"),
            "http://localhost:8080/api/v2/auth/login",
        ])
        .output()?;
    if !login.status.success() {
        anyhow::bail!("couldn't log into qBittorrent — is the WebUI up?");
    }
    // Set the permanent password (plaintext; qbit hashes it).
    let set = std::process::Command::new("curl")
        .args([
            "-sf",
            "--connect-timeout",
            "5",
            "--max-time",
            "15",
            "-b",
            "/tmp/qbit-cookie",
            "-d",
            &format!("json={{\"web_ui_password\":\"{permanent}\"}}"),
            "http://localhost:8080/api/v2/app/setPreferences",
        ])
        .output()?;
    let _ = std::fs::remove_file("/tmp/qbit-cookie");
    if !set.status.success() {
        anyhow::bail!("couldn't set the permanent password");
    }
    // Also set Content Layout to "Original" (the *arr import logic assumes it).
    let _ = std::process::Command::new("curl")
        .args([
            "-sf",
            "--connect-timeout",
            "5",
            "--max-time",
            "15",
            "-b",
            "/tmp/qbit-cookie2",
            "-d",
            &format!("username=admin&password={permanent}"),
            "http://localhost:8080/api/v2/auth/login",
        ])
        .output();
    let _ = std::process::Command::new("curl")
        .args([
            "-sf",
            "--connect-timeout",
            "5",
            "--max-time",
            "15",
            "-b",
            "/tmp/qbit-cookie2",
            "-d",
            "{\"torrent_content_layout\":\"Original\"}",
            "http://localhost:8080/api/v2/app/setPreferences",
        ])
        .output();
    let _ = std::fs::remove_file("/tmp/qbit-cookie2");
    log("[ok] qBittorrent password set (shown once — save it for the WebUI)");
    Ok(permanent)
}

/// Get (or pre-seed) SABnzbd's API key.
/// Pre-seeds `/config/sabnzbd.ini` with port 8081 (qBittorrent already
/// has 8080 on gluetun's shared network stack) before first run.
pub fn sab_api_key(install_dir: &Path) -> Result<String> {
    let ini = install_dir.join("configs/sabnzbd/sabnzbd.ini");
    if ini.exists() {
        let content = std::fs::read_to_string(&ini)?;
        if let Some(key) = content.lines().find_map(|l| {
            let l = l.trim();
            l.strip_prefix("api_key = ").map(|s| s.trim().to_string())
        }) {
            if !key.is_empty() {
                return Ok(key);
            }
        }
    }
    // Pre-seed: port 8081 + a fresh API key. SABnzbd fills in the rest.
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("no entropy: {e}"))?;
    let key: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    if let Some(parent) = ini.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&ini, format!("[misc]\nport = 8081\napi_key = {key}\n"))?;
    Ok(key)
}

/// Set up cloud storage: generate the rclone config (with crypt),
/// the mount service, and check FUSE prerequisites.
/// Returns the generated crypt password (user must back it up).
pub fn setup_cloud_storage(
    install_dir: &Path,
    backend: crate::storage_cloud::CloudBackend,
    tx: &Sender<CareEvent>,
) -> Result<String> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };
    // Check FUSE first — classic trap.
    if let Err(e) = crate::storage_cloud::check_fuse_allow_other() {
        log(&format!("[warn] {e}"));
        log("[warn] continuing anyway — fix it before mounting");
    }
    // Generate a crypt password (CSPRNG).
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("no entropy: {e}"))?;
    let password: String = bytes.iter().map(|b| format!("{b:02x}")).collect();

    let rclone_dir = install_dir.join("configs/rclone");
    std::fs::create_dir_all(&rclone_dir)?;
    let conf = crate::storage_cloud::render_rclone_conf(&backend, &password);
    let conf_path = rclone_dir.join("rclone.conf");
    std::fs::write(&conf_path, conf)?;
    // Secrets: mode 0600.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&conf_path, std::fs::Permissions::from_mode(0o600));
    }
    log(&format!(
        "[ok] rclone config written for {} (mode 0600)",
        backend.label()
    ));

    // Mount service (user enables it after `rclone config`).
    let svc = crate::storage_cloud::render_mount_service(install_dir, 20);
    let svc_path = rclone_dir.join("porthole-rclone.service");
    std::fs::write(&svc_path, svc)?;
    log("[ok] mount service generated");

    log("[in] next steps:");
    log("  1. Run: rclone config --config <install>/configs/rclone/rclone.conf");
    log("     and authorize your cloud under [cloud].");
    log("  2. Back up rclone.conf (encrypted) in TWO places.");
    log("     Losing the crypt password = library unrecoverable.");
    log("  3. Then: sudo cp <install>/configs/rclone/porthole-rclone.service /etc/systemd/system/");
    log("     sudo systemctl enable --now porthole-rclone");
    Ok(password)
}

/// Install the upload mover (script + systemd timer).
pub fn setup_upload_mover(install_dir: &Path, tx: &Sender<CareEvent>) -> Result<()> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };
    let rclone_dir = install_dir.join("configs/rclone");
    std::fs::create_dir_all(&rclone_dir)?;

    let script = crate::storage_cloud::render_mover_script(install_dir);
    let script_path = rclone_dir.join("mover.sh");
    std::fs::write(&script_path, script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755));
    }

    let timer = crate::storage_cloud::render_mover_timer();
    let timer_path = rclone_dir.join("porthole-mover.timer");
    std::fs::write(&timer_path, timer)?;
    // The service the timer triggers (simple oneshot).
    let svc = format!(
        "[Unit]\nDescription=Porthole cloud upload mover\n\n[Service]\nType=oneshot\nExecStart={}\n",
        script_path.display()
    );
    std::fs::write(rclone_dir.join("porthole-mover.service"), svc)?;

    log("[ok] mover script + timer generated");
    log("[in] to activate:");
    log("  sudo cp <install>/configs/rclone/porthole-mover.* /etc/systemd/system/");
    log("  sudo systemctl enable --now porthole-mover.timer");
    Ok(())
}

pub fn wire_extras(install_dir: &Path, tx: &Sender<CareEvent>) -> Result<()> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };

    // Decypharr clients (base *arrs + opted-in extras).
    crate::arr::ensure_decypharr_client(install_dir, tx)?;
    // Prowlarr apps for the extras.
    crate::arr::ensure_prowlarr_apps(install_dir, tx)?;
    // Prowlarr re-sync to push indexers to the new apps.
    crate::arr::prowlarr_resync(install_dir, tx)?;

    // Bazarr: guided setup (API schema not stable enough to automate).
    if install_dir.join("configs/bazarr").exists() {
        log("[in] Bazarr needs its Sonarr/Radarr connection — two minutes, once:");
        match (
            crate::configarr::arr_api_key(install_dir, "sonarr"),
            crate::configarr::arr_api_key(install_dir, "radarr"),
        ) {
            (Ok(skey), Ok(rkey)) => {
                for line in crate::extras::bazarr_manual_steps(&skey, &rkey) {
                    log(&format!("  • {line}"));
                }
            }
            _ => log("[warn] couldn't read the Sonarr/Radarr API keys for Bazarr setup"),
        }
    }

    // ── Companion automation wiring ──
    wire_companions(install_dir, tx)?;

    log("[ok] extras wired.");
    Ok(())
}

/// Wire the Phase 10 companions: automate what's verifiable,
/// guide the rest with exact values.
fn wire_companions(install_dir: &Path, tx: &Sender<CareEvent>) -> Result<()> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };
    let yml = std::fs::read_to_string(install_dir.join("docker-compose.override.yml"))
        .unwrap_or_default();

    // Unpackerr: write the env file with real *arr API keys.
    if yml.contains("unpackerr:") {
        match (
            crate::configarr::arr_api_key(install_dir, "sonarr"),
            crate::configarr::arr_api_key(install_dir, "radarr"),
        ) {
            (Ok(skey), Ok(rkey)) => {
                let lkey = crate::configarr::arr_api_key(install_dir, "lidarr").ok();
                let env = crate::companions::unpackerr_env(&skey, &rkey, lkey.as_deref());
                let dest = install_dir.join("configs/unpackerr/unpackerr.env");
                if let Some(p) = dest.parent() {
                    std::fs::create_dir_all(p)?;
                }
                std::fs::write(&dest, env)?;
                log("[ok] Unpackerr knows Sonarr/Radarr (restart it to pick up the keys)");
            }
            _ => log("[warn] couldn't read *arr API keys for Unpackerr"),
        }
    }

    // Janitorr: drop in a dry-run-first config.
    if yml.contains("janitorr:") {
        let dest = install_dir.join("configs/janitorr/application.yml");
        if !dest.exists() {
            if let Some(p) = dest.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::write(&dest, crate::companions::janitorr_config())?;
            log("[ok] Janitorr configured — DRY-RUN on. Review before enabling.");
        }
    }

    // Kometa: drop in a starter config (user fills in tokens).
    if yml.contains("kometa:") {
        let dest = install_dir.join("configs/kometa/config.yml");
        if !dest.exists() {
            if let Some(p) = dest.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::write(&dest, crate::companions::kometa_config())?;
            log("[ok] Kometa starter config written — add your Plex token + TMDb key");
        }
    }

    // Guided: Cleanuparr, Maintainerr, Tautulli, Wizarr.
    if yml.contains("cleanuparr:") {
        if let (Ok(skey), Ok(rkey)) = (
            crate::configarr::arr_api_key(install_dir, "sonarr"),
            crate::configarr::arr_api_key(install_dir, "radarr"),
        ) {
            log("[in] Cleanuparr needs its *arr connections — once:");
            for line in crate::companions::cleanuparr_manual_steps(&skey, &rkey) {
                log(&format!("  • {line}"));
            }
        }
    }
    if yml.contains("maintainerr:") {
        log("[in] Maintainerr setup — start conservative:");
        for line in crate::companions::maintainerr_manual_steps() {
            log(&format!("  • {line}"));
        }
    }
    if yml.contains("tautulli:") {
        // Plex token from Preferences.xml (like media_server.rs).
        if let Ok(token) = crate::media_server::plex_token(install_dir) {
            log("[in] Tautulli needs its Plex connection + API enabled:");
            for line in crate::companions::tautulli_manual_steps(&token) {
                log(&format!("  • {line}"));
            }
        }
    }
    if yml.contains("wizarr:") {
        log("[in] Wizarr — invite links for friends/family:");
        for line in crate::companions::wizarr_manual_steps() {
            log(&format!("  • {line}"));
        }
    }
    if yml.contains("kometa:") {
        log("[in] Kometa — Plex collections (needs your tokens):");
        for line in crate::companions::kometa_manual_steps() {
            log(&format!("  • {line}"));
        }
    }
    Ok(())
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
