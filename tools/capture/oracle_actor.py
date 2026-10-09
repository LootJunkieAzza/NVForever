#!/usr/bin/env python3
"""Independent reference implementation ("oracle") of the actor appearance document.

Written only from docs/spec-notes/actors.md (§1 data, §2 resolution rules, §5 "CLI
contract"), docs/spec-notes/simulation.md (§1 FormIDs and load order, §4 leveled lists),
docs/spec-notes/esm.md, docs/spec-notes/world.md §1 and the record layouts in
schemas/fnv/*.json, deliberately without looking at the Rust implementation, so the
parity lab can compare two independent implementations. Standard library only. It
imports the container helpers (tolerant zlib inflate, XXXX subrecords, cp1252 zstrings)
from oracle_esm.py and the load-order index (FormID mapping, winning definitions, cell
group membership) from oracle_world.py, which must sit in the same folder.

usage: oracle_actor.py appearance <data_dir> <plugin>... [--all | --ref <formid>]
                                  [--deterministic] [--schema DIR] [-o out.json]
       -> fnv.actor.appearance/1
`--schema` is accepted for command-line compatibility with fnvtool and ignored; without
`--ref` every ACHR of the load order is listed (`--all` is the default).
Exit codes: 0 ok, 1 input unreadable, 2 usage error.

What it does, in brief:

  Actors        every winning ACHR inside a cell-children group (world.md §1), ordered
                by global FormID; a winner with the Deleted flag is skipped (counted as
                `deleted`) [documented: actors.md §5].
  Base          ACHR NAME. An LVLN (as base or as a template) is picked once per actor:
                entries with level <= 1, the pool is all of them with "Calculate from
                all levels", else those of the highest eligible level; chance none is not
                applied; nested LVLN are followed (actors.md §2.1). A pool with more than
                one distinct entry makes the actor `random`; the pick itself then uses
                Python's random.Random seeded with the reference FormID (not the
                candidate's stream: only deterministic actors are comparable).
  Templates     per group (Traits bit 0, Model/Animation bit 6, Inventory bit 8 of the
                ACBS template flags) the first NPC_ along the TPLT chain whose flag is
                clear, starting at the actor base (§2.2).
  Skeleton      Model source MODL, else (absent, or a file name starting with
                "marker") the Traits source's, else meshes\\characters\\_male\\skeleton.nif.
  Armour        carried items of the Inventory source resolved at level 1 (simulation.md
                §4); ARMO candidates once each in order of first appearance, sorted by DT
                desc, DR (stored i16) desc, first appearance; worn unless no biped slot or
                a slot already worn (§2.3). Leveled lists that cannot yield armour are
                not resolved (they cannot change the output).
  random        (a) above, or (b) an Inventory-source entry with count > 0 naming an
                armour-random LVLI (§2.1, implemented as written).
  Parts         body (race, indices 0-2, slot not covered), head (race, when Head is not
                covered), head parts (PNAM, HDPT HNAM extras depth first), hair (Model
                source HAIR else race DNAM default; when Head is not covered and Hat is
                covered or Hair is not), worn armour (MOD3 for a female actor when
                present, else MODL) (§2.4).
  Paths         trimmed, '/' -> '\\', ASCII lower case, leading '\\' removed, prefixed
                with meshes\\ (models, .egt) or textures\\ (textures) unless already there.

Document: {"schema", "summary", "issues", "actors", "load_order"} as actors.md §5. Each
actor: reference, base, scale, npc, edid, female, race, skeleton, race_height, parts,
worn, hat, hair_color?, facegen?, race_facegen?, body_egt?, random; or {reference, base,
error}. f32 values are widened to Python floats, unrounded.

Readings of points the spec leaves open are marked SPEC-AMBIGUITY below.
"""
import argparse
import json
import os
import random as _random
import struct
import sys

CAPTURE_DIR = os.path.dirname(os.path.abspath(__file__))
if CAPTURE_DIR not in sys.path:
    sys.path.insert(0, CAPTURE_DIR)

import oracle_esm  # noqa: E402
import oracle_world  # noqa: E402

