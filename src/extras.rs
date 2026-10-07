//! Optional fleet members: media (Lidarr/Bazarr/Sportarr), downloads
//! (autobrr), maintenance (Unpackerr/Cleanuparr/Maintainerr/Janitorr),
//! stats (Tautulli/Jellystat), sharing (Wizarr/Kometa).
//!
//! Porthole doesn't patch the installer's `docker-compose.yml`. Instead it
//! generates `docker-compose.override.yml`, which the installer's compose
//! wrapper auto-discovers (it `cd`s into the install dir). Deleting the
//! override removes the extras; regenerating it changes the selection.
//! Idempotent and non-destructive to the base fleet.

use anyhow::{Context, Result};
use std::path::Path;

/// Which optional services the user wants.
#[derive(Clone, Copy, Debug, Default)]
pub struct Extras {
    // Media
    pub lidarr: bool,
    pub bazarr: bool,
    pub sportarr: bool,
    // Downloads
    pub autobrr: bool,
    // Maintenance (the fleet looks after itself)
    pub unpackerr: bool,
    pub cleanuparr: bool,
    pub maintainerr: bool,
    pub janitorr: bool,
    // Stats (one per media server)
    pub tautulli: bool,
    pub jellystat: bool,
    // Sharing
    pub wizarr: bool,
    pub kometa: bool,
}

impl Extras {
    pub fn any(&self) -> bool {
        self.lidarr
            || self.bazarr
            || self.sportarr
            || self.autobrr
            || self.unpackerr
            || self.cleanuparr
            || self.maintainerr
            || self.janitorr
            || self.tautulli
            || self.jellystat
            || self.wizarr
            || self.kometa
    }
}

