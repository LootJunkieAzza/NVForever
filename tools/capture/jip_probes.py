#!/usr/bin/env python3
"""JIP LN NVSE probes without the GECK (docs/handoff/jip-tasks.md, task J3).

  jip_probes.py job [--set S] [-o JOB.steps.json]       write the capture job
  jip_probes.py collect OUT.jsonl [--set S] [-o golden]  capture output -> fnv.ext.probes/1
  jip_probes.py compare GOLDEN.json                the golden against the expectations here
  jip_probes.py steps [--dir DIR]                  every set's job into tools/capture/jip-probes/

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

def _pl(body, base=None):
    """A script with rP = the player reference (and rB = a base form) set first."""
    head = "ref rP\nref rB\nBegin Function { }\n" + _form("rP", "14")
    if base:
        head += _form("rB", base)
    return head + body


def _classes(name):
    """Bit c set when AuxiliaryVariableGetAll c (classes 0-3) holds `name` as a key."""
    return " + ".join(f'(ar_HasKey (rP.AuxVarGetAll {c}) "{name}") * {1 << c}' for c in range(4))


# Third set: JIP's variable stores (auxiliary variables and RefMap arrays). These probes
# write, but only into the stores of the running session: capture_run.py loads a save
# and quits without saving, every name used is erased again in the same script, and the
# temporary-looking names start with "*". The wiki's concept pages (Auxiliary Variables,
# RefMap Array, Template:ModIndexOverride) are not in the export, so the class each name
# prefix selects is observed here, not looked up.
STORE_PROBES = [
    ("aux.float", _pl('rP.AuxVarSetFlt "*nvfF" 3.5\nlet fV := rP.AuxVarGetFlt "*nvfF"\nrP.AuxVarErase "*nvfF"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "3.5", "AuxiliaryVariableSetFloat / GetFloat"),
    ("aux.append.size", _pl('rP.AuxVarSetFlt "*nvfL" 1\nrP.AuxVarSetFlt "*nvfL" 2 -1\nrP.AuxVarSetFlt "*nvfL" 3 -1\nlet fV := rP.AuxVarSize "*nvfL"\nrP.AuxVarErase "*nvfL"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "3", "AuxiliaryVariableSetFloat (index -1 appends)"),
    ("aux.last", _pl('rP.AuxVarSetFlt "*nvfL" 1\nrP.AuxVarSetFlt "*nvfL" 2 -1\nlet fV := rP.AuxVarGetFlt "*nvfL" -1\nrP.AuxVarErase "*nvfL"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "2", "AuxiliaryVariableGetFloat (index -1 is the last element)"),
    ("aux.gap.size", _pl('rP.AuxVarSetFlt "*nvfG" 1 5\nlet fV := rP.AuxVarSize "*nvfG"\nrP.AuxVarErase "*nvfG"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
    ("aux.missing.float", _pl('SetFunctionValue (rP.AuxVarGetFlt "*nvfNone")'), False, None, ""),
    ("aux.missing.type", _pl('SetFunctionValue (rP.AuxVarType "*nvfNone")'), False, "0", "AuxiliaryVariableGetType (0: not defined)"),
    ("aux.missing.size", _pl('SetFunctionValue (rP.AuxVarSize "*nvfNone")'), False, "0", "AuxiliaryVariableGetSize (0 if not defined)"),
    ("aux.type.float", _pl('rP.AuxVarSetFlt "*nvfT" 1\nlet fV := rP.AuxVarType "*nvfT"\nrP.AuxVarErase "*nvfT"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "1", "AuxiliaryVariableGetType (1 float)"),
    ("aux.type.ref", _pl('rP.AuxVarSetRef "*nvfT" rP\nlet fV := rP.AuxVarType "*nvfT"\nrP.AuxVarErase "*nvfT"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "2", "AuxiliaryVariableGetType (2 reference)"),
    ("aux.type.string", _pl('rP.AuxVarSetStr "*nvfT" "abc"\nlet fV := rP.AuxVarType "*nvfT"\nrP.AuxVarErase "*nvfT"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "4", "AuxiliaryVariableGetType (4 string)"),
    ("aux.float.of.string", _pl('rP.AuxVarSetStr "*nvfT" "abc"\nlet fV := rP.AuxVarGetFlt "*nvfT"\nrP.AuxVarErase "*nvfT"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
    ("aux.string", _pl('rP.AuxVarSetStr "*nvfS" "abc"\nSetFunctionValue (rP.AuxVarGetStr "*nvfS")\nrP.AuxVarErase "*nvfS"'), True, "abc", "AuxiliaryVariableSetString / GetString"),
    ("aux.overwrite.type", _pl('rP.AuxVarSetFlt "*nvfT" 1\nrP.AuxVarSetStr "*nvfT" "abc"\nlet fV := rP.AuxVarType "*nvfT"\nrP.AuxVarErase "*nvfT"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
    ("aux.erase.returns", _pl('rP.AuxVarSetFlt "*nvfE" 1\nrP.AuxVarSetFlt "*nvfE" 2 -1\nrP.AuxVarSetFlt "*nvfE" 3 -1\nlet fV := rP.AuxVarErase "*nvfE" 0\nrP.AuxVarErase "*nvfE"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "2", "AuxiliaryVariableErase (returns the new size)"),
    ("aux.erase.shifts", _pl('rP.AuxVarSetFlt "*nvfE" 1\nrP.AuxVarSetFlt "*nvfE" 2 -1\nrP.AuxVarSetFlt "*nvfE" 3 -1\nrP.AuxVarErase "*nvfE" 0\nlet fV := rP.AuxVarGetFlt "*nvfE" 0\nrP.AuxVarErase "*nvfE"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
    ("aux.erase.all", _pl('rP.AuxVarSetFlt "*nvfE" 1\nrP.AuxVarSetFlt "*nvfE" 2 -1\nlet fV := rP.AuxVarErase "*nvfE"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
    ("aux.owner.base", _pl('AuxVarSetFlt "*nvfO" 7 0 rB\nlet fV := (AuxVarGetFlt "*nvfO" 0 rB) * 10 + (rP.AuxVarGetFlt "*nvfO")\nAuxVarErase "*nvfO" -1 rB\nSetFunctionValue fV', "104C0C").replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
    ("aux.case", _pl('rP.AuxVarSetFlt "*nvfC" 4\nlet fV := rP.AuxVarGetFlt "*NVFc"\nrP.AuxVarErase "*nvfC"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
]
for _name in ("nvfP", "*nvfP", "_nvfP", "*_nvfP"):
    _tag = _name.replace("*", "t").replace("_", "u")
    STORE_PROBES.append((f"aux.class.{_tag}", _pl(f'rP.AuxVarSetFlt "{_name}" 1\nlet fV := {_classes(_name)}\nrP.AuxVarErase "{_name}"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""))
STORE_PROBES += [
    ("refmap.float", _pl('rP.RefMapSetFlt "*nvfM" 4.5\nlet fV := rP.RefMapGetFlt "*nvfM"\nRefMapDestroy "*nvfM"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "4.5", "RefMapArraySetFloat / GetFloat"),
    ("refmap.size", _pl('rP.RefMapSetFlt "*nvfM" 1\nRefMapSetFlt "*nvfM" 2 rB\nrP.RefMapSetFlt "*nvfM" 3\nlet fV := RefMapArrayGetSize "*nvfM"\nRefMapDestroy "*nvfM"\nSetFunctionValue fV', "104C0C").replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
    ("refmap.type", _pl('rP.RefMapSetStr "*nvfM" "abc"\nlet fV := rP.RefMapGetType "*nvfM"\nRefMapDestroy "*nvfM"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "4", "RefMapArrayGetType (4 string)"),
    ("refmap.missing.type", _pl('SetFunctionValue (rP.RefMapGetType "*nvfNone")'), False, "0", "RefMapArrayGetType (0: no such key)"),
    ("refmap.missing.float", _pl('SetFunctionValue (rP.RefMapGetFlt "*nvfNone")'), False, "0", "RefMapArrayGetFloat (0 if not defined)"),
    ("refmap.first.key", _pl('rP.RefMapSetFlt "*nvfM" 1\nlet rB := (RefMapFirst "*nvfM")[0]\nRefMapDestroy "*nvfM"\nSetFunctionValue (GetFormIDString rB)'), True, "00000014", "RefMapArrayGetFirst (element 0 is the key)"),
    ("refmap.keys", _pl('rP.RefMapSetFlt "*nvfM" 1\nRefMapSetFlt "*nvfM" 2 rB\nlet fV := ar_Size (RefMapKeys "*nvfM")\nRefMapDestroy "*nvfM"\nSetFunctionValue fV', "104C0C").replace("ref rB\n", "ref rB\nfloat fV\n"), False, "2", "RefMapArrayGetKeys"),
    ("refmap.next.end", _pl('rP.RefMapSetFlt "*nvfM" 1\nlet fV := ar_Size (RefMapFirst "*nvfM")\nlet fV := fV * 10 + ar_Size (RefMapNext "*nvfM")\nRefMapDestroy "*nvfM"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
    ("refmap.destroyed", _pl('rP.RefMapSetFlt "*nvfM" 1\nRefMapDestroy "*nvfM"\nSetFunctionValue (RefMapArrayGetSize "*nvfM")'), False, None, ""),
    ("refmap.validate", _pl('rP.RefMapSetFlt "*nvfM" 1\nlet fV := RefMapValidate "*nvfM"\nRefMapDestroy "*nvfM"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "1", "RefMapArrayValidate (returns the size)"),
]

def _on_damage(params, body):
    """Register a health-damage handler on the player: a lambda with `params`."""
    return _pl(f"SetOnHealthDamageEventHandler (Begin Function {{{params}}}\n{body}\nEnd) 1 rP\nSetFunctionValue 1")


def _read(expr, text=False):
    return _pl(f"SetFunctionValue ({expr})")


# Fourth set: JIP event handlers, registered with xNVSE lambdas (a runtime-compiled
# script cannot declare a UDF form). Lambda parameters must be typed inside the braces
# (`{float fD, ref rS}`); parameters declared before Begin do not compile [observed:
# jip_ev_trial2, 2026-10-26]; `this` does not compile in a runtime-compiled lambda, GetSelf
# does [observed: jip_ev_trial3]. The steps depend on each other and run in order: register
# every handler, cause one event (DamageAV Health 1 on the player), read what the
# handlers stored in `*` auxiliary variables, undo. Writes stay in the session: the run
# never saves, the handlers die with the session, the last step restores the Health
# point and erases the variables.
EVENT_PROBES = [
    ("ev.reg.amount", _on_damage("float fD, ref rS", '  rP.AuxVarSetFlt "*nvfHD" fD\n  rP.AuxVarSetRef "*nvfHS" rS\n  rP.AuxVarSetFlt "*nvfHT" (GetSelf == rP)'), False, "1", "SetOnHealthDamageEventHandler (registers)"),
    ("ev.reg.order1", _on_damage("float fD, ref rS", '  rP.AuxVarSetFlt "*nvfHO" 1 -1'), False, "1", "SetOnHealthDamageEventHandler"),
    ("ev.reg.order2", _on_damage("float fD, ref rS", '  rP.AuxVarSetFlt "*nvfHO" 2 -1'), False, "1", "SetOnHealthDamageEventHandler"),
    ("ev.reg.noparams", _on_damage(" ", '  rP.AuxVarSetFlt "*nvfHN" 1'), False, "1", "SetOnHealthDamageEventHandler"),
    ("ev.reg.oneparam", _on_damage("float fD", '  rP.AuxVarSetFlt "*nvfH1" fD'), False, "1", "SetOnHealthDamageEventHandler"),
    ("loop.reg", _pl('SetGameMainLoopCallback (Begin Function { }\n  rP.AuxVarSetFlt "*nvfLC" ((rP.AuxVarGetFlt "*nvfLC") + 1)\n  rP.AuxVarSetFlt "*nvfLT" (GetSelf == rP)\nEnd) 1 1 1\nSetFunctionValue 1'), False, "1", "SetGameMainLoopCallback"),
    ("ev.damage", _pl("rP.DamageAV Health 1\nSetFunctionValue 1"), False, "1", "DamageAV"),
    ("ev.amount", _read('rP.AuxVarGetFlt "*nvfHD"'), False, "-1", "SetOnHealthDamageEventHandler (the damage is negative)"),
    ("ev.source.type", _read('rP.AuxVarType "*nvfHS"'), False, None, ""),
    ("ev.this", _read('rP.AuxVarGetFlt "*nvfHT"'), False, "1", "SetOnHealthDamageEventHandler (the actor is the calling reference)"),
    ("ev.order.size", _read('rP.AuxVarSize "*nvfHO"'), False, None, ""),
    ("ev.order.first", _read('rP.AuxVarGetFlt "*nvfHO" 0'), False, None, ""),
    ("ev.noparams.ran", _read('rP.AuxVarGetFlt "*nvfHN"'), False, None, ""),
    ("ev.oneparam.ran", _read('rP.AuxVarGetFlt "*nvfH1"'), False, None, ""),
    ("loop.ran", _read('(rP.AuxVarGetFlt "*nvfLC") > 0'), False, "1", "SetGameMainLoopCallback (runs every frame)"),
    ("loop.this", _read('rP.AuxVarGetFlt "*nvfLT"'), False, "1", "SetGameMainLoopCallback (on the player when not called on a reference)"),
    ("ev.cleanup", _pl('rP.RestoreAV Health 1\n' + "".join(f'rP.AuxVarErase "*nvf{n}"\n' for n in ("HD", "HS", "HT", "HO", "HN", "H1", "LC", "LT")) + "SetFunctionValue 1"), False, "1", "cleanup"),
]

# Fifth set: JIP's hit information, read inside SetOnHitEventHandler lambdas (OnHit
# passes no arguments, so the lambdas take none). One handler per value appends it to a
# `*` auxiliary variable on every firing; then a frag mine explosion (MineFragExplosion,
# FalloutNV.esm 07078B [observed: fnvtool esm inventory]) is placed at the player, whose
# Health is first raised by 1000 so the blast cannot kill. Reads give each firing's
# value (index 0 = first firing). The last step takes the 1000 back, heals and erases.
HIT_VALUES = [
    ("count", "1"),
    ("damage", "rP.GetHitHealthDamage"),
    ("location", "rP.GetHitLocation"),
    ("flag.explosion", "rP.GetHitExtendedFlag 8"),
    ("flag.critical", "rP.GetHitExtendedFlag 1"),
    ("projectile.isref", "IsReference (rP.GetHitProjectile)"),
    ("projectile.type", "GetType (rP.GetHitProjectile)"),
    ("attacker.isref", "IsReference (rP.GetHitAttacker)"),
    ("weapon.valid", "IsFormValid (rP.GetHitWeapon)"),
    ("pos.x", "(rP.GetHit3DData 0)[0]"),
    ("health", "rP.GetAV Health"),
]
# Expected values the wiki states for an explosion hit [documented: the page named].
HIT_EXPECTED = {
    "hit.location.0": ("-1", "GetHitLocation (always -1 for explosion hits)"),
    "hit.location.1": ("-1", "GetHitLocation (always -1 for explosion hits)"),
    "hit.flag.explosion.1": ("1", "GetHitExtendedFlag (8: explosion hit)"),
    "hit.pos.x.0": ("0", "GetHit3DData (all 0 for explosion hits)"),
    "hit.health.0": ("1200", "SetOnHitEventHandler (runs before the damage)"),
}


def _hit_steps():
    steps = [("hit.boost", _pl("rP.ModAV Health 1000\nSetFunctionValue (rP.GetAV Health)"), False, None, "")]
    for name, expr in HIT_VALUES:
        steps.append((f"hit.reg.{name}", _pl(f'SetOnHitEventHandler (Begin Function {{ }}\n  rP.AuxVarSetFlt "*nvfH.{name}" ({expr}) -1\nEnd) 1 rP\nSetFunctionValue 1'), False, "1", "SetOnHitEventHandler (registers)"))
    steps.append(("hit.boom", _pl("rP.PlaceAtMe rB 1\nSetFunctionValue 1", "7078B"), False, "1", "PlaceAtMe"))
    steps.append(("hit.settle", "SetFunctionValue 1", False, "1", "a step for the blast to land"))
    for name, _ in HIT_VALUES:
        steps.append((f"hit.{name}.size", _read(f'rP.AuxVarSize "*nvfH.{name}"'), False, None, ""))
        for i in range(2):
            exp, page = HIT_EXPECTED.get(f"hit.{name}.{i}", (None, ""))
            steps.append((f"hit.{name}.{i}", _read(f'rP.AuxVarGetFlt "*nvfH.{name}" {i}'), False, exp, page))
    steps.append(("hit.health.after", _read("rP.GetAV Health"), False, None, ""))
    steps.append(("hit.cleanup", _pl("rP.ModAV Health -1000\nrP.RestoreAV Health 2000\n" + "".join(f'rP.AuxVarErase "*nvfH.{n}"\n' for n, _ in HIT_VALUES) + "SetFunctionValue 1"), False, "1", "cleanup"))
    return steps


HIT_PROBES = _hit_steps()

# Sixth set: JIP getters of record fields (weapons, ammo, projectiles, explosions,
# armor), on the forms the wiki pages use as examples; IDs from fnvtool esm inventory
# [observed]. NVForever reads the same fields from the records, so these pin which field
# each trait number means. Read-only.
REC = {
    "slug.proj": "1572C5", "ammo12": "8ECF5", "slug.ammo": "13E447", "gren.expl": "7EA31",
    "gren.proj": "7EA30", "laser": "4336", "hunting": "4333", "sledge": "4352", "rev44": "8F215",
    "pistol10": "434F", "carbine": "8F21E", "gatling": "432E", "metal": "3307F", "leather": "20423",
    "ammo10": "4241", "proj10": "2CD5F", "t45d": "4443E",
}
REC_EXPECTED = {
    "killimpulse.laser": ("0", "GetWeaponKillImpulse (WeapLaserRifle: 0)"),
    "killimpulse.hunting": ("26", "GetWeaponKillImpulse (WeapHuntingRifle: 26)"),
    "impulsedist.laser": ("0", "GetWeaponImpulseDistance (WeapLaserRifle: 0)"),
    "vatseffect.sledge": ("0010F09F", "GetWeaponVATSEffect (WeapSuperSledge: MaulerKnockdownSpell)"),
}


def _rec(pid, key, expr, text=False):
    exp, page = REC_EXPECTED.get(pid, (None, ""))
    return (pid, _with(REC[key], f"SetFunctionValue ({expr})"), text, exp, page)


RECORD_PROBES = (
    [_rec(f"projtrait.slug.{i}", "slug.proj", f"GetProjectileTraitNumeric rF {i}") for i in range(15)]
    + [_rec(f"projtrait.gren.{i}", "gren.proj", f"GetProjectileTraitNumeric rF {i}") for i in (0, 1, 2, 3, 5, 6)]
    + [_rec(f"projflag.slug.{i}", "slug.proj", f"GetProjectileFlag rF {i}") for i in range(12)]
    + [_rec(f"projflag.gren.{i}", "gren.proj", f"GetProjectileFlag rF {i}") for i in (0, 1, 10)]
    + [_rec("projexpl.gren", "gren.proj", "GetFormIDString (GetProjectileExplosion rF)", True),
       _rec("projexpl.slug", "slug.proj", "GetFormIDString (GetProjectileExplosion rF)", True),
       _rec("projlight.slug", "slug.proj", "GetFormIDString (GetProjectileLight rF)", True)]
    + [_rec(f"expltrait.gren.{i}", "gren.expl", f"GetExplosionTraitNumeric rF {i}") for i in range(8)]
    + [_rec(f"explflag.gren.{i}", "gren.expl", f"GetExplosionFlag rF {i}") for i in range(1, 7)]
    + [_rec(f"expltraitform.gren.{i}", "gren.expl", f"GetFormIDString (GetExplosionTraitForm rF {i})", True) for i in range(7)]
    + [_rec(f"ammotrait.ammo12.{i}", "ammo12", f"GetAmmoTraitNumeric rF {i}") for i in range(4)]
    + [_rec(f"ammotrait.ammo10.{i}", "ammo10", f"GetAmmoTraitNumeric rF {i}") for i in range(4)]
    + [_rec("ammoproj.slug", "slug.ammo", "GetFormIDString (GetAmmoProjectile rF)", True),
       _rec("ammoproj.ammo10", "ammo10", "GetFormIDString (GetAmmoProjectile rF)", True)]
    + [_rec("killimpulse.laser", "laser", "GetWeaponKillImpulse rF"),
       _rec("killimpulse.hunting", "hunting", "GetWeaponKillImpulse rF"),
       _rec("impulsedist.laser", "laser", "GetWeaponImpulseDistance rF"),
       _rec("impulsedist.hunting", "hunting", "GetWeaponImpulseDistance rF"),
       _rec("critflags.laser", "laser", "GetWeaponCritFlags rF"),
       _rec("critflags.hunting", "hunting", "GetWeaponCritFlags rF"),
       _rec("vatseffect.sledge", "sledge", "GetFormIDString (GetWeaponVATSEffect rF)", True),
       _rec("vatseffect.laser", "laser", "GetFormIDString (GetWeaponVATSEffect rF)", True),
       _rec("onhit.rev44", "rev44", "GetWeaponOnHitSetting rF"),
       _rec("semiauto.rev44.0", "rev44", "GetWeaponSemiAutoFireDelay rF 0"),
       _rec("semiauto.rev44.1", "rev44", "GetWeaponSemiAutoFireDelay rF 1"),
       _rec("impactset.pistol10", "pistol10", "GetFormIDString (GetImpactDataSet rF)", True),
       _rec("model.carbine.0", "carbine", "GetWeaponModel rF 0", True),
       _rec("model.gatling.3", "gatling", "GetWeaponModel rF 3", True),
       _rec("model.pistol10.7", "pistol10", "GetWeaponModel rF 7", True),
       _rec("armorclass.metal", "metal", "GetArmorClass rF"),
       _rec("armorclass.leather", "leather", "GetArmorClass rF"),
       _rec("armorrct.metal", "metal", "GetArmorRCT rF", True),
       _rec("armorrct.t45d", "t45d", "GetArmorRCT rF", True)]
)

# Seventh set: core NVSE/FOSE form and reference getters (not JIP: asked for by the
# extender core work to check its VM), on vanilla FalloutNV.esm forms; IDs from fnvtool
# esm inventory [observed]. Read-only. Reference rows read the loaded save's state (Doc
# Mitchell's cell), so they can differ in a run that starts from no save.
NF = {
    "pistol10": "434F", "hunting": "4333", "ammo10": "4241", "leather": "20423", "caps": "F",
    "stimpak": "15169", "doc": "104C0C", "docref": "104C0F", "player": "14", "playerbase": "7",
    "saloon": "106185", "vcg02": "10A214", "actionboy": "31DBA", "slugproj": "1572C5",
    "list127": "1613D5",
}


def _nf(pid, key, expr, text=False):
    return (pid, _with(NF[key], f"SetFunctionValue ({expr})"), text, None, "")


def _nfid(pid, key, expr):
    return _nf(pid, key, f"GetFormIDString ({expr})", True)


_WEAPON_GETTERS = ["GetWeaponClipRounds", "GetAttackDamage", "GetWeaponType", "GetWeaponSkill",
                   "GetWeaponMinRange", "GetWeaponMaxRange", "GetWeaponFireRate", "GetWeaponSpread",
                   "GetWeaponMinSpread", "GetWeaponAnimJamTime", "GetWeaponHandGrip", "GetWeaponNumProjectiles",
                   "GetWeaponCritChance", "GetWeaponCritDamage", "GetWeaponActionPoints", "GetWeaponReach",
                   "GetWeaponSightFOV", "GetWeaponRequiredStrength", "GetWeaponRequiredSkill",
                   "GetWeaponResistType", "GetWeaponAttackAnimation", "GetWeaponAimArc", "GetWeaponLongBursts",
                   "GetWeaponFlags1", "GetWeaponFlags2", "GetWeaponBaseVATSChance"]

NVSE_FORM_PROBES = (
    [_nf(f"type.{k}", k, "GetType rF") for k in ("pistol10", "ammo10", "leather", "caps", "stimpak", "doc",
                                                 "docref", "player", "saloon", "vcg02", "actionboy", "slugproj", "list127")]
    + [_nfid("base.docref", "docref", "rF.GetBaseObject"),
       _nfid("base.player", "player", "rF.GetBaseObject"),
       _nf("isref.docref", "docref", "IsReference rF"),
       _nf("isref.doc", "doc", "IsReference rF"),
       _nf("valid.pistol10", "pistol10", "IsFormValid rF"),
       _nfid("cell.docref", "docref", "rF.GetParentCell"),
       _nf("formid.pistol10", "pistol10", "GetFormIDString rF", True),
       _nf("name.pistol10", "pistol10", "GetName rF", True),
       _nf("name.caps", "caps", "GetName rF", True),
       _nf("name.saloon", "saloon", "GetName rF", True),
       _nf("name.docref", "docref", "GetName rF", True),
       _nf("name.vcg02", "vcg02", "GetName rF", True)]
    + [_nf(f"weight.{k}", k, "GetWeight rF") for k in ("pistol10", "leather", "caps", "stimpak", "ammo10")]
    + [_nf(f"value.{k}", k, "GetItemValue rF") for k in ("pistol10", "leather", "caps", "stimpak", "ammo10")]
    + [_nf(f"health.{k}", k, "GetBaseHealth rF") for k in ("pistol10", "leather")]
    + [_nf(f"equiptype.{k}", k, "GetEquipType rF") for k in ("pistol10", "leather", "stimpak", "ammo10")]
    + [_nfid("repairlist.pistol10", "pistol10", "GetRepairList rF"),
       _nf("model.pistol10", "pistol10", "GetModelPath rF", True),
       _nf("icon.pistol10", "pistol10", "GetIconPath rF", True),
       _nf("ar.leather", "leather", "GetArmorArmorRating rF"),
       _nf("dt.leather", "leather", "GetArmorDT rF"),
       _nfid("ammo.pistol10", "pistol10", "GetWeaponAmmo rF"),
       _nfid("projectile.pistol10", "pistol10", "GetWeaponProjectile rF"),
       _nfid("criteffect.pistol10", "pistol10", "GetWeaponCritEffect rF")]
    + [_nf(f"weap.{g[9:].lower() if g.startswith('GetWeapon') else g[3:].lower()}.{k}", k, f"{g} rF")
       for g in _WEAPON_GETTERS for k in ("pistol10", "hunting")]
    + [_nf("list.count", "list127", "ListGetCount rF"),
       _nfid("list.nth0", "list127", "ListGetNthForm rF 0"),
       _nf("list.index", "list127", "ListGetFormIndex rF (ListGetNthForm rF 0)"),
       _nf("modindex.fnv", "pistol10", 'GetModIndex "FalloutNV.esm"'),
       _nf("sourcemod.pistol10", "pistol10", "GetSourceModIndex rF"),
       _nf("nthmod.0", "pistol10", "GetNthModName 0", True),
       _nf("scripted.pistol10", "pistol10", "IsScripted rF"),
       _nfid("script.doc", "doc", "GetScript rF"),
       _nfid("race.doc", "doc", "GetRace rF")]
)

# Eighth set: NVSE's variables by name (HasVariable, GetVariable, SetVariable,
# GetRefVariable, SetRefVariable). The quest is ExtProbeLoops06Q of the owner's test
# plugin NVFExtProbe.esp (loaded in the game and in the lab; its script declares
# `int iR`, `ref rForm`, `ref rList`, `int iDone`, tools/capture/ext-probes/loops/06),
# and the vanilla VCG02 (VCG02SCRIPT: nTargetCount, nGeckosKilled, bCheyenneAlive,
# bShootingTutorialActive) and Doc Mitchell (GSDocMitchellScript: DoOnce,
# BeginMedicalServicesBAD, Timer, PlayerLeft) [observed: fnvtool esm decode]. Sets write
# quest variables of the session only (the run never saves); each probe puts back what
# it changed where the old value can be read.
def _q(body):
    return ("ref rQ\nref rV\nref rD\nref rP\nfloat fO\nfloat fV\nBegin Function { }\n"
            'let rQ := GetFormFromMod "NVFExtProbe.esp" "B40"\n'
            + _form("rV", "10A214") + _form("rD", "104C0F") + _form("rP", "14") + body)


VAR_PROBES = [
    ("has.int", _q('SetFunctionValue (HasVariable "iDone" rQ)'), False, "1", "HasVariable"),
    ("has.ref", _q('SetFunctionValue (HasVariable "rList" rQ)'), False, "1", "HasVariable (any variable type)"),
    ("has.case", _q('SetFunctionValue (HasVariable "IDONE" rQ)'), False, None, ""),
    ("has.none", _q('SetFunctionValue (HasVariable "nvfNone" rQ)'), False, "0", "HasVariable"),
    ("has.vanilla", _q('SetFunctionValue (HasVariable "nGeckosKilled" rV)'), False, "1", "HasVariable"),
    ("has.ref.calling", _q('SetFunctionValue (rD.HasVariable "PlayerLeft")'), False, "1", "HasVariable (calling reference)"),
    ("has.base", _q('SetFunctionValue (HasVariable "PlayerLeft" (rD.GetBaseObject))'), False, None, ""),
    ("has.noscript", _q('SetFunctionValue (HasVariable "iDone" rP)'), False, None, ""),
    ("set.get", _q('let fO := GetVariable "iR" rQ\nSetVariable "iR" 7 rQ\nlet fV := GetVariable "iR" rQ\nSetVariable "iR" fO rQ\nSetFunctionValue fV'), False, "7", "SetVariable / GetVariable"),
    ("set.fraction", _q('let fO := GetVariable "iR" rQ\nSetVariable "iR" 2.75 rQ\nlet fV := GetVariable "iR" rQ\nSetVariable "iR" fO rQ\nSetFunctionValue fV'), False, None, ""),
    ("set.vanilla", _q('let fO := GetVariable "bShootingTutorialActive" rV\nSetVariable "bShootingTutorialActive" 5 rV\nlet fV := GetVariable "bShootingTutorialActive" rV\nSetVariable "bShootingTutorialActive" fO rV\nSetFunctionValue fV'), False, "5", "SetVariable / GetVariable"),
    ("set.float.calling", _q('let fO := rD.GetVariable "Timer"\nrD.SetVariable "Timer" 1.5\nlet fV := rD.GetVariable "Timer"\nrD.SetVariable "Timer" fO\nSetFunctionValue fV'), False, "1.5", "SetVariable / GetVariable (calling reference)"),
    ("get.missing", _q('SetFunctionValue (GetVariable "nvfNone" rQ)'), False, None, ""),
    ("get.refvar.as.number", _q('SetRefVariable "rForm" rP rQ\nSetFunctionValue (GetVariable "rForm" rQ)'), False, None, ""),
    ("ref.set.get", _q('SetRefVariable "rForm" rP rQ\nSetFunctionValue (GetFormIDString (GetRefVariable "rForm" rQ))'), True, "00000014", "SetRefVariable / GetRefVariable"),
    ("ref.get.intvar", _q('SetFunctionValue (GetFormIDString (GetRefVariable "iDone" rQ))'), True, None, ""),
    ("ref.get.missing", _q('SetFunctionValue (GetFormIDString (GetRefVariable "nvfNone" rQ))'), True, None, ""),
    ("ref.set.form", _q('SetRefVariable "rForm" rV rQ\nSetFunctionValue (GetFormIDString (GetRefVariable "rForm" rQ))'), True, "0010A214", "SetRefVariable (any form)"),
]

# Ninth set: reference walking in named cells (GetNumRefsInCell, GetFirstRefInCell),
# not around the player, so it does not depend on where the save stands. Doc Mitchell's
# house and the Prospector Saloon are not loaded in the owner's save (it stands at the
# airport), where only persistent references are seen [documented: GetFirstRefInCell].
# Form type codes 0 any, 200 actors, 201 items, 42 NPC_ [documented: GetRefsEtc pages;
# Form Type IDs]. Read-only.
CELLS = {"doc": "103DF9", "saloon": "106185"}
REF_PROBES = (
    [(f"num.{c}.{t}", _with(fid, f"SetFunctionValue (GetNumRefsInCell rF {t} 0 0)"), False, None, "")
     for c, fid in CELLS.items() for t in (0, 200, 201, 42)]
    + [(f"num.{c}.taken", _with(fid, "SetFunctionValue (GetNumRefsInCell rF 0 0 1)"), False, None, "")
       for c, fid in CELLS.items()]
    + [(f"first.{c}.200", _with(fid, "SetFunctionValue (GetFormIDString (GetFirstRefInCell rF 200 0 0))"), True, None, "")
       for c, fid in CELLS.items()]
)

# Tenth set: control bindings (GetControl code type, GetAltControl code) for the 28
# control codes, keyboard (type 0) and mouse (type 1), plus one code past the range.
# They read the owner's key bindings. Read-only.
CONTROL_PROBES = (
    [(f"control.{c}.{t}", f"SetFunctionValue (GetControl {c} {t})", False, None, "") for c in range(29) for t in (0, 1)]
    + [(f"control.{c}.default", f"SetFunctionValue (GetControl {c})", False, None, "") for c in (0, 4, 27)]
    + [(f"alt.{c}", f"SetFunctionValue (GetAltControl {c})", False, None, "") for c in range(29)]
)

# Eleventh set: doors, factions and NPC appearance on vanilla forms (Doc Mitchell's
# house door, Doc, Sunny Smiles, Trudy; NCR and Goodsprings factions); IDs from fnvtool
# esm inventory [observed]. Read-only.
AF = {"door": "103E61", "doc": "104C0C", "sunny": "104E84", "trudy": "104C6C",
      "ncr": "A46E7", "goodsprings": "104C6E", "bos": "1D3FE"}


def _af(pid, key, expr, text=False):
    return (pid, _with(AF[key], f"SetFunctionValue ({expr})"), text, None, "")


ACTOR_PROBES = (
    [_af("door.linked", "door", "GetFormIDString (rF.GetLinkedDoor)", True),
     _af("door.teleportcell", "door", "GetFormIDString (rF.GetTeleportCell)", True),
     _af("door.isload", "door", "rF.IsLoadDoor")]
    + [x for k in ("doc", "sunny", "trudy") for x in (
        [_af(f"factions.{k}", k, "GetBaseNumFactions rF")]
        + [_af(f"faction.{k}.{i}", k, f"GetFormIDString (GetBaseNthFaction {i} rF)", True) for i in range(3)]
        + [_af(f"rank.{k}.{i}", k, f"GetBaseNthRank {i} rF") for i in range(3)]
        + [_af(f"hair.{k}", k, "GetFormIDString (GetHair rF)", True),
           _af(f"eyes.{k}", k, "GetFormIDString (GetEyes rF)", True),
           _af(f"hairlength.{k}", k, "GetHairLength rF"),
           _af(f"haircolor.{k}.0", k, "GetHairColor 0 rF"),
           _af(f"haircolor.{k}.1", k, "GetHairColor 1 rF"),
           _af(f"haircolor.{k}.2", k, "GetHairColor 2 rF"),
           _af(f"haircolor.{k}.3", k, "GetHairColor 3 rF"),
           _af(f"height.{k}", k, "GetNPCHeight rF"),
           _af(f"weight.{k}", k, "GetNPCWeight rF"),
           _af(f"flagslow.{k}", k, "GetActorBaseFlagsLow rF"),
           _af(f"flagshigh.{k}", k, "GetActorBaseFlagsHigh rF")])]
    + [_af(f"ranks.{k}", k, "GetNumRanks rF") for k in ("ncr", "goodsprings", "bos")]
    + [_af(f"ranknames.{k}.size", k, "ar_Size (GetFactionRankNames rF)") for k in ("ncr", "goodsprings", "bos")]
    + [_af("ranknames.bos.0", "bos", "(GetFactionRankNames rF)[0]", True)]
)

# Twelfth set: base-form setters read back in the same session (SetWeaponClipRounds,
# SetWeight, SetName, SetBaseItemValue, SetObjectHealth): whether the change shows through
# the getter, and through a second form of access (a reference's base form, LNGetName).
# Each probe puts the old value back. Session only: the run never saves.
def _set(pid, key, body, text=False, exp=None, page=""):
    return (pid, ("ref rF\nref rD\nfloat fO\nfloat fV\nBegin Function { }\n" + _form("rF", NF[key])
                  + _form("rD", NF["docref"]) + body), text, exp, page)


SETTER_PROBES = [
    _set("clip.set.get", "pistol10", "let fO := GetWeaponClipRounds rF\nSetWeaponClipRounds 30 rF\nlet fV := GetWeaponClipRounds rF\nSetWeaponClipRounds fO rF\nSetFunctionValue fV", exp="30", page="SetWeaponClipRounds"),
    _set("clip.restored", "pistol10", "SetFunctionValue (GetWeaponClipRounds rF)"),
    _set("weight.set.get", "caps", "let fO := GetWeight rF\nSetWeight 0.5 rF\nlet fV := GetWeight rF\nSetWeight fO rF\nSetFunctionValue fV", exp="0.5", page="SetWeight"),
    _set("value.set.get", "pistol10", "let fO := GetItemValue rF\nSetBaseItemValue rF 999\nlet fV := GetItemValue rF\nSetBaseItemValue rF fO\nSetFunctionValue fV"),
    _set("health.set.get", "pistol10", "let fO := GetBaseHealth rF\nSetObjectHealth 123 rF\nlet fV := GetBaseHealth rF\nSetObjectHealth fO rF\nSetFunctionValue fV"),
    _set("name.set.get", "pistol10", 'SetName "NVF Pistol" rF\nSetFunctionValue (GetName rF)\nSetName "10mm Pistol" rF', text=True, exp="NVF Pistol", page="SetName"),
    _set("name.set.lngetname", "pistol10", 'SetName "NVF Pistol" rF\nSetFunctionValue (LNGetName rF)\nSetName "10mm Pistol" rF', text=True),
    _set("name.restored", "pistol10", "SetFunctionValue (GetName rF)", text=True),
    _set("name.npc.via.ref", "doc", 'SetName "NVF Doc" rF\nSetFunctionValue (GetName rD)\nSetName "Doc Mitchell" rF', text=True),
    _set("name.npc.ref.lngetname", "doc", 'SetName "NVF Doc" rF\nSetFunctionValue (rD.LNGetName)\nSetName "Doc Mitchell" rF', text=True),
]

# Thirteenth set: packages, quest objectives, ownership, model and texture paths and the
# raw form flags, on vanilla forms. Read-only. Ids ending in `.save` read state of the
# loaded save (Doc Mitchell's current package, equipment, the player's perks), so a run
# without the owner's save may differ there.
def _m(pid, body, text=False):
    return (pid, "ref rD\nref rF\nref rP\nref rG\nBegin Function { }\n" + _form("rD", NF["docref"])
            + _form("rP", NF["player"]) + body, text, None, "")


def _mf(pid, key, expr, text=False):
    return _m(pid, _form("rF", NF[key]) + f"SetFunctionValue ({expr})", text)


MISC_PROBES = (
    [_m("pkg.count", "SetFunctionValue (rD.GetPackageCount)"),
     _m("pkg.nth0", "SetFunctionValue (GetFormIDString (rD.GetNthPackage 0))", True),
     _m("pkg.nth1", "SetFunctionValue (GetFormIDString (rD.GetNthPackage 1))", True),
     _m("pkg.nth0.location", "let rG := rD.GetNthPackage 0\nSetFunctionValue (GetFormIDString (GetPackageLocation rG))", True),
     _m("pkg.nth0.distance", "let rG := rD.GetNthPackage 0\nSetFunctionValue (GetPackageTargetDistance rG)"),
     _m("pkg.current.save", "SetFunctionValue (GetFormIDString (rD.GetCurrentPackage))", True),
     _mf("obj.count", "vcg02", "GetQuestObjectiveCount rF")]
    + [_mf(f"obj.nth{i}", "vcg02", f"GetNthQuestObjective rF {i}") for i in range(4)]
    + [_m("owner.cell.docref", "SetFunctionValue (GetFormIDString (rD.GetParentCellOwner))", True),
       _m("owner.rank.docref", "SetFunctionValue (rD.GetOwningFactionRequiredRank)"),
       _m("equip.health.doc.save", "SetFunctionValue (rD.GetEquippedCurrentHealth 5)"),
       _m("equip.modflags.doc.save", "SetFunctionValue (rD.GetEquippedWeaponModFlags)"),
       _mf("perk.player.save", "actionboy", "rP.GetPerkRank rF"),
       _mf("altperk.player.save", "actionboy", "rP.GetAltPerkRank rF")]
    + [_mf(f"biped.leather.{c}", "leather", f"GetBipedModelPath {c} rF", True) for c in range(4)]
    + [_mf(f"texture.{k}", k, "GetTexturePath rF", True) for k in ("pistol10", "stimpak", "actionboy", "caps")]
    + [_mf(f"flags.{k}.{w}", k, f"GetFlags{w.capitalize()} rF") for k in ("pistol10", "caps", "doc", "leather", "vcg02")
       for w in ("low", "high")]
)

# Fourteenth set: quaternion and vector functions, the legacy ones (results written to
# variables named by strings) and their 6.4.8 "Ex" forms (variables passed directly).
# Each probe returns the result components as one text, "%.4f" each, in the order the
# function writes them (W X Y Z, or X Y Z). Read-only.
_V4 = "float fQw\nfloat fQx\nfloat fQy\nfloat fQz\nBegin Function { }\n"
_OUT4 = 'SetFunctionValue (sv_Construct "%.4f %.4f %.4f %.4f" fQw fQx fQy fQz)'
_OUT3 = 'SetFunctionValue (sv_Construct "%.4f %.4f %.4f" fQx fQy fQz)'


def _q4(pid, call):
    return (pid, _V4 + call + "\n" + _OUT4, True, None, "")


def _q3(pid, call):
    return (pid, _V4 + call + "\n" + _OUT3, True, None, "")


_N4 = '"fQw" "fQx" "fQy" "fQz"'
_N3 = '"fQx" "fQy" "fQz"'
MATH_PROBES = [
    _q4("qfromeuler.x90", f"QFromEuler {_N4} 90 0 0"),
    _q4("qfromeuler.y90", f"QFromEuler {_N4} 0 90 0"),
    _q4("qfromeuler.z90", f"QFromEuler {_N4} 0 0 90"),
    _q4("qfromeuler.xyz", f"QFromEuler {_N4} 30 45 60"),
    _q4("qfromeuler.xyz.pitchyaw", f"QFromEuler {_N4} 30 45 60 1"),
    _q4("qfromeulerex.xyz", "QFromEulerEx fQw fQx fQy fQz 30 45 60"),
    _q3("qtoeuler.z90", f"QToEuler {_N3} 0.7071 0 0 0.7071"),
    _q3("qtoeuler.xyz", f"QToEuler {_N3} 0.8224 0.0222 0.4396 0.3604"),
    _q3("qtoeulerex.z90", "QToEulerEx fQx fQy fQz 0.7071 0 0 0.7071"),
    _q4("qmult.qq", f"QMultQuatQuat {_N4} 0.7071 0.7071 0 0 0.7071 0 0 0.7071"),
    _q4("qmultex.qq", "QMultQuatQuatEx fQw fQx fQy fQz 0.7071 0.7071 0 0 0.7071 0 0 0.7071"),
    _q3("qmult.qv", f"QMultQuatVector3 {_N3} 0.7071 0 0 0.7071 1 0 0"),
    _q3("qmultex.qv", "QMultQuatVector3Ex fQx fQy fQz 0.7071 0 0 0.7071 1 0 0"),
    _q4("qnorm", f"QNormalize {_N4} 2 0 0 2"),
    _q4("qnormex", "QNormalizeEx fQw fQx fQy fQz 2 0 0 2"),
    _q4("qinterp.half", f"QInterpolate {_N4} 1 0 0 0 0.7071 0 0 0.7071 0.5"),
    _q4("qinterp.half.sli", f"QInterpolate {_N4} 1 0 0 0 0.7071 0 0 0.7071 0.5 1"),
    _q4("qinterpex.half", "QInterpolateEx fQw fQx fQy fQz 1 0 0 0 0.7071 0 0 0.7071 0.5"),
    _q4("qfromaa.z90", f"QFromAxisAngle {_N4} 0 0 1 90"),
    _q4("qfromaaex.z90", "QFromAxisAngleEx fQw fQx fQy fQz 0 0 1 90"),
    _q3("v3norm", f"V3Normalize {_N3} 3 0 4"),
    _q3("v3normex", "V3NormalizeEx fQx fQy fQz 3 0 4"),
    _q3("v3cross.xy", f"V3Crossproduct {_N3} 1 0 0 0 1 0"),
    _q3("v3crossex.xy", "V3CrossproductEx fQx fQy fQz 1 0 0 0 1 0"),
]

# Fifteenth set: mod local data (Set/Get/Exists/Remove/GetAllModLocalData) with a
# number, a string and a form, and the keys of GetUserTime (values change, so only the
# shape is probed). Mod local data belongs to the calling script's mod; a
# runtime-compiled script belongs to none, whatever that means for the store. Every key
# set is removed in the same probe. Session only.
def _ml(pid, body, text=False, exp=None, page=""):
    return (pid, "ref rF\nfloat fRes\nBegin Function { }\n" + _form("rF", NF["pistol10"]) + body, text, exp, page)


DATA_PROBES = [
    _ml("mld.number", 'SetModLocalData "nvfNum" 5.5\nlet fRes := GetModLocalData "nvfNum"\nRemoveModLocalData "nvfNum"\nSetFunctionValue fRes', exp="5.5", page="SetModLocalData / GetModLocalData"),
    _ml("mld.string", 'SetModLocalData "nvfStr" "abc"\nSetFunctionValue (GetModLocalData "nvfStr")\nRemoveModLocalData "nvfStr"', True, "abc", "SetModLocalData (a string)"),
    _ml("mld.form", 'SetModLocalData "nvfForm" rF\nSetFunctionValue (GetFormIDString (GetModLocalData "nvfForm"))\nRemoveModLocalData "nvfForm"', True, "0000434F", "SetModLocalData (a form)"),
    _ml("mld.exists", 'SetModLocalData "nvfNum" 1\nlet fRes := ModLocalDataExists "nvfNum"\nRemoveModLocalData "nvfNum"\nSetFunctionValue fRes', exp="1", page="ModLocalDataExists"),
    _ml("mld.exists.none", 'SetFunctionValue (ModLocalDataExists "nvfNone")', exp="0", page="ModLocalDataExists"),
    _ml("mld.get.none", 'SetFunctionValue (GetModLocalData "nvfNone")', exp="0", page="GetModLocalData (0 when no data)"),
    _ml("mld.case", 'SetModLocalData "nvfCase" 7\nlet fRes := GetModLocalData "NVFCASE"\nRemoveModLocalData "nvfCase"\nSetFunctionValue fRes'),
    _ml("mld.remove.existed", 'SetModLocalData "nvfNum" 1\nSetFunctionValue (RemoveModLocalData "nvfNum")', exp="1", page="RemoveModLocalData"),
    _ml("mld.remove.none", 'SetFunctionValue (RemoveModLocalData "nvfNone")', exp="0", page="RemoveModLocalData (0 if none existed)"),
    _ml("mld.all.size", 'SetModLocalData "nvfA" 1\nSetModLocalData "nvfB" 2\nlet fRes := ar_Size (GetAllModLocalData)\nRemoveModLocalData "nvfA"\nRemoveModLocalData "nvfB"\nSetFunctionValue fRes'),
    _ml("mld.all.haskey", 'SetModLocalData "nvfA" 1\nlet fRes := ar_HasKey (GetAllModLocalData) "nvfA"\nRemoveModLocalData "nvfA"\nSetFunctionValue fRes'),
    _ml("usertime.size", "SetFunctionValue (ar_Size (GetUserTime))"),
] + [_ml(f"usertime.has.{k}", f'SetFunctionValue (ar_HasKey (GetUserTime) "{k}")', exp="1", page="GetUserTime")
     for k in ("Year", "Month", "DayOfWeek", "Day", "Hour", "Minute", "Second", "Millisecond")]

# Sixteenth set: inventory references and clones. A 10mm pistol is added to the player,
# equipped, looked at through foreach (inventory references), unequipped and removed,
# one step each, in order; clones are made with TempCloneForm (never saved). Session
# only. The player may own 10mm pistols in the save, so counts are compared before and
# after the add.
def _inv(pid, body, text=False, exp=None, page=""):
    return (pid, "ref rPlayer\nref rGun\nref rItem\nfloat fRes\nBegin Function { }\n" + _form("rPlayer", NF["player"])
            + _form("rGun", NF["pistol10"]) + body, text, exp, page)


def _each(expr):
    """fRes := expr for the pistol's inventory reference, computed inside foreach: after a
    container foreach the loop variable is no longer a valid form, and calling a function
    on it ends the function without a value; a break alone is fine [observed:
    foreach.break.alone, foreach.read.after(.valid), foreach.break.then.read]."""
    return ("let fRes := -1\nforeach rItem <- rPlayer\n  if rItem.GetBaseObject == rGun\n"
            f"    let fRes := {expr}\n  endif\nloop\nSetFunctionValue fRes")
INV_PROBES = [
    # Ordered steps (the game runs one per loop; an added or equipped item may only show
    # up a loop later): count, add, look, equip, look, unequip, look, remove.
    _inv("inv.count.before", "SetFunctionValue (ar_Size (rPlayer.GetInvRefsForItem rGun))"),
    _inv("inv.add", "rPlayer.AddItem rGun 1 1\nSetFunctionValue 1", exp="1", page="AddItem"),
    _inv("inv.count.after", "SetFunctionValue (ar_Size (rPlayer.GetInvRefsForItem rGun))"),
    _inv("invref.found", _each("rItem.GetBaseObject == rGun"), exp="1", page="foreach over a container"),
    _inv("invref.foreach", _each("rItem.IsInventoryRef"), exp="1", page="IsInventoryRef"),
    # Which part ended the function without a value: a break out of a container foreach,
    # reading the loop variable after the loop, or both.
    _inv("foreach.break.alone", "foreach rItem <- rPlayer\n  if rItem.GetBaseObject == rGun\n    break\n  endif\nloop\nSetFunctionValue 1"),
    _inv("foreach.read.after", "foreach rItem <- rPlayer\nloop\nSetFunctionValue (rItem.IsInventoryRef)"),
    _inv("foreach.read.after.valid", "foreach rItem <- rPlayer\nloop\nSetFunctionValue (IsFormValid rItem)"),
    _inv("foreach.break.then.read", "foreach rItem <- rPlayer\n  if rItem.GetBaseObject == rGun\n    break\n  endif\nloop\nSetFunctionValue (rItem.IsInventoryRef)"),
    _inv("inv.equip", "rPlayer.EquipItem rGun 0 1\nSetFunctionValue 1", exp="1", page="EquipItem"),
    _inv("equipped.after.equip", _each("rItem.IsEquipped")),
    _inv("getequipped.after.equip", "SetFunctionValue (rPlayer.GetEquipped rGun)", exp="1", page="GetEquipped"),
    _inv("inv.unequip", "rPlayer.UnequipItem rGun 0 1\nSetFunctionValue 1", exp="1", page="UnequipItem"),
    _inv("equipped.not", _each("rItem.IsEquipped")),
    _inv("inv.remove", "rPlayer.RemoveItem rGun 1 1\nSetFunctionValue 1", exp="1", page="RemoveItem"),
    _inv("clone.iscloned", "let rItem := TempCloneForm rGun\nSetFunctionValue (IsClonedForm rItem)", exp="1", page="IsClonedForm"),
    _inv("clone.original", "SetFunctionValue (IsClonedForm rGun)", exp="0", page="IsClonedForm"),
    _inv("clone.name", "let rItem := TempCloneForm rGun\nSetFunctionValue (GetName rItem)", True),
    _inv("clone.modindex", "let rItem := TempCloneForm rGun\nSetFunctionValue (GetSourceModIndex rItem)"),
    _inv("clone.type", "let rItem := TempCloneForm rGun\nSetFunctionValue (GetType rItem)"),
    _inv("clone.clip", "let rItem := TempCloneForm rGun\nSetFunctionValue (GetWeaponClipRounds rItem)"),
    _inv("clone.indexed.iscloned", "let rItem := TempCloneForm rGun 1\nSetFunctionValue (IsClonedForm rItem)"),
    _inv("clone.indexed.modindex", "let rItem := TempCloneForm rGun 1\nSetFunctionValue (GetSourceModIndex rItem)"),
]

# Seventeenth set: the extender core's second-pass families. Weapon mods, faction
# arrays, race lists, ownership and key names are read-only; the perk, item-walk and
# disabled-key rows change the session only (each is undone in a later step) and run in
# order. Ids ending in `.save` read state of the owner's save. Forms from fnvtool esm
# inventory and world refs [observed]: WeapNVVarmintRifle 07EA24, Caucasian 000019,
# Toughness 031DE0, Doc's dresser 103E37 (persistent), Sunny Smiles' reference 104E85 (in the
# Prospector Saloon).
SF = dict(NF, varmint="7EA24", sledge="4352", caucasian="19", toughness="31DE0", dresser="103E37",
          door="103E61", sunnyref="104E85", sunny="104E84")


def _s2(pid, body, text=False, exp=None, page=""):
    return (pid, "ref rPlayer\nref rObj\nref rOther\nfloat fRes\nBegin Function { }\n" + _form("rPlayer", NF["player"])
            + body, text, exp, page)


def _s2f(pid, key, expr, text=False, exp=None, page=""):
    return _s2(pid, _form("rObj", SF[key]) + f"SetFunctionValue ({expr})", text, exp, page)


_WALK = ("let fRes := 0\nlet rOther := rPlayer.GetFirstRefForItem rObj\nwhile IsFormValid rOther\n"
         "  let fRes += 1\n  let rOther := rPlayer.GetNextRefForItem rObj\nloop\nSetFunctionValue fRes")


SECOND_PROBES = (
    [_s2f(f"mod.{w}.{i}", w, f"GetFormIDString (GetWeaponItemMod {i} rObj)", True) for w in ("pistol10", "varmint", "sledge") for i in (1, 2, 3)]
    + [_s2f(f"modeffect.{w}.{i}", w, f"GetWeaponItemModEffect {i} rObj") for w in ("pistol10", "varmint", "sledge") for i in (1, 2, 3)]
    + [_s2f(f"modvalue{v}.{w}.{i}", w, f"GetWeaponItemModValue{v} {i} rObj") for v in (1, 2) for w in ("pistol10", "varmint") for i in (1, 2, 3)]
    + [x for k in ("doc", "sunny") for x in (
        [_s2f(f"basefactions.{k}.size", k, "ar_Size (GetBaseFactions rObj)")]
        + [_s2f(f"basefactions.{k}.{i}", k, f"GetFormIDString ((GetBaseFactions rObj)[{i}])", True) for i in range(3)]
        + [_s2f(f"baseranks.{k}.size", k, "ar_Size (GetBaseRanks rObj)"),
           _s2f(f"baseranks.{k}.0", k, "(GetBaseRanks rObj)[0]")])]
    + [_s2f("activefactions.sunnyref.size.save", "sunnyref", "ar_Size (rObj.GetActiveFactions)"),
       _s2f("activefactions.sunnyref.0.save", "sunnyref", "GetFormIDString ((rObj.GetActiveFactions)[0])", True),
       _s2f("activeranks.sunnyref.0.save", "sunnyref", "(rObj.GetActiveRanks)[0]"),
       _s2("activefactions.player.size.save", "SetFunctionValue (ar_Size (rPlayer.GetActiveFactions))")]
    + [_s2f("racename.doc", "doc", "GetRaceName rObj", True),
       _s2f("racename.sunny", "sunny", "GetRaceName rObj", True),
       _s2f("racehairs.size", "caucasian", "ar_Size (GetRaceHairs rObj)"),
       _s2f("racehairs.0", "caucasian", "GetFormIDString ((GetRaceHairs rObj)[0])", True),
       _s2f("raceeyes.size", "caucasian", "ar_Size (GetRaceEyes rObj)"),
       _s2f("raceeyes.0", "caucasian", "GetFormIDString ((GetRaceEyes rObj)[0])", True)]
    + [_s2(f"owned.{t}.{o}", _form("rObj", SF[t]) + _form("rOther", SF[o]) + "SetFunctionValue (rObj.IsOwned rOther)")
       for t in ("dresser", "door") for o in ("doc", "docref", "player")]
    + [_s2(f"hasownership.{o}.{t}", _form("rObj", SF[t]) + _form("rOther", SF[o]) + "SetFunctionValue (rOther.HasOwnership rObj)")
       for t in ("dresser", "door") for o in ("docref", "player")]
    + [_s2(f"keyname.{k}", f"SetFunctionValue (GetKeyName {k})", True) for k in (17, 57, 256, 1, 0)]
    + [_s2(f"iscontrol.{k}", f"SetFunctionValue (IsControl {k})") for k in (17, 99, 57)]
    + [_s2("pressedkeys.size", "SetFunctionValue (ar_Size (GetPressedKeys))"),
       _s2("disabledkeys.before", "SetFunctionValue (ar_Size (GetDisabledKeys))"),
       _s2("disablekey.34", "DisableKey 34\nSetFunctionValue 1"),
       _s2("disabledkeys.after", "SetFunctionValue (ar_Size (GetDisabledKeys))"),
       _s2("disabledkeys.after.0", "SetFunctionValue ((GetDisabledKeys)[0])"),
       _s2("enablekey.34", "EnableKey 34\nSetFunctionValue 1"),
       _s2("disabledkeys.end", "SetFunctionValue (ar_Size (GetDisabledKeys))")]
    + [_s2f("perk.before.save", "toughness", "rPlayer.GetPerkRank rObj"),
       _s2f("altperk.before.save", "toughness", "rPlayer.GetAltPerkRank rObj"),
       _s2("perk.add", _form("rObj", SF["toughness"]) + "rPlayer.AddPerk rObj\nSetFunctionValue 1", exp="1", page="AddPerk"),
       _s2f("perk.after.add", "toughness", "rPlayer.GetPerkRank rObj"),
       _s2("perk.remove", _form("rObj", SF["toughness"]) + "rPlayer.RemovePerk rObj\nSetFunctionValue 1", exp="1", page="RemovePerk"),
       _s2f("perk.after.remove", "toughness", "rPlayer.GetPerkRank rObj")]
    + [_s2f("items.before.save", "stimpak", "ar_Size (rPlayer.GetInvRefsForItem rObj)"),
       _s2("items.add", _form("rObj", SF["stimpak"]) + "rPlayer.AddItem rObj 3 1\nSetFunctionValue 1", exp="1", page="AddItem"),
       _s2("items.walk", _form("rObj", SF["stimpak"]) + _WALK),
       _s2f("items.invrefs", "stimpak", "ar_Size (rPlayer.GetInvRefsForItem rObj)"),
       _s2("items.remove", _form("rObj", SF["stimpak"]) + "rPlayer.RemoveItem rObj 3 1\nSetFunctionValue 1", exp="1", page="RemoveItem"),
       _s2("items.walk.pistol.save", _form("rObj", SF["pistol10"]) + _WALK)]
)

# Eighteenth set: GetKeyName for every code 0-265 (keyboard scancodes, then the mouse
# buttons from 256), one row each. Read-only.
KEYNAME_PROBES = [(f"key.{k}", f"SetFunctionValue (GetKeyName {k})", True, None, "") for k in range(266)]

# Nineteenth set: base packages and spells, equipment masks, object effects, class
# names, swimming and the dialogue getters (no conversation is running). The hunting
# rifle's record carries an EITM (162C8F) and the 10mm pistol none [observed: fnvtool esm
# decode]. Read-only.
SF.update(hunting="4333", metal="3307F", t45d="4443E")
THIRD_PROBES = (
    [_s2f(f"basepackages.{k}.size", k, "ar_Size (GetBasePackages rObj)") for k in ("doc", "sunny")]
    + [_s2f(f"basepackages.{k}.0", k, "GetFormIDString ((GetBasePackages rObj)[0])", True) for k in ("doc", "sunny")]
    + [_s2f(f"basespells.{k}.size", k, "ar_Size (GetBaseSpellListSpells rObj)") for k in ("doc", "sunny")]
    + [_s2f(f"basespells.{k}.0", k, "GetFormIDString ((GetBaseSpellListSpells rObj)[0])", True) for k in ("doc", "sunny")]
    + [_s2f(f"slotsmask.{k}", k, "GetEquipmentSlotsMask rObj") for k in ("leather", "metal", "t45d", "pistol10")]
    + [_s2f(f"bipedmask.{k}", k, "GetEquipmentBipedMask rObj") for k in ("leather", "metal", "t45d")]
    + [_s2f(f"objecteffect.{k}", k, "GetFormIDString (GetObjectEffect rObj)", True) for k in ("hunting", "pistol10", "t45d")]
    + [_s2f(f"classname.{k}", k, "GetNameOfClass rObj", True) for k in ("doc", "sunny", "docref")]
    + [_s2("swimming", "SetFunctionValue (IsPlayerSwimming)"),
       _s2("dialogue.target", "SetFunctionValue (GetFormIDString (rPlayer.GetDialogueTarget))", True),
       _s2("dialogue.subject", "SetFunctionValue (GetFormIDString (rPlayer.GetDialogueSubject))", True),
       _s2("dialogue.speaker", "SetFunctionValue (GetFormIDString (rPlayer.GetDialogueSpeaker))", True)]
)

# Twentieth set: the extender core's "other" group. Steps run in order; the inventory
# rows add a 10mm pistol, 20 rounds of 10mm and 3 stimpaks to the player, equip the
# pistol, move stimpaks to Doc Mitchell's reference, and the last steps take all of it
# back (session only; the run never saves). Forms from fnvtool esm inventory [observed]:
# Ammo556mm 004240, AmmoList10mm 1537E6 (the pistol's NAM0), the leveled character
# VarVHDNCRRanger9Veteran 17A2FE, scripts GSDocMitchellScript 107230 and VCG02SCRIPT
# 10A1F0; ExtProbeLoops06Q of NVFExtProbe.esp holds `int iR`. Ids ending in `.save` read
# state of the owner's save or of the start position.
FX = dict(SF, ammo556="4240", ammolist10="1537E6", lvln="17A2FE", docscript="107230", vcg02script="10A1F0",
          ammo10="4241", cell="DEB16")
_H4 = ("ref rPlayer\nref rObj\nref rOther\nref rDoc\nref rItem\nfloat fRes\nBegin Function { }\n"
       + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]))


def _f4(pid, body, text=False, exp=None, page=""):
    return (pid, _H4 + body, text, exp, page)


def _o4(key, other=None):
    out = _form("rObj", FX[key])
    return out + (_form("rOther", FX[other]) if other else "")


def _in(base, expr):
    """fRes := expr for the player's inventory reference of `base` (rObj), in the loop."""
    return (_form("rObj", FX[base]) + "let fRes := -1\nforeach rItem <- rPlayer\n  if rItem.GetBaseObject == rObj\n"
            f"    let fRes := {expr}\n  endif\nloop\nSetFunctionValue fRes")


