//! A line on stderr when fnvtool dies of a fault no code handles (#1148: `script
//! roundtrip` ended now and then with exit 0xC0000005 and an empty stderr, and the
//! crash never came back when looked for). The report names the exception code, the
//! module and offset it hit in (the offset finds the function in fnvtool.pdb, which the
//! release profile writes), the address an access violation touched, and the stack. The
//! process still ends with the same exit code.
//!
//! [documented: Microsoft Learn, SetUnhandledExceptionFilter; EXCEPTION_POINTERS;
//! EXCEPTION_RECORD] The filter runs on the faulting thread once no handler takes the
//! exception; returning EXCEPTION_CONTINUE_SEARCH (0) lets the process end as it would
//! have. For an access violation (0xC0000005) and an in-page error (0xC0000006)
//! ExceptionInformation[0] is 0 read, 1 write, 8 execute and [1] the address; for an
//! in-page error [2] is the NTSTATUS of the failed read (a mapped file that could not
//! be read in). A fail-fast abort (0xC0000409) never reaches the filter; Rust prints its
//! own message before those (allocation failure, a panic while panicking).

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::io::Write;

    #[repr(C)]
    struct ExceptionRecord {
        code: u32,
        flags: u32,
        next: *mut ExceptionRecord,
        address: *mut c_void,
        params: u32,
        info: [usize; 15],
    }

    #[repr(C)]
    struct ExceptionPointers {
        record: *mut ExceptionRecord,
        context: *mut c_void,
    }

    type Filter = unsafe extern "system" fn(*const ExceptionPointers) -> i32;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetUnhandledExceptionFilter(filter: Option<Filter>) -> Option<Filter>;
        fn GetModuleHandleExW(flags: u32, name: *const c_void, module: *mut *mut c_void) -> i32;
        fn GetModuleFileNameW(module: *mut c_void, buf: *mut u16, size: u32) -> u32;
    }

    const FROM_ADDRESS: u32 = 0x4;
    const UNCHANGED_REFCOUNT: u32 = 0x2;
    const CONTINUE_SEARCH: i32 = 0;
    const STACK_OVERFLOW: u32 = 0xC000_00FD;

    fn name(code: u32) -> &'static str {
        match code {
            0xC000_0005 => "access violation",
            0xC000_0006 => "in-page error (a mapped file could not be read)",
            0xC000_001D => "illegal instruction",
            0xC000_0094 => "integer division by zero",
            0xC000_0096 => "privileged instruction",
            0x8000_0002 => "misaligned data",
            _ => "exception",
        }
    }

    /// The loaded module holding `addr` and the offset into it.
    fn module_of(addr: *mut c_void) -> Option<(String, usize)> {
        let mut h = std::ptr::null_mut();
        // SAFETY: with FROM_ADDRESS the name argument is an address inside a module; the
        // handle is only read while the process is still running, its count unchanged.
        if unsafe { GetModuleHandleExW(FROM_ADDRESS | UNCHANGED_REFCOUNT, addr, &mut h) } == 0 {
            return None;
        }
        let mut buf = [0u16; 1024];
        // SAFETY: the buffer's length in characters is passed.
        let len = unsafe { GetModuleFileNameW(h, buf.as_mut_ptr(), buf.len() as u32) } as usize;
        let path = String::from_utf16_lossy(&buf[..len.min(buf.len())]);
        let file = path.rsplit(['\\', '/']).next().unwrap_or(&path).to_string();
        Some((file, addr as usize - h as usize))
    }

    unsafe extern "system" fn report(p: *const ExceptionPointers) -> i32 {
        // SAFETY: the system passes a valid EXCEPTION_POINTERS for the faulting thread.
        let Some(r) = (unsafe { p.as_ref().and_then(|p| p.record.as_ref()) }) else { return CONTINUE_SEARCH };
        // No room on the stack to report a stack overflow (Rust's own handler already
        // said so).
        if r.code == STACK_OVERFLOW {
            return CONTINUE_SEARCH;
        }
        let at = match module_of(r.address) {
            Some((m, off)) => format!("{m}+0x{off:x}"),
            None => format!("{:p}", r.address),
        };
        let thread = std::thread::current();
        let mut line = format!("fnvtool: fatal {} 0x{:08X} at {at} (thread '{}')", name(r.code), r.code, thread.name().unwrap_or("<unnamed>"));
        if matches!(r.code, 0xC000_0005 | 0xC000_0006) && r.params >= 2 {
            let what = match r.info[0] {
                0 => "reading",
                1 => "writing",
                8 => "executing",
                _ => "touching",
            };
            line += &format!(", {what} 0x{:x}", r.info[1]);
            if let Some((m, off)) = module_of(r.info[1] as *mut c_void) {
                line += &format!(" ({m}+0x{off:x})");
            }
            if r.code == 0xC000_0006 && r.params >= 3 {
                line += &format!(", read status 0x{:08X}", r.info[2] as u32);
            }
        }
        let mut err = std::io::stderr().lock();
        let _ = writeln!(err, "{line}");
        let _ = writeln!(err, "{}", std::backtrace::Backtrace::force_capture());
        let _ = err.flush();
        CONTINUE_SEARCH
    }

    pub fn install() {
        // SAFETY: `report` has the filter's signature and lives for the process.
        unsafe { SetUnhandledExceptionFilter(Some(report)) };
    }
}

/// Report an unhandled fault on stderr before the process ends (module notes).
pub fn install() {
    #[cfg(windows)]
    win::install();
}

#[cfg(all(test, windows))]
mod tests {
    /// Set in the child run of [`a_fault_is_reported`].
    const CHILD: &str = "NVF_CRASH_REPORT_CHILD";

    #[test]
    fn crash_child() {
        if std::env::var_os(CHILD).is_none() {
            return;
        }
        super::install();
        // SAFETY: none; the fault is the point of this child run.
        unsafe { std::ptr::read_volatile(16 as *const u64) };
    }

    #[test]
    fn a_fault_is_reported() {
        let exe = std::env::current_exe().unwrap();
        let out = std::process::Command::new(exe)
            .args(["crash_report::tests::crash_child", "--exact", "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(0xC000_0005_u32 as i32), "{err}");
        assert!(err.contains("fnvtool: fatal access violation 0xC0000005 at "), "{err}");
        assert!(err.contains(", reading 0x10"), "{err}");
    }
}
