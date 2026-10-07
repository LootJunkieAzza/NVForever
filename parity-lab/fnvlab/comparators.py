"""Comparators: oracle (expected) vs candidate (actual).

Every comparator returns a ComparisonResult with:
  score     float in [0, 1]; 1.0 means identical under the comparator's rules
  failures  list of Failure(kind, path, expected, actual)
  stats     dict of counters for the report

Comparators never raise on data mismatch. They raise ComparatorError only for
misconfiguration (e.g. a missing key field), which the runner reports as an
"error" status for that scenario.
"""
from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Any, Callable

MAX_RECORDED_FAILURES = 200  # per scenario; totals are still counted


class ComparatorError(Exception):
    pass


@dataclass
class Failure:
    kind: str          # e.g. "value", "type", "missing", "extra", "length", "divergence"
    path: str          # JSON-pointer-like path, e.g. "/records/0x0001F4A2/edid"
    expected: Any = None
    actual: Any = None

    def to_json(self) -> dict:
        return {"kind": self.kind, "path": self.path,
                "expected": _clip(self.expected), "actual": _clip(self.actual)}


@dataclass
class ComparisonResult:
    score: float
    failures: list[Failure] = field(default_factory=list)
    total_failures: int = 0
    stats: dict = field(default_factory=dict)


def _clip(v: Any, limit: int = 300) -> Any:
    """Keep reports readable: long strings/containers are summarised."""
    if isinstance(v, str) and len(v) > limit:
        return v[:limit] + f"...(+{len(v) - limit} chars)"
    if isinstance(v, (list, dict)):
        s = repr(v)
        if len(s) > limit:
            kind = "list" if isinstance(v, list) else "dict"
            return f"<{kind} len={len(v)}>"
    return v


class _Collector:
    def __init__(self) -> None:
        self.failures: list[Failure] = []
        self.total = 0

    def add(self, f: Failure) -> None:
        self.total += 1
        if len(self.failures) < MAX_RECORDED_FAILURES:
            self.failures.append(f)


def _esc(key: Any) -> str:
    return str(key).replace("~", "~0").replace("/", "~1")


def floats_equal(a: float, b: float, abs_tol: float, rel_tol: float) -> bool:
    if isinstance(a, float) and isinstance(b, float):
        if math.isnan(a) and math.isnan(b):
            return True
        if math.isinf(a) or math.isinf(b):
            return a == b
    return math.isclose(a, b, rel_tol=rel_tol, abs_tol=abs_tol)


def _is_num(v: Any) -> bool:
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def deep_diff(expected: Any, actual: Any, path: str, col: _Collector,
              abs_tol: float, rel_tol: float, ignore: set[str],
              subset: bool = False) -> tuple[int, int]:
    """Recursive diff. Returns (leaves_matched, leaves_total) counted over `expected`.

    Numbers compare with tolerance (int vs float allowed). Dict keys present only
    in `actual` are reported as "extra" and count as one extra leaf in the total.
    """
    if path in ignore:
        return (0, 0)
    if isinstance(expected, dict):
        if not isinstance(actual, dict):
            col.add(Failure("type", path or "/", "object", type(actual).__name__))
            return (0, max(1, _leaf_count(expected)))
        if not expected and (not actual or subset):
            return (1, 1)  # empty object matches: one leaf
        m = t = 0
        for k, ev in expected.items():
            p = f"{path}/{_esc(k)}"
            if p in ignore:
                continue
            if k not in actual:
                col.add(Failure("missing", p, ev, None))
                t += max(1, _leaf_count(ev))
                continue
            mm, tt = deep_diff(ev, actual[k], p, col, abs_tol, rel_tol, ignore, subset)
            m += mm
            t += tt
        for k in actual:
            p = f"{path}/{_esc(k)}"
            if k not in expected and p not in ignore and not subset:
                col.add(Failure("extra", p, None, actual[k]))
                t += 1
        return (m, t)
    if isinstance(expected, list):
        if not isinstance(actual, list):
            col.add(Failure("type", path or "/", "array", type(actual).__name__))
            return (0, max(1, _leaf_count(expected)))
        if not expected and not actual:
            return (1, 1)  # two empty arrays: one matching leaf
        m = t = 0
        if len(expected) != len(actual):
            col.add(Failure("length", path or "/", len(expected), len(actual)))
        for i, ev in enumerate(expected):
            p = f"{path}/{i}"
            if i >= len(actual):
                t += max(1, _leaf_count(ev))
                continue
            mm, tt = deep_diff(ev, actual[i], p, col, abs_tol, rel_tol, ignore, subset)
            m += mm
            t += tt
        extra = len(actual) - len(expected)
        if extra > 0:
            t += extra
        return (m, t)
    # leaf
    if _is_num(expected) and _is_num(actual):
        ok = floats_equal(float(expected), float(actual), abs_tol, rel_tol) \
            if (isinstance(expected, float) or isinstance(actual, float)) else expected == actual
    else:
        ok = type(expected) is type(actual) and expected == actual
    if not ok:
        col.add(Failure("value", path or "/", expected, actual))
        return (0, 1)
    return (1, 1)


