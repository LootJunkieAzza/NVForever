#!/usr/bin/env python3
"""Job files for the in-game capture plugin, and goldens from its output.

The plugin (tools/capture/nvf_capture, docs/spec-notes/xnvse-capture.md) runs inside the
original game and executes `fnv.capture.steps/1` jobs. This script writes those jobs from
the parity lab's scenario inputs and turns the plugin's output into goldens:

  capture_steps.py actions <x.actions.json> --start TEXT [-o job.steps.json]
      fnv.sim.actions/1 -> job; every query op becomes a recorded result labelled like
      the candidate's observation (docs/spec-notes/simulation.md section 7)
  capture_steps.py cases <x.cases.json> --draws N --start TEXT [-o job.steps.json]
      fnv.sim.cases/1 -> job; N AddItem draws per case with inventory dumps
  capture_steps.py trace <out.jsonl> [-o golden.trace.json]
      plugin output -> fnv.sim.trace/1 golden (observations only)
  capture_steps.py samples <out.jsonl> --cases <x.cases.json> [-o golden.samples.json]
      plugin output -> fnv.sim.samples/1 golden (outcome counts per case)

FormIDs in the inputs are global FormIDs of the load order MASTERS below (the ten
masters, docs/spec-notes/simulation.md); the job refers to forms by plugin name and
plugin-local FormID (BuildRef + GetModIndex), so it does not depend on the game's load
order, and the output's load-order FormIDs are mapped back through the load order the
plugin records first. Standard library only. Exit codes: 0 ok, 1 bad input, 2 usage.
"""
import argparse
import json
import os
import sys

STEPS_FORMAT = "fnv.capture.steps/1"
MASTERS = ["FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm",
           "LonesomeRoad.esm", "GunRunnersArsenal.esm", "ClassicPack.esm", "MercenaryPack.esm",
           "TribalPack.esm", "CaravanPack.esm"]
LOAD_ORDER_LABEL = "__load_order__"
DRAW_FIELD = "draw"


class InputError(Exception):
    pass


# --------------------------------------------------------------------------- scripts

def function(decls, body):
    """A "Begin Function { } ... End" user-defined function."""
    lines = ["Begin Function { }"] + list(dict.fromkeys(decls)) + list(body) + ["End"]
    return "\n".join(lines) + "\n"


class Names:
    """Turns scenario names (player, 0x global FormID, Editor ID) into script operands."""

    def __init__(self, masters=MASTERS):
        self.masters = masters
        self.decls = []
        self.setup = []
        self.count = 0

    def operand(self, name):
        if not isinstance(name, str) or not name:
            raise InputError(f"bad form or reference name {name!r}")
        if name.lower() == "player":
            return "player"
        if name[:2].lower() == "0x":
            try:
                fid = int(name, 16)
            except ValueError as e:
                raise InputError(f"bad FormID {name!r}") from e
            index, local = fid >> 24, fid & 0xFFFFFF
            if index >= len(self.masters):
                raise InputError(f"{name}: load-order index {index} is not one of the {len(self.masters)} masters")
            self.count += 1
            var = f"rF{self.count}"
            self.decls.append(f"ref {var}")
            self.setup.append(f'let {var} := BuildRef (GetModIndex "{self.masters[index]}") 0x{local:06X}')
            return var
        if not all(c.isalnum() or c == "_" for c in name):
            raise InputError(f"bad Editor ID {name!r}")
        return name


def plain_name(name, what):
    """A global or actor-value name: used verbatim, so it must be an identifier."""
    if not isinstance(name, str) or not name or not all(c.isalnum() or c == "_" for c in name) or name[:2].lower() == "0x":
        raise InputError(f"{what} must be given by name, got {name!r}")
    return name


def run_script(statements, masters, names):
    n = Names(masters)
    ops = [n.operand(x) for x in names]
    return function(n.decls, n.setup + [s.format(*ops) for s in statements])


def query_script(expr, masters, names, text=False):
    n = Names(masters)
    ops = [n.operand(x) for x in names]
    var, decl = ("sV", "string_var sV") if text else ("fV", "float fV")
    return function(n.decls + [decl], n.setup + [f"let {var} := {expr.format(*ops)}", f"SetFunctionValue {var}"])


def cell_script(of, masters):
    n = Names(masters)
    o = n.operand(of)
    return function(n.decls + ["ref rC", "string_var sV"],
                    n.setup + [f"let rC := {o}.GetParentCell", "let sV := GetFormIDString rC", "SetFunctionValue sV"])


LOAD_ORDER_SCRIPT = function(
    ["int iI", "int iN", "string_var sV"],
    ['let sV := ""', "let iN := GetNumLoadedMods", "while iI < iN",
     '  let sV := sV + (GetNthModName iI) + "|"', "  let iI += 1", "loop", "SetFunctionValue sV"])


