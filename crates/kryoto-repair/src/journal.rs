use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

pub const JOURNAL: &str = ".kryoto-repair/transaction.json";
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    pub backup: Option<String>,
    pub sha256: Option<String>,
    pub readonly: bool,
    #[serde(default)]
    pub unix_mode: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Journal {
    pub schema: u32,
    pub state: String,
    pub source: String,
    pub version: String,
    pub entries: Vec<Entry>,
    pub directories: Vec<String>,
    #[serde(default)]
    pub report: Option<crate::repair::RepairReport>,
}

pub struct Lock {
    _file: File,
}
pub fn make_writable(path: &Path) -> Result<(), String> {
    let mut permissions = fs::metadata(path).map_err(|e| e.to_string())?.permissions();
    if !permissions.readonly() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(permissions.mode() | 0o200);
    }
    #[cfg(not(unix))]
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(path, permissions).map_err(|e| e.to_string())
}
pub fn check_idle(root: &Path) -> Result<(), String> {
    let path = safe_path(root, ".kryoto-repair/lock")?;
    if !path.exists() {
        return Ok(());
    }
    let file = File::open(path).map_err(|e| e.to_string())?;
    FileExt::try_lock_shared(&file)
        .map_err(|_| "A repair is running. Wait for it to finish before starting the game.")?;
    Ok(())
}
pub fn lock(root: &Path) -> Result<Lock, String> {
    fs::create_dir_all(safe_path(root, ".kryoto-repair")?).map_err(|e| e.to_string())?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(safe_path(root, ".kryoto-repair/lock")?)
        .map_err(|e| e.to_string())?;
    file.try_lock_exclusive()
        .map_err(|_| "Another repair is already using this game folder.")?;
    Ok(Lock { _file: file })
}

/// Validate every ancestor too: a crafted journal may not restore outside root.
pub fn safe_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let rel = Path::new(relative);
    if relative.is_empty()
        || relative.contains(':')
        || relative.contains('\\')
        || rel.components().any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("Unsafe path in repair record. No files were changed.".into());
    }
    let canonical_root = root.canonicalize().map_err(|e| e.to_string())?;
    let mut p = canonical_root.clone();
    for part in rel.components() {
        p.push(part.as_os_str());
        if let Ok(meta) = fs::symlink_metadata(&p) {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err("A repair path is a junction or symbolic link.".into());
                }
            }
            if meta.file_type().is_symlink()
                || !p
                    .canonicalize()
                    .map_err(|e| e.to_string())?
                    .starts_with(&canonical_root)
            {
                return Err("A repair path points outside the game folder.".into());
            }
        }
    }
    Ok(p)
}

