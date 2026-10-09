#!/usr/bin/env python3
"""Layer 5 quest captures (workstream 5C, docs/spec-notes/layer5-captures.md).

Quest state and quest-script behaviour recorded in the original game with the
nvf_capture plugin (docs/spec-notes/xnvse-capture.md), turned into private goldens for
the parity lab's `L5.quest.*` scenarios.

  defs        quest definitions (stages, objectives, script variables) from the
              repository's own `fnvtool esm decode` (read-only on the game's Data)
  state-job   a capture job that records the state of the quests in `defs`
  state       the quest state document (fnv.quest.state/1) from that job's output
  diff        the differences between two state documents (e.g. two saves)
  job         a capture job from a scenario's fnv.script.actions/1 input
  golden      the scenario golden (fnv.script.trace/1) from that job's output

Standard library only. Nothing here writes into the game folder; outputs go where -o
says (goldens belong in the private goldens directory, never in the repository).
"""
import argparse
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import capture_steps as cs  # noqa: E402

DEFS_FORMAT = "fnv.capture.quest_defs/1"
STATE_FORMAT = "fnv.quest.state/1"
ACTIONS_FORMAT = "fnv.script.actions/1"
TRACE_FORMAT = "fnv.script.trace/1"
GIVEN_PREFIX = "__given__"
SAMPLE_PREFIX = "sample"
MENU_PREFIX = "menu"

InputError = cs.InputError


# --------------------------------------------------------------------------- definitions

def _flag_names(value):
    """The `set` list of a decoded flags field (fnvtool esm decode), else []."""
    return list(value.get("set", [])) if isinstance(value, dict) else []


def quest_defs(quest, script=None):
    """One quest's definition from its decoded QUST record and (optional) decoded SCPT
    record (fnv.esm.decode/1 record objects). Stage entries: one per QSDT after an INDX
    [observed: FalloutNV.esm, docs/design/layer5.md section 2]; CTDA before the first QOBJ
    belong to the current entry, after it to the objective; texts are not kept."""
    out = {"edid": None, "formid": quest.get("formid"), "flags": [], "flag_bits": None, "priority": None,
           "delay": None, "script": None, "stages": [], "objectives": [], "variables": []}
    stage = entry = objective = None
    for sub in quest.get("subrecords", []):
        sig, v = sub.get("sig"), sub.get("value")
        if sig == "EDID":
            out["edid"] = v
        elif sig == "SCRI":
            out["script"] = v
        elif sig == "DATA" and isinstance(v, dict):
            out["flags"] = _flag_names(v.get("flags"))
            out["flag_bits"] = v.get("flags", {}).get("value") if isinstance(v.get("flags"), dict) else None
            out["priority"] = v.get("priority")
            out["delay"] = v.get("quest_delay")
        elif sig == "INDX":
            stage = {"index": v, "entries": []}
            entry = objective = None
            out["stages"].append(stage)
        elif sig == "QSDT" and stage is not None:
            entry = {"flags": _flag_names(v), "conditions": 0, "log": False, "compiled_size": 0}
            stage["entries"].append(entry)
        elif sig == "CNAM" and entry is not None:
            entry["log"] = True
        elif sig == "SCHR" and entry is not None and isinstance(v, dict):
            entry["compiled_size"] = v.get("compiled_size", 0)
        elif sig == "QOBJ":
            objective = {"index": v, "targets": 0, "conditions": 0}
            stage = entry = None
            out["objectives"].append(objective)
        elif sig == "QSTA" and objective is not None:
            objective["targets"] += 1
        elif sig == "CTDA":
            if objective is not None:
                objective["conditions"] += 1
            elif entry is not None:
                entry["conditions"] += 1
    if script is not None:
        out["variables"] = script_variables(script)
        out["script_edid"] = next((s.get("value") for s in script.get("subrecords", []) if s.get("sig") == "EDID"), None)
    if not out["edid"]:
        raise InputError(f"quest {out['formid']}: no EDID")
    return out


def script_variables(script):
    """[{index, name, type}] from a decoded SCPT record's SLSD/SCVR pairs, in record
    order. Variables are addressed by index, which is not declaration order
    [observed: docs/design/layer5.md section 2]."""
    out, pending = [], None
    for sub in script.get("subrecords", []):
        sig, v = sub.get("sig"), sub.get("value")
        if sig == "SLSD" and isinstance(v, dict):
            pending = {"index": v.get("index"), "type": v.get("type")}
        elif sig == "SCVR" and pending is not None:
            out.append({"index": pending["index"], "name": v, "type": pending["type"]})
            pending = None
    return out


