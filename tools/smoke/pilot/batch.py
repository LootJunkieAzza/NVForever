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
import re
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
    "target_unreachable": 20,
    "never_came": 20,
    "chain_abandoned": 20,
    "player_died": 20,
    "pilot_held": 20,
    "could_not_target": 10,
    "line_not_said": 8,
    "no_conversation": 8,
    "item_not_found": 6,
    "wait_timed_out": 6,
    "step_timed_out": 5,
    "fight_not_won": 3,
    "detour": 1,
    "run_ended": 10,
    "ai_kill_not_done": 8,
    "stuck": 2,
}


def chains(only: list[str], extra: list[str] | None = None) -> list[Path]:
    """The chain files, all or the named ones (name = file name without `.actions.json`):
    the New Vegas ones, and those of each `--chains-dir` (the TTW ones in `walkthrough/ttw`)."""
    every = sorted(CHAINS.glob("*.actions.json"))
    for d in extra or []:
        every += sorted(Path(d).glob("*.actions.json"))
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


def in_order(todo: list[Path]) -> list[Path]:
    """Each quest's main chain before its variants (`bye-bye-love` before
    `bye-bye-love-dawn`): variants share their main chain's opening, so run side by side
    they stall at the same step; after all the main chains they start once the opening
    has been tried."""
    names = {name_of(c) for c in todo}

    def variant(c: Path) -> bool:
        n = name_of(c)
        return any(n.startswith(m + "-") for m in names if m != n)

    return sorted(todo, key=lambda c: (variant(c), name_of(c)))


_WORD = re.compile(r"(?<![0-9A-Za-z])(0x)?([0-9A-Fa-f]{8})(?![0-9A-Za-z])")


def remap_ids(value, index: list[int | None]):
    """Each 8-hex-digit word whose top byte is a chain plugin's index gets that plugin's
    index in the run's load order (the pilot's `remap_ids`, for the headless trace)."""
    if isinstance(value, str):
        def sub(m: re.Match) -> str:
            word = int(m.group(2), 16)
            top = word >> 24
            to = index[top] if 0 < top < len(index) else None
            return m.group(0) if to is None else (m.group(1) or "") + f"{(to << 24) | (word & 0xFFFFFF):08X}"
        return _WORD.sub(sub, value)
    if isinstance(value, list):
        return [remap_ids(v, index) for v in value]
    if isinstance(value, dict):
        return {k: remap_ids(v, index) for k, v in value.items()}
    return value


def for_load_order(chain: Path, out: Path, order: list[str]) -> Path:
    """The chain with its FormIDs moved to `order` (a mod list's: Tale of Two Wastelands
    puts the DLC masters at other indexes), written under `out/chains`; `plugins` is the
    whole order, so the headless run and the game read it the same way."""
    raw = json.loads(chain.read_text(encoding="utf-8"))
    own = raw.get("plugins") or ["FalloutNV.esm"]
    low = [o.lower() for o in order]
    index = [low.index(n.lower()) if n.lower() in low else None for n in own]
    moved = remap_ids(raw, index)
    moved["plugins"] = order
    dst = out / "chains" / chain.name
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_text(json.dumps(moved, indent=1), encoding="utf-8")
    return dst


def headless_trace(chain: Path, out: Path, fnvtool: Path, data: Path) -> Path | None:
    """The chain's headless trace, made again when the chain or fnvtool is newer."""
    trace = out / "traces" / f"{name_of(chain)}.trace.json"
    newest = max(chain.stat().st_mtime, fnvtool.stat().st_mtime)
    if trace.exists() and trace.stat().st_mtime >= newest:
        return trace
    trace.parent.mkdir(parents=True, exist_ok=True)
    # The chain's own plugins (a DLC chain's FormIDs count from them), the base game alone
    # otherwise.
    plugins = json.loads(chain.read_text(encoding="utf-8")).get("plugins") or ["FalloutNV.esm"]
    cmd = [str(fnvtool), "script", "run", str(data), *plugins, "--schema", str(REPO / "schemas" / "fnv"), "--actions", str(chain)]
    with open(trace, "w", encoding="utf-8") as f:
        code = subprocess.run(cmd, stdout=f, stderr=subprocess.DEVNULL, cwd=REPO).returncode
    return trace if code == 0 else None


def load_order(args: argparse.Namespace) -> list[str]:
    """`--plugins-file`'s plugins in order, or none (each chain's own)."""
    if not getattr(args, "plugins_file", None):
        return []
    return [line.strip() for line in Path(args.plugins_file).read_text(encoding="utf-8").splitlines() if line.strip() and not line.startswith("#")]


