"""Running launches: the game locks, the processes, the logs and the verdicts."""
from __future__ import annotations

import glob
import json
import os
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

from .expect import Outcome, check_forbid, check_one, check_order, clean
from .plan import Launch, fmt
from .scenarios import GAME, Scenario
from .stacks import dump as dump_stacks

AID_PREFIXES = ("NVF_PLAY_TEST_", "NVF_PLAY_DEBUG_", "NVF_PLAY_AIDS_")
# The world test aids (the console's lines, the script steps, keys, talks, screenshots…)
# count from when the scripts first run, so a busy machine's late start does not run them
# before the world can take them (engine/nvf-play/src/pause.rs WorldAidClock). Set for
# every `--play` launch; a scenario can set it to "0" for the old clock.
FROM_SCRIPTS = "NVF_PLAY_AIDS_FROM_SCRIPTS"
OWNER = "smoke-owner.txt"


# ---------------------------------------------------------------- locks

def pid_alive(pid: int) -> bool:
    if pid <= 0:
        return False
    if os.name == "nt":
        import ctypes

        k32 = ctypes.windll.kernel32
        h = k32.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
        if not h:
            return k32.GetLastError() == 5  # access denied: it exists
        code = ctypes.c_ulong()
        ok = k32.GetExitCodeProcess(h, ctypes.byref(code))
        k32.CloseHandle(h)
        return bool(ok) and code.value == 259  # STILL_ACTIVE
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


GAME_PROCESSES = ("nvforever", "falloutnv")


def game_running() -> bool:
    """Whether an nvforever or FalloutNV process runs on this machine."""
    cmd = ["tasklist", "/FO", "CSV", "/NH"] if os.name == "nt" else ["ps", "-A", "-o", "comm="]
    try:
        out = subprocess.run(cmd, capture_output=True, text=True, timeout=30).stdout.lower()
    except (OSError, subprocess.SubprocessError):
        return True  # cannot tell: assume the lock is in use
    return any(name in out for name in GAME_PROCESSES)


class Locks:
    """The game-window locks every agent takes by `mkdir` (CLAUDE.md, the agents' brief).
    Taken all or none, in order; released only if this runner made them. A lock left by
    a smoke runner that died (its owner file names a process that is gone) is cleared."""

    def __init__(self, paths: list[Path], poll: float = 30.0, wait_max: float = 4 * 3600.0, say=print,
                 stale_after: float = 900.0, running=game_running):
        self.paths = [Path(p) for p in paths]
        self.poll, self.wait_max, self.say = poll, wait_max, say
        self.stale_after, self.running = stale_after, running
        self.held: list[Path] = []
        self.warned: set[tuple[Path, float]] = set()

    def warn_if_abandoned(self, p: Path) -> None:
        """Another session's plain mkdir lock that is old while no game runs: say so loudly
        (once per lock). It is never removed: only its maker may do that."""
        try:
            made = p.stat().st_ctime
        except OSError:
            return
        if (p / OWNER).exists() or (p, made) in self.warned or time.time() - made < self.stale_after:
            return
        if self.running():
            return
        self.warned.add((p, made))
        mins = (time.time() - made) / 60
        self.say(f"!!! LOCK LOOKS ABANDONED: {p} was made {mins:.0f} min ago "
                 f"({time.strftime('%H:%M', time.localtime(made))}) and no nvforever or FalloutNV runs. "
                 f"Not removed: ask the session that made it, or remove it if that session is gone.")

    def _stale(self, p: Path) -> bool:
        f = p / OWNER
        try:
            pid = int(f.read_text().split()[0])
        except (OSError, ValueError, IndexError):
            return False
        return not pid_alive(pid)

    @staticmethod
    def holder(p: Path) -> str:
        """Who holds a lock, as far as the folder tells (no lock is taken by this)."""
        try:
            return "held by " + (p / OWNER).read_text().strip()
        except OSError:
            pass
        try:
            made = time.strftime("%H:%M:%S", time.localtime(p.stat().st_ctime))
        except OSError:
            return "just released"
        return f"another session's mkdir lock, made at {made}"

    def try_take(self) -> Path | None:
        """Take every lock or none; the lock that is held by someone else, if any."""
        for p in self.paths:
            p.parent.mkdir(parents=True, exist_ok=True)
            try:
                p.mkdir()
            except FileExistsError:
                if self._stale(p):
                    self.say(f"lock {p}: left by a smoke runner that is gone; cleared")
                    shutil.rmtree(p, ignore_errors=True)
                    try:
                        p.mkdir()
                    except FileExistsError:
                        self.release()
                        return p
                else:
                    self.release()
                    return p
            (p / OWNER).write_text(f"{os.getpid()} smoke runner {time.strftime('%Y-%m-%d %H:%M:%S')}\n")
            self.held.append(p)
        return None

    def acquire(self) -> None:
        start, told = time.monotonic(), None
        while True:
            busy = self.try_take()
            if busy is None:
                return
            if busy != told:
                self.say(f"waiting for {busy}: {self.holder(busy)}")
                told = busy
            self.warn_if_abandoned(busy)
            if time.monotonic() - start > self.wait_max:
                raise TimeoutError(f"lock {busy} still held after {self.wait_max:.0f} s")
            time.sleep(self.poll)

    def release(self) -> None:
        for p in reversed(self.held):
            shutil.rmtree(p, ignore_errors=True)
        self.held.clear()

    def __enter__(self):
        self.acquire()
        return self

    def __exit__(self, *exc):
        self.release()


