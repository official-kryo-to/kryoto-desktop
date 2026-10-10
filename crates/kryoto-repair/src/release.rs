use crate::{
    journal::{digest, safe_path},
    settings::Emulator,
    tools::ToolPaths,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Write, path::Path, time::Duration};

#[derive(Debug, Clone, Serialize)]
pub struct Source {
    pub id: Emulator,
    pub label: &'static str,
    pub online: bool,
}
/// Both frontends consume this list. Manual/custom cracking has no auto-repair path.
pub fn sources() -> Vec<Source> {
    [
        Emulator::GbeFork,
        Emulator::Online,
        Emulator::Rune,
        Emulator::RuneSteak,
        Emulator::RuneSteamclient,
    ]
    .into_iter()
    .map(|id| Source {
        id,
        label: id.label(),
        online: id == Emulator::Online,
    })
    .collect()
}
pub fn repository(source: Emulator) -> Result<&'static str, String> {
    Ok(match source {
        Emulator::GbeFork => "Detanup01/gbe_fork",
        Emulator::Online => "official-kryo-to/kryoto-online",
        Emulator::Rune | Emulator::RuneSteak | Emulator::RuneSteamclient => "Mush-iii/rune-emu",
        Emulator::Custom => return Err("This source has no automatic repair.".into()),
    })
}
pub fn pick_asset(source: Emulator, names: &[String]) -> Option<&str> {
    let exact = |wanted: &str| {
        names
            .iter()
            .find(|n| n.eq_ignore_ascii_case(wanted))
            .map(String::as_str)
    };
    match source {
        Emulator::GbeFork => exact("emu-win-release.7z")
            .or_else(|| exact("emu-win-release-vs22.7z"))
            .or_else(|| exact("emu-win-release-vs26.7z")),
        Emulator::Rune => exact("rune-emu.zip"),
        Emulator::RuneSteak => exact("steakclient.zip"),
        Emulator::RuneSteamclient => exact("steamclient.zip"),
        Emulator::Online => names
            .iter()
            .find(|n| n.starts_with("kryoto-online-") && n.ends_with("-release.zip"))
            .map(String::as_str),
        Emulator::Custom => None,
    }
}

pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("KryotoRepair/0.1")
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(300))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let host = attempt.url().host_str().unwrap_or("");
            if attempt.previous().len() < 5
                && attempt.url().scheme() == "https"
                && [
                    "github.com",
                    "api.github.com",
                    "release-assets.githubusercontent.com",
                    "objects.githubusercontent.com",
                    "kryo.to",
                ]
                .contains(&host)
            {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}
