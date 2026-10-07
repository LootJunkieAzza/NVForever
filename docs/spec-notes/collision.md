# Spec notes: collision geometry (Layer 4, workstream 1)

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525, the mesh archives of
the ten masters), **[hypothesis]** needs verification.

Status 2026-10-07: **API stub only** (`crates/fnv-collide/src/shape.rs`, the contract with
`fnv-phys`, docs/design/layer4.md §7). The sections below are filled as each fact is
established on the data.

## 1. Where collision lives in a mesh

Collision is stored in the NIF as Havok blocks (`bhk*`, layouts in
`schemas/nif/havok.json`, re-expressed from nif.xml as documentation; all of them decode
exactly on every mesh, docs/design/layer2.md §2.3 [observed]). A scene node points to a
collision object, which holds a rigid body, which holds a shape tree. To be documented
here: the chain and its fields, which shape types occur and how often.

## 2. Havok scale

Havok data is stored in its own units; the factor to game units is commonly given as 7
for this engine generation [hypothesis]. To be measured: for every mesh with both,
collision-shape bounds against render-geometry bounds.

## 3. Shapes

To be documented: box, sphere, capsule, multi-sphere, convex vertices, list, transform /
convex transform, NiTriStrips shape, packed strips (hkPackedNiTriStripsData), MOPP tree —
what each stores and how it maps to `fnv_collide::Shape`.

## 4. Bodies, layers, materials

To be documented: motion type and quality, collision layer and material codes, mass,
friction, restitution, and the body transform of bhkRigidBodyT.

## 5. CLI contract

To be defined with the first `fnvtool collide` command.