APPEARANCE_SCHEMA = "fnv.actor.appearance/1"

FLAG_DELETED = 0x20

# ACBS (schemas/fnv/gameplay.json NPC_): flags u32 at +0 (bit 0 Female), template flags
# u16 at +22 [documented: xEdit via the schema].
ACBS_FEMALE = 0x1
TPL_TRAITS = 1 << 0
TPL_MODEL = 1 << 6
TPL_INVENTORY = 1 << 8

# ARMO BMDT biped flags [documented: xEdit via the schema].
SLOT_HEAD = 1 << 0
SLOT_HAIR = 1 << 1
SLOT_UPPER_BODY = 1 << 2
SLOT_LEFT_HAND = 1 << 3
SLOT_RIGHT_HAND = 1 << 4
SLOT_HAT = 1 << 10

# Leveled list flags (LVLF) [documented: simulation.md §4].
LVL_ALL_LEVELS = 0x1
LVL_EACH = 0x2
LVL_USE_ALL = 0x4

DISPLAY_LEVEL = 1
MAX_DEPTH = 64

BODY_ROLES = {0: ("body:upper_body", SLOT_UPPER_BODY), 1: ("body:left_hand", SLOT_LEFT_HAND),
              2: ("body:right_hand", SLOT_RIGHT_HAND)}
HEAD_NAMES = ("head", "ears", "mouth", "teeth_lower", "teeth_upper", "tongue", "left_eye", "right_eye")
DEFAULT_SKELETON = "meshes\\characters\\_male\\skeleton.nif"

_U32 = struct.Struct("<I")
_I16 = struct.Struct("<h")
_F32 = struct.Struct("<f")

_WHITESPACE = "\t\n\x0b\x0c\r "
_ASCII_LOWER = str.maketrans("ABCDEFGHIJKLMNOPQRSTUVWXYZ", "abcdefghijklmnopqrstuvwxyz")


def fid_text(value: int) -> str:
    return f"0x{value:08X}"


def archive_path(raw, prefix: str):
    """actors.md §2: trimmed, '/' -> '\\', lower case, leading '\\' removed, prefixed
    unless already starting with the prefix. None when absent or empty."""
    if raw is None:
        return None
    p = oracle_esm.zstring(raw).strip(_WHITESPACE)
    if not p:
        return None
    # SPEC-AMBIGUITY: "lower case" read as ASCII lower case; "leading \ removed" as the
    # whole run of leading backslashes.
    p = p.translate(_ASCII_LOWER).replace("/", "\\").lstrip("\\")
    if not p.startswith(prefix):
        p = prefix + p
    return p


def floats(raw):
    n = len(raw) // 4
    return list(struct.unpack_from(f"<{n}f", raw, 0)) if n else []


class ActorError(Exception):
    """An actor that cannot be resolved: kind (summary key) and text."""

    def __init__(self, kind: str, text: str):
        super().__init__(text)
        self.kind = kind
        self.text = text


# --------------------------------------------------------------------------- forms

class Forms:
    """Winning, non-deleted records of the load order, parsed on demand and cached."""

    def __init__(self, index: oracle_world.WorldIndex, diag: oracle_world.Diagnostics):
        self.index = index
        self.diag = diag
        self.cache = {}

    def sig(self, gid):
        """Record type of the winning definition, or None if undefined or deleted."""
        if gid is None:
            return None
        w = self.index.winners.get(gid)
        if w is None:
            return None
        pidx, pos, sig, _ = w
        flags = oracle_world._RECORD_HEADER.unpack_from(self.index.plugins[pidx].buf, pos)[2]
        if flags & FLAG_DELETED:
            return None
        return sig

    def subrecords(self, gid):
        """(plugin, [(sig, bytes)]) of the winning record in record order."""
        pidx, pos, _, _ = self.index.winners[gid]
        plugin = self.index.plugins[pidx]
        data, start, end, _, sig, fid = oracle_world.record_data(plugin, pos, self.diag)
        out = []
        if data is not None:
            problems = []
            where = f"{plugin.name} {oracle_esm.signature_text(sig)} 0x{fid:08X}"
            for s, off, size in oracle_esm.iter_subrecords(data, start, end, problems, where):
                out.append((s, bytes(data[off:off + size])))
            for _, code, detail in problems:
                self.diag.add(code, detail)
        return plugin, out

    def get(self, gid, want):
        """Parsed record of type `want` (bytes signature) or None."""
        if self.sig(gid) != want:
            return None
        key = (gid, want)
        hit = self.cache.get(key)
        if hit is None:
            plugin, subs = self.subrecords(gid)
            parser = PARSERS[want]
            hit = parser(self, gid, plugin, subs)
            self.cache[key] = hit
        return hit

    def map(self, plugin, raw, owner, what):
        """FormID field (4 bytes) -> global FormID; None for 0 or unmappable."""
        if raw is None or len(raw) < 4:
            return None
        stored = _U32.unpack_from(raw, 0)[0]
        if stored == 0:
            return None
        gid = plugin.map_fid(stored)
        if gid is None:
            self.diag.add("unmappable_field", f"{plugin.name} {fid_text(owner)} {what} 0x{stored:08X}")
        return gid


