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

## Phase 5 — Small-disk mode: the library lives in the cloud ✅

The imperative phase. Most users won't have hundreds of GB locally —
so the default architecture must be: **play locally, store remotely**,
with a small local disk. Researched October 2026.

**The streaming stack** (defaults Porthole applies, no flags to learn):

- [x] **Decypharr DFS mount as the default stream path** — the docs'
      recommended mount: lighter than rclone, auto-sized disk cache,
      10MB chunks. Care → "Optimize for small disk" merges it into the
      existing config (backup first). (rclone VFS `full` mode remains the
      own-cloud alternative; WebDAV is never the default — it has no
      local cache.)
- [x] **Symlink imports**: `default_download_action: symlink` — *arr
      "imports" become symlinks — instant, zero disk. No copies, no
      waiting, no 229 GB duplicate disasters.
- [x] **Cache auto-sizing**: Porthole measures free disk and sizes the
      DFS cache itself (~35% of free after reserving 25 GB transcode
      headroom, clamped 10–100 GB). (`src/storage.rs`, tested.)
- [x] **.strm files as the mountless alternative**: Care → "Use .strm
      files (no mount)" — sets `default_download_action: strm`, drops the
      mount block. Jellyfin plays them natively; Plex needs
      plex-strm-assistant (said on the confirm screen).

**Media-server settings, applied automatically** (the expert traps):

- [x] Plex: Care → "Tune media server for cloud" — token read from
      Preferences.xml (same as the installer), 7 prefs via `PUT /:/prefs`
      (all names verified against real Preferences.xml files):
      `autoEmptyTrash=0`, `GenerateBIFBehavior=never`,
      `GenerateChapterThumbBehavior=never`,
      `GenerateIntroMarkerBehavior=never`, `LoudnessAnalysisBehavior=never`,
      `ScheduledLibraryUpdatesEnabled=0`, `ButlerTaskDeepMediaAnalysis=0`.
- [x] Jellyfin: the installer provisions no API key and there's no
      unauthenticated settings API, so Porthole is honest about the
      boundary — "Tune media server" reports the exact status instead of
      pretending. (Jellyfin's defaults are already FUSE-tolerant: the
      daily "Scan Media Library" task exists, and real-time monitoring
      harmlessly no-ops on FUSE.)
- [x] **Transcode temp stays local**: Doctor's "Local disk" check warns
      honestly when free space drops under ~25 GB (fail under 10 GB).
      (Bind-mounting transcode temp locally is installer-level work.)

**Resilience** (mounts will drop; the fleet must not panic):

- [x] Health-gated mount lifecycle: Doctor's "Cloud drive" check runs
      `docker exec decypharr mountpoint -q /mnt/decypharr` (only when
      small-disk mode is active) and catches the dead-mount signature
      (`ENOTCONN transport endpoint not connected`).
- [x] One-key recovery: `[f] Restart Decypharr` right on the failed check
      — the recovery ritual (restore mount → restart consumers → rescan)
      in plain language, no terminal needed.
- [x] Honest bandwidth guidance: Care → "Test my connection speed"
      downloads 25 MB from Cloudflare's speed-test endpoint and reports
      the verdict plainly (<15 Mbps: 1080p risky; <25: 4K will buffer;
      <60: compressed 4K ok; 60+: remux territory).

**Traps & edge cases (researched Oct 2026):**

- **Decypharr DFS config, exact**: `mount.type: dfs`,
  `mount_path: /mnt`, `cache_expiry: 24h`, `cache_dir: /cache/dfs`,
  `disk_cache_size: 500MB`, `chunk_size: 8MB`, `read_ahead_size:
  128MB`, and **`allow_other: true` is required**. Symlinks for *arr
  "imports" resolve to the **FUSE path** — if the mount drops they
  dangle, items become unplayable, and a scan with empty-trash ON
  **wipes the library**. (See: empty-trash OFF, always.)
- **Plex has NEVER supported .strm natively** — Jellyfin/Emby/Kodi
  only. Plex needs a proxy shim (plex-strm-assistant :3000) plus an
  ffprobe pass for real metadata. Format is a single-URL text file;
  auth-walled URLs fail server-side (ffmpeg gets HTML). Offer .strm
  mode for Jellyfin first; Plex shim is phase-9b.