def _leaf_count(v: Any) -> int:
    if isinstance(v, dict):
        return sum(_leaf_count(x) for x in v.values()) or 1
    if isinstance(v, list):
        return sum(_leaf_count(x) for x in v) or 1
    return 1


def _select(doc: Any, pointer: str | None) -> Any:
    """Resolve a simple '/a/b/0' pointer. Empty/None selects the whole document."""
    if not pointer:
        return doc
    cur = doc
    for part in pointer.strip("/").split("/"):
        part = part.replace("~1", "/").replace("~0", "~")
        if isinstance(cur, list):
            try:
                cur = cur[int(part)]
            except (ValueError, IndexError) as e:
                raise ComparatorError(f"pointer {pointer!r} not resolvable: {e}") from e
        elif isinstance(cur, dict):
            if part not in cur:
                raise ComparatorError(f"pointer {pointer!r}: key {part!r} not found")
            cur = cur[part]
        else:
            raise ComparatorError(f"pointer {pointer!r} walks into a scalar")
    return cur


# --------------------------------------------------------------------------- exact

def compare_exact(expected: Any, actual: Any, cfg: dict) -> ComparisonResult:
    abs_tol = float(cfg.get("abs_tol", 0.0))
    rel_tol = float(cfg.get("rel_tol", 0.0))
    ignore = set(cfg.get("ignore_paths", []))
    sel = cfg.get("select")
    try:
        e = _select(expected, sel)
    except ComparatorError as ex:
        raise ComparatorError(f"oracle: {ex}") from ex
    try:
        a = _select(actual, sel)
    except ComparatorError:
        col = _Collector()
        col.add(Failure("missing", sel or "/", "<present>", None))
        return ComparisonResult(0.0, col.failures, col.total, {"leaves": _leaf_count(e)})
    col = _Collector()
    # subset: object keys present only in the candidate are not failures (arrays
    # must still match in length).
    m, t = deep_diff(e, a, "", col, abs_tol, rel_tol, ignore, bool(cfg.get("subset", False)))
    score = 1.0 if t == 0 else m / t
    if col.total and score == 1.0:  # e.g. only an ignored-length issue
        score = min(score, 1.0 - 1e-9)
    return ComparisonResult(score, col.failures, col.total, {"leaves_matched": m, "leaves_total": t})


# --------------------------------------------------------------------------- keyed

def _index(rows: Any, key: str | list[str], side: str) -> tuple[dict, list[Failure]]:
    if not isinstance(rows, list):
        raise ComparatorError(f"{side}: keyed comparator needs an array, got {type(rows).__name__}")
    keys = [key] if isinstance(key, str) else list(key)
    out: dict = {}
    problems: list[Failure] = []
    for i, row in enumerate(rows):
        if not isinstance(row, dict) or any(k not in row for k in keys):
            problems.append(Failure("malformed", f"/{side}/{i}", f"object with {keys}", _clip(row)))
            continue
        k = tuple(row[x] for x in keys) if len(keys) > 1 else row[keys[0]]
        if isinstance(k, list):
            k = tuple(k)
        if k in out:
            problems.append(Failure("duplicate_key", f"/{side}/{_esc(k)}", None, None))
            continue
        out[k] = row
    return out, problems


