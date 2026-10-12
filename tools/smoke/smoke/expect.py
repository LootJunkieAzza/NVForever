"""Expectations on a run's log: what must appear, in which order, how often, and what
must not appear.

An expectation is written in the scenario file as a string or as a table:

- `"text"`: a line containing this text. `…` in the text stands for any run of
  characters (as in the prose of the old CLAUDE.md list).
- `{ re = "regex" }`: a line matching the regular expression (Python `re.search`).
- `count = N`, or `min = N` / `max = N`: how many lines match (default: at least one).
- `range = [lo, hi]` with a capture group (named `n`, else the first group): a matching
  line whose number (commas dropped) lies in [lo, hi]; with `all = true` every matching
  line must (and at least one must match).
- `gap = ["first", "second"]` with `range`: seconds between the first line matching
  each, by the log's timestamps.
- `{ json = "a.b.*.c" }` (tool scenarios, whose output is JSON): the values at that path
  in each JSON document of the log (`*` every element, `?field=value` the elements whose
  field has that value, `#` the length of a list or object; `schema = "fnv.x/1"` only
  documents of that schema), with `equals = value`, `null = true` or `range`; `count`/`min`/`max` count
  the values that pass; `all = true` wants every value to pass. Default: at least one.
- `known_failing = "why"`: a failure is reported as KNOWN and does not fail the run; a
  pass is reported so the mark can be removed.
"""
from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from datetime import datetime

ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")
# Bevy/tracing timestamp at the start of a line: 2026-10-11T03:12:45.123456Z
STAMP = re.compile(r"^\s*(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?)Z?\b")
NUMBER = re.compile(r"-?[\d,]*\.?\d+")


class ScenarioError(ValueError):
    """A scenario file entry that cannot be understood."""


def text_regex(text: str) -> str:
    """Regex for a plain-text expectation: literal, `…` matches anything."""
    return ".*?".join(re.escape(part) for part in text.split("…"))


def clean(line: str) -> str:
    return ANSI.sub("", line.rstrip("\r\n"))


def stamp_seconds(line: str) -> float | None:
    m = STAMP.match(line)
    if not m:
        return None
    try:
        return datetime.fromisoformat(m.group(1)).timestamp()
    except ValueError:
        return None


def number(s: str) -> float:
    return float(s.replace(",", ""))


@dataclass
class Expect:
    label: str
    pattern: re.Pattern | None = None
    count: int | None = None
    min: int | None = None
    max: int | None = None
    range: tuple[float, float] | None = None
    all: bool = False
    gap: tuple["Expect", "Expect"] | None = None
    known_failing: str | None = None
    json_path: list[str] | None = None
    schema: str | None = None
    equals: object = None
    has_equals: bool = False

    @classmethod
    def parse(cls, item, where: str = "") -> "Expect":
        if isinstance(item, str):
            return cls(label=item, pattern=re.compile(text_regex(item)))
        if not isinstance(item, dict):
            raise ScenarioError(f"{where}: expectation must be a string or a table, got {item!r}")
        known = {"text", "re", "count", "min", "max", "range", "all", "gap", "known_failing", "label",
                 "json", "schema", "equals", "null"}
        extra = set(item) - known
        if extra:
            raise ScenarioError(f"{where}: unknown expectation keys {sorted(extra)}")
        e = cls(label=item.get("label", ""), known_failing=item.get("known_failing"))
        if "gap" in item:
            a, b = item["gap"]
            e.gap = (cls.parse(a, where), cls.parse(b, where))
            if "range" not in item:
                raise ScenarioError(f"{where}: gap needs a range")
            e.label = e.label or f"{a!r} to {b!r}"
        elif "json" in item:
            e.json_path = item["json"].split(".")
            e.schema = item.get("schema")
            e.has_equals = "equals" in item or bool(item.get("null"))
            e.equals = None if item.get("null") else item.get("equals")
            want = f" = {json.dumps(e.equals)}" if e.has_equals else ""
            e.label = e.label or f"{e.schema + ' ' if e.schema else ''}{item['json']}{want}"
        elif "re" in item:
            try:
                e.pattern = re.compile(item["re"])
            except re.error as err:
                raise ScenarioError(f"{where}: bad regex {item['re']!r}: {err}") from err
            e.label = e.label or item["re"]
        elif "text" in item:
            e.pattern = re.compile(text_regex(item["text"]))
            e.label = e.label or item["text"]
        else:
            raise ScenarioError(f"{where}: expectation needs text, re, json or gap")
        e.count = item.get("count")
        e.min = item.get("min")
        e.max = item.get("max")
        if "range" in item:
            lo, hi = item["range"]
            e.range = (float(lo), float(hi))
            if e.pattern is not None and e.pattern.groups == 0:
                raise ScenarioError(f"{where}: range needs a capture group in {e.label!r}")
        e.all = bool(item.get("all", False))
        return e

    def json_values(self, docs: list) -> list:
        out = []
        for d in docs:
            if self.schema and not (isinstance(d, dict) and d.get("schema") == self.schema):
                continue
            out += walk(d, self.json_path)
        return out

    def value_ok(self, v) -> bool:
        if self.has_equals and v != self.equals:
            return False
        if self.range is not None:
            if isinstance(v, bool) or not isinstance(v, (int, float)):
                return False
            return self.range[0] <= v <= self.range[1]
        return True

    def value(self, m: re.Match) -> float | None:
        g = m.groupdict().get("n")
        if g is None:
            g = m.group(1) if m.re.groups else None
        if g is None:
            return None
        try:
            return number(g)
        except ValueError:
            return None

    def in_range(self, m: re.Match) -> bool:
        if self.range is None:
            return True
        v = self.value(m)
        return v is not None and self.range[0] <= v <= self.range[1]

    def first(self, lines: list[str], start: int = 0) -> int | None:
        """Index of the first line at or after `start` that satisfies this expectation."""
        for i in range(start, len(lines)):
            m = self.pattern.search(lines[i])
            if m and self.in_range(m):
                return i
        return None


