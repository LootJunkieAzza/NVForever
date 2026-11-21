"""Every thread's call stack of a running process, for a launch that hung (Windows x64).

The runner calls `dump` before it kills a game that ran past its time, so a rare hang
leaves the place it stopped in the log folder (`<launch>.stacks.txt`) instead of only a
log that goes quiet. The stacks come from dbghelp's StackWalk64; function names and
source lines come from the PDB next to the exe (release builds of nvforever carry debug
info). Each thread is suspended while it is walked and resumed after. Nothing here
raises: a failure is said in the text.
"""
from __future__ import annotations

import ctypes
import sys
from pathlib import Path

KITS_DBGHELP = Path(r"C:\Program Files (x86)\Windows Kits\10\Debuggers\x64\dbghelp.dll")


def dump(pid: int, symbols: Path | None = None, max_frames: int = 80) -> str:
    """The stacks of every thread of `pid` as text (a line per frame)."""
    if sys.platform != "win32" or ctypes.sizeof(ctypes.c_void_p) != 8:
        return "thread stacks: only on 64-bit Windows\n"
    try:
        return _dump(pid, symbols, max_frames)
    except Exception as e:  # noqa: BLE001 - a diagnostic must never fail the run
        return f"thread stacks: {type(e).__name__}: {e}\n"


