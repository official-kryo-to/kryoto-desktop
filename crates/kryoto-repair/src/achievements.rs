//! Achievements: what a game has (from Steam), what the player has unlocked
//! (from the emulator's save), and the file gbe_fork needs to unlock anything.
//!
//! THE GAP THIS CLOSES. gbe_fork only records an achievement it finds in
//! `steam_settings/achievements.json`. A build without that file runs fine and
//! throws every unlock away - which was every gbe_fork build on kryo.to until
//! October 2026. So:
//!
//!   * Forge writes the file into each new gbe_fork build (`crack_tree`), with
//!     the icons beside it, so the release carries achievements to everyone,
//!     Kryoto Desktop or not.
//!   * Kryoto Desktop writes it into an installed game that lacks it, before
//!     the game starts (`ensure_gbe_schema`), so the existing catalogue gains
//!     achievements without a single rebuild.
//!   * Kryoto Desktop reads the unlocks back from wherever the emulator saved
//!     them (`read_unlocked`), the way Hydra does: every emulator keeps its own
//!     file in a known place, keyed by the Steam AppID.
//!
//! The schema comes from Steam's `IPlayerService/GetGameAchievements`, which
//! needs no key and answers with everything gbe_fork's generator would write:
//! API names, names and descriptions in any language, both icons, the hidden
//! flag and how many players have each one.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const STEAM_API: &str = "https://api.steampowered.com/IPlayerService/GetGameAchievements/v1/";
/// Where an icon file name from that answer lives.
const ICON_BASE: &str = "https://shared.fastly.steamstatic.com/community_assets/images/apps";
/// Inside `steam_settings`.
pub const GBE_FILE: &str = "achievements.json";
pub const GBE_IMAGES: &str = "achievement_images";

/// One achievement a game has.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Achievement {
    /// The API name the game unlocks it by.
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub hidden: bool,
    /// Full icon URLs. `None` when Steam lists none.
    pub icon: Option<String>,
    pub icon_gray: Option<String>,
    /// Share of Steam players who have it, 0-100.
    pub percent: Option<f32>,
}

/// An unlock read from an emulator's save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Unlock {
    pub name: String,
    /// Unix seconds, when the emulator recorded one.
    pub time: Option<u64>,
}

#[derive(Deserialize)]
struct SteamAnswer {
    #[serde(default)]
    response: SteamResponse,
}
#[derive(Deserialize, Default)]
struct SteamResponse {
    #[serde(default)]
    achievements: Vec<SteamAchievement>,
}
#[derive(Deserialize)]
struct SteamAchievement {
    internal_name: String,
    #[serde(default)]
    localized_name: String,
    #[serde(default)]
    localized_desc: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    icon_gray: Option<String>,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    player_percent_unlocked: Option<String>,
}

/// Steam's answer, as achievements. Pure, for tests.
pub fn parse_schema(appid: &str, body: &str) -> Result<Vec<Achievement>> {
    let answer: SteamAnswer =
        serde_json::from_str(body).map_err(|e| Error::Tool(format!("Steam's achievement list did not read: {e}")))?;
    let icon = |f: Option<String>| f.filter(|f| !f.trim().is_empty()).map(|f| format!("{ICON_BASE}/{appid}/{f}"));
    Ok(answer
        .response
        .achievements
        .into_iter()
        .filter(|a| !a.internal_name.trim().is_empty())
        .map(|a| Achievement {
            name: a.internal_name,
            display_name: a.localized_name,
            description: a.localized_desc,
            hidden: a.hidden,
            icon: icon(a.icon),
            icon_gray: icon(a.icon_gray),
            percent: a.player_percent_unlocked.and_then(|p| p.parse().ok()),
        })
        .collect())
}

