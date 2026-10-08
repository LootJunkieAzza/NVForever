#!/usr/bin/env python3
"""Independent reference reader ("oracle") for collision geometry in NIF meshes inside BSAs.

Written only from docs/spec-notes/collision.md (section 5 is the `fnv.collide.objects/1`
contract, sections 1-4 the facts), docs/spec-notes/nif.md, docs/spec-notes/bsa.md and the
niftools nif.xml read as documentation for the Havok block byte layouts, deliberately
without looking at the Rust implementation or its schemas, so the parity lab can compare
two independent readers. Standard library only. It imports oracle_nif.py (container
header, BSA walking, output helpers) and, through it, oracle_bsa.py; both must sit next
to it.

    python oracle_collide.py objects <archive.bsa> [-o out.json] [--filter text]

Every `.nif` file of the archive is read in archive record order. Files with a block-size
table are split into blocks by that table; scene objects are read up to their collision
reference (and child list for nodes); every collision object, body and shape block that
is followed is decoded completely and must consume its stored size exactly.

Havok block layouts used (Fallout 3 / New Vegas: NIF 20.2.0.7, user version 11, Bethesda
stream version <= 34; nif.xml conditions evaluated for those versions -- they give the
same layout for every stream version 14..34 present in the game). All little-endian;
"ref" is an i32 block index (-1 = none):

  collision objects  target ref, flags u16, body ref
                     (bhkBlendCollisionObject: + heir gain f32, velocity gain f32)
  world object head  shape ref, havok filter (layer u8, flags u8, group u16),
                     world-object info (4 unused, broad-phase u8, 3 unused, 12-byte property)
  bhkRigidBody(T)    world object head, entity info (response u8, unused u8, delay u16),
                     rigid-body info: 4 unused, havok filter, 4 unused, response u8,
                     unused u8, delay u16, 4 unused, translation vec4, rotation quaternion
                     (x, y, z, w), linear and angular velocity vec4, inertia 12 f32,
                     centre vec4, mass, linear damping, angular damping, friction,
                     restitution, max linear velocity, max angular velocity, penetration
                     depth (f32 each), motion system u8, deactivator u8, solver
                     deactivation u8, quality u8, 12 unused;
                     constraint count u32 + refs, body flags u32
  bhkSimpleShapePhantom  world object head, 8 unused, 16 f32 transform
  bhkAabbPhantom         world object head, 8 unused, min vec4, max vec4
  bhkSphereShape     material u32, radius f32
  bhkBoxShape        material, radius, 8 unused, half extents 3 f32, unused f32
  bhkCapsuleShape    material, radius, 8 unused, first point 3 f32, radius 1,
                     second point 3 f32, radius 2
  bhkConvexVerticesShape  material, radius, two 12-byte properties, vertex count u32 +
                     vec4 each, normal count u32 + vec4 each
  bhk(Convex)TransformShape  shape ref, material, radius, 8 unused, 16 f32 transform
  bhkListShape       count u32 + refs, material, two 12-byte properties,
                     filter count u32 + 4 bytes each
  bhkMoppBvTreeShape shape ref, 12 unused, scale f32, MOPP size u32, offset vec4, MOPP bytes
  bhkPackedNiTriStripsShape  user data u32, 4 unused, radius, 4 unused, scale vec4,
                     radius copy, scale copy vec4, data ref (no sub-part list in 20.2.0.7)
  hkPackedNiTriStripsData  triangle count u32 + (3 u16 indices + u16 welding) each,
                     vertex count u32, compressed u8, vertices (3 f32 each, or 3 u16 when
                     compressed), sub-part count u16 + (filter u32, vertex count u32,
                     material u32) each
  bhkNiTriStripsShape material, radius, 20 unused, grow-by u32, scale vec4,
                     strips-data count u32 + refs, filter count u32 + 4 bytes each
  bhkMultiSphereShape  material, 12-byte property, sphere count u32 + (centre 3 f32,
                     radius f32) each

Points where collision.md leaves a choice open are marked `# SPEC-AMBIGUITY` below.

Exit codes: 0 ok, 1 archive unreadable, 2 usage error. Diagnostics go to stderr.
"""
import argparse
import math
import mmap
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import oracle_nif  # noqa: E402

SCHEMA = "fnv.collide.objects/1"
HAVOK_SCALE = 7.0  # collision.md section 2

NO_STRING = 0xFFFFFFFF

