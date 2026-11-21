"""Tests for oracle_collide.py using tiny synthetic NIF files packed into synthetic BSAs.

Block payloads are built from the layouts listed in oracle_collide.py's docstring
(docs/spec-notes/collision.md, nif.md, nif.xml as documentation); no game data is used.

    python3 -m unittest discover -s tools/capture/tests -v
"""
import math
import os
import struct
import sys
import unittest

CAPTURE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, CAPTURE_DIR)
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import oracle_collide  # noqa: E402
from test_oracle_nif import build_nif, refs, tri_strips_data  # noqa: E402
from test_oracles import TempDirCase, build_bsa  # noqa: E402

PATH = "meshes\\test\\thing.nif"
ROT_Z90 = (0, -1, 0, 1, 0, 0, 0, 0, 1)  # row by row: x -> y, y -> -x
IDENTITY16 = (1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1)


# --------------------------------------------------------------------------- block builders

def collision_object(target, body, blend=False):
    out = struct.pack("<iHi", target, 1, body)
    return out + (struct.pack("<2f", 1.0, 1.0) if blend else b"")


def world_head(shape, layer):
    return struct.pack("<iBBH", shape, layer, 0, 0) + bytes(20)


def rigid_body(shape, layer=1, translation=(0, 0, 0, 0), rotation=(0, 0, 0, 1), mass=0.0,
               friction=0.5, restitution=0.4, motion=7, constraints=()):
    out = world_head(shape, layer) + struct.pack("<BBH", 0, 0, 0xFFFF)
    info = bytes(4) + struct.pack("<BBH", layer, 0, 0) + bytes(4)
    info += struct.pack("<BBH", 0, 0, 0xFFFF) + bytes(4)
    info += struct.pack("<4f", *translation) + struct.pack("<4f", *rotation)
    info += bytes(16 + 16 + 48 + 16)
    info += struct.pack("<8f", mass, 0.1, 0.05, friction, restitution, 104.4, 31.57, 0.15)
    info += struct.pack("<4B", motion, 1, 1, 1) + bytes(12)
    assert len(info) == 196
    return out + info + refs(constraints) + struct.pack("<I", 0)


def phantom(shape):
    return world_head(shape, 12) + bytes(8) + struct.pack("<16f", *IDENTITY16)


def box(half):
    return struct.pack("<If", 0, 0.1) + bytes(8) + struct.pack("<3f", *half) + struct.pack("<f", 0)


def sphere(radius):
    return struct.pack("<If", 0, radius)


def capsule(p1, p2, radius):
    return (struct.pack("<If", 0, radius) + bytes(8)
            + struct.pack("<3ff3ff", *p1, radius, *p2, radius))


def convex(points):
    out = struct.pack("<If", 0, 0.1) + bytes(24) + struct.pack("<I", len(points))
    out += b"".join(struct.pack("<4f", *p, 0.0) for p in points)
    return out + struct.pack("<I", 1) + struct.pack("<4f", 0, 0, 1, -1)


def column_matrix(rotation_rows, translation, e15=1.0):
    """16 floats: rotation element (row, col) at 4*col + row, translation at 12..14."""
    e = [0.0] * 16
    for row in range(3):
        for col in range(3):
            e[4 * col + row] = rotation_rows[3 * row + col]
    e[12:15] = translation
    e[15] = e15
    return e


def transform_shape(child, rotation_rows, translation, e15=1.0):
    return (struct.pack("<iIf", child, 0, 0.1) + bytes(8)
            + struct.pack("<16f", *column_matrix(rotation_rows, translation, e15)))


def list_shape(children):
    return (refs(children) + struct.pack("<I", 0) + bytes(24)
            + struct.pack("<I", len(children)) + bytes(4 * len(children)))


def mopp(child, code=b"\x01\x02\x03"):
    return (struct.pack("<i", child) + bytes(12) + struct.pack("<f", 1.0)
            + struct.pack("<I", len(code)) + struct.pack("<4f", 0, 0, 0, 1) + code)


