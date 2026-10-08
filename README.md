<div align="center">

# ⛵ Shiphand

**Your self-hosted media fleet, through one window.**

Automated setup *and* seamless integration for your entire media fleet —
TorBox, Decypharr, Prowlarr, Byparr, Sonarr, Radarr, Lidarr, Bazarr, Sportarr,
Seerr, Plex, Jellyfin, download clients, VPN, cloud storage, and companion
automation — with no expert knowledge required.

[![CI](https://github.com/nordicnode/shiphand/actions/workflows/ci.yml/badge.svg)](https://github.com/nordicnode/shiphand/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/nordicnode/shiphand)](https://github.com/nordicnode/shiphand/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

</div>

> Dozens of apps make a great media server — but only if they talk to each other.
> Shiphand installs them, then **introduces them to each other**: search
> sources shared, downloads handed off, libraries created, requests connected,
> subtitles fetched, music organized, cloud storage mounted.
> You answer a few questions; Shiphand does the hundred tiny configurations.

## The idea

The [TorBox-Media-Server](https://github.com/nordicnode/TorBox-Media-Server)
scripts proved the stack works. Shiphand is the next step: a beautiful
terminal UI that turns that stack into a *product* — one that a non-expert
can set up and run.

The core insight: **setup is 20% of the value, integration is 80%.** Anyone
can `docker compose up` thirty containers. The hard part is the wiring in
between — API keys synced, indexers shared, download clients assigned,
libraries created, VPN routed, cloud storage mounted. Shiphand does all of it,
and shows you the wiring map so you can see (and trust) what was connected.

## Screens

| View | What it does |
|------|--------------|
| **Fleet** (`1`) | Every service at a glance with **live Docker status** — `s`/`x`/`R` to start/stop/restart |
| **Setup** (`2`) | Guided wizard: preferences → dry-run plan → real provisioning with live logs |
| **Wiring** (`3`) | The integration map — every connection Shiphand makes, in plain words |
| **Doctor** (`4`) | Fleet health checks in plain language, with one-key fixes |
| **Care** (`5`) | Backups, one-key updates with automatic rollback, config regeneration, clean uninstall, Shiphand self-updates |
| **Logs** (`6`) | Everything Shiphand is doing, streamed live |
| **Help** (`?`) | Keyboard shortcuts, plain-language privacy and storage explainers |

## Quick start

```bash
cargo run --release
```

Or grab the latest release (a single ~2MB binary, no dependencies):

```bash
curl -fsSL -o shiphand.tar.gz \
  https://github.com/nordicnode/shiphand/releases/latest/download/shiphand-x86_64-linux.tar.gz
tar xzf shiphand.tar.gz
./shiphand
```

Shiphand checks for its own updates once a day and can install them from
the Care view (`5` → *Check for Shiphand updates*) — checksum-verified, no
package manager needed.

Keys: `Tab` switch views · `↑↓` move · `Enter` start setup · `?` help · `q` quit.

Works over SSH — it's a TUI, so it runs wherever your server lives.

## Project status

**v0.2.0** — all ten build phases complete, plus a production-readiness audit:

- **Fleet**: live Docker status, start/stop/restart per service
- **Setup**: guided wizard (media server, debrid provider, download profile,
  extras) → dry-run plan → real provisioning with live logs
- **Wiring**: the integration map — every connection, in plain words
- **Doctor**: plain-language health checks with one-key fixes
- **Care**: timestamped backups (manual or daily scheduled), one-key updates
  with automatic rollback, config regeneration, small-disk mode, cloud
  storage setup, VPN setup, clean uninstall, Shiphand self-updates
  (checksum-verified)
- **Extras**: Lidarr, Bazarr, Sportarr, autobrr + 8 companion apps
  (Unpackerr, Cleanuparr, Maintainerr, Janitorr, Tautulli, Jellystat,
  Wizarr, Kometa)
- **Storage**: rclone cloud mounts with encryption, automatic uploads,
  hardlink verification
- **Privacy**: VPN-routed downloads (gluetun), debrid provider choice

## License

MIT — see [LICENSE](LICENSE).
