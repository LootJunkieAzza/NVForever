"""Tests for oracle_actor.py using a tiny synthetic plugin (no game data).

Run from the repo root:

    python -m unittest discover -s tools/capture/tests -v
"""
import json
import os
import struct
import sys
import tempfile
import unittest

CAPTURE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, CAPTURE_DIR)

import oracle_actor  # noqa: E402


# =========================================================================== builders

def sub(sig, data):
    return sig + struct.pack("<H", len(data)) + data


def zstr(text):
    return text.encode("cp1252") + b"\0"


def u32(value):
    return struct.pack("<I", value)


def f32s(*values):
    return struct.pack(f"<{len(values)}f", *values)


def record(sig, formid, payload=b"", flags=0):
    return struct.pack("<4sIIIIHH", sig, len(payload), flags, formid, 0, 15, 0) + payload


def group(label, group_type, *children):
    body = b"".join(children)
    if isinstance(label, int):
        label = struct.pack("<I", label)
    return struct.pack("<4sI4siHHHH", b"GRUP", 24 + len(body), label, group_type, 0, 0, 0, 0) + body


def tes4():
    return record(b"TES4", 0, sub(b"HEDR", struct.pack("<fiI", 1.34, 0, 0x800)), flags=0x1)


def edid(text):
    return sub(b"EDID", zstr(text))


# ACBS template flag bits (actors.md §2.2).
TRAITS, MODEL, INVENTORY = 1 << 0, 1 << 6, 1 << 8
# ARMO biped flag bits (actors.md §1.1).
HEAD, HAIR, UPPER, LHAND, RHAND, HAT, EYE_GLASSES = 1, 2, 4, 8, 16, 1 << 10, 1 << 11


def acbs(female=False, tflags=0):
    return sub(b"ACBS", struct.pack("<IHHHHHHfhH", 1 if female else 0, 0, 0, 1, 0, 0, 100, 0.0, 0, tflags))


def npc(formid, name, female=False, tflags=0, tplt=None, race=0x100, hair=None, eyes=None, modl=None,
        pnams=(), hclr=None, facegen=None, items=()):
    payload = edid(name)
    if modl is not None:
        payload += sub(b"MODL", zstr(modl))
    payload += acbs(female, tflags)
    for item, count in items:
        payload += sub(b"CNTO", struct.pack("<Ii", item, count))
    if tplt is not None:
        payload += sub(b"TPLT", u32(tplt))
    if race is not None:
        payload += sub(b"RNAM", u32(race))
    for p in pnams:
        payload += sub(b"PNAM", u32(p))
    if hair is not None:
        payload += sub(b"HNAM", u32(hair))
    if eyes is not None:
        payload += sub(b"ENAM", u32(eyes))
    if hclr is not None:
        payload += sub(b"HCLR", bytes(hclr) + b"\0")
    if facegen is not None:
        fggs, fgga, fgts = facegen
        payload += sub(b"FGGS", f32s(*fggs)) + sub(b"FGGA", f32s(*fgga)) + sub(b"FGTS", f32s(*fgts))
    return record(b"NPC_", formid, payload)


def part(index, modl=None, icon=None):
    out = sub(b"INDX", u32(index))
    if modl is not None:
        out += sub(b"MODL", zstr(modl)) + sub(b"MODB", f32s(0.0))
    if icon is not None:
        out += sub(b"ICON", zstr(icon))
    return out


RACE_FG_MALE = ([0.5, -1.25], [0.25], [2.0])
RACE_FG_FEMALE = ([1.5], [0.5], [-0.5])


