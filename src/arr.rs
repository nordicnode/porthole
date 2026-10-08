//! *arr API integration: the wiring Shiphand owns.
//!
//! The installer sets up the initial download-client and Prowlarr links.
//! Shiphand verifies they stay correct and re-syncs when they drift:
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
    /// API version path: "v3" for Sonarr/Radarr/Sportarr, "v1" for Lidarr.
    api: &'static str,
    /// Download-client category field name.
    cat_field: &'static str,
    cat_imported_field: &'static str,
}

const ARRS: &[Arr] = &[
    Arr {
        name: "Sonarr",
        id: "sonarr",
        port: 8989,
        api: "v3",
        cat_field: "tvCategory",
        cat_imported_field: "tvImportedCategory",
    },
    Arr {
        name: "Radarr",
        id: "radarr",
        port: 7878,
        api: "v3",
        cat_field: "movieCategory",
        cat_imported_field: "movieImportedCategory",
    },
];

/// Extra *arrs, wired only when the user opted in.
const EXTRA_ARRS: &[Arr] = &[
    Arr {
        name: "Lidarr",
        id: "lidarr",
        port: 8686,
        api: "v1", // Lidarr v2 is API v1, not v3!
        cat_field: "musicCategory",
        cat_imported_field: "musicImportedCategory",
    },
    Arr {
        name: "Sportarr",
        id: "sportarr",
        port: 1867,
        api: "v3", // Sonarr-API-compatible
        cat_field: "tvCategory",
        cat_imported_field: "tvImportedCategory",
    },
];