def default_lock_paths(repo: Path) -> list[Path]:
    """NVF_SMOKE_LOCKS (paths separated by os.pathsep) or this machine's two locks."""
    env = os.environ.get("NVF_SMOKE_LOCKS")
    if env is not None:
        return [Path(p) for p in env.split(os.pathsep) if p.strip()]
    out = []
    cap = Path(r"C:\fnv-private\capture")
    if cap.is_dir():
        out.append(cap / "run.lock")
    out.append(main_checkout(repo) / ".claude" / "smoke.lock")
    return out


def main_checkout(repo: Path) -> Path:
    """The main checkout of a worktree (where the shared smoke lock lives)."""
    try:
        r = subprocess.run(["git", "-C", str(repo), "rev-parse", "--path-format=absolute", "--git-common-dir"],
                           capture_output=True, text=True, check=True)
        return Path(r.stdout.strip()).parent
    except (OSError, subprocess.CalledProcessError):
        return repo


# ---------------------------------------------------------------- verdicts

@dataclass
class Verdict:
    scenario: str
    status: str  # PASS FAIL KNOWN SKIP
    failures: list[Outcome] = field(default_factory=list)
    xpass: list[str] = field(default_factory=list)  # known-failing marks that now pass
    manual: str = ""
    reason: str = ""  # for SKIP / launch errors


def judge(s: Scenario, lines: list[str], exit_code: int | None, launch_error: str = "") -> Verdict:
    outs: list[Outcome] = []
    if launch_error:
        outs.append(Outcome(False, None, "launch", launch_error))
    elif exit_code != s.exit:
        outs.append(Outcome(False, None, "exit", f"exit code {exit_code}, want {s.exit}",
                            context=[ln for ln in lines if "error" in ln.lower() or "panicked" in ln][-3:]))
    outs += [check_one(e, lines) for e in s.expect]
    for seq in s.order:
        outs += check_order(seq, lines)
    outs += [check_forbid(e, lines) for e in s.forbid]
    hard = [o for o in outs if not o.ok and not o.known]
    known = [o for o in outs if not o.ok and o.known]
    xpass = [o.label for o in outs if o.ok and o.known]
    status = "FAIL" if hard else ("KNOWN" if known else "PASS")
    return Verdict(s.name, status, hard + known, xpass, s.manual)


def read_lines(path: Path) -> list[str]:
    with open(path, encoding="utf-8", errors="replace") as f:
        return [clean(ln) for ln in f]


# ---------------------------------------------------------------- running

@dataclass
class Env:
    repo: Path
    out: Path
    exe: Path | None
    data: str | None
    fnvtool: Path | None
    say: object = print
    masters: list[str] = field(default_factory=list)


def aid_free_environ(extra: dict[str, str]) -> dict[str, str]:
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith(AID_PREFIXES)}
    env.update(extra)
    return env


def game_env(launch: Launch) -> dict[str, str]:
    """The test aids a game launch runs with: its own, and FROM_SCRIPTS on for `--play`."""
    env = dict(launch.env)
    if "--play" in launch.args:
        env.setdefault(FROM_SCRIPTS, "1")
    return env


def launch_timeout(launch: Launch) -> float:
    explicit = max((s.timeout for s in launch.scenarios), default=0)
    return explicit or launch.run_seconds * 3 + 240


def expand(arg: str, ctx: dict[str, str], masters: list[str] = ()) -> list[str] | None:
    """Placeholders in tool arguments: {data}, {repo}, {env:NAME}, and `{masters}` (the
    scenario file's list of masters, one argument each); a result with `*` is a glob
    (one argument per file). None when an env placeholder is unset."""
    if arg == "{masters}":
        return list(masters)
    out = arg
    while "{env:" in out:
        i = out.index("{env:")
        j = out.index("}", i)
        val = os.environ.get(out[i + 5:j])
        if val is None:
            return None
        out = out[:i] + val + out[j + 1:]
    for k, v in ctx.items():
        out = out.replace("{" + k + "}", v)
    if "*" in out:
        return sorted(glob.glob(out)) or [out]
    return [out]


