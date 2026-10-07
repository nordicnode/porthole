//! Docker inspection: is Docker here, and what state is the fleet in?
//!
//! Everything here shells out to the real `docker` CLI — no daemons, no
//! SDKs. If Docker isn't installed, callers get honest "unknown" answers
//! instead of errors.

use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ServiceStatus {
    Running,
    Stopped,
    Failed,
    NotInstalled,
    /// Docker itself isn't available, so we can't know.
    Unknown,
}

/// Container names Porthole manages (mirrors docker-compose.yml).
/// Matches `Service::id` in services.rs for the eight local services.
pub const CONTAINERS: &[&str] = &[
    "decypharr",
    "prowlarr",
    "byparr",
    "radarr",
    "sonarr",
    "seerr",
    "plex",
    "jellyfin",
];

pub fn command_exists(cmd: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {cmd} >/dev/null 2>&1"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn docker_available() -> bool {
    command_exists("docker")
}

pub fn compose_available() -> bool {
    Command::new("docker")
        .args(["compose", "version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Query `docker ps -a` and map container names to their status.
/// Containers that don't exist at all report `NotInstalled`.
pub fn service_statuses() -> HashMap<String, ServiceStatus> {
    let mut map: HashMap<String, ServiceStatus> = CONTAINERS
        .iter()
        .map(|c| ((*c).to_string(), ServiceStatus::NotInstalled))
        .collect();

    if !docker_available() {
        for v in map.values_mut() {
            *v = ServiceStatus::Unknown;
        }
        return map;
    }

    let out = Command::new("docker")
        .args(["ps", "-a", "--format", "{{.Names}}\t{{.State}}"])
        .output();

    if let Ok(out) = out {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let mut parts = line.split('\t');
            if let (Some(name), Some(state)) = (parts.next(), parts.next()) {
                let status = match state {
                    "running" => ServiceStatus::Running,
                    "paused" | "created" | "exited" => ServiceStatus::Stopped,
                    _ => ServiceStatus::Failed, // restarting, dead, removing…
                };
                if map.contains_key(name) {
                    map.insert(name.to_string(), status);
                }
            }
        }
    }
    map
}

/// True if something is already listening on 127.0.0.1:port.
pub fn port_in_use(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_err()
}

/// True if we can open a TCP connection to 127.0.0.1:port within the timeout.
/// Used for health checks: the container may exist without its web UI
/// actually answering yet.
pub fn port_open(port: u16, timeout_ms: u64) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms)).is_ok()
}

fn docker_cmd(args: &[&str]) -> bool {
    Command::new("docker")
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Start a container by name. Returns true if Docker reported success.
pub fn start_container(name: &str) -> bool {
    docker_cmd(&["start", name])
}

/// Stop a container by name. Returns true if Docker reported success.
pub fn stop_container(name: &str) -> bool {
    docker_cmd(&["stop", name])
}

/// Restart a container by name. Returns true if Docker reported success.
pub fn restart_container(name: &str) -> bool {
    docker_cmd(&["restart", name])
}

/// Porthole's own data dir: installer checkouts, etc.
pub fn data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".local/share/porthole")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_commands() {
        assert!(command_exists("sh"));
        assert!(!command_exists("definitely-not-a-real-command-xyz"));
    }

    #[test]
    fn port_check_agrees_with_os() {
        // A port we hold must read as in-use.
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let held = listener.local_addr().unwrap().port();
        assert!(port_in_use(held));

        // A port we just verified free by binding it ourselves must read as
        // free. Releasing a socket can lag the OS by a moment, so retry.
        for _ in 0..50 {
            let probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let free = probe.local_addr().unwrap().port();
            drop(probe);
            if !port_in_use(free) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("port_in_use never agreed a released port was free");
    }

    #[test]
    fn statuses_cover_the_fleet() {
        let map = service_statuses();
        for c in CONTAINERS {
            assert!(map.contains_key(*c), "missing status for {c}");
        }
    }

    #[test]
    fn port_open_matches_listener() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(port_open(port, 500));
        drop(listener);
        // A port nothing listens on must read closed (retry: release can lag).
        for _ in 0..50 {
            if !port_open(port, 200) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("port_open never agreed a released port was closed");
    }
}
