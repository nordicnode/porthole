//! The setup wizard: each step mirrors a phase of the proven TorBox-Media-Server
//! installer, reframed in plain language around what gets *wired together*.
//!
//! Phase 0 (this skeleton): steps run in demo mode with simulated output so
//! the flow can be reviewed. Phase 1 will execute the real provisioning.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    Active,
    Done,
}

/// One wizard step.
pub struct WizardStep {
    /// Short title shown in the step list.
    pub title: &'static str,
    /// What happens, in words a non-expert understands.
    pub plain: &'static str,
    /// Which introductions this step performs (from the wiring map).
    pub wires_up: &'static str,
    /// Simulated log output for demo mode.
    pub demo_logs: &'static [&'static str],
}

pub static STEPS: &[WizardStep] = &[
    WizardStep {
        title: "Check the toolbox",
        plain: "Make sure Docker and the other tools Porthole needs are installed.",
        wires_up: "—",
        demo_logs: &[
            "[ok] docker 27.x found",
            "[ok] docker compose plugin found",
            "[ok] curl found",
        ],
    },
    WizardStep {
        title: "Check the doors",
        plain: "Make sure no other app is sitting on the ports your fleet needs.",
        wires_up: "—",
        demo_logs: &[
            "[ok] port 8282 free (Decypharr)",
            "[ok] port 8989 free (Sonarr)",
            "[ok] port 7878 free (Radarr)",
            "[ok] port 5055 free (Seerr)",
        ],
    },
    WizardStep {
        title: "Your preferences",
        plain: "Three questions: your TorBox key, where to keep things, Plex or Jellyfin.",
        wires_up: "—",
        demo_logs: &[
            "[in] TorBox API key accepted (hidden)",
            "[in] install dir: /opt/porthole",
            "[in] media server: Plex",
        ],
    },
    WizardStep {
        title: "Build the rooms",
        plain: "Create the folders and write each app's settings files for you.",
        wires_up: "—",
        demo_logs: &[
            "[ok] created /opt/porthole/config",
            "[ok] wrote decypharr config",
            "[ok] wrote sonarr / radarr / prowlarr configs",
            "[ok] wrote .env and docker-compose.yml",
        ],
    },
    WizardStep {
        title: "Launch the fleet",
        plain: "Start all nine services with Docker. This takes a minute on first run.",
        wires_up: "—",
        demo_logs: &[
            "[..] pulling images…",
            "[ok] 9 containers started",
            "[ok] all services responding",
        ],
    },
    WizardStep {
        title: "Introduce the crew",
        plain: "The important part: connect the apps to each other so they work as one.",
        wires_up: "Decypharr→Sonarr/Radarr · Prowlarr→Sonarr/Radarr · Byparr→Prowlarr",
        demo_logs: &[
            "[ok] Decypharr set as download client in Sonarr",
            "[ok] Decypharr set as download client in Radarr",
            "[ok] Prowlarr indexers shared with Sonarr + Radarr",
            "[ok] API keys synced between services",
        ],
    },
    WizardStep {
        title: "Set up your screen",
        plain: "Make your new libraries appear in Plex/Jellyfin and connect the request desk.",
        wires_up: "Sonarr/Radarr→Plex · Seerr→Sonarr/Radarr/Plex",
        demo_logs: &[
            "[ok] TV + Movies libraries created in Plex",
            "[ok] Seerr connected to Sonarr, Radarr and Plex",
            "[ok] default quality profiles applied",
        ],
    },
    WizardStep {
        title: "All done",
        plain: "Here's where everything lives. Request something in Seerr and watch it appear.",
        wires_up: "—",
        demo_logs: &[
            "[ok] Seerr:  http://localhost:5055",
            "[ok] Plex:   http://localhost:32400/web",
            "[ok] Sonarr: http://localhost:8989",
        ],
    },
];
