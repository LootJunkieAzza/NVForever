#!/usr/bin/env python3
"""JIP LN NVSE probes without the GECK (docs/handoff/jip-tasks.md, task J3).

  jip_probes.py job [--set S] [-o JOB.steps.json]       write the capture job
  jip_probes.py collect OUT.jsonl [--set S] [-o golden]  capture output -> fnv.ext.probes/1
  jip_probes.py compare GOLDEN.json                the golden against the expectations here

Each probe is a function script that nvf_capture compiles inside the running game and
whose result it records (docs/spec-notes/xnvse-capture.md §4–§5): JIP is loaded in the
owner's game, so read-only JIP calls with number, string or inline-array arguments can be
probed unattended through capture_run.py. Only getters are probed here: nothing that
changes the game, the save or a file. A probe's expected value is filled only where the
function's wiki page states the exact result [documented: the page named]; the others are
recorded as observed. The golden goes to `<goldens>/jip/<set>.json` (private).

Standard library only.
"""
import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import capture_steps as cs  # noqa: E402
import ext_probes as ep  # noqa: E402


# (id, script body after `Begin Function { }`, text result?, expected or None, page)
# A body that needs a ref variable declares it before Begin (only that works at run time).
PROBES = [
    ("fsin.90", "SetFunctionValue (fsin 90)", False, "1", "Fsin (degrees)"),
    ("fsin.30", "SetFunctionValue (fsin 30)", False, "0.5", "Fsin"),
    ("fcos.60", "SetFunctionValue (fcos 60)", False, "0.5", "Fcos (degrees)"),
    ("fcos.0", "SetFunctionValue (fcos 0)", False, "1", "Fcos"),
    ("ftan.45", "SetFunctionValue (ftan 45)", False, "1", "Ftan (degrees)"),
    ("fasin.1", "SetFunctionValue (fasin 1)", False, "90", "Fasin (degrees)"),
    ("fasin.out", "SetFunctionValue (fasin 2)", False, None, ""),
    ("facos.0", "SetFunctionValue (facos 0)", False, "90", "Facos (degrees)"),
    ("fatan.1", "SetFunctionValue (fatan 1)", False, "45", "Fatan (degrees)"),
    ("fatan2.q1", "SetFunctionValue (fatan2 1 1)", False, "45", "Fatan2 (y, x)"),
    ("fatan2.q2", "SetFunctionValue (fatan2 1 -1)", False, "135", "Fatan2 (quadrant from both signs)"),
    ("fatan2.q3", "SetFunctionValue (fatan2 -1 -1)", False, "-135", "Fatan2"),
    ("fsqrt.16", "SetFunctionValue (fsqrt 16)", False, "4", "Fsqrt"),
    ("fsqrt.neg", "SetFunctionValue (fsqrt -1)", False, None, ""),
    ("maxof.3", "SetFunctionValue (GetMaxOf 1 5 3)", False, "5", "GetMaxOf"),
    ("maxof.neg", "SetFunctionValue (GetMaxOf -1 -5)", False, "-1", "GetMaxOf"),
    ("minof.3", "SetFunctionValue (GetMinOf 1 5 3)", False, "1", "GetMinOf"),
    ("minof.5", "SetFunctionValue (GetMinOf 9 8 7 6 5)", False, "5", "GetMinOf (up to five values)"),
    ("av.unknown", "SetFunctionValue (StringToActorValue \"NotAnActorValue\")", False, "-1", "StringToActorValue (-1 for no match)"),
    ("av.strength", "SetFunctionValue (StringToActorValue \"Strength\")", False, None, ""),
    ("av.damagethreshold", "SetFunctionValue (StringToActorValue \"DamageThreshold\")", False, None, ""),
    ("av.case", "SetFunctionValue (StringToActorValue \"strength\")", False, None, ""),
    ("modname.0", "SetFunctionValue (GetModName 0)", True, "FalloutNV", "GetModName"),
    ("modname.0ext", "SetFunctionValue (GetModName 0 1)", True, "FalloutNV.esm", "GetModName (1 keeps the extension)"),
    ("modname.99", "SetFunctionValue (GetModName 99)", True, None, ""),
    ("headerversion.fnv", "SetFunctionValue (GetPluginHeaderVersion \"FalloutNV.esm\")", False, None, ""),
    ("validatemod.0", "SetFunctionValue (ValidateModIndex 0)", False, None, ""),
    ("validatemod.200", "SetFunctionValue (ValidateModIndex 200)", False, None, ""),
    ("formfrommod.player", "ref rF\nBegin Function { }\nlet rF := GetFormFromMod \"FalloutNV.esm\" \"14\"\nSetFunctionValue (GetFormIDString rF)", True, "00000014", "GetFormFromMod (ID without the mod index)"),
    ("formfrommod.zeros", "ref rF\nBegin Function { }\nlet rF := GetFormFromMod \"FalloutNV.esm\" \"000014\"\nSetFunctionValue (GetFormIDString rF)", True, "00000014", "GetFormFromMod (leading zeros may be dropped)"),
    ("stringtoref.player", "ref rF\nBegin Function { }\nlet rF := StringToRef \"00000014\"\nSetFunctionValue (GetFormIDString rF)", True, "00000014", "StringToRef (from GetFormIDString's text)"),
    ("fileexists.bsa", "SetFunctionValue (FileExists \"meshes\\ammo\\10mmammo.nif\")", False, None, ""),
    ("fileexists.none", "SetFunctionValue (FileExists \"meshes\\nvforever_no_such_file.nif\")", False, "0", "FileExists"),
    ("fileexists.folder", "SetFunctionValue (FileExists \"NVSE\" 1)", False, None, ""),
    ("lnversion", "SetFunctionValue (GetLNVersion)", False, "0", "GetLNVersion (always 0)"),
    ("nvseversionfull", "SetFunctionValue (GetNVSEVersionFull)", False, None, ""),
    ("random.single", "SetFunctionValue (GetRandomInRange 5 6)", False, "5", "GetRandomInRange (min <= x < max)"),
    ("gravity.x", "SetFunctionValue (GetLocalGravity X)", False, "0", "GetLocalGravity (X and Y are 0 unless set)"),
    ("gravity.z", "SetFunctionValue (GetLocalGravity Z)", False, None, ""),
    ("debugmode.tgm", "SetFunctionValue (GetDebugModeState 0)", False, None, ""),
    ("difficulty", "SetFunctionValue (GetGameDifficulty)", False, None, ""),
    ("dayspassed.same", "SetFunctionValue ((GetGameDaysPassed) == (GetGameDaysPassed 2281 10 13))", False, "1", "GetGameDaysPassed (no arguments = since 13 October 2281)"),
]


