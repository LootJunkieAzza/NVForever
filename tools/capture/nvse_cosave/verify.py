#!/usr/bin/env python3
"""Read-only clean-room `.nvse` co-save A/B verification suite.

Validates captured `.nvse` files from `tools/capture/nvse_cosave/campaigns.py`
against every `[observed]` byte-layout assertion documented in
`docs/spec-notes/nvse-cosaves.md`.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import parser as nvse_parser  # type: ignore[import-not-found]
else:
    from . import parser as nvse_parser


def _find_nvse(root: Path, stem: str) -> Path | None:
    direct = root / f"{stem}.nvse"
    if direct.is_file():
        return direct
    for p in root.rglob(f"{stem}.nvse"):
        if p.is_file():
            return p
    return None


def verify_dir(saves_dir: Path) -> tuple[int, int, list[str]]:
    """Run all clean-room `.nvse` A/B assertions on captures under `saves_dir`."""
    passed = 0
    failed = 0
    lines: list[str] = []

    def check(name: str, cond: bool, detail: str = "") -> None:
        nonlocal passed, failed
        if cond:
            passed += 1
            lines.append(f"PASS  {name}" + (f" ({detail})" if detail else ""))
        else:
            failed += 1
            lines.append(f"FAIL  {name}" + (f" ({detail})" if detail else ""))

    def load(stem: str) -> tuple[bytes, nvse_parser.NvseCosave] | None:
        p = _find_nvse(saves_dir, stem)
        if p is None:
            return None
        raw = p.read_bytes()
        return raw, nvse_parser.NvseCosave.parse(raw)

    # Campaign 1: core header, SDOM, ARVR, 0xFF UDF string_var, JPAV, JPRM, OSMS, 0xFF refs
    c00 = load("c00_base")
    if c00 is not None:
        raw00, p00 = c00
        check("header_v1_and_runtime", p00.format_version == 1 and p00.runtime_version == 0x040020D0, f"nvse={p00.nvse_version:#x}")
        mods = p00.mod_list()
        check("sdom_10_official_masters", len(mods) == 10 and mods[0] == "FalloutNV.esm" and mods[9] == "TribalPack.esm")
        check("base_empty_arrays", len(p00.arrays()) == 0)
        check("base_empty_jip_aux", len(p00.jip_aux_vars()) == 0)
        check("base_jpgf_unmapped_only", len(p00.unknown_chunks()) == 1 and p00.unknown_chunks()[0]["tag"] == "JPGF")

        c01 = load("c01_str_inline")
        if c01 is not None:
            check("udf_0xff_string_var_no_stvr", c01[0] == raw00, f"bytes={len(c01[0])} vs {len(raw00)}")

    c02 = load("c02_arr_list_inline")
    if c02 is not None:
        arrs = c02[1].arrays()
        check("arvr_list_count_and_kind", len(arrs) == 1 and arrs[0]["kind"] == "list" and arrs[0]["id"] == 1)
        if arrs:
            vals = [e["val"] for e in arrs[0]["entries"]]
            check(
                "arvr_list_elements",
                vals
                == [
                    {"type": "num", "value": 42.5},
                    {"type": "str", "value": "AlphaStr"},
                    {"type": "form", "value": 0x14},
                    {"type": "form", "value": 0x00104C0F},
                ],
            )

    c03 = load("c03_arr_map_inline")
    if c03 is not None:
        arrs = c03[1].arrays()
        check("arvr_map_kind_and_entries", len(arrs) == 1 and arrs[0]["kind"] == "map" and len(arrs[0]["entries"]) == 3)
        if arrs:
            by_k = {e["key"]: e["val"] for e in arrs[0]["entries"]}
            check(
                "arvr_map_values",
                by_k.get(-5.5) == {"type": "num", "value": 100.25}
                and by_k.get(10.0) == {"type": "str", "value": "MapVal"}
                and by_k.get(99.0) == {"type": "form", "value": 0x00104C0F},
            )

    c04 = load("c04_arr_strmap_nested")
    if c04 is not None:
        arrs = {a["id"]: a for a in c04[1].arrays()}
        check("arvr_strmap_nested_two_arrays", set(arrs.keys()) == {1, 2})
        if 1 in arrs and 2 in arrs:
            check("arvr_nested_child_refcount_2", len(arrs[1]["ref_mods"]) == 2 and arrs[1]["entries"][0]["val"] == {"type": "num", "value": 777.0})
            sm = {e["key"]: e["val"] for e in arrs[2]["entries"]}
            check(
                "arvr_strmap_values_and_nested_id",
                arrs[2]["kind"] == "stringmap"
                and sm.get("kNum") == {"type": "num", "value": -12.75}
                and sm.get("kStr") == {"type": "str", "value": "NestedHello"}
                and sm.get("kRef") == {"type": "form", "value": 0x000E3778}
                and sm.get("kArr") == {"type": "array", "value": 1},
            )

    c05 = load("c05_jip_aux_flt")
    if c05 is not None:
        aux = c05[1].jip_aux_vars()
        check(
            "jpav_float_elements",
            len(aux) == 1
            and aux[0]["owner"] == 0x14
            and aux[0]["name"] == "_cv_flt"
            and aux[0]["elements"] == [{"type": "num", "value": 123.5}, {"type": "num", "value": -42.25}],
        )

    c06 = load("c06_jip_aux_mix_priv")
    if c06 is not None:
        aux = {(a["owner"], a["name"]): a["elements"] for a in c06[1].jip_aux_vars()}
        check(
            "jpav_mixed_and_private",
            aux.get((0x00104C0F, "_cv_mix"))
            == [
                {"type": "form", "value": 0x000E3778},
                {"type": "str", "value": "AuxHello"},
                {"type": "num", "value": 9.0},
            ]
            and aux.get((0x14, "cv_priv")) == [{"type": "num", "value": 55.0}],
        )

    c07 = load("c07_jip_refmap")
    if c07 is not None:
        rmaps = {m["name"]: {e["key"]: e["val"] for e in m["entries"]} for m in c07[1].jip_refmaps()}
        check(
            "jprm_public_and_private_base_keys",
            rmaps.get("_cv_rmap")
            == {
                0x00000007: {"type": "num", "value": 88.5},
                0x00104C0C: {"type": "form", "value": 0x000E3778},
                0x000E3778: {"type": "str", "value": "RMapStr"},
            }
            and rmaps.get("cv_rmap_priv") == {0x00000007: {"type": "num", "value": 12.0}},
        )

    c10 = load("c10_showoff_strmap")
    if c10 is not None:
        smaps = {m["name"]: {e["key"]: e["val"] for e in m["entries"]} for m in c10[1].showoff_strmaps()}
        check(
            "osms_showoff_strmap",
            smaps.get("_cv_smap")
            == {
                "sKeyF": {"type": "num", "value": 19.25},
                "sKeyR": {"type": "form", "value": 0x00104C0F},
                "sKeyS": {"type": "str", "value": "ShowOffVal"},
            },
        )

    c12 = load("c12_created_0xff")
    if c12 is not None:
        arrs = c12[1].arrays()
        aux = {(a["owner"], a["name"]): a["elements"] for a in c12[1].jip_aux_vars()}
        ff_val = aux.get((0x14, "_cv_0xff"), [{}])[0].get("value", 0)
        check(
            "created_0xff_in_arvr_and_jpav",
            (ff_val >> 24) == 0xFF
            and aux.get((ff_val, "_cv_0xff_own")) == [{"type": "num", "value": 77.0}]
            and len(arrs) == 1
            and arrs[0]["entries"][0]["val"] == {"type": "form", "value": ff_val},
            f"rDrop=0x{ff_val:08X}",
        )
        rmaps = {m["name"]: {e["key"]: e["val"] for e in m["entries"]} for m in c12[1].jip_refmaps()}
        check("created_0xff_jprm_resolves_to_base_pen", rmaps.get("_cv_rmap_ff") == {0x0005B63C: {"type": "num", "value": 33.0}})

    # Campaign 2: SetModLocalData negative proof, JPLR, JPSV, SMGJ, SOAT
    c20 = load("c20_base")
    c22 = load("c22_modlocaldata")
    if c20 is not None and c22 is not None:
        check("modlocaldata_not_persisted", c20[0] == c22[0], f"bytes={len(c22[0])}")

    c23 = load("c23_jip_linkedref")
    if c23 is not None:
        links = c23[1].jip_linked_refs()
        check("jplr_single_link", links == [{"ref": 0x00104C0F, "linked": 0x001062AC, "mod_index": 0xFF}])

    c24 = load("c24_jip_addscriptvar")
    if c24 is not None:
        sv = {(v["owner"], v["name"]): v["val"] for v in c24[1].jip_added_script_vars()}
        check(
            "jpsv_float_values",
            sv.get((0x00080664, "cvAddedQ")) == {"type": "num", "value": 654.25}
            and sv.get((0x00104C0F, "cvAddedDoc")) == {"type": "num", "value": 0.0},
        )

    c25 = load("c25_jg_extramisc")
    if c25 is not None:
        stats = {s["name"]: s["value"] for s in c25[1].jg_extra_misc_stats()}
        check("smgj_signed_i32_stats", stats == {"CVCustomStat": 17, "CVSecondStat": -5})

    c26 = load("c26_showoff_auxtimer")
    if c26 is not None:
        tmrs = {(t["owner"], t["name"]): (t["time_a"], t["time_b"], t["flags"]) for t in c26[1].showoff_aux_timers()}
        check(
            "soat_showoff_timers",
            tmrs.get((0x14, "_cv_tmr_pub")) == (120.0, 120.0, 0)
            and tmrs.get((0x00104C0F, "cv_tmr_priv")) == (45.5, 45.5, 4),
        )

    # Campaign 3 & 4: JPSV ref union, JPLR multi + 0xFF, ARVR >255B & empty string
    c31 = load("c31_jpsv_ref_and_multi")
    if c31 is not None:
        sv = {(v["owner"], v["name"]): v["val"] for v in c31[1].jip_added_script_vars()}
        check(
            "jpsv_ref_and_negative_float",
            sv.get((0x00080664, "cvQRef")) == {"type": "form", "value": 0x001062AC}
            and sv.get((0x00080664, "cvQNum")) == {"type": "num", "value": -12.5},
        )

    c35 = load("c35_jplr_multi_and_0xff")
    if c35 is not None:
        links = c35[1].jip_linked_refs()
        check(
            "jplr_multi_and_0xff",
            len(links) == 2
            and links[0] == {"ref": 0x00104C0F, "linked": 0x001062AC, "mod_index": 0xFF}
            and (links[1]["ref"] >> 24) == 0xFF
            and links[1]["linked"] == 0x00104C0F,
        )

    c41 = load("c41_long_and_empty_str")
    if c41 is not None:
        arrs = c41[1].arrays()
        expected_long = ("A" * 256) + "0123456789ABCD"
        check(
            "arvr_u16_long_and_empty_string",
            len(arrs) == 1
            and arrs[0]["entries"][0]["val"] == {"type": "str", "value": expected_long}
            and arrs[0]["entries"][1]["val"] == {"type": "str", "value": ""},
        )

    return passed, failed, lines


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="Verify clean-room `.nvse` co-save captures against spec.")
    ap.add_argument("--saves-dir", type=Path, required=True)
    args = ap.parse_args(argv)
    passed, failed, lines = verify_dir(args.saves_dir)
    for ln in lines:
        print(ln)
    print(f"\nSummary: {passed} passed, {failed} failed")
    return 0 if (failed == 0 and passed > 0) else 1


if __name__ == "__main__":
    sys.exit(main())
