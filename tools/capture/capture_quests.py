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

BOOL_FIELDS = ("running", "completed", "disabled", "opened", "dead", "current", "destroyed", "found")


def typed(field, v):
    """A recorded number as the type its field has in fnv.script.trace/1 and
    fnv.quest.state/1: booleans for flags, integers for stages and counts, variables
    and globals as recorded."""
    if v is None or isinstance(v, str):
        return v
    head = field.split(".")[0]
    if field in BOOL_FIELDS or head in ("done", "menu", "tag") or (head == "objective" and field.count(".") == 2):
        return bool(v)
    if field in ("stage", "count", "frames", "locked") or head in ("threshold", "count"):
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
           "get_item_count", "get_disabled", "get_global", "get_av", "set_av"}
LOCAL_TYPES = ("short", "int", "long", "float")

# Round 2 additions (docs/spec-notes/layer5-captures.md section 8.2).
SETTLE_KILL = 60
SETTLE_PACKAGE = 60
REF_FIELDS = ("disabled", "dead", "destroyed")


def ref_field_expr(r, field):
    """The script expression that reads one field of the reference in ref variable `r`:
    `var.<name>` (its object script's variable, xNVSE's GetVariable called on the
    reference, trial q5b_trial_doc), `disabled`, `dead`."""
    if field == "disabled":
        return f"{r}.GetDisabled"
    if field == "dead":
        return f"{r}.GetDead"
    if field == "destroyed":
        # Round 4: SetDestroyed by VCG01 stages 0 and 65 (GetDestroyed [documented: GECK wiki]).
        return f"{r}.GetDestroyed"
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


# Round 3 additions (docs/spec-notes/layer5-captures.md section 9).
CLOCK_PREFIX = "clock"
# The second argument of the reputation functions: 1 fame, 0 infamy [documented: GECK
# wiki AddReputation, GetReputation; observed: trial q5c_trial_rep].
REP_AXES = {"fame": 1, "infamy": 0}
# GetReputationThreshold's second argument [documented: GECK wiki GetReputationThreshold].
REP_THRESHOLDS = {"mixed": 0, "good": 1, "bad": 2}
# The stopwatch: VCG00 (FalloutNV 0x00102037), whose GameMode counts fTimer down by
# GetSecondsPassed every frame while bRunTimer is 1 (delay 0.01; section 5.4). The job
# starts it with fTimer 100000 before the first action; no observation reads VCG00.
STOPWATCH_QUEST = "0x00102037"
STOPWATCH_START = 100000
SETTLE_MENU_CLOSE = 5


def rep_field_expr(r, field):
    """The script expression that reads one reputation field of the REPU in ref variable
    `r`: fame, infamy (GetReputation), pct.fame, pct.infamy (GetReputationPct),
    threshold.mixed / .good / .bad (GetReputationThreshold)."""
    parts = field.split(".")
    if len(parts) == 1 and field in REP_AXES:
        return f"GetReputation {r} {REP_AXES[field]}"
    if len(parts) == 2 and parts[0] == "pct" and parts[1] in REP_AXES:
        return f"GetReputationPct {r} {REP_AXES[parts[1]]}"
    if len(parts) == 2 and parts[0] == "threshold" and parts[1] in REP_THRESHOLDS:
        return f"GetReputationThreshold {r} {REP_THRESHOLDS[parts[1]]}"
    raise InputError(f"unknown reputation field {field!r}")


def _axis(a, i):
    axis = a.get("axis")
    if axis not in REP_AXES:
        raise InputError(f"action {i}: 'axis' must be 'fame' or 'infamy'")
    return REP_AXES[axis]


def _size(a, i):
    size = a.get("size")
    if not isinstance(size, int) or isinstance(size, bool):
        raise InputError(f"action {i}: 'size' must be an integer (the deed size)")
    return size


def joined(exprs):
    """'<v>|<v>|...' of the expressions, as text ($ gives six significant digits)."""
    return ' + "|" + '.join(f"($ ({e}))" for e in exprs)


