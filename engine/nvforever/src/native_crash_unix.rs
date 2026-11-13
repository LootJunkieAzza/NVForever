//! Linux native crash evidence (#2167; docs/design/player-reports.md §3.2): a crash no Rust
//! code sees (a fault in a driver, in the GPU stack, at shutdown: #1425, #1825) leaves a
//! line in the run log naming the signal, the faulting address, where the code was and the
//! top frames. The Windows side is `native_crash.rs`'s minidump; this module does not touch
//! it and is not compiled on Windows.
//!
//! A `sigaction` handler for `SIGSEGV`, `SIGBUS`, `SIGABRT`, `SIGFPE` and `SIGILL`, with
//! `SA_SIGINFO | SA_ONSTACK` and a `sigaltstack` of its own, so a stack overflow still runs
//! it. The handler writes straight to the run log's file descriptor (and to stderr) with
//! `write(2)`, from a fixed buffer: nothing in it allocates, locks, formats into a `String`
//! or calls back into the log plugin (see "Limits"). It then restores the default
//! disposition, unblocks the signal and re-raises it, so the process still dies by the same
//! signal (`-11` for `SIGSEGV`: `tools/smoke` keeps treating a crash as a crash).
//!
//! Module and offset come from `/proc/self/maps`, read with `open`/`read` into a fixed
//! buffer: a `/proc` read takes no allocator or loader lock, where `dladdr` and
//! `dl_iterate_phdr` take the dynamic loader's lock and could deadlock a crash inside it.
//! Frames come from libgcc's `_Unwind_Backtrace`, addresses only: symbol names would need
//! `dladdr` (the loader lock) or `backtrace`'s allocating resolver, so the log carries
//! module + offset, which the release's line tables can symbolise later.
//!
//! Order matters: the signal line (signal, fault address, pc and module) is composed and
//! written with no unwinding at all, so a stack the unwinder cannot walk — the common case
//! in these crashes — cannot cost the fact of the crash or where it happened. Only then are
//! the frames walked, under an `alarm(2)` watchdog (see [`UNWIND_WATCHDOG_SECS`]): on glibc
//! older than 2.35, `_Unwind_FindFDE` goes through `dl_iterate_phdr` and takes the loader
//! lock, so a fault while this thread held that lock would block the unwinder and freeze the
//! game. `SIGALRM`'s default action then ends the process instead of hanging it.
//!
//! Privacy: the run log is a file players send, so the line gets the same treatment as
//! `crash.rs` — the player's home folder is written `~` — done in place, in a fixed buffer.
//! A game on a removable drive is mounted under the player's name too (`/run/media/<user>`,
//! `/media/<user>`), and those prefixes are written `~` as well, so a module path from such
//! an install does not carry the name into the log.
//!
//! Test aid `NVF_PLAY_TEST_NATIVE_CRASH=<seconds>` (shared with `native_crash.rs`): a thread
//! writes to an unmapped address that long after the first frame, which this handler catches.
//!
//! Armed from `.init_array` (see [`arm`]): Rust's std installs a SIGSEGV handler of its own
//! on the first thread, with a `SIGSTKSZ` alternate stack per thread (8 KiB here). That
//! stack is too small for this handler's unwinder, and a signal taken on it dies in the
//! kernel's frame setup without reaching any handler at all — so this one is installed
//! before std looks, and std leaves both the disposition and the alternate stacks alone.
//!
//! Limits [our notes]: the handler runs on the crashing thread; a crash inside the allocator
//! is fine (no allocation here), and the module lookup takes no loader lock, so a fault
//! inside `dlopen` still gets its line. The unwinder is the one place that can take that
//! lock (glibc < 2.35, above), and that is why it runs after the line and under the
//! watchdog. A library or a driver that installs its own handler after us replaces ours
//! without telling us; such a crash leaves no line. The frames stop where the unwinder
//! cannot go on (no unwind tables, a corrupted stack); they are the top frames, and the line
//! before them is written either way. The guard that catches a fault inside the handler is
//! per thread (`gettid`): the thread already inside it has its line written before the
//! unwinder starts, and another thread faulting at that moment dies at once rather than
//! compose over it. Only the main thread has an alternate stack (this module installs it
//! there); a stack overflow on a thread with none — every other thread, since std is not
//! making them — leaves nothing, while any ordinary fault there is delivered on its normal
//! stack and does.
//!
//! Regression [our notes]: arming from `.init_array` also stops std installing its own
//! SIGSEGV/SIGBUS handler, which removes std's `thread '<name>' has overflowed its stack`
//! message. Most game code runs on Bevy task-pool threads, so a stack overflow there now
//! leaves nothing where it used to leave that line; the Limits paragraph above says why
//! those threads get no line. Reproducing std's message would mean a formatted write from a
//! handler that must not allocate; chaining to `.init_array`-era dispositions is not
//! possible either (std installs its own only when the thread starts).

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};