# Scene objects: NiAVObject and its descendants that can occur in Bethesda 20.2.0.7 files
# (nif.xml inheritance). All share the NiAVObject leading fields (nif.md section 3).
NODE_TYPES = frozenset({
    "NiNode", "BSFadeNode", "BSMultiBoundNode", "BSValueNode", "NiBillboardNode",
    "BSOrderedNode", "BSMasterParticleSystem", "BSRangeNode", "BSBlastNode",
    "BSDamageStage", "BSDebrisNode", "BSTreeNode", "BSLeafAnimNode", "NiSwitchNode",
    "NiLODNode", "NiBSAnimationNode", "NiBSParticleNode", "NiSortAdjustNode",
    "NiBone", "NiCollisionSwitch", "NiRoom", "NiRoomGroup", "NiWall", "AvoidNode",
    "RootCollisionNode",
})
OTHER_SCENE_TYPES = frozenset({
    "NiTriShape", "NiTriStrips", "BSSegmentedTriShape", "NiLines", "NiParticles",
    "NiParticleSystem", "NiMeshParticleSystem", "BSStripParticleSystem",
    "NiAutoNormalParticles", "NiRotatingParticles", "NiParticleMeshes",
    "NiCamera", "NiAmbientLight", "NiDirectionalLight", "NiPointLight", "NiSpotLight",
    "NiTextureEffect", "NiPortal",
})
SCENE_TYPES = NODE_TYPES | OTHER_SCENE_TYPES

COLLISION_OBJECT_TYPES = frozenset({
    "bhkCollisionObject", "bhkBlendCollisionObject", "bhkPCollisionObject",
    "bhkSPCollisionObject",
})
RIGID_BODY_TYPES = frozenset({"bhkRigidBody", "bhkRigidBodyT"})
PHANTOM_TYPES = frozenset({"bhkSimpleShapePhantom", "bhkAabbPhantom"})

MOTION_FIXED = 7
MOTION_KEYFRAMED = 6

SHAPE_KINDS = ("box", "capsule", "convex_hull", "sphere", "tri_mesh")
MAX_TREE_DEPTH = 64

_F = struct.Struct("<f")
_I = struct.Struct("<I")
_i = struct.Struct("<i")
_H = struct.Struct("<H")


class Skip(Exception):
    """The file's collision cannot be followed; the message is the `skipped` reason."""


def _num(x):
    """A float as a JSON number; NaN and infinities (not representable) -> null."""
    return x if math.isfinite(x) else None


# --------------------------------------------------------------------------- byte cursor

class Cursor:
    """Bounded little-endian reader over buf[pos:end] (one block)."""

    __slots__ = ("buf", "pos", "end", "what")

    def __init__(self, buf, pos, end, what):
        self.buf, self.pos, self.end, self.what = buf, pos, end, what

    def _need(self, n, field):
        if self.pos + n > self.end:
            raise Skip(f"{self.what}: {field} needs {n} bytes, only "
                       f"{max(self.end - self.pos, 0)} left in the block")

    def u8(self, field):
        self._need(1, field)
        v = self.buf[self.pos]
        self.pos += 1
        return v

    def u16(self, field):
        self._need(2, field)
        v = _H.unpack_from(self.buf, self.pos)[0]
        self.pos += 2
        return v

    def u32(self, field):
        self._need(4, field)
        v = _I.unpack_from(self.buf, self.pos)[0]
        self.pos += 4
        return v

    def ref(self, field):
        self._need(4, field)
        v = _i.unpack_from(self.buf, self.pos)[0]
        self.pos += 4
        return v

    def f32(self, field):
        self._need(4, field)
        v = _F.unpack_from(self.buf, self.pos)[0]
        self.pos += 4
        return v

    def f32s(self, n, field):
        if n == 0:
            return ()
        self._need(4 * n, field)
        v = struct.unpack_from(f"<{n}f", self.buf, self.pos)
        self.pos += 4 * n
        return v

    def refs(self, n, field):
        if n == 0:
            return ()
        self._need(4 * n, field)
        v = struct.unpack_from(f"<{n}i", self.buf, self.pos)
        self.pos += 4 * n
        return v

    def skip(self, n, field):
        self._need(n, field)
        self.pos += n

    def done(self):
        if self.pos != self.end:
            raise Skip(f"{self.what}: {self.end - self.pos} bytes left over at end of block")