def walk(node, path: list[str]) -> list:
    """Values at a dotted path; `*` takes every element of a list or object."""
    if not path:
        return [node]
    head, rest = path[0], path[1:]
    if head == "#":  # the length of a list or object
        return [len(node)] if isinstance(node, (list, dict)) else []
    if head.startswith("?") and "=" in head:  # the elements whose field has this value
        field_, want = head[1:].split("=", 1)
        kids = node.values() if isinstance(node, dict) else node if isinstance(node, list) else []
        return [v for k in kids if isinstance(k, dict) and str(k.get(field_)) == want for v in walk(k, rest)]
    if head == "*":
        kids = node.values() if isinstance(node, dict) else node if isinstance(node, list) else []
        return [v for k in kids for v in walk(k, rest)]
    if isinstance(node, dict) and head in node:
        return walk(node[head], rest)
    if isinstance(node, list) and head.lstrip("-").isdigit() and -len(node) <= int(head) < len(node):
        return walk(node[int(head)], rest)
    return []


_DOCS: dict[int, list] = {}


def json_docs(lines: list[str]) -> list:
    """Every JSON object in the log (tool output; other lines are skipped)."""
    key = id(lines)
    if key in _DOCS:
        return _DOCS[key]
    text, dec, i, out = "\n".join(lines), json.JSONDecoder(), 0, []
    while (j := text.find("{", i)) >= 0:
        try:
            obj, i = dec.raw_decode(text, j)
            out.append(obj)
        except ValueError:
            i = j + 1
    _DOCS.clear()
    _DOCS[key] = out
    return out


@dataclass
class Outcome:
    ok: bool
    known: str | None  # known-failing reason, when the expectation carries one
    what: str  # "missing", "unexpected", "count", ...
    label: str
    detail: str = ""
    context: list[str] = field(default_factory=list)


def hint_lines(lines: list[str], label: str, limit: int = 3) -> list[str]:
    """A few log lines that look like the one expected (same leading topic)."""
    lit = re.split(r"[…\\\[\](){}*+?|^$]", label, maxsplit=1)[0].strip()
    key = lit.split(":")[0] if ":" in lit[:40] else " ".join(lit.split()[:2])
    key = key.strip().lower()
    if len(key) < 3:
        return []
    out = [ln for ln in lines if key in ln.lower()]
    return [short(ln) for ln in out[-limit:]]


def short(line: str, n: int = 220) -> str:
    m = STAMP.match(line)
    if m:
        line = line[m.end():].lstrip()
    return line if len(line) <= n else line[: n - 1] + "…"


