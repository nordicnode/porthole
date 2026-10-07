# Porthole v0.2.0 — Real-Hardware E2E Checklist

Run on a real Linux host with Docker. Check off each item; note failures
with the exact error and where it appeared (screen + log line).

## 0. Host prep

- [ ] Docker Engine + `docker compose` v2 installed and running
- [ ] `curl`, `git`, `tar`, `sha256sum` present
- [ ] User in `docker` group (no sudo needed for `docker ps`)
- [ ] At least 25 GB free (Doctor warns below 25, fails below 10)
- [ ] Ports 8080, 8282, 8686, 7878, 8989, 8787, 6767, 1867, 9696, 8096,
      32400 free (or only the ones you plan to use)

## 1. Fresh install — Debrid profile (default)

- [ ] `./porthole` → welcome overlay → Enter → Setup
- [ ] Fill: TorBox API key, install dir (e.g. `~/media-fleet`), PUID/PGID
      (use `id -u` / `id -g`), TZ (e.g. `America/Los_Angeles`)
- [ ] Toggles: Plex, Debrid fleet, TorBox provider, quality 1080p or 4K
- [ ] All extras OFF for the first run
- [ ] Dry-run plan screen looks sane → confirm
- [ ] Provisioning completes; all steps green in the log
- [ ] **PUID/PGID/TZ check**: `grep -E "PUID|PGID|TZ" <install>/.env`
      shows your values, NOT `0:0` / `Etc/UTC`
      (a previous dry-run showed root/UTC despite Porthole passing env —
      this is the item to re-verify)
- [ ] `docker ps` shows: decypharr, prowlarr, byparr, sonarr, radarr,
      seerr, plex — all Up
- [ ] Fleet view (`1`) shows all Running (not just "port open")
- [ ] Plex web UI loads at `http://<host>:32400/web`
- [ ] Seerr at `:5055` can see the *arrs (Porthole wires API keys)

## 2. Integration wiring

- [ ] Wiring view (`3`) lists every connection; spot-check two:
  - [ ] Prowlarr → Sonarr/Radarr indexers synced (Prowlarr UI → Indexers)
  - [ ] Decypharr added as download client in Sonarr (Settings → Download Clients)
- [ ] Add a test show in Seerr → appears in Sonarr within a minute
- [ ] Decypharr WebDAV/DFS reachable from the *arrs

## 3. Doctor

- [ ] Doctor (`4`) → `d` to re-run: all green on a fresh install
- [ ] "Local disk" check reports sensible free space
- [ ] No warnings about services you didn't select

## 4. Extras

- [ ] Care (`5`) → Add or remove extra services → enable Lidarr + Bazarr
- [ ] `docker compose up -d` picks them up; `docker ps` shows them
- [ ] Lidarr UI at `:8686`; Bazarr at `:6767` (768 MB mem cap: `docker stats`)
- [ ] Disable them again via Care → containers removed (`--remove-orphans`)
- [ ] Jellystat: enable → `configs/jellystat-db/password.txt` exists, mode 600

## 5. Local-download profile + VPN

- [ ] Fresh install (or re-run Setup) with Self-downloaded profile
- [ ] `docker-compose.override.yml` exists, mode 600, single `services:` key,
      contains gluetun + qbittorrent + sabnzbd
- [ ] Care → Set up VPN for downloads → enter WireGuard key
- [ ] Override still valid YAML; key present; file still 600
- [ ] `docker exec gluetun wget -qO- ifconfig.me` shows VPN IP, not yours
- [ ] qBittorrent WebUI at `:8080` (via gluetun network stack)
- [ ] Confirm Plex/Jellyfin are NOT behind the VPN (remote access works)
- [ ] Care → Wire up download clients → qBittorrent password shown once;
      Sonarr/Radarr list qBittorrent as download client

## 6. Updates with rollback

- [ ] Care → Check for updates → pull completes
- [ ] If updates found: apply → images tagged `porthole-prev-*`
      (`docker images | grep porthole-prev`)
- [ ] After restart, health check waits for ALL services incl. extras
- [ ] Simulate failure (e.g. `docker stop sonarr` mid-update) → rollback
      restores previous images and configs

## 7. Backup & restore

- [ ] Care → Create backup → tarball in `~/.local/share/porthole/backups/`
- [ ] Tarball excludes `data/` (media not duplicated)
- [ ] Break something (edit `.env` badly) → Care → Restore → fleet recovers
- [ ] Restore stops ALL containers first (incl. extras), not just base

## 8. Small-disk mode

- [ ] Care → Optimize for small disk → Decypharr `config.json` gains DFS
      mount block + `default_download_action: symlink`
- [ ] Care → Test my connection speed → verdict is honest about 4K remux
      (≈100 Mbps needed)
- [ ] Plex: transcode temp is local; thumbnails/loudness off (Care → Tune
      media server for cloud)

## 9. Cloud storage (rclone)

- [ ] Care → Set up cloud storage → pick backend → guided `rclone config`
- [ ] Crypt password generated; **warning shown to save it**
- [ ] `rclone.conf` mode 600
- [ ] Care → Set up automatic uploads → systemd timer active
      (`systemctl --user list-timers` or system)
- [ ] Care → Test hardlinks → passes inside the *arr container
      (catches the EXDEV silent-copy trap)

## 10. Self-update

- [ ] Care → Check for Porthole updates → reports current when up to date
- [ ] (After v0.2.1 exists) update downloads, verifies SHA256, swaps binary,
      keeps `.bak` until success

## 11. Uninstall

- [ ] Care → Uninstall → plan lists ALL containers (not "8")
- [ ] Double confirmation → `docker ps` empty, install dir gone,
      systemd service disabled, `media-network` removed

---

## Known sandbox-only blockers (not re-testable here)

- Kernel lacked bridge networking; egress proxy blocked registry + TorBox
  API. Both are environment limits, not app bugs.

## Reporting a failure

Paste: the checklist item, what you expected, the exact error/log lines,
and `docker ps -a` output. I'll fix it from there.
