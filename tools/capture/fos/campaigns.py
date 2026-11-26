"""Declarative single-variable A/B `.fos` capture campaign definitions (Rounds 1-3).

Covers pure-vanilla 10-master probe campaigns exercising `.fos` converter keys
and parser/converter regression triggers using only `BuildRef 0 <decimal_id>`
and vanilla commands (zero custom `.esp` plugins generated or required).
Optional rare-extra probes requiring a manually authored GECK test plugin
(`XGLB` `0x22`, carried `LIGH` `0x26`, dual-slot ring `0x17`, charged `WEAP`
`0x28`) are documented in `tools/capture/fos/README.md`.
"""

from __future__ import annotations

from dataclasses import dataclass


def bref(var: str, form_id: int, mod_index: int = 0) -> str:
    """Emit a `ref <var> = BuildRef <mod_index> <decimal_id>` line."""
    return f"ref {var} = BuildRef {mod_index} {form_id}"


def _log_ammo(tag: str) -> tuple[str, ...]:
    return (
        "int rCnt = GetPlayerCurrentAmmoRounds",
        "ref aCur = GetPlayerCurrentAmmo",
        f'string_var sLog = sv_Construct "{tag} ammo=%i rounds=%g" aCur rCnt',
        'PrintToConsole "FOSPROBE %z" sLog',
    )


@dataclass(frozen=True)
class ProbeStep:
    """One atomic A/B capture step: commands to execute, settle ticks, and save stem."""

    name: str
    commands: tuple[str, ...] = ()
    wait_ticks: int = 0
    note: str = ""


@dataclass(frozen=True)
class Campaign:
    """A self-contained single-session A/B `.fos` capture campaign."""

    name: str
    round_num: int
    description: str
    steps: tuple[ProbeStep, ...]
    interval: int = 15
    max_wait: int = 140
    start_chargen_quest: str = "00102037"


# ---------------------------------------------------------------------------
# Round 1 campaigns (core player, world, actors, containers, globals, edge)
# ---------------------------------------------------------------------------

RUN1_PLAYER = Campaign(
    name="run1_player",
    round_num=1,
    description=(
        "Player ACHR 0x14 scale bit-4 gating (BUG #1), AV2 permanent/XP, "
        "AV3 AP/limb damage, ModAV radiation/hardcore in AV2 (BUG #7), and "
        "bit-22 non-skill SetAV Luck (BUG #2)."
    ),
    max_wait=95,
    steps=(
        ProbeStep("p00_base", (), 0, "Clean baseline in GSDocMitchellHouse (bit 4 unset)"),
        ProbeStep("p01_scale", ("Player.SetScale 1.25",), 0, "Sets ACHR 0x14 bit 4 scale"),
        ProbeStep(
            "p02_av_perm",
            ("Player.ModAV Strength 3", "Player.ModAV Speech 12", "RewardXP 500"),
            0,
            "AV2 permanent modifiers + XP (code 24)",
        ),
        ProbeStep(
            "p03_av_dmg",
            (
                "Player.DamageAV ActionPoints 15",
                "Player.DamageAV PerceptionCondition 40",
                "Player.DamageAV LeftAttackCondition 30",
            ),
            0,
            "AV3 action-point and limb condition damage",
        ),
        ProbeStep(
            "p04_rads_hc",
            (
                "Player.ModAV RadiationRads 125",
                "Player.ModAV Dehydration 150",
                "Player.ModAV Hunger 175",
                "Player.ModAV SleepDeprevation 200",
            ),
            1,
            "BUG #7: ModAV rads/hardcore needs land in AV2",
        ),
        ProbeStep(
            "p08_special_name",
            ("Player.SetAV Luck 9",),
            0,
            "BUG #2: SetAV non-skill code 11 lands in ACHR bit 22",
        ),
    ),
)

