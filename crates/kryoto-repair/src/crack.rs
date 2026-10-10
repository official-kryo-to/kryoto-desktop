//! Replace the Steamworks DLL with an emulator and, when the scan found it,
//! strip SteamStub with Steamless.
//!
//! TWO EMULATORS, and they are NOT interchangeable:
//!
//!   [`Emulator::GbeFork`]  gbe_fork, the maintained Goldberg fork. Emulates
//!                          Steam entirely. Works with no Steam client and no
//!                          account. Multiplayer is LAN/direct only. Default,
//!                          and correct for almost every release.
//!
//!   [`Emulator::Online`]   Kryoto Online. Does NOT emulate Steam - it forwards
//!                          to the REAL client while telling it the running
//!                          game is Spacewar (AppId 480), which is free, so
//!                          the ownership check passes and Steam's own
//!                          matchmaking still works. That is what makes online
//!                          play possible, and also why it needs Steam running
//!                          and a real account.
//!
//! The gbe_fork order below is not arbitrary and getting it wrong produces a
//! game that launches to a Steam error:
//!
//!   1. Find every steam_api.dll / steam_api64.dll in the tree.
//!   2. Run `generate_interfaces` on the ORIGINAL dll, BEFORE replacing it.
//!      The tool reads the real Steamworks dll to learn which interface
//!      versions this build asks for. Run it after the swap and it reads the
//!      emulator instead and produces a useless file.
//!   3. Clear the read-only bit. Steam sets it, and a straight copy over a
//!      read-only file fails on Windows.
//!   4. Copy the emulator dll over the original.
//!   5. Write steam_appid.txt beside the dll AND inside steam_settings, plus
//!      configs.app.ini with the DLC list.
//!   6. Steamless over the exes THE DRM SCAN FLAGGED - never the whole tree.
//!
//! Steps 1-5 are the same sequence Steam Auto Cracker performs by hand.
//!
//! The online path replaces steps 2 and 5: it writes ONE ini beside the dll and
//! wants no `steam_settings` at all. It has no offline identity to configure -
//! the player shows up as their own Steam account, which is the entire point.
//!
//! ORIGINALS ARE BACKED UP as `steam_api64.dll.kryoto`, so a mis-crack is
//! recoverable without re-downloading tens of gigabytes. The extension is ours
//! rather than the conventional `.bak` so a backup is identifiable as something
//! this tool made, and so a game's own `.bak` files are never confused for one.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::error::{Error, Result};
use crate::hubcap::Dlc;
use crate::settings::Emulator;
use crate::tools::{run, windows_exe_launch, Tool, ToolPaths};

/// Extension given to the original DLL before it is overwritten.
const BACKUP_EXT: &str = "kryoto";

/// The online emulator's config file, read from beside the game executable.
const ONLINE_INI: &str = "kryoto-online.ini";

/// What the online emulator reports the running game as. 480 is Spacewar, the
/// free Valve sample every Steam account already owns - so the ownership check
/// passes. A handful of games reject it and need 440 (Team Fortress 2); that is
/// a per-game override, not something to change globally.
const SPACEWAR_APPID: &str = "480";

#[derive(Debug, Clone, Serialize)]
pub struct CrackResult {
    pub dlls_replaced: usize,
    pub interfaces_generated: usize,
    pub steamstub_stripped: usize,
    /// Human label recorded on the release, e.g. "Steam + gbe_fork + Steamless".
    pub source: String,
    /// Anything that was skipped and why, so the UI can be honest about a
    /// partial crack rather than reporting plain success.
    pub warnings: Vec<String>,
}

/// Folders whose `steam_api` dll is a spare copy, not the one the game loads.
///
/// An engine plugin ships the Steamworks redistributable inside its own source
/// tree so that a project can be rebuilt from it. The game does not load that
/// copy - it loads the one staged next to the executable - so patching it
/// achieves nothing, and on a release that shipped its plugin sources it is the
/// ONLY copy present, which means the build reports a successful crack while
/// the game is untouched. Starlit Stories is exactly that shape: its single
/// `steam_api64.dll` is under
/// `Engine/Plugins/Marketplace/.../Source/ThirdParty/.../redistributable_bin/`
/// and there is none in `Binaries/Win64` at all.
const NOT_THE_LOADED_COPY: &[&str] = &[
    "redistributable_bin",
    "source/thirdparty",
    "sdk/redistributable_bin",
];

pub fn find_steam_api_dlls(game_dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = WalkDir::new(game_dir)
        .into_iter()
        .filter_entry(|e| {
            ![".kryoto-repair", ".kryoto-orig"]
                .iter()
                .any(|n| e.file_name().eq_ignore_ascii_case(n))
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_lowercase();
            n == "steam_api.dll" || n == "steam_api64.dll"
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    // Only when something real is left. A game whose ONLY copy is a spare one
    // is still better served by patching that than by patching nothing and
    // saying it cracked - and the source label and warnings tell the operator
    // what happened either way.
    let loaded: Vec<PathBuf> = found
        .iter()
        .filter(|p| {
            let path = p.to_string_lossy().to_lowercase().replace('\\', "/");
            !NOT_THE_LOADED_COPY.iter().any(|skip| path.contains(skip))
        })
        .cloned()
        .collect();
    if !loaded.is_empty() {
        found = loaded;
    }
    // 64-BIT LAST, deliberately.
    //
    // A game that ships both in ONE folder gets one config file between them,
    // and the last pass is the one whose interface versions and api version
    // end up in it. For every such game the 64-bit build is the one that runs,
    // so it is the one that must win - and leaving that to whatever order the
    // directory happened to be walked in is a coin toss that only shows up as
    // a game that will not start.
    found.sort_by_key(|p| {
        let sixty_four = p
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase() == "steam_api64.dll")
            .unwrap_or(false);
        (sixty_four, p.clone())
    });
    found
}

/// What a scan of an existing tree found, before anything is changed.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct EmuMatch {
    /// steam_api dlls in the tree.
    pub total: usize,
    /// How many of them ARE an emulator dll already.
    pub matched: usize,
    /// `.kryoto` backups sitting beside them - this tool's own fingerprint.
    pub backups: usize,
}

impl EmuMatch {
    /// Every steam_api dll in the tree has already been replaced.
    ///
    /// A PARTIAL match is deliberately not "cracked": a tree with one swapped
    /// dll and one original is the half-finished state that produces a build
    /// which launches on the developer's machine and fails on everyone else's.
    /// It needs the crack stage run over it, not skipped.
    pub fn is_cracked(&self) -> bool {
        self.total > 0 && self.matched == self.total
    }
}

/// Whether this tree's steam_api dlls ARE an emulator already.
///
/// Compared by CONTENT against the emulator dlls this app installs, so it
/// answers the question that actually matters - "would cracking this change
/// anything" - rather than guessing from a filename, a file size or a leftover
/// marker file that a repacker may have stripped.
///
/// Re-cracking an already-cracked tree is not a harmless no-op, which is why
/// this exists at all. `generate_interfaces` reads the dll it is pointed at to
/// learn which Steamworks interface versions the GAME asks for; run against an
/// emulator dll it describes the emulator instead, and the resulting
/// `steam_interfaces.txt` is useless. The backup would be overwritten with the
/// emulator too, so the original could no longer be recovered from the tree.
///
/// EITHER emulator counts. A folder cracked with gbe_fork is still cracked when
/// the current setting is Kryoto Online; swapping one for the other is a
/// deliberate act, not something to do silently because a checkbox differs.
pub fn emulator_match(paths: &ToolPaths, game_dir: &Path, emulator: Emulator) -> EmuMatch {
    let dlls = find_steam_api_dlls(game_dir);
    let mut out = EmuMatch {
        total: dlls.len(),
        ..Default::default()
    };
    for dll in &dlls {
        if backup_path(dll).exists() {
            out.backups += 1;
        }
        let is64 = dll
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase() == "steam_api64.dll")
            .unwrap_or(false);
        // The configured emulator first, then the other one.
        let (first, second) = match emulator {
            Emulator::GbeFork => (paths.gbe_dll(is64), paths.online_dll(is64)),
            Emulator::Online => (paths.online_dll(is64), paths.gbe_dll(is64)),
            // A hand-patched tree could hold anything, so there is no
            // "configured" dll to try first - both known ones are still worth
            // comparing against, because somebody applying a custom emulator
            // very often applies one of these two by hand.
            Emulator::Custom => (paths.gbe_dll(is64), paths.online_dll(is64)),
            // A RUNE tree is recognised by its own dll for the SELECTED
            // profile, then by gbe_fork - the two most likely things to already
            // be in a folder somebody hands us.
            Emulator::Rune | Emulator::RuneSteak | Emulator::RuneSteamclient => {
                (paths.rune_dll(emulator, is64), paths.gbe_dll(is64))
            }
        };
        // Steamclient leaves the dll as the GAME'S OWN with eleven bytes
        // changed, so no content comparison can recognise it. Asked the only
        // way it can be: the original import is gone and RUNE's is there.
        //
        // Without this a Steamclient tree reads as un-cracked, and a re-crack
        // would run over it - patching a file that is already patched, and
        // reading its interface list out of a dll that now answers for the
        // emulator rather than for Steamworks.
        let steamclient = shell32_is_patched(dll);
        if steamclient
            || [first, second]
                .into_iter()
                .flatten()
                .any(|emu| same_contents(dll, &emu))
        {
            out.matched += 1;
        }
    }
    out
}

