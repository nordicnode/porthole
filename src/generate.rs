//! Native config generation: Porthole writes the fleet's config files itself,
//! instead of shelling out for the deterministic parts.
//!
//! Every template here mirrors the TorBox-Media-Server installer's generators
//! byte-for-byte (see the `*_matches_installer` tests). If upstream changes a
//! template, update the template AND the test — never silently diverge.
//!
//! What this covers: `.env`, `decypharr/config.json`, the three *arr
//! `config.xml` files, and the systemd unit. The static `docker-compose.yml`
//! is copied from the installer checkout (it has no variables to fill).

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// Secrets
// ---------------------------------------------------------------------------

/// Random credentials, matching the installer's formats:
/// API keys are 32 lowercase hex chars; admin passwords 32 alphanumerics;
/// the Decypharr password 12 alphanumerics.
#[derive(Debug, Clone)]
pub struct Secrets {
    pub radarr_api_key: String,
    pub sonarr_api_key: String,
    pub prowlarr_api_key: String,
    pub radarr_admin_user: String,
    pub radarr_admin_pass: String,
    pub sonarr_admin_user: String,
    pub sonarr_admin_pass: String,
    pub prowlarr_admin_user: String,
    pub prowlarr_admin_pass: String,
    pub decypharr_user: String,
    pub decypharr_pass: String,
}

fn rand_hex_32() -> Result<String> {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).map_err(|e| anyhow::anyhow!("no entropy for API key: {e}"))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

fn rand_alnum(n: usize) -> Result<String> {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut buf = vec![0u8; n];
    getrandom::fill(&mut buf).map_err(|e| anyhow::anyhow!("no entropy for password: {e}"))?;
    Ok(buf
        .iter()
        .map(|b| CHARS[(b % 62) as usize] as char)
        .collect())
}

impl Secrets {
    /// Build from an existing .env file's values, generating fresh secrets
    /// only for fields that are missing or malformed. With an empty map this
    /// is a fully fresh set — the constructor for both cases.
    pub fn from_env_map(map: &HashMap<String, String>) -> Result<Self> {
        let hex_or_fresh = |key: &str| -> Result<String> {
            match map.get(key) {
                Some(v) if v.len() == 32 && v.chars().all(|c| c.is_ascii_hexdigit()) => {
                    Ok(v.to_lowercase())
                }
                _ => rand_hex_32(),
            }
        };
        let pass_or_fresh = |key: &str, n: usize| -> Result<String> {
            match map.get(key) {
                Some(v) if v.len() >= 8 => Ok(v.clone()),
                _ => rand_alnum(n),
            }
        };
        Ok(Self {
            radarr_api_key: hex_or_fresh("RADARR_API_KEY")?,
            sonarr_api_key: hex_or_fresh("SONARR_API_KEY")?,
            prowlarr_api_key: hex_or_fresh("PROWLARR_API_KEY")?,
            radarr_admin_user: map
                .get("RADARR_ADMIN_USER")
                .cloned()
                .unwrap_or_else(|| "admin".to_string()),
            radarr_admin_pass: pass_or_fresh("RADARR_ADMIN_PASS", 32)?,
            sonarr_admin_user: map
                .get("SONARR_ADMIN_USER")
                .cloned()
                .unwrap_or_else(|| "admin".to_string()),
            sonarr_admin_pass: pass_or_fresh("SONARR_ADMIN_PASS", 32)?,
            prowlarr_admin_user: map
                .get("PROWLARR_ADMIN_USER")
                .cloned()
                .unwrap_or_else(|| "admin".to_string()),
            prowlarr_admin_pass: pass_or_fresh("PROWLARR_ADMIN_PASS", 32)?,
            decypharr_user: map
                .get("DECYPHARR_USER")
                .cloned()
                .unwrap_or_else(|| "torbox".to_string()),
            decypharr_pass: pass_or_fresh("DECYPHARR_PASS", 12)?,
        })
    }