fn api_get(port: u16, api_key: &str, api: &str, path: &str) -> Result<String> {
    let out = Command::new("curl")
        .args([
            "-sf",
            "--connect-timeout",
            "5",
            "--max-time",
            "20",
            "-H",
            &format!("X-Api-Key: {api_key}"),
            &format!("http://localhost:{port}/api/{api}{path}"),
        ])
        .output()
        .context("curl failed")?;
    if !out.status.success() {
        anyhow::bail!("API request failed");
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn api_post(port: u16, api_key: &str, api: &str, path: &str, body: &str) -> Result<String> {
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
            &format!("http://localhost:{port}/api/{api}{path}"),
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
    format!(
        r#"{{"name":"Decypharr","implementation":"QBittorrent","configContract":"QBittorrentSettings","protocol":"torrent","enable":true,"priority":1,"removeCompletedDownloads":true,"removeFailedDownloads":true,"fields":[{{"name":"host","value":"decypharr"}},{{"name":"port","value":8282}},{{"name":"useSsl","value":false}},{{"name":"username","value":"http://{id}:{port}"}},{{"name":"password","value":"{key}"}},{{"name":"{cat}","value":"{id}"}},{{"name":"{cat_imp}","value":""}},{{"name":"initialState","value":0}},{{"name":"sequentialOrder","value":false}},{{"name":"firstAndLastFirst","value":false}}],"tags":[]}}"#,
        id = arr.id,
        port = arr.port,
        key = api_key.replace('"', "\\\""),
        cat = arr.cat_field,
        cat_imp = arr.cat_imported_field,
    )
}

/// Check whether the Decypharr download client is correctly wired in an *arr.
/// Returns Ok(true) when host/username/password all match.
fn decypharr_wired(arr: &Arr, api_key: &str) -> bool {
    let clients = match api_get(arr.port, api_key, arr.api, "/downloadclient") {
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
    let base: Vec<&Arr> = ARRS.iter().collect();
    ensure_decypharr_client_for(install_dir, tx, &base)?;
    // Extras only if the user opted in (config dir exists = opted in).
    let extras: Vec<&Arr> = EXTRA_ARRS
        .iter()
        .filter(|a| {
            install_dir
                .join(format!("configs/{}/config.xml", a.id))
                .exists()
        })
        .collect();
    ensure_decypharr_client_for(install_dir, tx, &extras)
}

fn ensure_decypharr_client_for(
    install_dir: &std::path::Path,
    tx: &std::sync::mpsc::Sender<CareEvent>,
    arrs: &[&Arr],
) -> Result<()> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };
    for arr in arrs {
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
        let clients = api_get(arr.port, &api_key, arr.api, "/downloadclient").unwrap_or_default();
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
                            "http://localhost:{}/api/{}/downloadclient/{id}?forceSave=true",
                            arr.port, arr.api
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
                arr.api,
                "/downloadclient?forceSave=true",
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
    let indexers = api_get(9696, &api_key, "v1", "/indexer").unwrap_or_default();
    if indexers.trim() == "[]" || indexers.trim().is_empty() {
        log("[warn] Prowlarr has no indexers configured yet — add some in its UI first");
        log("       (Shiphand will ask for indexer credentials in a later phase)");
        return Ok(());
    }
    log("[in] asking Prowlarr to re-sync its indexers into Sonarr/Radarr…");
    api_post(
        9696,
        &api_key,
        "v1",
        "/command",
        r#"{"name":"ApplicationIndexerSync"}"#,
    )
    .context("Prowlarr did not accept the sync command")?;
    log("[ok] sync command accepted — indexers are being pushed to the *arrs");
    Ok(())
}

/// Add Lidarr/Sportarr as Prowlarr applications (if opted in).
/// Mirrors the installer's Sonarr/Radarr app payloads.
pub fn ensure_prowlarr_apps(
    install_dir: &std::path::Path,
    tx: &std::sync::mpsc::Sender<CareEvent>,
) -> Result<()> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };
    let prowlarr_key = arr_api_key(install_dir, "prowlarr").map_err(|e| anyhow::anyhow!("{e}"))?;
    let apps = api_get(9696, &prowlarr_key, "v1", "/applications").unwrap_or_default();

    // (display name, implementation, baseUrl, api key, sync categories)
    struct Target {
        name: &'static str,
        impl_: &'static str,
        base_url: &'static str,
        key: String,
        cats: &'static str,
    }
    let mut targets: Vec<Target> = Vec::new();
    if install_dir.join("configs/lidarr/config.xml").exists() {
        if let Ok(key) = arr_api_key(install_dir, "lidarr") {
            // Audio categories: 3000 parent + subs. Keep it to the parent;
            // Prowlarr expands it.
            targets.push(Target {
                name: "Lidarr",
                impl_: "Lidarr",
                base_url: "http://lidarr:8686",
                key,
                cats: "[3000]",
            });
        }
    }
    if install_dir.join("configs/sportarr/config.xml").exists() {
        if let Ok(key) = arr_api_key(install_dir, "sportarr") {
            // Sportarr is Sonarr-API-compatible: register as a Sonarr app.
            // TV/Sports categories.
            targets.push(Target {
                name: "Sportarr",
                impl_: "Sonarr",
                base_url: "http://sportarr:1867",
                key,
                cats: "[5000,5040,5045,5060]",
            });
        }
    }

    for t in &targets {
        if apps.contains(&format!("\"name\":\"{}\"", t.name)) {
            log(&format!("[ok] Prowlarr already has the {} app", t.name));
            continue;
        }
        let body = format!(
            r#"{{"name":"{name}","implementation":"{impl_}","configContract":"{impl_}Settings","syncLevel":"fullSync","fields":[{{"name":"prowlarrUrl","value":"http://prowlarr:9696"}},{{"name":"baseUrl","value":"{base_url}"}},{{"name":"apiKey","value":"{key}"}},{{"name":"syncCategories","value":{cats}}}],"tags":[]}}"#,
            name = t.name,
            impl_ = t.impl_,
            base_url = t.base_url,
            key = t.key.replace('"', "\\\""),
            cats = t.cats,
        );
        match api_post(
            9696,
            &prowlarr_key,
            "v1",
            "/applications?forceSave=true",
            &body,
        ) {
            Ok(_) => log(&format!("[ok] {} app added to Prowlarr", t.name)),
            Err(e) => log(&format!(
                "[warn] could not add {} to Prowlarr: {e:#}",
                t.name
            )),
        }
    }
    Ok(())
}

