#!/usr/bin/env python3
"""Clean-room A/B `.fos` capture verification suite (61 pure-vanilla + 7 optional manual-GECK-plugin assertions, 68 total).

Point `--saves-dir` (or `FNV_FOS_SAVES_DIR`) at a private directory of local
Fallout: New Vegas 1.4.0.525 captures produced by `harness.py` to assert all
confirmed `[observed]` layouts, all 13 parser/converter regression cases, and
all 3 `[observed]` negative proofs (`0x17`, `0x26`, `0x28`).

Can be invoked either as a script or as a module:
    python tools/capture/fos/verify.py --saves-dir <dir>
    python -m tools.capture.fos.verify --saves-dir <dir>
"""

from __future__ import annotations

import argparse
import json
import os
import struct
import sys
from dataclasses import dataclass
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import parser as fos_parser  # type: ignore[import-not-found]
else:
    from . import parser as fos_parser


@dataclass(frozen=True)
class CheckResult:
    status: str
    tag: str
    detail: str


def has_achr_note_ref(fos: fos_parser.FosFile, note_fid: int) -> bool:
    """Return True if `note_fid` appears in `ACHR 0x14`'s tail notes list with zero
    `TYPE_NOTE` (kind 31) changed forms in the save (BUG #9 invariant).
    """
    r3 = fos.form_id_to_ref3(note_fid)
    if r3 is None:
        return False
    rec = fos.get_rec(1, 0x14)
    if rec is None:
        return False
    return (b"\x04\x7c" + r3 + b"\x7c") in rec["data"] and not any(
        entry["kind"] == 31 for entry in fos.changed
    )


def decode_player_tok76_from_file(fos: fos_parser.FosFile) -> int | None:
    """Extract `ACHR 0x14` process-block `tok[76]` magazine round count."""
    player = fos.decode_player(gate_scale_bit4=True)
    return player.get("tok76_mag_rounds")


def _round_av_map(av_map: dict[int, float]) -> dict[int, float]:
    return {k: round(v, 4) for k, v in av_map.items()}


