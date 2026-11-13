"""A game launch's window held in front while it runs (a scenario's `front = true`).

Some checks need the game's window to be the foreground window: NVIDIA's DLSS Frame
Generation generates frames only while its window is in front, and smoke's launches, started
one after another from a console, do not get the foreground by themselves. On Windows a
thread looks for the launched process's own top-level window once a second and, when another
window is in front, brings it forward with `SetForegroundWindow`, after a tap of Alt (Windows
lets a process set the foreground window only in some cases, one of them that the last input
event went to it or was a key the system takes as such; the tap is the usual way to meet
that). The tap goes to the window in front at the time, never the game's (it is only made
while another window is in front), and the window is brought forward [`TAP_SETTLE`] after the
key-up, so the key-up has reached that window first: Alt is NVForever's default Block key
(`engine/nvf-play/src/bindings.rs`), which the game must not see. Only that process's windows
are touched, and the thread stops with the launch. What it did goes to `<launch>.front.txt`
beside the log (the game writes the log itself, so the runner does not write into it).
Elsewhere it does nothing.

Launches that ask for it run after the others, one at a time with the machine to themselves
(both game locks, as `--exclusive`), whatever `--slots` says ([`split`]).
"""
from __future__ import annotations

import ctypes
import os
import threading
import time
from pathlib import Path
from typing import Iterable, Protocol

#: Seconds between the Alt key-up and `SetForegroundWindow`.
TAP_SETTLE = 0.05


class WindowApi(Protocol):
    def windows(self) -> Iterable[tuple[int, int, bool, bool]]:
        """Top-level windows: (handle, owning process id, visible, has an owner window)."""

    def foreground(self) -> int: ...

    def bring_forward(self, hwnd: int) -> None: ...


def pick_window(pid: int, windows: Iterable[tuple[int, int, bool, bool]]) -> int | None:
    """The process's main window: its first visible top-level window without an owner."""
    for hwnd, owner_pid, visible, owned in windows:
        if owner_pid == pid and visible and not owned:
            return hwnd
    return None


class Front:
    """Holds the main window of process `pid` in front, every `period` seconds, until stopped."""

    def __init__(self, pid: int, period: float = 1.0, api: WindowApi | None = None, log: Path | None = None):
        self.pid = pid
        self.period = period
        self.api = api if api is not None else (Win32() if os.name == "nt" else None)
        self.log = log
        self.brought = 0  # times the window was brought forward
        self.found = False  # the window was found at least once
        self.looks = 0
        self._started = time.monotonic()
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None

    def _write(self, line: str) -> None:
        if self.log is not None:
            try:
                with open(self.log, "a", encoding="utf-8") as f:
                    f.write(f"+{time.monotonic() - self._started:.1f} s {line}\n")
            except OSError:
                pass

    def once(self) -> bool:
        """One look: True when the window was found (in front already or brought forward)."""
        if self.api is None:
            return False
        self.looks += 1
        hwnd = pick_window(self.pid, self.api.windows())
        if hwnd is None:
            return False
        if not self.found:
            self.found = True
            self._write(f"front: process {self.pid}'s window found")
        if self.api.foreground() != hwnd:
            self.api.bring_forward(hwnd)
            self.brought += 1
            self._write(f"front: brought forward ({self.brought})")
        return True

    def summary(self) -> str:
        """One line for the run's output."""
        if self.api is None:
            return "front: not held (not on Windows)"
        if not self.found:
            return f"front: the window was never found ({self.looks} looks); the launch ran without being held in front"
        return f"front: window held in front, brought forward {self.brought} time(s) in {self.looks} looks"

    def _run(self) -> None:
        while not self._stop.is_set():
            try:
                self.once()
            except OSError:
                pass
            self._stop.wait(self.period)

    def start(self) -> "Front":
        if self.api is not None and self._thread is None:
            self._thread = threading.Thread(target=self._run, name=f"smoke front {self.pid}", daemon=True)
            self._thread.start()
        return self

    def stop(self) -> None:
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=5)
            self._thread = None
        self._write(self.summary())


class Win32:
    """The user32 calls [documented: Microsoft Learn, EnumWindows, GetWindowThreadProcessId,
    IsWindowVisible, GetWindow (GW_OWNER), IsIconic, ShowWindow (SW_RESTORE), keybd_event,
    SetForegroundWindow and its remarks on when a process may set the foreground window]."""

    GW_OWNER = 4
    SW_RESTORE = 9
    VK_MENU = 0x12
    KEYEVENTF_KEYUP = 0x0002

    def __init__(self):
        from ctypes import wintypes

        self.user32 = ctypes.WinDLL("user32", use_last_error=True)
        self.wintypes = wintypes
        u = self.user32
        u.EnumWindows.argtypes = [ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM), wintypes.LPARAM]
        u.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
        u.GetWindowThreadProcessId.restype = wintypes.DWORD
        u.IsWindowVisible.argtypes = [wintypes.HWND]
        u.GetWindow.argtypes = [wintypes.HWND, wintypes.UINT]
        u.GetWindow.restype = wintypes.HWND
        u.GetForegroundWindow.restype = wintypes.HWND
        u.SetForegroundWindow.argtypes = [wintypes.HWND]
        u.IsIconic.argtypes = [wintypes.HWND]
        u.ShowWindow.argtypes = [wintypes.HWND, ctypes.c_int]
        u.keybd_event.argtypes = [wintypes.BYTE, wintypes.BYTE, wintypes.DWORD, ctypes.c_size_t]

    def windows(self):
        wt = self.wintypes
        found: list[tuple[int, int, bool, bool]] = []

        @ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)
        def each(hwnd, _):
            pid = wt.DWORD()
            self.user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
            visible = bool(self.user32.IsWindowVisible(hwnd))
            owned = bool(self.user32.GetWindow(hwnd, self.GW_OWNER))
            found.append((int(hwnd or 0), int(pid.value), visible, owned))
            return True

        self.user32.EnumWindows(each, 0)
        return found

    def foreground(self) -> int:
        return int(self.user32.GetForegroundWindow() or 0)

    def bring_forward(self, hwnd: int) -> None:
        if self.user32.IsIconic(hwnd):
            self.user32.ShowWindow(hwnd, self.SW_RESTORE)
        self.user32.keybd_event(self.VK_MENU, 0, 0, 0)
        self.user32.keybd_event(self.VK_MENU, 0, self.KEYEVENTF_KEYUP, 0)
        # The key-up reaches the window in front now before the game is brought forward.
        time.sleep(TAP_SETTLE)
        self.user32.SetForegroundWindow(hwnd)


def split(launches: list) -> tuple[list, list]:
    """(the launches run as usual, the game launches holding their window in front), each in
    plan order: the second run after the first, one at a time with the machine to themselves."""
    fronts = [x for x in launches if getattr(x, "front", False)]
    return [x for x in launches if x not in fronts], fronts


def last_line(path: Path) -> str | None:
    """The last line of a launch's `.front.txt` (its summary), if any."""
    try:
        lines = Path(path).read_text(encoding="utf-8").splitlines()
    except OSError:
        return None
    return lines[-1] if lines else None
