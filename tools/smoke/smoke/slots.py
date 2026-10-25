"""Smoke slots: up to N nvforever smoke launches on this machine at once.

NVForever's smoke games take their input from test aids (environment variables), not
from the keyboard, so they do not need the screen to themselves. The original game
does: FalloutNV stops when it loses focus, so a capture holds both game locks
(`C:\\fnv-private\\capture\\run.lock` and the main checkout's `.claude\\smoke.lock`) by
`mkdir` and no nvforever window may open beside it.

How the two fit together:

- A smoke launch takes one of `count` slots: `slot-<i>.lock` files in a `smoke-slots`
  folder beside `smoke.lock`, held with an OS file lock for the launch (the lock goes
  when the process does, so a crashed runner never leaves a slot taken).
- While any slot is taken, `smoke.lock` exists as a *shared* lock (its owner file
  starts with `shared`). A capture's `mkdir smoke.lock` fails, so it waits, exactly as
  it did while one smoke game ran. The last slot to go removes it.
- A smoke launch never takes `run.lock` (the capture lock is the original game's); it
  only waits while that lock, or a plain `smoke.lock` of someone else, is there.
- Old-style runners (one game at a time, `Locks`) still work beside this: their
  `mkdir smoke.lock` fails on the shared lock, and a slot runner waits on theirs.
- Fairness: runners keep their first-come, first-served tickets (`Queue`). The first
  `count` runners in the queue share the slots: with r of them, each gets
  `count // r` slots and the oldest `count % r` one more. Later runners wait.

Every change of the shared lock and every slot take happens under a short OS file
lock (`smoke-slots/mutex.lock`), so two runners cannot both decide they are first.
"""
from __future__ import annotations

import os
import shutil
import threading
import time
from pathlib import Path

from .runner import OWNER, QUEUE, Queue, game_running, pid_alive

SHARED = "shared"  # first word of the shared smoke.lock's owner file
SLOT_LABEL = "smoke runner slots"  # a slot runner's queue ticket (old-style tickets say "smoke runner")
MAX_SLOTS = 16  # slots looked at when deciding the shared lock can go (runners may differ in count)
DEFAULT_SLOTS = 3
# Seconds between two smoke game starts on the machine. Games that load at the same time
# slow each other's start [observed 2026-10-24: "scripts ready in" 6.4-9.3 s with the
# machine alone, 10-21 s when several of 3 slots started together; short scenarios then
# ended before the scripts ran ("scripts: never ran") or lost keys to the start-up
# message boxes]. A launch lasts about 40 s, so 3 slots start one about every 13 s anyway.
DEFAULT_STAGGER = 12.0
LAST_START = "last-start.txt"


def default_count() -> int:
    try:
        return max(1, int(os.environ.get("NVF_SMOKE_SLOTS", DEFAULT_SLOTS)))
    except ValueError:
        return DEFAULT_SLOTS


def default_stagger() -> float:
    try:
        return max(0.0, float(os.environ.get("NVF_SMOKE_STAGGER", DEFAULT_STAGGER)))
    except ValueError:
        return DEFAULT_STAGGER


# ---------------------------------------------------------------- OS file locks