def inventory_dump(of, var, idx):
    """Statements appending "<formid hex>x<count>;" for every stack of `of` to `var`."""
    return [f'let {var} := ""', f"let {idx} := 0", f"let iN := {of}.GetNumItems", f"while {idx} < iN",
            f"  let rI := {of}.GetInventoryObject {idx}",
            f'  let {var} := {var} + (GetFormIDString rI) + "x" + $({of}.GetItemCount rI) + ";"',
            f"  let {idx} += 1", "loop"]


def draw_script(list_name, count, masters):
    n = Names(masters)
    lst = n.operand(list_name)
    body = (n.setup + ["player.RemoveAllItems"] + inventory_dump("player", "sA", "iI")
            + [f"player.AddItem {lst} {int(count)} 1"] + inventory_dump("player", "sB", "iJ")
            + ['let sV := sA + "#" + sB', "SetFunctionValue sV"])
    return function(n.decls + ["int iI", "int iJ", "int iN", "ref rI", "string_var sA", "string_var sB",
                               "string_var sV"], body)


# --------------------------------------------------------------------------- jobs

def query(script, label, field):
    return {"query": script, "label": label, "field": field}


def action_steps(i, a, masters):
    """Steps for one fnv.sim.actions/1 action (index i)."""
    op = a.get("op")
    label = a.get("label") or f"step{i}"
    g = lambda k, default=None: a.get(k, default)  # noqa: E731
    rs = lambda stmts, *names: {"script": run_script(stmts, masters, names)}  # noqa: E731
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
        return [{"script": function([], [f"set {name} to {float(g('value'))!r}"])}]
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
        return [query(query_script("{0}.GetItemCount {1}", masters, [g("of"), g("item")]), label, "count")]
    if op == "get_pos":
        return [query(cell_script(g("of"), masters), label, "cell")] + [
            query(query_script(f"{{0}}.GetPos {axis}", masters, [g("of")]), label, f"pos.{axis.lower()}")
            for axis in ("X", "Y", "Z")]
    if op == "get_disabled":
        return [query(query_script("{0}.GetDisabled", masters, [g("of")]), label, "disabled")]
    if op == "get_global":
        name = plain_name(g("name"), "get_global: the global")
        return [query(function(["float fV"], [f"let fV := {name}", "SetFunctionValue fV"]), label, "value")]
    if op == "get_av":
        av = plain_name(g("name"), "get_av: the actor value")
        return [query(query_script(f"{{0}}.GetAV {av}", masters, [g("of")]), label, "value")]
    if op == "get_weight":
        return [query(query_script("{0}.GetAV InventoryWeight", masters, [g("of")]), label, "weight"),
                query(query_script("{0}.GetAV CarryWeight", masters, [g("of")]), label, "capacity")]
    raise InputError(f"action {i}: op {op!r} has no in-game equivalent here")


def job(name, title, start, steps):
    return {"format": STEPS_FORMAT, "name": name, "title": title, "start": start,
            "steps": [query(LOAD_ORDER_SCRIPT, LOAD_ORDER_LABEL, "plugins")] + steps}


def actions_job(doc, name, start, masters=MASTERS):
    if doc.get("format") != "fnv.sim.actions/1":
        raise InputError("not a fnv.sim.actions/1 document")
    steps = []
    for i, a in enumerate(doc.get("actions", [])):
        steps.extend(action_steps(i, a, masters))
    return job(name, name, start, steps)


def cases_job(doc, name, start, draws, masters=MASTERS):
    if doc.get("format") != "fnv.sim.cases/1":
        raise InputError("not a fnv.sim.cases/1 document")
    if draws <= 0:
        raise InputError("--draws must be positive")
    steps = []
    for c in doc.get("cases", []):
        steps.append({"console": f"player.setlevel {int(c.get('level', 1))}"})
        steps.append({"frames": 5})
        script = draw_script(c["list"], c.get("count", 1), masters)
        steps.extend(query(script, c["label"], DRAW_FIELD) for _ in range(draws))
    steps.append({"script": function([], ["player.RemoveAllItems"])})
    return job(name, name, start, steps)


# --------------------------------------------------------------------------- output

