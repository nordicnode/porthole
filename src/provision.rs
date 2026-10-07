//! Real provisioning: the wizard collects preferences, shows a dry-run plan,
//! then orchestrates the proven TorBox-Media-Server installer with live logs.
//!
//! Design: orchestrate, don't rewrite. The shell installer is battle-tested
//! (55 stars, real users); Porthole drives it non-interactively
//! (`setup.sh --yes` + env vars) and streams its output into the UI.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;

use crate::docker;
use crate::services::SERVICES;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StepStatus {
    Pending,
    Active,
    Done,
    Failed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MediaServer {
    Plex,
    Jellyfin,
}

impl MediaServer {
    pub fn as_str(self) -> &'static str {
        match self {
            MediaServer::Plex => "plex",
            MediaServer::Jellyfin => "jellyfin",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MediaServer::Plex => "Plex",
            MediaServer::Jellyfin => "Jellyfin",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            MediaServer::Plex => MediaServer::Jellyfin,
            MediaServer::Jellyfin => MediaServer::Plex,
        }
    }
}

/// Everything the installer needs, collected by the wizard form.
#[derive(Clone, Debug)]
pub struct Preferences {
    pub torbox_api_key: String,
    pub install_dir: String,
    pub media_server: MediaServer,
    pub puid: String,
    pub pgid: String,
    pub tz: String,
    /// Quality profile choice: false = 1080p, true = 4K. Drives Configarr.
    pub quality_4k: bool,
    /// Optional fleet members.
    pub extras: crate::extras::Extras,
    /// How downloads happen: debrid vs self-downloaded vs hybrid.
    pub fleet_profile: crate::download::FleetProfile,
    /// Which debrid service (for Debrid/Hybrid profiles).
    pub debrid_provider: crate::download::DebridProvider,
}

impl Default for Preferences {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        Self {
            torbox_api_key: String::new(),
            install_dir: format!("{home}/porthole-stack"),
            media_server: MediaServer::Plex,
            puid: "1000".to_string(),
            pgid: "1000".to_string(),
            tz: "UTC".to_string(),
            quality_4k: false,
            extras: crate::extras::Extras::default(),
            fleet_profile: crate::download::FleetProfile::default(),
            debrid_provider: crate::download::DebridProvider::default(),
        }
    }
}

impl Preferences {
    /// Human-readable problems; empty means good to go.
    pub fn validate(&self) -> Vec<String> {
        let mut errs = Vec::new();
        if self.torbox_api_key.trim().is_empty() {
            errs.push("TorBox API key is required — grab one at torbox.app".to_string());
        }
        if self.install_dir.trim().is_empty() {
            errs.push("Install directory can't be empty".to_string());
        }
        for (label, v) in [("PUID", &self.puid), ("PGID", &self.pgid)] {
            if v.trim().parse::<u32>().is_err() {
                errs.push(format!("{label} must be a number"));
            }
        }
        if self.tz.trim().is_empty() {
            errs.push("Timezone can't be empty (e.g. America/Los_Angeles)".to_string());
        }
        errs
    }

    /// Env vars for `setup.sh --yes`. Never log these values directly —
    /// the API key must stay masked (see `masked_env` and log redaction).
    pub fn env_vars(&self) -> Vec<(String, String)> {
        vec![
            ("TORBOX_API_KEY".to_string(), self.torbox_api_key.clone()),
            ("TORBOX_INSTALL_DIR".to_string(), self.install_dir.clone()),
            (
                "TORBOX_MEDIA_SERVER".to_string(),
                self.media_server.as_str().to_string(),
            ),
            ("PUID".to_string(), self.puid.clone()),
            ("PGID".to_string(), self.pgid.clone()),
            ("TZ".to_string(), self.tz.clone()),
            ("TORBOX_START_SERVICES".to_string(), "true".to_string()),
        ]
    }

    /// Env vars safe to *display* (secret values masked).
    pub fn masked_env(&self) -> Vec<(String, String)> {
        self.env_vars()
            .into_iter()
            .map(|(k, v)| {
                let v = if k == "TORBOX_API_KEY" {
                    "•".repeat(v.chars().count().min(24))
                } else {
                    v
                };
                (k, v)
            })
            .collect()
    }
}