def race():
    data = bytes(16) + f32s(1.0, 0.75, 1.0, 1.0) + u32(1)
    payload = (edid("TestRace") + sub(b"DATA", data) + sub(b"DNAM", u32(0x200) + u32(0x201))
               + sub(b"NAM0", b"")
               + sub(b"MNAM", b"")
               + part(0, "Characters\\Head\\HeadHuman.NIF", "Characters/Male/HeadHumanOld.dds")
               + part(1, None, "characters\\male\\ears.dds")
               + part(6, "characters\\head\\EyeLeftHuman.nif", "characters\\eyes\\racelefteye.dds")
               + part(7, "characters\\head\\EyeRightHuman.nif")
               + sub(b"FNAM", b"")
               + part(0, "\\Characters\\Head\\HeadFemale.nif", "textures\\characters\\female\\headfemale.dds")
               + sub(b"NAM1", b"")
               + sub(b"MNAM", b"")
               + part(0, "characters\\_Male\\UpperBody.nif", "characters\\male\\upperbody.dds")
               + part(1, "characters\\_Male\\LeftHand.nif", "characters\\male\\hand.dds")
               + part(2, "characters\\_Male\\RightHand.nif")
               + part(3, "Characters\\_Male\\UpperBodyHumanMale.egt")
               + sub(b"FNAM", b"")
               + part(3, "Characters\\_Female\\UpperBodyHumanFemale.egt")
               + part(0, "characters\\_female\\upperbody.nif")
               + sub(b"HNAM", u32(0x200) + u32(0x201)) + sub(b"ENAM", u32(0x300))
               + sub(b"MNAM", b"") + sub(b"FGGS", f32s(*RACE_FG_MALE[0])) + sub(b"FGGA", f32s(*RACE_FG_MALE[1]))
               + sub(b"FGTS", f32s(*RACE_FG_MALE[2])) + sub(b"SNAM", b"\0\0")
               + sub(b"FNAM", b"") + sub(b"FGGS", f32s(*RACE_FG_FEMALE[0]))
               + sub(b"FGGA", f32s(*RACE_FG_FEMALE[1])) + sub(b"FGTS", f32s(*RACE_FG_FEMALE[2]))
               + sub(b"SNAM", b"\0\0"))
    return record(b"RACE", 0x100, payload)


def simple(sig, formid, name, modl=None, icon=None, extra=()):
    payload = edid(name)
    if modl is not None:
        payload += sub(b"MODL", zstr(modl))
    if icon is not None:
        payload += sub(b"ICON", zstr(icon))
    for e in extra:
        payload += sub(b"HNAM", u32(e))
    return record(sig, formid, payload)


def armo(formid, name, biped, dt, dr=0, mod3=None):
    payload = edid(name) + sub(b"BMDT", u32(biped) + bytes(4))
    payload += sub(b"MODL", zstr(f"Armor\\{name}\\M\\Outfit.nif"))
    if mod3 is not None:
        payload += sub(b"MOD3", zstr(mod3))
    payload += sub(b"DATA", struct.pack("<iif", 10, 100, 5.0))
    payload += sub(b"DNAM", struct.pack("<h2sfH2s", dr, b"\0\0", dt, 0, b"\0\0"))
    return record(b"ARMO", formid, payload)


def leveled(sig, formid, name, entries, chance_none=0, flags=0):
    payload = edid(name) + sub(b"LVLD", bytes([chance_none])) + sub(b"LVLF", bytes([flags]))
    for level, ref, count in entries:
        payload += sub(b"LVLO", struct.pack("<H2sIH2s", level, b"\0\0", ref, count, b"\0\0"))
    return record(sig, formid, payload)


def achr(formid, base, scale=None, flags=0):
    payload = sub(b"NAME", u32(base)) + sub(b"DATA", f32s(0, 0, 0, 0, 0, 0))
    if scale is not None:
        payload += sub(b"XSCL", f32s(scale))
    return record(b"ACHR", formid, payload, flags=flags)


# Armour (DT, DR): Leather (6, 0) upper body, MOD3; Metal (12, 0) upper body; Gloves (1, 5)
# both hands; Gloves2 (1, 10) left hand; Ring (50, 0) no slot; Glasses (1, 5); hats.
LEATHER, METAL, GLOVES, GLOVES2, RING, GLASSES, COWBOY, WIG, HELMET = range(0x500, 0x509)

ACTORS = {
    # reference: (base, npc record)
    0xA00: 0x930,  # plain male
    0xA01: 0x960,  # plain female
    0xA02: 0x931,  # armour order
    0xA03: 0x910,  # templates Traits + Model through LVLN 0x800
    0xA04: 0x911,  # template Inventory only through LVLN 0x800
    0xA05: 0x920,  # marker skeleton, Traits template with a real skeleton
    0xA06: 0x922,  # marker skeleton, no template
    0xA07: 0x940,  # cowboy hat (Hair + Hat)
    0xA08: 0x941,  # wig (Hair only), no eyes
    0xA09: 0x942,  # power helmet (Head + Hair)
    0xA0A: 0x950,  # armour list with chance none -> random
    0xA0B: 0x951,  # random non-armour list -> not random
    0xA0C: 0x952,  # armour-random list with count 0 -> not random
    0xA0D: 0x801,  # base LVLN with two entries -> random
    0xA0E: 0x800,  # base LVLN with one eligible entry
    0xA0F: 0x954,  # single-item armour list
    0xA10: 0x955,  # Use All armour list
    0xA11: 0x956,  # armour list with a pool of two -> random
}