# --------------------------------------------------------------------------- affine maths
# An affine map is (A, b): A a 3x3 matrix as a tuple of three row tuples, b a 3-vector;
# p' = A.p + b.

IDENTITY = (((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0)), (0.0, 0.0, 0.0))


def compose(outer, inner):
    """outer o inner (inner applied first)."""
    (a, b), (c, d) = outer, inner
    m = tuple(tuple(a[r][0] * c[0][k] + a[r][1] * c[1][k] + a[r][2] * c[2][k]
                    for k in range(3)) for r in range(3))
    t = tuple(a[r][0] * d[0] + a[r][1] * d[1] + a[r][2] * d[2] + b[r] for r in range(3))
    return m, t


def apply(aff, p):
    a, b = aff
    x, y, z = p
    return (a[0][0] * x + a[0][1] * y + a[0][2] * z + b[0],
            a[1][0] * x + a[1][1] * y + a[1][2] * z + b[1],
            a[2][0] * x + a[2][1] * y + a[2][2] * z + b[2])


def scene_local(translation, rotation, scale):
    """NiAVObject local transform: rotation stored row by row (nif.md section 6),
    uniform scale, then translation."""
    r = rotation
    a = tuple(tuple(r[3 * row + col] * scale for col in range(3)) for row in range(3))
    return a, tuple(translation)


MATRIX_TOLERANCE = 1e-3


def havok_matrix(e):
    """16 stored floats -> (affine, uniform scale) or None when the 3x3 part is not a
    rotation times a positive uniform scale. Element (row, col) = e[4*col + row],
    translation e[12..14]; elements 3, 7, 11, 15 ignored (collision.md section 3)."""
    a = tuple(tuple(e[4 * col + row] for col in range(3)) for row in range(3))
    cols = [(a[0][k], a[1][k], a[2][k]) for k in range(3)]
    dot = [[sum(cols[i][r] * cols[j][r] for r in range(3)) for j in range(3)] for i in range(3)]
    s2 = (dot[0][0] + dot[1][1] + dot[2][2]) / 3.0
    det = (a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
           - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
           + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]))
    # SPEC-AMBIGUITY: collision.md gives no tolerance for "rotation x uniform scale"; the
    # columns must have equal squared lengths and be orthogonal within 1e-3 of the mean
    # squared length, and the determinant must be positive (a mirror is not a rotation).
    if not (math.isfinite(s2) and s2 > 0.0 and det > 0.0):
        return None
    for i in range(3):
        for j in range(3):
            want = s2 if i == j else 0.0
            if abs(dot[i][j] - want) > MATRIX_TOLERANCE * s2:
                return None
    return (a, (e[12], e[13], e[14])), math.sqrt(s2)


def quaternion_rotation(x, y, z, w):
    """Rotation matrix of a quaternion stored x, y, z, w (nif.md section 4)."""
    n = x * x + y * y + z * z + w * w
    # SPEC-AMBIGUITY: collision.md does not say whether a non-unit body quaternion is
    # normalised; it is normalised here (a zero quaternion is taken as identity).
    if n == 0.0 or not math.isfinite(n):
        return IDENTITY[0]
    s = 2.0 / n
    return ((1 - s * (y * y + z * z), s * (x * y - z * w), s * (x * z + y * w)),
            (s * (x * y + z * w), 1 - s * (x * x + z * z), s * (y * z - x * w)),
            (s * (x * z - y * w), s * (y * z + x * w), 1 - s * (x * x + y * y)))


# --------------------------------------------------------------------------- block decoders

def _world_object_head(c):
    shape = c.ref("shape")
    layer = c.u8("havok filter layer")
    c.u8("havok filter flags")
    c.u16("havok filter group")
    c.skip(20, "world object info")
    return shape, layer


def decode_collision_object(c, type_name):
    c.ref("target")
    c.u16("flags")
    body = c.ref("body")
    if type_name == "bhkBlendCollisionObject":
        c.f32s(2, "heir gain, velocity gain")
    c.done()
    return body


