//! Achievements in Kryoto Desktop: the list a game lacks, given before it
//! starts; unlocks noticed while it runs; and the list with your progress for
//! the game page. The work is in `kryoto_repair::achievements`, shared with
//! Forge - this is the app's side of it.
//!
//! An installed gbe_fork build from before October 2026 has no achievement
//! list, so the emulator threw every unlock away. `before_launch` writes one
//! into it the first time it is played, which is what gives the whole existing
//! catalogue achievements without a rebuild.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use kryoto_repair::achievements::{self as ach, Achievement, Roots, Unlock};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::library::{LibraryGame, Running};

const POLL: Duration = Duration::from_secs(2);
/// How long a cached list is trusted before Steam is asked again.
const SCHEMA_FRESH: Duration = Duration::from_secs(7 * 24 * 3600);

fn client() -> reqwest::Client {
    reqwest::Client::builder().user_agent("Kryoto Desktop").build().unwrap_or_default()
}

/// The Steam AppID of an installed game: what the emulator was told, or what
/// Forge recorded on the release.
pub fn appid_of(install_dir: &Path) -> Option<String> {
    let digits = |s: String| {
        let s = s.trim().trim_start_matches('\u{feff}').to_string();
        (!s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) && s != "480").then_some(s)
    };
    for entry in walkdir::WalkDir::new(install_dir).max_depth(6).into_iter().filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name == "steam_appid.txt" {
            if let Some(id) = std::fs::read_to_string(entry.path()).ok().and_then(digits) {
                return Some(id);
            }
        } else if name == ".kryoto-release.json" {
            let v: Option<serde_json::Value> = std::fs::read(entry.path()).ok().and_then(|b| serde_json::from_slice(&b).ok());
            let id = v.and_then(|v| v["steam_appid"].as_u64().map(|n| n.to_string()).or_else(|| v["steam_appid"].as_str().map(str::to_string)));
            if let Some(id) = id.and_then(digits) {
                return Some(id);
            }
        }
    }
    None
}

/// Where this game's emulator saves: the PC's own folders, or the Wine/Proton
/// prefix it runs in.
pub fn roots_for(env: &[(String, String)]) -> Roots {
    let prefix = env.iter().find_map(|(k, v)| match k.as_str() {
        "WINEPREFIX" => Some(PathBuf::from(v)),
        "STEAM_COMPAT_DATA_PATH" => Some(PathBuf::from(v).join("pfx")),
        _ => None,
    });
    let Some(prefix) = prefix else { return Roots::from_env() };
    let users = prefix.join("drive_c").join("users");
    // Proton's user is steamuser; plain Wine uses the login name.
    let user = std::fs::read_dir(&users)
        .ok()
        .and_then(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.path())
                .find(|p| p.join("AppData").join("Roaming").is_dir() && !p.ends_with("Public"))
        });
    Roots {
        app_data: user.map(|u| u.join("AppData").join("Roaming")),
        public_documents: Some(users.join("Public").join("Documents")),
    }
}

/// Before the game starts: give its gbe_fork folders the list they lack.
/// Bounded, because it holds up Play: a slow Steam costs a few seconds once,
/// and the next start finds the file already there.
pub fn before_launch(game: &LibraryGame) {
    let dir = PathBuf::from(&game.install_dir);
    let Some(appid) = appid_of(&dir) else { return };
    let result = tauri::async_runtime::block_on(async {
        tokio::time::timeout(Duration::from_secs(8), ach::ensure_gbe_schema(&client(), &dir, &appid)).await
    });
    match result {
        Ok(Ok(n)) if n > 0 => crate::logging::info("achievements", &format!("{}: achievement list added ({n} folder(s))", game.title)),
        Ok(Err(e)) => crate::logging::warn("achievements", &format!("{}: {e}", game.title)),
        Err(_) => crate::logging::warn("achievements", &format!("{}: Steam was too slow; trying again next start", game.title)),
        _ => {}
    }
}

fn cache_file<R: Runtime>(app: &AppHandle<R>, appid: &str) -> Option<PathBuf> {
    Some(app.path().app_data_dir().ok()?.join("achievements").join(format!("{appid}.json")))
}

/// The game's list, from the app's cache while it is fresh, else Steam (and
/// the stale cache if Steam does not answer).
async fn schema<R: Runtime>(app: &AppHandle<R>, appid: &str) -> Vec<Achievement> {
    let file = cache_file(app, appid);
    let cached: Option<(Vec<Achievement>, bool)> = file.as_ref().and_then(|f| {
        let fresh = std::fs::metadata(f)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < SCHEMA_FRESH);
        let list = serde_json::from_slice(&std::fs::read(f).ok()?).ok()?;
        Some((list, fresh))
    });
    if let Some((list, true)) = &cached {
        return list.clone();
    }
    match ach::fetch_schema(&client(), appid, "english").await {
        Ok(list) => {
            if let Some(f) = &file {
                let _ = std::fs::create_dir_all(f.parent().unwrap_or(Path::new(".")));
                let _ = std::fs::write(f, serde_json::to_vec(&list).unwrap_or_default());
            }
            list
        }
        Err(_) => cached.map(|(l, _)| l).unwrap_or_default(),
    }
}

/// One achievement on the game page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameAchievement {
    #[serde(flatten)]
    pub achievement: Achievement,
    pub unlocked: bool,
    /// Unix seconds.
    pub unlocked_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameAchievements {
    pub appid: Option<String>,
    pub total: usize,
    pub unlocked: usize,
    /// Unlocked first, newest first; then the rest, most common first.
    pub list: Vec<GameAchievement>,
}

