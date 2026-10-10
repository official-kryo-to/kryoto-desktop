//! Client settings: where games go, what happens to archives, what opens first.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, Runtime};

pub const DEFAULT_CATALOG_ENDPOINT: &str = "https://kryo.to";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    #[serde(skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub recovery_error: Option<String>,
    /// Where downloaded games are installed, one folder each.
    pub library_dir: String,
    /// Delete the archive once a game is installed from it.
    pub delete_archives: bool,
    /// `store` or `library`.
    pub start_page: String,
    /// Wine or Proton for games that have not picked their own (not Windows).
    pub default_compat_tool: Option<String>,
    /// Minimize the client while a game runs.
    pub minimize_on_play: bool,
    /// Show a notification when a download finishes.
    pub notify_downloads: bool,
    /// kryo.to's palette: `monochrome`, `oled`, `amber`, `emerald`, `nord`, `sepia`, `blossom`.
    pub palette: String,
    /// `account` follows kryo.to; also `system`, `light` or `dark`.
    pub theme: String,
    /// kryo.to's corner setting: `sharp`, `soft`, `rounded`, `round`, `pill`.
    pub radius: String,
    /// `teletext` (the house face) or `mono`.
    pub font: String,
    /// Show adult games' art unblurred. Off by default, as on kryo.to.
    pub show_adult: bool,
    /// Take palette, corners, typeface and the adult blur from the signed-in
    /// kryo.to account instead of the four settings above.
    pub follow_account: bool,
    /// More library folders besides `library_dir` (which is where new games
    /// go). Settings > Storage adds and removes them.
    pub library_folders: Vec<String>,
    /// Send errors and crashes to kryo.to so they get fixed.
    pub send_reports: bool,
    /// The close button hides the window to the tray instead of quitting.
    pub close_to_tray: bool,
    /// Start (to the tray) when you sign in to Windows.
    pub start_with_system: bool,
    /// Buttons press in under the pointer, as on kryo.to.
    pub press_effect: bool,
    /// Add play time to the account on kryo.to (community statistics).
    pub share_playtime: bool,
    /// Parallel connections per download (1 = one stream).
    pub connections: u32,
    /// Download speed cap in MB/s; 0 is no cap.
    pub speed_limit_mb: u32,
    /// Optional local or staging Kryo.to origin for development.
    pub catalog_endpoint: String,
    /// Linux: MangoHud's overlay on every Windows game (when it is installed).
    pub linux_mangohud: bool,
    /// Linux: run Windows games under Feral GameMode (when it is installed).
    pub linux_gamemode: bool,
    /// Linux: Proton-GE's FSR upscaling at lower fullscreen resolutions.
    pub linux_fsr: bool,
    /// Settings written before 0.3 had 8 connections as the default. The first
    /// load after that moves an untouched 8 to the new default of 16, once.
    /// Missing from an old file reads as false (the field's own default, not
    /// the struct's).
    #[serde(default)]
    pub connections_v2: bool,
    /// The name you have in games: your K// username, the one typed below,
    /// or each build's own (player_name.rs). Games can pick their own.
    pub player_name_mode: crate::player_name::Mode,
    pub player_name: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            recovery_error: None,
            library_dir: String::new(),
            delete_archives: true,
            start_page: "library".into(),
            default_compat_tool: None,
            minimize_on_play: false,
            notify_downloads: true,
            palette: "monochrome".into(),
            theme: "account".into(),
            radius: "pill".into(),
            font: "teletext".into(),
            show_adult: false,
            follow_account: true,
            library_folders: Vec::new(),
            send_reports: true,
            close_to_tray: true,
            start_with_system: false,
            press_effect: true,
            share_playtime: true,
            connections: 16,
            speed_limit_mb: 0,
            catalog_endpoint: String::new(),
            linux_mangohud: false,
            linux_gamemode: false,
            linux_fsr: false,
            connections_v2: true,
            player_name_mode: crate::player_name::Mode::Account,
            player_name: String::new(),
        }
    }
}

pub fn normalize_catalog_endpoint(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    let mut url = url::Url::parse(value)
        .map_err(|_| "Use a full endpoint URL, such as http://localhost:3000.".to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err("Use an http(s) site origin only, without a path, query or credentials.".into());
    }
    if url.scheme() == "http" {
        let host = url.host_str().unwrap_or_default();
        let loopback = host == "localhost"
            || host.ends_with(".localhost")
            || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback());
        if !loopback {
            return Err("Use HTTPS, or HTTP on localhost only.".into());
        }
    }
    url.set_path("/");
    Ok(url.as_str().trim_end_matches('/').to_string())
}

pub fn catalog_endpoint(settings: &Settings) -> String {
    normalize_catalog_endpoint(&settings.catalog_endpoint)
        .ok()
        .filter(|endpoint| !endpoint.is_empty())
        .unwrap_or_else(|| DEFAULT_CATALOG_ENDPOINT.into())
}

