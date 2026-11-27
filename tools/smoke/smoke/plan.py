"""Launch planning: scenarios that name the same `launch` share one game run.

A shared run is safe when the members start the same way (identical nvforever
arguments) and their test aids do not fight: a timed aid (`[timed]` in the scenario
file) gets the members' items joined, moved by each member's `offset`; any other aid
must have the same value in every member that sets it (a member that does not set it
runs with it too, which the plan lists as a note). Members whose run is shared keep
their own expectations, all checked against the shared log. Which scenarios share a
launch is chosen by hand in the scenario file and was checked by running them both
ways (docs in the file); `--no-merge` runs every scenario on its own.

A run can start every game launch the same way on top of its scenarios' arguments
(`run`/`plan --size WxH --extra-args ARGS`, `with_window`): the extra arguments are added
after the launch's own, and the window size replaces a `--size` the launch has (either
form) or is added. Applied once per launch, after the merge; fnvtool runs are untouched.
"""
from __future__ import annotations

import re
import shlex
from dataclasses import dataclass, field, replace

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
    front: bool = False  # the window held in front while it runs (every member asks for it)

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
        return Launch(first.name, first.kind, members, list(first.args), dict(first.env), first.run_seconds, front=first.front)
    for s in members:
        if s.kind != GAME:
            raise ScenarioError(f"launch {name}: {s.name} is not a game scenario")
        if s.args != first.args:
            raise ScenarioError(f"launch {name}: {s.name} starts differently ({s.args} vs {first.args})")
        if s.front != first.front:
            raise ScenarioError(f"launch {name}: {s.name} and {first.name} differ in front (a shared launch holds its window in front for every member or none)")
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
    return Launch(name, GAME, list(members), list(first.args), env, run, notes, front=first.front)


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


SIZE = re.compile(r"[1-9]\d*x[1-9]\d*")


def window_size(text: str) -> str:
    """A `--size` value as nvforever takes it, `WxH` (e.g. 3840x2160); ValueError otherwise."""
    if not SIZE.fullmatch(text):
        raise ValueError(f"expected WxH with two whole numbers above 0, such as 3840x2160; got {text!r}")
    return text


def split_args(text: str) -> list[str]:
    """Arguments from one string, split like a shell: spaces separate, quotes group, and a
    backslash stays (Windows paths)."""
    lex = shlex.shlex(text, posix=True)
    lex.whitespace_split = True
    lex.escape = ""
    lex.commenters = ""
    return list(lex)


def with_window(args: list[str], size: str | None = None, extra: list[str] = ()) -> list[str]:
    """`args` with `extra` added, then `--size size` in place of every `--size X` /
    `--size=X` they hold, or added when they hold none."""
    out = list(args) + list(extra)
    if size is None:
        return out
    found = False
    i = 0
    while i < len(out):
        if out[i] == "--size" and i + 1 < len(out):
            out[i + 1] = size
            found = True
            i += 2
            continue
        if out[i].startswith("--size="):
            out[i] = f"--size={size}"
            found = True
        i += 1
    return out if found else out + ["--size", size]


def plan(suite: Suite, chosen: list[Scenario], no_merge: bool = False,
         size: str | None = None, extra: list[str] = ()) -> list[Launch]:
    """Launches for the chosen scenarios, in the scenario file's order; every game launch
    with the run's window `size` and `extra` arguments (`with_window`)."""
    if no_merge:
        out = [merge(s.name, [s], suite.timed) for s in chosen]
    else:
        out = []
        for key, members in groups(chosen).items():
            name = key if len(members) > 1 else members[0].name
            out.append(merge(name, members, suite.timed, suite.untimed))
    if size is None and not extra:
        return out
    return [replace(x, args=with_window(x.args, size, extra)) if x.kind == GAME else x for x in out]