def read_output(lines):
    """(load order [names], results [dict]) from the plugin's JSON lines."""
    results, plugins, ended = [], None, False
    for n, line in enumerate(lines, 1):
        line = line.strip()
        if not line:
            continue
        try:
            rec = json.loads(line)
        except json.JSONDecodeError as e:
            raise InputError(f"line {n}: {e}") from e
        kind = rec.get("kind")
        if kind == "result" and rec.get("label") == LOAD_ORDER_LABEL:
            if "value" not in rec or not isinstance(rec["value"], str):
                raise InputError(f"load order not recorded: {rec.get('error')}")
            plugins = [p for p in rec["value"].split("|") if p]
        elif kind == "result":
            results.append(rec)
        elif kind == "end":
            ended = True
        elif kind == "aborted":
            raise InputError("the job was aborted (a save was loaded while it ran)")
    if plugins is None:
        raise InputError("no load order in the output")
    if not ended:
        raise InputError("the output has no end line (job unfinished)")
    return plugins, results


def to_global(formid_text, plugins, masters=MASTERS):
    """A load-order FormID (hex text from GetFormIDString) -> global FormID string."""
    fid = int(formid_text.strip().lower().removeprefix("0x"), 16)
    index = fid >> 24
    if index == 0xFF:
        return f"0x{fid:08X}"  # created at run time
    if index >= len(plugins):
        raise InputError(f"FormID {formid_text} names load-order index {index}, but only {len(plugins)} plugins are loaded")
    lower = [m.lower() for m in masters]
    name = plugins[index].lower()
    if name not in lower:
        raise InputError(f"FormID {formid_text} belongs to {plugins[index]}, which is not one of the masters")
    return f"0x{(lower.index(name) << 24) | (fid & 0xFFFFFF):08X}"


def trace_golden(plugins, results):
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
            o["cell"] = to_global(v, plugins) if v else None
        elif field.startswith("pos."):
            o.setdefault("pos", [None, None, None])["xyz".index(field[4])] = v
        else:
            o[field] = v
    return {"format": "fnv.sim.trace/1", "source": "in-game capture (nvf_capture)", "load_order": plugins,
            "observations": list(obs.values())}


def parse_dump(text, plugins):
    out = {}
    for part in text.split(";"):
        if not part:
            continue
        fid, _, n = part.rpartition("x")
        if not fid:
            raise InputError(f"bad inventory entry {part!r}")
        out[to_global(fid, plugins)] = out.get(to_global(fid, plugins), 0) + int(round(float(n)))
    return out


def outcome_key(before, after):
    gained = {k: after[k] - before.get(k, 0) for k in after if after[k] - before.get(k, 0) > 0}
    if not gained:
        return "nothing"
    return ", ".join(f"{k} x{gained[k]}" for k in sorted(gained, key=lambda s: int(s, 16)))


def samples_golden(plugins, results, cases_doc):
    by_label = {}
    errors = {}
    for r in results:
        if r.get("field") != DRAW_FIELD:
            continue
        if "error" in r or not isinstance(r.get("value"), str) or "#" not in r["value"]:
            errors[r["label"]] = errors.get(r["label"], 0) + 1
            continue
        a, b = r["value"].split("#", 1)
        key = outcome_key(parse_dump(a, plugins), parse_dump(b, plugins))
        h = by_label.setdefault(r["label"], {})
        h[key] = h.get(key, 0) + 1
    cases = []
    for c in cases_doc.get("cases", []):
        h = by_label.get(c["label"], {})
        row = {"label": c["label"], "list": c["list"], "level": c.get("level", 1), "count": c.get("count", 1),
               "samples": sum(h.values()), "outcomes": dict(sorted(h.items()))}
        if errors.get(c["label"]):
            row["failed_draws"] = errors[c["label"]]
        cases.append(row)
    return {"format": "fnv.sim.samples/1", "source": "in-game capture (nvf_capture)", "load_order": plugins,
            "cases": cases}


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
    a = sub.add_parser("actions")
    a.add_argument("input")
    a.add_argument("--name")
    a.add_argument("--start", required=True, help="what to load before starting (shown in game)")
    a.add_argument("-o", "--output")
    c = sub.add_parser("cases")
    c.add_argument("input")
    c.add_argument("--name")
    c.add_argument("--start", required=True)
    c.add_argument("--draws", type=int, required=True)
    c.add_argument("-o", "--output")
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
            base = os.path.basename(args.input).split(".")[0]
            name = args.name or base
            doc = load_json(args.input)
            out = actions_job(doc, name, args.start) if args.cmd == "actions" else \
                cases_job(doc, name, args.start, args.draws)
        else:
            with open(args.input, encoding="utf-8") as f:
                plugins, results = read_output(f)
            out = trace_golden(plugins, results) if args.cmd == "trace" else \
                samples_golden(plugins, results, load_json(args.cases))
    except (OSError, json.JSONDecodeError, InputError, KeyError, ValueError) as e:
        print(f"capture_steps: {e}", file=sys.stderr)
        return 1
    write(out, args.output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
