#!/usr/bin/env python3
"""Job files for the in-game capture plugin, and goldens from its output.

The plugin (tools/capture/nvf_capture, docs/spec-notes/xnvse-capture.md) runs inside the
original game and executes `fnv.capture.steps/1` jobs. This script writes those jobs from
the parity lab's scenario inputs and turns the plugin's output into goldens:

  capture_steps.py actions <x.actions.json> --data <Data> --start TEXT [-o job.steps.json]
      fnv.sim.actions/1 -> job; every query op becomes a recorded result labelled like
      the candidate's observation (docs/spec-notes/simulation.md section 7)
  capture_steps.py cases <x.cases.json> --data <Data> --draws N --start TEXT [-o job]
      fnv.sim.cases/1 -> job; N AddItem draws per case, each followed by an inventory dump
  capture_steps.py trace <out.jsonl> [-o golden.trace.json]
      plugin output -> fnv.sim.trace/1 golden (observations only)
  capture_steps.py samples <out.jsonl> --cases <x.cases.json> [-o golden.samples.json]
      plugin output -> fnv.sim.samples/1 golden (outcome counts per case)

Scripts follow what the game's runtime compiler accepted in the diagnostic runs of
2026-10-07 (docs/spec-notes/xnvse-capture.md section 5) [observed]: forms by number
(`BuildRef <mod index> <decimal local FormID>`; editor IDs of base forms and references
are not resolvable in game, so `--data` resolves them here from the masters, read-only),
variables declared before `Begin Function`, no string variables (strings are returned
directly), and every function returns a value (`SetFunctionValue 1`), since a call that
returns nothing is reported as failed. Inputs name forms by global FormID of the load
order MASTERS, by Editor ID, or "player"; output FormIDs are mapped back through the mod
indices the job records first. Standard library only. Exit codes: 0 ok, 1 bad input.
"""
import argparse
import json
import os
import sys

CAPTURE_DIR = os.path.dirname(os.path.abspath(__file__))
if CAPTURE_DIR not in sys.path:
    sys.path.insert(0, CAPTURE_DIR)

STEPS_FORMAT = "fnv.capture.steps/1"
MASTERS = ["FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm",
           "LonesomeRoad.esm", "GunRunnersArsenal.esm", "ClassicPack.esm", "MercenaryPack.esm",
           "TribalPack.esm", "CaravanPack.esm"]
MODINDEX_LABEL = "__modindex__"
SLOTS = 12  # inventory stacks read after each leveled-list draw
NOT_LOADED = 255  # GetModIndex of a plugin that is not loaded [documented: GECK wiki]


class InputError(Exception):
    pass


# --------------------------------------------------------------------------- editor IDs

def edid_index(data_dir, masters=MASTERS):
    """Lower-case Editor ID -> global FormID of its winning, non-deleted record."""
    import oracle_esm
    import oracle_sim
    warnings = oracle_sim.Warnings()
    lower = [m.lower() for m in masters]
    by_fid = {}  # global FormID -> Editor ID of its current winner (None when deleted)
    for index, name in enumerate(masters):
        plugin = oracle_sim.open_plugin(data_dir, name, index, lower, warnings)
        for rec in oracle_esm.scan_plugin(plugin.buf, name).records:
            local = int(rec["formid"], 16)
            top = local >> 24
            target = plugin.remap[top] if top < len(plugin.remap) else None
            if target is not None:
                fid = (target << 24) | (local & 0xFFFFFF)
                by_fid[fid] = None if rec["flags"] & 0x20 else rec["edid"]
        plugin.buf = None
    return {e.lower(): fid for fid, e in by_fid.items() if e}


# --------------------------------------------------------------------------- scripts

def function(decls, body, returns_value=True):
    """A user-defined function script: declarations first, then the Function block."""
    lines = list(dict.fromkeys(decls)) + ["Begin Function { }"] + list(body)
    if not returns_value:
        lines.append("SetFunctionValue 1")
    return "\n".join(lines + ["End"]) + "\n"


