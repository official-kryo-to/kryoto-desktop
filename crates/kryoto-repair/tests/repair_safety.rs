use kryoto_repair::{
    detection,
    journal::{self, Journal},
    release,
    settings::Emulator,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Folder(PathBuf);
impl Folder {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "repair-check-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p.canonicalize().unwrap())
    }
    fn write(&self, name: &str, contents: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, contents).unwrap();
        p
    }
}
impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn pe(machine: u16) -> Vec<u8> {
    let mut bytes = vec![0; 128];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
    bytes[64..68].copy_from_slice(b"PE\0\0");
    bytes[68..70].copy_from_slice(&machine.to_le_bytes());
    bytes
}

#[test]
fn interrupted_repair_restores_replacements_and_removes_only_its_additions() {
    let f = Folder::new();
    let old = f.write("steam_api64.dll", b"original");
    let _lock = journal::lock(&f.0).unwrap();
    let mut record = Journal::prepare(
        &f.0,
        &[old.clone(), f.0.join("plugins/fix.dll")],
        &[f.0.join("plugins")],
        "RUNE",
        "v1",
    )
    .unwrap();
    assert_eq!(journal::load(&f.0).unwrap().unwrap().state, "pending");
    fs::write(&old, b"repaired").unwrap();
    f.write("plugins/fix.dll", b"fix");
    f.write("plugins/user.dll", b"user");
    record.restore(&f.0).unwrap();
    assert_eq!(fs::read(old).unwrap(), b"original");
    assert!(!f.0.join("plugins/fix.dll").exists());
    assert_eq!(fs::read(f.0.join("plugins/user.dll")).unwrap(), b"user");
}
#[test]
fn corrupted_backup_stops_undo_before_any_game_file_changes() {
    let f = Folder::new();
    let first = f.write("steam_api.dll", b"first");
    let second = f.write("steam_api64.dll", b"second");
    let _lock = journal::lock(&f.0).unwrap();
    let mut record =
        Journal::prepare(&f.0, &[first.clone(), second.clone()], &[], "GBE", "v1").unwrap();
    fs::write(&first, b"new first").unwrap();
    fs::write(&second, b"new second").unwrap();
    fs::write(
        f.0.join(record.entries[1].backup.as_ref().unwrap()),
        b"corrupt",
    )
    .unwrap();
    assert!(record.restore(&f.0).is_err());
    assert_eq!(fs::read(first).unwrap(), b"new first");
    assert_eq!(fs::read(second).unwrap(), b"new second");
}
#[test]
fn malicious_restore_path_is_rejected_before_restoring_other_files() {
    let f = Folder::new();
    let first = f.write("steam_api.dll", b"original");
    let _lock = journal::lock(&f.0).unwrap();
    let mut record = Journal::prepare(
        &f.0,
        &[first.clone(), f.0.join("added.dll")],
        &[],
        "GBE",
        "v1",
    )
    .unwrap();
    fs::write(&first, b"new").unwrap();
    record.entries[1].path = "../outside.dll".into();
    assert!(record.restore(&f.0).is_err());
    assert_eq!(fs::read(first).unwrap(), b"new");
    for path in ["../x", "/x", "C:/x", "a/../../x", "a\\x", ""] {
        assert!(journal::safe_path(&f.0, path).is_err(), "{path}");
    }
}
#[test]
fn repair_lock_blocks_a_second_repair_and_game_launch() {
    let f = Folder::new();
    assert!(journal::check_idle(&f.0).is_ok());
    let lock = journal::lock(&f.0).unwrap();
    assert!(journal::lock(&f.0).is_err());
    assert!(journal::check_idle(&f.0).is_err());
    drop(lock);
    assert!(journal::check_idle(&f.0).is_ok());
}
#[test]
fn a_second_source_requires_undo_and_undo_can_be_repeated() {
    let f = Folder::new();
    let dll = f.write("steam_api.dll", b"old");
    {
        let _lock = journal::lock(&f.0).unwrap();
        let _ = Journal::prepare(&f.0, std::slice::from_ref(&dll), &[], "GBE", "v1").unwrap();
    }
    assert!(Journal::prepare(&f.0, std::slice::from_ref(&dll), &[], "RUNE", "v2").is_err());
    fs::write(&dll, b"new").unwrap();
    journal::undo(&f.0).unwrap();
    journal::undo(&f.0).unwrap();
    assert_eq!(fs::read(&dll).unwrap(), b"old");
    assert!(Journal::prepare(&f.0, &[dll], &[], "RUNE", "v2").is_ok());
}
#[test]
fn an_empty_directory_is_not_a_game_and_dll_architecture_must_match() {
    let f = Folder::new();
    assert!(detection::scan(&f.0).is_err());
    f.write("Game.exe", &pe(0x8664));
    f.write("steam_api64.dll", &pe(0x14c));
    assert!(detection::scan(&f.0).is_err());
    f.write("steam_api64.dll", &pe(0x8664));
    assert!(detection::scan(&f.0).is_ok());
}
#[test]
fn a_steam_id_alone_does_not_prove_kryoto_origin_and_conflicting_ids_stop_repair() {
    let f = Folder::new();
    f.write("Game.exe", &pe(0x8664));
    f.write("steam_api64.dll", &pe(0x8664));
    f.write("steam_appid.txt", b"123");
    let game = detection::GameIdentity {
        slug: "game".into(),
        title: "Game".into(),
        steam_appid: "123".into(),
        source: None,
    };
    assert!(detection::verify_identity(&detection::scan(&f.0).unwrap(), &game, None).is_err());
    f.write("Kryoto.url", b"[InternetShortcut]\nURL=https://kryo.to\n");
    assert!(detection::verify_identity(&detection::scan(&f.0).unwrap(), &game, None).is_ok());
    f.write("steam_settings/configs.app.ini", b"app_id=999\n");
    assert!(detection::scan(&f.0).is_err());
}
#[test]
fn all_clients_receive_the_same_five_sources_and_rune_archives() {
    let sources = release::sources();
    assert_eq!(sources.len(), 5);
    assert_eq!(sources.iter().filter(|s| s.online).count(), 1);
    let names = [
        "rune-emu.zip",
        "steakclient.zip",
        "steamclient.zip",
        "emu-win-release-vs22.7z",
        "emu-win-debug.7z",
    ]
    .map(str::to_owned);
    assert_eq!(
        release::pick_asset(Emulator::Rune, &names),
        Some("rune-emu.zip")
    );
    assert_eq!(
        release::pick_asset(Emulator::RuneSteak, &names),
        Some("steakclient.zip")
    );
    assert_eq!(
        release::pick_asset(Emulator::RuneSteamclient, &names),
        Some("steamclient.zip")
    );
    assert_eq!(
        release::pick_asset(Emulator::GbeFork, &names),
        Some("emu-win-release-vs22.7z")
    );
}
#[test]
fn readonly_originals_can_be_backed_up_and_undo_restores_their_permissions() {
    let f = Folder::new();
    let dll = f.write("steam_api64.dll", b"original");
    let mut permissions = fs::metadata(&dll).unwrap().permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o540);
    }
    #[cfg(not(unix))]
    permissions.set_readonly(true);
    fs::set_permissions(&dll, permissions).unwrap();
    let _guard = journal::lock(&f.0).unwrap();
    let mut record = Journal::prepare(&f.0, std::slice::from_ref(&dll), &[], "RUNE", "v1").unwrap();
    journal::make_writable(&dll).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&dll).unwrap().permissions().mode() & 0o022, 0);
    }
    fs::write(&dll, b"replacement").unwrap();
    record.restore(&f.0).unwrap();
    assert_eq!(fs::read(&dll).unwrap(), b"original");
    assert!(fs::metadata(&dll).unwrap().permissions().readonly());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&dll).unwrap().permissions().mode() & 0o777,
            0o540
        );
    }
    journal::make_writable(&dll).unwrap();
}
#[test]
fn steamstub_detection_uses_valid_pe_sections_instead_of_the_filename() {
    let f = Folder::new();
    let plain = f.write("SteamStub.exe", &pe(0x8664));
    assert!(!detection::has_steamstub(&plain));
    let mut wrapped = pe(0x8664);
    wrapped[70..72].copy_from_slice(&1u16.to_le_bytes());
    wrapped[88..96].copy_from_slice(b".bind\0\0\0");
    let actual = f.write("Game.exe", &wrapped);
    assert!(detection::has_steamstub(&actual));
    wrapped.truncate(100);
    f.write("Game.exe", &wrapped);
    assert!(!detection::has_steamstub(&actual));
}
#[test]
fn download_updates_do_not_crowd_failure_details_out_of_the_support_report() {
    let mut logs = Vec::new();
    for _ in 0..1000 {
        kryoto_repair::repair::add_log(&mut logs, "Downloading RUNE", Some(10));
    }
    for i in 0..400 {
        kryoto_repair::repair::add_log(&mut logs, &format!("Interface {i}"), None);
    }
    kryoto_repair::repair::add_log(&mut logs, "RUNE v1 SHA256 abc123", None);
    kryoto_repair::repair::add_log(
        &mut logs,
        "Could not replace Steam API; previous files restored",
        None,
    );
    assert!(logs.len() <= 300);
    let text = kryoto_repair::repair::support_text(None, "RUNE", &logs);
    assert!(text.contains("SHA256 abc123"));
    assert!(text.contains("previous files restored"));
}
#[test]
fn support_report_fits_a_ticket_and_scrubs_user_paths() {
    let logs = vec![r"Failure under C:\Users\private-name\Game\steam_api.dll".to_owned(); 50];
    let text = kryoto_repair::repair::support_text(None, "RUNE", &logs);
    assert!(!text.contains("private-name"));
    assert!(text.chars().count() <= 3900);
    assert!(text.contains("RUNE"));
}