/// The signals a native crash arrives as.
const SIGNALS: [libc::c_int; 5] = [libc::SIGSEGV, libc::SIGBUS, libc::SIGABRT, libc::SIGFPE, libc::SIGILL];
/// Frames kept in the line: enough to see the fault's stack, small enough for one `write`.
const MAX_FRAMES: usize = 16;
/// `/proc/self/maps` is read into this: a running game had ~3000 mappings (284 KiB: the
/// allocator and the GPU driver map a lot), so this is a generous ceiling rather than a
/// guess; past it, a frame whose module lies later reads `unknown`.
const MAPS_BYTES: usize = 1024 * 1024;
/// The handler's stack: a stack overflow must still have room for this module's work.
const ALTSTACK_BYTES: usize = 64 * 1024;
/// The whole line (signal line, then the frames) is composed in a fixed buffer, then
/// written at once. Not on the handler's stack: the altstack a thread has can be small
/// (`SIGSTKSZ`), and a handler that outgrows it is never entered at all.
const LINE_BYTES: usize = 8 * 1024;
/// A module path is cut here; longer ones are only the game's own build paths.
const MODULE_BYTES: usize = 200;

/// This run's run-log file descriptor, opened at the start (`-1` until then).
static RUN_LOG_FD: AtomicI32 = AtomicI32::new(-1);
/// The tid of the thread inside the handler, or 0: a second signal there must not compose
/// another line over the first one's.
static CRASHING_TID: AtomicI32 = AtomicI32::new(0);
/// Seconds the unwinder gets before the process is left to die by `SIGALRM`: on glibc older
/// than 2.35 `_Unwind_Find_FDE` goes through `dl_iterate_phdr`, which takes the loader lock,
/// and a fault while that lock is held would otherwise freeze the game instead of ending it.
const UNWIND_WATCHDOG_SECS: libc::c_uint = 5;
/// The fixed buffers, allocated once at install (a handler may not allocate).
static MAPS_BUF: AtomicUsize = AtomicUsize::new(0);
static LINE_BUF: AtomicUsize = AtomicUsize::new(0);
static HOME_BUF: AtomicUsize = AtomicUsize::new(0);
static HOME_LEN: AtomicUsize = AtomicUsize::new(0);
static ALTSTACK: AtomicUsize = AtomicUsize::new(0);
/// `/run/media/<user>` and `/media/<user>`: a game on a removable drive carries the player's
/// name in its module paths there, so those are written `~` as well.
static MEDIA_BUF: [AtomicUsize; 2] = [AtomicUsize::new(0), AtomicUsize::new(0)];
static MEDIA_LEN: [AtomicUsize; 2] = [AtomicUsize::new(0), AtomicUsize::new(0)];

/// Arm the handler: its own alternate stack and the disposition for the five signals.
///
/// On Linux this runs from `.init_array`, before `main` and so before Rust's std installs
/// its own SIGSEGV handler (it does that at the first thread, and only while the
/// disposition is still `SIG_DFL`). That matters: std gives each thread an alternate stack
/// of `SIGSTKSZ` (8 KiB here), and this handler's unwinder needs more room than that, so a
/// signal taken on one dies in the kernel's frame setup without ever reaching us. Armed
/// before std looks, std leaves the disposition and the alternate stacks alone; with no
/// alternate stack `SA_ONSTACK` uses the normal one (a hardware fault then has room), and
/// the alternate stack installed here covers the main thread's stack overflow.
pub fn arm() {
    static ARMED: AtomicBool = AtomicBool::new(false);
    if ARMED.load(Ordering::SeqCst) {
        return;
    }
    // A stack overflow takes its signal with no stack left, so the handler runs on this one.
    let alt = alloc(ALTSTACK_BYTES);
    if alt.is_null() {
        return;
    }
    ALTSTACK.store(alt as usize, Ordering::SeqCst);
    let stack = libc::stack_t { ss_sp: alt.cast(), ss_flags: 0, ss_size: ALTSTACK_BYTES };
    // SAFETY: `stack` points at `ALTSTACK_BYTES` of live, process-lifetime memory.
    if unsafe { libc::sigaltstack(&stack, std::ptr::null_mut()) } != 0 {
        return;
    }
    for sig in SIGNALS {
        // SAFETY: the struct is filled in below; `handler` has the signature `sigaction`
        // calls for, and the alternate stack above is installed.
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = handler as *const () as usize;
            sa.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
            libc::sigemptyset(&mut sa.sa_mask);
            libc::sigaction(sig, &sa, std::ptr::null_mut());
        }
    }
    ARMED.store(true, Ordering::SeqCst);
}

/// Run [`arm`] before `main`: see its notes for why the order matters.
#[cfg(target_os = "linux")]
#[used]
#[link_section = ".init_array"]
static ARM_BEFORE_MAIN: extern "C" fn() = arm_before_main;

#[cfg(target_os = "linux")]
extern "C" fn arm_before_main() {
    arm()
}

