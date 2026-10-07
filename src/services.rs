//! The fleet: every service Porthole manages, described in plain language,
//! plus the wiring map — how Porthole introduces the services to each other
//! so the user never has to.

/// A single service in the fleet.
pub struct Service {
    /// Short id, e.g. "sonarr". Matches the docker compose service name;
    /// used by Phase 1 provisioning (see ROADMAP.md).
    #[allow(dead_code)]
    pub id: &'static str,
    /// Display name.
    pub name: &'static str,
    /// What it does, in words a non-expert understands.
    pub plain: &'static str,
    /// Default web UI port.
    pub port: u16,
    /// Group shown on the dashboard.
    pub group: &'static str,
}

/// One "introduction" Porthole performs: `from` wired into `to`.
pub struct Integration {
    pub from: &'static str,
    pub to: &'static str,
    /// What the user experiences, in plain language.
    pub plain: &'static str,
}

pub static SERVICES: &[Service] = &[
    Service {
        id: "torbox",
        name: "TorBox",
        plain:
            "Your cloud locker. Finds and stores your media in the cloud — nothing eats your disk.",
        port: 0, // no local UI; cloud service
        group: "Cloud",
    },
    Service {
        id: "decypharr",
        name: "Decypharr",
        plain: "The fetcher. Pulls finished downloads down from TorBox and hands them over.",
        port: 8282,
        group: "Downloads",
    },
    Service {
        id: "prowlarr",
        name: "Prowlarr",
        plain: "The scout. Knows where to search; shares its sources with the whole fleet.",
        port: 9696,
        group: "Search",
    },
    Service {
        id: "byparr",
        name: "Byparr",
        plain: "The helper. Makes tricky search sites load properly so Prowlarr can read them.",
        port: 8191,
        group: "Search",
    },
    Service {
        id: "sonarr",
        name: "Sonarr",
        plain: "Your TV butler. Follows your shows and grabs new episodes on its own.",
        port: 8989,
        group: "TV & Movies",
    },
    Service {
        id: "radarr",
        name: "Radarr",
        plain: "Your movie butler. Watches your movie list and grabs new releases on its own.",
        port: 7878,
        group: "TV & Movies",
    },
    Service {
        id: "seerr",
        name: "Seerr",
        plain: "The request desk. Search anything, hit request, and it starts downloading.",
        port: 5055,
        group: "Requests",
    },
    Service {
        id: "plex",
        name: "Plex",
        plain: "Your cinema. Beautiful apps on every device to watch it all.",
        port: 32400,
        group: "Watch",
    },
    Service {
        id: "jellyfin",
        name: "Jellyfin",
        plain: "Your cinema, the free and open way. Same job as Plex, no account needed.",
        port: 8096,
        group: "Watch",
    },
    Service {
        id: "lidarr",
        name: "Lidarr",
        plain: "Your music butler. Follows your artists and grabs new albums on its own.",
        port: 8686,
        group: "Extras",
    },
    Service {
        id: "bazarr",
        name: "Bazarr",
        plain: "The subtitle fairy. Fetches subtitles for everything automatically.",
        port: 6767,
        group: "Extras",
    },
    Service {
        id: "sportarr",
        name: "Sportarr",
        plain: "Your sports butler. Follows your teams and grabs the games. (Newer — opt-in.)",
        port: 1867,
        group: "Extras",
    },
];

/// The wiring map. This is Porthole's real product: every introduction below
/// is something the user would otherwise configure by hand across five
/// different settings pages with API keys.
pub static INTEGRATIONS: &[Integration] = &[
    Integration {
        from: "TorBox",
        to: "Decypharr",
        plain: "Your TorBox key is plugged in once — downloads flow from the cloud automatically.",
    },
    Integration {
        from: "Decypharr",
        to: "Sonarr",
        plain: "Finished episodes are handed to Sonarr by itself. You never touch a download folder.",
    },
    Integration {
        from: "Decypharr",
        to: "Radarr",
        plain: "Finished movies are handed to Radarr by itself. Same deal, no folders.",
    },
    Integration {
        from: "Prowlarr",
        to: "Sonarr",
        plain: "Search sources are configured once in Prowlarr, then shared with Sonarr. One place, not five.",
    },
    Integration {
        from: "Prowlarr",
        to: "Radarr",
        plain: "Same shared search sources for Radarr — configured once, used everywhere.",
    },
    Integration {
        from: "Byparr",
        to: "Prowlarr",
        plain: "Helps stubborn search sites load so Prowlarr can read them. Invisible to you.",
    },
    Integration {
        from: "Sonarr",
        to: "Plex",
        plain: "Your TV library appears in Plex on its own — no manual library setup.",
    },
    Integration {
        from: "Radarr",
        to: "Plex",
        plain: "Your movie library appears in Plex on its own too.",
    },
    Integration {
        from: "Sonarr",
        to: "Jellyfin",
        plain: "Same automatic libraries if you pick Jellyfin instead of Plex.",
    },
    Integration {
        from: "Radarr",
        to: "Jellyfin",
        plain: "Movies land in Jellyfin automatically as well.",
    },
    Integration {
        from: "Seerr",
        to: "Sonarr",
        plain: "Request a show in Seerr and Sonarr starts fetching it. No copy-pasting titles anywhere.",
    },
    Integration {
        from: "Seerr",
        to: "Radarr",
        plain: "Request a movie in Seerr and Radarr takes it from there.",
    },
    Integration {
        from: "Seerr",
        to: "Plex",
        plain: "What you request shows up where you watch. One login, one search box.",
    },
    Integration {
        from: "Configarr",
        to: "Sonarr",
        plain: "Expert quality profiles are synced in automatically — the right releases, every time, no settings maze.",
    },
    Integration {
        from: "Configarr",
        to: "Radarr",
        plain: "Same expert tuning for movies: quality, formats and naming, kept in sync.",
    },
    Integration {
        from: "Gluetun",
        to: "qBittorrent",
        plain: "Torrents run through your VPN — and if the VPN drops, the kill switch cuts everything. Nothing leaks.",
    },
    Integration {
        from: "Gluetun",
        to: "SABnzbd",
        plain: "Usenet downloads ride the same VPN tunnel, same kill-switch protection.",
    },
    Integration {
        from: "qBittorrent",
        to: "Sonarr",
        plain: "Finished episodes are handed to Sonarr by itself — it just appears in your library.",
    },
    Integration {
        from: "SABnzbd",
        to: "Sonarr",
        plain: "Usenet grabs flow straight into Sonarr, no manual importing.",
    },
    Integration {
        from: "autobrr",
        to: "qBittorrent",
        plain: "Catches new releases on private trackers the second they appear and pushes them to download.",
    },
    Integration {
        from: "Decypharr",
        to: "Lidarr",
        plain: "Finished albums are handed to Lidarr by itself — music included.",
    },
    Integration {
        from: "Decypharr",
        to: "Sportarr",
        plain: "Finished games are handed to Sportarr by itself.",
    },
    Integration {
        from: "Prowlarr",
        to: "Lidarr",
        plain: "Music search sources shared with Lidarr automatically.",
    },
    Integration {
        from: "Prowlarr",
        to: "Sportarr",
        plain: "Sports search sources shared with Sportarr automatically.",
    },
    Integration {
        from: "Sonarr",
        to: "Bazarr",
        plain: "Bazarr watches Sonarr's library and fetches subtitles for every episode.",
    },
    Integration {
        from: "Radarr",
        to: "Bazarr",
        plain: "Same for movies — subtitles appear next to the film, no hunting.",
    },
];
