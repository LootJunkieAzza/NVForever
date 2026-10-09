#!/usr/bin/env python3
"""Independent reference implementation ("oracle") for the Layer 5 script decoder.

Written from docs/spec-notes/scripts.md (section 1 "Script units" and section 3
"Canonical text"), without looking at the Rust decoder. It never reads the compiled bytes
(SCDA): it turns each unit's shipped source text (SCTX) into the canonical text, using
the function catalogue (schemas/script/functions.json: names, aliases, observed
spellings, parameter counts) and the unit's own reference list (SCRO/SCRV) and variable
names (SLSD/SCVR). The parity lab compares that with `fnvtool script decompile`, which
gets its text from the compiled bytes. Standard library only; the container helpers
(tolerant zlib inflate, subrecord iterator with XXXX handling, Windows-1252 text) come
from oracle_esm.py, which must sit in the same folder.

usage: oracle_script.py decompile <data_dir> <plugin>... --functions functions.json
                        [-o out.json]
       -> fnv.script.decompile/1
Exit codes: 0 ok, 1 input unreadable, 2 usage error.

What it does, in brief:

  Load order    plugins in the order given; plugin i has load-order index i. A stored
                FormID's top byte indexes the plugin's MAST list; the value equal to the
                master count means "this plugin". The top byte is replaced by the
                load-order index of the file it designates.
  Records       every record of every plugin is read (compressed ones inflated); its EDID
                is remembered for its global FormID (the last definition wins).
  Units         a unit starts at each SCHR and takes the SCDA/SCTX/SLSD/SCVR/SCRO/SCRV
                subrecords that follow it. Its owner label comes from the subrecords
                before it (scripts.md section 1.3). Units with neither SCDA nor SCTX are
                not listed.
  Text          the source is split into lines, comments (';' outside quotes) and blank
                lines are dropped, variable declarations are dropped, lines without a
                letter or digit are dropped; every other line is one statement and is
                printed in the canonical form of scripts.md section 3.

Document: "format", "load_order", "count", "units" ([{unit, text}] in load order and
file order; "text" only for a unit with both compiled bytes and source, the units whose
source can be checked against the bytes), "summary" and "warnings" (not compared by
the lab).
"""
import argparse
import json
import os
import re
import struct
import sys
from decimal import Decimal

CAPTURE_DIR = os.path.dirname(os.path.abspath(__file__))
if CAPTURE_DIR not in sys.path:
    sys.path.insert(0, CAPTURE_DIR)

from oracle_esm import decode_cp1252, inflate_zlib, iter_subrecords, zstring  # noqa: E402

FORMAT = "fnv.script.decompile/1"
HEADER_SIZE = 24
FLAG_COMPRESSED = 0x00040000
_RECORD = struct.Struct("<4sIII")
_U32 = struct.Struct("<I")
_I16 = struct.Struct("<h")

UNIT_SUBS = {b"SCDA", b"SCTX", b"SLSD", b"SCVR", b"SCRO", b"SCRV"}
DECLARATIONS = {"short", "int", "long", "float", "ref", "reference"}
PLAYER = 0x00000014          # the player reference (scripts.md section 3.2)
PLAYER_NAMES = {"player", "playerref"}
# Begin-block spellings in shipped source that differ from the catalogue's block name.
BLOCK_ALIASES = {"onpackageend": "onpackagedone"}

# Binary operators by precedence (higher binds tighter), scripts.md section 3.3.
BINARY = {"&&": 1, "||": 2, "==": 3, "!=": 3, "<": 4, ">": 4, "<=": 4, ">=": 4,
          "+": 5, "-": 5, "*": 6, "/": 6, "%": 6}
HEX_FORMID_RE = re.compile(r"^[0-9A-Fa-f]{8}$")
TOKEN_RE = re.compile(r'"[^"]*"?|==|!=|<=|>=|&&|\|\||[<>+\-*/%()]|[A-Za-z0-9_.]+|\S')
NUMBER_RE = re.compile(r"^(\d+\.?\d*|\.\d+)$")
WORD_RE = re.compile(r"^[a-z0-9_]+$")


class InputError(Exception):
    pass