def _dump(pid: int, symbols: Path | None, max_frames: int) -> str:
    from ctypes import wintypes as W

    class THREADENTRY32(ctypes.Structure):
        _fields_ = [("dwSize", W.DWORD), ("cntUsage", W.DWORD), ("th32ThreadID", W.DWORD),
                    ("th32OwnerProcessID", W.DWORD), ("tpBasePri", W.LONG), ("tpDeltaPri", W.LONG),
                    ("dwFlags", W.DWORD)]

    u64 = ctypes.c_uint64
    regs = ["Rax", "Rcx", "Rdx", "Rbx", "Rsp", "Rbp", "Rsi", "Rdi",
            "R8", "R9", "R10", "R11", "R12", "R13", "R14", "R15", "Rip"]

    class CONTEXT(ctypes.Structure):  # x64 CONTEXT, 1232 bytes
        _fields_ = ([(f"P{i}Home", u64) for i in range(1, 7)]
                    + [("ContextFlags", W.DWORD), ("MxCsr", W.DWORD)]
                    + [(s, W.WORD) for s in ("SegCs", "SegDs", "SegEs", "SegFs", "SegGs", "SegSs")]
                    + [("EFlags", W.DWORD)]
                    + [(d, u64) for d in ("Dr0", "Dr1", "Dr2", "Dr3", "Dr6", "Dr7")]
                    + [(r, u64) for r in regs]
                    + [("Rest", ctypes.c_byte * (512 + 26 * 16 + 6 * 8))])

    class ADDRESS64(ctypes.Structure):
        _fields_ = [("Offset", u64), ("Segment", W.WORD), ("Mode", ctypes.c_int)]

    class STACKFRAME64(ctypes.Structure):
        _fields_ = [("AddrPC", ADDRESS64), ("AddrReturn", ADDRESS64), ("AddrFrame", ADDRESS64),
                    ("AddrStack", ADDRESS64), ("AddrBStore", ADDRESS64), ("FuncTableEntry", ctypes.c_void_p),
                    ("Params", u64 * 4), ("Far", W.BOOL), ("Virtual", W.BOOL), ("Reserved", u64 * 3),
                    ("KdHelp", u64 * 20)]

    class SYMBOL_INFO(ctypes.Structure):
        _fields_ = [("SizeOfStruct", W.ULONG), ("TypeIndex", W.ULONG), ("Reserved", u64 * 2), ("Index", W.ULONG),
                    ("Size", W.ULONG), ("ModBase", u64), ("Flags", W.ULONG), ("Value", u64), ("Address", u64),
                    ("Register", W.ULONG), ("Scope", W.ULONG), ("Tag", W.ULONG), ("NameLen", W.ULONG),
                    ("MaxNameLen", W.ULONG), ("Name", ctypes.c_char * 1024)]

    class LINE64(ctypes.Structure):
        _fields_ = [("SizeOfStruct", W.DWORD), ("Key", ctypes.c_void_p), ("LineNumber", W.DWORD),
                    ("FileName", ctypes.c_char_p), ("Address", u64)]

    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    dbg = ctypes.WinDLL(str(KITS_DBGHELP) if KITS_DBGHELP.exists() else "dbghelp", use_last_error=True)
    k32.OpenProcess.restype = k32.OpenThread.restype = k32.CreateToolhelp32Snapshot.restype = W.HANDLE
    k32.OpenThread.argtypes = [W.DWORD, W.BOOL, W.DWORD]
    k32.OpenProcess.argtypes = [W.DWORD, W.BOOL, W.DWORD]
    k32.SuspendThread.argtypes = k32.ResumeThread.argtypes = k32.CloseHandle.argtypes = [W.HANDLE]
    k32.GetThreadContext.argtypes = [W.HANDLE, ctypes.c_void_p]
    dbg.SymFunctionTableAccess64.restype = ctypes.c_void_p
    dbg.SymFunctionTableAccess64.argtypes = [W.HANDLE, u64]
    dbg.SymGetModuleBase64.restype = u64
    dbg.SymGetModuleBase64.argtypes = [W.HANDLE, u64]
    dbg.SymInitialize.argtypes = [W.HANDLE, ctypes.c_char_p, W.BOOL]
    dbg.SymFromAddr.argtypes = [W.HANDLE, u64, ctypes.POINTER(u64), ctypes.POINTER(SYMBOL_INFO)]
    dbg.SymGetLineFromAddr64.argtypes = [W.HANDLE, u64, ctypes.POINTER(W.DWORD), ctypes.POINTER(LINE64)]
    table = ctypes.WINFUNCTYPE(ctypes.c_void_p, W.HANDLE, u64)(lambda h, a: dbg.SymFunctionTableAccess64(h, a))
    base = ctypes.WINFUNCTYPE(u64, W.HANDLE, u64)(lambda h, a: dbg.SymGetModuleBase64(h, a))
    dbg.StackWalk64.argtypes = [W.DWORD, W.HANDLE, W.HANDLE, ctypes.POINTER(STACKFRAME64), ctypes.c_void_p,
                                ctypes.c_void_p, type(table), type(base), ctypes.c_void_p]

    proc = k32.OpenProcess(0x1F0FFF, False, pid)
    if not proc:
        return f"thread stacks: cannot open process {pid} (error {ctypes.get_last_error()})\n"
    out = []
    dbg.SymSetOptions(0x2 | 0x10 | 0x4)  # undecorated names, line numbers, deferred loads
    path = str(symbols).encode() if symbols else None
    if not dbg.SymInitialize(proc, path, True):
        out.append(f"(symbols not initialised: error {ctypes.get_last_error()})")
    tids = []
    snap = k32.CreateToolhelp32Snapshot(4, 0)  # TH32CS_SNAPTHREAD
    te = THREADENTRY32()
    te.dwSize = ctypes.sizeof(te)
    ok = k32.Thread32First(snap, ctypes.byref(te))
    while ok:
        if te.th32OwnerProcessID == pid:
            tids.append(te.th32ThreadID)
        ok = k32.Thread32Next(snap, ctypes.byref(te))
    k32.CloseHandle(snap)
    sym = SYMBOL_INFO()
    for tid in tids:
        thread = k32.OpenThread(0x1F03FF, False, tid)
        if not thread:
            continue
        name = ctypes.c_wchar_p()
        if hasattr(k32, "GetThreadDescription"):
            k32.GetThreadDescription(thread, ctypes.byref(name))
        out.append(f"=== thread {tid} {name.value or ''}".rstrip())
        k32.SuspendThread(thread)
        try:
            buf = ctypes.create_string_buffer(ctypes.sizeof(CONTEXT) + 16)
            at = (ctypes.addressof(buf) + 15) & ~15  # CONTEXT is 16-byte aligned
            ctx = CONTEXT.from_address(at)
            ctx.ContextFlags = 0x10000B  # CONTEXT_FULL
            if not k32.GetThreadContext(thread, at):
                out.append(f"  (no context: error {ctypes.get_last_error()})")
                continue
            sf = STACKFRAME64()
            for a, v in ((sf.AddrPC, ctx.Rip), (sf.AddrFrame, ctx.Rbp), (sf.AddrStack, ctx.Rsp)):
                a.Offset, a.Mode = v, 3  # AddrModeFlat
            for _ in range(max_frames):
                if not dbg.StackWalk64(0x8664, proc, thread, ctypes.byref(sf), at, None, table, base, None):
                    break
                pc = sf.AddrPC.Offset
                if not pc:
                    break
                sym.SizeOfStruct, sym.MaxNameLen = ctypes.sizeof(SYMBOL_INFO) - 1024, 1024
                disp = u64()
                fn = sym.Name.decode(errors="replace") if dbg.SymFromAddr(proc, pc, ctypes.byref(disp), ctypes.byref(sym)) else "?"
                line = LINE64()
                line.SizeOfStruct = ctypes.sizeof(line)
                d32 = W.DWORD()
                src = ""
                if dbg.SymGetLineFromAddr64(proc, pc, ctypes.byref(d32), ctypes.byref(line)) and line.FileName:
                    src = f"  {line.FileName.decode(errors='replace')}:{line.LineNumber}"
                out.append(f"  {pc:016x} {fn[:240]}{src}")
        finally:
            k32.ResumeThread(thread)
            k32.CloseHandle(thread)
    dbg.SymCleanup(proc)
    k32.CloseHandle(proc)
    return "\n".join(out) + "\n"


if __name__ == "__main__":
    # python -m smoke.stacks PID [SYMBOL_DIR]: the same by hand, for a game that hangs now.
    sys.stdout.write(dump(int(sys.argv[1]), Path(sys.argv[2]) if len(sys.argv) > 2 else None))
