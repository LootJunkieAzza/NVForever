"""Tests for oracle_nif.py using tiny synthetic NIF files packed into synthetic BSAs.

The NIF builder follows docs/spec-notes/nif.md sections 2 and 3; no game data is used.

    python3 -m unittest discover -s tools/capture/tests -v
"""
import json
import os
import struct
import subprocess
import sys
import unittest

CAPTURE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, CAPTURE_DIR)
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import oracle_nif  # noqa: E402
from test_oracles import TempDirCase, build_bsa  # noqa: E402


def f32(x):
    """The value a float becomes after a round trip through f32."""
    return struct.unpack("<f", struct.pack("<f", x))[0]


def sized(s: bytes) -> bytes:
    return struct.pack("<I", len(s)) + s


def build_nif(blocks, strings=(), version=0x14020007, user_version=11, bs_version=34,
              roots=(0,), block_sizes=True, string_table=True):
    """blocks: list of (type name, payload bytes)."""
    vtext = ".".join(str((version >> s) & 0xFF) for s in (24, 16, 8, 0))
    out = bytearray(f"Gamebryo File Format, Version {vtext}\n".encode())
    out += struct.pack("<IBII", version, 1, user_version, len(blocks))
    out += struct.pack("<I", bs_version)
    for text in (b"artist\0", b"\0", b" PE Static Art\0"):
        out += bytes([len(text)]) + text
    types = []
    for name, _ in blocks:
        if name not in types:
            types.append(name)
    out += struct.pack("<H", len(types))
    for name in types:
        out += sized(name.encode())
    for name, _ in blocks:
        out += struct.pack("<H", types.index(name))
    if block_sizes:
        for _, payload in blocks:
            out += struct.pack("<I", len(payload))
    if string_table:
        out += struct.pack("<II", len(strings), max((len(s) for s in strings), default=0))
        for s in strings:
            out += sized(s.encode())
    out += struct.pack("<I", 0)  # groups
    for _, payload in blocks:
        out += payload
    out += struct.pack("<I", len(roots)) + struct.pack(f"<{len(roots)}i", *roots)
    return bytes(out)


def refs(values):
    return struct.pack("<I", len(values)) + struct.pack(f"<{len(values)}i", *values)


def av_object(name_index, translation=(0, 0, 0), rotation=(1, 0, 0, 0, 1, 0, 0, 0, 1),
              scale=1.0, bs_version=34, extra=(), props=()):
    out = struct.pack("<I", name_index) + refs(extra) + struct.pack("<i", -1)
    out += struct.pack("<I", 14) if bs_version > 26 else struct.pack("<H", 14)
    out += struct.pack("<3f", *translation) + struct.pack("<9f", *rotation)
    out += struct.pack("<f", scale) + refs(props) + struct.pack("<i", -1)
    return out


def node(name_index, children, effects=(), **kw):
    return av_object(name_index, **kw) + refs(children) + refs(effects)


def tri_shape(name_index, data_ref, **kw):
    # scene object + data ref + skin ref + some material bytes (not parsed by the oracle)
    return av_object(name_index, **kw) + struct.pack("<ii", data_ref, -1) + b"\x00" * 9


def geometry_data(vertices, flags=0x1001, normals=True, colors=False, uv_sets=1):
    n = len(vertices)
    out = struct.pack("<iHBB", 0, n, 0, 0)
    out += b"\x01" + b"".join(struct.pack("<3f", *v) for v in vertices) if vertices else b"\x00"
    out += struct.pack("<H", flags)
    out += b"\x01" if normals else b"\x00"
    if normals:
        out += struct.pack("<3f", 0, 0, 1) * n
        if flags & 0x1000:
            out += struct.pack("<3f", 1, 0, 0) * n * 2
    out += struct.pack("<4f", 0.5, 0.25, 0.125, 2.5)
    out += b"\x01" if colors else b"\x00"
    if colors:
        out += struct.pack("<4f", 1, 1, 1, 1) * n
    out += struct.pack("<2f", 0.5, 0.5) * n * uv_sets
    out += struct.pack("<Hi", 0, -1)
    return out