RUN1B_PLAYER = Campaign(
    name="run1b_player",
    round_num=1,
    description=(
        "Player factions (0x5E), perks, NPC_ bit-4 spells, equipped inventory "
        "(0x16/0x25/0x4A), active effects (t0/t3), karma/AV[231] HP damage, and "
        "Pip-Boy radio ON in Global[10] list2 (BUG #3)."
    ),
    max_wait=110,
    steps=(
        ProbeStep("pb00_base", (), 0),
        ProbeStep(
            "pb01_factions",
            (bref("rFac", 0x104C6E), "Player.SetFactionRank rFac 2"),
            0,
        ),
        ProbeStep(
            "pb02_perks",
            (
                bref("rP1", 0x031DD3),
                bref("rP2", 0x094EB9),
                "Player.AddPerk rP1",
                "Player.AddPerk rP2",
            ),
            0,
        ),
        ProbeStep(
            "pb03_spells",
            (bref("rSp", 0x1768BC), "Player.AddSpell rSp"),
            0,
        ),
        ProbeStep(
            "pb04_teammate",
            (
                bref("rDoc", 0x104C0F),
                "rDoc.SetPlayerTeammate 1",
                bref("rTP", 0x15C60E),
                "Player.AddPerk rTP",
            ),
            0,
        ),
        ProbeStep(
            "pb05_inv_equip_cond_hotkey",
            (
                bref("r9mm", 0x0E3778),
                bref("rA9", 0x08ED03),
                bref("r10mm", 0x00434F),
                bref("rA10", 0x004241),
                bref("rArm", 0x020423),
                bref("rCap", 0x00000F),
                "Player.AddItemHealthPercent r9mm 1 0.50",
                "Player.AddItem rA9 50",
                "Player.EquipItem r9mm",
                "Player.AddItemHealthPercent r10mm 1 0.60",
                "Player.AddItem rA10 40",
                "Player.AddItemHealthPercent rArm 1 0.75",
                "Player.EquipItem rArm",
                "Player.AddItem rCap 123",
                "SetHotkey 3 r9mm",
                "SetHotkey 5 rArm",
            ),
            2,
        ),
        ProbeStep("pb06_equip_10mm", (bref("r10mm", 0x00434F), "Player.EquipItem r10mm"), 3),
        ProbeStep("pb07_swap_back_9mm", (bref("r9mm", 0x0E3778), "Player.EquipItem r9mm"), 2),
        ProbeStep(
            "pb08_active_effect_t0",
            (
                bref("rBuf", 0x015163),
                bref("rRad", 0x015168),
                "Player.AddItem rBuf 1",
                "Player.EquipItem rBuf",
                "Player.AddItem rRad 1",
                "Player.EquipItem rRad",
            ),
            1,
        ),
        ProbeStep("pb09_active_effect_t3", (), 14),
        ProbeStep(
            "pb10_karma_hp_radio",
            (
                "RewardKarma 250",
                "Player.DamageAV Health 40",
                "Player.DamageAV RadiationRads -100",
            ),
            2,
        ),
        ProbeStep(
            "pb11_radio_on",
            (bref("rRad", 0x14DF03), "PipBoyRadio On rRad"),
            2,
        ),
    ),
)

