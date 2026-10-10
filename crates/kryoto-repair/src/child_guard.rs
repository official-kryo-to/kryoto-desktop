//! Tool processes do not outlive Forge.
//!
//! Found on a real bot-mode run: Forge was killed mid-download and
//! DepotDownloader carried on without it, alone, filling the disk with a 16 GB
//! game nobody was going to pack. Nothing was left to stop it - no window, no
//! queue, no cancel token - and the next Forge to start could not see it either,
//! so a restart would have run two downloads into the same tree.
//!
//! On Windows a child process simply outlives its parent. The fix is a job
//! object with "kill on job close": every tool Forge starts is put into it, and
//! when Forge's handle to the job closes - on a normal exit, a crash, or Task
//! Manager - the OS terminates everything inside. It is the operating system
//! doing it, so it works in exactly the cases where Forge cannot.
//!
//! A game started for a launch test by hand is deliberately NOT put in here:
//! somebody playing it should not have it vanish because they closed Forge.
//! Only the tools - DepotDownloader, 7-Zip, the emulator helpers - are contained.
//!
//! Elsewhere this is a no-op: a Linux child is reparented rather than kept
//! alive by us, and the pipeline's own cancel path handles the normal case.

#[cfg(windows)]
mod sys {
    use core::ffi::c_void;

    #[repr(C)]
    #[derive(Default)]
    pub struct BasicLimit {
        pub per_process_user_time_limit: i64,
        pub per_job_user_time_limit: i64,
        pub limit_flags: u32,
        pub minimum_working_set_size: usize,
        pub maximum_working_set_size: usize,
        pub active_process_limit: u32,
        pub affinity: usize,
        pub priority_class: u32,
        pub scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    pub struct IoCounters {
        pub counts: [u64; 6],
    }

    /// `JOBOBJECT_EXTENDED_LIMIT_INFORMATION`.
    #[repr(C)]
    #[derive(Default)]
    pub struct ExtendedLimit {
        pub basic: BasicLimit,
        pub io: IoCounters,
        pub process_memory_limit: usize,
        pub job_memory_limit: usize,
        pub peak_process_memory_used: usize,
        pub peak_job_memory_used: usize,
    }

    pub const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;
    pub const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;

    extern "system" {
        pub fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> *mut c_void;
        pub fn SetInformationJobObject(
            job: *mut c_void,
            class: i32,
            info: *const c_void,
            len: u32,
        ) -> i32;
        pub fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
    }
}

/// The one job, created on first use and never closed by us - its handle is
/// closed by the OS when this process ends, which is the whole mechanism.
#[cfg(windows)]
fn job() -> Option<isize> {
    use std::sync::OnceLock;
    static JOB: OnceLock<Option<isize>> = OnceLock::new();
    *JOB.get_or_init(|| {
        // SAFETY: plain Win32 calls; every pointer is either null or to a
        // correctly-shaped, live struct for the duration of the call.
        unsafe {
            let job = sys::CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
            if job.is_null() {
                return None;
            }
            let mut info = sys::ExtendedLimit::default();
            info.basic.limit_flags = sys::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = sys::SetInformationJobObject(
                job,
                sys::JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<sys::ExtendedLimit>() as u32,
            );
            (ok != 0).then_some(job as isize)
        }
    })
}

/// Tie a freshly spawned tool to this process's lifetime.
///
/// Best effort: a failure leaves the tool exactly as it was before this module
/// existed, which is running normally and merely not cleaned up after a crash.
/// Never a reason to fail the step that started it.
pub fn contain(child: &tokio::process::Child) {
    #[cfg(windows)]
    {
        let Some(job) = job() else { return };
        let Some(handle) = child.raw_handle() else {
            return;
        };
        // SAFETY: `handle` is the live process handle tokio owns for `child`,
        // borrowed for the length of this call only.
        unsafe {
            sys::AssignProcessToJobObject(job as *mut _, handle as *mut _);
        }
    }
    #[cfg(not(windows))]
    let _ = child;
}

/// The same, for a child started with `std::process`.
pub fn contain_std(child: &std::process::Child) {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        let Some(job) = job() else { return };
        // SAFETY: as above - a borrowed, live process handle.
        unsafe {
            sys::AssignProcessToJobObject(job as *mut _, child.as_raw_handle() as *mut _);
        }
    }
    #[cfg(not(windows))]
    let _ = child;
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn the_job_object_can_be_created_with_kill_on_close() {
        // A wrong struct layout makes SetInformationJobObject fail, and that
        // is exactly what this would catch.
        assert!(super::job().is_some());
    }

    /// THE ACTUAL CLAIM: kill the parent the way Task Manager does, and the
    /// tool it started goes with it.
    ///
    /// This test runs itself again as a helper process. The helper starts a
    /// long-running tool (`ping` for a minute), contains it, prints the tool's
    /// pid and waits. The test then force-kills the helper - no cleanup code of
    /// ours gets to run - and checks the tool is gone. Without the job object
    /// the ping would carry on for the full minute, which is exactly what
    /// DepotDownloader did.
    #[test]
    #[allow(clippy::zombie_processes)]
    fn a_contained_tool_dies_when_its_parent_is_killed() {
        use std::io::BufRead;

        if std::env::var_os("KRYOTO_CHILD_GUARD_HELPER").is_some() {
            let child = std::process::Command::new("ping")
                .args(["-n", "60", "127.0.0.1"])
                .stdout(std::process::Stdio::null())
                .spawn()
                .expect("ping starts");
            super::contain_std(&child);
            println!("TOOL_PID={}", child.id());
            // A pipe is block-buffered: without this the line sits in the
            // buffer until the process exits, which is after the kill.
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(60));
            return;
        }

        let exe = std::env::current_exe().unwrap();
        let mut helper = std::process::Command::new(exe)
            .args([
                "--exact",
                "child_guard::tests::a_contained_tool_dies_when_its_parent_is_killed",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("KRYOTO_CHILD_GUARD_HELPER", "1")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("helper starts");

        let out = std::io::BufReader::new(helper.stdout.take().unwrap());
        let pid: u32 = out
            .lines()
            .map_while(Result::ok)
            // Searched for, not matched at the start: libtest prints
            // `test <name> ... ` on the same line before the test's own output.
            .find_map(|l| {
                l.split_once("TOOL_PID=")
                    .and_then(|(_, rest)| rest.split_whitespace().next()?.parse().ok())
            })
            .expect("helper reported its tool");

        assert!(alive(pid), "the tool should be running before the kill");
        helper.kill().expect("helper killed"); // TerminateProcess: no cleanup runs
        let _ = helper.wait();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while alive(pid) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(!alive(pid), "the tool outlived the process that started it");
    }

    fn alive(pid: u32) -> bool {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .expect("tasklist runs");
        String::from_utf8_lossy(&out.stdout).contains(&pid.to_string())
    }

    #[tokio::test]
    async fn a_tool_is_put_in_the_job() {
        let child = tokio::process::Command::new("cmd")
            .args(["/C", "exit 0"])
            .spawn()
            .expect("cmd starts");
        super::contain(&child);
        let _ = child.wait_with_output().await;
    }
}