def _first(subs):
    out = {}
    for s, raw in subs:
        out.setdefault(s, raw)
    return out


def parse_npc(forms, gid, plugin, subs):
    first = _first(subs)
    acbs = first.get(b"ACBS")
    flags = _U32.unpack_from(acbs, 0)[0] if acbs is not None and len(acbs) >= 4 else 0
    tflags = struct.unpack_from("<H", acbs, 22)[0] if acbs is not None and len(acbs) >= 24 else 0
    hclr = first.get(b"HCLR")
    items = []
    pnams = []
    for s, raw in subs:
        if s == b"CNTO" and len(raw) >= 8:
            item = forms.map(plugin, raw[:4], gid, "CNTO")
            if item is not None:
                items.append((item, struct.unpack_from("<i", raw, 4)[0]))
        elif s == b"PNAM":
            part = forms.map(plugin, raw, gid, "PNAM")
            if part is not None:
                pnams.append(part)
    facegen = None
    if b"FGGS" in first and b"FGGA" in first and b"FGTS" in first:
        # SPEC-AMBIGUITY: `facegen` is written when all three arrays are present.
        facegen = {"fggs": floats(first[b"FGGS"]), "fgga": floats(first[b"FGGA"]),
                   "fgts": floats(first[b"FGTS"])}
    edid = first.get(b"EDID")
    return {
        "edid": None if edid is None else oracle_esm.zstring(edid),
        "female": bool(flags & ACBS_FEMALE),
        "tflags": tflags,
        "tplt": forms.map(plugin, first.get(b"TPLT"), gid, "TPLT"),
        "race": forms.map(plugin, first.get(b"RNAM"), gid, "RNAM"),
        "hair": forms.map(plugin, first.get(b"HNAM"), gid, "HNAM"),
        "eyes": forms.map(plugin, first.get(b"ENAM"), gid, "ENAM"),
        "modl_raw": first.get(b"MODL"),
        "pnams": pnams,
        "hair_color": list(hclr[:3]) if hclr is not None and len(hclr) >= 3 else None,
        "facegen": facegen,
        "items": items,
    }


def parse_race(forms, gid, plugin, subs):
    """Positional RACE sections (actors.md §1.1): NAM0 head data, NAM1 body data, each
    with MNAM / FNAM lists of INDX + MODL/ICON parts; after HNAM / ENAM, MNAM / FNAM
    followed by that sex's FaceGen arrays."""
    race = {"data": None, "dnam": (None, None),
            "head": ([], []), "body": ([], []), "facegen": ({}, {})}
    section = None
    sex = None
    part = None
    for s, raw in subs:
        if s == b"DATA" and race["data"] is None:
            race["data"] = raw
        elif s == b"DNAM" and section is None:
            race["dnam"] = (forms.map(plugin, raw[0:4], gid, "DNAM male"),
                            forms.map(plugin, raw[4:8], gid, "DNAM female"))
        elif s == b"NAM0":
            section, sex, part = "head", None, None
        elif s == b"NAM1":
            section, sex, part = "body", None, None
        elif s in (b"HNAM", b"ENAM"):
            section, sex, part = "facegen", None, None
        elif s == b"MNAM":
            sex, part = 0, None
        elif s == b"FNAM":
            sex, part = 1, None
        elif s == b"INDX" and section in ("head", "body") and sex is not None:
            index = _U32.unpack_from(raw, 0)[0] if len(raw) >= 4 else None
            part = {"index": index, "modl": None, "icon": None}
            race[section][sex].append(part)
        elif s == b"MODL" and part is not None:
            if part["modl"] is None:
                part["modl"] = raw
        elif s == b"ICON" and part is not None:
            if part["icon"] is None:
                part["icon"] = raw
        elif s in (b"FGGS", b"FGGA", b"FGTS") and sex is not None and section not in ("head", "body"):
            race["facegen"][sex].setdefault(s, raw)
    return race