def item_reads(i, reads, slot0):
    """(names, fields, expressions) for the `read` list of use_item: {"av": name} (the
    actor's actor value, field `av.<name>`) or {"item": form} (its item count, field
    `count.<form>`); the actor is ref slot {0}, items from slot0 on."""
    if not isinstance(reads, list) or not reads:
        raise InputError(f"action {i}: needs a non-empty 'read' list")
    names, fields, exprs = [], [], []
    for r in reads:
        if isinstance(r, dict) and "av" in r and len(r) == 1:
            av = cs.plain_name(r["av"], f"action {i}: the actor value")
            fields.append(f"av.{av}")
            exprs.append(f"{{0}}.GetAV {av}")
        elif isinstance(r, dict) and "item" in r and len(r) == 1:
            fields.append(f"count.{r['item']}")
            exprs.append(f"{{0}}.GetItemCount {{{slot0 + len(names)}}}")
            names.append(r["item"])
        else:
            raise InputError(f"action {i}: a read is {{\"av\": name}} or {{\"item\": form}}, got {r!r}")
    return names, fields, exprs


def stopwatch_expr(slot):
    """Seconds on the stopwatch (VCG00's countdown from STOPWATCH_START, in ref slot)."""
    return f'{STOPWATCH_START} - (GetVariable "fTimer" {{{slot}}})'


def stopwatch_steps(ctx):
    """Steps that start the stopwatch (before the first action of a job that uses it)."""
    return [{"script": quest_script(STOPWATCH_QUEST, ctx, [f'SetVariable "fTimer" {STOPWATCH_START} {{q}}',
                                                          'SetVariable "bRunTimer" 1 {q}', "StartQuest {q}",
                                                          "SetFunctionValue 1"])},
            {"frames": cs.SETTLE_FRAMES_LONG}]


def uses_stopwatch(doc):
    return any(a.get("op") in ("wait_av", "sample_av") or (a.get("op") == "use_item" and a.get("stopwatch"))
               for a in doc.get("actions", []))


