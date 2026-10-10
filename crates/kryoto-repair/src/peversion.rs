//! The version a Windows executable declares about itself.
//!
//! Steam's build id says WHICH BUILD this is, which is exact and completely
//! opaque - "b25104224" tells a reader nothing about whether they have the
//! update they were looking for. The game's own version does, and it is sitting
//! in the exe, put there by whoever built it.
//!
//! Both are worth having and they answer different questions, so this reads the
//! one Steam cannot supply and the catalogue can show either or both.
//!
//! ## Why this parses the PE by hand
//!
//! The obvious call is `GetFileVersionInfoW`, and it is Windows-only. Forge
//! builds Windows games ON LINUX as well - the release workflow does exactly
//! that - so a Windows-only reader would silently return nothing for half the
//! builds, which is worse than not having the feature. A PE resource directory
//! is the same bytes on every host, so it is read directly.
//!
//! Only the `.rsrc` section is scanned rather than the whole file: a game
//! executable can be a gigabyte, the version resource always lives there, and
//! `VS_FIXEDFILEINFO` starts with a distinctive signature that makes locating it
//! within that section reliable.

use std::path::Path;

/// `VS_FIXEDFILEINFO.dwSignature`. Fixed by the format.
const FIXED_FILE_INFO_SIGNATURE: u32 = 0xFEEF_04BD;

fn u16_at(buf: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(buf.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(buf: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(buf.get(at..at + 4)?.try_into().ok()?))
}

/// Read exactly `len` bytes from `at`, or nothing.
fn read_at(file: &mut std::fs::File, at: u64, len: usize) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    file.seek(SeekFrom::Start(at)).ok()?;
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// The `.rsrc` section's bytes, if this is a PE that has one.
///
/// SEEKS rather than reading the file in. A game executable is routinely
/// hundreds of megabytes and can be over a gigabyte; pulling all of it into
/// memory to read forty bytes near the end would make reading a handful of
/// candidates cost more than the archive step.
fn resource_section(exe: &Path) -> Option<Vec<u8>> {
    let mut file = std::fs::File::open(exe).ok()?;

    // MZ, then e_lfanew at 0x3C points at the PE header.
    let head = read_at(&mut file, 0, 0x40)?;
    if head.get(0..2)? != b"MZ" {
        return None;
    }
    let pe = u32_at(&head, 0x3C)? as u64;

    // PE signature + COFF header: section count and optional-header size.
    let coff = read_at(&mut file, pe, 24)?;
    if coff.get(0..4)? != b"PE\0\0" {
        return None;
    }
    let sections = u16_at(&coff, 6)? as usize;
    let optional_size = u16_at(&coff, 20)? as u64;
    // A malformed header must not send this reading a gigantic section table.
    if sections == 0 || sections > 96 {
        return None;
    }

    let table = pe + 24 + optional_size;
    let entries = read_at(&mut file, table, sections * 40)?;
    for i in 0..sections {
        let entry = i * 40;
        if entries.get(entry..entry + 8)?.starts_with(b".rsrc") {
            let size = u32_at(&entries, entry + 16)? as usize;
            let offset = u32_at(&entries, entry + 20)? as u64;
            // Resource sections are small. A declared size larger than this is
            // a corrupt header, not a section worth scanning.
            if size == 0 || size > 64 * 1024 * 1024 {
                return None;
            }
            return read_at(&mut file, offset, size);
        }
    }
    None
}

/// The four-part version an executable declares, e.g. `1.6.0.4`.
///
/// `None` when the file is not a PE, has no resource section, carries no version
/// resource, or declares 0.0.0.0 - which is what an unset version looks like and
/// is not worth showing as if it were an answer.
pub fn file_version(exe: &Path) -> Option<String> {
    let rsrc = resource_section(exe)?;
    let rsrc = rsrc.as_slice();

    // Walk to the signature. Aligned to 4 bytes, which every field in this
    // structure is, so stepping by 4 cannot miss it and is four times cheaper.
    let mut at = 0usize;
    while at + 52 <= rsrc.len() {
        if u32_at(rsrc, at) == Some(FIXED_FILE_INFO_SIGNATURE) {
            // dwFileVersionMS at +8, dwFileVersionLS at +12.
            let ms = u32_at(rsrc, at + 8)?;
            let ls = u32_at(rsrc, at + 12)?;
            let parts = [ms >> 16, ms & 0xFFFF, ls >> 16, ls & 0xFFFF];
            if parts.iter().all(|p| *p == 0) {
                // Present but never filled in. Keep looking - a file can carry
                // more than one version resource - and report nothing if that
                // is all there is.
                at += 4;
                continue;
            }
            // Trailing zero parts are noise: "1.6.0.0" is a version somebody
            // wrote as "1.6".
            let mut out: Vec<String> = parts.iter().map(|p| p.to_string()).collect();
            while out.len() > 2 && out.last().map(|s| s == "0").unwrap_or(false) {
                out.pop();
            }
            return Some(out.join("."));
        }
        at += 4;
    }
    None
}

/// What to suggest as the release's version.
///
/// Both readings are kept when both exist, because they answer different
/// questions: the declared version is what a person recognises, and the build id
/// is what makes it exact. Either alone is still useful, which is why this
/// degrades rather than requiring both.
pub fn suggest(exe_version: Option<&str>, build_id: Option<&str>) -> Option<String> {
    match (exe_version, build_id) {
        (Some(v), Some(b)) => Some(format!("{v} (b{b})")),
        (Some(v), None) => Some(v.to_string()),
        (None, Some(b)) => Some(format!("b{b}")),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_drops_trailing_zeroes_but_keeps_two_parts() {
        assert_eq!(suggest(Some("1.6"), None).as_deref(), Some("1.6"));
    }

    #[test]
    fn both_readings_are_kept_because_they_answer_different_questions() {
        assert_eq!(
            suggest(Some("1.6.2"), Some("25104224")).as_deref(),
            Some("1.6.2 (b25104224)")
        );
    }

    #[test]
    fn either_alone_still_suggests_something() {
        assert_eq!(
            suggest(None, Some("25104224")).as_deref(),
            Some("b25104224")
        );
        assert_eq!(suggest(Some("2.0.1"), None).as_deref(), Some("2.0.1"));
        assert_eq!(suggest(None, None), None);
    }

    #[test]
    fn a_file_that_is_not_a_pe_reports_nothing_rather_than_guessing() {
        let path = std::env::temp_dir().join("kryoto-not-a-pe.bin");
        std::fs::write(&path, b"this is not an executable").unwrap();
        assert_eq!(file_version(&path), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_pe_with_no_resource_section_reports_nothing() {
        // MZ + e_lfanew pointing at a PE header with zero sections.
        let mut bytes = vec![0u8; 0x100];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        // NumberOfSections = 0, SizeOfOptionalHeader = 0.
        let path = std::env::temp_dir().join("kryoto-empty-pe.bin");
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(file_version(&path), None);
        let _ = std::fs::remove_file(&path);
    }
}