def packed_shape(data, scale=1.0):
    return (struct.pack("<I", 0) + bytes(4) + struct.pack("<f", 0.1) + bytes(4)
            + struct.pack("<4f", scale, scale, scale, 0) + struct.pack("<f", 0.1)
            + struct.pack("<4f", scale, scale, scale, 0) + struct.pack("<i", data))


def packed_data(vertices, triangles, sub_parts, compressed=False):
    out = struct.pack("<I", len(triangles))
    out += b"".join(struct.pack("<4H", *t, 0) for t in triangles)
    out += struct.pack("<IB", len(vertices), 1 if compressed else 0)
    if compressed:
        out += b"".join(struct.pack("<3H", 0, 0, 0) for _ in vertices)
    else:
        out += b"".join(struct.pack("<3f", *v) for v in vertices)
    out += struct.pack("<H", len(sub_parts))
    out += b"".join(struct.pack("<III", 1, n, 5) for n in sub_parts)
    return out


def multi_sphere(spheres):
    out = struct.pack("<I", 0) + bytes(12) + struct.pack("<I", len(spheres))
    return out + b"".join(struct.pack("<4f", *c, r) for c, r in spheres)


def strips_shape(strips):
    return (struct.pack("<If", 0, 0.1) + bytes(20) + struct.pack("<I", 1)
            + struct.pack("<4f", 1, 1, 1, 0) + refs(strips)
            + struct.pack("<I", len(strips)) + bytes(4 * len(strips)))


def av_object(name_index, collision=-1, translation=(0, 0, 0), rotation=(1, 0, 0, 0, 1, 0, 0, 0, 1),
              scale=1.0, bs_version=34):
    out = struct.pack("<I", name_index) + refs(()) + struct.pack("<i", -1)
    out += struct.pack("<I", 14) if bs_version > 26 else struct.pack("<H", 14)
    out += struct.pack("<3f", *translation) + struct.pack("<9f", *rotation)
    out += struct.pack("<f", scale) + refs(()) + struct.pack("<i", collision)
    return out


def cnode(name_index, children=(), collision=-1, **kw):
    return av_object(name_index, collision, **kw) + refs(children) + refs(())


def f32(x):
    return struct.unpack("<f", struct.pack("<f", x))[0]


def one_body(shape_blocks, body_type="bhkRigidBody", body_kw=None, root_kw=None, extra=()):
    """Root BSFadeNode (block 0) with collision object 1 -> body 2 -> shape tree from 3.

    shape_blocks: list of (type, payload) starting at block 3 (the body's shape is 3)."""
    blocks = [
        ("BSFadeNode", cnode(0, collision=1, **(root_kw or {}))),
        ("bhkCollisionObject", collision_object(0, 2)),
        (body_type, rigid_body(3, **(body_kw or {}))),
    ] + list(shape_blocks) + list(extra)
    return build_nif(blocks, ["Root"])


