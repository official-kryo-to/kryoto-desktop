extern crate self as kryoto_core;
pub mod child_guard;
pub mod clipboard;
pub mod crack;
pub mod detection;
pub mod error;
pub mod hubcap;
pub mod journal;
pub mod peversion;
pub mod release;
pub mod repair;
pub mod settings;
pub mod tools;
use std::path::Path;
#[derive(Default)]
pub struct Cancel(std::sync::atomic::AtomicBool);
pub trait Cancellation: Send + Sync {
    fn requested(&self) -> bool;
}
impl Cancellation for Cancel {
    fn requested(&self) -> bool {
        Cancel::requested(self)
    }
}
impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn requested(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub fn cancel(&self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
pub fn find_executables(root: &Path) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = walkdir::WalkDir::new(root)
        .follow_links(false)
        .max_depth(12)
        .into_iter()
        .filter_entry(|e| {
            ![".kryoto-repair", ".kryoto-orig"].contains(&e.file_name().to_string_lossy().as_ref())
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .to_lowercase()
                .ends_with(".exe")
        })
        .filter(|e| !exe_is_noise(&e.file_name().to_string_lossy().to_lowercase()))
        .filter_map(|e| {
            let rel = e
                .path()
                .strip_prefix(root)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            let size = e.metadata().ok()?.len();
            Some((rel, size))
        })
        .collect();

    sort_executables(&mut out);
    out.truncate(100);
    out
}

/// An installer, redistributable or SDK tool rather than a game.
///
/// A scene release very often ships a `setup.exe` at the top of the tree, and
/// the ranking sorts by path DEPTH first - so an installer at depth 0 outranked
/// the game itself and was pre-selected as the thing to launch. `name` is the
/// lowercased file name. Shared with the archive check, which ranks a listing
/// the same way it ranks a folder.
pub fn exe_is_noise(name: &str) -> bool {
    const NOISE: [&str; 19] = [
        "setup",
        "install",
        "redist",
        "ueprereqsetup",
        "oalinst",
        "unitycrashhandler",
        "crashreport",
        "vcredist",
        "dxsetup",
        "directx",
        "dotnetfx",
        "uninstall",
        "unins000",
        "vvis",
        "vrad",
        "vbsp",
        "bspzip",
        "hammer",
        "studiomdl",
    ];
    name.contains("auto-fixer") || NOISE.iter().any(|n| name.contains(n))
}

/// Root-level and shallow executables first (portal.exe, hl2.exe, Game.exe),
/// then largest first within each depth.
pub fn sort_executables(exes: &mut [(String, u64)]) {
    exes.sort_by(|(path_a, size_a), (path_b, size_b)| {
        let depth_a = path_a.matches('/').count();
        let depth_b = path_b.matches('/').count();
        depth_a.cmp(&depth_b).then_with(|| size_b.cmp(size_a))
    });
}

pub mod launchtest {
    use std::path::PathBuf;
    pub fn pick_executable(candidates: &[PathBuf]) -> Option<PathBuf> {
        const NOT_THE_GAME: [&str; 10] = [
            "unins",
            "setup",
            "install",
            "redist",
            "vcredist",
            "dxsetup",
            "dotnet",
            "crashreport",
            "crashhandler",
            "config",
        ];
        candidates
            .iter()
            .find(|p| {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                !NOT_THE_GAME.iter().any(|bad| name.contains(bad))
            })
            .or_else(|| candidates.first())
            .cloned()
    }
}