def parse_simple(forms, gid, plugin, subs):
    first = _first(subs)
    edid = first.get(b"EDID")
    return {"edid": None if edid is None else oracle_esm.zstring(edid),
            "modl": first.get(b"MODL"), "mod3": first.get(b"MOD3"), "icon": first.get(b"ICON")}


def parse_hdpt(forms, gid, plugin, subs):
    out = parse_simple(forms, gid, plugin, subs)
    out["extra"] = [p for p in (forms.map(plugin, raw, gid, "HNAM") for s, raw in subs if s == b"HNAM")
                    if p is not None]
    return out


def parse_armo(forms, gid, plugin, subs):
    out = parse_simple(forms, gid, plugin, subs)
    first = _first(subs)
    bmdt = first.get(b"BMDT")
    out["biped"] = _U32.unpack_from(bmdt, 0)[0] if bmdt is not None and len(bmdt) >= 4 else 0
    dnam = first.get(b"DNAM")
    # SPEC-AMBIGUITY: a missing DNAM (or a 4-byte DNAM without DT) sorts as DR 0 / DT 0.
    out["dr"] = _I16.unpack_from(dnam, 0)[0] if dnam is not None and len(dnam) >= 2 else 0
    out["dt"] = _F32.unpack_from(dnam, 4)[0] if dnam is not None and len(dnam) >= 8 else 0.0
    return out


def parse_leveled(forms, gid, plugin, subs):
    first = _first(subs)
    lvld = first.get(b"LVLD")
    lvlf = first.get(b"LVLF")
    entries = []
    for s, raw in subs:
        if s == b"LVLO" and len(raw) >= 8:
            level = struct.unpack_from("<H", raw, 0)[0]
            ref = forms.map(plugin, raw[4:8], gid, "LVLO")
            # The count is optional in the layout [documented: schema]; absent -> 1.
            count = struct.unpack_from("<H", raw, 8)[0] if len(raw) >= 10 else 1
            if ref is not None:
                entries.append((level, ref, count))
    return {"chance_none": lvld[0] if lvld else 0, "flags": lvlf[0] if lvlf else 0,
            "global": forms.map(plugin, first.get(b"LVLG"), gid, "LVLG"), "entries": entries}


def parse_glob(forms, gid, plugin, subs):
    first = _first(subs)
    fltv = first.get(b"FLTV")
    return {"value": _F32.unpack_from(fltv, 0)[0] if fltv is not None and len(fltv) >= 4 else 0.0}


PARSERS = {b"NPC_": parse_npc, b"RACE": parse_race, b"HAIR": parse_simple, b"EYES": parse_simple,
           b"HDPT": parse_hdpt, b"ARMO": parse_armo, b"LVLI": parse_leveled, b"LVLN": parse_leveled,
           b"GLOB": parse_glob}


# --------------------------------------------------------------------------- leveled lists

def eligible(lst):
    return [e for e in lst["entries"] if e[0] <= DISPLAY_LEVEL]


def pool(lst):
    """simulation.md §4 step 3 (without Use All): all eligible entries with the all-levels
    flag, else those of the highest eligible level."""
    elig = eligible(lst)
    if lst["flags"] & LVL_ALL_LEVELS or not elig:
        return elig
    top = max(e[0] for e in elig)
    return [e for e in elig if e[0] == top]