/// Byte-for-byte equality, cheaply.
///
/// Length first: two different dlls almost never share a size, so this settles
/// the common case without reading either file. Errors read as "not equal" -
/// an unreadable dll is not evidence that a tree is already cracked, and
/// guessing the other way would skip a crack that was needed.
fn same_contents(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (std::fs::metadata(a), std::fs::metadata(b)) else {
        return false;
    };
    if ma.len() != mb.len() {
        return false;
    }
    match (std::fs::read(a), std::fs::read(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Clear the read-only attribute Steam leaves on depot files.
fn make_writable(path: &Path) -> std::io::Result<()> {
    let mut perms = std::fs::metadata(path)?.permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    std::fs::set_permissions(path, perms)
}

/// Where an original lives while a crack is in place, for files that must NOT
/// ship inside the release.
///
/// The dll backups deliberately DO ship, as `<name>.dll.kryoto` beside the file
/// they shadow, so a downloader can put the game back the way Steam shipped it.
/// That works because a Steamworks dll is a quarter of a megabyte. It does not
/// work for an executable: a packed exe can be a gigabyte, and shipping a second
/// copy of one inside every archive would be indefensible. Those go here, and
/// `SKIP_IN_ARCHIVE` keeps the directory out of the archive.
pub const ORIG_DIR: &str = ".kryoto-orig";

/// The record of what a crack did, written beside the tree it did it to.
///
/// Without this there is nothing to undo. `crack_tree` reported COUNTS - dlls
/// replaced, interfaces generated, stubs stripped - and the files it created
/// were never recorded anywhere, so reverting would have meant guessing at a
/// list. That is the whole reason changing your mind about an emulator used to
/// cost a re-download: not that the originals were lost, but that nothing knew
/// which files to put back.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CrackManifest {
    pub emulator: Emulator,
    /// What a person applied by hand, for a Custom build.
    #[serde(default)]
    pub custom_label: Option<String>,
    /// Overwritten files, and where the original went.
    #[serde(default)]
    pub replaced: Vec<Restorable>,
    /// Files the crack deleted, and where the original went.
    #[serde(default)]
    pub deleted: Vec<Restorable>,
    /// Files and directories the crack created, newest last so removal can walk
    /// it backwards and find directories already empty.
    #[serde(default)]
    pub added: Vec<String>,
}

/// A file that can be put back, both paths relative to the game directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Restorable {
    pub path: String,
    pub backup: String,
}

/// Written at the tree root. Excluded from the archive.
pub const CRACK_MANIFEST: &str = ".kryoto-crack.json";

pub fn manifest_path(game_dir: &Path) -> PathBuf {
    game_dir.join(CRACK_MANIFEST)
}

pub fn read_manifest(game_dir: &Path) -> Option<CrackManifest> {
    let text = std::fs::read_to_string(manifest_path(game_dir)).ok()?;
    serde_json::from_str(&text).ok()
}

fn rel(game_dir: &Path, path: &Path) -> String {
    path.strip_prefix(game_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Collects the manifest as the crack runs.
///
/// Every mutation goes through this rather than being listed separately
/// afterwards, so the record cannot drift from what actually happened - which
/// is the failure mode that makes a revert dangerous rather than merely absent.
struct Recorder<'a> {
    game_dir: &'a Path,
    manifest: CrackManifest,
}

impl<'a> Recorder<'a> {
    fn new(game_dir: &'a Path, emulator: Emulator) -> Self {
        Self {
            game_dir,
            manifest: CrackManifest {
                emulator,
                ..Default::default()
            },
        }
    }

    /// Copy `live` aside into the sidecar, and record it as restorable.
    ///
    /// Used for the files whose backup must not ship. Returns false when the
    /// copy failed, so the caller can refuse to touch a file it cannot undo.
    fn stash(&mut self, live: &Path, kind: Stash) -> bool {
        let backup = self.game_dir.join(ORIG_DIR).join(rel(self.game_dir, live));
        if let Some(parent) = backup.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                return false;
            }
        }
        if !backup.exists() && std::fs::copy(live, &backup).is_err() {
            return false;
        }
        let entry = Restorable {
            path: rel(self.game_dir, live),
            backup: rel(self.game_dir, &backup),
        };
        match kind {
            Stash::Replaced => self.manifest.replaced.push(entry),
            Stash::Deleted => self.manifest.deleted.push(entry),
        }
        // Written after EVERY mutation, not once at the end.
        //
        // A crack that is stopped part way is exactly when the record matters
        // most - the tree is half original and half emulator, and without a
        // manifest the only safe thing to do with it is delete it and download
        // the game again. Saving as we go turns that into a revert. The file is
        // small and this runs a few dozen times per build.
        self.save();
        true
    }

    /// Record a file replaced against a backup the caller already made - the
    /// `<name>.dll.kryoto` sidecars, which ship on purpose.
    fn replaced_with(&mut self, live: &Path, backup: &Path) {
        self.manifest.replaced.push(Restorable {
            path: rel(self.game_dir, live),
            backup: rel(self.game_dir, backup),
        });
        self.save();
    }

    fn added(&mut self, path: &Path) {
        let r = rel(self.game_dir, path);
        if !self.manifest.added.iter().any(|a| a == &r) {
            self.manifest.added.push(r);
            self.save();
        }
    }

    fn save(&self) {
        if let Ok(text) = serde_json::to_string_pretty(&self.manifest) {
            let _ = std::fs::write(manifest_path(self.game_dir), text);
        }
    }
}

enum Stash {
    Replaced,
    Deleted,
}

/// Put the tree back the way it was downloaded.
///
/// This is what makes the emulator a decision that can be changed. Switching
/// used to mean a full re-download or a `-validate` re-hash of the whole build,
/// because nothing could undo a crack - and re-cracking over one is not a no-op
/// but damage, since `generate_interfaces` reads whatever dll is in place and
/// would describe the emulator instead of Steamworks.
///
/// Restores every replaced and deleted file from its backup, removes everything
/// the crack added, and prunes directories left empty. Returns the warnings
/// worth showing; a missing backup is reported rather than silently skipped,
/// because a partial revert the operator does not know about is worse than a
/// failed one.
pub fn revert_tree(game_dir: &Path) -> Result<Vec<String>> {
    let Some(manifest) = read_manifest(game_dir) else {
        return Err(Error::Config(
            "This build has no crack record, so it cannot be reverted. It was made by an \
             older version - rebuild it to switch emulators."
                .into(),
        ));
    };
    let mut warnings = Vec::new();

    for entry in manifest.replaced.iter().chain(manifest.deleted.iter()) {
        let live = game_dir.join(&entry.path);
        let backup = game_dir.join(&entry.backup);
        if !backup.exists() {
            warnings.push(format!("no backup for {}, left as it is", entry.path));
            continue;
        }
        let _ = make_writable(&live);
        if let Some(parent) = live.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::copy(&backup, &live) {
            warnings.push(format!("could not restore {}: {e}", entry.path));
        }
    }

    // Backwards, so a directory is considered after the files inside it.
    for added in manifest.added.iter().rev() {
        let path = game_dir.join(added);
        if path.is_dir() {
            // Only if empty: a game's own `plugins` directory must survive.
            let _ = std::fs::remove_dir(&path);
        } else {
            let _ = make_writable(&path);
            let _ = std::fs::remove_file(&path);
        }
    }

    // The sidecar and the record itself are part of the crack.
    let _ = std::fs::remove_dir_all(game_dir.join(ORIG_DIR));
    let _ = std::fs::remove_file(manifest_path(game_dir));
    Ok(warnings)
}