def decode_record(fnvtool, plugin_path, schema, key, value):
    """The first record of `fnvtool esm decode <plugin> --schema <dir> --edid|--formid`."""
    run = subprocess.run([fnvtool, "esm", "decode", plugin_path, "--schema", schema, f"--{key}", value],
                         capture_output=True, text=True, encoding="utf-8")
    if run.returncode != 0:
        raise InputError(f"fnvtool esm decode --{key} {value}: exit {run.returncode}: {run.stderr.strip()[:300]}")
    recs = json.loads(run.stdout).get("records", [])
    if not recs:
        raise InputError(f"no record with {key} {value}")
    return recs[0]


def defs_document(quests, plugin):
    return {"format": DEFS_FORMAT, "plugin": plugin, "quests": quests}


# --------------------------------------------------------------------------- state job

def quest_fields(q):
    """Every state field recorded for a quest definition, in order."""
    fields = ["running", "completed", "stage"]
    fields += [f"done.{s['index']}" for s in q["stages"]]
    for o in q["objectives"]:
        fields += [f"objective.{o['index']}.displayed", f"objective.{o['index']}.completed"]
    fields += [f"var.{v['name']}" for v in q["variables"]]
    return fields


def field_expr(q, field):
    """The script expression that reads one state field of the quest in ref variable `q`.
    Quest Editor IDs do not compile in job scripts (trial q5_trial,
    docs/spec-notes/layer5-captures.md section 1), so the quest is built with BuildRef and
    its variables are read with xNVSE's GetVariable."""
    if field == "running":
        return f"GetQuestRunning {q}"
    if field == "completed":
        return f"GetQuestCompleted {q}"
    if field == "stage":
        return f"GetStage {q}"
    parts = field.split(".")
    if parts[0] == "done" and len(parts) == 2:
        return f"GetStageDone {q} {int(parts[1])}"
    if parts[0] == "objective" and len(parts) == 3 and parts[2] in ("displayed", "completed"):
        fn = "GetObjectiveDisplayed" if parts[2] == "displayed" else "GetObjectiveCompleted"
        return f"{fn} {q} {int(parts[1])}"
    if parts[0] == "var" and len(parts) == 2:
        return f'GetVariable "{cs.plain_name(parts[1], "a quest variable")}" {q}'
    raise InputError(f"unknown quest field {field!r}")


def read_query(expr, label, field):
    return cs.query(cs.function(["float fV"], [f"let fV := {expr}", "SetFunctionValue fV"]), label, field)


def quest_script(quest, ctx, body):
    """A function script with the quest (Editor ID or FormID) in a ref variable; `body`
    lines use {q} for it."""
    n = cs.Names(*ctx)
    q = n.operand(quest)
    return cs.function(n.decls + ["float fV"], n.setup + [line.format(q=q) for line in body])


def quest_query(quest, field, label, ctx):
    field_expr("rQ", field)  # validates the field before any name is resolved
    return cs.query(quest_script(quest, ctx, [f"let fV := {field_expr('{q}', field)}", "SetFunctionValue fV"]),
                    label, field)


def state_job(defs, name, start):
    ctx = (cs.MASTERS, None)
    steps = [quest_query(q["formid"], f, q["edid"], ctx) for q in defs["quests"] for f in quest_fields(q)]
    j = cs.job(name, start, steps, cs.MASTERS)
    j["title"] = f"Quest state of {len(defs['quests'])} quests (read-only, a few seconds)"
    return j


# --------------------------------------------------------------------------- values

BOOL_FIELDS = ("running", "completed", "disabled", "opened", "dead", "current")


def typed(field, v):
    """A recorded number as the type its field has in fnv.script.trace/1 and
    fnv.quest.state/1: booleans for flags, integers for stages and counts, variables
    and globals as recorded."""
    if v is None or isinstance(v, str):
        return v
    head = field.split(".")[0]
    if field in BOOL_FIELDS or head == "done" or (head == "objective" and field.count(".") == 2):
        return bool(v)
    if field in ("stage", "count", "frames", "locked"):
        return int(round(v))
    return v


# --------------------------------------------------------------------------- state document

def state_document(defs, mods, results, source=None):
    by = {}
    for r in results:
        by.setdefault(r["label"], {})[r["field"]] = r
    quests = []
    for q in defs["quests"]:
        got = by.get(q["edid"], {})
        s = {"edid": q["edid"], "formid": q["formid"], "errors": {}}
        done, not_done, objectives, variables = [], [], [], {}
        for f in quest_fields(q):
            r = got.get(f)
            if r is None:
                s["errors"][f] = "not recorded"
                continue
            if "error" in r:
                s["errors"][f] = r["error"]
                continue
            v = typed(f, r.get("value"))
            head = f.split(".")
            if head[0] == "done":
                (done if v else not_done).append(int(head[1]))
            elif head[0] == "objective":
                idx = int(head[1])
                o = next((x for x in objectives if x["index"] == idx), None)
                if o is None:
                    o = {"index": idx}
                    objectives.append(o)
                o[head[2]] = v
            elif head[0] == "var":
                variables[head[1]] = v
            else:
                s[f] = v
        s.update({"stages_done": done, "stages_not_done": not_done, "objectives": objectives, "variables": variables})
        if not s["errors"]:
            del s["errors"]
        quests.append(s)
    return {"format": STATE_FORMAT, "source": source or "in-game capture (nvf_capture)",
            "mod_indices": {str(k): v for k, v in sorted(mods.items())}, "quests": quests}