def reachable(lst):
    return eligible(lst) if lst["flags"] & LVL_USE_ALL else pool(lst)


class ArmourPredicates:
    """can_yield_armour / armour_random of actors.md §2.1, memoised (cycles: False)."""

    def __init__(self, forms: Forms):
        self.forms = forms
        self.yield_memo = {}
        self.random_memo = {}

    def can_yield(self, gid) -> bool:
        sig = self.forms.sig(gid)
        if sig == b"ARMO":
            return True
        if sig != b"LVLI":
            return False
        if gid in self.yield_memo:
            return bool(self.yield_memo[gid])
        self.yield_memo[gid] = False  # in progress: a cycle yields nothing more
        lst = self.forms.get(gid, b"LVLI")
        result = any(self.can_yield(ref) for _, ref, _ in reachable(lst))
        self.yield_memo[gid] = result
        return result

    def armour_random(self, gid) -> bool:
        if self.forms.sig(gid) != b"LVLI":
            return False
        if gid in self.random_memo:
            return bool(self.random_memo[gid])
        self.random_memo[gid] = False
        lst = self.forms.get(gid, b"LVLI")
        result = False
        if self.can_yield(gid):
            if lst["chance_none"] > 0 or lst["global"] is not None:
                result = True
            elif not lst["flags"] & LVL_USE_ALL and len({ref for _, ref, _ in pool(lst)}) > 1:
                result = True
            else:
                result = any(self.armour_random(ref) for _, ref, _ in reachable(lst))
        self.random_memo[gid] = result
        return result


# --------------------------------------------------------------------------- resolution