def round3_steps(i, a, ctx):
    """Steps for the round 3 operations, or None for any other op."""
    op = a.get("op")
    if op == "set_reputation":
        r = _ref(a, i, "rep")
        return [{"script": ref_script([r], ctx, [f"SetReputation {{0}} {_axis(a, i)} {literal(a['value'])}",
                                                 "SetFunctionValue 1"], ())}, {"frames": cs.SETTLE_FRAMES}]
    if op in ("add_reputation", "remove_reputation"):
        r, fn = _ref(a, i, "rep"), "AddReputation" if op == "add_reputation" else "RemoveReputation"
        return [{"script": ref_script([r], ctx, [f"{fn} {{0}} {_axis(a, i)} {_size(a, i)}", "SetFunctionValue 1"], ())},
                {"frames": cs.SETTLE_FRAMES}]
    if op == "get_reputation":
        r, label = _ref(a, i, "rep"), _label(a, i)
        out = []
        for f in _fields(a, i):
            rep_field_expr("r", f)
            out.append(cs.query(ref_script([r], ctx, [f"let fV := {rep_field_expr('{0}', f)}", "SetFunctionValue fV"]),
                                label, f))
        return out
    if op == "set_reputation_read":
        # Both axes set and the fields read in one script (one frame, no pause).
        r, label, fields = _ref(a, i, "rep"), _label(a, i), _fields(a, i)
        for f in fields:
            rep_field_expr("r", f)
        lines = [f"SetReputation {{0}} 1 {literal(a['fame'])}", f"SetReputation {{0}} 0 {literal(a['infamy'])}",
                 f"SetFunctionValue ({joined(rep_field_expr('{0}', f) for f in fields)})"]
        return [cs.query(ref_script([r], ctx, lines, ()), label, "|".join(fields))]
    if op == "use_item":
        # The actor uses (eats, drinks, injects) an aid item: EquipItem with an
        # ingestible consumes it [documented: GECK wiki EquipItem]; the reads follow in
        # the same script. With `stopwatch`, the stopwatch is read there too.
        actor, item, label = a.get("actor", "player"), _ref(a, i, "item"), _label(a, i)
        names, fields, exprs = item_reads(i, a.get("read"), 2)
        if a.get("stopwatch"):
            exprs = exprs + [stopwatch_expr(2 + len(names))]
            names = names + [STOPWATCH_QUEST]
        script_ = ref_script([actor, item] + names, ctx, ["{0}.EquipItem {1} 0 1", f"SetFunctionValue ({joined(exprs)})"], ())
        return [cs.query(script_, label, "|".join(fields)), {"frames": cs.SETTLE_FRAMES}]
    if op == "wait_av":
        # Waits (once per frame, at most max_frames) until the actor value equals
        # `value`, then reads the stopwatch and the value.
        of, label = a.get("of", "player"), _label(a, i)
        av = cs.plain_name(a.get("av"), f"action {i}: the actor value")
        n = int(a.get("max_frames", 0))
        if n <= 0:
            raise InputError(f"action {i}: wait_av needs positive max_frames")
        until = ref_script([of], ctx, ["let fV := 0", f"if {{0}}.GetAV {av} == {literal(a['value'])}", "  let fV := 1",
                                       "endif", "SetFunctionValue fV"])
        return [{"until": until, "max_frames": n},
                cs.query(ref_script([of, STOPWATCH_QUEST], ctx, [f"SetFunctionValue ({joined([stopwatch_expr(1), f'{{0}}.GetAV {av}'])})"], ()),
                         f"{CLOCK_PREFIX}:{label}", "seconds|value")]
    if op == "hold_menu":
        # The player activates `via` (its container menu opens); the menu stays open for
        # `frames` frames counting the opening one; the console's CloseAllMenus closes it.
        via, n = _ref(a, i, "via"), int(a.get("frames", 0))
        if n <= 0:
            raise InputError(f"action {i}: hold_menu needs positive frames")
        return [{"script": ref_script([via], ctx, ["{0}.Activate player", "SetFunctionValue 1"], ())},
                {"frames": n}, {"console": "CloseAllMenus"}, {"frames": SETTLE_MENU_CLOSE}]
    if op in ("damage_av", "restore_av"):
        of = a.get("of", "player")
        av = cs.plain_name(a.get("av"), f"action {i}: the actor value")
        fn = "DamageAV" if op == "damage_av" else "RestoreAV"
        return [{"script": ref_script([of], ctx, [f"{{0}}.{fn} {av} {literal(a['value'])}", "SetFunctionValue 1"], ())},
                {"frames": cs.SETTLE_FRAMES}]
    if op in ("add_spell", "remove_spell"):
        of, spell = a.get("of", "player"), _ref(a, i, "spell")
        fn = "AddSpell" if op == "add_spell" else "RemoveSpell"
        return [{"script": ref_script([of, spell], ctx, [f"{{0}}.{fn} {{1}}", "SetFunctionValue 1"], ())},
                {"frames": cs.SETTLE_FRAMES}]
    if op == "sample_av_damage":
        # Once per frame: the actor value is read, then damaged by `damage`; shows in
        # which frames something restored it.
        of, label = a.get("of", "player"), _label(a, i)
        av = cs.plain_name(a.get("av"), f"action {i}: the actor value")
        n = int(a.get("frames", 0))
        if n <= 0:
            raise InputError(f"action {i}: sample_av_damage needs positive frames")
        script_ = ref_script([of], ctx, [f"let fV := {{0}}.GetAV {av}", f"{{0}}.DamageAV {av} {literal(a['damage'])}",
                                         "SetFunctionValue fV"])
        return [cs.query(script_, f"{SAMPLE_PREFIX}:{label}", str(k)) for k in range(n)]
    if op == "sample_av":
        # Once per frame: the actor value and the stopwatch.
        of, label = a.get("of", "player"), _label(a, i)
        av = cs.plain_name(a.get("av"), f"action {i}: the actor value")
        n = int(a.get("frames", 0))
        if n <= 1:
            raise InputError(f"action {i}: sample_av needs at least 2 frames")
        script_ = ref_script([of, STOPWATCH_QUEST], ctx, [f"SetFunctionValue ({joined([f'{{0}}.GetAV {av}', stopwatch_expr(1)])})"], ())
        return [cs.query(script_, f"{SAMPLE_PREFIX}:{label}", str(k)) for k in range(n)]
    if op == "go_to_cell":
        # The console's coc (a teleport to the cell, by its Editor ID), then a pause for
        # the cell to load.
        cell = cs.plain_name(a.get("cell"), f"action {i}: the cell (an Editor ID)")
        return [{"console": f"coc {cell}"}, {"frames": GO_TO_CELL_FRAMES}]
    if op == "sample_jumps":
        # Once per frame: every read, as text; the golden keeps in which sample each read
        # first jumped (changed by at least min_step).
        label = _label(a, i)
        names, exprs = [], []
        for r in a.get("reads") or []:
            names, e = jump_read(i, r, names)
            exprs.append(e)
        n = int(a.get("frames", 0))
        if len(exprs) < 2 or n <= 1:
            raise InputError(f"action {i}: sample_jumps needs at least two reads and two frames")
        float(a["min_step"])
        script_ = ref_script(names, ctx, [f"SetFunctionValue ({joined(exprs)})"], ())
        return [cs.query(script_, f"{SAMPLE_PREFIX}:{label}", str(k)) for k in range(n)]
    return None


