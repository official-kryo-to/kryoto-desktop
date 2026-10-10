//! External tools, and how they are invoked on each platform.
//!
//! The pipeline is mostly orchestration around five Windows programs. That is
//! the awkward truth of this problem: the games are Windows builds, and the
//! tools that crack them (gbe_fork's generate_interfaces, Steamless) are
//! Windows binaries. So on Linux they run under Wine.
//!
//! Rather than sprinkle `if cfg!(windows)` through every call site, every
//! invocation goes through [`Tool::command`], which decides once whether a
//! binary is native or needs a Wine prefix. Two of the five are genuinely
//! cross-platform and must NOT be sent through Wine:
//!
//!   DepotDownloader  a .NET dll - run with native `dotnet` on both platforms
//!   7-Zip            native `7z`/`7zz` exists on Linux
//!   DIE              ships a native Linux console build
//!   generate_interfaces  Windows exe -> Wine
//!   Steamless            .NET Framework Windows app -> Wine (needs dotnet48)
//!
//! Getting that split wrong is not a small mistake: running `dotnet` under Wine
//! works badly and slowly, and sending a native ELF to Wine simply fails.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::error::{Error, Result};

/// How a given binary has to be launched on this host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Launch {
    /// Run it directly.
    Native,
    /// A Windows .exe on a non-Windows host: prefix with `wine`.
    Wine,
    /// A .NET dll: run as `dotnet <dll>`.
    Dotnet,
}

#[derive(Debug, Clone)]
pub struct Tool {
    pub name: &'static str,
    pub path: PathBuf,
    pub launch: Launch,
}

impl Tool {
    /// Build the command, applying the Wine or dotnet prefix as needed.
    pub fn command(&self, wine: &str, dotnet: &str) -> Command {
        let mut cmd = match self.launch {
            Launch::Native => Command::new(&self.path),
            Launch::Dotnet => {
                let mut c = Command::new(dotnet);
                c.arg(&self.path);
                c
            }
            Launch::Wine => {
                let mut c = Command::new(wine);
                c.arg(&self.path);
                c
            }
        };
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        // Prevent every spawned tool from flashing a console window in the
        // built app on Windows. Gated so the Linux build is unaffected.
        #[cfg(windows)]
        {
            #[allow(unused_imports)]
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd
    }
}

/// Decide how a Windows .exe should be launched on this host.
pub fn windows_exe_launch() -> Launch {
    if cfg!(windows) {
        Launch::Native
    } else {
        Launch::Wine
    }
}

/// Paths to everything the pipeline shells out to.
///
/// Every field is optional and checked by [`preflight`] BEFORE any long work
/// starts. A pipeline that dies four steps in because one exe was missing has
/// thrown away the whole download, which is the specific failure this exists to
/// prevent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolPaths {
    /// gbe_fork release directory. Point at the VARIANT folder that contains
    /// both `x64/` and `x86/` - e.g. `<release>/experimental`.
    #[serde(default)]
    pub gbe_dir: Option<PathBuf>,
    /// Kryoto Online release directory, holding `x86/` and `x64/`. Each holds
    /// the proxy (`steam_api(64).dll`) AND the patch core (`kryotoO.dll` /
    /// `kryotoO32.dll`). Only needed when the online emulator is selected.
    #[serde(default)]
    pub online_dir: Option<PathBuf>,
    /// RUNE release directories, one per profile.
    ///
    /// Each holds `steam_api.dll`, `steam_api64.dll` and the profile's config
    /// template - a flat layout, unlike gbe_fork's `x64/`/`x86/` split, so the
    /// same file name serves both architectures and the directory is the only
    /// thing that differs.
    #[serde(default)]
    pub rune_dir: Option<PathBuf>,
    #[serde(default)]
    pub rune_steak_dir: Option<PathBuf>,
    #[serde(default)]
    pub rune_steamclient_dir: Option<PathBuf>,
    /// Steamless.CLI.exe
    #[serde(default)]
    pub steamless_exe: Option<PathBuf>,
    /// 7z / 7zz / 7z.exe
    #[serde(default)]
    pub seven_zip: Option<PathBuf>,
    /// DepotDownloader.dll (only needed for Hubcap mode)
    #[serde(default)]
    pub depotdownloader_dll: Option<PathBuf>,
    /// Detect It Easy console. Optional: without it DRM detection falls back to
    /// the entropy heuristic, which never blocks on its own.
    #[serde(default)]
    pub die: Option<PathBuf>,
    /// `dotnet` executable. Default: whatever is on PATH.
    #[serde(default)]
    pub dotnet: Option<String>,
    /// `wine` executable. Default: whatever is on PATH. Ignored on Windows.
    #[serde(default)]
    pub wine: Option<String>,
}

impl ToolPaths {
    pub fn dotnet(&self) -> &str {
        self.dotnet.as_deref().unwrap_or("dotnet")
    }

    pub fn wine(&self) -> &str {
        self.wine.as_deref().unwrap_or("wine")
    }