class ActorResolver:
    def __init__(self, forms: Forms):
        self.forms = forms
        self.armour = ArmourPredicates(forms)

    # ------------------------------------------------------------ LVLN
    def pick_npc(self, gid, state, depth=0):
        """An NPC_ for `gid` (NPC_ or LVLN), picking each LVLN once per actor."""
        if depth > MAX_DEPTH:
            raise ActorError("leveled_npc_too_deep", f"leveled NPC nesting deeper than {MAX_DEPTH}")
        sig = self.forms.sig(gid)
        if sig == b"NPC_":
            return gid
        if sig != b"LVLN":
            raise ActorError("not_an_npc", f"{fid_text(gid)} is "
                             + ("undefined" if sig is None else oracle_esm.signature_text(sig))
                             + ", not NPC_ or LVLN")
        picks = state["picks"]
        if gid in picks:
            return picks[gid]
        lst = self.forms.get(gid, b"LVLN")
        candidates = pool(lst)
        if not candidates:
            raise ActorError("leveled_npc_empty", f"LVLN {fid_text(gid)} has no entry at level {DISPLAY_LEVEL}")
        if len({ref for _, ref, _ in candidates}) > 1:
            state["random"] = True
        choice = candidates[state["rng"].randrange(len(candidates))][1] if len(candidates) > 1 else candidates[0][1]
        picks[gid] = None  # guards against an LVLN that contains itself
        result = self.pick_npc(choice, state, depth + 1)
        picks[gid] = result
        return result

    def source(self, npc, bit, state):
        """First NPC_ along the TPLT chain whose template flag `bit` is clear (§2.2)."""
        seen = set()
        cur = npc
        while True:
            rec = self.forms.get(cur, b"NPC_")
            if not rec["tflags"] & bit:
                return cur, rec
            if cur in seen:
                raise ActorError("template_loop", f"template chain loops at {fid_text(cur)}")
            seen.add(cur)
            if rec["tplt"] is None:
                # SPEC-AMBIGUITY: a template flag without a usable TPLT ends the chain;
                # the NPC's own data is used.
                return cur, rec
            cur = self.pick_npc(rec["tplt"], state)

    # ------------------------------------------------------------ LVLI
    def resolve_items(self, gid, count, state, out, depth=0):
        """Append the ARMO FormIDs one carried entry yields at level 1 (simulation.md §4)."""
        if count <= 0 or depth > MAX_DEPTH:
            return
        sig = self.forms.sig(gid)
        if sig == b"ARMO":
            out.append(gid)
            return
        if sig != b"LVLI" or not self.armour.can_yield(gid):
            return  # cannot yield armour: the outcome does not matter
        lst = self.forms.get(gid, b"LVLI")
        rng = state["rng"]
        rolls = count if lst["flags"] & LVL_EACH else 1
        for _ in range(rolls):
            chance = lst["chance_none"]
            if lst["global"] is not None:
                glob = self.forms.get(lst["global"], b"GLOB")
                chance = max(0, min(100, int(glob["value"]))) if glob is not None else chance
            if chance > 0 and rng.randrange(100) < chance:
                continue
            mult = 1 if lst["flags"] & LVL_EACH else count
            if lst["flags"] & LVL_USE_ALL:
                chosen = eligible(lst)
            else:
                p = pool(lst)
                chosen = [p[rng.randrange(len(p))]] if len(p) > 1 else p
            for _, ref, n in chosen:
                self.resolve_items(ref, n * mult, state, out, depth + 1)

    # ------------------------------------------------------------ actor
    def resolve(self, ref_gid: int, base: int) -> dict:
        forms = self.forms
        state = {"picks": {}, "random": False, "rng": _random.Random(ref_gid)}
        npc = self.pick_npc(base, state)
        npc_rec = forms.get(npc, b"NPC_")
        traits_id, traits = self.source(npc, TPL_TRAITS, state)
        model_id, model = self.source(npc, TPL_MODEL, state)
        inv_id, inv = self.source(npc, TPL_INVENTORY, state)

        female = traits["female"]
        sex = 1 if female else 0
        race_id = traits["race"]
        race = forms.get(race_id, b"RACE")
        if race is None:
            raise ActorError("race_missing", "race " + ("absent" if race_id is None else fid_text(race_id))
                             + f" of {fid_text(traits_id)} is not a RACE")
        if race["data"] is None or len(race["data"]) < 24:
            raise ActorError("race_data_missing", f"RACE {fid_text(race_id)} has no DATA heights")
        race_height = _F32.unpack_from(race["data"], 20 if female else 16)[0]

        # Skeleton (§2.2).
        skeleton = None
        for raw in (model["modl_raw"], traits["modl_raw"]):
            path = archive_path(raw, "meshes\\")
            if path is not None and not path.rsplit("\\", 1)[-1].startswith("marker"):
                skeleton = path
                break
        if skeleton is None:
            skeleton = DEFAULT_SKELETON

        # random (b) (§2.1).
        for item, count in inv["items"]:
            if count > 0 and self.armour.armour_random(item):
                state["random"] = True
                break

        # Worn armour (§2.3).
        carried = []
        for item, count in inv["items"]:
            self.resolve_items(item, count, state, carried)
        candidates = []
        for item in carried:
            if item not in candidates:
                candidates.append(item)
        recs = {c: forms.get(c, b"ARMO") for c in candidates}
        ordered = sorted(range(len(candidates)),
                         key=lambda i: (-recs[candidates[i]]["dt"], -recs[candidates[i]]["dr"], i))
        worn = []
        covered = 0
        for i in ordered:
            biped = recs[candidates[i]]["biped"]
            if biped == 0 or biped & covered:
                continue
            worn.append(candidates[i])
            covered |= biped

        parts = []

        def add(role, form, model_path, texture=None):
            part = {"role": role, "form": fid_text(form), "model": model_path}
            if texture is not None:
                part["texture"] = texture
            parts.append(part)

        # 1. Body.
        body_egt = None
        for p in race["body"][sex]:
            if p["index"] == 3:
                if body_egt is None:
                    body_egt = archive_path(p["modl"], "meshes\\")
                continue
            role = BODY_ROLES.get(p["index"])
            path = archive_path(p["modl"], "meshes\\")
            if role is None or path is None or covered & role[1]:
                continue
            add(role[0], race_id, path, archive_path(p["icon"], "textures\\"))

        head_free = not covered & SLOT_HEAD
        # 2. Head.
        if head_free:
            eyes = forms.get(model["eyes"], b"EYES")
            eye_texture = archive_path(eyes["icon"], "textures\\") if eyes is not None else None
            for p in race["head"][sex]:
                path = archive_path(p["modl"], "meshes\\")
                if path is None or p["index"] is None or p["index"] >= len(HEAD_NAMES):
                    continue
                texture = archive_path(p["icon"], "textures\\")
                if p["index"] in (6, 7) and eye_texture is not None:
                    texture = eye_texture
                add("head:" + HEAD_NAMES[p["index"]], race_id, path, texture)

            # 3. Head parts, depth first.
            def head_part(gid, depth, chain):
                rec = forms.get(gid, b"HDPT")
                if rec is None or depth > MAX_DEPTH or gid in chain:
                    return
                path = archive_path(rec["modl"], "meshes\\")
                if path is not None:
                    add("head_part", gid, path)
                for extra in rec["extra"]:
                    head_part(extra, depth + 1, chain | {gid})

            for gid in model["pnams"]:
                head_part(gid, 0, frozenset())

            # 4. Hair.
            if covered & SLOT_HAT or not covered & SLOT_HAIR:
                hair_id = model["hair"]
                hair = forms.get(hair_id, b"HAIR")
                if hair is None:
                    hair_id = race["dnam"][sex]
                    hair = forms.get(hair_id, b"HAIR")
                if hair is not None:
                    path = archive_path(hair["modl"], "meshes\\")
                    if path is not None:
                        add("hair", hair_id, path, archive_path(hair["icon"], "textures\\"))

        # 5. Worn armour.
        for gid in worn:
            rec = recs[gid]
            path = archive_path(rec["mod3"], "meshes\\") if female else None
            if path is None:
                path = archive_path(rec["modl"], "meshes\\")
            if path is not None:
                add("armor", gid, path)

        actor = {
            "reference": fid_text(ref_gid),
            "base": fid_text(base),
            "scale": None,  # filled by the caller
            "npc": fid_text(npc),
            "edid": npc_rec["edid"],
            "female": female,
            "race": fid_text(race_id),
            "skeleton": skeleton,
            "race_height": race_height,
            "parts": parts,
            "worn": [fid_text(w) for w in worn],
            "hat": bool(covered & SLOT_HAT),
        }
        if model["hair_color"] is not None:
            actor["hair_color"] = model["hair_color"]
        if model["facegen"] is not None:
            actor["facegen"] = model["facegen"]
        rfg = race["facegen"][sex]
        if b"FGGS" in rfg and b"FGGA" in rfg and b"FGTS" in rfg:
            actor["race_facegen"] = {"fggs": floats(rfg[b"FGGS"]), "fgga": floats(rfg[b"FGGA"]),
                                     "fgts": floats(rfg[b"FGTS"])}
        if body_egt is not None:
            actor["body_egt"] = body_egt
        actor["random"] = state["random"]
        return actor


