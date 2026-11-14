# Session combat-style edits and temporary clones

- [documented] The public catalogue gives SetCombatStyle an actor reference and a
  CSTY form. Get/SetCombatStyleTraitNumeric expose IDs 0–63, and
  Get/SetCombatStyleFlag expose the combat-style flag bits. Sources:
  `schemas/script/functions.json`, [jip-traits-4](jip-traits-4.md),
  [ext-lutana](ext-lutana.md), and the public pages
  <https://geckwiki.com/index.php?title=SetCombatStyle>,
  <https://geckwiki.com/index.php?title=SetCombatStyleTraitNumeric> and
  <https://geckwiki.com/index.php?title=SetCombatStyleFlag>.
- [documented: repository source f30fb730] Typed AI consumers currently read
  immutable GameData.combat_styles. JIP setters write separate raw field overrides,
  while TempCloneForm creates an alias lacking an authored CSTY entry. Assignment
  refuses that alias; raw setter success is therefore not evidence an AI consumer
  used its values.
- [hypothesis: implementation policy] A temporary CSTY clone copies the source's
  effective typed style and its public raw trait/flag read values at creation.
  Pre-clone edits are included, later original/alias edits are independent, and a
  clone of a clone copies that clone's effective state. This defines the session
  policy under test; native copy timing is not measured here.
- [hypothesis: implementation policy] Use Cow over CombatStyleDef: immutable
  authored styles borrow GameData, while edited and cloned styles return an owned
  copy of scalar/tuple/array fields. Update typed state only on valid CSTY writes;
  do not parse JSON, allocate leaked objects or retain a mutable Game borrow when
  the combat AI reads a style. Keep package, explicit SetCombatStyle, actor Traits
  source and DefaultCombatstyle precedence, skipping invalid style candidates.
- [documented: current data model] Public raw getters and the typed parser have
  different defaults and representations. For example, missing range multipliers
  read as zero through the generic raw getter but default to one in typed AI data.
  Keep those public raw semantics, including record_float precision, separate.
  Clone raw snapshots copy the getter values rather than reconstructing them from
  typed defaults. Session setter values remain f64 for public reads, while typed
  consumers retain the existing f32/u32 representation. This is not new native
  quantization parity.
- [documented: repository source f30fb730] RecordFields uses numeric_fields and
  enums_to_numbers before the public numeric getter. The authored CSSD weapon
  restrictions enum 2 therefore reads as 2.0, rather than zero from an intermediate
  decoder label. This is an NVForever source fact, not an original-game observation.
  The corresponding authored/copy expectations preserve that existing reader.
- [documented: current data model] Trait 40 (acrobatic dodge chance) has a public raw
  getter/setter but no CombatStyleDef consumer field. Preserve its raw behavior and
  clone value; do not claim an acrobatic-dodge AI implementation. Unconsumed flag
  bits likewise remain raw, without inventing gameplay effects.
- [hypothesis: safety policy] Validate a style through an allocation-free alias
  walk bounded by the actual clone-map size and terminal lookup. Wrong record
  types, unresolved aliases and cycles cannot become valid styles. Do not rewrite
  global unclone behavior or infer that a mod has produced a cycle.
- [hypothesis: scope boundary] Preserve SetCombatStyle's existing target admission;
  this candidate validates the supplied style without adding an actor-base target
  gate. General caller/reference validation and cloned actor-base support are
  separate work. The R2 correction removes an unexecuted R1 target check because
  it was unrelated to accepting and reading effective CSTY forms.

## Owned controls and limits

[hypothesis: test design] Authored source API controls cover original pre/post-copy
edits, independent aliases and clone chains; all 64 raw traits, consumed typed
fields and flag bits; missing-field defaults and precise f64 setter reads;
non-CSTY/invalid assignment; package/set/base/default precedence; and a real core
weapon-selection or melee-decision consumer. Fixtures contain no installed mod
scripts or game assets. Raw getter assertions alone are insufficient AI acceptance.

[documented: repository source 805454db] Alias graphs and raw overrides are not
part of the existing snapshot format. The new typed session store is likewise
not serialized. Saved ExtStore and reference-extra FormIDs are not filtered or
remapped. On load unresolved combat-style copies fall back through the existing
package/set/base/default selection. This preserves main's session degradation;
it does not implement temporary-style persistence or native save behavior.

[hypothesis: diagnostic policy] The revised
[save policy](combat-style-save-preview.md) warns about session-only state and
allows saving, while retaining the independent scene-clone checked-snapshot
refusal. The previous draft's CSTY whole-save refusal is withdrawn in response
to the maintainer's compatibility request.

No compilation, regression or native result is claimed by this source note alone.
