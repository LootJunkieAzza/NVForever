"""Unit tests for tools/stress (no game needed): `python -m unittest discover -s tools/stress/tests`."""
from __future__ import annotations

import os
import re
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from stress import battle, cells, lib, tour  # noqa: E402

REPORT = textwrap.dedent("""\
    NVForever 0.2.0 crashed (panic)
    time (UTC): 2026-10-18 10:00:00
    thread: Compute Task Pool (3)
    message: index out of bounds: the len is 3 but the index is 7
    at: crates/fnv-nif/src/scene.rs:812:17
    where: WastelandNV cell -2,26 CrimsonCaravan, position 1, 2, 3
    command line: nvforever.exe
    run log: C:/h/logs/run.log

    backtrace:
       0: std::backtrace_rs::backtrace::trace::h0123456789abcdef
       1: std::panicking::begin_panic_handler
       2: core::panicking::panic_bounds_check
       3: fnv_nif::scene::Decoded::block::h89abcdef01234567
       4: nvf_assets::models::load
       5: nvforever::world::spawn_cell
       6: bevy_ecs::system::function_system::run
       7: BaseThreadInitThunk
    """)


class CrashReports(unittest.TestCase):
    def test_a_report_gives_message_place_and_the_engines_own_frames(self):
        c = lib.parse_crash(REPORT, "crash-1.txt")
        self.assertEqual(c.message, "index out of bounds: the len is 3 but the index is 7")
        self.assertEqual(c.location, "crates/fnv-nif/src/scene.rs:812:17")
        self.assertEqual(c.frames[:3], ["fnv_nif::scene::Decoded::block", "nvf_assets::models::load", "nvforever::world::spawn_cell"])
        self.assertNotIn("BaseThreadInitThunk", c.frames)

    def test_crashes_group_by_cause_whatever_the_numbers(self):
        a = lib.parse_crash(REPORT)
        b = lib.parse_crash(REPORT.replace("index is 7", "index is 9").replace(":812:17", ":812:30"))
        other = lib.parse_crash(REPORT.replace("index out of bounds", "called `Option::unwrap()` on a `None` value"))
        groups = lib.group_crashes([("cell A", a), ("cell B", b), ("cell C", other)])
        self.assertEqual([len(g["where"]) for g in groups], [2, 1])
        self.assertEqual(groups[0]["where"], ["cell A", "cell B"])


def fake_world():
    wl = {"worldspaces": [{"formid": "0x000DA726", "edid": "WastelandNV"}, {"formid": "0x01000F2B", "edid": "SierraMadre"}]}
    per = {
        "0x000DA726": {"cells": [{"formid": "0x000DA727", "edid": None, "grid": None},
                                 {"formid": "0x000DA7A6", "edid": "Goodsprings", "grid": [-17, 1]},
                                 {"formid": "0x000DA7A7", "edid": None, "grid": [-16, 2]}]},
        "0x01000F2B": {"cells": [{"formid": "0x01000F30", "edid": None, "grid": [0, 0]}]},
    }
    ints = {"cells": [{"formid": "0x00010C0E", "edid": "GSGeneralStore"}]}
    return cells.cells_from_json(wl, per, ints)