/// Service IDs that should be running after provisioning,
/// based on the user's choices (not everything in SERVICES).
fn expected_service_ids(prefs: &Preferences) -> Vec<&'static str> {
    let mut ids = vec![
        "decypharr",
        "prowlarr",
        "byparr",
        "sonarr",
        "radarr",
        "seerr",
    ];
    match prefs.media_server {
        MediaServer::Plex => ids.push("plex"),
        MediaServer::Jellyfin => ids.push("jellyfin"),
    }
    let e = &prefs.extras;
    if e.lidarr {
        ids.push("lidarr");
    }
    if e.bazarr {
        ids.push("bazarr");
    }
    if e.sportarr {
        ids.push("sportarr");
    }
    if e.autobrr {
        ids.push("autobrr");
    }
    if e.unpackerr {
        ids.push("unpackerr");
    }
    if e.cleanuparr {
        ids.push("cleanuparr");
    }
    if e.maintainerr {
        ids.push("maintainerr");
    }
    if e.janitorr {
        ids.push("janitorr");
    }
    if e.tautulli {
        ids.push("tautulli");
    }
    if e.jellystat {
        ids.push("jellystat");
        ids.push("jellystat-db");
    }
    if e.wizarr {
        ids.push("wizarr");
    }
    if e.kometa {
        ids.push("kometa");
    }
    // Local-download profile (gluetun has no UI port, but qbit/sab do).
    if prefs.fleet_profile.needs_local_clients() {
        ids.push("qbittorrent");
        ids.push("sabnzbd");
    }
    ids
}

/// One provisioning step.
pub struct StepDef {
    pub title: &'static str,
    pub plain: &'static str,
    pub wires_up: &'static str,
}

pub static STEPS: &[StepDef] = &[
    StepDef {
        title: "Check the toolbox",
        plain: "Make sure Docker and the other tools are installed.",
        wires_up: "—",
    },
    StepDef {
        title: "Check the doors",
        plain: "Make sure no other app is sitting on the ports your fleet needs.",
        wires_up: "—",
    },
    StepDef {
        title: "Fetch the installer",
        plain: "Download the proven setup scripts (TorBox-Media-Server).",
        wires_up: "—",
    },
    StepDef {
        title: "Build, launch & introduce",
        plain: "Generate configs, start all services, and wire them together.",
        wires_up: "Decypharr→Sonarr/Radarr · Prowlarr→Sonarr/Radarr · Seerr→everything",
    },
    StepDef {
        title: "Verify the fleet",
        plain: "Check every service actually came up healthy.",
        wires_up: "—",
    },
    StepDef {
        title: "Tune the quality",
        plain: "Apply the TRaSH Guides' expert quality profiles to Sonarr and Radarr.",
        wires_up: "Configarr→Sonarr/Radarr",
    },
    StepDef {
        title: "Wire the extras",
        plain: "Connect Lidarr, Bazarr and Sportarr (if you picked them).",
        wires_up: "Decypharr→Lidarr/Sportarr · Prowlarr→Lidarr/Sportarr",
    },
];

/// Events the worker thread sends back to the UI.
pub enum ProvEvent {
    Log(String),
    StepBegin(usize),
    StepDone(usize, bool),
    Finished(bool),
}

pub const INSTALLER_REPO: &str = "https://github.com/nordicnode/TorBox-Media-Server";

pub fn installer_dir() -> PathBuf {
    docker::data_dir().join("torbox-media-server")
}

