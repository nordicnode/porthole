//! Small-disk helpers: how much local disk is free, and how much of it
//! should be reserved for streaming cache and transcodes.
//!
//! Small-disk mode is Porthole's default architecture: files live in the
//! cloud, playback happens locally, and the local disk only holds a warm
//! cache plus transcode temp space.

use std::path::Path;
use std::process::Command;

pub const GB: u64 = 1_000_000_000;

/// Minimum free space we'd like to keep for Plex/Jellyfin transcodes
/// (research: 20–30 GB normally, 50 GB+ for 4K).
pub const TRANSCODE_HEADROOM_BYTES: u64 = 25 * GB;

/// Minimum DFS disk cache worth having.
pub const MIN_CACHE_BYTES: u64 = 10 * GB;

/// Cap on auto-sized cache: beyond this, more cache rarely helps playback.
pub const MAX_CACHE_BYTES: u64 = 100 * GB;

/// Free bytes available on the filesystem containing `path`, via `df`.
/// Returns an error instead of guessing when `df` is unavailable or the
/// output can't be parsed — callers must surface that honestly.
pub fn free_bytes(path: &Path) -> Result<u64, String> {
    let out = Command::new("df")
        .args(["-k", "--output=avail"])
        .arg(path)
        .output()
        .map_err(|e| format!("could not run df: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "df exited with status {}",
            out.status.code().unwrap_or(-1)
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    parse_df_avail(&text)
}

/// Parse `df -k --output=avail` output: a header line then one line with
/// available KiB. Kept separate so it's unit-testable without a disk.
pub fn parse_df_avail(text: &str) -> Result<u64, String> {
    let mut lines = text.lines();
    let header = lines.next().ok_or("df printed nothing")?;
    if !header.trim().eq_ignore_ascii_case("avail") {
        return Err(format!("unexpected df header: {header:?}"));
    }
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let kib: u64 = line
            .parse()
            .map_err(|_| format!("could not parse df avail value: {line:?}"))?;
        return Ok(kib.saturating_mul(1024));
    }
    Err("df printed no data line".to_string())
}

/// Auto-size the DFS disk cache from free space:
/// ~35% of free disk after reserving transcode headroom, clamped to
/// [10 GB, 100 GB]. Never negative.
pub fn suggested_cache_bytes(free: u64) -> u64 {
    let usable = free.saturating_sub(TRANSCODE_HEADROOM_BYTES);
    let sized = (usable as f64 * 0.35) as u64;
    sized.clamp(MIN_CACHE_BYTES, MAX_CACHE_BYTES)
}

/// Format bytes as a whole-GB string for config files ("30GB").
pub fn gb_string(bytes: u64) -> String {
    format!("{}GB", bytes.div_ceil(GB))
}

/// Disk-space verdict for the Doctor, in plain language.
#[derive(Debug, PartialEq, Eq)]
pub enum DiskVerdict {
    Plenty(u64),
    Tight(u64),
    Critical(u64),
}

impl DiskVerdict {
    pub fn from_free_bytes(free: u64) -> Self {
        if free >= TRANSCODE_HEADROOM_BYTES {
            DiskVerdict::Plenty(free)
        } else if free >= MIN_CACHE_BYTES {
            DiskVerdict::Tight(free)
        } else {
            DiskVerdict::Critical(free)
        }
    }

    pub fn free_gb(&self) -> u64 {
        match self {
            DiskVerdict::Plenty(f) | DiskVerdict::Tight(f) | DiskVerdict::Critical(f) => f / GB,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_df_output() {
        let text = "Avail\n  48239104\n";
        assert_eq!(parse_df_avail(text), Ok(48_239_104 * 1024));
    }

    #[test]
    fn df_rejects_garbage() {
        assert!(parse_df_avail("").is_err());
        assert!(parse_df_avail("Avail\nnotanumber\n").is_err());
        assert!(parse_df_avail("Something\n123\n").is_err());
    }

    #[test]
    fn cache_sizing_math() {
        // 100 GB free: reserve 25, 35% of 75 = ~26 GB.
        assert_eq!(suggested_cache_bytes(100 * GB), 26_250_000_000);
        // Tiny disk: clamps to the 10 GB minimum.
        assert_eq!(suggested_cache_bytes(20 * GB), MIN_CACHE_BYTES);
        // Huge disk: clamps to the 100 GB cap.
        assert_eq!(suggested_cache_bytes(1_000 * GB), MAX_CACHE_BYTES);
        // Nothing free: minimum, never negative.
        assert_eq!(suggested_cache_bytes(0), MIN_CACHE_BYTES);
    }

    #[test]
    fn gb_string_rounds_up() {
        assert_eq!(gb_string(30 * GB), "30GB");
        assert_eq!(gb_string(30 * GB + 1), "31GB");
    }

    #[test]
    fn verdict_thresholds() {
        assert_eq!(
            DiskVerdict::from_free_bytes(30 * GB),
            DiskVerdict::Plenty(30 * GB)
        );
        assert_eq!(
            DiskVerdict::from_free_bytes(15 * GB),
            DiskVerdict::Tight(15 * GB)
        );
        assert_eq!(
            DiskVerdict::from_free_bytes(5 * GB),
            DiskVerdict::Critical(5 * GB)
        );
    }
}
