#!/usr/bin/env python3
"""Script-function probes: the same calls in the original game and in ours, lined up.

A probe file (`fnv.function.probes/1`, TOML) names forms and lists batches of steps. Each
batch becomes one `fnv.capture.steps/1` job for the nvf_capture plugin (run unattended in
the original game by capture_run.py) and one `fnv.script.actions/1` input for our
headless runner (`fnvtool script run`, its `call` action). Both answer under the same
labels, so the two can be compared function by function.

  function_probe.py jobs <probes.toml> -o <dir>
      write <batch>.steps.json (game), <batch>.actions.json (ours) and probes.plan.json
  function_probe.py ours <dir> --fnvtool EXE --data DIR [--schema DIR] [--only BATCH...]
      run our side of every batch in <dir>: <batch>.trace.json
  function_probe.py game <dir> --game DIR [--save nvftest] [--only BATCH...]
      take the game locks, run every batch's job in the original game, and copy each
      job's output to <dir>/<batch>.game.jsonl
  function_probe.py compare <dir> [-o report.md] [--json report.json]
      line up the game's and our answers per probe (same / differs / errors)

Step kinds in a batch (`steps = [...]`; `@name` is a form from `[forms]`):
  {ask = "@sunny.GetDisposition player"}    a query (label: the text, or `label`)
  {ask = "...", ref = true}                 a query whose answer is a reference
  {do = "@ringo.SetAlert 1"}                a statement on both sides
  {frames = 60}                             game loops / our frames
  {console = "tgm"}                         a console line, game side only
  {until = "@x.HasLoaded3D", max_frames = 600, label = "..."}  wait until non-zero
  {moveto = "@sunny", cell = "GoodspringsWorld"}  player.MoveTo, then wait until the
      target has its 3D in game; ours moves the player and loads `cell` (optional)
  {coc = "GSGasStation", cell = "@gsgas"}   console coc, wait until in that cell
  {game = "<script line>", ours = "<call line>", label = "..."}  a query written per side
Any step may say `side = "game"` or `side = "ours"` to run on one side only, and a query
`fn = "Name"` when the function it probes is not the first word, and `note`.
A batch has `name`, `title`, `start` (the save it expects) and `ours_start` (runner
actions standing in for the save, e.g. `{op = "go_to_cell", cell = "GSGasStation"}`).

Game-side scripts follow the forms that compiled in game (xnvse-capture.md section 5):
refs declared before `Begin Function`, built with BuildRef from GetModIndex and the
decimal local FormID, numbers without a decimal point, every statement script ending with
`SetFunctionValue 1`. Standard library only.
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
import tomllib

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import capture_steps as cs  # noqa: E402

PROBES_FORMAT = "fnv.function.probes/1"
PLAN_FORMAT = "fnv.function.probe_plan/1"
ACTIONS_FORMAT = "fnv.script.actions/1"
REPO = os.path.dirname(os.path.dirname(HERE))
FUNCTIONS_JSON = os.path.join(REPO, "schemas", "script", "functions.json")
SETTLE = 10  # game loops after a statement (capture_steps.SETTLE_FRAMES)
LOAD_FRAMES = 1200  # at most this many loops for a target's 3D after MoveTo
AFTER_LOAD = 60
COC_FRAMES = 3600  # capture_steps.DOOR_LOOPS
AFTER_COC = 300  # capture_steps.AFTER_DOOR_FRAMES
TOLERANCE = 1e-3
STEP_KINDS = ("ask", "do", "frames", "console", "until", "moveto", "coc", "game", "screenshot")
HUD_FRAMES = 30  # loops between the HUD toggle and the picture (capture_frames.HUD_FRAMES)


class ProbeError(Exception):
    pass


# --------------------------------------------------------------------------- input

def load_probes(path):
    with open(path, "rb") as f:
        doc = tomllib.load(f)
    if doc.get("format") != PROBES_FORMAT:
        raise ProbeError(f"{path}: not a {PROBES_FORMAT} file")
    return doc


def function_names(path=FUNCTIONS_JSON):
    """Lower-case name or alias -> catalogue name, from schemas/script/functions.json."""
    with open(path, encoding="utf-8") as f:
        doc = json.load(f)
    out = {}
    for fn in doc["functions"]:
        for n in [fn["name"]] + list(fn.get("aliases") or []):
            out.setdefault(n.lower(), fn["name"])
    return out


def parse_form(spec, masters):
    """'Plugin.esm:00104C7D' -> (plugin, local FormID). The plugin must be a master."""
    if not isinstance(spec, str) or ":" not in spec:
        raise ProbeError(f"form {spec!r}: want 'Plugin.esm:<hex FormID>'")
    plugin, hexid = spec.rsplit(":", 1)
    match = [m for m in masters if m.lower() == plugin.lower()]
    if not match:
        raise ProbeError(f"form {spec!r}: {plugin} is not one of the masters")
    try:
        local = int(hexid, 16) & 0xFFFFFF
    except ValueError as e:
        raise ProbeError(f"form {spec!r}: bad FormID") from e
    return match[0], local


WORD = re.compile(r'"[^"]*"|[^\s,]+')


def words(text):
    return WORD.findall(text)


def split_call(text):
    """(subject or None, function word, argument words) of `[subject.]Function args`."""
    w = words(text)
    if not w:
        raise ProbeError("an empty call")
    head = w[0]
    if "." in head and not head.startswith('"'):
        subj, func = head.rsplit(".", 1)
        return subj, func, w[1:]
    return None, head, w[1:]


# --------------------------------------------------------------------------- the game side

class GameRefs:
    """`@name` -> ref variables of one game script (cf. capture_steps.Names)."""

    def __init__(self, forms, masters):
        self.forms, self.masters = forms, masters
        self.decls, self.setup, self.mods, self.vars = [], [], {}, {}

    def ref(self, name):
        if name in self.vars:
            return self.vars[name]
        if name not in self.forms:
            raise ProbeError(f"@{name} is not in [forms]")
        plugin, local = parse_form(self.forms[name], self.masters)
        if plugin not in self.mods:
            m = f"fM{len(self.mods) + 1}"
            self.mods[plugin] = m
            self.decls.append(f"float {m}")
            self.setup.append(f'let {m} := GetModIndex "{plugin}"')
        var = f"rF{len(self.vars) + 1}"
        self.vars[name] = var
        self.decls.append(f"ref {var}")
        self.setup.append(f"let {var} := BuildRef {self.mods[plugin]} {local}")
        return var

    def text(self, s):
        return re.sub(r"@([A-Za-z0-9_]+)", lambda m: self.ref(m.group(1)), s)


def game_query(text, forms, masters, ref=False):
    g = GameRefs(forms, masters)
    expr = g.text(text)
    if ref:
        return cs.function(g.decls + ["ref rScan"], g.setup + [f"let rScan := {expr}", "SetFunctionValue (GetFormIDString rScan)"])
    return cs.function(g.decls + ["float fV"], g.setup + [f"let fV := {expr}", "SetFunctionValue fV"])


def game_statement(text, forms, masters):
    g = GameRefs(forms, masters)
    lines = [g.text(t) for t in (text if isinstance(text, list) else [text])]
    return cs.function(g.decls, g.setup + lines, returns_value=False)


def game_until(text, forms, masters):
    g = GameRefs(forms, masters)
    cond = g.text(text)  # declares the refs first
    return cs.function(g.decls + ["float fV"], g.setup + [f"if {cond}", "  let fV := 1", "endif", "SetFunctionValue fV"])


def game_in_cell(cell, forms, masters):
    g = GameRefs(forms, masters)
    c = g.text(cell)
    return cs.function(g.decls + ["ref rP", "float fV"],
                       g.setup + ["let rP := player.GetParentCell", f"if rP == {c}", "  let fV := 1", "endif", "SetFunctionValue fV"])


# --------------------------------------------------------------------------- our side

def our_text(text, forms, masters):
    """`@name` -> the load-order FormID (0xIIxxxxxx) of our run with the same masters."""
    def sub(m):
        name = m.group(1)
        if name not in forms:
            raise ProbeError(f"@{name} is not in [forms]")
        plugin, local = parse_form(forms[name], masters)
        return f"0x{(masters.index(plugin) << 24) | local:08X}"
    return re.sub(r"@([A-Za-z0-9_]+)", sub, text)


# --------------------------------------------------------------------------- batches

def check_function(text, fnames, where):
    if fnames is None:
        return None
    _, func, _ = split_call(text)
    name = fnames.get(func.lower())
    if name is None:
        raise ProbeError(f"{where}: {func!r} is not a function in the catalogue")
    return name


def build_batch(batch, forms, masters, fnames=None):
    """(game job, our actions, plan rows) of one batch."""
    name = batch.get("name") or ""
    if not re.fullmatch(r"[A-Za-z0-9_.-]+", name):
        raise ProbeError(f"batch name {name!r} must be [A-Za-z0-9_.-]+")
    head = [cs.query(cs.modindex_query(m), cs.MODINDEX_LABEL, m) for m in masters]
    gsteps, ours, plan = list(head), [], []
    labels = set()

    def label_of(step, default):
        lab = step.get("label") or default
        if lab in labels:
            raise ProbeError(f"{name}: label {lab!r} used twice")
        labels.add(lab)
        return lab

    for i, step in enumerate(batch.get("steps", [])):
        where = f"{name} step {i}"
        kinds = [k for k in STEP_KINDS if k in step]
        if len(kinds) != 1 and not (kinds == ["game"] or set(kinds) == {"game"}):
            raise ProbeError(f"{where}: want exactly one of {', '.join(STEP_KINDS)}, got {kinds}")
        kind = kinds[0]
        side = step.get("side", "both")
        if side not in ("both", "game", "ours"):
            raise ProbeError(f"{where}: side must be both, game or ours")
        on_game, on_ours = side in ("both", "game"), side in ("both", "ours")
        row = None
        if kind == "ask":
            text = step["ask"]
            fn = step.get("fn") or check_function(text, fnames, where)
            lab = label_of(step, text)
            row = {"label": lab, "fn": fn, "text": text, "kind": "ask", "note": step.get("note", ""),
                   "tolerance": step.get("tolerance", TOLERANCE), "ref": bool(step.get("ref"))}
            if on_game:
                row["game_step"] = len(gsteps)
                gsteps.append(cs.query(game_query(text, forms, masters, step.get("ref", False)), lab, "v"))
            if on_ours:
                row["ours_step"] = len(ours)
                ours.append({"op": "call", "line": our_text(text, forms, masters), "label": lab})
        elif kind == "game":
            # A query written per side (e.g. a global on ours, an expression in game).
            lab = label_of(step, step["game"])
            row = {"label": lab, "fn": step.get("fn"), "text": step["game"], "kind": "ask", "note": step.get("note", ""),
                   "tolerance": step.get("tolerance", TOLERANCE), "ref": False}
            row["game_step"] = len(gsteps)
            gsteps.append(cs.query(game_query(step["game"], forms, masters), lab, "v"))
            if "ours" in step:
                row["ours_step"] = len(ours)
                o = step["ours"]
                ours.append(dict(o, label=lab) if isinstance(o, dict) else {"op": "call", "line": our_text(o, forms, masters), "label": lab})
        elif kind == "do":
            text = step["do"]
            if fnames is not None:
                for t in text if isinstance(text, list) else [text]:
                    if not t.lower().startswith(("set ", "let ", "if ", "endif", "else")):
                        check_function(t, fnames, where)
            if on_game:
                gsteps += [{"script": game_statement(text, forms, masters)}, {"frames": step.get("settle", SETTLE)}]
            if on_ours:
                for t in text if isinstance(text, list) else [text]:
                    ours.append({"op": "call", "line": our_text(t, forms, masters)})
                if step.get("settle", SETTLE) > SETTLE:
                    ours.append({"op": "wait_frames", "frames": step["settle"] - SETTLE})
        elif kind == "frames":
            n = int(step["frames"])
            if on_game:
                gsteps.append({"frames": n})
            if on_ours:
                ours.append({"op": "wait_frames", "frames": n})
        elif kind == "console":
            if side == "ours":
                raise ProbeError(f"{where}: console lines run in game only")
            gsteps += [{"console": step["console"]}, {"frames": step.get("settle", SETTLE)}]
        elif kind == "until":
            lab = label_of(step, "until " + step["until"])
            mx = int(step.get("max_frames", LOAD_FRAMES))
            row = {"label": lab, "fn": step.get("fn") or check_function(step["until"], fnames, where), "text": step["until"],
                   "kind": "until", "note": step.get("note", ""), "tolerance": step.get("tolerance", 0)}
            if on_game:
                row["game_step"] = len(gsteps)
                gsteps.append({"until": game_until(step["until"], forms, masters), "max_frames": mx})
            if on_ours:
                row["ours_step"] = len(ours)
                ours.append({"op": "call_until", "line": our_text(step["until"], forms, masters), "max_frames": mx, "label": lab})
        elif kind == "moveto":
            target = step["moveto"]
            if on_game:
                gsteps += [{"script": game_statement(f"player.MoveTo {target}", forms, masters)},
                           {"until": game_until(f"{target}.HasLoaded3D", forms, masters), "max_frames": LOAD_FRAMES},
                           {"frames": step.get("settle", AFTER_LOAD)}]
            if on_ours:
                ours.append({"op": "call", "line": our_text(f"player.MoveTo {target}", forms, masters)})
                if step.get("cell"):
                    ours.append({"op": "load_cell", "cell": our_text(step["cell"], forms, masters)})
                ours.append({"op": "wait_frames", "frames": step.get("settle", AFTER_LOAD)})
        elif kind == "coc":
            cell = step["coc"]
            if not re.fullmatch(r"[A-Za-z0-9_]+", cell):
                raise ProbeError(f"{where}: coc takes a cell's Editor ID")
            if on_game:
                gsteps.append({"console": f"coc {cell}"})
                if step.get("cell"):
                    gsteps.append({"until": game_in_cell(step["cell"], forms, masters), "max_frames": COC_FRAMES})
                gsteps.append({"frames": step.get("settle", AFTER_COC)})
            if on_ours:
                ours.append({"op": "go_to_cell", "cell": cell})
                ours.append({"op": "wait_frames", "frames": step.get("settle", AFTER_COC)})
        elif kind == "screenshot":
            # A picture of the game window (plugin version 7), the HUD hidden meanwhile;
            # the game only (our side takes its own with --screenshot runs).
            lab = step["screenshot"]
            if not re.fullmatch(r"[A-Za-z0-9_-]+", lab):
                raise ProbeError(f"{where}: a screenshot label is letters, digits, _ and -")
            if on_game:
                gsteps += [{"console": "tm"}, {"frames": HUD_FRAMES}, {"screenshot": lab}, {"console": "tm"}, {"frames": HUD_FRAMES}]
        if row is not None:
            plan.append(row)
    job = {"format": cs.STEPS_FORMAT, "name": name, "title": batch.get("title", name),
           "start": batch.get("start", ""), "steps": gsteps}
    actions = {"format": ACTIONS_FORMAT, "title": batch.get("title", name), "save": batch.get("start", ""),
               "seed": int(batch.get("seed", 1)), "actions": list(batch.get("ours_start", [])) + ours}
    return job, actions, plan


def write_jobs(doc, out_dir, fnames=None):
    masters = doc.get("masters") or cs.MASTERS
    forms = doc.get("forms", {})
    for n, spec in forms.items():
        if not re.fullmatch(r"[A-Za-z0-9_]+", n):
            raise ProbeError(f"form name {n!r} must be [A-Za-z0-9_]+")
        parse_form(spec, masters)
    os.makedirs(out_dir, exist_ok=True)
    plan = {"format": PLAN_FORMAT, "masters": masters, "batches": []}
    seen = set()
    for b in doc.get("batch", []):
        job, actions, rows = build_batch(b, forms, masters, fnames)
        if job["name"] in seen:
            raise ProbeError(f"batch {job['name']} given twice")
        seen.add(job["name"])
        cs.write(job, os.path.join(out_dir, f"{job['name']}.steps.json"))
        cs.write(actions, os.path.join(out_dir, f"{job['name']}.actions.json"))
        plan["batches"].append({"name": job["name"], "title": job["title"], "probes": rows})
    cs.write(plan, os.path.join(out_dir, "probes.plan.json"))
    return plan


# --------------------------------------------------------------------------- answers

def game_answers(lines):
    """{game step index: ("value", v) | ("error", text) | ("frames", n)} from the plugin's
    output lines, and the mod-index map."""
    out, mods = {}, {}
    for line in lines:
        line = line.strip()
        if not line:
            continue
        try:
            rec = json.loads(line)
        except json.JSONDecodeError:
            continue
        kind = rec.get("kind")
        if kind == "result" and rec.get("label") == cs.MODINDEX_LABEL:
            if isinstance(rec.get("value"), (int, float)) and int(rec["value"]) != cs.NOT_LOADED:
                mods[int(rec["value"])] = rec["field"]
        elif kind == "result":
            out[rec["step"]] = ("error", rec["error"]) if "error" in rec else ("value", rec.get("value"))
        elif kind == "waited":
            out[rec["step"]] = ("frames", rec.get("frames"))
        elif kind == "error" and "step" in rec:
            out[rec["step"]] = ("error", rec.get("error") or rec.get("message") or "error")
    return out, mods


def our_answers(trace):
    """{label: ("value", v, stub) | ("frames", n, None) | ("error", text, None)}."""
    out = {}
    for o in trace.get("observations", []):
        lab = o.get("label")
        if "errors" in o:
            out[lab] = ("error", "; ".join(str(v) for v in o["errors"].values()), None)
        elif "frames" in o:
            out[lab] = ("frames", o["frames"], None)
        else:
            out[lab] = ("value", o.get("value"), o.get("stub"))
    return out


def game_ref(text, mods):
    """A game GetFormIDString answer -> 'Plugin.esm:XXXXXX' (or the text when unmapped)."""
    if not isinstance(text, str) or not re.fullmatch(r"[0-9A-Fa-f]{8}", text):
        return text
    v = int(text, 16)
    if v == 0:
        return "none"
    plugin = mods.get(v >> 24)
    return f"{plugin}:{v & 0xFFFFFF:06X}" if plugin else text


def our_ref(text, masters):
    if not isinstance(text, str):
        return text
    m = re.fullmatch(r"(?:0x)?([0-9A-Fa-f]{8})", text.strip())
    if not m:
        return text
    v = int(m.group(1), 16)
    if v == 0:
        return "none"
    i = v >> 24
    return f"{masters[i]}:{v & 0xFFFFFF:06X}" if i < len(masters) else text


def verdict(row, g, o):
    """same / differs / game error / ours error / game only / ours only / no answer."""
    if g is None and o is None:
        return "no answer"
    if g is None:
        return "ours error" if o[0] == "error" else "ours only"
    if o is None:
        return "game error" if g[0] == "error" else "game only"
    if g[0] == "error" and o[0] == "error":
        return "both error"
    if g[0] == "error":
        return "game error"
    if o[0] == "error":
        return "ours error"
    gv, ov = g[1], o[1]
    if isinstance(gv, (int, float)) and isinstance(ov, (int, float)):
        return "same" if abs(float(gv) - float(ov)) <= float(row.get("tolerance", TOLERANCE)) else "differs"
    return "same" if str(gv) == str(ov) else "differs"


def compare(out_dir):
    """Rows for every probe: batch, label, fn, game, ours, stub, verdict, note."""
    with open(os.path.join(out_dir, "probes.plan.json"), encoding="utf-8") as f:
        plan = json.load(f)
    masters = plan["masters"]
    rows = []
    for b in plan["batches"]:
        gp = os.path.join(out_dir, f"{b['name']}.game.jsonl")
        op = os.path.join(out_dir, f"{b['name']}.trace.json")
        ga, mods = ({}, {})
        if os.path.exists(gp):
            with open(gp, encoding="utf-8") as f:
                ga, mods = game_answers(f)
        oa = {}
        if os.path.exists(op):
            with open(op, encoding="utf-8") as f:
                oa = our_answers(json.load(f))
        for p in b["probes"]:
            g = ga.get(p["game_step"]) if "game_step" in p else None
            o = oa.get(p["label"]) if "ours_step" in p else None
            if p.get("ref"):
                g = (g[0], game_ref(g[1], mods)) if g and g[0] == "value" else g
                o = (o[0], our_ref(o[1], masters), o[2]) if o and o[0] == "value" else o
            rows.append({"batch": b["name"], "label": p["label"], "fn": p.get("fn"), "note": p.get("note", ""),
                         "game": None if g is None else (g[1] if g[0] != "error" else "error: " + str(g[1])),
                         "ours": None if o is None else (o[1] if o[0] != "error" else "error: " + str(o[1])),
                         "stub": o[2] if o else None, "verdict": verdict(p, g, o)})
    return rows


def shown(v):
    if v is None:
        return "-"
    if isinstance(v, float):
        return f"{v:.4g}"
    return str(v).replace("|", "\\|")


def markdown(rows):
    out = ["| Batch | Probe | Function | Original | Ours | Verdict |", "|---|---|---|---|---|---|"]
    for r in rows:
        ours = shown(r["ours"]) + (" (stub)" if r["stub"] else "")
        out.append(f"| {r['batch']} | {shown(r['label'])} | {r['fn'] or ''} | {shown(r['game'])} | {ours} | {r['verdict']} |")
    counts = {}
    for r in rows:
        counts[r["verdict"]] = counts.get(r["verdict"], 0) + 1
    out.append("")
    out.append("Totals: " + ", ".join(f"{k} {v}" for k, v in sorted(counts.items())))
    return "\n".join(out) + "\n"


# --------------------------------------------------------------------------- running

def selected(plan, only):
    names = [b["name"] for b in plan["batches"]]
    for n in only or ():
        if n not in names:
            raise ProbeError(f"no batch {n} (batches: {', '.join(names)})")
    return [b for b in plan["batches"] if not only or b["name"] in only]


def run_ours(out_dir, fnvtool, data, schema, masters=None, only=None):
    with open(os.path.join(out_dir, "probes.plan.json"), encoding="utf-8") as f:
        plan = json.load(f)
    masters = masters or plan["masters"]
    done = []
    for b in selected(plan, only):
        actions = os.path.join(out_dir, f"{b['name']}.actions.json")
        trace = os.path.join(out_dir, f"{b['name']}.trace.json")
        cmd = [fnvtool, "script", "run", data, *masters, "--schema", schema, "--actions", actions, "--no-voice-lengths"]
        r = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace", check=False)
        if r.returncode != 0:
            raise ProbeError(f"{b['name']}: fnvtool script run failed: {r.stderr.strip()[-2000:]}")
        with open(trace, "w", encoding="utf-8", newline="\n") as f:
            f.write(r.stdout)
        done.append(trace)
    return done


def game_locks(capture_dir):
    """The two game locks (CLAUDE.md), taken through the smoke runner's lock class."""
    sys.path.insert(0, os.path.join(REPO, "tools", "smoke"))
    from smoke.runner import Locks, default_lock_paths  # noqa: E402
    from pathlib import Path
    paths = default_lock_paths(Path(REPO))
    return Locks(paths, poll=30.0, say=lambda m: print("function_probe:", m, flush=True))