/// Credentials for the real download clients (local profile).
pub struct LocalClients {
    pub qbit_password: String,
    pub sab_api_key: String,
}

fn qbit_client_json(password: &str) -> String {
    format!(
        r#"{{"name":"qBittorrent","implementation":"QBittorrent","configContract":"QBittorrentSettings","protocol":"torrent","priority":25,"removeCompletedDownloads":true,"removeFailedDownloads":false,"enable":true,"host":"gluetun","port":8080,"useSsl":false,"username":"admin","password":"{pw}","tvCategory":"sonarr","movieCategory":"radarr","musicCategory":"lidarr","contentLayout":"Original","tags":[]}}"#,
        pw = password.replace('"', "\\\""),
    )
}

fn sab_client_json(api_key: &str) -> String {
    format!(
        r#"{{"name":"SABnzbd","implementation":"Sabnzbd","configContract":"SabnzbdSettings","protocol":"usenet","priority":25,"removeCompletedDownloads":true,"removeFailedDownloads":false,"enable":true,"host":"gluetun","port":8081,"urlBase":"","apiKey":"{key}","tvCategory":"sonarr","movieCategory":"radarr","musicCategory":"lidarr","tags":[]}}"#,
        key = api_key.replace('"', "\\\""),
    )
}

/// Ensure qBittorrent + SABnzbd are wired as download clients.
/// Used when the fleet profile needs local (self-downloaded) clients.
/// qBittorrent and SABnzbd share gluetun's network stack, so the *arrs
/// reach them at `gluetun:<port>` on media-network.
pub fn ensure_local_download_clients(
    install_dir: &std::path::Path,
    creds: &LocalClients,
    tx: &std::sync::mpsc::Sender<CareEvent>,
) -> anyhow::Result<()> {
    let log = |s: &str| {
        let _ = tx.send(CareEvent::Log(s.to_string()));
    };
    let base: Vec<&Arr> = ARRS.iter().collect();
    for arr in base {
        let api_key = match arr_api_key(install_dir, arr.id) {
            Ok(k) => k,
            Err(_) => continue, // container not up — skip quietly
        };
        for (name, body) in [
            ("qBittorrent", qbit_client_json(&creds.qbit_password)),
            ("SABnzbd", sab_client_json(&creds.sab_api_key)),
        ] {
            let clients =
                api_get(arr.port, &api_key, arr.api, "/downloadclient").unwrap_or_default();
            if clients.contains(&format!("\"name\":\"{name}\"")) {
                log(&format!("[ok] {}: {name} client already wired", arr.name));
                continue;
            }
            log(&format!(
                "[in] {}: adding {name} download client…",
                arr.name
            ));
            match api_post(arr.port, &api_key, arr.api, "/downloadclient", &body) {
                Ok(_) => log(&format!("[ok] {}: {name} wired", arr.name)),
                Err(e) => log(&format!("[warn] {}: could not add {name}: {e:#}", arr.name)),
            }
        }
    }
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

        // Lidarr uses the music category and API v1.
        let lidarr = &EXTRA_ARRS[0];
        let ljson = decypharr_client_json(lidarr, "key789");
        assert!(ljson.contains(r#""name":"username","value":"http://lidarr:8686""#));
        assert!(ljson.contains(r#""name":"musicCategory","value":"lidarr""#));
        assert_eq!(lidarr.api, "v1");
    }

    #[test]
    fn decypharr_json_escapes_quotes_in_key() {
        let json = decypharr_client_json(&ARRS[0], r#"a"b"#);
        assert!(json.contains(r#""value":"a\"b""#));
    }
}