def run_launch(launch: Launch, env: Env, locks: Locks | None) -> tuple[int | None, str, float]:
    """Run one launch; its log goes to out/<name>.log. (exit code, error, seconds)."""
    log = env.out / f"{launch.name}.log"
    started = time.monotonic()
    if launch.kind == GAME:
        cmd = [str(env.exe), "--data", str(env.data), *launch.args, "--run-seconds", fmt(launch.run_seconds)]
        aids = game_env(launch)
        (env.out / f"{launch.name}.cmd.txt").write_text(
            "".join(f"{k}={v}\n" for k, v in sorted(aids.items())) + subprocess.list2cmdline(cmd) + "\n")
        (env.out / f"{launch.name}.stacks.txt").unlink(missing_ok=True)
        if locks:
            locks.acquire()
        started = time.monotonic()  # the launch's own time, without waiting for the locks
        try:
            with open(log, "w", encoding="utf-8", errors="replace") as f:
                p = subprocess.Popen(cmd, stdout=f, stderr=subprocess.STDOUT, cwd=env.out,
                                     env=aid_free_environ(aids))
                try:
                    code = p.wait(timeout=launch_timeout(launch))
                    err = ""
                except subprocess.TimeoutExpired:
                    # Where it stopped, before it goes: every thread's stack.
                    stacks = env.out / f"{launch.name}.stacks.txt"
                    stacks.write_text(dump_stacks(p.pid, Path(env.exe).parent), encoding="utf-8")
                    p.kill()
                    p.wait()
                    code, err = None, f"timed out after {launch_timeout(launch):.0f} s; thread stacks in {stacks}"
        finally:
            if locks:
                locks.release()
    else:
        s = launch.scenarios[0]
        ctx = {"data": str(env.data), "repo": str(env.repo)}
        code, err = 0, ""
        cmds = []
        with open(log, "w", encoding="utf-8", errors="replace") as f:
            for c in s.commands:
                args = []
                for a in c:
                    x = expand(a, ctx, env.masters)
                    if x is None:
                        return None, f"needs {a}", 0.0
                    args += x
                cmd = [str(env.fnvtool), *args]
                cmds.append(subprocess.list2cmdline(cmd))
                f.flush()
                try:
                    r = subprocess.run(cmd, stdout=f, stderr=subprocess.STDOUT, cwd=env.repo,
                                       timeout=launch_timeout(launch))
                except subprocess.TimeoutExpired:
                    code, err = None, "timed out"
                    break
                if r.returncode != 0:
                    code = r.returncode
                    break
        (env.out / f"{launch.name}.cmd.txt").write_text("\n".join(cmds) + "\n")
    secs = time.monotonic() - started
    (env.out / f"{launch.name}.exit").write_text(json.dumps({"exit": code, "error": err, "seconds": round(secs, 1)}))
    return code, err, secs


def missing_requirements(s: Scenario) -> list[str]:
    return [r for r in s.requires if not os.environ.get(r)]


def report(v: Verdict, launch: Launch, secs: float | None, log: Path, say=print, context: int = 3) -> None:
    where = f" [{launch.name}]" if launch.shared else ""
    t = f" {secs:.0f} s" if secs is not None else ""
    if v.status == "SKIP":
        say(f"SKIP  {v.scenario}{where}: {v.reason}")
        return
    say(f"{v.status:<5} {v.scenario}{where}{t}" + (f"  log {log}" if v.status == "FAIL" else ""))
    for o in v.failures:
        tag = "known" if o.known else o.what
        say(f"      {tag}: {o.label!r}" + (f" ({o.detail})" if o.detail else "")
            + (f" -- {o.known}" if o.known else ""))
        if not o.known:
            for c in o.context[:context]:
                say(f"        > {c}")
    for x in v.xpass:
        say(f"      now passes, drop its known_failing: {x!r}")
    if v.manual and v.status != "PASS":
        say(f"      by hand: {v.manual}")


def exe_is_stale(exe: Path, repo: Path, files: list[str]) -> list[str]:
    """Changed engine/crate sources newer than the exe."""
    try:
        t = exe.stat().st_mtime
    except OSError:
        return []
    out = []
    for f in files:
        if f.startswith(("engine/", "crates/")) and f.endswith((".rs", ".wgsl", ".toml")):
            try:
                if (repo / f).stat().st_mtime > t:
                    out.append(f)
            except OSError:
                pass
    return out


def python() -> str:
    return sys.executable or "python"
