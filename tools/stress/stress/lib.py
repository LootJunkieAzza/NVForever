"""What every stress test shares: launching nvforever with test aids under the game
locks, watching it for progress, its memory, crash collection, and the result folder.

The locks, the aid-free environment and the thread-stack dump are tools/smoke's
(imported, not copied): a stress run waits for a smoke run and the other way round.
"""
from __future__ import annotations

import ctypes
import json
import os
import re
import subprocess
import sys
import threading
import time
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "tools" / "smoke"))

from smoke.expect import clean  # noqa: E402
from smoke.runner import Locks, aid_free_environ, default_lock_paths  # noqa: E402
from smoke.stacks import dump as dump_stacks  # noqa: E402

EXE_NAME = "nvforever.exe" if os.name == "nt" else "nvforever"
TOOL_NAME = "fnvtool.exe" if os.name == "nt" else "fnvtool"

# Every stress launch: invulnerable, unseen, script boxes closed, nothing that stops the
# run (nvf-play stress.rs), and no message boxes.
BASE_AIDS = {
    "NVF_PLAY_TEST_GOD": "1",
    "NVF_PLAY_TEST_UNSEEN": "1",
    "NVF_PLAY_TEST_SCRIPT_MENUS": "0.05",
    "NVF_PLAY_TEST_STRESS": "1",
}
BASE_ARGS = ["--play", "--no-messages"]


def masters(repo: Path = REPO) -> list[str]:
    """The ten masters in load order, from tools/smoke/scenarios.toml (one list for both)."""
    with open(repo / "tools" / "smoke" / "scenarios.toml", "rb") as f:
        return list(tomllib.load(f)["masters"])


@dataclass
class Ctx:
    repo: Path
    exe: Path
    data: str
    fnvtool: Path
    out: Path  # this run's folder: target/stress/<test>/<run>
    say: object = print
    locks: Locks | None = None
    masters: list[str] = field(default_factory=list)


def run_folder(repo: Path, test: str, name: str | None = None) -> Path:
    stamp = name or time.strftime("%Y%m%d-%H%M%S")
    d = repo / "target" / "stress" / test / stamp
    d.mkdir(parents=True, exist_ok=True)
    return d


# ---------------------------------------------------------------- memory

def process_memory(pid: int) -> tuple[int, int] | None:
    """(working set, private bytes) of a process, Windows only; None elsewhere or on error."""
    if os.name != "nt":
        return None

    class PMC(ctypes.Structure):
        _fields_ = [("cb", ctypes.c_ulong), ("PageFaultCount", ctypes.c_ulong)] + [
            (n, ctypes.c_size_t) for n in ("PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
                                           "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage",
                                           "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage",
                                           "PrivateUsage")]

    k32 = ctypes.windll.kernel32
    h = k32.OpenProcess(0x1000 | 0x0010, False, pid)  # QUERY_LIMITED_INFORMATION | VM_READ
    if not h:
        return None
    try:
        c = PMC()
        c.cb = ctypes.sizeof(PMC)
        if not k32.K32GetProcessMemoryInfo(h, ctypes.byref(c), c.cb):
            return None
        return int(c.WorkingSetSize), int(c.PrivateUsage)
    finally:
        k32.CloseHandle(h)


# ---------------------------------------------------------------- crash reports

FRAME = re.compile(r"^\s*\d+:\s+(?P<fn>\S.*?)\s*$")
NOISE = ("std::", "core::", "alloc::", "backtrace::", "rust_begin_unwind", "<alloc::", "<core::", "<std::",
         "nvforever::crash::", "BaseThreadInitThunk", "RtlUserThreadStart", "__scrt_common_main")
HASHES = re.compile(r"::h[0-9a-f]{16}$")
NUMBERS = re.compile(r"0x[0-9a-fA-F]+|\d+")


