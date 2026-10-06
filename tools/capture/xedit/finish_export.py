"""Turn FNVEdit exports (*.xedit.inventory.json) into UTF-8 JSON goldens for the lab.

FNVEdit's script interpreter cannot escape non-ASCII characters itself, and its
TStringList.SaveToFile writes the system ANSI code page (Windows-1252 on Western
systems) [hypothesis: Delphi's default encoding; the converter also accepts UTF-8 and
UTF-16 with a BOM]. This rewrites each file in place as ASCII-only JSON (non-ASCII as
\\uXXXX), which is valid UTF-8. Running it twice is harmless.

    python tools/capture/xedit/finish_export.py C:\\fnv-private\\goldens\\esm

Python 3.11+, standard library only.
"""
from __future__ import annotations

import codecs
import json
import sys
from pathlib import Path

SCHEMA = "fnv.esm.inventory/1"


def decode(raw: bytes) -> str:
    if raw.startswith(b"\xef\xbb\xbf"):
        return raw[3:].decode("utf-8")
    if raw.startswith((b"\xff\xfe", b"\xfe\xff")):
        return raw.decode("utf-16")
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        pass
    # Windows-1252; its five undefined bytes map to the C1 control of the same value
    # (as fnv-util's decoder does), so nothing is lost.
    return raw.decode("cp1252", errors="fnv-c1")


def _c1(err: UnicodeDecodeError) -> tuple[str, int]:
    return "".join(chr(b) for b in err.object[err.start:err.end]), err.end


codecs.register_error("fnv-c1", _c1)


def finish(path: Path) -> int:
    doc = json.loads(decode(path.read_bytes()), strict=False)  # strict=False: raw control chars
    if doc.get("schema") != SCHEMA:
        raise ValueError(f"{path}: not a {SCHEMA} document")
    records = doc.get("records")
    if not isinstance(records, list):
        raise ValueError(f"{path}: no records array")
    text = json.dumps(doc, ensure_ascii=True, indent=None, separators=(",", ": "))
    path.write_text(text + "\n", encoding="utf-8")
    return len(records)


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print(__doc__, file=sys.stderr)
        return 2
    # Windows passes "C:\dir\" as C:\dir" (the \" escapes the quote); drop the stray quote.
    folder = Path(argv[0].strip().rstrip('"'))
    files = sorted(folder.glob("*.xedit.inventory.json"))
    if not files:
        print(f"no *.xedit.inventory.json files in {folder}", file=sys.stderr)
        return 1
    failed = 0
    for f in files:
        try:
            print(f"{f.name}: {finish(f)} records")
        except (ValueError, UnicodeDecodeError) as e:
            failed += 1
            print(f"{f.name}: ERROR {e}", file=sys.stderr)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
