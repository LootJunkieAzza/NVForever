"""Point the quest pilot at every walkthrough chain (issue #2365, docs/design/quest-pilot.md §7).

For each chain in `crates/fnv-game/walkthrough/` (or the ones named):

1. the headless trace: `fnvtool script run <Data> FalloutNV.esm --schema schemas/fnv
   --actions <chain>` (cached by the chain's and fnvtool's modification times);
2. the pilot: `nvforever --play --load <start> --pilot <chain> --pilot-expect <trace>
   --pilot-campaign --pilot-given --pilot-out <report>`, each game in a home folder of its
   own (a copy of the start save), at most `--jobs` at once, each at most `--minutes`;
3. a summary of every report, worst first: `summary.json` and `summary.md`.

A chain is run from its own `given` state on top of a world that has played the opening
(the start save, `pilot-after-intro` by default: make it once with a campaign of
`intro.actions.json` from `--new-game`). That reaches every chain; what it finds carries the
false alarms a thin `given` brings, so read a divergence against the chain's notes before
calling it a bug. A campaign (`--pilot-campaign` alone, chains in story order) has none.

    python tools/smoke/pilot/batch.py --exe <nvforever.exe> --data <Data> --fnvtool <fnvtool.exe> \
        --home F:/nvf-pilot-home [--jobs 2] [--minutes 30] [--only beyond-the-beef gi-blues]

Player reports stay off: every run passes `--data`.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
CHAINS = REPO / "crates" / "fnv-game" / "walkthrough"
AIDS = {
    "NVF_PLAY_TEST_SCRIPT_MENUS": "900",
    "NVF_PLAY_AIDS_FROM_SCRIPTS": "1",
    "NVF_PLAY_TEST_UNATTENDED": "1",
}
# What a finding of each kind weighs in the ranking: a soft lock or a quest that cannot go
# on above the pilot's own trouble.
WEIGHTS = {
    "softlock": 50,
    "no_way_out": 30,
    "unreachable": 20,
    "never_came": 20,
    "chain_abandoned": 20,
    "pilot_held": 20,
    "could_not_target": 10,
    "line_not_said": 8,
    "no_conversation": 8,
    "item_not_found": 6,
    "wait_timed_out": 6,
    "step_timed_out": 5,
    "fight_not_won": 3,
    "run_ended": 10,
    "ai_kill_not_done": 8,
    "stuck": 2,
}


def chains(only: list[str]) -> list[Path]:
    """The chain files, all or the named ones (name = file name without `.actions.json`)."""
    every = sorted(CHAINS.glob("*.actions.json"))
    if not only:
        return every
    names = {n.removesuffix(".actions.json") for n in only}
    picked = [c for c in every if c.name.removesuffix(".actions.json") in names]
    missing = names - {c.name.removesuffix(".actions.json") for c in picked}
    if missing:
        raise SystemExit(f"no such chain: {', '.join(sorted(missing))}")
    return picked


def name_of(chain: Path) -> str:
    return chain.name.removesuffix(".actions.json")


def headless_trace(chain: Path, out: Path, fnvtool: Path, data: Path) -> Path | None:
    """The chain's headless trace, made again when the chain or fnvtool is newer."""
    trace = out / "traces" / f"{name_of(chain)}.trace.json"
    newest = max(chain.stat().st_mtime, fnvtool.stat().st_mtime)
    if trace.exists() and trace.stat().st_mtime >= newest:
        return trace
    trace.parent.mkdir(parents=True, exist_ok=True)
    cmd = [str(fnvtool), "script", "run", str(data), "FalloutNV.esm", "--schema", str(REPO / "schemas" / "fnv"), "--actions", str(chain)]
    with open(trace, "w", encoding="utf-8") as f:
        code = subprocess.run(cmd, stdout=f, stderr=subprocess.DEVNULL, cwd=REPO).returncode
    return trace if code == 0 else None


def run_one(chain: Path, args: argparse.Namespace, slot: int) -> dict:
    """One chain in the game: its report (or what went wrong), with the run's facts."""
    name = name_of(chain)
    out = Path(args.out)
    report = out / "reports" / f"{name}.report.json"
    log = out / "logs" / f"{name}.log"
    report.parent.mkdir(parents=True, exist_ok=True)
    log.parent.mkdir(parents=True, exist_ok=True)
    trace = headless_trace(chain, out, Path(args.fnvtool), Path(args.data))
    # A home of its own for each slot, with the start save in it.
    home = Path(args.home) / f"batch-{slot}"
    saves = home / "saves"
    saves.mkdir(parents=True, exist_ok=True)
    start = Path(args.home) / "saves" / f"{args.start}.nvfsave"
    if start.exists():
        shutil.copy2(start, saves / start.name)
    cmd = [str(args.exe), "--data", str(args.data), "--play", "--no-voice", "--no-music", "--run-seconds", str(args.minutes * 60)]
    cmd += ["--load", args.start] if start.exists() else ["--new-game"]
    cmd += ["--pilot", str(chain), "--pilot-campaign", "--pilot-given", "--pilot-out", str(report)]
    if trace:
        cmd += ["--pilot-expect", str(trace)]
    if args.save_load:
        cmd += ["--pilot-save-load"]
    known = Path(__file__).with_name("known-differences.json")
    if known.exists():
        cmd += ["--pilot-known", str(known)]
    env = dict(os.environ, NVFOREVER_HOME=str(home), **AIDS)
    env.pop("NVF_REPORTS", None)
    report.unlink(missing_ok=True)
    began = time.monotonic()
    with open(log, "w", encoding="utf-8", errors="replace") as f:
        try:
            code = subprocess.run(cmd, stdout=f, stderr=subprocess.STDOUT, env=env, timeout=args.minutes * 60 + 300).returncode
        except subprocess.TimeoutExpired:
            code = None
    facts = {"chain": name, "exit": code, "seconds": round(time.monotonic() - began), "trace": bool(trace), "log": str(log)}
    if report.exists():
        try:
            facts["report"] = json.loads(report.read_text(encoding="utf-8"))
        except json.JSONDecodeError as e:
            facts["error"] = f"report unreadable: {e}"
    else:
        facts["error"] = "no report (the run crashed, timed out or never reached the end)"
    return facts