def build_plugin():
    npcs = [
        npc(0x900, "TemplateSource", female=True, hair=0x202, eyes=0x300, pnams=[0x403],
            modl="Characters\\_Male\\Skeleton.NIF", hclr=(10, 20, 30),
            facegen=([1.5, 2.25], [0.125], [-3.0]), items=[(LEATHER, 1)]),
        npc(0x901, "TooHigh", items=[(METAL, 1)]),
        npc(0x902, "OtherPick", items=[(METAL, 1)]),
        npc(0x910, "Templated", tflags=TRAITS | MODEL, tplt=0x800, race=None,
            modl="characters\\_male\\marker_creature.nif", items=[(METAL, 1)]),
        npc(0x911, "TemplatedInventory", tflags=INVENTORY, tplt=0x800, eyes=0x300, items=[(METAL, 1)]),
        npc(0x920, "MarkerSkel", tflags=TRAITS, tplt=0x921, modl="Characters\\_Male\\marker_creature.nif"),
        npc(0x921, "TraitsSkel", modl="Creatures/Special/Skeleton.nif"),
        npc(0x922, "MarkerOnly", modl="characters\\_male\\Marker_Creature.nif"),
        npc(0x930, "PlainMale", eyes=0x300, pnams=[0x400, 0x403]),
        npc(0x931, "Armoured", eyes=0x300,
            items=[(GLOVES, 1), (LEATHER, 1), (RING, 1), (GLASSES, 1), (METAL, 1), (GLOVES2, 2),
                   (LEATHER, 1), (0x700, 3)]),
        npc(0x940, "Cowboy", eyes=0x300, items=[(COWBOY, 1)]),
        npc(0x941, "Wigged", pnams=[0x400], items=[(WIG, 1)]),
        npc(0x942, "Helmeted", eyes=0x300, pnams=[0x400], items=[(HELMET, 1)]),
        npc(0x950, "RandomArmour", items=[(0x600, 1)]),
        npc(0x951, "RandomJunk", items=[(0x601, 1), (LEATHER, 1)]),
        npc(0x952, "CountZero", items=[(0x600, 0)]),
        npc(0x954, "SingleList", items=[(0x602, 1)]),
        npc(0x955, "UseAllList", items=[(0x603, 1)]),
        npc(0x956, "PoolOfTwo", items=[(0x604, 1)]),
        npc(0x960, "PlainFemale", female=True, eyes=0x300, items=[(LEATHER, 1), (GLOVES2, 1)]),
    ]
    refs = [achr(ref, base, scale=1.5 if ref == 0xA00 else None) for ref, base in ACTORS.items()]
    refs.append(achr(0xAFF, 0x930, flags=0x20))  # deleted: skipped
    body = (
        group(b"RACE", 0, race())
        + group(b"HAIR", 0,
                simple(b"HAIR", 0x200, "HairMale", "Characters\\Hair\\HairM.NIF", "characters\\hair\\hairm.dds"),
                simple(b"HAIR", 0x201, "HairFemale", "characters\\hair\\hairf.nif", "characters\\hair\\hairf.dds"),
                simple(b"HAIR", 0x202, "HairSpecial", "characters\\hair\\special.nif"))
        + group(b"EYES", 0, simple(b"EYES", 0x300, "EyeBlue", icon="Characters\\Eyes\\EyeBlue.dds"))
        + group(b"HDPT", 0,
                simple(b"HDPT", 0x400, "Beard", "characters\\head\\beard.nif", extra=[0x401]),
                simple(b"HDPT", 0x401, "Eyebrow", "characters\\head\\eyebrowm.nif", extra=[0x402]),
                simple(b"HDPT", 0x402, "Stubble", "characters\\head\\stubble.nif"),
                simple(b"HDPT", 0x403, "Mustache", "characters\\head\\mustache.nif"))
        + group(b"ARMO", 0,
                armo(LEATHER, "Leather", UPPER, 6.0, mod3="Armor\\Leather\\F\\Outfit.nif"),
                armo(METAL, "Metal", UPPER, 12.0),
                armo(GLOVES, "Gloves", LHAND | RHAND, 1.0, 5),
                armo(GLOVES2, "Gloves2", LHAND, 1.0, 10),
                armo(RING, "Ring", 0, 50.0),
                armo(GLASSES, "Glasses", EYE_GLASSES, 1.0, 5),
                armo(COWBOY, "Cowboy", HAIR | HAT, 1.0),
                armo(WIG, "Wig", HAIR, 0.0),
                armo(HELMET, "Helmet", HEAD | HAIR | HAT, 5.0))
        + group(b"MISC", 0, record(b"MISC", 0x700, edid("Junk1")), record(b"MISC", 0x701, edid("Junk2")))
        + group(b"LVLI", 0,
                leveled(b"LVLI", 0x600, "ArmourChanceNone", [(1, LEATHER, 1)], chance_none=50),
                leveled(b"LVLI", 0x601, "JunkRandom", [(1, 0x700, 1), (1, 0x701, 1)], chance_none=50),
                leveled(b"LVLI", 0x602, "ArmourSingle", [(1, METAL, 1), (5, LEATHER, 1)]),
                leveled(b"LVLI", 0x603, "ArmourUseAll", [(1, GLOVES2, 1), (1, GLASSES, 1), (1, 0x601, 1)],
                        flags=0x4),
                leveled(b"LVLI", 0x604, "ArmourPoolOfTwo", [(1, LEATHER, 1), (1, METAL, 1)]))
        + group(b"LVLN", 0,
                leveled(b"LVLN", 0x800, "LvlOne", [(1, 0x900, 1), (10, 0x901, 1)]),
                leveled(b"LVLN", 0x801, "LvlTwo", [(1, 0x900, 1), (1, 0x902, 1)], flags=0x1))
        + group(b"NPC_", 0, *npcs)
        + group(b"CELL", 0,
                group(0, 2, group(0, 3,
                                  record(b"CELL", 0xB00, edid("TestCell") + sub(b"DATA", b"\x01")),
                                  group(0xB00, 6, group(0xB00, 8, *refs[:6]), group(0xB00, 9, *refs[6:]))))))
    return tes4() + body