def decode_rigid_body(c):
    shape, layer = _world_object_head(c)
    c.skip(4, "entity info")
    c.skip(4, "unused")
    c.u8("info havok filter layer")
    c.skip(3, "info havok filter flags, group")
    c.skip(4 + 4 + 4, "unused, response, delay, unused")
    translation = c.f32s(4, "translation")
    rotation = c.f32s(4, "rotation")
    c.f32s(4 + 4 + 12 + 4, "velocities, inertia, centre")
    mass, _ld, _ad, friction, restitution = c.f32s(5, "mass .. restitution")
    c.f32s(3, "max velocities, penetration depth")
    motion = c.u8("motion system")
    c.skip(3, "deactivator, solver deactivation, quality")
    c.skip(12, "unused")
    n = c.u32("constraint count")
    c.refs(n, "constraints")
    c.u32("body flags")
    c.done()
    return {"shape": shape, "layer": layer, "translation": translation[:3],
            "rotation": rotation, "mass": mass, "friction": friction,
            "restitution": restitution, "motion": motion}


def decode_phantom(c, type_name):
    _world_object_head(c)
    c.skip(8, "unused")
    if type_name == "bhkSimpleShapePhantom":
        c.f32s(16, "transform")
    else:
        c.f32s(8, "aabb")
    c.done()


def decode_shape(c, type_name, buf):
    """Return a dict describing one shape block (fields as needed for the conversion)."""
    if type_name == "bhkSphereShape":
        c.u32("material")
        radius = c.f32("radius")
        c.done()
        return {"kind": "sphere", "radius": radius}
    if type_name == "bhkBoxShape":
        c.u32("material")
        c.f32("radius")
        c.skip(8, "unused")
        half = c.f32s(3, "dimensions")
        c.f32("unused float")
        c.done()
        return {"kind": "box", "half": half}
    if type_name == "bhkCapsuleShape":
        c.u32("material")
        radius = c.f32("radius")
        c.skip(8, "unused")
        p1 = c.f32s(3, "first point")
        c.f32("radius 1")
        p2 = c.f32s(3, "second point")
        c.f32("radius 2")
        c.done()
        # collision.md section 3: the bhkConvexShape `radius` (radius 1 / 2 are copies).
        return {"kind": "capsule", "radius": radius, "p1": p1, "p2": p2}
    if type_name == "bhkConvexVerticesShape":
        c.u32("material")
        c.f32("radius")
        c.skip(24, "vertices and normals properties")
        n = c.u32("vertex count")
        v = c.f32s(4 * n, "vertices")
        m = c.u32("normal count")
        c.f32s(4 * m, "normals")
        c.done()
        points = [(v[4 * k], v[4 * k + 1], v[4 * k + 2]) for k in range(n)]
        return {"kind": "convex_hull", "points": points}
    if type_name in ("bhkTransformShape", "bhkConvexTransformShape"):
        child = c.ref("shape")
        c.u32("material")
        c.f32("radius")
        c.skip(8, "unused")
        e = c.f32s(16, "transform")
        c.done()
        m = havok_matrix(e)
        if m is None:
            raise Skip(f"{c.what}: transform matrix is not rotation x uniform scale")
        return {"kind": "transform", "children": (child,), "matrix": m[0], "scale": m[1]}
    if type_name == "bhkListShape":
        n = c.u32("sub shape count")
        subs = c.refs(n, "sub shapes")
        c.u32("material")
        c.skip(24, "child shape and filter properties")
        f = c.u32("filter count")
        c.skip(4 * f, "filters")
        c.done()
        return {"kind": "list", "children": subs}
    if type_name == "bhkMoppBvTreeShape":
        child = c.ref("shape")
        c.skip(12, "unused")
        c.f32("scale")
        size = c.u32("MOPP data size")
        c.f32s(4, "MOPP offset")
        c.skip(size, "MOPP data")
        c.done()
        return {"kind": "list", "children": (child,)}
    if type_name == "bhkPackedNiTriStripsShape":
        # nif.xml: the shape's own sub-part list exists only until 20.0.0.5.
        c.u32("user data")
        c.skip(4, "unused")
        c.f32("radius")
        c.skip(4, "unused")
        c.f32s(4, "scale")
        c.f32("radius copy")
        c.f32s(4, "scale copy")
        data = c.ref("data")
        c.done()
        # collision.md section 3: the stored scale vectors are not applied (they copy the
        # node scale).
        return {"kind": "packed", "data": data}
    if type_name == "bhkNiTriStripsShape":
        c.u32("material")
        c.f32("radius")
        c.skip(20, "unused")
        c.u32("grow by")
        c.f32s(4, "scale")
        n = c.u32("strips data count")
        strips = c.refs(n, "strips data")
        f = c.u32("filter count")
        c.skip(4 * f, "filters")
        c.done()
        return {"kind": "strips_shape", "strips": strips}
    if type_name == "bhkMultiSphereShape":
        c.u32("material")
        c.skip(12, "shape property")
        n = c.u32("sphere count")
        s = c.f32s(4 * n, "spheres")
        c.done()
        spheres = [((s[4 * k], s[4 * k + 1], s[4 * k + 2]), s[4 * k + 3]) for k in range(n)]
        return {"kind": "multi_sphere", "spheres": spheres}
    return None