/// What `appid` has, in `language` (Steam's names: "english", "german", ...).
/// Empty for a game with none.
pub async fn fetch_schema(client: &reqwest::Client, appid: &str, language: &str) -> Result<Vec<Achievement>> {
    if appid.is_empty() || !appid.chars().all(|c| c.is_ascii_digit()) {
        return Err(Error::Config(format!("'{appid}' is not a Steam AppID")));
    }
    let res = client
        .get(STEAM_API)
        .query(&[("appid", appid), ("language", language)])
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| Error::Tool(format!("Steam's achievement list: {e}")))?;
    if !res.status().is_success() {
        return Err(Error::Tool(format!("Steam's achievement list answered {}", res.status())));
    }
    let body = res.text().await.map_err(|e| Error::Tool(format!("Steam's achievement list: {e}")))?;
    parse_schema(appid, &body)
}

/// The icon's file name inside `achievement_images`.
fn image_name(url: &str) -> Option<String> {
    let name = url.rsplit('/').next()?.split(['?', '#']).next()?;
    (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))).then(|| name.to_string())
}

/// gbe_fork's `steam_settings/achievements.json`, in the shape its own
/// generator writes. Icons point into `achievement_images/` when `images`.
pub fn gbe_json(list: &[Achievement], images: bool) -> serde_json::Value {
    serde_json::Value::Array(
        list.iter()
            .map(|a| {
                let img = |u: &Option<String>| {
                    u.as_deref()
                        .filter(|_| images)
                        .and_then(image_name)
                        .map(|n| format!("{GBE_IMAGES}/{n}"))
                        .unwrap_or_default()
                };
                serde_json::json!({
                    "name": a.name,
                    "displayName": a.display_name,
                    "description": a.description,
                    "hidden": if a.hidden { 1 } else { 0 },
                    "icon": img(&a.icon),
                    "icongray": img(&a.icon_gray),
                })
            })
            .collect(),
    )
}

/// Write `achievements.json` (and its icons) into a gbe_fork `steam_settings`
/// folder, unless one is already there - a release that shipped its own, or a
/// game this already ran for, is left exactly as it is.
///
/// Returns every path it created, for the caller's record of changes. Empty
/// when there was nothing to do: the file existed, or the game has no
/// achievements. A failed icon is skipped - the achievement still unlocks,
/// it just shows without a picture.
pub async fn write_gbe_schema(client: &reqwest::Client, settings: &Path, appid: &str) -> Result<Vec<PathBuf>> {
    let file = settings.join(GBE_FILE);
    if file.exists() {
        return Ok(Vec::new());
    }
    let list = fetch_schema(client, appid, "english").await?;
    if list.is_empty() {
        return Ok(Vec::new());
    }
    let mut created = Vec::new();
    let images = settings.join(GBE_IMAGES);
    let had_images = images.exists();
    std::fs::create_dir_all(&images).map_err(|e| Error::Io(format!("creating {}: {e}", images.display())))?;
    if !had_images {
        created.push(images.clone());
    }
    let urls: Vec<String> = list.iter().flat_map(|a| [a.icon.clone(), a.icon_gray.clone()]).flatten().collect();
    use futures_util::StreamExt;
    let fetched: Vec<Option<PathBuf>> = futures_util::stream::iter(urls)
        .map(|url| {
            let images = images.clone();
            async move {
                let name = image_name(&url)?;
                let dest = images.join(&name);
                if dest.exists() {
                    return None;
                }
                let res = client.get(&url).timeout(std::time::Duration::from_secs(20)).send().await.ok()?;
                if !res.status().is_success() {
                    return None;
                }
                let bytes = res.bytes().await.ok()?;
                std::fs::write(&dest, &bytes).ok()?;
                Some(dest)
            }
        })
        .buffer_unordered(8)
        .collect()
        .await;
    if had_images {
        created.extend(fetched.into_iter().flatten());
    }
    let body = serde_json::to_vec_pretty(&gbe_json(&list, true)).map_err(|e| Error::Io(e.to_string()))?;
    std::fs::write(&file, body).map_err(|e| Error::Io(format!("writing {}: {e}", file.display())))?;
    created.push(file);
    Ok(created)
}