class Names:
    """Turns scenario names into script operands: player, or ref variables built from
    (mod index, local FormID) with BuildRef."""

    def __init__(self, masters, edids):
        self.masters = masters
        self.edids = edids
        self.decls = []
        self.setup = []
        self.mods = {}
        self.count = 0

    def global_id(self, name):
        if not isinstance(name, str) or not name:
            raise InputError(f"bad form or reference name {name!r}")
        if name[:2].lower() == "0x":
            try:
                return int(name, 16)
            except ValueError as e:
                raise InputError(f"bad FormID {name!r}") from e
        if self.edids is None:
            raise InputError(f"{name!r} is an Editor ID; pass --data so it can be resolved to a FormID")
        fid = self.edids.get(name.lower())
        if fid is None:
            raise InputError(f"no record has Editor ID {name!r}")
        return fid

    def operand(self, name):
        if isinstance(name, str) and name.lower() == "player":
            return "player"
        fid = self.global_id(name)
        index, local = fid >> 24, fid & 0xFFFFFF
        if index >= len(self.masters):
            raise InputError(f"{name}: load-order index {index} is not one of the {len(self.masters)} masters")
        plugin = self.masters[index]
        if plugin not in self.mods:
            m = f"fM{len(self.mods) + 1}"
            self.mods[plugin] = m
            self.decls.append(f"float {m}")
            self.setup.append(f'let {m} := GetModIndex "{plugin}"')
        self.count += 1
        var = f"rF{self.count}"
        self.decls.append(f"ref {var}")
        self.setup.append(f"let {var} := BuildRef {self.mods[plugin]} {local}")
        return var


def plain_name(name, what):
    """A global or actor-value name: used verbatim, so it must be an identifier."""
    if not isinstance(name, str) or not name or not all(c.isalnum() or c == "_" for c in name) or name[:2].lower() == "0x":
        raise InputError(f"{what} must be given by name, got {name!r}")
    return name


def run_script(statements, ctx, names):
    n = Names(*ctx)
    ops = [n.operand(x) for x in names]
    return function(n.decls, n.setup + [s.format(*ops) for s in statements], returns_value=False)


def number_query(expr, ctx, names):
    n = Names(*ctx)
    ops = [n.operand(x) for x in names]
    return function(n.decls + ["float fV"], n.setup + [f"let fV := {expr.format(*ops)}", "SetFunctionValue fV"])


def cell_query(of, ctx):
    n = Names(*ctx)
    o = n.operand(of)
    return function(n.decls + ["ref rC"], n.setup + [f"let rC := {o}.GetParentCell", "SetFunctionValue (GetFormIDString rC)"])


def modindex_query(plugin):
    return function(["float fV"], [f'let fV := GetModIndex "{plugin}"', "SetFunctionValue fV"])


def slot_query(i):
    """'<load-order FormID>x<count>' of the player's i-th inventory stack ('00000000x0'
    beyond the last one)."""
    return function(["ref rI", "float fN"],
                    [f"let rI := player.GetInventoryObject {i}", "if rI", "  let fN := player.GetItemCount rI", "endif",
                     'SetFunctionValue ((GetFormIDString rI) + "x" + ($ fN))'])


# --------------------------------------------------------------------------- jobs

def query(script, label, field):
    return {"query": script, "label": label, "field": field}


