# Porthole roadmap

The guiding principle: **Porthole is the glue, not the installer.** Every
phase is judged by one question — *does the user need to know less than
before?*

## Phase 0 — Foundation ✅ (current)

- [x] TUI shell (ratatui): Fleet, Setup, Wiring, Logs, Help views
- [x] Fleet definitions: 9 services, plain-language descriptions, default ports
- [x] The wiring map: 13 integrations as first-class data (`services.rs`)
- [x] Guided setup flow: 8 steps mirroring the proven TorBox installer phases,
      each step naming what it wires together
- [x] Demo-mode runner so the flow can be reviewed end to end
- [x] CI: fmt, clippy, build, test

## Phase 1 — Real provisioning (next)

Orchestrate, don't rewrite. The TorBox-Media-Server scripts are battle-tested
(55 stars, real users); Phase 1 shells out to them with structured progress.

- [ ] Detect real state: `docker compose ps` → Fleet view shows
      running/stopped/failed per service (replaces "○ not set up")
- [ ] Step runner executes real phases: dependency checks, port checks,
      config generation, `docker compose up`, then the integration phase
      (arr API wiring, indexer sharing, auth sync, library creation)
- [ ] Stream real logs into the Logs view with per-step success/failure
- [ ] Guided preferences: TorBox API key (masked input), install dir,
      Plex vs Jellyfin, PUID/PGID — validated before anything runs
- [ ] Dry-run mode: show the full plan (every file, every API call) before
      executing — trust through transparency

## Phase 2 — Native provisioning

- [ ] Port config generation to Rust (compose file, `.env`, arr configs)
      so Porthole no longer shells out for the deterministic parts
- [ ] Health checks per service with plain-language diagnosis
      ("Sonarr can't reach Prowlarr — here's the one button that fixes it")
- [ ] One-key repair for the common breakages (expired API keys, moved paths)

## Phase 3 — Fleet management

- [ ] Start/stop/restart/update per service or whole fleet
- [ ] Update detection: new image versions with one-key safe updates + rollback
- [ ] Backup/restore of the whole configuration
- [ ] Uninstall that cleanly removes everything (mirrors `uninstall.sh`)

## Non-goals

- A GUI. The TUI works over SSH, which is where servers live. A web dashboard
  is a maybe-someday, not a plan.
- Supporting every *arr app ever. The curated nine are the product; breadth
  comes after depth.
- AI features. This project is deliberately non-AI: deterministic,
  inspectable, offline-capable.
