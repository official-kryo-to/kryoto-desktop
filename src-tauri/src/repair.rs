//! The same emulator engine as the portable app and Kryoto Forge.
use crate::{
    library::{self, LibraryGame},
    online::LocalOnline,
};
use kryoto_repair::{
    crack::EmuIdentity, detection::GameIdentity, journal, release, repair, settings::Emulator,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Emitter, Manager, Runtime};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LocalRepair {
    pub source: String,
    pub version: String,
    pub previous_online: Option<LocalOnline>,
    pub previous_source: Option<String>,
    pub previous_overrides: bool,
}
#[derive(Clone, Default)]
struct Session {
    game: Option<GameIdentity>,
    source: String,
    logs: Vec<String>,
}
#[derive(Default)]
pub struct Sessions(Mutex<HashMap<String, Session>>);
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    game_id: String,
    text: String,
    percent: Option<u8>,
}

fn game(app: &AppHandle, id: &str) -> Result<LibraryGame, String> {
    library::load(app)?
        .into_iter()
        .find(|g| g.id == id)
        .ok_or("That game is no longer in the library.".into())
}
fn root<R: Runtime>(app: &AppHandle<R>, game: &LibraryGame) -> Result<PathBuf, String> {
    let folders = crate::settings::all_folders(&crate::settings::load(app));
    crate::storage::game_folder(&folders, &game.install_dir)
        .unwrap_or_else(|| PathBuf::from(&game.install_dir))
        .canonicalize()
        .map_err(|_| "The installed game folder cannot be found.".into())
}
fn log(app: &AppHandle, id: &str, text: &str, percent: Option<u8>) {
    if let Some(sessions) = app.try_state::<Sessions>() {
        if let Ok(mut all) = sessions.0.lock() {
            let session = all.entry(id.into()).or_default();
            repair::add_log(&mut session.logs, text, percent);
        }
    }
}
fn progress(app: &AppHandle, id: &str, text: &str, percent: Option<u8>) {
    log(app, id, text, percent);
    let _ = app.emit(
        "repair-progress",
        Progress {
            game_id: id.into(),
            text: text.into(),
            percent,
        },
    );
}
pub fn can_launch(app: &AppHandle, game: &LibraryGame) -> Result<(), String> {
    let root = root(app, game)?;
    journal::check_idle(&root)?;
    if journal::load(&root)?.is_some_and(|r| r.state == "pending") {
        return Err(
            "A repair was interrupted. Undo it under Properties > Repair before starting the game."
                .into(),
        );
    }
    Ok(())
}

#[tauri::command]
pub fn repair_sources() -> Vec<release::Source> {
    release::sources()
}
#[tauri::command]
pub fn repair_state(app: AppHandle, game_id: String) -> Result<serde_json::Value, String> {
    let game = game(&app, &game_id)?;
    let record = journal::load(&root(&app, &game)?)?;
    Ok(
        serde_json::json!({ "undo_available": record.as_ref().is_some_and(|j| j.state != "undone"), "interrupted": record.as_ref().is_some_and(|j| j.state == "pending"), "report": record.as_ref().filter(|j| j.state == "applied").and_then(|j| j.report.as_ref()) }),
    )
}

