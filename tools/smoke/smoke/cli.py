"""python tools/smoke <command> — the smoke tests of CLAUDE.md as data.

  run    [selection] [--checks] [--no-merge] [--exe X] [--data D] [--fnvtool T]
  plan   [selection]            what would run, launch by launch (nothing is started)
  check  [selection]            judge the logs of the last run again (no launch)
  list                          every scenario with its areas

Selection (default --changed origin/main): --changed [BASE] | --all | --only NAME… |
--area AREA…. Logs go to <repo>/target/smoke/ (or --out / NVF_SMOKE_OUT); only a
compact summary is printed. Exit code 1 when a scenario or check fails.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

from . import choose
from .expect import ScenarioError
from .plan import Launch, plan
from .runner import (Env, Locks, Verdict, default_lock_paths, exe_is_stale, judge, missing_requirements,
                     read_lines, report, run_launch)
from .scenarios import GAME, Suite, load

HERE = Path(__file__).resolve().parent.parent  # tools/smoke
REPO = HERE.parent.parent
SCENARIOS = HERE / "scenarios.toml"
EXE_NAME = "nvforever.exe" if os.name == "nt" else "nvforever"
TOOL_NAME = "fnvtool.exe" if os.name == "nt" else "fnvtool"


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="python tools/smoke", description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    for name in ("run", "plan", "check", "list"):
        q = sub.add_parser(name)
        q.add_argument("--scenarios", type=Path, default=SCENARIOS)
        q.add_argument("--repo", type=Path, default=REPO)
        q.add_argument("--out", type=Path, default=None, help="logs (default <repo>/target/smoke or NVF_SMOKE_OUT)")
        if name == "list":
            continue
        g = q.add_mutually_exclusive_group()
        g.add_argument("--changed", nargs="?", const="origin/main", metavar="BASE",
                       help="scenarios whose paths match files changed against BASE (default)")
        g.add_argument("--all", action="store_true")
        g.add_argument("--only", nargs="+", metavar="NAME")
        g.add_argument("--area", nargs="+", metavar="AREA")
        q.add_argument("--no-merge", action="store_true", help="one launch per scenario")
        q.add_argument("--games-only", action="store_true", help="leave out fnvtool scenarios")
        q.add_argument("--tools-only", action="store_true", help="only fnvtool scenarios")
        if name == "run":
            q.add_argument("--checks", action="store_true", help="also run the push checks the change calls for")
            q.add_argument("--exe", type=Path, default=None, help="nvforever (default NVF_EXE or <repo>/engine/target/release)")
            q.add_argument("--data", default=None, help="game Data folder (default NVF_DATA or FNVLAB_DATA_DIR)")
            q.add_argument("--fnvtool", type=Path, default=None, help="fnvtool (default NVF_FNVTOOL or <repo>/target/release)")
            q.add_argument("--lock-poll", type=float, default=30.0)
            q.add_argument("--no-locks", action="store_true", help="for machines where nobody else runs the game")
            q.add_argument("--context", type=int, default=3, help="log lines shown per failed expectation")
    return p


def out_dir(a) -> Path:
    d = a.out or (Path(os.environ["NVF_SMOKE_OUT"]) if os.environ.get("NVF_SMOKE_OUT") else a.repo / "target" / "smoke")
    d.mkdir(parents=True, exist_ok=True)
    return d


def selection(a, suite: Suite, say) -> tuple[list, list[str] | None]:
    """The chosen scenarios and, for --changed, the changed files."""
    scen = suite.scenarios
    if a.games_only:
        scen = [s for s in scen if s.kind == GAME]
    if a.tools_only:
        scen = [s for s in scen if s.kind != GAME]
    if a.all:
        return scen, None
    if a.only:
        names = {s.name: s for s in scen}
        unknown = [n for n in a.only if n not in names]
        if unknown:
            raise SystemExit(f"unknown scenarios: {unknown} (python tools/smoke list)")
        return [names[n] for n in a.only], None
    if a.area:
        return choose.by_area(scen, a.area), None
    base = a.changed or "origin/main"
    files = choose.changed_files(a.repo, base)
    hits = choose.by_changes(scen, files)
    say(f"changed against {base}: {len(files)} files; {len(hits)} scenarios")
    for s, why in hits:
        more = f" (+{len(why) - 1})" if len(why) > 1 else ""
        say(f"  {s.name}: {why[0]}{more}")
    return [s for s, _ in hits], files


def print_checks(suite: Suite, files: list[str] | None, say) -> list:
    if files is None:
        return []
    cs = choose.checks_for(suite.checks, files)
    say("push checks for this change: " + (", ".join(c.name for c in cs) if cs else "none"))
    return cs


def cmd_list(a, suite: Suite, say) -> int:
    for s in suite.scenarios:
        share = f" [launch {s.launch}]" if s.launch else ""
        say(f"{s.name:<28} {s.kind:<4} {','.join(s.areas)}{share}")
    launches = plan(suite, suite.scenarios)
    games = [x for x in launches if x.kind == GAME]
    alone = sum(1 for s in suite.scenarios if s.kind == GAME)
    say(f"{len(suite.scenarios)} scenarios; game launches: {alone} one by one, {len(games)} with shared runs")
    return 0


def cmd_plan(a, suite: Suite, say) -> int:
    chosen, files = selection(a, suite, say)
    print_checks(suite, files, say)
    launches = plan(suite, chosen, a.no_merge)
    total = 0.0
    for x in launches:
        if x.kind == GAME:
            total += x.run_seconds
            env = " ".join(f"{k.replace('NVF_PLAY_TEST_', '')}={v}" for k, v in sorted(x.env.items()))
            say(f"{x.name}: {' '.join(s.name for s in x.scenarios)} | {' '.join(x.args)} --run-seconds {x.run_seconds:g}"
                + (f" | {env}" if env else ""))
            for n in x.notes:
                say(f"    note: {n}")
        else:
            say(f"{x.name}: fnvtool x{len(x.scenarios[0].commands)}")
    g = sum(1 for x in launches if x.kind == GAME)
    say(f"{len(chosen)} scenarios, {g} game launches ({total / 60:.1f} min of game time), {len(launches) - g} tool runs")
    return 0


def resolve(a, suite: Suite, say) -> Env:
    exe = a.exe or (Path(os.environ["NVF_EXE"]) if os.environ.get("NVF_EXE") else
                    a.repo / "engine" / "target" / "release" / EXE_NAME)
    tool = a.fnvtool or (Path(os.environ["NVF_FNVTOOL"]) if os.environ.get("NVF_FNVTOOL") else
                         a.repo / "target" / "release" / TOOL_NAME)
    data = a.data or os.environ.get("NVF_DATA") or os.environ.get("FNVLAB_DATA_DIR")
    return Env(a.repo, out_dir(a), exe, data, tool, say, suite.masters)


def run_checks(checks, env: Env, say) -> bool:
    ok = True
    (env.out / "checks").mkdir(exist_ok=True)
    for c in checks:
        log = env.out / "checks" / (c.name.replace(" ", "_").replace("/", "_") + ".log")
        t = time.monotonic()
        with open(log, "w", encoding="utf-8", errors="replace") as f:
            r = subprocess.run(c.cmd, cwd=env.repo / c.cwd, stdout=f, stderr=subprocess.STDOUT)
        secs = time.monotonic() - t
        if r.returncode == 0:
            say(f"PASS  check {c.name} {secs:.0f} s")
        else:
            ok = False
            say(f"FAIL  check {c.name} {secs:.0f} s (exit {r.returncode})  log {log}")
            tail = read_lines(log)
            keep = [ln for ln in tail if ln.startswith(("error", "warning", "FAIL", "test ", "---- ", "thread '"))
                    or "panicked" in ln][:12] or tail[-8:]
            for ln in keep:
                say(f"        > {ln[:220]}")
    return ok


def judge_launch(x: Launch, env: Env, code, err, secs, a, say, verdicts: list[Verdict]) -> None:
    log = env.out / f"{x.name}.log"
    lines = read_lines(log) if log.exists() else []
    for s in x.scenarios:
        v = judge(s, lines, code, err)
        verdicts.append(v)
        report(v, x, secs, log, say, getattr(a, "context", 3))


def cmd_run(a, suite: Suite, say) -> int:
    chosen, files = selection(a, suite, say)
    checks = print_checks(suite, files, say)
    env = resolve(a, suite, say)
    launches = plan(suite, chosen, a.no_merge)
    games = [x for x in launches if x.kind == GAME]
    if games:
        if not env.data:
            raise SystemExit("no game data: pass --data or set NVF_DATA")
        if not env.exe.exists():
            raise SystemExit(f"no nvforever at {env.exe}: build it (engine workspace) or pass --exe / NVF_EXE")
        stale = exe_is_stale(env.exe, a.repo, files or [])
        if stale:
            say(f"warning: {env.exe.name} is older than {len(stale)} changed sources (e.g. {stale[0]}); rebuild first")
    if any(x.kind != GAME for x in launches) and not env.fnvtool.exists():
        say(f"warning: no fnvtool at {env.fnvtool}; tool scenarios will fail (build it or pass --fnvtool)")
    say(f"{len(chosen)} scenarios in {len(games)} game launches and {len(launches) - len(games)} tool runs; logs in {env.out}")
    locks = None if a.no_locks else Locks(default_lock_paths(a.repo), poll=a.lock_poll, say=say)
    verdicts: list[Verdict] = []
    started = time.monotonic()
    try:
        for x in launches:
            missing = sorted({r for s in x.scenarios for r in missing_requirements(s)})
            if missing:
                for s in x.scenarios:
                    v = Verdict(s.name, "SKIP", reason=f"set {', '.join(missing)}")
                    verdicts.append(v)
                    report(v, x, None, env.out, say)
                continue
            code, err, secs = run_launch(x, env, locks)
            judge_launch(x, env, code, err, secs, a, say, verdicts)
    finally:
        if locks:
            locks.release()
    checks_ok = run_checks(checks, env, say) if (a.checks and checks) else True
    return finish(verdicts, env, time.monotonic() - started, checks_ok, say)


def cmd_check(a, suite: Suite, say) -> int:
    """Judge saved logs again: each scenario's own log or its shared launch's, the newer."""
    chosen, _ = selection(a, suite, say)
    env = Env(a.repo, out_dir(a), None, None, None, say)
    verdicts: list[Verdict] = []
    for s in chosen:
        names = [s.name] + ([s.launch] if s.launch else [])
        logs = [env.out / f"{n}.log" for n in names if (env.out / f"{n}.log").exists()]
        x = Launch(s.name, s.kind, [s])
        if not logs:
            v = Verdict(s.name, "SKIP", reason=f"no log {s.name}.log")
            verdicts.append(v)
            report(v, x, None, env.out, say)
            continue
        log = max(logs, key=lambda p: p.stat().st_mtime)
        x.name = log.stem
        x.scenarios = [s, s] if log.stem != s.name else [s]  # marks a shared log in the report
        meta = {}
        try:
            meta = json.loads((env.out / f"{log.stem}.exit").read_text())
        except (OSError, ValueError):
            pass
        lines = read_lines(log)
        v = judge(s, lines, meta.get("exit", 0), meta.get("error", ""))
        verdicts.append(v)
        report(v, x, meta.get("seconds"), log, say, getattr(a, "context", 3))
    return finish(verdicts, env, None, True, say)


def finish(verdicts: list[Verdict], env: Env, secs, checks_ok: bool, say) -> int:
    n = {k: sum(1 for v in verdicts if v.status == k) for k in ("PASS", "FAIL", "KNOWN", "SKIP")}
    t = f" in {secs / 60:.1f} min" if secs is not None else ""
    say(f"smoke: {n['PASS']} passed, {n['FAIL']} failed, {n['KNOWN']} known failing, {n['SKIP']} skipped{t}")
    (env.out / "summary.json").write_text(json.dumps(
        [{"scenario": v.scenario, "status": v.status,
          "failures": [{"what": o.what, "label": o.label, "detail": o.detail, "known": o.known} for o in v.failures]}
         for v in verdicts], indent=1))
    return 1 if n["FAIL"] or not checks_ok else 0


def main(argv=None) -> int:
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")
        except (AttributeError, ValueError):
            pass
    a = parser().parse_args(argv)
    say = lambda s: print(s, flush=True)  # noqa: E731
    try:
        suite = load(a.scenarios)
    except (ScenarioError, OSError) as e:
        print(f"scenario file: {e}", file=sys.stderr)
        return 2
    a.repo = a.repo.resolve()
    return {"run": cmd_run, "plan": cmd_plan, "check": cmd_check, "list": cmd_list}[a.cmd](a, suite, say)
