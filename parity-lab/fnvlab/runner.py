"""Scenario execution, scoring, baseline/regression logic.

Any problem in the oracle, the candidate, or the harness itself becomes an
"error" result for that scenario. Nothing here is allowed to crash the run.
"""
from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import time
import traceback
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from .comparators import COMPARATORS, ComparatorError, Failure
from .config import LabConfig, Scenario, substitute

STATUS_RANK = {"pass": 0, "fail": 1, "error": 2}
SCORE_EPS = 1e-9
REGRESSION_EPS = 1e-6  # baseline scores are stored rounded to 6 places


@dataclass
class ScenarioResult:
    id: str
    layer: int
    status: str                 # pass | fail | error
    score: float
    holdout: bool
    tags: list[str]
    duration_s: float
    failures: list[dict] = field(default_factory=list)
    total_failures: int = 0
    stats: dict = field(default_factory=dict)
    error: str | None = None
    description: str = ""

    def to_json(self, reveal: bool) -> dict:
        d = {"id": self.id, "layer": self.layer, "status": self.status,
             "score": round(self.score, 6), "holdout": self.holdout}
        if self.holdout and not reveal:
            d["redacted"] = True
            return d
        d.update({"description": self.description, "tags": self.tags,
                  "duration_s": round(self.duration_s, 3), "stats": self.stats,
                  "total_failures": self.total_failures, "failures": self.failures})
        if self.error:
            d["error"] = self.error
        return d


def failure_id(scenario_id: str, kind: str, path: str) -> str:
    """Stable across runs so the agent and humans can refer to a failure by ID."""
    return hashlib.sha1(f"{scenario_id}\x1f{kind}\x1f{path}".encode()).hexdigest()[:12]


def _load_oracle(s: Scenario, cfg: LabConfig) -> Any:
    if "inline" in s.oracle:
        return s.oracle["inline"]
    gp = Path(substitute(s.oracle["golden"], cfg.vars))
    if not gp.is_absolute():
        gp = cfg.goldens_dir / gp
    if not gp.exists():
        raise FileNotFoundError(f"golden not found: {gp}")
    with gp.open("r", encoding="utf-8") as f:
        return json.load(f)


# Candidate kinds: "tool" (the default: a command whose output is the answer) and "game"
# (a run of the engine, `nvforever --play`, driven by test aids, that writes its answer to
# `output_file`). A game run takes one of the machine's smoke slots
# (tools/smoke/smoke/slots.py), so it waits while the original game is captured and runs
# beside at most the slots' other nvforever games, as the smoke runner's do.
CANDIDATE_KINDS = ("tool", "game")
# Test aids the lab's own environment may hold are not passed on to a game run: only the
# scenario's `env` drives it (as the smoke runner's aid_free_environ).
AID_PREFIXES = ("NVF_PLAY_TEST_", "NVF_PLAY_DEBUG_", "NVF_PLAY_AIDS_")


def smoke_slot_gate(cfg: LabConfig):
    """A context manager holding one smoke slot for a game run, from the smoke runner's
    module beside the lab (`{lab_root}/../tools/smoke`); its lock paths are the smoke
    runner's (NVF_SMOKE_LOCKS, else this machine's two game locks)."""
    smoke_dir = (cfg.root / ".." / "tools" / "smoke").resolve()
    if str(smoke_dir) not in sys.path:
        sys.path.insert(0, str(smoke_dir))
    from smoke import slots  # noqa: PLC0415 - the smoke runner's package, beside the lab
    from smoke.runner import default_lock_paths  # noqa: PLC0415

    class Gate:
        def __enter__(self):
            paths = default_lock_paths(cfg.root.resolve().parent)
            self.pool = None
            if not paths:
                return self
            self.pool = slots.Slots(paths[-1], paths[:-1], want=1,
                                    say=lambda m: print(f"  [game slot] {m}", file=sys.stderr))
            self.slot = self.pool.acquire()
            return self

        def __exit__(self, *exc):
            if self.pool is not None:
                self.pool.release(self.slot)
                self.pool.finish()
            return False
    return Gate()


def _run_game(s: Scenario, cfg: LabConfig, gate) -> Any:
    """A "game" candidate: the engine run under a smoke slot with the scenario's test aids
    (`env`), its answer read from `output_file` (removed first, so a stale answer is never
    read), its log written beside it (`<output_file>.log`)."""
    argv = [substitute(a, cfg.vars) for a in s.candidate["argv"]]
    timeout = float(s.candidate.get("timeout_s", cfg.default_timeout_s))
    out_file = s.candidate.get("output_file")
    if not out_file:
        raise RuntimeError("a game candidate needs output_file")
    op = Path(substitute(out_file, cfg.vars))
    op.parent.mkdir(parents=True, exist_ok=True)
    op.unlink(missing_ok=True)
    log = op.with_name(op.name + ".log")
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith(AID_PREFIXES)}
    env.update({k: substitute(str(v), cfg.vars) for k, v in s.candidate.get("env", {}).items()})
    cwd = s.candidate.get("cwd")
    cwd = substitute(cwd, cfg.vars) if cwd else str(op.parent)
    with gate(cfg), open(log, "w", encoding="utf-8", errors="replace") as f:
        try:
            p = subprocess.Popen(argv, stdout=f, stderr=subprocess.STDOUT, cwd=cwd, env=env)
        except FileNotFoundError as e:
            raise RuntimeError(f"candidate executable not found: {argv[0]} ({e})") from e
        try:
            code = p.wait(timeout=timeout)
        except subprocess.TimeoutExpired as e:
            p.kill()  # this run's own process, by its handle
            p.wait()
            raise RuntimeError(f"game candidate timed out after {timeout}s; log {log}") from e
    if code != 0:
        tail = log.read_text(encoding="utf-8", errors="replace").strip().splitlines()[-20:]
        raise RuntimeError(f"game candidate exited with code {code}; log {log}\n--- log tail ---\n" + "\n".join(tail))
    try:
        with op.open("r", encoding="utf-8") as f:
            return json.load(f)
    except (json.JSONDecodeError, UnicodeDecodeError, OSError) as e:
        raise RuntimeError(f"game candidate wrote no answer ({e}); log {log}") from e


