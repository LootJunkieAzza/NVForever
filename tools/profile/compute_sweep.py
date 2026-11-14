"""Compute-pool sweep: the same campaign scenarios at several sizes of Bevy's compute task
pool, to see how frame time, slow frames and CPU use follow it on this machine.

Each size is one launch of every chosen scenario through tools/profile/campaign.py, with the
test aid `NVF_PLAY_TEST_COMPUTE_THREADS=<n>` (engine/nvforever/src/main.rs `task_pools`).
Sizes are run interleaved, round by round, so a slow drift (heat, background work) spreads
over all of them. Works with a default release build or a `profile` build.

    python tools/profile/compute_sweep.py --exe <nvforever.exe> --data <Data> \
        [--threads 4 8 12 16 24] [--rounds 2] [--only goodsprings-noon novac-morning battle-10] \
        [--placement auto|off]

`--placement off` passes `--cpu-cache-placement off` (cpu_placement.rs) to every launch, to
measure the whole processor on a machine where the game would otherwise restrict itself.
Runs land in target/profile-campaign/compute-sweep-<n>-r<round>/ (game-derived logs: keep
them out of the repository); the table at the end gives, per size and scenario, the mean
frame, the slowest frame, the frames over the scenario's slow limit and the CPU time a frame
over every thread, averaged over the rounds.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
DEFAULT_SCENARIOS = ["goodsprings-noon", "novac-morning", "battle-10", "run-goodsprings"]


def toml_value(v) -> str:
    """A TOML literal for the value types campaign.toml uses."""
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, str):
        return json.dumps(v)
    if isinstance(v, (int, float)):
        return repr(v)
    if isinstance(v, list):
        return "[" + ", ".join(toml_value(x) for x in v) + "]"
    if isinstance(v, dict):
        return "{ " + ", ".join(f"{k} = {toml_value(x)}" for k, x in v.items()) + " }"
    raise TypeError(f"no TOML form for {v!r}")


def sweep_campaign(src: dict, names: list[str], threads: int, placement_off: bool) -> str:
    """campaign.toml's defaults and the named scenarios, each with the compute-pool aid set
    to `threads` (and placement switched off when asked), as TOML text."""
    by_name = {s["name"]: s for s in src["scenario"]}
    missing = [n for n in names if n not in by_name]
    if missing:
        raise SystemExit(f"no such scenario in campaign.toml: {', '.join(missing)}")
    out = ["[defaults]"] + [f"{k} = {toml_value(v)}" for k, v in src.get("defaults", {}).items()] + [""]
    for name in names:
        sc = dict(by_name[name])
        aids = dict(sc.get("aids", {}))
        aids["NVF_PLAY_TEST_COMPUTE_THREADS"] = str(threads)
        sc["aids"] = aids
        if placement_off:
            sc["args"] = ["--cpu-cache-placement", "off"] + list(sc.get("args", []))
        out.append("[[scenario]]")
        out += [f"{k} = {toml_value(v)}" for k, v in sc.items()]
        out.append("")
    return "\n".join(out)


def summarise(runs: dict[int, list[Path]], names: list[str]) -> str:
    """The table: per size and scenario, means over the rounds that finished."""
    lines = ["threads  scenario              frame ms  slowest ms  slow frames  CPU ms/frame"]
    for n in sorted(runs):
        for name in names:
            rows = []
            for folder in runs[n]:
                path = folder / "results.json"
                if not path.is_file():
                    continue
                for s in json.loads(path.read_text(encoding="utf-8")).get("scenarios", []):
                    if s.get("name") == name and s.get("status") == "ok":
                        h = s.get("hitches")
                        h = h if isinstance(h, (int, float)) else len(h or [])
                        rows.append((s["frame_mean"], s["slowest_frame"], h, s.get("cpu_ms") or 0.0))
            if not rows:
                lines.append(f"{n:>7}  {name:<20}  (no finished run)")
                continue
            k = len(rows)
            mean = [sum(r[i] for r in rows) / k for i in range(4)]
            lines.append(f"{n:>7}  {name:<20}  {mean[0]:8.2f}  {mean[1]:10.1f}  {mean[2]:11.1f}  {mean[3]:12.1f}")
    return "\n".join(lines)


def main(argv=None) -> int:
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    p.add_argument("--exe", required=True, help="nvforever.exe (release or profile build)")
    p.add_argument("--data", required=True, help="the game's Data folder")
    p.add_argument("--threads", type=int, nargs="+", default=[4, 8, 12, 16, 24])
    p.add_argument("--rounds", type=int, default=2)
    p.add_argument("--only", nargs="+", default=DEFAULT_SCENARIOS, help="campaign scenarios to run")
    p.add_argument("--placement", choices=["auto", "off"], default="auto")
    p.add_argument("--campaign", type=Path, default=HERE / "campaign.toml")
    args = p.parse_args(argv)

    src = tomllib.loads(args.campaign.read_text(encoding="utf-8"))
    out_dir = ROOT / "target" / "profile-campaign"
    cfg_dir = out_dir / "compute-sweep-configs"
    cfg_dir.mkdir(parents=True, exist_ok=True)
    runs: dict[int, list[Path]] = {n: [] for n in args.threads}
    for r in range(1, args.rounds + 1):
        # Alternate the order each round so neither end of the list always runs first.
        order = args.threads if r % 2 else list(reversed(args.threads))
        for n in order:
            cfg = cfg_dir / f"compute-{n}.toml"
            cfg.write_text(sweep_campaign(src, args.only, n, args.placement == "off"), encoding="utf-8")
            name = f"compute-sweep-{n}-r{r}"
            print(f"round {r}: compute pool {n}", flush=True)
            subprocess.call([sys.executable, str(HERE / "campaign.py"), "run", "--exe", args.exe, "--data", args.data,
                             "--campaign", str(cfg), "--name", name])
            runs[n].append(out_dir / name)
    print()
    print(summarise(runs, args.only))
    return 0


if __name__ == "__main__":
    sys.exit(main())