def action_steps(i, a, ctx):
    """Steps for one fnv.sim.actions/1 action (index i)."""
    op = a.get("op")
    label = a.get("label") or f"step{i}"
    g = lambda k, default=None: a.get(k, default)  # noqa: E731
    rs = lambda stmts, *names: {"script": run_script(stmts, ctx, names)}  # noqa: E731
    if op == "activate":
        return [rs(["{0}.Activate {1}"], g("target"), g("actor", "player")), {"frames": 30}]
    if op == "add_item":
        return [rs([f"{{0}}.AddItem {{1}} {int(g('count'))} 1"], g("to"), g("item"))]
    if op == "remove_item":
        return [rs([f"{{0}}.RemoveItem {{1}} {int(g('count'))}"], g("from"), g("item"))]
    if op == "transfer":
        return [{"manual": f"Move {int(g('count'))} x {g('item')} from {g('from')} to {g('to')} through the container menu."}]
    if op == "transfer_all":
        return [rs(["{0}.RemoveAllItems {1}"], g("from"), g("to"))]
    if op == "drop_item":
        return [rs([f"{{0}}.Drop {{1}} {int(g('count'))}"], g("from"), g("item"))]
    if op == "remove_all_items":
        return [rs(["{0}.RemoveAllItems"], g("from"))]
    if op == "lock":
        level = g("level")
        return [rs(["{0}.Lock" + (f" {int(level)}" if level is not None else "")], g("target"))]
    if op in ("unlock", "enable", "disable"):
        return [rs(["{0}." + op.capitalize()], g("target"))]
    if op == "set_global":
        name = plain_name(g("name"), "set_global: the global")
        return [{"script": function([], [f"set {name} to {float(g('value'))!r}"], returns_value=False)}]
    if op == "set_av":
        return [rs([f"{{0}}.SetAV {plain_name(g('name'), 'set_av: the actor value')} {float(g('value'))!r}"], g("of"))]
    if op == "equip_item":
        return [rs(["{0}.EquipItem {1}"], g("actor"), g("item"))]
    if op == "unequip_item":
        return [rs(["{0}.UnequipItem {1}"], g("actor"), g("item"))]
    if op == "advance_time":
        return [{"manual": f"Wait {g('hours')} hours with the Wait menu (at most 24 at a time)."}]
    if op == "detach_cell":
        return [{"manual": f"Leave cell {g('cell')} through its door and go to another cell."}]
    if op == "attach_cell":
        return [{"manual": f"Return to cell {g('cell')} and wait until it has loaded."}]
    if op == "get_item_count":
        return [query(number_query("{0}.GetItemCount {1}", ctx, [g("of"), g("item")]), label, "count")]
    if op == "get_pos":
        return [query(cell_query(g("of"), ctx), label, "cell")] + [
            query(number_query(f"{{0}}.GetPos {axis}", ctx, [g("of")]), label, f"pos.{axis.lower()}")
            for axis in ("X", "Y", "Z")]
    if op == "get_disabled":
        return [query(number_query("{0}.GetDisabled", ctx, [g("of")]), label, "disabled")]
    if op == "get_global":
        name = plain_name(g("name"), "get_global: the global")
        return [query(function(["float fV"], [f"let fV := {name}", "SetFunctionValue fV"]), label, "value")]
    if op == "get_av":
        av = plain_name(g("name"), "get_av: the actor value")
        return [query(number_query(f"{{0}}.GetAV {av}", ctx, [g("of")]), label, "value")]
    if op == "get_weight":
        return [query(number_query("{0}.GetAV InventoryWeight", ctx, [g("of")]), label, "weight"),
                query(number_query("{0}.GetAV CarryWeight", ctx, [g("of")]), label, "capacity")]
    raise InputError(f"action {i}: op {op!r} has no in-game equivalent here")


def job(name, start, steps, masters):
    head = [query(modindex_query(m), MODINDEX_LABEL, m) for m in masters]
    return {"format": STEPS_FORMAT, "name": name, "title": name, "start": start, "steps": head + steps}


def actions_job(doc, name, start, edids=None, masters=MASTERS):
    if doc.get("format") != "fnv.sim.actions/1":
        raise InputError("not a fnv.sim.actions/1 document")
    steps = []
    for i, a in enumerate(doc.get("actions", [])):
        steps.extend(action_steps(i, a, (masters, edids)))
    return job(name, start, steps, masters)


def dump_steps(label, prefix):
    return [query(slot_query(i), label, f"{prefix}.slot{i}") for i in range(SLOTS)] + [
        query(function(["float fV"], ["let fV := player.GetNumItems", "SetFunctionValue fV"]), label, f"{prefix}.num")]


