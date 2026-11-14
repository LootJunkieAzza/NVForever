# Resolved own-image density for condition pictures

The hand-built condition producer keeps piece positions and six hit regions in
logical units (`pipboy/stats/mod.rs`, HEAD/TORSO/LIMB_PIECES). `Ctx::image_cut`
samples `w / zoom` source texels. These are NVForever source/layout contracts,
not observations of the original engine.

[observed: own installed DDS headers, read on 2026-11-14 without decoding artwork]
The base head source is 128 by 128 pixels and its shared atlas is 1024 by 1024.
An installed replacement has 512 by 512 head/torso sources and a 4096 by 4096 atlas.
These historical header facts do not establish which source `Pictures::picture`
selects, atlas packing density, or a rendered/native outcome. No assets or installed
UI definitions are included.

[design] The bounded correction applies only when the actual picture lookup returns
an own image (`rect=None`). Compute 128 logical units divided by that selected
`Assets<Image>` height, independently for the actual head, face, torso and each limb.
An atlas lookup that fails to load and actually falls back to an own image uses that
own image's size. A missing image retains zoom 1. `image_cut` uses the same Pictures
cache and source as the preceding density lookup.

[design] An actual atlas rectangle (`rect=Some`) keeps the previous zoom 1. A loose
HD DDS hidden behind a native atlas must not shrink the selected atlas piece.
No uniform-density `torso_broken` anchor or separate source-header cache is used.
An unrelated torso replacement or missing unused broken picture cannot set another
piece's density. Existing positions, logical sizes and six hit regions are retained.
HD atlas normalization, alternate logical metrics and artwork padding/layout changes
remain unqualified; this patch deliberately does not rescale atlas-backed art.

The authored CPU controls use real VFS, GameData, TAI parsing, Pictures, Ctx and the
condition producer with temporary synthetic DDS/atlas files. They cover native/4x
own images, a native 128-piece atlas alongside loose 512 sources, an unloaded atlas
falling back to own 512 images, torso-only HD and a missing unused broken-torso file
alongside a valid HD head. Actual resolved atlas/own identity and image dimensions
are checked. The original two cut-geometry controls and native/4x producer placement,
crop and all-six-hit assertions remain; two obsolete own-header-only helpers are
replaced by the actual resolved-source controls. Validation results belong in the PR.

Historical source f36/d883 hosted five-control checks and a private affected-source
comparison predate this maintainer correction. They did not distinguish an atlas
selected over loose art and do not accept this revised policy. Root/engine checks,
focused regression execution and exact current-source CI must be reported separately.
No full UI schedule, GPU output, installed scene, HD-atlas or native parity is claimed.