class Warnings:
    def __init__(self):
        self.items = []
        self.counts = {}

    def add(self, code, detail):
        n = self.counts.get(code, 0) + 1
        self.counts[code] = n
        if n <= 50:
            self.items.append({"code": code, "detail": detail})

    def finish(self):
        out = list(self.items)
        for code, n in sorted(self.counts.items()):
            if n > 50:
                out.append({"code": "warnings_suppressed", "detail": f"{code}: {n} total"})
        return out


# --------------------------------------------------------------------------- canonical atoms

def number(text):
    """Canonical number: integral values without a fraction, others as the shortest
    decimal that reads back to the same double, never with an exponent."""
    v = float(text)
    if v == int(v) and abs(v) < 1e15:
        return str(int(v))
    r = repr(v)
    if "e" in r or "E" in r:
        r = format(Decimal(r), "f")
    return r


def word(s):
    low = s.lower()
    return low if low and WORD_RE.match(low) else '"' + low + '"'


def fid(v):
    return f"0x{v:08X}"


# --------------------------------------------------------------------------- catalogue

class Catalogue:
    def __init__(self, path):
        with open(path, encoding="utf-8") as fh:
            doc = json.load(fh)
        self.by_name = {}
        for f in doc.get("functions", []):
            if not f.get("opcodes"):
                continue
            # Functions without a wiki page have no documented parameters, and a "..."
            # entry means an open-ended list: the count is unknown in both cases.
            params = f.get("params") or []
            variadic = any(p.get("name") == "..." for p in params)
            count = None if f.get("origin") == "observed-only" or variadic else len(params)
            # Actor-value parameters name an actor value, never a form (section 3.2).
            xedit = f.get("xedit_param_types") or []
            av = {i for i, p in enumerate(params) if p.get("kind") == "actor_value"}
            av |= {i for i, t in enumerate(xedit) if t == "ptActorValue"}
            entry = (f["name"].lower(), count, frozenset(av))
            for n in [f["name"]] + list(f.get("aliases") or []) + list(f.get("observed_spellings") or []):
                self.by_name.setdefault(n.lower(), entry)

    def get(self, name):
        return self.by_name.get(name.lower())


# --------------------------------------------------------------------------- plugins

class Plugin:
    def __init__(self, name, index, buf, remap):
        self.name, self.index, self.buf, self.remap = name, index, buf, remap

    def globalise(self, local):
        top = local >> 24
        if top >= len(self.remap) or self.remap[top] is None:
            return None
        return (self.remap[top] << 24) | (local & 0x00FFFFFF)


def record_data(buf, pos, end, flags, where, warnings):
    if not flags & FLAG_COMPRESSED:
        return buf, pos, end
    declared = _U32.unpack_from(buf, pos)[0]
    out, problems = inflate_zlib(buf[pos + 4:end], declared)
    for severity, code, detail in problems:
        warnings.add(code, f"{where}: {detail}")
        if severity == "error":
            out = None
    if out is None:
        return None, 0, 0
    return out, 0, len(out)


def subs(data, start, end, where, warnings):
    problems = []
    items = [(sig, bytes(data[off:off + size])) for sig, off, size in
             iter_subrecords(data, start, end, problems, where)]
    for _, code, detail in problems:
        warnings.add(code, detail)
    return items


def open_plugins(data_dir, names, warnings):
    lower = [n.lower() for n in names]
    out = []
    for i, name in enumerate(names):
        path = os.path.join(data_dir, name)
        try:
            with open(path, "rb") as fh:
                buf = fh.read()
        except OSError as exc:
            raise InputError(f"cannot read {path}: {exc}") from exc
        if buf[:4] != b"TES4":
            raise InputError(f"{name}: not a TES4 plugin")
        _, size, flags, _ = _RECORD.unpack_from(buf, 0)
        data, s, e = record_data(buf, HEADER_SIZE, HEADER_SIZE + size, flags, name, warnings)
        masters = [zstring(b) for sig, b in subs(data, s, e, name, warnings) if sig == b"MAST"]
        remap = []
        for m in masters:
            remap.append(lower.index(m.lower()) if m.lower() in lower else None)
        remap.append(i)
        out.append(Plugin(name, i, buf, remap))
    return out


