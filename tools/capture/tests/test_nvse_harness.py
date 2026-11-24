"""Bridge so `unittest discover -s tools/capture/tests` runs the offline NVSE probe harness suite."""

from __future__ import annotations

import importlib.util
from pathlib import Path

_MOD_PATH = Path(__file__).resolve().parent.parent / "nvse" / "tests" / "test_nvse_harness.py"
_SPEC = importlib.util.spec_from_file_location("_nvse_harness_tests_impl", _MOD_PATH)
assert _SPEC is not None and _SPEC.loader is not None
_MOD = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(_MOD)

TestCatalogValidation = _MOD.TestCatalogValidation
TestCitationAndPrivacyGate = _MOD.TestCitationAndPrivacyGate
TestEmitAndReadOnlyBoundary = _MOD.TestEmitAndReadOnlyBoundary
TestPersistenceClassifierAndVerify = _MOD.TestPersistenceClassifierAndVerify
