use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameIdentity {
    pub slug: String,
    pub title: String,
    pub steam_appid: String,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Evidence {
    pub appids: BTreeSet<String>,
    pub slugs: BTreeSet<String>,
    pub signals: BTreeSet<String>,
    pub dlls: Vec<PathBuf>,
    pub executable_dirs: BTreeSet<PathBuf>,
    pub title_hint: String,
    pub steamstub_files: Vec<PathBuf>,
}

pub fn appid(value: &str) -> Option<String> {
    let v = value.trim();
    let id = v.trim_start_matches('0');
    (v.len() <= 10 && !id.is_empty() && v.bytes().all(|b| b.is_ascii_digit()) && id != "480")
        .then(|| id.to_owned())
}

pub fn read_small(path: &Path) -> Option<String> {
    if std::fs::metadata(path).ok()?.len() > 64 * 1024 {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// Path names alone are not compatibility checks. Check the PE machine too.
pub fn pe_machine(path: &Path) -> Option<u16> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let mut head = [0; 64];
    file.read_exact(&mut head).ok()?;
    if &head[..2] != b"MZ" {
        return None;
    }
    let offset = u32::from_le_bytes(head[60..64].try_into().ok()?) as u64;
    if offset > 16 * 1024 * 1024 {
        return None;
    }
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut pe = [0; 6];
    file.read_exact(&mut pe).ok()?;
    if &pe[..4] != b"PE\0\0" {
        return None;
    }
    Some(u16::from_le_bytes([pe[4], pe[5]]))
}

pub fn normalize_title(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// SteamStub is identified from the PE section table, never a filename guess.
pub fn has_steamstub(path: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let read = || -> Option<bool> {
        let mut file = std::fs::File::open(path).ok()?;
        let mut head = [0u8; 64];
        file.read_exact(&mut head).ok()?;
        if &head[..2] != b"MZ" {
            return None;
        }
        let offset = u32::from_le_bytes(head[60..64].try_into().ok()?) as u64;
        if offset > 16 * 1024 * 1024 {
            return None;
        }
        file.seek(SeekFrom::Start(offset)).ok()?;
        let mut pe = [0u8; 24];
        file.read_exact(&mut pe).ok()?;
        if &pe[..4] != b"PE\0\0" {
            return None;
        }
        let sections = u16::from_le_bytes(pe[6..8].try_into().ok()?) as usize;
        if sections > 128 {
            return None;
        }
        let optional = u16::from_le_bytes(pe[20..22].try_into().ok()?) as u64;
        file.seek(SeekFrom::Start(offset + 24 + optional)).ok()?;
        for _ in 0..sections {
            let mut section = [0u8; 40];
            file.read_exact(&mut section).ok()?;
            if &section[..8] == b".bind\0\0\0" {
                return Some(true);
            }
        }
        Some(false)
    };
    read().unwrap_or(false)
}

pub fn scan(root: &Path) -> Result<Evidence, String> {
    let root = root
        .canonicalize()
        .map_err(|_| "The game folder does not exist.")?;
    if !root.is_dir() || root.parent().is_none() {
        return Err("Place auto-fixer-gbef.exe inside a game folder.".into());
    }
    let folder = root
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut out = Evidence {
        appids: BTreeSet::new(),
        slugs: BTreeSet::new(),
        signals: BTreeSet::new(),
        dlls: vec![],
        executable_dirs: BTreeSet::new(),
        title_hint: folder.clone(),
        steamstub_files: vec![],
    };
    if folder.to_lowercase().contains("kryoto") || folder.to_lowercase().contains("kryo.to") {
        out.signals.insert("Kryoto folder name".into());
        out.title_hint = regex::Regex::new(r"(?i)[\[\(\s._-]*(?:kryoto|kryo\.to)[\]\)\s._-]*")
            .unwrap()
            .replace_all(&folder, " ")
            .trim()
            .to_string();
    }
    let entries = WalkDir::new(&root)
        .follow_links(false)
        .max_depth(12)
        .into_iter()
        .filter_entry(|e| {
            !e.file_name()
                .to_string_lossy()
                .starts_with(".kryoto-repair")
        });
    let mut visited = 0;
    for entry in entries {
        let e = entry.map_err(|_| "Some game files cannot be read. Check folder permissions.")?;
        visited += 1;
        if visited > 100_000 {
            return Err(
                "Too many files. Put the fixer inside one game folder, not your whole library."
                    .into(),
            );
        }
        if !e.file_type().is_file() {
            continue;
        }
        // Junctions and reparse points may be reported differently on Windows.
        let real = e
            .path()
            .canonicalize()
            .map_err(|_| "A game file cannot be resolved.")?;
        if !real.starts_with(&root) {
            return Err("A game file points outside the game folder. Repair was stopped.".into());
        }
        let name = e.file_name().to_string_lossy().to_ascii_lowercase();
        let relative = e
            .path()
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
            .to_lowercase();
        if (name == "steam_api.dll" || name == "steam_api64.dll")
            && ![
                "redistributable_bin",
                "source/thirdparty",
                "sdk/",
                ".kryoto-orig/",
            ]
            .iter()
            .any(|s| relative.contains(s))
        {
            let expected = if name == "steam_api64.dll" {
                0x8664
            } else {
                0x14c
            };
            if pe_machine(e.path()) != Some(expected) {
                return Err(format!("{} is not a compatible Steam API DLL.", relative));
            }
            out.dlls.push(e.path().to_owned());
        }
        if name.ends_with(".exe")
            && !name.contains("auto-fixer")
            && !["unins", "setup", "redist", "crash", "generate_interfaces"]
                .iter()
                .any(|s| name.contains(s))
            && pe_machine(e.path()).is_some_and(|m| m == 0x8664 || m == 0x14c)
        {
            out.executable_dirs
                .insert(e.path().parent().unwrap().to_owned());
            if has_steamstub(e.path()) {
                out.steamstub_files.push(e.path().to_owned());
            }
        }
        if name == "steam_api.dll.kryoto" || name == "steam_api64.dll.kryoto" {
            out.signals.insert("Forge original DLL backup".into());
        }
        if name == ".kryoto-release.json" {
            if let Some(value) = read_small(e.path())
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            {
                if let Some(id) = value["steam_appid"].as_str().and_then(appid) {
                    out.appids.insert(id);
                }
                if let Some(slug) = value["slug"].as_str().filter(|s| safe_slug(s)) {
                    out.slugs.insert(slug.to_owned());
                }
                out.signals.insert("Kryoto release identity".into());
            }
        }
        if name == "steam_appid.txt" {
            if let Some(id) = read_small(e.path()).as_deref().and_then(appid) {
                out.appids.insert(id);
            }
        }
        if [
            "configs.app.ini",
            "configs.user.ini",
            "kryoto-online.ini",
            "steam_emu.ini",
            "steak_emu.ini",
        ]
        .contains(&name.as_str())
        {
            if let Some(text) = read_small(e.path()) {
                let lower = text.to_lowercase();
                if lower.contains("written by kryoto")
                    || lower.contains("account_name=kryoto")
                    || name == "kryoto-online.ini"
                {
                    out.signals.insert("Kryoto emulator configuration".into());
                }
                for line in text.lines() {
                    if let Some((key, value)) = line.split_once('=') {
                        if ["ogappid", "appid", "app_id", "steam_appid"]
                            .contains(&key.trim().to_lowercase().as_str())
                        {
                            if let Some(id) = appid(value) {
                                out.appids.insert(id);
                            }
                        }
                    }
                }
            }
        }
        if name.ends_with(".url") {
            if let Some(text) = read_small(e.path()) {
                for line in text.lines() {
                    if let Some(url) = line
                        .trim()
                        .strip_prefix("URL=")
                        .and_then(|s| url::Url::parse(s).ok())
                    {
                        if url.scheme() == "https" && url.host_str() == Some("kryo.to") {
                            if url.path() == "/" {
                                out.signals.insert("Kryoto release shortcut".into());
                            }
                            if let Some(slug) =
                                url.path().strip_prefix("/game/").filter(|s| safe_slug(s))
                            {
                                out.slugs.insert(slug.to_owned());
                                out.signals.insert("Kryoto game shortcut".into());
                            }
                        }
                    }
                }
            }
        }
    }
    if out.executable_dirs.is_empty() {
        return Err(
            "Place auto-fixer-gbef.exe inside a game folder. No game executable was found.".into(),
        );
    }
    if out.dlls.is_empty() {
        return Err(
            "No supported Steam API DLL was found. Place the fixer inside the game's main folder."
                .into(),
        );
    }
    if out.appids.len() > 1 || out.slugs.len() > 1 {
        return Err("The folder contains conflicting game identities. Repair was stopped.".into());
    }
    out.dlls.sort_by_key(|p| {
        (
            p.file_name()
                .unwrap_or_default()
                .eq_ignore_ascii_case("steam_api64.dll"),
            p.clone(),
        )
    });
    Ok(out)
}

pub fn safe_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 180
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn verify_identity(
    evidence: &Evidence,
    game: &GameIdentity,
    trusted_slug: Option<&str>,
) -> Result<(), String> {
    if !safe_slug(&game.slug) || appid(&game.steam_appid).is_none() {
        return Err("The catalog game has no valid Steam identity.".into());
    }
    if trusted_slug.is_some_and(|s| s != game.slug)
        || evidence.slugs.iter().any(|s| s != &game.slug)
        || evidence.appids.iter().any(|id| id != &game.steam_appid)
    {
        return Err(
            "The local game identity does not match Kryoto's catalog. No files were changed."
                .into(),
        );
    }
    if trusted_slug.is_none() && evidence.signals.is_empty() {
        return Err("No Kryoto release markers were found. A matching Steam id alone cannot verify where a game came from.".into());
    }
    Ok(())
}