RUN2_WORLD = Campaign(
    name="run2_world",
    round_num=1,
    description=(
        "Non-player ACHR scale/faction/AV21/bit23 ModAV (BUG #6), NPC inventory, "
        "SetEssential, ash pile (0x89), REFR locks/doors/containers/bit-6 owner "
        "(BUG #4), map markers (0x2C), created 0xFF, FACT/FLST/LVLI/INFO, "
        "terminal lockout (0x50), exterior weather Global[8], and CELL detach."
    ),
    max_wait=120,
    steps=(
        ProbeStep("w00_base", (), 0),
        ProbeStep(
            "w01_actor_scale_fac_av",
            (
                bref("rDoc", 0x104C0F),
                bref("rFac", 0x104C6E),
                "rDoc.SetScale 1.35",
                "rDoc.SetFactionRank rFac 2",
                "rDoc.ModAV Strength 4",
                "rDoc.DamageAV Health 15",
                "rDoc.DamageAV LeftAttackCondition 25",
            ),
            0,
        ),
        ProbeStep(
            "w02_actor_inv_equip",
            (
                bref("rDoc", 0x104C0F),
                bref("r9mm", 0x0E3778),
                bref("rA9", 0x08ED03),
                "rDoc.AddItem r9mm 1",
                "rDoc.AddItem rA9 20",
                "rDoc.EquipItem r9mm",
            ),
            1,
        ),
        ProbeStep(
            "w03_actor_dead_base",
            (
                bref("rDoc", 0x104C0F),
                bref("rDocBase", 0x104C0C),
                "rDoc.Kill",
                "SetEssential rDocBase 1",
            ),
            2,
        ),
        ProbeStep(
            "w04_actor_ashpile",
            (bref("rDoc", 0x104C0F), "rDoc.AttachAshPile 1"),
            1,
        ),
        ProbeStep(
            "w05_refs_scale_lock_door_cont",
            (
                bref("rDoor", 0x1062AC),
                "rDoor.SetOpenState 1",
                bref("rDoorLock", 0x103E61),
                "rDoorLock.Lock 50",
                bref("rDresser", 0x103E37),
                bref("rCap", 0x00000F),
                "rDresser.AddItem rCap 42",
                bref("rContLock", 0x103E49),
                "rContLock.Lock 75",
                "rContLock.AddItem rCap 17",
                bref("rContOwn", 0x103E4A),
                bref("rDocBase", 0x104C0C),
                "rContOwn.SetOwnership rDocBase",
                "rContOwn.SetScale 1.5",
            ),
            1,
        ),
        ProbeStep("w06_mapmarker_vis", (bref("rMM", 0x1088F6), "ShowMap rMM 0"), 0),
        ProbeStep("w07_mapmarker_travel", (bref("rMM", 0x1088F6), "ShowMap rMM 1"), 0),
        ProbeStep(
            "w08_created_0xff",
            (
                bref("rPen", 0x05B63C),
                bref("rNpc", 0x15F312),
                "Player.PlaceAtMe rPen 1",
                "Player.PlaceAtMe rNpc 1",
            ),
            2,
        ),
        ProbeStep(
            "w09_fact_repu_note_flst_lvli",
            (
                bref("rF1", 0x104C6E),
                bref("rF2", 0x104C7E),
                bref("rF3", 0x15EC58),
                "SetEnemy rF1 rF2",
                "SetAlly rF1 rF3",
                bref("rRep", 0x104C22),
                "AddReputation rRep 1 15",
                "AddReputation rRep 0 7",
                bref("rNote", 0x177875),
                "Player.AddNote rNote",
                bref("rFlst", 0x17BA3B),
                bref("r9mm", 0x0E3778),
                "AddFormToFormList rFlst r9mm",
                bref("rLvli", 0x16BACB),
                "AddItemToLeveledList rLvli r9mm 3 2 0.75",
            ),
            1,
        ),
        ProbeStep(
            "w11_terminal_pgre",
            (bref("rTerm", 0x11A30A), "rTerm.SetLockedOut 1"),
            1,
        ),
        ProbeStep(
            "w12_exterior_weather",
            (bref("rW", 0x0FFC88), "CenterOnCell Goodsprings", "ForceWeather rW"),
            4,
        ),
    ),
)

RUN3_FINAL = Campaign(
    name="run3_final",
    round_num=1,
    description=(
        "ECZN bit 1, CELL bit 1 + bit 3 owner after fog (BUG #5), MISC bit 0, "
        "CLAS tag skills, teammate perks, FACT bit 1 flags, and radio playback advance."
    ),
    max_wait=120,
    steps=(
        ProbeStep("r00_base", (), 0),
        ProbeStep("r01_eczn_respawns", (bref("rZ", 0x131F33), "SetZoneRespawns rZ 0"), 0),
        ProbeStep(
            "r02_cell_public_owner",
            (
                bref("rC", 0x103DF9),
                bref("rDoc", 0x104C0C),
                "SetCellPublicFlag rC 1",
                "SetCellOwnership rC rDoc",
            ),
            0,
        ),
        ProbeStep("r03_misc_quest_val", (bref("rP", 0x05B63C), "SetQuestObject rP 1"), 0),
        ProbeStep(
            "r06_kill_dismember",
            (
                bref("rNpc", 0x15F312),
                "ref rSpawn = Player.PlaceAtMe rNpc 1",
                "rSpawn.KillActor Player 1",
            ),
            2,
        ),
        ProbeStep(
            "r08b_tag_skills",
            (
                "Player.SetPlayerTagSkill Speech 1",
                "Player.SetPlayerTagSkill Science 1",
                "Player.SetPlayerTagSkill Lockpick 1",
            ),
            0,
        ),
        ProbeStep(
            "r09_teammate_perk",
            (bref("rP1", 0x031DD3), "Player.AddPerk rP1 1"),
            0,
        ),
        ProbeStep(
            "r15_radio_wait",
            (bref("rRad", 0x14DF03), "PipBoyRadio On rRad"),
            25,
        ),
    ),
)