def tri_shape_data(vertices, triangles, has_triangles=True, match_groups=((0, 1),), **kw):
    out = geometry_data(vertices, **kw) + struct.pack("<H", len(triangles))
    out += struct.pack("<I", 3 * len(triangles))
    out += b"\x01" if has_triangles else b"\x00"
    if has_triangles:
        out += b"".join(struct.pack("<3H", *t) for t in triangles)
    out += struct.pack("<H", len(match_groups))
    for g in match_groups:
        out += struct.pack("<H", len(g)) + struct.pack(f"<{len(g)}H", *g)
    return out


def tri_strips_data(vertices, strips, has_points=True, **kw):
    tri_count = sum(max(len(s) - 2, 0) for s in strips)
    out = geometry_data(vertices, **kw) + struct.pack("<H", tri_count)
    out += struct.pack("<H", len(strips)) + b"".join(struct.pack("<H", len(s)) for s in strips)
    out += b"\x01" if has_points else b"\x00"
    if has_points:
        out += b"".join(struct.pack(f"<{len(s)}H", *s) for s in strips)
    return out


VERTS = [(-1.0, 2.0, 0.1), (3.0, -4.0, 0.2), (0.5, 0.5, -7.3), (2.0, 2.0, 2.0)]


def sample_mesh():
    """BSFadeNode root with a child NiNode, a NiTriShape, a NiTriStrips, an orphan
    BSSegmentedTriShape, their data blocks and a property block that is not analysed."""
    strings = ["Root", "Sub", "Shape", "Strips"]
    blocks = [
        ("BSFadeNode", node(0, [1, 2, -1], translation=(1.5, -2.0, 3.25), scale=0.5)),  # 0
        ("NiNode", node(1, [3, 2])),                                                    # 1
        ("NiTriShape", tri_shape(2, 4)),                                                # 2
        ("NiTriStrips", tri_shape(3, 5)),                                               # 3
        ("NiTriShapeData", tri_shape_data(VERTS, [(0, 1, 2), (1, 2, 3)])),              # 4
        ("NiTriStripsData", tri_strips_data(VERTS, [[0, 1, 2, 3], [3, 3], [0, 1, 2]],
                                            flags=0x0, colors=True, uv_sets=0)),        # 5
        ("BSShaderPPLightingProperty", b"\x00" * 13),                                   # 6
        ("BSSegmentedTriShape", tri_shape(0xFFFFFFFF, -1, rotation=(0, 1, 0, -1, 0, 0, 0, 0, 1))),  # 7
        ("NiTriShapeData", tri_shape_data([], [], has_triangles=False, match_groups=(),
                                          normals=False, uv_sets=0, flags=0)),         # 8
    ]
    return build_nif(blocks, strings), blocks


class NifStatsTest(TempDirCase):
    def test_header_stats(self):
        data, blocks = sample_mesh()
        e = oracle_nif.stats_entry("meshes\\x.nif", data)
        self.assertIsNone(e["error"])
        self.assertEqual(e["version"], "20.2.0.7")
        self.assertEqual((e["user_version"], e["bs_version"], e["num_blocks"], e["strings"]),
                         (11, 34, len(blocks), 4))
        self.assertEqual(list(e["block_types"]), sorted(e["block_types"]))
        self.assertEqual(e["block_types"]["NiTriShapeData"], 2)
        self.assertEqual(e["block_types"]["BSFadeNode"], 1)

    def test_old_version_has_no_string_count(self):
        data = build_nif([("NiControllerSequence", b"\x00" * 5)], version=0x14000004,
                         user_version=10, bs_version=11, block_sizes=False, string_table=False)
        e = oracle_nif.stats_entry("a.kf", data)
        self.assertIsNone(e["error"])
        self.assertEqual((e["version"], e["user_version"], e["bs_version"], e["strings"]),
                         ("20.0.0.4", 10, 11, None))
        self.assertEqual(e["block_types"], {"NiControllerSequence": 1})

    def test_bad_header(self):
        for data in (b"", b"not a nif\n", sample_mesh()[0][:60]):
            e = oracle_nif.stats_entry("b.nif", data)
            self.assertIsNotNone(e["error"])
            self.assertTrue(all(e[k] is None for k in e if k not in ("path", "error")))