/// Open this run's log for the handler and take the fixed buffers it composes the line in.
/// Called by `crash::install` once the run log exists. Without a log the handler still runs
/// (armed before `main`) and writes its line to the console only.
pub fn install() {
    arm();
    let Some(path) = crate::crash::run_log_path() else { return };
    use std::os::unix::ffi::OsStrExt;
    let Ok(cpath) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { return };
    // SAFETY: a plain open(2) of this run's log; the flags ask for an append-only writer.
    let fd = unsafe { libc::open(cpath.as_ptr(), libc::O_WRONLY | libc::O_APPEND | libc::O_CLOEXEC) };
    if fd < 0 {
        return;
    }
    RUN_LOG_FD.store(fd, Ordering::SeqCst);
    // The home folder, for the redaction: read once, here, where allocating is allowed.
    if let Ok(home) = std::env::var("HOME") {
        let home = home.trim_end_matches('/');
        // `/` or a two-letter folder would hide every path: not a home to hide.
        if home.len() > 3 {
            keep(&HOME_BUF, &HOME_LEN, home.as_bytes());
            // The desktop mounts removable drives under the player's name too
            // (`/run/media/alex/...`, `/media/alex/...`), and a module path from a game
            // installed there would otherwise name the player in a file that says it may be
            // uploaded. The user name is the home folder's last segment.
            let user = home.rsplit('/').next().unwrap_or("");
            if !user.is_empty() {
                for (i, root) in ["/run/media/", "/media/"].iter().enumerate() {
                    let mut bytes = [0u8; 64];
                    let tail = user.as_bytes();
                    let head = root.as_bytes();
                    if head.len() + tail.len() <= bytes.len() {
                        bytes[..head.len()].copy_from_slice(head);
                        bytes[head.len()..head.len() + tail.len()].copy_from_slice(tail);
                        keep(&MEDIA_BUF[i], &MEDIA_LEN[i], &bytes[..head.len() + tail.len()]);
                    }
                }
            }
        }
    }
    let maps = alloc(MAPS_BYTES);
    if maps.is_null() {
        return;
    }
    MAPS_BUF.store(maps as usize, Ordering::SeqCst);
    // The line is composed here, not on the handler's stack: an alternate stack can be as
    // small as `SIGSTKSZ`, and a handler that outgrows it is never entered at all.
    let line = alloc(LINE_BYTES);
    if line.is_null() {
        return;
    }
    LINE_BUF.store(line as usize, Ordering::SeqCst);
}

/// Process-lifetime memory for a handler buffer (never freed: the process dies first).
fn alloc(bytes: usize) -> *mut u8 {
    // SAFETY: malloc with a nonzero size; the caller checks for null.
    unsafe { libc::malloc(bytes).cast() }
}

/// Copy `bytes` into process-lifetime memory and publish it in `ptr`/`len_at` (the handler
/// reads both from any thread, so they are atomics).
fn keep(ptr: &AtomicUsize, len_at: &AtomicUsize, bytes: &[u8]) {
    let p = alloc(bytes.len());
    if p.is_null() {
        return;
    }
    // SAFETY: `p` has room for `bytes`, allocated above.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len()) };
    ptr.store(p as usize, Ordering::SeqCst);
    len_at.store(bytes.len(), Ordering::SeqCst);
}

/// A crash as the handler sees it, with the code's module already found (a handler must not
/// allocate, so the path is a fixed array, not a `String`).
struct Fault {
    signal: libc::c_int,
    /// The faulting address (`si_addr`: the data address for SEGV/BUS/FPE/ILL).
    address: usize,
    /// The instruction pointer: where the thread was.
    pc: usize,
    module: [u8; MODULE_BYTES],
    module_len: usize,
    /// The module's base address, so `pc - base` is the offset.
    module_base: usize,
}

impl Fault {
    /// The module holding `pc`, from `maps`; empty when no mapping holds it.
    fn resolve(&mut self, maps: &[u8]) {
        let Some((path, base)) = map_of(self.pc, maps) else { return };
        let n = path.len().min(MODULE_BYTES);
        self.module[..n].copy_from_slice(&path[..n]);
        self.module_len = n;
        self.module_base = base;
    }

    /// `module+0xoffset`, or `unknown` when no mapping held `pc`.
    fn module_and_offset(&self) -> ([u8; MODULE_BYTES], usize) {
        let mut out = [0u8; MODULE_BYTES];
        if self.module_len == 0 {
            let unknown = b"unknown";
            out[..unknown.len()].copy_from_slice(unknown);
            return (out, unknown.len());
        }
        out[..self.module_len].copy_from_slice(&self.module[..self.module_len]);
        (out, self.module_len)
    }
}