    #[cfg(test)]
    fn fixture() -> Self {
        Self {
            radarr_api_key: "a1b2c3d4e5f60718293a4b5c6d7e8f90".to_string(),
            sonarr_api_key: "0f9e8d7c6b5a4938271605f4e3d2c1b0".to_string(),
            prowlarr_api_key: "1234567890abcdef1234567890abcdef".to_string(),
            radarr_admin_user: "admin".to_string(),
            radarr_admin_pass: "RarrAdminPass00000000000000000001".to_string(),
            sonarr_admin_user: "admin".to_string(),
            sonarr_admin_pass: "SarrAdminPass00000000000000000001".to_string(),
            prowlarr_admin_user: "admin".to_string(),
            prowlarr_admin_pass: "ParrAdminPass00000000000000000001".to_string(),
            decypharr_user: "torbox".to_string(),
            decypharr_pass: "DecyPass1234".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// .env
// ---------------------------------------------------------------------------

pub struct EnvParams<'a> {
    pub puid: &'a str,
    pub pgid: &'a str,
    pub tz: &'a str,
    pub torbox_api_key: &'a str,
    pub config_dir: &'a str,
    pub data_dir: &'a str,
    pub mount_dir: &'a str,
    /// "plex" or "jellyfin"
    pub media_server: &'a str,
    pub plex_claim: &'a str,
    pub secrets: &'a Secrets,
    /// e.g. "Tue Oct  6 18:00:00 UTC 2026" — injectable so tests are stable.
    pub generated_on: &'a str,
}

/// Render the `.env` file. Mirrors `generate_env_file` in setup.sh.
pub fn render_env(p: &EnvParams) -> String {
    let s = p.secrets;
    format!(
        "# TorBox Media Server - Environment Configuration\n\
         # Generated on {date}\n\
         \n\
         # User/Group IDs (match your host user)\n\
         PUID=\"{puid}\"\n\
         PGID=\"{pgid}\"\n\
         \n\
         # Timezone\n\
         TZ=\"{tz}\"\n\
         \n\
         # TorBox\n\
         TORBOX_API_KEY=\"{tbkey}\"\n\
         \n\
         # Paths\n\
         CONFIG_DIR=\"{config_dir}\"\n\
         DATA_DIR=\"{data_dir}\"\n\
         MOUNT_DIR=\"{mount_dir}\"\n\
         \n\
         # Docker Compose Profile (activates only the selected media server)\n\
         COMPOSE_PROFILES=\"{media_server}\"\n\
         \n\
         # Plex\n\
         PLEX_CLAIM=\"{plex_claim}\"\n\
         \n\
         # *arr API Keys (pre-seeded)\n\
         RADARR_API_KEY=\"{radarr_key}\"\n\
         SONARR_API_KEY=\"{sonarr_key}\"\n\
         PROWLARR_API_KEY=\"{prowlarr_key}\"\n\
         \n\
         # Decypharr credentials (pre-seeded)\n\
         DECYPHARR_USER=\"{decypharr_user}\"\n\
         DECYPHARR_PASS=\"{decypharr_pass}\"\n\
         \n\
         # Admin Credentials\n\
         RADARR_ADMIN_USER=\"{radarr_admin_user}\"\n\
         RADARR_ADMIN_PASS=\"{radarr_admin_pass}\"\n\
         SONARR_ADMIN_USER=\"{sonarr_admin_user}\"\n\
         SONARR_ADMIN_PASS=\"{sonarr_admin_pass}\"\n\
         PROWLARR_ADMIN_USER=\"{prowlarr_admin_user}\"\n\
         PROWLARR_ADMIN_PASS=\"{prowlarr_admin_pass}\"\n",
        date = p.generated_on,
        puid = p.puid,
        pgid = p.pgid,
        tz = p.tz,
        tbkey = p.torbox_api_key,
        config_dir = p.config_dir,
        data_dir = p.data_dir,
        mount_dir = p.mount_dir,
        media_server = p.media_server,
        plex_claim = p.plex_claim,
        radarr_key = s.radarr_api_key,
        sonarr_key = s.sonarr_api_key,
        prowlarr_key = s.prowlarr_api_key,
        decypharr_user = s.decypharr_user,
        decypharr_pass = s.decypharr_pass,
        radarr_admin_user = s.radarr_admin_user,
        radarr_admin_pass = s.radarr_admin_pass,
        sonarr_admin_user = s.sonarr_admin_user,
        sonarr_admin_pass = s.sonarr_admin_pass,
        prowlarr_admin_user = s.prowlarr_admin_user,
        prowlarr_admin_pass = s.prowlarr_admin_pass,
    )
}

// ---------------------------------------------------------------------------
// Decypharr config.json
// ---------------------------------------------------------------------------

/// Render `decypharr/config.json`. Mirrors `generate_decypharr_config`
/// byte-for-byte. Assembled manually (not via to_string_pretty) because the
/// installer keeps short arrays inline; individual values are still
/// JSON-escaped via serde_json, so this is safer than the shell version.
pub fn render_decypharr_config(api_key: &str, username: &str, password: &str) -> String {
    let q = |s: &str| serde_json::to_string(s).expect("value is valid JSON string");
    format!(
        "{{\n\
         \x20 \"debrids\": [\n\
         \x20\x20\x20 {{\n\
         \x20\x20\x20\x20\x20 \"name\": \"torbox\",\n\
         \x20\x20\x20\x20\x20 \"api_key\": {api_key},\n\
         \x20\x20\x20\x20\x20 \"folder\": \"/mnt/remote/torbox/__all__\",\n\
         \x20\x20\x20\x20\x20 \"rate_limit\": \"55/hour\",\n\
         \x20\x20\x20\x20\x20 \"use_webdav\": true\n\
         \x20\x20\x20 }}\n\
         \x20 ],\n\
         \x20 \"rclone\": {{\n\
         \x20\x20\x20 \"enabled\": true,\n\
         \x20\x20\x20 \"mount_path\": \"/mnt/remote\"\n\
         \x20 }},\n\
         \x20 \"qbittorrent\": {{\n\
         \x20\x20\x20 \"download_folder\": \"/data/downloads/\",\n\
         \x20\x20\x20 \"categories\": [\"sonarr\", \"radarr\"]\n\
         \x20 }},\n\
         \x20 \"username\": {username},\n\
         \x20 \"password\": {password},\n\
         \x20 \"port\": \"8282\",\n\
         \x20 \"log_level\": \"info\"\n\
         }}\n",
        api_key = q(api_key),
        username = q(username),
        password = q(password),
    )
}

// ---------------------------------------------------------------------------
// *arr config.xml
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub enum ArrService {
    Radarr,
    Sonarr,
    Prowlarr,
}

impl ArrService {
    fn port(self) -> u16 {
        match self {
            ArrService::Radarr => 7878,
            ArrService::Sonarr => 8989,
            ArrService::Prowlarr => 9696,
        }
    }
    fn ssl_port(self) -> u16 {
        match self {
            ArrService::Radarr => 9898,
            ArrService::Sonarr => 9898,
            ArrService::Prowlarr => 6969,
        }
    }
    fn branch(self) -> &'static str {
        match self {
            ArrService::Radarr => "master",
            ArrService::Sonarr => "main",
            ArrService::Prowlarr => "develop",
        }
    }
    fn instance(self) -> &'static str {
        match self {
            ArrService::Radarr => "Radarr",
            ArrService::Sonarr => "Sonarr",
            ArrService::Prowlarr => "Prowlarr",
        }
    }
}