- **VFS sizing math**: 25–50% of free disk, minimum ~10–20 GB;
  `vfs_cache_max_age` 12–24h ("recently watched stays warm").
  `--buffer-size` is per-file RAM — keep it small.
- **Plex cloud prefs** (via `PUT /:/prefs` or Preferences.xml, so
  Porthole sets them programmatically): preview thumbnails NEVER,
  extensive media analysis OFF, empty trash OFF, intro/credits
  detection OFF (it decodes the whole file), periodic full scan OFF,
  relay OFF.
- **Transcode math**: `./transcode:/transcode` + `TRANS_DIR`, 20–30 GB
  typical, 50 GB+ for 4K/multi-stream; a full disk gives "Not enough
  disk space to convert this item". ~2,000 PassMark per 1080p software
  transcode (~12,000 for 4K — Porthole should steer 4K-transcode boxes
  to direct play, not promise transcoding).
- **Mount resilience, exact**: systemd `Type=notify`,
  `ExecStop=fusermount -uz`, `Restart=always`; health via
  `mountpoint -q`; on ENOTCONN: lazy unmount → restart mount →
  verify → trigger Plex/Jellyfin refresh. Doctor automates this
  sequence.
- **Janitorr is a loaded gun**: port 8978, `application.yml`, needs
  Jellystat (or Streamystats — not both), **`dry-run: true` is the
  default** (keep it), "Leaving Soon" collections as the review
  queue. **Issue #234: "deleted half of library"** — bad Jellystat
  watch data + aggressive expiration = mass deletion. And Jellyfin
  deletes need a **dedicated user account** — an API key alone is
  insufficient. Porthole gates Janitorr behind a watched-data sanity
  check and keeps dry-run until the user explicitly arms it.
- **Bandwidth honesty**: measure at install (`speedtest-cli` +
  server→client iperf3). Thresholds: <15 Mbps = 1080p risky, 25 Mbps
  = compressed 4K, **60–80+ Mbps sustained for 4K remux** (typical
  remux ~100 Mbps peaks — budget +50% headroom for VBR). If the pipe
  can't do it, Porthole says so before promising 4K.

**Re-check at implementation** (index-researched Oct 2026, no live
verification — confirm before building): AllDebrid live host status;
Sportarr bug #229 fix state; Premiumize exact point costs (readable
via API); pCloud 2026 policy fine print; Bazarr+ fork vs conservative
upstream 1.6.x; Jellyfin 12.x API parity for Janitorr deletes.

## Phase 6 — Expert config, zero questions

Researched October 2026. The key finding: **Porthole should not
hand-configure the *arrs at all.** Ship **Configarr** (2026's better
default over Recyclarr: feature superset, Lidarr support, active
development, accepts Recyclarr templates) and generate its `config.yml`
from user answers. Configarr then syncs TRaSH-Guides quality profiles,
custom formats + scores, quality sizes, and file naming —
idempotently. Hand edits to managed profiles get reverted by design;
Porthole designs around that instead of fighting it.

**Shipped 2026-10-07** (10/10). Two honest deviations from the
research plan, both verified before building:

- **One question, not four.** The research proposed 1080p/4K, HDR,
  HD-audio, and anime. Verification showed the UHD profiles already
  encode optimal HDR/audio custom-format scores, and anime naming is
  always applied (`episodes.anime: default`). Asking the other three
  would be theater, not configuration — so Porthole asks only
  "1080p or 4K?" in the Setup wizard.
- **One-shot job, not a scheduled container.** Configarr runs as
  `docker run --rm` on the fleet's `media-network` (a) automatically
  as the last provisioning step, and (b) on demand via Care. A
  standing schedule would silently revert hand-edits on a timer;
  the explicit re-run keeps that behavior visible and chosen.
  Scheduled sync is deferred to Phase 10 if users ask for it.

- [x] **Configarr config generated natively** (`src/configarr.rs`):
      `config.yml` + `secrets.yml` from the quality answer. Template
      IDs verified against recyclarr/config-templates `templates.json`
      (Oct 2026): Sonarr `web-1080p`/`web-2160p`, Radarr
      `hd-bluray-web`/`uhd-bluray-web`. Naming always TRaSH `default`
      (standard/daily/anime). Secrets file written `0600`.
- [x] **TRaSH 2026 profiles applied**: via the templates above —
      quality definitions, profiles, custom-format groups, and scores
      all come from TRaSH-Guides through Configarr. No hand-maintained
      profile JSON in Porthole.
