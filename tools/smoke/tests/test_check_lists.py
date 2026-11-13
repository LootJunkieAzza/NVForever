"""The push checks are listed in five places that must agree (#2001): CI's steps
(.github/workflows/ci.yml), the `[[check]]` entries of tools/smoke/scenarios.toml (what
`python tools/smoke run --checks` runs), and the command blocks of CLAUDE.md,
CONTRIBUTING.md and docs/development.md. This test reads each as a list of commands
(cargo test, clippy and deny, and the Python suites), written the same way —
`(cd engine && cargo test --release --locked)` for a command run in engine/ — and checks
that the docs and CI give the same commands in the same order, and the `[[check]]` entries
the same set."""

import re
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools" / "smoke"))

from smoke import scenarios  # noqa: E402

DOCS = ["CLAUDE.md", "CONTRIBUTING.md", "docs/development.md"]
# A check command: cargo test / clippy / deny, or a Python suite.
CHECK = re.compile(r"^(cargo (test|clippy|deny)\b|python -m unittest discover\b)")


def norm(cmd: str, cwd: str | None = None) -> str:
    cmd = " ".join(cmd.split())
    return f"(cd {cwd} && {cmd})" if cwd else cmd


def from_doc(text: str) -> list[str]:
    """The check commands in a document's fenced code blocks, in order."""
    out, fenced = [], False
    for raw in text.splitlines():
        line = raw.strip()
        if line.startswith("```"):
            fenced = not fenced
            continue
        if not fenced:
            continue
        line = re.sub(r"\s+#.*$", "", line)
        m = re.match(r"^\(cd (\S+) && (.*)\)$", line)
        cmd, cwd = (m.group(2), m.group(1)) if m else (line, None)
        if CHECK.match(cmd):
            out.append(norm(cmd, cwd))
    return out


def from_ci(text: str) -> list[str]:
    """The check commands of ci.yml's steps, in order: `run:` lines (with their
    `working-directory`, the Python suites' `-v` dropped) and the cargo-deny action's
    `command` (with its `manifest-path`)."""
    steps: list[dict[str, str]] = []
    for raw in text.splitlines():
        line = raw.strip()
        if raw.lstrip().startswith("- ") and re.match(r"^ {6}- ", raw):
            steps.append({})
            line = line[2:]
        if not steps:
            continue
        m = re.match(r"^(uses|run|working-directory|command|manifest-path):\s*(.*)$", line)
        if m:
            steps[-1][m.group(1)] = m.group(2)
    out = []
    for s in steps:
        if "cargo-deny-action" in s.get("uses", ""):
            mp = s.get("manifest-path")
            out.append(norm(f"cargo deny {'--manifest-path ' + mp + ' ' if mp else ''}{s.get('command', 'check')}"))
        elif CHECK.match(s.get("run", "")):
            cmd = re.sub(r"\s-v$", "", s["run"])
            out.append(norm(cmd, s.get("working-directory")))
    return out


class CheckLists(unittest.TestCase):
    def setUp(self):
        self.ci = from_ci((ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8"))

    def test_ci_has_the_checks(self):
        self.assertIn("(cd engine && cargo test --release --locked)", self.ci)
        self.assertIn("cargo deny --manifest-path engine/Cargo.toml check licenses advisories bans sources", self.ci)
        self.assertEqual(len(self.ci), len(set(self.ci)), self.ci)

    def test_the_docs_list_what_ci_runs_in_its_order(self):
        for doc in DOCS:
            with self.subTest(doc=doc):
                self.assertEqual(from_doc((ROOT / doc).read_text(encoding="utf-8")), self.ci)

    def test_the_push_checks_run_what_ci_runs(self):
        suite = scenarios.load(ROOT / "tools" / "smoke" / "scenarios.toml")
        checks = sorted(norm(" ".join(c.cmd), None if c.cwd in ("", ".", None) else c.cwd) for c in suite.checks)
        self.assertEqual(checks, sorted(self.ci))

    def test_the_readers(self):
        doc = "text\n```bash\ncargo test --release --locked   # a note\n(cd engine && cargo clippy -- -D warnings)\ncargo build --release\npython tools/smoke run\n```\n`cargo test` outside a block\n"
        self.assertEqual(from_doc(doc), ["cargo test --release --locked", "(cd engine && cargo clippy -- -D warnings)"])
        ci = (
            "jobs:\n  ci:\n    steps:\n"
            "      - uses: actions/checkout@v7\n"
            "      - name: Deny\n        uses: EmbarkStudios/cargo-deny-action@abc # v2\n        with:\n          manifest-path: engine/Cargo.toml\n          command: check licenses\n"
            "      - name: Lab\n        run: python -m unittest discover -s parity-lab/tests -v\n"
            "      - name: Engine tests\n        run: cargo test --release --locked\n        working-directory: engine\n"
            "      - name: Other\n        run: sudo apt-get install x\n"
        )
        self.assertEqual(from_ci(ci), [
            "cargo deny --manifest-path engine/Cargo.toml check licenses",
            "python -m unittest discover -s parity-lab/tests",
            "(cd engine && cargo test --release --locked)",
        ])


if __name__ == "__main__":
    unittest.main()
