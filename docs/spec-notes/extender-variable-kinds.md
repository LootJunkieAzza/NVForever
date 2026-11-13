# extender variable kinds

[documented] The existing [ScriptVariableType schema](../../schemas/fnv/common.json) maps the serialized kind labels below. [observed] Authored synthetic SLSD/SCVR records reproduce the unit-reader mismatch: Types 2, 3 and 4 are named String (xNVSE), Array (xNVSE) and Ref (xNVSE). The compiler/runtime already represent string_var and array_var as extender kinds.

[design] Preserve these known schema meanings at the unit-reader boundary. Unknown type values stay unknown; slot indices and names are not rewritten. This note does not claim independent native producer observations for every string/ref type.