/// The list with what is unlocked. Unlocks the list does not name (a game
/// Steam lists none for) still count.
pub fn merge(list: Vec<Achievement>, unlocks: &[Unlock]) -> GameAchievements {
    let mut out: Vec<GameAchievement> = list
        .into_iter()
        .map(|a| {
            let u = unlocks.iter().find(|u| u.name == a.name);
            GameAchievement { unlocked: u.is_some(), unlocked_at: u.and_then(|u| u.time), achievement: a }
        })
        .collect();
    out.sort_by(|a, b| {
        b.unlocked.cmp(&a.unlocked).then_with(|| b.unlocked_at.cmp(&a.unlocked_at)).then_with(|| {
            b.achievement.percent.partial_cmp(&a.achievement.percent).unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    let unlocked = out.iter().filter(|a| a.unlocked).count();
    GameAchievements { appid: None, total: out.len(), unlocked, list: out }
}

#[tauri::command]
pub async fn game_achievements(app: AppHandle, id: String) -> Result<GameAchievements, String> {
    let game = crate::library::load(&app)?.into_iter().find(|g| g.id == id).ok_or("That game is no longer in the library.")?;
    let Some(appid) = appid_of(Path::new(&game.install_dir)) else {
        return Ok(GameAchievements { appid: None, total: 0, unlocked: 0, list: Vec::new() });
    };
    let list = schema(&app, &appid).await;
    // Where the game saves: its Wine/Proton prefix on Linux, the PC's own
    // folders on Windows - the same place the watcher reads while it runs.
    let roots = crate::library::plan_for(&app, &game, None).map(|(plan, _)| roots_for(&plan.env)).unwrap_or_else(|_| Roots::from_env());
    let unlocks = ach::read_unlocked(&roots, &appid);
    let mut out = merge(list, &unlocks);
    out.appid = Some(appid);
    Ok(out)
}

/// While the game runs: every couple of seconds, look at its save files, and
/// announce each achievement that was not unlocked before. Stops when the game
/// does.
pub fn watch<R: Runtime>(app: AppHandle<R>, game: &LibraryGame, roots: Roots) {
    let Some(appid) = appid_of(Path::new(&game.install_dir)) else { return };
    let (id, title) = (game.id.clone(), game.title.clone());
    std::thread::spawn(move || {
        let mut known: HashSet<String> = ach::read_unlocked(&roots, &appid).into_iter().map(|u| u.name).collect();
        let stamp = |roots: &Roots| -> Vec<(PathBuf, Option<SystemTime>)> {
            ach::save_files(roots, &appid).into_iter().map(|p| (p.clone(), std::fs::metadata(&p).and_then(|m| m.modified()).ok())).collect()
        };
        let mut seen = stamp(&roots);
        let mut list: Option<Vec<Achievement>> = None;
        loop {
            std::thread::sleep(POLL);
            let running = app.try_state::<Running>().is_some_and(|r| r.0.lock().map(|m| m.contains_key(&id)).unwrap_or(false));
            let now = stamp(&roots);
            if now != seen {
                seen = now;
                for u in ach::read_unlocked(&roots, &appid) {
                    if !known.insert(u.name.clone()) {
                        continue;
                    }
                    let list = list.get_or_insert_with(|| tauri::async_runtime::block_on(schema(&app, &appid)));
                    let a = list.iter().find(|a| a.name == u.name);
                    let name = a.map(|a| a.display_name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| u.name.clone());
                    let body = a.map(|a| a.description.clone()).filter(|d| !d.is_empty()).unwrap_or_else(|| title.clone());
                    crate::logging::info("achievements", &format!("{title}: unlocked {name}"));
                    let _ = app.emit(
                        "achievement-unlocked",
                        serde_json::json!({ "gameId": id, "name": u.name, "displayName": name, "icon": a.and_then(|a| a.icon.clone()) }),
                    );
                    let _ = app.emit("notify", crate::downloads::Notice::new(&format!("Achievement unlocked: {name}"), &body, Some(id.clone())));
                }
            }
            if !running {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(name: &str, percent: f32) -> Achievement {
        Achievement {
            name: name.into(),
            display_name: name.into(),
            description: String::new(),
            hidden: false,
            icon: None,
            icon_gray: None,
            percent: Some(percent),
        }
    }

    #[test]
    fn unlocked_come_first_newest_first_then_the_most_common() {
        let out = merge(
            vec![a("RARE", 1.0), a("COMMON", 90.0), a("OLD", 50.0), a("NEW", 5.0)],
            &[Unlock { name: "OLD".into(), time: Some(10) }, Unlock { name: "NEW".into(), time: Some(20) }],
        );
        let order: Vec<&str> = out.list.iter().map(|x| x.achievement.name.as_str()).collect();
        assert_eq!(order, ["NEW", "OLD", "COMMON", "RARE"]);
        assert_eq!((out.unlocked, out.total), (2, 4));
    }

    #[test]
    fn the_appid_comes_from_the_emulators_file_but_never_spacewar() {
        let dir = std::env::temp_dir().join(format!("kry-appid-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("bin/steam_settings")).unwrap();
        std::fs::write(dir.join("bin/steam_settings/steam_appid.txt"), "480").unwrap();
        assert_eq!(appid_of(&dir), None);
        std::fs::write(dir.join("bin/steam_appid.txt"), "\u{feff}620\r\n").unwrap();
        assert_eq!(appid_of(&dir).as_deref(), Some("620"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_proton_prefix_is_where_its_saves_are() {
        let r = roots_for(&[("STEAM_COMPAT_DATA_PATH".into(), "/p".into())]);
        assert_eq!(r.public_documents, Some(PathBuf::from("/p/pfx/drive_c/users/Public/Documents")));
    }
}
