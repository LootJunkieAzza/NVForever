# Source density for condition pictures

[documented: `engine/nvf-play/src/pipboy/stats/mod.rs`, `condition`, `HEAD`,
`TORSO` and `LIMB_PIECES`; `engine/nvf-play/src/pipboy/frame.rs`, `image_cut`]
The hand-built condition page positions its pieces and hit regions in logical
units. Its crop previously treated each source texel as one logical unit before
the page zoom: a requested width `w` sampled `w / zoom` texels.

[observed: own installed DDS headers, read on 2026-11-14 without decoding artwork]
The base head source is 128 by 128 pixels and its shared atlas is 1024 by 1024.
An installed high-density replacement has a 512 by 512 head and torso source,
and a 4096 by 4096 shared atlas. The shared atlas's dimensions therefore do not
describe a single piece's logical dimensions. No assets or installed UI definitions
are included in this contribution.

[hypothesis: uniform-density replacements preserve the existing logical metrics]
Use 128 logical units as the paperdoll source-height anchor, with the own
`torso_broken` DDS height as the density measurement. Multiply only the condition
pictures' sampling zoom by `128 / source_height`. For a fourfold replacement this
samples four times as many texels per logical edge while retaining the existing
node positions, display sizes and six limb interaction regions. This is a bounded
layout policy, not a native-engine parity claim or a general replacement-layout
interpreter. Nonuniform replacement sets and alternative logical metrics remain
unqualified.

[documented: `engine/nvf-play/src/interface.rs`, `own_texture`, `Pictures`;
`engine/nvf-assets/src/data.rs`, `GameData`; `crates/fnv-dds/src/lib.rs`, `Dds::parse`]
Read the normalized own-file path through the same game-data reader used by UI
pictures, validate the complete DDS through the existing parser, and cache the
optional dimensions on the session's `Pictures` object. Do not substitute an
atlas rectangle's trimmed or packed dimensions. A missing or invalid source
retains the previous sampling zoom. A fresh `Pictures` starts with an empty
metadata cache; live asset hot reload within one session is not promised.

[documented: the authored `paperdoll_*` tests in `interface.rs`,
`pipboy/frame.rs`, and `pipboy/stats/mod.rs`]
The five CPU controls cover normalized read caching, a fresh session, invalid or
incomplete DDS input, proportional and clipped crop geometry, and the actual
VFS-to-GameData-to-Ctx-to-condition producer. The producer creates synthetic DDS
bytes in its own temporary directory and compares the head crop plus all six
interaction regions at densities one and four. No game data is used. These tests
are authored but have not yet been compiled or run for this contribution; full UI
scheduling, GPU output, installed scenes and native comparison remain untested.
