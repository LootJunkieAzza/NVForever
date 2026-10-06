"""Static checks for the FNVEdit script (it cannot be executed outside FNVEdit)."""
import os
import re
import unittest

SCRIPT = os.path.join(os.path.dirname(__file__), "..", "xedit", "export_inventory.pas")


def strip_strings(line: str) -> str:
    return re.sub(r"'(?:[^']|'')*'", "''", line)


class XEditScriptTests(unittest.TestCase):
    def setUp(self):
        with open(SCRIPT, encoding="utf-8") as f:
            self.lines = f.read().splitlines()

    def test_starts_with_unit_after_line_comments(self):
        code = [l for l in self.lines if l.strip() and not l.strip().startswith("//")]
        self.assertTrue(code[0].strip().lower().startswith("unit "), code[0])

    def test_no_brace_or_paren_star_comments(self):
        # A '}' inside brace-comment text ends the comment early (the original bug:
        # "'unit' expected but ',' found"), so brace comments are banned entirely.
        for i, l in enumerate(self.lines, 1):
            code = strip_strings(l.split("//", 1)[0])
            self.assertNotIn("{", code, f"line {i}: {l}")
            self.assertNotIn("}", code, f"line {i}: {l}")
            self.assertNotIn("(*", code, f"line {i}: {l}")

    def test_balanced_begin_end(self):
        depth = 0
        for l in self.lines:
            code = strip_strings(l.split("//", 1)[0]).lower()
            depth += len(re.findall(r"\b(begin|try|case)\b", code))
            depth -= len(re.findall(r"\bend\b", code))
        # the final "end." closes the unit, which has no matching begin
        self.assertEqual(depth, -1)

    def test_output_name_matches_lab_scenarios(self):
        text = "\n".join(self.lines)
        self.assertIn("ChangeFileExt(fileName, '') + '.xedit.inventory.json'", text)


if __name__ == "__main__":
    unittest.main()