def diff_states(a, b):
    """[(quest, what, value in a, value in b)] for every difference between two state
    documents (quests matched by Editor ID)."""
    out = []
    qa = {q["edid"]: q for q in a["quests"]}
    qb = {q["edid"]: q for q in b["quests"]}
    for e in sorted(set(qa) | set(qb)):
        x, y = qa.get(e), qb.get(e)
        if x is None or y is None:
            out.append((e, "present", x is not None, y is not None))
            continue
        for k in ("running", "completed", "stage", "stages_done"):
            if x.get(k) != y.get(k):
                out.append((e, k, x.get(k), y.get(k)))
        ox = {o["index"]: o for o in x.get("objectives", [])}
        oy = {o["index"]: o for o in y.get("objectives", [])}
        for i in sorted(set(ox) | set(oy)):
            if ox.get(i) != oy.get(i):
                out.append((e, f"objective {i}", ox.get(i), oy.get(i)))
        vx, vy = x.get("variables", {}), y.get("variables", {})
        for n in sorted(set(vx) | set(vy), key=str.lower):
            if vx.get(n) != vy.get(n):
                out.append((e, f"var {n}", vx.get(n), vy.get(n)))
    return out


# --------------------------------------------------------------------------- actions -> job

def literal(v):
    """A numeric literal for a script line, never in exponent form (the compiler's
    handling of `1e-08` is not known)."""
    f = float(v)
    if f != f or f in (float("inf"), float("-inf")):
        raise InputError(f"not a finite number: {v!r}")
    if f.is_integer():
        return str(int(f))
    text = repr(f)
    if "e" in text or "E" in text:
        text = f"{f:.20f}".rstrip("0")
    return text


def _quest(a, i):
    q = a.get("quest")
    if not isinstance(q, str) or not q:
        raise InputError(f"action {i}: needs 'quest' (an Editor ID)")
    return cs.plain_name(q, f"action {i}: the quest")


def _fields(a, i):
    fields = a.get("fields")
    if not isinstance(fields, list) or not fields:
        raise InputError(f"action {i}: needs a non-empty 'fields' list")
    return fields


def _label(a, i):
    label = a.get("label")
    if not isinstance(label, str) or not label or label.startswith(GIVEN_PREFIX):
        raise InputError(f"action {i}: needs a 'label'")
    return label


def _script(quest, ctx, lines, settle=cs.SETTLE_FRAMES):
    return [{"script": quest_script(quest, ctx, lines + ["SetFunctionValue 1"])}, {"frames": settle}]


SIM_OPS = {"add_item", "remove_item", "remove_all_items", "enable", "disable", "set_global", "activate",
           "get_item_count", "get_disabled", "get_global"}
LOCAL_TYPES = ("short", "int", "long", "float")

# Round 2 additions (docs/spec-notes/layer5-captures.md section 8.2).
SETTLE_KILL = 60
SETTLE_PACKAGE = 60
REF_FIELDS = ("disabled", "dead")


def ref_field_expr(r, field):
    """The script expression that reads one field of the reference in ref variable `r`:
    `var.<name>` (its object script's variable, xNVSE's GetVariable called on the
    reference, trial q5b_trial_doc), `disabled`, `dead`."""
    if field == "disabled":
        return f"{r}.GetDisabled"
    if field == "dead":
        return f"{r}.GetDead"
    parts = field.split(".")
    if parts[0] == "var" and len(parts) == 2:
        return f'{r}.GetVariable "{cs.plain_name(parts[1], "an object-script variable")}"'
    raise InputError(f"unknown reference field {field!r}")


def ref_script(names, ctx, body, decls=("float fV",)):
    """A function script with the named forms in ref variables; `body` lines use {0},
    {1}, … for them."""
    n = cs.Names(*ctx)
    ops = [n.operand(x) for x in names]
    return cs.function(n.decls + list(decls), n.setup + [line.format(*ops) for line in body])


def _ref(a, i, key):
    r = a.get(key)
    if not isinstance(r, str) or not r:
        raise InputError(f"action {i}: needs {key!r} (a FormID, an Editor ID or 'player')")
    return r


def read_expr(i, read, slot):
    """(names, field, expression) for the `read` object of activate_read: a reference's
    field ({"ref", "field"}) or a quest's ({"quest", "field"}); the form goes into ref
    variable {slot}."""
    if not isinstance(read, dict) or "field" not in read or ("ref" in read) == ("quest" in read):
        raise InputError(f"action {i}: 'read' needs 'field' and one of 'ref' or 'quest'")
    if "ref" in read:
        ref_field_expr("r", read["field"])
        return read["ref"], read["field"], ref_field_expr(f"{{{slot}}}", read["field"])
    field_expr("q", read["field"])
    return cs.plain_name(read["quest"], f"action {i}: the quest"), read["field"], field_expr(f"{{{slot}}}", read["field"])


