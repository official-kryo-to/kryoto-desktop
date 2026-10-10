/// Clipboard data is a generated support report, never arbitrary paths or secrets.
pub fn copy(text: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        #[link(name = "user32")]
        unsafe extern "system" {
            fn OpenClipboard(owner: *mut c_void) -> i32;
            fn EmptyClipboard() -> i32;
            fn SetClipboardData(format: u32, data: *mut c_void) -> *mut c_void;
            fn CloseClipboard() -> i32;
            fn GetForegroundWindow() -> *mut c_void;
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GlobalAlloc(flags: u32, bytes: usize) -> *mut c_void;
            fn GlobalLock(memory: *mut c_void) -> *mut c_void;
            fn GlobalUnlock(memory: *mut c_void) -> i32;
            fn GlobalFree(memory: *mut c_void) -> *mut c_void;
        }
        let data = text
            .replace('\0', "")
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>();
        // Windows transfers ownership of this movable allocation on success.
        unsafe {
            let memory = GlobalAlloc(0x2, data.len() * 2);
            if memory.is_null() {
                return Err("Could not allocate the support report clipboard.".into());
            }
            let dest = GlobalLock(memory);
            if dest.is_null() {
                GlobalFree(memory);
                return Err("Could not prepare the support report clipboard.".into());
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), dest as *mut u16, data.len());
            GlobalUnlock(memory);
            if OpenClipboard(GetForegroundWindow()) == 0 {
                GlobalFree(memory);
                return Err("The clipboard is busy. Try Copy support report again.".into());
            }
            let ok = EmptyClipboard() != 0 && !SetClipboardData(13, memory).is_null();
            CloseClipboard();
            if !ok {
                GlobalFree(memory);
                return Err("Could not copy the support report.".into());
            }
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        #[cfg(target_os = "macos")]
        let commands = vec![("pbcopy", vec![])];
        #[cfg(not(target_os = "macos"))]
        let commands = vec![
            ("wl-copy", vec![]),
            ("xclip", vec!["-selection", "clipboard"]),
        ];
        for (program, args) in commands {
            if let Ok(mut child) = Command::new(program)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                if let Some(mut input) = child.stdin.take() {
                    input
                        .write_all(text.as_bytes())
                        .map_err(|e| e.to_string())?;
                }
                if child.wait().map_err(|e| e.to_string())?.success() {
                    return Ok(());
                }
            }
        }
        Err("Clipboard is unavailable. Install wl-clipboard or xclip and try again.".into())
    }
}
