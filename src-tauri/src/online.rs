//! Kryoto Online, set up on this PC - the same thing Kryoto Forge does to a
//! build, done to an installed game, and undone again.
//!
//! Beside every `steam_api(64).dll` in the game: Kryoto Online's proxy takes
//! the dll's place (the original is kept next to it), its core (`kryotoO.dll`,
//! or `kryotoO32.dll` for 32-bit) goes beside it, and `kryoto-online.ini` is
//! written next to each exe and dll, telling it which game this is. A shipped
//! `steam_appid.txt` is set aside, as Forge does - it would undo the spoof.
//!
//! Undo deletes what was added and puts every original back, so the game is
//! exactly as it was. The files come from Kryoto Online's own releases on
//! GitHub, fetched once and kept in the app's tools folder.

use crate::library::{self, LibraryGame};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, Runtime};


/// What the game is told it is: Spacewar, which every Steam account owns.
#[cfg(test)]
const SPACEWAR_APPID: &str = "480";
#[cfg(test)]
const BACKUP_EXT: &str = "kryoto-original";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LocalOnline {
    /// Kryoto Online's release, e.g. "v1.8.1".
    pub version: String,
    /// Files added, relative to the game's folder; deleted on undo.
    pub added: Vec<String>,
    /// Files replaced or set aside: (the file, its saved original).
    pub saved: Vec<(String, String)>,
    pub applied_at: u64,
}

#[cfg(test)]
fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

/// Every `steam_api.dll` / `steam_api64.dll` in the game, not under plugins.
#[cfg(test)]
fn steam_dlls(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        for e in read.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            if p.is_dir() {
                if depth < 8 && name != "plugins" {
                    stack.push((p, depth + 1));
                }
            } else if name == "steam_api64.dll" || name == "steam_api.dll" {
                out.push(p);
            }
        }
    }
    out
}

/// Folders an exe lives in, plus each dll's folder and the root: where the
/// proxy might look for its ini, since there is no telling which exe runs.
#[cfg(test)]
fn config_dirs(root: &Path, dlls: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = vec![root.to_path_buf()];
    let mut stack = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        for e in read.flatten() {
            let p = e.path();
            if p.is_dir() {
                if depth < 8 {
                    stack.push((p, depth + 1));
                }
            } else if e.file_name().to_string_lossy().to_ascii_lowercase().ends_with(".exe") {
                dirs.push(dir.clone());
            }
        }
    }
    dirs.extend(dlls.iter().filter_map(|d| d.parent().map(Path::to_path_buf)));
    dirs.sort();
    dirs.dedup();
    dirs
}

/// The DLC the release already unlocks: Forge writes them into gbe_fork's
/// `steam_settings/configs.app.ini` beside each dll (`[app::dlcs]`,
/// `<appid>=<name>`). kryo.to only keeps DLC names, so this is the one place
/// the ids Forge used are on the player's machine.
#[cfg(test)]
fn release_dlc(dlls: &[PathBuf]) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for dll in dlls {
        let Some(dir) = dll.parent() else { continue };
        let Ok(text) = std::fs::read_to_string(dir.join("steam_settings").join("configs.app.ini")) else { continue };
        let mut in_dlcs = false;
        for line in text.lines().map(str::trim) {
            if line.starts_with('[') {
                in_dlcs = line.eq_ignore_ascii_case("[app::dlcs]");
            } else if in_dlcs {
                if let Some((key, _)) = line.split_once('=') {
                    let key = key.trim();
                    if !key.is_empty() && key.chars().all(|c| c.is_ascii_digit()) && !ids.iter().any(|i| i == key) {
                        ids.push(key.to_string());
                    }
                }
            }
        }
    }
    ids
}

#[cfg(test)]
pub fn ini(appid: &str, dlc: &[String]) -> String {
    let unlock = dlc.join(",");
    format!(
        "; Written by Kryoto Desktop. Delete to fall back to defaults.\n\
         [Settings]\n\
         AppId={SPACEWAR_APPID}\n\
         ogAppId={appid}\n\
         PluginsFolder=plugins\n\
         GetStubbedLol=true\n\
         UnlockDLC={unlock}\n\
         EmulateTicket=true\n"
    )
}

fn game_root<R: Runtime>(app: &AppHandle<R>, game: &LibraryGame) -> PathBuf {
    let folders = crate::settings::all_folders(&crate::settings::load(app));
    crate::storage::game_folder(&folders, &game.install_dir).unwrap_or_else(|| PathBuf::from(&game.install_dir))
}

