"""The quest pilot's batch runner (tools/smoke/pilot/batch.py): chain picking and the summary."""

import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parents[1] / "pilot"
sys.path.insert(0, str(HERE))

import batch  # noqa: E402


class Picking(unittest.TestCase):
    def test_every_chain_by_default_and_named_ones(self):
        every = batch.chains([])
        self.assertTrue(len(every) > 100, "the walkthrough folder holds the chains")
        picked = batch.chains(["intro", "ghost-town-gunfight.actions.json"])
        self.assertEqual([batch.name_of(c) for c in picked], ["ghost-town-gunfight", "intro"])

    def test_main_chains_run_before_their_variants(self):
        picked = batch.chains(["bye-bye-love", "bye-bye-love-dawn", "beggar", "ant-misbehavin"])
        order = [batch.name_of(c) for c in batch.in_order(picked)]
        self.assertEqual(order, ["ant-misbehavin", "beggar", "bye-bye-love", "bye-bye-love-dawn"])

    def test_an_unknown_chain_is_refused(self):
        with self.assertRaises(SystemExit):
            batch.chains(["no-such-chain"])


class StartSave(unittest.TestCase):
    def test_a_missing_start_save_stops_the_batch(self):
        import contextlib, io, tempfile
        with tempfile.TemporaryDirectory() as home, contextlib.redirect_stderr(io.StringIO()) as err:
            code = batch.main(["--exe", "x", "--data", "x", "--fnvtool", "x", "--home", home, "--only", "intro", "--out", home])
        self.assertEqual(code, 2, "no game is started from a new game in its place")
        self.assertIn("no start save", err.getvalue())


class LoadOrder(unittest.TestCase):
    def test_formids_move_to_the_run_s_load_order(self):
        # A Dead Money chain's 0x01 is DeadMoney.esm; on a load order with it at 0x03 it moves.
        index = [0, 3]
        self.assertEqual(batch.remap_ids("0x0100D0A1", index), "0x0300D0A1")
        self.assertEqual(batch.remap_ids("scripted:0x0100C7F3 and 0100ABCD", index), "scripted:0x0300C7F3 and 0300ABCD")
        # The base game's (top byte 0), a name or a longer word are left alone.
        self.assertEqual(batch.remap_ids("0x0010A18E VMS18 0x0100D0A1FF", index), "0x0010A18E VMS18 0x0100D0A1FF")
        self.assertEqual(batch.remap_ids({"a": ["0x01000001"], "b": 3}, index), {"a": ["0x03000001"], "b": 3})

    def test_a_plugin_missing_from_the_order_keeps_its_ids(self):
        self.assertEqual(batch.remap_ids("0x01000001", [0, None]), "0x01000001")


class Summary(unittest.TestCase):
    def test_a_soft_lock_ranks_above_the_pilots_own_trouble(self):
        quiet = {"chain": "a", "report": {"steps": [{"step": 0}], "findings": [{"kind": "fight_not_won", "step": 0}], "divergences": [], "softlocks": 0, "headless": {}}}
        locked = {"chain": "b", "report": {"steps": [{"step": 0}], "findings": [], "divergences": [], "softlocks": 1, "headless": {}}}
        rows = batch.summarise([quiet, locked])
        self.assertEqual([r["chain"] for r in rows], ["b", "a"])
        self.assertEqual(rows[1]["findings"], {"fight_not_won": 1})

    def test_a_run_without_report_is_worst_and_said_so(self):
        crashed = {"chain": "c", "error": "no report"}
        fine = {"chain": "d", "report": {"steps": [], "findings": [], "divergences": [], "softlocks": 0}}
        rows = batch.summarise([fine, crashed])
        self.assertEqual(rows[0]["chain"], "c")
        text = batch.markdown(rows)
        self.assertIn("| c |", text)
        self.assertIn("**c**: no report", text)

    def test_quest_stoppers_are_quoted(self):
        r = {"chain": "e", "report": {"steps": [{"step": 3, "error": "x"}], "divergences": [], "softlocks": 0, "findings": [
            {"kind": "unreachable", "step": 3, "text": "door 00083919 is locked"},
            {"kind": "stuck", "step": 4, "text": "the last 10 units"},
        ]}}
        row = batch.summarise([r])[0]
        self.assertEqual(row["failed"], 1)
        self.assertEqual(len(row["first"]), 1)
        self.assertIn("door 00083919", row["first"][0])


if __name__ == "__main__":
    unittest.main()
