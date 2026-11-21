"""`python tools/smoke ...` from the repo root (or `python -m smoke ...` from tools/smoke)."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from smoke.cli import main  # noqa: E402

sys.exit(main())