/// Put `from` at `to`, first saving whatever is at `to` beside it.
#[cfg(test)]
fn place(root: &Path, from: &Path, to: &Path, rec: &mut LocalOnline) -> Result<(), String> {
    if to.exists() {
        let backup = PathBuf::from(format!("{}.{BACKUP_EXT}", to.display()));
        if !backup.exists() {
            std::fs::rename(to, &backup).map_err(|e| format!("Could not set aside {}: {e}", to.display()))?;
        }
        rec.saved.push((rel(root, to), rel(root, &backup)));
    } else {
        rec.added.push(rel(root, to));
    }
    std::fs::copy(from, to).map_err(|e| format!("Could not copy {}: {e}", to.display()))?;
    Ok(())
}

/// Why a game has no use for Kryoto Online, or `None` when it does.
///
/// It is for games Steam says play online, whose release does not already
/// bring its own way online: kryo.to marks those releases multiplayer, and
/// older ones say so in their source (OFME, Online-Fix, Kryoto Online).
pub fn online_unneeded(game: &serde_json::Value, local_source: Option<&str>) -> Option<&'static str> {
    if game["multiplayer"].as_bool() == Some(true) {
        return Some("This release already plays online.");
    }
    let has_fix = |s: &str| {
        let s = s.to_ascii_lowercase();
        s.contains("ofme")
            || s.contains("online-fix")
            || s.contains("onlinefix")
            || s.contains("online fix")
            || s.contains("kryoto online")
            || s.contains("kryotoo")
    };
    if [game["source"].as_str(), local_source].into_iter().flatten().any(has_fix) {
        return Some("This release already plays online.");
    }
    let features: Vec<&str> = game["features"].as_array().into_iter().flatten().filter_map(|f| f.as_str()).collect();
    let has = |name: &str| features.iter().any(|f| f.eq_ignore_ascii_case(name));
    let online = has("Online Co-op")
        || has("Online PvP")
        || has("MMO")
        || has("Cross-Platform Multiplayer")
        // Steam's plain "Multi-player" with no split screen listed is online.
        || (has("Multi-player") && !features.iter().any(|f| f.to_ascii_lowercase().contains("split screen")));
    if !online {
        return Some("Steam does not list online play for this game.");
    }
    None
}

async fn catalog_game<R: tauri::Runtime>(app: &AppHandle<R>, client: &reqwest::Client, slug: &str) -> Result<serde_json::Value, String> {
    let endpoint = crate::settings::catalog_endpoint(&crate::settings::load(app));
    let res = client.get(format!("{endpoint}/api/games/{slug}")).send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("kryo.to answered {}.", res.status().as_u16()));
    }
    let json = res.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
    Ok(json["game"].clone())
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("KryotoDesktop/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())
}

/// Whether the game page offers Kryoto Online: `None` when it does, else why not.
#[tauri::command]
pub async fn online_check(app: AppHandle, game_id: String) -> Result<Option<String>, String> {
    let game = library::load(&app)?.into_iter().find(|g| g.id == game_id).ok_or("That game is no longer in the library.")?;
    let Some(slug) = game.slug.clone() else { return Ok(Some("Not linked to a kryo.to page.".into())) };
    let catalog = catalog_game(&app, &client()?, &slug).await?;
    Ok(online_unneeded(&catalog, game.source.as_deref()).map(String::from))
}

