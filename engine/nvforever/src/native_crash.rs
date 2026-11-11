//! Native crash dumps (docs/design/player-reports.md §3.2; #1425, #1825): a crash no Rust
//! code sees (an access violation in a driver or in the game, a fault while shutting down)
//! leaves `<home>\logs\crash-<UTC time>-<pid>.dmp`, a small Windows minidump (thread stacks
//! and the module list, no heap: `MiniDumpNormal`, usually well under 1 MB), beside the run
//! log, which gets a line naming it. Written whether or not the player sends reports (it
//! helps players who send files by hand); sent only with reports on (reports/upload.rs).
//!
//! An unhandled-exception filter (`SetUnhandledExceptionFilter`): it runs only for an
//! exception nothing else handled, writes the dump, and lets the process end as Windows
//! would have ended it. Rust panics never reach it (crash.rs's panic hook has those), and
//! neither does a stack overflow (Rust's own handler reports and aborts).
//!
//! Test aid `NVF_PLAY_TEST_NATIVE_CRASH=<seconds>`: a thread of its own causes an access
//! violation that long after the first frame, so the dump can be checked.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Where this run's dump goes, decided at the start (nothing is worked out mid-crash).
static DUMP_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Dumps kept in the logs folder, the newest ones [our choice: they are larger than reports].
pub const KEEP_DUMPS: usize = 5;

/// Set the filter; `dir` is the logs folder and `stamp` this run's time stamp.
pub fn install(dir: &std::path::Path, stamp: &str) {
    let _ = DUMP_PATH.set(dir.join(format!("crash-{stamp}-{}.dmp", std::process::id())));
    #[cfg(windows)]
    win::install();
    test_crash();
}

fn test_crash() {
    let Some(secs) = std::env::var("NVF_PLAY_TEST_NATIVE_CRASH").ok().and_then(|v| v.trim().parse::<f32>().ok()) else { return };
    let _ = std::thread::Builder::new().name("nvf-test-native-crash".into()).spawn(move || {
        while crate::crash::place_text().is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        std::thread::sleep(std::time::Duration::from_secs_f32(secs.max(0.0)));
        bevy::log::warn!("test aid NVF_PLAY_TEST_NATIVE_CRASH: an access violation now");
        // An access violation, on purpose: a write to an address no process maps.
        // SAFETY: none; this is the test aid's crash.
        unsafe { std::ptr::without_provenance_mut::<u32>(16).write_volatile(1) };
    });
}

#[cfg(windows)]
mod win {
    use super::DUMP_PATH;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Diagnostics::Debug::{
        MiniDumpNormal, MiniDumpWriteDump, SetUnhandledExceptionFilter, EXCEPTION_POINTERS, MINIDUMP_EXCEPTION_INFORMATION,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId};

    /// Let the exception go on to Windows' own handling (the process ends).
    const EXCEPTION_CONTINUE_SEARCH: i32 = 0;

    pub fn install() {
        // SAFETY: setting a process-wide filter; `filter` has the signature Windows calls.
        unsafe { SetUnhandledExceptionFilter(Some(filter)) };
    }

    unsafe extern "system" fn filter(info: *const EXCEPTION_POINTERS) -> i32 {
        let Some(path) = DUMP_PATH.get() else { return EXCEPTION_CONTINUE_SEARCH };
        // SAFETY: Windows passes a valid EXCEPTION_POINTERS; both pointers are checked.
        let code: u32 = if info.is_null() || unsafe { (*info).ExceptionRecord.is_null() } {
            0
        } else {
            let c = unsafe { (*(*info).ExceptionRecord).ExceptionCode };
            c as u32
        };
        let written = std::fs::File::create(path).ok().is_some_and(|file| {
            let ex = MINIDUMP_EXCEPTION_INFORMATION { ThreadId: unsafe { GetCurrentThreadId() }, ExceptionPointers: info as *mut _, ClientPointers: 0 };
            // SAFETY: the handle is a file opened for writing above; `ex` lives across the call.
            unsafe {
                MiniDumpWriteDump(GetCurrentProcess(), GetCurrentProcessId(), file.as_raw_handle(), MiniDumpNormal, &ex, std::ptr::null(), std::ptr::null()) != 0
            }
        });
        crate::crash::native_crash_line(code, written.then_some(path.as_path()));
        EXCEPTION_CONTINUE_SEARCH
    }
}
