"""A game launch's window held in front while it runs (a scenario's `front = true`).

Some checks need the game's window to be the foreground window: NVIDIA's DLSS Frame
Generation generates frames only while its window is in front, and smoke's launches, started
one after another from a console, do not get the foreground by themselves. On Windows a
thread looks for the launched process's own top-level window once a second and, when another
window is in front, brings it forward with `SetForegroundWindow`, after a tap of Alt (Windows
lets a process set the foreground window only in some cases, one of them that the last input
event went to it or was a key the system takes as such; the tap is the usual way to meet
that). Only that process's windows are touched, and the thread stops with the launch.
Elsewhere it does nothing.
"""
from __future__ import annotations

import ctypes
import os
import threading
from typing import Iterable, Protocol


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

    def __init__(self, pid: int, period: float = 1.0, api: WindowApi | None = None):
        self.pid = pid
        self.period = period
        self.api = api if api is not None else (Win32() if os.name == "nt" else None)
        self.brought = 0  # times the window was brought forward
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None

    def once(self) -> bool:
        """One look: True when the window was found (in front already or brought forward)."""
        if self.api is None:
            return False
        hwnd = pick_window(self.pid, self.api.windows())
        if hwnd is None:
            return False
        if self.api.foreground() != hwnd:
            self.api.bring_forward(hwnd)
            self.brought += 1
        return True

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
        self.user32.SetForegroundWindow(hwnd)


def refusal(front_launches: list[str], slots: int, exclusive: bool) -> str | None:
    """Why launches asking for their window in front cannot run this way: they need the screen to
    themselves, so one game at a time (`--slots 1` or `--exclusive`)."""
    if front_launches and slots > 1 and not exclusive:
        return (f"{', '.join(front_launches)} hold their window in front (front = true), which needs one game "
                f"on the machine at a time: run with --slots 1 (or --exclusive), not {slots} slots")
    return None