def compare_keyed(expected: Any, actual: Any, cfg: dict) -> ComparisonResult:
    key = cfg.get("key")
    if not key:
        raise ComparatorError("keyed comparator requires 'key'")
    fields = cfg.get("fields")  # None = compare every field present in the oracle row
    abs_tol = float(cfg.get("abs_tol", 0.0))
    rel_tol = float(cfg.get("rel_tol", 0.0))
    sel = cfg.get("select")
    exp_rows = _select(expected, sel)
    try:
        act_rows = _select(actual, sel)
    except ComparatorError:
        col = _Collector()
        col.add(Failure("missing", sel or "/", "<collection>", None))
        return ComparisonResult(0.0, col.failures, col.total, {})
    e_idx, e_prob = _index(exp_rows, key, "oracle")
    if e_prob:
        raise ComparatorError(f"oracle has {len(e_prob)} malformed/duplicate rows, first: {e_prob[0].to_json()}")
    a_idx, a_prob = _index(act_rows, key, "candidate")
    col = _Collector()
    for p in a_prob:
        col.add(p)
    missing = extra = changed = equal = 0
    for k, erow in e_idx.items():
        kp = f"/{_esc(_fmt_key(k))}"
        arow = a_idx.get(k)
        if arow is None:
            missing += 1
            col.add(Failure("missing", kp, _clip(erow), None))
            continue
        sub = _Collector()
        names = fields if fields is not None else list(erow.keys())
        for f in names:
            if f not in erow:
                continue  # oracle lacks this field for this row; nothing to compare
            fp = f"{kp}/{_esc(f)}"
            if f not in arow:
                sub.add(Failure("missing_field", fp, erow[f], None))
                continue
            deep_diff(erow[f], arow[f], fp, sub, abs_tol, rel_tol, set())
        if sub.total:
            changed += 1
            for fl in sub.failures:
                col.add(fl)
            col.total += sub.total - len(sub.failures)
        else:
            equal += 1
    for k, arow in a_idx.items():
        if k not in e_idx:
            extra += 1
            col.add(Failure("extra", f"/{_esc(_fmt_key(k))}", None, _clip(arow)))
    denom = len(e_idx) + extra + len(a_prob)
    score = 1.0 if denom == 0 else equal / denom
    return ComparisonResult(score, col.failures, col.total,
                            {"oracle_rows": len(e_idx), "candidate_rows": len(a_idx), "equal": equal,
                             "changed": changed, "missing": missing, "extra": extra,
                             "malformed": len(a_prob)})


def _fmt_key(k: Any) -> str:
    if isinstance(k, tuple):
        return "|".join(str(x) for x in k)
    return str(k)


# --------------------------------------------------------------------------- sequence

def compare_sequence(expected: Any, actual: Any, cfg: dict) -> ComparisonResult:
    """Ordered traces. Score = length of the matching prefix / oracle length.

    Reports the first divergence (and a length mismatch, if any). Elements are
    compared with deep_diff so tolerances apply inside events.
    """
    abs_tol = float(cfg.get("abs_tol", 0.0))
    rel_tol = float(cfg.get("rel_tol", 0.0))
    fields = cfg.get("fields")
    sel = cfg.get("select")
    e = _select(expected, sel)
    try:
        a = _select(actual, sel)
    except ComparatorError:
        a = None
    if not isinstance(e, list):
        raise ComparatorError("oracle: sequence comparator needs an array")
    col = _Collector()
    if not isinstance(a, list):
        col.add(Failure("type", sel or "/", "array", type(a).__name__))
        return ComparisonResult(0.0, col.failures, col.total, {"oracle_len": len(e)})

    def proj(ev: Any) -> Any:
        if fields is not None and isinstance(ev, dict):
            return {k: ev.get(k) for k in fields}
        return ev

    prefix = 0
    for i, ev in enumerate(e):
        if i >= len(a):
            col.add(Failure("divergence", f"/{i}", _clip(ev), "<end of candidate trace>"))
            break
        sub = _Collector()
        deep_diff(proj(ev), proj(a[i]), f"/{i}", sub, abs_tol, rel_tol, set())
        if sub.total:
            col.add(Failure("divergence", f"/{i}", _clip(proj(ev)), _clip(proj(a[i]))))
            for fl in sub.failures[:20]:
                col.add(fl)
            break
        prefix += 1
    if len(a) != len(e):
        col.add(Failure("length", sel or "/", len(e), len(a)))
    if len(e) == 0:
        score = 1.0 if len(a) == 0 else 0.0
    else:
        score = prefix / len(e)
        if score == 1.0 and len(a) != len(e):
            score = len(e) / max(len(a), 1)
    return ComparisonResult(score, col.failures, col.total,
                            {"oracle_len": len(e), "candidate_len": len(a), "matching_prefix": prefix})


# --------------------------------------------------------------------------- image