#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}
#[derive(Debug, Serialize, Deserialize)]
struct Cache {
    version: String,
    archive_sha256: String,
    files: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Tools {
    pub version: String,
    pub archive_sha256: String,
    #[serde(skip)]
    pub paths: ToolPaths,
}

pub async fn install<F: Fn(&str, Option<u8>)>(
    client: &reqwest::Client,
    cache: &Path,
    source: Emulator,
    progress: &F,
) -> Result<Tools, String> {
    let repo = repository(source)?;
    progress("Checking the latest emulator release", None);
    let response = client
        .get(format!(
            "https://api.github.com/repos/{repo}/releases/latest"
        ))
        .send()
        .await
        .map_err(|e| format!("Could not reach the emulator release: {e}"))?
        .error_for_status()
        .map_err(|e| format!("The emulator release is unavailable: {e}"))?;
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("The release manifest is too large.".into());
    }
    let release: Release = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let names = release
        .assets
        .iter()
        .map(|a| a.name.clone())
        .collect::<Vec<_>>();
    let wanted =
        pick_asset(source, &names).ok_or("The release has no compatible Windows archive.")?;
    let asset = release.assets.iter().find(|a| a.name == wanted).unwrap();
    if asset.size == 0 || asset.size > 256 * 1024 * 1024 {
        return Err("The emulator archive size is invalid.".into());
    }
    let url = url::Url::parse(&asset.browser_download_url).map_err(|e| e.to_string())?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url
            .path()
            .starts_with(&format!("/{repo}/releases/download/"))
    {
        return Err("The emulator download is not from its upstream release.".into());
    }
    std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let key = format!(
        "{:x}",
        sha2::Sha256::digest(format!("{repo}:{}:{}", release.tag_name, asset.name).as_bytes())
    );
    use sha2::Digest;
    let dir = safe_path(cache, &key)?;
    let complete = dir.join("complete.json");
    if let Ok(text) = std::fs::read_to_string(&complete) {
        if let Ok(stored) = serde_json::from_str::<Cache>(&text) {
            let expected = asset
                .digest
                .as_deref()
                .and_then(|s| s.strip_prefix("sha256:"));
            if stored.version == release.tag_name
                && expected.is_none_or(|s| s == stored.archive_sha256)
                && !stored.files.is_empty()
                && stored.files.iter().all(|(p, hash)| {
                    safe_path(&dir, p)
                        .and_then(|p| digest(&p))
                        .is_ok_and(|h| h == *hash)
                })
            {
                return Ok(Tools {
                    version: stored.version,
                    archive_sha256: stored.archive_sha256,
                    paths: locate(&dir, source)?,
                });
            }
        }
        return Err(
            "The cached emulator files changed. Delete this tool cache and try again.".into(),
        );
    }
    // A unique fresh directory avoids trusting a half-unpacked previous attempt.
    let stage = safe_path(
        cache,
        &format!(
            "{key}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ),
    )?;
    std::fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let archive = stage.join(&asset.name);
    let mut output = std::fs::File::create(&archive).map_err(|e| e.to_string())?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let mut stream = response.bytes_stream();
    let mut done = 0u64;
    let mut last = None;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        done += chunk.len() as u64;
        if done > asset.size || done > 256 * 1024 * 1024 {
            return Err("The download exceeded its expected size.".into());
        }
        output.write_all(&chunk).map_err(|e| e.to_string())?;
        let percent = ((done * 100 / asset.size).min(100)) as u8;
        if last != Some(percent) {
            progress(
                &format!("Downloading {} latest release", source.label()),
                Some(percent),
            );
            last = Some(percent);
        }
    }
    output.sync_all().map_err(|e| e.to_string())?;
    drop(output);
    if done != asset.size {
        return Err("The emulator download was incomplete.".into());
    }
    let hash = digest(&archive)?;
    if let Some(expected) = asset
        .digest
        .as_deref()
        .and_then(|s| s.strip_prefix("sha256:"))
    {
        if hash != expected {
            return Err(
                "Emulator checksum verification failed. No game files were changed.".into(),
            );
        }
    }
    progress("Unpacking the emulator", None);
    let unpacked = stage.join("files");
    std::fs::create_dir_all(&unpacked).map_err(|e| e.to_string())?;
    unpack(&archive, &unpacked)?;
    let _ = locate(&unpacked, source)?;
    let mut files = BTreeMap::new();
    for entry in walkdir::WalkDir::new(&unpacked)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
    {
        let relative = entry
            .path()
            .strip_prefix(&unpacked)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        files.insert(relative, digest(entry.path())?);
    }
    let metadata = Cache {
        version: release.tag_name.clone(),
        archive_sha256: hash.clone(),
        files,
    };
    std::fs::write(
        unpacked.join("complete.json"),
        serde_json::to_vec(&metadata).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    // This cache path is validated under cache; only unused/incomplete files exist here.
    if dir.exists() {
        return Err("An incomplete emulator cache exists. Remove it and try again.".into());
    }
    std::fs::rename(&unpacked, &dir).map_err(|e| e.to_string())?;
    std::fs::remove_file(archive).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir(stage);
    Ok(Tools {
        version: release.tag_name,
        archive_sha256: hash,
        paths: locate(&dir, source)?,
    })
}

fn unpack(archive: &Path, root: &Path) -> Result<(), String> {
    let mut total = 0u64;
    let mut count = 0u32;
    if archive.extension().is_some_and(|e| e == "7z") {
        sevenz_rust::decompress_file_with_extract_fn(archive, root, |entry, reader, _| {
            count += 1;
            total = total.saturating_add(entry.size());
            if count > 10000 || total > 1024 * 1024 * 1024 {
                return Err(sevenz_rust::Error::other(
                    "Archive extraction limit exceeded",
                ));
            }
            let name = entry.name().replace('\\', "/");
            let target =
                safe_path(root, name.trim_end_matches('/')).map_err(sevenz_rust::Error::other)?;
            if entry.is_directory() {
                std::fs::create_dir_all(target)?;
            } else {
                std::fs::create_dir_all(target.parent().unwrap())?;
                let mut file = std::fs::File::create(target)?;
                std::io::copy(reader, &mut file)?;
            }
            Ok(true)
        })
        .map_err(|e| e.to_string())?;
    } else {
        let mut zip =
            zip::ZipArchive::new(std::fs::File::open(archive).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            count += 1;
            total = total.saturating_add(entry.size());
            if count > 10000
                || total > 1024 * 1024 * 1024
                || entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
            {
                return Err("Unsafe or oversized emulator archive.".into());
            }
            let name = entry.name().replace('\\', "/");
            let target = safe_path(root, name.trim_end_matches('/'))?;
            if entry.is_dir() {
                std::fs::create_dir_all(target).map_err(|e| e.to_string())?;
            } else {
                std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
                let mut file = std::fs::File::create(target).map_err(|e| e.to_string())?;
                std::io::copy(&mut entry, &mut file).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

pub fn locate(root: &Path, source: Emulator) -> Result<ToolPaths, String> {
    let find = |name: &str| {
        walkdir::WalkDir::new(root)
            .follow_links(false)
            .max_depth(6)
            .into_iter()
            .flatten()
            .find(|e| e.file_name().eq_ignore_ascii_case(name))
            .map(|e| e.path().to_owned())
    };
    let mut paths = ToolPaths::default();
    match source {
        Emulator::GbeFork => paths.gbe_dir = find("experimental").or_else(|| find("regular")),
        Emulator::Online => paths.online_dir = Some(root.to_owned()),
        Emulator::Rune => {
            paths.rune_dir = find("steam_api64.dll").and_then(|p| p.parent().map(Path::to_owned))
        }
        Emulator::RuneSteak => {
            paths.rune_steak_dir =
                find("steakclient64.dll").and_then(|p| p.parent().map(Path::to_owned))
        }
        Emulator::RuneSteamclient => {
            paths.rune_steamclient_dir = find("x64").and_then(|p| p.parent().map(Path::to_owned))
        }
        Emulator::Custom => return Err("Custom source is not supported.".into()),
    }
    let valid = match source {
        Emulator::GbeFork => {
            paths.gbe_dll(true).is_some()
                && paths.gbe_dll(false).is_some()
                && paths.generate_interfaces(true).is_some()
                && paths.generate_interfaces(false).is_some()
        }
        Emulator::Online => {
            paths.online_dll(true).is_some()
                && paths.online_dll(false).is_some()
                && paths.online_core(true).is_some()
                && paths.online_core(false).is_some()
        }
        Emulator::Rune => {
            paths.rune_dll(source, true).is_some()
                && paths.rune_dll(source, false).is_some()
                && paths.rune_ini(source).is_some()
        }
        Emulator::RuneSteak => paths.rune_steak_files().is_some(),
        Emulator::RuneSteamclient => {
            paths.rune_steamclient_support(true).is_some()
                && paths.rune_steamclient_support(false).is_some()
                && paths.rune_ini(source).is_some()
        }
        Emulator::Custom => false,
    };
    if !valid {
        return Err("The emulator archive is missing required files.".into());
    }
    Ok(paths)
}