pub fn is_catalog_origin(url: &url::Url, settings: &Settings) -> bool {
    if !settings.catalog_endpoint.trim().is_empty() {
        let Ok(endpoint) = url::Url::parse(&catalog_endpoint(settings)) else { return false };
        return url.origin() == endpoint.origin();
    }
    let host = url.host_str().unwrap_or("");
    url.scheme() == "https" && (host == "kryo.to" || host.ends_with(".kryo.to"))
}

fn file<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

/// Every library folder, the default (where new games go) first.
pub fn all_folders(s: &Settings) -> Vec<String> {
    let mut out = vec![s.library_dir.clone()];
    for f in &s.library_folders {
        if !out.iter().any(|o| same_path(o, f)) {
            out.push(f.clone());
        }
    }
    out
}

pub fn same_path(a: &str, b: &str) -> bool {
    let norm = |p: &str| {
        let p = p.trim().trim_end_matches(['\\', '/']).replace('\\', "/");
        if cfg!(windows) { p.to_lowercase() } else { p }
    };
    norm(a) == norm(b)
}

/// The settings as last read or written. `load` runs on every Store
/// navigation and page report, which is no reason to go to the disk each time.
static CACHE: std::sync::RwLock<Option<Settings>> = std::sync::RwLock::new(None);

pub fn write<R: Runtime>(app: &AppHandle<R>, settings: &Settings) -> Result<(), String> {
    let target = file(app)?;
    save_settings_file(&target, settings)?;
    if let Ok(mut cache) = CACHE.write() {
        *cache = Some(settings.clone());
    }
    Ok(())
}

fn save_settings_file(target: &std::path::Path, settings: &Settings) -> Result<(), String> {
    read_settings_file(target).map_err(|_| "Your existing settings could not be read. Recover them before saving changes.".to_string())?;
    replace_settings_file(target, settings)
}

fn replace_settings_file(target: &std::path::Path, settings: &Settings) -> Result<(), String> {
    use std::io::Write;
    let tmp = target.with_extension("json.tmp");
    let mut output = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    output.write_all(serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())?;
    output.sync_all().map_err(|e| e.to_string())?;
    drop(output);
    std::fs::rename(tmp, target).map_err(|e| e.to_string())
}

pub fn load<R: Runtime>(app: &AppHandle<R>) -> Settings {
    if let Some(s) = CACHE.read().ok().and_then(|c| c.clone()) {
        return s;
    }
    let s = read(app);
    if let Ok(mut cache) = CACHE.write() {
        *cache = Some(s.clone());
    }
    s
}

fn read<R: Runtime>(app: &AppHandle<R>) -> Settings {
    let mut s = file(app).and_then(|path| read_settings_file(&path)).unwrap_or_else(recovery_defaults);
    if s.library_dir.trim().is_empty() {
        let home = app.path().home_dir().unwrap_or_else(|_| PathBuf::from("."));
        s.library_dir = home.join("Kryoto Games").to_string_lossy().into_owned();
    }
    if !s.connections_v2 && s.recovery_error.is_none() {
        if s.connections == 8 {
            s.connections = 16;
        }
        s.connections_v2 = true;
        let _ = write(app, &s);
    }
    s
}

fn read_settings_file(path: &std::path::Path) -> Result<Settings, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("Settings could not be read: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(format!("Settings could not be read: {e}")),
    }
}

fn recovery_defaults(error: String) -> Settings {
    Settings { send_reports: false, share_playtime: false, recovery_error: Some(error), ..Settings::default() }
}

/// Explicit player action: preserve the damaged file before choosing safe defaults.
#[tauri::command(async)]
pub fn settings_recover(app: AppHandle) -> Result<Settings, String> {
    let current = load(&app);
    if current.recovery_error.is_none() { return Ok(current); }
    let target = file(&app)?;
    let settings = recover_settings_file(&target, current)?;
    if let Ok(mut cache) = CACHE.write() { *cache = Some(settings.clone()); }
    Ok(settings)
}

fn recover_settings_file(target: &std::path::Path, current: Settings) -> Result<Settings, String> {
    if target.exists() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
        let backup = target.with_extension(format!("json.backup-{stamp}"));
        // Keep the original in place until the complete replacement is ready.
        // A failed write or restart must still find the unreadable original,
        // rather than mistake a missing file for a first run with sharing on.
        let mut original = std::fs::File::open(target).map_err(|e| format!("Could not preserve your settings. No defaults were saved: {e}"))?;
        let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(backup)
            .map_err(|e| format!("Could not preserve your settings. No defaults were saved: {e}"))?;
        std::io::copy(&mut original, &mut output).map_err(|e| format!("Could not preserve your settings. No defaults were saved: {e}"))?;
        output.sync_all().map_err(|e| format!("Could not preserve your settings. No defaults were saved: {e}"))?;
    }
    let mut settings = Settings { recovery_error: None, ..current };
    settings.send_reports = false;
    settings.share_playtime = false;
    replace_settings_file(target, &settings)?;
    Ok(settings)
}