FOURTH_PROBES = [
    # 1. BuildRef (decimal FormIDs; hex literals do not compile at run time) and local indexes.
    _f4("buildref.player", "SetFunctionValue (GetFormIDString (BuildRef 0 20))", True),
    _f4("buildref.doc", "SetFunctionValue (GetFormIDString (BuildRef 0 1068044))", True),
    _f4("buildref.hex", "SetFunctionValue (GetFormIDString (BuildRef 0 0x104C0C))", True),
    _f4("localrefindex.docref", "SetFunctionValue (rDoc.GetLocalRefIndex)"),
    _f4("localrefindex.player", "SetFunctionValue (rPlayer.GetLocalRefIndex)"),
    # 2. Inside a runtime-compiled query.
    _f4("selfalt", "SetFunctionValue (GetFormIDString (GetSelfAlt))", True),
    _f4("currentscript.valid", "SetFunctionValue (IsFormValid (GetCurrentScript))"),
    _f4("currentscript.type", "SetFunctionValue (GetType (GetCurrentScript))"),
    _f4("currentscript.modindex", "SetFunctionValue (GetSourceModIndex (GetCurrentScript))"),
    # 3. Grids (the candidate starts in cell 000DEB16, where the save stands).
    _f4("gridstoload", "SetFunctionValue (GetGridsToLoad)"),
    _f4("ingrid.doc.0", "SetFunctionValue (GetInGrid rDoc 0)"),
    _f4("ingrid.doc.1", "SetFunctionValue (GetInGrid rDoc 1)"),
    _f4("ingridincell.player.0", _o4("cell") + "SetFunctionValue (GetInGridInCell rObj rPlayer 0)"),
    # 9 (read-only, before the inventory steps). Actors and leveled lists.
    _f4("ageclass.doc", _o4("doc") + "SetFunctionValue (GetAgeClass rObj)"),
    _f4("ageclass.sunny", _o4("sunny") + "SetFunctionValue (GetAgeClass rObj)"),
    _f4("headparts.doc.size", _o4("doc") + "SetFunctionValue (ar_Size (GetHeadParts rObj))"),
    _f4("headparts.doc.0", _o4("doc") + "SetFunctionValue (GetFormIDString ((GetHeadParts rObj)[0]))", True),
    _f4("levchar.size", _o4("lvln") + "SetFunctionValue (ar_Size (GetLevCharacterRefs rObj))"),
    _f4("levchar.0", _o4("lvln") + "SetFunctionValue (GetFormIDString ((GetLevCharacterRefs rObj)[0]))", True),
    _f4("headingx.player.doc.save", "SetFunctionValue (rPlayer.GetHeadingAngleX rDoc)"),
    # 10. Versions, script comparison, ResetAllVariables.
    _f4("fose.version", "SetFunctionValue (GetFOSEVersion)"),
    _f4("fose.revision", "SetFunctionValue (GetFOSERevision)"),
    _f4("fose.beta", "SetFunctionValue (GetFOSEBeta)"),
    _f4("comparescripts.same", _o4("docscript", "docscript") + "SetFunctionValue (CompareScripts rObj rOther)"),
    _f4("comparescripts.diff", _o4("docscript", "vcg02script") + "SetFunctionValue (CompareScripts rObj rOther)"),
    _f4("resetallvars.quest", 'let rObj := GetFormFromMod "NVFExtProbe.esp" "B40"\nSetVariable "iR" 7 rObj\n'
        'rObj.ResetAllVariables\nSetFunctionValue (GetVariable "iR" rObj)'),
    _f4("resetallvars.after", 'let rObj := GetFormFromMod "NVFExtProbe.esp" "B40"\nSetFunctionValue (GetVariable "iR" rObj)'),
    # 6. Base-form setters read back, then put back.
    _f4("playable.before", _o4("pistol10") + "SetFunctionValue (IsPlayable rObj)"),
    _f4("playable.set0", _o4("pistol10") + "SetIsPlayable 0 rObj\nlet fRes := IsPlayable rObj\nSetIsPlayable 1 rObj\nSetFunctionValue fRes"),
    _f4("questitem.before", _o4("pistol10") + "SetFunctionValue (IsQuestItem rObj)"),
    _f4("questitem.set1", _o4("pistol10") + "SetQuestItem 1 rObj\nlet fRes := IsQuestItem rObj\nSetQuestItem 0 rObj\nSetFunctionValue fRes"),
    _f4("questitem.after", _o4("pistol10") + "SetFunctionValue (IsQuestItem rObj)"),
    # 4. Ammo, with the pistol and 20 rounds added and the pistol equipped.
    _f4("inv.add.pistol", _o4("pistol10") + "rPlayer.AddItem rObj 1 1\nSetFunctionValue 1", exp="1", page="AddItem"),
    _f4("inv.add.ammo", _o4("ammo10") + "rPlayer.AddItem rObj 20 1\nSetFunctionValue 1", exp="1", page="AddItem"),
    _f4("inv.equip.pistol", _o4("pistol10") + "rPlayer.EquipItem rObj 0 1\nSetFunctionValue 1", exp="1", page="EquipItem"),
    # GetWeaponCanUseAmmo takes the weapon as the calling reference and the ammo as its one
    # argument [documented: GetWeaponCanUseAmmo]. Called with two form arguments
    # (`GetWeaponCanUseAmmo rWeapon rAmmo`) it crashed the game (2026-10-27, first try of
    # this set) [observed], so only the documented form is probed, on the pistol's
    # inventory reference.
    _f4("ammo.canuse.invref.10", _form("rOther", FX["ammo10"]) + _in("pistol10", "rItem.GetWeaponCanUseAmmo rOther")),
    _f4("ammo.canuse.invref.556", _form("rOther", FX["ammo556"]) + _in("pistol10", "rItem.GetWeaponCanUseAmmo rOther")),
    _f4("ammo.hasequipped.10", _o4("ammo10") + "SetFunctionValue (rPlayer.HasAmmoEquipped rObj)"),
    _f4("ammo.hasequipped.556", _o4("ammo556") + "SetFunctionValue (rPlayer.HasAmmoEquipped rObj)"),
    _f4("ammo.equippedcanuse.10", _o4("ammo10") + "SetFunctionValue (rPlayer.GetEquippedWeaponCanUseAmmo rObj)"),
    _f4("ammo.equippedcanuse.556", _o4("ammo556") + "SetFunctionValue (rPlayer.GetEquippedWeaponCanUseAmmo rObj)"),
    _f4("ammo.inlist", _o4("ammolist10") + "SetFunctionValue (rPlayer.IsEquippedAmmoInList rObj)"),
    _f4("ammo.usesammolist", _o4("ammolist10") + "SetFunctionValue (rPlayer.GetEquippedWeaponUsesAmmoList rObj)"),
    _f4("ammo.current", "SetFunctionValue (GetFormIDString (GetPlayerCurrentAmmo))", True),
    _f4("ammo.rounds", "SetFunctionValue (GetPlayerCurrentAmmoRounds)"),
    # 5. Condition of the equipped pistol and of its inventory reference.
    _f4("health.equipped.5", "SetFunctionValue (rPlayer.GetEquippedCurrentHealth 5)"),
    _f4("health.invref", _in("pistol10", "rItem.GetCurrentHealth")),
    _f4("health.set50", _in("pistol10", "rItem.SetCurrentHealth 50") .replace("let fRes := rItem.SetCurrentHealth 50", "rItem.SetCurrentHealth 50\n    let fRes := 1")),
    _f4("health.invref.after", _in("pistol10", "rItem.GetCurrentHealth")),
    _f4("health.equipped.5.after", "SetFunctionValue (rPlayer.GetEquippedCurrentHealth 5)"),
    # 7. Inventory-reference actions on a stack of 3 stimpaks, and on the pistol.
    _f4("ir.add.stimpak", _o4("stimpak") + "rPlayer.AddItem rObj 3 1\nSetFunctionValue 1", exp="1", page="AddItem"),
    _f4("ir.count.player.0.save", _o4("stimpak") + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
    _f4("ir.count.doc.0", _o4("stimpak") + "SetFunctionValue (rDoc.GetItemCount rObj)"),
    _f4("ir.refcount", _in("stimpak", "rItem.GetRefCount")),
    _f4("ir.copyir", _in("stimpak", "rItem.CopyIR rDoc")),
    _f4("ir.count.player.1.save", _o4("stimpak") + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
    _f4("ir.count.doc.1", _o4("stimpak") + "SetFunctionValue (rDoc.GetItemCount rObj)"),
    _f4("ir.copyiralt.valid", _in("stimpak", "IsFormValid (rItem.CopyIRAlt rDoc)")),
    _f4("ir.count.doc.2", _o4("stimpak") + "SetFunctionValue (rDoc.GetItemCount rObj)"),
    _f4("ir.removemeir.none", _in("stimpak", "rItem.RemoveMeIR")),
    _f4("ir.count.player.2.save", _o4("stimpak") + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
    _f4("ir.readd.stimpak", _o4("stimpak") + "rPlayer.AddItem rObj 3 1\nSetFunctionValue 1", exp="1", page="AddItem"),
    _f4("ir.removemeir.doc", _in("stimpak", "rItem.RemoveMeIR rDoc")),
    _f4("ir.count.player.3.save", _o4("stimpak") + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
    _f4("ir.count.doc.3", _o4("stimpak") + "SetFunctionValue (rDoc.GetItemCount rObj)"),
    _f4("ir.unequipme", _in("pistol10", "rItem.UnequipMe") .replace("let fRes := rItem.UnequipMe", "rItem.UnequipMe\n    let fRes := 1")),
    _f4("ir.isequipped.after.unequipme", _in("pistol10", "rItem.IsEquipped")),
    _f4("ir.equipme", _in("pistol10", "rItem.EquipMe") .replace("let fRes := rItem.EquipMe", "rItem.EquipMe\n    let fRes := 1")),
    _f4("ir.isequipped.after.equipme", _in("pistol10", "rItem.IsEquipped")),
    _f4("ir.unequip.again", _o4("pistol10") + "rPlayer.UnequipItem rObj 0 1\nSetFunctionValue 1"),
    _f4("ir.equipitem2", _o4("pistol10") + "rPlayer.EquipItem2 rObj 0 1\nSetFunctionValue 1"),
    _f4("ir.getequipped.after.equipitem2", _o4("pistol10") + "SetFunctionValue (rPlayer.GetEquipped rObj)"),
    # 8. CreateTempRef, kept in a temporary aux variable for the next frame's read.
    _f4("tempref.valid", _o4("stimpak") + 'let rItem := CreateTempRef rObj\nrPlayer.AuxVarSetRef "*nvfTR" rItem\nSetFunctionValue (IsFormValid rItem)'),
    _f4("tempref.base", _o4("stimpak") + "let rItem := CreateTempRef rObj\nSetFunctionValue (GetFormIDString (rItem.GetBaseObject))", True),
    # `rItem.GetRefCount` on a CreateTempRef reference crashed the game (2026-10-27)
    # [observed]: not probed.
    _f4("tempref.isinvref", _o4("stimpak") + "let rItem := CreateTempRef rObj\nSetFunctionValue (rItem.IsInventoryRef)"),
    _f4("tempref.next.frame.valid", 'let rItem := rPlayer.AuxVarGetRef "*nvfTR"\nSetFunctionValue (IsFormValid rItem)'),
    # Clean-up: everything added comes back off.
    _f4("cleanup.player", _o4("pistol10", "ammo10") + "rPlayer.UnequipItem rObj 0 1\nrPlayer.RemoveItem rObj 1 1\n"
        "rPlayer.RemoveItem rOther 20 1\n" + _form("rObj", FX["stimpak"])
        + 'rPlayer.RemoveItem rObj 3 1\nrPlayer.AuxVarErase "*nvfTR"\nSetFunctionValue 1', exp="1", page="cleanup"),
    _f4("cleanup.doc", _o4("stimpak") + "let fRes := rDoc.GetItemCount rObj\nrDoc.RemoveItem rObj fRes 1\nSetFunctionValue (rDoc.GetItemCount rObj)", exp="0", page="cleanup"),
]

# Twenty-first set: the functions the owner's mod list uses most that NVForever lacks
# (docs/handoff/modlist-scan-2026-10-26.md): JIP's AddItemAlt, RemoveItemTarget,
# EquipItemAlt, MessageExAlt, IsSpellTargetAlt, IsInAir, StopIdle, GetGameDaysPassed,
# SetGameHour, GetWeaponRefModFlags, GetPerkModifier, LU's GetDistance3D and
# JohnnyGuitar's EditorIDToFormID, GetEditorID and Clamp (JohnnyGuitar 528 is loaded in
# the owner's game). Steps in order; items added are taken back at the end, and
# SetGameHour only runs when TimeScale (global 00003A) is 30, since the page says it can
# crash otherwise, and the hour is put back from a `*` aux variable. Session only.
FX.update(gamehour="38", timescale="3A", chetref="104C79", dochair="2A185")
FF = FX
FIFTH_PROBES = [
    # JohnnyGuitar.
    _f4("jg.edid.to.form", 'SetFunctionValue (GetFormIDString (EditorIDToFormID "Weap10mmPistol"))', True),
    _f4("jg.edid.to.form.case", 'SetFunctionValue (GetFormIDString (EditorIDToFormID "weap10mmpistol"))', True),
    _f4("jg.edid.to.form.ref", 'SetFunctionValue (GetFormIDString (EditorIDToFormID "DocMitchellREF"))', True),
    _f4("jg.edid.to.form.none", 'SetFunctionValue (GetFormIDString (EditorIDToFormID "nvfNoSuchEditorID"))', True),
    _f4("jg.getedid.pistol", _o4("pistol10") + "SetFunctionValue (GetEditorID rObj)", True),
    _f4("jg.getedid.docref", "SetFunctionValue (GetEditorID rDoc)", True),
    _f4("jg.getedid.player", "SetFunctionValue (GetEditorID rPlayer)", True),
    _f4("jg.clamp.high", "SetFunctionValue (Clamp 5 0 3)"),
    _f4("jg.clamp.low", "SetFunctionValue (Clamp -1 0 3)"),
    _f4("jg.clamp.inside", "SetFunctionValue (Clamp 2.5 0 3)"),
    # LU / JIP read-only.
    _f4("distance3d.door.dresser", _o4("door", "dresser") + "SetFunctionValue (rObj.GetDistance3D rOther)"),
    _f4("distance3d.doc.door.save", _o4("door") + "SetFunctionValue (rDoc.GetDistance3D rObj)"),
    _f4("inair.player", "SetFunctionValue (rPlayer.IsInAir)"),
    _f4("spelltargetalt.stimpak.save", _o4("stimpak") + "SetFunctionValue (rPlayer.IsSpellTargetAlt rObj)"),
    _f4("perkmodifier.0.save", "SetFunctionValue (rPlayer.GetPerkModifier 0 10)"),
    _f4("dayspassed.noargs.save", "SetFunctionValue (GetGameDaysPassed)"),
    _f4("dayspassed.since.2281.10.13.save", "SetFunctionValue (GetGameDaysPassed 2281 10 13)"),
    _f4("dayspassed.same.again", "SetFunctionValue ((GetGameDaysPassed) == (GetGameDaysPassed 2281 10 13))"),
    _f4("globals.timescale", _form("rObj", FF["timescale"]) + "SetFunctionValue (GetGlobalValue rObj)"),
    _f4("stopidle.player", "rPlayer.StopIdle\nSetFunctionValue 1"),
    _f4("messageexalt", 'MessageExAlt 1 "NVForever probe"\nSetFunctionValue 1'),
    # SetGameHour, guarded by TimeScale 30.
    _f4("gamehour.set", _form("rObj", FF["timescale"]) + _form("rOther", FF["gamehour"])
        + "let fRes := -1\nif (GetGlobalValue rObj) == 30\n  let fRes := GetGlobalValue rOther\n"
        '  rPlayer.AuxVarSetFlt "*nvfGH" fRes\n  if fRes < 22\n    SetGameHour (fRes + 1)\n  endif\nendif\nSetFunctionValue (fRes >= 0)'),
    _f4("gamehour.after.minus.before", _form("rOther", FF["gamehour"])
        + 'let fRes := (GetGlobalValue rOther) - (rPlayer.AuxVarGetFlt "*nvfGH")\n'
        'SetFunctionValue ((fRes > 0.99) && (fRes < 1.01))'),
    _f4("gamehour.restore", _form("rObj", FF["timescale"])
        + 'let fRes := rPlayer.AuxVarGetFlt "*nvfGH"\nrPlayer.AuxVarSetFlt "*nvfGD" (GetGameDaysPassed)\n'
        'if (GetGlobalValue rObj) == 30\n  if fRes < 22\n    SetGameHour fRes\n  endif\nendif\n'
        'rPlayer.AuxVarErase "*nvfGH"\nSetFunctionValue 1'),
    # Setting an earlier hour: does the clock go back, or on to that hour the next day?
    _f4("gamehour.restore.hours.passed", 'let fRes := ((GetGameDaysPassed) - (rPlayer.AuxVarGetFlt "*nvfGD")) * 24\n'
        'rPlayer.AuxVarErase "*nvfGD"\nSetFunctionValue (sv_Construct "%.0f" fRes)', True),
    # The owner's Amata companion mod (AMFAmata.esp): its unbuilt functions, read-only or
    # changed and put back in the same step, on Doc Mitchell, Sunny Smiles and Chet.
    # MessageBoxExAlt is left out: it opens a message box, which would hold the run.
    _f4("amata.sethair", _o4("sunny", "dochair") + "let rItem := GetHair rObj\nSetHair rOther rObj\n"
        "let fRes := (GetHair rObj) == rOther\nSetHair rItem rObj\nSetFunctionValue fRes"),
    _f4("amata.sethair.restored", _o4("sunny") + "SetFunctionValue (GetFormIDString (GetHair rObj))", True),
    _f4("amata.sethaircolor", _o4("sunny") + "let fRes := GetHairColor 0 rObj\nrPlayer.AuxVarSetFlt \"*nvfHC\" fRes\n"
        "SetHairColor 1193046 rObj\nSetFunctionValue (GetHairColor 0 rObj)"),
    _f4("amata.sethaircolor.parts", _o4("sunny") + 'SetFunctionValue (sv_Construct "%.0f %.0f %.0f" (GetHairColor 1 rObj) (GetHairColor 2 rObj) (GetHairColor 3 rObj))', True),
    _f4("amata.sethaircolor.restore", _o4("sunny") + 'SetHairColor (rPlayer.AuxVarGetFlt "*nvfHC") rObj\nrPlayer.AuxVarErase "*nvfHC"\nSetFunctionValue (GetHairColor 0 rObj)'),
    _f4("amata.modbaseav", "let fRes := rDoc.GetBaseAV Strength\nrDoc.ModBaseActorValue Strength 2\n"
        "let fRes := (rDoc.GetBaseAV Strength) - fRes\nrDoc.ModBaseActorValue Strength -2\nSetFunctionValue fRes"),
    _f4("amata.modbaseav.after", "SetFunctionValue (rDoc.GetBaseAV Strength)"),
    _f4("amata.effecttrait.get", _o4("stimpak") + "SetFunctionValue (GetNthEffectTraitNumeric rObj 0 0)"),
    _f4("amata.effecttrait.get.duration", _o4("stimpak") + "SetFunctionValue (GetNthEffectTraitNumeric rObj 0 2)"),
    _f4("amata.effecttrait.set", _o4("stimpak") + "let fRes := GetNthEffectTraitNumeric rObj 0 0\nSetNthEffectTraitNumeric rObj 0 0 (fRes + 5)\n"
        "let fRes := (GetNthEffectTraitNumeric rObj 0 0) - fRes\nSetNthEffectTraitNumeric rObj 0 0 ((GetNthEffectTraitNumeric rObj 0 0) - 5)\nSetFunctionValue fRes"),
    _f4("amata.effecttrait.after", _o4("stimpak") + "SetFunctionValue (GetNthEffectTraitNumeric rObj 0 0)"),
    _f4("amata.valuealt.stimpak", _o4("stimpak") + "SetFunctionValue (GetValueAlt rObj)"),
    _f4("amata.valuealt.pistol", _o4("pistol10") + "SetFunctionValue (GetValueAlt rObj)"),
    _f4("amata.merchant.chet", _o4("chetref") + "SetFunctionValue (GetFormIDString (rObj.GetMerchantContainer))", True),
    _f4("amata.merchant.doc", "SetFunctionValue (GetFormIDString (rDoc.GetMerchantContainer))", True),
    _f4("amata.isfleeing.doc", "SetFunctionValue (rDoc.IsFleeing)"),
    _f4("amata.allitemrefs.doc.size.save", "SetFunctionValue (ar_Size (rDoc.GetAllItemRefs))"),
    _f4("amata.allitemrefs.player.size.save", "SetFunctionValue (ar_Size (rPlayer.GetAllItemRefs))"),
    _f4("amata.calcdamage.pistol.save", _o4("pistol10") + "SetFunctionValue (rPlayer.GetCalculatedWeaponDamage rObj)"),
    _f4("amata.crosshairex.save", "SetFunctionValue (GetFormIDString (GetCrosshairRefEx))", True),
    _f4("amata.teammatekillable", "SetFunctionValue (rDoc.ToggleTeammateKillable 1)"),
    _f4("amata.teammatekillable.off", "SetFunctionValue (rDoc.ToggleTeammateKillable 0)"),
    _f4("amata.wheeldisabled", "rDoc.SetWheelDisabled 1\nrDoc.SetWheelDisabled 0\nSetFunctionValue 1"),
    _f4("amata.update3d", "rDoc.Update3D\nSetFunctionValue 1"),
    _f4("amata.nohealthreset", "ToggleNoHealthReset 1 0.5\nToggleNoHealthReset 0 0.5\nSetFunctionValue 1"),
    # Items: AddItemAlt with a condition and equip, mod flags, EquipItemAlt, RemoveItemTarget.
    _f4("additemalt.stimpak", _o4("stimpak") + "rPlayer.AddItemAlt rObj 2 1 0 1\nSetFunctionValue 1", exp="1", page="AddItemAlt"),
    _f4("additemalt.pistol", _o4("pistol10") + "rPlayer.AddItemAlt rObj 1 0.5 1 1\nSetFunctionValue 1", exp="1", page="AddItemAlt"),
    _f4("additemalt.pistol.equipped", _o4("pistol10") + "SetFunctionValue (rPlayer.GetEquipped rObj)"),
    _f4("additemalt.pistol.health", _in("pistol10", "rItem.GetCurrentHealth")),
    _f4("modflags.invref", _in("pistol10", "rItem.GetWeaponRefModFlags")),
    _f4("modflags.equipped", "SetFunctionValue (rPlayer.GetEquippedWeaponModFlags)"),
    _f4("equipitemalt.unequip", _o4("pistol10") + "rPlayer.UnequipItem rObj 0 1\nSetFunctionValue 1"),
    _f4("equipitemalt", _o4("pistol10") + "rPlayer.EquipItemAlt rObj 0 1\nSetFunctionValue 1"),
    _f4("equipitemalt.equipped", _o4("pistol10") + "SetFunctionValue (rPlayer.GetEquipped rObj)"),
    _f4("amata.currentammo", "SetFunctionValue (GetFormIDString (rPlayer.GetCurrentAmmo))", True),
    _f4("amata.itemrefhealth.0", _in("pistol10", "rItem.GetItemRefCurrentHealth 0")),
    _f4("amata.itemrefhealth.1", _in("pistol10", "rItem.GetItemRefCurrentHealth 1")),
    _f4("amata.itemrefhealth.2", _in("pistol10", "rItem.GetItemRefCurrentHealth 2")),
    # Does every freshly added item come in below its base health (the pistol read 79.92)?
    _f4("fresh.leather.add", _o4("leather") + "rPlayer.AddItem rObj 1 1\nSetFunctionValue 1", exp="1", page="AddItem"),
    _f4("fresh.leather.health", _in("leather", "rItem.GetCurrentHealth")),
    _f4("fresh.leather.base", _o4("leather") + "SetFunctionValue (GetBaseHealth rObj)"),
    _f4("removeitemtarget.doc.before", _o4("stimpak") + "SetFunctionValue (rDoc.GetItemCount rObj)"),
    _f4("removeitemtarget", _o4("stimpak") + "rPlayer.RemoveItemTarget rObj rDoc 2 0\nSetFunctionValue 1"),
    _f4("removeitemtarget.doc.after", _o4("stimpak") + "SetFunctionValue (rDoc.GetItemCount rObj)"),
    # Clean-up.
    _f4("cleanup.fifth", _o4("pistol10", "stimpak") + "rPlayer.UnequipItem rObj 0 1\nrPlayer.RemoveItem rObj 1 1\n"
        "rDoc.RemoveItem rOther 2 1\n" + _form("rObj", FX["leather"]) + "rPlayer.RemoveItem rObj 1 1\nSetFunctionValue 1",
        exp="1", page="cleanup"),
]

# Twenty-second set: the xNVSE "other" group and the player-control locks, as the
# extender core asked (its values are hypotheses until these rows). Steps in order; every
# change is put back in the same or a later step (session only). Forms from fnvtool esm
# inventory and world refs [observed]: VCG01RemoveDocCarebearModeTriggerSCRIPT 11E65E,
# the Prospector Saloon's door 10618E (persistent), key HVCollarKey 175A56, NCR 0A46E7,
# BoS 01D3FE. A string_var is declared before Begin for Sv_Set.
FX.update(carebear="11E65E", saloondoor="10618E", collarkey="175A56", bos="1D3FE", docbase="104C0C")
_H5 = ("ref rPlayer\nref rObj\nref rOther\nref rDoc\nref rItem\nfloat fRes\nstring_var sText\nBegin Function { }\n"
       + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]))


def _f5(pid, body, text=False, exp=None, page=""):
    return (pid, _H5 + body, text, exp, page)


def _flags(j):
    """The 7 Alt flags with only flag j set."""
    return " ".join("1" if i == j else "0" for i in range(7))


_SEEN = " + ".join(f"(GetPlayerControlsDisabledAlt 1 {_flags(j)}) * {1 << j}" for j in range(7))
NVSE_FIFTH_PROBES = (
    # 1. Each DisablePlayerControlsAltEx bit, read back through the Alt getter's 7 flags
    # (bit j of the result: flag j seen as disabled), then enabled again.
    [_f5(f"controls.altex.bit{k}", f"DisablePlayerControlsAltEx {1 << k}\nlet fRes := {_SEEN}\n"
         f"EnablePlayerControlsAltEx {1 << k}\nSetFunctionValue fRes") for k in range(8)]
    + [_f5("controls.altex.none.after", f"SetFunctionValue ({_SEEN})")]
    # 2. Does the vanilla getter see an Alt lock?
    + [_f5("controls.alt.vanilla.sees", "DisablePlayerControlsAlt 1 0 0 0 0 0 0\nlet fRes := GetPlayerControlsDisabled 1 0 0 0 0 0 0\n"
           "EnablePlayerControlsAlt 1 0 0 0 0 0 0\nSetFunctionValue fRes"),
       # 3. A missing Alt argument.
       _f5("controls.alt.missing.args", "DisablePlayerControlsAlt 1\nlet fRes := GetPlayerControlsDisabledAlt 0 0 1\n"
           "let fRes := fRes * 10 + (GetPlayerControlsDisabledAlt 1 1 0 0 0 0 0 0)\nEnablePlayerControlsAlt 1\nSetFunctionValue fRes")]
    # 4. Console echo.
    + [_f5("consoleecho.start", "SetFunctionValue (GetConsoleEcho)"),
       _f5("consoleecho.set0.returns", "let fRes := SetConsoleEcho 0\nlet fRes := fRes * 10 + (GetConsoleEcho)\nSetConsoleEcho 1\nSetFunctionValue fRes")]
    # 5. Default forms.
    + [_f5("defaultform.0", "SetFunctionValue (GetFormIDString (GetNthDefaultForm 0))", True),
       _f5("defaultforms.size", "SetFunctionValue (ar_Size (GetDefaultForms))"),
       _f5("defaultform.set0", _o4("pistol10") + "let rItem := GetNthDefaultForm 0\nSetNthDefaultForm 0 rObj\n"
           "let rOther := GetNthDefaultForm 0\nSetNthDefaultForm 0 rItem\nSetFunctionValue (GetFormIDString rOther)", True),
       _f5("defaultform.0.restored", "SetFunctionValue (GetFormIDString (GetNthDefaultForm 0))", True)]
    # 6. Doc's front door (a load door): teleport target, then pointed at the saloon's door and back.
    + [_f5(f"door.teleport.{a.lower()}", _o4("door") + f"SetFunctionValue (rObj.GetDoorTeleport{a})") for a in ("X", "Y", "Z", "Rot")]
    + [_f5("door.teleport.set", _o4("door", "saloondoor") + "let rItem := rObj.GetLinkedDoor\n"
           'rPlayer.AuxVarSetFlt "*nvfDX" (rObj.GetDoorTeleportX)\nrPlayer.AuxVarSetFlt "*nvfDY" (rObj.GetDoorTeleportY)\n'
           'rPlayer.AuxVarSetFlt "*nvfDZ" (rObj.GetDoorTeleportZ)\nrPlayer.AuxVarSetFlt "*nvfDR" (rObj.GetDoorTeleportRot)\n'
           'rPlayer.AuxVarSetRef "*nvfDL" rItem\nrObj.SetDoorTeleport rOther\nSetFunctionValue (GetFormIDString (rObj.GetLinkedDoor))', True),
       _f5("door.teleport.set.x", _o4("door") + "SetFunctionValue (rObj.GetDoorTeleportX)"),
       _f5("door.teleport.restore", _o4("door") + 'let rItem := rPlayer.AuxVarGetRef "*nvfDL"\n'
           'rObj.SetDoorTeleport rItem (rPlayer.AuxVarGetFlt "*nvfDX") (rPlayer.AuxVarGetFlt "*nvfDY") (rPlayer.AuxVarGetFlt "*nvfDZ") (rPlayer.AuxVarGetFlt "*nvfDR")\n'
           + "".join(f'rPlayer.AuxVarErase "*nvfD{c}"\n' for c in "XYZRL") + "SetFunctionValue (GetFormIDString (rObj.GetLinkedDoor))", True),
       _f5("door.teleport.restored.x", _o4("door") + "SetFunctionValue (rObj.GetDoorTeleportX)")]
    # 7-8. Explicit references and script commands of a vanilla script.
    + [_f5("explicitrefs.count", _o4("carebear") + "SetFunctionValue (GetNumExplicitRefs rObj)"),
       _f5("explicitrefs.0", _o4("carebear") + "SetFunctionValue (GetFormIDString (GetNthExplicitRef 0 rObj))", True),
       _f5("hasscriptcommand.4192", _o4("carebear") + "SetFunctionValue (HasScriptCommand 4192 rObj)"),
       _f5("hasscriptcommand.4109", _o4("carebear") + "SetFunctionValue (HasScriptCommand 4109 rObj)"),
       _f5("hasscriptcommand.4192.event0", _o4("carebear") + "SetFunctionValue (HasScriptCommand 4192 rObj 0)"),
       _f5("hasscriptcommand.4192.event1", _o4("carebear") + "SetFunctionValue (HasScriptCommand 4192 rObj 1)")]
    # 9. Permanent flags.
    + [_f5("permanent.param", "SetFunctionValue (GetPermanent rDoc)"),
       _f5("permanent.calling", "SetFunctionValue (rDoc.GetPermanent)"),
       _f5("permanent.set", "let fRes := GetPermanent rDoc\nSetPermanent 1 rDoc\nlet fRes := fRes * 10 + (GetPermanent rDoc)\n"
           "let fRes := fRes * 10 + (rDoc.GetPermanent)\nSetPermanent 0 rDoc\nSetFunctionValue fRes")]
    # 10. RemoveScript, then the script put back with SetScript.
    + [_f5("removescript", _o4("docbase") + "let rItem := RemoveScript rObj\nlet rOther := rDoc.GetScript\n"
           "SetScript rItem rObj\nSetFunctionValue (sv_Construct \"%z %z\" (GetFormIDString rItem) (GetFormIDString rOther))", True),
       _f5("removescript.restored", "SetFunctionValue (GetFormIDString (rDoc.GetScript))", True)]
    # 11. RunScript on scripts without a Function block.
    + [_f5("runscript.quest", _o4("vcg02script") + "SetFunctionValue (RunScript rObj)"),
       _f5("runscript.object", _o4("carebear") + "SetFunctionValue (RunScript rObj)")]
    # 12. SetEditorID on a clone.
    + [_f5("seteditorid.clone", _o4("pistol10") + 'let rItem := TempCloneForm rObj\nSetEditorID rItem "nvfCloneProbe"\n'
           'SetFunctionValue (sv_Construct "%z %g" (GetEditorID rItem) ((EditorIDToFormID "nvfCloneProbe") == rItem))', True)]
    # 13-15. Formatted names and strings.
    + [_f5("setnameex", _o4("pistol10") + 'SetNameEx "Gun Five" rObj\nlet sText := GetName rObj\nSetNameEx "10mm Pistol" rObj\nSetFunctionValue sText', True),
       _f5("setnameex.format", _o4("pistol10") + 'SetNameEx "Gun %g" 5 rObj\nlet sText := GetName rObj\nSetNameEx "10mm Pistol" rObj\nSetFunctionValue sText', True),
       _f5("setnameex.restored", _o4("pistol10") + "SetFunctionValue (GetName rObj)", True),
       _f5("rankname.set", _o4("bos") + 'let sText := GetNthFactionRankName rObj 0\nSetNthFactionRankNameEx "Boss %g" 1 rObj 0\n'
           'let sText := GetNthFactionRankName rObj 0\nSetNthFactionRankNameEx "Squire" rObj 0\nSetFunctionValue sText', True),
       _f5("rankname.restored", _o4("bos") + "SetFunctionValue (GetNthFactionRankName rObj 0)", True),
       _f5("sv_set", 'Sv_Set "n=%g" 3 sText\nSetFunctionValue sText', True)]
    # 16. Power armour and controls flags.
    + [_f5("powerarmor.leather", _o4("leather") + "SetFunctionValue (IsPowerArmor rObj)"),
       _f5("powerarmor.t45d", _o4("t45d") + "SetFunctionValue (IsPowerArmor rObj)"),
       _f5("powerarmor.set", _o4("leather") + "SetIsPowerArmor 1 rObj\nlet fRes := IsPowerArmor rObj\nSetIsPowerArmor 0 rObj\nSetFunctionValue fRes"),
       _f5("iscontrol.88.before", "SetFunctionValue (IsControl 88)"),
       _f5("iscontrol.88.set", "SetIsControl 88 1\nlet fRes := IsControl 88\nSetIsControl 88 0\nSetFunctionValue fRes"),
       _f5("iscontrol.88.after", "SetFunctionValue (IsControl 88)")]
    # 17. Hotkeys (the player carries stimpaks in the save; hotkey 2's item is put back).
    + [_f5("hotkey.2.before.save", "SetFunctionValue (GetFormIDString (GetHotkeyItem 2))", True),
       _f5("hotkey.2.set", _o4("stimpak") + 'let rItem := GetHotkeyItem 2\nrPlayer.AuxVarSetRef "*nvfHK" rItem\n'
           "SetHotkeyItem 2 rObj\nSetFunctionValue (GetFormIDString (GetHotkeyItem 2))", True),
       _f5("hotkey.2.clear", "ClearHotkey 2\nSetFunctionValue (GetFormIDString (GetHotkeyItem 2))", True),
       _f5("hotkey.2.restore", 'let rItem := rPlayer.AuxVarGetRef "*nvfHK"\nif IsFormValid rItem\n  SetHotkeyItem 2 rItem\nendif\n'
           'rPlayer.AuxVarErase "*nvfHK"\nSetFunctionValue 1'),
       _f5("hotkey.3.not.carried", _o4("collarkey") + 'let rItem := GetHotkeyItem 3\nSetHotkeyItem 3 rObj\nlet rOther := GetHotkeyItem 3\n'
           "if IsFormValid rItem\n  SetHotkeyItem 3 rItem\nelse\n  ClearHotkey 3\nendif\nSetFunctionValue (GetFormIDString rOther)", True)]
    # 18. Owned items: condition and owner of the new item.
    + [_f5("owned.add", _o4("pistol10", "docbase") + "rPlayer.AddItemOwnership rObj 1 rOther 0\nSetFunctionValue 1"),
       _f5("owned.health", _in("pistol10", "rItem.GetCurrentHealth")),
       _f5("owned.owner", _in("pistol10", "GetFormIDString (rItem.GetOwner)").replace("let fRes := GetFormIDString", "let sText := GetFormIDString").replace("SetFunctionValue fRes", "SetFunctionValue sText"), True),
       _f5("owned.remove", _o4("pistol10") + "rPlayer.RemoveItem rObj 1 1\nSetFunctionValue 1"),
       _f5("ownedpercent.add", _o4("pistol10", "docbase") + "rPlayer.AddItemHealthPercentOwner rObj 1 0.5 rOther 0\nSetFunctionValue 1"),
       _f5("ownedpercent.health", _in("pistol10", "rItem.GetCurrentHealth")),
       _f5("ownedpercent.remove", _o4("pistol10") + "rPlayer.RemoveItem rObj 1 1\nSetFunctionValue 1")]
    # 19. Keys on Doc's door (unlocked) and on his dresser.
    + [_f5("openkey.door", _o4("door") + "SetFunctionValue (GetFormIDString (rObj.GetOpenKey))", True),
       # `rDoor.SetOpenKey rKey` on Doc's unlocked door crashed the game (2026-10-27) [observed]: not probed.
       _f5("openkey.door.restored", _o4("door") + "SetFunctionValue (GetFormIDString (rObj.GetOpenKey))", True),
       _f5("openkey.dresser", _o4("dresser") + "SetFunctionValue (GetFormIDString (rObj.GetOpenKey))", True)]
    # 21. The fly camera.
    + [_f5("tfc.before", "SetFunctionValue (GetTFC)"),
       _f5("tfc.toggle", "ToggleFlyCam\nSetFunctionValue 1"),
       _f5("tfc.after", "SetFunctionValue (GetTFC)"),
       _f5("tfc.toggle.back", "ToggleFlyCam\nSetFunctionValue 1"),
       _f5("tfc.end", "SetFunctionValue (GetTFC)")]
    # 22. The clip of the equipped pistol.
    + [_f5("clip.add", _o4("pistol10", "ammo10") + "rPlayer.AddItem rObj 1 1\nrPlayer.AddItem rOther 20 1\nSetFunctionValue 1"),
       _f5("clip.equip", _o4("pistol10") + "rPlayer.EquipItem rObj 0 1\nSetFunctionValue 1"),
       _f5("clip.before", "SetFunctionValue (GetPlayerCurrentAmmoRounds)"),
       _f5("clip.set3", "SetPlayerCurrentAmmoRounds 3\nSetFunctionValue 1"),
       _f5("clip.after", "SetFunctionValue (GetPlayerCurrentAmmoRounds)"),
       _f5("clip.cleanup", _o4("pistol10", "ammo10") + "rPlayer.UnequipItem rObj 0 1\nrPlayer.RemoveItem rObj 1 1\n"
           "rPlayer.RemoveItem rOther 20 1\nSetFunctionValue 1", exp="1", page="cleanup")]
)

# Twenty-third set: tokens, INI settings, event-handler lists, command opcodes and the
# debug functions, probed before NVForever builds them. Steps in order; every change
# is put back (session only). The player carries stimpaks in the owner's save; the
# collar key (175A56) is not carried. The INI settings read are in Fallout.ini /
# FalloutPrefs.ini of the owner's install.
_HE = ("ref rPlayer\nref rObj\nref rOther\nfloat fRes\nstring_var sText\narray_var aList\nBegin Function { }\n"
       + _form("rPlayer", NF["player"]))


def _f6(pid, body, text=False, exp=None, page=""):
    return (pid, _HE + body, text, exp, page)


_LAMBDA = "(Begin Function {ref rA, ref rB}\nEnd)"
SIXTH_PROBES = [
    # Tokens: value and ref on a carried item and on an item not carried.
    _f6("token.value.before", _o4("stimpak") + "SetFunctionValue (rPlayer.GetTokenValue rObj)"),
    _f6("token.ref.before", _o4("stimpak") + "SetFunctionValue (GetFormIDString (rPlayer.GetTokenRef rObj))", True),
    _f6("token.setvalue", _o4("stimpak") + "rPlayer.SetTokenValue rObj 65.5\nSetFunctionValue (rPlayer.GetTokenValue rObj)"),
    _f6("token.value.next", _o4("stimpak") + "SetFunctionValue (rPlayer.GetTokenValue rObj)"),
    _f6("token.setref", _o4("stimpak", "pistol10") + "rPlayer.SetTokenRef rObj rOther\nSetFunctionValue (GetFormIDString (rPlayer.GetTokenRef rObj))", True),
    _f6("token.setboth", _o4("stimpak", "docbase") + "rPlayer.SetTokenValueAndRef rObj 7 rOther\n"
        'SetFunctionValue (sv_Construct "%g %z" (rPlayer.GetTokenValue rObj) (GetFormIDString (rPlayer.GetTokenRef rObj)))', True),
    _f6("token.notcarried", _o4("collarkey") + "rPlayer.SetTokenValue rObj 3\nSetFunctionValue (rPlayer.GetTokenValue rObj)"),
    # INI settings.
    _f6("ini.grids", 'SetFunctionValue (GetNumericINISetting "uGridsToLoad:General")'),
    _f6("ini.grids.alias", 'SetFunctionValue (GetNumericIniSetting "uGridsToLoad:General")'),
    _f6("ini.fov", 'SetFunctionValue (GetNumericINISetting "fDefaultFOV:Display")'),
    _f6("ini.case", 'SetFunctionValue (GetNumericINISetting "UGRIDSTOLOAD:GENERAL")'),
    _f6("ini.missing", 'SetFunctionValue (GetNumericINISetting "nvfNoSuchSetting:General")'),
    _f6("ini.nocategory", 'SetFunctionValue (GetNumericINISetting "uGridsToLoad")'),
    _f6("ini.string", 'SetFunctionValue (GetStringIniSetting "sLanguage:General")', True),
    _f6("ini.string.missing", 'SetFunctionValue (GetStringIniSetting "nvfNoSuchSetting:General")', True),
    _f6("ini.set.numeric", 'let fRes := GetNumericINISetting "fDefaultFOV:Display"\n'
        'let fRes := (SetNumericINISetting "fDefaultFOV:Display" 80) * 1000 + (GetNumericINISetting "fDefaultFOV:Display")\n'
        'SetNumericINISetting "fDefaultFOV:Display" 75\nSetFunctionValue fRes'),
    _f6("ini.set.numeric.missing", 'SetFunctionValue (SetNumericINISetting "nvfNoSuchSetting:General" 1)'),
    _f6("ini.fov.after", 'SetFunctionValue (GetNumericINISetting "fDefaultFOV:Display")'),
    # The original's value (ENGLISH, observed above) is put back as a literal: a string_var
    # given to SetStringIniSetting is taken as its name ("sText") [observed: first run].
    _f6("ini.set.string", 'SetStringIniSetting "sLanguage:General" "NVF"\n'
        'let fRes := (GetStringIniSetting "sLanguage:General") == "NVF"\nSetStringIniSetting "sLanguage:General" "ENGLISH"\nSetFunctionValue fRes'),
    _f6("ini.set.string.var", 'let sText := "NVF"\nSetStringIniSetting "sLanguage:General" sText\n'
        'let sText := GetStringIniSetting "sLanguage:General"\nSetStringIniSetting "sLanguage:General" "ENGLISH"\nSetFunctionValue sText', True),
    _f6("ini.string.after", 'SetFunctionValue (GetStringIniSetting "sLanguage:General")', True),
    _f6("gmst.setex", 'let sText := GetStringSetting "sTraitMenuTitleText"\nSetStringGameSettingEx "sTraitMenuTitleText|NVF title"\n'
        'let fRes := (GetStringSetting "sTraitMenuTitleText") == "NVF title"\nSetStringSetting "sTraitMenuTitleText" sText\nSetFunctionValue fRes'),
    # Event-handler lists around one registered lambda (removed at the end).
    _f6("events.count.before", 'SetFunctionValue (ar_Size (GetEventHandlers "OnActorEquip"))'),
    _f6("events.register", f'SetEventHandler "OnActorEquip" {_LAMBDA}\nSetFunctionValue (ar_Size (GetEventHandlers "OnActorEquip"))'),
    _f6("events.all.haskey", 'SetFunctionValue (ar_HasKey (GetEventHandlers) "OnActorEquip")'),
    _f6("events.all.type", 'SetFunctionValue (TypeOf (GetEventHandlers))', True),
    _f6("events.filtered.type", 'SetFunctionValue (TypeOf (GetEventHandlers "OnActorEquip"))', True),
    _f6("events.unknown.size", 'SetFunctionValue (ar_Size (GetEventHandlers "nvfNoSuchEvent"))'),
    _f6("events.count.after", 'let aList := GetEventHandlers "OnActorEquip"\nSetFunctionValue (ar_Size aList)'),
    # Commands and debug.
    _f6("opcode.activate", 'SetFunctionValue (GetCommandOpcode "Activate")', exp="4109", page="GetCommandOpcode (Activate 4109)"),
    _f6("opcode.case", 'SetFunctionValue (GetCommandOpcode "activate")'),
    _f6("opcode.nvse", 'SetFunctionValue (GetCommandOpcode "GetNVSEVersion")'),
    _f6("opcode.svlength", 'SetFunctionValue (GetCommandOpcode "Sv_Length")'),
    _f6("opcode.jip", 'SetFunctionValue (GetCommandOpcode "GetLNVersion")'),
    _f6("opcode.alias", 'SetFunctionValue (GetCommandOpcode "GetType")'),
    _f6("opcode.unknown", 'SetFunctionValue (GetCommandOpcode "nvfNoSuchCommand")'),
    _f6("debugmode", "SetFunctionValue (GetDebugMode)"),
    _f6("debugmode.0", "SetFunctionValue (GetDebugMode 0)"),
    _f6("printdebug", 'PrintDebug "nvf"\nSetFunctionValue 1'),
    _f6("assert.true", "Assert 1\nSetFunctionValue 1"),
    _f6("assert.false", "Assert 0\nSetFunctionValue 1"),
    _f6("ar_dumpid", "let aList := Ar_List 1 2\nAr_DumpID aList\nSetFunctionValue 1"),
    _f6("debugselection", "SetFunctionValue (GetFormIDString (GetDebugSelection))", True),
]

# Twenty-fourth set: the extender core's packages, paths and form flags, as it asked.
# Paths and flags are changed and put back; the package edits on Doc Mitchell run last
# and are not put back (RemoveAllPackages empties his list; session only, never saved).
# Forms from fnvtool esm inventory [observed]: DefaultStayAtEditorLocation 0200D5,
# WeapSniperRifle 004353.
FX.update(stayput="200D5", sniper="4353")
_H7 = ("ref rPlayer\nref rObj\nref rOther\nref rDoc\nref rItem\nfloat fRes\nstring_var sText\nBegin Function { }\n"
       + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]))


