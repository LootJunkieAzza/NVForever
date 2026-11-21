# Scripted texture filtering

## Contract and supported subset

[documented] SetMaxAniso takes an integer filtering level
([GECK](https://geckwiki.com/index.php?title=SetMaxAniso); the page's notes are kept in
`schemas/script/functions.json`). The page lists six accepted levels from 1 up to 16
and says other values round down to the closest lower level, but the catalogue does not
retain the full table. That missing table and exact numeric encoding require a
permitted GECK/original-game probe; they are not inferred from GPU sampler behavior.

[design] The source-only mapping accepts the existing DDS consumer's explicit levels
1, 2, 4, 8 and 16 (the engine's own anisotropy steps, not a claim about the native
table). Other values refuse before changing the queued request. This is an
intentionally partial mapping: it does not guess a sixth native level or reproduce
undocumented/default/out-of-range rounding. The refusal message is NVForever's own
wording. No native opcode or provider/version advertisement is added.

[hypothesis] The command returns 0 like other commands without a documented result;
the native return value has not been observed.

## Actual consumer and lifetime

[design] A bounded transient request retains only the last accepted level until the
existing play-side source consumer has both TextureCache and Assets<Image>. It then
changes the existing future-DDS sampler policy and eligible currently held
cache/worker-pool images. It does not scan the world or all images every frame.
Missing resources defer consumption; Game restoration does not replay a pending
request. The applied renderer policy lasts until another explicit filtering/Graphics
setting change; this application-local lifetime is mapped, not saved/native parity.

[hypothesis] The level applies to textures that are already loaded, not only to later
loads, and is not kept in the save. Neither has been observed in the original game.

[design] Loaded images retain their handles, contents, formats, mip chains and other sampler
fields. Color/linear/normal and clamped variants keep their existing independent
identities. The two worker publication paths (main-thread cache adoption and
worker-store upload) use the current policy when the decoded image is published,
so a decode started before the request cannot publish an obsolete filtering level.
The existing Graphics preference and its UI are unchanged.

[design] This is a one-shot filtering-property command, not a held overlay or a source/material
restoration authority. Later external equal-handle writes remain untouched without
another command. A new explicit command intentionally changes only filtering on
eligible current cache members, including their current effective images; it does
not identify an original source by name/label or restore prior image data. Foreign
nearest/default samplers, unrelated runtime images and removed images are not
rewritten or recreated. Images no longer represented by the current cache/worker
pool are outside this subset; no global material rebinding or new lifetime registry
is introduced.

[design] A worker can decode the same key as a main-thread cold load while keeping a
different image handle in its material. After the ordinary worker-directory grace,
that noncanonical handle stays discoverable only while an external strong holder
retains it. The existing canonical cache map and worker grace remain unchanged;
canonical handles do not keep worker entries alive indefinitely. Removing the last
material allows the noncanonical entry to retire on the next sweep. This is bounded
engine ownership policy, not an inferred native texture-lifetime rule.

## Authored controls

Three core controls use the public source-query path for supported values, last-call
coalescing, strict refusal, ordinary non-strict recovery, the existing VM empty-
receiver boundary, unchanged World data and actual JSON restoration. Six asset
controls exercise real DDS/cache roles and handles, cold loading, both worker
publication paths, an in-flight producer, unchanged foreign sampler fields and
external writes/removal. A deterministic worker/main-key race also checks a real
material after directory grace, and a separate canonical-worker control retains
ordinary expiry and cleanup after the last material owner. Two headless play controls connect actual source requests
to the registered sampler consumer and verify deferred resource admission.

The original game's accepted-level table, rounding, encoding and loaded-texture/save
timing remain probe gaps. GPU appearance, full SetMaxAniso behavior, installed-mod
support and dependency-popup removal are not claimed. No original DDS, game data,
private paths or captures are included.