def _form(var, fid):
    """A ref variable set to a FalloutNV.esm form (ID without the mod index)."""
    return f'let {var} := GetFormFromMod "FalloutNV.esm" "{fid}"\n'


def _with(fid, body):
    return "ref rF\nBegin Function { }\n" + _form("rF", fid) + body


SUB = "This subject has a submarine as a subsequence"
# Second set: JIP calls that take forms or strings (batch 2). The forms are vanilla
# FalloutNV.esm records whose IDs come from `fnvtool esm inventory` [observed]; editor IDs
# cannot be used at run time, so each is fetched with GetFormFromMod. Long game text
# (descriptions, objective text) is recorded by its length only. A string_var parameter
# refuses a literal string ("script did not compile"), so the regex targets go through
# sv_Construct [observed: jip_rx_trial, 2026-10-26].
FORM_PROBES = [
    ("regex.match", f'SetFunctionValue (sv_RegexMatch (sv_Construct "Whatchamacallit") "(What)(.*)")', False, "1", "Sv RegexMatch"),
    ("regex.nomatch", 'SetFunctionValue (sv_RegexMatch (sv_Construct "Whatchamacallit") "x(.*)")', False, None, ""),
    ("regex.partial", 'SetFunctionValue (sv_RegexMatch (sv_Construct "Whatchamacallit") "What")', False, None, ""),
    ("regex.replace", f'SetFunctionValue (sv_RegexReplace (sv_Construct "{SUB}") "blip!|\\b(sub)([^ ]*)")', True, "This blip! has a blip! as a blip!", "Sv RegexReplace"),
    ("regex.search", f'SetFunctionValue (ar_Size (sv_RegexSearch (sv_Construct "{SUB}") "\\b(sub)([^ ]*)"))', False, "3", "Sv RegexSearch"),
    ("regex.search0", f'SetFunctionValue ((sv_RegexSearch (sv_Construct "{SUB}") "\\b(sub)([^ ]*)")[0])', True, None, ""),
    ("regex.validate", 'SetFunctionValue (ValidateRegex "[")', False, "5", "ValidateRegex (5 = Error_Brack)"),
    ("reftostring.base", _with("104C0C", "SetFunctionValue (RefToString rF)"), True, "FalloutNV.esm:104C0C", "RefToString"),
    ("reftostring.player", _with("14", "SetFunctionValue (rF.RefToString)"), True, None, ""),
    ("stringtoref.modkey", 'ref rF\nBegin Function { }\nlet rF := StringToRef "FalloutNV.esm:104C0C"\nSetFunctionValue (GetFormIDString rF)', True, "00104C0C", "StringToRef (from RefToString's text)"),
    ("lngetname.npc", _with("104C0C", "SetFunctionValue (LNGetName rF)"), True, None, ""),
    ("lngetname.cell", _with("106185", "SetFunctionValue (LNGetName rF)"), True, "Prospector Saloon", "LNGetName"),
    ("loadscreen.texture", _with("73857", "SetFunctionValue (GetLoadScreenTexture rF)"), True, "interface\\loading\\loading_billboard05.dds", "GetLoadScreenTexture"),
    ("formdesc.perk.len", _with("31DBA", "SetFunctionValue (sv_Length (GetFormDescription rF))"), False, None, ""),
    ("formdesc.npc.len", _with("104C0C", "SetFunctionValue (sv_Length (GetFormDescription rF))"), False, None, ""),
    ("objective.len", _with("10A214", "SetFunctionValue (sv_Length (GetObjectiveText rF 30))"), False, None, ""),
    ("objective.missing.len", _with("10A214", "SetFunctionValue (sv_Length (GetObjectiveText rF 9999))"), False, None, ""),
    ("bodypart.torso", _with("10CD74", "SetFunctionValue (GetBodyPartName rF 0)"), True, None, ""),
    ("bodypart.weapon", _with("10CD74", "SetFunctionValue (GetBodyPartName rF 14)"), True, None, ""),
    ("perkentry.0", _with("31DBA", "SetFunctionValue (GetNthPerkEntryString rF 0)"), True, None, ""),
    ("muzzleflash", _with("8F20F", "SetFunctionValue (GetProjectileMuzzleFlash rF)"), True, None, ""),
    ("fontfile.2", "SetFunctionValue (GetFontFile 2)", True, None, ""),
    ("fontfile.9", "SetFunctionValue (GetFontFile 9)", True, None, ""),
    ("stringsetting", 'SetFunctionValue (GetStringSetting "sTraitMenuTitleText")', True, None, ""),
]