/// For an INSTALLED game: give every gbe_fork `steam_settings` folder in it
/// the achievement list it lacks. This is how a build made before Forge wrote
/// the file gets achievements, with nothing re-downloaded. Returns how many
/// folders gained one.
pub async fn ensure_gbe_schema(client: &reqwest::Client, game_dir: &Path, appid: &str) -> Result<usize> {
    let mut done = 0;
    for entry in walkdir::WalkDir::new(game_dir).max_depth(8).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_dir() || entry.file_name() != "steam_settings" {
            continue;
        }
        // gbe_fork's folder, not a RUNE or Online one: it sits beside a
        // steam_api dll and has gbe's own config files.
        let dir = entry.path();
        let gbe = dir.join("steam_appid.txt").exists() || dir.join("configs.app.ini").exists() || dir.join("steam_interfaces.txt").exists();
        if gbe && !write_gbe_schema(client, dir, appid).await?.is_empty() {
            done += 1;
        }
    }
    Ok(done)
}

// ---- reading unlocks ---------------------------------------------------------

/// Where an emulator keeps `appid`'s unlocks, and how to read the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    /// gbe_fork, Goldberg, EMPRESS: `{ "NAME": { "earned": true, "earned_time": n } }`.
    GoldbergJson,
    /// RUNE, CODEX: `[NAME]` sections with `Achieved=1` and `UnlockTime=n`.
    CodexIni,
    /// OnlineFix: `[NAME]` sections with `achieved=true` and `timestamp=n`.
    OnlineFixIni,
}

/// Every place an emulator Kryoto ships (or that a player may have used
/// instead) writes unlocks, for this PC. The roots are the folders Windows
/// gives them - `%APPDATA%`, `%PUBLIC%\Documents` - passed in so tests and a
/// Wine prefix can use their own.
fn locations(roots: &Roots, appid: &str) -> Vec<(PathBuf, Format)> {
    let mut out = Vec::new();
    if let Some(a) = &roots.app_data {
        out.push((a.join("GSE Saves").join(appid).join("achievements.json"), Format::GoldbergJson));
        out.push((a.join("Goldberg SteamEmu Saves").join(appid).join("achievements.json"), Format::GoldbergJson));
        out.push((a.join("EMPRESS").join("remote").join(appid).join("achievements.json"), Format::GoldbergJson));
        out.push((a.join("Steam").join("CODEX").join(appid).join("achievements.ini"), Format::CodexIni));
    }
    if let Some(p) = &roots.public_documents {
        out.push((p.join("Steam").join("RUNE").join(appid).join("achievements.ini"), Format::CodexIni));
        out.push((p.join("Steam").join("CODEX").join(appid).join("achievements.ini"), Format::CodexIni));
        out.push((p.join("OnlineFix").join(appid).join("Stats").join("Achievements.ini"), Format::OnlineFixIni));
        out.push((p.join("OnlineFix").join(appid).join("Achievements.ini"), Format::OnlineFixIni));
    }
    out
}

/// The folders emulators save under.
#[derive(Debug, Clone, Default)]
pub struct Roots {
    pub app_data: Option<PathBuf>,
    pub public_documents: Option<PathBuf>,
}

impl Roots {
    /// This PC's, from the environment Windows sets.
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var_os(k).map(PathBuf::from).filter(|p| p.is_dir());
        Roots { app_data: var("APPDATA"), public_documents: var("PUBLIC").map(|p| p.join("Documents")) }
    }
}

/// The save files that exist for `appid`, newest first. A watcher polls these.
pub fn save_files(roots: &Roots, appid: &str) -> Vec<PathBuf> {
    let mut files: Vec<(PathBuf, std::time::SystemTime)> = locations(roots, appid)
        .into_iter()
        .filter_map(|(p, _)| std::fs::metadata(&p).and_then(|m| m.modified()).ok().map(|t| (p, t)))
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.1));
    files.into_iter().map(|(p, _)| p).collect()
}

