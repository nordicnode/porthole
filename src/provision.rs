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
    send(ProvEvent::StepBegin(1));
    let mut ports_ok = true;
    for svc in SERVICES.iter().filter(|s| s.port != 0) {
        if docker::port_in_use(svc.port) {
            log(&format!(
                "[fail] port {} is already in use (needed by {})",
                svc.port, svc.name
            ));
            ports_ok = false;
        } else {
            log(&format!("[ok] port {} free ({})", svc.port, svc.name));
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
    for svc in SERVICES.iter().filter(|s| s.port != 0) {
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
}
