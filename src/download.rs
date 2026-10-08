//! Download choice: which debrid, and how private.
//!
//! Two questions drive everything:
//! - Fleet profile: Debrid (default) vs Self-downloaded (VPN-routed)
//!   vs Hybrid.
//! - Debrid provider: Premiumize / AllDebrid / TorBox / Debrid-Link /
//!   Real-Debrid — all via Decypharr's `debrids[]`, so swapping is a
//!   config change, not a rebuild.

/// How downloads reach the disk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FleetProfile {
    /// Everything through Decypharr + the chosen debrid. Default.
    #[default]
    Debrid,
    /// Usenet + torrents via real clients, VPN-routed through gluetun.
    Local,
    /// Both: debrid for most, local clients available too.
    Hybrid,
}

impl FleetProfile {
    pub fn label(&self) -> &'static str {
        match self {
            FleetProfile::Debrid => "Debrid",
            FleetProfile::Local => "Self-downloaded",
            FleetProfile::Hybrid => "Hybrid",
        }
    }

    pub fn cycle(&self) -> Self {
        match self {
            FleetProfile::Debrid => FleetProfile::Local,
            FleetProfile::Local => FleetProfile::Hybrid,
            FleetProfile::Hybrid => FleetProfile::Debrid,
        }
    }

    /// Does this profile need the local download clients?
    pub fn needs_local_clients(&self) -> bool {
        matches!(self, FleetProfile::Local | FleetProfile::Hybrid)
    }

    /// Does this profile need Decypharr/debrid?
    pub fn needs_debrid(&self) -> bool {
        matches!(self, FleetProfile::Debrid | FleetProfile::Hybrid)
    }
}

/// Read the fleet profile from <install>/.shiphand-profile (line 1).
/// Defaults to Debrid if missing/unreadable.
pub fn read_fleet_profile(install_dir: &std::path::Path) -> FleetProfile {
    let path = install_dir.join(".shiphand-profile");
    if let Ok(content) = std::fs::read_to_string(&path) {
        match content.lines().next().unwrap_or("").trim() {
            "local" => FleetProfile::Local,
            "hybrid" => FleetProfile::Hybrid,
            _ => FleetProfile::Debrid,
        }
    } else {
        FleetProfile::Debrid
    }
}

/// The five debrid providers Decypharr supports, ranked honestly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DebridProvider {
    /// Bundles debrid + cloud + Usenet + VPN. Watch point fair-use.
    Premiumize,
    /// Budget pick. API caps: 12 req/s, 600/min per key.
    AllDebrid,
    /// Current default. July 2026 TOS overhaul disclosed in wizard.
    #[default]
    TorBox,
    /// Built-in seeding.
    DebridLink,
    /// Fallback only: keyword filter broke 50-70% of cached 4K.
    RealDebrid,
}

impl DebridProvider {
    /// Decypharr's `provider` identifier (verified Oct 2026).
    pub fn decypharr_id(&self) -> &'static str {
        match self {
            DebridProvider::Premiumize => "premiumize",
            DebridProvider::AllDebrid => "alldebrid",
            DebridProvider::TorBox => "torbox",
            DebridProvider::DebridLink => "debridlink",
            DebridProvider::RealDebrid => "realdebrid",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            DebridProvider::Premiumize => "Premiumize",
            DebridProvider::AllDebrid => "AllDebrid",
            DebridProvider::TorBox => "TorBox",
            DebridProvider::DebridLink => "Debrid-Link",
            DebridProvider::RealDebrid => "Real-Debrid",
        }
    }

    /// One honest line per provider, shown in the wizard.
    pub fn blurb(&self) -> &'static str {
        match self {
            DebridProvider::Premiumize => "Recommended: debrid + cloud + Usenet + VPN in one.",
            DebridProvider::AllDebrid => "Budget pick. Works fine, watch the API rate caps.",
            DebridProvider::TorBox => "The default so far. (Their 2026 TOS got worse — your call.)",
            DebridProvider::DebridLink => "Seeds for you — good if you care about ratios.",
            DebridProvider::RealDebrid => "Fallback only: their filter breaks much cached 4K.",
        }
    }

    pub fn cycle(&self) -> Self {
        match self {
            DebridProvider::Premiumize => DebridProvider::AllDebrid,
            DebridProvider::AllDebrid => DebridProvider::TorBox,
            DebridProvider::TorBox => DebridProvider::DebridLink,
            DebridProvider::DebridLink => DebridProvider::RealDebrid,
            DebridProvider::RealDebrid => DebridProvider::Premiumize,
        }
    }
}

