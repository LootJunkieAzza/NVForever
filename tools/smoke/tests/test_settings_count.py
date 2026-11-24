"""The settings page's registry count is written in two places that must agree (#3793):
`KNOWN` in engine/nvf-play/src/settings_layout.rs (an array whose length the compiler
checks against its entries) and the paragraph of docs/design/settings-page.md that names
the rows ("The table names every row the registry can hold …, N with …"). Pull requests
that add a row often bump one and not the other; this test catches that without building
the engine."""

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
LAYOUT = ROOT / "engine" / "nvf-play" / "src" / "settings_layout.rs"
PAGE = ROOT / "docs" / "design" / "settings-page.md"


def known_len(text: str) -> int:
    m = re.search(r"const KNOWN: \[\(&str, &str\); (\d+)\]", text)
    if not m:
        raise AssertionError("no `const KNOWN: [(&str, &str); N]` in settings_layout.rs")
    return int(m.group(1))


def page_count(text: str) -> int:
    m = re.search(
        r"The table names every row the registry can hold[^\n]*\n(\d+) with ", text
    )
    if not m:
        raise AssertionError(
            "no registry count paragraph ('The table names every row the registry can "
            "hold …,' then 'N with') in settings-page.md"
        )
    return int(m.group(1))


class SettingsCount(unittest.TestCase):
    def test_page_count_matches_known(self):
        known = known_len(LAYOUT.read_text(encoding="utf-8"))
        page = page_count(PAGE.read_text(encoding="utf-8"))
        self.assertEqual(
            page,
            known,
            f"docs/design/settings-page.md says {page} rows but KNOWN in "
            f"engine/nvf-play/src/settings_layout.rs has {known}: update the count "
            "paragraph and name the new rows",
        )

    def test_parsers(self):
        self.assertEqual(known_len("    const KNOWN: [(&str, &str); 7] = ["), 7)
        self.assertEqual(
            page_count("The table names every row the registry can hold [x],\n9 with a"),
            9,
        )
        with self.assertRaises(AssertionError):
            page_count("no paragraph here")


if __name__ == "__main__":
    unittest.main()