- [x] **Exact TRaSH naming applied**: `media_naming` set to TRaSH
      `default` for Sonarr (series/season/standard/daily/anime,
      rename on) and Radarr (folder/movie, rename on).
- [x] **Decypharr wiring, exact** (`src/arr.rs`): the installer
      already wires it on setup; Porthole **verifies** (Doctor
      "Download client" check) and **re-wires** (one-key fix + Care).
      Payload byte-mirrors the installer: QBittorrent mock, host
      `decypharr:8282`, username = the *arr's own URL
      (`http://sonarr:8989`), password = the *arr's API key,
      category `sonarr`/`radarr`, Remove Completed = Yes.
      (Research said Remove Failed = No and SABnzbd type; the
      installer's live code uses QBittorrent mock + Remove Failed =
      Yes — Porthole matches the proven working config.)
- [x] **Prowlarr sync, exact** (`src/arr.rs::prowlarr_resync`):
      re-syncs via `POST /api/v1/command
      {"name":"ApplicationIndexerSync"}` and verifies Prowlarr has
      indexers to push. Runs automatically after every Configarr sync.
- [x] **The entire question budget** — the only things Porthole ever
      asks the user, ever: 1080p vs 4K · host path for /data ·
      indexer credentials · debrid credentials · Plex vs Jellyfin ·
      primary language (if not English). **Seven** questions
      (was nine). Everything else is derived.
- [x] **Top-5 misconfigurations, designed out**: hand-editing synced
      profiles (Configarr reverts; Care warns before running);
      wrong Prowlarr resync (Porthole re-syncs + verifies);
      substring blocklist terms (TRaSH CFs, never substrings);
      Decypharr user/pass left blank (always arr URL + API key,
      verified by Doctor). Split bind mounts → Phase 7.
- [ ] Lidarr note: TRaSH guidance for music is community-grade
      (Davo guide, FLAC-first) with experimental Configarr support —
      ship with conservative defaults, mark experimental in the UI.
      (Deferred to Phase 7 fleet expansion, which adds Lidarr itself.)

## Phase 7 — Fleet expansion: music, subtitles, sports

The three highest-value, lowest-risk additions. All actively maintained;
all slot into the existing wiring with no new infrastructure.

**Shipped 2026-10-07** (9/10 — Bazarr's Sonarr/Radarr link is guided,
not automated; see below). Architecture: Porthole generates
`docker-compose.override.yml` (auto-discovered by the installer's
compose wrapper) instead of patching the installer's compose file.
All three default **off** in the wizard (Sportarr is explicitly newer);
a Care action adds/removes them post-install.

- [x] **Lidarr** (music). `lscr.io/linuxserver/lidarr`, port 8686.
      Decypharr download client (category `lidarr`, **API v1** — not
      v3); Prowlarr app with audio category 3000; music root
      `/data/media/music`. API key read from its self-generated
      `config.xml` after first start.
- [x] **Bazarr** (subtitles). `lscr.io/linuxserver/bazarr`, port 6767,
      768 MB memory cap (upstream balloons without one). Porthole
      starts it and shows the exact Sonarr/Radarr connection values
      (host, port, API key) for a 2-minute guided setup — Bazarr's
      settings API schema isn't stable enough to automate safely, and
      its settings POST hangs when the *arrs are unreachable. Same
      honest boundary as Jellyfin in Phase 5.
- [x] **Sportarr** (sports). `sportarr/sportarr:latest`, port 1867,
      opt-in. Sonarr-API-compatible: Decypharr client + Prowlarr app
      (registered as a Sonarr app per its README) reuse the existing
      code paths.
- [x] For each: service definition + plain-language description,
      wiring-map entries, Prowlarr sync (Lidarr/Sportarr),
      Doctor health checks (per-service loop + download-client check
      extended), Care backup coverage (automatic — backup tars the
      install dir minus `data/`), and the wizard question
      ("Music / Subtitles / Sports" toggles).

**Traps & edge cases (researched Oct 2026):**

- Lidarr is **`/api/v1`**, not v3 — a v3-assuming client breaks. All
  metadata goes through Servarr's proxy (`api.lidarr.audio`), so rate
  limits surface as HTTP 429s (Spotify import lists cause cache-miss
  storms; list sync floor ~6h). Prowlarr syncs it as an app profile
  with audio category **3000**. Decypharr's mock qBittorrent API
  explicitly supports Lidarr. Profiles are codec-centric
  (FLAC/MP3-320/AAC), unlike Sonarr/Radarr's resolution-centric ones.
