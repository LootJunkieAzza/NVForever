"""Declarative single-variable A/B `.nvse` co-save capture campaigns.

Every vanilla form is resolved strictly via `GetFormFromMod "FalloutNV.esm" "<hex>"`
(zero bare EditorIDs, zero custom plugins). Temporary loop bookkeeping uses `*cp_*`
JIP auxiliary variables, which are never persisted to `.nvse`, keeping baseline saves
completely clean.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class ProbeStep:
    """One atomic A/B `.nvse` capture step."""

    name: str
    commands: tuple[str, ...] = ()
    inline_save: bool = False
    wait_ticks: int = 1
    note: str = ""


@dataclass(frozen=True)
class Campaign:
    """A self-contained single-session A/B `.nvse` co-save capture campaign."""

    name: str
    description: str
    required_extenders: tuple[str, ...]
    steps: tuple[ProbeStep, ...]
    interval: int = 10
    start_chargen_quest: str = "00102037"


COSAVE_CORE_JIP = Campaign(
    name="cosave_core_jip",
    description=(
        "NVSE v1 header, SDOM mod list, ARVR List/Map/StringMap/nested arrays, "
        "0xFF UDF string_var negative check, JPAV permanent AuxVars (float/ref/string, "
        "public & private), JPRM permanent RefMaps, ShowOff OSMS AuxStringMapArray, "
        "and 0xFF created-reference persistence."
    ),
    required_extenders=("xNVSE 6.4.9", "JIP LN NVSE 57.30", "JohnnyGuitarNVSE 528", "ShowOffNVSE Plugin 184"),
    steps=(
        ProbeStep(
            "c00_base",
            (
                'PrintToConsole "STEP|c00_base|nvse=%g|jip=%g|jg=%g|so=%g" '
                '(GetNVSEVersionFull) (GetJIPLNVersion) '
                '(GetPluginVersion "JohnnyGuitarNVSE") (GetPluginVersion "ShowOffNVSE Plugin")',
            ),
            note="Clean baseline: SDOM + empty ARVS/ARVE + empty STVS/STVE + JPGF",
        ),
        ProbeStep(
            "c01_str_inline",
            (
                'string_var sA = sv_Construct "HelloNVSE"',
                'string_var sB = sv_Construct "Second_12345"',
                'PrintToConsole "STEP|c01_str_inline|sA=%g|sB=%g" sA sB',
                "con_Save c01_str_inline",
            ),
            inline_save=True,
            note="Negative check: 0xFF runtime UDF string_vars emit 0 inner STVS..STVE chunks",
        ),
        ProbeStep(
            "c01b_str_after",
            ('PrintToConsole "STEP|c01b_str_after|ok"',),
            note="Post-UDF baseline check",
        ),
        ProbeStep(
            "c02_arr_list_inline",
            (
                'ref rDoc = GetFormFromMod "FalloutNV.esm" "104C0F"',
                'array_var aList = Ar_Construct "array"',
                "Ar_Append aList 42.5",
                'Ar_Append aList "AlphaStr"',
                "Ar_Append aList Player",
                "Ar_Append aList rDoc",
                'PrintToConsole "STEP|c02_arr_list_inline|id=%g|sz=%g|rDoc=%i" aList (Ar_Size aList) rDoc',
                "con_Save c02_arr_list_inline",
            ),
            inline_save=True,
            note="ARVR packed list (key_type=1, packed=1) with num/str/ref elements",
        ),
        ProbeStep(
            "c03_arr_map_inline",
            (
                'ref rDoc = GetFormFromMod "FalloutNV.esm" "104C0F"',
                'array_var aMap = Ar_Construct "map"',
                "let aMap[-5.5] := 100.25",
                'let aMap[10] := "MapVal"',
                "let aMap[99] := rDoc",
                'PrintToConsole "STEP|c03_arr_map_inline|id=%g|sz=%g" aMap (Ar_Size aMap)',
                "con_Save c03_arr_map_inline",
            ),
            inline_save=True,
            note="ARVR numeric map (key_type=1, packed=0) with negative/positive f64 keys",
        ),
        ProbeStep(
            "c04_arr_strmap_nested",
            (
                'ref r9mm = GetFormFromMod "FalloutNV.esm" "0E3778"',
                'array_var aChild = Ar_Construct "array"',
                "Ar_Append aChild 777",
                'array_var aSMap = Ar_Construct "stringmap"',
                'let aSMap["kNum"] := -12.75',
                'let aSMap["kStr"] := "NestedHello"',
                'let aSMap["kRef"] := r9mm',
                'let aSMap["kArr"] := aChild',
                'PrintToConsole "STEP|c04_arr_strmap_nested|child=%g|smap=%g|r9mm=%i" aChild aSMap r9mm',
                "con_Save c04_arr_strmap_nested",
            ),
            inline_save=True,
            note="ARVR stringmap (key_type=3, packed=0) + nested child array (val_type=4, num_refs=2)",
        ),
        ProbeStep(
            "c05_jip_aux_flt",
            (
                'Player.AuxVarSetFlt "_cv_flt" 123.5 0',
                'Player.AuxVarSetFlt "_cv_flt" -42.25 1',
                'PrintToConsole "STEP|c05_jip_aux_flt|sz=%g" (Player.AuxVarSize "_cv_flt")',
            ),
            note="JPAV v10 float elements on Player (0x14)",
        ),
        ProbeStep(
            "c06_jip_aux_mix_priv",
            (
                'ref rDoc = GetFormFromMod "FalloutNV.esm" "104C0F"',
                'ref r9mm = GetFormFromMod "FalloutNV.esm" "0E3778"',
                'rDoc.AuxVarSetRef "_cv_mix" r9mm 0',
                'rDoc.AuxVarSetStr "_cv_mix" "AuxHello" 1',
                'rDoc.AuxVarSetFlt "_cv_mix" 9.0 2',
                'Player.AuxVarSetFlt "cv_priv" 55.0 0',
                'PrintToConsole "STEP|c06_jip_aux_mix_priv|mixsz=%g|priv=%g" (rDoc.AuxVarSize "_cv_mix") (Player.AuxVarGetFlt "cv_priv")',
            ),
            note="JPAV v10 mixed ref/string/float elements + private name",
        ),
        ProbeStep(
            "c07_jip_refmap",
            (
                'ref rDoc = GetFormFromMod "FalloutNV.esm" "104C0F"',
                'ref r9mm = GetFormFromMod "FalloutNV.esm" "0E3778"',
                'RefMapSetFlt "_cv_rmap" 88.5 Player',
                'RefMapSetRef "_cv_rmap" r9mm rDoc',
                'RefMapSetStr "_cv_rmap" "RMapStr" r9mm',
                'RefMapSetFlt "cv_rmap_priv" 12.0 Player',
                'PrintToConsole "STEP|c07_jip_refmap|pubsz=%g|privsz=%g" (RefMapSize "_cv_rmap") (RefMapSize "cv_rmap_priv")',
            ),
            note="JPRM v10 public/private RefMaps + base-form key resolution",
        ),
        ProbeStep(
            "c10_showoff_strmap",
            (
                'ref rDoc = GetFormFromMod "FalloutNV.esm" "104C0F"',
                'AuxStringMapArraySetFloat "_cv_smap" "sKeyF" 19.25',
                'AuxStringMapArraySetRef "_cv_smap" "sKeyR" rDoc',
                'AuxStringMapArraySetString "_cv_smap" "sKeyS" "ShowOffVal"',
                'PrintToConsole "STEP|c10_showoff_strmap|sz=%g" (AuxStringMapArrayGetSize "_cv_smap")',
            ),
            note="ShowOff OSMS v10 AuxStringMapArray float/ref/string values",
        ),
        ProbeStep(
            "c12_created_0xff",
            (
                'ref rPen = GetFormFromMod "FalloutNV.esm" "05B63C"',
                "ref rDrop = Player.PlaceAtMe rPen 1",
                'Player.AuxVarSetRef "_cv_0xff" rDrop 0',
                'rDrop.AuxVarSetFlt "_cv_0xff_own" 77.0 0',
                'RefMapSetFlt "_cv_rmap_ff" 33.0 rDrop',
                'array_var aFF = Ar_Construct "array"',
                "Ar_Append aFF rDrop",
                'PrintToConsole "STEP|c12_created_0xff|rDrop=%i|arr=%g" rDrop aFF',
                "con_Save c12_created_0xff",
            ),
            inline_save=True,
            note="0xFF created ref stored in ARVR, JPAV value/owner, and JPRM base-resolved key",
        ),
    ),
)

COSAVE_JIP_JG_SHOWOFF = Campaign(
    name="cosave_jip_jg_showoff",
    description=(
        "SetModLocalData negative proof, JIP JPLR SetLinkedReference, JIP JPSV "
        "AddScriptVariable (float + ref), JohnnyGuitar SMGJ ExtraMiscStats, and "
        "ShowOff SOAT AuxTimer."
    ),
    required_extenders=("xNVSE 6.4.9", "JIP LN NVSE 57.30", "JohnnyGuitarNVSE 528", "ShowOffNVSE Plugin 184"),
    steps=(
        ProbeStep("c20_base", ('PrintToConsole "STEP|c20_base|ok"',), note="Clean baseline"),
        ProbeStep(
            "c22_modlocaldata",
            (
                'ref rDoc = GetFormFromMod "FalloutNV.esm" "104C0F"',
                'SetModLocalData "cv_mld_num" 42.5',
                'SetModLocalData "cv_mld_str" "MLDHello"',
                'SetModLocalData "cv_mld_ref" rDoc',
                'PrintToConsole "STEP|c22_modlocaldata|has=%g|num=%g" (ModLocalDataExists "cv_mld_num") (GetModLocalData "cv_mld_num")',
            ),
            note="Negative proof: SetModLocalData is session-only, not written to .nvse",
        ),
        ProbeStep(
            "c23_jip_linkedref",
            (
                'ref rDoc = GetFormFromMod "FalloutNV.esm" "104C0F"',
                'ref rDoor = GetFormFromMod "FalloutNV.esm" "1062AC"',
                "rDoc.SetLinkedReference rDoor",
                "ref rLnk = rDoc.GetLinkedRef",
                'PrintToConsole "STEP|c23_jip_linkedref|lnk=%i" rLnk',
            ),
            note="JIP JPLR v9 linked reference",
        ),
        ProbeStep(
            "c24_jip_addscriptvar",
            (
                'ref rDoc = GetFormFromMod "FalloutNV.esm" "104C0F"',
                'ref rQ = GetFormFromMod "FalloutNV.esm" "080664"',
                'int okDoc = rDoc.AddScriptVariable "cvAddedDoc"',
                'SetVariable "cvAddedDoc" 321.5 rDoc',
                'int okQ = AddScriptVariable "cvAddedQ" rQ',
                'SetVariable "cvAddedQ" 654.25 rQ',
                'PrintToConsole "STEP|c24_jip_addscriptvar|okDoc=%g|vDoc=%g|okQ=%g|vQ=%g" okDoc (GetVariable "cvAddedDoc" rDoc) okQ (GetVariable "cvAddedQ" rQ)',
            ),
            note="JIP JPSV v9 added script variables",
        ),
        ProbeStep(
            "c25_jg_extramisc",
            (
                'InitExtraMiscStat "CVCustomStat"',
                'ModExtraMiscStat "CVCustomStat" 17',
                'InitExtraMiscStat "CVSecondStat"',
                'ModExtraMiscStat "CVSecondStat" -5',
                'PrintToConsole "STEP|c25_jg_extramisc|v1=%g|v2=%g" (GetExtraMiscStat "CVCustomStat") (GetExtraMiscStat "CVSecondStat")',
            ),
            note="JohnnyGuitar SMGJ v1 signed i32 extra misc stats",
        ),
        ProbeStep(
            "c26_showoff_auxtimer",
            (
                'ref rDoc = GetFormFromMod "FalloutNV.esm" "104C0F"',
                'Player.AuxTimerStart "_cv_tmr_pub" 120.0 0',
                'rDoc.AuxTimerStart "cv_tmr_priv" 45.5 4',
                'PrintToConsole "STEP|c26_showoff_auxtimer|t1=%g|t2=%g" (Player.AuxTimerTimeLeft "_cv_tmr_pub") (rDoc.AuxTimerTimeLeft "cv_tmr_priv")',
            ),
            note="ShowOff SOAT v1 persistent aux timers",
        ),
    ),
)

COSAVE_EDGE_TYPES = Campaign(
    name="cosave_edge_types",
    description=(
        "JPSV 8-byte ref vs float union, multi-entry JPLR with 0xFF created ref, "
        "and ARVR >255-byte u16 string length + empty string."
    ),
    required_extenders=("xNVSE 6.4.9", "JIP LN NVSE 57.30", "ShowOffNVSE Plugin 184"),
    steps=(
        ProbeStep("c30_base", ('PrintToConsole "STEP|c30_base|ok"',), note="Clean baseline"),
        ProbeStep(
            "c31_jpsv_ref_and_multi",
            (
                'ref rQ = GetFormFromMod "FalloutNV.esm" "080664"',
                'ref rDoor = GetFormFromMod "FalloutNV.esm" "1062AC"',
                'AddScriptVariable "cvQNum" rQ',
                'SetVariable "cvQNum" -12.5 rQ',
                'AddScriptVariable "cvQRef" rQ',
                'SetRefVariable "cvQRef" rDoor rQ',
                'ref rGot = GetRefVariable "cvQRef" rQ',
                'PrintToConsole "STEP|c31_jpsv_ref_and_multi|num=%g|ref=%i" (GetVariable "cvQNum" rQ) rGot',
            ),
            note="JPSV v9 8-byte union: f64 -12.5 vs low-u32 FormID 0x001062AC",
        ),
        ProbeStep(
            "c35_jplr_multi_and_0xff",
            (
                'ref rDoc = GetFormFromMod "FalloutNV.esm" "104C0F"',
                'ref rDoor = GetFormFromMod "FalloutNV.esm" "1062AC"',
                'ref rPen = GetFormFromMod "FalloutNV.esm" "05B63C"',
                "ref rDrop = Player.PlaceAtMe rPen 1",
                "rDoc.SetLinkedReference rDoor",
                "rDrop.SetLinkedReference rDoc",
                "ref rL1 = rDoc.GetLinkedRef",
                "ref rL2 = rDrop.GetLinkedRef",
                'PrintToConsole "STEP|c35_jplr_multi_and_0xff|drop=%i|l1=%i|l2=%i" rDrop rL1 rL2',
            ),
            note="JPLR v9 multiple entries including 0xFF created reference",
        ),
        ProbeStep(
            "c41_long_and_empty_str",
            (
                'string_var s64 = "' + ("A" * 64) + '"',
                'string_var sLong = s64 + s64 + s64 + s64 + "0123456789ABCD"',
                'string_var sEmp = ""',
                'array_var aL = Ar_Construct "array"',
                "Ar_Append aL sLong",
                "Ar_Append aL sEmp",
                'PrintToConsole "STEP|c41_long_and_empty_str|len=%g|arr=%g" (sv_Length sLong) aL',
                "con_Save c41_long_and_empty_str",
            ),
            inline_save=True,
            note="ARVR v2 u16 string length > 255 (270 bytes) and empty string (0 bytes)",
        ),
    ),
)

ALL_CAMPAIGNS: tuple[Campaign, ...] = (
    COSAVE_CORE_JIP,
    COSAVE_JIP_JG_SHOWOFF,
    COSAVE_EDGE_TYPES,
)
CAMPAIGNS_BY_NAME: dict[str, Campaign] = {c.name: c for c in ALL_CAMPAIGNS}
