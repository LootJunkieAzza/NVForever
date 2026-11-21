"""Actual fnvtool boundary over authored inputs; explicit external binary, no build.

Run the same file against a baseline and candidate FNVT_DIAGNOSTIC_BIN. Baseline
outcomes are recorded as they occur: missing commands/fields are not setup errors.
No installed Data, mod source, original game, native plugin or graphical acceptance.
"""

import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest


def sub(sig, data):
    return sig + struct.pack("<H", len(data)) + data


def record(sig, fid, data):
    return sig + struct.pack("<IIIIHH", len(data), 0, fid, 0, 15, 0) + data


def authored_plugin():
    header = record(b"TES4", 0, sub(b"HEDR", struct.pack("<fII", 1.34, 1, 0x801)))
    # Original authored SCPT with an unreachable unknown call. Coverage must inspect
    # it without executing the block. A real extender declaration selects Auto source
    # compilation; the original bytecode contains only a valid script-name marker.
    source = b"scn DiagnosticOwned\nstring_var ownedSourceSelection\nBegin GameMode\nif 0\nNvfDiagnosticOwnedMissing\nendif\nEnd\n"
    schr = struct.pack("<IIIIHH", 0, 0, 4, 0, 0, 1)
    unit = record(b"SCPT", 0x800, sub(b"SCHR", schr) + sub(b"SCDA", b"\x1d\x00\x00\x00") + sub(b"SCTX", source))
    return header + b"GRUP" + struct.pack("<I4si8s", len(unit) + 24, b"SCPT", 0, bytes(8)) + unit