def cases_job(doc, name, start, draws, edids=None, masters=MASTERS):
    if doc.get("format") != "fnv.sim.cases/1":
        raise InputError("not a fnv.sim.cases/1 document")
    if draws <= 0:
        raise InputError("--draws must be positive")
    ctx = (masters, edids)
    steps = []
    for c in doc.get("cases", []):
        steps.append({"console": f"player.setlevel {int(c.get('level', 1))}"})
        steps.append({"frames": 5})
        steps.append({"script": run_script(["player.RemoveAllItems"], ctx, [])})
        steps.extend(dump_steps(c["label"], "base"))
        draw = run_script(["player.RemoveAllItems", f"player.AddItem {{0}} {int(c.get('count', 1))} 1"], ctx, [c["list"]])
        for n in range(draws):
            steps.append({"script": draw})
            steps.extend(dump_steps(c["label"], f"draw{n}"))
    steps.append({"script": run_script(["player.RemoveAllItems"], ctx, [])})
    return job(name, start, steps, masters)


# --------------------------------------------------------------------------- output

def read_output(lines):
    """(mod index -> plugin name, results [dict]) from the plugin's JSON lines."""
    results, mods, ended = [], {}, False
    for n, line in enumerate(lines, 1):
        line = line.strip()
        if not line:
            continue
        try:
            rec = json.loads(line)
        except json.JSONDecodeError as e:
            raise InputError(f"line {n}: {e}") from e
        kind = rec.get("kind")
        if kind == "result" and rec.get("label") == MODINDEX_LABEL:
            if not isinstance(rec.get("value"), (int, float)):
                raise InputError(f"mod index of {rec.get('field')} not recorded: {rec.get('error')}")
            if int(rec["value"]) != NOT_LOADED:
                mods[int(rec["value"])] = rec["field"]
        elif kind == "result":
            results.append(rec)
        elif kind == "end":
            ended = True
        elif kind == "aborted":
            raise InputError("the job was aborted (a save was loaded while it ran)")
    if not mods:
        raise InputError("no mod indices in the output")
    if not ended:
        raise InputError("the output has no end line (job unfinished)")
    return mods, results


def to_global(formid_text, mods, masters=MASTERS):
    """A load-order FormID (hex text from GetFormIDString) -> global FormID string."""
    fid = int(formid_text.strip().lower().removeprefix("0x"), 16)
    index = fid >> 24
    if index == 0xFF:
        return f"0x{fid:08X}"  # created at run time
    if index not in mods:
        raise InputError(f"FormID {formid_text} belongs to load-order index {index}, which is not one of the masters")
    return f"0x{(masters.index(mods[index]) << 24) | (fid & 0xFFFFFF):08X}"


def trace_golden(mods, results):
    obs = {}
    for r in results:
        o = obs.setdefault(r["label"], {"label": r["label"]})
        if "error" in r:
            o["error"] = r["error"]
            continue
        v, field = r.get("value"), r["field"]
        if field == "count":
            o["count"] = int(round(v))
        elif field == "disabled":
            o["disabled"] = bool(v)
        elif field == "cell":
            o["cell"] = to_global(v, mods) if v and int(v, 16) else None
        elif field.startswith("pos."):
            o.setdefault("pos", [None, None, None])["xyz".index(field[4])] = v
        else:
            o[field] = v
    return {"format": "fnv.sim.trace/1", "source": "in-game capture (nvf_capture)",
            "mod_indices": {str(k): v for k, v in sorted(mods.items())}, "observations": list(obs.values())}


def parse_slot(text, mods):
    """(global FormID, count) of one slot result, or None for an empty slot."""
    fid, _, n = text.rpartition("x")
    if not fid:
        raise InputError(f"bad inventory slot {text!r}")
    if int(fid, 16) == 0:
        return None
    count = int(round(float(n)))
    return (to_global(fid, mods), count) if count > 0 else None