def _f7(pid, body, text=False, exp=None, page=""):
    return (pid, _H7 + body, text, exp, page)


SEVENTH_PROBES = [
    # Paths.
    _f7("scope.sniper", _o4("sniper") + "SetFunctionValue (GetScopeModelPath rObj)", True),
    _f7("scope.set", _o4("sniper") + 'let sText := GetScopeModelPath rObj\nSetScopeModelPath "nvf\\scope.nif" rObj\n'
        "let fRes := (GetScopeModelPath rObj) == \"nvf\\scope.nif\"\nSetScopeModelPath \"interface\\HUD\\mildotscope.NIF\" rObj\nSetFunctionValue fRes"),
    _f7("scope.restored", _o4("sniper") + "SetFunctionValue (GetScopeModelPath rObj)", True),
] + [_f7(f"bipedicon.leather.{c}", _o4("leather") + f"SetFunctionValue (GetBipedIconPath {c} rObj)", True) for c in range(4)] + [
    _f7("modelpathex", _o4("pistol10") + 'SetModelPathEx "nvf\\gun.nif" rObj\nlet sText := GetModelPath rObj\n'
        'SetModelPathEx "Weapons\\1HandPistol\\10mmPistol.NIF" rObj\nSetFunctionValue sText', True),
    _f7("modelpathex.restored", _o4("pistol10") + "SetFunctionValue (GetModelPath rObj)", True),
    _f7("bipedmodelpathex.format", _o4("leather") + 'SetBipedModelPathEx "b%g.nif" 2 1 rObj\nlet sText := GetBipedModelPath 1 rObj\n'
        'SetBipedModelPathEx "Armor\\LeatherArmor\\F\\OutfitF.NIF" 1 rObj\nSetFunctionValue sText', True),
    _f7("bipedmodelpathex.restored", _o4("leather") + "SetFunctionValue (GetBipedModelPath 1 rObj)", True),
    # Form flags.
    _f7("flags.pistol.low", _o4("pistol10") + "SetFunctionValue (GetFlagsLow rObj)"),
    _f7("flags.pistol.high", _o4("pistol10") + "SetFunctionValue (GetFlagsHigh rObj)"),
    _f7("flags.docref.low", "SetFunctionValue (GetFlagsLow rDoc)"),
    _f7("flags.docref.high", "SetFunctionValue (GetFlagsHigh rDoc)"),
    _f7("flags.docref.calling.low", "SetFunctionValue (rDoc.GetFlagsLow)"),
    _f7("flags.pistol.set", _o4("pistol10") + "let fRes := GetFlagsLow rObj\nSetFlagsLow 1024 rObj\nlet rItem := rObj\n"
        "let fRes := fRes * 10000 + (GetFlagsLow rObj)\nSetFlagsLow 9 rObj\nSetFunctionValue fRes"),
    _f7("flags.pistol.restored", _o4("pistol10") + "SetFunctionValue (GetFlagsLow rObj)"),
    # Hair and eyes flags.
    _f7("hairflags.doc", _o4("docbase") + "SetFunctionValue (GetHairFlags (GetHair rObj))"),
    _f7("eyesflags.doc", _o4("docbase") + "SetFunctionValue (GetEyesFlags (GetEyes rObj))"),
    _f7("hairflags.set", _o4("docbase") + "let rItem := GetHair rObj\nlet fRes := GetHairFlags rItem\nrPlayer.AuxVarSetFlt \"*nvfHF\" fRes\n"
        "SetHairFlags rItem 9\nSetFunctionValue (GetHairFlags rItem)"),
    _f7("hairflags.restore", _o4("docbase") + 'let rItem := GetHair rObj\nSetHairFlags rItem (rPlayer.AuxVarGetFlt "*nvfHF")\n'
        'rPlayer.AuxVarErase "*nvfHF"\nSetFunctionValue (GetHairFlags rItem)'),
    # Weapon-mod flags of the equipped weapon.
    _f7("modflags.add", _o4("pistol10", "ammo10") + "rPlayer.AddItem rObj 1 1\nrPlayer.AddItem rOther 20 1\nSetFunctionValue 1"),
    _f7("modflags.equip", _o4("pistol10") + "rPlayer.EquipItem rObj 0 1\nSetFunctionValue 1"),
    _f7("modflags.before", "SetFunctionValue (rPlayer.GetEquippedWeaponModFlags)"),
    _f7("modflags.set1", "rPlayer.SetEquippedWeaponModFlags 1\nSetFunctionValue (rPlayer.GetEquippedWeaponModFlags)"),
    _f7("modflags.set1.next", "SetFunctionValue (rPlayer.GetEquippedWeaponModFlags)"),
    _f7("modflags.cleanup", _o4("pistol10", "ammo10") + "rPlayer.SetEquippedWeaponModFlags 0\nrPlayer.UnequipItem rObj 0 1\n"
        "rPlayer.RemoveItem rObj 1 1\nrPlayer.RemoveItem rOther 20 1\nSetFunctionValue 1", exp="1", page="cleanup"),
    # Packages on Doc (last: not put back).
    _f7("pkg.nth0.before", "SetFunctionValue (GetFormIDString (rDoc.GetNthPackage 0))", True),
    _f7("pkg.count.before", "SetFunctionValue (rDoc.GetPackageCount)"),
    _f7("pkg.radius.0", "let rItem := rDoc.GetNthPackage 0\nSetFunctionValue (GetPackageLocationRadius rItem)"),
    _f7("pkg.radius.set", "let rItem := rDoc.GetNthPackage 0\nlet fRes := GetPackageLocationRadius rItem\nSetPackageLocationRadius rItem 321\n"
        "let fRes := fRes * 1000 + (GetPackageLocationRadius rItem)\nSetFunctionValue fRes"),
    _f7("pkg.locref.set", "let rItem := rDoc.GetNthPackage 0\nrDoc.SetPackageLocationReference rItem rPlayer\nSetFunctionValue 1"),
    _f7("pkg.target.set.calling", "let rItem := rDoc.GetNthPackage 0\nrDoc.SetPackageTargetReference rItem\nSetFunctionValue 1"),
    _f7("pkg.addat0", _o4("stayput") + "SetFunctionValue (rDoc.AddPackageAt rObj 0)"),
    _f7("pkg.nth0.after.add", "SetFunctionValue (GetFormIDString (rDoc.GetNthPackage 0))", True),
    _f7("pkg.count.after.add", "SetFunctionValue (rDoc.GetPackageCount)"),
    _f7("pkg.removeat0", "SetFunctionValue (GetFormIDString (rDoc.RemovePackageAt 0))", True),
    _f7("pkg.nth0.after.remove", "SetFunctionValue (GetFormIDString (rDoc.GetNthPackage 0))", True),
    _f7("pkg.addat.minus1", _o4("stayput") + "SetFunctionValue (rDoc.AddPackageAt rObj -1)"),
    _f7("pkg.count.after.minus1", "SetFunctionValue (rDoc.GetPackageCount)"),
    _f7("pkg.addat.past", _o4("stayput") + "SetFunctionValue (rDoc.AddPackageAt rObj 99)"),
    _f7("pkg.count.after.past", "SetFunctionValue (rDoc.GetPackageCount)"),
    _f7("pkg.removeall", "rDoc.RemoveAllPackages\nSetFunctionValue (GetFormIDString (rDoc.GetNthPackage 0))", True),
    _f7("pkg.count.after.removeall", "SetFunctionValue (rDoc.GetPackageCount)"),
]

# Twenty-fifth set: the extender core's last families, as it asked: animations and the
# generic form lists on Doc Mitchell, the con_ console functions, UI traits; and which
# references carry form flag 64 (Doc's reference reads 1097 in the game, 1033 + 64).
# Forms from fnvtool esm inventory [observed]: CaucasianOld 0042C3, EyebrowM 0B5BEA,
# BeardMustacheOld 07B47B. Game settings, INI values and head parts are put back; the
# package and animation edits on Doc are not (session only, never saved). Con_SCOF is
# left out: it writes a file into the owner's game folder (owner-review, J9).
FX.update(caucold="42C3", beardold="7B47B", list127="1613D5")
_H8 = ("ref rPlayer\nref rObj\nref rOther\nref rDoc\nref rBase\nref rF\nref rItem\nfloat fRes\nstring_var sText\n"
       "Begin Function { }\n" + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + _form("rBase", NF["doc"]))
