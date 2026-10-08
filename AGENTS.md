# AGENTS.md — Shiphand

Guidance for AI coding assistants working in this repo.

## What this is

Shiphand is a Rust TUI (ratatui) that installs and **integrates** a
self-hosted media stack: TorBox, Decypharr, Prowlarr, Byparr, Sonarr, Radarr,
Seerr, Plex, Jellyfin. The product thesis: setup is 20% of the value,
integration is 80%. See `ROADMAP.md` for phases.

## Conventions

- **Plain language on screen.** Every service description, wizard step, and
  log line must be understandable by a non-expert. No jargon in the UI.
  Jargon belongs in code comments, never in `services.rs` `plain` fields.
- **The wiring map is data.** Integrations live in `services.rs`
  (`INTEGRATIONS`), not buried in provisioning code. New wiring = new entry.
- **Provisioning phases mirror TorBox-Media-Server.** The shell installer at
  `github.com/nordicnode/TorBox-Media-Server` is the reference implementation;
  `provision.rs` steps map 1:1 to its phases until Phase 2 ports them natively.
- Keep the TUI working over SSH: no mouse-required interactions, no
  sixel/kitty image protocols.

## Checks

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build
cargo test
```