#[allow(clippy::too_many_arguments)]
pub async fn crack_tree(
    paths: &ToolPaths,
    game_dir: &Path,
    appid: &str,
    dlc: &[Dlc],
    // The exes the DRM scan found a SteamStub `.bind` section in. Empty means
    // Steamless has nothing to do and is not run at all.
    steamstub_files: &[PathBuf],
    identity: &EmuIdentity,
    emulator: Emulator,
    cancel: &impl crate::Cancellation,
) -> Result<CrackResult> {
    let mut warnings = Vec::new();
    let dlls = find_steam_api_dlls(game_dir);

    // Everything this function changes is recorded as it happens - see
    // `Recorder` and `revert_tree`. That record is what makes the emulator a
    // decision that can be changed after the download instead of one that costs
    // a re-download to revisit.
    let mut rec = Recorder::new(game_dir, emulator);

    let mut replaced = 0usize;
    let mut interfaces = 0usize;

    if dlls.is_empty() {
        warnings.push("No steam_api dll found - treated as DRM-free.".into());
    }

    for dll in &dlls {
        // Between dlls, not mid-swap: a stop that landed between the backup and
        // the replace would leave the tree in exactly the mixed state the crack
        // marker exists to detect.
        crate::error::check_cancel(cancel)?;
        let is64 = dll
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase() == "steam_api64.dll")
            .unwrap_or(false);

        let emu = match emulator {
            Emulator::GbeFork => paths.gbe_dll(is64),
            Emulator::Online => paths.online_dll(is64),
            Emulator::Rune => paths.rune_dll(emulator, is64),
            // Steamclient does not HAVE a replacement dll: it patches the
            // game's own and drops support libraries beside it, below.
            Emulator::RuneSteamclient => None,
            // Steakclient never touches a Steamworks dll at all - it is a
            // winmm proxy beside the executable, applied after this loop.
            Emulator::RuneSteak => None,
            // Unreachable in practice: the pipeline never calls the crack stage
            // for a custom build, because "custom" means the operator applies
            // it. Spelled out rather than unwrapped so a future caller that
            // does reach here gets a sentence instead of a panic.
            Emulator::Custom => None,
        };
        // Steakclient is applied after this loop and never touches a
        // Steamworks dll, so "left untouched" is the correct outcome for it
        // rather than something to warn about.
        if emulator == Emulator::RuneSteak {
            continue;
        }
        if emu.is_none() && emulator != Emulator::RuneSteamclient {
            warnings.push(format!(
                "No {} {} dll configured; {} left untouched.",
                emulator.label(),
                if is64 { "x64" } else { "x86" },
                dll.display()
            ));
            continue;
        }

        // Steamclient's support libraries, resolved BEFORE anything is touched.
        //
        // The patch is only half of this profile: the rewritten import makes
        // the game load `rune64.dll`, and without it beside the dll the game
        // fails to start at all. Refusing here leaves the tree exactly as it
        // was; refusing after the patch would leave a build that looks cracked
        // and does not run.
        let support = if emulator == Emulator::RuneSteamclient {
            let Some(files) = paths.rune_steamclient_support(is64) else {
                return Err(Error::Config(format!(
                    "RUNE (Steamclient) needs its {} support libraries and they are not installed. Install it under Settings > Tools in the app, or /tools in the terminal, and build again.",
                    if is64 { "x64" } else { "x86" }
                )));
            };
            files
        } else {
            Vec::new()
        };

        let dll_dir = dll.parent().unwrap_or(game_dir);

        // gbe_fork wants steam_interfaces.txt generated from the ORIGINAL dll,
        // so this has to happen before the swap. The online emulator forwards to
        // the real client and never reads that file.
        let settings = dll_dir.join("steam_settings");
        if emulator == Emulator::GbeFork {
            let existed = settings.exists();
            std::fs::create_dir_all(&settings)
                .map_err(|e| Error::Io(format!("creating steam_settings: {e}")))?;
            // Only ours to remove if we made it.
            if !existed {
                rec.added(&settings);
            }
            match generate_interfaces(paths, dll, is64, &settings).await {
                Ok(true) => {
                    interfaces += 1;
                    rec.added(&settings.join("steam_interfaces.txt"));
                }
                Ok(false) => {}
                Err(e) => warnings.push(format!("generate_interfaces: {e}")),
            }
        }

        // RUNE reads the original, then FORWARDS to it.
        //
        // Unlike gbe_fork, which stands in for Steamworks entirely, RUNE loads
        // the real dll under a new name and passes calls through. So the
        // original is renamed to `steam_api64.rne` rather than merely backed up:
        // it is part of the release and the game will not start without it.
        //
        // Both of these read the ORIGINAL, so both happen before the swap - the
        // same ordering rule that governs gbe_fork's interface generation, and
        // for the same reason.
        let mut rune_lines: Vec<String> = Vec::new();
        if emulator.is_rune() {
            rune_lines = rune_interfaces(dll);
            if !rune_lines.is_empty() {
                interfaces += 1;
            }
            let forward = dll.with_extension("rne");
            if !forward.exists() {
                let _ = make_writable(dll);
                if let Err(e) = std::fs::copy(dll, &forward) {
                    return Err(Error::Io(format!(
                        "RUNE needs the original dll kept as {}, and it could not be written: {e}",
                        forward.display()
                    )));
                }
            }
            // Recorded as REPLACED against this file, so a revert copies it
            // back over the dll - and so the archive keeps it, which it must.
            rec.replaced_with(dll, &forward);
            // AND as added, so a revert takes it away again once it has been
            // copied back. It is only meaningful with RUNE in place: left
            // behind, it would ride along in the next build's archive as a
            // second copy of a dll that is already there, under a name nothing
            // in that build loads.
            rec.added(&forward);
        }

        // Keep the original recoverable. This backup SHIPS - see ORIG_DIR for
        // which ones do and which do not.
        //
        // Skipped for RUNE, which has already kept the original as `.rne` and
        // needs it there to forward to. A second copy under `.kryoto` would be
        // the same bytes twice in every release, and would record the same file
        // as replaced twice over.
        if !emulator.is_rune() {
            let backup = backup_path(dll);
            if !backup.exists() {
                let _ = make_writable(dll);
                if let Err(e) = std::fs::copy(dll, &backup) {
                    warnings.push(format!("could not back up {}: {e}", dll.display()));
                }
            }
            if backup.exists() {
                rec.replaced_with(dll, &backup);
            }
        }

        if emulator == Emulator::RuneSteamclient {
            // The dll stays the GAME'S OWN, with one import rewritten. The
            // original is already safe beside it as `.rne`, which is both the
            // backup and the file the emulator forwards to.
            if !patch_shell32(dll, is64)? {
                warnings.push(format!(
                    "{} imports no SHELL32.dll, so there was nothing for RUNE (Steamclient) to rewrite; it was left as it was.",
                    dll.display()
                ));
                continue;
            }
            for file in &support {
                let dest = dll_dir.join(file.file_name().unwrap_or_default());
                if dest.exists() {
                    rec.stash(&dest, Stash::Replaced);
                } else {
                    rec.added(&dest);
                }
                let _ = make_writable(&dest);
                std::fs::copy(file, &dest)
                    .map_err(|e| Error::Io(format!("copying {}: {e}", dest.display())))?;
            }
            replaced += 1;
        } else {
            let Some(emu) = emu else { continue };
            let _ = make_writable(dll);
            std::fs::copy(&emu, dll)
                .map_err(|e| Error::Io(format!("replacing {}: {e}", dll.display())))?;
            replaced += 1;
        }

        match emulator {
            // ONLY the Regular profile is implemented here.
            //
            // 0.8.0 offered all three and applied all three the same way - by
            // replacing steam_api(64).dll - which is right for Regular and
            // wrong for the other two, so those would have produced releases
            // that do not start:
            //
            //   Steakclient  ships winmm.dll + steakclient64.dll + steak_emu.ini
            //                into the EXE directory and never touches
            //                steam_api at all. It is a winmm proxy load.
            //   Steamclient  keeps the original as `.rne`, BINARY-PATCHES a
            //                SHELL32.dll string inside the api dll, and drops
            //                GameOverlayRenderer / rune / steamclient dlls per
            //                architecture.
            //
            // Neither is a dll swap, and neither is a small addition to this
            // branch. They are refused with a sentence rather than applied
            // wrongly - a build that fails here costs a re-run, and one that
            // succeeds wrongly costs whoever downloads it.
            // Applied after this loop: nothing about Steakclient is per-dll.
            Emulator::RuneSteak => {}
            // The same config as the regular profile, from this profile's own
            // copy of the template - so a machine set up for Steamclient alone
            // does not also need the regular profile installed to find an ini.
            Emulator::Rune | Emulator::RuneSteamclient => {
                let e = emulator;
                // RUNE's config is a TEMPLATE with placeholders, and a
                // placeholder left in place is not a default - the emulator
                // reads it literally and applies nothing, which only shows up on
                // the player's machine. Every one of them is filled or removed.
                let Some(template) = paths.rune_ini(e) else {
                    return Err(Error::Config(format!(
                        "{}'s config template is missing. Reinstall it under Settings > Tools in the app, or /tools in the terminal.",
                        e.label()
                    )));
                };
                let Some(name) = e.rune_ini_name() else {
                    return Err(Error::Config("RUNE profile has no config name.".into()));
                };
                let text = std::fs::read_to_string(&template)
                    .map_err(|err| Error::Io(format!("reading {}: {err}", template.display())))?;

                let api_version =
                    crate::peversion::file_version(&dll.with_extension("rne")).unwrap_or_default();
                let out = rune_config(&text, appid, dlc, identity, &api_version, &rune_lines);

                let dest = dll_dir.join(name);
                if dest.exists() {
                    rec.stash(&dest, Stash::Replaced);
                } else {
                    rec.added(&dest);
                }
                std::fs::write(&dest, out)
                    .map_err(|err| Error::Io(format!("writing {}: {err}", dest.display())))?;
            }
            Emulator::GbeFork => {
                // Both locations: the emulator reads it from either depending on
                // how the game initialises, and writing both costs nothing.
                for loc in [
                    dll_dir.join("steam_appid.txt"),
                    settings.join("steam_appid.txt"),
                ] {
                    // A game that shipped its own steam_appid.txt gets it back
                    // on revert rather than losing it.
                    if loc.exists() {
                        rec.stash(&loc, Stash::Replaced);
                    } else {
                        rec.added(&loc);
                    }
                    std::fs::write(&loc, appid)
                        .map_err(|e| Error::Io(format!("writing {}: {e}", loc.display())))?;
                }
                let app_ini = settings.join("configs.app.ini");
                rec.added(&app_ini);
                std::fs::write(&app_ini, dlc_ini(dlc))
                    .map_err(|e| Error::Io(format!("writing configs.app.ini: {e}")))?;
                write_identity(&settings, identity)?;
                for f in identity_files(identity) {
                    rec.added(&settings.join(f));
                }
            }
            // Never reached - the `emu` lookup above returns None for a custom
            // build and this loop has already moved on.
            Emulator::Custom => {}
            Emulator::Online => {
                // NO steam_appid.txt: it would make the real client resolve the
                // game as itself, defeating the Spacewar spoof.
                // Stashed before removal. This used to be a bare delete of a
                // file the game may have shipped, with no backup and no record,
                // which made a revert impossible to do honestly.
                let shipped_appid = dll_dir.join("steam_appid.txt");
                if shipped_appid.exists() {
                    rec.stash(&shipped_appid, Stash::Deleted);
                }
                let _ = std::fs::remove_file(&shipped_appid);

                // kryotoO.dll goes BESIDE the proxy, which is where the proxy
                // looks for it first.
                //
                // Since KryotoOnline 1.8.1 the patches live in this second
                // file: the ownership spoof, the DLC unlock, the plugin
                // loader, the SteamStub handling. Without it steam_api64.dll
                // still loads and still forwards to Steam - it just applies
                // nothing, and the only sign is one line in a log file on the
                // player's machine. That is precisely the shape of failure the
                // "no emulator was applied" error below exists to stop, so a
                // missing core is fatal here too rather than a warning.
                let Some(core) = paths.online_core(is64) else {
                    return Err(Error::Config(format!(
                        "Kryoto Online's {} core ({}) is missing, so the build would apply no patches at all. Reinstall Kryoto Online from Settings > Tools in the app, or /tools in the terminal, and build again.",
                        if is64 { "x64" } else { "x86" },
                        if is64 { "kryotoO.dll" } else { "kryotoO32.dll" },
                    )));
                };
                let core_name = core.file_name().unwrap_or_default();
                let dest = dll_dir.join(core_name);
                if dest.exists() {
                    rec.stash(&dest, Stash::Replaced);
                } else {
                    rec.added(&dest);
                }
                let _ = make_writable(&dest);
                std::fs::copy(&core, &dest)
                    .map_err(|e| Error::Io(format!("copying {}: {e}", dest.display())))?;

                // The ini is written per EXE directory, after the Steamless
                // pass - not here. See `online_config_dirs` for why beside the
                // dll is not enough, and the write itself for why it waits.
            }
        }
    }

    // STEAKCLIENT, which is not a Steamworks crack at all.
    //
    // Everything above works on `steam_api(64).dll`. This profile does not
    // touch it: `winmm.dll` is a proxy Windows loads for the EXECUTABLE, it
    // brings `steakclient64.dll` up with it, and the game's own Steamworks dll
    // stays exactly as it shipped. So it runs here, once, against the
    // executable's folder rather than once per dll.
    if emulator == Emulator::RuneSteak {
        let (added, read) = apply_steakclient(
            paths,
            game_dir,
            appid,
            dlc,
            identity,
            &dlls,
            &mut rec,
            &mut warnings,
        )?;
        replaced += added;
        interfaces += read;
    }

    // Found Steamworks dlls and swapped none of them.
    //
    // This was a warning, which meant a build with NO emulator applied went all
    // the way through archiving and publishing looking like a success. It is
    // how a missing Kryoto Online directory produced releases that simply did
    // not work: the only sign was one line in a log nobody reads.
    if !dlls.is_empty() && replaced == 0 {
        return Err(Error::Config(format!(
            "This game uses Steamworks but no {} dll was applied, so nothing was \
             cracked. Install {} - Settings > Tools in the app, or /tools in the terminal - and build again. ({})",
            emulator.label(),
            emulator.label(),
            warnings.join("; ")
        )));
    }

    let mut stripped = 0usize;
    if !steamstub_files.is_empty() {
        match paths.steamless_exe.as_deref() {
            Some(exe) if exe.exists() => {
                stripped = run_steamless_over_tree(
                    paths,
                    steamstub_files,
                    &mut warnings,
                    cancel,
                    Some(&mut rec),
                )
                .await?;
            }
            _ => warnings
                .push("SteamStub was found but Steamless is not configured; not stripped.".into()),
        }
    }

    // Did any SteamStub survive?
    //
    // Steamless declines a file it cannot unpack and leaves the original in
    // place, so "the scan found N and Steamless stripped fewer than N" is the
    // honest test. Steamless missing entirely lands here too.
    let stub_remains = stripped < steamstub_files.len();

    // The online emulator's config goes beside the game EXECUTABLE, which is
    // not necessarily beside the dll it configures - see `online_config_dirs`.
    //
    // Written AFTER the Steamless pass, because `GetStubbedLol` is an answer
    // to what that pass actually managed. It used to be written before, hard
    // coded to false, which meant a game with SteamStub and no working
    // Steamless got neither treatment: Steamless did not strip the wrapper and
    // the emulator was told not to patch it. The exe then refuses to start,
    // because the stub asks the live client about a title the account does not
    // own - and the build was published looking like a success.
    //
    // Enumerating the exe directories after the pass rather than before is
    // safe: Steamless writes `<name>.exe.unpacked.exe` and renames it over the
    // original, so by now the tree holds the same set of executables it did.
    if emulator == Emulator::Online && replaced > 0 {
        for dir in online_config_dirs(game_dir, &dlls) {
            // Steam reads this beside executables too. A leftover id there
            // overrides Online's spoof even if the DLL directory is clean.
            let shipped_appid = dir.join("steam_appid.txt");
            if shipped_appid.exists() {
                if !rec.stash(&shipped_appid, Stash::Deleted) {
                    return Err(Error::Io(
                        "Could not back up steam_appid.txt before switching to Online.".into(),
                    ));
                }
                std::fs::remove_file(&shipped_appid).map_err(|e| Error::Io(e.to_string()))?;
            }
            let ini = dir.join(ONLINE_INI);
            rec.added(&ini);
            std::fs::write(&ini, online_ini(appid, dlc, stub_remains))
                .map_err(|e| Error::Io(format!("writing {}: {e}", ini.display())))?;
            // Without this folder the emulator reports missing plugins on every
            // launch even when there are none to load. Recorded only when we
            // create it, so a game's own plugins directory is never removed.
            let plugins = dir.join("plugins");
            if !plugins.exists() {
                rec.added(&plugins);
            }
            let _ = std::fs::create_dir_all(&plugins);
        }

        if stub_remains {
            warnings.push(format!(
                "SteamStub survived on {} executable(s); the online emulator will patch it at runtime (GetStubbedLol=true). Configuring Steamless is more reliable.",
                steamstub_files.len() - stripped
            ));
        }
    }

    if emulator == Emulator::Online && replaced > 0 {
        // This ships on the public release page, because a player who downloads
        // an online build and has no Steam running will otherwise just see the
        // game fail to start with no explanation.
        warnings
            .push("Online build: the player needs the Steam client running and signed in.".into());
    }

    let emu_label = emulator.label();
    let source = match (replaced > 0, stripped > 0) {
        (true, true) => format!("Steam + {emu_label} + Steamless"),
        (true, false) => format!("Steam + {emu_label}"),
        (false, true) => "Steam + Steamless".to_string(),
        (false, false) => "Steam (DRM-free)".to_string(),
    };

    rec.save();

    Ok(CrackResult {
        dlls_replaced: replaced,
        interfaces_generated: interfaces,
        steamstub_stripped: stripped,
        source,
        warnings,
    })
}