class NifSceneTest(TempDirCase):
    def test_scene(self):
        data, _ = sample_mesh()
        status, objects, shapes = oracle_nif.scene_file("meshes\\x.nif", data)
        self.assertEqual(status, "ok")
        by_key = {o["key"]: o for o in objects}
        self.assertEqual([o["key"] for o in objects],
                         [f"meshes\\x.nif#{i}" for i in (0, 1, 2, 3, 7)])
        root = by_key["meshes\\x.nif#0"]
        self.assertEqual((root["type"], root["name"], root["parent"]), ("BSFadeNode", "Root", None))
        self.assertEqual(root["translation"], [1.5, -2.0, 3.25])
        self.assertEqual(root["scale"], 0.5)
        self.assertEqual(root["rotation"], [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0])
        self.assertEqual(by_key["meshes\\x.nif#1"]["parent"], 0)
        self.assertEqual(by_key["meshes\\x.nif#2"]["parent"], 0)  # lowest index wins over 1
        self.assertEqual(by_key["meshes\\x.nif#3"]["parent"], 1)
        orphan = by_key["meshes\\x.nif#7"]
        self.assertEqual((orphan["type"], orphan["name"], orphan["parent"]),
                         ("BSSegmentedTriShape", None, None))
        self.assertEqual(orphan["rotation"][:4], [0.0, 1.0, 0.0, -1.0])

        s = {x["key"]: x for x in shapes}
        lst = s["meshes\\x.nif#4"]
        self.assertEqual(lst["type"], "NiTriShapeData")
        self.assertEqual((lst["vertices"], lst["triangles"], lst["has_normals"], lst["has_tangents"],
                          lst["uv_sets"], lst["has_vertex_colors"]), (4, 2, True, True, 1, False))
        self.assertEqual(lst["aabb_min"], [-1.0, -4.0, f32(-7.3)])
        self.assertEqual(lst["aabb_max"], [3.0, 2.0, 2.0])
        self.assertEqual(lst["bound_center"], [0.5, 0.25, 0.125])
        self.assertEqual(lst["bound_radius"], 2.5)
        strips = s["meshes\\x.nif#5"]
        self.assertEqual((strips["type"], strips["triangles"], strips["has_tangents"],
                          strips["uv_sets"], strips["has_vertex_colors"]),
                         ("NiTriStripsData", 2 + 0 + 1, False, 0, True))
        empty = s["meshes\\x.nif#8"]
        self.assertEqual((empty["vertices"], empty["triangles"], empty["has_normals"]), (0, 0, False))
        self.assertIsNone(empty["aabb_min"])
        self.assertIsNone(empty["aabb_max"])

    def test_old_stream_version_u16_flags(self):
        blocks = [("NiNode", node(0, [1], bs_version=26)),
                  ("NiTriShape", tri_shape(0, -1, bs_version=26, translation=(7, 8, 9)))]
        data = build_nif(blocks, ["n"], bs_version=26)
        status, objects, shapes = oracle_nif.scene_file("o.nif", data)
        self.assertEqual(status, "ok")
        self.assertEqual(objects[1]["translation"], [7.0, 8.0, 9.0])
        self.assertEqual(objects[1]["parent"], 0)

    def test_non_finite_floats_become_null(self):
        nan = float("nan")
        blocks = [("NiNode", node(0, [], translation=(nan, 1.0, float("inf"))))]
        status, objects, _ = oracle_nif.scene_file("n.nif", build_nif(blocks, ["n"]))
        self.assertEqual(status, "ok")
        self.assertEqual(objects[0]["translation"], [None, 1.0, None])
        self.assertNotIn("NaN", oracle_nif.render({"objects": objects}, ("objects",)))

    def test_strips_without_points(self):
        blocks = [("NiTriStripsData", tri_strips_data(VERTS, [[0, 1, 2, 3]], has_points=False))]
        status, _, shapes = oracle_nif.scene_file("s.nif", build_nif(blocks))
        self.assertEqual(status, "ok")
        self.assertEqual(shapes[0]["triangles"], 0)

    def test_skipped(self):
        old = build_nif([("NiNode", b"\x00" * 4)], version=0x14000004, bs_version=11,
                        block_sizes=False, string_table=False)
        self.assertEqual(oracle_nif.scene_file("t.nif", old),
                         ("skipped", "no block-size table (version 20.0.0.4)"))
        data, _ = sample_mesh()
        status, reason = oracle_nif.scene_file("t.nif", data[:-10])
        self.assertEqual(status, "skipped")
        self.assertTrue(reason.startswith("container:"))
        # A data block with a byte too many does not decode exactly.
        bad = build_nif([("NiTriShapeData", tri_shape_data(VERTS, [(0, 1, 2)]) + b"\x00")])
        status, reason = oracle_nif.scene_file("t.nif", bad)
        self.assertEqual(status, "skipped")
        self.assertIn("block 0 (NiTriShapeData)", reason)

    def test_archive_documents_and_cli(self):
        mesh, _ = sample_mesh()
        old = build_nif([("NiNode", b"\x00" * 4)], version=0x14000004, bs_version=11,
                        block_sizes=False, string_table=False)
        archive = build_bsa([
            ("meshes\\a", "x.nif", mesh, "zlib"),
            ("meshes\\a", "y.kf", old, "raw"),
            ("meshes\\a", "readme.txt", b"hello", "raw"),
            ("meshes\\b", "z.nif", b"garbage", "raw"),
        ], archive_flags=0x83)
        path = self.write("Test.bsa", archive)
        script = os.path.join(CAPTURE_DIR, "oracle_nif.py")
        out = os.path.join(self.tmp, "stats.json")
        r = subprocess.run([sys.executable, script, "stats", path, "--out", out], capture_output=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        with open(out, encoding="utf-8") as f:
            stats = json.load(f)
        self.assertEqual(stats["schema"], "fnv.nif.stats/1")
        self.assertEqual(stats["archive"], "Test.bsa")
        paths = [e["path"] for e in stats["files"]]
        self.assertEqual(sorted(paths), ["meshes\\a\\x.nif", "meshes\\a\\y.kf", "meshes\\b\\z.nif"])
        by_path = {e["path"]: e for e in stats["files"]}
        self.assertIsNotNone(by_path["meshes\\b\\z.nif"]["error"])
        self.assertEqual(by_path["meshes\\a\\x.nif"]["num_blocks"], 9)

        r = subprocess.run([sys.executable, script, "scene", path], capture_output=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        scene = json.loads(r.stdout)
        self.assertEqual(list(scene), ["schema", "archive", "skipped", "objects", "shapes"])
        self.assertEqual({s["path"] for s in scene["skipped"]},
                         {"meshes\\a\\y.kf", "meshes\\b\\z.nif"})
        self.assertEqual(len(scene["objects"]), 5)
        self.assertEqual(len(scene["shapes"]), 3)
        self.assertTrue(scene["objects"][0]["key"].startswith("meshes\\a\\x.nif#"))

        r = subprocess.run([sys.executable, script, "stats", self.write("bad.bsa", b"nope")],
                           capture_output=True)
        self.assertEqual(r.returncode, 1)
        r = subprocess.run([sys.executable, script], capture_output=True)
        self.assertEqual(r.returncode, 2)

    def test_embedded_path_prefix(self):
        """Archive flag 0x100: each file's data starts with its u8-length path."""
        mesh, _ = sample_mesh()
        full = b"meshes\\a\\x.nif"
        archive = build_bsa([("meshes\\a", "x.nif", bytes([len(full)]) + full + mesh, "raw")],
                            archive_flags=0x103)
        doc = oracle_nif.stats_document(archive, "E.bsa")
        self.assertIsNone(doc["files"][0]["error"])
        self.assertEqual(doc["files"][0]["num_blocks"], 9)


if __name__ == "__main__":
    unittest.main()