#[tauri::command]
pub async fn online_apply(app: AppHandle, game_id: String) -> Result<LibraryGame, String> {
    crate::repair::repair_apply(app.clone(), game_id.clone(), kryoto_repair::settings::Emulator::Online).await?;
    library::load(&app)?.into_iter().find(|g| g.id == game_id).ok_or("That game is no longer in the library.".into())
}
fn undo_files(root: &Path, rec: &LocalOnline) {
    for f in &rec.added {
        if let Ok(p) = crate::launch::inside(root, f) {
            if p.is_dir() {
                // Only if still empty: plugins a player put in stay.
                let _ = std::fs::remove_dir(p);
            } else {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    for (file, backup) in &rec.saved {
        if let (Ok(file), Ok(backup)) = (crate::launch::inside(root, file), crate::launch::inside(root, backup)) {
            if backup.exists() {
                let _ = std::fs::remove_file(&file);
                let _ = std::fs::rename(&backup, &file);
            }
        }
    }
}

#[tauri::command(async)]
pub fn online_undo(app: AppHandle, game_id: String) -> Result<LibraryGame, String> {
    let game = library::load(&app)?.into_iter().find(|g| g.id == game_id).ok_or("That game is no longer in the library.")?;
    if game.repair.is_some() {
        crate::repair::repair_undo(app.clone(), game_id.clone())?;
        return library::load(&app)?.into_iter().find(|g| g.id == game_id).ok_or("That game is no longer in the library.".into());
    }
    if app.state::<library::Running>().0.lock().map_err(|e| e.to_string())?.contains_key(&game_id) { return Err("Close the game before Undo.".into()) }
    let rec = game.online.clone().ok_or("Kryoto Online is not set up for this game.")?;
    undo_files(&game_root(&app, &game), &rec);
    crate::logging::info("online", &format!("Kryoto Online removed from {}", game.title));
    let updated = library::update(&app, |games| {
        let g = games.iter_mut().find(|g| g.id == game_id).ok_or("That game is no longer in the library.")?;
        g.online = None;
        Ok(g.clone())
    })?;
    let _ = app.emit("library-changed", ());
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn online_is_offered_only_where_it_is_missing() {
        let g = |v: serde_json::Value| v;
        let online = serde_json::json!(["Single-player", "Multi-player", "Online Co-op"]);
        // Steam lists online play, the release has no way online: offered.
        assert_eq!(online_unneeded(&g(serde_json::json!({"features": online, "source": "Steam + gbe_fork"})), None), None);
        // Marked multiplayer on kryo.to: the release brings its own.
        assert!(online_unneeded(&g(serde_json::json!({"features": online, "multiplayer": true})), None).is_some());
        // Older releases only say so in their source.
        for source in ["OFME", "Steam + Online-Fix", "onlinefix", "Steam + Kryoto Online", "KryotoO"] {
            assert!(online_unneeded(&g(serde_json::json!({"features": online, "source": source})), None).is_some(), "{source}");
        }
        assert!(online_unneeded(&g(serde_json::json!({"features": online})), Some("OFME")).is_some());
        // Single-player, or split screen only: nothing to play online.
        assert!(online_unneeded(&g(serde_json::json!({"features": ["Single-player"]})), None).is_some());
        assert!(online_unneeded(&g(serde_json::json!({"features": ["Multi-player", "Shared/Split Screen Co-op"]})), None).is_some());
        assert_eq!(online_unneeded(&g(serde_json::json!({"features": ["Multi-player"]})), None), None);
        assert!(online_unneeded(&g(serde_json::json!({})), None).is_some());
    }

    #[test]
    fn set_up_then_undo_leaves_the_game_as_it_was() {
        let base = std::env::temp_dir().join("kryoto-online-test");
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("Game");
        let tools = base.join("tools");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("bin/steam_api64.dll"), "valve").unwrap();
        std::fs::write(root.join("bin/steam_appid.txt"), "12345").unwrap();
        std::fs::write(root.join("bin/Game.exe"), "exe").unwrap();
        std::fs::create_dir_all(tools.join("x64")).unwrap();
        std::fs::write(tools.join("x64/steam_api64.dll"), "proxy").unwrap();
        std::fs::write(tools.join("x64/kryotoO.dll"), "core").unwrap();

        let dll = root.join("bin/steam_api64.dll");
        assert_eq!(steam_dlls(&root), vec![dll.clone()]);
        let mut rec = LocalOnline::default();
        place(&root, &tools.join("x64/steam_api64.dll"), &dll, &mut rec).unwrap();
        place(&root, &tools.join("x64/kryotoO.dll"), &root.join("bin/kryotoO.dll"), &mut rec).unwrap();
        std::fs::rename(root.join("bin/steam_appid.txt"), root.join("bin/steam_appid.txt.kryoto-original")).unwrap();
        rec.saved.push(("bin/steam_appid.txt".into(), "bin/steam_appid.txt.kryoto-original".into()));
        assert_eq!(std::fs::read_to_string(&dll).unwrap(), "proxy");
        assert!(config_dirs(&root, std::slice::from_ref(&dll)).contains(&root.join("bin")));

        undo_files(&root, &rec);
        assert_eq!(std::fs::read_to_string(&dll).unwrap(), "valve");
        assert_eq!(std::fs::read_to_string(root.join("bin/steam_appid.txt")).unwrap(), "12345");
        assert!(!root.join("bin/kryotoO.dll").exists());
        assert!(!root.join("bin/steam_api64.dll.kryoto-original").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_ini_spoofs_spacewar_and_names_the_real_game() {
        let t = ini("1190600", &["1190610".into(), "1190611".into()]);
        assert!(t.contains("AppId=480\n") && t.contains("ogAppId=1190600\n") && t.contains("EmulateTicket=true"));
        assert!(t.contains("UnlockDLC=1190610,1190611\n"));
    }

    #[test]
    fn dlc_comes_from_the_release_forge_built() {
        let base = std::env::temp_dir().join(format!("kryoto-dlc-{}", std::process::id()));
        let settings = base.join("bin/steam_settings");
        std::fs::create_dir_all(&settings).unwrap();
        std::fs::write(
            settings.join("configs.app.ini"),
            "[app::general]\nbuild_id=1\n[app::dlcs]\nunlock_all=1\n2001=Soundtrack\n2002 = Artbook\n\n[app::other]\n9=x\n",
        )
        .unwrap();
        assert_eq!(release_dlc(&[base.join("bin/steam_api64.dll")]), vec!["2001".to_string(), "2002".to_string()]);
        assert!(release_dlc(&[base.join("nowhere/steam_api64.dll")]).is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }
}