RUN4_EDGE = Campaign(
    name="run4_edge",
    round_num=1,
    description="REPU bit 1 exact fame/infamy, AddNote -> ACHR 0x14 tail (BUG #9), and CHAL bit 1.",
    max_wait=90,
    steps=(
        ProbeStep("e00_base", (), 0),
        ProbeStep(
            "e01_repu",
            (
                bref("rRep", 0x104C22),
                "AddReputationExact rRep 1 15",
                "AddReputationExact rRep 0 7",
            ),
            0,
        ),
        ProbeStep("e02_note", (bref("rNote", 0x177875), "AddNote rNote"), 0),
    ),
)

RUN5_SURGICAL = Campaign(
    name="run5_surgical",
    round_num=1,
    description="NPC_ 0x7 bit 24 SexChange (BUG #8) and 0x8D weapon mod flags (equipped & unequipped).",
    max_wait=90,
    steps=(
        ProbeStep("s00_base", (), 0),
        ProbeStep("s02_sex_change", ("Player.SexChange",), 1),
        ProbeStep(
            "s03_equip_9mm",
            (
                bref("r9mm", 0x0E3778),
                bref("rA9", 0x08ED03),
                bref("rAHP", 0x13E43A),
                bref("r10mm", 0x00434F),
                bref("rA10", 0x004241),
                "Player.AddItem r9mm 1",
                "Player.AddItem rA9 50",
                "Player.AddItem rAHP 30",
                "Player.AddItem r10mm 1",
                "Player.AddItem rA10 40",
                "Player.EquipItem r9mm",
                "Player.SetAlert 1",
            ),
            3,
        ),
        ProbeStep(
            "s04_mod_ammo_0x8d_0x6e",
            (
                bref("rAHP", 0x13E43A),
                "SetEquippedWeaponModFlags 3",
                "Player.EquipItem rAHP",
                "SetPlayerCurrentAmmoRounds 7",
            ),
            3,
        ),
        ProbeStep("s05_unequip_9mm", (bref("r10mm", 0x00434F), "Player.EquipItem r10mm"), 3),
    ),
)

RUN6_LAST = Campaign(
    name="run6_last",
    round_num=1,
    description=(
        "QUST bit 2 delay, FACT bit 2 modifier, AddNote -> ACHR 0x14 tail (BUG #9), "
        "REFR Disable bit 0, and KillActor FORM_DEAD + 0x5F dismembered limbs + Global[2]."
    ),
    max_wait=90,
    steps=(
        ProbeStep("q00_base", (), 0),
        ProbeStep("q01_misc_stat", ("ModPCMiscStat 0 7",), 0),
        ProbeStep(
            "q02_qust",
            (
                bref("rQ", 0x10A213),
                "StartQuest rQ",
                "SetQuestDelay rQ 2.5",
                "SetObjectiveDisplayed rQ 10 1",
                "SetObjectiveCompleted rQ 10 1",
            ),
            1,
        ),
        ProbeStep(
            "q03_facmod",
            (
                bref("rF1", 0x104C6E),
                bref("rF2", 0x104C7E),
                "SetFactionReaction rF1 rF2 42",
            ),
            0,
        ),
        ProbeStep("q04_note", (bref("rNote", 0x175AB0), "AddNote rNote"), 0),
        ProbeStep("q05_door_disable", (bref("rDoor", 0x1062AC), "rDoor.Disable"), 0),
        ProbeStep(
            "q06_doc_kill_dismember",
            (bref("rDoc", 0x104C0F), "rDoc.KillActor Player 1"),
            3,
        ),
        ProbeStep("q08_ammo_rounds", ("SetPlayerCurrentAmmoRounds 7",), 2),
    ),
)

# ---------------------------------------------------------------------------
# Round 2 campaigns (Global[1], QUST, 0x1D, ECZN 31, PGRE, TACT 23, 0x5B, etc.)
# ---------------------------------------------------------------------------