def check_json(e: Expect, lines: list[str]) -> Outcome:
    vals = e.json_values(json_docs(lines))
    good = [v for v in vals if e.value_ok(v)]
    shown = ", ".join(json.dumps(v)[:40] for v in vals[:6]) + (" …" if len(vals) > 6 else "")
    if e.count is not None or e.min is not None or e.max is not None:
        lo = e.count if e.count is not None else (e.min or 0)
        hi = e.count if e.count is not None else (e.max if e.max is not None else float("inf"))
        ok = lo <= len(good) <= hi
        return Outcome(ok, e.known_failing, "count", e.label, f"{len(good)} of {len(vals)} values pass ({shown})")
    if not vals:
        return Outcome(False, e.known_failing, "missing", e.label, "no such path in the output")
    ok = len(good) == len(vals) if e.all else bool(good)
    return Outcome(ok, e.known_failing, "value", e.label, "" if ok else f"got {shown}")


def check_one(e: Expect, lines: list[str]) -> Outcome:
    if e.json_path is not None:
        return check_json(e, lines)
    if e.gap is not None:
        a, b = (x.first(lines) for x in e.gap)
        if a is None or b is None:
            return Outcome(False, e.known_failing, "missing", e.label, "a line of the pair is missing")
        ta, tb = stamp_seconds(lines[a]), stamp_seconds(lines[b])
        if ta is None or tb is None:
            return Outcome(False, e.known_failing, "gap", e.label, "lines carry no timestamp")
        gap = tb - ta
        ok = e.range[0] <= gap <= e.range[1]
        return Outcome(ok, e.known_failing, "gap", e.label, f"{gap:.2f} s apart, want {e.range[0]:g}–{e.range[1]:g}",
                       [] if ok else [short(lines[a]), short(lines[b])])
    hits = [(i, m) for i, ln in enumerate(lines) if (m := e.pattern.search(ln))]
    if e.count is not None or e.min is not None or e.max is not None:
        n = len(hits)
        lo = e.count if e.count is not None else (e.min if e.min is not None else 0)
        hi = e.count if e.count is not None else (e.max if e.max is not None else float("inf"))
        ok = lo <= n <= hi
        want = f"{e.count}" if e.count is not None else f"{lo}–{'' if hi == float('inf') else hi}"
        ctx = [short(lines[i]) for i, _ in hits[:3]] if not ok else []
        return Outcome(ok, e.known_failing, "count", e.label, f"{n} lines, want {want}", ctx)
    if not hits:
        return Outcome(False, e.known_failing, "missing", e.label, "", hint_lines(lines, e.label))
    if e.range is not None:
        vals = [(i, e.value(m)) for i, m in hits]
        good = [v for _, v in vals if v is not None and e.range[0] <= v <= e.range[1]]
        ok = len(good) == len(vals) if e.all else bool(good)
        if not ok:
            shown = ", ".join("?" if v is None else f"{v:g}" for _, v in vals[:6])
            return Outcome(False, e.known_failing, "range", e.label,
                           f"values {shown}; want {e.range[0]:g}–{e.range[1]:g}{' (every line)' if e.all else ''}",
                           [short(lines[i]) for i, _ in vals[:2]])
    return Outcome(True, e.known_failing, "ok", e.label)


def check_order(seq: list[Expect], lines: list[str]) -> list[Outcome]:
    out = []
    at = 0
    for k, e in enumerate(seq):
        i = e.first(lines, at)
        if i is None:
            anywhere = e.first(lines, 0)
            detail = "" if anywhere is None else f"found only before the previous step (line {anywhere + 1})"
            prev = seq[k - 1].label if k else ""
            ctx = hint_lines(lines, e.label)
            out.append(Outcome(False, e.known_failing, "order" if anywhere is not None else "missing", e.label,
                               detail + (f"; expected after {prev!r}" if prev else ""), ctx))
            return out
        out.append(Outcome(True, e.known_failing, "ok", e.label))
        at = i + 1
    return out


def check_forbid(e: Expect, lines: list[str]) -> Outcome:
    if e.pattern is None:
        raise ScenarioError(f"forbid needs text or re: {e.label!r}")
    for ln in lines:
        m = e.pattern.search(ln)
        if m and e.in_range(m):
            return Outcome(False, e.known_failing, "unexpected", e.label, "", [short(ln)])
    return Outcome(True, e.known_failing, "ok", e.label)
