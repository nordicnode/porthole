<div align="center">

# ⛵ Porthole

**Your self-hosted media fleet, through one window.**

Automated setup *and* seamless integration for TorBox, Decypharr, Prowlarr,
Byparr, Sonarr, Radarr, Seerr, Plex and Jellyfin — with no expert knowledge
required.

[![CI](https://github.com/nordicnode/porthole/actions/workflows/ci.yml/badge.svg)](https://github.com/nordicnode/porthole/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

</div>

> Nine apps make a great media server — but only if they talk to each other.
> Porthole installs them, then **introduces them to each other**: search
> sources shared, downloads handed off, libraries created, requests connected.
> You answer three questions; Porthole does the hundred tiny configurations.

## The idea

The [TorBox-Media-Server](https://github.com/nordicnode/TorBox-Media-Server)
scripts proved the stack works. Porthole is the next step: a beautiful
terminal UI that turns that stack into a *product* — one that a non-expert
can set up and run.

The core insight: **setup is 20% of the value, integration is 80%.** Anyone
can `docker compose up` nine containers. The hard part is the wiring in
between — API keys synced, indexers shared, download clients assigned,
libraries created. Porthole does all of it, and shows you the wiring map so
you can see (and trust) what was connected.

## Screens

| View | What it does |
|------|--------------|
| **Fleet** (`1`) | Every service at a glance with **live Docker status** — `s`/`x`/`R` to start/stop/restart |
| **Setup** (`2`) | Guided wizard: 3 questions → dry-run plan → real provisioning with live logs |
| **Wiring** (`3`) | The integration map — every connection Porthole makes, in plain words |
| **Doctor** (`4`) | Fleet health checks in plain language, with one-key fixes |
| **Care** (`5`) | Backups, one-key updates with automatic rollback, config regeneration, clean uninstall |
| **Logs** (`6`) | Everything Porthole is doing, streamed live |
| **Help** (`?`) | The one-paragraph version of all of this |

## Quick start

```bash
cargo run --release
```

Keys: `Tab` switch views · `↑↓` move · `Enter` start setup · `?` help · `q` quit.

Works over SSH — it's a TUI, so it runs wherever your server lives.

## Project status

**Phase 3 — native provisioning + care** (current): Porthole generates all
config files itself (`.env`, Decypharr config, the three *arr configs, systemd
unit) — byte-identical to the installer's output, verified against the real
shell functions. The Care view handles backups, restore, one-key updates with
automatic rollback, native config regeneration, and clean uninstall.
See [ROADMAP.md](ROADMAP.md) for what's next.

## License

MIT — see [LICENSE](LICENSE).