/// The signal handler itself: one line, then die by the same signal.
extern "C" fn handler(sig: libc::c_int, info: *mut libc::siginfo_t, ctx: *mut libc::c_void) {
    // Per thread, not process-wide: a second thread that faults while the first composes its
    // line dies at once (the first line is already written by then) instead of composing over
    // it, and a *different* signal arriving inside this handler on the same thread is let
    // through to its default action. A synchronous fault inside the handler does not come
    // back here at all: the delivered signal is blocked during the handler (no SA_NODEFER),
    // so the kernel force-delivers it with the default action and the process dies. Either
    // way the line written before the unwinder is kept.
    let tid = current_tid();
    if CRASHING_TID.swap(tid, Ordering::SeqCst) != 0 {
        reraise(sig);
    }
    if LINE_BUF.load(Ordering::SeqCst) == 0 || MAPS_BUF.load(Ordering::SeqCst) == 0 {
        // A crash before the run log was opened (`crash::install`): the console still gets
        // the signal, and nothing here may touch an allocator.
        let mut buf = [0u8; 96];
        let mut line = Line::new(&mut buf);
        line.push(b"native crash, signal ");
        line.push(signal_name(sig));
        line.push(b" (");
        line.push_dec(sig as usize);
        line.push(b")\n");
        let len = line.len;
        write_lines(&line.bytes[..len]);
        reraise(sig);
    }
    // SAFETY: `info` and `ctx` are the kernel's arguments for a SA_SIGINFO handler.
    let mut fault = unsafe { fault_from(sig, info, ctx) };
    // SAFETY: the maps buffer is live process-lifetime memory, filled by the call.
    let maps = unsafe { read_maps() };
    fault.resolve(maps);
    // The signal line first, before any unwinding: it needs no stack walk, so a corrupt or
    // blocked stack cannot cost the fact that the process crashed, or where.
    // SAFETY: the line buffer is process-lifetime memory, only this handler writes it.
    let mut line = Line::new(unsafe { std::slice::from_raw_parts_mut(LINE_BUF.load(Ordering::SeqCst) as *mut u8, LINE_BYTES) });
    let len = compose_fault_line(&fault, &mut line);
    let len = redact_in_place(line.bytes, len, &hide());
    write_lines(&line.bytes[..len]);
    // The watchdog: a fault taken while this thread holds the loader lock would leave the
    // unwinder blocked forever. `SIGALRM`'s default action ends the process instead, and the
    // line above is already on disk.
    // SAFETY: alarm(2) is async-signal-safe and needs no state.
    unsafe { libc::alarm(UNWIND_WATCHDOG_SECS) };
    let mut frames = [0usize; MAX_FRAMES];
    let frame_count = unwind(&mut frames);
    // SAFETY: alarm(0) cancels the watchdog above.
    unsafe { libc::alarm(0) };
    let mut line = Line::new(unsafe { std::slice::from_raw_parts_mut(LINE_BUF.load(Ordering::SeqCst) as *mut u8, LINE_BYTES) });
    let len = compose_frames(&frames[..frame_count], maps, &mut line);
    let len = redact_in_place(line.bytes, len, &hide());
    write_lines(&line.bytes[..len]);
    reraise(sig);
}

/// This thread's id, by raw syscall: `syscall(2)` is async-signal-safe, where glibc's
/// `gettid` wrapper is newer than some release bases.
fn current_tid() -> libc::c_int {
    // SAFETY: a plain gettid(2) with no arguments.
    unsafe { libc::syscall(libc::SYS_gettid) as libc::c_int }
}

/// The fault `sig` describes: `si_addr` as the address, the context's instruction pointer.
///
/// # Safety
/// `info` and `ctx` must be the pointers a `SA_SIGINFO` handler was called with.
unsafe fn fault_from(sig: libc::c_int, info: *mut libc::siginfo_t, ctx: *mut libc::c_void) -> Fault {
    // `si_addr` only means an address for a fault the kernel raised (`si_code > 0`); for a
    // signal sent by a process (abort, kill) the same bits are a pid.
    let address = if info.is_null() || unsafe { (*info).si_code } <= 0 {
        0
    } else {
        // SAFETY: a non-null siginfo_t from the kernel; si_addr is valid for these signals.
        unsafe { (*info).si_addr() as usize }
    };
    Fault {
        signal: sig,
        address,
        // SAFETY: `ctx` is the kernel's ucontext_t (or null, which this handles).
        pc: unsafe { instruction_pointer(ctx) },
        module: [0; MODULE_BYTES],
        module_len: 0,
        module_base: 0,
    }
}

/// The instruction pointer in a `ucontext_t`, per architecture.
///
/// # Safety
/// `ctx` must be null or a `ucontext_t` the kernel passed to a signal handler.
unsafe fn instruction_pointer(ctx: *mut libc::c_void) -> usize {
    if ctx.is_null() {
        return 0;
    }
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: the kernel's ucontext_t; its gregs hold REG_RIP.
        unsafe { (*(ctx as *const libc::ucontext_t)).uc_mcontext.gregs[libc::REG_RIP as usize] as usize }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: the kernel's ucontext_t; its mcontext holds the pc.
        unsafe { (*(ctx as *const libc::ucontext_t)).uc_mcontext.pc as usize }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        let _ = ctx;
        0
    }
}

