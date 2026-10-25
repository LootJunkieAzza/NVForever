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

**GetObjectType** (alias **GetType**; FOSE1) `GetType form` or `ref.GetType`: the
form-type code of a base form; given a reference (or called on one), the code of its base
form. An NPC gives 42, armor 24, ammo 41 [documented: GetObjectType, exported in
`FOSE.xml` 2026-10-25]. The page recommends checking IsFormValid and IsReference first,
and `let` over `set` so a bad call reports an error [documented: GetObjectType]. The
catalogue already has it under GetObjectType with GetType as its alias [observed:
`schemas/script/functions.json`]; an earlier version of this note missed the alias. The
codes 0-120 follow the plugin record types (appendix) [documented: Form Type IDs,
pasted by the owner from geckwiki.com, 2026-10-25]. Points the page makes: aid items and magazines are ALCH (47) and skill books
BOOK (25), whatever the Pip-Boy tab; armor addons report ARMO (24), not ARMA (96), and
IsArmorAddon tells them apart; CLOT (26), INGR (29) and TLOD (68) are unused; 200
(actors) and 201 (inventory objects) exist only as filters for reference walking
(GetFirstRef).

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
| GetObjectType (GetType) | FOSE1 | yes | — |
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

whether GetType on a placed reference ever reports 58-60 rather than its base form's code; GetFormIDString's case and padding; GetModIndex's
matching rules; GetSourceModIndex for run-time forms; GetFormFromMod for an unloaded
plugin.

## Appendix: form type codes

[documented: Form Type IDs, pasted by the owner from geckwiki.com, 2026-10-25]. Inventory items are 24, 25, 31, 40, 41, 46, 47, 49,
103, 108, 115 and 116.