/// Render the `debrids[]` JSON array for Decypharr's config.json.
/// Keeps the installer's field shape (`name` = provider id).
pub fn render_debrids(provider: &DebridProvider, api_key: &str) -> String {
    format!(
        r#"[{{"name":"{id}","api_key":"{key}","folder":"/mnt/remote/{id}/__all__","rate_limit":"55/hour","use_webdav":true}}]"#,
        id = provider.decypharr_id(),
        key = api_key.replace('"', "\\\""),
    )
}

/// Render the download-clients override (gluetun + qBittorrent + SABnzbd).
///
/// The downloaders ride `network_mode: service:gluetun` — gluetun's
/// built-in firewall is the kill switch (on by default). Plex/Jellyfin/
/// Seerr stay OFF the VPN (remote access dies behind it).
/// VPN credentials are added later via Care (the user must bring them).
/// Just the service blocks (no header) — for combining with other overrides.
pub fn render_download_services() -> String {
    String::from(
        "\x20 gluetun:\n\
         \x20\x20\x20 image: qmcgaw/gluetun:latest\n\
         \x20\x20\x20 container_name: gluetun\n\
         \x20\x20\x20 restart: unless-stopped\n\
         \x20\x20\x20 cap_add:\n\
         \x20\x20\x20\x20\x20 - NET_ADMIN\n\
         \x20\x20\x20 devices:\n\
         \x20\x20\x20\x20\x20 - /dev/net/tun:/dev/net/tun\n\
         \x20\x20\x20 networks:\n\
         \x20\x20\x20\x20\x20 - media-network\n\
         \x20\x20\x20 ports:\n\
         \x20\x20\x20\x20\x20 - \"127.0.0.1:8080:8080\" # qBittorrent WebUI\n\
         \x20\x20\x20\x20\x20 - \"127.0.0.1:8081:8081\" # SABnzbd\n\
         \x20\x20\x20 environment:\n\
         \x20\x20\x20\x20\x20 - PUID=${PUID:-1000}\n\
         \x20\x20\x20\x20\x20 - PGID=${PGID:-1000}\n\
         \x20\x20\x20\x20\x20 - TZ=${TZ:-UTC}\n\
         \x20\x20\x20\x20\x20 # ── Set these in Care → Set up VPN for downloads ──\n\
         \x20\x20\x20\x20\x20 - VPN_SERVICE_PROVIDER=\n\
         \x20\x20\x20\x20\x20 - VPN_TYPE=wireguard\n\
         \x20\x20\x20\x20\x20 - WIREGUARD_PRIVATE_KEY=\n\
         \x20\x20\x20\x20\x20 # LAN access so the *arrs can reach the clients:\n\
         \x20\x20\x20\x20\x20 - FIREWALL_OUTBOUND_SUBNETS=192.168.0.0/16,172.16.0.0/12\n\
         \x20\x20\x20 volumes:\n\
         \x20\x20\x20\x20\x20 - \"${CONFIG_DIR}/gluetun:/gluetun\"\n\
         \n\
         \x20 qbittorrent:\n\
         \x20\x20\x20 image: lscr.io/linuxserver/qbittorrent:latest\n\
         \x20\x20\x20 container_name: qbittorrent\n\
         \x20\x20\x20 restart: unless-stopped\n\
         \x20\x20\x20 network_mode: \"service:gluetun\"\n\
         \x20\x20\x20 depends_on:\n\
         \x20\x20\x20\x20\x20 gluetun:\n\
         \x20\x20\x20\x20\x20\x20\x20 condition: service_started\n\
         \x20\x20\x20 environment:\n\
         \x20\x20\x20\x20\x20 - PUID=${PUID:-1000}\n\
         \x20\x20\x20\x20\x20 - PGID=${PGID:-1000}\n\
         \x20\x20\x20\x20\x20 - TZ=${TZ:-UTC}\n\
         \x20\x20\x20 volumes:\n\
         \x20\x20\x20\x20\x20 - \"${CONFIG_DIR}/qbittorrent:/config\"\n\
         \x20\x20\x20\x20\x20 - \"${DATA_DIR}:/data\"\n\
         \n\
         \x20 sabnzbd:\n\
         \x20\x20\x20 image: lscr.io/linuxserver/sabnzbd:latest\n\
         \x20\x20\x20 container_name: sabnzbd\n\
         \x20\x20\x20 restart: unless-stopped\n\
         \x20\x20\x20 network_mode: \"service:gluetun\"\n\
         \x20\x20\x20 depends_on:\n\
         \x20\x20\x20\x20\x20 gluetun:\n\
         \x20\x20\x20\x20\x20\x20\x20 condition: service_started\n\
         \x20\x20\x20 environment:\n\
         \x20\x20\x20\x20\x20 - PUID=${PUID:-1000}\n\
         \x20\x20\x20\x20\x20 - PGID=${PGID:-1000}\n\
         \x20\x20\x20\x20\x20 - TZ=${TZ:-UTC}\n\
         \x20\x20\x20 volumes:\n\
         \x20\x20\x20\x20\x20 - \"${CONFIG_DIR}/sabnzbd:/config\"\n\
         \x20\x20\x20\x20\x20 - \"${DATA_DIR}:/data\"\n",
    )
}

