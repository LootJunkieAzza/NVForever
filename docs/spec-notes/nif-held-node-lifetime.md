# Lifetime of script edits to drawn NIF nodes

- [documented] SetNifBlockScale and SetNifBlockTranslation operate on the calling
  reference's loaded model. Their public notes say changes reset on parent-cell
  reload or Update3D: <https://geckwiki.com/index.php?title=SetNifBlockScale> and
  <https://geckwiki.com/index.php?title=SetNifBlockTranslation>. Root-rebuild and
  outfit-specific native behavior has not been measured here.
- [documented: repository source] At `54830c0d`, `world::travel` and exterior
  retirement despawn loaded roots and children; `nif_mirror::Memory` keeps held
  transforms by reference/name. Returning can give the reference new entities.
  See `engine/nvforever/src/world.rs` and `engine/nvforever/src/nif_mirror.rs` at
  that commit. This is a reading of NVForever source, not an original-game result.
- [documented: repository source] Main `805454db`, `nif_mirror::mirror`, stores
  translation/rotation/scale in `held` before resolving a drawn root. The later
  held loop resolves by name each frame. A one-shot setter can therefore precede
  the first model publication, or the appearance of the named node. Structural
  edits instead resolve an already drawn root once; a missing target is dropped.
- [documented: public review] The maintainer identified that `b24124e2` dropped
  `SceneTarget::Legacy` and `Missing` in the entity-backed producer, breaking the
  preceding startup-write behavior. Its unpublished-actor test also expected the
  wrong drop. Source: [PR #2419 review](https://github.com/LootJunkieAzza/NVForever/pull/2419#issuecomment-6874990859).
- [documented: repository source] At `54830c0d`, main-menu/save handoff starts a
  process, vanilla Update3D is a no-op in `nvse_amata`, and player Update3DAlt may
  enqueue an appearance request without rebuilding an unchanged look. See
  `engine/nvforever/src/main.rs` and `crates/fnv-game/src/nvse_amata.rs`. These are
  separate implementation limits, not native observations or fixes in this work.

## Compatibility and lifetime policy

[hypothesis: portable engineering policy, not native parity] Keep startup writes
while preventing a hold from transferring to a later namesake. Before any concrete
model has been published, TRS writes wait by reference, selected view and name.
As soon as an actual model root is available, capture its entity generation even
if the named child is absent. Bind the first actual matching node in that model,
then replace the pending name with the existing model/node identity. Root/node
replacement, an observed disappearance, or departure from the model subtree
expires this hold. Reordering or reparenting inside the same subtree does not.
Camera switches cannot redirect a queued concrete view.

[hypothesis: engineering ordering policy] Promotion must preserve chronological
last-writer order independently for translation, rotation and scale. Different
pending names can resolve to one node; an old pending component must not overwrite
a newer bound component, and a newer scale must not erase an older translation.
Entity edit order is renderer-local and checked rather than wrapping. Exhaustion
refuses further entity edits instead of reversing chronology. File-only held state
does not use this new ordering metadata.

[documented: implementation boundary] The model token is exactly the existing
`SceneIdentity.model`: the selected `root_of` entity's bits, including generation.
PlayerBody rebuilds despawn their old rig roots (`player_body.rs` at `805454db`),
and dropped-model replacement despawns its prior root (`nvf-play/src/dropped.rs`
at that commit). No additional asset-load generation is introduced. A custom
topology that replaces a model underneath the same surviving root without an
observable root lifetime change cannot be distinguished while the pending name
has never bound a node. That case is not qualified as a replacement safeguard.

[documented: producer source] A retained rig resource can name a root whose
Transform no longer satisfies NodeQuery. Publication forgets that model instead
of emitting an empty-tree Model token. An edit queued after that observed absence
remains Missing even if Transform returns before consumption. A new setter after
valid republication still binds normally. This is an observed publication boundary,
not the separately excluded removal/reinsertion between publications. An authored
producer control covers both the negative and fresh-setter positive; execution
of this R3 addition is pending.

[hypothesis: portable engineering policy] A published model with a missing name
permits a pending TRS write only with that model's captured identity. It may bind
a node added after publication, including a node created by deferred Insert in
the same frame. A previously published model that disappears remains identified:
forgetting its tree must not reopen unrestricted cold name binding. No readable
tree is invented for a pending write; cold getters retain their missing value.

[hypothesis: compatibility policy] Before publication, non-TRS operations retain
main's consume-once name/index lookup on an already drawn root for queued edits.
Public structural dispatch still requires a mirrored target; this does not add a
new cold script admission route. Missing structural targets are not kept for
future destructive replay. Published concrete targets
still require their captured node generation and actual subtree membership.
This does not introduce Created reservations, attachment loading, persistent
handles or a component-reincarnation counter.

[documented: repository source] `SceneMirror::set_tree` and `take_edits` retain
the legacy host API. The renderer's drawn-flat file branch keeps its original
name-held transforms and material/particle handlers. Its authored controls seed
file metadata and a synthetic parent/shape graph; they do not qualify actual
flat spawning, NIF parsing or GPU output.

## Qualification

[documented: authored test record] Hosted CI for prior `b24124e2` passed eight
core controls and 13 producer controls once each, alongside the canonical root
and engine suites and strict checks ([run 43557253676](https://github.com/LootJunkieAzza/NVForever/actions/runs/43557253676)).
Those passes did not establish startup-write compatibility: one producer control
asserted the erroneous unpublished-actor drop. This repair changes that assertion
to require the cold write, preserving its later replacement-actor negative.

[documented: authored test scope] The correction adds actual mirror-producer
controls for cold player selection, actor TRS before drawing, a late child in an
empty published model, immediate replacement of that captured empty root,
pinning a pending hold before the child exists, deferred Insert
then TRS through public dispatch (including its not-yet-drawn core placeholder),
alias promotion with newer bound/per-property edits, and consume-once
cold structural operations. Existing stale-generation,
root/child query-liveness, subtree, rename and view-isolation controls remain.
Core controls distinguish a published missing name from a forgotten identified
model. These corrected controls are authored and have not yet been executed at
this source freeze; old CI passes are not attributed to the corrected tree.

[documented: validation boundary] Producer tests use authored ECS graphs and
PlayerBody rigs through the real `mirror_game` implementation. They bypass
ScriptState acquisition, asset loading, rendering and native code. A baseline
comparison must use compatible common fixtures and actual old production behavior;
a missing new helper/API compile failure is not behavioral evidence. The Game-local
core fixture belongs in the combined integration binary; it changes no global
file reader, environment or process-wide root configuration.

[documented: pending checks] The maintainer requests unchanged sequential smoke
scenarios `nif-player-model-selection` and `nif-script-root-renames`; neither has
run for this repair. Full Update3D support, unchanged-entity Transform removal and
reinsertion between publications, material/property/vertex cache lifetime, native
timing/parity, special-IDLE lifetime and complete armor HUD support remain separate.