GO_TO_CELL_FRAMES = 600

# Round 4 additions (docs/spec-notes/layer5-captures.md section 10: the intro chain from
# the new-game save nvfnew).
WAIT_PREFIX = "wait"
BOX_PREFIX = "box"
# The message-box menu: open while a ShowMessage box is shown [observed: trial
# q5d_trial_open, the pack plugins' boxes and the GECK wiki's MenuMode list
# [documented]].
MESSAGE_MENU = 1001
# The round 4 stopwatch: the skull in Doc Mitchell's house (FalloutNV 0x00104C10,
# VCG01BrahminSkullSCRIPT counts `timer` down by GetSecondsPassed in GameMode while
# `runTimer` is 1; with `grabbed` 0 nothing happens when it runs out). An object script,
# because VCG00 (round 3's stopwatch) is itself observed here; it needs the player in
# Doc's house, where every round 4 input stays.
WATCH_REF = "0x00104C10"
SETTLE_CLOSE = 3


def watch_expr(slot):
    return f'{STOPWATCH_START} - {{{slot}}}.GetVariable "timer"'


def wait_field_expr(slot, field):
    """The expression of a wait_quest field ({slot} = the quest), as a quest field."""
    return field_expr(f"{{{slot}}}", field)


def round4_steps(i, a, ctx):
    """Steps for the round 4 operations, or None for any other op."""
    op = a.get("op")
    if op == "start_stopwatch":
        return [{"script": ref_script([WATCH_REF], ctx, [f'{{0}}.SetVariable "timer" {STOPWATCH_START}',
                                                         '{0}.SetVariable "runTimer" 1', "SetFunctionValue 1"], ())},
                {"frames": 2}]
    if op == "close_menus":
        # The console's CloseAllMenus: closes the open menu (message box, name, race,
        # tag-skill, trait and SPECIAL menus) like the player's OK / Done
        # [observed: trials q5d_trial_open, q5d_trial_drive1/2].
        return [{"console": "CloseAllMenus"}, {"frames": SETTLE_CLOSE}]
    if op == "dismiss_message_boxes":
        # Up to `max` times: wait (once per frame, at most wait_frames) for a message box,
        # note whether one is open, close menus. `closed` counts the boxes closed.
        label = _label(a, i) if a.get("label") is not None else None
        n, wait = int(a.get("max", 0)), int(a.get("wait_frames", 0))
        if n <= 0 or wait <= 0:
            raise InputError(f"action {i}: dismiss_message_boxes needs positive 'max' and 'wait_frames'")
        probe = cs.function(["float fV"], ["let fV := 0", f"if MenuMode {MESSAGE_MENU}", "  let fV := 1", "endif",
                                           "SetFunctionValue fV"])
        out = []
        for k in range(n):
            out += [{"until": probe, "max_frames": wait}]
            if label:
                out.append(cs.query(probe, f"{BOX_PREFIX}:{label}", str(k)))
            out += [{"console": "CloseAllMenus"}, {"frames": SETTLE_CLOSE}]
        return out
    if op in ("wait_quest", "wait_menu"):
        # Polled once per frame (at most max_frames) until the condition holds; in the
        # next frame the condition and, with `stopwatch`, the stopwatch are read.
        label, n = _label(a, i), int(a.get("max_frames", 0))
        if n <= 0:
            raise InputError(f"action {i}: {op} needs positive max_frames")
        if op == "wait_quest":
            q, field = _quest(a, i), a.get("field")
            field_expr("q", field)
            want = literal(1 if a.get("value", True) is True else 0 if a.get("value") is False else a["value"])
            names, test = [q], f"{wait_field_expr(0, field)} == {want}"
        else:
            names, test = [], f"MenuMode {int(a['menu'])}"
        lines = ["let fV := 0", f"if {test}", "  let fV := 1", "endif"]
        until = ref_script(names, ctx, lines + ["SetFunctionValue fV"])
        exprs = ["fV"] + ([watch_expr(len(names))] if a.get("stopwatch") else [])
        read = ref_script(names + ([WATCH_REF] if a.get("stopwatch") else []), ctx,
                          lines + [f"SetFunctionValue ({joined(exprs)})"])
        return [{"until": until, "max_frames": n},
                cs.query(read, f"{WAIT_PREFIX}:{label}", "found|seconds" if a.get("stopwatch") else "found")]
    if op == "get_menus":
        label, codes = _label(a, i), a.get("menus")
        if not isinstance(codes, list) or not codes:
            raise InputError(f"action {i}: get_menus needs a non-empty 'menus' list of codes")
        return [cs.query(cs.function(["float fV"], ["let fV := 0", f"if MenuMode {int(c)}", "  let fV := 1", "endif",
                                                    "SetFunctionValue fV"]), label, f"menu.{int(c)}") for c in codes]
    if op == "get_tag_skills":
        label, skills = _label(a, i), a.get("skills")
        if not isinstance(skills, list) or not skills:
            raise InputError(f"action {i}: get_tag_skills needs a non-empty 'skills' list")
        return [cs.query(cs.function(["float fV"], [f"let fV := IsPlayerTagSkill {cs.plain_name(s, 'a skill')}",
                                                    "SetFunctionValue fV"]), label, f"tag.{s}") for s in skills]
    return None