@dataclass
class Crash:
    message: str
    location: str
    frames: list[str]
    text: str
    path: str = ""

    def signature(self, top: int = 5) -> str:
        """The panic's cause as a key: its message with numbers and addresses blanked, where
        it was raised (without the line), and the top frames that are the engine's own."""
        loc = re.sub(r":\d+(:\d+)?$", "", self.location)
        return " | ".join([NUMBERS.sub("#", self.message)[:160], loc, *self.frames[:top]])


def parse_crash(text: str, path: str = "") -> Crash:
    """A crash report (engine/nvforever/src/crash.rs report_text)."""
    message = location = ""
    frames: list[str] = []
    in_bt = False
    for line in text.splitlines():
        if line.startswith("message: ") and not message:
            message = line[len("message: "):].strip()
        elif line.startswith("at: ") and not location:
            location = line[len("at: "):].strip()
        elif line.strip() == "backtrace:":
            in_bt = True
        elif in_bt:
            m = FRAME.match(line)
            if m:
                fn = HASHES.sub("", m.group("fn"))
                if not fn.startswith(NOISE) and fn not in frames:
                    frames.append(fn)
    return Crash(message, location, frames, text, path)


def group_crashes(items: list[tuple[str, Crash]], top: int = 5) -> list[dict]:
    """(what crashed, crash) pairs grouped by signature, the biggest group first."""
    groups: dict[str, dict] = {}
    for what, c in items:
        g = groups.setdefault(c.signature(top), {"signature": c.signature(top), "message": c.message,
                                                  "location": c.location, "frames": c.frames[:top], "where": [],
                                                  "report": c.path})
        g["where"].append(what)
    return sorted(groups.values(), key=lambda g: (-len(g["where"]), g["signature"]))


# ---------------------------------------------------------------- launching

def kill_tree(p: subprocess.Popen) -> None:
    """The process and whatever it started (a wrapper's child too)."""
    if os.name == "nt":
        subprocess.run(["taskkill", "/T", "/F", "/PID", str(p.pid)], capture_output=True)
    p.kill()


@dataclass
class Launch:
    name: str
    args: list[str]
    aids: dict[str, str]
    run_seconds: float
    progress: re.Pattern | None = None  # lines that count as progress
    hang_seconds: float = 180.0  # no progress this long: a hang
    memory_every: float = 0.0  # seconds between memory samples (0: none)


@dataclass
class Result:
    name: str
    exit: int | None
    seconds: float
    status: str  # ok, crashed, hung, failed (non-zero exit without a report)
    log: Path
    lines: list[str]
    crashes: list[Crash]
    memory: list[tuple[float, int, int]]  # (seconds, working set, private bytes)
    stacks: Path | None = None

    def tail(self, n: int = 40) -> list[str]:
        return self.lines[-n:]


def collect_crashes(home: Path) -> list[Crash]:
    logs = home / "logs"
    out = []
    for p in sorted(logs.glob("crash-*.txt")) if logs.is_dir() else []:
        out.append(parse_crash(p.read_text(encoding="utf-8", errors="replace"), str(p)))
    return out