- Bazarr has **no env-var config surface** upstream — Sonarr/Radarr
  addresses + API keys go through its UI/API (use container DNS
  `sonarr`, never localhost). **Trap: its settings-save POST hangs
  forever if Sonarr/Radarr is down** (unbounded retry inside the
  request) — Porthole must bound it (`curl --max-time 60`) and apply
  settings only when both *arrs answer. Never write ip/port without
  both API keys (false "already configured"). Steady-state ~174 MiB —
  cap at 768m, not 128m (OOM crash-loops). Health check: unauthenticated
  `/ping`. Subtitle providers: OpenSubtitles.com needs a free account,
  Podnapisi/Subscene are anonymous, avoid OpenSubtitles.org.
  Bazarr+ fork adds a lot but **crash-loops on upstream databases**
  (Alembic mismatch) — ship upstream, back up before any migration.
- Sportarr's Sonarr-v3-compatible API is real (Series→League,
  Episode→Event mapping) with an additive-only contract since
  v4.0.1023. **Open bug #229: interactive search passes indexer
  categories to Torznab/Newznab, so miscategorized sports releases
  never match** — Porthole should broaden categories by default and
  re-check the bug at implementation. It ships its own Plex/Jellyfin
  metadata agents (use those, not the generic ones). IPTV DVR is
  early alpha — don't promise it.
- Cross-cutting: +0.75–1.5 GB RAM for the three; backups must capture
  `*.db*` (while stopped) + `/config` with identical UID/GID and
  path pairs.

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

**Traps & edge cases (researched Oct 2026):**

- **Plex must NOT go behind gluetun** — it kills remote access.
  Only the downloaders (*arrs, qBittorrent, SABnzbd, Prowlarr) ride
  `network_mode: service:gluetun`; Plex/Jellyfin/Seerr stay on the
  normal network. LAN egress needs
  `FIREWALL_OUTBOUND_SUBNETS=192.168.1.0/24`.
- **qBittorrent 5.x generates a random admin password on first
  run** — Porthole pre-seeds it (`QBITTORRENT_PASSWORD` or the PBKDF2
  config key) or the user is locked out. Content Layout must be
  **"Original"** (not "Create subfolder"), per-category paths
  `/data/torrents/<category>` — the *arr import logic assumes this.
- **Bind qBittorrent to the VPN interface too** (Settings → Advanced
  → Network Interface → tun0/wg0): second layer if the firewall ever
  fails open. Verify the whole chain: `curl ifconfig.me` from inside
  a routed container shows the VPN IP, then stop gluetun and confirm
  traffic dies.
- **AllDebrid: 16 of 52 advertised hosts online** (Aug 2026 spot
  check — re-verify at implementation) and a **12 req/s + 600/min per
  key** API cap — budget across 4+ *arrs; use Prowlarr as the single
  query point.
- **Premiumize points**: per-GB point costs with daily regen — heavy
  4K days can exhaust them; Porthole should surface the balance, not
  just fail downloads.
- **Decypharr per-provider quirks**: `debrids[]` entries carry
  provider/api_key/rate_limit/refresh/workers; `default_download_action`
  is `symlink|download|strm|none`. TorBox is the roughest provider,
  Real-Debrid the smoothest. Per-*arr provider splits need a **second
  Decypharr instance (:8283)** plus Remote Path Mapping — document,
  don't automate, v1.
- **autobrr has no shippable defaults** (v1.87.0) — filters are
  inherently manual. Porthole deploys it, wires the *arr APIs, and
  ships commented example filters. Honest, not magic.

## Phase 9 — Storage: your drives, your cloud, encrypted

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

**Traps & edge cases (researched Oct 2026):**

- **`--allow-other` needs `user_allow_other` uncommented in
  `/etc/fuse.conf`** or Plex gets permission denied on the mount.
  Classic trap — Porthole checks and fixes (or instructs) at install.
- **Wrong crypt password = silently empty folder**, no error.
  Porthole always verifies against a known file after setup, and
  **rclone.conf holds crypt passwords/salts + OAuth tokens** — back
  it up encrypted (age/gpg), two locations, never plaintext in git.
  Losing crypt secrets = data permanently unrecoverable.
