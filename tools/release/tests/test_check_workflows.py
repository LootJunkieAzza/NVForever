"""Tests for tools/release/check_workflows.py (#1999): the repository's own workflows pass,
and each rule catches what it is for."""

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools" / "release"))

import check_workflows as cw  # noqa: E402

SHA = "d1527c8f67ef9af357290475d4af4b0fab8f12e0"

GOOD = f"""name: release
permissions:
  contents: read
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: Swatinem/rust-cache@{SHA} # v2
        with:
          workspaces: x
  publish:
    needs: build
    permissions:
      contents: write
    steps:
      - name: Release
        uses: softprops/action-gh-release@{SHA} # v3
"""


class CheckWorkflows(unittest.TestCase):
    def test_the_repositorys_workflows_pass(self):
        files = sorted((ROOT / ".github" / "workflows").glob("*.yml"))
        self.assertTrue(files)
        for f in files:
            self.assertEqual(cw.problems_in(f.name, f.read_text(encoding="utf-8")), [], f.name)
        self.assertEqual(cw.main(ROOT), 0)

    def test_a_good_release_workflow_passes(self):
        self.assertEqual(cw.problems_in("release.yml", GOOD), [])

    def test_write_at_the_top_is_refused(self):
        text = GOOD.replace("permissions:\n  contents: read\njobs:", "permissions:\n  contents: write\njobs:")
        p = cw.problems_in("release.yml", text)
        self.assertTrue(any("the workflow's permissions write contents" in x for x in p), p)
        self.assertTrue(any("do not give contents: read" in x for x in p), p)
        p = cw.problems_in("release.yml", GOOD.replace("permissions:\n  contents: read\n", "permissions: write-all\n", 1))
        self.assertTrue(any("write *" in x for x in p), p)

    def test_no_workflow_permissions_is_refused(self):
        p = cw.problems_in("release.yml", GOOD.replace("permissions:\n  contents: read\n", "", 1))
        self.assertEqual(p, ["release.yml: no workflow-level permissions (the token would get the repository's default)"])

    def test_write_in_another_job_or_workflow_is_refused(self):
        # The build job, or the publish job of a workflow not listed.
        text = GOOD.replace("    runs-on: ubuntu-latest\n    steps:", "    runs-on: ubuntu-latest\n    permissions:\n      contents: write\n    steps:", 1)
        self.assertEqual(cw.problems_in("release.yml", text), ["release.yml: job build writes contents, and is not listed in WRITERS"])
        self.assertEqual(cw.problems_in("ci.yml", GOOD), ["ci.yml: job publish writes contents, and is not listed in WRITERS"])

    def test_third_party_actions_need_a_sha_and_a_comment(self):
        p = cw.problems_in("release.yml", GOOD.replace(f"Swatinem/rust-cache@{SHA} # v2", "Swatinem/rust-cache@v2"))
        self.assertEqual(len(p), 1)
        self.assertIn("Swatinem/rust-cache is pinned to 'v2', not a commit SHA", p[0])
        p = cw.problems_in("release.yml", GOOD.replace(f"rust-cache@{SHA} # v2", "rust-cache@main"))
        self.assertIn("pinned to 'main'", p[0])
        p = cw.problems_in("release.yml", GOOD.replace(f"action-gh-release@{SHA} # v3", f"action-gh-release@{SHA}"))
        self.assertEqual(len(p), 1)
        self.assertIn("has no '# <tag>' comment", p[0])
        # GitHub's own actions and local ones may stay on a tag or path.
        local = GOOD.replace("actions/checkout@v7", "./.github/actions/local")
        self.assertEqual(cw.problems_in("release.yml", local), [])


if __name__ == "__main__":
    unittest.main()