/// Render an *arr `config.xml`. Mirrors `generate_arr_configs`.
pub fn render_arr_config(service: ArrService, api_key: &str) -> String {
    format!(
        "<Config>\n\
         \x20 <LogLevel>info</LogLevel>\n\
         \x20 <EnableSsl>False</EnableSsl>\n\
         \x20 <Port>{port}</Port>\n\
         \x20 <SslPort>{ssl_port}</SslPort>\n\
         \x20 <UrlBase></UrlBase>\n\
         \x20 <BindAddress>*</BindAddress>\n\
         \x20 <ApiKey>{api_key}</ApiKey>\n\
         \x20 <AuthenticationMethod>Forms</AuthenticationMethod>\n\
         \x20 <AuthenticationRequired>DisabledForLocalAddresses</AuthenticationRequired>\n\
         \x20 <Branch>{branch}</Branch>\n\
         \x20 <InstanceName>{instance}</InstanceName>\n\
         </Config>\n",
        port = service.port(),
        ssl_port = service.ssl_port(),
        api_key = api_key,
        branch = service.branch(),
        instance = service.instance(),
    )
}

// ---------------------------------------------------------------------------
// systemd unit
// ---------------------------------------------------------------------------

/// Render the systemd unit. Mirrors `generate_systemd_service`.
/// NOTE: installing it still needs root — Porthole writes the file for
/// review and offers the install command rather than sudo-ing silently.
pub fn render_systemd_service(env_file: &str, install_dir: &str, docker_bin: &str) -> String {
    format!(
        "[Unit]\n\
         Description=TorBox Media Server - Mount Propagation & Services\n\
         After=local-fs.target network-online.target docker.service\n\
         Requires=docker.service\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         EnvironmentFile={env_file}\n\
         \n\
         # Step 1: Set up FUSE mount propagation (required for rclone WebDAV in Decypharr)\n\
         # Guard with findmnt to prevent mount stacking on repeated restarts\n\
         ExecStartPre=/bin/bash -c \"findmnt -n '$MOUNT_DIR' >/dev/null 2>&1 || mount --bind '$MOUNT_DIR' '$MOUNT_DIR'\"\n\
         ExecStartPre=/bin/bash -c \"mount --make-shared '$MOUNT_DIR'\"\n\
         \n\
         # Step 2: Start all containers (foreground so systemd tracks the process)\n\
         ExecStart={docker_bin} compose --env-file \"{env_file}\" up --remove-orphans\n\
         \n\
         # On stop: bring containers down gracefully\n\
         ExecStop={docker_bin} compose --env-file \"{env_file}\" stop\n\
         \n\
         # Clean up bind mount left by FUSE propagation\n\
         ExecStopPost=-/bin/bash -c \"umount -l '$MOUNT_DIR' || true\"\n\
         \n\
         Restart=on-failure\n\
         RestartSec=10\n\
         \n\
         WorkingDirectory=\"{install_dir}\"\n\
         TimeoutStartSec=120\n\
         TimeoutStopSec=60\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n",
    )
}