RUN7_AMMO_QUEST_COMP = Campaign(
    name="run7_ammo_quest_comp",
    round_num=2,
    description="Global[1] active_quest and QUST bits 1/2/29/30/31.",
    max_wait=160,
    steps=(
        ProbeStep("r7_00_base", (), 0),
        ProbeStep(
            "r7_06_unequip_9mm",
            (bref("r10mm", 0x00434F), "Player.AddItem r10mm 1", "Player.EquipItem r10mm"),
            2,
        ),
        ProbeStep(
            "r7_07_qust_vcg02",
            (
                bref("rQ", 0x10A214),
                "StartQuest rQ",
                "SetStage rQ 10",
                "SetQuestDelay rQ 2.5",
                "SetObjectiveDisplayed rQ 10 1",
                "SetObjectiveCompleted rQ 10 1",
                "SetObjectiveDisplayed rQ 20 1",
            ),
            2,
        ),
        ProbeStep(
            "r7_08_qust_vms01_active",
            (
                bref("rQ2", 0x080664),
                "StartQuest rQ2",
                "SetStage rQ2 10",
                "SetObjectiveDisplayed rQ2 10 1",
                "SetCurrentQuest rQ2",
            ),
            2,
        ),
    ),
)

RUN8_VAULT11_WORLD = Campaign(
    name="run8_vault11_world",
    round_num=2,
    description=(
        "ECZN bit 31 (BisonSteve01 + Vault11c), 1-stage destructible 0x56/0x5B stage 0 "
        "(BUG #10), PGRE bits 0-3, world NOTE pickup -> ACHR 0x14 tail (BUG #9), and TACT bit 23."
    ),
    max_wait=140,
    steps=(
        ProbeStep("r8_00_base", (), 0),
        ProbeStep("r8_01_bisonsteve_eczn", ("CenterOnCell BisonSteve01",), 6),
        ProbeStep("r8_02_vault11c_eczn", ("CenterOnCell Vault11c",), 6),
        ProbeStep(
            "r8_03_dest_partial",
            (bref("rFE", 0x0EA490), "rFE.DamageObject 1"),
            2,
        ),
        ProbeStep(
            "r8_04_dest_destroy",
            (bref("rFE", 0x0EA490), "rFE.DamageObject 9999"),
            3,
        ),
        ProbeStep(
            "r8_05_pgre_move_disable",
            (
                bref("rM1", 0x0EA807),
                bref("rM2", 0x0EA808),
                "float z1 = rM1.GetPos Z",
                "rM1.SetPos Z (z1 + 25.0)",
                "rM2.Disable",
            ),
            3,
        ),
        ProbeStep(
            "r8_06_note_pickup",
            (bref("rNoteRef", 0x0E8893), "rNoteRef.Activate Player 1"),
            2,
        ),
        ProbeStep(
            "r8_07_tact_actor_ref",
            (
                bref("rTactRef", 0x0E889D),
                bref("rDocRef", 0x104C0F),
                bref("rRadioRef", 0x14DF03),
                "rTactRef.SetTalkingActivatorActor rDocRef",
                "rRadioRef.SetTalkingActivatorActor rDocRef",
            ),
            2,
        ),
    ),
)

RUN9_EXTRAS_NPC_0XFF = Campaign(
    name="run9_extras_npc_0xff",
    round_num=2,
    description="Player inventory item extras 0x21 (owner) & 0x23 (rank), plus NPC_ 0x7 bit 2 SPECIAL.",
    max_wait=160,
    steps=(
        ProbeStep("r9_00_base", (), 0),
        ProbeStep(
            "r9_02_pickup_own_scale_orig",
            (
                bref("rPlunger", 0x1059AE),
                bref("rDocBase", 0x104C0C),
                "rPlunger.SetOwnership rDocBase",
                "rPlunger.Activate Player 1",
            ),
            2,
        ),
        ProbeStep("r9_03_gomorrah_xrnk", ("CenterOnCell Gomorrah01",), 5),
        ProbeStep(
            "r9_04_pickup_xrnk",
            (bref("rGlass", 0x11FCC5), "rGlass.Activate Player 1"),
            2,
        ),
        ProbeStep("r9_12_npc_modpcattrib", ("ModPCAttribute Strength 2",), 1),
    ),
)