/// Render `docker-compose.override.yml`.
///
/// Conventions mirror the installer's compose: localhost-bound ports,
/// `${CONFIG_DIR}/<svc>:/config`, `${DATA_DIR}:/data`, `media-network`,
/// PUID/PGID/TZ from the environment. Bazarr gets a 768 MB cap —
/// upstream can balloon without one.
/// Just the service blocks (no header) — for combining with other overrides.
pub fn render_extras_services(extras: &Extras) -> String {
    let mut out = String::new();
    if extras.lidarr {
        out.push_str(
            "  lidarr:\n\
             \x20\x20\x20 image: lscr.io/linuxserver/lidarr:latest\n\
             \x20\x20\x20 container_name: lidarr\n\
             \x20\x20\x20 restart: unless-stopped\n\
             \x20\x20\x20 networks:\n\
             \x20\x20\x20\x20\x20 - media-network\n\
             \x20\x20\x20 ports:\n\
             \x20\x20\x20\x20\x20 - \"127.0.0.1:8686:8686\"\n\
             \x20\x20\x20 environment:\n\
             \x20\x20\x20\x20\x20 - PUID=${PUID:-1000}\n\
             \x20\x20\x20\x20\x20 - PGID=${PGID:-1000}\n\
             \x20\x20\x20\x20\x20 - TZ=${TZ:-UTC}\n\
             \x20\x20\x20 volumes:\n\
             \x20\x20\x20\x20\x20 - \"${CONFIG_DIR}/lidarr:/config\"\n\
             \x20\x20\x20\x20\x20 - \"${DATA_DIR}:/data\"\n",
        );
    }
    if extras.bazarr {
        out.push_str(
            "  bazarr:\n\
             \x20\x20\x20 image: lscr.io/linuxserver/bazarr:latest\n\
             \x20\x20\x20 container_name: bazarr\n\
             \x20\x20\x20 restart: unless-stopped\n\
             \x20\x20\x20 mem_limit: 768m\n\
             \x20\x20\x20 networks:\n\
             \x20\x20\x20\x20\x20 - media-network\n\
             \x20\x20\x20 ports:\n\
             \x20\x20\x20\x20\x20 - \"127.0.0.1:6767:6767\"\n\
             \x20\x20\x20 environment:\n\
             \x20\x20\x20\x20\x20 - PUID=${PUID:-1000}\n\
             \x20\x20\x20\x20\x20 - PGID=${PGID:-1000}\n\
             \x20\x20\x20\x20\x20 - TZ=${TZ:-UTC}\n\
             \x20\x20\x20 volumes:\n\
             \x20\x20\x20\x20\x20 - \"${CONFIG_DIR}/bazarr:/config\"\n\
             \x20\x20\x20\x20\x20 - \"${DATA_DIR}:/data\"\n",
        );
    }
    if extras.autobrr {
        out.push_str(
            "  autobrr:\n\
             \x20\x20\x20 image: ghcr.io/autobrr/autobrr:latest\n\
             \x20\x20\x20 container_name: autobrr\n\
             \x20\x20\x20 restart: unless-stopped\n\
             \x20\x20\x20 networks:\n\
             \x20\x20\x20\x20\x20 - media-network\n\
             \x20\x20\x20 ports:\n\
             \x20\x20\x20\x20\x20 - \"127.0.0.1:7474:7474\"\n\
             \x20\x20\x20 environment:\n\
             \x20\x20\x20\x20\x20 - PUID=${PUID:-1000}\n\
             \x20\x20\x20\x20\x20 - PGID=${PGID:-1000}\n\
             \x20\x20\x20\x20\x20 - TZ=${TZ:-UTC}\n\
             \x20\x20\x20 volumes:\n\
             \x20\x20\x20\x20\x20 - \"${CONFIG_DIR}/autobrr:/config\"\n",
        );
    }
    // ── Maintenance companions ──
    if extras.unpackerr {
        // No port, no UI — polls the *arrs via API. API keys are written
        // by Care (they don't exist until the *arrs first run).
        out.push_str(
            "  unpackerr:\n\
             \x20\x20\x20 image: golift/unpackerr:latest\n\
             \x20\x20\x20 container_name: unpackerr\n\
             \x20\x20\x20 restart: unless-stopped\n\
             \x20\x20\x20 networks:\n\
             \x20\x20\x20\x20\x20 - media-network\n\
             \x20\x20\x20 env_file:\n\
             \x20\x20\x20\x20\x20 - \"${CONFIG_DIR}/unpackerr/unpackerr.env\"\n\
             \x20\x20\x20 environment:\n\
             \x20\x20\x20\x20\x20 - PUID=${PUID:-1000}\n\
             \x20\x20\x20\x20\x20 - PGID=${PGID:-1000}\n\
             \x20\x20\x20\x20\x20 - TZ=${TZ:-UTC}\n\
             \x20\x20\x20\x20\x20 - UN_INTERVAL=2m\n\
             \x20\x20\x20 volumes:\n\
             \x20\x20\x20\x20\x20 - \"${CONFIG_DIR}/unpackerr:/config\"\n\
             \x20\x20\x20\x20\x20 - \"${DATA_DIR}:/data\"\n",
        );
    }
    if extras.cleanuparr {
        out.push_str(
            "  cleanuparr:\n\
             \x20\x20\x20 image: ghcr.io/cleanuparr/cleanuparr:latest\n\
             \x20\x20\x20 container_name: cleanuparr\n\
             \x20\x20\x20 restart: unless-stopped\n\
             \x20\x20\x20 networks:\n\
             \x20\x20\x20\x20\x20 - media-network\n\
             \x20\x20\x20 ports:\n\
             \x20\x20\x20\x20\x20 - \"127.0.0.1:11011:11011\"\n\
             \x20\x20\x20 environment:\n\
             \x20\x20\x20\x20\x20 - PUID=${PUID:-1000}\n\
             \x20\x20\x20\x20\x20 - PGID=${PGID:-1000}\n\
             \x20\x20\x20\x20\x20 - TZ=${TZ:-UTC}\n\
             \x20\x20\x20\x20\x20 - PORT=11011\n\
             \x20\x20\x20 volumes:\n\
             \x20\x20\x20\x20\x20 - \"${CONFIG_DIR}/cleanuparr:/config\"\n",
        );
    }
    if extras.maintainerr {
        out.push_str(
            "  maintainerr:\n\
             \x20\x20\x20 image: ghcr.io/maintainerr/maintainerr:latest\n\
             \x20\x20\x20 container_name: maintainerr\n\
             \x20\x20\x20 restart: unless-stopped\n\
             \x20\x20\x20 networks:\n\
             \x20\x20\x20\x20\x20 - media-network\n\
             \x20\x20\x20 ports:\n\
             \x20\x20\x20\x20\x20 - \"127.0.0.1:6246:6246\"\n\
             \x20\x20\x20 environment:\n\
             \x20\x20\x20\x20\x20 - PUID=${PUID:-1000}\n\
             \x20\x20\x20\x20\x20 - PGID=${PGID:-1000}\n\
             \x20\x20\x20\x20\x20 - TZ=${TZ:-UTC}\n\
             \x20\x20\x20 volumes:\n\
             \x20\x20\x20\x20\x20 - \"${CONFIG_DIR}/maintainerr:/opt/data\"\n",
        );
    }
    if extras.janitorr {
        out.push_str(
            "  janitorr:\n\
             \x20\x20\x20 image: ghcr.io/schaka/janitorr:latest\n\
             \x20\x20\x20 container_name: janitorr\n\
             \x20\x20\x20 restart: unless-stopped\n\
             \x20\x20\x20 networks:\n\
             \x20\x20\x20\x20\x20 - media-network\n\
             \x20\x20\x20 ports:\n\
             \x20\x20\x20\x20\x20 - \"127.0.0.1:8978:8978\"\n\
             \x20\x20\x20 environment:\n\
             \x20\x20\x20\x20\x20 - PUID=${PUID:-1000}\n\
             \x20\x20\x20\x20\x20 - PGID=${PGID:-1000}\n\
             \x20\x20\x20\x20\x20 - TZ=${TZ:-UTC}\n\
             \x20\x20\x20 volumes:\n\
             \x20\x20\x20\x20\x20 - \"${CONFIG_DIR}/janitorr:/config\"\n",
        );
    }
    // ── Stats (one per media server) ──
    if extras.tautulli {
        out.push_str(
            "  tautulli:\n\
             \x20\x20\x20 image: lscr.io/linuxserver/tautulli:latest\n\
             \x20\x20\x20 container_name: tautulli\n\
             \x20\x20\x20 restart: unless-stopped\n\
             \x20\x20\x20 networks:\n\
             \x20\x20\x20\x20\x20 - media-network\n\
             \x20\x20\x20 ports:\n\
             \x20\x20\x20\x20\x20 - \"127.0.0.1:8181:8181\"\n\
             \x20\x20\x20 environment:\n\
             \x20\x20\x20\x20\x20 - PUID=${PUID:-1000}\n\
             \x20\x20\x20\x20\x20 - PGID=${PGID:-1000}\n\
             \x20\x20\x20\x20\x20 - TZ=${TZ:-UTC}\n\
             \x20\x20\x20 volumes:\n\
             \x20\x20\x20\x20\x20 - \"${CONFIG_DIR}/tautulli:/config\"\n",
        );
    }
    if extras.jellystat {
        // Jellystat needs PostgreSQL — provisioned alongside.
        out.push_str(
            "  jellystat-db:\n\
             \x20\x20\x20 image: postgres:16-alpine\n\
             \x20\x20\x20 container_name: jellystat-db\n\
             \x20\x20\x20 restart: unless-stopped\n\
             \x20\x20\x20 networks:\n\
             \x20\x20\x20\x20\x20 - media-network\n\
             \x20\x20\x20 environment:\n\
             \x20\x20\x20\x20\x20 - POSTGRES_USER=jellystat\n\
             \x20\x20\x20\x20\x20 - POSTGRES_DB=jfstat\n\
             \x20\x20\x20\x20\x20 - POSTGRES_PASSWORD_FILE=/run/secrets/db_password\n\
             \x20\x20\x20 secrets:\n\
             \x20\x20\x20\x20\x20 - db_password\n\
             \x20\x20\x20 volumes:\n\
             \x20\x20\x20\x20\x20 - \"${CONFIG_DIR}/jellystat-db:/var/lib/postgresql/data\"\n",
        );
        out.push_str(
            "  jellystat:\n\
             \x20\x20\x20 image: cyfershepard/jellystat:latest\n\
             \x20\x20\x20 container_name: jellystat\n\
             \x20\x20\x20 restart: unless-stopped\n\
             \x20\x20\x20 networks:\n\
             \x20\x20\x20\x20\x20 - media-network\n\
             \x20\x20\x20 ports:\n\
             \x20\x20\x20\x20\x20 - \"127.0.0.1:3000:3000\"\n\
             \x20\x20\x20 depends_on:\n\
             \x20\x20\x20\x20\x20 jellystat-db:\n\
             \x20\x20\x20\x20\x20\x20\x20 condition: service_started\n\
             \x20\x20\x20 environment:\n\
             \x20\x20\x20\x20\x20 - PUID=${PUID:-1000}\n\
             \x20\x20\x20\x20\x20 - PGID=${PGID:-1000}\n\
             \x20\x20\x20\x20\x20 - TZ=${TZ:-UTC}\n\
             \x20\x20\x20\x20\x20 - POSTGRES_USER=jellystat\n\
             \x20\x20\x20\x20\x20 - POSTGRES_DB=jfstat\n\
             \x20\x20\x20\x20\x20 - POSTGRES_IP=jellystat-db\n\
             \x20\x20\x20\x20\x20 - POSTGRES_PORT=5432\n\
             \x20\x20\x20\x20\x20 - POSTGRES_PASSWORD_FILE=/run/secrets/db_password\n\
             \x20\x20\x20 secrets:\n\
             \x20\x20\x20\x20\x20 - db_password\n\
             \x20\x20\x20 volumes:\n\
             \x20\x20\x20\x20\x20 - \"${CONFIG_DIR}/jellystat:/app/backend/backup\"\n",
        );
    }
    // ── Sharing ──
    if extras.wizarr {
        out.push_str(
            "  wizarr:\n\
             \x20\x20\x20 image: ghcr.io/wizarrrr/wizarr:latest\n\
             \x20\x20\x20 container_name: wizarr\n\
             \x20\x20\x20 restart: unless-stopped\n\
             \x20\x20\x20 networks:\n\
             \x20\x20\x20\x20\x20 - media-network\n\
             \x20\x20\x20 ports:\n\
             \x20\x20\x20\x20\x20 - \"127.0.0.1:5690:5690\"\n\
             \x20\x20\x20 environment:\n\
             \x20\x20\x20\x20\x20 - PUID=${PUID:-1000}\n\
             \x20\x20\x20\x20\x20 - PGID=${PGID:-1000}\n\
             \x20\x20\x20\x20\x20 - TZ=${TZ:-UTC}\n\
             \x20\x20\x20 volumes:\n\
             \x20\x20\x20\x20\x20 - \"${CONFIG_DIR}/wizarr:/data\"\n",
        );
    }
    if extras.kometa {
        out.push_str(
            "  kometa:\n\
             \x20\x20\x20 image: ghcr.io/kometa-team/kometa:latest\n\
             \x20\x20\x20 container_name: kometa\n\
             \x20\x20\x20 restart: unless-stopped\n\
             \x20\x20\x20 networks:\n\
             \x20\x20\x20\x20\x20 - media-network\n\
             \x20\x20\x20 environment:\n\
             \x20\x20\x20\x20\x20 - PUID=${PUID:-1000}\n\
             \x20\x20\x20\x20\x20 - PGID=${PGID:-1000}\n\
             \x20\x20\x20\x20\x20 - TZ=${TZ:-UTC}\n\
             \x20\x20\x20\x20\x20 - KOMETA_RUN=true\n\
             \x20\x20\x20 volumes:\n\
             \x20\x20\x20\x20\x20 - \"${CONFIG_DIR}/kometa:/config\"\n",
        );
    }
    if extras.sportarr {
        out.push_str(
            "  sportarr:\n\
             \x20\x20\x20 image: sportarr/sportarr:latest\n\
             \x20\x20\x20 container_name: sportarr\n\
             \x20\x20\x20 restart: unless-stopped\n\
             \x20\x20\x20 networks:\n\
             \x20\x20\x20\x20\x20 - media-network\n\
             \x20\x20\x20 ports:\n\
             \x20\x20\x20\x20\x20 - \"127.0.0.1:1867:1867\"\n\
             \x20\x20\x20 environment:\n\
             \x20\x20\x20\x20\x20 - PUID=${PUID:-1000}\n\
             \x20\x20\x20\x20\x20 - PGID=${PGID:-1000}\n\
             \x20\x20\x20\x20\x20 - TZ=${TZ:-UTC}\n\
             \x20\x20\x20 volumes:\n\
             \x20\x20\x20\x20\x20 - \"${CONFIG_DIR}/sportarr:/config\"\n\
             \x20\x20\x20\x20\x20 - \"${DATA_DIR}:/data\"\n",
        );
    }
    out
}