// ---------------------------------------------------------------------------
// Reading existing configs back
// ---------------------------------------------------------------------------

/// Parse a KEY="value" (or KEY=value) env file into a map. Comments and
/// blank lines are skipped. Used to preserve existing secrets when
/// rewriting configs.
pub fn read_env_file(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return map;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let v = v.trim().trim_matches('"').to_string();
            map.insert(k.trim().to_string(), v);
        }
    }
    map
}

// ---------------------------------------------------------------------------
// Write everything out
// ---------------------------------------------------------------------------

/// A file to write: relative path, content, and unix mode.
pub struct GeneratedFile {
    pub rel_path: String,
    pub content: String,
    pub mode: u32,
}

/// Everything the native generator needs. Bundled so the signature stays sane.
pub struct GenInputs<'a> {
    pub install_dir: &'a str,
    pub config_dir: &'a str,
    pub data_dir: &'a str,
    pub mount_dir: &'a str,
    pub tz: &'a str,
    pub puid: &'a str,
    pub pgid: &'a str,
    pub torbox_api_key: &'a str,
    pub media_server: &'a str,
    pub plex_claim: &'a str,
    pub secrets: &'a Secrets,
    pub generated_on: &'a str,
    pub docker_bin: &'a str,
}

/// Generate the full config set for an install dir. Returns (path, content,
/// mode) tuples; the caller writes them.
pub fn generate_all(inp: &GenInputs) -> Vec<GeneratedFile> {
    let env = render_env(&EnvParams {
        puid: inp.puid,
        pgid: inp.pgid,
        tz: inp.tz,
        torbox_api_key: inp.torbox_api_key,
        config_dir: inp.config_dir,
        data_dir: inp.data_dir,
        mount_dir: inp.mount_dir,
        media_server: inp.media_server,
        plex_claim: inp.plex_claim,
        secrets: inp.secrets,
        generated_on: inp.generated_on,
    });
    vec![
        GeneratedFile {
            rel_path: ".env".to_string(),
            content: env,
            mode: 0o600,
        },
        GeneratedFile {
            rel_path: "configs/decypharr/config.json".to_string(),
            content: render_decypharr_config(
                inp.torbox_api_key,
                &inp.secrets.decypharr_user,
                &inp.secrets.decypharr_pass,
            ),
            mode: 0o600,
        },
        GeneratedFile {
            rel_path: "configs/radarr/config.xml".to_string(),
            content: render_arr_config(ArrService::Radarr, &inp.secrets.radarr_api_key),
            mode: 0o600,
        },
        GeneratedFile {
            rel_path: "configs/sonarr/config.xml".to_string(),
            content: render_arr_config(ArrService::Sonarr, &inp.secrets.sonarr_api_key),
            mode: 0o600,
        },
        GeneratedFile {
            rel_path: "configs/prowlarr/config.xml".to_string(),
            content: render_arr_config(ArrService::Prowlarr, &inp.secrets.prowlarr_api_key),
            mode: 0o600,
        },
        GeneratedFile {
            rel_path: "torbox-media-server.service".to_string(),
            content: render_systemd_service(
                &format!("{}/.env", inp.install_dir),
                inp.install_dir,
                inp.docker_bin,
            ),
            mode: 0o644,
        },
    ]
}

