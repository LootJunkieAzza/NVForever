#!/usr/bin/env python3
"""Writes the merchant restock capture jobs of 2026-10-08 (docs/spec-notes/simulation.md
section 3, docs/spec-notes/layer4-captures.md section 2) into this folder. All run
unattended on the save `nvftest` (inside the Goodsprings gas station, GameDaysPassed
12.388, 25 October 2281), the player staying in the cell; time moves by GameHour.

  python tools/capture/jobs/make_restock_jobs.py
"""
import json
import os

import make_backlog_jobs as mb

BOX = mb.RINGO_BOX
RINGO = "0x00104C7D"     # Ringo (ACHR), whose XMRC names the box
STIMPAK = "0x00015169"


class Job:
    """Steps with a running count of hours waited (labels say when a read happened)."""

    def __init__(self, every, fields=mb.CLOCK[:2], ringo=False):
        self.steps, self.n, self.h = [], 0, 0
        self.every, self.fields, self.ringo = every, fields, ringo

    def read(self, tag, stim=False):
        self.steps += [mb.q("{0}.GetItemCount {1}", tag, "count", BOX, mb.CAPS)]
        if stim:
            self.steps += [mb.q("{0}.GetItemCount {1}", tag, "stim", BOX, STIMPAK)]
        self.steps += mb.clock(tag, self.fields)
        if self.ringo:
            self.steps += [mb.cs.query(mb.cs.cell_query(RINGO, mb.CTX), tag, "ringo_cell"),
                           mb.q("{0}.GetCurrentAIPackage", tag, "ringo_pkg", RINGO),
                           mb.q("{0}.GetDisabled", tag, "ringo_disabled", RINGO)]

    def wait(self, hours, stim=False):
        for _ in range(hours // self.every):
            self.steps += mb.advance(self.n, self.every)
            self.n += 1
            self.h += self.every
            self.read(f"t+{self.h}", stim)

    def empty(self, tag=None):
        self.steps += mb.act(["{0}.RemoveAllItems"], BOX)
        self.read(f"{tag} emptied t+{self.h}" if tag else f"emptied t+{self.h}")


def gmst(name):
    return mb.cs.query(f"float fV\nBegin Function {{ }}\nlet fV := GetGameSetting \"{name}\"\n"
                       "SetFunctionValue fV\nEnd\n", "gmst", name)


def reads():
    """The midnight-rule scenario's sequence with read-only checks every 2 hours."""
    j = Job(2)
    j.read("start")
    j.empty("E0")
    j.wait(72)
    j.wait(96)
    j.empty("E7")
    j.wait(24)
    j.empty("E8")
    j.steps += mb.act(["{0}.AddItem {1} 1"], BOX, STIMPAK)
    j.read(f"stim added t+{j.h}", True)
    j.wait(72, stim=True)
    j.steps += [gmst("iHoursToRespawnCell"), gmst("iDaysToRespawnVendor")]
    return mb.job("mr_diag_reads", "Merchant restock diagnostic: midnight scenario with 2-hourly reads",
                  "nvftest (inside the Goodsprings gas station)", j.steps)


def calendar():
    steps = mb.clock("c0")
    for i in range(12):
        steps += mb.advance(i, 24) + mb.clock(f"c{i + 1}")
    return mb.job("mr_diag_calendar", "nvftest calendar over 12 days (read-only)", "nvftest", steps)


def month_short(ringo=False):
    """Emptied at once after each restock, across the October/November boundary."""
    j = Job(2, ringo=True) if ringo else Job(6, mb.CLOCK[:4])
    j.read("start")
    j.empty()
    j.wait(48)
    j.empty()
    j.wait(72)
    j.empty()
    j.wait(96)
    if ringo:
        return mb.job("mr_diag_ringo", "month_short sequence with Ringo's cell and AI package every 2 h",
                      "nvftest", j.steps)
    return mb.job("mr_diag_month_short", "restock across the October/November boundary, short untouched spans",
                  "nvftest", j.steps)


def long_untouched():
    j = Job(6, mb.CLOCK[:4])
    j.read("start")
    j.empty()
    j.wait(168)
    j.empty()
    j.wait(48)
    j.wait(120)
    j.empty()
    j.wait(48)
    return mb.job("mr_diag_long_untouched", "restock after a long untouched span inside November", "nvftest",
                  j.steps)


def untouched(name, hours):
    j = Job(6)
    j.read("start")
    j.empty()
    j.wait(168)
    j.empty()
    j.wait(48)
    j.wait(hours)
    j.empty()
    j.wait(96)
    return mb.job(name, f"restock after {hours} h untouched inside November", "nvftest", j.steps)


def predict_days():
    """Held-out check of the restock days (0 or 3 modulo 7): 31, 35, 38, 42."""
    steps, n = [], 0

    def read(tag):
        return [mb.q("{0}.GetItemCount {1}", tag, "count", BOX, mb.CAPS)] + mb.clock(tag, mb.CLOCK[:2])

    def empty(tag):
        return mb.act(["{0}.RemoveAllItems"], BOX) + read(tag)

    steps += read("start")
    for _ in range(17):
        steps += mb.advance(n, 24)
        n += 1
    steps += read("day 29") + empty("emptied day 29")
    for d in range(30, 43):
        steps += mb.advance(n, 24)
        n += 1
        steps += read(f"day {d}")
        if d in (31, 35, 38):
            steps += empty(f"emptied day {d}")
    return mb.job("mr_predict_days", "held-out check of the restock days 31, 35, 38, 42", "nvftest", steps)


def main():
    for j in (reads(), calendar(), month_short(), long_untouched(), month_short(ringo=True),
              untouched("mr_diag_untouched_3d", 72), untouched("mr_diag_untouched_4d", 96), predict_days()):
        path = os.path.join(mb.HERE, f"{j['name']}.steps.json")
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(json.dumps(j, indent=1) + "\n")
        print(f"{path}: {len(j['steps'])} steps")


if __name__ == "__main__":
    main()
