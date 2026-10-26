#!/usr/bin/env python3
"""Run the extender probe quests unattended (tools/capture/ext-probes/), one job each.

  ext_probe_run.py --game <FNV folder> --save NAME [--sets strings arrays …] [--risky]

For every probe quest script in the sets (quest = script name + "Q"; a selector script,
one with `iWhich`, gives one job per probe number) it writes a capture job
(fnv.capture.steps/1, docs/spec-notes/xnvse-capture.md §4) that turns on the console log
with SCOF ("Creates a text file that logs console input and output" [documented:
SetConsoleOuputFile]), starts the quest, and waits. capture_run.py then starts the
original game through nvse_loader.exe, loads the save, runs the jobs and quits.

A crash ends a launch: the job that was running is recorded as crashed and dropped, and
the game is started again for the jobs left. At the end each set's console logs are
collected with ext_probes.py into <goldens>/ext/<set>.json and compared with the set's
README (ext-probes/<set>/README.md).

The loops runaway script and the deep-recursion caller (expected to hang or crash) only
run with --risky. Everything this writes is under the capture directory and the goldens
directory; the game folder and the Saves folder are only read (capture_run.py backs up the
rolling saves). Standard library only.
"""
import argparse
import json
import os
import re
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import capture_run as cr  # noqa: E402
import capture_steps as cs  # noqa: E402
import ext_probes as ep  # noqa: E402

PROBES = os.path.join(HERE, "ext-probes")
SETS = ["strings", "expressions", "arrays", "loops", "udf"]
RISKY = {"ExtProbeLoops09", "ExtProbeUdf05"}
# Game loops to wait after starting a quest: its script runs within the default 5 s
# processing delay (the quests carry that flag); loops/02 adds a second frame.
WAIT_FRAMES = 600
REPORT_FORMAT = "fnv.ext.proberun/1"


def probe_scripts(set_name, risky=False):
    """[(script name, branch count or 0)] of a set's quest scripts, in file order."""
    out = []
    d = os.path.join(PROBES, set_name)
    for f in sorted(os.listdir(d)):
        if not f.endswith(".txt"):
            continue
        with open(os.path.join(d, f), encoding="utf-8") as fh:
            text = fh.read()
        m = re.match(r"scn (\w+)", text)
        if not m:
            continue
        name = m.group(1)
        if name in RISKY and not risky:
            continue
        branches = len(re.findall(r"^\s*(?:if|elseif) iWhich == \d+", text, re.M))
        out.append((name, branches))
    return out


