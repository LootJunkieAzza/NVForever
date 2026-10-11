"""Launch planning: scenarios that name the same `launch` share one game run.

A shared run is safe when the members start the same way (identical nvforever
arguments) and their test aids do not fight: a timed aid (`[timed]` in the scenario
file) gets the members' items joined, moved by each member's `offset`; any other aid
must have the same value in every member that sets it (a member that does not set it
runs with it too, which the plan lists as a note). Members whose run is shared keep
their own expectations, all checked against the shared log. Which scenarios share a
launch is chosen by hand in the scenario file and was checked by running them both
ways (docs in the file); `--no-merge` runs every scenario on its own.
"""
from __future__ import annotations

from dataclasses import dataclass, field

from .expect import ScenarioError
from .scenarios import GAME, Scenario, Suite, Timed


@dataclass
class Launch:
    name: str
    kind: str
    scenarios: list[Scenario]
    args: list[str] = field(default_factory=list)
    env: dict[str, str] = field(default_factory=dict)
    run_seconds: float = 0.0
    notes: list[str] = field(default_factory=list)

    @property
    def shared(self) -> bool:
        return len(self.scenarios) > 1


def fmt(v: float) -> str:
    return f"{v:g}"


def shift(value: str, t: Timed, offset: float) -> str:
    """Move every time in a timed test-aid value by `offset` seconds."""
    if not offset:
        return value
    if t.shift == "value":
        return fmt(float(value) + offset)
    items = []
    for item in value.split(t.sep):
        if not item.strip():
            continue
        parts = item.split(":")
        if t.shift == "lead":
            parts[0] = fmt(float(parts[0]) + offset)
        else:  # all
            parts = [fmt(float(p) + offset) for p in parts]
        items.append(":".join(parts))
    return t.sep.join(items)


def merge(name: str, members: list[Scenario], timed: dict[str, Timed], untimed: set[str] | None = None) -> Launch:
    """One launch for `members` (raises ScenarioError when they cannot share)."""
    first = members[0]
    if len(members) == 1:
        return Launch(first.name, first.kind, members, list(first.args), dict(first.env), first.run_seconds)
    for s in members:
        if s.kind != GAME:
            raise ScenarioError(f"launch {name}: {s.name} is not a game scenario")
        if s.args != first.args:
            raise ScenarioError(f"launch {name}: {s.name} starts differently ({s.args} vs {first.args})")
        if s.offset and untimed is not None:
            unknown = [k for k in s.env if k not in timed and k not in untimed]
            if unknown:
                raise ScenarioError(f"launch {name}: {s.name} has an offset but {unknown} are not known to be timed or untimed")
    env: dict[str, str] = {}
    notes: list[str] = []
    for var in sorted({k for s in members for k in s.env}):
        setters = [s for s in members if var in s.env]
        if var in timed:
            t = timed[var]
            items: list[str] = []
            for s in setters:  # the same item twice is one action (members that share a setup)
                items += [i for i in shift(s.env[var], t, s.offset).split(t.sep) if i and i not in items]
            env[var] = t.sep.join(items)
            continue
        values = {s.env[var] for s in setters}
        if len(values) > 1:
            raise ScenarioError(f"launch {name}: {var} differs between {[s.name for s in setters]}")
        env[var] = values.pop()
        others = [s.name for s in members if var not in s.env]
        if others:
            notes.append(f"{var}={env[var]} also applies to {', '.join(others)}")
    run = max(s.run_seconds + s.offset for s in members)
    return Launch(name, GAME, list(members), list(first.args), env, run, notes)


def groups(scenarios: list[Scenario]) -> dict[str, list[Scenario]]:
    out: dict[str, list[Scenario]] = {}
    for s in scenarios:
        out.setdefault(s.launch or s.name, []).append(s)
    return out


def validate_launches(suite: Suite) -> None:
    names = {s.name for s in suite.scenarios}
    for key, members in groups(suite.scenarios).items():
        if len(members) < 2:
            continue
        if key in names and not any(m.name == key for m in members):
            raise ScenarioError(f"launch name {key} clashes with a scenario name")
        merge(key, members, suite.timed, suite.untimed)


def plan(suite: Suite, chosen: list[Scenario], no_merge: bool = False) -> list[Launch]:
    """Launches for the chosen scenarios, in the scenario file's order."""
    if no_merge:
        return [merge(s.name, [s], suite.timed) for s in chosen]
    out = []
    for key, members in groups(chosen).items():
        name = key if len(members) > 1 else members[0].name
        out.append(merge(name, members, suite.timed, suite.untimed))
    return out