def decode_packed_data(c, buf):
    ntri = c.u32("triangle count")
    c.skip(8 * ntri, "triangles")
    nv = c.u32("vertex count")
    compressed = c.u8("compressed")
    if compressed:
        return {"compressed": True}
    v = c.f32s(3 * nv, "vertices")
    nsub = c.u16("sub part count")
    c.skip(12 * nsub, "sub parts")
    c.done()
    return {"compressed": False, "triangles": ntri, "vertices": v}


def decode_tri_strips_data(c, version):
    """NiTriStripsData (nif.md section 3): positions and strip triangle count."""
    c.ref("group id")
    n = c.u16("vertex count")
    c.skip(2, "keep/compress flags")
    positions = ()
    if c.u8("has vertices"):
        positions = c.f32s(3 * n, "vertices")
    flags = c.u16("vector flags")
    uv_sets = flags & 0x1 if version == oracle_nif.V20_2_0_7 else flags & 0x3F
    if c.u8("has normals"):
        c.skip(12 * n, "normals")
        if flags & 0x1000:
            c.skip(24 * n, "tangents and bitangents")
    c.skip(16, "bounding sphere")
    if c.u8("has vertex colours"):
        c.skip(16 * n, "vertex colours")
    c.skip(8 * n * uv_sets, "uv sets")
    c.skip(6, "consistency, additional data")
    c.u16("triangle count")
    strips = c.u16("strip count")
    lengths = [c.u16("strip length") for _ in range(strips)]
    triangles = 0
    if c.u8("has points"):
        # collision.md section 5: every three consecutive strip indices form a triangle;
        # degenerate ones (two equal indices) only join strips and are left out.
        for k in lengths:
            c._need(2 * k, "strip points")
            pts = struct.unpack_from(f"<{k}H", c.buf, c.pos) if k else ()
            c.pos += 2 * k
            for j in range(k - 2):
                x, y, z = pts[j], pts[j + 1], pts[j + 2]
                if x != y and y != z and x != z:
                    triangles += 1
    c.done()
    return {"vertices": positions, "triangles": triangles}


# --------------------------------------------------------------------------- one file

class _File:
    """Block access with per-block decode caching."""

    def __init__(self, path, buf, header, starts):
        self.path, self.buf, self.h, self.starts = path, buf, header, starts
        self.types = [header.type_names[t] for t in header.block_type_index]
        self.cache = {}
        self.root = None

    def cursor(self, i):
        start = self.starts[i]
        return Cursor(self.buf, start, start + self.h.block_sizes[i],
                      f"block {i} ({self.types[i]})")

    def check_ref(self, ref, what, holder):
        if ref < 0 or ref >= len(self.types):
            raise Skip(f"missing block: {what} reference {ref} of block {holder} "
                       f"({self.types[holder]})")
        return self.types[ref]


class _Acc:
    """Leaf counts and running bounds of one collision object."""

    __slots__ = ("counts", "hull_points", "mesh_vertices", "mesh_triangles", "lo", "hi")

    def __init__(self):
        self.counts = {}
        self.hull_points = self.mesh_vertices = self.mesh_triangles = 0
        self.lo = [math.inf] * 3
        self.hi = [-math.inf] * 3

    def leaf(self, kind):
        self.counts[kind] = self.counts.get(kind, 0) + 1

    def point(self, p, r=0.0):
        lo, hi = self.lo, self.hi
        for k in range(3):
            v = p[k]
            if v - r < lo[k]:
                lo[k] = v - r
            if v + r > hi[k]:
                hi[k] = v + r

    def points_flat(self, aff, flat):
        """Add every xyz triple of a flat coordinate sequence, transformed."""
        (a0, a1, a2), b = aff
        lo, hi = self.lo, self.hi
        it = iter(flat)
        for x, y, z in zip(it, it, it):
            px = a0[0] * x + a0[1] * y + a0[2] * z + b[0]
            py = a1[0] * x + a1[1] * y + a1[2] * z + b[1]
            pz = a2[0] * x + a2[1] * y + a2[2] * z + b[2]
            if px < lo[0]:
                lo[0] = px
            if px > hi[0]:
                hi[0] = px
            if py < lo[1]:
                lo[1] = py
            if py > hi[1]:
                hi[1] = py
            if pz < lo[2]:
                lo[2] = pz
            if pz > hi[2]:
                hi[2] = pz