def counts_expr(items, container_slot, first_slot):
    """'<count>|<count>|...': the counts of `items` (ref variables {first_slot}, ...)
    held by the actor or container in {container_slot}, as text."""
    return ' + "|" + '.join(f"($ {{{container_slot}}}.GetItemCount {{{first_slot + k}}})" for k in range(len(items)))


def activate_line(a):
    """The Activate statement: with `run_on_activate` the target's OnActivate block runs
    (Activate's second parameter, [documented: GECK wiki Activate]); without it the
    default activation happens and the block does not run [observed: trials
    q5b_trial_test2/3, q5b_trial_doc2/3, docs/spec-notes/layer5-captures.md section 8]."""
    return "{0}.Activate {1} 1" if a.get("run_on_activate") else "{0}.Activate {1}"


def round2_steps(i, a, ctx):
    """Steps for the round 2 operations, or None for any other op."""
    op = a.get("op")
    if op == "activate" and a.get("run_on_activate"):
        steps = [{"script": ref_script([_ref(a, i, "target"), a.get("actor", "player")], ctx,
                                       [activate_line(a), "SetFunctionValue 1"], ())},
                 {"frames": cs.SETTLE_FRAMES_LONG}]
        if a.get("label"):
            # As fnv.sim.actions/1 activate: observed by the container menu it opened.
            steps += [cs.query(cs.function(["float fV"], [f"let fV := MenuMode {cs.CONTAINER_MENU}", "SetFunctionValue fV"]),
                               a["label"], "opened"),
                      {"console": "CloseAllMenus"}, {"frames": cs.SETTLE_FRAMES_LONG}]
        return steps
    if op == "set_ref_var":
        r, var = _ref(a, i, "ref"), cs.plain_name(a.get("var"), f"action {i}: the variable")
        return [{"script": ref_script([r], ctx, [f'{{0}}.SetVariable "{var}" {literal(a["value"])}', "SetFunctionValue 1"])},
                {"frames": cs.SETTLE_FRAMES}]
    if op == "get_ref":
        r, label = _ref(a, i, "ref"), _label(a, i)
        out = []
        for f in _fields(a, i):
            ref_field_expr("r", f)
            out.append(cs.query(ref_script([r], ctx, [f"let fV := {ref_field_expr('{0}', f)}", "SetFunctionValue fV"]),
                                label, f))
        return out
    if op == "activate_read":
        # Activate and a read in the same script: shows whether the OnActivate block has
        # run when Activate returns.
        target, actor, label = _ref(a, i, "target"), a.get("actor", "player"), _label(a, i)
        name, field, expr = read_expr(i, a.get("read"), 2)
        script = ref_script([target, actor, name], ctx, [activate_line(a), f"let fV := {expr}", "SetFunctionValue fV"])
        return [cs.query(script, label, field), {"frames": cs.SETTLE_FRAMES_LONG}]
    if op == "sample_ref_var":
        r, label = _ref(a, i, "ref"), _label(a, i)
        var = cs.plain_name(a.get("var"), f"action {i}: the variable")
        n = int(a.get("frames", 0))
        if n <= 0:
            raise InputError(f"action {i}: sample_ref_var needs positive frames")
        script = ref_script([r], ctx, [f'let fV := {{0}}.GetVariable "{var}"', "SetFunctionValue fV"])
        return [cs.query(script, f"{SAMPLE_PREFIX}:{label}", str(k)) for k in range(n)]
    if op == "add_item_read":
        # AddItem and the counts of `read` items in the same script; no pause after it,
        # so a following sample_items starts on the next frame.
        to, item, label = _ref(a, i, "to"), _ref(a, i, "item"), _label(a, i)
        items = a.get("read") or []
        if not isinstance(items, list) or not items:
            raise InputError(f"action {i}: add_item_read needs a non-empty 'read' list of items")
        script = ref_script([to, item] + items, ctx, [f"{{0}}.AddItem {{1}} {int(a['count'])} 1",
                                                      f"SetFunctionValue ({counts_expr(items, 0, 2)})"], ())
        return [cs.query(script, label, "counts")]
    if op == "sample_items":
        of, label = _ref(a, i, "of"), _label(a, i)
        items = a.get("items") or []
        n = int(a.get("frames", 0))
        if not isinstance(items, list) or not items or n <= 0:
            raise InputError(f"action {i}: sample_items needs 'items' and positive 'frames'")
        script = ref_script([of] + items, ctx, [f"SetFunctionValue ({counts_expr(items, 0, 1)})"], ())
        return [cs.query(script, f"{SAMPLE_PREFIX}:{label}", str(k)) for k in range(n)]
    if op == "menu_gap":
        # A menu held open for `frames` game loops (the container menu of `via`, opened by
        # the player's activation, closed by the console's CloseAllMenus); the quest
        # variable is read in the last loop of the menu and then once per loop.
        via, q, label = _ref(a, i, "via"), _quest(a, i), _label(a, i)
        var = cs.plain_name(a.get("var"), f"action {i}: the variable")
        n, after = int(a.get("frames", 0)), int(a.get("after", 0))
        if n <= 0 or after <= 0:
            raise InputError(f"action {i}: menu_gap needs positive 'frames' and 'after'")
        read = quest_script(q, ctx, [f'let fV := GetVariable "{var}" {{q}}', "SetFunctionValue fV"])
        return ([{"script": ref_script([via], ctx, ["{0}.Activate player", "SetFunctionValue 1"], ())},
                 {"frames": n - 1}, cs.query(read, f"{MENU_PREFIX}:{label}", "before"), {"console": "CloseAllMenus"}]
                + [cs.query(read, f"{SAMPLE_PREFIX}:{label}", str(k)) for k in range(after)])
    if op == "kill":
        return [{"script": ref_script([_ref(a, i, "target")], ctx, ["{0}.Kill", "SetFunctionValue 1"], ())},
                {"frames": SETTLE_KILL}]
    if op == "evaluate_package":
        return [{"script": ref_script([_ref(a, i, "of")], ctx, ["{0}.EvaluatePackage", "SetFunctionValue 1"], ())},
                {"frames": SETTLE_PACKAGE}]
    if op == "is_current_package":
        # xNVSE's GetCurrentPackage compared with the package.
        of, pkg, label = _ref(a, i, "of"), _ref(a, i, "package"), _label(a, i)
        script = ref_script([of, pkg], ctx, ["let rP := {0}.GetCurrentPackage", "let fV := 0", "if rP == {1}",
                                             "  let fV := 1", "endif", "SetFunctionValue fV"], ("ref rP", "float fV"))
        return [cs.query(script, label, "current")]
    return None


