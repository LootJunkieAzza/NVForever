# First-person depth: the arms and weapon are never hidden by the world

Status 2026-10-27: on `claude/first-person-depth`. Owner report 2026-10-27, testing weapon
lowering (PR #858, [player-body.md §1.4b](player-body.md)): an M4A1-style rifle pushed
through the Prospector Saloon's door in first person; can the clipping stop without
changing collision?

## 1. The problem

The first-person arms rig and its weapon (`player_body.rs`, render layer
`sky::ARMS_LAYER`) are real geometry in front of the eye, drawn by the main camera
against the same depth buffer as the world. The player's collision capsule keeps the eye
some 20–30 units from a wall, while a long gun reaches 60 units or more ahead of the eye
[observed: varmint rifle at the saloon's east wall, `first-person-depth` smoke scenario].
So the barrel, and at a wall the hands, went into the wall or a door or an actor standing
close, and the world drew over them. Weapon lowering shortens the reach but does not stop
it.

Whether the original draws its first-person model in a pass of its own, or with a depth
range of its own, is [hypothesis]: it is the usual technique in first-person games and the
original's weapon is not seen cut by walls in normal play, but this was not checked in a
capture (observer check 27).

## 2. What is done [engineering choice]

The depth of everything drawn as a first-person shape is squeezed into the nearest share
of the depth range, `FIRST_PERSON_DEPTH_SHARE` = 1/64 (`nvf_assets::nif_material`,
module notes "First-person depth"; `first_person_depth.wgsl`):

- Bevy uses reverse z (1 at the near plane), so a fragment at depth z writes
  `1 − share × (1 − z)`. The squeeze is affine in z, so the shapes keep their own order
  (the hand in front of the stock, the stock in front of the barrel) and are in front of
  every world surface farther than near ÷ (1 − share): 5.08 units with the main camera's
  near plane at 5, which clips everything nearer anyway.
- It is a material switch, `NifSurface::first_person`, in the pipeline key. The arms rig's
  materials get it with the shade-light switch (`player_body.rs` `arms_material`); the
  copies other systems make of those materials (Pip-Boy screen, Stealth Boy field, effect
  shader holes, texture swaps, actor fades) are made from the current material and keep it.
  The player's muzzle flash at the first-person muzzle gets copies with the switch
  (`shot_fx.rs` `first_person_model`): drawn at world depth, the squeezed barrel hid it.
- The fragment shaders write the depth (`@builtin(frag_depth)`): the main pass
  (nif_material.wgsl, `FIRST_PERSON_DEPTH`) and the depth prepass, which gets the NIF
  prepass fragment for these shapes whatever their alpha (nif_prepass.wgsl). Writing the
  depth in the fragment turns off early depth testing for these shapes only, a few
  hundred thousand pixels at most. It was chosen over a vertex-stage squeeze, which needs
  copies of Bevy's mesh and prepass vertex shaders (skinning, morphs, motion vectors) to
  keep in step with every Bevy update, and gives the same depths (the squeeze is affine
  in z, which is linear across a triangle on the screen).
- Only perspective views squeeze: the arms shade light's orthographic shadow cascade,
  which the arms cast into (`player_body.rs` `ArmsShade`), keeps true depths. No other
  shadow-casting light has `ARMS_LAYER` (the flashlight casts from layer 0 only).
  Prepasses with colour targets (normals, motion vectors) are left alone; the main camera
  has a depth-only prepass.

Kept as it was: one camera, so lighting, sun shadows, the shade light, fog, the image
space grade, MSAA and the depth prepass users all work unchanged; the nodes and the world
positions of the rig (muzzle, `ShellCasingNode`, the weapon-lowering rays, iron sights)
are untouched; collision is untouched. The second-camera alternative (a camera for
`ARMS_LAYER` only, depth cleared) was not taken: it needs its HDR, MSAA, tonemapping and
image-space settings kept equal to the main camera's, its own shadow views (the hidden-
light shadow-leak history, `player_body.rs` `ArmsShade`), and the flash would need the
same treatment anyway.

## 3. Readers of the depth prepass

The prepass holds the squeezed depth, so water and blended world shapes (smoke, glow
cards) are not drawn over the arms. The passes that turn the prepass into distances:

| Pass | Effect of the squeeze | Done |
|---|---|---|
| Soft intersections (nif_material.wgsl `soft_fade`) | a soft shape in front of the arms (the first-person flash) faded as if the arms were at the near plane | undone (`unsqueeze`) |
| SSAO (ssao.wgsl) | the arms flat at the near plane, no occlusion of their own | undone in `depth_at` |
| TAA (taa.wgsl) | the arms read as nearer than `CAMERA_LOCKED` (48 units): all of them now count as moving with the camera, also a barrel reaching past 48 units | kept (as intended there) |
| Motion blur and Pip-Boy sharpness (imagespace_blur.wgsl) | the arms nearer than the sharp depth: kept sharp | kept |
| Light shafts, eye mask | the arms are not sky | unchanged |
| Water's screen-space reflection | rays hit the arms as before (they are in front either way) | unchanged |
| Forward decals | a decal box behind the arms fails the depth test there, as before | unchanged |

`unsqueeze` takes every prepass value in the squeezed share back to its true depth; a
world surface could only be there nearer than 5.08 units.

## 4. Known differences

- The reflection camera (`reflection.rs`) also draws `ARMS_LAYER` and the flag squeezes
  there too: the arms' reflection is in front of the reflected world. It is seldom in the
  reflected picture.
- Shell casings, impact effects, the flash's add-on particles and the projectiles are
  world objects at world depth: where they pass in front of the weapon on the screen near
  the ejection port or the muzzle, the weapon now hides them.
- With `--first-person-body`, the arms rig is always in front of the body below the view;
  it was in front of it anyway except where the two met.

## 5. Checks

- Smoke scenario `first-person-depth` (tools/smoke/scenarios.toml): the varmint rifle at
  the saloon's east wall, lowered, then aimed and fired. A build from before the change
  drew only the stock there; with it the whole rifle and the left hand show (frames
  judged 2026-10-27, not committed).
- The flash summary line `shot fx: N muzzle flashes drawn in front of the world with the
  first-person weapon`.