class CollideTest(TempDirCase):
    def run_file(self, data, path=PATH):
        result = oracle_collide.collide_file(path, data)
        self.assertEqual(result[0], "ok", result)
        return result[1], result[2]

    def assertVec(self, got, want, places=4):
        self.assertEqual(len(got), len(want))
        for g, w in zip(got, want):
            self.assertAlmostEqual(g, w, places=places)

    # ------------------------------------------------------------------ root, body fields

    def test_box_on_root_and_body_fields(self):
        data = one_body([("bhkBoxShape", box((1.0, 2.0, 3.0)))],
                        body_kw={"layer": 10, "mass": 20.0, "friction": 0.25,
                                 "restitution": 0.75, "motion": 4,
                                 "translation": (100, 100, 100, 0),
                                 "rotation": (0.0, 0.0, 0.7071068, 0.7071068)})
        objects, phantoms = self.run_file(data)
        self.assertEqual(phantoms, [])
        self.assertEqual(len(objects), 1)
        o = objects[0]
        self.assertEqual(o["key"], PATH + "#1")
        self.assertEqual((o["node"], o["node_block"], o["object_type"], o["body_type"]),
                         ("Root", 0, "bhkCollisionObject", "bhkRigidBody"))
        self.assertEqual((o["motion"], o["layer"]), ("dynamic", 10))
        self.assertEqual((o["mass"], o["friction"], o["restitution"]),
                         (20.0, 0.25, f32(0.75)))
        self.assertEqual(o["shapes"], {"box": 1})
        self.assertEqual((o["hull_points"], o["mesh_vertices"], o["mesh_triangles"]), (0, 0, 0))
        # bhkRigidBody: the stored transform is not applied.
        self.assertVec(o["aabb_min"], [-7, -14, -21])
        self.assertVec(o["aabb_max"], [7, 14, 21])

    def test_motion_mapping(self):
        for code, want in ((7, "static"), (6, "keyframed"), (2, "dynamic"), (8, "dynamic"),
                           (1, "dynamic")):
            data = one_body([("bhkSphereShape", sphere(1.0))], body_kw={"motion": code})
            self.assertEqual(self.run_file(data)[0][0]["motion"], want)

    def test_rigid_body_t_applies_body_transform(self):
        q = (0.0, 0.0, math.sin(math.pi / 4), math.cos(math.pi / 4))  # 90 deg about z
        kw = {"translation": (1.0, 2.0, 3.0, 0.0), "rotation": q}
        shape = [("bhkBoxShape", box((1.0, 0.5, 0.25)))]
        plain = self.run_file(one_body(shape, "bhkRigidBody", kw))[0][0]
        self.assertVec(plain["aabb_min"], [-7, -3.5, -1.75])
        self.assertVec(plain["aabb_max"], [7, 3.5, 1.75])
        t = self.run_file(one_body(shape, "bhkRigidBodyT", kw))[0][0]
        self.assertEqual(t["body_type"], "bhkRigidBodyT")
        self.assertVec(t["aabb_min"], [7 * (1 - 0.5), 7 * (2 - 1), 7 * (3 - 0.25)])
        self.assertVec(t["aabb_max"], [7 * (1 + 0.5), 7 * (2 + 1), 7 * (3 + 0.25)])

    def test_rigid_body_t_rotation_direction(self):
        # A single hull point (1, 0, 0) rotated +90 deg about z lands on +y.
        q = (0.0, 0.0, math.sin(math.pi / 4), math.cos(math.pi / 4))
        o = self.run_file(one_body([("bhkConvexVerticesShape", convex([(1.0, 0.0, 0.0)]))],
                                   "bhkRigidBodyT", {"rotation": q}))[0][0]
        self.assertVec(o["aabb_min"], [0, 7, 0])
        self.assertVec(o["aabb_max"], [0, 7, 0])

    # ------------------------------------------------------------------ node transforms

    def test_collision_on_child_node_with_transform_and_scale(self):
        q = (0.0, 0.0, 0.0, 1.0)
        blocks = [
            ("BSFadeNode", cnode(0, children=[1])),
            ("NiNode", cnode(1, collision=2, translation=(10, 0, 0), rotation=ROT_Z90,
                             scale=2.0)),
            ("bhkCollisionObject", collision_object(1, 3)),
            ("bhkRigidBodyT", rigid_body(4, translation=(1, 0, 0, 0), rotation=q)),
            ("bhkSphereShape", sphere(0.5)),
        ]
        o = self.run_file(build_nif(blocks, ["Root", "Child"]))[0][0]
        self.assertEqual((o["key"], o["node"], o["node_block"]), (PATH + "#2", "Child", 1))
        self.assertEqual(o["shapes"], {"sphere": 1})
        # centre: body (1,0,0) *7 *2 -> (14,0,0), rotate -> (0,14,0), + (10,0,0); r = 0.5*7*2
        self.assertVec(o["aabb_min"], [3, 7, -7])
        self.assertVec(o["aabb_max"], [17, 21, 7])

    def test_accumulated_transform_below_the_root(self):
        # The root's own transform (translation, scale) is not applied.
        blocks = [
            ("BSFadeNode", cnode(0, children=[1], translation=(0, 0, 5), scale=0.5)),
            ("NiNode", cnode(1, children=[2], translation=(2, 0, 0), scale=3.0)),
            ("NiNode", cnode(2, collision=3, translation=(0, 1, 0))),
            ("bhkCollisionObject", collision_object(2, 4)),
            ("bhkRigidBody", rigid_body(5)),
            ("bhkConvexVerticesShape", convex([(1, 0, 0), (0, 0, 2)])),
        ]
        o = self.run_file(build_nif(blocks, ["Root", "A", "B"]))[0][0]
        self.assertEqual((o["hull_points"], o["shapes"]), (2, {"convex_hull": 1}))
        # p -> 7p, then B: +(0,1,0); A: *3 +(2,0,0); root: not applied
        def place(p):
            x, y, z = (7 * c for c in p)
            y += 1
            return 3 * x + 2, 3 * y, 3 * z
        a, b = place((1, 0, 0)), place((0, 0, 2))
        self.assertVec(o["aabb_min"], [min(a[k], b[k]) for k in range(3)])
        self.assertVec(o["aabb_max"], [max(a[k], b[k]) for k in range(3)])

    def test_capsule_radius_scales_with_node_scale(self):
        blocks = [
            ("BSFadeNode", cnode(0, children=[1])),
            ("NiNode", cnode(1, collision=2, scale=2.0)),
            ("bhkCollisionObject", collision_object(1, 3)),
            ("bhkRigidBody", rigid_body(4)),
            ("bhkCapsuleShape", capsule((0, 0, -1), (0, 0, 1), 0.5)),
        ]
        o = self.run_file(build_nif(blocks, ["Root", "N"]))[0][0]
        self.assertEqual(o["shapes"], {"capsule": 1})
        self.assertVec(o["aabb_min"], [-7, -7, -21])
        self.assertVec(o["aabb_max"], [7, 7, 21])

    def test_lowest_index_parent_wins(self):
        # Node 2 is listed by nodes 0 and 1; the parent is 0 (identity), not 1 (offset).
        blocks = [
            ("BSFadeNode", cnode(0, children=[1, 2])),
            ("NiNode", cnode(1, children=[2], translation=(100, 0, 0))),
            ("NiNode", cnode(1, collision=3)),
            ("bhkCollisionObject", collision_object(2, 4)),
            ("bhkRigidBody", rigid_body(5)),
            ("bhkSphereShape", sphere(1.0)),
        ]
        o = self.run_file(build_nif(blocks, ["Root", "N"]))[0][0]
        self.assertVec(o["aabb_min"], [-7, -7, -7])

    def test_old_stream_version_flags_u16(self):
        blocks = [
            ("NiNode", cnode(0, children=[1], bs_version=25)),
            ("NiNode", cnode(1, collision=2, bs_version=25, translation=(1, 2, 3))),
            ("bhkCollisionObject", collision_object(1, 3)),
            ("bhkRigidBody", rigid_body(4)),
            ("bhkSphereShape", sphere(1.0)),
        ]
        o = self.run_file(build_nif(blocks, ["Root", "N"], bs_version=25))[0][0]
        self.assertVec(o["aabb_min"], [-6, -5, -4])

    def test_root_transform_not_applied(self):
        root_kw = {"translation": (100, 200, 300), "rotation": ROT_Z90, "scale": 3.0}
        on_root = self.run_file(one_body([("bhkBoxShape", box((1.0, 2.0, 3.0)))],
                                         root_kw=root_kw))[0][0]
        self.assertVec(on_root["aabb_min"], [-7, -14, -21])
        self.assertVec(on_root["aabb_max"], [7, 14, 21])
        # Below a rotated root: the child's own transform applies, the root's does not.
        blocks = [
            ("BSFadeNode", cnode(0, children=[1], **root_kw)),
            ("NiNode", cnode(1, collision=2, translation=(10, 0, 0), rotation=ROT_Z90)),
            ("bhkCollisionObject", collision_object(1, 3)),
            ("bhkRigidBody", rigid_body(4)),
            ("bhkConvexVerticesShape", convex([(1.0, 0.0, 0.0)])),
        ]
        o = self.run_file(build_nif(blocks, ["Root", "N"]))[0][0]
        self.assertVec(o["aabb_min"], [10, 7, 0])
        self.assertVec(o["aabb_max"], [10, 7, 0])

    def test_object_outside_the_root_composes_its_chain(self):
        blocks = [
            ("BSFadeNode", cnode(0, translation=(50, 0, 0))),
            ("NiNode", cnode(1, children=[2], translation=(0, 0, 4))),   # not under the root
            ("NiNode", cnode(1, collision=3, translation=(1, 0, 0))),
            ("bhkCollisionObject", collision_object(2, 4)),
            ("bhkRigidBody", rigid_body(5)),
            ("bhkConvexVerticesShape", convex([(0.0, 0.0, 0.0)])),
        ]
        o = self.run_file(build_nif(blocks, ["Root", "N"]))[0][0]
        self.assertVec(o["aabb_min"], [1, 0, 4])

    def test_collision_on_geometry_object(self):
        blocks = [
            ("BSFadeNode", cnode(0, children=[1])),
            ("NiTriShape", av_object(1, collision=2, translation=(0, 0, 1))
             + struct.pack("<ii", -1, -1) + b"\x00" * 9),
            ("bhkCollisionObject", collision_object(1, 3)),
            ("bhkRigidBody", rigid_body(4)),
            ("bhkSphereShape", sphere(1.0)),
        ]
        o = self.run_file(build_nif(blocks, ["Root", "Shape"]))[0][0]
        self.assertEqual((o["node"], o["node_block"]), ("Shape", 1))
        self.assertVec(o["aabb_max"], [7, 7, 8])

    # ------------------------------------------------------------------ shape trees

    def test_list_with_transform_and_convex_transform(self):
        shapes = [
            ("bhkListShape", list_shape([4, 5, 7])),                                   # 3
            ("bhkBoxShape", box((0.5, 0.5, 0.5))),                                     # 4
            ("bhkTransformShape", transform_shape(6, ROT_Z90, (5, 0, 0), e15=0.0)),    # 5
            ("bhkConvexVerticesShape", convex([(1, 0, 0)])),                           # 6
            ("bhkConvexTransformShape", transform_shape(8, (1, 0, 0, 0, 1, 0, 0, 0, 1),
                                                        (0, 0, -3), e15=0.37)),       # 7
            ("bhkSphereShape", sphere(0.25)),                                          # 8
        ]
        o = self.run_file(one_body(shapes))[0][0]
        self.assertEqual(o["shapes"], {"box": 1, "convex_hull": 1, "sphere": 1})
        self.assertEqual(o["hull_points"], 1)
        # hull point (1,0,0) rotated +90 deg about z (column reading) -> (0,1,0) + (5,0,0)
        # sphere at (0,0,-3) r 0.25; box +-0.5
        self.assertVec(o["aabb_min"], [-3.5, -3.5, 7 * -3.25])
        self.assertVec(o["aabb_max"], [35, 7, 3.5])

    def test_transform_matrix_scale_applies_to_points_and_radii(self):
        third = 1.0 / 7.0
        shapes = [
            ("bhkTransformShape", transform_shape(4, (third, 0, 0, 0, third, 0, 0, 0, third),
                                                  (1, 0, 0))),
            ("bhkMultiSphereShape", multi_sphere([((7, 0, 0), 0.5), ((-7, 0, 0), 1.0)])),
        ]
        o = self.run_file(one_body(shapes))[0][0]
        self.assertEqual(o["shapes"], {"sphere": 2})
        # centres (7,0,0)/7 + (1,0,0) = (2,0,0) and (0,0,0), *7 -> 14 and 0;
        # radii 0.5*7/7 and 1*7/7
        self.assertVec(o["aabb_min"], [-1, -1, -1])
        self.assertVec(o["aabb_max"], [14.5, 1, 1])

    def test_matrix_not_rotation_times_uniform_scale_is_skipped(self):
        for rows in ((2, 0, 0, 0, 1, 0, 0, 0, 1),        # non-uniform scale
                     (1, 0.5, 0, 0, 1, 0, 0, 0, 1),      # shear
                     (-1, 0, 0, 0, 1, 0, 0, 0, 1),       # mirror
                     (0, 0, 0, 0, 0, 0, 0, 0, 0)):       # degenerate
            data = one_body([("bhkTransformShape", transform_shape(4, rows, (0, 0, 0))),
                             ("bhkSphereShape", sphere(1.0))])
            result = oracle_collide.collide_file(PATH, data)
            self.assertEqual(result[0], "skipped", rows)
            self.assertIn("rotation x uniform scale", result[1])

    def test_mopp_over_packed_strips_with_sub_parts(self):
        verts = [(0, 0, 0), (1, 0, 0), (0, 1, 0), (2, 2, 2), (3, 2, 2), (2, 3, -1)]
        tris = [(0, 1, 2), (2, 1, 0), (3, 4, 5), (5, 4, 3)]
        shapes = [
            ("bhkMoppBvTreeShape", mopp(4)),
            ("bhkPackedNiTriStripsShape", packed_shape(5, scale=2.0)),  # scale not applied
            ("hkPackedNiTriStripsData", packed_data(verts, tris, [3, 3])),
        ]
        o = self.run_file(one_body(shapes))[0][0]
        self.assertEqual(o["shapes"], {"tri_mesh": 1})
        self.assertEqual((o["mesh_vertices"], o["mesh_triangles"], o["hull_points"]), (6, 4, 0))
        self.assertVec(o["aabb_min"], [0, 0, -7])
        self.assertVec(o["aabb_max"], [21, 21, 14])

    def test_shared_shape_counted_per_reference(self):
        shapes = [("bhkListShape", list_shape([4, 4])), ("bhkBoxShape", box((1, 1, 1)))]
        o = self.run_file(one_body(shapes))[0][0]
        self.assertEqual(o["shapes"], {"box": 2})

    def test_ni_tri_strips_shape(self):
        verts = [(-1.0, 2.0, 0.1), (3.0, -4.0, 0.2), (0.5, 0.5, -7.3), (2.0, 2.0, 2.0)]
        shapes = [
            ("bhkNiTriStripsShape", strips_shape([4, 5])),
            ("NiTriStripsData", tri_strips_data(verts, [[0, 1, 2, 3], [3, 3, 0, 1, 1]])),
            ("NiTriStripsData", tri_strips_data(verts[:3], [[0, 1, 2]], flags=0, uv_sets=0)),
        ]
        o = self.run_file(one_body(shapes))[0][0]
        self.assertEqual(o["shapes"], {"tri_mesh": 1})
        # strip [0,1,2,3]: 2; strip [3,3,0,1,1]: only (3,0,1) is not degenerate; [0,1,2]: 1
        self.assertEqual((o["mesh_vertices"], o["mesh_triangles"]), (7, 4))
        self.assertVec(o["aabb_min"], [-7, f32(-4.0) * 7, f32(-7.3) * 7])
        self.assertVec(o["aabb_max"], [21, 14, 14])

    def test_empty_list_has_null_bounds(self):
        o = self.run_file(one_body([("bhkListShape", list_shape([]))]))[0][0]
        self.assertEqual(o["shapes"], {})
        self.assertIsNone(o["aabb_min"])
        self.assertIsNone(o["aabb_max"])

    # ------------------------------------------------------------------ objects and phantoms

    def test_blend_object_and_phantom(self):
        blocks = [
            ("BSFadeNode", cnode(0, children=[1, 2], collision=-1)),
            ("NiNode", cnode(1, collision=3)),
            ("NiNode", cnode(2, collision=5)),
            ("bhkBlendCollisionObject", collision_object(1, 4, blend=True)),
            ("bhkRigidBody", rigid_body(7, motion=6, layer=8)),
            ("bhkSPCollisionObject", collision_object(2, 6)),
            ("bhkSimpleShapePhantom", phantom(8)),
            ("bhkSphereShape", sphere(1.0)),
            ("bhkCylinderShape", b"\x00" * 60),  # unsupported, but only under the phantom
        ]
        objects, phantoms = self.run_file(build_nif(blocks, ["Root", "Bone", "Trigger"]))
        self.assertEqual([o["key"] for o in objects], [PATH + "#3"])
        self.assertEqual((objects[0]["object_type"], objects[0]["motion"], objects[0]["layer"]),
                         ("bhkBlendCollisionObject", "keyframed", 8))
        self.assertEqual(phantoms, [{"key": PATH + "#5", "body_type": "bhkSimpleShapePhantom"}])

    def test_rows_in_scene_object_block_order(self):
        blocks = [
            ("BSFadeNode", cnode(0, children=[5, 1])),
            ("NiNode", cnode(1, collision=2)),
            ("bhkCollisionObject", collision_object(1, 3)),
            ("bhkRigidBody", rigid_body(4)),
            ("bhkSphereShape", sphere(1.0)),
            ("NiNode", cnode(1, collision=6)),
            ("bhkCollisionObject", collision_object(5, 7)),
            ("bhkRigidBody", rigid_body(4)),
        ]
        objects, _ = self.run_file(build_nif(blocks, ["Root", "N"]))
        self.assertEqual([(o["key"], o["node_block"]) for o in objects],
                         [(PATH + "#2", 1), (PATH + "#6", 5)])

    def test_unnamed_scene_object(self):
        blocks = [
            ("NiNode", cnode(0xFFFFFFFF, collision=1)),
            ("bhkCollisionObject", collision_object(0, 2)),
            ("bhkRigidBody", rigid_body(3)),
            ("bhkSphereShape", sphere(1.0)),
        ]
        self.assertIsNone(self.run_file(build_nif(blocks, []))[0][0]["node"])

    def test_no_collision(self):
        blocks = [("BSFadeNode", cnode(0, children=[1])), ("NiNode", cnode(0))]
        self.assertEqual(self.run_file(build_nif(blocks, ["Root"])), ([], []))

    # ------------------------------------------------------------------ skipped files

    def assertSkipped(self, data, needle):
        result = oracle_collide.collide_file(PATH, data)
        self.assertEqual(result[0], "skipped", result)
        self.assertIn(needle, result[1])

    def test_skip_reasons(self):
        self.assertSkipped(build_nif([("NiNode", cnode(0))], ["R"], version=0x14000004,
                                     user_version=10, bs_version=11, block_sizes=False,
                                     string_table=False), "no block-size table")
        self.assertSkipped(b"not a nif\n", "container")
        self.assertSkipped(one_body([("bhkCylinderShape", b"\x00" * 60)]),
                           "unsupported shape type bhkCylinderShape")
        self.assertSkipped(one_body([("bhkListShape", list_shape([4, -1])),
                                     ("bhkBoxShape", box((1, 1, 1)))]), "missing block")
        self.assertSkipped(one_body([("bhkMoppBvTreeShape", mopp(4)),
                                     ("bhkPackedNiTriStripsShape", packed_shape(5)),
                                     ("hkPackedNiTriStripsData",
                                      packed_data([(0, 0, 0)] * 3, [(0, 1, 2)], [3],
                                                  compressed=True))]),
                           "compressed packed vertices")
        self.assertSkipped(one_body([("bhkBoxShape", box((1, 1, 1)) + b"\x00")]),
                           "1 bytes left over")
        self.assertSkipped(one_body([("bhkBoxShape", box((1, 1, 1))[:20])]), "needs")
        self.assertSkipped(one_body([("bhkListShape", list_shape([3]))]), "cycle")
        blocks = [("NiNode", cnode(0, collision=1)), ("NiCollisionData", b"\x00" * 10)]
        self.assertSkipped(build_nif(blocks, ["R"]), "unsupported collision object type")
        blocks = [("NiNode", cnode(0, collision=1)),
                  ("bhkCollisionObject", collision_object(0, 7))]
        self.assertSkipped(build_nif(blocks, ["R"]), "missing block")

    def test_skipped_file_contributes_no_phantoms(self):
        blocks = [
            ("BSFadeNode", cnode(0, children=[1, 2])),
            ("NiNode", cnode(0, collision=3)),
            ("NiNode", cnode(0, collision=5)),
            ("bhkSPCollisionObject", collision_object(1, 4)),
            ("bhkSimpleShapePhantom", phantom(7)),
            ("bhkCollisionObject", collision_object(2, 6)),
            ("bhkRigidBody", rigid_body(-1)),
            ("bhkSphereShape", sphere(1.0)),
        ]
        self.assertSkipped(build_nif(blocks, ["R"]), "missing block")

    # ------------------------------------------------------------------ whole archive / CLI

    def test_archive_document_and_filter(self):
        good = one_body([("bhkBoxShape", box((1, 1, 1)))])
        blocks = [
            ("NiNode", cnode(0, collision=1)),
            ("bhkSPCollisionObject", collision_object(0, 2)),
            ("bhkSimpleShapePhantom", phantom(3)),
            ("bhkSphereShape", sphere(1.0)),
        ]
        trigger = build_nif(blocks, ["T"])
        old = build_nif([("NiNode", cnode(0))], ["R"], version=0x14000004, user_version=11,
                        bs_version=11, block_sizes=False, string_table=False)
        archive = build_bsa([
            ("meshes\\b", "good.nif", good, "zlib"),
            ("meshes\\b", "anim.kf", good, "raw"),
            ("meshes\\a", "trigger.nif", trigger, "raw"),
            ("meshes\\a", "old.nif", old, "raw"),
        ], archive_flags=0x3)
        path = self.write("Test - Meshes.bsa", archive)
        rc, doc = self.run_main(oracle_collide, ["objects", path])
        self.assertEqual(rc, 0)
        self.assertEqual(list(doc), ["format", "archive", "scale", "objects", "phantoms",
                                     "skipped"])
        self.assertEqual((doc["format"], doc["archive"], doc["scale"]),
                         ("fnv.collide.objects/1", "Test - Meshes.bsa", 7.0))
        self.assertEqual([o["key"] for o in doc["objects"]], ["meshes\\b\\good.nif#1"])
        self.assertEqual(doc["phantoms"], [{"key": "meshes\\a\\trigger.nif#1",
                                            "body_type": "bhkSimpleShapePhantom"}])
        self.assertEqual([s["path"] for s in doc["skipped"]], ["meshes\\a\\old.nif"])
        rc, doc = self.run_main(oracle_collide, ["objects", path, "--filter", "GOOD"])
        self.assertEqual((len(doc["objects"]), doc["phantoms"], doc["skipped"]), (1, [], []))

    def test_unreadable_archive(self):
        path = self.write("bad.bsa", b"XXXX" + bytes(40))
        self.assertEqual(oracle_collide.main(["objects", path, "-o",
                                              os.path.join(self.tmp, "o.json")]), 1)


if __name__ == "__main__":
    unittest.main()