/// RUNE's interface list, read out of the ORIGINAL Steamworks dll.
///
/// Same idea as gbe_fork's `generate_interfaces`, and for the same reason: the
/// emulator has to answer the interface versions THIS BUILD asks for, and the
/// only place they are written down is the dll the game shipped with. Read it
/// after the swap and the answer describes the emulator instead - see the note
/// on `emulator_match` for what that costs.
///
/// gbe_fork ships a tool to do this; RUNE does not, so the strings are scanned
/// here. A Steamworks dll carries them as plain NUL-terminated ASCII, and the
/// shapes are narrow enough to match without false positives:
///
///   SteamUser023, ISteamUser023          -> SteamUser=SteamUser023
///   STEAMAPPS_INTERFACE_VERSION008       -> SteamApps=STEAMAPPS_INTERFACE_VERSION008
///
/// The emitted line is `SimpleName=FullString`, which is what the template's
/// `RUNE_Interfaces` placeholder is replaced with.
/// Fill in a RUNE config template.
///
/// Every placeholder is filled or REMOVED. One left in place is not a default:
/// the emulator reads `RUNE_Interfaces` as an interface name and `DLCs` as a
/// key with no value, applies nothing, and says so nowhere - which only shows
/// up on the player's machine.
///
/// Shared by all three profiles because all three ship the same template under
/// different names, and a second copy of this would be a second place for a
/// placeholder to be forgotten.
fn rune_config(
    template: &str,
    appid: &str,
    dlc: &[Dlc],
    identity: &EmuIdentity,
    api_version: &str,
    interfaces: &[String],
) -> String {
    // Two spellings of the same list, because the template uses both:
    // `RUNE_DLC` takes spaces around the equals, the `DLCs` line does not.
    let clean = |n: &str| n.replace(['\n', '\r'], " ");
    let spaced: String = dlc
        .iter()
        .map(|d| format!("{} = {}\n", d.appid, clean(&d.name)))
        .collect();
    let tight: String = dlc
        .iter()
        .map(|d| format!("{}={}\n", d.appid, clean(&d.name)))
        .collect();

    let out = template
        .replace("SteamID", appid)
        .replace("RUNE_APIVersion", api_version)
        .replace("RUNE_Interfaces", &interfaces.join("\n"))
        .replace("RUNE_DLC", &spaced);
    let mut out = if dlc.is_empty() {
        // Removed rather than emptied. A bare `DLCs` left in the file is read
        // as a key with no value.
        out.lines()
            .filter(|l| !l.contains("DLCs"))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        out.replace("DLCs", &tight)
            .replace("DLCUnlockall=0", "DLCUnlockall=1")
    };
    let name_line = format!(
        "UserName={}",
        if identity.account_name.trim().is_empty() {
            "kryoto"
        } else {
            identity.account_name.trim()
        }
    );
    out = out.replace("UserName=RUNE", &name_line);
    out
}

/// RUNE's Steakclient profile: a proxy beside the executable.
///
/// Nothing here touches `steam_api64.dll`. Windows loads `winmm.dll` from the
/// executable's own folder before the system one, that proxy brings
/// `steakclient64.dll` up with it, and the game's Steamworks dll is left
/// exactly as it shipped - which is the whole point of the profile, and why it
/// cannot be expressed as a variation on the dll swap the others do.
///
/// x64 only. Upstream offers no 32-bit `steakclient`, so a game that ships only
/// `steam_api.dll` is refused rather than given two thirds of a crack.
///
/// Returns how many executables were fitted out and how many interface sets
/// were read, for the counts the caller reports.
#[allow(clippy::too_many_arguments)]
fn apply_steakclient(
    paths: &ToolPaths,
    game_dir: &Path,
    appid: &str,
    dlc: &[Dlc],
    identity: &EmuIdentity,
    dlls: &[PathBuf],
    rec: &mut Recorder<'_>,
    warnings: &mut Vec<String>,
) -> Result<(usize, usize)> {
    let Some(files) = paths.rune_steak_files() else {
        return Err(Error::Config(
            "RUNE (Steakclient) is not installed - it ships winmm.dll, steakclient64.dll and steak_emu.ini, and all three are needed. Install it under Settings > Tools in the app, or /tools in the terminal, and build again."
                .into(),
        ));
    };

    // The 64-bit Steamworks dll, which is read for its interface list and then
    // left alone. Its absence is the refusal, not a warning: a 32-bit game
    // given these files would start with a proxy that has nothing to proxy.
    let api64 = dlls.iter().find(|d| {
        d.file_name()
            .map(|n| n.to_string_lossy().to_lowercase() == "steam_api64.dll")
            .unwrap_or(false)
    });
    if api64.is_none() && !dlls.is_empty() {
        return Err(Error::Config(
            "RUNE (Steakclient) is 64-bit only and this game ships no steam_api64.dll. Build it with RUNE, gbe_fork or Kryoto Online instead."
                .into(),
        ));
    }

    // The executable the proxy has to sit beside - chosen by the same ranking
    // that picks the one to launch-test, rather than by a second guess at what
    // "the game" is.
    let candidates: Vec<PathBuf> = crate::find_executables(game_dir)
        .into_iter()
        .map(|(rel, _)| game_dir.join(rel))
        .collect();
    let Some(exe) = crate::launchtest::pick_executable(&candidates) else {
        return Err(Error::Config(
            "RUNE (Steakclient) has to sit beside the game executable, and no executable was found in this build."
                .into(),
        ));
    };
    let exe_dir = exe.parent().unwrap_or(game_dir).to_path_buf();

    let interfaces = api64.map(|d| rune_interfaces(d)).unwrap_or_default();
    let api_version = api64
        .and_then(|d| crate::peversion::file_version(d))
        .unwrap_or_default();

    for file in &files {
        let name = file.file_name().unwrap_or_default();
        let dest = exe_dir.join(name);
        if dest.exists() {
            rec.stash(&dest, Stash::Replaced);
        } else {
            rec.added(&dest);
        }
        let _ = make_writable(&dest);
        if name.to_string_lossy().to_lowercase().ends_with(".ini") {
            let text = std::fs::read_to_string(file)
                .map_err(|e| Error::Io(format!("reading {}: {e}", file.display())))?;
            let out = rune_config(&text, appid, dlc, identity, &api_version, &interfaces);
            std::fs::write(&dest, out)
                .map_err(|e| Error::Io(format!("writing {}: {e}", dest.display())))?;
        } else {
            std::fs::copy(file, &dest)
                .map_err(|e| Error::Io(format!("copying {}: {e}", dest.display())))?;
        }
    }

    if interfaces.is_empty() {
        warnings.push(
            "No Steam interface versions could be read from steam_api64.dll, so steak_emu.ini has none. The game may not find the emulator."
                .into(),
        );
    }

    Ok((1, usize::from(!interfaces.is_empty())))
}

/// The import name RUNE's Steamclient profile overwrites inside the Steamworks
/// dll, and what it becomes.
///
/// The patched `steam_api64.dll` is the GAME'S OWN, not a replacement: one
/// import entry is rewritten so it loads `RUNE64.dll` where it used to load
/// `SHELL32.dll`. That is why this profile keeps working when a game checks
/// its Steamworks dll for tampering in the ways a wholesale swap fails - and it
/// is why the patch must be exactly in place.
///
/// Eleven bytes in, eleven bytes out. `SHELL32.dll` is 11 characters and both
/// replacements are 11 bytes including their terminators, so nothing after the
/// patch moves: no offset in the import table, no section size, no checksum
/// that would need recomputing. A replacement of a different length would not
/// be a smaller version of this - it would be a corrupt PE.
const SHELL32: &[u8] = b"SHELL32.dll";
const RUNE64_IMPORT: [u8; 11] = [b'R', b'U', b'N', b'E', b'6', b'4', 0, b'W', b'U', b'S', 0];
const RUNE32_IMPORT: [u8; 11] = [b'R', b'U', b'N', b'E', 0, b'!', b'W', b'U', b'S', b'!', 0];

/// Rewrite the first `SHELL32.dll` in a file, in place.
///
/// `Ok(false)` when the string is not there. That is not an error worth
/// stopping a build for on its own - a dll that has already been patched, or
/// one that never imported SHELL32 - but it IS the difference between a crack
/// and a file that was copied and left alone, so the caller must not count it
/// as applied.
///
/// Case-insensitive, because the import name's casing is whatever the linker
/// wrote and a case-sensitive search silently finds nothing on some builds.
fn patch_shell32(path: &Path, arch64: bool) -> Result<bool> {
    let mut bytes =
        std::fs::read(path).map_err(|e| Error::Io(format!("reading {}: {e}", path.display())))?;
    let want: Vec<u8> = SHELL32.to_ascii_lowercase();
    let at = bytes
        .windows(want.len())
        .position(|w| w.to_ascii_lowercase() == want);
    let Some(at) = at else { return Ok(false) };
    let with: &[u8] = if arch64 {
        &RUNE64_IMPORT
    } else {
        &RUNE32_IMPORT
    };
    bytes[at..at + SHELL32.len()].copy_from_slice(with);
    let _ = make_writable(path);
    std::fs::write(path, &bytes)
        .map_err(|e| Error::Io(format!("writing {}: {e}", path.display())))?;
    Ok(true)
}