def iter_records(plugin, warnings):
    """(sig, form_id, flags, data, start, end) for every record, in file order."""
    buf = plugin.buf
    pos = 0
    n = len(buf)
    # The TES4 record comes first; skip it.
    _, size, _, _ = _RECORD.unpack_from(buf, 0)
    pos = HEADER_SIZE + size
    while pos + HEADER_SIZE <= n:
        sig, size, flags, form_id = _RECORD.unpack_from(buf, pos)
        if sig == b"GRUP":
            pos += HEADER_SIZE      # descend: a group's records follow its header
            continue
        start, end = pos + HEADER_SIZE, pos + HEADER_SIZE + size
        pos = end
        where = f"{plugin.name} {sig.decode('latin-1')} 0x{form_id:08X}"
        data, s, e = record_data(buf, start, end, flags, where, warnings)
        if data is None:
            continue
        yield sig, form_id, flags, data, s, e


# --------------------------------------------------------------------------- units

def owner_label(sig, form, state):
    """Owner label suffix for the next unit of a record (scripts.md section 1.3)."""
    s = sig.decode("latin-1")
    head = f"{s} {fid(form)}"
    if sig == b"SCPT":
        return head
    if sig == b"QUST" and state["stage"] is not None:
        label = f"{head} stage {state['stage']} entry {state['stage_units']}"
        state["stage_units"] += 1
        return label
    if sig == b"INFO":
        return f"{head} {'end' if state['next'] else 'begin'}"
    if sig == b"PACK" and state["pack"] is not None:
        return f"{head} {state['pack']}"
    if sig == b"TERM" and state["items"] > 0:
        return f"{head} item {state['items'] - 1}"
    if sig == b"PERK" and state["entries"] > 0:
        return f"{head} entry {state['entries'] - 1}"
    if sig == b"REFR" and state["patrol"]:
        return f"{head} patrol"
    return f"{head} unit {state['ordinal']}"


def record_units(plugin, sig, form, items):
    """Units of one record: [{label, scda, sctx, vars: {index: name}, refs: [...]}]."""
    state = {"stage": None, "stage_units": 0, "next": False, "pack": None,
             "items": 0, "entries": 0, "patrol": False, "ordinal": 0}
    out = []
    cur = None
    for s, b in items:
        if cur is not None and s in UNIT_SUBS:
            if s == b"SCDA":
                cur["scda"] = True
            elif s == b"SCTX":
                cur["sctx"] = decode_cp1252(b)
            elif s == b"SLSD":
                cur["pending"] = _U32.unpack_from(b, 0)[0] if len(b) >= 4 else None
            elif s == b"SCVR":
                if cur.get("pending") is not None:
                    cur["vars"].setdefault(zstring(b).lower(), cur["pending"])
                cur["pending"] = None
            elif s == b"SCRO":
                local = _U32.unpack_from(b, 0)[0]
                cur["refs"].append(("form", plugin.globalise(local), local))
            elif s == b"SCRV":
                cur["refs"].append(("local", _U32.unpack_from(b, 0)[0], None))
            continue
        cur = None
        if sig == b"QUST" and s == b"INDX":
            state["stage"] = _I16.unpack_from(b, 0)[0]
            state["stage_units"] = 0
        elif sig == b"INFO" and s == b"NEXT":
            state["next"] = True
        elif s in (b"POBA", b"POEA", b"POCA"):
            state["pack"] = {b"POBA": "begin", b"POEA": "end", b"POCA": "change"}[s]
        elif sig == b"TERM" and s == b"ITXT":
            state["items"] += 1
        elif sig == b"PERK" and s == b"PRKE":
            state["entries"] += 1
        elif s == b"XPPA":
            state["patrol"] = True
        elif s == b"SCHR":
            cur = {"label": owner_label(sig, form, state), "scda": False, "sctx": None,
                   "vars": {}, "refs": []}
            state["ordinal"] += 1
            out.append(cur)
    return out


# --------------------------------------------------------------------------- source text

def strip_comment(line):
    quoted = False
    for i, ch in enumerate(line):
        if ch == '"':
            quoted = not quoted
        elif ch == ";" and not quoted:
            return line[:i]
    return line


def tokens(line):
    out = []
    for t in TOKEN_RE.findall(line):
        if t == ",":
            continue
        if out and out[-1].endswith(".") and not NUMBER_RE.match(out[-1]) and re.match(r"[A-Za-z_]", t):
            out[-1] += t        # "SomeREF. Disable": a space after the dot
            continue
        out.append(t)
    return out