def jump_read(i, r, names):
    """(names, expression) for one read of sample_jumps: {"global": name},
    {"ref": form, "var": name} (an object-script variable) or {"quest": edid, "var":
    name}; forms are appended to `names` (ref slots in order)."""
    if not isinstance(r, dict):
        raise InputError(f"action {i}: a read must be an object, got {r!r}")
    if set(r) == {"global"}:
        return names, cs.plain_name(r["global"], f"action {i}: the global")
    if set(r) == {"ref", "var"}:
        var = cs.plain_name(r["var"], f"action {i}: the variable")
        return names + [r["ref"]], f'{{{len(names)}}}.GetVariable "{var}"'
    if set(r) == {"quest", "var"}:
        var = cs.plain_name(r["var"], f"action {i}: the variable")
        q = cs.plain_name(r["quest"], f"action {i}: the quest")
        return names + [q], f'GetVariable "{var}" {{{len(names)}}}'
    raise InputError(f"action {i}: a read is {{global}}, {{ref, var}} or {{quest, var}}, got {r!r}")


def jumps_observation(a, rows):
    """What a sample_jumps action observed: `lags`, for each read after the first, the
    sample index of its first jump minus that of the first read (None when either never
    jumped)."""
    bad = [r for r in rows if "error" in r]
    if bad or not rows:
        return {"error": f"{len(bad)} of {len(rows)} samples failed" + (f": {bad[0]['error']}" if bad else "")}
    n, step = len(a["reads"]), float(a["min_step"])
    series = [split_values(r["value"], n) for r in sorted(rows, key=lambda r: int(r["field"]))]
    first = []
    for k in range(n):
        at = next((j for j in range(1, len(series)) if abs(series[j][k] - series[j - 1][k]) >= step), None)
        first.append(at)
    if first[0] is None:
        return {"error": "the first read never jumped"}
    return {"lags": [None if f is None else f - first[0] for f in first[1:]]}


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
        if a.get("stopwatch"):
            # Round 4: the stopwatch (re)started in the same script (the same frame).
            return [{"script": ref_script([_quest(a, i), WATCH_REF], ctx,
                                          [f'SetVariable "{var}" {literal(a["value"])} {{0}}',
                                           f'{{1}}.SetVariable "timer" {STOPWATCH_START}', '{1}.SetVariable "runTimer" 1',
                                           "SetFunctionValue 1"], ())}, {"frames": cs.SETTLE_FRAMES}]
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
    if steps is None:
        steps = round3_steps(i, a, ctx)
    if steps is None:
        steps = round4_steps(i, a, ctx)
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
    if kind == "reputation":
        r = g.get("rep")
        if not isinstance(r, str) or not r:
            raise InputError(f"given {i}: needs 'rep'")
        return [cs.query(ref_script([r], ctx, [f"let fV := {rep_field_expr('{0}', f)}", "SetFunctionValue fV"]), label, f)
                for f in given_expected(g)]
    if kind == "actor_value":
        av = cs.plain_name(g.get("av"), f"given {i}: the actor value")
        return [cs.query(cs.number_query(f"{{0}}.GetAV {av}", ctx, [g.get("of", "player")]), label, "value")]
    if kind == "menu":
        # Round 4: a menu open at the start (nvfnew loads with a pack plugin's message
        # box open, which holds GameMode until it is closed).
        return [cs.query(cs.function(["float fV"], ["let fV := 0", f"if MenuMode {int(g['menu'])}", "  let fV := 1",
                                                    "endif", "SetFunctionValue fV"]), label, "open")]
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
    if kind == "reputation":
        return {f: g[f] for f in REP_AXES if f in g}
    if kind == "actor_value":
        return {"value": g["value"]}
    if kind == "menu":
        return {"open": bool(g["open"])}
    raise InputError(f"unknown given kind {kind!r}")