/// Write (or remove) the override file to match the selection.
/// Removing all extras deletes the file — the base fleet is untouched.
/// Render the COMPLETE override file: extras + download clients (if local profile).
/// This is the single source of truth — callers must not append to the file.
pub fn render_full_override(
    extras: &Extras,
    fleet_profile: &crate::download::FleetProfile,
) -> String {
    let mut out = String::from(
        "# Generated by Porthole — do not edit by hand.\n\
         # Managed via Setup wizard and Care → Add or remove extra services.\n\
         # The installer's docker-compose.yml is never touched.\n\
         services:\n",
    );
    out.push_str(&render_extras_services(extras));
    if fleet_profile.needs_local_clients() {
        out.push_str(&crate::download::render_download_services());
    }
    out
}

/// Write the complete override (extras + download clients).
pub fn write_full_override(
    install_dir: &Path,
    extras: &Extras,
    fleet_profile: &crate::download::FleetProfile,
) -> Result<()> {
    let path = install_dir.join("docker-compose.override.yml");
    if extras.any() || fleet_profile.needs_local_clients() {
        std::fs::write(&path, render_full_override(extras, fleet_profile))
            .context("writing override file")?;
    } else if path.exists() {
        std::fs::remove_file(&path).context("removing override file")?;
    }
    Ok(())
}

