use crate::{
    crack::{self, EmuIdentity},
    detection::{self, Evidence, GameIdentity},
    hubcap::Dlc,
    journal::{self, Journal},
    release,
    settings::Emulator,
    Cancel,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize)]
pub struct Probe {
    pub game: GameIdentity,
    pub evidence: Vec<String>,
    pub undo_available: bool,
    pub interrupted: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairReport {
    pub game: GameIdentity,
    pub source: String,
    pub version: String,
    pub archive_sha256: String,
    pub dlls_replaced: usize,
    pub interfaces_generated: usize,
    pub warnings: Vec<String>,
}

pub async fn identify(
    client: &reqwest::Client,
    root: &Path,
    trusted_slug: Option<&str>,
) -> Result<(GameIdentity, Evidence), String> {
    let evidence = detection::scan(root)?;
    let slug = trusted_slug.or_else(|| evidence.slugs.first().map(String::as_str));
    let game = if let Some(slug) = slug {
        if !detection::safe_slug(slug) {
            return Err("Invalid Kryoto game link.".into());
        }
        let json: serde_json::Value = client
            .get(format!("https://kryo.to/api/games/{slug}"))
            .send()
            .await
            .map_err(|e| format!("Could not verify this game on Kryoto: {e}"))?
            .error_for_status()
            .map_err(|_| "This game is not available on Kryoto.")?
            .json()
            .await
            .map_err(|_| "Kryoto's game details were unreadable.")?;
        serde_json::from_value::<GameIdentity>(json["game"].clone())
            .map_err(|_| "Kryoto has no valid Steam identity for this game.")?
    } else if let Some(id) = evidence.appids.first() {
        let json: serde_json::Value = client
            .get("https://kryo.to/api/games/resolve")
            .query(&[("appid", id)])
            .send()
            .await
            .map_err(|e| format!("Could not verify this game on Kryoto: {e}"))?
            .error_for_status()
            .map_err(|_| "Kryoto could not resolve this Steam id.")?
            .json()
            .await
            .map_err(|_| "Kryoto's answer was unreadable.")?;
        let games = json["games"]
            .as_array()
            .ok_or("Kryoto's game lookup was unreadable.")?;
        if games.len() != 1 {
            return Err(
                "This Steam id does not identify exactly one available Kryoto game.".into(),
            );
        }
        serde_json::from_value(games[0].clone())
            .map_err(|_| "Kryoto has no valid Steam identity for this game.")?
    } else {
        let json: serde_json::Value = client
            .get("https://kryo.to/api/games")
            .query(&[("q", evidence.title_hint.as_str()), ("limit", "100")])
            .send()
            .await
            .map_err(|e| format!("Could not search Kryoto: {e}"))?
            .error_for_status()
            .map_err(|_| "Kryoto's game search is unavailable.")?
            .json()
            .await
            .map_err(|_| "Kryoto's search answer was unreadable.")?;
        let games = json["games"]
            .as_array()
            .ok_or("Kryoto's game lookup was unreadable.")?;
        let matches: Vec<_> = games
            .iter()
            .filter(|g| {
                g["title"].as_str().is_some_and(|title| {
                    detection::normalize_title(title)
                        == detection::normalize_title(&evidence.title_hint)
                })
            })
            .collect();
        if matches.len() != 1 {
            return Err("The folder name does not identify exactly one Kryoto game. Keep the game configuration or original DLL backups and try again.".into());
        }
        serde_json::from_value((*matches[0]).clone())
            .map_err(|_| "Kryoto has no valid Steam identity for this game.")?
    };
    detection::verify_identity(&evidence, &game, trusted_slug)?;
    Ok((game, evidence))
}

pub async fn probe(root: &Path, trusted_slug: Option<&str>) -> Result<Probe, String> {
    let (game, evidence) = identify(&release::client()?, root, trusted_slug).await?;
    let record = journal::load(root)?;
    Ok(Probe {
        game,
        evidence: evidence.signals.into_iter().collect(),
        undo_available: record.as_ref().is_some_and(|j| j.state != "undone"),
        interrupted: record.as_ref().is_some_and(|j| j.state == "pending"),
    })
}

fn dlcs(evidence: &Evidence) -> Vec<Dlc> {
    let mut ids = BTreeMap::new();
    for dll in &evidence.dlls {
        if let Some(text) = dll
            .parent()
            .and_then(|p| detection::read_small(&p.join("steam_settings/configs.app.ini")))
        {
            let mut in_dlcs = false;
            for line in text.lines().map(str::trim) {
                if line.starts_with('[') {
                    in_dlcs = line.eq_ignore_ascii_case("[app::dlcs]");
                } else if in_dlcs {
                    if let Some((key, name)) = line.split_once('=') {
                        if let Some(id) = detection::appid(key) {
                            ids.insert(id, name.trim().to_string());
                        }
                    }
                }
            }
        }
    }
    ids.into_iter()
        .map(|(appid, name)| Dlc { appid, name })
        .collect()
}

fn original(dll: &Path, root: &Path) -> Result<Option<PathBuf>, String> {
    let relative = dll
        .strip_prefix(root)
        .map_err(|_| "DLL outside game folder.")?
        .to_string_lossy()
        .replace('\\', "/");
    let candidates = [
        format!("{relative}.kryoto"),
        format!("{relative}.kryoto-original"),
        dll.with_extension("rne")
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/"),
    ];
    for candidate in candidates {
        let path = journal::safe_path(root, &candidate)?;
        if path.is_file() {
            if detection::pe_machine(&path) != detection::pe_machine(dll) {
                return Err("The original Steam API backup has the wrong architecture.".into());
            }
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// All Forge mutation paths, including its own sidecars, are journalled before use.
fn plan(root: &Path, evidence: &Evidence) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut files = BTreeSet::new();
    let mut directories = BTreeSet::new();
    let mut dirs = evidence.executable_dirs.clone();
    dirs.insert(root.to_owned());
    for dll in &evidence.dlls {
        files.insert(dll.clone());
        files.insert(PathBuf::from(format!("{}.kryoto", dll.display())));
        files.insert(dll.with_extension("rne"));
        dirs.insert(dll.parent().unwrap().to_owned());
    }
    for dir in dirs {
        for name in [
            "steam_appid.txt",
            "kryoto-online.ini",
            "steam_emu.ini",
            "steak_emu.ini",
            "kryotoO.dll",
            "kryotoO32.dll",
            "winmm.dll",
            "steakclient64.dll",
            "rune.dll",
            "rune64.dll",
            "steamclient.dll",
            "steamclient64.dll",
            "GameOverlayRenderer.dll",
            "GameOverlayRenderer64.dll",
        ] {
            files.insert(dir.join(name));
        }
        directories.insert(dir.join("plugins"));
        directories.insert(dir.join("steam_settings"));
        for name in [
            "steam_appid.txt",
            "steam_interfaces.txt",
            "configs.app.ini",
            "configs.user.ini",
            "account_avatar.png",
            "account_avatar_default.png",
        ] {
            files.insert(dir.join("steam_settings").join(name));
        }
    }
    files.insert(root.join(crack::CRACK_MANIFEST));
    let base_files = files.clone();
    for path in base_files {
        let relative = path.strip_prefix(root).unwrap();
        let sidecar = root.join(crack::ORIG_DIR).join(relative);
        let mut parent = sidecar.parent();
        while let Some(p) = parent {
            if p == root {
                break;
            }
            directories.insert(p.to_owned());
            parent = p.parent();
        }
        files.insert(sidecar);
    }
    (
        files.into_iter().collect(),
        directories.into_iter().collect(),
    )
}

pub async fn apply<F: Fn(&str, Option<u8>) + Send + Sync>(
    root: &Path,
    cache: &Path,
    trusted_slug: Option<&str>,
    source: Emulator,
    identity: &EmuIdentity,
    progress: F,
) -> Result<RepairReport, String> {
    progress("Checking if I am inside a game folder", None);
    let root = root
        .canonicalize()
        .map_err(|_| "Place the fixer inside a game folder.")?;
    if journal::load(&root)?.is_some_and(|j| j.state != "undone") {
        return Err(
            "A previous repair has a backup here. Undo it before applying another source.".into(),
        );
    }
    detection::scan(&root)?;
    progress("Verifying the detected game on Kryoto", None);
    let client = release::client()?;
    let (game, evidence) = identify(&client, &root, trusted_slug).await?;
    progress(&format!("Checking {} compatibility", source.label()), None);
    let dlc = dlcs(&evidence);
    let mut originals = vec![];
    for dll in &evidence.dlls {
        let original = original(dll, &root)?;
        if original.is_none()
            && (source.is_rune()
                || source == Emulator::GbeFork
                    && !dll
                        .parent()
                        .unwrap()
                        .join("steam_settings/steam_interfaces.txt")
                        .is_file())
        {
            return Err("The original Steam API backup is missing. This source needs it to configure the game's interfaces safely. Re-extract the Kryoto release or contact support.".into());
        }
        originals.push((dll.clone(), original));
    }
    let _guard = journal::lock(&root)?;
    let tools = release::install(&client, cache, source, &progress).await?;
    // Reject a plan if Forge would patch a spare DLL that the compatibility scan excluded.
    if crack::find_steam_api_dlls(&root) != evidence.dlls {
        return Err("The Steam DLL layout contains extra SDK or backup copies. Contact support before replacing them.".into());
    }
    progress("Backing up emulator files", None);
    let (files, directories) = plan(&root, &evidence);
    let mut record = Journal::prepare(&root, &files, &directories, source.label(), &tools.version)?;
    let outcome: Result<RepairReport, String> = async {
        for target in &files {
            if target.is_file() {
                journal::make_writable(target)?;
            }
        }
        for (dll, backup) in &originals {
            if let Some(backup) = backup {
                journal::make_writable(dll)?;
                std::fs::copy(backup, dll).map_err(|e| {
                    format!(
                        "Could not restore the original Steam API for interface generation: {e}"
                    )
                })?;
                // Copying an original can carry its readonly attribute too.
                journal::make_writable(dll)?;
                let forge_backup = PathBuf::from(format!("{}.kryoto", dll.display()));
                if !forge_backup.exists() {
                    std::fs::copy(backup, forge_backup).map_err(|e| e.to_string())?;
                }
            }
        }
        progress(
            &format!(
                "Applying {} and configuring Steam interfaces",
                source.label()
            ),
            None,
        );
        // Only the emulator phase. Steamless/acquisition/archive/upload are excluded.
        let result = crack::crack_tree(
            &tools.paths,
            &root,
            &game.steam_appid,
            &dlc,
            &evidence.steamstub_files,
            identity,
            source,
            &Cancel::new(),
        )
        .await
        .map_err(|e| e.to_string())?;
        if result.dlls_replaced == 0 {
            return Err("The emulator did not replace any game files.".into());
        }
        let report = RepairReport {
            game,
            source: source.label().into(),
            version: tools.version,
            archive_sha256: tools.archive_sha256,
            dlls_replaced: result.dlls_replaced,
            interfaces_generated: result.interfaces_generated,
            warnings: result.warnings,
        };
        record.report = Some(report.clone());
        record.state = "applied".into();
        record.save(&root)?;
        Ok(report)
    }
    .await;
    match outcome {
        Ok(report) => {
            progress("Done. Start the game and check whether it works", None);
            Ok(report)
        }
        Err(error) => {
            progress("Repair failed; restoring the backed-up files", None);
            match record.restore(&root) { Ok(()) => Err(format!("{error} The previous files were restored.")), Err(restore) => Err(format!("{error} Undo could not finish: {restore}. Keep .kryoto-repair and contact support.")) }
        }
    }
}

pub fn add_log(logs: &mut Vec<String>, text: &str, percent: Option<u8>) {
    if percent.is_some_and(|n| n % 10 != 0) {
        return;
    }
    let line = format!(
        "{text}{}",
        percent.map(|n| format!(" {n}%")).unwrap_or_default()
    );
    if logs.last() == Some(&line) {
        return;
    }
    if logs.len() >= 300 {
        logs.remove(0);
    }
    logs.push(line);
}

pub fn support_text(game: Option<&GameIdentity>, source: &str, logs: &[String]) -> String {
    let title = game
        .map(|g| g.title.as_str())
        .unwrap_or("an unidentified game");
    let page = game
        .map(|g| format!("https://kryo.to/game/{}", g.slug))
        .unwrap_or_default();
    let mut text = format!("Hello staff, I am having problems with {title}. I tried the Kryoto auto-fixer ({source}) and it did not fix my issue.\nGame: {page}\nSteam id: {}\nSystem: {} / {}\nPlease describe the error or what happens when you launch:\n\nRepair log:\n", game.map(|g| g.steam_appid.as_str()).unwrap_or("unknown"), std::env::consts::OS, std::env::consts::ARCH);
    let user_path = regex::Regex::new(r"(?i)(?:[A-Z]:[\\/]Users[\\/]|/home/)[^\\/\s]+").unwrap();
    for line in logs
        .iter()
        .rev()
        .take(24)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let scrubbed = user_path.replace_all(line, "[user]");
        text.extend(scrubbed.chars().take(140));
        text.push('\n');
    }
    text.chars().take(3900).collect()
}