def run(ctx: Ctx, launch: Launch) -> Result:
    """One game launch under the locks. Its own NVForever home (`<out>/<name>.home`) keeps
    its run log and crash reports apart; its console output goes to `<out>/<name>.log`."""
    home = ctx.out / f"{launch.name}.home"
    home.mkdir(parents=True, exist_ok=True)
    log = ctx.out / f"{launch.name}.log"
    cmd = [str(ctx.exe), "--data", str(ctx.data), *BASE_ARGS, *launch.args, "--run-seconds", f"{launch.run_seconds:g}"]
    aids = {**BASE_AIDS, **launch.aids, "NVFOREVER_HOME": str(home)}
    (ctx.out / f"{launch.name}.cmd.txt").write_text(
        "".join(f"{k}={v}\n" for k, v in sorted(aids.items())) + subprocess.list2cmdline(cmd) + "\n", encoding="utf-8")
    if ctx.locks:
        ctx.locks.acquire()
    memory: list[tuple[float, int, int]] = []
    status, stacks = "ok", None
    started = time.monotonic()
    try:
        with open(log, "w", encoding="utf-8", errors="replace") as f:
            p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, cwd=ctx.out,
                                 env=aid_free_environ(aids), text=True, encoding="utf-8", errors="replace")
            last = [time.monotonic()]

            def pump():
                for raw in p.stdout:
                    f.write(raw)
                    f.flush()
                    if launch.progress is None or launch.progress.search(raw):
                        last[0] = time.monotonic()

            t = threading.Thread(target=pump, daemon=True)
            t.start()
            next_mem = 0.0
            limit = launch.run_seconds * 2 + 300
            while p.poll() is None:
                now = time.monotonic()
                if launch.memory_every and now - started >= next_mem:
                    m = process_memory(p.pid)
                    if m:
                        memory.append((round(now - started, 1), *m))
                    next_mem = now - started + launch.memory_every
                if now - last[0] > launch.hang_seconds or now - started > limit:
                    stacks = ctx.out / f"{launch.name}.stacks.txt"
                    stacks.write_text(dump_stacks(p.pid, Path(ctx.exe).parent), encoding="utf-8")
                    kill_tree(p)
                    status = "hung"
                    break
                time.sleep(0.5)
            p.wait()
            t.join(timeout=10)
            p.stdout.close()
    finally:
        if ctx.locks:
            ctx.locks.release()
    secs = time.monotonic() - started
    lines = [clean(x) for x in log.read_text(encoding="utf-8", errors="replace").splitlines()]
    crashes = collect_crashes(home)
    code = p.returncode
    if status != "hung":
        if crashes:
            status = "crashed"
        elif code != 0:
            status = "failed"
    if memory:
        with open(ctx.out / f"{launch.name}.memory.csv", "w", encoding="utf-8") as m:
            m.write("seconds,working_set,private_bytes\n")
            m.writelines(f"{s},{w},{pb}\n" for s, w, pb in memory)
    r = Result(launch.name, code, round(secs, 1), status, log, lines, crashes, memory, stacks)
    (ctx.out / f"{launch.name}.exit.json").write_text(json.dumps(
        {"exit": code, "status": status, "seconds": r.seconds, "crashes": [c.path for c in crashes]}), encoding="utf-8")
    return r


# ---------------------------------------------------------------- results

STRAY = re.compile(r"stray entity command #(\d+) \(ignored; the entity is gone\): (.*?)(?:; where: (.*))?$")


def stray_commands(lines: list[str]) -> dict:
    """Commands the engine sent to entities that were gone (nvforever stray_commands.rs):
    how many warnings, and each distinct command or system with its first place."""
    count, first = 0, {}
    for ln in lines:
        if m := STRAY.search(ln):
            count += 1
            what = m.group(2)
            key = what.split(": ", 1)[0]
            first.setdefault(key, {"what": what, "where": m.group(3) or ""})
    return {"count": count, "distinct": list(first.values())}


def stray_report(s: dict) -> list[str]:
    if not s["count"]:
        return []
    out = ["", f"## Stray entity commands ({s['count']} logged, ignored)", "",
           "Commands for entities that were gone; each names what failed (stray_commands.rs).", ""]
    out += [f"- `{d['what']}`" + (f" at {d['where']}" if d["where"] else "") for d in s["distinct"]]
    return out


def write_results(out: Path, results: dict, report: list[str]) -> None:
    (out / "results.json").write_text(json.dumps(results, indent=1), encoding="utf-8")
    (out / "report.md").write_text("\n".join(report) + "\n", encoding="utf-8")


def mb(n: float) -> str:
    return f"{n / 2**20:.0f} MB"


STAMP = re.compile(r"^\s*(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?)Z?\b")


def stamp_secs(line: str) -> float | None:
    """A log line's time, seconds since the epoch (UTC)."""
    m = STAMP.match(line)
    if not m:
        return None
    s = m.group(1)
    whole, _, frac = s.partition(".")
    t = time.mktime(time.strptime(whole, "%Y-%m-%dT%H:%M:%S"))
    return t + (float("0." + frac) if frac else 0.0)