def _walk(f, index, aff, radius_scale, acc, holder, what, path):
    """Visit the shape tree rooted at block `index` (reached from block `holder`)."""
    type_name = f.check_ref(index, what, holder)
    if index in path:
        raise Skip(f"shape tree cycle at block {index} ({type_name})")
    if len(path) >= MAX_TREE_DEPTH:
        raise Skip(f"shape tree deeper than {MAX_TREE_DEPTH} at block {index}")
    shape = f.cache.get(index)
    if shape is None:
        shape = decode_shape(f.cursor(index), type_name, f.buf)
        if shape is None:
            raise Skip(f"unsupported shape type {type_name} (block {index})")
        f.cache[index] = shape
    kind = shape["kind"]
    path = path + (index,)
    if kind == "list":
        for child in shape["children"]:
            _walk(f, child, aff, radius_scale, acc, index, "sub shape", path)
    elif kind == "transform":
        # The matrix's uniform scale applies to the child's points and radii alike.
        _walk(f, shape["children"][0], compose(aff, shape["matrix"]),
              radius_scale * shape["scale"], acc, index, "shape", path)
    elif kind == "box":
        acc.leaf("box")
        hx, hy, hz = shape["half"]
        for sx in (-hx, hx):
            for sy in (-hy, hy):
                for sz in (-hz, hz):
                    acc.point(apply(aff, (sx, sy, sz)))
    elif kind == "sphere":
        acc.leaf("sphere")
        acc.point(apply(aff, (0.0, 0.0, 0.0)), shape["radius"] * radius_scale)
    elif kind == "capsule":
        acc.leaf("capsule")
        r = shape["radius"] * radius_scale
        acc.point(apply(aff, shape["p1"]), r)
        acc.point(apply(aff, shape["p2"]), r)
    elif kind == "convex_hull":
        acc.leaf("convex_hull")
        acc.hull_points += len(shape["points"])
        for p in shape["points"]:
            acc.point(apply(aff, p))
    elif kind == "multi_sphere":
        for centre, r in shape["spheres"]:
            acc.leaf("sphere")
            acc.point(apply(aff, centre), r * radius_scale)
    elif kind == "packed":
        data_index = shape["data"]
        dtype = f.check_ref(data_index, "data", index)
        if dtype != "hkPackedNiTriStripsData":
            raise Skip(f"unsupported packed data type {dtype} (block {data_index})")
        data = f.cache.get(data_index)
        if data is None:
            data = decode_packed_data(f.cursor(data_index), f.buf)
            f.cache[data_index] = data
        if data["compressed"]:
            raise Skip(f"compressed packed vertices (block {data_index})")
        acc.leaf("tri_mesh")
        acc.mesh_vertices += len(data["vertices"]) // 3
        acc.mesh_triangles += data["triangles"]
        acc.points_flat(aff, data["vertices"])
    elif kind == "strips_shape":
        # collision.md section 5: a bhkNiTriStripsShape is one tri_mesh leaf whatever the
        # number of NiTriStripsData blocks it lists; vertices and triangles are summed.
        acc.leaf("tri_mesh")
        for data_index in shape["strips"]:
            dtype = f.check_ref(data_index, "strips data", index)
            if dtype != "NiTriStripsData":
                raise Skip(f"unsupported strips data type {dtype} (block {data_index})")
            data = f.cache.get(data_index)
            if data is None:
                data = decode_tri_strips_data(f.cursor(data_index), f.h.version)
                f.cache[data_index] = data
            acc.mesh_vertices += len(data["vertices"]) // 3
            acc.mesh_triangles += data["triangles"]
            acc.points_flat(aff, data["vertices"])
    else:  # pragma: no cover - decode_shape only returns the kinds above
        raise Skip(f"unsupported shape type {type_name} (block {index})")


