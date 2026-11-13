# The rest of the registered extender commands with no wiki page (2026-11-13, #2326)

After the 33 that the mod lists call (`ext-unlisted-commands.md`, #2318), the installed
plugins register 205 more commands with no wiki page and no catalogue row, by nvse.log:

| Plugin | Count |
|---|---|
| JIP LN | ~50 |
| ShowOff | 41 |
| SUP | 46 |
| kNVSE | 20 |
| JohnnyGuitar | 17 |
| One111 | 8 |
| ITR | 6 |

20 more log entries named `NONE` are placeholders, not commands. No script in the owner's
mod lists calls any of these, so they go family by family. This note grows with each
batch. Code: `crates/fnv-game/src/nvse_unlisted2.rs`; ShowOff's string maps live in the
VM, `crates/fnv-script/src/vm/jip.rs`. Catalogue rows come from `PAGELESS` in
`tools/capture/script_functions.py`.

## 1. Sources

- **Names, opcodes and plugins**: nvse.log of the original game (xNVSE 6.4.9), read the
  same way as `ext-unlisted-commands.md` §1. The opcodes of every command the catalogue
  gained in #2318 and here are added to `schemas/script/command_opcodes.json` from those
  log lines. That file still lacks the other commands of the plugins that were not
  installed when it was recorded (ShowOff, kNVSE, SUP, One111, ITR); that is a follow-up.
- **Argument shapes**: a compile-only run in the original. Each name was compiled with 23
  argument patterns (none; one to nine zeros; quoted strings; a ref variable; mixes; a bare
  `X`) inside `if 0 … endif`, so the game compiled each call and ran none [observed: the
  sandbox's stock game, save nvfairport, 2026-11-13]. The run was split into jobs of 150
  calls (one of 4,715 hung the game before its first frame). How to read the results:
  - A command that compiles with every pattern takes no arguments; the compiler then
    ignores extra words.
  - A command with parameters refuses extra words, which gives exact counts.
  - A string parameter also takes a bare word or a number, so it shows as
    `0 | "a" | rP | X`. A number or form parameter shows as `0 | rP`.

  These are [observed] for the counts and [hypothesis] for the kinds.
- **Behaviour**: probe sets in `tools/capture/jip_probes.py` (`rest1` here), run unattended
  by nvf_capture v10 in the Blue Moon sandbox's stock game, save nvfairport. Rows are cited
  as `rest1.label`; the goldens are private.
- **Not read**: no plugin DLL or source, and no nv-rs.

## 2. Network services, tests, logs, an address: neutral answers (batch 1)

NVForever answers 0 and does nothing for the commands below [decision: extender-compat.md
§5.8]. None of them was run in the original. The network ones would reach outside services,
and the rest test or reset the plugin itself or read the original's memory.

| Command | What it reaches | Shape [observed] |
|---|---|---|
| BBPostMessage, BBReportMessage, BBDeleteUserMessage, BBLikeMessage, BBHideMessage, BBGetUserMessages, BBGetUserInfo, BBGetInfoForReference, BBIsReferenceNote, BBOnCellChange, BBReturnAllMessagesToDefaultCell, BBGetCurrentNotesInCell, BBIsBBLoaded, BBAreMessagesFetched, BBGetBBKey, BBAPIKeyExist, BBNexusAPIKeyExist, BBIsTextEditMenuActive, BBGetTextMenuString, BBSetTextMenuTitle | SUP's bulletin board, a web service [hypothesis: by the names] | none, one id, one reference, one string, or (BBPostMessage) a string and a reference, (BBReportMessage) an id and a string |
| UpdateCheckerCheckForUpdates, UpdateCheckerReturnArray, GetSUPDateCheckerCurrentStage, GetSUPDateCheckerDoneStage, GetPastebinReaderCurrentStage, GetPastebinReaderDoneStage | SUP's update checker and pastebin reader (web) | `0 0 "a"`; one number; none |
| SUPTest, LunetteCMD, ERDTestFunc | the plugins' own tests | none; one number |
| SUPWRITETOLOG, DumpTileInfoChilds | SUP's own log | one string; an optional string |
| RefAddrxData | JohnnyGuitar: a reference's data in the original's memory | one number |
| NullArgs | JohnnyGuitar: answers 0 [observed: rest1.nullargs, nullargs.1] | an optional number |
| kNVSEReset | resets kNVSE | none |

BBIsBBLoaded's 0 lets a mod see that the board is not there.

## 3. ShowOff's string maps (batch 1)

`AuxStringMapArray…` (aliases `AuxStringMap…`) keep named maps from string keys to a
number, a form or a string. [observed: set rest1]:

- **One store for everyone.** The map is the same whether the caller is the player, Doc
  Mitchell or no reference (`sm.other.ref`, `sm.noref.*`). Names starting with `*` are kept
  apart, as JIP's temporary variables are [hypothesis: unsaved, like those]. NVForever keeps
  the others in the saved store.
- **Keys ignore case** (`sm.getfloat.case`).
- **Setters**: SetFloat, SetString and SetRef `name key value` answer 0; setting a key again
  replaces its value.
- **Getters**: GetFloat, GetString and GetRef `name key` give 0, "" or no form when the
  value is of another type or the key is missing.
- **GetValueType** `name key`: 1 number, 2 form, 4 string, 0 none (also for no such map).
- **GetSize** `name`: the number of keys.
- **GetKeys**: a list of the keys. In what order is [hypothesis]: NVForever lists them by
  lower-case key, which matched the order they were set in the probe.
- **GetAsArray**: a string map of the keys and values.
- **GetFirst / GetNext**: a `[key, value]` list each; after the last, no array.
- **GetAll** `0|1`: no array when the store has no map; with `1` a string map of the
  temporary maps by name [observed: getall.0, getall.1; hypothesis: `0` the saved maps].
- **SetFromArray** `name array [keep]`: the map becomes the array's string keys and values,
  or with `keep` non-zero gains them; answers 0 (`fromarray`, `fromarray.keep`,
  `fromarray.replace`).
- **EraseKey**: the number of keys left, 0 for a key that was not there.
- **ValidateValues**: the keys left. All values were alive in the probe [hypothesis: a value
  whose form is gone is dropped, as RefMapArrayValidate does].
- **Destroy**: 0 either way.

## 4. ShowOff's `_OLD` versions and IsJumping_BROKEN (batch 1)

Matrix_IsMatrix_OLD, Matrix_Transpose_OLD, Matrix3x3_GetQuaternion_OLD,
Quaternion_GetMatrix_OLD, Matrix_Dump_OLD, Matrix_ApplyOperationWithScalar_OLD,
Matrix_ApplyOperationWithMatrix_OLD, ListAddArray_OLD and TopicInfoSetResponseStrings_OLD
take the current command's arguments [observed: the shape run] and answered as the current
command wherever both compiled [observed: rest1.old.*/new.*: IsMatrix, Dump, ListAddArray
with the count before, the answer, the count after and after the removal all equal]. The
probe rows for the other matrix ones did not compile, in either version (a fault of the
probe script). NVForever hands each `_OLD` command to the current one [hypothesis: the same
for the rows not compared].

IsJumping_BROKEN read 1 for the standing player [observed: rest1.old.isjumping], while the
current IsJumping did not compile called on a reference. NVForever answers 1 [hypothesis:
always, as the name says it is broken].

## 5. Pure getters (batch 1)

- **StringContainsOnlyASCII** `text`: 1 when every character is below 128
  [observed: rest1.ascii.plain 1, ascii.high 0]. An empty literal does not compile in the
  original.
- **GetFuncCodeByName** `name`: the command's opcode, by name or alias, any case; -1 for
  none [observed: rest1.funccode.*: GetAV and GetActorValue 4110, AttachLight 10361].
- **GetFuncNameByCode** `code`: the command's name; an unknown code fails the call
  [observed: rest1.funcname.*: 4110 GetActorValue, 10361 AttachLight, 1 and 65000 "script
  call failed"]. NVForever gives the catalogue's name, else the longest of the opcode
  table's names for the code (an alias is the shorter) [hypothesis].
- **IsNewGame**: 0 in a loaded save [observed: rest1.isnewgame]. NVForever gives 1 from the
  New Game menu until a save is loaded [hypothesis].

## 6. SUP's JSON commands (batch 2)

These work on a JSON file under the game folder (`Data…` reaches Data), by top-level key.
Probed with a file of our own in the sandbox [observed: sets json1, json2; each command
that crashed ran alone]. In NVForever, files a mod writes go to NVForever's own layer,
never the game folder (`crate::mod_files`). Code: `crates/fnv-game/src/nvse_json.rs`.

- **JSONCacheFile** `path flag`: reads the file into a cache; 0, or -4 for no such file.
- **JSONIsFileCached** `path`: 1 or 0.
- **JSONToString** `path`: the cached copy in one line, keys sorted; "" when not cached.
- **JSONCloseFile** `path flag [n]`: `1` drops the cached copy and gives 0; `0` gives -5
  and keeps it.
- **JSONCacheFileFromString** `text name`: -3 both times, with either order, for text whose
  quotes came through escaped [hypothesis: -3 is "not JSON"; NVForever caches valid text
  under the name].
- **JSONGetValueType** `path key flag`: object 1, array 2, string 3, number 5, missing -2
  [hypothesis: boolean 4, null 0]. Only top-level keys: `obj/inner`, `obj.inner` and
  `/obj/inner` all gave -2.
- **JSONGetValueArray** `path key flag`: an array of 3 for a 3-element array. Its elements
  read back neither as numbers nor as strings, and an object gave an array of 1 and a
  number one of 2 [observed: json1.arr.*, json2.arr.*]. NVForever gives the array's
  elements, an object's pairs as a string map, and nothing for a scalar [hypothesis; open].
- **The last flag of the readers and writers**: 0 is the file on disk, 1 the cached copy.
  With 0 the file is rewritten at once, indented by four spaces, and the cache keeps its
  old copy. With 1 a file that is not cached gives -4 (`json2.uncached.set.cache`).
- **JSONSetValueFloat / JSONSetValueString** `path key value flag [n]`: set the key; 1.
- **JSONAppendValueFloat / JSONAppendValueString**: add to the key's array; 1.
- **JSONEraseArrayElement** `path key index flag`: take an element out; 1.
- **JSONClearKey** `path key flag`: empty the key's object (`{}`); 1.
- **JSONEraseKey** `path key flag`: crashed the original with either flag
  (`json2r0`, `json2r1`). NVForever takes the key out and gives 1 [decision].
- **Float writes**: some came out as `true` in the original (4 appended, 2 set into the
  cache) and others as numbers (9, 10) [observed]. NVForever writes the number [decision:
  the plugin's fault, nothing a mod can rely on].

## 7. Left for the next batches

kNVSE's animation commands (20), JIP's (~50), JohnnyGuitar's (17), the rest of ShowOff's
(~13), SUP's NIF and misc commands (~10), One111's (5) and ITR's (6). Their shapes are in
the compile-shape run; each family gets its probe set.
