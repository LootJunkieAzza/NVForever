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

SETS = {"jip-getters": PROBES, "jip-forms": FORM_PROBES, "jip-stores": STORE_PROBES, "jip-events": EVENT_PROBES,
        "jip-hits": HIT_PROBES, "jip-records": RECORD_PROBES, "nvse-forms": NVSE_FORM_PROBES, "nvse-vars": VAR_PROBES,
        "nvse-refs": REF_PROBES, "nvse-controls": CONTROL_PROBES, "nvse-actors": ACTOR_PROBES,
        "nvse-setters": SETTER_PROBES, "nvse-misc": MISC_PROBES, "nvse-math": MATH_PROBES,
        "nvse-data": DATA_PROBES, "nvse-inv": INV_PROBES, "nvse-second": SECOND_PROBES,
        "nvse-keynames": KEYNAME_PROBES, "nvse-third": THIRD_PROBES, "nvse-fourth": FOURTH_PROBES,
        "jip-fifth": FIFTH_PROBES, "nvse-fifth": NVSE_FIFTH_PROBES}
# Sets whose probes write (into the session only, see STORE_PROBES, EVENT_PROBES, HIT_PROBES
# and VAR_PROBES).
WRITING_SETS = {"jip-stores", "jip-events", "jip-hits", "nvse-vars", "nvse-setters", "nvse-data", "nvse-inv",
                "nvse-second", "nvse-fourth", "jip-fifth", "nvse-fifth"}


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
