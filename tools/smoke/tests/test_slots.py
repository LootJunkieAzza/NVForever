"""Smoke slots (tools/smoke/smoke/slots.py) and the parallel runner (cli.run_launches)."""
from __future__ import annotations

import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))

from smoke import cli  # noqa: E402
from smoke.plan import Launch  # noqa: E402
from smoke.runner import OWNER, Locks, Queue, Verdict  # noqa: E402
from smoke.scenarios import GAME, Scenario  # noqa: E402
from smoke.slots import FileLock, Slot, Slots, file_is_locked  # noqa: E402


def quiet(_):
    pass


class Machine:
    """A temp folder laid out like the owner's: a capture lock and the shared smoke lock."""

    def __init__(self, d: str):
        self.root = Path(d)
        self.gate = self.root / "capture" / "run.lock"
        self.shared = self.root / "repo" / ".claude" / "smoke.lock"
        self.live: set[int] = set()

    def runner(self, pid: int, count: int = 3, **kw) -> Slots:
        self.live.add(pid)
        q = Queue(self.gate.parent / "smoke-queue", pid=pid, alive=lambda p: p in self.live)
        return Slots(self.shared, [self.gate], count=count, poll=0.01, wait_max=0.1, say=quiet,
                     queue=q, running=lambda: True, **kw)