RUN10_SURGICAL = Campaign(
    name="run10_surgical",
    round_num=2,
    description=(
        "0x1D player teammates, 0xFF created REFR/ACRE script ref vars, and "
        "4-stage destructible ElectricBox01 (0x56 HP + 0x5B stage=2)."
    ),
    max_wait=180,
    steps=(
        ProbeStep("r10_00_base", (), 0),
        ProbeStep(
            "r10_08_comp_hired",
            (
                bref("rQ", 0x0B16D0),
                bref("rBoone", 0x096BCE),
                'SetVariable "bBooneHired" 1 rQ',
                "rBoone.MoveTo Player",
                "rBoone.SetPlayerTeammate 1",
                'SetVariable "HasBeenHired" 1 rBoone',
                'SetVariable "IsFollowingDefault" 1 rBoone',
                'SetVariable "Waiting" 0 rBoone',
                "rBoone.EvaluatePackage",
            ),
            12,
        ),
        ProbeStep(
            "r10_09_0xff_self_refvars",
            (
                bref("rSpk", 0x158679),
                bref("rSec", 0x119790),
                "Player.PlaceAtMe rSpk 1",
                "Player.PlaceAtMe rSec 1",
            ),
            5,
        ),
        ProbeStep("r10_11_hoover_dest_base", ("CenterOnCell HooverDamIntPowerPlant01",), 6),
        ProbeStep("r10_12_dest_stage0", (bref("rBox", 0x14DB1C), "rBox.DamageObject 5"), 2),
        ProbeStep("r10_13_dest_stage1", (bref("rBox", 0x14DB1C), "rBox.DamageObject 10"), 2),
        ProbeStep("r10_14_dest_stage2", (bref("rBox", 0x14DB1C), "rBox.DamageObject 20"), 2),
        ProbeStep("r10_15_dest_stage3", (bref("rBox", 0x14DB1C), "rBox.DamageObject 100"), 3),
    ),
)

RUN14_TOK76_MAGAZINE = Campaign(
    name="run14_tok76_magazine",
    round_num=2,
    description="Active equipped weapon magazine count in ACHR 0x14 process block tok[76] (BUG #13).",
    max_wait=160,
    steps=(
        ProbeStep("r14_00_base", (), 0),
        ProbeStep(
            "r14_01_equip_9mm",
            (
                bref("r9mm", 0x0E3778),
                bref("rA9", 0x08ED03),
                bref("rAHP", 0x13E43A),
                bref("r10mm", 0x00434F),
                bref("rA10", 0x004241),
                "Player.AddItem r9mm 1",
                "Player.AddItem rA9 50",
                "Player.AddItem rAHP 30",
                "Player.AddItem r10mm 1",
                "Player.AddItem rA10 40",
                "Player.EquipItem r9mm",
                "Player.SetAlert 1",
            ),
            6,
        ),
        ProbeStep(
            "r14_02_swap_hp_wait",
            _log_ammo("S02_INIT") + (bref("rAHP", 0x13E43A), "Player.EquipItem rAHP"),
            25,
        ),
        ProbeStep(
            "r14_03_fire_hp_1",
            _log_ammo("S03_AFTER_SWAP") + (bref("r9mm", 0x0E3778), "Player.FireWeapon r9mm"),
            6,
        ),
        ProbeStep(
            "r14_04_fire_hp_2",
            _log_ammo("S04_AFTER_FIRE1") + (bref("r9mm", 0x0E3778), "Player.FireWeapon r9mm"),
            6,
        ),
        ProbeStep(
            "r14_05_unequip_9mm",
            _log_ammo("S05_BEFORE_UNEQUIP")
            + (bref("r10mm", 0x00434F), "Player.EquipItem r10mm"),
            8,
        ),
    ),
)

# ---------------------------------------------------------------------------
# Round 3 campaigns (final 12 targets, BUG #11/#12/#13, negative proofs)
# ---------------------------------------------------------------------------