    /// gbe_fork's emulator DLL for the given architecture.
    ///
    /// Tolerates `gbe_dir` pointing either at the variant folder (which holds
    /// `x64/` and `x86/`) or directly at one arch folder, because both are
    /// reasonable things for a person to pick in a directory chooser and
    /// guessing wrong produces a baffling "file not found".
    pub fn gbe_dll(&self, arch64: bool) -> Option<PathBuf> {
        let dir = self.gbe_dir.as_ref()?;
        let file = if arch64 {
            "steam_api64.dll"
        } else {
            "steam_api.dll"
        };
        let arch = if arch64 { "x64" } else { "x86" };
        let candidates = [
            dir.join(arch).join(file),
            dir.join(file),
            dir.parent().map(|p| p.join(arch).join(file))?,
        ];
        candidates.into_iter().find(|c| c.exists())
    }

    /// Kryoto Online's Steamworks proxy for the given architecture.
    /// The directory holding this RUNE profile's files.
    pub fn rune_profile_dir(&self, emulator: crate::settings::Emulator) -> Option<&PathBuf> {
        match emulator {
            crate::settings::Emulator::Rune => self.rune_dir.as_ref(),
            crate::settings::Emulator::RuneSteak => self.rune_steak_dir.as_ref(),
            crate::settings::Emulator::RuneSteamclient => self.rune_steamclient_dir.as_ref(),
            _ => None,
        }
    }

    /// RUNE's replacement Steamworks dll for this profile.
    ///
    /// Flat, unlike gbe_fork: both architectures sit in one directory under
    /// their own names, so there is no `x64`/`x86` to pick between.
    pub fn rune_dll(&self, emulator: crate::settings::Emulator, arch64: bool) -> Option<PathBuf> {
        let dir = self.rune_profile_dir(emulator)?;
        let file = if arch64 {
            "steam_api64.dll"
        } else {
            "steam_api.dll"
        };
        let candidate = dir.join(file);
        candidate.exists().then_some(candidate)
    }

    /// The config template this profile reads. Steakclient uses its own name,
    /// which is the sort of difference that turns into a silent no-op.
    ///
    /// The Steamclient zip puts its copy inside `x64/` and `x86/` rather than
    /// at the top, and the two are byte-for-byte identical - so either will do
    /// and the candidates simply cover both layouts.
    pub fn rune_ini(&self, emulator: crate::settings::Emulator) -> Option<PathBuf> {
        let dir = self.rune_profile_dir(emulator)?;
        let name = crate::settings::Emulator::rune_ini_name(emulator)?;
        [
            dir.join(name),
            dir.join("x64").join(name),
            dir.join("x86").join(name),
        ]
        .into_iter()
        .find(|c| c.exists())
    }

    /// The three files RUNE's Steakclient profile ships beside the game EXE.
    ///
    /// Steakclient is not a Steamworks replacement at all: `winmm.dll` is a
    /// proxy that Windows loads for the executable, it brings
    /// `steakclient64.dll` up with it, and `steam_api64.dll` is never touched.
    /// So this returns the whole set or nothing - two of the three is not a
    /// partial crack, it is a game that will not start.
    pub fn rune_steak_files(&self) -> Option<Vec<PathBuf>> {
        let dir = self.rune_steak_dir.as_ref()?;
        let files: Vec<PathBuf> = ["winmm.dll", "steakclient64.dll", "steak_emu.ini"]
            .iter()
            .map(|f| dir.join(f))
            .collect();
        files.iter().all(|f| f.exists()).then_some(files)
    }

    /// The support dlls RUNE's Steamclient profile drops beside a patched dll.
    ///
    /// Per architecture, and named differently in each - `rune64.dll` against
    /// `rune.dll` - so there is no one file name that serves both. All or
    /// nothing for the same reason as Steakclient: the patched Steamworks dll
    /// looks for these by name and a missing one is a game that does not run.
    pub fn rune_steamclient_support(&self, arch64: bool) -> Option<Vec<PathBuf>> {
        let dir = self.rune_steamclient_dir.as_ref()?;
        let names: [&str; 3] = if arch64 {
            [
                "GameOverlayRenderer64.dll",
                "rune64.dll",
                "steamclient64.dll",
            ]
        } else {
            ["GameOverlayRenderer.dll", "rune.dll", "steamclient.dll"]
        };
        let arch = if arch64 { "x64" } else { "x86" };
        let files: Vec<PathBuf> = names
            .iter()
            .map(|f| {
                let nested = dir.join(arch).join(f);
                if nested.exists() {
                    nested
                } else {
                    dir.join(f)
                }
            })
            .collect();
        files.iter().all(|f| f.exists()).then_some(files)
    }

    pub fn online_dll(&self, arch64: bool) -> Option<PathBuf> {
        let file = if arch64 {
            "steam_api64.dll"
        } else {
            "steam_api.dll"
        };
        self.online_file(arch64, file)
    }

