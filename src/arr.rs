//! *arr API integration: the wiring Porthole owns.
//!
//! The installer sets up the initial download-client and Prowlarr links.
//! Porthole verifies they stay correct and re-syncs when they drift:
//! - Decypharr as the download client (QBittorrent mock, username = the
//!   *arr's own URL, password = the *arr's API key — callback routing,
//!   not auth). Mirrors the installer's exact payload.
//! - Prowlarr indexer re-sync via `ApplicationIndexerSync` (the per-app
//!   sync button silently no-ops on stale mappings after rebuilds).

use anyhow::{Context, Result};
use std::process::Command;

use crate::care::CareEvent;
use crate::configarr::arr_api_key;

/// One *arr we manage.
struct Arr {
    /// "Sonarr" / "Radarr" — display.
    name: &'static str,
    /// "sonarr" / "radarr" — config dir + container + category.
    id: &'static str,
    port: u16,
}

const ARRS: &[Arr] = &[
    Arr {
        name: "Sonarr",
        id: "sonarr",
        port: 8989,
    },
    Arr {
        name: "Radarr",
        id: "radarr",
        port: 7878,
    },
];

fn api_get(port: u16, api_key: &str, path: &str) -> Result<String> {
    let out = Command::new("curl")
        .args([
            "-sf",
            "--connect-timeout",
            "5",
            "--max-time",
            "20",
            "-H",
            &format!("X-Api-Key: {api_key}"),
            &format!("http://localhost:{port}{path}"),
        ])
        .output()
        .context("curl failed")?;
    if !out.status.success() {
        anyhow::bail!("API request failed");
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn api_post(port: u16, api_key: &str, path: &str, body: &str) -> Result<String> {
    let out = Command::new("curl")
        .args([
            "-sf",
            "--connect-timeout",
            "5",
            "--max-time",
            "30",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "-H",
            &format!("X-Api-Key: {api_key}"),
            &format!("http://localhost:{port}{path}"),
            "-d",
            body,
        ])
        .output()
        .context("curl failed")?;
    if !out.status.success() {
        anyhow::bail!("API request failed");
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// The Decypharr download-client payload. Byte-mirrors the installer's
/// `configure_arr_service` (QBittorrent mock, torrent protocol).
fn decypharr_client_json(arr: &Arr, api_key: &str) -> String {
    let cat_field = if arr.id == "sonarr" {
        "tvCategory"
    } else {
        "movieCategory"
    };
    let cat_imported = if arr.id == "sonarr" {
        "tvImportedCategory"
    } else {
        "movieImportedCategory"
    };
    format!(
        r#"{{"name":"Decypharr","implementation":"QBittorrent","configContract":"QBittorrentSettings","protocol":"torrent","enable":true,"priority":1,"removeCompletedDownloads":true,"removeFailedDownloads":true,"fields":[{{"name":"host","value":"decypharr"}},{{"name":"port","value":8282}},{{"name":"useSsl","value":false}},{{"name":"username","value":"http://{id}:{port}"}},{{"name":"password","value":"{key}"}},{{"name":"{cat}","value":"{id}"}},{{"name":"{cat_imp}","value":""}},{{"name":"initialState","value":0}},{{"name":"sequentialOrder","value":false}},{{"name":"firstAndLastFirst","value":false}}],"tags":[]}}"#,
        id = arr.id,
        port = arr.port,
        key = api_key.replace('"', "\\\""),
        cat = cat_field,
        cat_imp = cat_imported,
    )
}

/// Check whether the Decypharr download client is correctly wired in an *arr.
/// Returns Ok(true) when host/username/password all match.
fn decypharr_wired(arr: &Arr, api_key: &str) -> bool {
    let clients = match api_get(arr.port, api_key, "/api/v3/downloadclient") {
        Ok(c) => c,
        Err(_) => return false,
    };
    // Find the Decypharr client's fields and check the three that matter.
    // Simple substring checks — robust enough for this structured JSON.
    let marker = "\"name\":\"Decypharr\"";
    let Some(idx) = clients
        .find(marker)
        .or_else(|| clients.find("\"name\": \"Decypharr\""))
    else {
        return false;
    };
    let tail = &clients[idx..];
    // Bound the search to this client object (next "name": at top level
    // would be far away; 3000 chars is plenty for the fields array).
    let tail = &tail[..tail.len().min(3000)];
    let expect_user = format!("http://{}:{}", arr.id, arr.port);
    tail.contains("\"name\":\"host\",\"value\":\"decypharr\"")
        && tail.contains(&format!(
            "\"name\":\"username\",\"value\":\"{expect_user}\""
        ))
        && tail.contains(&format!("\"name\":\"password\",\"value\":\"{api_key}\""))
}

/// Ensure the Decypharr download client exists and is correct in both
/// *arrs. Adds it if missing, refreshes the password if the API key
/// rotated. Idempotent.
pub fn ensure_decypharr_client(
    install_dir: &std::path::Path,
    tx: &std::sync::mpsc::Sender<CareEvent>,
) -> Result<()> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };
    for arr in ARRS {
        let api_key = arr_api_key(install_dir, arr.id).map_err(|e| anyhow::anyhow!("{e}"))?;
        if decypharr_wired(arr, &api_key) {
            log(&format!(
                "[ok] {}: Decypharr download client is correct",
                arr.name
            ));
            continue;
        }
        log(&format!(
            "[in] {}: (re)wiring Decypharr download client…",
            arr.name
        ));
        let body = decypharr_client_json(arr, &api_key);
        // Try update-in-place first if a Decypharr client exists with an id.
        let clients = api_get(arr.port, &api_key, "/api/v3/downloadclient").unwrap_or_default();
        let existing_id = clients
            .find("\"name\":\"Decypharr\"")
            .and_then(|i| {
                // crude: find "id":N before the name within this object
                let head = &clients[..i];
                head.rfind("\"id\":").map(|j| {
                    head[j + 5..]
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect::<String>()
                })
            })
            .filter(|s| !s.is_empty());
        let res = match existing_id {
            Some(id) => {
                let out = Command::new("curl")
                    .args([
                        "-sf",
                        "--connect-timeout",
                        "5",
                        "--max-time",
                        "30",
                        "-X",
                        "PUT",
                        "-H",
                        "Content-Type: application/json",
                        "-H",
                        &format!("X-Api-Key: {api_key}"),
                        &format!(
                            "http://localhost:{}/api/v3/downloadclient/{id}?forceSave=true",
                            arr.port
                        ),
                        "-d",
                        &body,
                    ])
                    .output();
                out.map(|o| o.status.success()).unwrap_or(false)
            }
            None => api_post(
                arr.port,
                &api_key,
                "/api/v3/downloadclient?forceSave=true",
                &body,
            )
            .is_ok(),
        };
        if res && decypharr_wired(arr, &api_key) {
            log(&format!(
                "[ok] {}: Decypharr download client wired",
                arr.name
            ));
        } else {
            anyhow::bail!(
                "{}: could not wire the Decypharr download client — check its web UI",
                arr.name
            );
        }
    }
    Ok(())
}

/// Re-sync Prowlarr's indexers into the *arr apps.
///
/// The per-app "sync" button in Prowlarr silently no-ops when indexer
/// IDs go stale (e.g. after a rebuild). `ApplicationIndexerSync` is the
/// reliable path. Verifies Prowlarr actually has indexers to push.
pub fn prowlarr_resync(
    install_dir: &std::path::Path,
    tx: &std::sync::mpsc::Sender<CareEvent>,
) -> Result<()> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };
    let api_key = arr_api_key(install_dir, "prowlarr").map_err(|e| anyhow::anyhow!("{e}"))?;
    let indexers = api_get(9696, &api_key, "/api/v1/indexer").unwrap_or_default();
    if indexers.trim() == "[]" || indexers.trim().is_empty() {
        log("[warn] Prowlarr has no indexers configured yet — add some in its UI first");
        log("       (Porthole will ask for indexer credentials in a later phase)");
        return Ok(());
    }
    log("[in] asking Prowlarr to re-sync its indexers into Sonarr/Radarr…");
    api_post(
        9696,
        &api_key,
        "/api/v1/command",
        r#"{"name":"ApplicationIndexerSync"}"#,
    )
    .context("Prowlarr did not accept the sync command")?;
    log("[ok] sync command accepted — indexers are being pushed to the *arrs");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decypharr_json_matches_installer_contract() {
        let arr = &ARRS[0]; // Sonarr
        let json = decypharr_client_json(arr, "key123");
        // The exact wiring the research specified:
        assert!(json.contains(r#""implementation":"QBittorrent""#));
        assert!(json.contains(r#""name":"host","value":"decypharr""#));
        assert!(json.contains(r#""name":"port","value":8282"#));
        assert!(json.contains(r#""name":"username","value":"http://sonarr:8989""#));
        assert!(json.contains(r#""name":"password","value":"key123""#));
        assert!(json.contains(r#""name":"tvCategory","value":"sonarr""#));
        assert!(json.contains(r#""removeCompletedDownloads":true"#));

        let radarr = &ARRS[1];
        let rjson = decypharr_client_json(radarr, "key456");
        assert!(rjson.contains(r#""name":"username","value":"http://radarr:7878""#));
        assert!(rjson.contains(r#""name":"movieCategory","value":"radarr""#));
    }

    #[test]
    fn decypharr_json_escapes_quotes_in_key() {
        let json = decypharr_client_json(&ARRS[0], r#"a"b"#);
        assert!(json.contains(r#""value":"a\"b""#));
    }
}