class Unit:
    def __init__(self, info, edid, cat, warnings):
        self.info, self.edid, self.cat, self.warnings = info, edid, cat, warnings
        self.vars = info["vars"]                  # lower name -> index
        self.var_names = set(self.vars)
        # SCRO entries by lower EDID; SCRV entries are local variables (printed by name).
        self.by_edid = {}
        self.local_forms = {}                     # stored (file-local) FormID -> global
        for kind, value, local in info["refs"]:
            if kind == "form" and value is not None:
                self.local_forms[local] = value
                e = edid.get(value)
                if e:
                    self.by_edid.setdefault(e.lower(), value)
                if value == PLAYER:
                    for n in PLAYER_NAMES:
                        self.by_edid.setdefault(n, value)

    # -- names
    def ref(self, name):
        """A name used as a reference: a local variable, or a reference list form."""
        low = name.lower()
        if low in self.var_names:
            return low
        if low in self.by_edid:
            return fid(self.by_edid[low])
        if low in PLAYER_NAMES:
            return fid(PLAYER)
        self.warnings.add("unresolved_reference", f"{self.info['label']}: {name}")
        return word(name)

    def atom(self, tok, as_word=False):
        """A parameter or operand that is not a function call. `as_word`: the position
        takes an actor value, so a name there is never a form."""
        if tok.startswith('"'):
            text = tok.strip('"')
            if not as_word and text.lower() in self.by_edid:
                return fid(self.by_edid[text.lower()])   # a quoted Editor ID
            return word(text)
        if NUMBER_RE.match(tok):
            return number(tok)
        if as_word:
            return word(tok)
        if "." in tok:
            head, _, tail = tok.partition(".")
            return f"{self.ref(head)}.{tail.lower()}"
        low = tok.lower()
        if low in self.var_names:
            return low
        if low in self.by_edid:
            return fid(self.by_edid[low])
        if low in PLAYER_NAMES:
            return fid(PLAYER)
        if HEX_FORMID_RE.match(tok):
            # A FormID written in hex (file-local, like a stored FormID), when the
            # reference list holds it.
            g = self.local_forms.get(int(tok, 16))
            if g is not None:
                return fid(g)
        return word(tok)

    def args(self, toks, av=frozenset()):
        """Statement or Begin parameters: atoms; '-' before a number makes it negative;
        parentheses are dropped. `av`: positions that take an actor value."""
        out = []
        i = 0
        while i < len(toks):
            t = toks[i]
            if t == "-" and i + 1 < len(toks) and NUMBER_RE.match(toks[i + 1]):
                out.append(number("-" + toks[i + 1]) if float(toks[i + 1]) else "0")
                i += 2
                continue
            if t in ("(", ")") or not (t.startswith('"') or any(ch.isalnum() for ch in t)):
                i += 1                  # parentheses and stray punctuation (section 3.2)
                continue
            out.append(self.atom(t, len(out) in av))
            i += 1
        return out

    def function(self, tok):
        """(ref prefix or None, canonical name, parameter count, actor-value positions)
        if tok names a function."""
        ref = None
        name = tok
        if "." in tok:
            head, _, name = tok.partition(".")
            ref = head or None          # ".Disable": no reference (section 3.2)
        if ref is None and tok.lower() in self.var_names:
            return None
        f = self.cat.get(name)
        if f is None:
            return None
        return (self.ref(ref) if ref is not None else None,) + f

    # -- expressions
    def expr(self, toks):
        """Infix tokens -> canonical postfix text."""
        out, ops = [], []
        prev_operand = False
        i = 0
        while i < len(toks):
            t = toks[i]
            if t == "(":
                ops.append("(")
                prev_operand = False
            elif t == ")":
                while ops and ops[-1] != "(":
                    out.append(ops.pop())
                if ops:
                    ops.pop()
                prev_operand = True
            elif t in BINARY and not (t == "-" and not prev_operand):
                p = BINARY[t]
                while ops and ops[-1] != "(" and (ops[-1] == "~" or BINARY.get(ops[-1], 0) >= p):
                    out.append(ops.pop())
                ops.append(t)
                prev_operand = False
            elif t == "-":
                ops.append("~")
                prev_operand = False
            else:
                f = self.function(t)
                if f is not None:
                    ref, name, nparams, av = f
                    args = []
                    while (nparams is None or len(args) < nparams) and i + 1 < len(toks) \
                            and toks[i + 1] not in BINARY and toks[i + 1] not in ("(", ")"):
                        i += 1
                        args.append(self.atom(toks[i], len(args) in av))
                    head = f"{ref}.{name}" if ref else name
                    out.append(f"{head}({' '.join(args)})")
                else:
                    out.append(self.atom(t))
                prev_operand = True
            i += 1
        while ops:
            op = ops.pop()
            if op != "(":
                out.append(op)
        return " ".join(out)

    # -- statements
    def statement(self, toks):
        head = toks[0].lower()
        rest = toks[1:]
        if head in ("scn", "scriptname"):
            return "scriptname"
        if head == "begin":
            name = rest[0].lower() if rest else ""
            name = BLOCK_ALIASES.get(name, name)
            return " ".join(["begin", name] + self.args(rest[1:]))
        if head == "end":
            return "end"
        if head in ("if", "elseif"):
            e = self.expr(rest)
            return f"{head} {e}" if e else head
        if head == "else":
            return "else"
        if head == "endif":
            return "endif"
        if head == "return":
            return "return"
        if head == "set":
            low = [t.lower() for t in rest]
            k = low.index("to") if "to" in low else len(rest)
            target = rest[:k]
            tgt = self.atom(target[0]) if len(target) == 1 else " ".join(self.atom(t) for t in target)
            return f"set {tgt} to {self.expr(rest[k + 1:])}"
        f = self.function(toks[0])
        if f is None:
            self.warnings.add("unknown_statement", f"{self.info['label']}: {toks[0]}")
            ref, name, av = None, toks[0].lower(), frozenset()
        else:
            ref, name, nparams, av = f
        call = f"{ref}.{name}" if ref else name
        args = self.args(rest, av)
        if nparams is not None:
            args = args[:nparams]     # the compiler ignores surplus arguments (section 3.2)
        return " ".join([call] + args)

    def text(self):
        lines = []
        src = self.info["sctx"].rstrip("\0")
        for raw in src.replace("\r\n", "\n").replace("\r", "\n").split("\n"):
            line = strip_comment(raw).strip()
            if not line or not any(ch.isalnum() for ch in line):
                continue
            toks = tokens(line)
            if not toks or toks[0].lower() in DECLARATIONS:
                continue
            lines.append(self.statement(toks))
        return lines


