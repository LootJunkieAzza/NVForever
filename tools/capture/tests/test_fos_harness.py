"""Bridge so `unittest discover -s tools/capture/tests` runs the offline `.fos` synthetic suite."""

from __future__ import annotations

import importlib.util
from pathlib import Path

_MOD_PATH = Path(__file__).resolve().parent.parent / "fos" / "tests" / "test_parser_synthetic.py"
_SPEC = importlib.util.spec_from_file_location("_fos_harness_tests_impl", _MOD_PATH)
assert _SPEC is not None and _SPEC.loader is not None
_MOD = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(_MOD)

TestCampaignsSchemaAndHygiene = _MOD.TestCampaignsSchemaAndHygiene
TestCursorAndPrimitives = _MOD.TestCursorAndPrimitives
TestGlobalsAndChangedForms = _MOD.TestGlobalsAndChangedForms
TestHarnessReadOnlyStagingAndCollect = _MOD.TestHarnessReadOnlyStagingAndCollect