def action_steps(i, a, ctx):
    """In-game steps for one fnv.script.actions/1 action (index i). Everything that names
    a quest runs in a job script with the quest in a ref variable: neither job scripts
    nor console lines resolve quest Editor IDs at run time, and quest variables are
    read and written with xNVSE's GetVariable / SetVariable (trials q5_trial and
    q5_trial2, docs/spec-notes/layer5-captures.md section 1)."""
    op = a.get("op")
    if op == "set_stage":
        return _script(_quest(a, i), ctx, [f"SetStage {{q}} {int(a['stage'])}"], cs.SETTLE_FRAMES_LONG)
    if op == "set_stage_read":
        # SetStage and the read in one script: shows whether the stage's result script
        # has run when SetStage returns.
        q, label, field = _quest(a, i), _label(a, i), a.get("field")
        field_expr("rQ", field)
        script = quest_script(q, ctx, [f"SetStage {{q}} {int(a['stage'])}", f"let fV := {field_expr('{q}', field)}",
                                       "SetFunctionValue fV"])
        return [cs.query(script, label, field), {"frames": cs.SETTLE_FRAMES_LONG}]
    if op in ("start_quest", "stop_quest"):
        fn = "StartQuest" if op == "start_quest" else "StopQuest"
        return _script(_quest(a, i), ctx, [f"{fn} {{q}}"])
    if op == "set_quest_var":
        var = cs.plain_name(a.get("var"), f"action {i}: the variable")
        return _script(_quest(a, i), ctx, [f'SetVariable "{var}" {literal(a["value"])} {{q}}'])
    if op == "set_objective":
        q, idx = _quest(a, i), int(a["index"])
        lines = []
        if "displayed" in a:
            lines.append(f"SetObjectiveDisplayed {{q}} {idx} {int(bool(a['displayed']))}")
        if "completed" in a:
            lines.append(f"SetObjectiveCompleted {{q}} {idx} {int(bool(a['completed']))}")
        if not lines:
            raise InputError(f"action {i}: set_objective needs 'displayed' or 'completed'")
        return _script(q, ctx, lines)
    if op == "set_local":
        # A Set statement with a numeric literal into a local variable of the given type,
        # read back in the same script (the game's own compiler and Set statement).
        kind, label = a.get("type"), _label(a, i)
        if kind not in LOCAL_TYPES:
            raise InputError(f"action {i}: set_local type must be one of {', '.join(LOCAL_TYPES)}")
        script = cs.function([f"{kind} xV"], [f"set xV to {literal(a['value'])}", "SetFunctionValue xV"])
        return [cs.query(script, label, "value")]
    if op == "get_quest":
        q, label = _quest(a, i), _label(a, i)
        return [quest_query(q, f, label, ctx) for f in _fields(a, i)]
    if op == "wait_frames":
        return [{"frames": int(a["frames"])}]
    if op == "sample_var":
        # The variable read once per game loop for `frames` loops (one step runs per
        # loop, docs/spec-notes/xnvse-capture.md section 4); the golden keeps how it
        # changed.
        q, label = _quest(a, i), _label(a, i)
        var = cs.plain_name(a.get("var"), f"action {i}: the variable")
        n = int(a["frames"])
        if n <= 0:
            raise InputError(f"action {i}: sample_var needs positive frames")
        script = quest_script(q, ctx, [f'let fV := GetVariable "{var}" {{q}}', "SetFunctionValue fV"])
        return [cs.query(script, f"{SAMPLE_PREFIX}:{label}", str(k)) for k in range(n)]
    if op in SIM_OPS and not (op == "activate" and a.get("run_on_activate")):
        return cs.action_steps(i, a, ctx, time_mode="hour")
    steps = round2_steps(i, a, ctx)
    if steps is not None:
        return steps
    raise InputError(f"action {i}: unknown op {op!r}")


