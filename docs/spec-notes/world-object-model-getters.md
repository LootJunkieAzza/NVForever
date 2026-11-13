# World-object model getters

[observed: baseline 0b16bea3 portable source] `GameData::FIELD_SIGS` retains lazy
record-field handles for ACTI but not STAT, MSTT, FURN, DOOR or TACT. The existing
`GetModelPath` handler reads MODL through those fields. A decoded authored MODL can
therefore be present while this public getter receives no field for those five types.
The field definitions are the repository's `schemas/fnv/objects.json` and
`schemas/fnv/placed.json`.

[implementation] Retain the five additional winning-record handles. Keep lazy field
decoding and return the authored string through the existing getter, including its
case, spaces and separators. The owned synthetic fixture covers all six types, a
model-less/null control, a placed reference, clone-local model changes, original
independence and a fresh Game. These fixtures contain no installed plugin or asset.

[observed: earlier owned portable qualification, separate source] The isolated
repair's three tests failed before the admission change and passed afterward; its
owned headless fixture returned all six paths afterward. That earlier aggregate
branch is not this upstream candidate's tested source. Current 0b16 qualification is
pending; an assertion failure in a loop does not prove later loop cases executed.

[hypothesis: native contract not measured here] This metadata correction does not
establish original-game clone copy-versus-inheritance behavior. It does not fix or
qualify cloned-form scene placement, rendering, physics, model reload or snapshot
lifetime. Adding record handles has an unmeasured memory cost; no speedup is claimed.