_WALK8 = ("let fRes := 0\nlet sText := \"\"\nwhile IsFormValid rItem && fRes < 40\n"
          "  let sText += (GetFormIDString rItem) + (sv_Construct \":%g:%g \" (GetFlagsLow rItem) (GetType (rItem.GetBaseObject)))\n"
          "  let fRes += 1\n  let rItem := GetNextRef\nloop\nSetFunctionValue sText")


def _f8(pid, body, text=False, exp=None, page=""):
    return (pid, _H8 + body, text, exp, page)


def _id8(pid, expr):
    return _f8(pid, f"SetFunctionValue (GetFormIDString ({expr}))", True)


EIGHTH_PROBES = [
    # Form flags: single references, then walks of Doc's house (not loaded in the save:
    # persistent references only) and the player's own cell (loaded).
    _f8("flags.player", "SetFunctionValue (GetFlagsLow rPlayer)"),
    _f8("flags.docref", "SetFunctionValue (GetFlagsLow rDoc)"),
    _f8("flags.chet", _o4("chetref") + "SetFunctionValue (GetFlagsLow rObj)"),
    _f8("flags.saloondoor", _o4("saloondoor") + "SetFunctionValue (GetFlagsLow rObj)"),
    _f8("walk.doccell", _form("rF", CELLS["doc"]) + "let rItem := GetFirstRefInCell rF 0 0 0\n" + _WALK8, True),
    _f8("walk.here.actors", "let rItem := GetFirstRef 200 0 0\n" + _WALK8, True),
    _f8("walk.here", "let rItem := GetFirstRef 0 0 0\n" + _WALK8, True),
    # Animations (KFFZ): the base's list or the reference's.
    _f8("anim.base.0", "SetFunctionValue (GetNthAnimation 0 rBase)", True),
    _f8("anim.ref.0", "SetFunctionValue (rDoc.GetNthAnimation 0)", True),
    _f8("anim.add", 'SetFunctionValue (AddAnimation "nvf\\test.kf" rBase)'),
    _f8("anim.add.base.0", "SetFunctionValue (GetNthAnimation 0 rBase)", True),
    _f8("anim.add.ref.0", "SetFunctionValue (rDoc.GetNthAnimation 0)", True),
    _f8("anim.add2", 'SetFunctionValue (AddAnimation "nvf\\second.kf" rBase)'),
    _f8("anim.add2.1", "SetFunctionValue (GetNthAnimation 1 rBase)", True),
    _f8("anim.del", 'SetFunctionValue (DelAnimation "nvf\\test.kf" rBase)'),
    _f8("anim.del.0", "SetFunctionValue (GetNthAnimation 0 rBase)", True),
    _f8("anim.del.missing", 'SetFunctionValue (DelAnimation "nvf\\none.kf" rBase)'),
    _f8("anim.delall", "SetFunctionValue (DelAnimations rBase)"),
    _f8("anim.delall.0", "SetFunctionValue (GetNthAnimation 0 rBase)", True),
    # Generic form lists.
    _id8("generic.10.ref.0", "GenericGetForm 10 rDoc 0"),
    _id8("generic.10.base.0", "GenericGetForm 10 rBase 0"),
    _id8("generic.10.ref.1", "GenericGetForm 10 rDoc 1"),
    _id8("generic.5.ref.0", "GenericGetForm 5 rDoc 0"),
    _id8("generic.5.base.0", "GenericGetForm 5 rBase 0"),
    _f8("generic.race.0", _o4("caucold") + "SetFunctionValue (GetFormIDString (GenericGetForm 0 rObj 0))", True),
    _f8("generic.race.1", _o4("caucold") + "SetFunctionValue (GetFormIDString (GenericGetForm 1 rObj 0))", True),
    _f8("generic.list.13.0", _o4("list127") + "SetFunctionValue (GetFormIDString (GenericGetForm 13 rObj 0))", True),
    _f8("generic.list.13.1", _o4("list127") + "SetFunctionValue (GetFormIDString (GenericGetForm 13 rObj 1))", True),
    _f8("generic.10.replace", _o4("beardold") + "let rItem := GenericGetForm 10 rDoc 0\nrPlayer.AuxVarSetRef \"*nvfHP\" rItem\n"
        "SetFunctionValue (GetFormIDString (GenericReplaceForm 10 rDoc rObj 0))", True),
    _id8("generic.10.after.replace", "GenericGetForm 10 rDoc 0"),
    _f8("generic.10.restore", 'let rItem := rPlayer.AuxVarGetRef "*nvfHP"\nlet rOther := GenericReplaceForm 10 rDoc rItem 0\n'
        'rPlayer.AuxVarErase "*nvfHP"\nSetFunctionValue (GetFormIDString (GenericGetForm 10 rDoc 0))', True),
    _f8("generic.5.add", _o4("stayput") + "SetFunctionValue (GenericAddForm 5 rDoc rObj 0)"),
    _id8("generic.5.after.add", "rDoc.GetNthPackage 0"),
    _id8("generic.5.delete", "GenericDeleteForm 5 rDoc 0"),
    _id8("generic.5.after.delete", "rDoc.GetNthPackage 0"),
    _f8("generic.5.add.minus2", _o4("stayput") + "SetFunctionValue (GenericAddForm 5 rDoc rObj -2)"),
    _f8("generic.5.count.minus2", "SetFunctionValue (rDoc.GetPackageCount)"),
    _id8("generic.5.last.minus2", "rDoc.GetNthPackage ((rDoc.GetPackageCount) - 1)"),
    # Console functions.
    _f8("con.echo", "SetConsoleEcho 1\nSetFunctionValue (GetConsoleEcho)"),
    _f8("con.gmst.before", 'SetFunctionValue (GetNumericGameSetting "fJumpFallHeightMin")'),
    _f8("con.gmst.set", 'con_SetGameSetting "fJumpFallHeightMin" "100"\nSetFunctionValue (GetNumericGameSetting "fJumpFallHeightMin")'),
    _f8("con.gmst.restore", 'con_SetGameSetting "fJumpFallHeightMin" "600"\nSetFunctionValue (GetNumericGameSetting "fJumpFallHeightMin")'),
    _f8("con.scof.name", "SetFunctionValue (GetConsoleOutputFilename)", True),
    _f8("con.scof.has", "SetFunctionValue (HasConsoleOutputFilename)"),
    _f8("con.tfc.before", "SetFunctionValue (GetTFC)"),
    _f8("con.tfc.on", "Con_TFC\nSetFunctionValue (GetTFC)"),
    _f8("con.tfc.on.next", "SetFunctionValue (GetTFC)"),
    _f8("con.tfc.off", "Con_TFC\nSetFunctionValue (GetTFC)"),
    _f8("con.tfc.off.next", "SetFunctionValue (GetTFC)"),
    _f8("con.ini.get", 'SetFunctionValue (con_GetINISetting "fDefaultFOV:Display")'),
    _f8("con.ini.set", 'con_SetINISetting "fDefaultFOV:Display" "80"\nSetFunctionValue (GetNumericINISetting "fDefaultFOV:Display")'),
    _f8("con.ini.restore", 'con_SetINISetting "fDefaultFOV:Display" "75"\nSetFunctionValue (GetNumericINISetting "fDefaultFOV:Display")'),
    _f8("con.echo.off", "SetConsoleEcho 0\nSetFunctionValue (GetConsoleEcho)"),
    # UI traits.
    _f8("ui.x", 'SetFunctionValue (GetUIFloat "HUDMainMenu/_X")'),
    _f8("ui.visible", 'SetFunctionValue (GetUIFloat "HUDMainMenu/visible")'),
    _f8("ui.missing", 'SetFunctionValue (GetUIFloat "NVFNoMenu/_nvf")'),
    _f8("ui.user.set", 'SetUIFloat "HUDMainMenu/_nvfprobe" 5\nSetFunctionValue (GetUIFloat "HUDMainMenu/_nvfprobe")'),
    _f8("ui.user.next", 'SetFunctionValue (GetUIFloat "HUDMainMenu/_nvfprobe")'),
    _f8("ui.stringex", 'SetUIStringEx "HUDMainMenu/_nvfstr" "n%g" 3\nSetFunctionValue (GetUIString "HUDMainMenu/_nvfstr")', True),
]

# Twenty-sixth set (next batch N1): LU's leveled-list functions on an LVLI (Ammo10mm75
# 02935E, CapsFortuneFinder 0645D4), an LVLN (VarVHDNCRRanger9Veteran 17A2FE) and an LVLC
# (VEncTier4SupermutantMelee50 174435, adding VEncTier4SupermutantMinigun50 174434);
# forms from fnvtool esm inventory [observed]. CapsFortuneFinder is rebuilt to one entry
# (3 stimpaks, chance none 0) so AddItem of the list is certain, and the stimpaks are
# taken back. The list edits are not put back (LU's are not savebaked [documented:
# RemoveFormFromLeveledList]; session only, never saved). The versions the extender
# plugins report come first, under the names the wiki gives and the likely others.
FX.update(lvli="2935E", fortune="645D4", lvlc="174435", lvlc2="174434", stim="15169", capsf="F", docnpc="104C0C")
_HL = ("ref rPlayer\nref rLst\nref rObj\nref rOther\nfloat fRes\nBegin Function { }\n" + _form("rPlayer", NF["player"]))


def _lv(pid, lst, body, text=False):
    return (pid, _HL + _form("rLst", FX[lst]) + body, text, None, "")


def _lvid(pid, lst, expr):
    return _lv(pid, lst, f"SetFunctionValue (GetFormIDString ({expr}))", True)


_PLUGIN_NAMES = ["ShowOffNVSE Plugin", "ShowOff NVSE Plugin", "ShowOffNVSE", "kNVSE", "kNVSE Plugin",
                 "JIP LN NVSE", "JohnnyGuitarNVSE"]


def _lv_reads(tag, lst):
    return [
        _lv(f"{tag}.num", lst, "SetFunctionValue (GetNumLevItems rLst)"),
        _lv(f"{tag}.flags", lst, "SetFunctionValue (GetLeveledListFlags rLst)"),
        _lv(f"{tag}.chancenone", lst, "SetFunctionValue (GetChanceNone rLst)"),
        _lvid(f"{tag}.chancenoneglobal", lst, "GetChanceNoneGlobal rLst"),
        _lvid(f"{tag}.nth0", lst, "GetNthLevItem rLst 0"),
        _lv(f"{tag}.nth0.level", lst, "SetFunctionValue (GetNthLevItemLevel rLst 0)"),
        _lv(f"{tag}.nth0.count", lst, "SetFunctionValue (GetNthLevItemCount rLst 0)"),
        _lvid(f"{tag}.nth.past", lst, "GetNthLevItem rLst 99"),
        _lv(f"{tag}.nth.past.level", lst, "SetFunctionValue (GetNthLevItemLevel rLst 99)"),
        _lv(f"{tag}.index.level1", lst, "SetFunctionValue (GetLevItemIndexByLevel rLst 1)"),
        _lv(f"{tag}.index.level100", lst, "SetFunctionValue (GetLevItemIndexByLevel rLst 100)"),
    ]