- **The hardlink trap, precisely**: two bind mounts of the same host
  volume are different vfsmounts — `link()` returns EXDEV and the
  *arrs silently fall back to FULL copies (229 GB of duplicates
  measured). Rule: one `/data` root at the identical container path
  everywhere; Porthole smoke-tests `touch`+`ln` *inside the
  containers* at install.
- **Google BYO OAuth, step by step**: Cloud project → enable Drive
  API → consent screen **must be "Published"** (Testing = tokens die
  every 7 days) → Desktop-app OAuth client → paste ID/secret into
  `rclone config`. Every step is a drop-off point; Porthole walks it.
- **750 GB/day Drive cap**: configure `--drive-stop-on-upload-limit`
  + `--max-transfer 700G` so the uploader pauses instead of erroring.
- **SnapRAID is not real-time** (nightly `snapraid sync`; parity disk
  ≥ largest data disk) — Porthole must not imply live redundancy.
- **pCloud is a secondary backend only** (throttling/abuse-flag
  history) despite the good media profile; EU endpoint is
  `eapi.pcloud.com`.
- Upload mover: **systemd timer over cron**, `flock -n` lockfile,
  `rclone move --min-age 15m --delete-empty-src-dirs --exclude
  "*.partial~" --exclude "*.!qB"`; bandwidth timetable
  `--bwlimit "01:00,off 08:00,30M"`; dynamic throttling via
  `rclone rcd` + `rclone rc core/bwlimit`.

## Phase 10 — The fleet looks after itself: companion automation

- [ ] **Janitorr** — schedule-based "watched it, delete it" cleaning
      for the truly disk-poor (moved from Phase 5: it's a companion
      service, not small-disk core). Starts dry-run; the cache's LRU
      eviction already keeps recently-watched warm.

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
- [ ] **Configarr** — syncs TRaSH-Guides quality profiles and custom
      formats into Sonarr/Radarr/Lidarr. This is the "sane defaults"
      play: the single biggest no-expert-knowledge win in the *arr
      world, and exactly Porthole's thesis. Promoted to a core
      architectural piece — see Phase 6 for the full design
      (generated `config.yml`, the four-question budget, exact TRaSH
      profiles and naming, Decypharr wiring specifics).
- [ ] **Wizarr** (v2026.7.0) — invite links and onboarding for
      friends/family (Plex/Jellyfin/Emby). Wiring: media-server API +
      Seerr link. Strongest fit for the thesis: sharing the fleet with
      non-technical people, zero explanation needed.
- [ ] Kometa (Plex collections/metadata automation) — strong for Plex
      users; skip entirely on Jellyfin (no equivalent). Optional,
      Plex-profile only.

**Traps & edge cases (researched Oct 2026):**

- **Maintainerr deletes for real.** Always start in dry-run mode with
  the pre-execution review page (v3.17.0+) — Porthole's default rule
  set must be conservative, and the first run is review-only.
- **Jellystat needs PostgreSQL** — that's a second container and
  `pg_dump`-based backups, not just a volume copy. Porthole must
  provision and back up both.
- **Recyclarr reverts hand-edited profiles.** First sync merges, but
  the next sync reverts any hand edits to *managed* profiles. Porthole
  must warn before the user touches them (or better: never present the
  managed profiles as editable).
- **Kometa needs a TMDb API key** — another signup in the wizard, and
  its sane default config is ~150–300 lines of YAML Porthole must
  template, not ask about.
- **Cleanuparr over Decluttarr** as the default (per-*arr scoping,
  actively maintained) — and stall timeouts must be generous with
  Decypharr in the chain, or it will kill slow-but-healthy debrid
  downloads.
- Dependency order matters: Postgres → Jellystat → Plex/Jellyfin →
  Tautulli → Maintainerr; Prowlarr → *arrs → Recyclarr. Porthole must
  bring them up in order and wire the full API-key chain itself: *arr
  keys → Prowlarr/Bazarr/Cleanuparr/Unpackerr/Recyclarr/Maintainerr;
  Plex token → Tautulli/Maintainerr/Wizarr/Kometa; Jellystat
  x-api-token → Maintainerr/Janitorr; Tautulli key → Maintainerr;
  Seerr key → Maintainerr.
- Footprint: ~9 more containers (fleet ≈ 20), +2–3 GB RAM idle.
  The wizard should say this plainly.

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