def given_steps(i, g, ctx):
    """Queries checking that the save matches a `given` precondition."""
    label = f"{GIVEN_PREFIX}:{i}"
    kind = g.get("kind")
    if kind == "quest":
        q = cs.plain_name(g.get("quest"), f"given {i}: the quest")
        return [quest_query(q, f, label, ctx) for f in given_fields(g)]
    if kind == "global":
        name = cs.plain_name(g.get("name"), f"given {i}: the global")
        return [read_query(name, label, "value")]
    if kind == "item_count":
        return [cs.query(cs.number_query("{0}.GetItemCount {1}", ctx, [g.get("of"), g.get("item")]), label, "count")]
    if kind == "disabled":
        return [cs.query(cs.number_query("{0}.GetDisabled", ctx, [g.get("of")]), label, "disabled")]
    if kind == "ref":
        r = g.get("ref")
        if not isinstance(r, str) or not r:
            raise InputError(f"given {i}: needs 'ref'")
        return [cs.query(ref_script([r], ctx, [f"let fV := {ref_field_expr('{0}', f)}", "SetFunctionValue fV"]), label, f)
                for f in given_expected(g)]
    raise InputError(f"given {i}: unknown kind {kind!r}")


def given_fields(g):
    fields = [f for f in ("running", "completed", "stage") if f in g]
    fields += [f"done.{n}" for n in g.get("done", [])] + [f"done.{n}" for n in g.get("not_done", [])]
    for o in g.get("objectives", []):
        fields += [f"objective.{o['index']}.{k}" for k in ("displayed", "completed") if k in o]
    fields += [f"var.{n}" for n in g.get("vars", {})]
    return fields


def given_expected(g):
    """{field: expected value} of a given precondition."""
    kind = g.get("kind")
    if kind == "quest":
        out = {f: g[f] for f in ("running", "completed", "stage") if f in g}
        out.update({f"done.{n}": True for n in g.get("done", [])})
        out.update({f"done.{n}": False for n in g.get("not_done", [])})
        for o in g.get("objectives", []):
            out.update({f"objective.{o['index']}.{k}": o[k] for k in ("displayed", "completed") if k in o})
        out.update({f"var.{n}": v for n, v in g.get("vars", {}).items()})
        return out
    if kind == "global":
        return {"value": g["value"]}
    if kind == "item_count":
        return {"count": g["count"]}
    if kind == "disabled":
        return {"disabled": g["disabled"]}
    if kind == "ref":
        out = {f"var.{n}": v for n, v in g.get("vars", {}).items()}
        out.update({f: g[f] for f in REF_FIELDS if f in g})
        return out
    raise InputError(f"unknown given kind {kind!r}")


def actions_job(doc, name, start, edids=None, console_first=()):
    if doc.get("format") != ACTIONS_FORMAT:
        raise InputError(f"not a {ACTIONS_FORMAT} document")
    ctx = (cs.MASTERS, edids)
    steps = [{"console": line} for line in console_first]
    for i, g in enumerate(doc.get("given", [])):
        steps += given_steps(i, g, ctx)
    for i, a in enumerate(doc.get("actions", [])):
        steps += action_steps(i, a, ctx)
    j = cs.job(name, start, steps, cs.MASTERS)
    j["title"] = doc.get("title", name)
    return j


# --------------------------------------------------------------------------- golden