class DiagnosticCliBoundary(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        value = os.environ.get("FNVT_DIAGNOSTIC_BIN")
        if not value:
            raise unittest.SkipTest("FNVT_DIAGNOSTIC_BIN is required; no runtime credit")
        cls.binary = Path(value).resolve(strict=True)
        cls.repo = Path(__file__).resolve().parents[3]
        cls.schema = cls.repo / "schemas/fnv"

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="nvf authored diagnostic ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.data = self.root / "Data"
        self.data.mkdir()
        (self.data / "Diagnostic.esm").write_bytes(authored_plugin())

    def cli(self, *args, success=True, error_contains=None):
        result = subprocess.run([str(self.binary), *map(str, args)], capture_output=True, timeout=30, cwd=self.repo)
        # Test controller may preserve these raw streams before TemporaryDirectory
        # cleanup. They contain authored inputs only.
        print(json.dumps({"argv": list(map(str, args)), "exit": result.returncode,
                          "stdout": result.stdout.decode("utf8", "replace"),
                          "stderr": result.stderr.decode("utf8", "replace")}))
        if success:
            self.assertEqual(result.returncode, 0, result.stderr.decode("utf8", "replace"))
            return json.loads(result.stdout)
        self.assertNotEqual(result.returncode, 0)
        self.assertIsNotNone(error_contains, "negative calls need their actual refusal boundary")
        self.assertIn(error_contains, result.stderr.decode("utf8", "replace"))
        return result

    def test_connections_inspects_unreachable_real_compiled_call_without_running(self):
        doc = self.cli("ext", "connections", self.data, "Diagnostic.esm", "--schema", self.schema)
        self.assertFalse(doc["executed"])
        plugin = doc["plugins"][0]
        self.assertEqual(plugin["units_kept_bytes"], 0)
        use = next(row for row in plugin["functions"] if row["name"].lower() == "nvfdiagnosticownedmissing")
        self.assertEqual(use["status"], "unknown")
        self.assertEqual((use["units"], use["calls"]), (1, 1))

    def test_connections_unknown_filter_is_not_a_clean_empty_report(self):
        self.cli("ext", "connections", self.data, "Diagnostic.esm", "--schema", self.schema, "--plugin", "Missing.esp", success=False, error_contains="Selected plugin is not in the load order")

    def test_log_digest_real_cli_redaction_detail_diff_and_new_file_guard(self):
        log = self.root / "run.log"
        before = self.root / "previous.log"
        prefix = "2026-11-15T00:00:00.000000Z INFO nvf_play::mod_log: mods: "
        before.write_text(prefix + "load order, 1 plugin\n", encoding="utf8")
        log.write_text(prefix + "load order, 1 plugin\n" + prefix + "Private.esp: SCPT 1: skipped: private words\n"
                       + prefix + "at exit: Private.esp: 0 script errors, 0 failed calls, 47 skipped statements, 0 functions not built\n", encoding="utf8")
        doc = self.cli("ext", "mod-log", log, "--previous", before)
        self.assertNotIn("Private.esp", json.dumps(doc))
        self.assertEqual(doc["run"]["sessions"][0]["reported_exit_counts"]["skipped_statements"], 47)
        self.assertTrue(any(row["kind"] == "script_failure" for row in doc["changes"]["counts"]))
        detail = self.cli("ext", "mod-log", log, "--detail")
        self.assertIn("private words", json.dumps(detail))
        unchanged = log.read_bytes()
        self.cli("ext", "mod-log", log, "--summary", log, success=False, error_contains="Summary requires a new output file")
        self.assertEqual(log.read_bytes(), unchanged)
        self.cli("ext", "mod-log", log, "--previous", self.root / "missing.log", success=False, error_contains="Cannot open explicit mod-log input")

    def test_summary_recognizes_synthetic_install_without_a_data_argument(self):
        original = self.root / "synthetic-original/Data"
        original.mkdir(parents=True)
        # A literal sentinel exercises GameInstall's path guard, not plugin decoding.
        (original / "FalloutNV.esm").write_bytes(b"authored guard sentinel")
        log = self.root / "outside.log"
        log.write_text("2026-11-15T00:00:00.000000Z INFO test: no mod aid\n", encoding="utf8")
        summary = original / "new-summary.json"
        self.cli("ext", "mod-log", log, "--summary", summary, success=False, error_contains="inside the original game install")
        self.assertFalse(summary.exists())
        self.assertEqual(sorted(p.name for p in original.iterdir()), ["FalloutNV.esm"])

    def test_script_runner_actual_trace_refusal_and_print_survive_successful_rows(self):
        scripts = self.root / "mods/NVSE/Plugins/Scripts"
        scripts.mkdir(parents=True)
        # A known empty-reference failure continues with zero; a successful SrRun row
        # therefore cannot substitute for looking at the actual trace events.
        (scripts / "gr_diagnostic.txt").write_text('ref r\nPrintC "authored-print-witness"\nr.Disable\n', encoding="utf8")
        args = ("ext", "script-runner", self.data, "Diagnostic.esm", "--schema", self.schema,
                "--root", self.root / "mods", "--no-data-files", "--run")
        doc = self.cli(*args)
        self.assertEqual(doc["failed"], 0)
        self.assertTrue(doc["ran"]["restart"][0]["ok"])
        self.assertIn("diagnostics", doc, "successful rows alone hide the real call failure")
        self.assertNotIn("authored-print-witness", json.dumps(doc["diagnostics"]))
        counts = doc["diagnostics"]["counts"]
        self.assertGreaterEqual(counts["call_failed"], 1)
        self.assertGreaterEqual(counts["printed"], 1)
        detailed = self.cli(*args, "--detail")
        entries = detailed["diagnostics"]["entries"]
        self.assertTrue(any(r["kind"] == "call_failed" and "empty reference" in r["words"] for r in entries))
        self.assertTrue(any(r["kind"] == "printed" and "authored-print-witness" in r["words"] for r in entries))

    def test_existing_mo2_profile_expansion_is_preserved(self):
        instance = self.root / "instance"
        profile = instance / "profiles/owned"
        profile.mkdir(parents=True)
        (profile / "plugins.txt").write_text("# This file was automatically generated by Mod Organizer.\n*Diagnostic.esm\n", encoding="utf8")
        (profile / "loadorder.txt").write_text("Diagnostic.esm\n", encoding="utf8")
        (profile / "modlist.txt").write_text("", encoding="utf8")
        (instance / "ModOrganizer.ini").write_text("[General]\ngamePath=" + self.root.as_posix() + "\n", encoding="utf8")
        doc = self.cli("ext", "report", self.data, "--mo2-profile", profile, "--schema", self.schema)
        self.assertEqual(doc["format"], "fnv.ext.report/1")
        self.assertEqual(doc["summary"]["units_picked"], 1)

    def test_runner_summary_limit_preserves_actual_existing_success_rows(self):
        scripts = self.root / "mods/NVSE/Plugins/Scripts"
        scripts.mkdir(parents=True)
        # More distinct prints than the summary cap, but within the existing VM step
        # budget. Compiler/setup/runtime errors cannot satisfy the assertions below.
        (scripts / "gr_many.txt").write_text("".join(f'PrintC "owned-{i}"\n' for i in range(4097)), encoding="utf8")
        doc = self.cli("ext", "script-runner", self.data, "Diagnostic.esm", "--schema", self.schema,
                       "--root", self.root / "mods", "--no-data-files", "--run")
        self.assertEqual(doc["failed"], 0)
        self.assertEqual(doc["ran"]["restart"], [{"file": "gr_many.txt", "ok": True}])
        self.assertIn("diagnostics", doc)
        self.assertFalse(doc["diagnostics"]["available"])
        self.assertEqual(doc["diagnostics"]["reason"], "retention_limit")
        self.assertNotIn("counts", doc["diagnostics"])
        self.assertNotIn("entries", doc["diagnostics"])

    def test_existing_report_and_scan_positive_controls(self):
        report = self.cli("ext", "report", self.data, "Diagnostic.esm", "--schema", self.schema)
        self.assertEqual(report["format"], "fnv.ext.report/1")
        scan = self.cli("ext", "scan", self.data, "Diagnostic.esm", "--schema", self.schema)
        self.assertEqual(scan["format"], "fnv.ext.scan/1")
        self.assertEqual(scan["summary"]["units"], 1)


if __name__ == "__main__":
    unittest.main()
