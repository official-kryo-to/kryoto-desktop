//! Kryoto Desktop, run from the website (kryo.to `lib/desktop-remote.ts`).
//!
//! A game queued on kryo.to ("Download in Kryoto Desktop") starts here, and
//! every download's progress shows on kryo.to's Downloads page, from any
//! device. The Store view's page script does the talking, signed in like any
//! browser (BROWSER_STATE_SCRIPT in lib.rs): it takes what kryo.to queued and
//! hands it to `remote_apply`, and sends `remote_snapshot` back while
//! something moves. Only a kryo.to page in the Store may call either.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State, Webview};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::io::Write;

use crate::downloads::{self, Downloads};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteItem {
    id: String,
    slug: Option<String>,
    title: String,
    status: String,
    received: u64,
    total: Option<u64>,
    speed: u64,
    extracted: u64,
    extract_total: Option<u64>,
    error: Option<String>,
    queue_order: u64,
    added_at: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    protocol: u8,
    install_id: String,
    name: String,
    version: String,
    os: String,
    downloads: Vec<RemoteItem>,
}

fn from_store(app: &AppHandle, webview: &Webview) -> Result<(), String> {
    if webview.label() != crate::STORE {
        return Err("Only the Store may ask.".into());
    }
    let page = webview.url().map_err(|e| e.to_string())?;
    if !crate::settings::is_catalog_origin(&page, &crate::settings::load(app)) {
        return Err("Only kryo.to may ask.".into());
    }
    Ok(())
}

