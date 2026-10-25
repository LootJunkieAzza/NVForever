# Spec note: extender form functions

Covers the extender's functions for asking what a form is and where it comes from:
validity and kind (IsFormValid, IsReference, GetType, TypeOf), form IDs as text
(GetFormIDString, GetRawFormIDString) and load-order indexes (GetModIndex,
GetSourceModIndex, GetModName, GetFormFromMod).

Written for task D5 of the extender plan ([extender-core-tasks.md](../handoff/extender-core-tasks.md))
from the GECK wiki export. Everything the pages state is tagged `[documented: <page>]`;
what they leave out is **open (to observe)**. Value kinds and conversions in general are
[ext-values.md](ext-values.md)'s.

Behaviour is our own wording of the GECK wiki export (`C:\fnv-private\geckwiki\`,
private); no extender source, no opcode numbers or argument encodings (those come only
from a cited document or from compiling our own test scripts with xNVSE,
[extender-compat.md](../design/extender-compat.md) §4.1).

## 1. Validity and kind

**IsFormValid** `IsFormValid refVar` (FOSE1): 1 when the ref variable holds a reference
or a base form that is loaded; 0 when it holds nothing, or an object not in memory (a
non-persistent reference elsewhere). Meant to guard calls that would crash on a bad ref
[documented: IsFormValid].

**IsReference** `IsReference refVar` (FOSE1): 1 when the variable holds a valid
reference (not a base form). **Always 1 inside a compiler-override block** [documented:
IsReference]. The Function syntax page advises IsFormValid first, then IsReference,
because IsReference on an invalid form may raise an extender error [documented: Function
syntax].

**GetType**: the IsFormValid page's example calls it on a base form (`GetType SomeRef`)
and on a reference (`SomeRef.GetType`) to get a form-type number [documented:
IsFormValid example]. **It has no page in the export and no row in
`schemas/script/functions.json`**, so its arguments, its numbers and which origin it
belongs to are open. Before D5 builds it, the owner adds its page (and its form-type
table) to the next `Special:Export`; the catalogue then gains the row on regeneration.

**TypeOf** `TypeOf expr` (NVSE): the kind of a value as a string, one of "Number",
"Form", "String", "Array", "Map", "StringMap" [documented: TypeOf]. A form of any type is
"Form"; TypeOf does not tell form types apart (that is GetType's job) [documented:
TypeOf, by its list of results].

## 2. Form IDs as text

**GetFormIDString** `GetFormIDString refVar` (NVSE): the form ID in hexadecimal; compiles
only with a ref variable, not a form named directly [documented: GetFormIDString].

**GetRawFormIDString** `GetRawFormIDString refVarOrElement` (NVSE): the same, but for any
value, valid form or not; works on a ref variable or an array element; **returns
"00000000" at run time when given a form or reference named directly** [documented:
GetRawFormIDString].

Both pages warn that a form ID depends on the load order, so a stored string can name a
different form after a load [documented: GetFormIDString; GetRawFormIDString]. Padding to
8 digits: the raw function's "00000000" shows it for that function [documented:
GetRawFormIDString]; letter case and padding of GetFormIDString: open.

## 3. Load-order indexes

**GetModIndex** `GetModIndex "File.esp"` (FOSE1): the plugin's index in the load order;
**255 when it is not loaded** (the extender's version) [documented: GetModIndex]. Whether
the name is matched without regard to case, and whether the extension is required: open.

**GetSourceModIndex** `GetSourceModIndex form` or `ref.GetSourceModIndex` (FOSE1): the
load-order index of the plugin the form belongs to [documented: GetSourceModIndex]. For
a form made at run time (a placed or cloned one, whose ID starts with FF): open.

**GetModName** `GetModName index[, keepExt]` (LU, a plugin, not core): the file name at
that index, without the extension unless `keepExt` is 1; index 0 is FalloutNV
[documented: GetModName].

**GetFormFromMod** `GetFormFromMod "File.esp" "hexID"` (alias GFFM; LU, not core): the
form with that ID in that plugin, the ID given **without the mod index** (the first two
hex digits); leading zeros may be dropped. Unlike BuildRef or StringToRef it does not
resolve unloaded non-persistent references [documented: GetFormFromMod]. Its result for
a plugin that is not loaded: open (the page's example checks IsModLoaded first).

The mod index is the top two hex digits of a loaded form's ID [documented:
GetFormFromMod, "without the mod index, i.e. the first two numbers"]. NVForever's load
order already assigns these (fnv-esm); D5 maps these functions onto it.

## 4. Catalogue

| Function | Origin | In the catalogue | Core family |
|---|---|---|---|
| IsFormValid | FOSE1 | yes | values |
| IsReference | FOSE1 | yes | values |
| TypeOf | NVSE | yes | values |
| GetType | ? | **no** | — |
| GetFormIDString | NVSE | yes | strings |
| GetRawFormIDString | NVSE | yes | — |
| GetModIndex | FOSE1 | yes | — |
| GetSourceModIndex | FOSE1 | yes | — |
| GetModName | LU | yes | — (a plugin's) |
| GetFormFromMod | LU | yes | — (a plugin's) |

[observed: `schemas/script/functions.json`]. GetModName and GetFormFromMod come from the
LU plugin, not the extender core; D5 may build them anyway, but they belong in a plugin
module (C2's split).

## 5. Open (to observe), collected

GetType entirely (page missing); GetFormIDString's case and padding; GetModIndex's
matching rules; GetSourceModIndex for run-time forms; GetFormFromMod for an unloaded
plugin.