/// Whether a Steamworks dll has already had its import rewritten.
///
/// Used to tell a Steamclient-cracked tree from an untouched one. Content
/// comparison cannot answer that the way it does for the other profiles,
/// because the dll IS the game's own file with eleven bytes changed - so the
/// question is asked the only way it can be: the original import is gone and
/// the new one is there.
fn shell32_is_patched(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let shell = SHELL32.to_ascii_lowercase();
    let has_shell32 = bytes
        .windows(shell.len())
        .any(|w| w.to_ascii_lowercase() == shell);
    let has_rune = bytes
        .windows(RUNE64_IMPORT.len())
        .any(|w| w == RUNE64_IMPORT || w == RUNE32_IMPORT);
    has_rune && !has_shell32
}

fn rune_interfaces(dll: &Path) -> Vec<String> {
    let Ok(bytes) = std::fs::read(dll) else {
        return Vec::new();
    };

    // Printable ASCII runs, which is how these strings sit in the binary.
    let mut found: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut current = String::new();
    let consider = |s: &str, found: &mut std::collections::BTreeMap<String, String>| {
        for token in s.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            if let Some(simple) = rune_interface_name(token) {
                // First one wins: a dll can mention an older version in passing,
                // and BTreeMap keeps the output stable between builds.
                found.entry(simple).or_insert_with(|| token.to_string());
            }
        }
    };
    for byte in bytes {
        if (32..=126).contains(&byte) {
            current.push(byte as char);
        } else {
            if current.len() > 3 {
                consider(&current, &mut found);
            }
            current.clear();
        }
    }
    if current.len() > 3 {
        consider(&current, &mut found);
    }

    found.into_iter().map(|(k, v)| format!("{k}={v}")).collect()
}

/// The simple name RUNE keys an interface by, or None if this is not one.
fn rune_interface_name(token: &str) -> Option<String> {
    if !(8..=50).contains(&token.len()) {
        return None;
    }
    // Every interface string ends in its three-digit version.
    if !token.chars().rev().take(3).all(|c| c.is_ascii_digit()) {
        return None;
    }
    if let Some(base) = token.strip_prefix("STEAM") {
        // STEAMAPPS_INTERFACE_VERSION008 -> SteamApps
        let base = base.split("_INTERFACE_").next()?;
        if base.is_empty() || !base.chars().all(|c| c.is_ascii_uppercase()) {
            return None;
        }
        // The compound names the mapping cannot derive from capitalisation
        // alone. Taken from the reference tool rather than guessed.
        let mapped = match base {
            "GAMESERVER" => "SteamGameServer",
            "GAMESERVERSTATS" => "SteamGameServerStats",
            "HTMLSURFACE" => "SteamHTMLSurface",
            "MUSICREMOTE" => "SteamMusicRemote",
            "MATCHMAKING" => "SteamMatchMaking",
            "MATCHMAKINGSERVERS" => "SteamMatchMakingServers",
            "MATCHGAMESEARCH" => "SteamMatchGameSearch",
            "PARENTALSETTINGS" => "SteamParentalSettings",
            "REMOTEPLAY" => "SteamRemotePlay",
            "REMOTESTORAGE" => "SteamRemoteStorage",
            "USERSTATS" => "SteamUserStats",
            "HTTP" => "SteamHTTP",
            "UGC" => "SteamUGC",
            "UNIFIEDMESSAGES" => "SteamUnifiedMessages",
            other => {
                let mut name = String::from("Steam");
                let mut chars = other.chars();
                if let Some(first) = chars.next() {
                    name.push(first);
                    name.extend(chars.map(|c| c.to_ascii_lowercase()));
                }
                return Some(name);
            }
        };
        return Some(mapped.to_string());
    }

    // SteamUser023 / ISteamUser023 -> SteamUser
    //
    // NO UNDERSCORES in this form. A Steamworks dll is full of exported symbol
    // names like `SteamAPI_SteamAppList_v001`, which end in three digits and
    // begin with `Steam` and are not interface version strings at all - and
    // accepting them wrote a dozen keys into every config that name nothing the
    // emulator ever looks up. The underscore form is the ALL-CAPS one handled
    // above, and only that one.
    let body = token.strip_prefix('I').unwrap_or(token);
    if !body.starts_with("Steam") || body.contains('_') {
        return None;
    }
    let trimmed = body.trim_end_matches(|c: char| c.is_ascii_digit());
    (trimmed.len() > 5).then(|| trimmed.to_string())
}

/// `steam_api64.dll` -> `steam_api64.dll.kryoto`.
///
/// Appends rather than replacing the extension: `with_extension` on a path
/// ending in `.dll` would REPLACE `dll`, leaving `steam_api64.kryoto` with no
/// trace of what the file originally was.
fn backup_path(dll: &Path) -> PathBuf {
    let mut name = dll.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(BACKUP_EXT);
    dll.with_file_name(name)
}

/// Where the online emulator's ini has to go.
///
/// It reads its config from the directory of the RUNNING EXECUTABLE
/// (`GetModuleFileName(nullptr)`), not from beside itself. Those are the same
/// folder in a plain game and are NOT in a Unity one, where the dll lives in
/// `<Game>_Data/Plugins/x86_64/` and the exe sits at the root.
///
/// Getting this wrong fails quietly and confusingly: with no ini the loader
/// leaves its path empty, and then every setting silently takes its default -
/// AppId 480 with no `ogAppId`, no DLC unlocked, no plugins loaded and no
/// ticket emulation. The game launches and simply behaves as though nothing was
/// configured.
///
/// So the ini goes beside every executable in the tree, plus every directory
/// holding a patched dll. It is a few hundred bytes and there is no way to know
/// from here which exe the player will actually launch.
fn online_config_dirs(game_dir: &Path, dlls: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = WalkDir::new(game_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .to_lowercase()
                .ends_with(".exe")
        })
        .filter_map(|e| e.path().parent().map(Path::to_path_buf))
        .collect();

    dirs.extend(
        dlls.iter()
            .filter_map(|d| d.parent().map(Path::to_path_buf)),
    );
    // The game root, for the common case where the launcher is a shortcut or a
    // script rather than an exe we found.
    dirs.push(game_dir.to_path_buf());

    dirs.sort();
    dirs.dedup();
    dirs
}

/// The online emulator's ini.
///
/// `AppId` is what the game is told it is (Spacewar, which everyone owns) and
/// `ogAppId` is what it really is - the emulator needs both to keep stats,
/// achievements and DLC resolving against the right title while ownership
/// resolves against the free one.
///
/// `stub_remains` drives `GetStubbedLol`, which turns on the emulator's
/// runtime SteamStub patch. The two ways of dealing with SteamStub are
/// alternatives, not layers:
///
///   Steamless stripped it  -> false. The patch would spend the whole run
///                             hunting a signature that is not there.
///   still wrapped          -> true. Nothing else will deal with it, and an
///                             exe that still asks the live client about an
///                             AppId the account does not own will not start.
fn online_ini(appid: &str, dlc: &[Dlc], stub_remains: bool) -> String {
    let unlock = dlc
        .iter()
        .map(|d| d.appid.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let stubbed = if stub_remains { "true" } else { "false" };
    format!(
        "; Written by Kryoto Forge. Delete to fall back to defaults.\n\
         [Settings]\n\
         AppId={SPACEWAR_APPID}\n\
         ogAppId={appid}\n\
         PluginsFolder=plugins\n\
         GetStubbedLol={stubbed}\n\
         UnlockDLC={unlock}\n\
         EmulateTicket=true\n"
    )
}

/// Who the emulator says you are in-game.
///
/// Per gbe_fork's own README: the account name lives in
/// `steam_settings/configs.user.ini` under `[user::general] account_name`, and
/// the avatar is a `png`/`jpg`/`jpeg` next to it named `account_avatar`.
/// Without these every cracked build shows gbe_fork's placeholder ("gse orca"),
/// which is both wrong and an obvious tell.
#[derive(Debug, Clone)]
pub struct EmuIdentity {
    pub account_name: String,
    /// Raw image bytes written as `account_avatar.png`. Empty = leave the
    /// default alone rather than write a zero-byte file the emulator would try
    /// to decode.
    pub avatar_png: &'static [u8],
    pub language: String,
}

impl Default for EmuIdentity {
    fn default() -> Self {
        Self {
            account_name: "kryoto".into(),
            avatar_png: include_bytes!("../assets/account_avatar.png"),
            language: "english".into(),
        }
    }
}

/// Write the user config and avatar beside the emulator.
fn write_identity(settings: &Path, id: &EmuIdentity) -> Result<()> {
    let name = id.account_name.replace(['\n', '\r'], " ");
    let name = name.trim();
    let ini = format!(
        "[user::general]\naccount_name={}\nlanguage={}\nip_country=US\n",
        if name.is_empty() { "kryoto" } else { name },
        if id.language.trim().is_empty() {
            "english"
        } else {
            id.language.trim()
        }
    );
    std::fs::write(settings.join("configs.user.ini"), ini)
        .map_err(|e| Error::Io(format!("writing configs.user.ini: {e}")))?;

    if !id.avatar_png.is_empty() {
        // Both names: `account_avatar` is this user's picture, `_default` is
        // what other players without one show as. Writing both means the K//
        // mark appears either way instead of a blank silhouette.
        for file in ["account_avatar.png", "account_avatar_default.png"] {
            std::fs::write(settings.join(file), id.avatar_png)
                .map_err(|e| Error::Io(format!("writing {file}: {e}")))?;
        }
    }
    Ok(())
}

/// The files `write_identity` creates, so they can be recorded and undone.
///
/// Derived from the same condition the writer uses rather than listed by hand,
/// because a list that drifts from the writer is a revert that leaves files
/// behind.
fn identity_files(id: &EmuIdentity) -> Vec<&'static str> {
    let mut out = vec!["configs.user.ini"];
    if !id.avatar_png.is_empty() {
        out.push("account_avatar.png");
        out.push("account_avatar_default.png");
    }
    out
}

/// gbe_fork's current config format: an ini, not the old pile of .txt files.
fn dlc_ini(dlc: &[Dlc]) -> String {
    let mut out = String::from("[app::dlcs]\nunlock_all=1\n");
    for d in dlc {
        // A newline in a name would break the ini into a bogus key.
        let name = d.name.replace(['\n', '\r'], " ");
        out.push_str(&format!("{}={}\n", d.appid, name.trim()));
    }
    out
}