class CellsTest(unittest.TestCase):
    def test_the_list_has_interiors_then_grid_cells_without_persistent_ones(self):
        cs = fake_world()
        self.assertEqual([c.kind for c in cs], ["interior", "exterior", "exterior", "exterior"])
        self.assertEqual(cs[0].line(), "interior 00010C0E GSGeneralStore")
        self.assertEqual(cs[1].line(), "exterior 000DA726 -17 1 000DA7A6 Goodsprings")
        self.assertEqual(cs[2].label, "WastelandNV")  # a cell without its own Editor ID

    def test_choose_by_kind_stride_world_and_limit(self):
        cs = fake_world()
        self.assertEqual(len(cells.choose(cs, "interior")), 1)
        self.assertEqual([c.cell for c in cells.choose(cs, "exterior", stride=2)], ["000DA7A7", "01000F30"])
        self.assertEqual([c.world for c in cells.choose(cs, "exterior", worlds=["SierraMadre"])], ["01000F2B"])
        self.assertEqual(len(cells.choose(cs, limit=2)), 2)
        self.assertEqual([len(b) for b in cells.batches(cs, 3)], [3, 1])

    def test_a_batch_log_gives_loads_skips_and_the_last_cell_started(self):
        lines = ["INFO nvf_play::stress: stress cell: 1/3 start interior 00010C0E GSGeneralStore",
                 "INFO nvf_play::stress: stress cell: 1/3 loaded in 1.25 s interior 00010C0E GSGeneralStore",
                 "INFO nvf_play::stress: stress cell: 2/3 start exterior ...",
                 "INFO nvf_play::stress: stress cell: 2/3 skipped: not in the worldspace's grid",
                 "INFO nvf_play::stress: stress cell: 3/3 start exterior ..."]
        self.assertEqual(cells.read_batch(lines), {"loaded": {1: 1.25}, "skipped": {2: "not in the worldspace's grid"}, "last_started": 3})

    def test_a_crash_ends_its_batch_and_the_sweep_goes_on_after_the_cell(self):
        cs = fake_world()
        calls = []

        def fake_run(ctx, launch):
            listed = Path(launch.aids["NVF_PLAY_TEST_CELLS"].split(";")[0]).read_text().splitlines()[1:]
            calls.append(listed)
            lines, status, crashes = [], "ok", []
            for i, ln in enumerate(listed, 1):
                lines.append(f"stress cell: {i}/{len(listed)} start {ln}")
                if "000DA7A6" in ln:  # Goodsprings crashes
                    status, crashes = "crashed", [lib.parse_crash(REPORT)]
                    break
                lines.append(f"stress cell: {i}/{len(listed)} loaded in 0.5 s {ln}")
            return lib.Result(launch.name, 101 if crashes else 0, 1.0, status, Path("x.log"), lines, crashes, [])

        with tempfile.TemporaryDirectory() as d:
            ctx = lib.Ctx(Path(d), Path("nvforever"), "Data", Path("fnvtool"), Path(d), say=lambda *_: None)
            real, lib.run = lib.run, fake_run
            try:
                res = cells.run(ctx, cs, batch=3, dwell=0, hang=10, start_args=[])
            finally:
                lib.run = real
        self.assertEqual(len(calls), 2)
        self.assertEqual(len(calls[1]), 2)  # the two cells after Goodsprings
        self.assertEqual((res["total"], res["ok"], res["crashed"]), (4, 3, 1))
        self.assertEqual(res["crash_groups"][0]["where"], [cs[1].name()])
        self.assertTrue(any("Crash groups" in ln for ln in cells.report(res)))