def _run_candidate(s: Scenario, cfg: LabConfig, gate=smoke_slot_gate) -> Any:
    if s.candidate.get("kind", "tool") == "game":
        return _run_game(s, cfg, gate)
    argv = [substitute(a, cfg.vars) for a in s.candidate["argv"]]
    timeout = float(s.candidate.get("timeout_s", cfg.default_timeout_s))
    cwd = s.candidate.get("cwd")
    cwd = substitute(cwd, cfg.vars) if cwd else str(cfg.root)
    try:
        proc = subprocess.run(argv, capture_output=True, timeout=timeout, cwd=cwd)
    except FileNotFoundError as e:
        raise RuntimeError(f"candidate executable not found: {argv[0]} ({e})") from e
    except subprocess.TimeoutExpired as e:
        raise RuntimeError(f"candidate timed out after {timeout}s") from e
    if proc.returncode != 0:
        tail = proc.stderr.decode("utf-8", "replace").strip()[-2000:]
        last = tail.splitlines()[-1] if tail else "(no stderr)"
        raise RuntimeError(f"candidate exited with code {proc.returncode}: {last}\n--- stderr tail ---\n{tail}")
    out_file = s.candidate.get("output_file")
    try:
        if out_file:
            op = Path(substitute(out_file, cfg.vars))
            with op.open("r", encoding="utf-8") as f:
                return json.load(f)
        return json.loads(proc.stdout.decode("utf-8"))
    except (json.JSONDecodeError, UnicodeDecodeError, OSError) as e:
        head = proc.stdout[:300].decode("utf-8", "replace")
        raise RuntimeError(f"candidate output is not valid JSON: {e}; stdout head: {head!r}") from e


def run_scenario(s: Scenario, cfg: LabConfig) -> ScenarioResult:
    t0 = time.monotonic()

    def err(msg: str) -> ScenarioResult:
        return ScenarioResult(s.id, s.layer, "error", 0.0, s.holdout, s.tags,
                              time.monotonic() - t0, error=msg, description=s.description)
    try:
        expected = _load_oracle(s, cfg)
    except Exception as e:  # noqa: BLE001 - every failure must become a result
        return err(f"oracle: {type(e).__name__}: {e}")
    try:
        actual = _run_candidate(s, cfg)
    except Exception as e:  # noqa: BLE001
        return err(f"candidate: {e}")
    try:
        cmp = COMPARATORS[s.comparator["kind"]]
        res = cmp(expected, actual, {**s.comparator, "_goldens_dir": str(cfg.goldens_dir)})
    except ComparatorError as e:
        return err(f"comparator: {e}")
    except Exception as e:  # noqa: BLE001
        return err(f"harness: {type(e).__name__}: {e}\n{traceback.format_exc(limit=5)}")
    fails = []
    for f in res.failures:
        fj = f.to_json()
        fj["id"] = failure_id(s.id, f.kind, f.path)
        fails.append(fj)
    status = "pass" if res.total_failures == 0 and res.score >= 1.0 - SCORE_EPS else "fail"
    return ScenarioResult(s.id, s.layer, status, max(0.0, min(1.0, res.score)), s.holdout, s.tags,
                          time.monotonic() - t0, fails, res.total_failures, res.stats,
                          description=s.description)


def run_all(scenarios: list[Scenario], cfg: LabConfig, progress=None) -> list[ScenarioResult]:
    out = []
    for s in scenarios:
        if progress:
            progress(s)
        out.append(run_scenario(s, cfg))
    return out


# --------------------------------------------------------------------------- baseline

def load_baseline(cfg: LabConfig) -> dict | None:
    if not cfg.baseline_path.exists():
        return None
    try:
        return json.loads(cfg.baseline_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return None


def find_regressions(results: list[ScenarioResult], baseline: dict | None) -> list[dict]:
    if not baseline:
        return []
    base = baseline.get("scenarios", {})
    regs = []
    for r in results:
        b = base.get(r.id)
        if b is None:
            continue
        worse_status = STATUS_RANK[r.status] > STATUS_RANK.get(b["status"], 2)
        worse_score = round(r.score, 6) < float(b["score"]) - REGRESSION_EPS
        if worse_status or worse_score:
            regs.append({"id": r.id, "layer": r.layer, "holdout": r.holdout,
                         "baseline_status": b["status"], "baseline_score": b["score"],
                         "status": r.status, "score": round(r.score, 6)})
    return regs


def make_baseline(latest: dict) -> dict:
    return {"created_from_run": latest.get("started_at"),
            "scenarios": {s["id"]: {"status": s["status"], "score": s["score"]}
                          for s in latest.get("results", [])}}