# --------------------------------------------------------------------------- document

def placed_actors(index: oracle_world.WorldIndex):
    """Global FormIDs of the winning ACHR records inside cell-children groups."""
    return sorted(gid for gid, (_, _, sig, ctx) in index.winners.items() if sig == b"ACHR" and ctx is not None)


def appearance_document(index, diag, load_order, deterministic: bool, only=None) -> dict:
    forms = Forms(index, diag)
    resolver = ActorResolver(forms)
    outcomes = {}
    base_kinds = {}
    worn_items = {}
    actors = []

    def count(table, key):
        table[key] = table.get(key, 0) + 1

    refs = placed_actors(index) if only is None else [only]
    for gid in refs:
        if forms.sig(gid) is None:
            count(outcomes, "deleted")
            continue
        plugin, subs = forms.subrecords(gid)
        first = _first(subs)
        base = forms.map(plugin, first.get(b"NAME"), gid, "NAME")
        if base is None:
            count(outcomes, "error: no_base")
            actors.append({"reference": fid_text(gid), "base": None, "error": "the reference has no NAME base"})
            continue
        base_sig = forms.sig(base)
        count(base_kinds, "missing" if base_sig is None else oracle_esm.signature_text(base_sig))
        try:
            actor = resolver.resolve(gid, base)
        except ActorError as exc:
            count(outcomes, "error: " + exc.kind)
            actors.append({"reference": fid_text(gid), "base": fid_text(base), "error": exc.text})
            continue
        xscl = first.get(b"XSCL")
        actor["scale"] = _F32.unpack_from(xscl, 0)[0] if xscl is not None and len(xscl) >= 4 else 1.0
        count(outcomes, "resolved random" if actor["random"] else "resolved deterministic")
        if deterministic and actor["random"]:
            continue
        count(worn_items, str(len(actor["worn"])))
        actors.append(actor)

    issues = []
    for code in sorted(diag.counts):
        for detail in diag.examples.get(code, []):
            issues.append({"code": code, "detail": detail})
        extra = diag.counts[code] - len(diag.examples.get(code, []))
        if extra > 0:
            issues.append({"code": code, "detail": f"... and {extra} more"})
    return {
        "schema": APPEARANCE_SCHEMA,
        "summary": {"outcomes": dict(sorted(outcomes.items())),
                    "base_kinds": dict(sorted(base_kinds.items())),
                    "worn_items": dict(sorted(worn_items.items(), key=lambda kv: int(kv[0])))},
        "issues": issues,
        "actors": actors,
        "load_order": list(load_order),
    }


