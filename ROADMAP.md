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

## Phase 1 — Real provisioning ✅ (current)

Orchestrate, don't rewrite. The TorBox-Media-Server scripts are battle-tested
(55 stars, real users); Porthole shells out to them with structured progress.

- [x] Detect real state: `docker ps -a` → Fleet view shows
      running/stopped/failed/not-installed per service (`r` to refresh)
- [x] Real preflight checks: tool presence (`docker`, `git`, `curl`,
      compose plugin) and real port-conflict checks via bind tests
- [x] Guided preferences: TorBox API key (masked input + log redaction),
      install dir, Plex vs Jellyfin, PUID/PGID, timezone — validated
      before anything runs
- [x] Dry-run plan screen: exact command + env (key masked) shown before
      executing; nothing runs until the user confirms
- [x] Step runner executes the real `setup.sh --yes` with the collected env,
      streaming stdout/stderr live into the Logs view with per-step
      success/failure
- [x] Post-install verify step re-checks every container and prints URLs
- [x] Installer auto-fetched: shallow-clones TorBox-Media-Server into
      `~/.local/share/porthole` on first run, pulls updates when present

## Phase 2 — Fleet management + Doctor ✅ (current)

The product thesis extended to maintenance: Porthole doesn't just set the
fleet up, it keeps it healthy — still with no expert knowledge required.

- [x] Fleet actions from the dashboard: start / stop / restart per service
      (`s` / `x` / `R`), with live status refresh and plain feedback
- [x] Doctor view: fresh health check of Docker, compose, install location,
      and every service (exists? running? port actually answering?)
- [x] One-key fixes: the Doctor offers `[f]` to start stopped services,
      then re-checks automatically
- [x] Honest diagnosis: crash-looping services get a plain explanation,
      not a useless "have you tried restarting it"
- [x] Install location remembered in `~/.config/porthole/config.json`
      (saved by the wizard on success)

## Phase 3 — Native provisioning + care

- [x] Port config generation to Rust (`.env`, Decypharr `config.json`, the three
      *arr `config.xml` files, systemd unit) — **byte-identical** to the
      installer's output, verified by diffing against the real shell functions
      with identical inputs. Secrets are generated with a real CSPRNG
      (`getrandom`) and preserved from the existing `.env` on rewrite.
- [x] Update detection with one-key safe updates + rollback: `docker compose
      pull` diffed per-service, then backup → tag current images → pull →
      restart → 90s health check → automatic rollback (re-tag + config restore)
      on any failure.
- [x] Backup/restore of the whole configuration (timestamped tarballs in
      `~/.local/share/porthole/backups/`, media data excluded).
- [x] Uninstall that cleanly removes everything (mirrors `uninstall.sh`),
      with a double confirmation.
- [x] Care view (`5`): plain-language confirmations for every destructive
      action, live progress logs, backup picker for restore.
- [ ] The Setup wizard still orchestrates `setup.sh` for the initial install
      (it also does API-based *arr wiring, which is not deterministic and not
      yet ported) — native generation is used by Care's "Regenerate configs".

## Phase 4 — Polish & packaging

- [x] `porthole --version`, version shown in the header bar
- [x] First-run welcome overlay: what Porthole is, in two paragraphs —
      `Enter` starts the Setup wizard, `Esc` looks around first
- [x] Self-update: checks GitHub releases at most once a day (background,
      silent on failure), Care view can install with checksum verification;
      refuses dev builds (`target/`) honestly
- [x] Release CI: pushing tag `vX.Y.Z` (matching Cargo.toml) builds the
      release binary, packages `porthole-x86_64-linux.tar.gz` + SHA256SUMS,
      and publishes a GitHub release

## Non-goals

- A GUI. The TUI works over SSH, which is where servers live. A web dashboard
  is a maybe-someday, not a plan.
- Supporting every *arr app ever. The curated nine are the product; breadth
  comes after depth.
- AI features. This project is deliberately non-AI: deterministic,
  inspectable, offline-capable.