/// Point Decypharr at the chosen debrid provider.
/// Rewrites the `debrids[]` array in `configs/decypharr/config.json`,
/// preserving everything else. The installer always writes TorBox;
/// this corrects it when the user picked differently.
pub fn set_debrid_provider(
    install_dir: &std::path::Path,
    provider: &DebridProvider,
    api_key: &str,
) -> anyhow::Result<()> {
    let path = install_dir.join("configs/decypharr/config.json");
    let raw = std::fs::read_to_string(&path)
        .map_err(|_| anyhow::anyhow!("no Decypharr config yet — is it installed?"))?;
    let mut cfg: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|_| anyhow::anyhow!("could not parse Decypharr config"))?;
    let entry: serde_json::Value = serde_json::from_str(&render_debrids(provider, api_key))
        .map_err(|_| anyhow::anyhow!("internal error: generated invalid JSON"))?;
    // render_debrids returns an array; take its single entry.
    let entry = entry
        .as_array()
        .and_then(|a| a.first().cloned())
        .ok_or_else(|| anyhow::anyhow!("internal error: empty debrids array"))?;
    cfg["debrids"] = serde_json::Value::Array(vec![entry]);
    let out = serde_json::to_string_pretty(&cfg)
        .map_err(|_| anyhow::anyhow!("could not serialize Decypharr config"))?;
    std::fs::write(&path, out)?;
    Ok(())
}

/// Plain-language privacy explainer for the Help view.
pub fn privacy_explainer() -> Vec<String> {
    vec![
        "What your internet provider can see — honestly:".to_string(),
        "".to_string(),
        "With debrid (the default): your provider sees encrypted connections".to_string(),
        "to the debrid service — not what you download. No torrent swarm,".to_string(),
        "no harvestable IP addresses. It can see timing and volume, but".to_string(),
        "not content. This is already private enough for most people.".to_string(),
        "".to_string(),
        "With self-downloaded + VPN: the VPN hides even which services".to_string(),
        "you use. Your provider sees only encrypted VPN traffic.".to_string(),
        "The kill switch (built into gluetun) cuts everything if the".to_string(),
        "VPN drops — nothing leaks onto your bare connection.".to_string(),
        "".to_string(),
        "What Shiphand does NOT do:".to_string(),
        "• No WARP toggle — Cloudflare would see everything instead of".to_string(),
        "  your ISP. That's not privacy, that's a change of watcher.".to_string(),
        "• Plex/Jellyfin stay off the VPN — remote streaming breaks".to_string(),
        "  behind it, and they don't need hiding.".to_string(),
        "".to_string(),
        "Heavy seeder? A seedbox (a rented server that seeds for you)".to_string(),
        "is the alternative — but if you already pay for debrid, it's".to_string(),
        "mostly redundant.".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_ids_match_decypharr() {
        assert_eq!(DebridProvider::Premiumize.decypharr_id(), "premiumize");
        assert_eq!(DebridProvider::AllDebrid.decypharr_id(), "alldebrid");
        assert_eq!(DebridProvider::TorBox.decypharr_id(), "torbox");
        assert_eq!(DebridProvider::DebridLink.decypharr_id(), "debridlink");
        assert_eq!(DebridProvider::RealDebrid.decypharr_id(), "realdebrid");
    }

    #[test]
    fn debrids_json_shape() {
        let j = render_debrids(&DebridProvider::Premiumize, "key123");
        assert!(j.contains(r#""name":"premiumize""#));
        assert!(j.contains(r#""api_key":"key123""#));
        assert!(j.starts_with('[') && j.ends_with(']'));
    }

    #[test]
    fn download_override_has_kill_switch_basics() {
        let yml = format!("services:\n{}", render_download_services());
        assert!(yml.contains("gluetun:"));
        assert!(yml.contains("qbittorrent:"));
        assert!(yml.contains("sabnzbd:"));
        assert!(yml.contains("network_mode: \"service:gluetun\""));
        assert!(yml.contains("FIREWALL_OUTBOUND_SUBNETS"));
        // Media servers must NOT be here.
        assert!(!yml.contains("plex:"));
        assert!(!yml.contains("jellyfin:"));
    }

    #[test]
    fn profile_helpers() {
        assert!(FleetProfile::Debrid.needs_debrid());
        assert!(!FleetProfile::Debrid.needs_local_clients());
        assert!(FleetProfile::Local.needs_local_clients());
        assert!(FleetProfile::Hybrid.needs_debrid() && FleetProfile::Hybrid.needs_local_clients());
    }
}
