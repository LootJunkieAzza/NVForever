"""The scenario file (tools/smoke/scenarios.toml): smoke tests as data.

Top-level tables:

- `[timed.<ENV VAR>]` `sep`, `shift`: a test aid whose value is a list of timed items
  (`<seconds>:<what>`). Lists of scenarios sharing a launch are joined with `sep`; a
  member's `offset` moves its items by that many seconds (`shift`: "lead" the first
  field of each item, "all" every field, "value" the whole value is one time).
- `[[check]]` the "checks before every push": `name`, `paths` (globs), `cmd`, `cwd`.
- `[[scenario]]` one smoke test (see `Scenario`).
"""
from __future__ import annotations

import tomllib
from dataclasses import dataclass, field
from pathlib import Path

from .expect import Expect, ScenarioError

GAME, TOOL = "game", "tool"
SCENARIO_KEYS = {
    "name", "kind", "source", "areas", "paths", "args", "run_seconds", "env", "launch", "offset",
    "expect", "order", "forbid", "manual", "requires", "commands", "exit", "note", "timeout", "ends",
}


@dataclass
class Timed:
    sep: str = ","
    shift: str = "lead"  # lead | all | value


@dataclass
class Check:
    name: str
    paths: list[str]
    cmd: list[str]
    cwd: str = "."


@dataclass
class Scenario:
    name: str
    kind: str = GAME
    source: str = ""  # where the check came from (CLAUDE.md paragraph, doc section)
    areas: list[str] = field(default_factory=list)
    paths: list[str] = field(default_factory=list)  # globs over changed files that call for it
    args: list[str] = field(default_factory=list)  # nvforever arguments, without --data/--run-seconds
    run_seconds: float = 0.0
    env: dict[str, str] = field(default_factory=dict)
    launch: str = ""  # scenarios with the same launch name share one game run
    offset: float = 0.0  # seconds added to this scenario's timed test aids in a shared run
    expect: list[Expect] = field(default_factory=list)
    order: list[list[Expect]] = field(default_factory=list)
    forbid: list[Expect] = field(default_factory=list)
    manual: str = ""  # what a person must still compare by hand (printed, not checked)
    requires: list[str] = field(default_factory=list)  # env vars needed (else skipped)
    commands: list[list[str]] = field(default_factory=list)  # tool scenarios: fnvtool arguments
    exit: int = 0
    note: str = ""
    timeout: float = 0.0  # seconds; 0 = derived from run_seconds
    ends: str = "run finished"  # game scenarios: the line the run ends with (always expected)

    def all_expectations(self):
        yield from self.expect
        for seq in self.order:
            yield from seq
        yield from self.forbid


@dataclass
class Suite:
    scenarios: list[Scenario]
    checks: list[Check]
    timed: dict[str, Timed]
    untimed: set[str] = field(default_factory=set)  # test aids known to hold no times
    masters: list[str] = field(default_factory=list)  # `{masters}` in tool commands

    def by_name(self) -> dict[str, Scenario]:
        return {s.name: s for s in self.scenarios}


def _expects(items, where) -> list[Expect]:
    if not isinstance(items, list):
        raise ScenarioError(f"{where}: expected a list")
    return [Expect.parse(x, f"{where}[{i}]") for i, x in enumerate(items)]


def parse_scenario(d: dict) -> Scenario:
    name = d.get("name")
    if not name or not isinstance(name, str):
        raise ScenarioError(f"scenario without a name: {d!r:.80}")
    extra = set(d) - SCENARIO_KEYS
    if extra:
        raise ScenarioError(f"{name}: unknown keys {sorted(extra)}")
    s = Scenario(name=name)
    s.kind = d.get("kind", GAME)
    if s.kind not in (GAME, TOOL):
        raise ScenarioError(f"{name}: kind must be game or tool")
    for key in ("source", "launch", "manual", "note"):
        setattr(s, key, d.get(key, ""))
    s.areas = list(d.get("areas", []))
    s.paths = list(d.get("paths", []))
    s.args = [str(a) for a in d.get("args", [])]
    s.run_seconds = float(d.get("run_seconds", 0))
    s.env = {k: str(v) for k, v in d.get("env", {}).items()}
    s.ends = str(d.get("ends", s.ends))
    s.offset = float(d.get("offset", 0))
    s.requires = list(d.get("requires", []))
    s.commands = [[str(a) for a in c] for c in d.get("commands", [])]
    s.exit = int(d.get("exit", 0))
    s.timeout = float(d.get("timeout", 0))
    s.expect = _expects(d.get("expect", []), f"{name}.expect")
    s.order = [_expects(seq, f"{name}.order[{i}]") for i, seq in enumerate(d.get("order", []))]
    s.forbid = _expects(d.get("forbid", []), f"{name}.forbid")
    for e in [x for seq in s.order for x in seq] + s.forbid:
        if e.pattern is None:
            raise ScenarioError(f"{name}: order and forbid take text or re expectations, not {e.label!r}")
    if s.kind == GAME:
        if s.run_seconds <= 0:
            raise ScenarioError(f"{name}: a game scenario needs run_seconds")
        if any(a.startswith("--run-seconds") or a == "--data" for a in s.args):
            raise ScenarioError(f"{name}: give run_seconds as its own key and leave --data to the runner")
        # Every unattended run ends with this line (nvforever main.rs), or with the one the
        # scenario names for a run that ends another way (`ends`); always expected.
        s.expect.insert(0, Expect.parse(s.ends))
    elif not s.commands:
        raise ScenarioError(f"{name}: a tool scenario needs commands")
    if not s.paths and not s.areas:
        raise ScenarioError(f"{name}: needs paths or areas so it can be selected")
    return s


def load_text(text: str) -> Suite:
    data = tomllib.loads(text)
    try:
        timed = {k: Timed(**v) for k, v in data.get("timed", {}).items()}
        checks = [Check(**c) for c in data.get("check", [])]
    except TypeError as e:
        raise ScenarioError(f"timed or check table: {e}") from e
    extra = set(data) - {"timed", "check", "scenario", "untimed", "masters"}
    if extra:
        raise ScenarioError(f"unknown top-level keys {sorted(extra)}")
    for k, t in timed.items():
        if t.shift not in ("lead", "all", "value"):
            raise ScenarioError(f"timed.{k}: shift must be lead, all or value")
    scenarios = [parse_scenario(d) for d in data.get("scenario", [])]
    names = [s.name for s in scenarios]
    dup = {n for n in names if names.count(n) > 1}
    if dup:
        raise ScenarioError(f"duplicate scenario names: {sorted(dup)}")
    suite = Suite(scenarios, checks, timed, set(data.get("untimed", [])), list(data.get("masters", [])))
    from .plan import validate_launches  # late import: plan uses this module's types

    validate_launches(suite)
    return suite


def load(path: Path) -> Suite:
    return load_text(Path(path).read_text(encoding="utf-8"))