def verify_directory(saves_dir: str | os.PathLike[str]) -> list[CheckResult]:
    """Run 61 pure-vanilla (+ 7 optional manual-GECK-plugin) assertions (68 total) against `.fos` files in `saves_dir`."""
    sdir = Path(saves_dir)
    if not sdir.is_dir():
        raise FileNotFoundError(f"Saves directory does not exist: {sdir}")

    cache: dict[str, fos_parser.FosFile] = {}

    def load(filename: str) -> fos_parser.FosFile:
        if filename not in cache:
            path = sdir / filename
            if not path.is_file():
                raise FileNotFoundError(f"Missing capture file: {path}")
            cache[filename] = fos_parser.FosFile(path)
        return cache[filename]

    checks: list[CheckResult] = []

    def ok(tag: str, cond: bool, detail: str) -> None:
        checks.append(CheckResult("PASS" if cond else "FAIL", tag, detail))

    # 1. Vanilla 10-master invariant
    p00 = load("p00_base.fos")
    ok(
        "env.vanilla_10_masters",
        len(p00.plugins) == 10 and p00.plugins[0] == "FalloutNV.esm",
        f"{len(p00.plugins)} masters: {p00.plugins[:3]}...",
    )

    # 2. BUG #1: Player scale bit-4 gating
    p01 = load("p01_scale.fos")
    r00 = p00.get_rec(1, 0x14)
    r01 = p01.get_rec(1, 0x14)
    assert r00 is not None and r01 is not None
    ok(
        "bug1.player_scale_bit4_absent",
        not (r00["flags"] & 0x10) and r00["data"][0x4A4:0x4A8] == b"\x00\x7c\x04\x7c",
        f"p00 flags={r00['flags']:#010x} @0x4a4={r00['data'][0x4A4:0x4AC].hex(' ')}",
    )
    ok(
        "bug1.player_scale_bit4_present",
        bool(r01["flags"] & 0x10) and p01.decode_player()["scale"] == 1.25,
        f"p01 flags={r01['flags']:#010x} @0x4a4={r01['data'][0x4A4:0x4AD].hex(' ')} scale=1.25",
    )
    v5_err = None
    try:
        unconditional = p00.decode_player(gate_scale_bit4=False)
        v5_err = unconditional.get("actor_extras_err") or unconditional.get("inv_err")
    except fos_parser.ParseError as exc:
        v5_err = str(exc)
    ok("bug1.fos_v5_fails_on_p00", v5_err is not None, f"unconditional decoder err={v5_err}")

    # 3. AV1 / AV2 / AV3 / AV[231] + BUG #7 (ModAV rads/hardcore in AV2)
    p02 = load("p02_av_perm.fos").decode_player()
    p03 = load("p03_av_dmg.fos").decode_player()
    p04 = load("p04_rads_hc.fos").decode_player()
    pb09 = load("pb09_active_effect_t3.fos").decode_player()
    pb10 = load("pb10_karma_hp_radio.fos").decode_player()
    ok(
        "v2.av1_active_effects",
        _round_av_map(pb09["av1_mod"]) == {5: 2.0, 7: 3.0, 16: 60.0, 20: 32.0},
        f"pb09 AV1={_round_av_map(pb09['av1_mod'])}",
    )
    ok(
        "v2.av2_perm_xp_karma",
        p02["av2_base"].get(5) == 3.0
        and p02["av2_base"].get(43) == 12.0
        and p02["av2_base"].get(24) == 500.0
        and pb10["av2_base"].get(23) == 250.0,
        f"p02 AV2={_round_av_map(p02['av2_base'])}, pb10 AV2={_round_av_map(pb10['av2_base'])}",
    )
    ok(
        "v2.av3_damage_ap_limbs",
        12 in p03["av3_dmg"]
        and p03["av3_dmg"].get(25) == -40.0
        and p03["av3_dmg"].get(27) == -30.0,
        f"p03 AV3={_round_av_map(p03['av3_dmg'])}",
    )
    ok(
        "v2.av231_health_damage",
        round(pb10["av231_hp_dmg"], 4) == -40.0,
        f"pb10 AV[231]={pb10['av231_hp_dmg']}",
    )
    ok(
        "bug7.modav_rads_hc_in_av2",
        p04["av2_base"].get(54) == 125.0
        and p04["av2_base"].get(73) == 150.0
        and p04["av2_base"].get(74) == 175.0
        and p04["av2_base"].get(75) == 200.0
        and p04["av1_mod"].get(10) == -1.0,
        f"p04 AV2={_round_av_map(p04['av2_base'])} AV1={_round_av_map(p04['av1_mod'])}",
    )

    # 4. BUG #2: ACHR bit 22 stores all SetAV base overrides (not just skills 32..=45)
    p08 = load("p08_special_name.fos")
    r08 = p08.get_rec(1, 0x14)
    assert r08 is not None
    sig = fos_parser.PROCESS_AV_LIST_ANCHOR
    idx = r08["data"].find(sig)
    b22_bytes = r08["data"][idx - 9 : idx]
    ok(
        "bug2.bit22_non_skill_luck",
        bool(r08["flags"] & (1 << 22))
        and b22_bytes == b"\x04\x7c\x0b\x7c\x00\x00\x10\x41\x7c"
        and p08.decode_player()["bit22_av_overrides"] == [(11, 9.0)],
        f"p08 bit22 pre-anchor={b22_bytes.hex(' ')} (av=11 Luck, f32=9.0)",
    )

    # 5. BUG #3: Global[10] Pip-Boy radio state in list2
    pb10f = load("pb10_karma_hp_radio.fos")
    pb11f = load("pb11_radio_on.fos")
    r15f = load("r15_radio_wait.fos")
    rad11 = pb11f.decode_radio()
    rad15 = r15f.decode_radio()
    assert rad11 is not None and rad15 is not None
    ok(
        "bug3.radio_header_unchanged_and_list2_active",
        rad11["on"] == 0
        and rad11["pipboy_on"] is True
        and rad11["pipboy_station"] == 0x14DF03
        and rad15["active_stations"][0]["playback_ms"] > rad11["active_stations"][0]["playback_ms"],
        f"pb10 len={len(pb10f.globals1[10])} -> pb11 len={len(pb11f.globals1[10])} "
        f"hdr_on={rad11['on']} list2_station={rad11['pipboy_station']:#010x} "
        f"ms={rad11['active_stations'][0]['playback_ms']}->{rad15['active_stations'][0]['playback_ms']}",
    )

    # 6. BUG #4: REFR bit 6 (0x40) for 0x21 ownership
    w05 = load("w05_refs_scale_lock_door_cont.fos")
    own_rec = w05.get_rec(0, 0x103E4A)
    assert own_rec is not None
    ok(
        "bug4.refr_bit6_owner_0x21",
        (own_rec["flags"] & 0x50) == 0x50
        and own_rec["data"][5:13] == b"\x04\x7c\x21\x7c\x00\x1b\x42\x7c"
        and w05.decode_refr(0x103E4A, include_bit6_bit10=False).get("err") is not None
        and ("0x21_owner", 0x104C0C) in w05.decode_refr(0x103E4A)["extras"],
        f"0x103E4A flags={own_rec['flags']:#010x} owner_ref={w05.resolve(own_rec['data'][9:12]):#010x}",
    )

    # 7. BUG #5: CELL bit 3 owner AFTER bit-31 fog
    r02 = load("r02_cell_public_owner.fos")
    cell = r02.get_rec(7, 0x103DF9)
    cell_dec = r02.decode_cell(0x103DF9)
    assert cell is not None and cell_dec is not None
    ok(
        "bug5.cell_owner_after_fog",
        cell["flags"] == 0x8000000A
        and cell["data"][:2] == b"\x20\x7c"
        and cell_dec["owner"] == 0x104C0C
        and cell["data"][-1] == 0x7C,
        f"CELL 0x103DF9 flags={cell['flags']:#010x} len={len(cell['data'])} owner={cell_dec['owner']:#010x}",
    )

    # 8. BUG #6: Non-player ACHR ModAV lands under bit 23
    w00 = load("w00_base.fos")
    w01 = load("w01_actor_scale_fac_av.fos")
    d0 = w00.get_rec(1, 0x104C0F)
    d1 = w01.get_rec(1, 0x104C0F)
    assert d0 is not None and d1 is not None
    idx1 = d1["data"].find(sig)
    b23 = d1["data"][idx1 - 9 : idx1]
    ok(
        "bug6.npc_modav_bit23",
        bool(d1["flags"] & (1 << 23))
        and not (d0["flags"] & (1 << 23))
        and b23 == b"\x04\x7c\x05\x7c\x00\x00\x80\x40\x7c"
        and w01.decode_actor(1, 0x104C0F)["av_bit23_mod"] == [(5, 4.0)],
        f"DocMitchell flags {d0['flags']:#010x}->{d1['flags']:#010x} bit23={b23.hex(' ')}",
    )

    # 9. BUG #8: NPC_ 0x7 Player.SexChange writes bit 24 (0x01000000) u8 |
    s02 = load("s02_sex_change.fos")
    npc7 = s02.get_rec(10, 7)
    assert npc7 is not None
    ok(
        "bug8.npc7_sexchange_bit24",
        npc7["flags"] == 0x01000000
        and npc7["data"] == b"\x01\x7c"
        and s02.decode_player()["npc_base"]["female"] is True,
        f"s02 NPC_ 0x7 flags={npc7['flags']:#010x} data={npc7['data'].hex(' ')}",
    )

    # 10. Round 1 v2/v3/v4/v5 confirmations (25 checks)
    pb01 = load("pb01_factions.fos").decode_player()
    pb02 = load("pb02_perks.fos").decode_player()
    pb03 = load("pb03_spells.fos")
    pb05 = load("pb05_inv_equip_cond_hotkey.fos").decode_player()
    r09 = load("r09_teammate_perk.fos")
    r09p = r09.decode_player()
    e01 = load("e01_repu.fos")
    q03 = load("q03_facmod.fos")
    q05 = load("q05_door_disable.fos")
    q08 = load("q08_ammo_rounds.fos")
    s04 = load("s04_mod_ammo_0x8d_0x6e.fos").decode_player()
    s05 = load("s05_unequip_9mm.fos").decode_player()
    r01 = load("r01_eczn_respawns.fos")
    r03 = load("r03_misc_quest_val.fos")
    w03 = load("w03_actor_dead_base.fos")
    w04 = load("w04_actor_ashpile.fos")
    w06 = load("w06_mapmarker_vis.fos")
    w07 = load("w07_mapmarker_travel.fos")
    w09 = load("w09_fact_repu_note_flst_lvli.fos")
    w11 = load("w11_terminal_pgre.fos")
    w12 = load("w12_exterior_weather.fos")

    ok(
        "v2.player_0x5E_factions",
        (0x104C6E, 2) in dict(pb01["actor_extras"]).get("0x5E_factions", []),
        f"{dict(pb01['actor_extras']).get('0x5E_factions')}",
    )
    ok(
        "v2.player_perks",
        (0x31DD3, 1) in pb02["perks"] and (0x94EB9, 1) in pb02["perks"],
        f"{pb02['perks']}",
    )
    ok("v2.player_teammate_perks", (0x31DD3, 1) in r09p["teammate_perks"], f"{r09p['teammate_perks']}")
    ok(
        "v2.player_spells_npc_bit4",
        pb03.get_rec(10, 7)["flags"] == 0x10
        and pb03.decode_npc_or_crea(10, 7)["spells"] == [0x1768BC],
        f"{pb03.get_rec(10, 7)['data'].hex(' ')}",
    )
    ok(
        "v2.clas_tag_skills",
        r09.get_rec(33, 0x57E6A)["flags"] == 2
        and r09.decode_clas(0x57E6A)["tag_skills"][1] == 36,
        f"{r09.decode_clas(0x57E6A)['tag_skills']}",
    )
    ok(
        "v2.repu_fame_infamy",
        e01.decode_repu(0x104C22)["fame"] == 15.0 and e01.decode_repu(0x104C22)["infamy"] == 7.0,
        "fame=15.0 infamy=7.0",
    )
    ok(
        "v2.chal_bit1",
        load("pb08_active_effect_t0.fos").decode_chal(0x15EAF1)["progress"] == 1
        and load("pb08_active_effect_t0.fos").decode_chal(0x15EAF1)["chal_flags"] == 0,
        "progress=1 flags=0",
    )
    ok(
        "v2.flst_u32_count",
        w09.get_rec(37, 0x17BA3B)["data"][:5] == b"\x01\x00\x00\x00\x7c"
        and len(w09.get_rec(37, 0x17BA3B)["data"]) == 9,
        "u32(1)|ref3|",
    )
    ok(
        "v2.lvli_bit31",
        w09.get_rec(40, 0x16BACB)["flags"] == 0x80000000
        and len(w09.get_rec(40, 0x16BACB)["data"]) == 17,
        w09.get_rec(40, 0x16BACB)["data"].hex(" "),
    )
    ok(
        "v2.mapmarker_0x2C",
        w06.get_rec(0, 0x1088F6)["data"] == b"\x04\x7c\x2c\x7c\x01\x7c"
        and w07.get_rec(0, 0x1088F6)["data"] == b"\x04\x7c\x2c\x7c\x03\x7c",
        "0x01 -> 0x03",
    )
    ok("v2.refr_form_disabled", q05.decode_refr(0x1062AC)["form_flags"] == 0x80B, "0x0000080B")
    ok(
        "v2.actor_form_dead_0x10000",
        q08.decode_actor(1, 0x104C0F)["form_flags"] == 0x00010409,
        "DocMitchellREF FORM_DEAD 0x10000",
    )
    ok("v2.dead_counts", q08.decode_dead_counts()["entries"] == [(0x104C0C, 1)], "DocMitchell base count=1")
    ok("v2.weather", w12.decode_weather()["current"] == 0x0FFC88, "0x000FFC88")
    ok(
        "v2.weapon_mods_0x8D",
        any(any(e.get("0x8D_mods") == 3 for e in it["entries"]) for it in s04["inventory"])
        and any(any(e.get("0x8D_mods") == 3 for e in it["entries"]) for it in s05["inventory"]),
        "0x8D=3 equipped & unequipped",
    )
    ok(
        "v3.actor_0x5F_limbs_gone",
        ("0x5F_limbs", (2, [(1, True)])) in q08.decode_actor(1, 0x104C0F)["extras"],
        "head dismembered mask=2, (1, True)",
    )
    ok(
        "v3.actor_0x89_ashpile",
        any(e[0] == "0x89_ashpile" for e in w04.decode_actor(1, 0x104C0F)["extras"]),
        "DocMitchell ash pile",
    )
    ok(
        "v3.terminal_0x50",
        w11.get_rec(0, 0x11A30A)["data"] == b"\x04\x7c\x50\x7c\x01\x7c\x01\x7c",
        "flags=1 lock_level=1",
    )
    ok("v3.eczn_bit1_respawns", r01.decode_eczn(0x131F33)["zone_flags"] == 0x01, "flags=0x01 never_resets")
    ok("v3.misc_bit0_quest", r03.decode_misc(0x05B63C)["form_flags"] == 0x409, "0x409 Quest Item")
    ok("v3.npc_bit1_essential", w03.decode_npc_or_crea(10, 0x104C0C)["essential"] is True, "ACBS bit 1 Essential")
    ok(
        "v3.fact_bit2_modifier",
        b"\x2a\x00\x00\x00\x7c" in q03.get_rec(34, 0x104C6E)["data"],
        "i32 modifier=42",
    )
    ok(
        "v4.fact_bit1_flags",
        r09.decode_fact(0x104C7E)["faction_flags"] == 0x0111,
        "u32 flags=0x0111",
    )
    ok(
        "v4.hotkeys_0x4A",
        any(any(e.get("0x4A_hotkey") == 2 for e in it["entries"]) for it in pb05["inventory"]),
        "slot 3 -> 0x4A=2",
    )
    pb08 = load("pb08_active_effect_t0.fos").decode_player()
    ok(
        "v4.active_effects",
        len(pb09["active_effects"]) == 7
        and pb09["active_effects"][1]["elapsed"] > pb08["active_effects"][1]["elapsed"],
        "7 effects with advancing elapsed time",
    )

    # 11. Round 2 confirmations & bugs #9/#10 (14 checks)
    r7_06 = load("r7_06_unequip_9mm.fos")
    r7_07 = load("r7_07_qust_vcg02.fos")
    r7_08 = load("r7_08_qust_vms01_active.fos")
    r8_01 = load("r8_01_bisonsteve_eczn.fos")
    r8_02 = load("r8_02_vault11c_eczn.fos")
    r8_03 = load("r8_03_dest_partial.fos")
    r8_04 = load("r8_04_dest_destroy.fos")
    r8_05 = load("r8_05_pgre_move_disable.fos")
    r8_06 = load("r8_06_note_pickup.fos")
    r8_07 = load("r8_07_tact_actor_ref.fos")
    r9_02 = load("r9_02_pickup_own_scale_orig.fos")
    r9_04 = load("r9_04_pickup_xrnk.fos")
    r9_12 = load("r9_12_npc_modpcattrib.fos")
    r10_08 = load("r10_08_comp_hired.fos")
    r10_09 = load("r10_09_0xff_self_refvars.fos")
    r10_12 = load("r10_12_dest_stage0.fos")
    r10_14 = load("r10_14_dest_stage2.fos")
    r10_15 = load("r10_15_dest_stage3.fos")
    r14_01 = load("r14_01_equip_9mm.fos")
    r14_04 = load("r14_04_fire_hp_2.fos")

    ok(
        "r2.global1_active_quest",
        r7_06.decode_active_quest() is None
        and r7_07.decode_active_quest() == 0x10A214
        and r7_08.decode_active_quest() == 0x080664,
        "None -> 0x10A214 (VCG02) -> 0x080664 (VMS01)",
    )
    q_vcg02 = r7_07.decode_qust(0x10A214)
    ok(
        "r2.qust_bits_1_2_29_30_31",
        q_vcg02["flags"] == 0xE0000006
        and q_vcg02["quest_flags"] == 0x21
        and q_vcg02["delay"] == 2.5
        and q_vcg02["objectives"] == [(10, 3), (20, 1)],
        f"QUST 0x10A214 flags={q_vcg02['flags']:#010x} bit1=0x21 bit2=2.5f bit29={q_vcg02['objectives']}",
    )
    p10_08 = r10_08.decode_player()
    ok(
        "r2.player_0x1D_teammates",
        bool(p10_08["flags"] & (1 << 11))
        and dict(p10_08["actor_extras"]).get("0x1D_teammates") == [0x00096BCE],
        f"flags={p10_08['flags']:#010x} 0x1D={dict(p10_08['actor_extras']).get('0x1D_teammates')}",
    )
    ez1 = r8_01.decode_eczn(0x0D7034)
    ez1_after = r8_02.decode_eczn(0x0D7034)
    ez2 = r8_02.decode_eczn(0x0E9081)
    ok(
        "r2.eczn_bit31",
        ez1["stamps"] == (0, 132, 0, 2)
        and ez1_after["stamps"] == (132, 132, 0, 2)
        and ez2["stamps"] == (0, 132, 0, 6),
        "BisonSteve level=2 (detach 0->132), Vault11Zone level=6",
    )
    pg_mv = r8_05.decode_pgre(0x0EA807)
    pg_dis = r8_05.decode_pgre(0x0EA808)
    ok(
        "r2.pgre_places",
        pg_mv["flags"] == 0x00000006
        and pg_mv["place"]["at"] == 0x0E6A6E
        and pg_mv["place"]["havok_len"] == 4,
        f"PGRE 0x0EA807 flags=0x6 cell=0x0E6A6E z={pg_mv['place']['pos'][2]:.2f}",
    )
    ok(
        "r2.pgre_flags",
        pg_dis["flags"] == 0x00000001
        and pg_dis["lead_u8"] == 0xFF
        and pg_dis["form_flags"] == 0x0100080B,
        "PGRE 0x0EA808 flags=0x1 u8=0xff form_flags=0x0100080B (FORM_DISABLED)",
    )
    t_v11 = r8_07.decode_tact(0x0E88AD)
    t_rnv = r8_07.decode_tact(0x14DF02)
    ok(
        "r2.tact_bit23_actor",
        t_v11["flags"] == 0x00800000
        and t_v11["talking_actor"] == 0x104C0F
        and t_rnv["flags"] == 0x00800000
        and t_rnv["talking_actor"] == 0x104C0F,
        "TACT 0x0E88AD & 0x14DF02 bit23 -> 0x104C0F",
    )
    npc7_sp = r9_12.decode_npc_or_crea(10, 7)
    ok(
        "r2.npc7_bit2_special",
        npc7_sp["flags"] == 0x00000004 and npc7_sp["special"] == [7, 5, 5, 5, 5, 5, 5],
        "NPC_ 0x7 bit2 = [7,5,5,5,5,5,5]",
    )
    p9_02 = r9_02.decode_player()
    p9_04 = r9_04.decode_player()
    ok(
        "r2.item_extra_0x21_player_inv",
        any(
            it["item"] == 0x0340A4
            and any(e.get("0x21_owner") == 0x104C0C for e in it["entries"])
            for it in p9_02["inventory"]
        ),
        "Plunger 0x0340A4 in player inv has 0x21_owner=0x104C0C",
    )
    ok(
        "r2.item_extra_0x23_rank_player_inv",
        any(
            it["item"] == 0x0340B5 and any(e.get("0x23_rank") == 1 for e in it["entries"])
            for it in p9_04["inventory"]
        ),
        "ShotGlass01 0x0340B5 in player inv has 0x23_rank=1",
    )
    cr_refr = r10_09.decode_refr(0xFF00131C)
    sc_refr = dict(cr_refr["extras"]).get("0x0D_script")
    cr_acre = r10_09.decode_actor(2, 0xFF00131D)
    sc_acre = dict(cr_acre["extras"]).get("0x0D_script")
    ok(
        "r2.created_0xff_script_refvars",
        (3, ("ref", 0xFF00131C)) in sc_refr["vars"]
        and (2, ("ref", 0xFF00131D)) in sc_acre["vars"],
        f"REFR 0xFF00131C vars={sc_refr['vars']}, ACRE 0xFF00131D vars={sc_acre['vars']}",
    )
    ok(
        "bug9.notes_in_achr14_not_type31",
        has_achr_note_ref(load("q04_note.fos"), 0x175AB0)
        and has_achr_note_ref(load("e02_note.fos"), 0x177875)
        and has_achr_note_ref(r8_06, 0x0E88A0),
        "AddNote (q04/e02) + world pickup (r8_06) append ref3| to ACHR 0x14 tail with 0 TYPE_NOTE",
    )
    d_fe_part = r8_03.decode_refr(0x0EA490)
    d_fe_dest = r8_04.decode_refr(0x0EA490)
    d_eb_s0 = r10_12.decode_refr(0x14DB1C)
    d_eb_s2 = r10_14.decode_refr(0x14DB1C)
    d_eb_s3 = r10_15.decode_refr(0x14DB1C)
    ok(
        "bug10.dest_0x5B_stage0_and_multistage",
        d_fe_part["extras"] == [("0x56_hp", 1.0)]
        and ("0x5B_dest", (0x020D70, 0)) in d_fe_dest["extras"]
        and d_fe_dest["form_flags"] == 0x01840009
        and d_eb_s0["extras"] == [("0x56_hp", 45.0)]
        and ("0x5B_dest", (0x020D31, 2)) in d_eb_s2["extras"]
        and ("0x56_hp", 0.0) in d_eb_s3["extras"]
        and ("0x5B_dest", (0x020D31, 2)) in d_eb_s3["extras"],
        "FireExtinguisher destroyed writes 0x5B stage=0; ElectricBox01 writes 0x56 HP + 0x5B stage=2",
    )
    ok(
        "r2.achr14_tok76_magazine_rounds",
        decode_player_tok76_from_file(r14_01) == 13
        and decode_player_tok76_from_file(r14_04) == 12
        and not any(
            any("0x6E" in k for k in e)
            for it in r14_04.decode_player()["inventory"]
            for e in it["entries"]
        ),
        "r14_01 tok[76]=13 -> r14_04 tok[76]=12 with zero 0x6E on weapon",
    )

    # 12. Round 3 confirmations, bugs #12/#13 (pure 10-master vanilla), plus optional
    # GECK test-plugin checks (bug #11, 0x22/0x1C/0x30, and 0x26/0x17/0x28 negative proofs)
    r19_01 = load("r19_01_misc_setitemvalue.fos")
    r19_02 = load("r19_02_npc_crea_fullname.fos")
    r19_03 = load("r19_03_player_fullname.fos")
    r19_04 = load("r19_04_tact_fullname.fos")
    r19_06 = load("r19_06_npc_multibit.fos")
    r19b_03 = load("r19b_03_matchface.fos")
    r20_10 = load("r20_10_boone_hp.fos")
    r27_03 = load("r27_03_player_7rounds.fos")

    ok(
        "r3.misc_bit1_values",
        r19_01.decode_misc(0x07C128)["value"] == 0
        and r19_01.decode_misc(0x0340A4)["value"] == 777
        and r19_01.decode_misc(0x05B63C)["value"] == 1234,
        "HD00Infirmary=0, Plunger=777, Pencil01=1234 (MISC bit 1 i32 LE |)",
    )
    ok(
        "r3.npc_crea_bit5_names",
        r19_02.decode_npc_or_crea(10, 0x104C0C)["name"] == "Orris's Thug"
        and r19_02.decode_npc_or_crea(11, 0x10C769)["name"] == "Damaged Duraframe Eyebot"
        and r19_03.decode_npc_or_crea(10, 7)["name"] == "Orris's Thug"
        and r19_03.player_name == "Orris's Thug",
        "NPC_ 0x104C0C, CREA 0x10C769, NPC_ 0x7 all emit bit 5 u16|str|",
    )
    npc7_mb = r19_06.decode_npc_or_crea(10, 7)
    doc_mb = r19_06.decode_npc_or_crea(10, 0x104C0C)
    ok(
        "r3.npc_multibit_order_1_4_2_5_24",
        npc7_mb["flags"] == 0x01000034
        and npc7_mb["name"] == "Orris's Thug"
        and npc7_mb["sex_change_bit24"] is True
        and npc7_mb["rem"] == 0
        and doc_mb["flags"] == 0x22
        and doc_mb["name"] == "Orris's Thug"
        and doc_mb["rem"] == 0,
        "NPC_ 0x7 bits 4+2+5+24 and DocMitchell bits 1+5 parse to EOF (rem=0)",
    )
    t_rnv3 = r19_04.decode_tact(0x14DF02)
    t_v113 = r19_04.decode_tact(0x0E88AD)
    ok(
        "r3.tact_bit2_names_before_bit23",
        t_rnv3["flags"] == 0x00800004
        and t_rnv3["name"] == "Orris's Thug"
        and t_rnv3["talking_actor"] == 0x104C0F
        and t_rnv3["rem"] == 0
        and t_v113["name"] == "Orris's Thug"
        and t_v113["talking_actor"] == 0x104C0F,
        "TACT 0x14DF02 & 0x0E88AD flags=0x00800004: bit2 name| before bit23 actor|",
    )
    dec_app = r19b_03.decode_npc_or_crea(10, 7)
    ok(
        "r3.npc7_bit11_player_appearance",
        dec_app["flags"] == 0x00000800
        and dec_app["len"] == 676
        and dec_app["rem"] == 0
        and dec_app["appearance"]["hair"] == 0x0002DDEE
        and dec_app["appearance"]["eyes"] == 0x00004253
        and dec_app["appearance"]["hcol"] == 0x002E4654
        and dec_app["appearance"]["parts"] == [0x000B5BEA],
        "NPC_ 0x7 bit11 len=676 rem=0 (50 fggs + 30 fgga + 50 fgts + hair/eyes/hlen/hcol/parts)",
    )

    # BUG #12: Item extra 0x3E (EquipItem <item> 1)
    boone_legacy = r20_10.decode_actor(1, 0x096BCE, strict_legacy_0x3e=True)
    boone_fixed = r20_10.decode_actor(1, 0x096BCE, strict_legacy_0x3e=False)
    ok(
        "bug12.item_extra_0x3E_crashes_parse_item_list",
        "unknown item extra 0x3E" in str(boone_legacy.get("err", ""))
        and any(
            any(e.get("0x3E_cannot_wear_or_unequip") is True for e in it["entries"])
            for it in boone_fixed["inventory"]
        ),
        "r20_10_boone_hp (EquipItem 9mm 1): legacy fails on 0x3E; fixed parses 0x3E cleanly",
    )

    # BUG #13: 0x6E + 0x39 on NPC-dropped WEAP REFRs (absent in player inv)
    drop_357 = r27_03.decode_refr(0xFF001315)
    drop_9mm = r27_03.decode_refr(0xFF001316)
    ok(
        "bug13.extra_0x6E_confirmed_on_dropped_refr_and_absent_in_player_inv",
        ("0x6E_ammo", (0x08ED02, 5)) in drop_357["extras"]
        and ("0x39_dropped_from", 0xFF001314) in drop_357["extras"]
        and ("0x6E_ammo", (0x08ED03, 4)) in drop_9mm["extras"]
        and ("0x39_dropped_from", 0xFF001312) in drop_9mm["extras"],
        "0x6E (ref3|u32|) + 0x39 confirmed on NPC-dropped 0xFF001315 (.357:5) & 0xFF001316 (9mm:4)",
    )

    # Optional manual GECK test-plugin captures (run when present in --saves-dir)
    optional_geck_files = (
        "r21_02_pickup_xglb_orig_scale.fos",
        "r21_03_cont_and_drop_picked.fos",
        "r21_04_torch_equipped.fos",
        "r21_08_rings_dual_two_copies.fos",
        "r23_01_torch_equip_log.fos",
        "r23_04_combat_after.fos",
    )
    if all((sdir / fn).is_file() for fn in optional_geck_files):
        r21_02 = load("r21_02_pickup_xglb_orig_scale.fos")
        r21_03 = load("r21_03_cont_and_drop_picked.fos")
        r21_04 = load("r21_04_torch_equipped.fos")
        r21_08 = load("r21_08_rings_dual_two_copies.fos")
        r23_01 = load("r23_01_torch_equip_log.fos")
        r23_04 = load("r23_04_combat_after.fos")

        p21_02 = r21_02.decode_player()
        pen_entries = next(it["entries"] for it in p21_02["inventory"] if it["item"] == 0x05B63C)
        plu_entries = next(it["entries"] for it in p21_02["inventory"] if it["item"] == 0x0340A4)
        dr21 = r21_03.decode_refr(0x103E37)
        dr_pen = next(it["entries"] for it in dr21["inventory"] if it["item"] == 0x05B63C)
        ok(
            "r3.item_extra_0x22_xglb",
            any(e.get("0x22") == 0x38 for e in pen_entries)
            and any(e.get("0x22") == 0x38 for e in plu_entries)
            and any(e.get("0x22") == 0x38 for e in dr_pen)
            and ("0x22", 0x38) in r21_03.decode_refr(0x0A000813)["extras"],
            "0x22 ref3(0x38 GameHour)| confirmed in player inv, container inv, & re-dropped REFR",
        )
        ok(
            "r3.item_extra_0x1C_orig_ref",
            any(e.get("0x1C") == 0x0A000810 for e in pen_entries)
            and any(e.get("0x1C") == 0x0A000813 for e in pen_entries)
            and any(e.get("0x1C") == 0x0A000810 for e in dr_pen),
            "0x1C ref3(orig REFR)| confirmed in player inv & container inv",
        )
        ok(
            "r3.item_extra_0x30_scale",
            any(e.get("0x30") == 1.5 for e in pen_entries)
            and any(e.get("0x30") == 1.75 for e in plu_entries)
            and any(e.get("0x30") == 1.5 for e in dr_pen),
            "0x30 f32(1.5 / 1.75)| confirmed in player inv & container inv",
        )

        # BUG #11: REFR bit 10 (0x400) for 0x1C holder on picked-up world REFRs
        picked_legacy = r21_02.decode_refr(0x0A000810, include_bit6_bit10=False)
        picked_fixed = r21_02.decode_refr(0x0A000810, include_bit6_bit10=True)
        ok(
            "bug11.refr_bit10_0x1C_omitted_from_extra_bits",
            picked_legacy["flags"] == 0x00000403
            and picked_legacy["rem"] == 8
            and picked_fixed["rem"] == 0
            and ("0x1C", 0x14) in picked_fixed["extras"],
            "REFR 0x0A000810 flags=0x403: legacy rem=8, fixed parses 0x1C=0x14 with rem=0",
        )

        # Negative proofs: 0x26, 0x17, 0x28
        ok(
            "r3.neg_0x26_carried_ligh_unequippable",
            next(
                it["entries"] for it in r23_01.decode_player()["inventory"] if it["item"] == 0x0A000801
            )
            == []
            and all(
                "0x26" not in e
                for it in r21_04.decode_player()["inventory"]
                for e in it["entries"]
            ),
            "GECK CanBeCarried LIGH (Time=120s) refused equip, zero 0x26 emitted",
        )
        ok(
            "r3.neg_0x17_worn_left_unused",
            next(
                it["entries"] for it in r21_08.decode_player()["inventory"] if it["item"] == 0x0A000802
            )[0].get("0x16_worn")
            is True
            and all(
                "0x17_worn" not in e
                for it in r21_08.decode_player()["inventory"]
                for e in it["entries"]
            ),
            "Dual-slot ring BMDT 0x180 equipped twice wears 1 copy with 0x16, zero 0x17 emitted",
        )
        foe_k = r23_04.get_rec(1, 0xFF001311)["data"]
        foe_p = r23_04.get_rec(1, 0xFF001313)["data"]
        ok(
            "r3.neg_0x28_weapon_charge_unused",
            b"\x28\x7c" not in foe_k[:200] and b"\x28\x7c" not in foe_p[:200],
            "GECK EITM(charge=500,cost=25)+EAMT=500 knife & pistol dealt live combat damage with zero 0x28",
        )

    return checks


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        description="Verify 61 pure-vanilla (+ 7 optional manual-GECK-plugin) clean-room A/B `.fos` capture invariants (68 total) against a local capture directory."
    )
    ap.add_argument(
        "--saves-dir",
        default=os.environ.get("FNV_FOS_SAVES_DIR"),
        help="Private directory containing captured `.fos` save pairs (or env FNV_FOS_SAVES_DIR).",
    )
    ap.add_argument("--json", action="store_true", help="Emit machine-readable JSON results.")
    args = ap.parse_args(argv)

    if not args.saves_dir:
        ap.error("--saves-dir (or FNV_FOS_SAVES_DIR) is required.")

    results = verify_directory(args.saves_dir)
    passed = sum(1 for r in results if r.status == "PASS")

    if args.json:
        print(
            json.dumps(
                {
                    "passed": passed,
                    "total": len(results),
                    "checks": [{"status": r.status, "tag": r.tag, "detail": r.detail} for r in results],
                },
                indent=2,
            )
        )
    else:
        for r in results:
            print(f"[{r.status}] {r.tag}: {r.detail}")
        print(f"\nSUMMARY: {passed}/{len(results)} assertions PASSED.")

    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    sys.exit(main())
