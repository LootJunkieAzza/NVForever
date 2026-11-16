# Drawn script model attachments

Implementation context: at pinned public main
`63db0209cc77ab7ce45eed44a90fecf925e47dd1`, `nif_blocks::call` records
AttachModel specifications but `nvforever::nif_mirror` does not draw them. Modes
and runtime lists are documented in [ext-jippp.md](ext-jippp.md); this change
implements the reference attachment consumer, not a native plugin ABI.

## Consumer and ownership

[hypothesis: engineering policy] A reference attachment is a registered structural
intent plus one owned instance in the selected concrete model. Mode 3 can create
it in the current model, waiting for an initially absent parent. Mode 1 waits for
the next model if one has already been published. Mode 0 stops future creation
but retains an already drawn instance. Mode 2 also removes that exact instance. The existing
script return values and runtime-list API remain unchanged. Legacy/headless edit
drains also retire unconsumed structural side requests; the drawing producer takes
them before its mirror drains ordinary edits. The omitted player
view retains the public mirror's third-person default; this adds no selector.

[hypothesis: engineering policy] A first available model pins a cold request.
An observed or synchronous removal of its Transform, or of a captured parent,
retires its instance. A different parent with the same name does not inherit it
within that model. A pending parent request invalidated before its first creation
does not gain authority in a replacement model; a fresh request is required. A
new model can replay structural intent that already produced an instance; old
translation/rotation/scale holds retain the existing concrete-entity lifetime
and are not replayed; attachment material values belong to each produced entity.
A fresh explicit registration permits a parent-lifetime retry. Model preparation
retains the existing cache and its remembered failures. Missing asset services
can arrive while a request waits, but an attempted preparation cached as `None`
is not retried merely because a file later appears. The ordinary sweeper keeps
failed entries; model loading needs a fresh cache or explicit cache reset, and
a failed intent requires fresh registration (or an eligible model
replacement) before another preparation attempt. Repeated registration of
a live instance does not duplicate geometry. Session replacement retires all
old owned instances. Exact ownership, not a display label, controls mode 2.

Implementation context: `ModelCtx` owns model preparation and
its cache and sweeper. Mirror-only hosts may not yet have the asset resources;
the producer still captures queued recipients and processes session/lifetime
retirement and explicit removal. Only model preparation waits for those resources,
so asset arrival does not revive an expired recipient. A separate attachment variant retains the source graph;
ordinary and streamed models do not allocate it. Attachment node metadata and
drawable indices are captured from the same decoded file during preparation; there is no second file read joined by
name. Duplicate and absent names therefore do not collapse geometry, materials,
local transforms or roots. Static models retain every source root. Animated
models use the existing Rig spawn path for embedded idle animation and material
tracks; an incomplete rig-to-source correspondence is refused before any entity is created. Existing
model, texture-role, material and animation preparation remains authoritative.
The host's GroupRig, actor PlayGroup routing and collision-pose publication retain
their sole reference ownership. Attached named-group dispatch is not enrolled as
a second unqualified GroupRig for the same FormId.

[hypothesis: engineering policy] The producer validates the complete cached graph
before issuing Commands. Removal retires only entities bearing this instance's
ownership marker, preserves foreign children, and never searches by names.
Dependent owned attachments retire in the same producer pass rather than drawing
one frame as unparented objects. The
producer preserves registration order for duplicate-name publication. Its bounded
ancestry checks accept at most 255 parent links and refuse deeper attachments before
spawning; checked instance IDs never wrap. The mirror publishes real resulting
entities and attachment-local material/property metadata only after the Commands barrier. Newly created node metadata is not
invented before the producer runs. The existing AttachLight path is independent. AttachFormModel retains its legacy
runtime-list and ordinary-edit behavior; it does not enroll this per-reference
producer. Its physical lifecycle remains unimplemented by this leaf.

## Bounded coverage

Authored controls use compiled AttachModel/NIF queries, synthetic NIF bytes,
actual ModelCtx/mesh/material assets and the production producer/mirror. They
cover missing files/parents, replacement and same-entity Transform removal,
rename/namesake isolation, duplicate/unnamed shapes, multiple static roots,
mode 0/1/2/3, cache source consistency and neighboring owners. Additional controls
keep AttachFormModel mode 3 followed by 2/0 outside the physical consumer, with
a real AttachModel positive, and queue a foreign-child despawn before actual
attachment cleanup while preserving a surviving foreign sibling. No game assets,
DLL implementation or native addresses are used. Qualification is tied to the
exact source and execution receipt; source alone establishes no rendering result.

Base-form-wide replay, CopyRuntimeNodes/ClearRuntimeNodes ownership semantics,
particle simulation, attached named-group dispatch, legacy alpha/stencil override
lifetime, native parity and
full B42 mod acceptance are outside this reference-consumer leaf. No capability/version spoofing or silent original-model
fallback is introduced.

## Opt-in physical fixture

`nif-reference-attach-owned` uses a separately admitted, authored two-shape NIF
and fresh HOME through the canonical smoke launcher. An opt-in child of the
mirror reports ordinary `ModelCtx` preparation, first post-transform CPU mesh
geometry and bound material fields, followed by actual recipient ancestry,
captured entity identities, retention and removal. It does not consult producer
ownership state to declare success, retain CPU meshes globally, or compare pixels.
With the opt-in absent, it installs no system or resource.

The scenario and `tools/smoke/fixtures/attach-model-owned/judge_fields.py` must
both accept the same complete log. Neither a registered request, a getter, a
manifest hash nor a recorder receipt substitutes for those physical fields.
Input creation and admission refuse resolved output/source-Data overlap; selected
masters and archives remain linked read-only by contract and checked hashes.
The fixture README describes the explicit admission step required before launch.
This authored diagnostic does not establish native behavior or a mod profile's
acceptance. Execution qualification belongs to the exact source and artifact.

For this empty-target fixture, the core queues the attachment change before
looking up a mirrored node for its numeric return. With no empty-named node the
return can be 0; with such a published descendant it can be 1. The physical
producer separately accepts the model root when no captured node supplies the
parent. The smoke accepts either submission return and does not use it as a
physical-success oracle. No producer return or target policy is changed.