def outcome_key(before, after):
    gained = {k: after[k] - before.get(k, 0) for k in after if after[k] - before.get(k, 0) > 0}
    if not gained:
        return "nothing"
    return ", ".join(f"{k} x{gained[k]}" for k in sorted(gained, key=lambda s: int(s, 16)))


def samples_golden(mods, results, cases_doc):
    dumps = {}  # (label, prefix) -> {"items": {}, "num": n, "bad": bool}
    for r in results:
        prefix, _, part = r.get("field", "").partition(".")
        d = dumps.setdefault((r["label"], prefix), {"items": {}, "num": None, "bad": False})
        if "error" in r:
            d["bad"] = True
        elif part == "num":
            d["num"] = int(round(r["value"]))
        elif part.startswith("slot"):
            s = parse_slot(r["value"], mods)
            if s:
                d["items"][s[0]] = d["items"].get(s[0], 0) + s[1]
    cases = []
    for c in cases_doc.get("cases", []):
        label = c["label"]
        base = dumps.get((label, "base"))
        h, failed = {}, 0
        for (lab, prefix), d in sorted(dumps.items()):
            if lab != label or not prefix.startswith("draw"):
                continue
            # A draw counts only if every slot read and no stack lay beyond the slots read.
            if d["bad"] or base is None or base["bad"] or d["num"] is None or d["num"] > SLOTS:
                failed += 1
                continue
            key = outcome_key(base["items"], d["items"])
            h[key] = h.get(key, 0) + 1
        row = {"label": label, "list": c["list"], "level": c.get("level", 1), "count": c.get("count", 1),
               "samples": sum(h.values()), "outcomes": dict(sorted(h.items()))}
        if failed:
            row["failed_draws"] = failed
        cases.append(row)
    return {"format": "fnv.sim.samples/1", "source": "in-game capture (nvf_capture)",
            "mod_indices": {str(k): v for k, v in sorted(mods.items())}, "cases": cases}


# --------------------------------------------------------------------------- CLI

def write(doc, path):
    text = json.dumps(doc, indent=1, ensure_ascii=False) + "\n"
    if path:
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
    else:
        sys.stdout.write(text)


def load_json(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def main(argv=None):
    ap = argparse.ArgumentParser(prog="capture_steps.py", description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    for cmd in ("actions", "cases"):
        p = sub.add_parser(cmd)
        p.add_argument("input")
        p.add_argument("--data", help="game Data folder (read-only), to resolve Editor IDs to FormIDs")
        p.add_argument("--name")
        p.add_argument("--start", required=True, help="what to load before starting (shown in game)")
        p.add_argument("-o", "--output")
        if cmd == "cases":
            p.add_argument("--draws", type=int, required=True)
    t = sub.add_parser("trace")
    t.add_argument("input")
    t.add_argument("-o", "--output")
    s = sub.add_parser("samples")
    s.add_argument("input")
    s.add_argument("--cases", required=True)
    s.add_argument("-o", "--output")
    args = ap.parse_args(argv)
    try:
        if args.cmd in ("actions", "cases"):
            name = args.name or os.path.basename(args.input).split(".")[0]
            doc = load_json(args.input)
            edids = edid_index(args.data) if args.data else None
            out = actions_job(doc, name, args.start, edids) if args.cmd == "actions" else \
                cases_job(doc, name, args.start, args.draws, edids)
        else:
            with open(args.input, encoding="utf-8") as f:
                mods, results = read_output(f)
            out = trace_golden(mods, results) if args.cmd == "trace" else \
                samples_golden(mods, results, load_json(args.cases))
    except (OSError, json.JSONDecodeError, InputError, KeyError, ValueError) as e:
        print(f"capture_steps: {e}", file=sys.stderr)
        return 1
    except Exception as e:  # oracle_sim.InputError and friends when reading the masters
        print(f"capture_steps: {type(e).__name__}: {e}", file=sys.stderr)
        return 1
    write(out, args.output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