| Code | Record | Form type |
|---|---|---|
| 0 | `NONE` | none |
| 1 | `TES4` | file header |
| 2 | `GRUP` | group |
| 3 | `GMST` | game setting |
| 4 | `TXST` | texture set |
| 5 | `MICN` | menu icon |
| 6 | `GLOB` | global |
| 7 | `CLAS` | class |
| 8 | `FACT` | faction |
| 9 | `HDPT` | head part |
| 10 | `HAIR` | hair |
| 11 | `EYES` | eyes |
| 12 | `RACE` | race |
| 13 | `SOUN` | sound |
| 14 | `ASPC` | acoustic space |
| 15 | `SKIL` | skill |
| 16 | `MGEF` | base effect |
| 17 | `SCPT` | script |
| 18 | `LTEX` | land texture |
| 19 | `ENCH` | object effect |
| 20 | `SPEL` | actor effect |
| 21 | `ACTI` | activator |
| 22 | `TACT` | talking activator |
| 23 | `TERM` | terminal |
| 24 | `ARMO` | armor (armor addons too) |
| 25 | `BOOK` | book |
| 26 | `CLOT` | clothing (unused) |
| 27 | `CONT` | container |
| 28 | `DOOR` | door |
| 29 | `INGR` | ingredient (unused) |
| 30 | `LIGH` | light |
| 31 | `MISC` | miscellaneous item |
| 32 | `STAT` | static |
| 33 | `SCOL` | static collection |
| 34 | `MSTT` | moveable static |
| 35 | `PWAT` | placeable water |
| 36 | `GRAS` | grass |
| 37 | `TREE` | tree |
| 38 | `FLOR` | flora |
| 39 | `FURN` | furniture |
| 40 | `WEAP` | weapon |
| 41 | `AMMO` | ammo |
| 42 | `NPC_` | NPC |
| 43 | `CREA` | creature |
| 44 | `LVLC` | leveled creature |
| 45 | `LVLN` | leveled character |
| 46 | `KEYM` | key |
| 47 | `ALCH` | ingestible (aid items, magazines) |
| 48 | `IDLM` | idle marker |
| 49 | `NOTE` | note |
| 50 | `COBJ` | constructible object |
| 51 | `PROJ` | projectile |
| 52 | `LVLI` | leveled item |
| 53 | `WTHR` | weather |
| 54 | `CLMT` | climate |
| 55 | `REGN` | region |
| 56 | `NAVI` | navmesh info map |
| 57 | `CELL` | cell |
| 58 | `REFR` | reference |
| 59 | `ACHR` | character reference |
| 60 | `ACRE` | creature reference |
| 61 | `PMIS` | missile projectile |
| 62 | `PGRE` | grenade projectile |
| 63 | `PBEA` | beam projectile |
| 64 | `PFLA` | flame projectile |
| 65 | `WRLD` | worldspace |
| 66 | `LAND` | land |
| 67 | `NAVM` | navmesh |
| 68 | `TLOD` | unknown (unused) |
| 69 | `DIAL` | dialogue topic |
| 70 | `INFO` | dialogue topic info |
| 71 | `QUST` | quest |
| 72 | `IDLE` | idle |
| 73 | `PACK` | package |
| 74 | `CSTY` | combat style |
| 75 | `LSCR` | load screen |
| 76 | `LVSP` | leveled spell |
| 77 | `ANIO` | animated object |
| 78 | `WATR` | water |
| 79 | `EFSH` | effect shader |
| 80 | `TOFT` | offset table |
| 81 | `EXPL` | explosion |
| 82 | `DEBR` | debris |
| 83 | `IMGS` | imagespace |
| 84 | `IMAD` | imagespace adapter |
| 85 | `FLST` | form list |
| 86 | `PERK` | perk |
| 87 | `BPTD` | body part data |
| 88 | `ADDN` | addon node |
| 89 | `AVIF` | actor value info |
| 90 | `RADS` | radiation stage |
| 91 | `CAMS` | camera shot |
| 92 | `CPTH` | camera path |
| 93 | `VTYP` | voice type |
| 94 | `IPCT` | impact data |
| 95 | `IPDS` | impact data set |
| 96 | `ARMA` | armor addon (unused: addons report 24) |
| 97 | `ECZN` | encounter zone |
| 98 | `MESG` | message |
| 99 | `RGDL` | ragdoll |
| 100 | `DOBJ` | default object manager |
| 101 | `LGTM` | lighting template |
| 102 | `MUSC` | music |
| 103 | `IMOD` | weapon mod |
| 104 | `REPU` | reputation |
| 105 | `PCBE` | continuous beam projectile |
| 106 | `RCPE` | recipe |
| 107 | `RCCT` | recipe category |
| 108 | `CHIP` | casino chip |
| 109 | `CSNO` | casino |
| 110 | `LSCT` | load screen type |
| 111 | `MSET` | media set |
| 112 | `ALOC` | media location controller |
| 113 | `CHAL` | challenge |
| 114 | `AMEF` | ammo effect |
| 115 | `CCRD` | caravan card |
| 116 | `CMNY` | caravan money (faction currency too) |
| 117 | `CDCK` | caravan deck |
| 118 | `DEHY` | dehydration stage |
| 119 | `HUNG` | hunger stage |
| 120 | `SLPD` | sleep deprivation stage |
| 200 | — | actor (reference walking only) |
| 201 | — | inventory object (reference walking only) |

## Implementation table (D5)

One row per function the plan lists for this task (docs/handoff/extender-core-tasks.md), taken from the sections above and their pages; "fmt" means a format string with up to 20 values (ext-format.md). Anything not stated by a page is marked open.

| Function | Aliases | Arguments (type, ? = optional) | Returns | Behaviour [documented] | Edge cases [documented unless marked] |
|---|---|---|---|---|---|
| GetObjectType | GetType | form? (or calling reference) | int | form-type code of a base form, or of a reference's base form | codes: appendix; check IsFormValid/IsReference first |
| IsFormValid | — | ref variable | bool | 1 when it holds a loaded reference or base form | 0 when empty or not in memory |
| IsReference | — | ref variable | bool | 1 when it holds a reference (not a base form) | always 1 in a compiler-override block; may error on an invalid form |
| GetFormFromMod | GFFM | plugin name string; hex form ID string (no mod index) | form | the form with that ID in that plugin (LU plugin, not core) | leading zeros optional; does not resolve unloaded non-persistent references; unloaded plugin: open |
| GetModIndex | — | plugin file name string | int | the plugin's load-order index | 255 when not loaded; name matching rules: open |
| GetSourceModIndex | — | form? (or calling reference) | int | load-order index of the plugin the form belongs to | run-time (FF) forms: open |
| TypeOf | — | expression | string | "Number", "Form", "String", "Array", "Map" or "StringMap" | all forms are "Form" |