#[tauri::command(async)]
pub fn settings_get(app: AppHandle) -> Settings {
    load(&app)
}

#[tauri::command(async)]
pub fn settings_save(app: AppHandle, mut settings: Settings) -> Result<Settings, String> {
    if settings.library_dir.trim().is_empty() {
        return Err("Pick a folder for the library.".into());
    }
    settings.catalog_endpoint = normalize_catalog_endpoint(&settings.catalog_endpoint)?;
    settings.player_name = crate::player_name::clean(&settings.player_name).unwrap_or_default();
    // A choice made in Settings is the player's, never migrated again.
    settings.connections_v2 = true;
    std::fs::create_dir_all(&settings.library_dir)
        .map_err(|e| format!("Cannot use {}: {e}", settings.library_dir))?;
    let before = load(&app);
    write(&app, &settings)?;
    if before.catalog_endpoint != settings.catalog_endpoint {
        crate::catalog_endpoint_changed(&app, &settings)?;
    }
    if before.start_with_system != settings.start_with_system {
        if let Err(e) = crate::system::set_autostart(settings.start_with_system) {
            crate::logging::error("settings", &format!("start with Windows: {e}"));
            return Err(format!("Saved, but starting with Windows could not be turned {}: {e}", if settings.start_with_system { "on" } else { "off" }));
        }
    }
    Ok(settings)
}

#[cfg(test)]
mod tests {
    #[test]
    fn failed_recovery_write_keeps_the_original_and_safe_restart_choices() {
        let dir = std::env::temp_dir().join(format!("kryoto-recovery-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, "damaged settings").unwrap();
        // Block the staging write after the backup succeeds.
        std::fs::create_dir(path.with_extension("json.tmp")).unwrap();
        let current = recovery_defaults(read_settings_file(&path).unwrap_err());
        assert!(recover_settings_file(&path, current).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "damaged settings");
        let restarted = recovery_defaults(read_settings_file(&path).unwrap_err());
        assert!(!restarted.send_reports && !restarted.share_playtime);
        assert!(restarted.recovery_error.is_some());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn damaged_existing_settings_preserve_bytes_and_disable_sharing() {
        let path = std::env::temp_dir().join(format!("kryoto-settings-{}-{}.json", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        assert!(read_settings_file(&path).unwrap().recovery_error.is_none());
        std::fs::write(&path, "{\"sendReports\":false,").unwrap();
        let settings = recovery_defaults(read_settings_file(&path).unwrap_err());
        assert!(settings.recovery_error.is_some());
        assert!(!settings.send_reports && !settings.share_playtime);
        assert!(save_settings_file(&path, &Settings::default()).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"sendReports\":false,");
        let recovered = recover_settings_file(&path, settings).unwrap();
        assert!(recovered.recovery_error.is_none());
        assert!(!recovered.send_reports && !recovered.share_playtime);
        let backup = std::fs::read_dir(path.parent().unwrap()).unwrap().flatten().find(|entry| entry.file_name().to_string_lossy().starts_with(&format!("{}.backup-", path.file_name().unwrap().to_string_lossy()))).unwrap().path();
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "{\"sendReports\":false,");
        std::fs::remove_file(backup).unwrap();
        std::fs::remove_file(path).unwrap();
    }
    use super::{catalog_endpoint, is_catalog_origin, normalize_catalog_endpoint, read_settings_file, recovery_defaults, recover_settings_file, save_settings_file, Settings, DEFAULT_CATALOG_ENDPOINT};

    #[test]
    fn an_old_file_is_moved_to_the_new_connection_default_once() {
        let old: Settings = serde_json::from_str(r#"{"connections":8}"#).unwrap();
        assert!(!old.connections_v2);
        let new: Settings = serde_json::from_str(r#"{"connections":8,"connectionsV2":true}"#).unwrap();
        assert!(new.connections_v2);
    }

    #[test]
    fn custom_endpoint_is_a_safe_origin() {
        assert_eq!(normalize_catalog_endpoint(" http://localhost:3000/ ").unwrap(), "http://localhost:3000");
        assert_eq!(normalize_catalog_endpoint("").unwrap(), "");
        assert!(normalize_catalog_endpoint("http://example.com").is_err());
        assert!(normalize_catalog_endpoint("http://127.0.0.1:3000/path").is_err());
        assert!(normalize_catalog_endpoint("https://user@example.com").is_err());
    }

    #[test]
    fn custom_origin_replaces_the_production_catalog_trust() {
        let settings = Settings { catalog_endpoint: "http://localhost:3000".into(), ..Default::default() };
        assert_eq!(catalog_endpoint(&settings), "http://localhost:3000");
        assert!(is_catalog_origin(&"http://localhost:3000/game/demo".parse().unwrap(), &settings));
        assert!(!is_catalog_origin(&"https://kryo.to/game/demo".parse().unwrap(), &settings));
        assert_eq!(catalog_endpoint(&Settings::default()), DEFAULT_CATALOG_ENDPOINT);
    }
}