def f32(value):
    """The f32 nearest to `value`, widened back to a Python float."""
    return struct.unpack("<f", struct.pack("<f", value))[0]


class ActorOracleTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls._tmp = tempfile.TemporaryDirectory()
        tmp = cls._tmp.name
        with open(os.path.join(tmp, "Test.esm"), "wb") as f:
            f.write(build_plugin())
        cls.docs = {}
        for name, extra in (("all", []), ("det", ["--deterministic"])):
            out = os.path.join(tmp, f"{name}.json")
            code = oracle_actor.main(["appearance", tmp, "Test.esm", "-o", out] + extra)
            assert code == 0, code
            with open(out, encoding="utf-8") as f:
                cls.docs[name] = json.load(f)
        cls.actors = {a["reference"]: a for a in cls.docs["all"]["actors"]}

    @classmethod
    def tearDownClass(cls):
        cls._tmp.cleanup()

    def actor(self, ref):
        return self.actors[f"0x{ref:08X}"]

    def roles(self, ref):
        return [(p["role"], p["model"]) for p in self.actor(ref)["parts"]]

    # ------------------------------------------------------------------ document
    def test_document_shape_and_deleted_reference(self):
        doc = self.docs["all"]
        self.assertEqual(doc["schema"], "fnv.actor.appearance/1")
        self.assertEqual(list(doc), ["schema", "summary", "issues", "actors", "load_order"])
        refs = [a["reference"] for a in doc["actors"]]
        self.assertEqual(refs, sorted(f"0x{r:08X}" for r in ACTORS))
        self.assertEqual(doc["summary"]["outcomes"]["deleted"], 1)
        self.assertEqual(doc["summary"]["base_kinds"], {"LVLN": 2, "NPC_": len(ACTORS) - 2})

    # ------------------------------------------------------------------ race sections
    def test_male_race_parts_in_order(self):
        a = self.actor(0xA00)
        self.assertEqual(a["scale"], 1.5)
        self.assertEqual(a["npc"], "0x00000930")
        self.assertEqual(a["edid"], "PlainMale")
        self.assertFalse(a["female"])
        self.assertEqual(a["race"], "0x00000100")
        self.assertEqual(a["race_height"], 1.0)
        self.assertEqual(a["skeleton"], "meshes\\characters\\_male\\skeleton.nif")
        self.assertEqual(a["parts"], [
            {"role": "body:upper_body", "form": "0x00000100", "model": "meshes\\characters\\_male\\upperbody.nif",
             "texture": "textures\\characters\\male\\upperbody.dds"},
            {"role": "body:left_hand", "form": "0x00000100", "model": "meshes\\characters\\_male\\lefthand.nif",
             "texture": "textures\\characters\\male\\hand.dds"},
            {"role": "body:right_hand", "form": "0x00000100", "model": "meshes\\characters\\_male\\righthand.nif"},
            {"role": "head:head", "form": "0x00000100", "model": "meshes\\characters\\head\\headhuman.nif",
             "texture": "textures\\characters\\male\\headhumanold.dds"},
            {"role": "head:left_eye", "form": "0x00000100", "model": "meshes\\characters\\head\\eyelefthuman.nif",
             "texture": "textures\\characters\\eyes\\eyeblue.dds"},
            {"role": "head:right_eye", "form": "0x00000100", "model": "meshes\\characters\\head\\eyerighthuman.nif",
             "texture": "textures\\characters\\eyes\\eyeblue.dds"},
            {"role": "head_part", "form": "0x00000400", "model": "meshes\\characters\\head\\beard.nif"},
            {"role": "head_part", "form": "0x00000401", "model": "meshes\\characters\\head\\eyebrowm.nif"},
            {"role": "head_part", "form": "0x00000402", "model": "meshes\\characters\\head\\stubble.nif"},
            {"role": "head_part", "form": "0x00000403", "model": "meshes\\characters\\head\\mustache.nif"},
            {"role": "hair", "form": "0x00000200", "model": "meshes\\characters\\hair\\hairm.nif",
             "texture": "textures\\characters\\hair\\hairm.dds"},
        ])
        self.assertEqual(a["worn"], [])
        self.assertFalse(a["hat"])
        self.assertEqual(a["body_egt"], "meshes\\characters\\_male\\upperbodyhumanmale.egt")
        self.assertEqual(a["race_facegen"], {"fggs": [0.5, -1.25], "fgga": [0.25], "fgts": [2.0]})
        self.assertNotIn("facegen", a)
        self.assertNotIn("hair_color", a)
        self.assertFalse(a["random"])

    def test_female_race_parts_facegen_and_mod3(self):
        a = self.actor(0xA01)
        self.assertTrue(a["female"])
        self.assertEqual(a["race_height"], 0.75)
        self.assertEqual(a["scale"], 1.0)
        self.assertEqual(a["body_egt"], "meshes\\characters\\_female\\upperbodyhumanfemale.egt")
        self.assertEqual(a["race_facegen"], {"fggs": [1.5], "fgga": [0.5], "fgts": [-0.5]})
        # Upper body covered by the outfit; the female race has no hands; no eye meshes.
        self.assertEqual(self.roles(0xA01), [
            ("head:head", "meshes\\characters\\head\\headfemale.nif"),
            ("hair", "meshes\\characters\\hair\\hairf.nif"),           # race default for females
            ("armor", "meshes\\armor\\leather\\f\\outfit.nif"),         # MOD3
            ("armor", "meshes\\armor\\gloves2\\m\\outfit.nif"),         # no MOD3: MODL
        ])
        self.assertEqual(a["parts"][0]["texture"], "textures\\characters\\female\\headfemale.dds")
        self.assertEqual(a["worn"], ["0x00000500", "0x00000503"])

    def test_eye_texture_falls_back_to_race_icon(self):
        parts = {p["role"]: p for p in self.actor(0xA08)["parts"]}
        self.assertEqual(parts["head:left_eye"]["texture"], "textures\\characters\\eyes\\racelefteye.dds")
        self.assertNotIn("texture", parts["head:right_eye"])

    # ------------------------------------------------------------------ worn armour
    def test_worn_order_dt_then_dr_then_appearance_with_shared_slots(self):
        a = self.actor(0xA02)
        # Metal (DT 12) beats Leather (DT 6) for the upper body; Gloves2 (DR 10) takes the
        # left hand before Gloves (DR 5); Glasses (DR 5, after Gloves) fits; Ring has no slot.
        self.assertEqual(a["worn"], ["0x00000501", "0x00000503", "0x00000505"])
        self.assertEqual([r for r, _ in self.roles(0xA02) if r.startswith("body:")], ["body:right_hand"])
        self.assertEqual([p["form"] for p in a["parts"] if p["role"] == "armor"],
                         ["0x00000501", "0x00000503", "0x00000505"])
        self.assertTrue(all("texture" not in p for p in a["parts"] if p["role"] == "armor"))
        self.assertEqual(self.docs["all"]["summary"]["worn_items"]["3"], 1)

    def test_single_item_and_use_all_lists(self):
        self.assertEqual(self.actor(0xA0F)["worn"], ["0x00000501"])
        self.assertFalse(self.actor(0xA0F)["random"])
        self.assertEqual(self.actor(0xA10)["worn"], ["0x00000503", "0x00000505"])
        self.assertFalse(self.actor(0xA10)["random"])

    # ------------------------------------------------------------------ headgear
    def test_hat_keeps_hair_with_hat_flag(self):
        a = self.actor(0xA07)
        self.assertTrue(a["hat"])
        self.assertIn(("hair", "meshes\\characters\\hair\\hairm.nif"), self.roles(0xA07))
        self.assertIn(("head:head", "meshes\\characters\\head\\headhuman.nif"), self.roles(0xA07))

    def test_hair_without_hat_hides_hair(self):
        a = self.actor(0xA08)
        self.assertFalse(a["hat"])
        roles = [r for r, _ in self.roles(0xA08)]
        self.assertNotIn("hair", roles)
        self.assertIn("head:head", roles)
        self.assertEqual(roles.count("head_part"), 3)
        self.assertEqual(roles[-1], "armor")

    def test_head_covered_hides_head_parts_and_hair(self):
        a = self.actor(0xA09)
        self.assertTrue(a["hat"])
        self.assertEqual([r for r, _ in self.roles(0xA09)],
                         ["body:upper_body", "body:left_hand", "body:right_hand", "armor"])

    # ------------------------------------------------------------------ templates
    def test_traits_and_model_from_lvln_template(self):
        a = self.actor(0xA03)
        self.assertEqual(a["npc"], "0x00000910")
        self.assertEqual(a["edid"], "Templated")
        self.assertTrue(a["female"])                      # Traits from 0x900
        self.assertEqual(a["race"], "0x00000100")
        self.assertEqual(a["skeleton"], "meshes\\characters\\_male\\skeleton.nif")
        self.assertEqual(a["hair_color"], [10, 20, 30])
        self.assertEqual(a["facegen"], {"fggs": [1.5, 2.25], "fgga": [0.125], "fgts": [-3.0]})
        self.assertEqual(a["worn"], ["0x00000501"])         # own inventory (Metal)
        roles = self.roles(0xA03)
        self.assertIn(("hair", "meshes\\characters\\hair\\special.nif"), roles)
        self.assertIn(("head_part", "meshes\\characters\\head\\mustache.nif"), roles)
        self.assertIn(("armor", "meshes\\armor\\metal\\m\\outfit.nif"), roles)  # female, no MOD3
        hair = [p for p in a["parts"] if p["role"] == "hair"][0]
        self.assertNotIn("texture", hair)
        self.assertFalse(a["random"])

    def test_inventory_only_template(self):
        a = self.actor(0xA04)
        self.assertFalse(a["female"])
        self.assertEqual(a["worn"], ["0x00000500"])          # Leather from 0x900, not own Metal
        self.assertIn(("armor", "meshes\\armor\\leather\\m\\outfit.nif"), self.roles(0xA04))
        self.assertNotIn("facegen", a)

    def test_lvln_base(self):
        a = self.actor(0xA0E)
        self.assertEqual(a["base"], "0x00000800")
        self.assertEqual(a["npc"], "0x00000900")
        self.assertEqual(a["edid"], "TemplateSource")
        self.assertFalse(a["random"])
        self.assertTrue(self.actor(0xA0D)["random"])

    # ------------------------------------------------------------------ skeleton
    def test_marker_skeleton_falls_back_to_traits_then_default(self):
        self.assertEqual(self.actor(0xA05)["skeleton"], "meshes\\creatures\\special\\skeleton.nif")
        self.assertEqual(self.actor(0xA06)["skeleton"], "meshes\\characters\\_male\\skeleton.nif")

    # ------------------------------------------------------------------ random
    def test_random_predicate(self):
        self.assertTrue(self.actor(0xA0A)["random"])     # armour list with chance none
        self.assertFalse(self.actor(0xA0B)["random"])    # random list without armour
        self.assertEqual(self.actor(0xA0B)["worn"], ["0x00000500"])
        self.assertFalse(self.actor(0xA0C)["random"])    # armour-random entry with count 0
        self.assertEqual(self.actor(0xA0C)["worn"], [])
        self.assertTrue(self.actor(0xA11)["random"])     # pool of two armours

    def test_deterministic_filter(self):
        det = self.docs["det"]
        refs = {a["reference"] for a in det["actors"]}
        random_refs = {f"0x{r:08X}" for r in (0xA0A, 0xA0D, 0xA11)}
        self.assertEqual(refs, {f"0x{r:08X}" for r in ACTORS} - random_refs)
        self.assertEqual(det["summary"]["outcomes"],
                         {"deleted": 1, "resolved deterministic": len(ACTORS) - 3, "resolved random": 3})
        for a in det["actors"]:
            self.assertEqual(a, self.actors[a["reference"]])