// libgcc's unwind entry points (the C runtime already links libgcc_s for Rust's unwinding).
extern "C" {
    fn _Unwind_Backtrace(trace: extern "C" fn(*mut libc::c_void, *mut libc::c_void) -> libc::c_int, arg: *mut libc::c_void) -> libc::c_int;
    fn _Unwind_GetIP(ctx: *mut libc::c_void) -> usize;
}

/// `_Unwind_Reason_Code`: only `_URC_NO_REASON` (0) goes on, so any other value stops it.
const UNWIND_STOP: libc::c_int = 5;

struct Trace {
    frames: *mut [usize; MAX_FRAMES],
    count: usize,
}

extern "C" fn trace_frame(ctx: *mut libc::c_void, arg: *mut libc::c_void) -> libc::c_int {
    // SAFETY: `arg` is the `Trace` `unwind` passes, live across the call.
    let trace = unsafe { &mut *(arg as *mut Trace) };
    // SAFETY: `ctx` is libgcc's unwind context for this frame.
    let ip = unsafe { _Unwind_GetIP(ctx) };
    if ip != 0 && trace.count < MAX_FRAMES {
        // SAFETY: `frames` is the caller's array, live across the call, `count` bounded.
        unsafe { (*trace.frames)[trace.count] = ip };
        trace.count += 1;
    }
    if trace.count >= MAX_FRAMES {
        return UNWIND_STOP;
    }
    0
}

/// Collect the frames' instruction pointers, dropping the unwinder's own first frame.
fn unwind(frames: &mut [usize; MAX_FRAMES]) -> usize {
    let mut trace = Trace { frames: frames as *mut _, count: 0 };
    // SAFETY: the callback and its argument are live for the call; the unwinder itself
    // allocates nothing.
    unsafe { _Unwind_Backtrace(trace_frame, &mut trace as *mut Trace as *mut libc::c_void) };
    let n = trace.count;
    if n > 1 {
        // The first is `unwind`'s own return address (the handler), not a faulting frame.
        frames.copy_within(1..n, 0);
        return n - 1;
    }
    0
}

/// The `Line` buffer: fixed bytes, no formatting into a `String` (a handler may not allocate).
struct Line<'a> {
    bytes: &'a mut [u8],
    len: usize,
}

impl Line<'_> {
    fn new(bytes: &mut [u8]) -> Line<'_> {
        Line { bytes, len: 0 }
    }

    fn push(&mut self, text: &[u8]) {
        let n = text.len().min(self.bytes.len() - self.len);
        self.bytes[self.len..self.len + n].copy_from_slice(&text[..n]);
        self.len += n;
    }

    /// `0x` and the number in lowercase hex, no leading zeros.
    fn push_hex(&mut self, mut value: usize) {
        self.push(b"0x");
        let mut digits = [0u8; 16];
        let mut n = 0;
        loop {
            digits[n] = b"0123456789abcdef"[value & 0xf];
            n += 1;
            value >>= 4;
            if value == 0 {
                break;
            }
        }
        for i in (0..n).rev() {
            self.push(&digits[i..=i]);
        }
    }

    /// An unsigned number in decimal.
    fn push_dec(&mut self, mut value: usize) {
        let mut digits = [0u8; 20];
        let mut n = 0;
        loop {
            digits[n] = b'0' + (value % 10) as u8;
            n += 1;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        for i in (0..n).rev() {
            self.push(&digits[i..=i]);
        }
    }

    fn push_padded(&mut self, value: i64, width: usize) {
        // At most 20 columns; the stamp's fields are 2, 4 and 2 wide.
        let mut digits = [0u8; 20];
        let mut n = 0;
        let mut v = value;
        loop {
            digits[n] = b'0' + (v % 10) as u8;
            n += 1;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        for _ in n..width.min(20) {
            self.push(b"0");
        }
        for i in (0..n).rev() {
            self.push(&digits[i..=i]);
        }
    }

    /// The run log's line prefix: `YYYYMMDD-HHMMSS ERROR nvforever::crash: ` (the log
    /// plugin's own stamp prints this way for native crashes: `crash::native_crash_line`).
    fn push_prefix(&mut self, at: u64) {
        let (y, m, d) = crate::crash::civil_from_days((at / 86_400) as i64);
        self.push_padded(y, 4);
        self.push_padded(i64::from(m), 2);
        self.push_padded(i64::from(d), 2);
        self.push(b"-");
        self.push_padded((at / 3600 % 24) as i64, 2);
        self.push_padded((at / 60 % 60) as i64, 2);
        self.push_padded((at % 60) as i64, 2);
        self.push(b" ERROR nvforever::crash: ");
    }
}

/// Compose the crash's lines in one buffer: the signal with the fault address, the code's
/// module and the top frames, one line each, `<stamp> ERROR nvforever::crash: ` prefixed.
/// The handler writes the two halves separately (the signal line before it unwinds), so this
/// is the shape the tests read.
#[cfg(test)]
fn write_fault_line(fault: &Fault, frames: &[usize], maps: &[u8], line: &mut Line) -> usize {
    compose_fault_line(fault, line);
    compose_frames(frames, maps, line);
    line.len
}

/// The one line that names the crash: signal, fault address, and where the code was. Needs
/// no unwinding, so the handler writes it before it walks the stack.
fn compose_fault_line(fault: &Fault, line: &mut Line) -> usize {
    line.push_prefix(now_secs());
    line.push(b"native crash, signal ");
    line.push(signal_name(fault.signal));
    line.push(b" (");
    line.push_dec(fault.signal as usize);
    line.push(b"), fault address ");
    line.push_hex(fault.address);
    line.push(b"; at ");
    line.push_hex(fault.pc);
    line.push(b" ");
    let (module, module_len) = fault.module_and_offset();
    line.push(&module[..module_len]);
    line.push(b"+");
    line.push_hex(fault.pc.wrapping_sub(fault.module_base));
    line.push(b"\n");
    line.len
}

/// One line per frame: address and module + offset, as `_Unwind_Backtrace` gave them.
fn compose_frames(frames: &[usize], maps: &[u8], line: &mut Line) -> usize {
    for (i, &ip) in frames.iter().enumerate() {
        line.push_prefix(now_secs());
        line.push(b"  frame ");
        line.push_dec(i + 1);
        line.push(b": ");
        line.push_hex(ip);
        line.push(b" ");
        if let Some((path, base)) = map_of(ip, maps) {
            let n = path.len().min(MODULE_BYTES);
            line.push(&path[..n]);
            line.push(b"+");
            line.push_hex(ip.wrapping_sub(base));
        } else {
            line.push(b"unknown");
        }
        line.push(b"\n");
    }
    line.len
}

/// The signal's name, as a player and a developer read it.
fn signal_name(sig: libc::c_int) -> &'static [u8] {
    match sig {
        libc::SIGSEGV => b"SIGSEGV",
        libc::SIGBUS => b"SIGBUS",
        libc::SIGABRT => b"SIGABRT",
        libc::SIGFPE => b"SIGFPE",
        libc::SIGILL => b"SIGILL",
        _ => b"SIGUNKNOWN",
    }
}

