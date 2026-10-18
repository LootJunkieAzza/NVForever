"""Compare catalogue parameter counts with the argument indexes each Impl reads.

usage: python -I tools/script_param_audit.py <repo root> <decompile.json> [Name,Name,...]

<decompile.json> is `fnvtool script decompile --schema schemas/fnv <Data> FalloutNV.esm`
(game data, never committed). With names, also prints those functions' bodies. A row is a
candidate only: read the body (helpers that take `a` are false hits). Findings:
docs/design/script-function-params.md.
"""
import json, re, sys
from collections import defaultdict
from pathlib import Path

root = Path(sys.argv[1])
fdir = root / "crates/fnv-game/src/functions"
cat = json.load(open(root / "schemas/script/functions.json", encoding="utf-8"))["functions"]
params = {}
for f in cat:
    for op in f.get("opcodes", []):
        params[int(op, 16)] = f
reg = (fdir / "mod.rs").read_text(encoding="utf-8")
entries = re.findall(r'\((0x[0-9A-Fa-f]+),\s*"([^"]+)",\s*Impl\(([\w:]+)\)\)', reg)

src = {p.stem: p.read_text(encoding="utf-8") for p in fdir.glob("*.rs")}


def body(module, name):
    text = src.get(module)
    if text is None:
        return None
    m = re.search(r'\bfn ' + re.escape(name) + r'\b[^{]*\{', text)
    if not m:
        return None
    i, depth = m.end(), 1
    while depth and i < len(text):
        depth += {"{": 1, "}": -1}.get(text[i], 0)
        i += 1
    return text[m.start():i]


def body2(path, name):
    if not path.exists():
        return None
    src[str(path)] = path.read_text(encoding="utf-8")
    return body(str(path), name)


# shipped-script calls: name -> list of arg counts (canonical text: statements
# `ref.func a b`, expressions `func(a, b)`)
calls = defaultdict(list)
names = {n.lower() for _, n, _ in entries}
lines = [l for u in json.load(open(sys.argv[2], encoding="utf-8"))["units"] for l in u["text"]]
for line in lines:
    for m in re.finditer(r'(?:^|[\s(.,])([a-z0-9_]+)\(([^()]*)\)', line, re.I):
        if m.group(1).lower() in names:
            calls[m.group(1).lower()].append(len([x for x in m.group(2).split(",") if x.strip()]))
    m = re.match(r'(?:[\w]+\.)?([a-z0-9_]+)((?:\s+\S+)*)\s*$', line, re.I)
    if m and m.group(1).lower() in names and "(" not in line:
        calls[m.group(1).lower()].append(len(m.group(2).split()))

rows = []
for op, name, path in entries:
    op = int(op, 16)
    parts = path.split("::")
    if parts[0] == "crate":
        b = body2(root / "crates/fnv-game/src" / ("/".join(parts[1:-1]) + ".rs"), parts[-1])
        if b is None:
            b = body2(root / "crates/fnv-game/src" / "/".join(parts[1:-1]) / "mod.rs", parts[-1])
    elif len(parts) == 2:
        b = body(parts[0], parts[1])
    else:
        b = body("mod", parts[0])
    f = params.get(op)
    nparams = len(f.get("params", [])) if f else None
    if b is None:
        rows.append((name, nparams, None, "body not found", 0))
        continue
    idx = [int(x) for x in re.findall(r'\ba\.(?:form|opt_form|num|num_or|int|code|axis|str|get)\((\d+)', b)]
    idx += [int(x) - 1 for x in re.findall(r'a\.0\[\.\.(\d+)\]', b)]
    delegates = bool(re.search(r'[(,]\s*&?a\s*[,)]', b.split("{", 1)[1])) or "a.0" in b or "a.len()" in b or ".iter()" in b and "a." in b
    maxread = max(idx) + 1 if idx else 0
    over = [n for n in calls.get(name.lower(), []) if n > maxread]
    note = "delegates/len" if delegates else ""
    if nparams is not None and nparams > maxread:
        rows.append((name, nparams, maxread, note, len(over)))

rows.sort(key=lambda r: (-r[4], r[0]))
print(f"{'function':32} params read  shipped-calls-passing-more  note")
for name, np_, mr, note, over in rows:
    pn = ""
    f = next((x for x in cat if x["name"] == name), None)
    if f and mr is not None:
        pn = ", ".join(p["name"] + ("?" if p.get("optional") else "") for p in f.get("params", [])[mr:])
    print(f"{name:32} {np_!s:6} {mr!s:5} {over:5}  {note:14} unread: {pn}")

if len(sys.argv) > 3:
    want = set(sys.argv[3].split(","))
    for op, name, path in entries:
        if name in want:
            parts = path.split("::")
            if parts[0] == "crate":
                b = body2(root / "crates/fnv-game/src" / ("/".join(parts[1:-1]) + ".rs"), parts[-1]) or body2(root / "crates/fnv-game/src" / "/".join(parts[1:-1]) / "mod.rs", parts[-1])
            elif len(parts) == 2:
                b = body(parts[0], parts[1])
            else:
                b = body("mod", parts[0])
            print("=====", name, path)
            print(b)