RUN19A_NAMES_MISC_FACE = Campaign(
    name="run19a_names_misc_face",
    round_num=3,
    description=(
        "MISC bit 1 SetItemValue, NPC_/CREA bit 5 SetActorFullName, TACT bit 2 + bit 23 "
        "ordering, and NPC_ multi-bit ordering (bits 1 -> 4 -> 2 -> 5 -> 24)."
    ),
    max_wait=120,
    steps=(
        ProbeStep("r19_00_base", ("SetInChargen 0", "EnablePlayerControls 1 1 1 1 1 1 1"), 2),
        ProbeStep(
            "r19_01_misc_setitemvalue",
            (
                bref("rHD00", 0x07C128),
                bref("rPlungerRef", 0x1059AE),
                bref("rPenBase", 0x05B63C),
                "Player.AddItem rHD00 1",
                "rPlungerRef.SetItemValue 777",
                "ref rPenRef = Player.PlaceAtMe rPenBase 1",
                "rPenRef.SetItemValue 1234",
            ),
            3,
        ),
        ProbeStep(
            "r19_02_npc_crea_fullname",
            (
                bref("rDocRef", 0x104C0F),
                bref("rMsgThug", 0x171DBF),
                bref("rEDERef", 0x1732D1),
                bref("rMsgEDE", 0x1717D3),
                "rDocRef.SetActorFullName rMsgThug",
                "rEDERef.MoveTo Player",
                "rEDERef.SetActorFullName rMsgEDE",
            ),
            3,
        ),
        ProbeStep(
            "r19_03_player_fullname",
            (bref("rMsgThug", 0x171DBF), "Player.SetActorFullName rMsgThug"),
            2,
        ),
        ProbeStep(
            "r19_04_tact_fullname",
            (
                bref("rRadioRef", 0x14DF03),
                bref("rTactRef", 0x0E889D),
                bref("rDocRef", 0x104C0F),
                bref("rMsgThug", 0x171DBF),
                "rRadioRef.SetTalkingActivatorActor rDocRef",
                "rRadioRef.SetActorFullName rMsgThug",
                "rTactRef.SetTalkingActivatorActor rDocRef",
                "rTactRef.SetActorFullName rMsgThug",
            ),
            3,
        ),
        ProbeStep(
            "r19_06_npc_multibit",
            (
                bref("rDocBase", 0x104C0C),
                bref("rSpell", 0x1768BC),
                "SetEssential rDocBase 1",
                "ModPCAttribute Strength 2",
                "Player.AddSpell rSpell",
                "Player.SexChange",
            ),
            2,
        ),
    ),
)

RUN19B_APPEARANCE = Campaign(
    name="run19b_appearance",
    round_num=3,
    description="NPC_ 0x7 bit 11 player_appearance (676B FaceGen payload via MatchFaceGeometry).",
    max_wait=120,
    start_chargen_quest="",
    steps=(
        ProbeStep("r19b_00_base", ("SetInChargen 0", "EnablePlayerControls 1 1 1 1 1 1 1"), 2),
        ProbeStep(
            "r19b_02_matchrace",
            (bref("rDoc", 0x104C0F), "Player.MatchRace rDoc"),
            2,
        ),
        ProbeStep(
            "r19b_03_matchface",
            (bref("rDoc", 0x104C0F), "Player.MatchFaceGeometry rDoc 100"),
            2,
        ),
    ),
)

RUN20A_AMMO_AND_NOUNEQUIP = Campaign(
    name="run20a_ammo_and_nounequip",
    round_num=3,
    description="Alt-ammo checks across weapon classes and EquipItem noUnequip=1 emitting 0x3E (BUG #12).",
    max_wait=160,
    steps=(
        ProbeStep("r20_00_base", ("SetInChargen 0", "EnablePlayerControls 1 1 1 1 1 1 1"), 2),
        ProbeStep(
            "r20_10_boone_hp",
            (
                bref("rBoone", 0x096BCE),
                bref("r9mm", 0x0E3778),
                bref("rAHP", 0x13E43A),
                "rBoone.MoveTo Player",
                "rBoone.AddItem r9mm 1",
                "rBoone.AddItem rAHP 30",
                "rBoone.EquipItem r9mm 1",
                "rBoone.SetAlert 1",
                "rBoone.FireWeapon r9mm",
            ),
            6,
        ),
    ),
)