/// Run the full provisioning in the calling thread (spawn it in a worker
/// thread; it blocks for minutes). Sends events on `tx`.
pub fn run_provision(prefs: Preferences, tx: Sender<ProvEvent>) {
    let send = |e: ProvEvent| {
        let _ = tx.send(e);
    };
    let log = |s: &str| send(ProvEvent::Log(s.to_string()));

    // Remember the quality answer for later re-runs (Care action).
    let quality_path = std::path::Path::new(&prefs.install_dir).join(".porthole-quality");
    if let Some(parent) = quality_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        &quality_path,
        if prefs.quality_4k { "4k\n" } else { "1080p\n" },
    );
    // Remember the download choices too (Doctor + Care need them).
    let profile_path = std::path::Path::new(&prefs.install_dir).join(".porthole-profile");
    let _ = std::fs::write(
        &profile_path,
        format!(
            "{}\n{}\n{}\n",
            match prefs.fleet_profile {
                crate::download::FleetProfile::Debrid => "debrid",
                crate::download::FleetProfile::Local => "local",
                crate::download::FleetProfile::Hybrid => "hybrid",
            },
            prefs.debrid_provider.decypharr_id(),
            prefs.media_server.as_str(),
        ),
    );

    // Write the unified override (extras + download clients) BEFORE setup.sh
    // runs `docker compose up` (the installer's compose wrapper auto-discovers it).
    // Single valid YAML — never append fragments.
    let install = std::path::Path::new(&prefs.install_dir);
    if let Err(e) = crate::extras::write_full_override(install, &prefs.extras, &prefs.fleet_profile)
    {
        log(&format!("[warn] could not write override: {e:#}"));
    } else if prefs.fleet_profile.needs_local_clients() {
        log("[ok] download clients (gluetun + qBittorrent + SABnzbd) queued");
    }
    if let Err(e) = crate::extras::ensure_data_dirs(install, &prefs.extras) {
        log(&format!("[warn] could not create extras data dirs: {e:#}"));
    }
    if prefs.fleet_profile == crate::download::FleetProfile::Local
        || prefs.fleet_profile == crate::download::FleetProfile::Hybrid
    {
        log("[note] Self-downloaded needs a VPN login — add it in Care → Set up VPN for downloads");
    }

    // ── Step 0: toolbox ──
    send(ProvEvent::StepBegin(0));
    let mut toolbox_ok = true;
    for tool in ["docker", "git", "curl"] {
        if docker::command_exists(tool) {
            log(&format!("[ok] {tool} found"));
        } else {
            log(&format!("[warn] {tool} not found"));
            if tool == "docker" {
                toolbox_ok = false;
            }
        }
    }
    if docker::compose_available() {
        log("[ok] docker compose plugin found");
    } else {
        log("[warn] 'docker compose' plugin not found — the installer will try to install it (may need sudo)");
    }
    if !toolbox_ok {
        log("[warn] continuing anyway — the installer can install Docker itself");
    }
    send(ProvEvent::StepDone(0, true));

    // ── Step 1: ports ──
    // Only check ports for services that will actually be installed:
    // the base fleet + selected media server + opted-in extras/companions.
    // (Checking Plex's port when the user picked Jellyfin was a real bug.)
    send(ProvEvent::StepBegin(1));
    let mut ports_ok = true;
    let mut needed_ports: Vec<(u16, &str)> = vec![
        (8282, "Decypharr"),
        (9696, "Prowlarr"),
        (8191, "Byparr"),
        (8989, "Sonarr"),
        (7878, "Radarr"),
        (5055, "Seerr"),
    ];
    match prefs.media_server {
        crate::provision::MediaServer::Plex => needed_ports.push((32400, "Plex")),
        crate::provision::MediaServer::Jellyfin => needed_ports.push((8096, "Jellyfin")),
    }
    // Extras (only opted-in).
    let e = &prefs.extras;
    if e.lidarr {
        needed_ports.push((8686, "Lidarr"));
    }
    if e.bazarr {
        needed_ports.push((6767, "Bazarr"));
    }
    if e.sportarr {
        needed_ports.push((1867, "Sportarr"));
    }
    if e.autobrr {
        needed_ports.push((7474, "autobrr"));
    }
    if e.cleanuparr {
        needed_ports.push((11011, "Cleanuparr"));
    }
    if e.maintainerr {
        needed_ports.push((6246, "Maintainerr"));
    }
    if e.janitorr {
        needed_ports.push((8978, "Janitorr"));
    }
    if e.tautulli {
        needed_ports.push((8181, "Tautulli"));
    }
    if e.jellystat {
        needed_ports.push((3000, "Jellystat"));
    }
    if e.wizarr {
        needed_ports.push((5690, "Wizarr"));
    }
    // Local-download profile: gluetun publishes qbit/sab ports.
    if prefs.fleet_profile.needs_local_clients() {
        needed_ports.push((8080, "qBittorrent"));
        needed_ports.push((8081, "SABnzbd"));
    }
    for (port, name) in needed_ports {
        if docker::port_in_use(port) {
            log(&format!(
                "[fail] port {port} is already in use (needed by {name})"
            ));
            ports_ok = false;
        } else {
            log(&format!("[ok] port {port} free ({name})"));
        }
    }
    send(ProvEvent::StepDone(1, ports_ok));
    if !ports_ok {
        log("[fail] free the ports above, then run setup again");
        send(ProvEvent::Finished(false));
        return;
    }

    // ── Step 2: fetch installer ──
    send(ProvEvent::StepBegin(2));
    let dir = installer_dir();
    let fetch_ok = if dir.join("setup.sh").exists() {
        log(&format!("[in] installer already at {}", dir.display()));
        let updated = Command::new("git")
            .args(["-C", &dir.to_string_lossy(), "pull", "--ff-only"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if updated {
            log("[ok] installer updated to latest");
        } else {
            log("[warn] couldn't update installer; using the existing copy");
        }
        true
    } else {
        if let Some(parent) = dir.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        log(&format!(
            "[in] downloading installer from {INSTALLER_REPO} …"
        ));
        let cloned = Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                INSTALLER_REPO,
                &dir.to_string_lossy(),
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if cloned {
            log("[ok] installer downloaded");
        } else {
            log("[fail] couldn't download the installer — check your network and git");
        }
        cloned
    };
    send(ProvEvent::StepDone(2, fetch_ok));
    if !fetch_ok {
        send(ProvEvent::Finished(false));
        return;
    }

    // ── Step 3: execute setup.sh --yes ──
    send(ProvEvent::StepBegin(3));
    log("[in] running setup.sh --yes — first run takes a few minutes");
    let mut cmd = Command::new("bash");
    cmd.arg("setup.sh")
        .arg("--yes")
        .current_dir(&dir)
        .stdin(Stdio::null()) // never let it prompt; prefs came from the form
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in prefs.env_vars() {
        cmd.env(k, v);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            log(&format!("[fail] couldn't start the installer: {e}"));
            send(ProvEvent::StepDone(3, false));
            send(ProvEvent::Finished(false));
            return;
        }
    };
    // Stream stdout and stderr concurrently so a chatty stream can't deadlock.
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let tx_out = tx.clone();
    let h_out = std::thread::spawn(move || {
        if let Some(out) = stdout {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                let _ = tx_out.send(ProvEvent::Log(format!("  {line}")));
            }
        }
    });
    let tx_err = tx.clone();
    let h_err = std::thread::spawn(move || {
        if let Some(err) = stderr {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                let _ = tx_err.send(ProvEvent::Log(format!("  {line}")));
            }
        }
    });
    let exec_ok = child.wait().map(|s| s.success()).unwrap_or(false);
    let _ = h_out.join();
    let _ = h_err.join();
    send(ProvEvent::StepDone(3, exec_ok));
    if !exec_ok {
        log("[fail] the installer reported an error — see the log above");
        send(ProvEvent::Finished(false));
        return;
    }

    // ── Step 4: verify ──
    send(ProvEvent::StepBegin(4));
    std::thread::sleep(std::time::Duration::from_secs(3));
    let statuses = docker::service_statuses();
    let mut all_ok = true;
    // Only expect services that should be installed (not the ones the
    // user didn't pick — their absence is correct, not a failure).
    let expected = expected_service_ids(&prefs);
    for svc in SERVICES.iter().filter(|s| s.port != 0) {
        if !expected.contains(&svc.id) {
            continue;
        }
        match statuses
            .get(svc.id)
            .copied()
            .unwrap_or(docker::ServiceStatus::NotInstalled)
        {
            docker::ServiceStatus::Running => {
                log(&format!(
                    "[ok] {} is running → http://localhost:{}",
                    svc.name, svc.port
                ));
            }
            other => {
                log(&format!("[warn] {} isn't running ({other:?})", svc.name));
                all_ok = false;
            }
        }
    }
    send(ProvEvent::StepDone(4, all_ok));
    if all_ok {
        log("[ok] fleet verified — request something in Seerr and watch it appear");
    } else {
        log("[warn] some services didn't come up — check the log above");
    }

    // ── Step 5: expert quality profiles (Configarr) ──
    // Best-effort: a Configarr failure must not fail the whole install.
    if all_ok {
        // Point Decypharr at the chosen debrid provider first (the
        // installer always writes TorBox; correct it when different).
        if prefs.fleet_profile.needs_debrid()
            && prefs.debrid_provider != crate::download::DebridProvider::TorBox
        {
            let install = std::path::Path::new(&prefs.install_dir);
            match crate::download::set_debrid_provider(
                install,
                &prefs.debrid_provider,
                &prefs.torbox_api_key,
            ) {
                Ok(()) => log(&format!(
                    "[ok] Decypharr now uses {}",
                    prefs.debrid_provider.label()
                )),
                Err(e) => log(&format!("[warn] debrid provider not switched: {e:#}")),
            }
        }
        send(ProvEvent::StepBegin(5));
        log("[in] applying expert quality profiles (TRaSH Guides)…");
        let install = std::path::Path::new(&prefs.install_dir);
        let (ctx_tx, ctx_rx) = std::sync::mpsc::channel();
        let quality_ok =
            match crate::care::apply_quality_profiles(install, prefs.quality_4k, &ctx_tx) {
                Ok(()) => true,
                Err(e) => {
                    log(&format!("[warn] quality profiles skipped: {e:#}"));
                    log("[warn] you can apply them later: Care → Apply expert quality profiles");
                    false
                }
            };
        // Drain Configarr's progress into the provision log (redacted).
        for msg in ctx_rx.try_iter() {
            if let crate::care::CareEvent::Log(line) = msg {
                log(&line);
            }
        }
        send(ProvEvent::StepDone(5, quality_ok));
    }

    // ── Step 6: wire the extras ──
    // Best-effort like Step 5.
    if all_ok && prefs.extras.any() {
        send(ProvEvent::StepBegin(6));
        let install = std::path::Path::new(&prefs.install_dir);
        let (ctx_tx, ctx_rx) = std::sync::mpsc::channel();
        let extras_ok = match crate::care::wire_extras(install, &ctx_tx) {
            Ok(()) => true,
            Err(e) => {
                log(&format!("[warn] extras wiring skipped: {e:#}"));
                false
            }
        };
        for msg in ctx_rx.try_iter() {
            if let crate::care::CareEvent::Log(line) = msg {
                log(&line);
            }
        }
        send(ProvEvent::StepDone(6, extras_ok));
    }

    send(ProvEvent::Finished(all_ok));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_prefs() -> Preferences {
        Preferences {
            torbox_api_key: "tb-test-key".to_string(),
            ..Preferences::default()
        }
    }

    #[test]
    fn validation_requires_api_key() {
        let errs = Preferences::default().validate();
        assert!(errs.iter().any(|e| e.contains("API key")));
    }

    #[test]
    fn validation_accepts_good_prefs() {
        assert!(valid_prefs().validate().is_empty());
    }

    #[test]
    fn validation_rejects_bad_ids() {
        let mut p = valid_prefs();
        p.puid = "not-a-number".to_string();
        assert!(valid_prefs().validate().is_empty());
        assert!(p.validate().iter().any(|e| e.contains("PUID")));
    }

    #[test]
    fn masked_env_never_shows_the_key() {
        let p = valid_prefs();
        let env = p.masked_env();
        let key = env
            .iter()
            .find(|(k, _)| k == "TORBOX_API_KEY")
            .map(|(_, v)| v.clone())
            .unwrap();
        assert!(!key.contains("tb-test-key"));
        // …but the real env still carries it for the installer.
        let real = p
            .env_vars()
            .into_iter()
            .find(|(k, _)| k == "TORBOX_API_KEY")
            .map(|(_, v)| v)
            .unwrap();
        assert_eq!(real, "tb-test-key");
    }

    #[test]
    fn steps_are_defined() {
        assert!(!STEPS.is_empty());
        assert!(STEPS.iter().any(|s| s.title.contains("introduce")));
    }

    #[test]
    fn expected_services_match_choices() {
        // Base + Plex (default), no extras.
        let p = valid_prefs();
        let ids = expected_service_ids(&p);
        assert!(ids.contains(&"sonarr"));
        assert!(ids.contains(&"plex"));
        assert!(!ids.contains(&"jellyfin"));
        assert!(!ids.contains(&"lidarr"));

        // Jellyfin instead of Plex.
        let mut p2 = valid_prefs();
        p2.media_server = MediaServer::Jellyfin;
        let ids2 = expected_service_ids(&p2);
        assert!(ids2.contains(&"jellyfin"));
        assert!(!ids2.contains(&"plex"));

        // Opted-in extras appear; others don't.
        let mut p3 = valid_prefs();
        p3.extras.lidarr = true;
        p3.extras.jellystat = true;
        let ids3 = expected_service_ids(&p3);
        assert!(ids3.contains(&"lidarr"));
        assert!(ids3.contains(&"jellystat"));
        assert!(ids3.contains(&"jellystat-db"));
        assert!(!ids3.contains(&"bazarr"));

        // Local profile adds the download clients.
        let mut p4 = valid_prefs();
        p4.fleet_profile = crate::download::FleetProfile::Local;
        let ids4 = expected_service_ids(&p4);
        assert!(ids4.contains(&"qbittorrent"));
        assert!(ids4.contains(&"sabnzbd"));
    }
}