/// Everything unlocked for `appid`, across every emulator's save on this PC.
/// The same achievement in two saves counts once, at its earliest time.
pub fn read_unlocked(roots: &Roots, appid: &str) -> Vec<Unlock> {
    let mut all: Vec<Unlock> = Vec::new();
    for (path, format) in locations(roots, appid) {
        let Ok(raw) = std::fs::read(&path) else { continue };
        let text = String::from_utf8_lossy(&raw);
        let text = text.trim_start_matches('\u{feff}');
        let found = match format {
            Format::GoldbergJson => parse_goldberg(text),
            Format::CodexIni => parse_ini(text, |k, v| (k.eq_ignore_ascii_case("Achieved") && v.trim() == "1").then_some(()), "UnlockTime"),
            Format::OnlineFixIni => parse_ini(text, |k, v| (k.eq_ignore_ascii_case("achieved") && v.trim().eq_ignore_ascii_case("true")).then_some(()), "timestamp"),
        };
        for u in found {
            match all.iter_mut().find(|x| x.name == u.name) {
                Some(x) => x.time = match (x.time, u.time) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                },
                None => all.push(u),
            }
        }
    }
    all
}

/// gbe_fork's save: an object keyed by name, or (older Goldberg) an array.
pub fn parse_goldberg(text: &str) -> Vec<Unlock> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return Vec::new() };
    let one = |name: &str, a: &serde_json::Value| -> Option<Unlock> {
        let earned = a["earned"].as_bool().unwrap_or(false) || a["earned"].as_u64() == Some(1);
        earned.then(|| Unlock { name: name.to_string(), time: a["earned_time"].as_u64().filter(|t| *t > 0) })
    };
    match &v {
        serde_json::Value::Object(map) => map.iter().filter_map(|(k, a)| one(k, a)).collect(),
        serde_json::Value::Array(list) => list.iter().filter_map(|a| one(a["name"].as_str()?, a)).collect(),
        _ => Vec::new(),
    }
}