def _motion(code):
    if code == MOTION_FIXED:
        return "static"
    if code == MOTION_KEYFRAMED:
        return "keyframed"
    return "dynamic"


def collide_file(path, buf):
    """Analyse one file. Returns ("skipped", reason) or ("ok", objects, phantoms)."""
    try:
        h = oracle_nif.read_header(buf)
    except oracle_nif.NifError as exc:
        return "skipped", f"container: {exc}"
    if h.block_sizes is None:
        return "skipped", f"no block-size table (version {oracle_nif.version_text(h.version)})"
    # SPEC-AMBIGUITY: the Havok layouts are only known for 20.2.0.7 with stream version
    # <= 34 (nif.xml conditions); any other version with a size table is skipped.
    if h.version != oracle_nif.V20_2_0_7 or h.bs_version > 34:
        return "skipped", (f"unsupported version {oracle_nif.version_text(h.version)} "
                           f"stream {h.bs_version}")
    starts = []
    pos = h.data_start
    for size in h.block_sizes:
        starts.append(pos)
        pos += size
    if pos > len(buf):
        return "skipped", f"container: block sizes end at {pos}, beyond the file end {len(buf)}"
    try:
        footer = oracle_nif.Reader(buf, pos)
        roots = footer.u32("footer root count")
        root = footer.i32("footer root") if roots else None
        footer.skip(4 * (roots - 1) if roots else 0, "footer roots")
    except oracle_nif.NifError as exc:
        return "skipped", f"container: {exc}"
    f = _File(path, buf, h, starts)
    # SPEC-AMBIGUITY: "the root" is the footer's first root (always block 0 in the game
    # data, nif.md section 2); further footer roots are composed like other nodes.
    f.root = root
    try:
        return ("ok",) + _collide(f)
    except Skip as exc:
        return "skipped", str(exc)


def _read_scene_object(f, i):
    """NiAVObject leading fields (nif.md section 3) -> (name, local affine, scale,
    collision ref, child refs or None)."""
    c = f.cursor(i)
    h = f.h
    name_index = c.u32("name")
    if name_index == NO_STRING:
        name = None
    elif name_index < len(h.strings):
        name = h.strings[name_index]
    else:
        raise Skip(f"{c.what}: name string index {name_index} out of range "
                   f"({len(h.strings)} strings)")
    c.refs(c.u32("extra data count"), "extra data")
    c.ref("controller")
    if h.bs_version > 26:
        c.u32("flags")
    else:
        c.u16("flags")
    translation = c.f32s(3, "translation")
    rotation = c.f32s(9, "rotation")
    scale = c.f32("scale")
    c.refs(c.u32("property count"), "properties")
    collision = c.ref("collision object")
    children = None
    if f.types[i] in NODE_TYPES:
        children = c.refs(c.u32("child count"), "children")
    return name, scene_local(translation, rotation, scale), scale, collision, children