/// The module holding `pc` and its base: the line of `/proc/self/maps` whose range holds it.
fn map_of(pc: usize, maps: &[u8]) -> Option<(&[u8], usize)> {
    for line in maps.split(|&b| b == b'\n') {
        let Some(dash) = line.iter().position(|&b| b == b'-') else { continue };
        let Some(start) = parse_hex(&line[..dash]) else { continue };
        let rest = &line[dash + 1..];
        let Some(space) = rest.iter().position(|&b| b == b' ') else { continue };
        let Some(end) = parse_hex(&rest[..space]) else { continue };
        if pc < start || pc >= end {
            continue;
        }
        // `start-end perms offset dev inode pathname`: four more fields, then the pathname
        // (the inode column is padded, so its end is not the fifth space).
        // `rest` still starts with the end address: begin after it.
        let mut at = space + 1;
        for _ in 0..4 {
            while rest.get(at) == Some(&b' ') {
                at += 1;
            }
            while rest.get(at).is_some_and(|&b| b != b' ') {
                at += 1;
            }
        }
        while rest.get(at) == Some(&b' ') {
            at += 1;
        }
        let path = rest.get(at..).unwrap_or_default();
        let path = path.strip_suffix(b"\r").unwrap_or(path);
        return Some((path, start));
    }
    None
}

/// A hexadecimal number from `/proc/self/maps`, without `str` or allocation.
fn parse_hex(bytes: &[u8]) -> Option<usize> {
    let mut value = 0usize;
    for &b in bytes {
        let digit = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => return None,
        };
        value = value.checked_mul(16)?.checked_add(usize::from(digit))?;
    }
    Some(value)
}

/// `/proc/self/maps`, read into the fixed buffer with plain syscalls (no allocator, no
/// loader lock). An empty slice when it cannot be read: modules are then `unknown`.
///
/// # Safety
/// Must be called after `install` allocated the buffer (the handler only runs then).
unsafe fn read_maps() -> &'static [u8] {
    let ptr = MAPS_BUF.load(Ordering::SeqCst) as *mut u8;
    if ptr.is_null() {
        return &[];
    }
    const PATH: &[u8] = b"/proc/self/maps\0";
    // SAFETY: PATH is NUL-terminated; the fd is closed below.
    let fd = unsafe { libc::open(PATH.as_ptr().cast(), libc::O_RDONLY | libc::O_CLOEXEC) };
    if fd < 0 {
        return &[];
    }
    let mut total = 0;
    while total < MAPS_BYTES {
        // SAFETY: `ptr + total` has `MAPS_BYTES - total` bytes left.
        let n = unsafe { libc::read(fd, ptr.add(total).cast(), MAPS_BYTES - total) };
        if n <= 0 {
            break;
        }
        total += n as usize;
    }
    // SAFETY: closing the fd just opened.
    unsafe { libc::close(fd) };
    // SAFETY: `ptr` is process-lifetime memory of MAPS_BYTES; `total` was read into it.
    unsafe { std::slice::from_raw_parts(ptr, total) }
}

