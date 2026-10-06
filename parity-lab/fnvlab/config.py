"""Lab configuration and scenario loading (stdlib only)."""
from __future__ import annotations

import json
import os
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from .comparators import COMPARATORS


class ConfigError(Exception):
    pass


@dataclass
class LabConfig:
    root: Path                     # directory containing the config file
    vars: dict[str, str]
    goldens_dir: Path
    scenarios_dirs: list[Path]
    runs_dir: Path
    baseline_path: Path
    default_timeout_s: float = 600.0

    @staticmethod
    def load(path: str | os.PathLike) -> "LabConfig":
        p = Path(path).resolve()
        try:
            # utf-8-sig: Windows PowerShell 5.1's "-Encoding utf8" writes a BOM.
            raw = json.loads(p.read_text(encoding="utf-8-sig"))
        except (OSError, json.JSONDecodeError) as e:
            raise ConfigError(f"cannot read config {p}: {e}") from e
        local = raw.get("local_overlay")
        if local:
            lp = (p.parent / local)
            if lp.exists():
                try:
                    overlay = json.loads(lp.read_text(encoding="utf-8-sig"))
                except json.JSONDecodeError as e:
                    raise ConfigError(f"bad local overlay {lp}: {e}") from e
                raw = _merge(raw, overlay)
        root = p.parent
        vars_ = {k: str(v) for k, v in raw.get("vars", {}).items()}
        # Environment overrides keep private paths out of committed config.
        for k in list(vars_):
            env = os.environ.get(f"FNVLAB_{k.upper()}")
            if env is not None:
                vars_[k] = env
        vars_.setdefault("lab_root", str(root))

        def rp(v: str) -> Path:
            v = substitute(v, vars_)
            q = Path(v)
            return q if q.is_absolute() else (root / q)

        sd = raw.get("scenarios_dirs", raw.get("scenarios_dir", "scenarios"))
        if isinstance(sd, str):
            sd = [sd]
        return LabConfig(
            root=root,
            vars=vars_,
            goldens_dir=rp(raw.get("goldens_dir", "goldens")),
            scenarios_dirs=[rp(x) for x in sd],
            runs_dir=rp(raw.get("runs_dir", "runs")),
            baseline_path=rp(raw.get("baseline", "baseline.json")),
            default_timeout_s=float(raw.get("default_timeout_s", 600)),
        )


def unset_vars(cfg: LabConfig) -> list[str]:
    """Variables still holding a committed "SET_ME" placeholder."""
    return sorted(k for k, v in cfg.vars.items() if v.startswith("SET_ME"))


def _merge(a: dict, b: dict) -> dict:
    out = dict(a)
    for k, v in b.items():
        if isinstance(v, dict) and isinstance(out.get(k), dict):
            out[k] = _merge(out[k], v)
        else:
            out[k] = v
    return out


_VAR = re.compile(r"\{([a-zA-Z_][a-zA-Z0-9_]*)\}")


def substitute(s: str, vars_: dict[str, str]) -> str:
    """Expand {name} placeholders; variables may reference other variables."""
    def rep(m: re.Match) -> str:
        k = m.group(1)
        if k not in vars_:
            raise ConfigError(f"unknown variable {{{k}}} in {s!r}")
        return vars_[k]
    out = s
    for _ in range(10):
        nxt = _VAR.sub(rep, out)
        if nxt == out:
            return nxt
        out = nxt
    raise ConfigError(f"variable expansion does not terminate for {s!r}")


@dataclass
class Scenario:
    id: str
    layer: int
    description: str
    oracle: dict
    candidate: dict
    comparator: dict
    tags: list[str] = field(default_factory=list)
    holdout: bool = False
    source_file: str = ""

    @staticmethod
    def from_json(d: dict, source: str) -> "Scenario":
        for req in ("id", "layer", "oracle", "candidate", "comparator"):
            if req not in d:
                raise ConfigError(f"{source}: scenario missing '{req}'")
        kind = d["comparator"].get("kind")
        if kind not in COMPARATORS:
            raise ConfigError(f"{source}: scenario {d['id']}: unknown comparator kind {kind!r}")
        if not isinstance(d["layer"], int) or not 0 <= d["layer"] <= 7:
            raise ConfigError(f"{source}: scenario {d['id']}: layer must be an int 0..7")
        o = d["oracle"]
        if not ("golden" in o or "inline" in o):
            raise ConfigError(f"{source}: scenario {d['id']}: oracle needs 'golden' or 'inline'")
        if "argv" not in d["candidate"]:
            raise ConfigError(f"{source}: scenario {d['id']}: candidate needs 'argv'")
        return Scenario(
            id=d["id"], layer=d["layer"], description=d.get("description", ""),
            oracle=o, candidate=d["candidate"], comparator=d["comparator"],
            tags=list(d.get("tags", [])), holdout=bool(d.get("holdout", False)), source_file=source,
        )


def load_scenarios(cfg: LabConfig) -> list[Scenario]:
    out: list[Scenario] = []
    seen: dict[str, str] = {}
    for sdir in cfg.scenarios_dirs:
        if not sdir.exists():
            continue
        for f in sorted(sdir.rglob("*.json")):
            try:
                data = json.loads(f.read_text(encoding="utf-8"))
            except json.JSONDecodeError as e:
                raise ConfigError(f"{f}: invalid JSON: {e}") from e
            items = data["scenarios"] if isinstance(data, dict) and "scenarios" in data else data
            if isinstance(items, dict):
                items = [items]
            for d in items:
                s = Scenario.from_json(d, str(f))
                if s.id in seen:
                    raise ConfigError(f"duplicate scenario id {s.id} in {f} and {seen[s.id]}")
                seen[s.id] = str(f)
                out.append(s)
    out.sort(key=lambda s: (s.layer, s.id))
    return out


def filter_scenarios(scen: list[Scenario], layers: list[int] | None, ids: list[str] | None,
                     tags: list[str] | None) -> list[Scenario]:
    import fnmatch
    res = []
    for s in scen:
        if layers and s.layer not in layers:
            continue
        if ids and not any(fnmatch.fnmatchcase(s.id, pat) or s.id == pat for pat in ids):
            continue
        if tags and not set(tags) & set(s.tags):
            continue
        res.append(s)
    return res


def load_json_file(p: Path) -> Any:
    return json.loads(p.read_text(encoding="utf-8"))
