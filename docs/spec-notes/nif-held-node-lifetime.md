# Lifetime of script edits to drawn NIF nodes

- [documented] SetNifBlockScale and SetNifBlockTranslation operate on the calling
  reference's loaded model. Their public notes say changes reset on parent-cell
  reload or Update3D: <https://geckwiki.com/index.php?title=SetNifBlockScale> and
  <https://geckwiki.com/index.php?title=SetNifBlockTranslation>. Root-rebuild and
  outfit-specific native behavior has not been measured here.
- [observed: source at 54830c0d] world::travel despawns loaded cell roots; exterior
  stream retirement also despawns their children. Returning spawns the same placed
  reference with new actor/node entities. nif_mirror::Memory keeps transforms by
  reference/view/name across that destruction and applies them to the next matching
  name. No model or node entity generation belongs to the stored transform.
- [observed: source at 54830c0d] The script mirror and backend consumption occur at
  different times. Binding only at consumption allows an edit queued against an old
  published model to act on a replacement model created in between.
- [observed: source at 54830c0d] Titans' plain mod scripts use these setters for
  armor bones and Pip-Boy positioning; the private source audit records the selected
  SCPT consumer IDs and hashes. No native plugin implementation informed this design.
- [observed: source at 54830c0d] Save/main-menu handoff starts a new process, so this
  is a drawn-node lifetime defect, not evidence of a cross-session memory leak.
- [observed: source at 54830c0d] Vanilla Update3D is a no-op in nvse_amata; player
  Update3DAlt can enqueue an appearance request without forcing an unchanged body to
  rebuild. Those separate API gaps are not fixed or qualified here.

[design: private candidate, execution pending] A renderer publication associates each
node with an opaque model/node identity. Core holds integers supplied by the backend,
not Bevy entities, native pointers or game memory layouts. The edit queue captures
that identity while the script operates on the published node. The renderer refuses
a stale or missing target rather than resolving its old name onto a replacement.
Held transforms belong to the concrete node as well as its model, covering a child
replaced under the same root. Both selected root and node must currently satisfy
the actual node query; cached rig enumeration is not proof of liveness. Leaving
the model subtree expires the target, while a sibling reorder or reparenting within
the same subtree does not. A camera switch alone changes neither identity.

[design: private candidate] Legacy/headless set_tree hosts retain their existing name-only
queue contract; renderer publications explicitly opt into identity enforcement.
The actual entity-backed renderer refuses Legacy even for a never-published actor;
there is no startup window in which an unknown name gains future authority. Only
currently drawn-flat file models admit a separate, unqualified legacy path.
That path retains transform remembering before any concrete root lookup and its
original non-transform name/index handlers. Material metadata updates can precede
a failed concrete shape lookup, as before. Existing legacy held-file values retain
their original name-based replay semantics; their lifecycle or representation changes
are not qualified by the entity-backed fix.
Existing take_edits remains available for these hosts, while the real renderer uses
the identity-preserving consumer. Same-frame insertion/attachment before a node has
been published is not treated as a safe identity binding; those structural-order
semantics remain outside this patch. Refusal must not be described as native parity.

Authored controls use the actual CPU mirror/PlayerBody producer: queued edits
before replacement, direct replacement without an absent frame, disappearance and
reappearance, child removal/recreation, unchanged-rig hold across pose/POV changes,
independent first/third rigs, and unrenamed factory labels. Additional controls
exercise stale selected roots, root/child Transform removal and readdition, indexed
child reorder with an untouched sibling, and positive rename followed by a transform.
The child-replacement control explicitly reuses an Entity index at its next generation
through Bevy's public despawn_no_free/spawn_at API; the fixture asserts both conditions.
Legacy file controls seed authored metadata and use the real producer; they do not
qualify NIF parsing or GPU material drawing. Positive non-transform handler controls
assign RefVisual to a synthetic parent and its shape so unchanged root_of/walk can
visit that shape. Normal dropped-flat spawning tags mesh entities, not this parent;
these controls do not qualify that actual spawn topology. The absent-root transform
control deliberately supplies no RefVisual and exercises file metadata only. Independent core controls
will verify queue capture and compatibility without depending on renderer helpers.

All controls are NOTRUN at this private R3 source freeze. Nine added producer tests
and two added core tests are authored; test counts are source counts, not executions.
Existing per-view/root-rename controls remain unchanged. The five original R1 producer fixtures can compare against original 548 production
in isolation. The additional stale-root control directly inspects the new held map;
a complete R2 comparison therefore requires retaining that test-visible candidate
API while reversing the relevant production behavior, or a separate fixture-only
packaging. The new core identity API controls likewise require the candidate API.
Neither is represented as an unchanged-base compile-failure test. Player rigs that survive a cell transition without node replacement
are not reset by this identity change. Material-cache lifetime, complete
Update3D support, AttachModel, native timing, GPU rendering, Loop255 and complete
armor-HUD behavior remain unqualified.
