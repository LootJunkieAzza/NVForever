# Legacy BC5 data and normal-map consumption

[documented] Microsoft's DDS Programming Guide maps legacy FourCC `BC5S` to
DXGI_FORMAT_BC5_SNORM and `ATI2` to BC5_UNORM. A `DX10` FourCC instead introduces
an additional 20-byte header. This leaf supports the two documented legacy
codes and, since #2981 stage 1, DX10 files naming BC5_UNORM (83) or BC5_SNORM (84)
as a single 2D texture (dds.md §6), which take the same paths below. An
unverified `BC5U` FourCC alias is still not read: the guide's table does not list
it (rechecked 2026-11-19). Source (2026-11-14):
https://learn.microsoft.com/en-us/windows/win32/direct3ddds/dx-graphics-dds-pguide

[observed: dds.md §1] The surveyed game files have no DX10 header; DX10 BC5
files come from other tools (mods, texture packs).

[documented] BC5 has two independent 8-byte channels, red then green, over the
same 4x4 texels. Each channel has two 8-bit endpoints and sixteen three-bit
row-major indices. Signed endpoints use SNORM8 (both -128 and -127 map to -1).
Raw signed endpoint comparison chooses six interpolated values when the first
is greater; otherwise four interpolated values plus -1/+1. Unsigned uses 0..1
and extra values 0/+1. Interpolate floating-point decoded endpoints, not the
integer-rounded DXT5 alpha palette. Microsoft's reference keeps the raw signed
comparison even for -127 versus -128. Sources (2026-11-14): Direct3D 11.3
Functional Specification sections 3.2.3.3 and 19.5.9-19.5.12, and Block Compression:
https://microsoft.github.io/DirectX-Specs/d3d/archive/D3D11_3_FunctionalSpec.htm
https://learn.microsoft.com/en-us/windows/win32/direct3d10/d3d10-graphics-programming-guide-resources-block-compression

[hypothesis/engineering policy] The core decoder exposes normalized two-channel
floats without a normal role, Z reconstruction, green flip or sRGB. Preserve
existing header, mip/face bounds and trailing-byte policy. Generic linear GPU
images use native BC5 when supported and block-aligned; CPU fallback uses
RG8_SNORM / RG8_UNORM with rounded quantization. A color/sRGB request refuses
BC5 explicitly instead of inventing a gamma mapping. Existing formats retain
their existing paths.

Implementation context: Bevy's normal-map material path reads unsigned XYZ
(or unsigned XY for the supported two-channel formats). Raw signed BC5 values
are not that encoding. Normal slots in NIF, impact decals and landscape must
request Normal, including the landscape shader's folded layer slots. Water
height/noise remain generic Linear data. See `texture.rs`, `nif_spawn.rs`,
`nvforever/src/{decals,terrain}.rs` and the vendored PBR normal-map shader.

[hypothesis/engineering policy] Introduce Color, Linear and Normal texture roles
in every cache, worker, failure, snapshot, gate and release key. Preserve the
old bool API as Color/Linear. NIF, decal and terrain normal slots select Normal; water stays Linear. For BC5 Normal, decode signed XY (or unsigned
XY*2-1), derive positive Z with sqrt(max(0, 1-x*x-y*y)), then bias and round XYZ
to RGBA8_UNORM. This uses the existing three-component normal path, avoiding a
vendor shader change and its unclamped two-component square root. Existing
NIF green flipping remains in the material. The synthesized alpha is storage,
not a specular mask: carry original no-alpha metadata with the loaded handle
through worker/snapshot/main adoption and material construction. Non-BC5 normal
formats keep their previous bytes, format and alpha classification. This is
an engine ingestion policy, not a claim about native filtering or GPU parity.