pub fn digest(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn load(root: &Path) -> Result<Option<Journal>, String> {
    let p = safe_path(root, JOURNAL)?;
    if !p.exists() {
        return Ok(None);
    }
    if fs::metadata(&p).map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024 {
        return Err("The repair record is too large.".into());
    }
    let text = fs::read_to_string(p).map_err(|e| e.to_string())?;
    let record: Journal = serde_json::from_str(&text)
        .map_err(|_| "The repair record is unreadable. Keep its backups and contact support.")?;
    if record.schema != 1
        || !["pending", "applied", "undone"].contains(&record.state.as_str())
        || record.entries.len() > 10000
        || record.directories.len() > 10000
    {
        return Err("The repair record is invalid.".into());
    }
    Ok(Some(record))
}

impl Journal {
    pub fn prepare(
        root: &Path,
        files: &[PathBuf],
        directories: &[PathBuf],
        source: &str,
        version: &str,
    ) -> Result<Self, String> {
        if load(root)?.is_some_and(|j| j.state != "undone") {
            return Err("Undo the previous repair before applying another source.".into());
        }
        let mut record = Self {
            schema: 1,
            state: "pending".into(),
            source: source.into(),
            version: version.into(),
            entries: vec![],
            directories: vec![],
            report: None,
        };
        let mut size = 0u64;
        let generation = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for (i, file) in files.iter().enumerate() {
            let relative = file
                .strip_prefix(root)
                .map_err(|_| "A repair target is outside the game folder.")?
                .to_string_lossy()
                .replace('\\', "/");
            let file = safe_path(root, &relative)?;
            if file.exists() && !file.is_file() {
                return Err(format!("{} is not a regular file.", relative));
            }
            let mut entry = Entry {
                path: relative,
                backup: None,
                sha256: None,
                readonly: false,
                unix_mode: None,
            };
            if file.exists() {
                let meta = fs::metadata(&file).map_err(|e| e.to_string())?;
                size = size
                    .checked_add(meta.len())
                    .ok_or("Backup size overflow.")?;
                if size > 512 * 1024 * 1024 {
                    return Err(
                        "Emulator files exceed the 512 MB repair backup limit. Contact support."
                            .into(),
                    );
                }
                entry.readonly = meta.permissions().readonly();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    entry.unix_mode = Some(meta.permissions().mode() & 0o777);
                }
                let backup = format!(".kryoto-repair/backups/{generation}/{i}");
                let target = safe_path(root, &backup)?;
                fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
                // FlushFileBuffers requires a writable handle on Windows.
                // Copy through this handle rather than inheriting readonly
                // permissions onto the backup before it is flushed.
                let mut source = File::open(&file).map_err(|e| e.to_string())?;
                let mut backup_file = File::create(&target).map_err(|e| e.to_string())?;
                std::io::copy(&mut source, &mut backup_file)
                    .map_err(|e| format!("Could not back up {}: {e}", entry.path))?;
                backup_file.sync_all().map_err(|e| e.to_string())?;
                drop(backup_file);
                let hash = digest(&file)?;
                if digest(&target)? != hash {
                    return Err("Backup verification failed. No game files were changed.".into());
                }
                entry.backup = Some(backup);
                entry.sha256 = Some(hash);
            }
            record.entries.push(entry);
        }
        for dir in directories {
            let relative = dir
                .strip_prefix(root)
                .map_err(|_| "A folder is outside the game.")?
                .to_string_lossy()
                .replace('\\', "/");
            if relative.is_empty() {
                continue;
            }
            let dir = safe_path(root, &relative)?;
            if dir.exists() && !dir.is_dir() {
                return Err("A configuration folder is not a directory.".into());
            }
            if !dir.exists() {
                record.directories.push(relative);
            }
        }
        // Backups and write-ahead record are durable BEFORE the first game mutation.
        record.save(root)?;
        Ok(record)
    }

    pub fn save(&self, root: &Path) -> Result<(), String> {
        let temp = safe_path(root, ".kryoto-repair/transaction.new")?;
        let mut file = File::create(&temp).map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(temp, safe_path(root, JOURNAL)?).map_err(|e| e.to_string())
    }

    pub fn restore(&mut self, root: &Path) -> Result<(), String> {
        // Validate the entire plan and ALL backup hashes before touching any file.
        for entry in &self.entries {
            safe_path(root, &entry.path)?;
            if let Some(backup) = &entry.backup {
                if !backup.starts_with(".kryoto-repair/backups/")
                    || digest(&safe_path(root, backup)?)? != entry.sha256.as_deref().unwrap_or("")
                {
                    return Err("A repair backup is missing or changed. Keep the repair folder and contact support.".into());
                }
            }
        }
        for directory in &self.directories {
            safe_path(root, directory)?;
        }
        for entry in &self.entries {
            let path = safe_path(root, &entry.path)?;
            if path.exists() {
                make_writable(&path)?;
            }
            if let Some(backup) = &entry.backup {
                fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
                fs::copy(safe_path(root, backup)?, &path)
                    .map_err(|e| format!("Could not restore {}: {e}", entry.path))?;
                let mut permissions = fs::metadata(&path)
                    .map_err(|e| e.to_string())?
                    .permissions();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = entry.unix_mode.unwrap_or_else(|| {
                        if entry.readonly {
                            permissions.mode() & !0o222
                        } else {
                            permissions.mode() | 0o200
                        }
                    });
                    permissions.set_mode(mode & 0o777);
                }
                #[cfg(not(unix))]
                permissions.set_readonly(entry.readonly);
                fs::set_permissions(path, permissions).map_err(|e| e.to_string())?;
            } else if path.exists() {
                fs::remove_file(path).map_err(|e| e.to_string())?;
            }
        }
        let mut dirs = self.directories.clone();
        dirs.sort_by_key(|s| std::cmp::Reverse(s.len()));
        for directory in dirs {
            let p = safe_path(root, &directory)?;
            if p.is_dir() {
                let _ = fs::remove_dir(p);
            }
        }
        self.state = "undone".into();
        self.save(root)
    }
}

pub fn undo(root: &Path) -> Result<(), String> {
    let _guard = lock(root)?;
    let mut record = load(root)?.ok_or("No repair backup was found.")?;
    if record.state == "undone" {
        return Ok(());
    }
    record.restore(root)
}