    /// Kryoto Online's patch core - `kryotoO.dll` on x64, `kryotoO32.dll` on
    /// x86.
    ///
    /// Separate files since KryotoOnline 1.8.1. The proxy loads this at
    /// startup and applies NOTHING without it: no ownership spoof, no DLC, no
    /// plugins, no SteamStub handling. It does not refuse to load, which is
    /// the point - a game whose steam_api64.dll will not load dies with no
    /// message - so a release built without this file looks fine here and is
    /// inert on the player's machine. [`crate::crack`] treats it as fatal.
    pub fn online_core(&self, arch64: bool) -> Option<PathBuf> {
        let file = if arch64 {
            "kryotoO.dll"
        } else {
            "kryotoO32.dll"
        };
        self.online_file(arch64, file)
    }

    /// Find one file inside the Kryoto Online directory.
    ///
    /// The fixed candidates cover the layouts a person picks by hand in a
    /// directory chooser. The WALK after them exists because the release zip
    /// used to wrap everything in a `kryoto-online-<tag>-<config>/` folder, so
    /// an auto-installed copy sits one level below every fixed guess - the
    /// install reported success, this returned None, and every online build
    /// died on "no Kryoto Online dll was applied". Newer zips are flat and hit
    /// the first candidate; the walk is what makes an already-installed copy
    /// start working without a reinstall.
    fn online_file(&self, arch64: bool, file: &str) -> Option<PathBuf> {
        let dir = self.online_dir.as_ref()?;
        let arch = if arch64 { "x64" } else { "x86" };

        let fixed = [
            dir.join(arch).join(file),
            dir.join(file),
            dir.join("build").join(arch).join(file),
        ];
        if let Some(hit) = fixed.into_iter().find(|c| c.exists()) {
            return Some(hit);
        }

        // Depth 4 reaches `<dir>/<wrapper>/<arch>/<file>` with room to spare
        // and stops well short of walking a game tree by accident.
        let mut best: Option<PathBuf> = None;
        for entry in walkdir::WalkDir::new(dir)
            .max_depth(4)
            .into_iter()
            .flatten()
        {
            if !entry.file_type().is_file() {
                continue;
            }
            if !entry.file_name().eq_ignore_ascii_case(file) {
                continue;
            }
            let path = entry.path();
            // The parent must be the right arch folder, or an x86 dll picked
            // out of an x64 tree would be handed to a 64-bit game.
            let in_arch_dir = path
                .parent()
                .and_then(|p| p.file_name())
                .is_some_and(|n| n.eq_ignore_ascii_case(arch));
            if !in_arch_dir {
                continue;
            }
            // Both a release and a debug copy can be installed side by side.
            // Prefer release; a debug build is a fallback, not a default.
            let is_debug = path.to_string_lossy().to_lowercase().contains("debug");
            if !is_debug {
                return Some(path.to_path_buf());
            }
            best.get_or_insert_with(|| path.to_path_buf());
        }
        best
    }

    /// gbe_fork's `generate_interfaces` tool, which lives under
    /// `<release>/tools/generate_interfaces/` - one level ABOVE the variant
    /// folder, so this walks up rather than looking beside the DLLs.
    pub fn generate_interfaces(&self, arch64: bool) -> Option<PathBuf> {
        let dir = self.gbe_dir.as_ref()?;
        let exe = if arch64 {
            "generate_interfaces_x64.exe"
        } else {
            "generate_interfaces_x86.exe"
        };
        let mut up: Option<&Path> = Some(dir.as_path());
        for _ in 0..3 {
            let Some(d) = up else { break };
            let cand = d.join("tools").join("generate_interfaces").join(exe);
            if cand.exists() {
                return Some(cand);
            }
            up = d.parent();
        }
        None
    }
}

pub async fn run(mut cmd: Command, what: &str, timeout_secs: u64) -> Result<String> {
    cmd.kill_on_drop(true);
    let child = cmd
        .spawn()
        .map_err(|e| Error::Tool(format!("{what}: could not start ({e})")))?;
    crate::child_guard::contain(&child);

    let out = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| Error::Tool(format!("{what}: timed out after {timeout_secs}s")))?
    .map_err(|e| Error::Tool(format!("{what}: {e}")))?;

    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if !out.status.success() {
        return Err(Error::Tool(format!(
            "{what} exited with {}: {}",
            out.status,
            tool_message(&text)
        )));
    }
    Ok(text)
}
fn tool_message(text: &str) -> String {
    let kept: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !is_ascii_art(l))
        .collect();
    if kept.is_empty() {
        // Nothing but a banner. Saying so beats an empty string after the colon,
        // which reads like the message got lost.
        return "no output".to_string();
    }
    let joined = kept.join(" ");
    joined.chars().take(300).collect()
}
fn is_ascii_art(line: &str) -> bool {
    let solid: Vec<char> = line.chars().filter(|c| !c.is_whitespace()).collect();
    if solid.len() < 8 {
        return false;
    }
    let alnum = solid.iter().filter(|c| c.is_alphanumeric()).count();
    alnum * 5 < solid.len()
}