/// An ini of `[NAME]` sections, unlocked when `unlocked(key, value)` says so
/// for one of its lines.
fn parse_ini(text: &str, unlocked: impl Fn(&str, &str) -> Option<()>, time_key: &str) -> Vec<Unlock> {
    let mut out = Vec::new();
    let mut section: Option<(String, bool, Option<u64>)> = None;
    let flush = |s: Option<(String, bool, Option<u64>)>, out: &mut Vec<Unlock>| {
        if let Some((name, true, time)) = s {
            if !name.eq_ignore_ascii_case("SteamAchievements") && !name.eq_ignore_ascii_case("Steam") {
                out.push(Unlock { name, time });
            }
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            flush(section.take(), &mut out);
            section = Some((line[1..line.len() - 1].to_string(), false, None));
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        if let Some(s) = section.as_mut() {
            if unlocked(k.trim(), v).is_some() {
                s.1 = true;
            }
            if k.trim().eq_ignore_ascii_case(time_key) {
                s.2 = v.trim().parse::<u64>().ok().filter(|t| *t > 0);
            }
        }
    }
    flush(section.take(), &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEAM: &str = r#"{"response":{"achievements":[
        {"internal_name":"ACH.WAKE_UP","localized_name":"You Monster","localized_desc":"Reunite with GLaDOS","icon":"WAKE_UP.jpg","icon_gray":"WAKE_UP_BW.jpg","hidden":false,"player_percent_unlocked":"64.7"},
        {"internal_name":"SECRET","localized_name":"Secret","localized_desc":"","icon":"a1b2.jpg","icon_gray":"","hidden":true}
    ]}}"#;

    #[test]
    fn steams_list_becomes_achievements() {
        let list = parse_schema("620", STEAM).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "ACH.WAKE_UP");
        assert_eq!(list[0].icon.as_deref(), Some("https://shared.fastly.steamstatic.com/community_assets/images/apps/620/WAKE_UP.jpg"));
        assert_eq!(list[0].percent, Some(64.7));
        assert!(list[1].hidden && list[1].icon_gray.is_none());
        assert!(parse_schema("620", r#"{"response":{}}"#).unwrap().is_empty());
    }

    #[test]
    fn gbe_gets_the_shape_its_own_generator_writes() {
        let list = parse_schema("620", STEAM).unwrap();
        let v = gbe_json(&list, true);
        assert_eq!(v[0]["name"], "ACH.WAKE_UP");
        assert_eq!(v[0]["displayName"], "You Monster");
        assert_eq!(v[0]["icon"], "achievement_images/WAKE_UP.jpg");
        assert_eq!(v[0]["icongray"], "achievement_images/WAKE_UP_BW.jpg");
        assert_eq!(v[1]["hidden"], 1);
        assert_eq!(v[1]["icongray"], "");
    }

    #[test]
    fn unlocks_are_read_from_every_emulators_save() {
        let root = std::env::temp_dir().join(format!("kry-ach-{}", std::process::id()));
        let app = root.join("AppData");
        let public = root.join("Public");
        std::fs::create_dir_all(app.join("GSE Saves/620")).unwrap();
        std::fs::create_dir_all(public.join("Steam/RUNE/620")).unwrap();
        std::fs::write(
            app.join("GSE Saves/620/achievements.json"),
            r#"{"ACH.WAKE_UP":{"earned":true,"earned_time":1700000000},"LOCKED":{"earned":false,"earned_time":0}}"#,
        )
        .unwrap();
        std::fs::write(
            public.join("Steam/RUNE/620/achievements.ini"),
            "\u{feff}[SteamAchievements]\nCount=2\n[ACH.WAKE_UP]\nAchieved=1\nUnlockTime=1600000000\n[ACH.LASER]\nAchieved=1\nUnlockTime=1650000000\n[NOPE]\nAchieved=0\n",
        )
        .unwrap();
        let roots = Roots { app_data: Some(app), public_documents: Some(public) };
        let mut got = read_unlocked(&roots, "620");
        got.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(
            got,
            vec![
                Unlock { name: "ACH.LASER".into(), time: Some(1650000000) },
                // Two saves, one achievement: the earlier time.
                Unlock { name: "ACH.WAKE_UP".into(), time: Some(1600000000) },
            ]
        );
        assert_eq!(save_files(&roots, "620").len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Against Steam: `cargo test -p kryoto-repair live_schema -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_schema_is_written_for_gbe() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = std::env::temp_dir().join(format!("kry-gbe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let client = reqwest::Client::builder().user_agent("kryoto").build().unwrap();
        let made = rt.block_on(write_gbe_schema(&client, &dir, "620")).unwrap();
        let list: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join(GBE_FILE)).unwrap()).unwrap();
        let images = std::fs::read_dir(dir.join(GBE_IMAGES)).unwrap().count();
        println!("{} achievements, {images} icons, {} paths recorded", list.as_array().unwrap().len(), made.len());
        assert_eq!(list.as_array().unwrap().len(), 51);
        assert!(images >= 51);
        // A second run leaves it alone.
        assert!(rt.block_on(write_gbe_schema(&client, &dir, "620")).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn older_goldbergs_array_and_onlinefix_read_too() {
        assert_eq!(parse_goldberg(r#"[{"name":"A","earned":true,"earned_time":5}]"#), vec![Unlock { name: "A".into(), time: Some(5) }]);
        let of = parse_ini("[A]\nachieved=true\ntimestamp=9\n[B]\nachieved=false\n", |k, v| (k.eq_ignore_ascii_case("achieved") && v.trim() == "true").then_some(()), "timestamp");
        assert_eq!(of, vec![Unlock { name: "A".into(), time: Some(9) }]);
    }
}
