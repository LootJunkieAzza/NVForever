# Whole-save refusal for the temporary CSTY preview

- [documented: source f30fb730] GameSnapshot saves reference extras, script state
  and ExtStore contents but not TempCloneForm's alias graph. The private R2 session
  style maps likewise have no persistence closure. A style alias may appear in an
  extender array or other carrier even when no actor is assigned it.
- [hypothesis: preview safety policy] Refuse a whole save while any temporary CSTY
  copy is tracked. Check both retained copy metadata and declared clone origins;
  do not infer support from a live-reference-only scan. Keep known copies unsafe
  even if their alias relation was removed. Do not filter/remap aliases, discard
  tombstones or exempt the generic FE namespace.
- [hypothesis: implementation policy] Expose the uniquely named
  ensure_combat_style_snapshot_supported guard. Engine saving invokes it after
  SaveGame callbacks and before snapshot construction, encoding or enqueueing.
  Headless SaveLoad invokes it before its existing snapshot/encode path; that path
  has no SaveGame callback on f30 and this change does not add one.
- [hypothesis: composition policy] The guard is independent of the scene-clone
  checked_snapshot work (#2416). When both are present, preserve both guards after
  callbacks; never replace the scene check with a CSTY-only policy. Raw snapshot
  construction remains a diagnostic primitive, not an approved save boundary.
- [documented: source policy] Plain authored style field edits remain session-only:
  the existing snapshot/restore path omits raw/typed overrides and resets them.
  No alias graph or new style state is serialized by this preview.

Creating a temporary style, even one never assigned to an actor, consequently
makes this preview unsaveable until the session resets. That is a conservative
draft limitation, not finished mod compatibility or native save behavior.

[hypothesis: owned controls] Direct guard controls retain aliases in actual
ExtStore arrays, deleted-reference extras and retained copy metadata. The real
engine save entry point must reject a clone created by its SaveGame callback,
without enqueueing or replacing an existing owned test file; an authored-edit
neighbor must still enqueue. The actual headless SaveLoad helper must reject a
clone before roundtrip, while plain authored edits still roundtrip with the old
reset policy. All new controls are unrun until separately qualified.