def _collide(f):
    scene = {}
    parent = {}
    for i, type_name in enumerate(f.types):
        if type_name in SCENE_TYPES:
            name, local, scale, collision, children = _read_scene_object(f, i)
            scene[i] = (name, local, scale, collision)
            if children:
                for child in children:  # block order: the lowest-index node wins
                    if child >= 0 and child not in parent:
                        parent[child] = i
    objects, phantoms = [], []
    seen = set()
    for i in sorted(scene):
        name, _local, _scale, co = scene[i]
        if co < 0:
            continue
        # SPEC-AMBIGUITY: a collision object referenced by several scene objects is
        # reported once, for the first of them in block order.
        if co in seen:
            continue
        seen.add(co)
        co_type = f.check_ref(co, "collision object", i)
        if co_type not in COLLISION_OBJECT_TYPES:
            raise Skip(f"unsupported collision object type {co_type} (block {co})")
        body = decode_collision_object(f.cursor(co), co_type)
        body_type = f.check_ref(body, "body", co)
        key = f"{f.path}#{co}"
        if body_type in PHANTOM_TYPES:
            # collision.md section 5: phantom shape trees are not followed, so an
            # unsupported shape under a phantom does not skip the file.
            decode_phantom(f.cursor(body), body_type)
            phantoms.append({"key": key, "body_type": body_type})
            continue
        if body_type not in RIGID_BODY_TYPES:
            raise Skip(f"unsupported body type {body_type} (block {body})")
        rb = decode_rigid_body(f.cursor(body))
        # Scene-object-to-root: product of the local transforms of the scene object and
        # its ancestors below the root; the root's own transform is not applied
        # (collision.md sections 4 and 5). An object not under the root composes its whole
        # ancestor chain.
        world = IDENTITY
        acc_scale = 1.0
        node = i
        chain = set()
        while node is not None:
            if node in chain:
                raise Skip(f"scene graph cycle at block {node}")
            chain.add(node)
            if node == f.root:
                break
            entry = scene.get(node)
            if entry is None:  # parent is a node type, so always present; defensive
                break
            world = compose(entry[1], world)
            acc_scale *= entry[2]
            node = parent.get(node)
        k = HAVOK_SCALE
        aff = compose(world, (((k, 0.0, 0.0), (0.0, k, 0.0), (0.0, 0.0, k)), (0.0, 0.0, 0.0)))
        if body_type == "bhkRigidBodyT":
            body_aff = (quaternion_rotation(*rb["rotation"]), rb["translation"])
            aff = compose(aff, body_aff)
        # SPEC-AMBIGUITY: radii scale by |accumulated scale| (a negative node scale would
        # otherwise turn the interval inside out).
        radius_scale = HAVOK_SCALE * abs(acc_scale)
        acc = _Acc()
        _walk(f, rb["shape"], aff, radius_scale, acc, body, "shape", ())
        has_points = acc.lo[0] <= acc.hi[0]
        objects.append({
            "key": key,
            "node": name,
            "node_block": i,
            "object_type": co_type,
            "body_type": body_type,
            "motion": _motion(rb["motion"]),
            "layer": rb["layer"],
            "mass": _num(rb["mass"]),
            "friction": _num(rb["friction"]),
            "restitution": _num(rb["restitution"]),
            "shapes": {kind: acc.counts[kind] for kind in SHAPE_KINDS if kind in acc.counts},
            "hull_points": acc.hull_points,
            "mesh_vertices": acc.mesh_vertices,
            "mesh_triangles": acc.mesh_triangles,
            # collision.md section 5: an empty tree has null bounds.
            "aabb_min": [_num(v) for v in acc.lo] if has_points else None,
            "aabb_max": [_num(v) for v in acc.hi] if has_points else None,
        })
    return objects, phantoms


# --------------------------------------------------------------------------- archive

def objects_document(buf, archive_name, filter_text=None):
    objects, phantoms, skipped = [], [], []
    needle = filter_text.lower() if filter_text else None
    for path, data, err in oracle_nif.iter_archive(buf, (".nif",)):
        # SPEC-AMBIGUITY: --filter keeps files whose archive path contains the text,
        # ignoring ASCII case; other files appear nowhere (not even in `skipped`).
        if needle is not None and needle not in path.lower():
            continue
        if err is not None:
            skipped.append({"path": path, "reason": err})
            continue
        result = collide_file(path, data)
        if result[0] == "skipped":
            skipped.append({"path": path, "reason": result[1]})
        else:
            objects.extend(result[1])
            phantoms.extend(result[2])
    return {"format": SCHEMA, "archive": archive_name, "scale": HAVOK_SCALE,
            "objects": objects, "phantoms": phantoms, "skipped": skipped}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Collision objects of the meshes in a BSA.")
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("objects", help=f"emit {SCHEMA}")
    p.add_argument("archive")
    p.add_argument("-o", "--out", "--output", dest="out", help="write JSON here instead of stdout")
    p.add_argument("--filter", help="only files whose path contains this text (any case)")
    p.add_argument("--schema", help="accepted for CLI compatibility and ignored")
    args = parser.parse_args(argv)
    name = os.path.basename(args.archive)
    try:
        f, buf = oracle_nif.open_archive(args.archive)
    except OSError as exc:
        print(f"oracle_collide: cannot read {args.archive}: {exc}", file=sys.stderr)
        return 1
    try:
        doc = objects_document(buf, name, args.filter)
    except ValueError as exc:
        print(f"oracle_collide: {args.archive}: {exc}", file=sys.stderr)
        return 1
    finally:
        if isinstance(buf, mmap.mmap):
            buf.close()
        f.close()
    oracle_nif.write_output(oracle_nif.render(doc, ("objects", "phantoms", "skipped")), args.out)
    for e in doc["skipped"]:
        print(f"oracle_collide: skipped {e['path']}: {e['reason']}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