def run_one(chain: Path, args: argparse.Namespace, slot: int) -> dict:
    """One chain in the game: its report (or what went wrong), with the run's facts."""
    name = name_of(chain)
    out = Path(args.out)
    report = out / "reports" / f"{name}.report.json"
    log = out / "logs" / f"{name}.log"
    report.parent.mkdir(parents=True, exist_ok=True)
    log.parent.mkdir(parents=True, exist_ok=True)
    order = load_order(args)
    if order:
        chain = for_load_order(chain, out, order)
    trace = headless_trace(chain, out, Path(args.fnvtool), Path(args.data))
    # A home of its own for each slot, with the start save in it.
    home = Path(args.home) / f"batch-{slot}"
    saves = home / "saves"
    saves.mkdir(parents=True, exist_ok=True)
    start = Path(args.home) / "saves" / f"{args.start}.nvfsave"
    if start.exists():
        shutil.copy2(start, saves / start.name)
    cmd = [str(args.exe), "--data", str(args.data), "--play", "--no-voice", "--no-music", "--run-seconds", str(args.minutes * 60)]
    # A small window, placed, so several batches can be watched side by side.
    if getattr(args, "size", ""):
        cmd += ["--size", args.size]
    if getattr(args, "window_at", ""):
        cmd += [f"--window-at={args.window_at}"]
    # A chain that names its own start ("save": the intro and new-game chains) begins there:
    # that save if the home has it, else a new game. The intro run from the after-intro
    # save found its quests long past and was abandoned (sweep 2, 2026-11-22).
    own = json.loads(chain.read_text(encoding="utf-8")).get("save")
    if own:
        mine = Path(args.home) / "saves" / f"{own}.nvfsave"
        if mine.exists():
            shutil.copy2(mine, saves / mine.name)
        cmd += ["--load", own] if mine.exists() else ["--new-game"]
    else:
        cmd += ["--load", args.start] if start.exists() else ["--new-game"]
    if order:
        cmd += ["--plugins", *order]
    cmd += ["--pilot", str(chain), "--pilot-campaign", "--pilot-given", "--pilot-out", str(report), "--pilot-minutes", str(args.minutes)]
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
            "first": [f"{f['kind']} (step {f.get('step')}): {f.get('text', '')[:200]}" for f in findings if f["kind"] in ("no_way_out", "unreachable", "target_unreachable", "never_came", "chain_abandoned", "player_died", "pilot_held")][:3],
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
    ap.add_argument("--size", default="640x360", help="the game window's size, WxH (small, to watch several batches at once); empty for the game's own")
    ap.add_argument("--window-at", default="", help="where the game window's top-left corner goes, X,Y on the desktop (a batch each its own place)")
    ap.add_argument("--start", default="pilot-after-intro", help="save to start each chain from (in <home>/saves); empty for a new game. A named save that is missing stops the batch")
    ap.add_argument("--out", default=str(REPO / "target" / "pilot-batch"))
    ap.add_argument("--jobs", type=int, default=2)
    ap.add_argument("--minutes", type=int, default=40)
    ap.add_argument("--only", nargs="*", default=[])
    ap.add_argument("--skip-fail", action="store_true", help="leave out the fail-* chains (a quest's failure paths)")
    ap.add_argument("--save-load", action="store_true", help="the game's state through a save and a load before each milestone")
    ap.add_argument("--chains-dir", action="append", default=[], help="another folder of chains, beside crates/fnv-game/walkthrough (e.g. its ttw/ folder for Tale of Two Wastelands)")
    ap.add_argument("--plugins-file", help="a load order of its own, one plugin a line (a mod list merged by mo2_merge.py: Tale of Two Wastelands); the chains' FormIDs are moved to it")
    ap.add_argument("--resume", action="store_true", help="keep the chains that already have a report in --out; run the others")
    args = ap.parse_args(argv)
    # A missing start save once let a batch fall back to a new game without a word: every
    # chain then began in Doc Mitchell's house with the controls held (2026-11-17).
    if args.start and not (Path(args.home) / "saves" / f"{args.start}.nvfsave").exists():
        print(f"pilot batch: no start save {Path(args.home) / 'saves' / (args.start + '.nvfsave')}: copy one in, or pass --start '' for a new game", file=sys.stderr)
        return 2
    todo = [c for c in chains(args.only, args.chains_dir) if not (args.skip_fail and c.name.startswith("fail-"))]
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
    todo = in_order(todo)
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
