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

---

# The next roadmap — growing the fleet

Researched October 2026 (see `~/workspace/research_notes/
selfhosted-media-landscape-2026-20261007-0149/`). The guiding principle is
unchanged: **Porthole is the glue, not the installer.** Every addition below
is judged by whether the user needs to know less than before — each new
service must arrive pre-wired to the rest, not as another tab to configure.

Two facts shape everything:

1. **Debrid-only is proven.** Decypharr exposes mock qBittorrent/SABnzbd
   APIs over TorBox (and others); the *arrs genuinely can't tell it isn't
   a local client. No Servarr-family app fundamentally needs a real
   download client. (Caveat: TorBox is Decypharr's roughest edge — issue
   tracker shows timeouts and uncached-handling bugs; budget testing or
   pin a community fork carrying the fixes.)
2. **All Servarr *arrs share one shape**: Prowlarr indexer sync
   (indexers configured once, pushed to every app), the same `/api/v3`
   REST patterns with API-key auth, and the same download-client
   abstraction with per-app categories. Adding a Servarr app is mostly
   wiring, not invention.

And one red flag that reshapes provider choice:

3. **TorBox's July 2026 TOS overhaul.** TorBox is now operated by an
   opaque UAE free-zone entity (Anonymous Systems FZ-LLC, billing via
   Delaware ReAnonymous LLC); the new terms push 100% liability to
   users and require consent to session-replay telemetry, device IDs
   and IP geolocation, with broad "governmental request" disclosure.
   Reliability is also reported declining. Porthole must not hard-code
   TorBox as the only debrid — provider choice becomes a first-class
   wizard question (see Phase 8).

## Phase 5 — Fleet expansion: music, subtitles, sports

The three highest-value, lowest-risk additions. All actively maintained;
all slot into the existing wiring with no new infrastructure.