def actions_job(doc, name, start, edids=None, console_first=()):
    if doc.get("format") != ACTIONS_FORMAT:
        raise InputError(f"not a {ACTIONS_FORMAT} document")
    ctx = (cs.MASTERS, edids)
    steps = [{"console": line} for line in console_first]
    for i, g in enumerate(doc.get("given", [])):
        steps += given_steps(i, g, ctx)
    if uses_stopwatch(doc):
        steps += stopwatch_steps(ctx)
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


def split_values(text, n):
    """The numbers of a '<v>|<v>|...' result."""
    parts = str(text).split("|")
    if len(parts) != n:
        raise InputError(f"expected {n} values, got {text!r}")
    return [float(x) for x in parts]


def av_sample_observation(rows):
    """What a sample_av action observed: `changes` (how many samples differ from the one
    before) and `per_second` (the change of the value over the sampled span divided by
    the stopwatch seconds of that span)."""
    bad = [r for r in rows if "error" in r]
    if bad or len(rows) < 2:
        return {"error": f"{len(bad)} of {len(rows)} samples failed" + (f": {bad[0]['error']}" if bad else "")}
    series = [split_values(r["value"], 2) for r in sorted(rows, key=lambda r: int(r["field"]))]
    changes = sum(1 for p, q in zip(series, series[1:]) if q[0] != p[0])
    span = series[-1][1] - series[0][1]
    if span <= 0:
        return {"changes": changes, "error": "the stopwatch did not run"}
    return {"changes": changes, "per_second": (series[-1][0] - series[0][0]) / span}