# =========================================================================== creatures

STATS = 1 << 1


def crea(formid, name, modl=None, nifz=(), kffz=(), bnam=None, tflags=0, tplt=None, kind=None):
    payload = edid(name)
    if modl is not None:
        payload += sub(b"MODL", zstr(modl))
    if nifz:
        payload += sub(b"NIFZ", b"".join(zstr(n) for n in nifz) + b"\0")
    payload += sub(b"ACBS", struct.pack("<IHHHHHHfhH", 0, 0, 0, 1, 0, 0, 100, 0.0, 0, tflags))
    if tplt is not None:
        payload += sub(b"TPLT", u32(tplt))
    if kind is not None:
        payload += sub(b"DATA", bytes([kind]) + bytes(16))
    if kffz:
        payload += sub(b"KFFZ", b"".join(zstr(n) for n in kffz) + b"\0")
    if bnam is not None:
        payload += sub(b"BNAM", f32s(bnam))
    return record(b"CREA", formid, payload)


def acre(formid, base, scale=None, flags=0):
    payload = sub(b"NAME", u32(base)) + sub(b"DATA", f32s(0, 0, 0, 0, 0, 0))
    if scale is not None:
        payload += sub(b"XSCL", f32s(scale))
    return record(b"ACRE", formid, payload, flags=flags)