def compare_image(expected: Any, actual: Any, cfg: dict) -> ComparisonResult:
    """Rendered frame vs reference frame (fnvlab.image for the metrics).

    expected / actual: ``{"image": path, "masks": [[x0, y0, x1, y1], ...]}``; a relative
    expected path is under the goldens directory (passed by the runner as
    ``_goldens_dir``), masks are in the reference image's pixels and apply to both.
    cfg: ``min_ssim`` (required), optional ``max_mae``, ``max_mean_color_delta``,
    ``width`` (working width, default 192).
    Score: 1.0 when every threshold holds, else the SSIM (0 if the sizes' aspect ratios
    differ by more than 1%).
    """
    import os
    from . import image as im

    if "min_ssim" not in cfg:
        raise ComparatorError("image comparator needs 'min_ssim'")
    if not isinstance(expected, dict) or not isinstance(actual, dict) or "image" not in expected or "image" not in actual:
        raise ComparatorError("image comparator expects {'image': path} documents on both sides")
    ep = expected["image"]
    if not os.path.isabs(ep):
        ep = os.path.join(cfg.get("_goldens_dir", "."), ep)
    try:
        ei, ai = im.load(ep), im.load(actual["image"])
    except (OSError, im.ImageError) as e:
        raise ComparatorError(f"cannot read image: {e}") from e
    col = _Collector()
    ra, rb = ei.width / ei.height, ai.width / ai.height
    stats: dict = {"expected_size": [ei.width, ei.height], "actual_size": [ai.width, ai.height]}
    if abs(ra - rb) / ra > 0.01:
        col.add(Failure("aspect", "/image", [ei.width, ei.height], [ai.width, ai.height]))
        return ComparisonResult(0.0, col.failures, col.total, stats)
    masks = expected.get("masks") or []
    width = int(cfg.get("width", 192))
    scale = ai.width / ei.width
    a_masks = [[m[0] * scale, m[1] * scale, m[2] * scale, m[3] * scale] for m in masks]
    m = im.compare(im.downsample(ei, width, masks), im.downsample(ai, width, a_masks))
    stats.update({"ssim": round(m.ssim, 4), "mae": round(m.mae, 4), "mean_color_delta": round(m.mean_color_delta, 4),
                  "working_size": [m.width, m.height], "tiles": m.tiles})
    if m.ssim < cfg["min_ssim"]:
        col.add(Failure("ssim", "/image", f">= {cfg['min_ssim']}", round(m.ssim, 4)))
    if "max_mae" in cfg and m.mae > cfg["max_mae"]:
        col.add(Failure("mae", "/image", f"<= {cfg['max_mae']}", round(m.mae, 4)))
    if "max_mean_color_delta" in cfg and m.mean_color_delta > cfg["max_mean_color_delta"]:
        col.add(Failure("mean_color", "/image", f"<= {cfg['max_mean_color_delta']}", round(m.mean_color_delta, 4)))
    score = 1.0 if col.total == 0 else max(0.0, min(1.0, m.ssim))
    return ComparisonResult(score, col.failures, col.total, stats)


# --------------------------------------------------------------------------- distribution

def _gammainc_upper(a: float, x: float) -> float:
    """Regularised upper incomplete gamma Q(a, x) = Gamma(a, x) / Gamma(a), for a > 0.

    Series for x < a + 1, Lentz continued fraction otherwise (the textbook pair; both
    converge quickly in their region). Accurate to ~1e-12, plenty for p-values.
    """
    if x <= 0.0:
        return 1.0
    ln_pre = a * math.log(x) - x - math.lgamma(a)
    if x < a + 1.0:
        term = total = 1.0 / a
        n = a
        for _ in range(10_000):
            n += 1.0
            term *= x / n
            total += term
            if abs(term) < abs(total) * 1e-15:
                break
        return max(0.0, 1.0 - total * math.exp(ln_pre))
    tiny = 1e-300
    b = x + 1.0 - a
    c = 1.0 / tiny
    d = 1.0 / b
    h = d
    for i in range(1, 10_000):
        an = -i * (i - a)
        b += 2.0
        d = an * d + b
        d = tiny if abs(d) < tiny else d
        c = b + an / c
        c = tiny if abs(c) < tiny else c
        d = 1.0 / d
        delta = d * c
        h *= delta
        if abs(delta - 1.0) < 1e-15:
            break
    return min(1.0, math.exp(ln_pre) * h)


def chi2_sf(stat: float, df: int) -> float:
    """P(X >= stat) for X ~ chi-square with df degrees of freedom."""
    if df <= 0:
        return 1.0
    return _gammainc_upper(df / 2.0, stat / 2.0)


