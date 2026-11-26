# The shader that reads a bounce light volume (#3009)

An interior's bounced light is to come from a small 3D texture, a grid of points in the room's
air, in place of a part of the flat ambient light. This note is about the shader that reads such a
texture: the bindings, the read, the light made of it and the surfaces that take part. **Built:
the shader and its bindings. Nothing makes or binds a texture**: there is no setting, no upload and
nothing to see. All of it is ours [design], none of it is a claim about the original game.

## The bindings

A view that has bevy_pbr's component `NvfRtgiVolumeBindings` (`engine/vendor/README.md`) has three
bindings more in its mesh view bind group, for the fragment stage: 47, a uniform; 48, a filterable
float 3D texture; 49, a filtering sampler. Its pipelines have the key bit
`MeshPipelineKey::NVF_RTGI_VOLUME` (30), its layouts `MeshPipelineViewLayoutKey::NVF_RTGI_VOLUME`
(20). Both come from that one component, so they agree. No shader def follows from the bit: a
material that reads the bindings pushes `NVF_RTGI_VOLUME` itself.

The uniform is 80 bytes (`rtgi_volume.wesl` `NvfRtgiVolume`):

| Row | xyz | w |
|---|---|---|
| origin | the first point that is not of the shell, in Bevy's world and game units | 1 / spacing |
| dimensions | W, H, D, the points without the shell | how far a read is moved off the surface |
| atlas | 1 / the texture's sides | D + 2, a slab's texels along z |
| fit | pay, least, thin | pull |
| light | b, g, share | 0 |

## The texture

Four slabs one after another along z, each W x H x D points with one texel of zeros all around, so
(W + 2) x (H + 2) x 4 (D + 2) texels:

| Slab | RGB | Alpha |
|---|---|---|
| 0 | C0, the mean of the light L around the point | m, 1 at a point that counts, else 0 |
| 1 | Cx, 2 x the mean of L w.x | m v, v the luminance of C0 |
| 2 | Cy | 0 |
| 3 | Cz | 0 |

Everything is 0 where m is 0.

## The read

At the place P with the geometric normal G, on the side the camera sees, and the shading normal n:

```text
q = (P + offset G - origin) / spacing          outside -1 < q < dimensions: E = v = 0
t = (q + 1.5 + (0, 0, s (D + 2))) / atlas      of slab s; linear filter
w = 1 / max(m, thin)                           thin = 0.25
E = max(w (C0 + Cx n.x + Cy n.y + Cz n.z), 0)
v = w (m v)
```

`thin` is 0.25, the least support [design]: one point of eight in reach fades out instead of being
taken for the whole. The shader takes it from the uniform (`fit.z`), so whoever fills the uniform
supplies it, for an empty texture too: with 0 a read without support divides by 0.

G is the triangle's, from the world position's derivatives, taken before anything is discarded;
where a triangle is too small to have one, n. The reads are of an explicit level, so they need no
uniform control flow.

## The light

S is the display-value sum (`lights/display_shader.rs`), A the ambient part of it with the ambient
occlusion in it, occ that occlusion: `diffuse_occlusion`, times the ambient occlusion's factor per
colour channel where the view has it. Where the diffuse light was `diffuse colour x S^2.2`, it is
the diffuse colour times:

```text
k     = least + (1 - least) min(pay v, 1)
light = max(S - (1 - b) k A, 0)^2.2 + g E occ
light += pull (Y(light) was / Y(was) - light)     was = max(S, 0)^2.2, where Y(was) > 0
if share < 1:  light = was + share (light - was)
```

Since the display sum is shown by the sRGB curve (lights/display_shader.rs; docs/spec-notes/world.md
§8 "Dark values of the sum"), the line takes `light` back to a sum, `light^(1/2.2)`, and shows
that as the sum is: `to_linear(min(to_display(diffuse colour) x light^(1/2.2), 1))`. With the
volume's change at 0, `light` is `max(S, 0)^2.2` and the line is the one without the volume.

Y is the luminance (0.2126, 0.7152, 0.0722). The occlusion is in A and once on the bounce, in
linear light. The clip to 1 per channel comes after, as before. Emissive light, the highlights, the
reflections and the fog are as before.

- **Share** is for a fade: at 0 the light is `was`, at 1 it is the new one, both exactly.
- **No support is not the stock light.** Where the read finds no point that counts, k is `least`, and
  a part of the ambient goes though nothing comes in its place. With b = least = 0.25 a fragment
  that has ambient light alone shows 81.25 % of its display sum there, 63.3 % of its linear light
  (0.8125^2.2) at full share. A uniform with b = 1, g = 0, pull = 0 and share = 0 is the stock light
  whatever the texture holds.

No value of b, g, pay, least, thin, pull or the offset is in this code: all come with the uniform.

## Who reads it

A NIF surface's main-pass pipeline in a view with the bit (`nif_material.rs` `reads_rtgi_volume`),
where the blend is opaque or alpha to coverage and the surface is none of: refracting, first-person
(arms, weapon, Pip-Boy), Pip-Boy screen, added linear light, depth-writing blend (hair), landscape.
So solid decals, actors, faces and the third-person body read. The mesh terrain blend's pipelines
give the def up again. Blended, added and multiplied shapes, water, terrain, grass and trees keep
the stock light; so do unlit shapes, which are in no lighting. A blended cut-out reads while FSR or
DLSS has it drawn as a mask and not while it is blended.

## The hook

`lights.rs` patches bevy_pbr's `pbr_functions` once, at its load: the display-value sum, the sun
visibility, the ambient occlusion, this hook (an import and the diffuse line, `rtgi_volume/hook.rs`),
the reflection gate. It is always put in, and reported as installed only once the patched shader is
stored. When a line is missing or there twice, the source stays as it was and one error is logged
(`HookState::Failed`): then no view may get a volume.

## What nothing bound means

- No view's layout has an entry at 47-49, no pipeline has the def, and every composed shader
  declares what it declares without this [tested without a GPU, `rtgi_volume/tests.rs`: bevy_pbr's
  own layouts and view key systems on wgpu's no-op device, and the shaders composed and validated
  with and without the hook, over the shader defs listed there].

What a run without a volume does differ in:

- The numbers of mesh pipeline keys: the packed fields above the new bit (samples, blend,
  tonemapping, shadow filter, projection, transmission, target format) are one bit higher. They are
  only in memory; the disk caches of pipelines hold none.
- One shader asset more (`rtgi_volume.wesl`), the edits in the text of `pbr_functions` and
  `nif_material.wesl`, and one log line ("RTGI volume hook installed").
- The cache of composed shaders is keyed by its sources' text, so the entries of shaders that use
  those two are composed once more after an update that brings this. How long that takes is not
  measured here.

Not shown without a GPU: that a bound volume reads right, that a pipeline with the def compiles on
a device, what either costs, and that the picture without a volume is the one of before.