class FileLock:
    """An exclusive, non-blocking OS lock on one file (released when the process ends)."""

    def __init__(self, path: Path):
        self.path = Path(path)
        self.f = None

    def try_lock(self) -> bool:
        if self.f is not None:
            return True
        self.path.parent.mkdir(parents=True, exist_ok=True)
        f = open(self.path, "a+b")
        try:
            if os.name == "nt":
                import msvcrt

                f.seek(0)
                msvcrt.locking(f.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl

                fcntl.flock(f.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            f.close()
            return False
        self.f = f
        return True

    def unlock(self) -> None:
        if self.f is None:
            return
        try:
            if os.name == "nt":
                import msvcrt

                self.f.seek(0)
                msvcrt.locking(self.f.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                import fcntl

                fcntl.flock(self.f.fileno(), fcntl.LOCK_UN)
        except OSError:
            pass
        self.f.close()
        self.f = None

    @property
    def held(self) -> bool:
        return self.f is not None


def file_is_locked(path: Path) -> bool:
    """Whether another holder has `path` locked (this looks without keeping it)."""
    if not path.exists():
        return False
    probe = FileLock(path)
    if probe.try_lock():
        probe.unlock()
        return False
    return True


def is_slot_ticket(t: Path) -> bool:
    try:
        return SLOT_LABEL in t.read_text()
    except OSError:
        return False


# ---------------------------------------------------------------- slots

class Slot:
    def __init__(self, index: int, lock: FileLock):
        self.index, self.lock = index, lock


class Slots:
    """The machine's smoke slots, for one runner (thread-safe: a runner's workers share it)."""

    def __init__(self, shared: Path, gates: list[Path], count: int | None = None, poll: float = 5.0,
                 wait_max: float = 4 * 3600.0, say=print, running=game_running, queue: Queue | None = None,
                 stale_after: float = 900.0, stagger: float | None = None, clock=time.time):
        self.shared = Path(shared)
        self.gates = [Path(g) for g in gates]
        self.count = count or default_count()
        self.stagger = default_stagger() if stagger is None else stagger
        self.clock = clock
        self.poll, self.wait_max, self.say = poll, wait_max, say
        self.running, self.stale_after = running, stale_after
        self.dir = self.shared.parent / "smoke-slots"
        first = self.gates[0] if self.gates else self.shared
        self.queue = queue if queue is not None else Queue(first.parent / QUEUE)
        self.queue.label = SLOT_LABEL
        self.mutex = FileLock(self.dir / "mutex.lock")
        self.guard = threading.Lock()  # this process's workers, one at a time through the mutex
        self.mine: list[Slot] = []
        self.told: set[str] = set()
        self.warned: set[tuple[Path, float]] = set()

    # -- queue share

    def share(self) -> int:
        """How many slots this runner may hold now (0: wait, others are first).

        An old-style runner (one game at a time; its ticket is not marked "slots") ahead
        in the queue gets its turn alone: it needs smoke.lock free, so slot runners behind
        it wait. Old-style runners behind wait for every ticket ahead of them anyway, so
        they do not count in the share."""
        self.queue.join()
        ahead, behind = self.queue.live()
        if any(not is_slot_ticket(t) for t in ahead):
            return 0
        if len(ahead) >= self.count:
            return 0
        room = self.count - len(ahead) - 1
        # Slot runners behind an old-style ticket wait for its turn alone, so they do not
        # share the slots now (counting them left one runner on one slot, 2026-10-24).
        before_old = 0
        for t in behind:
            if not is_slot_ticket(t):
                break
            before_old += 1
        sharing = len(ahead) + 1 + min(room, before_old)
        runners = min(sharing, self.count)
        return self.count // runners + (1 if len(ahead) < self.count % runners else 0)

    # -- the shared lock

    def _shared_state(self) -> str:
        """'free', 'shared', 'stale' (an old-style runner that is gone) or 'other'."""
        if not self.shared.exists():
            return "free"
        try:
            words = (self.shared / OWNER).read_text().split()
        except OSError:
            return "other"
        if words and words[0] == SHARED:
            return "shared"
        try:
            if not pid_alive(int(words[0])):
                return "stale"
        except (ValueError, IndexError):
            pass
        return "other"

    def _any_slot_taken(self) -> bool:
        return any(file_is_locked(self.dir / f"slot-{i}.lock") for i in range(1, MAX_SLOTS + 1))

    def _write_shared(self) -> None:
        (self.shared / OWNER).write_text(
            f"{SHARED} by smoke slots ({self.dir}), first taken by {os.getpid()} at {time.strftime('%Y-%m-%d %H:%M:%S')}\n")

    def _busy(self) -> str | None:
        """Why no slot can be taken now (None: one was taken into self.mine)."""
        for g in self.gates:
            if g.exists():
                self._warn_if_abandoned(g)
                return f"{g} ({self._holder(g)})"
        state = self._shared_state()
        if state == "other":
            self._warn_if_abandoned(self.shared)
            return f"{self.shared} ({self._holder(self.shared)})"
        if state == "stale":
            self.say(f"lock {self.shared}: left by a smoke runner that is gone; cleared")
            shutil.rmtree(self.shared, ignore_errors=True)
            state = "free"
        if self.stagger > 0:
            try:
                since = self.clock() - float((self.dir / LAST_START).read_text().split()[0])
            except (OSError, ValueError, IndexError):
                since = None
            if since is not None and 0 <= since < self.stagger:
                return f"spreading game starts ({self.stagger:.0f} s apart)"
        if state == "free":
            self.shared.parent.mkdir(parents=True, exist_ok=True)
            try:
                self.shared.mkdir()
            except FileExistsError:
                return f"{self.shared} (just taken by someone else)"
            self._write_shared()
        for i in range(1, self.count + 1):
            lock = FileLock(self.dir / f"slot-{i}.lock")
            if lock.try_lock():
                try:
                    (self.dir / f"slot-{i}.txt").write_text(f"{os.getpid()} smoke runner {time.strftime('%Y-%m-%d %H:%M:%S')}\n")
                except OSError:
                    pass
                try:
                    (self.dir / LAST_START).write_text(f"{self.clock()} slot {i} by {os.getpid()}\n")
                except OSError:
                    pass
                self.mine.append(Slot(i, lock))
                return None
        if state == "free" and not self._any_slot_taken():
            shutil.rmtree(self.shared, ignore_errors=True)  # made it and then found no slot (count 0?)
        return f"all {self.count} smoke slots in use"

    @staticmethod
    def _holder(p: Path) -> str:
        try:
            return "held by " + (p / OWNER).read_text().strip()
        except OSError:
            pass
        try:
            return "another session's mkdir lock, made at " + time.strftime("%H:%M:%S", time.localtime(p.stat().st_ctime))
        except OSError:
            return "just released"

    def _warn_if_abandoned(self, p: Path) -> None:
        try:
            made = p.stat().st_ctime
        except OSError:
            return
        if (p / OWNER).exists() or (p, made) in self.warned or time.time() - made < self.stale_after:
            return
        if self.running():
            return
        self.warned.add((p, made))
        self.say(f"!!! LOCK LOOKS ABANDONED: {p} was made {(time.time() - made) / 60:.0f} min ago "
                 f"({time.strftime('%H:%M', time.localtime(made))}) and no nvforever or FalloutNV runs. "
                 f"Not removed: ask the session that made it, or remove it if that session is gone.")

    def _clear_stale_shared(self) -> None:
        """A shared lock with no slot taken (its runners crashed between launches): remove it,
        so captures are not kept out. Under the mutex."""
        if self._shared_state() == "shared" and not self._any_slot_taken():
            shutil.rmtree(self.shared, ignore_errors=True)

    # -- taking and giving back

    def try_take(self) -> Slot | str:
        """A slot, or why there is none now."""
        with self.guard:
            share = self.share()
            if share == 0:
                return f"{len(self.queue.ahead())} runner(s) first in the smoke queue"
            if len(self.mine) >= share:
                return f"this runner's share of the slots ({share}) in use"
            if not self.mutex.try_lock():
                return "another runner is taking a slot"
            try:
                why = self._busy()
                return self.mine[-1] if why is None else why
            finally:
                self.mutex.unlock()

    def acquire(self) -> Slot:
        start = time.monotonic()
        last = None
        while True:
            got = self.try_take()
            if isinstance(got, Slot):
                return got
            quiet = got.startswith(("another runner is taking", "this runner's share", "spreading"))
            if got != last and not quiet:
                self.say(f"waiting: {got}")
                last = got  # the last reason said, so short quiet waits between do not repeat it
            if time.monotonic() - start > self.wait_max:
                raise TimeoutError(f"no smoke slot after {self.wait_max:.0f} s: {got}")
            time.sleep(self.poll if "queue" in got else min(self.poll, 2.0))

    def release(self, slot: Slot) -> None:
        with self.guard:
            while not self.mutex.try_lock():
                time.sleep(0.05)
            try:
                slot.lock.unlock()
                if slot in self.mine:
                    self.mine.remove(slot)
                self._clear_stale_shared()
            finally:
                self.mutex.unlock()

    def finish(self) -> None:
        for s in list(self.mine):
            self.release(s)
        self.queue.leave()