# --------------------------------------------------------------------------- document

def decompile(data_dir, names, functions_path):
    warnings = Warnings()
    cat = Catalogue(functions_path)
    plugins = open_plugins(data_dir, names, warnings)
    edid = {}
    found = []
    for p in plugins:
        for sig, form_id, _flags, data, s, e in iter_records(p, warnings):
            where = f"{p.name} {sig.decode('latin-1')} 0x{form_id:08X}"
            items = subs(data, s, e, where, warnings)
            g = p.globalise(form_id)
            if g is None:
                warnings.add("unmappable_formid", where)
                g = form_id
            for sub, b in items:
                if sub == b"EDID":
                    edid[g] = zstring(b)
                    break
            if any(sub == b"SCHR" for sub, _ in items):
                for u in record_units(p, sig, g, items):
                    u["label"] = f"{p.name}|{u['label']}"
                    found.append(u)
    rows = []
    with_text = 0
    for u in found:
        if not u["scda"] and u["sctx"] is None:
            continue
        row = {"unit": u["label"]}
        if u["scda"] and u["sctx"] is not None:
            row["text"] = Unit(u, edid, cat, warnings).text()
            with_text += 1
        rows.append(row)
    return {
        "format": FORMAT,
        "load_order": list(names),
        "count": len(rows),
        "units": rows,
        "summary": {"units_found": len(found), "units_listed": len(rows), "with_text": with_text},
        "warnings": warnings.finish(),
    }


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    d = sub.add_parser("decompile")
    d.add_argument("data_dir")
    d.add_argument("plugins", nargs="+")
    d.add_argument("--functions", required=True)
    d.add_argument("-o", "--out")
    try:
        a = ap.parse_args(argv)
    except SystemExit as exc:
        return 2 if exc.code else 0
    try:
        doc = decompile(a.data_dir, a.plugins, a.functions)
    except (InputError, OSError, ValueError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    text = json.dumps(doc, indent=1)
    if a.out:
        with open(a.out, "w", encoding="utf-8") as fh:
            fh.write(text + "\n")
    else:
        sys.stdout.write(text + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