SETS = {"jip-getters": PROBES, "jip-forms": FORM_PROBES}


def script(body):
    if body.startswith("ref "):
        return body + "\nEnd\n"
    return "Begin Function { }\n" + body + "\nEnd\n"


def job_name(set_name):
    return set_name.replace("-", "_")


def job(set_name="jip-getters"):
    steps = [{"query": script(b), "label": pid, "field": "value", **({"text": True} if text else {})}
             for pid, b, text, _, _ in SETS[set_name]]
    return {"format": cs.STEPS_FORMAT, "name": job_name(set_name),
            "title": f"JIP {set_name[4:]}, runtime-compiled (read-only)",
            "start": "any save (read-only)", "steps": steps}


def _fmt(v):
    if isinstance(v, float):
        return f"{v:.6g}"
    return "" if v is None else str(v)


def collect(out_jsonl, set_name="jip-getters"):
    rows, errors = [], []
    with open(out_jsonl, encoding="utf-8") as f:
        for line in f:
            r = json.loads(line)
            if r.get("kind") != "result":
                continue
            if "error" in r:
                errors.append({"id": r["label"], "error": r["error"]})
            else:
                rows.append({"id": r["label"], "value": _fmt(r.get("value"))})
    return {"format": ep.FORMAT, "set": set_name, "version": "JIP 57.30, xNVSE 6.4.9", "sources": [os.path.basename(out_jsonl)],
            "rows": rows, "files": [], "duplicates": [], "errors": errors}


def expected(set_name="jip-getters"):
    return {pid: exp for pid, _, _, exp, _ in SETS[set_name]}


def compare(golden):
    return ep.compare(golden, expected(golden.get("set", "jip-getters")))


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    j = sub.add_parser("job")
    j.add_argument("--set", default="jip-getters", choices=sorted(SETS))
    j.add_argument("-o", "--out")
    c = sub.add_parser("collect")
    c.add_argument("output")
    c.add_argument("--set", default="jip-getters", choices=sorted(SETS))
    c.add_argument("-o", "--out")
    k = sub.add_parser("compare")
    k.add_argument("golden")
    a = ap.parse_args(argv)
    if a.cmd == "job":
        a.out = a.out or f"{job_name(a.set)}.steps.json"
        with open(a.out, "w", encoding="utf-8", newline="\n") as f:
            json.dump(job(a.set), f, indent=1)
            f.write("\n")
        print(a.out)
        return 0
    if a.cmd == "collect":
        doc = collect(a.output, a.set)
        out = a.out or os.path.join(str(ep.default_goldens()), "jip", f"{a.set}.json")
        os.makedirs(os.path.dirname(out), exist_ok=True)
        with open(out, "w", encoding="utf-8") as f:
            json.dump(doc, f, indent=1, ensure_ascii=False)
            f.write("\n")
        print(json.dumps({"out": out, "rows": len(doc["rows"]), "errors": len(doc["errors"])}))
        return 0
    with open(a.golden, encoding="utf-8") as f:
        res = compare(json.load(f))
    print(json.dumps(res, indent=1))
    return 1 if res["mismatches"] or res["missing"] else 0


if __name__ == "__main__":
    sys.exit(main())
