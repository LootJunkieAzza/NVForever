#!/usr/bin/env python3
"""Writes the owner-decision capture jobs (`od_*.steps.json`) into this folder: one job per
quest-critical question of docs/owner-review.md that the original game can answer. The
index, with the owner-review item and the outcome that settles each one, is
docs/spec-notes/owner-decision-captures.md. The AI questions that need hours of game time
are sessions of the observer campaign `tools/capture/observe/owner_decisions.campaign.json`
(observe.py).

  python tools/capture/jobs/make_owner_decision_jobs.py

The jobs follow docs/spec-notes/xnvse-capture.md section 5: forms by `BuildRef <mod index>
<decimal local FormID>` (no Editor ID in any script text; console lines may name cells,
as `coc` needs), ref variables declared before `Begin Function`, every action script ends
with `SetFunctionValue 1`, TimeScale is never touched. `tgm` is a toggle, so a job that
needs god mode switches it on and off again itself. Every job puts the player where it
needs them first, so the jobs can run in any order in one session; all start from the
save `nvftest` (inside GSGasStation, early game) except od_09a (`nvfdoc`). No game data is
stored here: only script text and FormIDs, found with `fnvtool` and the masters' Editor IDs
(given in the comments).

Script functions used, from the GECK wiki [documented: the owner's export,
C:/fnv-private/geckwiki]: Activate, MoveTo, GetPos, GetAngle, SetAngle, GetHeadingAngle,
GetParentCell, GetInSameCell, MenuMode (1008 container, 1009 dialogue, 1012 sleep/wait:
wait-menu.md), GetSitting, GetSleeping, GetDisabled, Enable, Disable, GetDead, IsTalking,
SetPlayerTeammate, GetPlayerTeammate, EvaluatePackage, AddScriptPackage,
RemoveScriptPackage, GetCurrentPackage, GetCurrentAIPackage, GetCurrentAIProcedure,
SetActorsAI, ResetAI, GetDetected, GetAnimAction, IsMoving, IsInCombat, GetCombatTarget,
StartCombat, StopCombat, SetAlert, GetIsAlerted, IsWeaponOut, GetMajorCrimeCount,
GetActorFactionPlayerEnemy, GetAV Health, IsSpellTarget, Disable/Enable, AddItem,
RemoveItem, DisablePlayerControls, EnablePlayerControls, GetPlayerControlsDisabled,
IsSneaking, Kill; xNVSE/FOSE: TapControl (4 attack, 5 activate, 7 ready item, 8 crouch),
TapKey, GetCrosshairRef, GetLinkedDoor, GetVariable / SetVariable on a reference
(layer5-captures.md section 9.3 f), GetFormIDString, BuildRef, GetModIndex.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, HERE)

import capture_steps as cs  # noqa: E402
import observe  # noqa: E402

CTX = (cs.MASTERS, None)
PREFIX = "od_"

# Saves (C:/Users/<user>/Documents/My Games/FalloutNV/Saves, read by the game only).
SAVE_TEST = "nvftest"   # inside GSGasStation with RingoRef, early game (before Sunny's quest)
SAVE_DOC = "nvfdoc"     # Doc Mitchell's house after the intro

# FalloutNV.esm forms (Editor IDs from the masters; types and places from `fnvtool world
# refs` / `ai show` / `dialogue lines`, 2026-10-18).
RINGO = "0x00104C7D"            # RingoRef, in GSGasStation (the nvftest save's cell)
GAS_STATION_CELL = "GSGasStation"
GAS_STATION_CELL_ID = "0x00109038"
DOC_HOUSE_CELL = "GSDocMitchellHouse"
DOC_HOUSE_CELL_ID = "0x00103DF9"
DOC_SKULL = "0x00104C10"        # VCG01SkullBrahminActivator (script VCG01BrahminSkullSCRIPT: runtimer, timer)
DOC = "0x00104C0F"              # DocMitchellREF
BOONE = "0x00096BCE"            # CraigBooneREF
BOONE_ROOM_CELL = "NovacBoonesRoom"
BOONE_ROOM_CELL_ID = "0x000CAE83"
BOONE_ROOM_DOOR = "0x000CED40"  # OffDoorHotelSmL01, the door the follower-boone smoke scenario leaves by
BOONE_ROOM_EXT_DOOR = "0x000CAFA5"  # its other side outside (the inner door's XTEL; arrival 23776, -29504, 126.76 degrees)
BOONE_FOLLOW = "0x000B1609"     # FollowersBooneFollowPlayerDEFAULT
BOONE_WAIT = "0x000B16D1"       # FollowersBooneFollowPlayerWAIT
SETTLER = "0x00104F03"          # GoodspringsSettler01Ref (sandboxes outside, ai-observer.md section 7)
STAY_AT_EDITOR = "0x000200D5"   # DefaultStayAtEditorLocation (TRAVEL then DONE, ai-observer.md section 7)
SANDBOX_256 = "0x00018E6A"      # DefaultSandboxEditorLocation256
GAS_STATION_DOOR = "0x0010903F"  # OffDoorMetalSmL01 in GSGasStation
NO_GREETING = [                 # no GREETING line in `fnvtool dialogue talk` from a new game
    ("primm_deputy", "0x000CDE03"),   # PrimmNCRDeputyRef1
    ("hv_ranger", "0x000ACD6F"),      # HVRangerREF
    ("powder_ganger", "0x00104C70"),  # GSPG01Ref
]
HOSTAGE_DEAD = "0x0012319B"     # Techatticup hostage 1 (TecMineHostage, SCPT 0x00123191)
HOSTAGE_LIVING = "0x0012319C"   # Techatticup hostage 2
GECKO = "0x00157B3D"            # a Very Aggressive gecko (smoke scenario creature-names)
NCR_VICTIM = "0x000F36F5"       # one of the six NCR troopers of battle-ncr-fiends
NCR_DISTANT = [("primm_trooper_1", "0x000A23B7"), ("primm_trooper_3", "0x000A23B9"),
               ("primm_trooper_4", "0x000A23BA")]  # PrimmNCRTrooper1REF, 3REF, 4REF
AURA_TOKEN = "0x0013B1A8"       # NVLegionExplorerAuraToken (script LegionExplorerAuraToken: in combat, casts the aura on its holder)
AURA = "0x001267A8"             # LegionExplorerAura (Touch, area 50, 10 s; its effect script gives +2 Perception to members of VCaesarsLegionFaction only)
LEGION = "0x000EE68A"           # VCaesarsLegionFaction
LEAD_PIPE = "0x00004337"        # WeapLeadPipe (melee: no ammo, no aim)
NASH = "0x000E2882"             # PrimmJohnsonNashRef
RED_LUCY = "0x00134830"         # VMS38RedLucyREF
RALPH = "0x0010D8E2"            # VFSRalph's reference in Mick & Ralph's
GOODBYE_TOPIC = "0x000000D4"    # GOODBYE (DIAL), fnvtool dialogue lines --topic GOODBYE
RALPH_EVERY = 60                # observer samples (10 game loops each) between two goodbyes
RALPH_SAYS = 12

# Menu codes (MenuMode): 1001 message box, 1008 container, 1009 dialogue [documented: GECK
# wiki GetMenuTargetRef], 1012 sleep/wait [observed: six scripts' Begin MenuMode 1012,
# wait-menu.md section 5].
MENUS = (("1009", "dialogue"), ("1012", "sleep_wait"), ("1008", "container"), ("1001", "message"))
# Control codes [documented: GECK wiki TapControl].
ATTACK, ACTIVATE, READY, CROUCH = 4, 5, 7, 8
TAB = 15  # DirectX scan code of Tab [documented: GECK wiki IsKeyPressed scan-code table]
WHEEL_DOWN = 265  # the mouse wheel turned down [hypothesis: xNVSE numbers mouse inputs from 256, the wheel 264 up and 265 down; not in the wiki export]
SCROLL_STEPS = 6

ACTION_FRAMES = 30   # after an action (xnvse-capture.md section 5)
MOVETO_FRAMES = 120  # MoveTo is queued [documented: GECK wiki MoveTo]
CELL_FRAMES = 300    # after arriving in a cell (cs.AFTER_DOOR_FRAMES)


# --------------------------------------------------------------------------- step helpers

def act(stmts, forms=(), decls=()):
    """Statements with {0}, {1} ... the forms; ends with SetFunctionValue 1."""
    if isinstance(stmts, str):
        stmts = [stmts]
    n = cs.Names(*CTX)
    ops = [n.operand(f) for f in forms]
    return {"script": cs.function(n.decls + list(decls), n.setup + [s.format(*ops) for s in stmts], returns_value=False)}


def num(expr, label, field, forms=()):
    return cs.query(cs.number_query(expr, CTX, list(forms)), label, field)


def form(expr, label, field, forms=()):
    """A form-valued expression as GetFormIDString text ("00000000" for none)."""
    n = cs.Names(*CTX)
    ops = [n.operand(f) for f in forms]
    return cs.query(cs.function(n.decls + ["ref rX"], n.setup + [f"let rX := {expr.format(*ops)}",
                                                                 "SetFunctionValue (GetFormIDString rX)"]),
                    label, field)


def record(parts, label, field, forms=()):
    """Several values in one query, joined by spaces: parts = [(kind, expr)], kind "num"
    (through `$`, six significant digits: ai-observer.md section 4) or "ref"."""
    n = cs.Names(*CTX)
    ops = [n.operand(f) for f in forms]
    decls, body, pieces = [], [], []
    for i, (kind, expr) in enumerate(parts):
        var = f"{'r' if kind == 'ref' else 'f'}V{i}"
        decls.append(f"{'ref' if kind == 'ref' else 'float'} {var}")
        body.append(f"let {var} := {expr.format(*ops)}")
        pieces.append(f"(GetFormIDString {var})" if kind == "ref" else f"($ {var})")
    joined = ' + " " + '.join(pieces)
    return cs.query(cs.function(n.decls + decls, n.setup + body + [f"SetFunctionValue ({joined})"]), label, field)


def until(cond, forms=(), max_frames=1200, decls=()):
    """Wait (once per game loop) until `cond` holds."""
    n = cs.Names(*CTX)
    ops = [n.operand(f) for f in forms]
    return {"until": cs.function(n.decls + list(decls) + ["float fV"],
                                 n.setup + [f"if {cond.format(*ops)}", "  let fV := 1", "endif", "SetFunctionValue fV"]),
            "max_frames": max_frames}


def wait(frames):
    return {"frames": frames}


def menus(label):
    return [num(f"MenuMode {code}", label, f"menu.{name}") for code, name in MENUS] + [num("MenuMode", label, "menu.any")]


def close_menus():
    return [{"console": "CloseAllMenus"}, wait(60)]


def shot(label):
    return {"screenshot": label}


def god_on():
    return [{"console": "tgm"}]


def god_off():
    return [{"console": "tgm"}]


def coc(cell, cell_id):
    """Console coc (it takes the Editor ID) and wait until the player is in the cell."""
    return [{"console": f"coc {cs.plain_name(cell, 'the cell')}"},
            {"until": cs.in_cell_until(cell_id, CTX), "max_frames": cs.DOOR_LOOPS}, wait(CELL_FRAMES)]


def gas_station():
    return coc(GAS_STATION_CELL, GAS_STATION_CELL_ID)


def go_to(ref, dx=0, dy=0):
    """The player to a reference (MoveTo with world-axis offsets), wherever it is."""
    return [act(f"player.MoveTo {{0}} {cs.number(dx)} {cs.number(dy)} 0", [ref]),
            until("player.GetInSameCell {0}", [ref], cs.DOOR_LOOPS), wait(CELL_FRAMES)]


def face(ref):
    """Turn the player toward a reference (the wiki's GetHeadingAngle formula)."""
    return act(["let fA := player.GetAngle Z + player.GetHeadingAngle {0}", "player.SetAngle Z fA"], [ref],
               decls=["float fA"])


def pose(who, label, field="pose"):
    """x y z heading of an actor (`player` or a FormID)."""
    o = "player" if who == "player" else "{0}"
    forms = [] if who == "player" else [who]
    return record([("num", f"{o}.GetPos X"), ("num", f"{o}.GetPos Y"), ("num", f"{o}.GetPos Z"),
                   ("num", f"{o}.GetAngle Z")], label, field, forms)


# The AI state of one actor in one query: package (form), package type, procedure,
# detects the player, animation action, moving, in combat, combat target, x, y, z, heading.
AI_PARTS = [("ref", "{0}.GetCurrentPackage"), ("num", "{0}.GetCurrentAIPackage"),
            ("num", "{0}.GetCurrentAIProcedure"), ("num", "{0}.GetDetected player"), ("num", "{0}.GetAnimAction"),
            ("num", "{0}.IsMoving"), ("num", "{0}.IsInCombat"), ("ref", "{0}.GetCombatTarget"),
            ("num", "{0}.GetPos X"), ("num", "{0}.GetPos Y"), ("num", "{0}.GetPos Z"), ("num", "{0}.GetAngle Z")]
AI_FIELDS = "package type procedure detects_player anim_action moving in_combat combat_target x y z heading"


def ai_state(who, label, parts=AI_PARTS):
    return record(parts, label, "ai", [who])


def ai_samples(who, prefix, n, every):
    steps = []
    for i in range(n):
        steps += [ai_state(who, f"{prefix} {i:02d}"), wait(every)]
    return steps


def dismiss():
    """Close a message box (MenuMode 1001) if one is up: in the save nvfstrip some actors and
    places open one (Johnson Nash, Red Lucy) [observed 2026-10-18]. con_CloseAllMenus from a
    function script closed a dialogue in the observer's hook [observed 2026-10-08]."""
    return [act(["if MenuMode 1001", "  con_CloseAllMenus", "endif"]), wait(30)]


def job(name, title, start, steps, prelude=False):
    """`prelude`: close any menu the save starts with (the second round's jobs)."""
    if prelude:
        steps = close_menus() + steps
    j = cs.job(PREFIX + name, start, steps, cs.MASTERS)
    j["title"] = title
    return j


def start_text(save, what):
    return f"{save}: {what} Never saved; unattended (capture_run.py --save {save})."


# --------------------------------------------------------------------------- the jobs

def no_greeting():
    """Item 2: can an actor with no eligible GREETING line be talked to? Three actors with
    no greeting in our headless tool, and RingoRef (has one) as the control."""
    steps = gas_station()
    for name, ref in [("control_ringo", RINGO)] + NO_GREETING:
        steps += [num("{0}.GetDisabled", f"{name} at start", "disabled", [ref]),
                  num("{0}.GetDead", f"{name} at start", "dead", [ref]),
                  act(["if {0}.GetDisabled", "  {0}.Enable 0", "endif"], [ref]), wait(ACTION_FRAMES),
                  act("{0}.MoveTo player 0 120 0", [ref]), wait(MOVETO_FRAMES),
                  num("player.GetInSameCell {0}", f"{name} moved", "same_cell", [ref]),
                  act("{0}.Activate player", [ref]), wait(ACTION_FRAMES)]
        for after, frames in (("30 loops", 0), ("150 loops", 120)):
            if frames:
                steps.append(wait(frames))
            label = f"{name} activated, {after}"
            steps += menus(label) + [num("{0}.IsTalking", label, "talking", [ref]),
                                     form("{0}.GetDialogueTarget", label, "dialogue_target", [ref]),
                                     shot(f"{name}_activated_{after.split()[0]}")]
        steps += close_menus()
    return job("02_no_greeting",
               "Actors with no eligible greeting: does Activate open dialogue (owner-review: dialogue (g)) (about 1 min)",
               start_text(SAVE_TEST, "moves three actors (enabled if disabled) next to the player in GSGasStation."),
               steps)


def two_poses(label):
    """Player and Boone in one record: px py pz ph bx by bz bh same_cell boone_cell."""
    return record([("num", "player.GetPos X"), ("num", "player.GetPos Y"), ("num", "player.GetPos Z"),
                   ("num", "player.GetAngle Z"), ("num", "{0}.GetPos X"), ("num", "{0}.GetPos Y"),
                   ("num", "{0}.GetPos Z"), ("num", "{0}.GetAngle Z"), ("num", "player.GetInSameCell {0}"),
                   ("ref", "{0}.GetParentCell")], label, "poses", [BOONE])


def after_door(prefix):
    steps = []
    for i in range(30):  # every 2 loops for the first 60 loops: where the teammate is put
        steps += [two_poses(f"{prefix} {i:02d}"), wait(2)]
    for i in range(30, 50):  # then every 30 loops
        steps += [two_poses(f"{prefix} {i:02d}"), wait(30)]
    return steps


def boone_package(label):
    return [form("{0}.GetCurrentPackage", label, "package", [BOONE]),
            num("{0}.GetCurrentAIPackage", label, "package_type", [BOONE]),
            num("{0}.GetCurrentAIProcedure", label, "procedure", [BOONE]),
            num("{0}.GetPlayerTeammate", label, "teammate", [BOONE])]


def teammate_door():
    """Item 3: where does a following teammate arrive through a load door? Boone hired as
    his recruit line does it (VNPCFollowers.bBooneHired, SetPlayerTeammate), his Follow
    package checked (and given as a script package only if the conditions did not choose
    it), the player out of his room by its door and back in; then the same with his Wait
    package (bWaiting / Waiting set)."""
    steps = god_on() + coc(BOONE_ROOM_CELL, BOONE_ROOM_CELL_ID)
    steps += [act("{0}.MoveTo player 0 150 0", [BOONE]), wait(MOVETO_FRAMES),
              {"console": "set VNPCFollowers.bBooneHired to 1"},
              act(["{0}.SetPlayerTeammate 1", "{0}.EvaluatePackage"], [BOONE]), wait(60)]
    steps += boone_package("hired")
    steps += [act(["let rP := {0}.GetCurrentPackage", "if rP != {1}", "  {0}.AddScriptPackage {1}", "endif"],
                  [BOONE, BOONE_FOLLOW], decls=["ref rP"]), wait(60)]
    steps += boone_package("follow package")
    # Stand at the door and let him come along; record where both are before the door.
    steps += [act("player.MoveTo {0} 0 0 0", [BOONE_ROOM_DOOR]), wait(MOVETO_FRAMES), wait(300),
              two_poses("before the door out"), pose(BOONE_ROOM_DOOR, "before the door out", "door")]
    steps += [act("{0}.Activate player", [BOONE_ROOM_DOOR]),
              {"until": cs.left_cell_until(BOONE_ROOM_DOOR, CTX), "max_frames": cs.DOOR_LOOPS}]
    steps += after_door("out, following") + boone_package("outside")
    # Back in through the outside door. The first run's `GetLinkedDoor` + Activate never
    # took the player in (the wait timed out) [observed 2026-10-18].
    # Activating the outside door by script did not take the player in either (second
    # round, 2026-10-19 [observed]), so the player now faces it and presses Activate
    # (TapControl); MoveTo to the inner door is the last resort, recorded as `way_in`.
    steps += [face(BOONE_ROOM_EXT_DOOR), act("player.SetAngle X 0"), wait(10), act(f"TapControl {ACTIVATE}"),
              until("player.GetInSameCell {0}", [BOONE_ROOM_DOOR], 600),
              num("player.GetInSameCell {0}", "way in", "by_activate", [BOONE_ROOM_DOOR]),
              act(["if player.GetInSameCell {0} == 0", "  player.MoveTo {0}", "endif"], [BOONE_ROOM_DOOR]),
              {"until": cs.in_cell_until(BOONE_ROOM_CELL_ID, CTX), "max_frames": cs.DOOR_LOOPS}]
    steps += after_door("in, following") + boone_package("back inside")
    # Told to wait: his Wait package is chosen by his `Waiting` variable (the smoke
    # scenario sets it; the companion wheel does in the game).
    steps += [act(["{0}.RemoveScriptPackage", "{0}.SetVariable \"Waiting\" 1"], [BOONE]), wait(60),
              act("{0}.EvaluatePackage", [BOONE]), wait(120)] + boone_package("told to wait")
    steps += [act("player.MoveTo {0} 0 0 0", [BOONE_ROOM_DOOR]), wait(MOVETO_FRAMES),
              act("{0}.Activate player", [BOONE_ROOM_DOOR]),
              {"until": cs.left_cell_until(BOONE_ROOM_DOOR, CTX), "max_frames": cs.DOOR_LOOPS}]
    steps += after_door("out, waiting") + boone_package("outside, waiting") + god_off()
    return job("03_teammate_door",
               "Teammates through a door: where Boone arrives, following and waiting (owner-review: Followers) (about 2 min)",
               start_text(SAVE_TEST, "coc NovacBoonesRoom; hires Boone by his quest variable and SetPlayerTeammate."),
               steps, prelude=True)


def controls_state(label):
    return [num("GetPlayerControlsDisabled 0 0 0 0 0 0 1", label, "disabled.sneaking"),
            num("GetPlayerControlsDisabled 1 0 0 0 0 0 0", label, "disabled.movement"),
            num("player.IsSneaking", label, "sneaking")]


def stand_up():
    return [act(["if player.IsSneaking", f"  TapControl {CROUCH}", "endif"]), wait(120)]


def enable_controls_sneak():
    """Item 4: does EnablePlayerControls with the seventh number (sneaking) left out give
    sneaking back? Each case starts with everything enabled and standing, then taps the
    crouch control and reads IsSneaking."""
    cases = [("control: enable with no numbers", "1 1 1 1 1 1 1", ""),
             ("enable 1 1 1 1 1 1 after all seven off", "1 1 1 1 1 1 1", "1 1 1 1 1 1"),
             ("enable 1 1 1 1 1 1 after sneaking off", "0 0 0 0 0 0 1", "1 1 1 1 1 1"),
             ("control: enable 1 1 1 1 1 1 1 after all seven off", "1 1 1 1 1 1 1", "1 1 1 1 1 1 1")]
    steps = gas_station() + [act("EnablePlayerControls"), wait(30)]
    steps += stand_up() + controls_state("start")
    for label, off, on in cases:
        steps += [act(f"DisablePlayerControls {off}"), wait(10)] + controls_state(f"{label}: disabled")
        steps += [act(f"EnablePlayerControls {on}".rstrip()), wait(10)] + controls_state(f"{label}: enabled")
        steps += [act(f"TapControl {CROUCH}"), wait(120)] + controls_state(f"{label}: crouch tapped")
        steps += stand_up() + [act("EnablePlayerControls"), wait(30)]
    steps += controls_state("end")
    return job("04_enable_controls_sneak",
               "EnablePlayerControls with sneaking left out: does crouch work again (owner-review: Ctrl crouch) (about 1 min)",
               start_text(SAVE_TEST, "coc GSGasStation; player controls only."), steps)


def set_actors_ai():
    """Item 5: what SetActorsAI 0 stops: package, procedure, position, detection,
    animation action. In the first run the settler stood at an idle marker with AI on, so
    nothing could freeze [observed 2026-10-18]; now he is moved 800 units off and given
    DefaultStayAtEditorLocation, so he walks back when his AI is on."""
    steps = [{"script": observe.set_hour_script(10)}, wait(cs.SETTLE_FRAMES_LONG)]
    steps += go_to(SETTLER, 0, -300) + dismiss()
    steps += [act("{0}.MoveTo player 0 800 0", [SETTLER]), wait(MOVETO_FRAMES),
              act("{0}.AddScriptPackage {1}", [SETTLER, STAY_AT_EDITOR]), wait(60)]
    # The second round's settler reached DONE (17) about 90 loops into his walk, before
    # SetActorsAI 0 [observed 2026-10-19], so the AI now goes off 30 loops in.
    steps += ai_samples(SETTLER, "AI on, walking back", 2, 15)
    steps += [act("{0}.SetActorsAI 0", [SETTLER])] + ai_samples(SETTLER, "AI off", 20, 30)
    # Walk right up to it: does it still detect, turn or greet?
    steps += [act("player.MoveTo {0} 0 -100 0", [SETTLER]), wait(MOVETO_FRAMES), face(SETTLER)]
    steps += ai_samples(SETTLER, "AI off, player close", 10, 30)
    steps += [act("{0}.SetActorsAI 1", [SETTLER])] + ai_samples(SETTLER, "AI back on", 20, 30)
    steps += [act("{0}.RemoveScriptPackage", [SETTLER]), wait(60)]
    return job("05_set_actors_ai",
               "SetActorsAI 0 on a settler walking back to his place: package, movement, detection, animation (owner-review: stubs-2) (about 2 min)",
               start_text(SAVE_TEST, "clock to 10:00, player to GoodspringsSettler01Ref, who is moved 800 units off."), steps,
               prelude=True)


def reset_ai():
    """Item 6: ResetAI and the AI codes. (a) a script package run to its end (procedure
    17 DONE expected), then ResetAI; (b) ResetAI in the middle of a script package;
    (c) a sleeping actor (package type 4 SLEEP, GetSleeping) and ResetAI. The first run
    waited for procedure 17 and for GetSleeping 3 and timed out on both [observed
    2026-10-18]; now the waits are sampled every 300 loops, so a timeout still shows
    what he was doing, and part (c) waits for the SLEEP procedure (8) first."""
    steps = gas_station() + dismiss() + [act("{0}.MoveTo player 0 150 0", [RINGO]), wait(MOVETO_FRAMES)]
    after_reset = [("1 loop", 1), ("10 loops", 9), ("30 loops", 20), ("120 loops", 90), ("300 loops", 180)]

    def resets(prefix):
        out = [act("{0}.ResetAI", [RINGO])]
        for name, frames in after_reset:
            out += [wait(frames), ai_state(RINGO, f"{prefix}: after ResetAI, {name}"),
                    num("{0}.GetSleeping", f"{prefix}: after ResetAI, {name}", "sleeping", [RINGO])]
        return out

    def watch(prefix, n):
        out = []
        for i in range(n):
            out += [ai_state(RINGO, f"{prefix} {i:02d}"), num("{0}.GetSleeping", f"{prefix} {i:02d}", "sleeping", [RINGO]),
                    wait(300)]
        return out

    # (a) to the end of DefaultStayAtEditorLocation, moved away from his place first.
    steps += [act("{0}.MoveTo {1}", [RINGO, GAS_STATION_DOOR]), wait(MOVETO_FRAMES), ai_state(RINGO, "a: moved away"),
              act("{0}.AddScriptPackage {1}", [RINGO, STAY_AT_EDITOR]), wait(ACTION_FRAMES),
              ai_state(RINGO, "a: script package given")]
    steps += watch("a: on the package", 8)
    steps += [until("{0}.GetCurrentAIProcedure == 17", [RINGO], 1200), ai_state(RINGO, "a: procedure 17")]
    steps += resets("a")
    # (b) mid-package.
    steps += [act("{0}.MoveTo {1}", [RINGO, GAS_STATION_DOOR]), wait(MOVETO_FRAMES),
              act("{0}.AddScriptPackage {1}", [RINGO, STAY_AT_EDITOR]), wait(15), ai_state(RINGO, "b: mid-package")]
    steps += resets("b")
    steps += [act("{0}.RemoveScriptPackage", [RINGO]), wait(300)]
    # (c) asleep: his sleep package starts at 22:00 (GSRingoSleepPackage22x8).
    steps += [{"script": observe.set_hour_script(23)}, wait(cs.SETTLE_FRAMES_LONG)]
    steps += watch("c: after 23:00", 12)
    steps += [until("{0}.GetCurrentAIProcedure == 8", [RINGO], 3600), ai_state(RINGO, "c: sleep procedure"),
              until("{0}.GetSleeping == 3", [RINGO], 1800), ai_state(RINGO, "c: asleep"),
              num("{0}.GetSleeping", "c: asleep", "sleeping", [RINGO])]
    steps += resets("c")
    return job("06_reset_ai",
               "ResetAI and the AI codes: DONE (17), mid-package, asleep (type 4) (owner-review: stubs ResetAI) (about 6 min)",
               start_text(SAVE_TEST, "RingoRef brought to GSGasStation; clock to 23:00 for the last part."), steps,
               prelude=True)


def script_packages():
    """Item 8: does a second AddScriptPackage replace the first (the wiki) or stack on it
    (our runtime)? And does EvaluatePackage keep a script package?"""
    steps = gas_station()

    def read(label):
        return [ai_state(RINGO, label)]

    steps += read("start")
    steps += [act("{0}.AddScriptPackage {1}", [RINGO, STAY_AT_EDITOR]), wait(ACTION_FRAMES)] + read("first added")
    steps += [act("{0}.AddScriptPackage {1}", [RINGO, SANDBOX_256])]
    for name, frames in (("10 loops", 10), ("60 loops", 50), ("300 loops", 240)):
        steps += [wait(frames)] + read(f"second added, {name}")
    steps += [act("{0}.RemoveScriptPackage", [RINGO])]
    for name, frames in (("10 loops", 10), ("60 loops", 50), ("300 loops", 240)):
        steps += [wait(frames)] + read(f"one removed, {name}")
    steps += [act("{0}.RemoveScriptPackage", [RINGO]), wait(300)] + read("removed again")
    # EvaluatePackage with a script package on (the wiki: not within a few frames of
    # RemoveScriptPackage, hence the 300 loops above).
    steps += [act("{0}.AddScriptPackage {1}", [RINGO, SANDBOX_256]), wait(60)] + read("added for EvaluatePackage")
    steps += [act("{0}.EvaluatePackage", [RINGO])]
    for name, frames in (("10 loops", 10), ("300 loops", 290)):
        steps += [wait(frames)] + read(f"after EvaluatePackage, {name}")
    steps += [act("{0}.RemoveScriptPackage", [RINGO]), wait(60)]
    return job("08_script_packages",
               "AddScriptPackage twice, RemoveScriptPackage, EvaluatePackage (owner-review: runtime differs, script packages) (about 1 min)",
               start_text(SAVE_TEST, "RingoRef in GSGasStation."), steps)


def disabled_ref_script():
    """Item 9 (a): does a disabled reference's object script run its GameMode? The skull
    in Doc Mitchell's house counts `timer` down while `runtimer` is 1."""
    steps = coc(DOC_HOUSE_CELL, DOC_HOUSE_CELL_ID)

    def timer(label):
        return [num('{0}.GetVariable "timer"', label, "timer", [DOC_SKULL]),
                num("{0}.GetDisabled", label, "disabled", [DOC_SKULL])]

    steps += timer("start")
    steps += [act(['{0}.SetVariable "timer" 1000', '{0}.SetVariable "runtimer" 1'], [DOC_SKULL]), wait(30)]
    steps += timer("enabled, counting") + [wait(60)] + timer("enabled, 60 loops later")
    steps += [act("{0}.Disable", [DOC_SKULL]), wait(10)] + timer("disabled")
    for i in range(5):
        steps += [wait(60)] + timer(f"disabled, {60 * (i + 1)} loops later")
    steps += [act("{0}.Enable", [DOC_SKULL]), wait(10)] + timer("enabled again") + [wait(60)] + timer(
        "enabled again, 60 loops later")
    steps += [act('{0}.SetVariable "timer" 0', [DOC_SKULL])]
    return job("09a_disabled_ref_script",
               "Disabled reference: does its object script's GameMode run (owner-review: runtime differs, disabled references) (about 30 s)",
               start_text(SAVE_DOC, "Doc Mitchell's house; disables and enables the skull activator."), steps)


def carried_item_script():
    """Item 9 (b): does a scripted item run its GameMode in an NPC's inventory? The
    Legion explorer token (script LegionExplorerAuraToken) casts LegionExplorerAura on its
    holder after 2 s in combat. The first run read IsSpellTarget 0 for the player too
    [observed 2026-10-18]: the aura is a Touch spell of area 50 whose effect script raises
    Perception by 2 only for members of VCaesarsLegionFaction (fnvtool esm decode,
    script decompile), so both holders join that faction first, Perception is read beside
    IsSpellTarget, and the aura cast directly on the player checks the reading itself."""
    def aura(who, label):
        o = "player" if who == "player" else "{0}"
        forms = [] if who == "player" else [who]
        label = f"{'player' if who == 'player' else 'ringo'}: {label}"
        return [num(f"{o}.IsSpellTarget {{{len(forms)}}}", label, "aura", forms + [AURA]),
                num(f"{o}.GetAV Perception", label, "perception", forms),
                num(f"{o}.IsInCombat", label, "in_combat", forms),
                num(f"{o}.GetItemCount {{{len(forms)}}}", label, "tokens", forms + [AURA_TOKEN])]

    steps = god_on() + gas_station() + dismiss() + [act("{0}.MoveTo player 0 150 0", [RINGO]), wait(MOVETO_FRAMES)]
    steps += aura("player", "before") + aura(RINGO, "before")
    steps += [act(["player.AddToFaction {1} 0", "{0}.AddToFaction {1} 0"], [RINGO, LEGION]), wait(30)]
    # The reading checked: the aura cast on the player directly (10 s), then let it end.
    steps += [act("player.CastImmediateOnSelf {0}", [AURA]), wait(30)] + aura("player", "aura cast directly")
    steps += [wait(900)] + aura("player", "direct aura over")
    # The token is a non-playable armour (ARMO, "Aura Token Kicker"); in the second round
    # its script cast nothing on either holder, the player included [observed
    # 2026-10-19], so both now also equip it.
    steps += [act("player.AddItem {0} 1 1", [AURA_TOKEN]), act("{0}.AddItem {1} 1 1", [RINGO, AURA_TOKEN]), wait(30),
              act(["player.EquipItem {1} 1 1", "{0}.EquipItem {1} 1 1"], [RINGO, AURA_TOKEN]), wait(30),
              num("player.GetEquipped {0}", "player: tokens given", "equipped", [AURA_TOKEN]),
              num("{0}.GetEquipped {1}", "ringo: tokens given", "equipped", [RINGO, AURA_TOKEN])]
    steps += aura("player", "tokens given") + aura(RINGO, "tokens given")
    # Ringo set on the player read in_combat 0 for both throughout the second round
    # [observed 2026-10-19], so each holder now gets a spawned gecko to fight: one placed
    # at Ringo and set on him (and he on it), one placed at the player and set on them.
    # The gecko killed Ringo in the first try of this (od_12 then found his body)
    # [observed 2026-10-19]: he gets 2,000 more Health first (never saved), and this job
    # runs after the other Ringo jobs.
    spawn_on = CREATED_ATTACKER + ["if rG", "  rG.StartCombat {0}", "endif"]
    steps += [act("{0}.ModAV Health 2000", [RINGO])]
    steps += [act(["let rB := {1}.GetBaseObject", "{0}.PlaceAtMe rB 1 200 1"], [RINGO, GECKO], decls=["ref rB"]), wait(30),
              act(spawn_on + ["if rG", "  {0}.StartCombat rG", "endif"], [RINGO], decls=["ref rA", "ref rG"]), wait(30),
              act(["let rB := {0}.GetBaseObject", "player.PlaceAtMe rB 1 300 1"], [GECKO], decls=["ref rB"]), wait(30),
              act([x.replace("{0}", "player") for x in spawn_on], decls=["ref rA", "ref rG"])]
    for frames, total in ((150, 150), (150, 300), (300, 600), (300, 900)):
        steps += [wait(frames)] + aura("player", f"in combat, {total} loops") + aura(RINGO, f"in combat, {total} loops")
    steps += [act(CREATED_ATTACKER + ["if rG", "  rG.Kill", "endif"], decls=["ref rA", "ref rG"]),
              act(CREATED_ATTACKER + ["if rG", "  rG.Kill", "endif"], decls=["ref rA", "ref rG"]), wait(30)]
    steps += [act(["{0}.StopCombat", "player.RemoveItem {1} 1 1", "{0}.RemoveItem {1} 1 1",
                   "player.RemoveFromFaction {2}", "{0}.RemoveFromFaction {2}"], [RINGO, AURA_TOKEN, LEGION]),
              wait(60)] + god_off()
    return job("09b_carried_item_script",
               "Scripted item in an NPC's inventory: does its GameMode run (owner-review: runtime differs, scripted items) (about 1.5 min)",
               start_text(SAVE_TEST, "RingoRef; both join the Legion faction; Ringo fights the player (god mode)."), steps,
               prelude=True)


def stray_markers():
    """Item 10: TecMineHostage (SCPT 0x00123191) with its stray ElseIf/EndIf: its
    variables (freed, doonce, button) before and after its OnActivate runs (Activate with
    the run-OnActivate flag), on the living hostage with freed 0 and 1 and on the dead
    one (layer5-captures.md 8.3 c has the menus; this adds the variables)."""
    def variables(ref, label):
        return [num(f'{{0}}.GetVariable "{v}"', label, f"var.{v}", [ref]) for v in ("freed", "doonce", "button")] + [
            num("{0}.GetDead", label, "dead", [ref])]

    steps = []
    for name, ref, freed in (("living, freed 0", HOSTAGE_LIVING, 0), ("living, freed 1", HOSTAGE_LIVING, 1),
                             ("dead", HOSTAGE_DEAD, None)):
        if freed is not None:
            steps += [act(f'{{0}}.SetVariable "freed" {freed}', [ref]), wait(10)]
        else:
            steps += [act(["if {0}.GetDead == 0", "  {0}.Kill", "endif"], [ref]), wait(ACTION_FRAMES)]
        steps += variables(ref, f"{name}, before")
        steps += [act("{0}.Activate player 1", [ref]), wait(ACTION_FRAMES)]
        steps += variables(ref, f"{name}, activated") + menus(f"{name}, activated") + [shot(f"hostage_{name.replace(', ', '_').replace(' ', '_')}")]
        steps += close_menus() + variables(ref, f"{name}, after CloseAllMenus")
    return job("10_stray_markers",
               "TecMineHostage's stray ElseIf/EndIf: its variables after OnActivate (owner-review: 5B runtime decisions) (about 30 s)",
               start_text(SAVE_TEST, "the Techatticup hostages, activated from afar; kills hostage 1."), steps)


def open_dialogue(ref, label):
    """Activate the actor and wait for the dialogue menu. A message box in its place (the
    first run, save nvfstrip [observed 2026-10-18]) is recorded, closed, and the actor
    activated once more."""
    return dismiss() + [act("{0}.Activate player", [ref]), wait(ACTION_FRAMES)] + menus(f"{label}: first activation") + [
        act(["if MenuMode 1001", "  con_CloseAllMenus", "endif"]), wait(30),
        act(["if MenuMode 1009 == 0", "  {0}.Activate player", "endif"], [ref]),
        until("MenuMode 1009", max_frames=600),
        # A third try, turned to the actor again: od_12's first conversation never opened
        # after two activations in the second round [observed 2026-10-19].
        face(ref), act(["if MenuMode 1009 == 0", "  {0}.Activate player", "endif"], [ref]),
        until("MenuMode 1009", max_frames=600)]


def clock_in_dialogue():
    """Item 11: does the game clock run while the dialogue menu is open? Every sample
    carries MenuMode 1009, so only samples with it 1 count (the first run never opened
    the dialogue [observed 2026-10-18])."""
    steps = go_to(RINGO, 0, -150) + dismiss() + [act("{0}.MoveTo player 0 120 0", [RINGO]), wait(MOVETO_FRAMES),
                                                face(RINGO)]
    clock = [("num", "GameHour"), ("num", "GameDaysPassed"), ("num", "MenuMode 1009")]
    steps += [record(clock, "before", "clock"), wait(300), record(clock, "before, 300 loops later", "clock")]
    steps += open_dialogue(RINGO, "ringo")
    for i in range(6):
        steps += [record(clock, f"dialogue open {i:02d}", "clock"), wait(300)]
    steps += close_menus()
    for i in range(3):
        steps += [record(clock, f"dialogue closed {i:02d}", "clock"), wait(300)]
    return job("11_clock_in_dialogue",
               "Game clock while the dialogue menu is open (owner-review: dialogue pause (2)) (about 1.5 min)",
               start_text(SAVE_TEST, "player to RingoRef wherever he is; talks to him."), steps, prelude=True)


CREATED_ATTACKER = [  # the last actor created at run time in the loaded area that is alive
    "let rA := GetFirstRef 200 1", "while rA != 0",
    f"  if rA.GetSourceModIndex == {cs.CREATED_MOD_INDEX}", "    if rA.GetDead == 0", "      let rG := rA",
    "    endif", "  endif", "  let rA := GetNextRef", "loop"]


def spawned_state(label):
    """In combat, combat target, x, y, z of the spawned gecko (found by a scan: PlaceAtMe's
    result cannot be kept, ai-observer.md section 11)."""
    return cs.query(cs.function(["ref rA", "ref rG", "ref rT", "float fC", "float fX", "float fY"], CREATED_ATTACKER + [
        "if rG", "  let fC := rG.IsInCombat", "  let rT := rG.GetCombatTarget", "  let fX := rG.GetPos X",
        "  let fY := rG.GetPos Y",
        '  SetFunctionValue (($ fC) + " " + (GetFormIDString rT) + " " + ($ fX) + " " + ($ fY))',
        "else", '  SetFunctionValue "none"', "endif"]), label, "attacker")


def attack_in_dialogue():
    """Item 12: an attack while the dialogue menu is open: a gecko spawned behind the
    player and set on them (StartCombat), then the speaker himself. Does the conversation
    end, and does the attacker act while it is open? First run [observed 2026-10-18]: the
    placed gecko never engaged, and the second dialogue never opened, so Ringo's attack
    came with no dialogue open; now the gecko is a fresh one and every opening is checked."""
    def watch(prefix, attacker=None):
        out = []
        for name, frames in (("30 loops", 30), ("150 loops", 120), ("300 loops", 150)):
            label = f"{prefix}, {name}"
            who = [spawned_state(label)] if attacker is None else [
                num("{0}.IsInCombat", label, "attacker_in_combat", [attacker]),
                form("{0}.GetCombatTarget", label, "attacker_target", [attacker]), pose(attacker, label, "attacker_pose")]
            out += [wait(frames)] + menus(label) + who + [
                pose("player", label, "player_pose"), num("player.GetAV Health", label, "player_health"),
                shot(f"{prefix.replace(' ', '_')}_{name.split()[0]}")]
        return out

    steps = god_on() + go_to(RINGO, 0, -150) + dismiss() + [act("{0}.MoveTo player 0 120 0", [RINGO]),
                                                           wait(MOVETO_FRAMES), face(RINGO)]
    steps += open_dialogue(RINGO, "first dialogue") + [wait(60)] + menus("before the gecko")
    steps += [act(["let rB := {0}.GetBaseObject", "player.PlaceAtMe rB 1 400 1"], [GECKO], decls=["ref rB"]), wait(30),
              cs.query(cs.function(["ref rA", "ref rG"], CREATED_ATTACKER + ["if rG", "  rG.StartCombat player", "endif",
                                                                           "SetFunctionValue (GetFormIDString rG)"]),
                       "gecko set on the player", "attacker_ref")]
    steps += watch("gecko attacks")
    steps += close_menus() + [act(CREATED_ATTACKER + ["if rG", "  rG.Kill", "endif"], decls=["ref rA", "ref rG"]),
                              wait(300)]
    steps += open_dialogue(RINGO, "second dialogue") + [wait(60)] + menus("before Ringo attacks")
    steps += [act("{0}.StartCombat player", [RINGO])] + watch("speaker attacks", RINGO)
    steps += close_menus() + [act("{0}.StopCombat", [RINGO]), wait(60)] + god_off()
    return job("12_attack_in_dialogue",
               "An attack while the dialogue menu is open: does the conversation end (owner-review: dialogue pause (3)) (about 1.5 min)",
               start_text(SAVE_TEST, "talks to RingoRef; a spawned gecko and then Ringo attack the player (god mode)."), steps,
               prelude=True)


def victim_state(label):
    return [ai_state(NCR_VICTIM, label), num("{0}.GetMajorCrimeCount", label, "major_crimes", [NCR_VICTIM]),
            num("{0}.GetActorFactionPlayerEnemy", label, "faction_enemy", [NCR_VICTIM]),
            num("{0}.GetAV Health", label, "health", [NCR_VICTIM])]


SWINGS = 6
SWING_EVERY = 5     # game loops between observer samples while swinging
SWING_SAMPLES = 12  # samples from one swing to the next (60 loops)


def swings_observed():
    """The swings inside an observe step (plugin version 9). The second round's held
    attack key landed no hit in third person [observed 2026-10-19]; the observer's
    player_kills campaign hit by switching to first person, aiming every game loop and
    tapping Attack, so this does the same: a once hook switches to first person, a
    repeated hook turns the player to the victim (heading and pitch) every sample, and
    once hooks tap Attack every SWING_SAMPLES samples. xNVSE's OnHit / OnStartCombat /
    OnCombatEnd events record every hit that lands and who starts a fight, with the
    victim's health at that moment (ai-observer.md section 12)."""
    aim = ["let fA := player.GetAngle Z + player.GetHeadingAngle {0}", "player.SetAngle Z fA",
           "let fD := player.GetDistance {0}", "let fZ := {0}.GetPos Z - player.GetPos Z - 30",
           "if fD > 1", "  let fP := 0 - fZ / fD * 57.3", "  player.SetAngle X fP", "endif"]
    timed = [{"after": 1, "script": {"lines": ["if IsPC1stPerson == 0", "  TapControl 13", "endif"]}},
             {"after": 1, "repeat": True, "script": {"lines": aim, "names": [NCR_VICTIM],
                                                     "decls": ["float fA", "float fD", "float fZ", "float fP"]}}]
    for i in range(SWINGS):
        timed.append({"after": 6 + SWING_SAMPLES * i, "script": {"lines": [f"TapControl {ATTACK}"]}})
    samples = 6 + SWING_SAMPLES * SWINGS + 12
    session = {"name": "swings", "hours": 0.5, "every": SWING_EVERY, "settle_frames": 1, "track": [NCR_VICTIM],
               "fields": "fight", "events": ["OnHit", "OnStartCombat", "OnCombatEnd"], "timed": timed}
    campaign = {"format": observe.CAMPAIGN_FORMAT, "name": "od13", "sessions": [session]}
    steps = observe.session_job(campaign, session, CTX)["steps"]
    for s in steps:
        if "observe" in s:
            s["observe"]["max_samples"] = samples
    return steps + [act(["if IsPC1stPerson", "  TapControl 13", "endif"]), wait(30)]


def assault_faction():
    """Item 13: does an assault on one NCR trooper turn the whole faction? Distant Primm
    troopers before and after the player hits a trooper with no witnesses. The first
    run's hits did not land (victim's health 50 before and after) [observed 2026-10-18];
    now the player swings a lead pipe from 70 units, holding the attack control."""
    def distant(prefix, samples):
        out = go_to(NCR_DISTANT[0][1], 400, 0) + dismiss()
        for name, ref in NCR_DISTANT:
            out += [num("{0}.GetActorFactionPlayerEnemy", f"{prefix} {name}", "faction_enemy", [ref]),
                    num("{0}.GetMajorCrimeCount", f"{prefix} {name}", "major_crimes", [ref])]
        for i in range(samples):
            for name, ref in NCR_DISTANT:
                out.append(ai_state(ref, f"{prefix} {name} {i:02d}"))
            out.append(wait(60))
        return out

    steps = god_on() + distant("before", 5)
    steps += [{"console": "cow WastelandNV -18 0"}, wait(CELL_FRAMES)] + dismiss()
    steps += [act(["player.AddItem {0} 1 1", "player.EquipItem {0} 0 1"], [LEAD_PIPE]), wait(ACTION_FRAMES),
              act("{0}.MoveTo player 0 70 0", [NCR_VICTIM]), wait(MOVETO_FRAMES), face(NCR_VICTIM),
              act("player.SetAngle X 0"), act(["if player.IsWeaponOut == 0", f"  TapControl {READY}", "endif"]),
              wait(120)]
    steps += victim_state("victim before")
    steps += swings_observed()
    steps += [wait(120)] + victim_state("victim after the hits")
    steps += distant("after", 30)
    steps += [act("{0}.StopCombat", [NCR_VICTIM]), act("player.RemoveItem {0} 1 1", [LEAD_PIPE])] + god_off()
    return job("13_assault_faction",
               "Assault on one NCR trooper: do distant NCR troopers turn hostile (owner-review: karma and crime 8) (about 2 min)",
               start_text(SAVE_TEST, "Primm's NCR troopers, then a trooper hit with a lead pipe at Goodsprings' edge (god mode)."),
               steps, prelude=True)


def set_alert():
    """Item 15 (part): GetIsAlerted against IsInCombat. The observer's samples read
    GetIsAlerted 0 throughout 35,437 in-combat samples (the 2026-10-08 Goodsprings fight),
    so the alert of the wiki's GetIsAlerted ("purely an animation state", set by SetAlert)
    is not the detection Alert; this job shows what SetAlert does to IsInCombat. The
    detection Alert itself is session `alert_noise` of owner_decisions.campaign.json."""
    def read(label):
        return [num("{0}.GetIsAlerted", label, "alerted", [RINGO]), num("{0}.IsInCombat", label, "in_combat", [RINGO]),
                num("{0}.IsWeaponOut", label, "weapon_out", [RINGO]), ai_state(RINGO, label)]

    steps = gas_station() + read("start")
    steps += [act("{0}.SetAlert 1", [RINGO]), wait(60)] + read("SetAlert 1") + [wait(300)] + read("SetAlert 1, 300 loops")
    steps += [act("{0}.SetAlert 0", [RINGO]), wait(120)] + read("SetAlert 0")
    return job("15_set_alert",
               "SetAlert / GetIsAlerted against IsInCombat (owner-review: alert 4) (about 20 s)",
               start_text(SAVE_TEST, "RingoRef in GSGasStation."), steps)


def dialogue_screen(name, ref, shots=(300, 600, 900)):
    """Activate, then pictures of the dialogue screen (the topic list once the greeting has
    been said): for visual review."""
    steps = go_to(ref, 0, 0) + [act("{0}.MoveTo player 0 120 0", [ref]), wait(MOVETO_FRAMES), face(ref)]
    steps += open_dialogue(ref, name)
    last = 0
    for at in shots:
        steps += [wait(at - last), shot(f"{name}_{at}")]
        last = at
    # The topic list shows three entries with scroll arrows [observed 2026-10-19, od_20's
    # shots]. TapKey of the Down arrow did not move it [observed 2026-10-19], so the
    # mouse wheel (xNVSE's key code 265, wheel down) is tried, a picture after each turn.
    for k in range(SCROLL_STEPS):
        steps += [act(f"TapKey {WHEEL_DOWN}"), wait(20), shot(f"{name}_down_{k + 1}")]
    steps += menus(f"{name} topic list") + close_menus()
    return steps


def goodbye_lists():
    """Dialogue (visual review): does the original add the generic GOODBYE to a topic list
    whose only Goodbye entries run an end script (Johnson Nash's Caravan offer; Ringo's)?"""
    steps = dialogue_screen("ringo", RINGO) + dialogue_screen("johnson_nash", NASH)
    return job("20_dlg_goodbye_lists",
               "Topic lists of Ringo and Johnson Nash: is a generic Goodbye shown (owner-review: item 22, visual) (about 1.5 min)",
               start_text(SAVE_TEST, "talks to RingoRef, then to Johnson Nash in Primm; screenshots."), steps,
               prelude=True)


def info_connection():
    """Dialogue (visual review): Red Lucy's topic list; her choice "What's this place?"
    has no line of its own, only a shared line (INFC) of another topic (dialogue.md 1.2)."""
    steps = dialogue_screen("red_lucy", RED_LUCY)
    return job("21_dlg_info_connection",
               "Red Lucy's topic list: is a choice with only a shared line offered (owner-review: dialogue (e), visual) (about 1 min)",
               start_text(SAVE_TEST, "player to Red Lucy in the Thorn; screenshots."), steps, prelude=True)


def random_end():
    """Dialogue: GOODBYE's pool for Mick and Ralph is Mick 0x00143A86, Ralph 0x00143A87,
    Mick 0x00143A88 (Random End), Ralph 0x00143A89 (Random End). If an unsayable Random
    End line still closes the pool, Ralph always says 0x00143A87; if not, he also says
    0x00143A89. The second round's Tab in the topic list did nothing (TapKey does not
    reach the dialogue menu) [observed 2026-10-19], so Ralph is made to say GOODBYE by
    script (Say with subtitles, every RALPH_EVERY samples, RALPH_SAYS times) inside an
    observe step, with a screenshot while he talks (the subtitle names the line). xNVSE's
    SayToDone event is recorded too; its second argument read the topic (GOODBYE), not
    the line [observed 2026-10-19]."""
    steps = go_to(RALPH, 0, 0) + [act("{0}.MoveTo player 0 120 0", [RALPH]), wait(MOVETO_FRAMES), face(RALPH)]
    timed = [{"after": RALPH_EVERY, "repeat": True, "script": {"lines": ["{0}.Say {1} 1"], "names": [RALPH, GOODBYE_TOPIC]}}]
    session = {"name": "goodbyes", "hours": 2.0, "every": 10, "settle_frames": 1, "track": [RALPH], "fields": "fight",
               "events": ["SayToDone"], "timed": timed, "close_menus_after": 0,
               "shots": {"label": "ralph_says", "when": ["{0}.IsTalking", RALPH], "gap": 30, "max": RALPH_SAYS + 2}}
    campaign = {"format": observe.CAMPAIGN_FORMAT, "name": "od22", "sessions": [session]}
    obs = observe.session_job(campaign, session, CTX)["steps"]
    for st in obs:
        if "observe" in st:
            st["observe"]["max_samples"] = RALPH_EVERY * RALPH_SAYS + 10
    steps += obs
    return job("22_dlg_random_end",
               "Ralph's goodbyes: does an unsayable Random End line close the pool (owner-review: #120) (about 3 min)",
               start_text(SAVE_TEST, "player to Ralph in Mick & Ralph's (Freeside); Ralph says GOODBYE twelve times by script."), steps,
               prelude=True)


def all_jobs():
    return [no_greeting(), teammate_door(), enable_controls_sneak(), set_actors_ai(), reset_ai(),
            script_packages(), disabled_ref_script(), carried_item_script(), stray_markers(), clock_in_dialogue(),
            attack_in_dialogue(), assault_faction(), set_alert(), goodbye_lists(), info_connection(), random_end()]


def main():
    for j in all_jobs():
        path = os.path.join(HERE, j["name"] + ".steps.json")
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            json.dump(j, f, indent=1)
            f.write("\n")
        print(path, len(j["steps"]), "steps")


if __name__ == "__main__":
    main()