- [ ] **Lidarr** (music — Sonarr/Radarr's sibling, active Sep 2026).
      Wiring: Prowlarr syncs music indexers automatically; Decypharr
      category `lidarr`; lands in `/music` for Plex/Jellyfin; same API-key
      auth pattern as the other *arrs, so Doctor/Care/regen extend
      mechanically. Open question: no Seerr-equivalent request manager
      for music was found — requests may stay inside Lidarr's own UI
      until something emerges.
- [ ] **Bazarr** (subtitles, v1.6.2 Sep 2026). Wiring: needs only the
      Sonarr/Radarr/Lidarr API keys Porthole already holds, plus media
      paths — no download client at all. Auto-fetches and upgrades
      subtitles beside the media; Plex webhook refreshes the library.
      Near-zero infrastructure cost, high everyday value.
- [ ] **Sportarr** (sports PVR — the standout new *arr, repo Oct 2025,
      Sonarr-API-compatible, Prowlarr sync works, Decypharr supported).
      Wiring: same shape as Sonarr, so it rides the existing patterns;
      younger and less battle-tested than the family, so ship it as an
      opt-in fleet member, not a default.
- [ ] For each: service definition + plain-language description, wiring-map
      entries, Prowlarr sync, Doctor health checks, Care backup coverage,
      native config generation where deterministic, and a wizard question
      ("Which of these do you want?" — music / subtitles / sports,
      all default-on except Sportarr).

## Phase 6 — The fleet looks after itself: companion automation

The thesis extended: not just installed and wired, but *maintained*
without expertise. Each of these is actively maintained and API-wired
to the fleet.

- [ ] **Maintainerr** (v3.30.0 Oct 2026) — rule-based collection
      management: deletes watched-and-aging media, cleans up Seerr
      requests. Wiring: needs a watch-stats source, which chains to the
      next two. This is the "counterweight to hoarding."
- [ ] **Tautulli** (Plex, v2.18.1) / **Jellystat** (Jellyfin, v1.1.12) —
      watch stats and monitoring, one per the chosen media server.
      Wiring: API tokens auto-configured; Jellystat's REST API feeds
      Maintainerr directly.
- [ ] **Cleanuparr** (v2.10.6) — queue hygiene: kills stalled/blocked
      downloads and re-searches. Wiring: *arr + download-client APIs
      Porthole already holds. (Run *or* Decluttarr, not both.)
- [ ] **Unpackerr** — auto-extracts archives so *arr imports never stall.
      Tiny, harmless, default-on candidate.
- [ ] **Recyclarr / Configarr** — syncs TRaSH-Guides quality profiles and
      custom formats into Sonarr/Radarr. This is the "sane defaults"
      play: the single biggest no-expert-knowledge win in the *arr world,
      and exactly Porthole's thesis.
- [ ] **Wizarr** (v2026.7.0) — invite links and onboarding for
      friends/family (Plex/Jellyfin/Emby). Wiring: media-server API +
      Seerr link. Strongest fit for the thesis: sharing the fleet with
      non-technical people, zero explanation needed.
- [ ] Kometa (Plex collections/metadata automation) — strong for Plex
      users; skip entirely on Jellyfin (no equivalent). Optional,
      Plex-profile only.

## Phase 7 — Storage: your drives, your cloud, encrypted

The user asked about FUSE mounts, rclone, GDrive/Dropbox mounting —
then about downloading locally and pushing to remote storage
automatically, privately. Researched October 2026. Verdicts:
**rclone mount is still the standard** for cloud storage;
**mergerfs is still the standard local pooler** (usually + SnapRAID
parity); **cloudplow is effectively dead** (no commits since Aug 2023,
no blessed successor — its feature set is a checklist to reimplement
natively). No disruption on the horizon (watch: ZFS AnyRAID for
mixed-capacity redundancy, still roadmap-stage).

**Mounts & pooling:**

- [ ] **rclone mounts, guided**: Google Drive, pCloud, Dropbox —
      Porthole walks through authorization once, then writes the mount
      config with media-tuned VFS cache presets (`--vfs-cache-mode
      full`, Plex/Jellyfin-friendly chunk/buffer sizes). No flags for
      the user to learn.
- [ ] **Bring-your-own Google OAuth, automated**: rclone's shared
      Google client_id is being retired during 2026 — every new setup
      needs its own Google Cloud OAuth client (and it must be
      "Published" or refresh tokens die in 7 days). Porthole walks the
      user through creating it once, then stores it. This is exactly
      the kind of expert-knowledge trap Porthole exists to remove.
- [ ] **mergerfs pooling**: combine local drives into one mount with
      individually readable disks; optional SnapRAID parity. Wiring:
      the pool becomes the single path Plex/Jellyfin and the *arrs see.
- [ ] **The debrid shortcut**: Decypharr already exposes the debrid cloud
      as WebDAV/NFSv4/SMB — Porthole presents this as the zero-config
      storage option ("your debrid cloud as a drive"), with
      rclone/mergerfs as the bring-your-own-hardware path.
- [ ] **Provider matrix, honest**: pCloud has the best media profile in
      2026 (no file cap, fastest uploads, lifetime plans); Google Drive
      works but enforces 750 GB/day uploads and 5 TB max files;
      Dropbox's ~3 TB plan ceiling is too small for libraries; OneDrive
      not recommended. rclone crypt covers all of them regardless.

**The encrypted upload pipeline** (download local → archive to cloud):

- [ ] **Scheduled, lock-guarded `rclone move`** (systemd timer, every
      15–60 min) — still the 2026 standard; poll-driven beats
      event-driven on reliability. Flags that matter: `--min-age`,
      `--delete-empty-src-dirs`, `--drive-stop-on-upload-limit`,
      partial-file excludes. Porthole implements the mover natively;
      event triggers stay manual/advanced.
- [ ] **rclone crypt ON by default**: client-side encryption before
      anything touches the cloud (scrypt + NaCl SecretBox — not
      AES-256, despite what guides claim). The provider sees ciphertext
      and metadata only; without it, Drive actively scans and blocks
      policy-violating files. Porthole enforces password+salt backup
      (lose it = unrecoverable, wrong password = silently empty
      folders) and asserts filename *and* directory encryption.
- [ ] **Bandwidth time-tables**: rclone's native `--bwlimit` schedule
      (uncapped overnight, capped daytime) as the default; optional
      "throttle while Plex/Jellyfin is streaming" via `rclone rcd`.
      Router QoS stays manual.
- [ ] **The hardlink smoke test** — the killer integration detail:
      *arrs must import LOCALLY, never through an rclone mount (FUSE
      can't hardlink; atomic moves break across filesystems). The
      classic silent failure: separate bind mounts of the same volume
      still fail EXDEV, and the *arr quietly falls back to full copies
      (documented 229 GB duplicate disasters). Porthole enforces the
      single `/data` root bind-mounted identically into every container
      and *tests* hardlinking at install time. This is seamlessness you
      can verify.

**Privacy, end to end** (shown in plain language, not as a toggle farm):

- [ ] With the VPN profile (Phase 8): ISP sees only the VPN server IP,
      timestamps, and volume. Torrent swarm peers see the VPN IP.
      Usenet-over-TLS hides content from the ISP with no swarm at all.
- [ ] With rclone crypt: the cloud provider sees ciphertext + metadata
      only (directory structure, sizes, and access times still leak —
      Porthole says so honestly).
- [ ] DNS goes through the VPN or encrypted DNS (DoH/DoT) — no leaks.
- [ ] Doctor gains storage checks: mount answering? pool healthy?
      parity in sync? uploader running? All in plain language.

## Phase 8 — Download choice: providers, privacy, and beyond debrid-only

Debrid stays the default path, but *which* debrid — and how private the
whole thing is — becomes a first-class choice. Researched October 2026.

**Debrid provider choice** (all five integrate with Decypharr via API
key, so swapping is a config change, not a rebuild):

- [ ] Wizard asks which debrid: **Premiumize** (recommended — bundles
      debrid + cloud + Usenet + VPN, excellent API; watch the point-based
      fair use under heavy *arr automation), **AllDebrid** (budget pick),
      **TorBox** (current default, kept for continuity — but the July
      2026 TOS overhaul and declining reliability are disclosed honestly
      in the wizard), **Debrid-Link** (built-in seeding), **Real-Debrid**
      (fallback only — keyword copyright filter since May 2026 broke
      50–70% of cached mainstream 4K; strict single-IP enforcement).
- [ ] No service has an official Sonarr/Radarr plugin; Decypharr remains
      the multi-provider standard. Porthole pins a Decypharr build with
      the TorBox fixes (community forks carry unmerged patches).

**The privacy stack** — what the ISP can and cannot see, in plain
language on screen:

- [ ] **The local-download privacy profile**: **gluetun** as the Docker
      VPN gateway (still the 2026 standard) — qBittorrent/SABnzbd/Prowlarr
      ride `network_mode: service:gluetun`; kill switch is gluetun's
      built-in firewall (on by default); qBittorrent additionally bound
      to the VPN interface as a second layer. VPN picks, verified
      torrent-friendly with port forwarding: **PIA** or **Proton VPN**
      (Mullvad is private but dropped port forwarding — worse for
      seeding; Nord/Surfshark/Express have none). Plex/Jellyfin/Seerr
      stay OFF the VPN.
- [ ] **Honest guidance Porthole gives**: Usenet-over-SSL and debrid-over-
      HTTPS already blind the ISP to *content* (it sees only encrypted
      sessions, endpoints, timing, volume — no swarm, no harvestable
      IPs); a VPN on top hides *which* provider you use. **WARP is not a
      VPN replacement** — it encrypts transit from the ISP but Cloudflare
      sees everything, with ~2yr retention and no location choice.
      Porthole says this plainly instead of offering a WARP toggle.
- [ ] **Fleet profiles in the wizard**: *Debrid (recommended)* vs
      *Usenet + Torrent (self-downloaded, VPN-routed)* vs *Hybrid*. One
      question, everything downstream rewires: *arr download clients
      point at Decypharr's mocks or the real clients; categories
      (`sonarr`, `radarr`, `lidarr`…) configured automatically.
- [ ] **SABnzbd 5.x** — the default NZB client (best *arr integration).
      The nzbget.com community fork as the lightweight alternative
      (original NZBGet is discontinued — never ship the dead repo).
- [ ] **qBittorrent 5.x** — the default torrent client (native categories
      the *arrs' import logic assumes). Transmission/Deluge only if a
      user brings their own.
- [ ] **autobrr** as the optional power-user add for private-tracker
      racing; **seedboxes** documented as the heavy-seeding alternative
      (largely redundant if you already pay for debrid).

## Phase 9 — Small-disk mode: the library lives in the cloud

The imperative phase. Most users won't have hundreds of GB locally —
so the default architecture must be: **play locally, store remotely**,
with a small local disk. Researched October 2026.

**The streaming stack** (defaults Porthole applies, no flags to learn):

- [ ] **Decypharr DFS mount as the default stream path** — the docs'
      recommended mount: lighter than rclone, ~500MB disk cache, 8MB
      chunks. (rclone VFS `full` mode remains the own-cloud alternative;
      WebDAV is never the default — it has no local cache.)
- [ ] **Symlink imports**: with Decypharr as the download client, *arr
      "imports" become symlinks — instant, zero disk. No copies, no
      waiting, no 229 GB duplicate disasters.
- [ ] **Cache auto-sizing**: Porthole measures free disk at install and
      sizes the VFS cache itself (warm 10–30 GB per 4K stream is enough;
      keeps 10 GB headroom; `--vfs-cache-mode full` always — without it,
      remux seek/resume demonstrably breaks). Buffer kept small
      (per-open-file RAM — a 2026 incident OOMed a box at 256 MB).
- [ ] **.strm files as the mountless alternative**: playable with no
      mount at all — worth offering for the most disk-poor setups.

**Media-server settings, applied automatically** (the expert traps):

- [ ] Plex: preview thumbnails=Never, chapter/intro markers as scheduled
      task (not on-scan), loudness analysis=Never, extensive media
      analysis=off, periodic scans=off — and critically,
      **empty-trash-automatically=OFF** (a scan during a mount outage
      with it on deletes library entries).
- [ ] Jellyfin: real-time monitoring doesn't fire on FUSE — Porthole
      configures scheduled scans instead.
- [ ] **Transcode temp stays local**: Porthole reserves ~25 GB free
      (50 GB+ if 4K transcodes are frequent) and warns honestly at
      install if the disk can't hold it.

**Resilience** (mounts will drop; the fleet must not panic):

- [ ] Health-gated mount lifecycle: `mountpoint -q` before consumers
      start; lazy unmount + remount on failure; systemd automount.
- [ ] Doctor learns the dead-mount signature (`ENOTCONN transport
      endpoint not connected`) and the recovery ritual: restore mount
      → restart consumers → rescan → manual empty-trash — automated,
      in plain language.
- [ ] Honest bandwidth guidance in the wizard: 4K remux direct play
      needs ~100–120 Mbps sustained per stream. If the connection
      can't do it, Porthole says so before promising 4K.

**For the truly disk-poor** (optional):

- [ ] **Janitorr**: schedule-based "watched it, delete it" cleaning —
      the cache's LRU eviction already keeps recently-watched warm;
      Janitorr makes deletion a policy instead of an accident.

Note on ordering: this phase is listed ninth but is architecturally
foundational — small-disk mode should be the *default* Porthole
assumes, with big-local-disk as the advanced path, not the reverse.

## Explicitly deferred — researched, not planned

- [ ] **Readarr** (books) — officially retired and archived 2025-06-27
      (metadata source died). Revivals exist (Librarr, bookshelf,
      readarr-rresurrected) but the verdict is *watch, don't bet yet*.
      Revisit when one hits stable.
- [ ] **Comics** (Mylar3 / Kapowarr) — both active, defensible as a
      later domain phase; needs container-level proxy work for VPN
      egress. Not now.
- [ ] **Games/ROMs** (Questarr / ROMarr) — active and Seerr already
      tracks them; a natural "Phase 9" if the fleet keeps growing.
- [ ] **Whisparr** (adult) — active but niche; never a default.
- [ ] **Tdarr** (transcoding) — closed-source and heavy; prefer Unmanic
      (open, simpler) if transcoding demand appears.
- [ ] **Huntarr** — dead (repo deleted Feb 2026 after a security review
      found unauthenticated *arr API-key dumps). Its maintained fork
      **Seekarr** is the only acceptable form of that automation.

## Non-goals

- A GUI. The TUI works over SSH, which is where servers live. A web dashboard
  is a maybe-someday, not a plan.
- Supporting every *arr app ever. The curated nine are the product; breadth
  comes after depth.
- AI features. This project is deliberately non-AI: deterministic,
  inspectable, offline-capable.