def _pool(cats: list, expected_min: list[float], min_expected: float) -> list[list]:
    """Group categories so every group's smallest expected count reaches min_expected.

    Categories already large enough stay alone; the small ones are merged (in order of
    increasing expectation) into one pooled group, and if that group is still too small
    it absorbs the smallest remaining single category, until it is large enough or only
    one group is left.
    """
    order = sorted(range(len(cats)), key=lambda i: (expected_min[i], str(cats[i])))
    big = [[cats[i]] for i in order if expected_min[i] >= min_expected]
    small = [cats[i] for i in order if expected_min[i] < min_expected]
    if not small:
        return big
    pooled = list(small)
    pooled_e = sum(expected_min[cats.index(c)] for c in small)
    while pooled_e < min_expected and big:
        g = big.pop(0)  # smallest remaining single category
        pooled.extend(g)
        pooled_e += expected_min[cats.index(g[0])]
    return big + [pooled]


def _histogram(row: dict, field_name: str, side: str, label: str) -> dict:
    h = row.get(field_name)
    if not isinstance(h, dict) or not all(_is_num(v) and v >= 0 for v in h.values()):
        raise ValueError(f"{side} case {label!r}: {field_name!r} must map outcomes to non-negative numbers")
    return h


def _test_case(oracle_counts: dict, cand: dict, cfg: dict) -> dict:
    """Chi-square test of one case. Returns {p, chi2, df, mode, tv, impossible}."""
    min_expected = float(cfg.get("min_expected", 5.0))
    n_o = float(sum(oracle_counts.values()))
    probs = cand.get("probabilities")
    if probs is not None:
        # Goodness of fit: oracle samples against the candidate's exact distribution.
        cats = sorted(set(oracle_counts) | set(probs), key=str)
        total_p = float(sum(probs.values()))
        p = [float(probs.get(c, 0.0)) / total_p if total_p > 0 else 0.0 for c in cats]
        obs = [float(oracle_counts.get(c, 0)) for c in cats]
        impossible = [c for c, pi, o in zip(cats, p, obs) if pi == 0.0 and o > 0]
        tv = 0.5 * sum(abs(o / n_o - pi) for o, pi in zip(obs, p)) if n_o else 0.0
        if impossible:
            return {"mode": "fit", "p": 0.0, "chi2": math.inf, "df": len(cats) - 1, "tv": tv,
                    "impossible": impossible}
        exp = [n_o * pi for pi in p]
        groups = _pool(cats, exp, min_expected)
        chi2 = 0.0
        for g in groups:
            o = sum(obs[cats.index(c)] for c in g)
            e = sum(exp[cats.index(c)] for c in g)
            if e > 0:
                chi2 += (o - e) ** 2 / e
        df = len(groups) - 1
        return {"mode": "fit", "p": chi2_sf(chi2, df), "chi2": chi2, "df": df, "tv": tv, "impossible": []}
    # Homogeneity: two samples (2 x K contingency table).
    counts = cand.get("outcomes", {})
    n_c = float(sum(counts.values()))
    cats = sorted(set(oracle_counts) | set(counts), key=str)
    o_row = [float(oracle_counts.get(c, 0)) for c in cats]
    c_row = [float(counts.get(c, 0)) for c in cats]
    total = n_o + n_c
    tv = 0.5 * sum(abs(a / n_o - b / n_c) for a, b in zip(o_row, c_row)) if n_o and n_c else 1.0
    if n_o == 0 or n_c == 0:
        return {"mode": "homogeneity", "p": 0.0 if n_o != n_c else 1.0, "chi2": math.inf, "df": 0, "tv": tv,
                "impossible": []}
    col = [a + b for a, b in zip(o_row, c_row)]
    # The smaller row has the smaller expected counts: pool on that row's expectation.
    e_min = [k * min(n_o, n_c) / total for k in col]
    groups = _pool(cats, e_min, min_expected)
    chi2 = 0.0
    for g in groups:
        idx = [cats.index(c) for c in g]
        k = sum(col[i] for i in idx)
        for row, n in ((o_row, n_o), (c_row, n_c)):
            e = n * k / total
            o = sum(row[i] for i in idx)
            if e > 0:
                chi2 += (o - e) ** 2 / e
    df = len(groups) - 1
    return {"mode": "homogeneity", "p": chi2_sf(chi2, df), "chi2": chi2, "df": df, "tv": tv, "impossible": []}