/// The player's home folder, already in the fixed buffer (empty when it was not set).
fn kept(ptr: &AtomicUsize, len_at: &AtomicUsize) -> &'static [u8] {
    let p = ptr.load(Ordering::SeqCst) as *const u8;
    let len = len_at.load(Ordering::SeqCst);
    if p.is_null() || len == 0 {
        return &[];
    }
    // SAFETY: `p` points at `len` bytes copied at install, live for the process.
    unsafe { std::slice::from_raw_parts(p, len) }
}

fn home() -> &'static [u8] {
    kept(&HOME_BUF, &HOME_LEN)
}

/// The paths a line writes `~` for: the home folder and the removable-drive mounts under the
/// player's name (empty entries are skipped).
fn hide() -> [&'static [u8]; 3] {
    [home(), kept(&MEDIA_BUF[0], &MEDIA_LEN[0]), kept(&MEDIA_BUF[1], &MEDIA_LEN[1])]
}

/// Write `text` to the run log and the console with `write(2)`: no lock, no buffering (the
/// log plugin may be the thing that crashed). The console is written too because a player
/// started from a terminal sees it, and `tools/smoke` reads both.
fn write_lines(text: &[u8]) {
    if text.is_empty() {
        return;
    }
    let fd = RUN_LOG_FD.load(Ordering::SeqCst);
    if fd >= 0 {
        // SAFETY: the fd was opened for append at install; a short write is retried below.
        unsafe { write_all(fd, text) };
    }
    // SAFETY: fd 2 is the console; a closed one just fails.
    unsafe { write_all(2, text) };
}

/// `write(2)` until every byte is out or the kernel refuses.
///
/// # Safety
/// `fd` must be a file descriptor this process may write to.
unsafe fn write_all(fd: libc::c_int, mut text: &[u8]) {
    while !text.is_empty() {
        // SAFETY: `text` is a live slice of `len` bytes.
        let n = unsafe { libc::write(fd, text.as_ptr().cast(), text.len()) };
        if n <= 0 {
            return;
        }
        text = &text[n as usize..];
    }
}

/// Restore the default disposition, unblock the signal and re-raise it: the process ends by
/// the same signal, so the exit status is unchanged (`-11` for `SIGSEGV`).
fn reraise(sig: libc::c_int) -> ! {
    // SAFETY: plain signal(2) calls on this thread; `_exit` never returns.
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = libc::SIG_DFL;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(sig, &sa, std::ptr::null_mut());
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, sig);
        libc::sigprocmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut());
        libc::raise(sig);
        libc::_exit(128 + sig);
    }
}

/// Seconds since the epoch, without `SystemTime`'s formatting (the prefix is built above).
fn now_secs() -> u64 {
    // SAFETY: CLOCK_REALTIME is always available; a failure reads as 1970.
    let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
    if unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, &mut ts) } != 0 {
        return 0;
    }
    ts.tv_sec.max(0) as u64
}