/// Create the data dirs the extras need. Mirrors the installer's layout.
pub fn ensure_data_dirs(install_dir: &Path, extras: &Extras) -> Result<()> {
    let data = install_dir.join("data");
    let mut dirs = Vec::new();
    if extras.lidarr {
        dirs.push("media/music");
        dirs.push("downloads/lidarr");
    }
    if extras.sportarr {
        dirs.push("media/sports");
        dirs.push("downloads/sportarr");
    }
    for d in dirs {
        std::fs::create_dir_all(data.join(d)).context("creating data dir")?;
    }
    if extras.jellystat {
        // PostgreSQL password (Docker secret). Generated once, kept.
        let pw_path = install_dir.join("configs/jellystat-db/password.txt");
        if !pw_path.exists() {
            if let Some(p) = pw_path.parent() {
                std::fs::create_dir_all(p)?;
            }
            let mut bytes = [0u8; 24];
            getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("no entropy: {e}"))?;
            let pw: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            std::fs::write(&pw_path, pw)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&pw_path, std::fs::Permissions::from_mode(0o600));
            }
        }
    }
    Ok(())
}

/// Exact Bazarr connection values for the guided setup.
/// (Bazarr's settings API schema isn't stable enough to automate safely;
// Porthole shows these instead — same honest boundary as Jellyfin.
pub fn bazarr_manual_steps(sonarr_key: &str, radarr_key: &str) -> Vec<String> {
    vec![
        "Open Bazarr at http://localhost:6767.".to_string(),
        format!("Settings → Sonarr: Host `sonarr`, Port `8989`, API key `{sonarr_key}` → Test → Save."),
        format!("Settings → Radarr: Host `radarr`, Port `7878`, API key `{radarr_key}` → Test → Save."),
        "Settings → Languages: pick your subtitle languages and Save.".to_string(),
        "Settings → Providers: enable OpenSubtitles.com (free account) — Subscene is dead, ignore old guides.".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_only_has_selected_services() {
        let e = Extras {
            lidarr: true,
            bazarr: false,
            sportarr: true,
            autobrr: true,
            unpackerr: true,
            cleanuparr: true,
            maintainerr: true,
            janitorr: true,
            tautulli: true,
            jellystat: true,
            wizarr: true,
            kometa: true,
        };
        let yml = render_full_override(&e, &crate::download::FleetProfile::Debrid);
        assert!(yml.contains("lidarr:"));
        assert!(yml.contains("sportarr:"));
        assert!(!yml.contains("bazarr:"));
        assert!(yml.contains("127.0.0.1:8686:8686"));
        assert!(yml.contains("127.0.0.1:1867:1867"));
        assert!(yml.contains("media-network"));
        assert!(yml.contains("${CONFIG_DIR}/lidarr:/config"));
    }

    #[test]
    fn bazarr_gets_memory_cap() {
        let e = Extras {
            bazarr: true,
            ..Extras::default()
        };
        assert!(
            render_full_override(&e, &crate::download::FleetProfile::Debrid)
                .contains("mem_limit: 768m")
        );
    }

    #[test]
    fn write_and_remove_override() {
        let dir = std::env::temp_dir().join("porthole-extras-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let e = Extras {
            lidarr: true,
            ..Extras::default()
        };
        write_full_override(&dir, &e, &crate::download::FleetProfile::Debrid).unwrap();
        assert!(dir.join("docker-compose.override.yml").exists());
        write_full_override(
            &dir,
            &Extras::default(),
            &crate::download::FleetProfile::Debrid,
        )
        .unwrap();
        assert!(!dir.join("docker-compose.override.yml").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn data_dirs_created() {
        let dir = std::env::temp_dir().join("porthole-extras-dirs-test");
        let _ = std::fs::remove_dir_all(&dir);
        let e = Extras {
            lidarr: true,
            sportarr: true,
            ..Extras::default()
        };
        ensure_data_dirs(&dir, &e).unwrap();
        assert!(dir.join("data/media/music").exists());
        assert!(dir.join("data/downloads/lidarr").exists());
        assert!(dir.join("data/media/sports").exists());
        assert!(!dir.join("data/media/movies").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn full_override_is_single_valid_yaml() {
        // Extras + local profile must produce ONE services: block, not two.
        let e = Extras {
            lidarr: true,
            jellystat: true,
            ..Extras::default()
        };
        let yml = render_full_override(&e, &crate::download::FleetProfile::Local);
        // Exactly one top-level services: key.
        assert_eq!(
            yml.matches("\nservices:").count() + usize::from(yml.starts_with("services:")),
            1
        );
        assert!(yml.contains("lidarr:"));
        assert!(yml.contains("jellystat:"));
        assert!(yml.contains("gluetun:"));
        assert!(yml.contains("qbittorrent:"));

        // Debrid profile: no download clients.
        let yml2 = render_full_override(&e, &crate::download::FleetProfile::Debrid);
        assert!(yml2.contains("lidarr:"));
        assert!(!yml2.contains("gluetun:"));
    }
}