/// What the downloads are doing, for kryo.to's Downloads page. Finished ones
/// are left out after a day, so the list is about now.
#[tauri::command]
pub fn remote_snapshot(app: AppHandle, webview: Webview, state: State<'_, Downloads>) -> Result<Snapshot, String> {
    from_store(&app, &webview)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let downloads = downloads::downloads_list(state)
        .into_iter()
        .filter(|d| d.finished_at.is_none_or(|f| now.saturating_sub(f) < 86_400))
        .take(60)
        .map(|d| RemoteItem {
            title: if d.meta.title.is_empty() { downloads::title_from_file(&d.file_name) } else { d.meta.title.clone() },
            status: serde_json::to_value(d.status).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default(),
            id: d.id,
            slug: d.slug,
            received: d.received,
            total: d.total,
            speed: d.speed,
            extracted: d.extracted,
            extract_total: d.extract_total,
            error: d.error,
            queue_order: d.queue_order,
            added_at: d.added_at,
        })
        .collect();
    Ok(Snapshot {
        protocol: 2,
        install_id: crate::logging::install_id(&app),
        name: sysinfo::System::host_name().unwrap_or_else(|| "Kryoto Desktop".into()),
        version: app.package_info().version.to_string(),
        os: format!("{} {}", std::env::consts::OS, sysinfo::System::os_version().unwrap_or_default()).trim().to_string(),
        downloads,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCommand {
    id: String,
    kind: String,
    url: Option<String>,
    slug: Option<String>,
    title: Option<String>,
    download_id: Option<String>,
}

/// Do what kryo.to queued: start a download (from kryo.to's own filehost only),
/// or pause, resume or cancel one. Confirm each durable acceptance separately.
#[tauri::command]
pub fn remote_apply(app: AppHandle, webview: Webview, state: State<'_, Downloads>, commands: Vec<RemoteCommand>) -> Result<Vec<RemoteResult>, String> {
    from_store(&app, &webview)?;
    let _guard = APPLY.lock().map_err(|_| "remote command lock poisoned")?;
    let settings = crate::settings::load(&app);
    let file = app.path().app_data_dir().map_err(|e| e.to_string())?.join("remote-receipts.json");
    let mut receipts = read_receipts(&file)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
    receipts.retain(|_, at| now.saturating_sub(*at) < 30 * 86400);
    let mut results = Vec::new();
    for c in commands.into_iter().take(20) {
        let id = c.id.clone();
        if id.is_empty() || id.len() > 19 || !id.bytes().all(|b| b.is_ascii_digit()) {
            results.push(RemoteResult { id, error: Some("Invalid remote command ID.".into()), retryable: false });
            continue;
        }
        let accepted = accept_once(&file, &mut receipts, &id, now, || -> Result<(), String> { match c.kind.as_str() {
            "download" => {
                let url = c.url.as_deref().and_then(|u| url::Url::parse(u).ok()).ok_or("Invalid download URL.")?;
                if !downloads::is_ours(&url, &settings) {
                    return Err("This download does not come from Kryoto.".into());
                }
                let slug = c.slug.filter(|s| !s.is_empty() && s.len() <= 160 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'));
                let title = c.title.map(|t| t.chars().filter(|ch| !ch.is_control()).take(200).collect::<String>()).filter(|t| !t.is_empty());
                crate::logging::info("remote", &format!("queued from kryo.to: {}", title.as_deref().unwrap_or("a game")));
                downloads::enqueue_remote(&app, &id, url.to_string(), slug, title)?;
            }
            "pause" | "resume" | "cancel" => {
                let download_id = c.download_id.filter(|i| !i.is_empty() && i.len() <= 80).ok_or("Invalid download ID.")?;
                let current = downloads::snapshot(&app, &download_id).ok_or("That download is no longer in the list.")?;
                if current.status == downloads::Status::Installed {
                    return Err("That download has already finished transferring.".into());
                }
                match c.kind.as_str() {
                    "pause" => {
                        downloads::download_pause(app.clone(), state.clone(), download_id.clone());
                        downloads::remote_control(&app, &download_id, downloads::Status::Paused)?;
                    }
                    "resume" => {
                        downloads::download_resume(app.clone(), download_id)?;
                        downloads::persist_checked(&app)?;
                    }
                    _ => {
                        downloads::download_cancel(app.clone(), state.clone(), download_id.clone());
                        downloads::remote_control(&app, &download_id, downloads::Status::Canceled)?;
                    }
                }
            }
            _ => return Err("Unknown remote action.".into()),
        } Ok(()) });
        let error = accepted.err();
        if let Some(e) = &error {
            receipts.remove(&id);
            crate::logging::error("remote", e);
        }
        let retryable = error.as_deref().is_none_or(retryable_error);
        results.push(RemoteResult { id, error, retryable });
    }
    Ok(results)
}

static APPLY: Mutex<()> = Mutex::new(());

fn retryable_error(error: &str) -> bool {
    !matches!(error, "Invalid remote command ID." | "Invalid download URL." | "Invalid download ID." |
        "This download does not come from Kryoto." | "That download is no longer in the list." |
        "That download has already finished transferring." | "Unknown remote action.")
}

fn read_receipts(file: &std::path::Path) -> Result<BTreeMap<String, u64>, String> {
    match std::fs::read_to_string(file) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("Remote receipts could not be read: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(format!("Remote receipts could not be read: {e}")),
    }
}

fn accept_once(file: &std::path::Path, receipts: &mut BTreeMap<String, u64>, id: &str, now: u64, apply: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    if receipts.contains_key(id) { return Ok(()); }
    apply()?;
    receipts.insert(id.into(), now);
    let saved = (|| -> Result<(), String> {
        let tmp = file.with_extension("json.tmp");
        let mut output = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        output.write_all(&serde_json::to_vec(receipts).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        output.sync_all().map_err(|e| e.to_string())?;
        drop(output);
        std::fs::rename(tmp, file).map_err(|e| e.to_string())
    })();
    if let Err(e) = saved {
        receipts.remove(id);
        return Err(format!("Could not save remote receipt: {e}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_targets_are_permanent_but_persistence_and_settling_are_retryable() {
        assert!(!retryable_error("That download is no longer in the list."));
        assert!(!retryable_error("This download does not come from Kryoto."));
        assert!(retryable_error("Could not save remote receipt: access denied"));
        assert!(retryable_error("Wait for the download to stop, then try again."));
    }

    #[test]
    fn durable_receipt_recognizes_a_retry_after_restart() {
        let dir = std::env::temp_dir().join(format!("kryoto-receipts-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&dir).unwrap();
        let file = dir.join("receipts.json");
        let mut receipts = read_receipts(&file).unwrap();
        let mut applied = 0;
        accept_once(&file, &mut receipts, "123", 1, || { applied += 1; Ok(()) }).unwrap();
        let mut restarted = read_receipts(&file).unwrap();
        accept_once(&file, &mut restarted, "123", 2, || { applied += 1; Ok(()) }).unwrap();
        assert_eq!(applied, 1);
        assert_eq!(restarted.get("123"), Some(&1));
        assert!(accept_once(&file, &mut restarted, "124", 3, || Err("native rejection".into())).is_err());
        assert!(!read_receipts(&file).unwrap().contains_key("124"));
        let unwritable = dir.join("missing-parent/receipts.json");
        assert!(accept_once(&unwritable, &mut restarted, "125", 4, || Ok(())).is_err());
        assert!(!restarted.contains_key("125"));
        std::fs::write(&file, "damaged receipt").unwrap();
        assert!(read_receipts(&file).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[derive(Serialize)]
pub struct RemoteResult {
    id: String,
    error: Option<String>,
    retryable: bool,
}