/// `text` with every `folder` written as `~`, in place, in a fixed buffer: the same
/// treatment `crash::redact` gives logged lines, without touching the allocator. Letters in
/// any case, `\` and `/` alike, a run of separators counting as one, and only where the
/// folder name ends (a non-name byte or a separator), so `/home/al` leaves `/home/alex`.
fn redact_in_place(line: &mut [u8], len: usize, prefixes: &[&[u8]]) -> usize {
    if prefixes.iter().all(|p| p.is_empty()) || len == 0 {
        return len;
    }
    let sep = |c: u8| matches!(c, b'\\' | b'/');
    let name = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-');
    let mut out = 0;
    let mut i = 0;
    while i < len {
        let hit = prefixes.iter().filter(|p| !p.is_empty()).find_map(|prefix| {
            if !(line[i] == prefix[0] || (sep(prefix[0]) && sep(line[i]))) {
                return None;
            }
            // Does `prefix` start at `i`?
            let (mut li, mut hi) = (i, 0);
            let mut match_end = None;
            while hi < prefix.len() {
                if sep(prefix[hi]) {
                    if !line.get(li).is_some_and(|&c| sep(c)) {
                        break;
                    }
                    while hi < prefix.len() && sep(prefix[hi]) {
                        hi += 1;
                    }
                    while li < len && sep(line[li]) {
                        li += 1;
                    }
                } else {
                    if !line.get(li).is_some_and(|c| c.eq_ignore_ascii_case(&prefix[hi])) {
                        break;
                    }
                    li += 1;
                    hi += 1;
                }
                if hi == prefix.len() {
                    match_end = Some(li);
                }
            }
            // The folder name must end there (the tail of a longer name is not a match).
            match_end.filter(|&end| end == len || sep(line[end]) || !name(line[end]))
        });
        if let Some(end) = hit {
            line[out] = b'~';
            out += 1;
            i = end;
            continue;
        }
        line[out] = line[i];
        out += 1;
        i += 1;
    }
    // Zero the moved tail: the caller may keep the buffer.
    for b in &mut line[out..len] {
        *b = 0;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(line: &[u8], len: usize) -> String {
        String::from_utf8_lossy(&line[..len]).into_owned()
    }

    #[test]
    fn a_ucontext_gives_its_instruction_pointer() {
        // SAFETY: a zeroed ucontext_t is only given to the accessor; REG_RIP is set after.
        let mut ctx: libc::ucontext_t = unsafe { std::mem::zeroed() };
        #[cfg(target_arch = "x86_64")]
        {
            ctx.uc_mcontext.gregs[libc::REG_RIP as usize] = 0x7f00_1234_5678;
        }
        let fault = unsafe { fault_from(libc::SIGSEGV, std::ptr::null_mut(), &mut ctx as *mut _ as *mut libc::c_void) };
        assert_eq!(fault.signal, libc::SIGSEGV);
        assert_eq!(fault.address, 0);
        #[cfg(target_arch = "x86_64")]
        assert_eq!(fault.pc, 0x7f00_1234_5678);
    }

    #[test]
    fn the_line_names_the_signal_address_module_and_frames() {
        let fault = Fault { signal: libc::SIGSEGV, address: 0x10, pc: 0x7f00_0000_1000, module: [0; MODULE_BYTES], module_len: 0, module_base: 0 };
        let mut buf = [0u8; LINE_BYTES];
        let mut line = Line::new(&mut buf);
        let n = write_fault_line(&fault, &[0x7f00_0000_2000, 0x7f00_0000_3000], b"", &mut line);
        let line = text(line.bytes, n);
        assert!(line.contains("native crash, signal SIGSEGV (11), fault address 0x10; at 0x7f0000001000 unknown+0x7f0000001000"), "{line}");
        assert!(line.contains("frame 1: 0x7f0000002000 unknown"), "{line}");
        assert!(line.contains("frame 2: 0x7f0000003000 unknown"), "{line}");
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches("ERROR nvforever::crash:").count(), 3);
    }

    #[test]
    fn a_module_comes_from_the_maps_line() {
        let maps = b"7f0000000000-7f0000010000 r-xp 00000000 08:01 1 /usr/lib/libc.so.6\n7f0000100000-7f0000200000 rw-p 00000000 00:00 0 [heap]\n";
        let (path, base) = map_of(0x7f00_0000_1234, maps).expect("a mapping holds the pc");
        assert_eq!(path, b"/usr/lib/libc.so.6");
        let padded = b"7c97d8400000-7c97d8428000 r--p 00000000 08:01 47194014                   /usr/lib/libc.so.6\n";
        assert_eq!(map_of(0x7c97_d841_0000, padded).map(|m| m.0), Some(&b"/usr/lib/libc.so.6"[..]));
        assert_eq!(base, 0x7f00_0000_0000);
        assert!(map_of(0x1000, maps).is_none());
        assert_eq!(map_of(0x7f00_0010_0000, maps).map(|m| m.0), Some(&b"[heap]"[..]));
    }

    #[test]
    fn the_home_folder_is_written_as_a_tilde() {
        let mut buf = *b"at /home/alex/nvf/target/release/nvforever+0x10 and /home/alexx/x and /home/alex\n";
        let len = buf.len();
        let n = redact_in_place(&mut buf, len, &[b"/home/alex"]);
        assert_eq!(text(&buf, n), "at ~/nvf/target/release/nvforever+0x10 and /home/alexx/x and ~\n");
    }

    #[test]
    fn a_removable_drive_under_the_players_name_is_written_as_a_tilde_too() {
        let mut buf = *b"at /run/media/alex/USB/nvforever+0x10 and /media/alex/DVD/x.so+0x20 and /media/bob/x\n";
        let len = buf.len();
        let n = redact_in_place(&mut buf, len, &[b"/home/alex", b"/run/media/alex", b"/media/alex"]);
        assert_eq!(text(&buf, n), "at ~/USB/nvforever+0x10 and ~/DVD/x.so+0x20 and /media/bob/x\n");
    }

    #[test]
    fn a_signal_numbers_its_name() {
        assert_eq!(signal_name(libc::SIGSEGV), b"SIGSEGV");
        assert_eq!(signal_name(libc::SIGABRT), b"SIGABRT");
        assert_eq!(signal_name(libc::SIGTERM), b"SIGUNKNOWN");
    }
}