class FileLockTests(unittest.TestCase):
    def test_a_second_holder_is_refused_until_unlocked(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "x.lock"
            a, b = FileLock(p), FileLock(p)
            self.assertTrue(a.try_lock())
            self.assertFalse(b.try_lock())
            self.assertTrue(file_is_locked(p))
            a.unlock()
            self.assertFalse(file_is_locked(p))
            self.assertTrue(b.try_lock())
            b.unlock()


class SlotTests(unittest.TestCase):
    def test_one_runner_takes_every_slot_and_the_shared_lock_lives_as_long(self):
        with tempfile.TemporaryDirectory() as d:
            m = Machine(d)
            r = m.runner(1, count=3)
            got = [r.acquire() for _ in range(3)]
            self.assertEqual(sorted(s.index for s in got), [1, 2, 3])
            self.assertTrue((m.shared / OWNER).read_text().startswith("shared"))
            self.assertFalse(m.gate.exists())  # the capture lock is never taken
            self.assertIn("in use", r.try_take())
            r.release(got[0])
            self.assertTrue(m.shared.is_dir())  # two still run
            r.release(got[1])
            r.release(got[2])
            self.assertFalse(m.shared.exists())  # the last one out removes it
            r.finish()

    def test_waits_while_the_capture_lock_is_held_and_never_touches_it(self):
        with tempfile.TemporaryDirectory() as d:
            m = Machine(d)
            m.gate.mkdir(parents=True)
            r = m.runner(1)
            with self.assertRaises(TimeoutError):
                r.acquire()
            self.assertTrue(m.gate.is_dir())
            self.assertFalse(m.shared.exists())
            m.gate.rmdir()
            s = r.acquire()
            r.release(s)
            r.finish()

    def test_a_capture_mkdir_fails_while_a_slot_runs(self):
        with tempfile.TemporaryDirectory() as d:
            m = Machine(d)
            r = m.runner(1)
            s = r.acquire()
            with self.assertRaises(FileExistsError):
                m.shared.mkdir()  # what a capture (or an old-style runner) does
            old = Locks([m.gate, m.shared], poll=0.01, wait_max=0.05, say=quiet,
                        queue=Queue(m.root / "other-queue", pid=2, alive=lambda p: True))
            self.assertEqual(old.try_take(), m.shared)
            self.assertFalse(m.gate.exists())  # the old runner gave the capture lock back
            r.release(s)
            r.finish()

    def test_someone_elses_plain_smoke_lock_is_waited_for_not_removed(self):
        with tempfile.TemporaryDirectory() as d:
            m = Machine(d)
            m.shared.mkdir(parents=True)  # a capture took both and is running
            r = m.runner(1)
            with self.assertRaises(TimeoutError):
                r.acquire()
            self.assertTrue(m.shared.is_dir())
            self.assertFalse((m.shared / OWNER).exists())
            r.finish()

    def test_a_dead_old_style_runners_lock_is_cleared(self):
        with tempfile.TemporaryDirectory() as d:
            m = Machine(d)
            m.shared.mkdir(parents=True)
            (m.shared / OWNER).write_text("999999999 smoke runner\n")
            r = m.runner(1)
            s = r.acquire()
            self.assertTrue((m.shared / OWNER).read_text().startswith("shared"))
            r.release(s)
            r.finish()

    def test_a_shared_lock_left_with_no_slot_taken_is_cleared(self):
        with tempfile.TemporaryDirectory() as d:
            m = Machine(d)
            m.shared.mkdir(parents=True)
            (m.shared / OWNER).write_text("shared by smoke slots, a runner that crashed\n")
            r = m.runner(1)
            s = r.acquire()  # taken over as it is
            r.release(s)
            self.assertFalse(m.shared.exists())
            r.finish()

    def test_runners_share_the_slots_oldest_first(self):
        with tempfile.TemporaryDirectory() as d:
            m = Machine(d)
            first = m.runner(1, count=3)
            first.queue.join()
            time.sleep(0.002)
            second = m.runner(2, count=3)
            second.queue.join()
            self.assertEqual(first.share(), 2)
            self.assertEqual(second.share(), 1)
            a, b = first.acquire(), first.acquire()
            self.assertIn("share", first.try_take())  # its third would be the second runner's
            c = second.acquire()
            self.assertEqual(sorted(s.index for s in (a, b, c)), [1, 2, 3])
            self.assertIn("share", second.try_take())
            first.release(a)
            first.release(b)
            first.finish()  # the first is done: the second may now take all three
            m.live.discard(1)
            self.assertEqual(second.share(), 3)
            more = [second.acquire(), second.acquire()]
            for s in [c, *more]:
                second.release(s)
            second.finish()
            self.assertFalse(m.shared.exists())

    def test_a_runner_beyond_the_slot_count_waits_its_turn(self):
        with tempfile.TemporaryDirectory() as d:
            m = Machine(d)
            rs = []
            for pid in (1, 2, 3):
                r = m.runner(pid, count=2)
                r.queue.join()
                rs.append(r)
                time.sleep(0.002)
            self.assertEqual([r.share() for r in rs], [1, 1, 0])
            self.assertIn("queue", rs[2].try_take())
            for r in rs:
                r.finish()

    def test_an_old_style_runner_ahead_gets_its_turn_alone_and_one_behind_does_not_count(self):
        with tempfile.TemporaryDirectory() as d:
            m = Machine(d)
            m.live.add(9)
            old = Queue(m.gate.parent / "smoke-queue", pid=9, alive=lambda p: p in m.live)
            old.join()  # "9 smoke runner …": one game at a time
            time.sleep(0.002)
            r = m.runner(1, count=3)
            self.assertEqual(r.share(), 0)
            self.assertIn("queue", r.try_take())
            old.leave()
            m.live.discard(9)
            later = Queue(m.gate.parent / "smoke-queue", pid=10, alive=lambda p: True)
            later.join()  # an old-style runner behind: it waits for r anyway
            self.assertEqual(r.share(), 3)
            later.leave()
            r.finish()

    def test_one_slot_is_the_old_one_game_at_a_time(self):
        with tempfile.TemporaryDirectory() as d:
            m = Machine(d)
            r = m.runner(1, count=1)
            s = r.acquire()
            self.assertIsInstance(s, Slot)
            self.assertIn("in use", r.try_take())
            r.release(s)
            r.finish()


def launch(name: str, home: str | None = None, kind: str = GAME) -> Launch:
    env = {"NVFOREVER_HOME": home} if home else {}
    return Launch(name, kind, [Scenario(name)], ["--play"], env, 1.0)


class GateTests(unittest.TestCase):
    def gate(self, d, *argv):
        a = cli.parser().parse_args(["run", "--repo", d, *argv])
        lock = Path(d) / "smoke.lock"
        real = cli.default_lock_paths
        cli.default_lock_paths = lambda repo: [Path(d) / "run.lock", lock]
        try:
            return cli.game_gate(a, quiet)
        finally:
            cli.default_lock_paths = real

    def test_one_slot_means_the_machine_alone(self):
        with tempfile.TemporaryDirectory() as d:
            for argv in (["--slots", "1"], ["--exclusive"], ["--slots", "1", "--jobs", "3"]):
                g, jobs = self.gate(d, *argv)
                self.assertIsInstance(g, Locks, argv)
                self.assertEqual(jobs, 1, argv)
            g, jobs = self.gate(d, "--slots", "3")
            self.assertIsInstance(g, Slots)
            self.assertEqual(jobs, 3)


class ParallelRunTests(unittest.TestCase):
    def test_launches_with_the_same_home_stay_in_order_on_one_worker(self):
        ls = [launch("save-write", "save-home"), launch("a"), launch("save-load", "save-home"),
              launch("b"), launch("save-order", "save-home"), launch("tool", "save-home", kind="tool")]
        self.assertEqual(cli.chains(ls), [[0, 2, 4], [1], [3], [5]])

    def test_runs_at_once_and_reports_in_plan_order(self):
        ls = [launch("save-write", "h"), launch("a"), launch("b"), launch("save-load", "h")]
        running, peak, order = [0], [0], []
        guard = threading.Lock()

        def fake_run(x, env, locks):
            with guard:
                running[0] += 1
                peak[0] = max(peak[0], running[0])
                order.append(x.name)
            time.sleep(0.15)
            with guard:
                running[0] -= 1
            return 0, "", 0.15

        def fake_judge(x, env, code, err, secs, a, say, out):
            out.append(Verdict(x.name, "PASS"))

        real_run, real_judge = cli.run_launch, cli.judge_launch
        cli.run_launch, cli.judge_launch = fake_run, fake_judge
        try:
            vs = cli.run_launches(ls, SimpleNamespace(out=Path(".")), None, 3, SimpleNamespace(), quiet)
        finally:
            cli.run_launch, cli.judge_launch = real_run, real_judge
        self.assertEqual([v.scenario for v in vs], ["save-write", "a", "b", "save-load"])
        self.assertEqual(peak[0], 3)
        self.assertLess(order.index("save-write"), order.index("save-load"))


if __name__ == "__main__":
    unittest.main()