def sample_observation(a, values):
    """What a sample_var action observed: `changes` (how many times the value changed
    within the sampled loops), `drops` (previous minus new value at each of `runs`
    changes after the first `skip`) and, as diagnostics, the loops between changes."""
    changes, frames, last, last_k = [], [], None, None
    for k, v in values:
        if last is not None and v != last:
            changes.append(last - v)
            frames.append(k - last_k)
        if last is None or v != last:
            last_k = k
        last = v
    obs = {"label": a["label"]}
    runs, skip = a.get("runs"), int(a.get("skip", 0))
    diag = {"label": a["label"], "samples": len(values), "changes": len(changes), "loops_between": frames,
            "all_drops": changes, "first": values[0][1] if values else None, "last": values[-1][1] if values else None}
    if a.get("values"):
        # Round 2 addition: the successive distinct values (a state variable's steps).
        obs["values"] = [v for n, (_, v) in enumerate(values) if n == 0 or v != values[n - 1][1]]
    if runs is None:
        obs["changes"] = len(changes)
    else:
        if len(changes) < skip + runs:
            obs["error"] = f"only {len(changes)} changes in {len(values)} loops, {skip} + {runs} wanted"
        else:
            obs["drops"] = changes[skip:skip + runs]
    return obs, diag


def split_counts(text, n):
    """The item counts of a '<count>|<count>' result (integers)."""
    parts = str(text).split("|")
    if len(parts) != n:
        raise InputError(f"expected {n} counts, got {text!r}")
    return [int(round(float(x))) for x in parts]


def items_observation(a, rows):
    """What a sample_items action observed, per item: `first.<item>` (the count at the
    first sample), `final.<item>` (at the last) and `settled.<item>` (the index of the
    first sample from which the count stays at its final value)."""
    items = a["items"]
    bad = [r for r in rows if "error" in r]
    if bad or not rows:
        return {"error": f"{len(bad)} of {len(rows)} samples failed" + (f": {bad[0]['error']}" if bad else "")}
    series = [split_counts(r["value"], len(items)) for r in sorted(rows, key=lambda r: int(r["field"]))]
    obs = {}
    for k, item in enumerate(items):
        values = [row[k] for row in series]
        settled = len(values) - 1
        while settled > 0 and values[settled - 1] == values[-1]:
            settled -= 1
        obs.update({f"first.{item}": values[0], f"final.{item}": values[-1], f"settled.{item}": settled})
    return obs


def menu_gap_observation(a, before, rows):
    """What a menu_gap action observed: `drop`, the variable's value read in the last
    loop of the menu minus its value at the first change after the menu closed."""
    if before is None or "error" in before:
        return {"error": f"the value in the menu was not read: {before and before.get('error')}"}
    bad = [r for r in rows if "error" in r]
    if bad:
        return {"error": f"{len(bad)} samples failed: {bad[0]['error']}"}
    v0 = before["value"]
    for r in sorted(rows, key=lambda r: int(r["field"])):
        if r["value"] != v0:
            return {"drop": v0 - r["value"]}
    return {"error": f"no change in {len(rows)} loops after the menu"}


def golden_document(doc, mods, results, source=None):
    """fnv.script.trace/1 golden: one observation per labelled action (fields typed as
    in the trace contract), the given preconditions checked against the save."""
    acts = doc.get("actions", [])
    by_label, samples, given, menus = {}, {}, {}, {}
    for r in results:
        label = r["label"]
        if label.startswith(MENU_PREFIX + ":"):
            menus[label.split(":", 1)[1]] = r
        elif label.startswith(GIVEN_PREFIX + ":"):
            given.setdefault(int(label.split(":", 1)[1]), {})[r["field"]] = r
        elif label.startswith(SAMPLE_PREFIX + ":"):
            samples.setdefault(label.split(":", 1)[1], []).append(r)
        else:
            by_label.setdefault(label, []).append(r)
    problems = []
    for i, g in enumerate(doc.get("given", [])):
        for f, want in given_expected(g).items():
            r = given.get(i, {}).get(f)
            if r is None or "error" in r:
                problems.append(f"given {i} {f}: {'not recorded' if r is None else r['error']}")
                continue
            got = typed(f, r.get("value"))
            if got != want:
                problems.append(f"given {i} {f}: expected {want!r}, the save has {got!r}")
    observations, diagnostics = [], []
    for i, a in enumerate(acts):
        label = a.get("label")
        if not label:
            continue
        if a.get("op") == "menu_gap":
            observations.append({"label": label, "step": i,
                                 **menu_gap_observation(a, menus.get(label), samples.get(label, []))})
            continue
        if a.get("op") == "sample_items":
            rows = samples.get(label, [])
            observations.append({"label": label, "step": i, **items_observation(a, rows)})
            continue
        if a.get("op") in ("sample_var", "sample_ref_var"):
            rows = samples.get(label, [])
            bad = [r for r in rows if "error" in r]
            values = [(int(r["field"]), r["value"]) for r in rows if "error" not in r]
            obs, diag = sample_observation(a, values)
            if bad:
                obs["error"] = f"{len(bad)} samples failed: {bad[0]['error']}"
            observations.append({"step": i, **obs})
            diagnostics.append(diag)
            continue
        rows = by_label.get(label)
        if rows is None:
            continue
        obs = {"label": label, "step": i}
        for r in rows:
            f = r["field"]
            if a.get("op") in ("get_global",):
                f = "value"
            if "error" in r:
                obs.setdefault("errors", {})[f] = r["error"]
            elif a.get("op") == "add_item_read":
                obs.update({f"count.{k}": n for k, n in zip(a["read"], split_counts(r.get("value"), len(a["read"])))})
            elif a.get("op") == "get_package":
                v = r.get("value")
                obs[f] = cs.to_global(v, mods) if isinstance(v, str) and v.strip() and int(v, 16) else None
            else:
                obs[f] = typed(f, r.get("value"))
        observations.append(obs)
    out = {"format": TRACE_FORMAT, "source": source or "in-game capture (nvf_capture)",
           "mod_indices": {str(k): v for k, v in sorted(mods.items())},
           "observations": observations, "diagnostics": diagnostics}
    return out, problems


