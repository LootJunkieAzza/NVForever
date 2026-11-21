"""Run reports: runs/latest.json (machine) and runs/latest.md (human/agent)."""
from __future__ import annotations

import json
from collections import defaultdict
from pathlib import Path

from .runner import ScenarioResult


def build_report(results: list[ScenarioResult], regressions: list[dict], started_at: str,
                 reveal: bool, filters: dict) -> dict:
    per_layer: dict[int, dict] = defaultdict(lambda: {"total": 0, "pass": 0, "fail": 0, "error": 0,
                                                      "score_sum": 0.0})
    for r in results:
        L = per_layer[r.layer]
        L["total"] += 1
        L[r.status] += 1
        L["score_sum"] += r.score
    layers = {}
    for k in sorted(per_layer):
        L = per_layer[k]
        layers[str(k)] = {"total": L["total"], "pass": L["pass"], "fail": L["fail"], "error": L["error"],
                          "mean_score": round(L["score_sum"] / L["total"], 6) if L["total"] else 0.0}
    # next_work: lowest layer first, then worst score; holdouts never appear here.
    todo = [r for r in results if r.status != "pass" and not r.holdout]
    todo.sort(key=lambda r: (r.layer, r.score, r.id))
    next_work = []
    for r in todo:
        item = {"id": r.id, "layer": r.layer, "status": r.status, "score": round(r.score, 6),
                "total_failures": r.total_failures,
                "top_failures": [f["id"] for f in r.failures[:5]]}
        if r.error:
            item["error"] = r.error.splitlines()[0][:300]
        next_work.append(item)
    return {
        "schema": "fnvlab.run/1",
        "started_at": started_at,
        "filters": filters,
        "revealed": reveal,
        "summary": {"total": len(results),
                    "pass": sum(r.status == "pass" for r in results),
                    "fail": sum(r.status == "fail" for r in results),
                    "error": sum(r.status == "error" for r in results),
                    "holdouts": sum(r.holdout for r in results),
                    "regressions": len(regressions)},
        "layers": layers,
        "regressions": [_redact_reg(x, reveal) for x in regressions],
        "next_work": next_work,
        "results": [r.to_json(reveal) for r in results],
    }


def _redact_reg(x: dict, reveal: bool) -> dict:
    if x.get("holdout") and not reveal:
        return {"id": x["id"], "layer": x["layer"], "holdout": True, "redacted": True}
    return x


def render_markdown(rep: dict, max_failures: int = 15) -> str:
    s = rep["summary"]
    lines = [f"# Parity lab run — {rep['started_at']}", ""]
    lines.append(f"**{s['pass']}/{s['total']} pass**, {s['fail']} fail, {s['error']} error; "
                 f"{s['holdouts']} holdout(s); **{s['regressions']} regression(s)**.")
    if rep.get("revealed"):
        lines.append("")
        lines.append("> Holdout details were REVEALED by a human for this run.")
    lines += ["", "## Per layer", "", "| Layer | Total | Pass | Fail | Error | Mean score |",
              "|---|---|---|---|---|---|"]
    for k, L in rep["layers"].items():
        lines.append(f"| {k} | {L['total']} | {L['pass']} | {L['fail']} | {L['error']} | {L['mean_score']:.4f} |")
    if rep["regressions"]:
        lines += ["", "## Regressions vs baseline", ""]
        for r in rep["regressions"]:
            if r.get("redacted"):
                lines.append(f"- `{r['id']}` (holdout, details redacted)")
            else:
                lines.append(f"- `{r['id']}`: {r['baseline_status']} {r['baseline_score']} → "
                             f"{r['status']} {r['score']}")
    lines += ["", "## Next work (lowest layer first, then worst score)", ""]
    if not rep["next_work"]:
        lines.append("_Nothing outstanding among visible scenarios._")
    for i, w in enumerate(rep["next_work"], 1):
        extra = f" — {w['error']}" if w.get("error") else ""
        lines.append(f"{i}. `{w['id']}` (L{w['layer']}, {w['status']}, score {w['score']:.4f}, "
                     f"{w['total_failures']} failure(s)){extra}")
    lines += ["", "## Scenario details", ""]
    for r in rep["results"]:
        if r.get("redacted"):
            lines.append(f"### `{r['id']}` — {r['status']} (holdout, score {r['score']:.4f})")
            lines.append("")
            lines.append("_Details redacted. Do not try to infer them._")
            lines.append("")
            continue
        lines.append(f"### `{r['id']}` — {r['status']} (score {r['score']:.4f}, L{r['layer']})")
        lines.append("")
        if r.get("description"):
            lines.append(r["description"])
            lines.append("")
        if r.get("stats"):
            lines.append("Stats: " + ", ".join(f"{k}={v}" for k, v in r["stats"].items()))
            lines.append("")
        if r.get("error"):
            lines.append("```")
            lines.append(r["error"][:3000])
            lines.append("```")
            lines.append("")
        if r.get("failures"):
            lines.append(f"Failures (showing {min(max_failures, len(r['failures']))} of {r['total_failures']}):")
            lines.append("")
            for f in r["failures"][:max_failures]:
                lines.append(f"- `{f['id']}` {f['kind']} at `{f['path']}`: expected `{_short(f['expected'])}`, "
                             f"got `{_short(f['actual'])}`")
            lines.append("")
    return "\n".join(lines) + "\n"


def _short(v, n: int = 120) -> str:
    s = json.dumps(v, ensure_ascii=False) if not isinstance(v, str) else v
    s = s.replace("`", "'")
    return s if len(s) <= n else s[:n] + "…"


def write_reports(rep: dict, runs_dir: Path) -> tuple[Path, Path]:
    runs_dir.mkdir(parents=True, exist_ok=True)
    hist = runs_dir / "history"
    hist.mkdir(exist_ok=True)
    jp = runs_dir / "latest.json"
    mp = runs_dir / "latest.md"
    data = json.dumps(rep, indent=2, ensure_ascii=False)
    jp.write_text(data, encoding="utf-8")
    mp.write_text(render_markdown(rep), encoding="utf-8")
    stamp = rep["started_at"].replace(":", "").replace("-", "")
    target = hist / f"{stamp}.json"
    n = 1
    while target.exists():  # several runs within one second
        target = hist / f"{stamp}-{n}.json"
        n += 1
    target.write_text(data, encoding="utf-8")
    return jp, mp