def wait_observation(a, r, zero=0.0):
    """What a wait_quest / wait_menu action observed: `found` (the condition, read in the
    frame after the wait ended) and with `stopwatch` `seconds` (the stopwatch then)."""
    if r is None or "error" in r:
        return {"error": f"not recorded: {r and r.get('error')}"}
    if a.get("stopwatch"):
        found, seconds = split_values(r["value"], 2)
        obs = {"found": bool(found)}
        if found:
            obs["seconds"] = seconds - zero
        return obs
    # The read is text ($ of the condition, `joined`), "0" or "1".
    return {"found": float(r["value"]) != 0}


def boxes_observation(rows):
    """What a dismiss_message_boxes action observed: `closed`, the boxes that were open
    when closed."""
    bad = [r for r in rows if "error" in r]
    if bad or not rows:
        return {"error": f"{len(bad)} of {len(rows)} reads failed" + (f": {bad[0]['error']}" if bad else "")}
    return {"closed": sum(1 for r in rows if r["value"])}


def golden_document(doc, mods, results, source=None):
    """fnv.script.trace/1 golden: one observation per labelled action (fields typed as
    in the trace contract), the given preconditions checked against the save."""
    acts = doc.get("actions", [])
    by_label, samples, given, menus, clocks, waits, boxes = {}, {}, {}, {}, {}, {}, {}
    for r in results:
        label = r["label"]
        if label.startswith(WAIT_PREFIX + ":"):
            waits[label.split(":", 1)[1]] = r
        elif label.startswith(BOX_PREFIX + ":"):
            boxes.setdefault(label.split(":", 1)[1], []).append(r)
        elif label.startswith(CLOCK_PREFIX + ":"):
            clocks[label.split(":", 1)[1]] = r
        elif label.startswith(MENU_PREFIX + ":"):
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
    started = None  # stopwatch seconds at the last use_item with `stopwatch`
    watch_zero = 0.0  # round 4: start_stopwatch sets the skull's timer to its start value
    for i, a in enumerate(acts):
        label = a.get("label")
        if not label:
            continue
        if a.get("op") in ("wait_quest", "wait_menu"):
            observations.append({"label": label, "step": i, **wait_observation(a, waits.get(label), watch_zero)})
            continue
        if a.get("op") == "dismiss_message_boxes":
            observations.append({"label": label, "step": i, **boxes_observation(boxes.get(label, []))})
            continue
        if a.get("op") == "wait_av":
            c = clocks.get(label)
            obs = {"label": label, "step": i}
            if c is None or "error" in c:
                obs["error"] = f"not recorded: {c and c.get('error')}"
            elif started is None:
                obs["error"] = "no use_item with a stopwatch before it"
            else:
                seconds, value = split_values(c["value"], 2)
                obs["found"] = value == float(a["value"])
                if obs["found"]:
                    obs["seconds"] = seconds - started
            observations.append(obs)
            continue
        if a.get("op") == "sample_av_damage":
            rows = sorted(samples.get(label, []), key=lambda r: int(r["field"]))
            bad = [r for r in rows if "error" in r]
            obs = {"label": label, "step": i}
            if bad or not rows:
                obs["error"] = f"{len(bad)} of {len(rows)} samples failed"
            else:
                obs["values"] = [r["value"] for r in rows]
            observations.append(obs)
            continue
        if a.get("op") == "sample_jumps":
            observations.append({"label": label, "step": i, **jumps_observation(a, samples.get(label, []))})
            continue
        if a.get("op") == "sample_av":
            observations.append({"label": label, "step": i, **av_sample_observation(samples.get(label, []))})
            continue
        if a.get("op") in ("use_item", "set_reputation_read"):
            rows = by_label.get(label) or []
            obs = {"label": label, "step": i}
            if not rows or "error" in rows[0]:
                obs["error"] = rows[0]["error"] if rows else "not recorded"
            else:
                fields = rows[0]["field"].split("|")
                values = split_values(rows[0]["value"], len(fields) + (1 if a.get("stopwatch") else 0))
                if a.get("stopwatch"):
                    started = values.pop()
                obs.update({f: typed(f, v) for f, v in zip(fields, values)})
            observations.append(obs)
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