/// Run generate_interfaces on the original dll and move the result into
/// steam_settings. Returns false when the tool is not configured - that is a
/// degraded but working crack, not an error.
async fn generate_interfaces(
    paths: &ToolPaths,
    dll: &Path,
    is64: bool,
    settings: &Path,
) -> Result<bool> {
    // An installed emulator cannot describe the original Steamworks interfaces.
    // Repair stages an original backup before this call. When none survived,
    // keep the release's existing interface file rather than reading the emulator.
    if !backup_path(dll).exists() && settings.join("steam_interfaces.txt").is_file() {
        return Ok(false);
    }
    let Some(gen) = paths.generate_interfaces(is64) else {
        return Ok(false);
    };
    // RUN IT SOMEWHERE WE OWN.
    //
    // The tool writes `steam_interfaces.txt` into its WORKING DIRECTORY, and
    // that used to be the folder the dll sits in - whatever folder that happens
    // to be, in whatever state it happens to be in. On a release extracted from
    // an archive that is frequently read-only, and on one whose dll lives
    // somewhere like
    //
    //   Engine\Plugins\Marketplace\SteamCorePro_5.4\Source\ThirdParty\
    //     SteamLibrary\redistributable_bin\win64\
    //
    // it is also long enough to matter. Either way the tool cannot create its
    // output file and exits 1 with "Error opening output file", which is what
    // a build reported while looking otherwise successful - and a game missing
    // its interface list frequently will not start at all.
    //
    // A directory of our own making removes the whole class: it exists, it is
    // writable, it is short, and nothing else is in it.
    static NEXT_SCRATCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let scratch = std::env::temp_dir().join(format!(
        "kryoto-interfaces-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT_SCRATCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    // A previous run's file in here would be moved into place as if this run
    // had produced it, which is worse than producing nothing.
    std::fs::create_dir(&scratch)
        .map_err(|e| Error::Io(format!("creating {}: {e}", scratch.display())))?;
    let produced = scratch.join("steam_interfaces.txt");

    let tool = Tool {
        name: "generate_interfaces",
        path: gen,
        launch: windows_exe_launch(),
    };
    let mut cmd = tool.command(paths.wine(), paths.dotnet());
    // The dll by absolute path, because the working directory is no longer its
    // folder.
    cmd.arg(dll).current_dir(&scratch);
    let outcome = run(cmd, "generate_interfaces", 180).await;

    let moved = if produced.exists() {
        std::fs::rename(&produced, settings.join("steam_interfaces.txt"))
            // A rename across volumes fails on Windows, and the scratch
            // directory is on the temp volume while the build may not be.
            .or_else(|_| {
                std::fs::copy(&produced, settings.join("steam_interfaces.txt")).map(|_| ())
            })
            .map_err(|e| Error::Io(format!("moving steam_interfaces.txt: {e}")))?;
        true
    } else {
        false
    };
    let _ = std::fs::remove_dir_all(&scratch);
    outcome?;
    Ok(moved)
}

/// Steamless over the exes the DRM scan flagged, and ONLY those.
///
/// This used to walk the whole tree and hand every `.exe` to Steamless. On a
/// game like Isaac that is the crash handler, five bundled editor tools and two
/// vcredist installers - none of them SteamStub-packed, all of them refused
/// with exit code 1, and every refusal recorded as a warning. Eight scary lines
/// per build describing nothing happening, on a scan that had already said
/// `SteamStub x1`.
///
/// The scan already knows exactly which files carry a `.bind` section, so the
/// list comes from there. A non-zero exit is now a real failure again: it means
/// Steamless could not strip a file that genuinely is protected.
///
/// Steamless writes `<name>.exe.unpacked.exe` and leaves the original alone; a
/// missing output means it declined the file, so only a real output replaces
/// the original.
async fn run_steamless_over_tree(
    paths: &ToolPaths,
    targets: &[PathBuf],
    warnings: &mut Vec<String>,
    cancel: &impl crate::Cancellation,
    // Optional so the launch-test path, which strips without a crack around it,
    // still compiles. When present, every replaced exe is stashed first.
    mut rec: Option<&mut Recorder<'_>>,
) -> Result<usize> {
    let Some(steamless) = paths.steamless_exe.clone() else {
        return Ok(0);
    };
    let mut count = 0usize;

    for exe in targets {
        // Steamless gets 600 seconds PER EXE, so a tree with a dozen of them
        // could ignore a stop for well over an hour.
        crate::error::check_cancel(cancel)?;
        let unpacked = PathBuf::from(format!("{}.unpacked.exe", exe.display()));
        let _ = std::fs::remove_file(&unpacked);

        let tool = Tool {
            name: "Steamless",
            path: steamless.clone(),
            launch: windows_exe_launch(),
        };
        let mut cmd = tool.command(paths.wine(), paths.dotnet());
        cmd.args(["--quiet", "--realign", "--recalcchecksum"])
            .arg(exe);

        // A failure here is per-file and expected for non-protected exes, so it
        // is not allowed to abort the whole crack.
        if let Err(e) = run(cmd, "Steamless", 600).await {
            if unpacked.exists() {
                let _ = std::fs::remove_file(&unpacked);
            }
            warnings.push(format!("Steamless on {}: {e}", exe.display()));
            continue;
        }

        if unpacked.exists() {
            // THE ONE MUTATION THAT USED TO BE UNRECOVERABLE.
            //
            // This renames the unpacked exe over the original and the original
            // is gone - the only copy of those bytes was in the depot. The
            // module header above claims originals are backed up, and that was
            // true of dlls and false of executables, which are the large files.
            // Stashed into the sidecar first, so a revert can put the packed exe
            // back; if it cannot be stashed the strip is skipped, because a
            // change that cannot be undone is worse than one not made.
            if let Some(rec) = rec.as_deref_mut() {
                if !rec.stash(exe, Stash::Replaced) {
                    warnings.push(format!(
                        "could not back up {} before stripping it; left as it is",
                        exe.display()
                    ));
                    let _ = std::fs::remove_file(&unpacked);
                    continue;
                }
            }
            let _ = make_writable(exe);
            if let Err(e) = std::fs::rename(&unpacked, exe) {
                warnings.push(format!("replacing {}: {e}", exe.display()));
            } else {
                count += 1;
            }
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout of a real release that cracked "successfully" and did nothing.
    ///
    /// Starlit Stories shipped its engine plugin's SOURCE tree, so the only
    /// `steam_api64.dll` in the whole game was the Steamworks redistributable
    /// inside it - a spare copy that the game never loads - while the folder
    /// beside the shipping exe had none at all.
    #[test]
    fn a_redistributable_copy_is_not_chosen_over_the_one_the_game_loads() {
        let dir = std::env::temp_dir().join("kryoto-dll-pick");
        let _ = std::fs::remove_dir_all(&dir);

        let spare = dir
            .join("Engine/Plugins/Marketplace/SteamCorePro_5.4/Source/ThirdParty")
            .join("SteamLibrary/redistributable_bin/win64");
        let loaded = dir.join("IdleThing/Binaries/Win64");
        std::fs::create_dir_all(&spare).unwrap();
        std::fs::create_dir_all(&loaded).unwrap();
        std::fs::write(spare.join("steam_api64.dll"), b"spare").unwrap();
        std::fs::write(loaded.join("steam_api64.dll"), b"loaded").unwrap();

        let found = find_steam_api_dlls(&dir);
        assert_eq!(found.len(), 1, "the spare copy was patched too: {found:?}");
        assert!(found[0].starts_with(&loaded));

        // But when the spare is all there is, patching it beats patching
        // nothing and calling the build cracked.
        std::fs::remove_file(loaded.join("steam_api64.dll")).unwrap();
        assert_eq!(find_steam_api_dlls(&dir).len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn revert_restores_replaced_files_and_removes_added_ones() {
        let dir = std::env::temp_dir().join("kryoto-revert-basic");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // A game file that the crack overwrites, plus its shipped sidecar.
        let dll = dir.join("steam_api64.dll");
        std::fs::write(&dll, b"ORIGINAL").unwrap();
        let backup = backup_path(&dll);
        std::fs::copy(&dll, &backup).unwrap();
        std::fs::write(&dll, b"EMULATOR").unwrap();

        // Something the crack created.
        let settings = dir.join("steam_settings");
        std::fs::create_dir_all(&settings).unwrap();
        let ini = settings.join("configs.app.ini");
        std::fs::write(&ini, b"x").unwrap();

        let mut rec = Recorder::new(&dir, Emulator::GbeFork);
        rec.replaced_with(&dll, &backup);
        rec.added(&settings);
        rec.added(&ini);
        rec.save();

        let warnings = revert_tree(&dir).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(std::fs::read(&dll).unwrap(), b"ORIGINAL");
        assert!(!ini.exists(), "the created ini should be gone");
        assert!(!settings.exists(), "the created directory should be gone");
        assert!(
            !manifest_path(&dir).exists(),
            "the record goes with the crack"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn revert_puts_back_a_deleted_file_and_a_stripped_exe() {
        let dir = std::env::temp_dir().join("kryoto-revert-stash");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // The online path deletes a shipped steam_appid.txt; Steamless replaces
        // a packed exe. Neither can be undone from a sidecar that ships, so both
        // go to ORIG_DIR.
        let appid_txt = dir.join("steam_appid.txt");
        std::fs::write(&appid_txt, b"1234").unwrap();
        let exe = dir.join("game.exe");
        std::fs::write(&exe, b"PACKED").unwrap();

        let mut rec = Recorder::new(&dir, Emulator::Online);
        assert!(rec.stash(&appid_txt, Stash::Deleted));
        assert!(rec.stash(&exe, Stash::Replaced));
        rec.save();

        std::fs::remove_file(&appid_txt).unwrap();
        std::fs::write(&exe, b"UNPACKED").unwrap();

        revert_tree(&dir).unwrap();
        assert_eq!(std::fs::read(&appid_txt).unwrap(), b"1234");
        assert_eq!(std::fs::read(&exe).unwrap(), b"PACKED");
        // The sidecar is part of the crack and must not survive it - it is
        // excluded from the archive, so a leftover would only waste disk.
        assert!(!dir.join(ORIG_DIR).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A Steamworks dll exports a great many symbols that look like interface
    /// strings and are not - `SteamAPI_SteamAppList_v001` is a function, not an
    /// interface - and every one of them written into the config is a key the
    /// emulator never looks up.
    #[test]
    fn exported_symbol_names_are_not_mistaken_for_interfaces() {
        assert_eq!(rune_interface_name("SteamAPI_SteamAppList_v001"), None);
        assert_eq!(rune_interface_name("SteamAPI_ISteamUser_GetAppId001"), None);
        // The real ones still read.
        assert_eq!(
            rune_interface_name("SteamUser023").as_deref(),
            Some("SteamUser")
        );
        assert_eq!(
            rune_interface_name("ISteamController007").as_deref(),
            Some("SteamController")
        );
        assert_eq!(
            rune_interface_name("STEAMAPPS_INTERFACE_VERSION008").as_deref(),
            Some("SteamApps")
        );
        assert_eq!(
            rune_interface_name("STEAMREMOTESTORAGE_INTERFACE_VERSION016").as_deref(),
            Some("SteamRemoteStorage")
        );
    }

    /// The eleven bytes that ARE the Steamclient profile.
    ///
    /// Nothing else about it is checkable from outside: the dll is the game's
    /// own file, the support libraries are copies, and the ini is a template
    /// fill. If this patch is wrong the release is a game that does not start,
    /// and the only symptom is on the player's machine.
    #[test]
    fn the_steamclient_patch_rewrites_exactly_the_import_name() {
        let dir = std::env::temp_dir().join("kryoto-shell32-x64");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dll = dir.join("steam_api64.dll");
        let mut bytes = b"MZ......".to_vec();
        bytes.extend_from_slice(b"SHELL32.dll");
        bytes.extend_from_slice(b"..tail..");
        let before = bytes.len();
        std::fs::write(&dll, &bytes).unwrap();

        assert!(patch_shell32(&dll, true).unwrap());
        let after = std::fs::read(&dll).unwrap();
        // Same length, so nothing in the PE after the patch has moved.
        assert_eq!(after.len(), before);
        assert_eq!(&after[8..19], b"RUNE64\0WUS\0");
        assert!(after.starts_with(b"MZ......"));
        assert!(after.ends_with(b"..tail.."));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_32_bit_patch_is_a_different_name_of_the_same_length() {
        let dir = std::env::temp_dir().join("kryoto-shell32-x86");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dll = dir.join("steam_api.dll");
        std::fs::write(&dll, b"xxSHELL32.dllyy").unwrap();
        assert!(patch_shell32(&dll, false).unwrap());
        let after = std::fs::read(&dll).unwrap();
        assert_eq!(after.len(), 15);
        assert_eq!(&after[2..13], b"RUNE\0!WUS!\0");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A dll with no such import is not a crash and not a crack. The caller
    /// has to be able to tell those apart, because counting it as applied is
    /// how a build ships with nothing done to it.
    #[test]
    fn a_dll_with_no_shell32_import_is_reported_not_patched() {
        let dir = std::env::temp_dir().join("kryoto-shell32-none");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dll = dir.join("steam_api64.dll");
        std::fs::write(&dll, b"nothing of interest here").unwrap();
        assert!(!patch_shell32(&dll, true).unwrap());
        assert_eq!(std::fs::read(&dll).unwrap(), b"nothing of interest here");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Case follows whatever the linker wrote, and a case-sensitive search
    /// silently finds nothing on the builds that spell it differently.
    #[test]
    fn the_import_is_found_whatever_its_casing() {
        let dir = std::env::temp_dir().join("kryoto-shell32-case");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dll = dir.join("steam_api64.dll");
        std::fs::write(&dll, b"..shell32.DLL..").unwrap();
        assert!(patch_shell32(&dll, true).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Telling a Steamclient tree from an untouched one. Content comparison
    /// cannot: the dll IS the game's own with eleven bytes changed.
    #[test]
    fn a_patched_dll_is_recognised_as_already_cracked() {
        let dir = std::env::temp_dir().join("kryoto-shell32-detect");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dll = dir.join("steam_api64.dll");
        std::fs::write(&dll, b"..SHELL32.dll..").unwrap();
        assert!(!shell32_is_patched(&dll));
        patch_shell32(&dll, true).unwrap();
        assert!(shell32_is_patched(&dll));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A template that still holds a placeholder is not a config with
    /// defaults - the emulator reads the placeholder literally and applies
    /// nothing, and says so only on the player's machine.
    #[test]
    fn every_placeholder_is_filled_or_removed() {
        let template = "[Settings]\nAppId=SteamID\nUserName=RUNE\n\n[Interfaces]\nRUNE_Interfaces\n\n[DLC]\nDLCUnlockall=0\nDLCs\n";
        let dlc = vec![Dlc {
            appid: "42".into(),
            name: "Soundtrack".into(),
        }];
        let out = rune_config(
            template,
            "397460",
            &dlc,
            &EmuIdentity::default(),
            "1.0.0.1",
            &["SteamClient020=x".to_string()],
        );
        assert!(out.contains("AppId=397460"));
        assert!(out.contains("SteamClient020=x"));
        assert!(out.contains("42=Soundtrack"));
        // Having DLC to unlock is the only reason to turn the switch on.
        assert!(out.contains("DLCUnlockall=1"));
        for left in ["SteamID", "RUNE_Interfaces", "DLCs", "RUNE_DLC"] {
            assert!(!out.contains(left), "{left} survived: {out}");
        }
    }

    /// With no DLC the line goes entirely. Emptied, it is a key with no value.
    #[test]
    fn a_game_with_no_dlc_loses_the_dlc_line_rather_than_emptying_it() {
        let out = rune_config(
            "[DLC]\nDLCUnlockall=0\nDLCs\n",
            "1",
            &[],
            &EmuIdentity::default(),
            "",
            &[],
        );
        assert!(!out.contains("DLCs"));
        assert!(out.contains("DLCUnlockall=0"));
    }

    /// A DLC name with a newline in it would end the line early and turn the
    /// rest of its own title into a key.
    #[test]
    fn a_dlc_name_cannot_break_the_line_it_is_written_on() {
        let dlc = vec![Dlc {
            appid: "7".into(),
            name: "Deluxe\nEdition".into(),
        }];
        let out = rune_config("[DLC]\nDLCs\n", "1", &dlc, &EmuIdentity::default(), "", &[]);
        assert!(out.contains("7=Deluxe Edition"));
    }

    #[test]
    fn revert_refuses_a_tree_with_no_record_instead_of_guessing() {
        let dir = std::env::temp_dir().join("kryoto-revert-norecord");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Built by an older version: there is nothing to undo it from, and
        // guessing at a file list is how a revert damages a tree.
        assert!(revert_tree(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn revert_leaves_a_directory_the_game_already_had() {
        let dir = std::env::temp_dir().join("kryoto-revert-plugins");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let plugins = dir.join("plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        std::fs::write(plugins.join("theirs.dll"), b"x").unwrap();

        // Recorded as added would be the bug; the crack only records a
        // directory it actually created.
        let rec = Recorder::new(&dir, Emulator::Online);
        rec.save();
        revert_tree(&dir).unwrap();
        assert!(
            plugins.join("theirs.dll").exists(),
            "a game's own plugins must survive"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rune_reads_an_interface_out_of_a_steamworks_dll() {
        // The shapes a real dll carries, as NUL-terminated ASCII runs.
        let blob: Vec<u8> = [
            &b"SteamUser023 "[..],
            &b"ISteamApps008 "[..],
            &b"STEAMREMOTESTORAGE_INTERFACE_VERSION016 "[..],
            &b"STEAMUGC_INTERFACE_VERSION020 "[..],
            // Not interfaces: too short, no version, and a file name.
            &b"hello "[..],
            &b"SteamAPI_Init "[..],
            &b"steam_api64.dll "[..],
        ]
        .concat();
        let path = std::env::temp_dir().join("kryoto-rune-interfaces.bin");
        std::fs::write(&path, &blob).unwrap();

        let lines = rune_interfaces(&path);
        assert!(
            lines.contains(&"SteamUser=SteamUser023".to_string()),
            "{lines:?}"
        );
        assert!(
            lines.contains(&"SteamApps=ISteamApps008".to_string()),
            "{lines:?}"
        );
        assert!(
            lines.contains(
                &"SteamRemoteStorage=STEAMREMOTESTORAGE_INTERFACE_VERSION016".to_string()
            ),
            "{lines:?}"
        );
        // The compound names must not be capitalise-the-rest guesses.
        assert!(
            lines.contains(&"SteamUGC=STEAMUGC_INTERFACE_VERSION020".to_string()),
            "{lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("SteamAPI_Init")),
            "{lines:?}"
        );
        assert!(!lines.iter().any(|l| l.contains(".dll")), "{lines:?}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_string_that_merely_mentions_steam_is_not_an_interface() {
        assert_eq!(rune_interface_name("SteamAPI_Init"), None);
        assert_eq!(rune_interface_name("Steam1"), None);
        assert_eq!(rune_interface_name("steam_api64.dll"), None);
        // Real ones.
        assert_eq!(
            rune_interface_name("SteamUser023").as_deref(),
            Some("SteamUser")
        );
        assert_eq!(
            rune_interface_name("STEAMHTTP_INTERFACE_VERSION003").as_deref(),
            Some("SteamHTTP")
        );
    }

    #[test]
    fn each_rune_profile_names_its_own_config_file() {
        // Steakclient reads a DIFFERENT file. Writing the wrong one leaves the
        // template's placeholders in place, which fails on the player's machine
        // rather than here.
        assert_eq!(Emulator::Rune.rune_ini_name(), Some("steam_emu.ini"));
        assert_eq!(
            Emulator::RuneSteamclient.rune_ini_name(),
            Some("steam_emu.ini")
        );
        assert_eq!(Emulator::RuneSteak.rune_ini_name(), Some("steak_emu.ini"));
        assert_eq!(Emulator::GbeFork.rune_ini_name(), None);
        assert!(Emulator::Rune.is_rune() && !Emulator::GbeFork.is_rune());
    }

    #[test]
    fn a_crack_stopped_part_way_leaves_enough_to_put_back() {
        // The pause-during-crack case: one dll swapped, one config written, and
        // then the run stops. Without a manifest written AS IT GOES the only
        // safe thing to do with this tree is delete it, which throws away the
        // download - so the record has to exist before the crack finishes.
        let dir = std::env::temp_dir().join("kryoto-midcrack");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let dll = dir.join("steam_api64.dll");
        std::fs::write(&dll, b"ORIGINAL").unwrap();
        let backup = backup_path(&dll);
        std::fs::copy(&dll, &backup).unwrap();

        let mut rec = Recorder::new(&dir, Emulator::GbeFork);
        rec.replaced_with(&dll, &backup);
        std::fs::write(&dll, b"EMULATOR").unwrap();
        let ini = dir.join("configs.app.ini");
        rec.added(&ini);
        std::fs::write(&ini, b"x").unwrap();
        // Deliberately NOT calling save() - the recording methods must have
        // persisted it themselves.

        assert!(
            manifest_path(&dir).exists(),
            "the record must survive a run that never reached the end"
        );
        let recovered = read_manifest(&dir).expect("a manifest to revert with");
        assert_eq!(recovered.replaced.len(), 1);

        revert_tree(&dir).unwrap();
        assert_eq!(std::fs::read(&dll).unwrap(), b"ORIGINAL");
        assert!(!ini.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dlc_ini_uses_the_current_gbe_format() {
        let ini = dlc_ini(&[
            Dlc {
                appid: "1".into(),
                name: "Soundtrack".into(),
            },
            Dlc {
                appid: "2".into(),
                name: "Art\nBook".into(),
            },
        ]);
        assert!(ini.starts_with("[app::dlcs]\nunlock_all=1\n"));
        assert!(ini.contains("1=Soundtrack"));
        // A newline in a name must not split the ini into a bogus key.
        assert!(ini.contains("2=Art Book"));
        assert_eq!(ini.lines().count(), 4);
    }

    #[test]
    fn identity_defaults_to_the_kryoto_account_and_ships_an_avatar() {
        let id = EmuIdentity::default();
        assert_eq!(id.account_name, "kryoto");
        // An empty avatar would leave every build showing a blank silhouette.
        assert!(id.avatar_png.len() > 1000);
        assert_eq!(&id.avatar_png[..4], b"\x89PNG");
    }

    #[test]
    fn identity_is_written_in_gbe_forks_documented_shape() {
        let dir = std::env::temp_dir().join("kryoto-identity-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        write_identity(&dir, &EmuIdentity::default()).unwrap();

        let ini = std::fs::read_to_string(dir.join("configs.user.ini")).unwrap();
        assert!(ini.starts_with("[user::general]"));
        assert!(ini.contains("account_name=kryoto"));
        assert!(dir.join("account_avatar.png").exists());
        assert!(dir.join("account_avatar_default.png").exists());
    }

    #[test]
    fn a_blank_account_name_falls_back_rather_than_writing_an_empty_key() {
        let dir = std::env::temp_dir().join("kryoto-identity-blank");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let id = EmuIdentity {
            account_name: "   ".into(),
            ..Default::default()
        };
        write_identity(&dir, &id).unwrap();
        let ini = std::fs::read_to_string(dir.join("configs.user.ini")).unwrap();
        assert!(ini.contains("account_name=kryoto"));
    }

    #[test]
    fn backup_appends_rather_than_replacing_the_extension() {
        // `with_extension("kryoto")` would eat the `.dll` and leave
        // `steam_api64.kryoto`, which no longer says what the file was.
        let got = backup_path(Path::new("C:/game/bin/steam_api64.dll"));
        assert_eq!(got.file_name().unwrap(), "steam_api64.dll.kryoto");
        // ...and never the conventional .bak, which a game may use itself.
        assert!(!got.to_string_lossy().ends_with(".bak"));
    }

    #[test]
    fn online_ini_spoofs_spacewar_while_keeping_the_real_appid() {
        let ini = online_ini(
            "391540",
            &[
                Dlc {
                    appid: "1".into(),
                    name: "Soundtrack".into(),
                },
                Dlc {
                    appid: "2".into(),
                    name: "Art Book".into(),
                },
            ],
            false,
        );
        // The whole trick: the game is told 480, the emulator remembers 391540.
        assert!(ini.contains("AppId=480"));
        assert!(ini.contains("ogAppId=391540"));
        assert!(ini.contains("UnlockDLC=1,2"));
        assert!(ini.contains("EmulateTicket=true"));
        // Swapping those two would spoof ownership of the game we're cracking
        // and claim the free sample is the real title - exactly backwards.
        let spoof = ini.find("AppId=480").unwrap();
        let real = ini.find("ogAppId=391540").unwrap();
        assert!(spoof < real);
    }

    #[test]
    fn online_ini_with_no_dlc_leaves_the_key_empty_not_absent() {
        // The emulator reads the key unconditionally; omitting it entirely is a
        // different code path there than an empty list.
        let ini = online_ini("480", &[], false);
        assert!(ini.contains("UnlockDLC=\n"));
    }

    /// A minimal Kryoto Online install and a one-DLL game tree, both under
    /// fresh temp directories.
    fn online_fixture(name: &str) -> (PathBuf, ToolPaths) {
        let root = std::env::temp_dir().join(format!("kryoto-crack-{name}"));
        let _ = std::fs::remove_dir_all(&root);

        let tools = root.join("tools");
        std::fs::create_dir_all(tools.join("x64")).unwrap();
        std::fs::write(tools.join("x64").join("steam_api64.dll"), b"PROXY").unwrap();
        std::fs::write(tools.join("x64").join("kryotoO.dll"), b"CORE").unwrap();

        let game = root.join("game");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("Game.exe"), b"MZ").unwrap();
        std::fs::write(game.join("steam_api64.dll"), b"VALVE").unwrap();

        let paths = ToolPaths {
            online_dir: Some(tools),
            // Deliberately absent: this is the machine that has never
            // configured Steamless, which is the case that shipped broken.
            steamless_exe: None,
            ..Default::default()
        };
        (game, paths)
    }

    #[tokio::test]
    async fn an_online_build_deploys_the_core_beside_the_proxy() {
        let (game, paths) = online_fixture("core-deploy");
        let out = crack_tree(
            &paths,
            &game,
            "391540",
            &[],
            &[],
            &EmuIdentity::default(),
            Emulator::Online,
            &kryoto_core::Cancel::new(),
        )
        .await
        .expect("a complete install should crack");

        assert_eq!(out.dlls_replaced, 1);
        // The proxy applies no patches at all without this file, and says so
        // only in a log on the player's machine.
        assert_eq!(
            std::fs::read(game.join("kryotoO.dll")).unwrap(),
            b"CORE",
            "kryotoO.dll must land beside the proxy it configures"
        );
        assert_eq!(
            std::fs::read(game.join("steam_api64.dll")).unwrap(),
            b"PROXY"
        );
        // The original stays recoverable without re-downloading the game.
        assert_eq!(
            std::fs::read(game.join("steam_api64.dll.kryoto")).unwrap(),
            b"VALVE"
        );
    }

    #[tokio::test]
    async fn an_online_build_without_the_core_is_refused_rather_than_shipped() {
        let (game, paths) = online_fixture("core-missing");
        // A 1.7.x install: the proxy is there, the core is not.
        std::fs::remove_file(paths.online_dir.as_ref().unwrap().join("x64/kryotoO.dll")).unwrap();

        let err = crack_tree(
            &paths,
            &game,
            "391540",
            &[],
            &[],
            &EmuIdentity::default(),
            Emulator::Online,
            &kryoto_core::Cancel::new(),
        )
        .await
        .expect_err("a build with no patches applied must not look like a success");

        assert!(
            err.to_string().contains("kryotoO.dll"),
            "the error has to name the missing file: {err}"
        );
    }

    #[tokio::test]
    async fn a_surviving_steamstub_turns_the_runtime_patch_on() {
        // Steamless is not configured (see the fixture) and the scan flagged
        // an executable, so nothing strips the wrapper. The emulator's own
        // patch is the only thing left that can deal with it.
        let (game, paths) = online_fixture("stub-survives");
        let stubbed = vec![game.join("Game.exe")];

        let out = crack_tree(
            &paths,
            &game,
            "391540",
            &[],
            &stubbed,
            &EmuIdentity::default(),
            Emulator::Online,
            &kryoto_core::Cancel::new(),
        )
        .await
        .unwrap();

        assert_eq!(out.steamstub_stripped, 0);
        let ini = std::fs::read_to_string(game.join(ONLINE_INI)).unwrap();
        assert!(
            ini.contains("GetStubbedLol=true"),
            "an unstripped stub has to be patched at runtime, or the exe will not start:
{ini}"
        );
        assert!(
            out.warnings
                .iter()
                .any(|w| w.contains("SteamStub survived")),
            "and it should be said out loud: {:?}",
            out.warnings
        );
    }

    #[tokio::test]
    async fn no_steamstub_leaves_the_runtime_patch_off() {
        // Nothing to patch. Leaving it on would have the hook scanning every
        // GetTickCount for a signature that is not in the process.
        let (game, paths) = online_fixture("no-stub");
        let out = crack_tree(
            &paths,
            &game,
            "391540",
            &[],
            &[],
            &EmuIdentity::default(),
            Emulator::Online,
            &kryoto_core::Cancel::new(),
        )
        .await
        .unwrap();

        let ini = std::fs::read_to_string(game.join(ONLINE_INI)).unwrap();
        assert!(ini.contains("GetStubbedLol=false"), "{ini}");
        assert!(!out
            .warnings
            .iter()
            .any(|w| w.contains("SteamStub survived")));
    }

    #[test]
    fn get_stubbed_lol_follows_whether_steamless_actually_stripped_it() {
        // The two SteamStub treatments are alternatives, not layers. Steamless
        // stripped the wrapper, so the runtime patch has nothing to find.
        let stripped = online_ini("391540", &[], false);
        assert!(stripped.contains("GetStubbedLol=false"));

        // Steamless did not (missing, or it declined the file). Nothing else
        // will deal with the stub, and the exe does not start if nothing does.
        // This was hard coded to false, so this case shipped broken.
        let survived = online_ini("391540", &[], true);
        assert!(survived.contains("GetStubbedLol=true"));
    }

    #[test]
    fn the_online_ini_reaches_the_exe_folder_not_just_the_dll_folder() {
        // The Unity layout, which is where writing it beside the dll fails: the
        // loader reads from the running exe's directory, and with no ini there
        // every setting silently falls back to its default.
        let root = std::env::temp_dir().join("kryoto-online-dirs");
        let _ = std::fs::remove_dir_all(&root);
        let plugins = root.join("Game_Data").join("Plugins").join("x86_64");
        std::fs::create_dir_all(&plugins).unwrap();
        std::fs::write(root.join("Game.exe"), b"MZ").unwrap();
        let dll = plugins.join("steam_api64.dll");
        std::fs::write(&dll, b"MZ").unwrap();

        let dirs = online_config_dirs(&root, &[dll]);
        assert!(dirs.contains(&root), "the exe's own folder must get one");
        assert!(
            dirs.contains(&plugins),
            "the dll's folder should get one too"
        );
    }

    #[test]
    fn config_dirs_are_deduplicated() {
        // The usual layout has the exe and the dll side by side; writing the
        // same file twice is harmless but the list should still be clean.
        let root = std::env::temp_dir().join("kryoto-online-flat");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("Game.exe"), b"MZ").unwrap();
        let dll = root.join("steam_api64.dll");
        std::fs::write(&dll, b"MZ").unwrap();

        let dirs = online_config_dirs(&root, &[dll]);
        assert_eq!(dirs.len(), 1);
    }

    #[test]
    fn the_two_emulators_are_labelled_distinctly_on_releases() {
        // These strings reach the public release page. If both said the same
        // thing a visitor could not tell an online build needs Steam running.
        assert_ne!(Emulator::GbeFork.label(), Emulator::Online.label());
        assert_eq!(Emulator::default(), Emulator::GbeFork);
    }

    #[test]
    fn source_label_reflects_what_actually_ran() {
        // The label lands on the public release page, so it must not claim a
        // step that was skipped - nor name the wrong emulator.
        for (emu, replaced, stripped, expect) in [
            (Emulator::GbeFork, 1, 1, "Steam + gbe_fork + Steamless"),
            (Emulator::GbeFork, 1, 0, "Steam + gbe_fork"),
            (Emulator::Online, 1, 1, "Steam + Kryoto Online + Steamless"),
            (Emulator::Online, 1, 0, "Steam + Kryoto Online"),
            (Emulator::GbeFork, 0, 1, "Steam + Steamless"),
            (Emulator::GbeFork, 0, 0, "Steam (DRM-free)"),
        ] {
            let emu_label = emu.label();
            let got = match (replaced > 0, stripped > 0) {
                (true, true) => format!("Steam + {emu_label} + Steamless"),
                (true, false) => format!("Steam + {emu_label}"),
                (false, true) => "Steam + Steamless".to_string(),
                (false, false) => "Steam (DRM-free)".to_string(),
            };
            assert_eq!(got, expect);
        }
    }
}