def summarise(results: list[dict]) -> list[dict]:
    """One row a chain: counts, finding kinds, the weight that ranks it (worst first)."""
    rows = []
    for r in results:
        rep = r.get("report") or {}
        steps = rep.get("steps", [])
        findings = rep.get("findings", [])
        kinds: dict[str, int] = {}
        for f in findings:
            kinds[f["kind"]] = kinds.get(f["kind"], 0) + 1
        failed = sum(1 for s in steps if "error" in s)
        softlocks = rep.get("softlocks", 0)
        weight = softlocks * WEIGHTS["softlock"] + sum(WEIGHTS.get(k, 1) * n for k, n in kinds.items()) + 2 * failed + 3 * len(rep.get("divergences", []))
        if "error" in r:
            weight += 100
        rows.append({
            "chain": r["chain"],
            "weight": weight,
            "steps": len(steps),
            "failed": failed,
            "divergences": len(rep.get("divergences", [])),
            "softlocks": softlocks,
            "findings": kinds,
            "headless": rep.get("headless", {}),
            "seconds": r.get("seconds"),
            "error": r.get("error"),
            "first": [f"{f['kind']} (step {f.get('step')}): {f.get('text', '')[:200]}" for f in findings if f["kind"] in ("no_way_out", "unreachable", "never_came", "chain_abandoned", "pilot_held")][:3],
        })
    rows.sort(key=lambda x: (-x["weight"], x["chain"]))
    return rows


def markdown(rows: list[dict]) -> str:
    lines = [
        "# Quest pilot batch",
        "",
        f"{len(rows)} chains. Worst first; weight = soft locks and findings that stop a quest above the pilot's own trouble.",
        "",
        "| chain | weight | steps | failed | divergences | soft locks | findings | done the headless way |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for r in rows:
        kinds = ", ".join(f"{k} {n}" for k, n in sorted(r["findings"].items())) or "-"
        headless = ", ".join(f"{k} {n}" for k, n in sorted(r["headless"].items())) or "-"
        lines.append(f"| {r['chain']} | {r['weight']} | {r['steps']} | {r['failed']} | {r['divergences']} | {r['softlocks']} | {kinds} | {headless} |")
    lines += ["", "## What stops a quest", ""]
    for r in rows:
        if r["error"] or r["first"]:
            lines.append(f"- **{r['chain']}**: " + "; ".join(([r["error"]] if r["error"] else []) + r["first"]))
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--exe", required=True)
    ap.add_argument("--data", required=True)
    ap.add_argument("--fnvtool", required=True)
    ap.add_argument("--home", required=True, help="the pilot's NVForever home (never the player's): holds the start save")
    ap.add_argument("--start", default="pilot-after-intro", help="save to start each chain from (in <home>/saves); --new-game when absent")
    ap.add_argument("--out", default=str(REPO / "target" / "pilot-batch"))
    ap.add_argument("--jobs", type=int, default=2)
    ap.add_argument("--minutes", type=int, default=40)
    ap.add_argument("--only", nargs="*", default=[])
    ap.add_argument("--skip-fail", action="store_true", help="leave out the fail-* chains (a quest's failure paths)")
    ap.add_argument("--save-load", action="store_true", help="the game's state through a save and a load before each milestone")
    ap.add_argument("--resume", action="store_true", help="keep the chains that already have a report in --out; run the others")
    args = ap.parse_args(argv)
    todo = [c for c in chains(args.only) if not (args.skip_fail and c.name.startswith("fail-"))]
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    results: list[dict] = []
    if args.resume:
        kept = [c for c in todo if (out / "reports" / f"{name_of(c)}.report.json").exists()]
        for c in kept:
            rep = json.loads((out / "reports" / f"{name_of(c)}.report.json").read_text(encoding="utf-8"))
            results.append({"chain": name_of(c), "report": rep, "kept": True})
        todo = [c for c in todo if c not in kept]
        print(f"pilot batch: {len(kept)} chains kept from an earlier run", flush=True)
    print(f"pilot batch: {len(todo)} chains, {args.jobs} at once, at most {args.minutes} min each; out {out}", flush=True)
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        free = list(range(args.jobs))
        futures = {}
        pending = list(todo)
        while pending or futures:
            while pending and free:
                slot = free.pop()
                chain = pending.pop(0)
                futures[pool.submit(run_one, chain, args, slot)] = slot
            done, _ = concurrent.futures.wait(futures, return_when=concurrent.futures.FIRST_COMPLETED)
            for fut in done:
                free.append(futures.pop(fut))
                r = fut.result()
                results.append(r)
                rep = r.get("report") or {}
                print(f"{r['chain']}: {len(rep.get('steps', []))} steps, {len(rep.get('findings', []))} findings, {len(rep.get('divergences', []))} divergences, {rep.get('softlocks', 0)} soft locks{' — ' + r['error'] if 'error' in r else ''}", flush=True)
                rows = summarise(results)
                (out / "summary.json").write_text(json.dumps(rows, indent=1), encoding="utf-8")
                (out / "summary.md").write_text(markdown(rows), encoding="utf-8")
    print(f"pilot batch: summary in {out / 'summary.md'}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