def many_cells(n: int) -> list[cells.Cell]:
    return [cells.Cell("exterior", f"{0x100000 + i:08X}", "WastelandNV", "000DA726", i % 40 - 20, i // 40 - 20) for i in range(n)]


def fake_game(crash_on: set[str], crash_after_load: bool = False, calls: list | None = None):
    """A stand-in for lib.run: logs every cell of the batch's list as loaded, and crashes
    on the cells named (before their load line, or after it)."""
    def run(ctx, launch):
        listed = Path(launch.aids["NVF_PLAY_TEST_CELLS"].split(";")[0]).read_text().splitlines()[1:]
        if calls is not None:
            calls.append(len(listed))
        lines, status, crashes = [], "ok", []
        for i, ln in enumerate(listed, 1):
            lines.append(f"stress cell: {i}/{len(listed)} start {ln}")
            bad = any(c in ln for c in crash_on)
            if bad and not crash_after_load:
                status, crashes = "crashed", [lib.parse_crash(REPORT)]
                break
            lines.append(f"stress cell: {i}/{len(listed)} loaded in 0.5 s {ln}")
            if bad:
                status, crashes = "crashed", [lib.parse_crash(REPORT)]
                break
        return lib.Result(launch.name, 101 if crashes else 0, 1.0, status, Path("x.log"), lines, crashes, [])
    return run


class SweepTest(unittest.TestCase):
    def sweep(self, out: Path, cs, batch: int, game, on_batch=None):
        ctx = lib.Ctx(out, Path("nvforever"), "Data", Path("fnvtool"), out, say=lambda *_: None)
        real, lib.run = lib.run, game
        try:
            return cells.run(ctx, cs, batch=batch, dwell=0, hang=10, start_args=[], on_batch=on_batch)
        finally:
            lib.run = real

    def test_a_big_batch_is_one_launch_and_stride_takes_negative_grid_lines(self):
        cs = many_cells(250)
        calls = []
        with tempfile.TemporaryDirectory() as d:
            res = self.sweep(Path(d), cs, 250, fake_game(set(), calls=calls))
        self.assertEqual((calls, res["total"], res["ok"]), ([250], 250, 250))
        strided = cells.choose(cs, "exterior", stride=4)
        self.assertTrue(strided and all(c.x % 4 == 0 and c.y % 4 == 0 for c in strided))
        self.assertIn(-20, {c.x for c in strided})

    def test_a_crash_after_the_load_line_is_still_that_cells(self):
        cs = many_cells(5)
        with tempfile.TemporaryDirectory() as d:
            res = self.sweep(Path(d), cs, 5, fake_game({cs[2].cell}, crash_after_load=True))
        self.assertEqual((res["total"], res["ok"], res["crashed"]), (5, 4, 1))
        bad = [r for r in res["cells"] if r["status"] == "crashed"]
        self.assertEqual((bad[0]["cell"], bad[0]["loaded_in"]), (cs[2].name(), 0.5))

    def test_a_stopped_sweep_resumes_with_its_whole_state(self):
        cs = many_cells(9)
        reports = []
        with tempfile.TemporaryDirectory() as d:
            out = Path(d)
            # The runner stops after its second batch (as if the machine went down).
            class Stop(Exception):
                pass

            def stop_after_two(res):
                reports.append(res["total"])
                if len(reports) == 2:
                    raise Stop

            with self.assertRaises(Stop):
                self.sweep(out, cs, 3, fake_game({cs[1].cell}), stop_after_two)
            self.assertTrue((out / cells.STATE).exists())
            calls = []
            res = self.sweep(out, cs, 3, fake_game({cs[1].cell}, calls=calls))
        # First run: batch 1 crashed on cell 2 (cells 1-2 done), batch 2 did cells 3-5.
        self.assertEqual(reports, [2, 5])
        self.assertEqual(calls, [3, 1])  # the rest: cells 6-8, then 9
        self.assertEqual((res["total"], res["ok"], res["crashed"], res["launches"]), (9, 8, 1, 4))
        self.assertEqual(res["crash_groups"][0]["where"], [cs[1].name()])
        self.assertEqual(len({r["cell"] for r in res["cells"]}), 9)


class TourTest(unittest.TestCase):
    def test_the_ring_lies_inside_the_grid(self):
        pts = tour.ring((-30, -30), (29, 29), n=4, inset=0.5)
        self.assertEqual(len(pts), 4)
        for x, y in pts:
            self.assertLess(abs(x), 30 * 4096 * 0.51)
            self.assertLess(abs(y), 30 * 4096 * 0.51)
        text = tour.route_text("0x000DA726", pts, 1024, 1, 2)
        self.assertIn("laps 2", text)
        self.assertIn("world 0x000DA726", text)

    def sample(self, t, lap_entities):
        return (f"INFO nvforever::stress_sample: stress sample: t {t:.1f} s, frames 60, frame mean 16.70 ms, frame worst 30.10 ms, "
                f"entities {lap_entities}, refs drawn 2000, meshes 900, images 1200, materials 300, audio 4, cells 25, "
                f"bodies 3000/10/40, actors 12, nan actors 0, ragdolls 0, projectiles 0, at WastelandNV cell -17,1 Goodsprings, position 1, 2, 3")

    def test_a_sample_line_reads_into_numbers(self):
        s = tour.parse_sample(self.sample(5, 10000))
        self.assertEqual((s["t"], s["entities"], s["bodies"], s["refs drawn"], s["frame mean"]), (5.0, 10000, 3050, 2000, 16.7))
        self.assertTrue(s["at"].startswith("WastelandNV cell -17,1"))

    def test_a_count_that_stays_up_on_coming_back_is_a_possible_leak(self):
        lines = []
        for lap in (1, 2, 3):
            lines.append(f"INFO stress tour: lap {lap} starts")
            for wp in (1, 2):
                lines.append(f"INFO stress tour: lap {lap} at waypoint {wp}/2 (0, 0)")
                lines.append(self.sample(lap * 10 + wp, 10000 + (lap - 1) * 2000))
        samples = tour.tag_laps(lines)
        self.assertEqual([(s["lap"], s["waypoint"]) for s in samples][:2], [(1, 1), (1, 2)])
        leaks = tour.growth(samples)
        self.assertIn("entities", leaks)
        self.assertTrue(leaks["entities"]["rising_every_lap"])
        self.assertEqual(leaks["entities"]["grew_at"][0]["first"], 10000)
        self.assertNotIn("meshes", leaks)

    def test_meshes_kept_at_a_laps_end_are_not_released(self):
        # Lap 1: meshes rise in a town and stay up back at the start; lap 2: they fall back.
        held = [800] * 5 + [4000] * 10 + [4700] * 5
        freed = [800] * 5 + [4000] * 10 + [850] * 5
        samples = ([{"lap": 1, "meshes": v, "images": 600} for v in held]
                   + [{"lap": 2, "meshes": v, "images": 600} for v in freed])
        bad = tour.not_released(samples)
        self.assertEqual(list(bad), ["meshes"])
        self.assertEqual(bad["meshes"], [{"lap": 1, "start": 800, "end": 4700, "peak": 4700}])

    def test_start_up_and_hitches_read_from_the_log(self):
        lines = ["INFO nvforever::stress_sample: stress start-up: 61 frames, 4210 ms, slowest 2545.84 ms",
                 "INFO stress hitch: 300 ms at t 1633.2 s, 2 cells loaded in it (180 ms), at WastelandNV cell -4,-12, position 1, 2, 3",
                 "INFO stress hitch: 340 ms at t 1663.3 s, 0 cells loaded in it (0 ms), at WastelandNV cell 2,-13"]
        startup, hitches = tour.startup_and_hitches(lines)
        self.assertEqual(startup, {"frames": 61, "ms": 4210.0, "slowest_ms": 2545.84})
        self.assertEqual([(h["ms"], h["cells"]) for h in hitches], [(340.0, 0), (300.0, 2)])
        self.assertTrue(hitches[1]["at"].endswith("position 1, 2, 3"))

    def test_memory_goes_on_the_nearest_sample(self):
        samples = [{"t": 1.0}, {"t": 10.0}]
        tour.add_memory(samples, [(0.5, 100, 200), (9.0, 300, 400)])
        self.assertEqual((samples[0]["working set"], samples[1]["private bytes"]), (100, 400))


class BattleTest(unittest.TestCase):
    def test_the_layout_puts_the_sides_across_the_centre(self):
        aid, player = battle.layout(10, 0, (0.0, 0.0), 3000.0)
        self.assertEqual(aid, "5;LvlNCRTrooper:10:-1500:0;LvlLegionRaiderMelee:10:1500:0")
        self.assertEqual(player, (0.0, -1800.0))
        self.assertEqual(battle.layout(10, 5, (0.0, 0.0))[0].count(";"), 3)

    def test_the_log_gives_kills_hits_lines_switches_and_spacing(self):
        t = "2026-10-18T10:00:{:02d}.000000Z  INFO "
        lines = [
            t.format(0) + "nvf_play::stress: stress battle: group 1 LvlNCRTrooper (0x0010A4F2): 10 of 10 placed around (-1500, 0): 0xFF000800..0xFF000809",
            t.format(0) + "nvf_play::stress: stress battle: group 2 LvlLegionRaiderMelee (0x001252E6): 10 of 10 placed around (1500, 0): 0xFF00080A..0xFF000813",
            t.format(0) + "nvf_play::stress: stress battle: 2 groups, 20 fighters, 20 set on the nearest enemy",
            t.format(3) + "nvforever::npc_combat: npc combat: LvlNCRTrooper 0xFF000801 fights LvlLegionRaiderMelee 0xFF000810 with WeapNVServiceRifle (gun)",
            t.format(9) + "nvforever::npc_combat: npc combat: LvlNCRTrooper 0xFF000801 fights LvlLegionRaiderMelee 0xFF000811 with WeapNVServiceRifle (gun)",
            t.format(10) + "nvforever::npc_combat: npc combat: LvlLegionRaiderMelee 0xFF000810 swings at LvlNCRTrooper 0xFF000801 from 84 units (feet to feet, reach 96)",
            t.format(12) + "nvf_play::combat: combat line: 0xFF000801 says Attack line 0015B6C4",
            t.format(20) + "nvforever::combat_tactics: battle: LvlLegionRaiderMelee 0xFF000810 killed by LvlNCRTrooper 0xFF000801",
            # Created actors have no Editor ID: FormIDs alone.
            t.format(25) + "nvforever::combat_tactics: battle: 0xFF000802 killed by 0xFF000811",
            t.format(30) + "nvforever::npc_combat: npc combat: LvlLegionRaiderMelee 0xFF000811 takes cover 300 units away",
            t.format(59) + "nvforever::combat_tactics: battle summary: 2 sides, 20 actors; NCRFactionNV (10): 9 standing, 1 dead, 30 hits, 4 kills, 0 flights; CaesarsLegionFaction (10): 6 standing, 4 dead, 10 hits, 1 kills, 2 flights; first fighter at 1 s",
            t.format(59) + "nvforever::npc_combat: npc combat summary: 20 actors fought (...); 200 shots (200 from a muzzle, 0 from the eyes; 6 down the sights, 194 from the hip, sights raised 2 times), 12 blows (1 went on a combo, the longest 2; 0 missed), 7 reloads (0 round by round); 9 flee checks, 2 fled; the player was hit 0 times",
            t.format(59) + "nvf_play::combat: combat: 0 shots (0 drawn from the muzzle, 0 from behind cover), 200 projectiles, 50 actor hits, 150 world hits (0 objects' hit blocks, 0 objects knocked away), 0 explosions, 0 kills, 12 melee impacts",
            t.format(59) + "nvforever: actor animation: up to 17 actors posed at once, 0.799 ms per frame on average",
        ]
        c = battle.analyse(lines)
        self.assertEqual(c["placed"][0], {"group": 1, "base": "LvlNCRTrooper", "placed": 10, "asked": 10, "ids": [0xFF000800, 0xFF000809]})
        self.assertEqual((c["time_to_kill_s"]["first"], c["time_to_kill_s"]["count"]), (20.0, 2))
        self.assertEqual(c["deaths_by_group"], {"LvlLegionRaiderMelee": {"dead": 1, "median_s": 20.0, "last_s": 20.0},
                                                "LvlNCRTrooper": {"dead": 1, "median_s": 25.0, "last_s": 25.0}})
        self.assertEqual(c["melee_impacts"], 12)
        self.assertEqual((c["shots"], c["actor_hits"], c["hit_rate"], c["fled"]), (200, 50, 0.25, 2))
        self.assertEqual((c["target_switches"], c["cover_taken"]), (1, 1))
        self.assertEqual(c["melee_spacing"]["closest"], 84)
        self.assertEqual(c["combat_lines_per_minute"], {"Attack": 1.0})
        self.assertEqual([s["faction"] for s in c["sides"]], ["NCRFactionNV", "CaesarsLegionFaction"])
        self.assertIn("nvforever", c["system_times"])


@unittest.skipUnless(os.name == "nt", "the fake game is a .cmd")
class LaunchTest(unittest.TestCase):
    """lib.run against a fake game: a script that prints lines, then crashes or hangs."""

    def fake(self, d: Path, body: str) -> Path:
        script = d / "fake_game.py"
        script.write_text(textwrap.dedent(body), encoding="utf-8")
        cmd = d / "fake_game.cmd"
        cmd.write_text(f'@"{sys.executable}" "{script}" %*\n', encoding="utf-8")
        return cmd

    def test_a_crash_report_in_the_launch_home_is_collected(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            exe = self.fake(d, f"""
                import os, pathlib, sys
                print("stress cell: 1/1 start interior 00010C0E", flush=True)
                logs = pathlib.Path(os.environ["NVFOREVER_HOME"]) / "logs"
                logs.mkdir(parents=True, exist_ok=True)
                (logs / "crash-20261018-100000-1.txt").write_text({REPORT!r})
                assert os.environ["NVF_PLAY_TEST_STRESS"] == "1"
                sys.exit(101)
                """)
            ctx = lib.Ctx(d, exe, "Data", Path("fnvtool"), d)
            r = lib.run(ctx, lib.Launch("one", [], {}, run_seconds=5, progress=re.compile("stress"), hang_seconds=30))
            self.assertEqual((r.status, r.exit), ("crashed", 101))
            self.assertEqual(r.crashes[0].message, "index out of bounds: the len is 3 but the index is 7")
            self.assertIn("stress cell: 1/1 start", r.lines[0])
            self.assertTrue((d / "one.cmd.txt").exists())

    def test_no_progress_is_a_hang(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            exe = self.fake(d, """
                import time
                print("stress cell: 1/1 start", flush=True)
                time.sleep(60)
                """)
            ctx = lib.Ctx(d, exe, "Data", Path("fnvtool"), d)
            r = lib.run(ctx, lib.Launch("hang", [], {}, run_seconds=120, progress=re.compile("stress"), hang_seconds=2, memory_every=0.5))
            self.assertEqual(r.status, "hung")
            self.assertTrue(r.stacks and r.stacks.exists())


if __name__ == "__main__":
    unittest.main()