def compare_distribution(expected: Any, actual: Any, cfg: dict) -> ComparisonResult:
    """Random outcomes (spec section 8.1): per case, sampled outcome counts from the
    original game against the candidate's samples or exact probabilities.

    Documents hold an array of cases (``select``), keyed by ``key`` (default ``label``).
    Oracle case: ``{"<key>": .., "outcomes": {"<outcome>": count}}``. Candidate case:
    the same, or ``"probabilities": {"<outcome>": p}`` (exact distribution; used when
    present). Per case, Pearson's chi-square test: goodness of fit against the
    candidate's probabilities, else homogeneity of the two samples; categories whose
    expected count is below ``min_expected`` (default 5) are pooled. A case passes when
    the p-value is at least ``alpha`` (default 0.01) and, in fit mode, the oracle shows no
    outcome the candidate gives probability 0. Score = passing cases / (oracle cases +
    extra candidate cases). Stats: per-case p-value, chi-square, degrees of freedom and
    total-variation distance.
    """
    key = cfg.get("key", "label")
    alpha = float(cfg.get("alpha", 0.01))
    sel = cfg.get("select")
    field_name = cfg.get("field", "outcomes")
    e_rows = _select(expected, sel)
    try:
        a_rows = _select(actual, sel)
    except ComparatorError:
        col = _Collector()
        col.add(Failure("missing", sel or "/", "<collection>", None))
        return ComparisonResult(0.0, col.failures, col.total, {})
    e_idx, e_prob = _index(e_rows, key, "oracle")
    if e_prob:
        raise ComparatorError(f"oracle has {len(e_prob)} malformed/duplicate rows, first: {e_prob[0].to_json()}")
    a_idx, a_prob = _index(a_rows, key, "candidate")
    col = _Collector()
    for p in a_prob:
        col.add(p)
    cases: dict = {}
    passed = 0
    for k, erow in e_idx.items():
        kp = f"/{_esc(_fmt_key(k))}"
        try:
            oh = _histogram(erow, field_name, "oracle", _fmt_key(k))
        except ValueError as ex:
            raise ComparatorError(str(ex)) from ex
        if not oh or sum(oh.values()) <= 0:
            raise ComparatorError(f"oracle case {_fmt_key(k)!r} has no samples")
        arow = a_idx.get(k)
        if arow is None:
            col.add(Failure("missing", kp, _clip(oh), None))
            continue
        cand: dict = {}
        try:
            if "probabilities" in arow:
                cand["probabilities"] = _histogram(arow, "probabilities", "candidate", _fmt_key(k))
            else:
                cand["outcomes"] = _histogram(arow, field_name, "candidate", _fmt_key(k))
        except ValueError as ex:
            col.add(Failure("malformed", kp, f"{field_name} or probabilities", _clip(str(ex))))
            continue
        r = _test_case(oh, cand, cfg)
        cases[_fmt_key(k)] = {"p": round(r["p"], 6), "chi2": None if math.isinf(r["chi2"]) else round(r["chi2"], 4),
                              "df": r["df"], "tv": round(r["tv"], 4), "mode": r["mode"]}
        if r["impossible"]:
            col.add(Failure("impossible_outcome", kp, _clip(r["impossible"]), "probability 0"))
        elif r["p"] < alpha:
            col.add(Failure("distribution", kp, f"p >= {alpha}", round(r["p"], 6)))
        else:
            passed += 1
    extra = [k for k in a_idx if k not in e_idx]
    for k in extra:
        col.add(Failure("extra", f"/{_esc(_fmt_key(k))}", None, _clip(a_idx[k])))
    denom = len(e_idx) + len(extra) + len(a_prob)
    score = 1.0 if denom == 0 else passed / denom
    return ComparisonResult(score, col.failures, col.total,
                            {"oracle_cases": len(e_idx), "passed": passed, "extra": len(extra),
                             "alpha": alpha, "cases": cases})


# --------------------------------------------------------------------------- registry

def _planned(name: str) -> Callable[[Any, Any, dict], ComparisonResult]:
    def f(_e: Any, _a: Any, _c: dict) -> ComparisonResult:
        raise ComparatorError(f"comparator '{name}' is planned but not implemented yet")
    return f


COMPARATORS: dict[str, Callable[[Any, Any, dict], ComparisonResult]] = {
    "exact": compare_exact,
    "keyed": compare_keyed,
    "sequence": compare_sequence,
    "image": compare_image,
    "distribution": compare_distribution,
}