LEVELED_PROBES = (
    [(f"plugin.version.{n}", f'SetFunctionValue (GetPluginVersion "{n}")', False, None, "") for n in _PLUGIN_NAMES]
    + [(f"plugin.installed.{n}", f'SetFunctionValue (IsPluginInstalled "{n}")', False, None, "") for n in _PLUGIN_NAMES]
    # Reads.
    + _lv_reads("lvli", "lvli") + _lv_reads("lvln", "lvln") + _lv_reads("lvlc", "lvlc")
    + [
        _lv("lvli.index.form0", "lvli", "let rObj := GetNthLevItem rLst 0\nSetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
        _lv("lvli.index.missing", "lvli", _form("rObj", FX["capsf"]) + "SetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
        _lv("lvli.hasdeep.nth0", "lvli", "let rObj := GetNthLevItem rLst 0\nSetFunctionValue (LeveledListHasFormDeep rLst rObj)"),
        _lv("lvli.hasdeep.missing", "lvli", _form("rObj", FX["capsf"]) + "SetFunctionValue (LeveledListHasFormDeep rLst rObj)"),
        # LVLI: add, read the new entry, replace, remove.
        _lv("lvli.add", "lvli", _form("rObj", FX["capsf"]) + "LeveledListAddForm rLst rObj 7 25 1.0\nSetFunctionValue (GetNumLevItems rLst)"),
        _lv("lvli.add.index", "lvli", _form("rObj", FX["capsf"]) + "SetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
        _lv("lvli.add.level", "lvli", _form("rObj", FX["capsf"]) + "SetFunctionValue (GetNthLevItemLevel rLst (GetLevItemIndexByForm rLst rObj))"),
        _lv("lvli.add.count", "lvli", _form("rObj", FX["capsf"]) + "SetFunctionValue (GetNthLevItemCount rLst (GetLevItemIndexByForm rLst rObj))"),
        _lv("lvli.add.index.level7", "lvli", "SetFunctionValue (GetLevItemIndexByLevel rLst 7)"),
        _lv("lvli.add.hasdeep", "lvli", _form("rObj", FX["capsf"]) + "SetFunctionValue (LeveledListHasFormDeep rLst rObj)"),
        _lv("lvli.add.twice", "lvli", _form("rObj", FX["capsf"]) + "LeveledListAddForm rLst rObj 3 5 1.0\nSetFunctionValue (GetNumLevItems rLst)"),
        _lv("lvli.add.twice.index", "lvli", _form("rObj", FX["capsf"]) + "SetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
        _lv("lvli.replace", "lvli", _form("rObj", FX["capsf"]) + _form("rOther", FX["stim"])
            + "LeveledListReplaceForm rLst rObj rOther\nSetFunctionValue (GetLevItemIndexByForm rLst rOther)"),
        _lv("lvli.replace.old", "lvli", _form("rObj", FX["capsf"]) + "SetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
        _lv("lvli.remove", "lvli", _form("rObj", FX["stim"]) + "SetFunctionValue (LeveledListRemoveForm rLst rObj)"),
        _lv("lvli.remove.num", "lvli", "SetFunctionValue (GetNumLevItems rLst)"),
        _lv("lvli.remove.again", "lvli", _form("rObj", FX["stim"]) + "SetFunctionValue (LeveledListRemoveForm rLst rObj)"),
        # LVLI: Nth setters, chance none, flags.
        _lv("lvli.setcount", "lvli", "SetNthLevItemCount rLst 0 42\nSetFunctionValue (GetNthLevItemCount rLst 0)"),
        _lv("lvli.setlevel", "lvli", "SetNthLevItemLevel rLst 0 9\nSetFunctionValue (GetNthLevItemLevel rLst 0)"),
        _lvid("lvli.setlevel.nth0", "lvli", "GetNthLevItem rLst 0"),
        _lv("lvli.setitem", "lvli", _form("rObj", FX["stim"]) + "SetNthLevItem rLst 0 rObj\nSetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
        _lv("lvli.chancenone.set", "lvli", "SetChanceNone rLst 35\nSetFunctionValue (GetChanceNone rLst)"),
        _lv("lvli.chancenone.big", "lvli", "SetChanceNone rLst 250\nSetFunctionValue (GetChanceNone rLst)"),
        _lv("lvli.flags.set", "lvli", "SetLeveledListFlags rLst 5\nSetFunctionValue (GetLeveledListFlags rLst)"),
        _lv("lvli.flags.set8", "lvli", "SetLeveledListFlags rLst 15\nSetFunctionValue (GetLeveledListFlags rLst)"),
        _lv("lvli.removenth", "lvli", "let fRes := GetNumLevItems rLst\nRemoveNthLevItem rLst 0\nSetFunctionValue (fRes * 100 + (GetNumLevItems rLst))"),
        _lv("lvli.removenth.past", "lvli", "let fRes := GetNumLevItems rLst\nRemoveNthLevItem rLst 99\nSetFunctionValue (fRes * 100 + (GetNumLevItems rLst))"),
        _lv("lvli.showoff.remove", "lvli", "let rObj := GetNthLevItem rLst 0\nSetFunctionValue (RemoveFormFromLeveledList rLst rObj)"),
        _lv("lvli.clear", "lvli", "LeveledListClear rLst\nSetFunctionValue (GetNumLevItems rLst)"),
        _lvid("lvli.clear.nth0", "lvli", "GetNthLevItem rLst 0"),
        # A list rebuilt to one entry, then added to the player.
        _lv("fortune.num.before", "fortune", "SetFunctionValue (GetNumLevItems rLst)"),
        _lv("fortune.rebuild", "fortune", _form("rObj", FX["stim"])
            + "LeveledListClear rLst\nSetChanceNone rLst 0\nSetLeveledListFlags rLst 0\nLeveledListAddForm rLst rObj 1 3 1.0\n"
            "SetFunctionValue (GetNumLevItems rLst)"),
        _lv("fortune.stim.before", "fortune", _form("rObj", FX["stim"]) + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
        _lv("fortune.additem", "fortune", "rPlayer.AddItem rLst 1 1\nSetFunctionValue 1"),
        _lv("fortune.stim.after", "fortune", _form("rObj", FX["stim"]) + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
        _lv("fortune.list.carried", "fortune", "SetFunctionValue (rPlayer.GetItemCount rLst)"),
        _lv("fortune.cleanup", "fortune", _form("rObj", FX["stim"]) + "rPlayer.RemoveItem rObj 3 1\nSetFunctionValue 1"),
        _lv("fortune.stim.cleaned", "fortune", _form("rObj", FX["stim"]) + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
        # LVLN and LVLC: add and remove.
        _lv("lvln.add", "lvln", _form("rObj", FX["docnpc"]) + "LeveledListAddForm rLst rObj 1 1 0\nSetFunctionValue (GetNumLevItems rLst)"),
        _lv("lvln.add.index", "lvln", _form("rObj", FX["docnpc"]) + "SetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
        _lv("lvln.add.count", "lvln", _form("rObj", FX["docnpc"]) + "SetFunctionValue (GetNthLevItemCount rLst (GetLevItemIndexByForm rLst rObj))"),
        _lv("lvln.remove", "lvln", _form("rObj", FX["docnpc"]) + "SetFunctionValue (LeveledListRemoveForm rLst rObj)"),
        _lv("lvln.remove.num", "lvln", "SetFunctionValue (GetNumLevItems rLst)"),
        _lv("lvlc.add.list", "lvlc", _form("rObj", FX["lvlc2"]) + "LeveledListAddForm rLst rObj 2 1 0\nSetFunctionValue (GetNumLevItems rLst)"),
        _lv("lvlc.add.list.index", "lvlc", _form("rObj", FX["lvlc2"]) + "SetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
        _lv("lvlc.hasdeep.sub", "lvlc", _form("rObj", FX["lvlc2"]) + "let rOther := GetNthLevItem rObj 0\nSetFunctionValue (LeveledListHasFormDeep rLst rOther)"),
        _lv("lvlc.flags.set", "lvlc", "let fRes := GetLeveledListFlags rLst\nSetLeveledListFlags rLst 4\n"
            "let fRes := fRes * 10 + (GetLeveledListFlags rLst)\nSetFunctionValue fRes"),
        _lv("lvlc.clear", "lvlc", "LeveledListClear rLst\nSetFunctionValue (GetNumLevItems rLst)"),
        # Not a leveled list.
        _lv("notlist.num", "fortune", _form("rObj", FX["stim"]) + "SetFunctionValue (GetNumLevItems rObj)"),
        _lv("notlist.flags", "fortune", _form("rObj", FX["stim"]) + "SetFunctionValue (GetLeveledListFlags rObj)"),
    ]
)

# Twenty-seventh set (next batch N14): which references carry form flag 64 at run time.
# Doc reads 1097 where his ACHR record carries 0x400 (fnvtool esm inventory) [observed].
# Persistent ACHRs of Goodsprings (Chet, Joe Cobb 0xC00, Sunny, Trudy, Ringo, Easy Pete),
# the player, a loaded ACRE near the airport (14DEE2), REFRs whose record carries 0x40
# (Hidden From Local Map in the schema), an NPC and a creature placed with PlaceAtMe
# (non-persistent), the ACRE and the placed NPC killed, and Doc with his cell loaded (the
# player moved there, last). Session only, never saved.
_FREFS = {"doc": "104C0F", "chet": "104C79", "joecobb": "104C68", "sunny": "104E85", "trudy": "104C6D",
          "ringo": "104C7D", "easypete": "104C80", "player": "14", "acre": "14DEE2",
          "hidden.16941d": "16941D", "hidden.1690f1": "1690F1", "hidden.e7514": "E7514", "hidden.101a47": "101A47"}
_HF = ("ref rPlayer\nref rF\nref rItem\nfloat fRes\nstring_var sText\nBegin Function { }\n" + _form("rPlayer", NF["player"]))
_FLAGS_OF = ('if IsFormValid rF\n  SetFunctionValue (sv_Construct "%g %g" (GetFlagsLow rF) (rF.GetDisabled))\n'
             'else\n  SetFunctionValue "none"\nendif')
_AUXF = 'let rF := rPlayer.AuxVarGetRef "*nvfFL"\n'


def _fl(pid, body):
    return (pid, _HF + body, True, None, "")


def _flref(pid, fid, after=""):
    return _fl(pid, _form("rF", fid) + after + _FLAGS_OF)


FLAG_PROBES = (
    [_flref(f"ref.{k}", fid) for k, fid in _FREFS.items()]
    + [
        _flref("ref.doc.persistent", "104C0F", "SetFunctionValue (rF.IsPersistent)\nreturn\n"),
    ]
    # Whether bit 64 follows having spoken with the player.
    + [_flref(f"talked.{k}", _FREFS[k], "SetFunctionValue (rF.GetTalkedToPC)\nreturn\n")
       for k in ("doc", "chet", "joecobb", "sunny", "trudy", "ringo", "easypete")]
    + [
        _flref("acre.dead.before", "14DEE2", "SetFunctionValue (rF.GetDead)\nreturn\n"),
        # Placed NPC (Doc's base) and placed creature (the ACRE's base).
        _fl("placed.npc", _form("rItem", NF["doc"]) + 'let rF := rPlayer.PlaceAtMe rItem 1\nrPlayer.AuxVarSetRef "*nvfFL" rF\n' + _FLAGS_OF),
        _fl("placed.npc.next", _AUXF + _FLAGS_OF),
        _fl("placed.npc.next2", _AUXF + _FLAGS_OF),
        _fl("placed.npc.kill", _AUXF + "rF.Kill\n" + _FLAGS_OF),
        _fl("placed.npc.dead", _AUXF + _FLAGS_OF),
        _fl("placed.npc.dead2", _AUXF + _FLAGS_OF),
        _fl("placed.npc.getdead", _AUXF + "SetFunctionValue (rF.GetDead)"),
        _fl("placed.creature", _form("rItem", "14DEE2") + 'let rItem := rItem.GetBaseObject\nlet rF := rPlayer.PlaceAtMe rItem 1\n'
            'rPlayer.AuxVarSetRef "*nvfFL" rF\n' + _FLAGS_OF),
        _fl("placed.creature.next", _AUXF + _FLAGS_OF),
        _fl("placed.cleanup", _AUXF + 'rF.Disable\nrF.MarkForDelete\nrPlayer.AuxVarErase "*nvfFL"\nSetFunctionValue "1"'),
        # The loaded ACRE killed.
        _flref("acre.kill", "14DEE2", "rF.Kill\n"),
        _flref("acre.dead", "14DEE2"),
        _flref("acre.dead2", "14DEE2"),
        _flref("acre.getdead", "14DEE2", "SetFunctionValue (rF.GetDead)\nreturn\n"),
        # Doc with his cell loaded.
        _flref("doc.moveto", "104C0F", "rPlayer.MoveTo rF\n"),
        _flref("doc.loaded", "104C0F"),
        _flref("doc.loaded2", "104C0F"),
        _flref("doc.loaded3", "104C0F"),
        _fl("doc.loaded.player.cell", "SetFunctionValue (GetFormIDString (rPlayer.GetParentCell))"),
        _fl("doc.loaded.walk", "let rItem := GetFirstRef 0 0 0\nlet fRes := 0\nlet sText := \"\"\nwhile IsFormValid rItem && fRes < 40\n"
            "  let sText += (GetFormIDString rItem) + (sv_Construct \":%g \" (GetFlagsLow rItem))\n"
            "  let fRes += 1\n  let rItem := GetNextRef\nloop\nSetFunctionValue sText"),
    ]
)

# Twenty-eighth set (next batch N3): INI files under Data\Config, read and written by
# LU (GetINIFloat, GetINIString, SetINIFloat), ShowOff (GetINI…OrDefault_Cached,
# GetINI…OrCreate_Cached) and MCM (Get/SetModINISetting). Before the run the harness
# writes INI_FILE_TEXT to Data\Config\NVFExtProbe\probe.ini and Data\Config\NVFExtProbe.ini;
# every write names an NVFExtProbe path; after the run the files are read back (what the
# game wrote) and removed (owner-review: probe files in the game's Data\Config).
INI_FILE_TEXT = "[General]\nfNum=3.5\niInt=7\nsText=hello world\n[Other]\nfNum=-2\n"
_PI = '"NVFExtProbe\\probe.ini"'
_NEW = '"NVFExtProbe\\created.ini"'


def _ini(pid, body, text=False):
    return (pid, "float fRes\nstring_var sText\nBegin Function { }\n" + body, text, None, "")


def _gf(pid, key, path=_PI):
    return _ini(pid, f'SetFunctionValue (GetINIFloat "{key}" {path})')


INI_FILE_PROBES = [
    # LU reads.
    _gf("lu.float", "General:fNum"),
    _gf("lu.float.int", "General:iInt"),
    _gf("lu.float.other", "Other:fNum"),
    _gf("lu.float.slash", "General/fNum"),
    _gf("lu.float.backslash", "General\\fNum"),
    _gf("lu.float.case", "general:FNUM"),
    _gf("lu.float.string", "General:sText"),
    _gf("lu.float.missingkey", "General:fNone"),
    _gf("lu.float.missingfile", "General:fNum", '"NVFExtProbe\\none.ini"'),
    _gf("lu.float.noext", "General:fNum", '"NVFExtProbe\\probe"'),
    _gf("lu.float.top", "General:fNum", '"NVFExtProbe.ini"'),
    _ini("lu.string", f'SetFunctionValue (GetINIString "General:sText" {_PI})', True),
    _ini("lu.string.num", f'SetFunctionValue (GetINIString "General:fNum" {_PI})', True),
    _ini("lu.string.missing", f'SetFunctionValue (GetINIString "General:sNone" {_PI})', True),
    # ShowOff cached reads.
    _ini("so.float", f'SetFunctionValue (GetINIFloatOrDefault_Cached "General:fNum" {_PI} 9)'),
    _ini("so.float.alias", f'SetFunctionValue (GetINIFloat_Cached "General:fNum" {_PI} 9)'),
    _ini("so.float.slash", f'SetFunctionValue (GetINIFloatOrDefault_Cached "General/fNum" {_PI} 9)'),
    _ini("so.float.missingkey", f'SetFunctionValue (GetINIFloatOrDefault_Cached "General:fNone" {_PI} 9)'),
    _ini("so.float.missingfile", 'SetFunctionValue (GetINIFloatOrDefault_Cached "General:fNum" "NVFExtProbe\\none.ini" 9)'),
    _ini("so.float.nodefault", f'SetFunctionValue (GetINIFloatOrDefault_Cached "General:fNone" {_PI})'),
    _ini("so.float.string", f'SetFunctionValue (GetINIFloatOrDefault_Cached "General:sText" {_PI} 9)'),
    _ini("so.string", f'SetFunctionValue (GetINIStringOrDefault_Cached "General:sText" {_PI} "dflt")', True),
    _ini("so.string.missing", f'SetFunctionValue (GetINIStringOrDefault_Cached "General:sNone" {_PI} "dflt")', True),
    _ini("so.string.num", f'SetFunctionValue (GetINIStringOrDefault_Cached "General:fNum" {_PI} "dflt")', True),
    # ShowOff creates (cache only: no SaveCachedIniFile).
    _ini("so.create.float", f'SetFunctionValue (GetINIFloatOrCreate_Cached "General:fMade" {_PI} 4.5 "made by a probe")'),
    _ini("so.create.float.read", f'SetFunctionValue (GetINIFloatOrDefault_Cached "General:fMade" {_PI} 9)'),
    _ini("so.create.float.lu", f'SetFunctionValue (GetINIFloat "General:fMade" {_PI})'),
    _ini("so.create.float.existing", f'SetFunctionValue (GetINIFloatOrCreate_Cached "General:fNum" {_PI} 4.5)'),
    _ini("so.create.string", f'SetFunctionValue (GetINIStringOrCreate_Cached "General:sMade" {_PI} "new" "")', True),
    _ini("so.create.string.read", f'SetFunctionValue (GetINIStringOrDefault_Cached "General:sMade" {_PI} "dflt")', True),
    # LU writes: an existing file, then a new one.
    _ini("lu.set", f'SetINIFloat "General:fSet" 2.25 {_PI}\nSetFunctionValue (GetINIFloat "General:fSet" {_PI})'),
    _ini("lu.set.existing", f'SetINIFloat "Other:fNum" 6 {_PI}\nSetFunctionValue (GetINIFloat "Other:fNum" {_PI})'),
    _ini("lu.set.cached", f'SetFunctionValue (GetINIFloatOrDefault_Cached "Other:fNum" {_PI} 9)'),
    _ini("lu.set.newfile", f'SetINIFloat "New:fVal" 1.5 {_NEW}\nSetFunctionValue (GetINIFloat "New:fVal" {_NEW})'),
    _ini("lu.set.newfile.cached", f'SetFunctionValue (GetINIFloatOrDefault_Cached "New:fVal" {_NEW} 9)'),
    # MCM.
    _ini("mcm.get", 'SetFunctionValue (GetModINISetting "NVFExtProbe/General/fNum")'),
    _ini("mcm.get.missing", 'SetFunctionValue (GetModINISetting "NVFExtProbe/General/fNone")'),
    _ini("mcm.set", 'SetModINISetting "NVFExtProbe/General/fMcm" 5\nSetFunctionValue (GetModINISetting "NVFExtProbe/General/fMcm")'),
    _ini("mcm.set.lu", 'SetFunctionValue (GetINIFloat "General:fMcm" "NVFExtProbe.ini")'),
]

# Twenty-ninth set (next batch N7): JIP and LU actor and game odds. Forms from fnvtool
# esm inventory [observed]: Water2 161902, Rad1 0335A2, MaleAdult11 173561, Toughness
# 031DE0, Chet 104C79, the dead ACRE near the airport 14DEE2, Doc's house 103DF9.
# Settings changed are put back where a getter can read the old value; the player is
# moved into Doc's house last. Session only, never saved. Left out: Disable/
# EnableNavMeshAlt (navmesh edits) and GetProjectileRefImpactRef (needs a live
# projectile inside an impact handler).
FX.update(water2="161902", rad1="335A2", maleadult11="173561", toughness="31DE0")
_H9 = ("ref rPlayer\nref rDoc\nref rBase\nref rObj\nref rOther\nref rF\nfloat fRes\nstring_var sText\nBegin Function { }\n"
       + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + _form("rBase", NF["doc"]))


def _n7(pid, body, text=False):
    return (pid, _H9 + body, text, None, "")


def _n7v(pid, expr, text=False):
    return _n7(pid, f"SetFunctionValue ({expr})", text)


def _n7id(pid, expr, pre=""):
    return _n7(pid, pre + f"SetFunctionValue (GetFormIDString ({expr}))", True)


_POS = 'SetFunctionValue (sv_Construct "%.0f %.0f %.0f" (rObj.GetPos X) (rObj.GetPos Y) (rObj.GetPos Z))'

N7_PROBES = [
    # Keywords.
    _n7v("kw.none", 'HasKeyword rBase "NVFKey"'),
    _n7("kw.assign", 'AssignKeyword rBase "NVFKey"\nSetFunctionValue (HasKeyword rBase "NVFKey")'),
    _n7v("kw.ref", 'HasKeyword rDoc "NVFKey"'),
    _n7v("kw.case", 'HasKeyword rBase "nvfkey"'),
    _n7v("kw.other", 'HasKeyword rBase "NVFOther"'),
    _n7(f"kw.list", _o4("list127") + 'AssignKeyword rObj "NVFListKey"\nlet rOther := ListGetNthForm rObj 0\n'
        'SetFunctionValue (HasKeyword rOther "NVFListKey")'),
    _n7(f"kw.list.self", _o4("list127") + 'SetFunctionValue (HasKeyword rObj "NVFListKey")'),
    # Hardcore stages and tracking.
    _n7(f"hc.water2", _o4("water2") + "SetFunctionValue (GetHardcoreStageThreshold rObj)"),
    _n7(f"hc.rad1", _o4("rad1") + "SetFunctionValue (GetHardcoreStageThreshold rObj)"),
    _n7(f"hc.water2.set", _o4("water2") + "let fRes := GetHardcoreStageThreshold rObj\nrPlayer.AuxVarSetFlt \"*nvfHC\" fRes\n"
        "SetHardcoreStageThreshold rObj 321\nSetFunctionValue (GetHardcoreStageThreshold rObj)"),
    _n7(f"hc.water2.restore", _o4("water2") + 'SetHardcoreStageThreshold rObj (rPlayer.AuxVarGetFlt "*nvfHC")\n'
        'rPlayer.AuxVarErase "*nvfHC"\nSetFunctionValue (GetHardcoreStageThreshold rObj)'),
    _n7(f"hc.notstage", _o4("pistol10") + "SetFunctionValue (GetHardcoreStageThreshold rObj)"),
    _n7("hc.tracking.off", "SetFunctionValue (ToggleHardcoreTracking 0)"),
    _n7("hc.tracking.on", "SetFunctionValue (ToggleHardcoreTracking 1)"),
    # Voice type.
    _n7id("voice.ref", "rDoc.GetActorVoiceType"),
    _n7id("voice.base", "GetActorVoiceType rBase"),
    _n7("voice.set", _o4("maleadult11") + "let rOther := rDoc.GetActorVoiceType\nrPlayer.AuxVarSetRef \"*nvfVT\" rOther\n"
        "rDoc.SetActorVoiceType rObj\nSetFunctionValue (GetFormIDString (rDoc.GetActorVoiceType))", True),
    _n7id("voice.set.base", "GetActorVoiceType rBase"),
    _n7("voice.restore", 'let rOther := rPlayer.AuxVarGetRef "*nvfVT"\nrDoc.SetActorVoiceType rOther\nrPlayer.AuxVarErase "*nvfVT"\n'
        "SetFunctionValue (GetFormIDString (rDoc.GetActorVoiceType))", True),
    # Base karma.
    _n7v("karma.base", "GetBaseKarma rBase"),
    _n7v("karma.ref", "rDoc.GetBaseKarma"),
    _n7("karma.save", 'let fRes := GetBaseKarma rBase\nrPlayer.AuxVarSetFlt "*nvfKA" fRes\nSetFunctionValue (rPlayer.AuxVarGetFlt "*nvfKA")'),
    _n7("karma.set", "SetBaseKarma 37.5 rBase\nSetFunctionValue (GetBaseKarma rBase)"),
    _n7("karma.set.int", "SetBaseKarma 37 rBase\nSetFunctionValue (GetBaseKarma rBase)"),
    _n7("karma.set.calling", "rBase.SetBaseKarma 21\nSetFunctionValue (GetBaseKarma rBase)"),
    _n7("karma.set.ref", "rDoc.SetBaseKarma -12\nSetFunctionValue (GetBaseKarma rBase)"),
    _n7("karma.restore", 'SetBaseKarma (rPlayer.AuxVarGetFlt "*nvfKA") rBase\nrPlayer.AuxVarErase "*nvfKA"\nSetFunctionValue (GetBaseKarma rBase)'),
    # Perk entries.
    _n7(f"perk.v1.0", _o4("toughness") + "SetFunctionValue (GetNthPerkEntryValue1 rObj 0)"),
    _n7(f"perk.v1.past", _o4("toughness") + "SetFunctionValue (GetNthPerkEntryValue1 rObj 99)"),
    _n7(f"perk.v1.set", _o4("toughness") + 'let fRes := GetNthPerkEntryValue1 rObj 0\nrPlayer.AuxVarSetFlt "*nvfPK" fRes\n'
        "SetNthPerkEntryValue1 rObj 0 9\nSetFunctionValue (GetNthPerkEntryValue1 rObj 0)"),
    _n7(f"perk.v1.restore", _o4("toughness") + 'SetNthPerkEntryValue1 rObj 0 (rPlayer.AuxVarGetFlt "*nvfPK")\nrPlayer.AuxVarErase "*nvfPK"\n'
        "SetFunctionValue (GetNthPerkEntryValue1 rObj 0)"),
] + [_n7v(f"perkmod.{f}", f"rPlayer.GetPerkModifier {f} 100") for f in (0, 1, 2, 10, 37)] + [
    # Names.
    _n7v("name.before", "GetName rBase", True),
    _n7("name.set", 'LNSetName "NVF Doc" rBase\nSetFunctionValue (GetName rBase)', True),
    _n7v("name.set.ref", "rDoc.GetName", True),
    _n7("name.set.byref", 'LNSetName "NVF Doc Ref" rDoc\nSetFunctionValue (GetName rBase)', True),
    _n7("name.restore", 'LNSetName "Doc Mitchell" rBase\nSetFunctionValue (GetName rBase)', True),
    # Message.
    _n7("n7.messageexalt", 'MessageExAlt 2 "NVF probe message"\nSetFunctionValue 1'),
    # Water, air.
    _n7v("inwater.player", "rPlayer.IsInWater"),
    _n7v("n7.inair.player", "rPlayer.IsInAir"),
    _n7v("inwater.doc", "rDoc.IsInWater"),
    _n7v("inair.doc", "rDoc.IsInAir"),
    # Killer.
    _n7(f"killer.acre", _form("rF", "14DEE2") + "SetFunctionValue (GetFormIDString (rF.GetKiller))", True),
    _n7id("killer.alive", "rDoc.GetKiller"),
    _n7("killer.placed", _form("rObj", "14DEE2") + 'let rObj := rObj.GetBaseObject\nlet rF := rPlayer.PlaceAtMe rObj 1\n'
        'rPlayer.AuxVarSetRef "*nvfKL" rF\nrF.Kill rPlayer\nSetFunctionValue 1'),
    _n7("killer.placed.next", 'let rF := rPlayer.AuxVarGetRef "*nvfKL"\nSetFunctionValue (sv_Construct "%g " (rF.GetDead) + GetFormIDString (rF.GetKiller))', True),
    _n7("killer.placed.nokiller", _form("rObj", "14DEE2") + 'let rObj := rObj.GetBaseObject\nlet rF := rPlayer.PlaceAtMe rObj 1\n'
        'rPlayer.AuxVarSetRef "*nvfKL2" rF\nrF.Kill\nSetFunctionValue 1'),
    _n7("killer.placed.nokiller.next", 'let rF := rPlayer.AuxVarGetRef "*nvfKL2"\nSetFunctionValue (GetFormIDString (rF.GetKiller))', True),
    _n7("killer.cleanup", 'let rF := rPlayer.AuxVarGetRef "*nvfKL"\nrF.Disable\nrF.MarkForDelete\nlet rF := rPlayer.AuxVarGetRef "*nvfKL2"\n'
        'rF.Disable\nrF.MarkForDelete\nrPlayer.AuxVarErase "*nvfKL"\nrPlayer.AuxVarErase "*nvfKL2"\nSetFunctionValue 1'),
    # Speed multiplier.
    _n7v("speed.before", 'rDoc.GetActorValue "SpeedMult"'),
    _n7("speed.set", 'rDoc.SetSpeedMult 150\nSetFunctionValue (rDoc.GetActorValue "SpeedMult")'),
    _n7v("speed.set.next", 'rDoc.GetActorValue "SpeedMult"'),
    _n7("speed.restore", 'rDoc.SetSpeedMult 100\nSetFunctionValue (rDoc.GetActorValue "SpeedMult")'),
    # Linked reference.
    _n7id("linked.before", "rDoc.GetLinkedRef"),
    _n7id("linked.set", "rDoc.GetLinkedRef", "rDoc.SetLinkedReference rPlayer\n"),
    _n7id("linked.reset", "rDoc.GetLinkedRef", "rDoc.SetLinkedReference\n"),
    # UnequipItemAlt.
    _n7("unequipalt.add", _o4("pistol10") + "rPlayer.AddItem rObj 1 1\nSetFunctionValue 1"),
    _n7("unequipalt.equip", _o4("pistol10") + "rPlayer.EquipItem rObj 0 1\nSetFunctionValue 1"),
    _n7v("unequipalt.equipped", "rPlayer.GetEquipped " + "(GetFormFromMod \"FalloutNV.esm\" \"434F\")"),
    _n7("unequipalt", _o4("pistol10") + "rPlayer.UnequipItemAlt rObj 0 1\nSetFunctionValue (rPlayer.GetEquipped rObj)"),
    _n7("unequipalt.next", _o4("pistol10") + "SetFunctionValue (rPlayer.GetEquipped rObj)"),
    _n7("unequipalt.cleanup", _o4("pistol10") + "rPlayer.RemoveItem rObj 1 1\nSetFunctionValue 1"),
    # Reference flags (LU).
] + [_n7v(f"refflag.doc.{f}", f"rDoc.GetReferenceFlag {f}") for f in (1, 64, 1024, 2048, 524288)] + [
    _n7(f"refflag.chet.64", _o4("chetref") + "SetFunctionValue (rObj.GetReferenceFlag 64)"),
    _n7(f"refflag.door.set", _o4("saloondoor") + "rObj.SetReferenceFlag 128 1\nSetFunctionValue (GetFlagsLow rObj)"),
    _n7(f"refflag.door.get", _o4("saloondoor") + "SetFunctionValue (rObj.GetReferenceFlag 128)"),
    _n7(f"refflag.door.clear", _o4("saloondoor") + "rObj.SetReferenceFlag 128 0\nSetFunctionValue (GetFlagsLow rObj)"),
    # CastImmediate: a stimpak on the damaged player.
    _n7("cast.damage", 'rPlayer.DamageActorValue "Health" 40\nSetFunctionValue (rPlayer.GetActorValue "Health")'),
    _n7("cast.stimpak", _o4("stimpak") + 'rPlayer.CastImmediate rObj\nSetFunctionValue (rPlayer.GetActorValue "Health")'),
    _n7v("cast.next", 'rPlayer.GetActorValue "Health"'),
    _n7v("cast.next2", 'rPlayer.GetActorValue "Health"'),
    _n7("cast.heal", 'rPlayer.RestoreActorValue "Health" 1000\nSetFunctionValue (rPlayer.GetActorValue "Health")'),
    # MoveToEditorPosition on Chet.
    _n7("editorpos.before", _o4("chetref") + _POS, True),
    _n7("editorpos.moved", _o4("chetref") + "rObj.MoveTo rPlayer\nSetFunctionValue 1"),
    _n7("editorpos.moved.pos", _o4("chetref") + 'SetFunctionValue (GetFormIDString (rObj.GetParentCell))', True),
    _n7("editorpos.reset", _o4("chetref") + "SetFunctionValue (rObj.MoveToEditorPosition 1)"),
    _n7("editorpos.after", _o4("chetref") + _POS, True),
    _n7("editorpos.after.cell", _o4("chetref") + 'SetFunctionValue (GetFormIDString (rObj.GetParentCell))', True),
    _n7v("editorpos.player", "rPlayer.MoveToEditorPosition 0"),
    # MoveToCell: the player into Doc's house (last).
    _n7("movetocell", _form("rF", CELLS["doc"]) + "SetFunctionValue (rPlayer.MoveToCell rF 0 0 0)"),
    _n7id("movetocell.cell", "rPlayer.GetParentCell"),
    _n7id("movetocell.cell2", "rPlayer.GetParentCell"),
]

# Thirtieth set: the player-state getters the N10 sweep ranks first (GetWaterImmersionPerc,
# IsInKillCam, GetPCUsingScope/IronSights, GetAutoMove, GetActorVelocity, IsIdlePlayingEx)
# and LU's controller functions (next batch N8), standing still at the airport with no
# gamepad connected. Idle NVPHMCHouseIdle 176EE1 from fnvtool esm inventory [observed].
# DisableButton is undone with EnableButton.
FX.update(idlehouse="176EE1")


def _ps(pid, body, text=False):
    return (pid, _H9 + body, text, None, "")


def _psv(pid, expr, text=False):
    return _ps(pid, f"SetFunctionValue ({expr})", text)


PLAYER_STATE_PROBES = (
    [_psv("ps.water.player", "rPlayer.GetWaterImmersionPerc"),
     _psv("ps.water.doc", "rDoc.GetWaterImmersionPerc"),
     _psv("ps.killcam", "IsInKillCam"),
     _psv("ps.scope", "GetPCUsingScope"),
     _psv("ps.ironsights", "GetPCUsingIronSights"),
     _psv("ps.automove", "GetAutoMove"),
     _psv("ps.swimming", "IsPlayerSwimming"),
     _psv("ps.chargen", "IsInCharGen")]
    + [_psv(f"ps.velocity.player.{a or 'all'}", f"rPlayer.GetActorVelocity {a}".rstrip()) for a in ("", "X", "Y", "Z")]
    + [_psv("ps.velocity.doc", "rDoc.GetActorVelocity"),
       _ps("ps.idle.player", _o4("idlehouse") + "SetFunctionValue (rPlayer.IsIdlePlayingEx rObj)"),
       _ps("ps.idle.doc", _o4("idlehouse") + "SetFunctionValue (rDoc.IsIdlePlayingEx rObj)"),
       # Controller (no pad connected).
       _psv("pad.controller", "GetController"),
       _psv("pad.pressed", "GetPressedButtons"),
       _psv("pad.lt", "GetLeftTrigger"),
       _psv("pad.rt", "GetRightTrigger"),
       _psv("pad.lsx", "GetLeftStickX"),
       _psv("pad.lsy", "GetLeftStickY"),
       _psv("pad.rsx", "GetRightStickX"),
       _psv("pad.rsy", "GetRightStickY"),
       _psv("pad.button.a", "IsButtonPressed 4096"),
       _psv("pad.disabled.before", "IsButtonDisabled 4096"),
       _ps("pad.disable", "DisableButton 4096\nSetFunctionValue (IsButtonDisabled 4096)"),
       _psv("pad.disable.next", "IsButtonDisabled 4096"),
       _psv("pad.disable.other", "IsButtonDisabled 8192"),
       _ps("pad.disable.two", "DisableButton 12288\nSetFunctionValue ((IsButtonDisabled 4096) * 10 + (IsButtonDisabled 8192))"),
       _ps("pad.enable.one", "EnableButton 8192\nSetFunctionValue ((IsButtonDisabled 4096) * 10 + (IsButtonDisabled 8192))"),
       _ps("pad.enable", "EnableButton 4096\nSetFunctionValue (IsButtonDisabled 4096)")]
)

# Thirty-first set (next batch N2): MCM's script functions, in a load order of its own
# (the usual eleven plugins plus "The Mod Configuration Menu.esp" last; every other set
# keeps the usual order). In play no menu is open, so StartMenu (the pause menu, where
# MCM's tiles live) is not loaded: these rows are what a mod's script sees outside the
# menu. MCM.ini (Data\Config, made by MCM.dll) is read through GetModINISetting.
MCM_LOAD_ORDER_EXTRA = "The Mod Configuration Menu.esp"


def _mc(pid, body, text=False):
    return (pid, "float fRes\nstring_var sText\nBegin Function { }\n" + body, text, None, "")


MCM_PROBES = [
    _mc("mcm.loaded", 'SetFunctionValue (IsModLoaded "The Mod Configuration Menu.esp")'),
    _mc("mcm.plugin.version", 'SetFunctionValue (GetPluginVersion "MCM Extensions")'),
    _mc("mcm.ini.maxwidth", 'SetFunctionValue (GetModINISetting "MCM/List/MaxWidth")'),
    _mc("mcm.ini.initial", 'SetFunctionValue (GetModINISetting "MCM/Messages/Initial")'),
    _mc("mcm.ini.missing", 'SetFunctionValue (GetModINISetting "MCM/List/NVFNone")'),
    _mc("mcm.startmenu.visible", 'SetFunctionValue (GetUIFloat "StartMenu/visible")'),
    _mc("mcm.tile.visible", 'SetFunctionValue (GetUIFloat "StartMenu/MCM/visible")'),
    _mc("mcm.getfloat.1.1", 'SetFunctionValue (GetMCMFloat 1 1 "_enable")'),
    _mc("mcm.getfloat.0.0", 'SetFunctionValue (GetMCMFloat 0 0 "visible")'),
    _mc("mcm.setfloat", 'SetMCMFloat 1 1 "_nvfprobe" 5\nSetFunctionValue (GetMCMFloat 1 1 "_nvfprobe")'),
    _mc("mcm.setfloat.ui", 'SetFunctionValue (GetUIFloat "StartMenu/MCM/*:1/*:1/_nvfprobe")'),
    _mc("mcm.setstring", 'SetMCMString 1 1 "_title" "NVF probe"\nSetFunctionValue 1'),
    _mc("mcm.menumode", "SetFunctionValue (MenuMode 1013)"),
]

# Thirty-second set (next batch N9): JohnnyGuitar, ShowOff and ITR leftovers. ITR's
# plugin is not installed in this game, so its rows show what a script gets without it.
# Left out: SendStealingAlarm (karma loss and hostile owners). Extra misc stats and aux
# timers live on the player for the session only, never saved.
def _n9(pid, body, text=False):
    return (pid, _H9.replace("string_var sText\n", "string_var sText\narray_var aF\n") + body, text, None, "")


def _n9v(pid, expr, text=False):
    return _n9(pid, f"SetFunctionValue ({expr})", text)


_BASEFLAGS = ("let fRes := 0\nlet sText := \"\"\nwhile fRes < 32\n  if ActorHasBaseFlag fRes rObj\n"
              "    let sText += sv_Construct \"%g \" fRes\n  endif\n  let fRes += 1\nloop\nSetFunctionValue sText")

N9_PROBES = [
    _n9v("jg.pipboy", "GetPipBoyMode"),
    _n9v("jg.lerp", "Lerp 0 10 0.25"),
    _n9v("jg.lerp.over", "Lerp 0 10 1.5"),
    _n9v("jg.lerp.under", "Lerp 0 10 -0.5"),
    _n9v("jg.lerp.desc", "Lerp 10 0 0.25"),
    _n9v("jg.miscstat.none", 'GetExtraMiscStat "NVFStat"'),
    _n9("jg.miscstat.mod", 'ModExtraMiscStat "NVFStat" 3\nSetFunctionValue (GetExtraMiscStat "NVFStat")'),
    _n9("jg.miscstat.mod2", 'ModExtraMiscStat "NVFStat" -1\nSetFunctionValue (GetExtraMiscStat "NVFStat")'),
    _n9v("jg.miscstat.case", 'GetExtraMiscStat "nvfstat"'),
    _n9("jg.camshake", "SetCameraShakeNoHUDShudder 0.5 0.1\nSetFunctionValue 1"),
    _n9(f"jg.armoralt.clear", _o4("leather") + "SetFunctionValue (ClearArmorAltTexture rObj 1 -1)"),
    _n9(f"jg.armoralt.clear.idx", _o4("leather") + "SetFunctionValue (ClearArmorAltTexture rObj 1 0)"),
    _n9(f"jg.weap1st.get", _o4("pistol10") + "SetFunctionValue (GetFormIDString (GetWeapon1stPersonModel rObj 0))", True),
    _n9(f"jg.weap1st.same", _o4("pistol10") + "let rOther := GetWeapon1stPersonModel rObj 0\nSetWeapon1stPersonModel rObj 0 rOther\n"
        "SetFunctionValue (GetFormIDString (GetWeapon1stPersonModel rObj 0))", True),
    # ShowOff: actor base flags, as the list of set flag indices 0-31.
    _n9("so.baseflags.doc", "let rObj := rBase\n" + _BASEFLAGS, True),
    _n9("so.baseflags.player", _form("rObj", "7") + _BASEFLAGS, True),
    _n9("so.baseflags.acre", _form("rObj", "14DEE2") + "let rObj := rObj.GetBaseObject\n" + _BASEFLAGS, True),
    _n9v("so.baseflag.ref", "rDoc.ActorHasBaseFlag 1"),
    # ShowOff aux timers (owner: the player when none is given).
    _n9v("so.timer.left.none", 'AuxTimerTimeLeft "NVFT"'),
    _n9v("so.timer.start", 'AuxTimerStart "NVFT" 5'),
    _n9v("so.timer.left", 'Ceil (AuxTimerTimeLeft "NVFT")'),
    _n9v("so.timer.elapsed", 'Ceil (AuxTimerTimeElapsed "NVFT" 0)'),
    _n9v("so.timer.pause", 'AuxTimerPaused "NVFT" 1'),
    _n9v("so.timer.paused", 'AuxTimerPaused "NVFT" 0'),
    _n9v("so.timer.restart", 'AuxTimerStart "NVFT" 7'),
    _n9v("so.timer.left.restart", 'Ceil (AuxTimerTimeLeft "NVFT")'),
    _n9v("so.timer.stop", 'AuxTimerStop "NVFT" 0'),
    _n9v("so.timer.left.stopped", 'AuxTimerTimeLeft "NVFT"'),
    _n9v("so.timer.stop.again", 'AuxTimerStop "NVFT" 0'),
    _n9v("so.timer.doc.start", 'rDoc.AuxTimerStart "NVFT" 3'),
    _n9v("so.timer.doc.left.player", 'AuxTimerTimeLeft "NVFT"'),
    _n9v("so.timer.doc.left", 'Ceil (rDoc.AuxTimerTimeLeft "NVFT")'),
    _n9v("so.timer.doc.stop", 'rDoc.AuxTimerStop "NVFT" 0'),
    _n9v("so.filecache.clear", 'ClearFileCacheShowOff "NVFExtProbe\\probe.ini" 1'),
    # Files (read-only, relative to Data).
    _n9v("file.esp", 'FileExists "NVFExtProbe.esp"'),
    _n9v("file.bsa", 'FileExists "meshes\\weapons\\1handpistol\\10mmpistol.nif"'),
    _n9v("file.none", 'FileExists "nvf\\none.nif"'),
    _n9v("file.folder", 'FileExists "NVSE" 1'),
    _n9v("file.folder.asfile", 'FileExists "NVSE"'),
    _n9("files.plugins", 'let aF := GetFilesInFolder "NVSE\\Plugins" "*.dll"\n'
        'SetFunctionValue (sv_Construct "%g " (ar_Size aF) + aF[0])', True),
    _n9("files.plugins.all", 'let aF := GetFilesInFolder "NVSE\\Plugins"\nSetFunctionValue (ar_Size aF)'),
    _n9("files.none", 'let aF := GetFilesInFolder "NVFNoFolder"\nSetFunctionValue (ar_Size aF)'),
    # ITR (not installed).
    _n9v("itr.crouch", "rDoc.ForceCrouch 1"),
    _n9v("itr.target", "rDoc.ForceCombatTarget rPlayer"),
]

# Thirty-third set (lu-leveled follow-up): which entries LeveledListAddForm takes, and
# whether an edited list reaches AddItem, a newly placed container and ResetInventory.
# Forms from fnvtool esm inventory [observed]: Ammo556mm 004240, Caps001 00000F,
# VStreetCrBrahmin2 (CREA) 17B587, VarVHDNCRRanger8Veteran (LVLN) 17A2FD,
# VLootAmmoBoxGen30 (CONT) 167815 holding 02FE5D. Lists edited for the session only.
FX.update(ammo556="4240", brahmin="17B587", lvln2="17A2FD", ammobox="167815", boxlist="2FE5D")


def _l2(pid, lst, body, text=False):
    head = _HL.replace("float fRes\n", "float fRes\nstring_var sText\n")
    return (pid, head + _form("rLst", FX[lst]) + body, text, None, "")


_NUM = "SetFunctionValue (GetNumLevItems rLst)"

LEVELED2_PROBES = [
    # AddItem of an unedited list (does it give anything at all?).
    _l2("additem.plain.before", "fortune", "SetFunctionValue (rPlayer.GetNumItems)"),
    _l2("additem.plain", "fortune", "rPlayer.AddItem rLst 1 1\nSetFunctionValue 1"),
    _l2("additem.plain.after", "fortune", "SetFunctionValue (rPlayer.GetNumItems)"),
    _l2("additem.ammo10.before", "lvli", _form("rObj", "4241") + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
    _l2("additem.ammo10.cn0", "lvli", "SetChanceNone rLst 0\nrPlayer.AddItem rLst 1 1\nSetFunctionValue 1"),
    _l2("additem.ammo10.after", "lvli", _form("rObj", "4241") + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
    # What an unedited LVLI takes.
    _l2("type.lvli.same", "lvli", _form("rObj", FX["ammo556"]) + "LeveledListAddForm rLst rObj 1 1 1.0\n" + _NUM),
    _l2("type.lvli.same.index", "lvli", _form("rObj", FX["ammo556"]) + "SetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
    _l2("type.lvli.caps.simple", "lvli", _form("rObj", FX["capsf"]) + "LeveledListAddForm rLst rObj 1 1 0\n" + _NUM),
    _l2("type.lvli.caps.health", "lvli", _form("rObj", FX["capsf"]) + "LeveledListAddForm rLst rObj 1 1 1.0\n" + _NUM),
    _l2("type.lvli.stim", "lvli", _form("rObj", FX["stim"]) + "LeveledListAddForm rLst rObj 1 1 1.0\n" + _NUM),
    _l2("type.lvli.list", "lvli", _form("rObj", FX["fortune"]) + "LeveledListAddForm rLst rObj 1 1 0\n" + _NUM),
    _l2("player.level", "lvli", "SetFunctionValue (rPlayer.GetLevel)"),
    _l2("type.lvli.atlevel", "lvli", _form("rOther", FX["ammo556"]) + "LeveledListAddForm rLst rOther (rPlayer.GetLevel) 1 1.0\n" + _NUM),
    _l2("type.lvli.abovelevel", "lvli", _form("rOther", FX["ammo556"]) + "LeveledListAddForm rLst rOther ((rPlayer.GetLevel) + 1) 1 1.0\n" + _NUM),
    _l2("type.lvli.level2", "lvli", _form("rObj", FX["ammo556"]) + "LeveledListAddForm rLst rObj 2 1 1.0\n" + _NUM),
    _l2("type.lvli.level7", "lvli", _form("rObj", FX["ammo556"]) + "LeveledListAddForm rLst rObj 7 1 1.0\n" + _NUM),
    _l2("type.lvli.count25", "lvli", _form("rObj", FX["ammo556"]) + "LeveledListAddForm rLst rObj 1 25 1.0\n" + _NUM),
    _l2("type.lvli.order", "lvli", "let fRes := 0\nlet sText := \"\"\nwhile fRes < (GetNumLevItems rLst)\n"
        "  let sText += (GetFormIDString (GetNthLevItem rLst fRes)) + (sv_Construct \":%g:%g \" (GetNthLevItemLevel rLst fRes) (GetNthLevItemCount rLst fRes))\n"
        "  let fRes += 1\nloop\nSetFunctionValue sText", True),
    # LVLC and LVLN.
    _l2("type.lvlc.crea", "lvlc", _form("rObj", FX["brahmin"]) + "LeveledListAddForm rLst rObj 1 1 0\n" + _NUM),
    _l2("type.lvlc.list", "lvlc", _form("rObj", FX["lvlc2"]) + "LeveledListAddForm rLst rObj 1 1 0\n" + _NUM),
    _l2("type.lvlc.npc", "lvlc", _form("rObj", FX["docnpc"]) + "LeveledListAddForm rLst rObj 1 1 0\n" + _NUM),
    _l2("type.lvln.list", "lvln", _form("rObj", FX["lvln2"]) + "LeveledListAddForm rLst rObj 1 1 0\n" + _NUM),
    _l2("type.lvln.crea", "lvln", _form("rObj", FX["brahmin"]) + "LeveledListAddForm rLst rObj 1 1 0\n" + _NUM),
    # A container from the edited list: VLootAmmoBoxGen30 holds 02FE5D.
    _l2("box.list.num", "boxlist", _NUM),
    _l2("box.list.rebuild", "boxlist", _form("rObj", FX["stim"])
        + "LeveledListClear rLst\nSetChanceNone rLst 0\nSetLeveledListFlags rLst 0\nLeveledListAddForm rLst rObj 1 3 1.0\n" + _NUM),
    _l2("box.place", "boxlist", _form("rObj", FX["ammobox"]) + 'let rOther := rPlayer.PlaceAtMe rObj 1\nrPlayer.AuxVarSetRef "*nvfBX" rOther\nSetFunctionValue 1'),
    _l2("box.stim", "boxlist", _form("rObj", FX["stim"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfBX"\nSetFunctionValue (rOther.GetItemCount rObj)'),
    _l2("box.items", "boxlist", 'let rOther := rPlayer.AuxVarGetRef "*nvfBX"\nSetFunctionValue (rOther.GetNumItems)'),
    _l2("box.reset.rebuild", "boxlist", _form("rObj", FX["stim"]) + "LeveledListClear rLst\nLeveledListAddForm rLst rObj 1 5 1.0\n" + _NUM),
    _l2("box.reset", "boxlist", 'let rOther := rPlayer.AuxVarGetRef "*nvfBX"\nrOther.ResetInventory\nSetFunctionValue 1'),
    _l2("box.reset.stim", "boxlist", _form("rObj", FX["stim"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfBX"\nSetFunctionValue (rOther.GetItemCount rObj)'),
    _l2("box.reset.stim.next", "boxlist", _form("rObj", FX["stim"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfBX"\nSetFunctionValue (rOther.GetItemCount rObj)'),
    _l2("box.additem", "boxlist", _form("rObj", FX["stim"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfBX"\nrOther.AddItem rLst 1 1\n'
        "SetFunctionValue (rOther.GetItemCount rObj)"),
    _l2("box.additem.next", "boxlist", _form("rObj", FX["stim"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfBX"\nSetFunctionValue (rOther.GetItemCount rObj)'),
    _l2("player.additem.boxlist.before", "boxlist", _form("rObj", FX["stim"]) + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
    _l2("player.additem.boxlist", "boxlist", "rPlayer.AddItem rLst 1 1\nSetFunctionValue 1"),
    _l2("player.additem.boxlist.after", "boxlist", _form("rObj", FX["stim"]) + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
    _l2("player.additem.boxlist.cleanup", "boxlist", _form("rObj", FX["stim"]) + "rPlayer.RemoveItem rObj 5 1\nSetFunctionValue (rPlayer.GetItemCount rObj)"),
    _l2("box.cleanup", "boxlist", 'let rOther := rPlayer.AuxVarGetRef "*nvfBX"\nrOther.Disable\nrOther.MarkForDelete\nrPlayer.AuxVarErase "*nvfBX"\nSetFunctionValue 1'),
]

# Thirty-fourth set: JIP's live-projectile functions, read inside a
# SetOnProjectileImpactEventHandler lambda on the 10mm pistol's projectile (no arguments;
# the projectile is the calling reference, GetSelf [documented]). The player fires the
# pistol with FireWeapon (no need to equip it [documented: FireWeapon]); each impact
# appends its values to `*` auxiliary variables on the player. The handler dies with the
# session; the last step erases the variables. The navmesh functions are left out: no
# script function reads whether a navmesh is disabled.
PROJ_VALUES = [
    ("count", "1"),
    ("impact.isref", "IsReference (GetSelf.GetProjectileRefImpactRef)"),
    ("impact.type", "GetType ((GetSelf.GetProjectileRefImpactRef).GetBaseObject)"),
    ("source.player", "(GetSelf.GetProjectileRefSource) == rP"),
    ("weapon.ispistol", "(GetSelf.GetProjectileRefWeapon) == rGun"),
    ("damage", "GetSelf.GetProjectileRefDamage"),
    ("self.type", "GetType (GetSelf.GetBaseObject)"),
]


def _pj(pid, body, text=False):
    return (pid, "ref rP\nref rGun\nref rProj\nfloat fRes\nBegin Function { }\n" + _form("rP", NF["player"])
            + _form("rGun", NF["pistol10"]) + body, text, None, "")


def _proj_steps():
    steps = [_pj("proj.form", "SetFunctionValue (GetFormIDString (GetWeaponProjectile rGun))", True)]
    # A first handler, then the one that reads everything: whether the second replaces
    # the first shows in proj.first.size (one handler per projectile was seen when each
    # value had its own handler).
    steps.append(_pj("proj.reg.first", "let rProj := GetWeaponProjectile rGun\n"
                     'SetOnProjectileImpactEventHandler (Begin Function { }\n  rP.AuxVarSetFlt "*nvfPJ.first" 1 -1\nEnd) 1 rProj\n'
                     "SetFunctionValue 1"))
    body = "".join(f'  rP.AuxVarSetFlt "*nvfPJ.{name}" ({expr}) -1\n' for name, expr in PROJ_VALUES
                   if name in ("count", "impact.isref", "source.player", "self.type"))
    steps.append(_pj("proj.reg", "let rProj := GetWeaponProjectile rGun\n"
                     f"SetOnProjectileImpactEventHandler (Begin Function {{ }}\n{body}End) 1 rProj\nSetFunctionValue 1"))
    # The rest in a handler of its own: a lambda of all seven lines does not compile in
    # the game, four do.
    body2 = "".join(f'  rP.AuxVarSetFlt "*nvfPJ.{name}" ({expr}) -1\n' for name, expr in PROJ_VALUES
                    if name in ("impact.type", "weapon.ispistol", "damage"))
    steps.append(_pj("proj.reg2", "let rProj := GetWeaponProjectile rGun\n"
                     f"SetOnProjectileImpactEventHandler (Begin Function {{ }}\n{body2}End) 1 rProj\nSetFunctionValue 1"))
    # Look down so the shot lands near the player within a few frames.
    steps.append(_pj("proj.aim", "rP.SetAngle X 80\nSetFunctionValue (rP.GetAngle X)"))
    steps.append(_pj("proj.fire", "rP.FireWeapon rGun\nSetFunctionValue 1"))
    steps += [_pj(f"proj.settle{i}", "SetFunctionValue 1") for i in range(10)]
    for name, _ in [("first", "")] + PROJ_VALUES:
        steps.append(_pj(f"proj.{name}.size", f'SetFunctionValue (rP.AuxVarSize "*nvfPJ.{name}")'))
        steps.append(_pj(f"proj.{name}.0", f'SetFunctionValue (rP.AuxVarGetFlt "*nvfPJ.{name}" 0)'))
    steps.append(_pj("proj.cleanup", "".join(f'rP.AuxVarErase "*nvfPJ.{n}"\n' for n, _ in [("first", "")] + PROJ_VALUES)
                     + "SetFunctionValue 1"))
    return steps


PROJECTILE_PROBES = _proj_steps()

# Thirty-fifth set (next batch N4): JIP's sound, weather and image-space traits. Trait
# ID ranges from the function pages [documented: GetSoundTraitNumeric 0-12,
# GetWeatherTraitNumeric 0-20, GetWeatherRGBColor types 0-9 / times 0-5 / layers 0-3,
# GetImageSpaceTrait 0-32, GetImageSpaceModTrait 0-67]. Forms from fnvtool esm inventory
# [observed]: EMTMedicalBeep01 (SOUN) 174325, HazmatImageSpace (IMGS) 0542F2,
# FadeToBlack9sISFX (IMAD) 173904. The weather rows name the weather the save has current
# at the airport (wth.current: 00171285) directly, so they test the trait functions and
# not which weather the save happens to have (a headless run cannot know that). A string_var as SetSoundSourceFile's path does not compile ($sText), so
# the sound's path is put back as a literal. Each setter's row is followed by a row putting the old value back (session
# only, never saved).
FX.update(beep="174325", hazmat="542F2", fadeblack="173904")


def _sk(pid, body, text=False):
    return (pid, "ref rPlayer\nref rObj\nref rW8\nfloat fRes\nstring_var sText\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + body, text, None, "")


def _skv(pid, pre, expr, text=False):
    return _sk(pid, pre + f"SetFunctionValue ({expr})", text)


_SND = _form("rObj", FX["beep"])
_IMGS = _form("rObj", FX["hazmat"])
_IMAD = _form("rObj", FX["fadeblack"])
_WTH = _form("rW8", "171285")

SKY_SOUND_PROBES = (
    # Sound.
    [_skv(f"snd.trait.{i}", _SND, f"GetSoundTraitNumeric rObj {i}") for i in range(13)]
    + [_skv("snd.trait.past", _SND, "GetSoundTraitNumeric rObj 13"),
       _sk("snd.trait.set", _SND + 'let fRes := GetSoundTraitNumeric rObj 3\nrPlayer.AuxVarSetFlt "*nvfSN" fRes\n'
           "SetFunctionValue (SetSoundTraitNumeric rObj 3 7)"),
       _skv("snd.trait.set.read", _SND, "GetSoundTraitNumeric rObj 3"),
       _skv("snd.trait.restore", _SND + 'SetSoundTraitNumeric rObj 3 (rPlayer.AuxVarGetFlt "*nvfSN")\nrPlayer.AuxVarErase "*nvfSN"\n',
            "GetSoundTraitNumeric rObj 3"),
       _skv("snd.file", _SND, "GetSoundSourceFile rObj", True),
       _sk("snd.file.set", _SND + 'let sText := GetSoundSourceFile rObj\nrPlayer.AuxVarSetStr "*nvfSF" sText\n'
           'SetSoundSourceFile rObj "fx\\nvf\\probe.wav"\nSetFunctionValue (GetSoundSourceFile rObj)', True),
       _sk("snd.file.restore", _SND + 'SetSoundSourceFile rObj "fx\\emt\\medicalbeep\\emtmedicalbeep01.wav"\nrPlayer.AuxVarErase "*nvfSF"\n'
           "SetFunctionValue (GetSoundSourceFile rObj)", True),
       _skv("snd.playing.before", _SND, "IsSoundPlaying rObj"),
       _sk("snd.play", _SND + "PlaySound rObj\nSetFunctionValue (IsSoundPlaying rObj)"),
       _skv("snd.playing.next", _SND, "IsSoundPlaying rObj")]
    # Weather.
    + [_skv("wth.current", "let rW8 := GetCurrentWeather\n", "GetFormIDString rW8", True),
       _skv("wth.percent", "", "Ceil ((GetCurrentWeatherPercent) * 100)")]
    + [_skv(f"wth.trait.{i}", _WTH, f"GetWeatherTraitNumeric rW8 {i}") for i in range(21)]
    + [_sk("wth.trait.set", _WTH + 'let fRes := GetWeatherTraitNumeric rW8 4\nrPlayer.AuxVarSetFlt "*nvfWT" fRes\n'
           "SetFunctionValue (SetWeatherTraitNumeric rW8 4 0.5)"),
       _skv("wth.trait.set.read", _WTH, "GetWeatherTraitNumeric rW8 4"),
       _skv("wth.trait.restore", _WTH + 'SetWeatherTraitNumeric rW8 4 (rPlayer.AuxVarGetFlt "*nvfWT")\nrPlayer.AuxVarErase "*nvfWT"\n',
            "GetWeatherTraitNumeric rW8 4")]
    + [_skv(f"wth.rgb.type{k}.day", _WTH, f"sv_Construct \"%.0f\" (GetWeatherRGBColor rW8 {k} 1)", True) for k in range(10)]
    + [_skv(f"wth.rgb.ambient.time{t}", _WTH, f"sv_Construct \"%.0f\" (GetWeatherRGBColor rW8 3 {t})", True) for t in range(6)]
    + [_skv(f"wth.rgb.clouds.layer{c}", _WTH, f"sv_Construct \"%.0f\" (GetWeatherRGBColor rW8 2 1 {c})", True) for c in range(4)]
    + [_sk("wth.rgb.set", _WTH + 'let fRes := GetWeatherRGBColor rW8 3 1\nrPlayer.AuxVarSetFlt "*nvfWC" fRes\n'
           "SetFunctionValue (SetWeatherRGBColor rW8 3 1 16711935)"),
       _skv("wth.rgb.set.read", _WTH, 'sv_Construct "%.0f" (GetWeatherRGBColor rW8 3 1)', True)]
    # Where out-of-range parts go: each value set, then read back (the extender core
    # asked after 16711935 read 16199937).
    + [_skv(f"wth.rgb.set.{v}", _WTH + f"SetWeatherRGBColor rW8 3 1 {v}\n", 'sv_Construct "%.0f" (GetWeatherRGBColor rW8 3 1)', True)
       for v in (255255255, 0, 1, 255, 256, 1000, 255256000, 255000256, 256000000, 300000000, 12345678, 999999999)]
    + [_skv("wth.rgb.restore", _WTH + 'SetWeatherRGBColor rW8 3 1 (rPlayer.AuxVarGetFlt "*nvfWC")\nrPlayer.AuxVarErase "*nvfWC"\n',
            'sv_Construct "%.0f" (GetWeatherRGBColor rW8 3 1)', True)]
    + [_skv(f"wth.imod.time{t}", _WTH, f"GetFormIDString (GetWeatherImageSpaceMod rW8 {t})", True) for t in range(4)]
    # Image space and image-space modifier.
    + [_skv(f"imgs.trait.{i}", _IMGS, f"GetImageSpaceTrait rObj {i}") for i in range(33)]
    + [_sk("imgs.trait.set", _IMGS + 'let fRes := GetImageSpaceTrait rObj 15\nrPlayer.AuxVarSetFlt "*nvfIS" fRes\n'
           "SetImageSpaceTrait rObj 15 3.25\nSetFunctionValue (GetImageSpaceTrait rObj 15)"),
       _skv("imgs.trait.restore", _IMGS + 'SetImageSpaceTrait rObj 15 (rPlayer.AuxVarGetFlt "*nvfIS")\nrPlayer.AuxVarErase "*nvfIS"\n',
            "GetImageSpaceTrait rObj 15")]
    + [_skv(f"imad.trait.{i}", _IMAD, f"GetImageSpaceModTrait rObj {i}") for i in range(68)]
    + [_sk("imad.trait.set", _IMAD + 'let fRes := GetImageSpaceModTrait rObj 0\nrPlayer.AuxVarSetFlt "*nvfIM" fRes\n'
           "SetImageSpaceModTrait rObj 0 4.5\nSetFunctionValue (GetImageSpaceModTrait rObj 0)"),
       _skv("imad.trait.restore", _IMAD + 'SetImageSpaceModTrait rObj 0 (rPlayer.AuxVarGetFlt "*nvfIM")\nrPlayer.AuxVarErase "*nvfIM"\n',
            "GetImageSpaceModTrait rObj 0")]
)

# Thirty-sixth set (next batch N5): kNVSE's animation functions on a loaded actor: a
# copy of Doc Mitchell's NPC placed beside the player (his own reference's cell is not
# loaded). KF paths from fnvtool bsa list of Fallout - Meshes.bsa [observed]. The copy is
# removed at the end; the weapon override is undone.
_KF_IDLE = "characters\\_male\\locomotion\\mtidle.kf"
_KF_SMILE = "characters\\_male\\idleanims\\cg03dadsmilestanding.kf"
_KF_H2H = "characters\\_male\\h2hidle.kf"
_KF_JUMP = "characters\\_male\\1hpjumplandright.kf"
_HK = ("ref rPlayer\nref rA\nref rObj\nfloat fRes\nstring_var sText\narray_var aA\nBegin Function { }\n"
       + _form("rPlayer", NF["player"]))
_GETA = 'let rA := rPlayer.AuxVarGetRef "*nvfKA"\n'
_JOIN = ("let fRes := 0\nlet sText := sv_Construct \"%g:\" (ar_Size aA)\nwhile fRes < (ar_Size aA)\n"
         "  let sText += \" \" + aA[fRes]\n  let fRes += 1\nloop\nSetFunctionValue sText")
# The active list after ActivateAnim came back in another order on a second run, and
# ar_Sort does not compile at run time: that row keeps the size and whether each of the
# three sequences is in it.
_HAS3 = ("SetFunctionValue (sv_Construct \"%g %g %g %g\" (ar_Size aA) "
         "((ar_Find \"Characters\\_Male\\h2hidle.kf\" aA) >= 0) "
         "((ar_Find \"Characters\\_Male\\Locomotion\\mtidle.kf\" aA) >= 0) "
         "((ar_Find \"characters\\_male\\idleanims\\cg03dadsmilestanding.kf\" aA) >= 0))")


def _kn(pid, body, text=False):
    return (pid, _HK + body, text, None, "")


def _knv(pid, expr, text=False):
    return _kn(pid, _GETA + f"SetFunctionValue ({expr})", text)


KNVSE_PROBES = [
    _kn("kn.place", _form("rObj", NF["doc"]) + 'let rA := rPlayer.PlaceAtMe rObj 1\nrPlayer.AuxVarSetRef "*nvfKA" rA\nSetFunctionValue 1'),
    _kn("kn.settle", "SetFunctionValue 1"),
    _kn("kn.settle2", "SetFunctionValue 1"),
] + [_kn(f"kn.settle{i}", "SetFunctionValue 1") for i in range(3, 6)] + [
    # What plays right after placing depends on blend timing (runs gave the idle, empty
    # slots, or a __TempBlendSequence__, and GetPlayingAnimSequencePaths gave ar_Null in
    # some runs): only whether anything is active is kept.
    _kn("kn.active", _GETA + "let aA := rA.GetActiveAnims 0\nSetFunctionValue ((ar_Size aA) > 0)"),
    _knv("kn.animplaying.idle", "rA.IsAnimPlayingAlt Idle"),
    _knv("kn.seq.mtidle", f'rA.IsAnimSequencePlaying "{_KF_IDLE}"'),
    _kn("kn.bygroup.idle", _GETA + "let aA := rA.GetAnimationsByAnimGroup Idle\n" + _JOIN, True),
    _knv("kn.globaltime", "Ceil (rA.GetGlobalAnimTime 0)"),
    # SetActorAnimationPath: an override of the Idle group.
    _knv("kn.setpath", f'rA.SetActorAnimationPath 0 1 "{_KF_H2H}"'),
    _kn("kn.setpath.bygroup", _GETA + "let aA := rA.GetAnimationsByAnimGroup Idle\n" + _JOIN, True),
    _knv("kn.setpath.off", f'rA.SetActorAnimationPath 0 0 "{_KF_H2H}"'),
    _kn("kn.setpath.off.bygroup", _GETA + "let aA := rA.GetAnimationsByAnimGroup Idle\n" + _JOIN, True),
    _knv("kn.setpath.missing", 'rA.SetActorAnimationPath 0 1 "characters\\_male\\nvfnone.kf"'),
    # PlayAnimationPath and ActivateAnim / DeactivateAnim.
    _kn("kn.play", _GETA + f'rA.PlayAnimationPath "{_KF_SMILE}" 0\nSetFunctionValue (rA.IsAnimSequencePlaying "{_KF_SMILE}")'),
    _knv("kn.play.next", f'rA.IsAnimSequencePlaying "{_KF_SMILE}"'),
    _kn("kn.play.attrs", _GETA + f'let aA := ar_Keys (rA.GetAnimationAttributes "{_KF_SMILE}")\n' + _JOIN, True),
    _kn("kn.activate", _GETA + f'rA.ActivateAnim "{_KF_H2H}" 0 2 1 1 0\nSetFunctionValue (rA.IsAnimSequencePlaying "{_KF_H2H}")'),
    _knv("kn.activate.next", f'rA.IsAnimSequencePlaying "{_KF_H2H}"'),
    _kn("kn.activate.active", _GETA + "let aA := rA.GetActiveAnims 0\n" + _HAS3, True),
    _kn("kn.deactivate", _GETA + f'rA.DeactivateAnim "{_KF_H2H}" 0\nSetFunctionValue (rA.IsAnimSequencePlaying "{_KF_H2H}")'),
    _knv("kn.deactivate.next", f'rA.IsAnimSequencePlaying "{_KF_H2H}"'),
    # ForcePlayIdle / ForceStopIdle.
    _kn("kn.idle.play", _GETA + _form("rObj", FX["idlehouse"]) + "rA.ForcePlayIdle rObj\nSetFunctionValue (rA.IsIdlePlayingEx rObj)"),
    _kn("kn.idle.next", _GETA + _form("rObj", FX["idlehouse"]) + "SetFunctionValue (rA.IsIdlePlayingEx rObj)"),
    # Whether the idle still plays after ForceStopIdle varied between runs (1 or 0, also
    # three frames later), so ForceStopIdle is only called here, not read back.
    _kn("kn.idle.stop", _GETA + "rA.ForceStopIdle\nSetFunctionValue 1"),
    # A weapon's animation override.
    _kn("kn.weappath", _form("rObj", NF["pistol10"]) + f'SetFunctionValue (SetWeaponAnimationPath rObj 0 1 "{_KF_JUMP}")'),
    _kn("kn.weappath.off", _form("rObj", NF["pistol10"]) + f'SetFunctionValue (SetWeaponAnimationPath rObj 0 0 "{_KF_JUMP}")'),
    # The player (first person in the save).
    _kn("kn.player.paths", "let aA := rPlayer.GetPlayingAnimSequencePaths\n" + _JOIN, True),
    _kn("kn.player.animplaying.idle", "SetFunctionValue (rPlayer.IsAnimPlayingAlt Idle)"),
    _kn("kn.cleanup", _GETA + 'rA.Disable\nrA.MarkForDelete\nrPlayer.AuxVarErase "*nvfKA"\nSetFunctionValue 1'),
]

# Thirty-seventh set (next batch N6): JIP's NIF block functions on a loaded actor (a
# copy of Doc's NPC placed beside the player, as knvse-anim), by node name: the Bip01
# skeleton. Changes are made on that reference only (InsertNode / AttachModel mode 3,
# instantaneous) and go with it at the end. ModelHasBlock reads the 10mm pistol's model.
def _nb(pid, body, text=False):
    return (pid, _HK.replace("float fRes\n", "float fRes\nfloat fX\nfloat fY\nfloat fZ\n") + body, text, None, "")


def _nbv(pid, expr, text=False):
    return _nb(pid, _GETA + f"SetFunctionValue ({expr})", text)


# Array elements go through float variables first: sv_Construct with aA[0] as an
# argument fails at run time ("script call failed" with a 3-element array).
_V3 = ('let fX := aA[0]\nlet fY := aA[1]\nlet fZ := aA[2]\n'
       'SetFunctionValue (sv_Construct "%.2f %.2f %.2f" fX fY fZ)')

NIF_PROBES = [
    _nb("nb.place", _form("rObj", NF["doc"]) + 'let rA := rPlayer.PlaceAtMe rObj 1\nrPlayer.AuxVarSetRef "*nvfKA" rA\nSetFunctionValue 1'),
    _nb("nb.settle", "SetFunctionValue 1"),
    _nb("nb.settle2", "SetFunctionValue 1"),
] + [_nb(f"nb.settle{i}", "SetFunctionValue 1") for i in range(3, 6)] + [
    _nb("nb.trans.head.size", _GETA + 'let aA := rA.GetNifBlockTranslation "Bip01 Head" 0\nSetFunctionValue (ar_Size aA)'),
    _nb("nb.trans.alt", _GETA.replace("", "") + 'let fRes := rA.GetNifBlockTranslationAlt "Bip01 Head" fX fY fZ 0\n'
        'SetFunctionValue (sv_Construct "%g %.2f %.2f %.2f" fRes fX fY fZ)', True),
    _nb("nb.rot.alt", _GETA + 'let fRes := rA.GetNifBlockRotationAlt "Bip01 Head" fX fY fZ 0\n'
        'SetFunctionValue (sv_Construct "%g %.2f %.2f %.2f" fRes fX fY fZ)', True),
    _nb("nb.children.bip01", _GETA + 'let aA := rA.GetChildBlocks "Bip01"\n' + _JOIN, True),
    _nb("nb.children.head", _GETA + 'let aA := rA.GetChildBlocks "Bip01 Head"\n' + _JOIN, True),
    _nb("nb.children.missing", _GETA + 'let aA := rA.GetChildBlocks "NVFNone"\nSetFunctionValue (ar_Size aA)'),
    _nb("nb.parents.head", _GETA + 'let aA := rA.GetNifBlockParentNodes "Bip01 Head"\n' + _JOIN, True),
    # The head's pose follows the idle animation (two runs differed in the second
    # decimal): only the vector's size is kept.
    _nb("nb.trans.head", _GETA + 'let aA := rA.GetNifBlockTranslation "Bip01 Head" 0\nSetFunctionValue (ar_Size aA)'),
    _nb("nb.rot.head", _GETA + 'let aA := rA.GetNifBlockRotation "Bip01 Head" 0\nSetFunctionValue (ar_Size aA)'),
    _nb("nb.rot.head.local", _GETA + 'let aA := rA.GetNifBlockRotation "Bip01 Head" 2\nSetFunctionValue (ar_Size aA)'),
    _nb("nb.trans.missing", _GETA + 'let aA := rA.GetNifBlockTranslation "NVFNone" 0\nSetFunctionValue (ar_Size aA)'),
    _nbv("nb.scale.head", 'rA.GetNifBlockScale "Bip01 Head"'),
    _nbv("nb.scale.missing", 'rA.GetNifBlockScale "NVFNone"'),
    _nb("nb.scale.set", _GETA + 'rA.SetNifBlockScale "Bip01 Head" 1.5\nSetFunctionValue (rA.GetNifBlockScale "Bip01 Head")'),
    _nbv("nb.scale.next", 'rA.GetNifBlockScale "Bip01 Head"'),
    _nb("nb.trans.set", _GETA + 'rA.SetNifBlockTranslation "Bip01 Head" 1 2 3\nlet aA := rA.GetNifBlockTranslation "Bip01 Head" 0\n' + _V3, True),
    _nb("nb.rot.set", _GETA + 'rA.SetNifBlockRotation "Bip01 Head" 10 20 30 0\nlet aA := rA.GetNifBlockRotation "Bip01 Head" 0\n' + _V3, True),
] + [_nbv(f"nb.flag.head.{f}", f'rA.GetNifBlockFlag "Bip01 Head" {f}') for f in range(4)] + [
    _nb("nb.flag.set", _GETA + 'rA.SetNifBlockFlag "Bip01 Head" 0 1\nSetFunctionValue (rA.GetNifBlockFlag "Bip01 Head" 0)'),
    _nb("nb.flag.clear", _GETA + 'rA.SetNifBlockFlag "Bip01 Head" 0 0\nSetFunctionValue (rA.GetNifBlockFlag "Bip01 Head" 0)'),
    _nb("nb.rename", _GETA + 'SetFunctionValue (rA.RenameNifBlock "Bip01 Head" "NVFHead")'),
    _nbv("nb.rename.new", 'rA.GetNifBlockScale "NVFHead"'),
    _nbv("nb.rename.old", 'rA.GetNifBlockScale "Bip01 Head"'),
    _nb("nb.rename.back", _GETA + 'SetFunctionValue (rA.RenameNifBlock "NVFHead" "Bip01 Head")'),
    _nb("nb.insert", _GETA + 'SetFunctionValue (InsertNode rA 3 "Bip01 Head|NVFNode")'),
    _nb("nb.insert.children", _GETA + 'let aA := rA.GetChildBlocks "Bip01 Head"\n' + _JOIN, True),
    _nb("nb.insert.parent", _GETA + 'SetFunctionValue (InsertNode rA 3 "Bip01 Head|^NVFParent")'),
    _nb("nb.insert.parent.parents", _GETA + 'let aA := rA.GetNifBlockParentNodes "Bip01 Head"\n' + _JOIN, True),
    _nb("nb.insert.case", _GETA + 'SetFunctionValue (InsertNode rA 3 "bip01 head|NVFNode2")'),
    _nb("nb.attach", _GETA + 'SetFunctionValue (AttachModel rA 3 "NVFNode|Weapons\\1HandPistol\\10mmPistol.NIF")'),
    _nb("nb.attach.children", _GETA + 'let aA := rA.GetChildBlocks "NVFNode"\n' + _JOIN, True),
    _nb("nb.remove", _GETA + 'SetFunctionValue (rA.RemoveNifBlock "NVFNode")'),
    _nbv("nb.remove.scale", 'rA.GetNifBlockScale "NVFNode"'),
    _nb("nb.hasblock.pistol", _form("rObj", NF["pistol10"]) + 'SetFunctionValue (ModelHasBlock rObj "ProjectileNode")'),
    _nb("nb.hasblock.missing", _form("rObj", NF["pistol10"]) + 'SetFunctionValue (ModelHasBlock rObj "NVFNone")'),
    _nb("nb.hasblock.case", _form("rObj", NF["pistol10"]) + 'SetFunctionValue (ModelHasBlock rObj "projectilenode")'),
    _nb("nb.player.children", 'let aA := rPlayer.GetChildBlocks "Bip01" 2\n' + _JOIN, True),
    # A world object: the 10mm pistol placed as an item beside the player.
    _nb("nb.obj.place", _form("rObj", NF["pistol10"]) + 'let rA := rPlayer.PlaceAtMe rObj 1\nrPlayer.AuxVarSetRef "*nvfKO" rA\nSetFunctionValue 1'),
] + [_nb(f"nb.obj.settle{i}", "SetFunctionValue 1") for i in range(5)] + [
    _nb("nb.obj.trans.size", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nlet aA := rA.GetNifBlockTranslation "ProjectileNode" 0\nSetFunctionValue (ar_Size aA)'),
    _nb("nb.obj.rot.size", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nlet aA := rA.GetNifBlockRotation "ProjectileNode" 0\nSetFunctionValue (ar_Size aA)'),
    _nb("nb.obj.trans", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nlet aA := rA.GetNifBlockTranslation "ProjectileNode" 0\n' + _V3, True),
    _nb("nb.obj.rot", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nlet aA := rA.GetNifBlockRotation "ProjectileNode" 0\n' + _V3, True),
    _nb("nb.obj.trans.set", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nrA.SetNifBlockTranslation "ProjectileNode" 1 2 3\n'
        'let aA := rA.GetNifBlockTranslation "ProjectileNode" 0\n' + _V3, True),
    _nb("nb.obj.rot.set", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nrA.SetNifBlockRotation "ProjectileNode" 10 20 30 0\n'
        'let aA := rA.GetNifBlockRotation "ProjectileNode" 0\n' + _V3, True),
    _nb("nb.obj.scale", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nSetFunctionValue (rA.GetNifBlockScale "ProjectileNode")'),
    _nb("nb.obj.children", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nlet aA := rA.GetChildBlocks "10MMPistol"\n' + _JOIN, True),
    _nb("nb.obj.cleanup", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nrA.Disable\nrA.MarkForDelete\nrPlayer.AuxVarErase "*nvfKO"\nSetFunctionValue 1'),
    _nb("nb.cleanup", _GETA + 'rA.Disable\nrA.MarkForDelete\nrPlayer.AuxVarErase "*nvfKA"\nSetFunctionValue 1'),
]

# Thirty-eighth set (next batch N11): which xNVSE events fire in the game, and with which
# two arguments. One SetEventHandler lambda per event appends its firing count and both
# arguments (as references) to `*` auxiliary variables on the player; then the events
# are caused cheaply: the player's inventory (OnAdd, OnDrop, OnActorEquip/OnEquip,
# OnActorUnequip/OnUnequip), a placed creature (OnLoad, OnDisable/OnEnable,
# OnStartCombat, OnHit/OnHitWith, OnDeath/OnMurder), a placed NPC (OnPackageStart/
# Change), a stimpak cast on the player (OnMagicEffectHit). Left out: events that open
# a menu or dialogue (OnActivate, OnOpen/OnClose, OnSell). Handlers die with the session.
_EVENTS = ["OnAdd", "OnDrop", "OnActorEquip", "OnEquip", "OnActorUnequip", "OnUnequip", "OnLoad", "OnDisable",
           "OnEnable", "OnStartCombat", "OnHit", "OnHitWith", "OnDeath", "OnMurder", "OnPackageStart",
           "OnPackageChange", "OnMagicEffectHit", "OnCombatEnd"]
_HE = ("ref rP\nref rA\nref rB\nref rObj\nfloat fRes\nBegin Function { }\n" + _form("rP", NF["player"]))


def _ev(pid, body, text=False):
    return (pid, _HE + body, text, None, "")


def _ev_reg(name):
    k = name.lower()
    return _ev(f"evn.reg.{k}", f'SetEventHandler "{name}" (Begin Function {{ref rE1, ref rE2}}\n'
               f'  rP.AuxVarSetFlt "*nvfE.{k}" 1 -1\n  rP.AuxVarSetRef "*nvfE.{k}.1" rE1 -1\n'
               f'  rP.AuxVarSetRef "*nvfE.{k}.2" rE2 -1\nEnd)\nSetFunctionValue 1')


# An argument as "player", "creature" / "npc" (the placed ones), "ref:<base form>" for any
# other reference (made-at-run-time
# references get a different ID each run) or the form's ID.
_ARG = ('let rB := rP.AuxVarGetRef "*nvfEC"\nlet rObj := rP.AuxVarGetRef "*nvfEN"\n'
        'if rA == rP\n  SetFunctionValue "player"\nelseif rA == rB\n  SetFunctionValue "creature"\n'
        'elseif rA == rObj\n  SetFunctionValue "npc"\nelseif IsReference rA\n'
        '  SetFunctionValue ("ref:" + GetFormIDString (rA.GetBaseObject))\nelse\n'
        '  SetFunctionValue (GetFormIDString rA)\nendif')
_CRE = 'let rA := rP.AuxVarGetRef "*nvfEC"\n'
_NPC = 'let rB := rP.AuxVarGetRef "*nvfEN"\n'

NVSE_EVENT_PROBES = (
    [_ev_reg(n) for n in _EVENTS]
    + [
        # Inventory.
        _ev("evn.do.add", _form("rObj", NF["stimpak"]) + "rP.AddItem rObj 1 1\nSetFunctionValue 1"),
        _ev("evn.do.drop", _form("rObj", NF["stimpak"]) + 'rP.Drop rObj 1\nSetFunctionValue 1'),
        _ev("evn.do.addgun", _form("rObj", NF["pistol10"]) + "rP.AddItem rObj 1 1\nSetFunctionValue 1"),
        _ev("evn.do.equip", _form("rObj", NF["pistol10"]) + "rP.EquipItem rObj 0 1\nSetFunctionValue 1"),
        _ev("evn.do.unequip", _form("rObj", NF["pistol10"]) + "rP.UnequipItem rObj 0 1\nSetFunctionValue 1"),
        _ev("evn.do.removegun", _form("rObj", NF["pistol10"]) + "rP.RemoveItem rObj 1 1\nSetFunctionValue 1"),
        # A placed creature (the airport ACRE's base) and a placed NPC (Doc's).
        _ev("evn.do.placecreature", _form("rObj", "14DEE2") + 'let rObj := rObj.GetBaseObject\nlet rA := rP.PlaceAtMe rObj 1\n'
            'rP.AuxVarSetRef "*nvfEC" rA\nSetFunctionValue 1'),
        _ev("evn.do.placenpc", _form("rObj", NF["doc"]) + 'let rB := rP.PlaceAtMe rObj 1\nrP.AuxVarSetRef "*nvfEN" rB\nSetFunctionValue 1'),
    ]
    + [_ev(f"evn.settle{i}", "SetFunctionValue 1") for i in range(4)]
    + [
        _ev("evn.do.disable", _CRE + "rA.Disable\nSetFunctionValue 1"),
        _ev("evn.do.enable", _CRE + "rA.Enable\nSetFunctionValue 1"),
        _ev("evn.do.package", _NPC + "rB.EvaluatePackage\nSetFunctionValue 1"),
        _ev("evn.do.combat", _CRE + "rA.StartCombat rP\nSetFunctionValue 1"),
        _ev("evn.do.cast", _form("rObj", NF["stimpak"]) + "rP.CastImmediate rObj\nSetFunctionValue 1"),
        _ev("evn.do.boost", "rP.ModAV Health 1000\nSetFunctionValue 1"),
        _ev("evn.do.hit", _CRE + _form("rObj", "7078B") + "rA.PlaceAtMe rObj 1\nSetFunctionValue 1"),
    ]
    + [_ev(f"evn.settle.hit{i}", "SetFunctionValue 1") for i in range(3)]
    + [
        _ev("evn.do.kill", _CRE + "rA.Kill rP\nSetFunctionValue 1"),
        _ev("evn.do.stopcombat", _NPC + "rB.StopCombat\nSetFunctionValue 1"),
    ]
    + [_ev(f"evn.settle.kill{i}", "SetFunctionValue 1") for i in range(3)]
    + [x for n in _EVENTS for x in (
        # Whether it fired: how many times depends on frame timing (two runs gave
        # OnHit 4 and 1, OnLoad 4 and 3).
        _ev(f"evn.{n.lower()}.fired", f'SetFunctionValue ((rP.AuxVarSize "*nvfE.{n.lower()}") > 0)'),
        _ev(f"evn.{n.lower()}.arg1", f'let rA := rP.AuxVarGetRef "*nvfE.{n.lower()}.1" 0\n' + _ARG, True),
        _ev(f"evn.{n.lower()}.arg2", f'let rA := rP.AuxVarGetRef "*nvfE.{n.lower()}.2" 0\n' + _ARG, True),
    )]
    + [
        _ev("evn.cleanup", _CRE + _NPC + "rA.Disable\nrA.MarkForDelete\nrB.Disable\nrB.MarkForDelete\n"
            + "rP.ModAV Health -1000\nrP.RestoreAV Health 2000\nSetFunctionValue 1"),
    ]
)

# Thirty-ninth set (N11, JIP and JohnnyGuitar handlers): the SetOn...EventHandler events
# a script can cause without a menu or input: FireWeapon (player), UseAidItem (a stimpak
# used through EquipItem), CrippledLimb and PlayGroup (a placed copy of Doc's NPC),
# Ragdoll (a placed creature killed), NoteAdded (AddNote vYesManBunkerNote 177875,
# fnvtool esm inventory [observed]) and JohnnyGuitar's ActorValueChange (ModAV on the
# player). Each lambda takes the arguments the function page lists [documented] and
# appends what it got to `*` auxiliary variables. Handlers die with the session.
FX.update(note="177875")
_HJ = ("ref rP\nref rA\nref rB\nref rObj\nfloat fRes\nBegin Function { }\n" + _form("rP", NF["player"]))


def _je(pid, body, text=False):
    return (pid, _HJ + body, text, None, "")


def _jread(pid, expr, text=False):
    return _je(pid, f"SetFunctionValue ({expr})", text)


_JC = 'let rA := rP.AuxVarGetRef "*nvfJC"\n'
_JN = 'let rB := rP.AuxVarGetRef "*nvfJN"\n'

JIP_EVENT2_PROBES = [
    # Actors to act on.
    _je("je.place.creature", _form("rObj", "14DEE2") + 'let rObj := rObj.GetBaseObject\nlet rA := rP.PlaceAtMe rObj 1\n'
        'rP.AuxVarSetRef "*nvfJC" rA\nSetFunctionValue 1'),
    _je("je.place.npc", _form("rObj", NF["doc"]) + 'let rB := rP.PlaceAtMe rObj 1\nrP.AuxVarSetRef "*nvfJN" rB\nSetFunctionValue 1'),
] + [_je(f"je.settle{i}", "SetFunctionValue 1") for i in range(5)] + [
    # Registrations.
    _je("je.reg.fire", 'SetOnFireWeaponEventHandler (Begin Function {ref rWp}\n  rP.AuxVarSetFlt "*nvfJ.fire" (GetSelf == rP) -1\n'
        '  rP.AuxVarSetRef "*nvfJ.fire.w" rWp -1\nEnd) 1 rP\nSetFunctionValue 1'),
    _je("je.reg.aid", _form("rObj", NF["stimpak"]) + 'SetOnUseAidItemEventHandler (Begin Function {ref rIt}\n'
        '  rP.AuxVarSetFlt "*nvfJ.aid" (GetSelf == rP) -1\n  rP.AuxVarSetRef "*nvfJ.aid.i" rIt -1\nEnd) 1 rObj\nSetFunctionValue 1'),
    _je("je.reg.cripple", _JN + 'SetOnCrippledLimbEventHandler (Begin Function {int iLimb}\n'
        '  rP.AuxVarSetFlt "*nvfJ.cripple" iLimb -1\nEnd) 1 rB\nSetFunctionValue 1'),
    _je("je.reg.playgroup", _JN + 'SetOnPlayGroupEventHandler (Begin Function {int iGroup}\n'
        '  rP.AuxVarSetFlt "*nvfJ.group" iGroup -1\nEnd) 1 rB Idle\nSetFunctionValue 1'),
    _je("je.reg.ragdoll", _JC + 'SetOnRagdollEventHandler (Begin Function { }\n'
        '  rP.AuxVarSetFlt "*nvfJ.ragdoll" (GetSelf == (rP.AuxVarGetRef "*nvfJC")) -1\nEnd) 1\nSetFunctionValue 1'),
    _je("je.reg.note", 'SetOnNoteAddedEventHandler (Begin Function {ref rNote}\n  rP.AuxVarSetRef "*nvfJ.note" rNote -1\nEnd) 1\n'
        'SetFunctionValue 1'),
    # Three arguments [documented]; a handler taking four crashed the game when Health
    # changed [observed: jip_events2 crash, 2026-10-26].
    _je("je.reg.av", 'SetOnActorValueChangeEventHandler 1 (Begin Function {int iAV, float fOld, float fNew}\n'
        '  rP.AuxVarSetFlt "*nvfJ.av" iAV -1\n  rP.AuxVarSetFlt "*nvfJ.av.d" (fNew - fOld) -1\nEnd) 0\nSetFunctionValue 1'),
    # Causes.
    _je("je.do.fire", _form("rObj", NF["pistol10"]) + "rP.FireWeapon rObj\nSetFunctionValue 1"),
    _je("je.do.aid", _form("rObj", NF["stimpak"]) + "rP.EquipItem rObj 0 1\nSetFunctionValue 1"),
    _je("je.do.cripple", _JN + 'rB.DamageAV LeftAttackCondition 200\nSetFunctionValue 1'),
    _je("je.do.playgroup", _JN + "rB.PlayGroup Idle 1\nSetFunctionValue 1"),
    _je("je.do.note", _form("rObj", FX["note"]) + "rP.AddNote rObj\nSetFunctionValue 1"),
    _je("je.do.av", "rP.ModAV Health 5\nSetFunctionValue 1"),
    _je("je.do.kill", _JC + "rA.Kill rP\nSetFunctionValue 1"),
] + [_je(f"je.settle.after{i}", "SetFunctionValue 1") for i in range(4)] + [
    # Reads: whether each fired, and its first values.
    _jread("je.fire.fired", '(rP.AuxVarSize "*nvfJ.fire") > 0'),
    _jread("je.fire.self", 'rP.AuxVarGetFlt "*nvfJ.fire" 0'),
    _jread("je.fire.weapon", 'GetFormIDString (rP.AuxVarGetRef "*nvfJ.fire.w" 0)', True),
    _jread("je.aid.fired", '(rP.AuxVarSize "*nvfJ.aid") > 0'),
    _jread("je.aid.self", 'rP.AuxVarGetFlt "*nvfJ.aid" 0'),
    _jread("je.aid.item", 'GetFormIDString (rP.AuxVarGetRef "*nvfJ.aid.i" 0)', True),
    _jread("je.cripple.fired", '(rP.AuxVarSize "*nvfJ.cripple") > 0'),
    _jread("je.cripple.limb", 'rP.AuxVarGetFlt "*nvfJ.cripple" 0'),
    _jread("je.group.fired", '(rP.AuxVarSize "*nvfJ.group") > 0'),
    _jread("je.group.index", 'rP.AuxVarGetFlt "*nvfJ.group" 0'),
    _jread("je.ragdoll.fired", '(rP.AuxVarSize "*nvfJ.ragdoll") > 0'),
    _jread("je.ragdoll.self", 'rP.AuxVarGetFlt "*nvfJ.ragdoll" 0'),
    _jread("je.note.fired", '(rP.AuxVarSize "*nvfJ.note") > 0'),
    _jread("je.note.form", 'GetFormIDString (rP.AuxVarGetRef "*nvfJ.note" 0)', True),
    _jread("je.av.fired", '(rP.AuxVarSize "*nvfJ.av") > 0'),
    _jread("je.av.code", 'rP.AuxVarGetFlt "*nvfJ.av" 0'),
    _jread("je.av.delta", 'rP.AuxVarGetFlt "*nvfJ.av.d" 0'),
    _je("je.cleanup", _JC + _JN + "rA.Disable\nrA.MarkForDelete\nrB.Disable\nrB.MarkForDelete\nrP.ModAV Health -5\n"
        'rP.AuxVarErase "*nvfJC"\nrP.AuxVarErase "*nvfJN"\nSetFunctionValue 1'),
]

# Fortieth set (next batch N13): xNVSE and JIP leftovers. GetFalloutDirectory gives a
# path of the owner's machine, so the row keeps only whether it ends with the game
# folder's name and a backslash. Read-only.
_HL3 = ("ref rPlayer\nref rDoc\nref rObj\nfloat fRes\nstring_var sText\narray_var aA\nBegin Function { }\n"
        + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]))


def _lo(pid, body, text=False):
    return (pid, _HL3 + body, text, None, "")


N13_PROBES = [
    _lo("lo.paired", _form("rObj", NF["stimpak"]) + "SetFunctionValue (rPlayer.GetPaired rObj rDoc)"),
    _lo("lo.testexpr.ok", "SetFunctionValue (TestExpr (1 + 1))"),
    _lo("lo.testexpr.badindex", "let aA := ar_List 1 2\nSetFunctionValue (TestExpr aA[5])"),
    _lo("lo.testexpr.goodindex", "let aA := ar_List 1 2\nSetFunctionValue (TestExpr aA[1])"),
    _lo("lo.testexpr.nullref", "SetFunctionValue (TestExpr (rObj.GetName))"),
    _lo("lo.testexpr.uninitarray", "SetFunctionValue (TestExpr (ar_Size aA))"),
    _lo("lo.testexpr.stringkey", 'let aA := ar_Map "a"::1\nSetFunctionValue (TestExpr aA["b"])'),
    _lo("lo.falloutdir.end", 'let sText := GetFalloutDirectory\n'
        'SetFunctionValue (sv_Find "Fallout New Vegas\\" sText) == ((sv_Length sText) - 18)'),
    _lo("lo.falloutdir.len.positive", "let sText := GetFalloutDirectory\nSetFunctionValue ((sv_Length sText) > 0)"),
    _lo("lo.calcdamage.base", _form("rObj", NF["pistol10"]) + "SetFunctionValue (GetCalculatedWeaponDamage rObj)"),
    _lo("lo.calcdamage.hunting", _form("rObj", NF["hunting"]) + "SetFunctionValue (GetCalculatedWeaponDamage rObj)"),
    _lo("lo.questlinks.size", "let aA := GetCurrentQuestObjectiveTeleportLinks\nSetFunctionValue (ar_Size aA)"),
    _lo("lo.printvar", "let fRes := 2.5\nPrintVar fRes\nSetFunctionValue 1"),
    _lo("lo.printvar.many", 'let fRes := 2.5\nlet sText := "x"\nPrintVar fRes sText\nSetFunctionValue 1'),
    _lo("lo.debugprintvar", "let fRes := 2.5\nDebugPrintVar fRes\nSetFunctionValue 1"),
    _lo("lo.solditem", "SetFunctionValue (GetFormIDString (GetSoldItemInvRef))", True),
]

# Forty-first and forty-second sets (next batch N12): what extender state survives a save
# and load. save-set runs on nvfairport, sets everything and reads it back; the run then
# ends with the console line "save nvfprobe" (capture_run --quit-console). save-read runs
# on nvfprobe and reads it all again. The owner allowed this one save, removed after the
# run with its co-save files (owner-review, 2026-10-26). Aux variable kinds by name:
# plain (public), "_" (private to the mod), "*" (temporary) [documented: JIP's auxiliary
# variable page names the prefixes; which survive is what is probed]. Forms as in
# lu-leveled and sky-sound; ExtProbeLoops06Q (NVFExtProbe.esp B40) holds `int iR`.
_HS = ("ref rP\nref rDoc\nref rBase\nref rObj\nref rQ\nfloat fRes\nBegin Function { }\n" + _form("rP", NF["player"])
       + _form("rDoc", NF["docref"]) + _form("rBase", NF["doc"]) + 'let rQ := GetFormFromMod "NVFExtProbe.esp" "B40"\n')


def _sv(pid, body, text=False):
    return (pid, _HS + body, text, None, "")


# What both runs read, in the same order.
def _save_reads(tag):
    rows = []
    for pre in ("", "_", "*"):
        k = {"": "pub", "_": "priv", "*": "temp"}[pre]
        rows += [
            _sv(f"{tag}.aux.flt.{k}", f'SetFunctionValue (rP.AuxVarGetFlt "{pre}nvfSF")'),
            _sv(f"{tag}.aux.ref.{k}", f'SetFunctionValue (GetFormIDString (rP.AuxVarGetRef "{pre}nvfSR"))', True),
            _sv(f"{tag}.aux.str.{k}", f'SetFunctionValue (rP.AuxVarGetStr "{pre}nvfSS")', True),
            _sv(f"{tag}.refmap.{k}", f'SetFunctionValue (rDoc.RefMapGetFlt "{pre}nvfSM")'),
        ]
    rows += [
        _sv(f"{tag}.questvar", 'SetFunctionValue (GetVariable "iR" rQ)'),
        _sv(f"{tag}.controls.alt", "SetFunctionValue (GetPlayerControlsDisabledAlt 1)"),
        _sv(f"{tag}.controls.vanilla", "SetFunctionValue (GetPlayerControlsDisabled)"),
        _sv(f"{tag}.loop.keep", 'let fRes := rP.AuxVarGetFlt "nvfSL1"\nSetFunctionValue (fRes > 0)'),
        _sv(f"{tag}.loop.auto", 'let fRes := rP.AuxVarGetFlt "nvfSL8"\nSetFunctionValue (fRes > 0)'),
        _sv(f"{tag}.loop.keep.stamp", 'rP.AuxVarSetFlt "nvfSL1s" (rP.AuxVarGetFlt "nvfSL1")\nSetFunctionValue 1'),
        _sv(f"{tag}.loop.auto.stamp", 'rP.AuxVarSetFlt "nvfSL8s" (rP.AuxVarGetFlt "nvfSL8")\nSetFunctionValue 1'),
        _sv(f"{tag}.loop.wait1", "SetFunctionValue 1"),
        _sv(f"{tag}.loop.wait2", "SetFunctionValue 1"),
        _sv(f"{tag}.loop.keep.running", 'SetFunctionValue ((rP.AuxVarGetFlt "nvfSL1") > (rP.AuxVarGetFlt "nvfSL1s"))'),
        _sv(f"{tag}.loop.auto.running", 'SetFunctionValue ((rP.AuxVarGetFlt "nvfSL8") > (rP.AuxVarGetFlt "nvfSL8s"))'),
        _sv(f"{tag}.sound.trait3", _form("rObj", FX["beep"]) + "SetFunctionValue (GetSoundTraitNumeric rObj 3)"),
        _sv(f"{tag}.lvli.lu.num", _form("rObj", FX["lvli"]) + "SetFunctionValue (GetNumLevItems rObj)"),
        _sv(f"{tag}.lvln.showoff.num", _form("rObj", FX["lvln"]) + "SetFunctionValue (GetNumLevItems rObj)"),
        _sv(f"{tag}.lvlc.lu.num", _form("rObj", FX["lvlc"]) + "SetFunctionValue (GetNumLevItems rObj)"),
        _sv(f"{tag}.name", "SetFunctionValue (GetName rBase)", True),
        _sv(f"{tag}.karma", "SetFunctionValue (GetBaseKarma rBase)"),
    ]
    return rows


SAVE_SET_PROBES = (
    [_sv(f"set.aux.{pre or 'pub'}", f'rP.AuxVarSetFlt "{pre}nvfSF" 2.5\nrP.AuxVarSetRef "{pre}nvfSR" rDoc\n'
         f'rP.AuxVarSetStr "{pre}nvfSS" "kept?"\nrDoc.RefMapSetFlt "{pre}nvfSM" 4\nSetFunctionValue 1') for pre in ("", "_", "*")]
    + [_sv("set.questvar", 'SetVariable "iR" 7 rQ\nSetFunctionValue (GetVariable "iR" rQ)'),
       _sv("set.controls.alt", "DisablePlayerControlsAlt 0 0 0 1 0 0 0\nSetFunctionValue (GetPlayerControlsDisabledAlt 1)"),
       _sv("set.controls.vanilla", "DisablePlayerControls 0 0 0 0 0 0 1\nSetFunctionValue (GetPlayerControlsDisabled)"),
       _sv("set.loop.keep", 'SetGameMainLoopCallback (Begin Function { }\n  rP.AuxVarSetFlt "nvfSL1" ((rP.AuxVarGetFlt "nvfSL1") + 1)\nEnd) 1 1 1\n'
           "SetFunctionValue 1"),
       _sv("set.loop.auto", 'SetGameMainLoopCallback (Begin Function { }\n  rP.AuxVarSetFlt "nvfSL8" ((rP.AuxVarGetFlt "nvfSL8") + 1)\nEnd) 1 1 9\n'
           "SetFunctionValue 1"),
       _sv("set.sound", _form("rObj", FX["beep"]) + "SetSoundTraitNumeric rObj 3 7\nSetFunctionValue (GetSoundTraitNumeric rObj 3)"),
       _sv("set.lvli.lu", _form("rObj", FX["lvli"]) + _form("rQ", FX["ammo556"]) + "LeveledListAddForm rObj rQ 1 1 1.0\nSetFunctionValue (GetNumLevItems rObj)"),
       _sv("set.lvln.showoff", _form("rObj", FX["lvln"]) + "let rQ := GetNthLevItem rObj 0\nRemoveFormFromLeveledList rObj rQ\nSetFunctionValue (GetNumLevItems rObj)"),
       _sv("set.lvlc.lu", _form("rObj", FX["lvlc"]) + "let rQ := GetNthLevItem rObj 0\nLeveledListRemoveForm rObj rQ\nSetFunctionValue (GetNumLevItems rObj)"),
       _sv("set.name", 'LNSetName "NVF Saved Doc" rBase\nSetFunctionValue (GetName rBase)', True),
       _sv("set.karma", "SetBaseKarma 77 rBase\nSetFunctionValue (GetBaseKarma rBase)")]
    + [_sv(f"set.wait{i}", "SetFunctionValue 1") for i in range(3)]
    + _save_reads("before")
)

SAVE_READ_PROBES = (
    [_sv(f"read.wait{i}", "SetFunctionValue 1") for i in range(3)]
    + _save_reads("after")
    + [_sv("read.cleanup", "EnablePlayerControlsAlt\nEnablePlayerControls\nSetFunctionValue 1")]
)

# Forty-third set: SetWeatherRGBColor with out-of-range parts, each set from the same
# start (100100100) so the earlier value cannot show through, then read back; and
# SetUIFloat on traits HUDMainMenu has (read first), read back the same frame and later.
# The weather colour and the HUD trait are put back.
_RGB_CASES = [100100100, 100256100, 100300100, 100511100, 100512100, 100999100, 100100256, 100100300,
              100100999, 100300300, 100999999, 256100100, 999100100, 100255255, 100255256]
_HUD_TRAITS = ["visible", "alpha", "x", "y", "width", "height", "depth"]


def _ru(pid, body, text=False):
    return (pid, "ref rPlayer\nref rW8\nfloat fRes\nBegin Function { }\n" + _form("rPlayer", NF["player"])
            + _form("rW8", "171285") + body, text, None, "")


_RGBREAD = 'SetFunctionValue (sv_Construct "%.0f" (GetWeatherRGBColor rW8 3 1))'

RGB_UI_PROBES = (
    [_ru("rgb.save", 'rPlayer.AuxVarSetFlt "*nvfRC" (GetWeatherRGBColor rW8 3 1)\n' + _RGBREAD, True)]
    + [_ru(f"rgb.{v}", f"SetWeatherRGBColor rW8 3 1 100100100\nSetWeatherRGBColor rW8 3 1 {v}\n" + _RGBREAD, True) for v in _RGB_CASES]
    + [_ru("rgb.restore", 'SetWeatherRGBColor rW8 3 1 (rPlayer.AuxVarGetFlt "*nvfRC")\nrPlayer.AuxVarErase "*nvfRC"\n' + _RGBREAD, True)]
    + [_ru(f"ui.hud.{t}", f'SetFunctionValue (GetUIFloat "HUDMainMenu/{t}")') for t in _HUD_TRAITS]
    + [_ru("ui.alpha.save", 'rPlayer.AuxVarSetFlt "*nvfUA" (GetUIFloat "HUDMainMenu/alpha")\nSetFunctionValue 1'),
       _ru("ui.alpha.set", 'SetUIFloat "HUDMainMenu/alpha" 128\nSetFunctionValue (GetUIFloat "HUDMainMenu/alpha")'),
       _ru("ui.alpha.next", 'SetFunctionValue (GetUIFloat "HUDMainMenu/alpha")')]
    + [_ru(f"ui.alpha.wait{i}", "SetFunctionValue 1") for i in range(3)]
    + [_ru("ui.alpha.later", 'SetFunctionValue (GetUIFloat "HUDMainMenu/alpha")'),
       _ru("ui.alpha.restore", 'SetUIFloat "HUDMainMenu/alpha" (rPlayer.AuxVarGetFlt "*nvfUA")\nrPlayer.AuxVarErase "*nvfUA"\n'
           'SetFunctionValue (GetUIFloat "HUDMainMenu/alpha")'),
       _ru("ui.depth.set", 'let fRes := GetUIFloat "HUDMainMenu/depth"\nSetUIFloat "HUDMainMenu/depth" (fRes + 1)\n'
           'let fRes := (GetUIFloat "HUDMainMenu/depth") - fRes\nSetUIFloat "HUDMainMenu/depth" ((GetUIFloat "HUDMainMenu/depth") - 1)\n'
           "SetFunctionValue fRes"),
       _ru("ui.missingtrait.set", 'SetUIFloat "HUDMainMenu/nvfnone" 5\nSetFunctionValue (GetUIFloat "HUDMainMenu/nvfnone")')]
)

# Forty-fourth set: the functions of docs/handoff/modlist-scan-2026-10-26.md's 40 most
# used that no set had called: SetUIString, SetUIStringAlt, SetUIFloatAlt (on the HUD's
# hardcore Dehydration text, a path the GetUIString page names [documented], and on
# HUDMainMenu's alpha), GetLoadedTypeArray, GetCrosshairRef, StopSound (UIVATSLoop
# 15511B, fnvtool esm inventory [observed]). UI values are put back.
FX.update(vatsloop="15511B")
_DEHY = "HUDMainMenu/HardcoreMode/Dehydration/string"


def _g1(pid, body, text=False):
    return (pid, "ref rPlayer\nref rObj\nfloat fRes\nstring_var sText\narray_var aA\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + body, text, None, "")


GAPS1_PROBES = [
    # UI strings.
    _g1("ui.dehy.get", f'SetFunctionValue (GetUIString "{_DEHY}")', True),
    _g1("ui.dehy.get.backslash", 'SetFunctionValue (GetUIString "HUDMainMenu\\HardcoreMode\\Dehydration\\string")', True),
    _g1("ui.dehy.save", f'let sText := GetUIString "{_DEHY}"\nrPlayer.AuxVarSetStr "*nvfUS" sText\nSetFunctionValue 1'),
    _g1("ui.setuistring", f'SetUIString "{_DEHY}" "NVF one"\nSetFunctionValue (GetUIString "{_DEHY}")', True),
    _g1("ui.setuistringalt", f'SetFunctionValue (SetUIStringAlt "{_DEHY}" "NVF %g" 2)'),
    _g1("ui.setuistringalt.read", f'SetFunctionValue (GetUIString "{_DEHY}")', True),
    _g1("ui.setuistringalt.missing", 'SetFunctionValue (SetUIStringAlt "HUDMainMenu/NVFNone/string" "x")'),
    # "%z" with a string_var put the variable's name ("sText") in the trait in the game,
    # so a row reads that, and the text read first (H20) is put back as a literal.
    _g1("ui.stringalt.z", f'let sText := rPlayer.AuxVarGetStr "*nvfUS"\nSetUIStringAlt "{_DEHY}" "%z" sText\n'
        f'SetFunctionValue (GetUIString "{_DEHY}")', True),
    _g1("ui.dehy.restore", f'SetUIStringAlt "{_DEHY}" "H20"\nrPlayer.AuxVarErase "*nvfUS"\n'
        f'SetFunctionValue (GetUIString "{_DEHY}")', True),
    # UI float, Alt form.
    _g1("ui.floatalt.set", 'SetFunctionValue (SetUIFloatAlt "HUDMainMenu/alpha" 128)'),
    _g1("ui.floatalt.read", 'SetFunctionValue (GetUIFloat "HUDMainMenu/alpha")'),
    _g1("ui.floatalt.same", 'SetFunctionValue (SetUIFloatAlt "HUDMainMenu/alpha" 128)'),
    _g1("ui.floatalt.missing", 'SetFunctionValue (SetUIFloatAlt "HUDMainMenu/nvfnone" 1)'),
    _g1("ui.floatalt.restore", 'SetUIFloatAlt "HUDMainMenu/alpha" 255\nSetFunctionValue (GetUIFloat "HUDMainMenu/alpha")'),
    # GetLoadedTypeArray: form type 40 weapons, 47 ingestibles; all plugins and one.
    _g1("glta.weap", "let aA := GetLoadedTypeArray 40\nSetFunctionValue (ar_Size aA)"),
    _g1("glta.weap.esm", "let aA := GetLoadedTypeArray 40 0\nSetFunctionValue (ar_Size aA)"),
    _g1("glta.weap.first", "let aA := GetLoadedTypeArray 40 0\nlet rObj := aA[0]\nSetFunctionValue (GetFormIDString rObj)", True),
    _g1("glta.alch", "let aA := GetLoadedTypeArray 47\nSetFunctionValue (ar_Size aA)"),
    _g1("glta.probe", 'let aA := GetLoadedTypeArray 40 (GetModIndex "NVFExtProbe.esp")\nSetFunctionValue (ar_Size aA)'),
    _g1("glta.alias", "let aA := GLTA 47 0\nSetFunctionValue (ar_Size aA)"),
    # Crosshair.
    _g1("crosshair", "SetFunctionValue (GetFormIDString (GetCrosshairRef))", True),
    # StopSound on a looping sound.
    _g1("sound.loop.play", _form("rObj", FX["vatsloop"]) + "PlaySound rObj\nSetFunctionValue (IsSoundPlaying rObj)"),
    _g1("sound.loop.next", _form("rObj", FX["vatsloop"]) + "SetFunctionValue (IsSoundPlaying rObj)"),
    _g1("sound.loop.stop", _form("rObj", FX["vatsloop"]) + "StopSound rObj\nSetFunctionValue (IsSoundPlaying rObj)"),
    _g1("sound.loop.stopped", _form("rObj", FX["vatsloop"]) + "SetFunctionValue (IsSoundPlaying rObj)"),
]

# Forty-fifth set: the most used functions of docs/handoff/modlist-function-counts.json
# that were neither built nor probed, taking the ones a script can read or set safely
# (menus, pauses and dialogue left out), and whether a stimpak used by the player raises
# OnMagicEffectHit on the player (the extender core asked). Forms from fnvtool esm
# inventory [observed]: Ammo10mmJHPHandLoad 140AA8, AmmoEffectMagnumDTReduction 17BA46,
# VStreetCrBrahmin2 (CREA) 17B587, Toughness 031DE0, GameHour 000038,
# VLootAmmoBoxGen30 167815 holding 02FE5D. Changes are put back or session only.
FX.update(jhp="140AA8", amef="17BA46")


def _g2(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nfloat fRes\narray_var aA\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _g2v(pid, expr, text=False):
    return _g2(pid, f"SetFunctionValue ({expr})", text)


GAPS2_PROBES = [
    # Effects of an ingestible.
    _g2("eff.stim.num", _form("rObj", NF["stimpak"]) + "SetFunctionValue (GetNumEffects rObj)"),
    _g2("eff.stim.0", _form("rObj", NF["stimpak"]) + "SetFunctionValue (GetFormIDString (GetNthEffectBase rObj 0))", True),
    _g2("eff.stim.past", _form("rObj", NF["stimpak"]) + "SetFunctionValue (GetFormIDString (GetNthEffectBase rObj 9))", True),
    # Ammo effects.
    _g2("ammo.jhp.num", _form("rObj", FX["jhp"]) + "SetFunctionValue (GetNumAmmoEffects rObj)"),
    _g2("ammo.jhp.0", _form("rObj", FX["jhp"]) + "SetFunctionValue (GetFormIDString (GetNthAmmoEffect rObj 0))", True),
    _g2("ammo.10mm.num", _form("rObj", NF["ammo10"]) + "SetFunctionValue (GetNumAmmoEffects rObj)"),
] + [_g2(f"amef.trait.{i}", _form("rObj", FX["amef"]) + f"SetFunctionValue (GetAmmoEffectTraitNumeric rObj {i})") for i in range(4)] + [
    # Globals, perks, creatures.
    _g2("global.gamehour.int", _form("rObj", FX["gamehour"]) + "SetFunctionValue (Floor (GetGlobalVariable rObj))"),
] + [_g2(f"perkflag.toughness.{i}", _form("rObj", FX["toughness"]) + f"SetFunctionValue (GetPerkFlag rObj {i})") for i in range(5)] + [
    _g2("creaturetype.brahmin", _form("rObj", FX["brahmin"]) + "SetFunctionValue (GetCreatureType rObj)"),
    _g2v("creaturetype.doc", "rDoc.GetCreatureType"),
    _g2("pickupsound.stim", _form("rObj", NF["stimpak"]) + "SetFunctionValue (GetFormIDString (GetPickupSound rObj))", True),
    _g2("pickupsound.pistol", _form("rObj", NF["pistol10"]) + "SetFunctionValue (GetFormIDString (GetPickupSound rObj))", True),
    _g2v("optionalpatch.none", 'GetOptionalPatch "NVFNoSuchPatch"'),
    # Inventory.
    _g2v("invweight.player", "Floor (rPlayer.GetInventoryWeight)"),
    _g2v("invweight.player.playable", "Floor (rPlayer.GetInventoryWeight 1)"),
    _g2("allitems.player", "let aA := rPlayer.GetAllItems\nSetFunctionValue (ar_Size aA)"),
    _g2("allitems.player.weapons", "let aA := rPlayer.GetAllItems 40\nSetFunctionValue (ar_Size aA)"),
    _g2("equippeditemref.none", "SetFunctionValue (GetFormIDString (rPlayer.GetEquippedItemRef 5))", True),
    _g2("dps.pistol", _form("rObj", NF["pistol10"]) + "SetFunctionValue (Floor ((rPlayer.GetCalculatedWeaponDPS rObj) * 100))"),
    _g2("basecontainer.remove", _form("rObj", FX["ammobox"]) + _form("rOther", FX["boxlist"])
        + "BaseRemoveItem rOther 1 rObj\nSetFunctionValue 1"),
    # Actor values, teammates, distance, combat.
    _g2v("avmod.player.health", 'rPlayer.GetActorValueModifier Health'),
    _g2("teammates", "let aA := GetTeammates\nSetFunctionValue (ar_Size aA)"),
    _g2v("distance2d.doc", "Floor (rPlayer.GetDistance2D rDoc)"),
    _g2v("crosshairwater", "GetFormIDString (GetCrosshairWater)", True),
    # Names and toggles, put back.
    _g2("fullnamealt", 'let fRes := 1\nrDoc.SetFullNameAlt "NVF Alt Doc"\nSetFunctionValue (rDoc.GetName)', True),
    _g2("fullnamealt.base", _form("rObj", NF["doc"]) + 'SetFunctionValue (GetName rObj)', True),
    _g2("fullnamealt.restore", 'rDoc.SetFullNameAlt "Doc Mitchell"\nSetFunctionValue (rDoc.GetName)', True),
    _g2v("pipboylight.state", "TogglePipBoyLight"),
    _g2("pipboylight.on", "TogglePipBoyLight 1\nSetFunctionValue (TogglePipBoyLight)"),
    _g2("pipboylight.off", "TogglePipBoyLight 0\nSetFunctionValue (TogglePipBoyLight)"),
    _g2v("pipboylight.off.next", "TogglePipBoyLight"),
    _g2("combatdisabled", "SetFunctionValue (rDoc.SetCombatDisabled 1)"),
    _g2("combatdisabled.again", "SetFunctionValue (rDoc.SetCombatDisabled 0)"),
    _g2v("hasscriptblock.doc.gamemode", "rDoc.HasScriptBlock 0"),
    _g2v("isanimplayingex.player", "rPlayer.IsAnimPlayingEx 0 0 0"),
    # Does a stimpak the player uses raise OnMagicEffectHit on the player?
    _g2("mgef.reg", 'SetEventHandler "OnMagicEffectHit" (Begin Function {ref rE1, ref rE2}\n'
        '  rPlayer.AuxVarSetFlt "*nvfMG" (rE1 == rPlayer) -1\n  rPlayer.AuxVarSetRef "*nvfMG.e" rE2 -1\nEnd)\nSetFunctionValue 1'),
    _g2("mgef.damage", 'rPlayer.DamageAV Health 30\nSetFunctionValue 1'),
    _g2("mgef.use", _form("rObj", NF["stimpak"]) + "rPlayer.EquipItem rObj 0 1\nSetFunctionValue 1"),
] + [_g2(f"mgef.wait{i}", "SetFunctionValue 1") for i in range(3)] + [
    _g2v("mgef.fired", '(rPlayer.AuxVarSize "*nvfMG") > 0'),
    _g2v("mgef.onplayer", 'rPlayer.AuxVarGetFlt "*nvfMG" 0'),
    _g2v("mgef.effect", 'GetFormIDString (rPlayer.AuxVarGetRef "*nvfMG.e" 0)', True),
    _g2("mgef.cleanup", 'rPlayer.RestoreAV Health 100\nrPlayer.AuxVarErase "*nvfMG"\nrPlayer.AuxVarErase "*nvfMG.e"\nSetFunctionValue 1'),
]

# Forty-sixth set: the next unbuilt, unprobed functions of
# docs/handoff/modlist-function-counts.json that a script can read or set safely. Left
# out: menus and the cursor's movement, pauses, FakeScriptEvent (runs another script's
# block), output-variable forms (WorldToScreen), 3D-only reads. The base effect is the
# stimpak's first (0000014E, gaps2); TimeScale is FalloutNV.esm 00003A. Changes are put
# back or session only.
def _g3(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nfloat fRes\nstring_var sText\narray_var aA\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _g3v(pid, expr, text=False):
    return _g3(pid, f"SetFunctionValue ({expr})", text)


_MGEF = _form("rObj", "14E")
GAPS3_PROBES = [
    _g3v("menu.active", "GetActiveMenuMode"),
    _g3v("time.multiplier", "GetGlobalTimeMultiplier"),
] + [_g3(f"actors.level{i}", f"let aA := GetActorsByProcessingLevel {i}\nSetFunctionValue (ar_Size aA)") for i in range(3)] + [
    # Level 3 holds about 2,000 actors and its count moved by one between two captures.
    _g3("actors.level3.any", "let aA := GetActorsByProcessingLevel 3\nSetFunctionValue ((ar_Size aA) > 1000)"),
    _g3("folders.nvse", 'let aA := GetFoldersInFolder "NVSE"\nSetFunctionValue (ar_Size aA)'),
    _g3("folders.nvse.first", 'let aA := GetFoldersInFolder "NVSE"\nlet sText := aA[0]\nSetFunctionValue sText', True),
    _g3("folders.none", 'let aA := GetFoldersInFolder "NVFNoFolder"\nSetFunctionValue (ar_Size aA)'),
    _g3v("ground.player", "rPlayer.GetGroundMaterial"),
    _g3v("ragdolled.player", "rPlayer.GetIsRagdolled"),
    _g3(f"perk.entries", _form("rObj", FX["toughness"]) + "SetFunctionValue (GetPerkEntryCount rObj)"),
    _g3(f"perk.entry0.type", _form("rObj", FX["toughness"]) + "SetFunctionValue (GetNthPerkEntryType rObj 0)"),
    _g3("mgef.archetype", _MGEF + "SetFunctionValue (GetBaseEffectArchetype rObj)"),
    _g3("mgef.av", _MGEF + "SetFunctionValue (GetBaseEffectAV rObj)"),
] + [_g3(f"mgef.flag.{i}", _MGEF + f"SetFunctionValue (GetBaseEffectFlag rObj {i})") for i in range(4)] + [
] + [_g3(f"effectflag.stim.{i}", _form("rObj", NF["stimpak"]) + f"SetFunctionValue (GetEffectFlag rObj {i})") for i in range(3)] + [
] + [_g3(f"cellflag.doc.{i}", _form("rObj", CELLS["doc"]) + f"SetFunctionValue (GetCellFlag rObj {i})") for i in range(4)] + [
    _g3("keywordforms", _form("rObj", NF["doc"]) + 'AssignKeyword rObj "NVFGapKey"\nlet aA := GetKeywordForms "NVFGapKey"\nSetFunctionValue (ar_Size aA)'),
    _g3("keywordforms.none", 'let aA := GetKeywordForms "NVFNoKey"\nSetFunctionValue (ar_Size aA)'),
    _g3("global.set", _form("rObj", "3A") + 'let fRes := GetGlobalVariable rObj\nrPlayer.AuxVarSetFlt "*nvfGV" fRes\n'
        "SetFunctionValue (SetGlobalVariable rObj 12)"),
    _g3("global.set.read", _form("rObj", "3A") + "SetFunctionValue (GetGlobalVariable rObj)"),
    _g3("global.restore", _form("rObj", "3A") + 'SetGlobalVariable rObj (rPlayer.AuxVarGetFlt "*nvfGV")\nrPlayer.AuxVarErase "*nvfGV"\n'
        "SetFunctionValue (GetGlobalVariable rObj)"),
    _g3v("combatdisabled.before", "rDoc.GetCombatDisabled"),
    _g3("combatdisabled.set", "rDoc.SetCombatDisabled 1\nSetFunctionValue (rDoc.GetCombatDisabled)"),
    _g3("combatdisabled.clear", "rDoc.SetCombatDisabled 0\nSetFunctionValue (rDoc.GetCombatDisabled)"),
    _g3("effectshader.none", _form("rObj", "14E") + "SetFunctionValue (rPlayer.HasEffectShader rObj)"),
    _g3("destruction.pistol", _form("rObj", NF["pistol10"]) + "SetFunctionValue (GetDestructionDataNumStages rObj)"),
    _g3("cursor.x", "SetFunctionValue (GetCursorPos X)"),
    _g3("biped.slot2", "SetFunctionValue (ToggleBipedSlotVisibility 2)"),
    _g3("reloadmodels", "rPlayer.ReloadEquippedModels\nSetFunctionValue 1"),
    # SetWeaponOut on the player with the pistol equipped.
    _g3("weaponout.add", _form("rObj", NF["pistol10"]) + "rPlayer.AddItem rObj 1 1\nrPlayer.EquipItem rObj 0 1\nSetFunctionValue 1"),
    _g3("weaponout.draw", "rPlayer.SetWeaponOut 1\nSetFunctionValue (rPlayer.IsWeaponOut)"),
] + [_g3(f"weaponout.wait{i}", "SetFunctionValue 1") for i in range(5)] + [
    _g3v("weaponout.drawn", "rPlayer.IsWeaponOut"),
    _g3("weaponout.holster", "rPlayer.SetWeaponOut 0\nSetFunctionValue 1"),
] + [_g3(f"weaponout.wait.h{i}", "SetFunctionValue 1") for i in range(5)] + [
    _g3v("weaponout.holstered", "rPlayer.IsWeaponOut"),
    _g3("weaponout.cleanup", _form("rObj", NF["pistol10"]) + "rPlayer.UnequipItem rObj 0 1\nrPlayer.RemoveItem rObj 1 1\nSetFunctionValue 1"),
    # DropAlt.
    _g3("dropalt", _form("rObj", NF["stimpak"]) + "let fRes := rPlayer.GetItemCount rObj\nrPlayer.DropAlt rObj 1\nSetFunctionValue fRes"),
    _g3v("dropalt.after", "rPlayer.GetItemCount (GetFormFromMod \"FalloutNV.esm\" \"15169\")"),
]

# Forty-seventh set: the next unbuilt, unprobed functions of
# docs/handoff/modlist-function-counts.json read or set safely. Climate rows name a climate
# (NVtheStripClimate 161242) so they do not depend on the save; gaps4.climate.current is
# the one save row. Forms from fnvtool esm inventory [observed]: Recipe127mm 17BA3E,
# musDocMitchellHouse (MUSC) 171B4A, VStreetCrBrahmin2 (CREA) 17B587. Changes put back.
FX.update(stripclimate="161242", recipe127="17BA3E", musdoc="171B4A")


def _g4(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nfloat fRes\narray_var aA\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _g4v(pid, expr, text=False):
    return _g4(pid, f"SetFunctionValue ({expr})", text)


_CLM = _form("rObj", FX["stripclimate"])
GAPS4_PROBES = [
    _g4v("laa", "GetIsLAA"),
    _g4v("climate.current", "GetFormIDString (GetCurrentClimate)", True),
    _g4("climate.num", _CLM + "SetFunctionValue (GetClimateNumWeatherTypes rObj)"),
    _g4("climate.weather0", _CLM + "SetFunctionValue (GetFormIDString (GetClimateNthWeatherType rObj 0))", True),
    _g4("climate.global0", _CLM + "SetFunctionValue (GetFormIDString (GetClimateNthWeatherGlobal rObj 0))", True),
    _g4("climate.weather.past", _CLM + "SetFunctionValue (GetFormIDString (GetClimateNthWeatherType rObj 99))", True),
    _g4("isnight.strip", _CLM + "SetFunctionValue (IsNight rObj)"),
    _g4("bodypart.brahmin", _form("rObj", FX["brahmin"]) + "SetFunctionValue (GetFormIDString (GetBodyPartData rObj))", True),
    _g4("formrefs.stimpak", _form("rObj", NF["stimpak"]) + "let aA := GetFormRefs rObj\nSetFunctionValue (ar_Size aA)"),
    _g4("formrefs.docbase", _form("rObj", NF["doc"]) + "let aA := GetFormRefs rObj 1\nSetFunctionValue (ar_Size aA)"),
    _g4v("falltime.player", "rPlayer.GetFallTimeElapsed"),
    _g4v("attacking.player", "rPlayer.IsAttacking"),
    _g4v("combatwith.doc", "rDoc.IsInCombatWith rPlayer"),
    _g4v("crimeorenemy.doc", "rDoc.IsCrimeOrEnemy"),
    _g4v("runspeed.player.int", "Floor (rPlayer.GetRunSpeed)"),
    _g4v("activequest", "GetFormIDString (GetActiveQuest)", True),
    _g4(f"perk.entry0.function", _form("rObj", FX["toughness"]) + "SetFunctionValue (GetNthPerkEntryFunction rObj 0)"),
    _g4("cell.imagespace.doc", _form("rObj", CELLS["doc"]) + "SetFunctionValue (GetFormIDString (GetCellImageSpace rObj))", True),
    _g4("recipe.outputs", _form("rObj", FX["recipe127"]) + "let aA := GetRecipeOutputForms rObj\nSetFunctionValue (ar_Size aA)"),
    _g4("recipe.output0", _form("rObj", FX["recipe127"]) + "let aA := GetRecipeOutputForms rObj\nlet rOther := aA[0]\n"
        "SetFunctionValue (GetFormIDString rOther)", True),
    _g4("music.playing", _form("rObj", FX["musdoc"]) + "SetFunctionValue (IsMusicPlaying rObj)"),
    _g4v("music.playing.any", "IsMusicPlaying"),
    # Weapon flags and sounds, put back.
] + [_g4(f"weaponflag.pistol.{i}", _form("rObj", NF["pistol10"]) + f"SetFunctionValue (WeaponHasFlag {i} rObj)") for i in range(6)] + [
    _g4("weaponflag.set", _form("rObj", NF["pistol10"]) + "let fRes := WeaponHasFlag 5 rObj\nSetWeaponFlag 5 1 rObj\n"
        "let fRes := fRes * 10 + (WeaponHasFlag 5 rObj)\nSetWeaponFlag 5 0 rObj\nSetFunctionValue fRes"),
] + [_g4(f"weaponsound.pistol.{i}", _form("rObj", NF["pistol10"]) + f"SetFunctionValue (GetFormIDString (GetWeaponSound rObj {i}))", True)
     for i in range(4)] + [
    _g4("weaponsound.set", _form("rObj", NF["pistol10"]) + _form("rOther", FX["beep"])
        + 'let rDoc := GetWeaponSound rObj 0\nrPlayer.AuxVarSetRef "*nvfWS" rDoc\nSetWeaponSound rObj 0 rOther\n'
        "SetFunctionValue (GetFormIDString (GetWeaponSound rObj 0))", True),
    _g4("weaponsound.restore", _form("rObj", NF["pistol10"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfWS"\nSetWeaponSound rObj 0 rOther\n'
        'rPlayer.AuxVarErase "*nvfWS"\nSetFunctionValue (GetFormIDString (GetWeaponSound rObj 0))', True),
    # Sound flags.
    _g4("soundflag.set", _form("rObj", FX["beep"]) + "SetFunctionValue (SetSoundFlag rObj 0 1)"),
    _g4("soundflag.clear", _form("rObj", FX["beep"]) + "SetFunctionValue (SetSoundFlag rObj 0 0)"),
    _g4("stopsoundalt.none", _form("rObj", FX["beep"]) + "SetFunctionValue (StopSoundAlt rObj rPlayer)"),
    # Reference names and gravity.
    _g4("refname.set", 'rDoc.SetRefName "NVF Ref Doc"\nSetFunctionValue (rDoc.GetName)', True),
    _g4("refname.base", _form("rObj", NF["doc"]) + "SetFunctionValue (GetName rObj)", True),
    _g4("gravity.set", "rPlayer.SetActorGravityMult 1\nSetFunctionValue 1"),
    _g4v("optionalpatch.set.none", 'SetOptionalPatch "NVFNoSuchPatch" 1'),
]

# Forty-eighth set: what the extender core asked after building gaps2-4: the save's Guns
# skill and hardcore state (DPS and the stimpak's effects depend on them), weapon sound
# IDs 3-11 on weapons that have more sounds (WeapMinigun 00433F, WeapGatlingLaser 00432E,
# WeapFlamer 00432D), perk entry types and functions on more perks, GetActorValueModifier
# after damage by type filter, and GetRunSpeed sneaking and armoured. Forms from
# fnvtool esm inventory [observed]. Damage, sneak and armour are put back.
_G5_PERKS = {"leadbelly": "44CA9", "intensetraining": "44CB1", "educated": "31DD8", "radresistance": "31DA9",
             "smallframe": "135EC6", "commando": "99828"}


def _g5(pid, body, text=False):
    return (pid, "ref rPlayer\nref rObj\nfloat fRes\nBegin Function { }\n" + _form("rPlayer", NF["player"]) + body, text, None, "")


def _g5v(pid, expr, text=False):
    return _g5(pid, f"SetFunctionValue ({expr})", text)


GAPS5_PROBES = (
    [_g5v("save.guns", 'rPlayer.GetActorValue "Guns"'),
     _g5v("save.guns.base", 'rPlayer.GetBaseActorValue "Guns"'),
     _g5v("save.hardcore", "rPlayer.IsHardcore"),
     _g5v("save.level", "rPlayer.GetLevel")]
    + [_g5(f"weapsound.{w}.{i}", _form("rObj", fid) + f"SetFunctionValue (GetFormIDString (GetWeaponSound rObj {i}))", True)
       for w, fid in (("minigun", "433F"), ("gatling", "432E"), ("flamer", "432D")) for i in range(12)]
    + [_g5(f"perk.{k}.{what}.{i}", _form("rObj", fid) + f"SetFunctionValue ({fn} rObj {i})")
       for k, fid in _G5_PERKS.items() for i in range(2)
       for what, fn in (("type", "GetNthPerkEntryType"), ("function", "GetNthPerkEntryFunction"))]
    + [_g5(f"perk.{k}.count", _form("rObj", fid) + "SetFunctionValue (GetPerkEntryCount rObj)") for k, fid in _G5_PERKS.items()]
    # Actor value modifiers after damage.
    + [_g5v(f"avmod.before.{f}", f"rPlayer.GetActorValueModifier Health {f}") for f in (0, 1, 2, 3)]
    + [_g5("avmod.damage", "rPlayer.DamageAV Health 30\nSetFunctionValue (rPlayer.GetAV Health)")]
    + [_g5v(f"avmod.after.{f}", f"rPlayer.GetActorValueModifier Health {f}") for f in (0, 1, 2, 3)]
    + [_g5("avmod.restore", "rPlayer.RestoreAV Health 30\nSetFunctionValue (rPlayer.GetActorValueModifier Health)")]
    # Run speed: standing, sneaking, armoured.
    + [_g5v("run.base", "Floor (rPlayer.GetRunSpeed)"),
       _g5("run.sneak.on", "rPlayer.SetForceSneak 1\nSetFunctionValue 1")]
    + [_g5(f"run.sneak.wait{i}", "SetFunctionValue 1") for i in range(3)]
    + [_g5v("run.sneak", "Floor (rPlayer.GetRunSpeed)"),
       _g5v("run.sneak.issneaking", "rPlayer.IsSneaking"),
       _g5("run.sneak.off", "rPlayer.SetForceSneak 0\nSetFunctionValue 1")]
    + [_g5(f"run.sneak.offwait{i}", "SetFunctionValue 1") for i in range(3)]
    + [_g5v("run.sneak.after", "Floor (rPlayer.GetRunSpeed)"),
       _g5("run.armour.on", _form("rObj", NF["leather"]) + "rPlayer.AddItem rObj 1 1\nrPlayer.EquipItem rObj 0 1\nSetFunctionValue 1")]
    + [_g5(f"run.armour.wait{i}", "SetFunctionValue 1") for i in range(2)]
    + [_g5v("run.armour", "Floor (rPlayer.GetRunSpeed)"),
       _g5("run.armour.off", _form("rObj", NF["leather"]) + "rPlayer.UnequipItem rObj 0 1\nrPlayer.RemoveItem rObj 1 1\nSetFunctionValue 1")]
    + [_g5(f"run.armour.offwait{i}", "SetFunctionValue 1") for i in range(2)]
    + [_g5v("run.armour.after", "Floor (rPlayer.GetRunSpeed)")]
)

# Forty-ninth set: GetCalculatedWeaponDPS on the skill curve (the 10mm pistol at Guns
# 17, 50 and 100; WeapKnifeCombat 004326 at the save's skills and at Melee Weapons 50),
# as the extender core asked, then the next unprobed JIP functions of the counts that
# have a getter or a visible effect. The 10mm pistol's projectile is 02CD5F (projectile
# set); MineFragExplosion 07078B; EnchFourEyesHat 17B78D. Skills, values, flags and the
# player's limb are put back; the perk description and the pistol's effect are session
# only (a string_var cannot be passed back, gaps1).
FX.update(knife="4326", fourenc="17B78D", pistolproj="2CD5F")


def _g6(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nfloat fRes\nstring_var sText\narray_var aA\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _g6v(pid, expr, text=False):
    return _g6(pid, f"SetFunctionValue ({expr})", text)


_DPS = "SetFunctionValue (Floor ((rPlayer.GetCalculatedWeaponDPS rObj) * 100))"
GAPS6_PROBES = [
    # DPS on the skill curve. SetActorValue on a skill read back doubled in a first run
    # (17 read 34), so each point records GetActorValue and GetBaseActorValue too, and
    # the skill is put back with ModActorValue by the difference from the value read first.
    _g6("dps.guns.save", 'let fRes := rPlayer.GetActorValue "Guns"\nrPlayer.AuxVarSetFlt "*nvfGS" fRes\nSetFunctionValue fRes'),
    _g6("dps.pistol.start", _form("rObj", NF["pistol10"]) + _DPS),
    _g6("dps.guns.set50", 'rPlayer.SetActorValue "Guns" 50\n'
        'SetFunctionValue (sv_Construct "%g %g" (rPlayer.GetActorValue "Guns") (rPlayer.GetBaseActorValue "Guns"))', True),
    _g6("dps.pistol.at50", _form("rObj", NF["pistol10"]) + _DPS),
    _g6("dps.guns.set100", 'rPlayer.SetActorValue "Guns" 100\n'
        'SetFunctionValue (sv_Construct "%g %g" (rPlayer.GetActorValue "Guns") (rPlayer.GetBaseActorValue "Guns"))', True),
    _g6("dps.pistol.at100", _form("rObj", NF["pistol10"]) + _DPS),
    _g6("dps.guns.restore", 'rPlayer.ModActorValue "Guns" ((rPlayer.AuxVarGetFlt "*nvfGS") - (rPlayer.GetActorValue "Guns"))\n'
        'rPlayer.AuxVarErase "*nvfGS"\nSetFunctionValue (rPlayer.GetActorValue "Guns")'),
    _g6("dps.pistol.again", _form("rObj", NF["pistol10"]) + _DPS),
    _g6("dps.melee.save", 'let fRes := rPlayer.GetActorValue "MeleeWeapons"\nrPlayer.AuxVarSetFlt "*nvfMS" fRes\nSetFunctionValue fRes'),
    _g6("dps.knife", _form("rObj", FX["knife"]) + _DPS),
    _g6("dps.knife.damage", _form("rObj", FX["knife"]) + "SetFunctionValue (Floor ((GetCalculatedWeaponDamage rObj) * 100))"),
    _g6("dps.melee.mod16", 'rPlayer.ModActorValue "MeleeWeapons" 16\nSetFunctionValue (rPlayer.GetActorValue "MeleeWeapons")'),
    _g6("dps.knife.at.mod16", _form("rObj", FX["knife"]) + _DPS),
    _g6("dps.melee.restore", 'rPlayer.ModActorValue "MeleeWeapons" ((rPlayer.AuxVarGetFlt "*nvfMS") - (rPlayer.GetActorValue "MeleeWeapons"))\n'
        'rPlayer.AuxVarErase "*nvfMS"\nSetFunctionValue (rPlayer.GetActorValue "MeleeWeapons")'),
    # Effect traits.
] + [_g6(f"efftrait.stim.0.{t}", _form("rObj", NF["stimpak"]) + f"SetFunctionValue (GetNthEffectTraitNumeric rObj 0 {t})") for t in range(4)] + [
    _g6("efftrait.set", _form("rObj", NF["stimpak"]) + "let fRes := GetNthEffectTraitNumeric rObj 0 0\nrPlayer.AuxVarSetFlt \"*nvfET\" fRes\n"
        "SetFunctionValue (SetNthEffectTraitNumeric rObj 0 0 77)"),
    _g6("efftrait.set.read", _form("rObj", NF["stimpak"]) + "SetFunctionValue (GetNthEffectTraitNumeric rObj 0 0)"),
    _g6("efftrait.restore", _form("rObj", NF["stimpak"]) + 'SetNthEffectTraitNumeric rObj 0 0 (rPlayer.AuxVarGetFlt "*nvfET")\n'
        'rPlayer.AuxVarErase "*nvfET"\nSetFunctionValue (GetNthEffectTraitNumeric rObj 0 0)'),
    # Description, value, object effect.
    _g6("desc.toughness", _form("rObj", FX["toughness"]) + "SetFunctionValue (sv_Length (GetFormDescription rObj))"),
    _g6("desc.set", _form("rObj", FX["toughness"]) + 'SetFunctionValue (SetFormDescription rObj "NVF desc %g" 3)'),
    _g6("desc.set.read", _form("rObj", FX["toughness"]) + "SetFunctionValue (GetFormDescription rObj)", True),
    # SetValueAlt returned 0 in a first run, not the old value: the value is read first.
    _g6("value.set", _form("rObj", NF["pistol10"]) + 'let fRes := GetValue rObj\nrPlayer.AuxVarSetFlt "*nvfVA" fRes\n'
        "SetFunctionValue (SetValueAlt rObj 999)"),
    _g6("value.set.read", _form("rObj", NF["pistol10"]) + "SetFunctionValue (GetValue rObj)"),
    _g6("value.restore", _form("rObj", NF["pistol10"]) + 'SetValueAlt rObj (rPlayer.AuxVarGetFlt "*nvfVA")\nrPlayer.AuxVarErase "*nvfVA"\n'
        "SetFunctionValue (GetValue rObj)"),
    _g6("objeffect.pistol", _form("rObj", NF["pistol10"]) + "SetFunctionValue (GetFormIDString (GetObjectEffect rObj))", True),
    _g6("objeffect.set", _form("rObj", NF["pistol10"]) + _form("rOther", FX["fourenc"]) + "SetFunctionValue (SetObjectEffect rObj rOther)"),
    _g6("objeffect.set.read", _form("rObj", NF["pistol10"]) + "SetFunctionValue (GetFormIDString (GetObjectEffect rObj))", True),
    # Projectile flags and explosion.
] + [_g6(f"projflag.pistol.{i}", _form("rObj", FX["pistolproj"]) + f"SetFunctionValue (GetProjectileFlag rObj {i})") for i in range(6)] + [
    _g6("projflag.set", _form("rObj", FX["pistolproj"]) + "let fRes := GetProjectileFlag rObj 3\nSetProjectileFlag rObj 3 1\n"
        "let fRes := fRes * 10 + (GetProjectileFlag rObj 3)\nSetProjectileFlag rObj 3 0\nSetFunctionValue fRes"),
    _g6("projexplosion", _form("rObj", FX["pistolproj"]) + "SetFunctionValue (GetFormIDString (GetProjectileExplosion rObj))", True),
    _g6("projexplosion.set", _form("rObj", FX["pistolproj"]) + _form("rOther", "7078B")
        + 'let rDoc := GetProjectileExplosion rObj\nrPlayer.AuxVarSetRef "*nvfPX" rDoc\n'
        "SetProjectileExplosion rObj rOther\nSetFunctionValue (GetFormIDString (GetProjectileExplosion rObj))", True),
    _g6("projexplosion.restore", _form("rObj", FX["pistolproj"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfPX"\n'
        'SetProjectileExplosion rObj rOther\nrPlayer.AuxVarErase "*nvfPX"\n'
        "SetFunctionValue (GetFormIDString (GetProjectileExplosion rObj))", True),
    # Aux variables as arrays; ref maps of refs.
    _g6("auxarray.get", 'rPlayer.AuxVarSetFlt "*nvfAA" 1 0\nrPlayer.AuxVarSetFlt "*nvfAA" 2 1\n'
        'let aA := rPlayer.AuxiliaryVariableGetAsArray "*nvfAA"\nSetFunctionValue (ar_Size aA)'),
    _g6("auxarray.set", 'let aA := ar_List 5 6 7\nlet fRes := rPlayer.AuxiliaryVariableSetFromArray "*nvfAB" aA\n'
        'SetFunctionValue (sv_Construct "%g %g %g" fRes (rPlayer.AuxVarSize "*nvfAB") (rPlayer.AuxVarGetFlt "*nvfAB" 2))', True),
    _g6("refmap.setref", 'rDoc.RefMapArraySetRef "*nvfRR" rPlayer\nSetFunctionValue (GetFormIDString (rDoc.RefMapArrayGetRef "*nvfRR"))', True),
    _g6("refmap.erase", 'SetFunctionValue (rDoc.RefMapArrayErase "*nvfRR")'),
    _g6("refmap.erased", 'SetFunctionValue (GetFormIDString (rDoc.RefMapArrayGetRef "*nvfRR"))', True),
    # Actor states.
    _g6v("poisoned.player", "rPlayer.GetIsPoisoned"),
    _g6v("detection.pc", "GetPCDetectionState"),
    _g6v("proclevel.player", "rPlayer.GetActorProcessingLevel"),
    _g6v("proclevel.doc", "rDoc.GetActorProcessingLevel"),
    _g6v("persistent.doc", "rDoc.SetPersistent 1"),
    _g6("cripple.player", "rPlayer.CrippleLimb 2\nSetFunctionValue (rPlayer.GetActorValue LeftAttackCondition)"),
    _g6("cripple.restore", "rPlayer.RestoreAV LeftAttackCondition 100\nSetFunctionValue (rPlayer.GetActorValue LeftAttackCondition)"),
    _g6("stick.disable", "SetStickDisabled 0 1\nSetStickDisabled 0 0\nSetFunctionValue 1"),
    _g6v("uicomponent.hud", 'IsComponentLoaded "HUDMainMenu"'),
    _g6v("uicomponent.none", 'IsComponentLoaded "NVFNoComponent"'),
]

# Fiftieth set: the critical part of GetCalculatedWeaponDPS, as the extender core asked
# (the player's crit actor values, each weapon's crit damage and chance, DPS with crit
# damage 0, crit chance 0 and Luck +5, all put back), then the next unprobed JIP functions
# of the counts with a getter or a visible effect. Forms from fnvtool esm inventory
# [observed]: 40mmGrenadeExplosion 07EA31, BloodyMess 094EBA, SilverSting 167475,
# NukaVendingMachine (CONT) 020525, ElectricBox01 020D31, CrDogVicious 01CFA3,
# Gomorrah00 (CELL) 10D629, VCG01DocMitchellSandbox (PACK) 10B756, LegateArmorRightH
# (ARMA) 1649DF. The explosion, the perk, the weapons and the actor values are put back.
# GetAngleEx and GetAnglesBetweenPoints did not compile here: their outputs are script
# variables, which a runtime-compiled function does not take [observed].
# GetDroppedRefs, GetPlayerPerks, GetFollowers, GetTempEffects and GetDetectingActors read
# ar_Size -1 here: no array at all when there is nothing to list [observed]. ModAV Luck +5
# left CritChance and DPS unchanged after two frames; ModAV CritChance +10 doubled the crit
# part of both weapons' DPS [observed].
FX.update(gren40="7EA31", bloodymess="94EBA", silversting="167475", nuka="20525", ebox="20D31",
          vicious="1CFA3", gomorrah="10D629", docsandbox="10B756", legatearma="1649DF")


def _g7(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nfloat fRes\nfloat fAx\nfloat fAy\nfloat fAz\narray_var aA\n"
            "Begin Function { }\n" + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _g7v(pid, expr, text=False):
    return _g7(pid, f"SetFunctionValue ({expr})", text)


def _g7w(pid, key, expr, text=False):
    return _g7(pid, _form("rObj", key) + f"SetFunctionValue ({expr})", text)


_DPS7 = "SetFunctionValue (Floor ((rPlayer.GetCalculatedWeaponDPS rObj) * 100))"


def _crit(w, fid):
    """Crit rows for one weapon: its crit fields, DPS, then DPS with crit damage 0 and crit chance 0."""
    return [
        _g7w(f"crit.{w}.critdamage", fid, "GetWeaponCritDamage rObj"),
        _g7w(f"crit.{w}.critchance", fid, "Floor ((GetWeaponCritChance rObj) * 1000)"),
        _g7w(f"crit.{w}.damage", fid, "Floor ((GetCalculatedWeaponDamage rObj) * 100)"),
        _g7(f"crit.{w}.dps", _form("rObj", fid) + _DPS7),
        _g7(f"crit.{w}.dmg0", _form("rObj", fid) + 'let fRes := GetWeaponCritDamage rObj\nrPlayer.AuxVarSetFlt "*nvfCD" fRes\n'
            "SetWeaponCritDamage 0 rObj\n" + _DPS7),
        _g7(f"crit.{w}.dmg0.restore", _form("rObj", fid) + 'SetWeaponCritDamage (rPlayer.AuxVarGetFlt "*nvfCD") rObj\n'
            'rPlayer.AuxVarErase "*nvfCD"\nSetFunctionValue (GetWeaponCritDamage rObj)'),
        _g7(f"crit.{w}.chance0", _form("rObj", fid) + 'let fRes := GetWeaponCritChance rObj\nrPlayer.AuxVarSetFlt "*nvfCC" fRes\n'
            "SetWeaponCritChance 0 rObj\n" + _DPS7),
        _g7(f"crit.{w}.chance0.restore", _form("rObj", fid) + 'SetWeaponCritChance (rPlayer.AuxVarGetFlt "*nvfCC") rObj\n'
            'rPlayer.AuxVarErase "*nvfCC"\nSetFunctionValue (Floor ((GetWeaponCritChance rObj) * 1000))'),
        _g7(f"crit.{w}.dps.again", _form("rObj", fid) + _DPS7),
    ]


GAPS7_PROBES = (
    [_g7v(f"crit.av.{av.lower()}", f'Floor ((rPlayer.GetActorValue "{av}") * 100)')
     for av in ("CritChance", "Luck", "Strength", "MeleeDamage", "UnarmedDamage", "Agility")]
    + _crit("pistol", NF["pistol10"]) + _crit("knife", FX["knife"])
    + [_g7("crit.luck.mod5", 'rPlayer.ModActorValue "Luck" 5\n'
           'SetFunctionValue (sv_Construct "%g %g" (rPlayer.GetActorValue "Luck") (Floor ((rPlayer.GetActorValue "CritChance") * 100)))', True),
       _g7("crit.luck.wait0", "SetFunctionValue 1"),
       _g7("crit.luck.wait1", "SetFunctionValue 1"),
       _g7v("crit.luck.critchance", 'Floor ((rPlayer.GetActorValue "CritChance") * 100)'),
       _g7("crit.pistol.luck5", _form("rObj", NF["pistol10"]) + _DPS7),
       _g7("crit.knife.luck5", _form("rObj", FX["knife"]) + _DPS7),
       _g7("crit.cc.mod10", 'rPlayer.ModActorValue "CritChance" 10\n'
           'SetFunctionValue (Floor ((rPlayer.GetActorValue "CritChance") * 100))'),
       _g7("crit.pistol.cc20", _form("rObj", NF["pistol10"]) + _DPS7),
       _g7("crit.knife.cc20", _form("rObj", FX["knife"]) + _DPS7),
       _g7("crit.cc.restore", 'rPlayer.ModActorValue "CritChance" -10\n'
           'SetFunctionValue (Floor ((rPlayer.GetActorValue "CritChance") * 100))'),
       _g7("crit.luck.restore", 'rPlayer.ModActorValue "Luck" -5\n'
           'SetFunctionValue (sv_Construct "%g %g" (rPlayer.GetActorValue "Luck") (Floor ((rPlayer.GetActorValue "CritChance") * 100)))', True)]
    # Dimensions and terrain.
    + [_g7v(f"objdim.doc.{a}", f"Floor ((rDoc.GetObjectDimensions {a}) * 100)") for a in "XYZ"]
    + [_g7w(f"objdim.nuka.{a}", FX["nuka"], f"Floor ((GetObjectDimensions {a} rObj) * 100)") for a in "XYZ"]
    + [_g7v("terrain.player", "Floor (GetTerrainHeight (rPlayer.GetPos X) (rPlayer.GetPos Y))"),
       _g7v("terrain.far", "Floor (GetTerrainHeight 900000 900000)")]
    # Lists of things.
    + [_g7("baseitems.nuka", _form("rObj", FX["nuka"]) + "let aA := GetBaseItems rObj\nSetFunctionValue (ar_Size aA)"),
       _g7("baseitems.doc", "let aA := rDoc.GetBaseItems\nSetFunctionValue (ar_Size aA)"),
       _g7("droppedrefs.doc", "let aA := rDoc.GetDroppedRefs\nSetFunctionValue (ar_Size aA)"),
       _g7("playerperks", "let aA := GetPlayerPerks\nSetFunctionValue (ar_Size aA)"),
       _g7("followers.player", "let aA := rPlayer.GetFollowers\nSetFunctionValue (ar_Size aA)"),
       _g7("tempeffects.player", "let aA := rPlayer.GetTempEffects\nSetFunctionValue (ar_Size aA)"),
       _g7("detecting.player", "let aA := rPlayer.GetDetectingActors\nSetFunctionValue (ar_Size aA)"),
       _g7("pickfromlist.ammo10", _form("rObj", FX["ammolist10"]) + "let rOther := rPlayer.PickFromList rObj\nSetFunctionValue (GetFormIDString rOther)", True),
       _g7("pickfromlist.list127", _form("rObj", NF["list127"]) + "let rOther := rPlayer.PickFromList rObj\nSetFunctionValue (GetFormIDString rOther)", True)]
    # Item and form kinds.
    + [_g7w("ispoison.silversting", FX["silversting"], "GetIsPoison rObj"),
       _g7w("ispoison.stimpak", NF["stimpak"], "GetIsPoison rObj"),
       _g7w("isarmoraddon.legate", FX["legatearma"], "IsArmorAddon rObj"),
       _g7w("isarmoraddon.leather", NF["leather"], "IsArmorAddon rObj"),
       _g7w("isitem.stimpak", NF["stimpak"], "GetIsItem rObj"),
       _g7w("isitem.nuka", FX["nuka"], "GetIsItem rObj"),
       _g7v("isitem.docref", "rDoc.GetIsItem"),
       _g7w("respawns.nuka", FX["nuka"], "GetContainerRespawns rObj"),
       _g7w("destruction.ebox", FX["ebox"], "GetDestructionDataHealth rObj"),
       _g7w("skeleton.vicious", FX["vicious"], "GetSkeletonModel rObj", True),
       _g7w("skeleton.doc", NF["doc"], "GetSkeletonModel rObj", True),
       _g7w("cellowner.gomorrah", FX["gomorrah"], "GetFormIDString (GetOwnerOfCell rObj)", True),
       _g7w("cellowner.doc", CELLS["doc"], "GetFormIDString (GetOwnerOfCell rObj)", True)]
    + [_g7w(f"packageflag.docsandbox.{i}", FX["docsandbox"], f"GetPackageFlag rObj {i}") for i in (0, 1, 2, 9, 10, 13, 14, 21)]
    # Actor and player state.
    + [_g7v("hctracking", "GetHardcoreTracking"),
       _g7v("alwaysrun", "GetAlwaysRun"),
       _g7v("pcincombat", "IsPCInCombat"),
       _g7v("encumbrance.player", "Floor ((rPlayer.GetEncumbranceRate) * 100)"),
       _g7v("immobile.doc", "rDoc.GetIsImmobile"),
       _g7w("immobile.ebox", FX["ebox"], "GetIsImmobile rObj"),
       _g7v("mobile.doc", "rDoc.IsMobile"),
       _g7v("lastextdoor", "GetFormIDString (GetPCLastExteriorDoor)", True),
       _g7v("leveledbase.doc", "GetFormIDString (rDoc.GetLeveledActorBase)", True),
       _g7v("lightamount.player", "Floor (rPlayer.GetActorLightAmount)"),
       _g7("resetfalltime", "rPlayer.ResetFallTime\nSetFunctionValue (rPlayer.GetFallTimeElapsed)")]
    # Explosion flags and forms, put back.
    + [_g7w(f"explflag.gren40.{i}", FX["gren40"], f"GetExplosionFlag rObj {i}") for i in range(1, 7)]
    + [_g7w("explflag.set", FX["gren40"], "SetExplosionFlag rObj 4 (1 - (GetExplosionFlag rObj 4))"),
       _g7w("explflag.set.read", FX["gren40"], "GetExplosionFlag rObj 4"),
       _g7w("explflag.restore", FX["gren40"], "SetExplosionFlag rObj 4 (1 - (GetExplosionFlag rObj 4))")]
    + [_g7w(f"expltrait.gren40.{i}", FX["gren40"], f"GetFormIDString (GetExplosionTraitForm rObj {i})", True) for i in range(5)]
    + [_g7("expltrait.set", _form("rObj", FX["gren40"]) + _form("rOther", NF["stimpak"])
           + 'let rDoc := GetExplosionTraitForm rObj 4\nrPlayer.AuxVarSetRef "*nvfXT" rDoc\nSetExplosionTraitForm rObj 4 rOther\n'
           "SetFunctionValue (GetFormIDString (GetExplosionTraitForm rObj 4))", True),
       _g7("expltrait.restore", _form("rObj", FX["gren40"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfXT"\n'
           "if rOther\nSetExplosionTraitForm rObj 4 rOther\nelse\nSetExplosionTraitForm rObj 4\nendif\n"
           'rPlayer.AuxVarErase "*nvfXT"\n'
           "SetFunctionValue (GetFormIDString (GetExplosionTraitForm rObj 4))", True)]
    # Perk level and flags, put back.
    + [_g7w("perklevel.bloodymess", FX["bloodymess"], "GetPerkLevel rObj")]
    + [_g7w(f"perkflag.bloodymess.{i}", FX["bloodymess"], f"GetPerkFlag rObj {i}") for i in range(3)]
    + [_g7("perklevel.set", _form("rObj", FX["bloodymess"]) + 'let fRes := GetPerkLevel rObj\nrPlayer.AuxVarSetFlt "*nvfPL" fRes\n'
           "SetPerkLevel rObj 8\nSetFunctionValue (GetPerkLevel rObj)"),
       _g7("perklevel.restore", _form("rObj", FX["bloodymess"]) + 'SetPerkLevel rObj (rPlayer.AuxVarGetFlt "*nvfPL")\n'
           'rPlayer.AuxVarErase "*nvfPL"\nSetFunctionValue (GetPerkLevel rObj)'),
       _g7w("perkflag.set", FX["bloodymess"], "SetPerkFlag rObj 2 1"),
       _g7w("perkflag.set.read", FX["bloodymess"], "GetPerkFlag rObj 2"),
       _g7w("perkflag.restore", FX["bloodymess"], "SetPerkFlag rObj 2 0"),
       _g7v("pcmovedcell", "GetPCMovedCell")]
)

# Fifty-first set: the next JIP setters of the counts that have a getter to read them back
# (worldspace climate and flags, climate textures, armour audio templates and model paths,
# destruction health, container respawns, package flags, interaction), and a few getters.
# WastelandNV (WRLD) 0DA726 from fnvtool esm inventory [observed]. Everything is put back
# except the leather armour's world model path and the climate's sun glare texture, which
# are session only (a string_var cannot be passed back, gaps1). SetDestructionDataHealth,
# SetContainerRespawns and SetPackageFlag returned 0 here, not the new or old value;
# SetArmorAudioTemplate with no form left the old template in place [observed].
FX.update(wasteland="DA726")


def _g8(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nfloat fRes\narray_var aA\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _g8v(pid, expr, text=False):
    return _g8(pid, f"SetFunctionValue ({expr})", text)


def _g8w(pid, key, expr, text=False):
    return _g8(pid, _form("rObj", key) + f"SetFunctionValue ({expr})", text)


def _restore_ref(setter, aux):
    """Put back a form saved in aux variable `aux` with `setter rObj <form>`, or none."""
    return (f'let rOther := rPlayer.AuxVarGetRef "{aux}"\nif rOther\n{setter} rOther\nelse\n{setter}\nendif\n'
            f'rPlayer.AuxVarErase "{aux}"\n')


GAPS8_PROBES = (
    # Worldspace climate, set and put back.
    [_g8w("wrldclimate.wasteland", FX["wasteland"], "GetFormIDString (GetWorldspaceClimate rObj)", True),
     _g8("wrldclimate.set", _form("rObj", FX["wasteland"]) + _form("rOther", FX["stripclimate"])
         + 'let rDoc := GetWorldspaceClimate rObj\nrPlayer.AuxVarSetRef "*nvfWC" rDoc\nSetWorldspaceClimate rObj rOther\n'
         "SetFunctionValue (GetFormIDString (GetWorldspaceClimate rObj))", True),
     _g8("wrldclimate.restore", _form("rObj", FX["wasteland"]) + _restore_ref("SetWorldspaceClimate rObj", "*nvfWC")
         + "SetFunctionValue (GetFormIDString (GetWorldspaceClimate rObj))", True),
     _g8v("refreshclimate", "RefreshCurrentClimate"),
     _g8v("climate.current.after", "GetFormIDString (GetCurrentClimate)", True)]
    # Worldspace flags.
    + [_g8w(f"wrldflag.wasteland.{i}", FX["wasteland"], f"GetWorldspaceFlag rObj {i}") for i in range(8)]
    + [_g8w("wrldflag.set", FX["wasteland"], "SetWorldspaceFlag rObj 1 (1 - (GetWorldspaceFlag rObj 1))"),
       _g8w("wrldflag.set.read", FX["wasteland"], "GetWorldspaceFlag rObj 1"),
       _g8w("wrldflag.restore", FX["wasteland"], "SetWorldspaceFlag rObj 1 (1 - (GetWorldspaceFlag rObj 1))"),
       _g8w("wrldflag.restore.read", FX["wasteland"], "GetWorldspaceFlag rObj 1")]
    # Climate textures.
    + [_g8w("climate.sunglare", FX["stripclimate"], "GetClimateSunGlareTexture rObj", True),
       _g8w("climate.nightsky", FX["stripclimate"], "GetClimateNightSkyModel rObj", True),
       _g8("climate.sunglare.set", _form("rObj", FX["stripclimate"])
           + 'SetClimateSunGlareTexture rObj "NVF\\Glare.dds"\nSetFunctionValue (GetClimateSunGlareTexture rObj)', True)]
    # Armour: audio templates and model paths.
    + [_g8w(f"armoraudio.{k}", fid, "GetFormIDString (GetArmorAudioTemplate rObj)", True)
       for k, fid in (("leather", NF["leather"]), ("hazmat", FX["hazmat"]))]
    + [_g8("armoraudio.set", _form("rObj", NF["leather"]) + _form("rOther", FX["hazmat"])
           + 'let rDoc := GetArmorAudioTemplate rObj\nrPlayer.AuxVarSetRef "*nvfAT" rDoc\n'
           "let rOther := GetArmorAudioTemplate rOther\nSetArmorAudioTemplate rObj rOther\n"
           "SetFunctionValue (GetFormIDString (GetArmorAudioTemplate rObj))", True),
       _g8("armoraudio.restore", _form("rObj", NF["leather"]) + _restore_ref("SetArmorAudioTemplate rObj", "*nvfAT")
           + "SetFunctionValue (GetFormIDString (GetArmorAudioTemplate rObj))", True)]
    + [_g8w(f"bipedpath.leather.{i}", NF["leather"], f"GetBipedModelPath {i} rObj", True) for i in range(4)]
    + [_g8("bipedpath.set", _form("rObj", NF["leather"])
           + 'SetBipedModelPathAlt rObj 2 "NVF\\Ground%g.nif" 7\nSetFunctionValue (GetBipedModelPath 2 rObj)', True)]
    # Destruction health, container respawns, package flags.
    + [_g8("destruction.set", _form("rObj", FX["ebox"]) + 'let fRes := GetDestructionDataHealth rObj\nrPlayer.AuxVarSetFlt "*nvfDH" fRes\n'
           "SetFunctionValue (SetDestructionDataHealth rObj 75)"),
       _g8w("destruction.set.read", FX["ebox"], "GetDestructionDataHealth rObj"),
       _g8("destruction.restore", _form("rObj", FX["ebox"]) + 'SetDestructionDataHealth rObj (rPlayer.AuxVarGetFlt "*nvfDH")\n'
           'rPlayer.AuxVarErase "*nvfDH"\nSetFunctionValue (GetDestructionDataHealth rObj)'),
       _g8w("respawns.set", FX["nuka"], "SetContainerRespawns 1 rObj"),
       _g8w("respawns.set.read", FX["nuka"], "GetContainerRespawns rObj"),
       _g8w("respawns.restore", FX["nuka"], "SetContainerRespawns 0 rObj"),
       _g8w("respawns.restore.read", FX["nuka"], "GetContainerRespawns rObj"),
       _g8w("packageflag.set", FX["docsandbox"], "SetPackageFlag rObj 10 1"),
       _g8w("packageflag.set.read", FX["docsandbox"], "GetPackageFlag rObj 10"),
       _g8w("packageflag.restore", FX["docsandbox"], "SetPackageFlag rObj 10 0"),
       _g8w("packageflag.restore.read", FX["docsandbox"], "GetPackageFlag rObj 10")]
    # Interaction on a reference, put back.
    + [_g8v("interaction.doc", "rDoc.GetInteractionDisabled"),
       _g8("interaction.set", "rDoc.SetInteractionDisabled 1\nSetFunctionValue (rDoc.GetInteractionDisabled)"),
       _g8("interaction.restore", "rDoc.SetInteractionDisabled 0\nSetFunctionValue (rDoc.GetInteractionDisabled)"),
       _g8w("interaction.base", NF["doc"], "GetInteractionDisabled rObj")]
    # Getters.
    + [_g8v("mine.doc", "rDoc.GetMineArmed"),
       _g8v("modtype.player.2", "rPlayer.EquippedWeaponHasModType 2"),
       _g8v("ammorounds.player", "rPlayer.GetCurrentAmmoRounds"),
       _g8v("reticlerange.int", "Floor (GetReticleRange)")]
)

# Fifty-second set: melee and unarmed DPS, as the extender core asked after gaps7 (the
# knife's rate and crit part did not fit the record). For WeapKnifeCombat 004326,
# WeapNVMachete 0CE569, WeapSledgehammer 004351, Fists 0001F4 and WeapBrassKnuckles
# 004324 [observed, fnvtool esm inventory]: the record fields the formula may use, then
# GetCalculatedWeaponDamage and GetCalculatedWeaponDPS at the save's skills, with crit
# damage 0, with CritChance +10, and with the weapon's skill +16. Everything is put back.
# GetWeaponSpeed did not compile here [observed]; GetWeaponAnimMult is read instead.
_G9_WEAPONS = {"knife": "4326", "machete": "CE569", "sledge": "4351", "fists": "1F4", "knuckles": "4324"}
_G9_FIELDS = (("critdamage", "GetWeaponCritDamage rObj"), ("critmult", "Floor ((GetWeaponCritChance rObj) * 1000)"),
              ("attackdamage", "GetAttackDamage rObj"),
              ("reach", "Floor ((GetWeaponReach rObj) * 1000)"), ("animmult", "Floor ((GetWeaponAnimMult rObj) * 1000)"),
              ("animattackmult", "Floor ((GetWeaponAnimAttackMult rObj) * 1000)"),
              ("attackanim", "GetWeaponAttackAnimation rObj"), ("skill", "GetWeaponSkill rObj"), ("type", "GetWeaponType rObj"))


def _g9(pid, body, text=False):
    return (pid, "ref rPlayer\nref rObj\nfloat fRes\nBegin Function { }\n" + _form("rPlayer", NF["player"]) + body, text, None, "")


def _g9v(pid, expr, text=False):
    return _g9(pid, f"SetFunctionValue ({expr})", text)


_DPS9 = "SetFunctionValue (Floor ((rPlayer.GetCalculatedWeaponDPS rObj) * 100))"
_DMG9 = "SetFunctionValue (Floor ((GetCalculatedWeaponDamage rObj) * 100))"


def _g9w(w):
    fid = _G9_WEAPONS[w]
    return (
        [_g9(f"{w}.{k}", _form("rObj", fid) + f"SetFunctionValue ({e})") for k, e in _G9_FIELDS]
        + [_g9(f"{w}.damage", _form("rObj", fid) + _DMG9),
           _g9(f"{w}.dps", _form("rObj", fid) + _DPS9),
           _g9(f"{w}.dmg0", _form("rObj", fid) + 'let fRes := GetWeaponCritDamage rObj\nrPlayer.AuxVarSetFlt "*nvfCD" fRes\n'
               "SetWeaponCritDamage 0 rObj\n" + _DPS9),
           _g9(f"{w}.dmg0.restore", _form("rObj", fid) + 'SetWeaponCritDamage (rPlayer.AuxVarGetFlt "*nvfCD") rObj\n'
               'rPlayer.AuxVarErase "*nvfCD"\nSetFunctionValue (GetWeaponCritDamage rObj)')]
    )


_SKILLS = (("MeleeWeapons", ("knife", "machete", "sledge")), ("Unarmed", ("fists", "knuckles")))
GAPS9_PROBES = (
    [_g9v(f"melee.av.{av.lower()}", f'Floor ((rPlayer.GetActorValue "{av}") * 100)')
     for av in ("MeleeWeapons", "Unarmed", "Strength", "Agility", "MeleeDamage", "UnarmedDamage", "CritChance")]
    + [r for w in _G9_WEAPONS for r in _g9w(w)]
    # CritChance +10.
    + [_g9("cc.mod10", 'rPlayer.ModActorValue "CritChance" 10\nSetFunctionValue (Floor ((rPlayer.GetActorValue "CritChance") * 100))')]
    + [_g9(f"{w}.cc20", _form("rObj", fid) + _DPS9) for w, fid in _G9_WEAPONS.items()]
    + [_g9("cc.restore", 'rPlayer.ModActorValue "CritChance" -10\nSetFunctionValue (Floor ((rPlayer.GetActorValue "CritChance") * 100))')]
    # Each weapon's skill +16: damage and DPS, then crit damage 0 again.
    + [r for av, ws in _SKILLS for r in (
        [_g9(f"skill.{av.lower()}.mod16", f'rPlayer.ModActorValue "{av}" 16\nSetFunctionValue (rPlayer.GetActorValue "{av}")')]
        + [r2 for w in ws for r2 in (
            _g9(f"{w}.s16.damage", _form("rObj", _G9_WEAPONS[w]) + _DMG9),
            _g9(f"{w}.s16.dps", _form("rObj", _G9_WEAPONS[w]) + _DPS9),
            _g9(f"{w}.s16.dmg0", _form("rObj", _G9_WEAPONS[w]) + 'let fRes := GetWeaponCritDamage rObj\nrPlayer.AuxVarSetFlt "*nvfCD" fRes\n'
                "SetWeaponCritDamage 0 rObj\n" + _DPS9),
            _g9(f"{w}.s16.dmg0.restore", _form("rObj", _G9_WEAPONS[w]) + 'SetWeaponCritDamage (rPlayer.AuxVarGetFlt "*nvfCD") rObj\n'
                'rPlayer.AuxVarErase "*nvfCD"\nSetFunctionValue (GetWeaponCritDamage rObj)'))]
        + [_g9(f"skill.{av.lower()}.restore", f'rPlayer.ModActorValue "{av}" -16\nSetFunctionValue (rPlayer.GetActorValue "{av}")')])]
)

# Fifty-third set: the next JIP functions of the counts with something to read back: ref
# map strings, RunScriptSnippet, arrays written to and read from a file (nvfprobe_arr.txt
# in the game folder, removed after the run), impact data sets and models (the 40mm
# grenade explosion's impact data set 093BA4, gaps7), shell casings, worldspace image
# spaces, ray casts, SetPosEx, tilt angles, effect bases, sneaking and first person.
# Everything is put back except the dirt impact model and shell casing paths (session only).
# IsThirdPerson is ppNVSE (not installed); IsPC1stPerson is read instead.


def _g10(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nfloat fRes\nfloat fAx\nfloat fAy\nfloat fAz\narray_var aA\n"
            "Begin Function { }\n" + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _g10v(pid, expr, text=False):
    return _g10(pid, f"SetFunctionValue ({expr})", text)


def _g10w(pid, key, expr, text=False):
    return _g10(pid, _form("rObj", key) + f"SetFunctionValue ({expr})", text)


def _wait10(pid):
    return _g10(pid, "SetFunctionValue 1")


FX.update(ipds40="93BA4")
_POS = "let fAx := rPlayer.GetPos X\nlet fAy := rPlayer.GetPos Y\nlet fAz := rPlayer.GetPos Z\n"
GAPS10_PROBES = (
    # Ref map strings.
    [_g10("refmap.setstring", 'rDoc.RefMapArraySetString "*nvfRS" "NVF str"\nSetFunctionValue (rDoc.RefMapArrayGetString "*nvfRS")', True),
     _g10("refmap.getstring.none", 'SetFunctionValue (rPlayer.RefMapArrayGetString "*nvfRS")', True),
     _g10("refmap.string.erase", 'rDoc.RefMapArrayErase "*nvfRS"\nSetFunctionValue (rDoc.RefMapArrayGetString "*nvfRS")', True),
     # RunScriptSnippet on the player, put back.
     _g10("snippet.luck", 'RunScriptSnippet 0 "ModAV Luck 1"\nSetFunctionValue (rPlayer.GetActorValue Luck)'),
     _g10("snippet.luck.restore", 'RunScriptSnippet 0 "ModAV Luck -1"\nSetFunctionValue (rPlayer.GetActorValue Luck)'),
     # Arrays to and from a file.
     _g10("arrayfile.write", 'let aA := ar_List 4 5 6\nSetFunctionValue (WriteArrayToFile "nvfprobe_arr.txt" 0 aA)'),
     _g10("arrayfile.read.size", 'let aA := ReadArrayFromFile "nvfprobe_arr.txt"\nSetFunctionValue (ar_Size aA)'),
     _g10("arrayfile.read.1", 'let aA := ReadArrayFromFile "nvfprobe_arr.txt"\nlet fRes := aA[1]\nSetFunctionValue fRes'),
     _g10("arrayfile.append", 'let aA := ar_List 7 8 9\nlet fRes := WriteArrayToFile "nvfprobe_arr.txt" 1 aA\n'
          'let aA := ReadArrayFromFile "nvfprobe_arr.txt"\nSetFunctionValue (ar_Size aA)'),
     _g10("arrayfile.missing", 'let aA := ReadArrayFromFile "nvfprobe_none.txt"\nSetFunctionValue (ar_Size aA)')]
    # Impact data sets and models.
    + [_g10w(f"ipds.gren40.{i}", FX["ipds40"], f"GetFormIDString (GetImpactDataSetForm rObj {i})", True) for i in range(12)]
    + [_g10("ipct.model.0", _form("rObj", FX["ipds40"]) + "let rOther := GetImpactDataSetForm rObj 0\n"
            "SetFunctionValue (GetImpactDataModel rOther)", True),
       _g10("ipds.set", _form("rObj", FX["ipds40"]) + 'let rOther := GetImpactDataSetForm rObj 0\nrPlayer.AuxVarSetRef "*nvfID" rOther\n'
            "let rOther := GetImpactDataSetForm rObj 4\nSetImpactDataSetForm rObj 0 rOther\n"
            "SetFunctionValue (GetFormIDString (GetImpactDataSetForm rObj 0))", True),
       _g10("ipds.restore", _form("rObj", FX["ipds40"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfID"\nSetImpactDataSetForm rObj 0 rOther\n'
            'rPlayer.AuxVarErase "*nvfID"\nSetFunctionValue (GetFormIDString (GetImpactDataSetForm rObj 0))', True),
       _g10("ipct.model.set", _form("rObj", FX["ipds40"]) + "let rOther := GetImpactDataSetForm rObj 0\n"
            'SetImpactDataModel rOther "NVF\\Impact.nif"\nSetFunctionValue (GetImpactDataModel rOther)', True),
       _g10w("shell.pistol", NF["pistol10"], "GetWeaponShellCasingModel rObj", True),
       _g10("shell.set", _form("rObj", NF["pistol10"]) + 'SetWeaponShellCasingModel rObj "NVF\\Shell.nif"\n'
            "SetFunctionValue (GetWeaponShellCasingModel rObj)", True),
       _g10w("wrldimagespace.wasteland", FX["wasteland"], "GetFormIDString (GetWorldspaceImagespace rObj)", True)]
    # Ray cast straight up from above the player, and from far away.
    + [_g10("p2p.up", _POS + "SetFunctionValue (Floor (GetP2PRayCastRange fAx fAy (fAz + 100) fAx fAy (fAz + 20000) 0))"),
       _g10("p2p.down", _POS + "SetFunctionValue (Floor (GetP2PRayCastRange fAx fAy (fAz + 100) fAx fAy (fAz - 1000) 0))")]
    # SetPosEx on Doc, translated and put back.
    + [_g10("posex.doc", 'let fAx := rDoc.GetPos X\nrPlayer.AuxVarSetFlt "*nvfPX" fAx\nrDoc.SetPosEx 10 0 0 1\n'
            'SetFunctionValue (Floor ((rDoc.GetPos X) - fAx + 0.5))'),
       _g10("posex.doc.restore", 'rDoc.SetPosEx -10 0 0 1\nlet fAx := rPlayer.AuxVarGetFlt "*nvfPX"\nrPlayer.AuxVarErase "*nvfPX"\n'
            'SetFunctionValue (Floor ((rDoc.GetPos X) - fAx + 0.5))')]
    # Tilt angle on Doc, put back.
    + [_g10v("tilt.doc.x", "Floor ((rDoc.GetActorTiltAngle X) * 100)"),
       _g10("tilt.doc.set", "rDoc.SetActorTiltAngle X 20\nSetFunctionValue (Floor ((rDoc.GetActorTiltAngle X) * 100))"),
       _g10("tilt.doc.restore", "rDoc.SetActorTiltAngle X 0\nSetFunctionValue (Floor ((rDoc.GetActorTiltAngle X) * 100))")]
    # Effect base of the stimpak's first effect, put back.
    + [_g10w("effbase.stimpak.0", NF["stimpak"], "GetFormIDString (GetNthEffectBase rObj 0)", True),
       _g10("effbase.set", _form("rObj", NF["stimpak"]) + _form("rOther", FX["silversting"])
            + 'let rDoc := GetNthEffectBase rObj 0\nrPlayer.AuxVarSetRef "*nvfEB" rDoc\nlet rOther := GetNthEffectBase rOther 0\n'
            "let rOther := SetNthEffectBase rObj 0 rOther\nSetFunctionValue (GetFormIDString rOther)", True),
       _g10w("effbase.set.read", NF["stimpak"], "GetFormIDString (GetNthEffectBase rObj 0)", True),
       _g10("effbase.restore", _form("rObj", NF["stimpak"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfEB"\nSetNthEffectBase rObj 0 rOther\n'
            'rPlayer.AuxVarErase "*nvfEB"\nSetFunctionValue (GetFormIDString (GetNthEffectBase rObj 0))', True)]
    # Sneaking and first person, put back.
    + [_g10("sneak.toggle.on", "TogglePlayerSneaking 1\nSetFunctionValue 1"), _wait10("sneak.wait0"), _wait10("sneak.wait1"),
       _g10v("sneak.on", "rPlayer.IsSneaking"),
       _g10("sneak.toggle.off", "TogglePlayerSneaking 0\nSetFunctionValue 1"), _wait10("sneak.wait2"), _wait10("sneak.wait3"),
       _g10v("sneak.off", "rPlayer.IsSneaking"),
       _g10v("view.first", "IsPC1stPerson"),
       _g10("view.toggle", 'let fRes := IsPC1stPerson\nrPlayer.AuxVarSetFlt "*nvfTP" fRes\nToggleFirstPerson (1 - fRes)\nSetFunctionValue 1'),
       _wait10("view.wait0"), _wait10("view.wait1"),
       _g10v("view.first.after", "IsPC1stPerson"),
       _g10("view.restore", 'let fRes := rPlayer.AuxVarGetFlt "*nvfTP"\nToggleFirstPerson fRes\nrPlayer.AuxVarErase "*nvfTP"\nSetFunctionValue 1'),
       _wait10("view.wait2"), _wait10("view.wait3"),
       _g10v("view.first.restored", "IsPC1stPerson")]
)

SETS = {"jip-getters": PROBES, "jip-forms": FORM_PROBES, "jip-stores": STORE_PROBES, "jip-events": EVENT_PROBES,
        "jip-hits": HIT_PROBES, "jip-records": RECORD_PROBES, "nvse-forms": NVSE_FORM_PROBES, "nvse-vars": VAR_PROBES,
        "nvse-refs": REF_PROBES, "nvse-controls": CONTROL_PROBES, "nvse-actors": ACTOR_PROBES,
        "nvse-setters": SETTER_PROBES, "nvse-misc": MISC_PROBES, "nvse-math": MATH_PROBES,
        "nvse-data": DATA_PROBES, "nvse-inv": INV_PROBES, "nvse-second": SECOND_PROBES,
        "nvse-keynames": KEYNAME_PROBES, "nvse-third": THIRD_PROBES, "nvse-fourth": FOURTH_PROBES,
        "jip-fifth": FIFTH_PROBES, "nvse-fifth": NVSE_FIFTH_PROBES, "nvse-sixth": SIXTH_PROBES,
        "nvse-seventh": SEVENTH_PROBES,
        "nvse-eighth": EIGHTH_PROBES,
        "lu-leveled": LEVELED_PROBES,
        "nvse-flags": FLAG_PROBES,
        "ini-files": INI_FILE_PROBES,
        "jip-n7": N7_PROBES,
        "player-state": PLAYER_STATE_PROBES,
        "mcm": MCM_PROBES,
        "plugin-odds": N9_PROBES,
        "lu-leveled2": LEVELED2_PROBES,
        "projectile": PROJECTILE_PROBES,
        "sky-sound": SKY_SOUND_PROBES,
        "knvse-anim": KNVSE_PROBES,
        "nif-blocks": NIF_PROBES,
        "nvse-events": NVSE_EVENT_PROBES,
        "jip-events2": JIP_EVENT2_PROBES,
        "nvse-leftovers": N13_PROBES,
        "save-set": SAVE_SET_PROBES,
        "save-read": SAVE_READ_PROBES,
        "rgb-ui": RGB_UI_PROBES,
        "gaps1": GAPS1_PROBES,
        "gaps2": GAPS2_PROBES,
        "gaps3": GAPS3_PROBES,
        "gaps4": GAPS4_PROBES,
        "gaps5": GAPS5_PROBES,
        "gaps6": GAPS6_PROBES,
        "gaps7": GAPS7_PROBES,
        "gaps8": GAPS8_PROBES,
        "gaps9": GAPS9_PROBES,
        "gaps10": GAPS10_PROBES}
# Sets whose probes write (into the session only, see STORE_PROBES, EVENT_PROBES, HIT_PROBES
# and VAR_PROBES).
WRITING_SETS = {"jip-stores", "jip-events", "jip-hits", "nvse-vars", "nvse-setters", "nvse-data", "nvse-inv",
                "nvse-second", "nvse-fourth", "jip-fifth", "nvse-fifth", "nvse-sixth", "nvse-seventh", "nvse-eighth", "lu-leveled", "nvse-flags", "ini-files", "jip-n7", "player-state", "mcm", "plugin-odds", "lu-leveled2", "projectile", "sky-sound", "knvse-anim", "nif-blocks", "nvse-events", "jip-events2", "nvse-leftovers", "save-set", "save-read", "rgb-ui", "gaps1", "gaps2", "gaps3", "gaps4", "gaps5", "gaps6", "gaps7", "gaps8", "gaps9", "gaps10"}


def script(body):
    if body.startswith(("ref ", "float ")):
        return body + "\nEnd\n"
    return "Begin Function { }\n" + body + "\nEnd\n"


STEPS_DIR = os.path.join(HERE, "jip-probes")


def steps_text(set_name):
    """The job file of a set as committed in STEPS_DIR (the lab's candidate,
    `fnvtool ext queries`, runs these)."""
    return json.dumps(job(set_name), indent=1) + "\n"


def job_name(set_name):
    return set_name.replace("-", "_")


def job(set_name="jip-getters"):
    steps = [{"query": script(b), "label": pid, "field": "value", **({"text": True} if text else {})}
             for pid, b, text, _, _ in SETS[set_name]]
    return {"format": cs.STEPS_FORMAT, "name": job_name(set_name),
            "title": f"{set_name}, runtime-compiled" + ("" if set_name in WRITING_SETS else " (read-only)"),
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
    st = sub.add_parser("steps")
    st.add_argument("--dir", default=STEPS_DIR)
    a = ap.parse_args(argv)
    if a.cmd == "steps":
        os.makedirs(a.dir, exist_ok=True)
        for name in SETS:
            with open(os.path.join(a.dir, f"{job_name(name)}.steps.json"), "w", encoding="utf-8", newline="\n") as f:
                f.write(steps_text(name))
        print(json.dumps({"dir": a.dir, "sets": list(SETS)}))
        return 0
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