// ---------------------------------------------------------------------------
// Tests: byte-identical with the installer (modulo the date line)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const DATE: &str = "Tue Oct  6 18:00:00 UTC 2026";

    fn params<'a>(secrets: &'a Secrets) -> EnvParams<'a> {
        EnvParams {
            puid: "1000",
            pgid: "1000",
            tz: "UTC",
            torbox_api_key: "tbk-0123456789abcdef",
            config_dir: "/home/u/porthole-stack/configs",
            data_dir: "/home/u/porthole-stack/data",
            mount_dir: "/mnt/torbox-media",
            media_server: "plex",
            plex_claim: "",
            secrets,
            generated_on: DATE,
        }
    }

    #[test]
    fn secrets_have_the_right_shape() {
        use std::collections::HashMap;
        let s = Secrets::from_env_map(&HashMap::new()).unwrap();
        for k in [&s.radarr_api_key, &s.sonarr_api_key, &s.prowlarr_api_key] {
            assert_eq!(k.len(), 32);
            assert!(k
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        }
        assert_eq!(s.decypharr_pass.len(), 12);
        assert_eq!(s.radarr_admin_user, "admin");
        // Uniqueness: two generations must differ.
        let t = Secrets::from_env_map(&HashMap::new()).unwrap();
        assert_ne!(s.radarr_api_key, t.radarr_api_key);
    }

    #[test]
    fn secrets_preserved_from_env() {
        use std::collections::HashMap;
        let mut map = HashMap::new();
        map.insert(
            "RADARR_API_KEY".to_string(),
            "a1b2c3d4e5f60718293a4b5c6d7e8f90".to_string(),
        );
        map.insert("RADARR_ADMIN_USER".to_string(), "boss".to_string());
        let s = Secrets::from_env_map(&map).unwrap();
        // Present + valid → preserved as-is.
        assert_eq!(s.radarr_api_key, "a1b2c3d4e5f60718293a4b5c6d7e8f90");
        assert_eq!(s.radarr_admin_user, "boss");
        // Missing → freshly generated with the right shape.
        assert_eq!(s.sonarr_api_key.len(), 32);
        assert_ne!(s.sonarr_api_key, s.radarr_api_key);
    }

    #[test]
    fn env_matches_installer() {
        let s = Secrets::fixture();
        let got = render_env(&params(&s));
        let want = "# TorBox Media Server - Environment Configuration\n\
            # Generated on Tue Oct  6 18:00:00 UTC 2026\n\
            \n\
            # User/Group IDs (match your host user)\n\
            PUID=\"1000\"\n\
            PGID=\"1000\"\n\
            \n\
            # Timezone\n\
            TZ=\"UTC\"\n\
            \n\
            # TorBox\n\
            TORBOX_API_KEY=\"tbk-0123456789abcdef\"\n\
            \n\
            # Paths\n\
            CONFIG_DIR=\"/home/u/porthole-stack/configs\"\n\
            DATA_DIR=\"/home/u/porthole-stack/data\"\n\
            MOUNT_DIR=\"/mnt/torbox-media\"\n\
            \n\
            # Docker Compose Profile (activates only the selected media server)\n\
            COMPOSE_PROFILES=\"plex\"\n\
            \n\
            # Plex\n\
            PLEX_CLAIM=\"\"\n\
            \n\
            # *arr API Keys (pre-seeded)\n\
            RADARR_API_KEY=\"a1b2c3d4e5f60718293a4b5c6d7e8f90\"\n\
            SONARR_API_KEY=\"0f9e8d7c6b5a4938271605f4e3d2c1b0\"\n\
            PROWLARR_API_KEY=\"1234567890abcdef1234567890abcdef\"\n\
            \n\
            # Decypharr credentials (pre-seeded)\n\
            DECYPHARR_USER=\"torbox\"\n\
            DECYPHARR_PASS=\"DecyPass1234\"\n\
            \n\
            # Admin Credentials\n\
            RADARR_ADMIN_USER=\"admin\"\n\
            RADARR_ADMIN_PASS=\"RarrAdminPass00000000000000000001\"\n\
            SONARR_ADMIN_USER=\"admin\"\n\
            SONARR_ADMIN_PASS=\"SarrAdminPass00000000000000000001\"\n\
            PROWLARR_ADMIN_USER=\"admin\"\n\
            PROWLARR_ADMIN_PASS=\"ParrAdminPass00000000000000000001\"\n";
        assert_eq!(got, want);
    }

    #[test]
    fn decypharr_config_matches_installer() {
        let got = render_decypharr_config("tbk-0123456789abcdef", "torbox", "DecyPass1234");
        let want = "{\n\
            \x20 \"debrids\": [\n\
            \x20\x20\x20 {\n\
            \x20\x20\x20\x20\x20 \"name\": \"torbox\",\n\
            \x20\x20\x20\x20\x20 \"api_key\": \"tbk-0123456789abcdef\",\n\
            \x20\x20\x20\x20\x20 \"folder\": \"/mnt/remote/torbox/__all__\",\n\
            \x20\x20\x20\x20\x20 \"rate_limit\": \"55/hour\",\n\
            \x20\x20\x20\x20\x20 \"use_webdav\": true\n\
            \x20\x20\x20 }\n\
            \x20 ],\n\
            \x20 \"rclone\": {\n\
            \x20\x20\x20 \"enabled\": true,\n\
            \x20\x20\x20 \"mount_path\": \"/mnt/remote\"\n\
            \x20 },\n\
            \x20 \"qbittorrent\": {\n\
            \x20\x20\x20 \"download_folder\": \"/data/downloads/\",\n\
            \x20\x20\x20 \"categories\": [\"sonarr\", \"radarr\"]\n\
            \x20 },\n\
            \x20 \"username\": \"torbox\",\n\
            \x20 \"password\": \"DecyPass1234\",\n\
            \x20 \"port\": \"8282\",\n\
            \x20 \"log_level\": \"info\"\n\
            }\n";
        assert_eq!(got, want);
    }

    #[test]
    fn arr_configs_match_installer() {
        let radarr = render_arr_config(ArrService::Radarr, "a1b2c3d4e5f60718293a4b5c6d7e8f90");
        assert!(radarr.contains("<Port>7878</Port>"));
        assert!(radarr.contains("<Branch>master</Branch>"));
        assert!(radarr.contains("<InstanceName>Radarr</InstanceName>"));
        assert!(radarr.contains("<ApiKey>a1b2c3d4e5f60718293a4b5c6d7e8f90</ApiKey>"));
        assert!(radarr.contains(
            "<AuthenticationRequired>DisabledForLocalAddresses</AuthenticationRequired>"
        ));

        let sonarr = render_arr_config(ArrService::Sonarr, "0f9e8d7c6b5a4938271605f4e3d2c1b0");
        assert!(sonarr.contains("<Port>8989</Port>"));
        assert!(sonarr.contains("<Branch>main</Branch>"));

        let prowlarr = render_arr_config(ArrService::Prowlarr, "1234567890abcdef1234567890abcdef");
        assert!(prowlarr.contains("<Port>9696</Port>"));
        assert!(prowlarr.contains("<SslPort>6969</SslPort>"));
        assert!(prowlarr.contains("<Branch>develop</Branch>"));
    }

    #[test]
    fn systemd_unit_matches_installer() {
        let got = render_systemd_service(
            "/home/u/porthole-stack/.env",
            "/home/u/porthole-stack",
            "/usr/bin/docker",
        );
        assert!(got.contains("[Unit]"));
        assert!(got.contains("EnvironmentFile=/home/u/porthole-stack/.env"));
        assert!(got.contains("findmnt -n '$MOUNT_DIR'"));
        assert!(got.contains("WantedBy=multi-user.target"));
        assert!(got.contains("WorkingDirectory=\"/home/u/porthole-stack\""));
    }
}
