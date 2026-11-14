# Original NVForever touch action families

These 104 SVGs are original NVForever geometry, authored with Codex assistance under
GPL-3.0-or-later (the root LICENSE). No game artwork, brands, logos or third-party icon
library were copied. Companion and Aid are artwork identities for future controls;
this asset set does not add game actions or bindings.

The four families have different drawings and negative spaces:

- Precise line: light continuous outlines, an open boot/upward Jump and framed-person VATS.
- Bold filled: solid silhouettes with generous transparent counters and separated masses.
- Rounded: soft closed forms, a horizontal projectile, paired airborne boots, a pill wrist
  display, dotted focus reticle, filled selected person and compact crouch.
- Industrial: chamfered hard planes, double directional chevrons and detached radial tiles.

Every family uses a common 32 by 32 viewBox and the same 26 semantic IDs/legacy indices.
The manifest declares its stroke/fill profile. The strict nvf-touch-svg/2 compiler rejects
unsupported semantics rather than approximating them. Even-odd compound paths provide
real transparent holes; all output remains a tintable white coverage mask.

SVG source is authoritative. Generated Rust geometry belongs only in Cargo OUT_DIR;
preview sheets belong in a private review directory. The runtime rasterizes at actual
physical content-box dimensions after layout, with the selected family in its bounded
cache key. Family selection changes artwork only, independently of skins, placements,
control bindings, hit targets and opacity/tint. No artwork or final layout is locked by
these review previews; rendered device feedback remains pending.
