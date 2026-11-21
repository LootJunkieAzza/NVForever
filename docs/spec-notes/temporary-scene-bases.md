# Temporary non-actor scene bases — initial scope

[documented] [TempCloneForm](https://geckwiki.com/index.php/TempCloneForm) creates
temporary base forms; its documentation and [extender save notes](ext-save.md)
state that these bases are not saved. This does not specify every property of a
reference placed from such a base; original-game lifetime controls remain needed.

[observed: portable 19c0f08a source] TempCloneForm stores a Game alias while
World::place admits only authored FormStore signatures. Thus an owned static
clone can report IsClonedForm but fail PlaceAtMe with unknown base. Merely using
the original ID for placement would lose the clone's identity and model override.
Drawing and collision independently resolve immutable records, so admitting the
ID alone would still leave the created reference without its chosen model.

[hypothesis: initial portable policy] Admit direct temporary clones of supported
non-inventory, non-actor scene bases through an explicit session-base descriptor.
Preserve the alias as RefState.base; use its known original for immutable type
metadata and its current model override for the created drawing/collision model.
Ordinary authored placement stays on its existing path. Reject unsupported clone
shapes explicitly rather than silently placing the original.

[hypothesis: save contract unresolved] Base nonpersistence does not establish a
safe policy for references created from that base. Silently omitting those references
or their tombstones can leave saved variables, quests, ownership and other references
dangling. That proposed omission policy was rejected in independent review before
production changes. Audit actual save/roundtrip boundaries and reference closure.
An interim candidate must explicitly refuse unsupported saves before enqueue/write,
after SaveGame callbacks, or implement a reviewed reconstructible contract. Neither
choice establishes native save parity or full playable B42 support.

[design] Owned controls must exercise the public source call, preserve the clone
base identity, route drawing and physics to the same selected model, and check
ordinary-reference independence plus explicit safe save/restore boundaries.
Test unknown and unsupported bases explicitly. Required engine/native checks
remain pending; no rendered/native run or complete B42 operation is established.

[hypothesis: remaining work] Clone-of-clone edits, post-placement model reload,
NPC/inventory cloning, all per-record traits, IDLE admission and complete save
parity are separate. They must not be implied by a nonzero placement return.

[hypothesis: historical interim guard] A checked Game snapshot inspects the retained
reference bases and deleted-created base tombstones against its clone table. Both
actual engine saving and the headless save/load boundary must use it. Engine
SaveGame callbacks run first; rejection then happens before serialization/enqueue,
leaving an existing file untouched. This deliberately refuses unsupported saves
instead of deleting reference closure. It is not a playable/native save solution.

[design: preview scope] Direct clones of STAT, MSTT and PROJ may use a session
descriptor, as inert model references only. Activator, talking-activator, door,
furniture, inventory and actor interactions are not admitted. PROJ admission does
not turn the created reference into a flying shot or establish complete B42 use.
The descriptor stores the authored original and the effective pre-placement MODL
override: clone-local first, then the original's session override. Explicit empty
is distinct from no override. Both created drawing and collision use that descriptor;
the selected path determines model kind, including an authored .spt replaced by NIF.
Post-placement model and script changes on both alias and its inherited original,
and clone chains, are explicitly refused in this candidate. Unrelated ordinary
placement and setters remain unchanged. The alias
itself stays the reference base. No screenshot, collision or consumer pass is yet
claimed; the implementation must remain a draft until the remaining validation
and persistence semantics are qualified.

[observed: source audit] ref_script/base_script do not inherit an alias's authored
original script. This preview therefore refuses originals or aliases with attached
scripts rather than create a reference that silently loses its script. General
script inheritance remains outside this initial admission; later script edits to
a placed alias or its original are also explicitly refused.

[hypothesis: historical practical save limit] A deleted clone reference keeps its base tombstone.
Consequently the guard still refuses saving after deletion, until an appropriate
load/reset replaces that state. This provisional safety policy is not a playable
persistence solution and must not be staged as a completed mod repair.

## Historical source port

[documented: qualification scope] This provisional source port is based on exact
3dccb3c1c633e7d5eb06501de98dc580d7cda8a2 and is Part of #2391, not a
complete implementation of that issue. Earlier 19c core and selector controls are
historical evidence only. Compilation, tests, rendering and native controls on
this port have not run. Whole-save refusal, including deleted-created reference
tombstones, remains intentional until a complete persistence contract exists.
This is not a playable persistence or complete B42 acceptance claim.


## Deleted inert references: bounded persistence proposal

Repository context: exact main1790ab94a5ae8a8a4ef56b84e3454c3363d78866 keeps
deleted created references in World snapshot deleted_created, mapping the original
reference to its base. The temporary scene descriptor and Game clone map are
session-only. The checked save boundary therefore rejects even a deleted
reference. This is an intentional existing guard, not evidence of native behavior.

[hypothesis: engineering persistence policy] Admit only deleted created references
whose direct STAT, MSTT or PROJ clone already has an admitted inert scene descriptor.
Retain the exact tombstone reference/base pair, authored original, supported kind
and optional effective model override. Preserve empty override separately from no
override. Do not replace the base with its original, remove the tombstone or revive
the temporary form. Restored provenance is private bookkeeping: it is not inserted
into the clone lookup, temporary World base table, renderer or script instances.
Saved variables can retain the old ID; the base remains invalid after load.

[hypothesis: version policy] Ordinary snapshots remain fnv.game.state/1. A snapshot
with retired provenance uses fnv.game.state/2-retired-scenes and a clone-sequence
reservation. The feature tag deliberately leaves the existing /2 unknown-version
test aid unsupported. Older readers reject the new format through their existing
format check instead of silently ignoring the extra metadata. New readers reject
provenance on /1, the feature format without provenance, invalid alias ranges or
sequence reservations, orphaned or live aliases, overlapping reference identities
and reference allocators that would reuse a retired ID.

[hypothesis: identity policy] The shared clone sequence advances beyond all clones
allocated before that save; only retired scene descriptors are retained. Allocation
skips loaded-record, existing-clone and reserved retired identities. It fails before
overflowing the local index-zero range or the existing compiled-unit namespace at
FE200000. This bounds the existing allocator; it does not declare an original-game
capacity or persist unrelated unused clones. Ordinary snapshots without retired
provenance retain their existing temporary-clone invalid-ID behavior.

[hypothesis: load scope] Identical load orders, including plugins appended after the
saved list, may restore this format. Reordered or missing saved plugins explicitly
refuse before the generic fnv-save FormID walker can reinterpret alias identities.
Ordinary saves retain their existing name-based plugin remapping. Restoration
checks the actual loaded original kind and script admission, and refuses authored
alias collisions. A loaded retired alias stays unavailable to scene placement.
This first policy does not claim arbitrary load-order migration.

Live scene clones, clone chains, unsupported/scripted or inventory placement, and
unadmitted aliases keep their existing refusal boundaries. SaveGame callbacks
still run before checking; the engine refuses unsupported state before enqueueing
or replacing a file. No gameplay input, save-request scheduling, native version,
physics, rendering or clone combat-style policy changes.

The shared authored comparison uses the actual TempCloneForm, SetModelPath,
PlaceAtMe and MarkForDelete source calls, checked snapshots, World/Game restoration,
the engine SaveGame callback and fnv-save container writer/read-for-load. It
asserts exact IDs/provenance, no clone revival, independent originals, repeated
saves, allocator reservation, malformed/collision refusals, ordinary unused-ID
carriers and ordinary plugin remapping. The previous tombstone-refusal assertion
is explicitly replaced by the proposed successful headless save boundary on both
comparison sides; its original main body is retained in preparation evidence.
No passing count is implied by these source controls. Actual CPU, full-profile
save/load, rendered and native qualification belong to an exact executed revision.
An installed temporary-reference creator has not been attributed by this proposal;
it is not a complete B42 or #2391 implementation.


## Source-bound CPU qualification

[implementation evidence] The historical source-port and guard qualifications
above describe their original checkpoints. The bounded retired-scene candidate
has since completed two separate controlled Windows CPU comparisons on public
main `1790ab94a5ae8a8a4ef56b84e3454c3363d78866` plus the authored fixtures.
These are NVForever implementation results, not original-game observations.

The Rust 1.97.1 core comparison ran the same 19 integration controls on both
sides: baseline 12 passed and seven failed at their first temporary-base
snapshot/save refusal; candidate 19 passed, with zero ignored on either side.
Later assertions in each failing baseline case were not reached. Ordinary, live,
scripted, inventory, chain and unused-clone compatibility controls remain part
of that comparison. An earlier compile timeout ran zero cases and supplies no
regression outcome.

A separate normal Rust 1.99.0 engine-workspace comparison ran the same three
SaveNow/container controls on both sides: baseline one ordinary-remap control
passed and two failed at their first checked-snapshot refusal; candidate three
passed, zero ignored. It exercised SaveGame callbacks, enqueue admission, the
actual save-container writer/reader and World/Game restore. Later baseline
container/restore assertions were not reached after those refusals. These
engine results are not a combined 22-case executable or a reuse of the core
binary.

The public-source adoption retains the authored production and fixture bytes.
Documentation changes and a later current-main composition have not been
compiled or executed by this adoption. Broader suites, strict lint, installed
GAME scenarios, native parity, existing user-save recovery, rendered/collision
behavior and complete B42/profile acceptance remain unqualified.