Authored controls cover independent literal endpoint/index palettes, signed
ordering, partial block edges, mip/truncation bounds, unsigned neighbors, role
isolation, same-role worker deduplication, release, original alpha metadata and
actual material consumers. Separate roles can hold separate decoded copies of
one file; callers with the same role reuse it. This tradeoff keeps a Linear data
consumer independent from a Normal consumer. No installed texture bytes or
asset-specific substitutions enter this source. Qualification belongs to the
exact tested revision and is recorded in its PR; CPU tests do not establish
rendered or native parity.

## Authored rendered material field

Implementation: `nif_spawn::shape_material` selects the Normal
role through `StoreTextures` and generates tangents from mesh positions/UVs.
The normal map reaches the existing `NifMaterial` shader. The default `Props`
has no specular mask, so the reference image's alpha and BC5's original-no-alpha
metadata do not change this field's lighting. `fnv_vfs::Vfs::open` mounts loose
files under `textures/`; files at the root do not enter its asset namespace.

[hypothesis: engineering policy] `NVF_PLAY_TEST_BC5_FIELD=1` installs the field
in the existing normal material plugin. It requires an explicit QA
`NVFOREVER_HOME`. Its `bc5-field-output` parent may already exist; each
invocation atomically reserves a new run child beneath it (time/process identity
plus a bounded collision suffix). An existing non-directory or symlink parent
is refused. Prior runs and legacy files directly in the parent are preserved.
Every file inside the new child still uses create_new, so no evidence is replaced. Source literals generate four authored 8x8 DDS files: signed BC5S
endpoint blocks (64,-64), (-64,64), (96,0), (0,-96); independent RGBA reference
(192,63,217,255), (63,192,217,255), (224,128,211,255), (128,31,211,255);
flat (128,128,255,255); and white. Each signed block selects endpoint zero.
The reference tuples are fixed inputs, never production-decoder output. The
local VFS must return each exact payload before any field entity is created.
It has no installed archive, global texture-cache or missing-file fallback.

[hypothesis: engineering policy] Three copies of an authored two-triangle quad
use actual `shape_material` and `MeshMaterial3d<NifMaterial>` with white in
slot 0 and signed/reference/flat in slot 1. Their camera covers 6x2 world units
onto a 384x128 RGBA8_UNORM target. Render layer 31 is isolated, apart from the
normal plugin's existing zero-area transmission anchor. A foreign layer user
observed during any active update or at readback completion fails the field,
even when the received pixels otherwise pass. Only owned entities are retired.
Ambient is zero only
for this camera; exposure EV 0, directional illuminance 2 and light direction
normalize(0.25,0.35,1) are fixed. No shadows, tonemapping, dither, MSAA or
postprocess components are used. Normal world cameras and lights are unchanged.

[hypothesis: engineering policy] After 5 seconds and 10 render-schedule
completions with the extracted field camera and target, capture actual
`ReadbackComplete` bytes. Capture again at least 10 such submitted frames after
the first completion. This clock runs after render-graph submission, not on
main-loop ticks. The 30-second aid deadline fails incomplete captures. Four
24x24 regions start at panel-local (16,16), (72,8), (8,72), (88,88), away from
texture and triangle boundaries. Every signed/reference RGB difference must
be <= 1; reference/flat mean absolute RGB difference must be >= 8 in at least two
regions; every region's mean luminance must be strictly between 8 and 247;
all target alpha must be 255 and the readback exactly 196,608 bytes. Retain both
full raw files, PPM views and reports, including failed comparisons. These
predeclared thresholds are not adjusted to make a run pass.

Process: the `bc5-signed-normal-owned-field` normal GAME scenario uses an
explicit relative QA home under the smoke output directory and no environment
placeholder. Repeating the same `--out` and home creates a separate run child. Stock world bootstrap
remains read-only and separate from the wholly authored field. The scenario
requires two actual capture markers and the comparator result; shader/wgpu
errors fail. Review both complete images and record the reviewer role alongside
raw-file/source/input hashes. Assistant image inspection must not be described
as human inspection. The aid's CPU and launcher controls do not qualify rendered
pixels. No native filtering, arbitrary NIF parser, installed texture repair or
device-loss-cause claim follows from this field.