RUN27_DEAD_NPC_DROP_0X6E = Campaign(
    name="run27_dead_npc_drop_0x6e",
    round_num=3,
    description=(
        "Pure 10-master vanilla combat death drop emitting 0x6E (ref3|u32|) + 0x39 "
        "on dropped 0xFF WEAP REFRs, paired with player-inventory absence check (BUG #13)."
    ),
    max_wait=120,
    steps=(
        ProbeStep(
            "r27_00_base",
            ("SetInChargen 0", "EnablePlayerControls 1 1 1 1 1 1 1", "Player.SetAV Health 5000"),
            2,
        ),
        ProbeStep(
            "r27_01_foe_shooting",
            (
                bref("rPG", 0x15F312),
                bref("r9mm", 0x0E3778),
                bref("rAHP", 0x13E43A),
                bref("r357", 0x08F216),
                bref("rA357J", 0x140A9E),
                "ref rF1 = Player.PlaceAtMe rPG 1",
                "rF1.RemoveAllItems",
                "rF1.AddItem r9mm 1",
                "rF1.AddItem rAHP 30",
                "rF1.EquipItem r9mm",
                "rF1.SetAV Aggression 3",
                "rF1.StartCombat Player",
                'Player.AuxVarSetRef "*f1" rF1',
                "ref rF2 = Player.PlaceAtMe rPG 1",
                "rF2.RemoveAllItems",
                "rF2.AddItem r357 1",
                "rF2.AddItem rA357J 24",
                "rF2.EquipItem r357",
                "rF2.SetAV Aggression 3",
                "rF2.StartCombat Player",
                'Player.AuxVarSetRef "*f2" rF2',
            ),
            30,
        ),
        ProbeStep(
            "r27_02_foes_killed_drop",
            (
                'ref rF1 = Player.AuxVarGetRef "*f1"',
                'ref rF2 = Player.AuxVarGetRef "*f2"',
                "rF1.KillActor Player",
                "rF2.KillActor Player",
            ),
            8,
        ),
        ProbeStep(
            "r27_03_player_7rounds",
            (
                bref("r9mm", 0x0E3778),
                bref("rA9", 0x08ED03),
                bref("rAHP", 0x13E43A),
                "Player.AddItem r9mm 1",
                "Player.AddItem rA9 50",
                "Player.AddItem rAHP 30",
                "Player.EquipItem r9mm",
                "Player.EquipItem rAHP",
                "Player.SetCurrentAmmoRounds 7",
            ),
            4,
        ),
        ProbeStep("r27_04_player_unequip_7rounds", (bref("r9mm", 0x0E3778), "Player.UnequipItem r9mm"), 3),
        ProbeStep(
            "r27_05_player_drop_7rounds",
            (
                bref("r9mm", 0x0E3778),
                bref("rAHP", 0x13E43A),
                "Player.EquipItem r9mm",
                "Player.EquipItem rAHP",
                "Player.SetCurrentAmmoRounds 7",
                "Player.Drop r9mm 1",
            ),
            4,
        ),
    ),
)


ALL_CAMPAIGNS: tuple[Campaign, ...] = (
    RUN1_PLAYER,
    RUN1B_PLAYER,
    RUN2_WORLD,
    RUN3_FINAL,
    RUN4_EDGE,
    RUN5_SURGICAL,
    RUN6_LAST,
    RUN7_AMMO_QUEST_COMP,
    RUN8_VAULT11_WORLD,
    RUN9_EXTRAS_NPC_0XFF,
    RUN10_SURGICAL,
    RUN14_TOK76_MAGAZINE,
    RUN19A_NAMES_MISC_FACE,
    RUN19B_APPEARANCE,
    RUN20A_AMMO_AND_NOUNEQUIP,
    RUN27_DEAD_NPC_DROP_0X6E,
)

CAMPAIGNS_BY_NAME: dict[str, Campaign] = {c.name: c for c in ALL_CAMPAIGNS}

COVERAGE_SUMMARY: dict[str, int] = {
    "total_converter_keys": 87,
    "confirmed_observed": 70,
    "regression_bugs_caught": 13,
    "regression_keys_affected": 15,
    "negative_proof_extras": 3,
    "unresolved_hypothesis_keys": 0,
    "vanilla_campaigns": len(ALL_CAMPAIGNS),
    "verify_assertions_vanilla": 61,
    "verify_assertions_optional_geck": 7,
    "verify_assertions": 68,
}