def render(doc: dict, list_key: str) -> str:
    """JSON with one element of `list_key` per line (as oracle_esm.render)."""
    lines = ["{"]
    keys = list(doc)
    for i, key in enumerate(keys):
        value = doc[key]
        comma = "," if i < len(keys) - 1 else ""
        if key == list_key and value:
            lines.append(f" {json.dumps(key)}: [")
            lines.append(",\n".join(" " + json.dumps(item, ensure_ascii=False) for item in value))
            lines.append(f" ]{comma}")
        else:
            lines.append(f" {json.dumps(key)}: {json.dumps(value, ensure_ascii=False)}{comma}")
    lines.append("}")
    return "\n".join(lines) + "\n"


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Actor appearance oracle (fnv.actor.appearance/1).")
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("appearance", help=f"emit {APPEARANCE_SCHEMA}")
    p.add_argument("data_dir")
    p.add_argument("plugins", nargs="+", metavar="plugin")
    sel = p.add_mutually_exclusive_group()
    sel.add_argument("--all", action="store_true", help="every ACHR of the load order (default)")
    sel.add_argument("--ref", help="one reference by global FormID (0x...)")
    p.add_argument("--deterministic", action="store_true", help="leave out actors whose `random` is true")
    p.add_argument("--schema", help="accepted for fnvtool compatibility; ignored")
    p.add_argument("-o", "--output", help="write JSON here instead of stdout")
    args = parser.parse_args(argv)

    lowered = [name.lower() for name in args.plugins]
    if len(set(lowered)) != len(lowered) or len(args.plugins) > 255:
        print("oracle_actor: a plugin is listed twice, or more than 255 plugins", file=sys.stderr)
        return 2
    only = None
    if args.ref is not None:
        try:
            only = int(args.ref, 16)
        except ValueError:
            print(f"oracle_actor: --ref {args.ref!r} is not a hexadecimal FormID", file=sys.stderr)
            return 2
    if not os.path.isdir(args.data_dir):
        print(f"oracle_actor: data directory not found: {args.data_dir}", file=sys.stderr)
        return 1
    diag = oracle_world.Diagnostics()
    index, error = oracle_world.load(args.data_dir, args.plugins, diag)
    if index is None:
        print(f"oracle_actor: {error}", file=sys.stderr)
        return 1
    try:
        if only is not None:
            w = index.winners.get(only)
            if w is None or w[2] != b"ACHR":
                print(f"oracle_actor: --ref {args.ref}: no ACHR with that FormID", file=sys.stderr)
                return 1
        doc = appearance_document(index, diag, args.plugins, args.deterministic, only)
    finally:
        index.close()
    text = render(doc, "actors")
    if args.output:
        with open(args.output, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
    else:
        sys.stdout.buffer.write(text.encode("utf-8"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