#[tauri::command]
pub async fn repair_apply(
    app: AppHandle,
    game_id: String,
    source: Emulator,
) -> Result<repair::RepairReport, String> {
    let before = game(&app, &game_id)?;
    if app
        .state::<library::Running>()
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .contains_key(&game_id)
    {
        return Err("Close the game before changing its emulator.".into());
    }
    let root = root(&app, &before)?;
    let slug = before
        .slug
        .as_deref()
        .ok_or("Link this game to its Kryoto page first, under Properties > kryo.to.")?;
    app.state::<Sessions>()
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .insert(
            game_id.clone(),
            Session {
                source: source.label().into(),
                ..Default::default()
            },
        );
    progress(&app, &game_id, "Checking the installed game", None);
    let probe = match repair::probe(&root, Some(slug)).await {
        Ok(probe) => probe,
        Err(error) => {
            progress(&app, &game_id, &error, None);
            return Err(error);
        }
    };
    if let Some(session) = app
        .state::<Sessions>()
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .get_mut(&game_id)
    {
        session.game = Some(probe.game);
        session
            .logs
            .extend(probe.evidence.into_iter().map(|s| format!("Detected: {s}")));
    }
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("repair-tools");
    let settings = crate::settings::load(&app);
    let mut identity = EmuIdentity::default();
    if !settings.player_name.trim().is_empty() {
        identity.account_name = settings.player_name.clone();
    }
    let report = match repair::apply(
        &root,
        &cache,
        Some(slug),
        source,
        &identity,
        |text, percent| progress(&app, &game_id, text, percent),
    )
    .await
    {
        Ok(report) => report,
        Err(error) => {
            progress(&app, &game_id, &error, None);
            return Err(error);
        }
    };
    log(
        &app,
        &game_id,
        &format!(
            "{} {} · SHA256 {} · {} DLLs · {} interface lists",
            report.source,
            report.version,
            report.archive_sha256,
            report.dlls_replaced,
            report.interfaces_generated
        ),
        None,
    );
    let saved = library::update(&app, |games| {
        let current = games
            .iter_mut()
            .find(|g| g.id == game_id)
            .ok_or("The library game disappeared during repair.")?;
        current.repair = Some(LocalRepair {
            source: report.source.clone(),
            version: report.version.clone(),
            previous_online: before.online.clone(),
            previous_source: before.source.clone(),
            previous_overrides: before.apply_overrides,
        });
        current.source = Some(format!("Steam + {}", report.source));
        current.online = if source == Emulator::Online {
            Some(LocalOnline {
                version: report.version.clone(),
                ..Default::default()
            })
        } else {
            None
        };
        current.apply_overrides = true;
        Ok(())
    });
    if let Err(error) = saved {
        journal::undo(&root)
            .map_err(|undo| format!("Library save failed: {error}. Undo failed: {undo}"))?;
        return Err(format!(
            "Library save failed: {error}. The previous game files were restored."
        ));
    }
    let _ = app.emit("library-changed", ());
    Ok(report)
}

#[tauri::command]
pub fn repair_undo(app: AppHandle, game_id: String) -> Result<(), String> {
    let before = game(&app, &game_id)?;
    if app
        .state::<library::Running>()
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .contains_key(&game_id)
    {
        return Err("Close the game before Undo.".into());
    }
    journal::undo(&root(&app, &before)?)?;
    if let Some(previous) = &before.repair {
        library::update(&app, |games| {
            let game = games
                .iter_mut()
                .find(|g| g.id == game_id)
                .ok_or("The library game disappeared.")?;
            game.online = previous.previous_online.clone();
            game.source = previous.previous_source.clone();
            game.apply_overrides = previous.previous_overrides;
            game.repair = None;
            Ok(())
        })?;
    }
    let _ = app.emit("library-changed", ());
    Ok(())
}

#[tauri::command]
pub fn repair_support(app: AppHandle, game_id: String) -> Result<String, String> {
    let game = game(&app, &game_id)?;
    let all = app.state::<Sessions>();
    let sessions = all.0.lock().map_err(|e| e.to_string())?;
    let saved = journal::load(&root(&app, &game)?)?.and_then(|j| j.report);
    let fallback = Session {
        game: saved.as_ref().map(|r| r.game.clone()),
        source: saved.as_ref().map(|r| r.source.clone()).unwrap_or_else(|| game.source.clone().unwrap_or_default()),
        logs: saved.as_ref().map(|r| {
            let mut lines = vec![format!("{} {} · SHA256 {} · {} DLLs · {} interface lists", r.source, r.version, r.archive_sha256, r.dlls_replaced, r.interfaces_generated)];
            lines.extend(r.warnings.clone()); lines
        }).unwrap_or_else(|| vec!["The repair session log is unavailable. Describe what happens when the game starts.".into()]),
    };
    let session = sessions.get(&game_id).unwrap_or(&fallback);
    let text = repair::support_text(session.game.as_ref(), &session.source, &session.logs);
    kryoto_repair::clipboard::copy(&text)?;
    Ok(text)
}
#[tauri::command]
pub fn repair_support_url(app: AppHandle, game_id: String) -> Result<String, String> {
    let game = game(&app, &game_id)?;
    let mut url = url::Url::parse("https://kryo.to/support").map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("new", "1")
        .append_pair("category", "download")
        .append_pair(
            "subject",
            &format!("{} does not work after auto-fixer", game.title),
        );
    if let Some(slug) = &game.slug {
        url.query_pairs_mut().append_pair("game", slug);
    }
    Ok(url.into())
}