def jobs_for(sets, log_dir, risky=False):
    """[(job name, set, steps)] in run order (job names sort in that order)."""
    jobs, n = [], 0
    for s in sets:
        for script, branches in probe_scripts(s, risky):
            quest = script + "Q"
            for k in (range(1, branches + 1) if branches else [None]):
                n += 1
                tag = f"{script}.{k}" if k else script
                name = f"ext_{n:03d}_{tag.replace('.', '_')}"
                log = os.path.join(log_dir, f"{name}.console.txt")
                steps = [{"console": f'scof "{log}"'}, {"console": f"StartQuest {quest}"}]
                if k:
                    steps += [{"frames": WAIT_FRAMES // 2}, {"console": f"set {quest}.iWhich to {k}"}]
                steps.append({"frames": WAIT_FRAMES})
                jobs.append((name, s, steps))
    return jobs


def write_job(path_dir, name, steps):
    job = {"format": cs.STEPS_FORMAT, "name": name, "title": f"Extender probe {name}",
           "start": "the save given to ext_probe_run.py", "steps": steps}
    path = os.path.join(path_dir, f"{name}.steps.json")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(job, f, indent=1)
        f.write("\n")
    return path


def run_all(jobs, job_dir, capture_dir, game, save, timeout, launch=cr.run, max_launches=200):
    """Launch until every job has ended or crashed. Returns {job: "ended" | "crashed" | …}."""
    status = {name: None for name, _, _ in jobs}
    paths = {name: write_job(job_dir, name, steps) for name, _, steps in jobs}
    launches = 0
    while any(v is None for v in status.values()) and launches < max_launches:
        launches += 1
        todo = [n for n, v in status.items() if v is None]
        planned = {n: (paths[n], None) for n in todo}
        report = launch(planned, sorted(todo), capture_dir, game, save, timeout, kill_on_timeout=True)
        states = report.get("jobs", {})
        for n in todo:
            if states.get(n, {}).get("status") == "ended":
                status[n] = "ended"
        left = [n for n in sorted(todo) if status[n] is None]
        if left:
            # The first job that did not end was the one running when the game stopped.
            first = left[0]
            problem = "; ".join(report.get("problems", [])) or "did not end"
            status[first] = "crashed" if "exited" in problem else f"failed: {problem}"
            stale = os.path.join(capture_dir, "jobs", f"{first}.steps.json")
            if os.path.exists(stale):
                os.remove(stale)          # do not run it again next launch
    return status, launches


def collect(jobs, log_dir, goldens, version):
    """Per set: collect its console logs into <goldens>/ext/<set>.json and compare."""
    results = {}
    for s in sorted({j[1] for j in jobs}):
        logs = [os.path.join(log_dir, f"{n}.console.txt") for n, js, _ in jobs if js == s]
        logs = [p for p in logs if os.path.exists(p)]
        if not logs:
            results[s] = {"logs": 0}
            continue
        doc = ep.collect(logs, s, version)
        out = os.path.join(goldens, "ext", f"{s}.json")
        os.makedirs(os.path.dirname(out), exist_ok=True)
        with open(out, "w", encoding="utf-8") as f:
            json.dump(doc, f, indent=1, ensure_ascii=False)
            f.write("\n")
        with open(os.path.join(PROBES, s, "README.md"), encoding="utf-8") as f:
            cmp = ep.compare(doc, ep.readme_expected(f.read()))
        results[s] = {"logs": len(logs), "golden": out, "rows": len(doc["rows"]), "compare": cmp}
    return results


def main(argv=None, game=None, launch=cr.run):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--game", required=True, help="the game folder (holds nvse_loader.exe); only read")
    ap.add_argument("--save", required=True, help="save to load (file name without .fos)")
    ap.add_argument("--sets", nargs="+", default=SETS, choices=SETS)
    ap.add_argument("--risky", action="store_true", help="also run the runaway loop and deep recursion")
    ap.add_argument("--capture-dir", default=cr.DEFAULT_CAPTURE_DIR)
    ap.add_argument("--goldens", default=None, help="goldens directory (default: ext_probes.py's)")
    ap.add_argument("--version", default="6.4.9", help="the xNVSE version, recorded in the goldens")
    ap.add_argument("--timeout", type=int, default=900, help="seconds per launch (default 900)")
    ap.add_argument("--dry-run", action="store_true", help="only write the jobs and print the plan")
    ap.add_argument("-o", "--output", help="report file (default: stdout)")
    a = ap.parse_args(argv)
    base = os.path.join(a.capture_dir, "ext")
    job_dir, log_dir = os.path.join(base, "jobs"), os.path.join(base, "console")
    for d in (job_dir, log_dir):
        os.makedirs(d, exist_ok=True)
    jobs = jobs_for(a.sets, log_dir, a.risky)
    if a.dry_run:
        for name, s, steps in jobs:
            write_job(job_dir, name, steps)
        print(json.dumps({"jobs": [n for n, _, _ in jobs], "job_dir": job_dir}, indent=1))
        return 0
    game = game or cr.Game(a.game)
    problem = game.check() or cr.check_save(a.save, os.path.join(os.path.expanduser("~"), "Documents", "My Games", "FalloutNV", "Saves"))
    if problem:
        print(f"ext_probe_run: {problem}", file=sys.stderr)
        return 1
    if game.running():
        print("ext_probe_run: the game is already running; close it first", file=sys.stderr)
        return 1
    if cr.pending_jobs(a.capture_dir):
        print(f"ext_probe_run: other jobs are pending in {os.path.join(a.capture_dir, 'jobs')}; move them away first", file=sys.stderr)
        return 1
    status, launches = run_all(jobs, job_dir, a.capture_dir, game, a.save, a.timeout, launch)
    goldens = a.goldens or str(ep.default_goldens())
    report = {"format": REPORT_FORMAT, "save": a.save, "launches": launches,
              "crashed": [n for n, v in status.items() if v == "crashed"],
              "failed": {n: v for n, v in status.items() if v not in ("ended", "crashed")},
              "sets": collect(jobs, log_dir, goldens, a.version)}
    cs.write(report, a.output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
