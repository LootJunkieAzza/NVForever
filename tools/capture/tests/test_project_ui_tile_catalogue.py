"""Regeneration must preserve the catalogue consumed by the real Game/VM control.

The CLI reads an authored empty wiki and an owned empty Data directory. No native
captures, game data or private wiki export is needed for this pageless command.
"""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[3]


class ProjectUITileCatalogue(unittest.TestCase):
    def test_fresh_generator_row_matches_the_real_vm_catalogue(self):
        catalogue = json.loads((ROOT / "schemas/script/functions.json").read_text(encoding="utf8"))
        current = [row for row in catalogue["functions"] if row["name"] == "ProjectUITile"]
        self.assertEqual(len(current), 1)
        with tempfile.TemporaryDirectory(prefix="nvf-project-ui-catalogue-") as directory:
            owned = Path(directory)
            wiki = owned / "authored-empty-wiki.xml"
            wiki.write_text('<mediawiki xmlns="http://www.mediawiki.org/xml/export-0.10/"/>', encoding="utf8")
            data = owned / "owned-empty-data"
            data.mkdir()
            output = owned / "functions.json"
            result = subprocess.run(
                [sys.executable, "-B", str(ROOT / "tools/capture/script_functions.py"),
                 "--wiki", str(wiki), "--data", str(data),
                 "--notes", str(ROOT / "schemas/script/notes.json"),
                 "--out", str(output), "--out-md", str(owned / "catalogue.md")],
                cwd=ROOT, capture_output=True, text=True, encoding="utf8", timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            generated = json.loads(output.read_text(encoding="utf8"))
        rows = [row for row in generated["functions"] if row["name"] == "ProjectUITile"]
        # This entire row is consumed by FunctionTable and source -> Game/VM in
        # corrected_projection_catalogue_metadata_drives_the_real_source_vm.
        self.assertEqual(rows, current)
        row = rows[0]
        self.assertEqual([(p["name"], p["kind"]) for p in row["params"]],
                         [("tilePath", "string"), ("block", "string"),
                          ("x", "float"), ("y", "float"),
                          ("width", "float"), ("height", "float")])
        self.assertEqual(row["calling_reference"], "reference")
        self.assertEqual(row["returns"], {"type": "int", "name": None})
        notes = json.loads((ROOT / "schemas/script/notes.json").read_text(encoding="utf8"))
        note, = [n for n in notes["functions"] if n["page"] == "ProjectUITile"]
        self.assertEqual(row["summary"], note["summary"])


if __name__ == "__main__":
    unittest.main()
