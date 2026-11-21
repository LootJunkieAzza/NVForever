"""`python tools/stress ...` from the repo root (or `python -m stress ...` from tools/stress)."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from stress.cli import main  # noqa: E402

sys.exit(main())