CREATURES = {
    0xC00: 0xD00,  # plain gecko
    0xC01: 0xD01,  # template: model and stats from 0xD00
    0xC02: 0xD02,  # template through an LVLC with two entries -> random
    0xC03: 0xD03,  # marker skeleton -> error
    0xC04: 0x930,  # an NPC base -> error
}


def build_creature_plugin():
    creatures = [
        crea(0xD00, "Gecko", "Creatures\\NVGecko\\Skeleton.NIF", nifz=["NVGecko.NIF", " Sub/Spikes.nif "],
             kffz=["SpecialIdle.KF"], bnam=0.55, kind=1),
        crea(0xD01, "GeckoCopy", nifz=["Ignored.nif"], bnam=2.0, tflags=MODEL | STATS, tplt=0xD00, kind=0),
        crea(0xD02, "LevelledGecko", tflags=MODEL, tplt=0xE00),
        crea(0xD03, "Marker", "marker_creature.nif"),
        crea(0xD04, "Other", "creatures\\other\\skeleton.nif", nifz=["other.nif"]),
    ]
    refs = [acre(ref, base, scale=0.7 if ref == 0xC00 else None) for ref, base in CREATURES.items()]
    refs.append(acre(0xCFF, 0xD00, flags=0x20))
    body = (
        group(b"RACE", 0, race())
        + group(b"NPC_", 0, npc(0x930, "PlainMale"))
        + group(b"CREA", 0, *creatures)
        + group(b"LVLC", 0, leveled(b"LVLC", 0xE00, "LvlGecko", [(1, 0xD00, 1), (1, 0xD04, 1)]))
        + group(b"CELL", 0,
                group(0, 2, group(0, 3,
                                  record(b"CELL", 0xB00, edid("TestCell") + sub(b"DATA", b"\x01")),
                                  group(0xB00, 6, group(0xB00, 8, *refs))))))
    return tes4() + body


class CreatureOracleTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls._tmp = tempfile.TemporaryDirectory()
        tmp = cls._tmp.name
        with open(os.path.join(tmp, "Test.esm"), "wb") as f:
            f.write(build_creature_plugin())
        cls.docs = {}
        for name, extra in (("all", []), ("det", ["--deterministic"])):
            out = os.path.join(tmp, f"{name}.json")
            code = oracle_actor.main(["creatures", tmp, "Test.esm", "-o", out] + extra)
            assert code == 0, code
            with open(out, encoding="utf-8") as f:
                cls.docs[name] = json.load(f)
        cls.by_ref = {c["reference"]: c for c in cls.docs["all"]["creatures"]}

    @classmethod
    def tearDownClass(cls):
        cls._tmp.cleanup()

    def c(self, ref):
        return self.by_ref[f"0x{ref:08X}"]

    def test_document(self):
        doc = self.docs["all"]
        self.assertEqual(doc["schema"], "fnv.actor.creatures/1")
        self.assertEqual(list(doc), ["schema", "summary", "issues", "creatures", "load_order"])
        self.assertEqual(doc["summary"]["outcomes"]["deleted"], 1)
        self.assertEqual(doc["summary"]["base_kinds"], {"CREA": 4, "NPC_": 1})

    def test_plain_creature(self):
        self.assertEqual(self.c(0xC00), {
            "reference": "0x00000C00", "base": "0x00000D00", "creature": "0x00000D00", "edid": "Gecko",
            "kind": "Mutated Animal", "skeleton": "meshes\\creatures\\nvgecko\\skeleton.nif", "base_scale": f32(0.55),
            "parts": [{"role": "body", "form": "0x00000D00", "model": "meshes\\creatures\\nvgecko\\nvgecko.nif"},
                      {"role": "body", "form": "0x00000D00", "model": "meshes\\creatures\\nvgecko\\sub\\spikes.nif"}],
            "anims": ["specialidle.kf"], "random": False, "scale": f32(0.7)})

    def test_template_groups(self):
        c = self.c(0xC01)
        self.assertEqual((c["creature"], c["edid"], c["kind"]), ("0x00000D01", "GeckoCopy", "Mutated Animal"))
        self.assertEqual(c["base_scale"], f32(0.55))
        self.assertEqual([p["form"] for p in c["parts"]], ["0x00000D00", "0x00000D00"])

    def test_lvlc_template_is_random(self):
        c = self.c(0xC02)
        self.assertTrue(c["random"])
        self.assertEqual(c["base_scale"], 1.0)
        self.assertNotIn(f"0x{0xC02:08X}", {x["reference"] for x in self.docs["det"]["creatures"]})

    def test_errors(self):
        self.assertIn("names no skeleton", self.c(0xC03)["error"])
        self.assertIn("not a creature base", self.c(0xC04)["error"])


class PathTest(unittest.TestCase):
    def test_archive_path(self):
        self.assertEqual(oracle_actor.archive_path(b" \\Characters/Head/X.NIF \0", "meshes\\"),
                         "meshes\\characters\\head\\x.nif")
        self.assertEqual(oracle_actor.archive_path(b"Textures\\A.dds\0", "textures\\"), "textures\\a.dds")
        self.assertIsNone(oracle_actor.archive_path(b"  \0", "meshes\\"))
        self.assertIsNone(oracle_actor.archive_path(None, "meshes\\"))

    def test_f32_widening(self):
        self.assertEqual(oracle_actor.floats(f32s(0.1)), [f32(0.1)])


if __name__ == "__main__":
    unittest.main()