def run_game(out_dir, game_dir, save, capture_dir, timeout, extra=(), only=None):
    import capture_run
    with open(os.path.join(out_dir, "probes.plan.json"), encoding="utf-8") as f:
        plan = json.load(f)
    jobs = [os.path.join(out_dir, f"{b['name']}.steps.json") for b in selected(plan, only)]
    report = os.path.join(out_dir, "game.run.json")
    with game_locks(capture_dir):
        print(f"function_probe: the original game starts now and takes over the screen ({time.strftime('%H:%M')})", flush=True)
        code = capture_run.main(list(jobs) + ["--game", game_dir, "--save", save, "--capture-dir", capture_dir,
                                             "--timeout", str(timeout), "--kill-on-timeout", "-o", report, *extra])
    with open(report, encoding="utf-8") as f:
        rep = json.load(f)
    for name, s in rep.get("jobs", {}).items():
        if s.get("output"):
            shutil.copyfile(s["output"], os.path.join(out_dir, f"{name}.game.jsonl"))
    return code, rep


def main(argv=None):
    ap = argparse.ArgumentParser(prog="function_probe.py", description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    j = sub.add_parser("jobs", help="write the game jobs, our actions and the plan")
    j.add_argument("probes")
    j.add_argument("-o", "--out", required=True)
    j.add_argument("--functions", default=FUNCTIONS_JSON, help="catalogue to check function names against ('' to skip)")
    o = sub.add_parser("ours", help="run our side (fnvtool script run)")
    o.add_argument("dir")
    o.add_argument("--fnvtool", default=os.environ.get("NVF_FNVTOOL"), required=os.environ.get("NVF_FNVTOOL") is None)
    o.add_argument("--data", default=os.environ.get("NVF_DATA"), required=os.environ.get("NVF_DATA") is None)
    o.add_argument("--schema", default=os.path.join(REPO, "schemas", "fnv"))
    o.add_argument("--only", nargs="+", metavar="BATCH", help="these batches only")
    g = sub.add_parser("game", help="run the game side (capture_run, unattended)")
    g.add_argument("dir")
    g.add_argument("--game", required=True, help="the game folder (only read)")
    g.add_argument("--save", default="nvftest")
    g.add_argument("--capture-dir", default=r"C:\fnv-private\capture")
    g.add_argument("--timeout", type=int, default=1800)
    g.add_argument("--only", nargs="+", metavar="BATCH", help="these batches only (the plugin runs them in name order)")
    c = sub.add_parser("compare", help="line up both sides")
    c.add_argument("dir")
    c.add_argument("-o", "--output", help="markdown table (default: stdout)")
    c.add_argument("--json", help="also write the rows as JSON")
    args = ap.parse_args(argv)
    try:
        if args.cmd == "jobs":
            fnames = function_names(args.functions) if args.functions else None
            plan = write_jobs(load_probes(args.probes), args.out, fnames)
            n = sum(len(b["probes"]) for b in plan["batches"])
            print(f"{len(plan['batches'])} batches, {n} probes -> {args.out}")
        elif args.cmd == "ours":
            for t in run_ours(args.dir, args.fnvtool, args.data, args.schema, only=args.only):
                print(t)
        elif args.cmd == "game":
            code, rep = run_game(args.dir, args.game, args.save, args.capture_dir, args.timeout, only=args.only)
            for name, s in rep.get("jobs", {}).items():
                print(f"{name}: {s.get('status')} ({s.get('errors', 0)} failed steps)")
            for p in rep.get("problems", []):
                print("problem:", p)
            return code
        elif args.cmd == "compare":
            rows = compare(args.dir)
            text = markdown(rows)
            if args.output:
                with open(args.output, "w", encoding="utf-8", newline="\n") as f:
                    f.write(text)
            else:
                sys.stdout.write(text)
            if args.json:
                cs.write(rows, args.json)
    except (ProbeError, cs.InputError, OSError, tomllib.TOMLDecodeError, KeyError) as e:
        print(f"function_probe: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
