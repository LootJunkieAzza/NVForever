# JIP's NIF block functions in NVForever (N6)

JIP's functions that read and change a reference's model nodes by name (GetChildBlocks,
GetNifBlockTranslation/Rotation/Scale/Flag and their setters, RenameNifBlock,
RemoveNifBlock, InsertNode, AttachModel, ModelHasBlock). Facts are from their pages
`[documented]` and the peer's nif-blocks set in the original game `[observed]`. Nodes
are known by name only; no memory layout of the original is used (extender-compat.md
§5.8).

## The split

- **Game side (`fnv_game::nif_blocks`):** a mirror of the node tree of every reference
  a script asked about (`SceneMirror`), filled by the engine each frame, and the scripts'
  changes queued for the engine. A change shows in the mirror at once [observed: a set
  reads back in the same frame]. Until the engine has told a tree, the getters give
  ar_Null or 0, as the original does before a placed actor's 3D exists [observed: 5
  frames after PlaceAtMe].
- **Engine side (`nvforever` nif_mirror.rs):** a reference's root is its actor rig (the
  player's: the third-person body), else the topmost entity drawing it. An actor's tree
  starts at its skeleton's root node, `Scene Root` [observed: GetNifBlockParentNodes of
  `Bip01 Head` ends there, #2132]; actors' skeleton node names come from the rig (their
  entities carry no `Name`), their parts' shapes are named as their models spell them.
  The player's tree and those of references scripts created (actors placed with
  PlaceAtMe, dropped items) are told every frame, watched or not, so a first read finds
  them [observed: the original reads the player's tree, Doc's copy and a pistol placed
  six frames before at once]; an actor a script creates is drawn the frame after, its
  parts read then rather than by the read-ahead (residents.rs `at_once`, #2133). Any
  other reference is told from the frame after a script first asks about it: a first
  read of it gives ar_Null or 0 [hypothesis: the original's reads it at once]. A created item drawn
  as flat shapes (a weapon on the ground) has no node entities: its tree is its model
  file's (names, parents, local transforms; rotation from the stored rows taken as
  columns [observed: the pistol's ProjectileNode reads 90 0 90]), with the scripts' set
  transforms over it. Each frame the queued changes are applied; a set
  translation, rotation or scale is held over the animation, frame after frame [observed:
  set-then-read stays exact on an animated bone]; the trees of the watched references are
  told back, depth first, a repeated name shown as `name:0`, `name:1` … [observed:
  `Object`, `Object:0`].
- **ModelHasBlock:** the base form's model file is read through the host's reader
  (`fnv_game::mod_files::read_data`) and its header string table searched for the exact
  name [observed: case-sensitive].

## Conventions

- Rotation: degrees, X Y Z extrinsic (glam `EulerRot::ZYX` read backwards); the getter's
  "intrinsic" form (whichRotation 2) is not yet told apart.
- GetNifBlockTranslation / Rotation with an axis letter give that number; with a number
  or nothing, the three as an array [observed: JIP 57.3 returns an array].
- Name lookups: exact first, then without regard to case [observed: InsertNode took a
  lower-case target].

## Not built yet

What the nif-blocks set still finds different on the engine (#2132, 49 of 60 rows): the head's
FaceGen nodes are named by the original in its own way (HeadAnims:0, BSFaceGenNiNodeBiped,
FaceGenMouth …, names the data files do not hold [observed]), where NVForever gives the
head models' shape names; the player's skeleton lists 90 nodes under Bip01 against the
original's 108 (Bip01 Looking, Translate, Rotate …); GetNifBlockFlag's flag 2, and
RenameNifBlock, InsertNode and RemoveNifBlock on the head as the set chains them.
Rotations read from drawn nodes go through nif_spawn's transpose; whether they read
as the original's needs a set that reads a static's node [hypothesis].

AttachModel's model is not drawn (the mirror keeps the request); the first-person model
(`pcModel`); GetBlockTextureSet; the world-space translation (`getWorld`).