# --------------------------------------------------------------------------- CLI

def load_json(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def main(argv=None):
    ap = argparse.ArgumentParser(prog="capture_quests.py", description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("defs", help="quest definitions from fnvtool esm decode")
    p.add_argument("--fnvtool", required=True)
    p.add_argument("--data", required=True, help="the game's Data folder (only read)")
    p.add_argument("--schema", required=True, help="schemas/fnv")
    p.add_argument("--plugin", default="FalloutNV.esm")
    p.add_argument("--quest", action="append", required=True, metavar="EDID")
    p.add_argument("-o", "--output")

    p = sub.add_parser("state-job", help="capture job recording the state of the quests in a defs file")
    p.add_argument("--defs", required=True)
    p.add_argument("--name", default="q5_state")
    p.add_argument("--start", default="Any save (read-only).")
    p.add_argument("-o", "--output")

    p = sub.add_parser("state", help="fnv.quest.state/1 from a state job's output")
    p.add_argument("output_jsonl")
    p.add_argument("--defs", required=True)
    p.add_argument("--source")
    p.add_argument("-o", "--output")

    p = sub.add_parser("diff", help="differences between two state documents")
    p.add_argument("a")
    p.add_argument("b")

    p = sub.add_parser("job", help="capture job from a fnv.script.actions/1 scenario input")
    p.add_argument("actions")
    p.add_argument("--name", required=True)
    p.add_argument("--start", required=True)
    p.add_argument("--data", help="the game's Data folder (only read), to resolve Editor IDs of forms")
    p.add_argument("--defs", help="a defs file whose quests resolve quest Editor IDs without --data")
    p.add_argument("--console-first", action="append", default=[], metavar="LINE")
    p.add_argument("-o", "--output")

    p = sub.add_parser("golden", help="fnv.script.trace/1 golden from a scenario job's output")
    p.add_argument("output_jsonl")
    p.add_argument("--actions", required=True)
    p.add_argument("--source")
    p.add_argument("--allow-given-mismatch", action="store_true",
                   help="write the golden even when the save does not match the scenario's givens")
    p.add_argument("-o", "--output")

    args = ap.parse_args(argv)
    try:
        if args.cmd == "defs":
            path = os.path.join(args.data, args.plugin)
            quests = []
            for e in args.quest:
                q = decode_record(args.fnvtool, path, args.schema, "edid", e)
                s = None
                scri = next((x.get("value") for x in q.get("subrecords", []) if x.get("sig") == "SCRI"), None)
                if scri:
                    s = decode_record(args.fnvtool, path, args.schema, "formid", scri)
                quests.append(quest_defs(q, s))
            cs.write(defs_document(quests, args.plugin), args.output)
        elif args.cmd == "state-job":
            cs.write(state_job(load_json(args.defs), args.name, args.start), args.output)
        elif args.cmd == "state":
            with open(args.output_jsonl, encoding="utf-8") as f:
                mods, results = cs.read_output(f)
            cs.write(state_document(load_json(args.defs), mods, results, args.source), args.output)
        elif args.cmd == "diff":
            for q, what, x, y in diff_states(load_json(args.a), load_json(args.b)):
                print(f"{q}\t{what}\t{json.dumps(x)}\t{json.dumps(y)}")
        elif args.cmd == "job":
            edids = cs.edid_index(args.data) if args.data else {}
            if args.defs:
                edids.update({q["edid"].lower(): int(q["formid"], 16) for q in load_json(args.defs)["quests"]})
            cs.write(actions_job(load_json(args.actions), args.name, args.start, edids, args.console_first), args.output)
        elif args.cmd == "golden":
            with open(args.output_jsonl, encoding="utf-8") as f:
                mods, results = cs.read_output(f)
            doc, problems = golden_document(load_json(args.actions), mods, results, args.source)
            for p_ in problems:
                print(f"capture_quests: {p_}", file=sys.stderr)
            if problems and not args.allow_given_mismatch:
                return 1
            cs.write(doc, args.output)
    except (OSError, json.JSONDecodeError, InputError, KeyError, ValueError) as e:
        print(f"capture_quests: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
