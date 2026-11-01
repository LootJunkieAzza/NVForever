# The weapon-instance plugin's functions: ppNVSE (2026-11-01, #1014)

ppNVSE's 42 catalogued script functions (catalogue origin `PP`) as NVForever builds them:
on the game side (`crates/fnv-game/src/nvse_pp.rs`, reached from `nvse::call`), the
file functions in `crates/fnv-game/src/mod_files.rs`, and two functions' output
variables in the VM (`out_params`, `crates/fnv-script/src/vm.rs`). Sources: the GECK
wiki export [documented: the page named]. ppNVSE is not installed in the probe game, so
nothing here is `[observed]`; what a page leaves open is marked `[hypothesis]`. Tests:
`crates/fnv-game/tests/pp.rs`.

Eleven catalogue names have no page under their own name. Their pages are there under
other titles, which the catalogue records (`page` in `schemas/script/functions.json`):
AX_Dump … AX_Size on "AX Dump" … "AX Size", GetAllAttachedWeaponMods on "GetAttachedMod"
(whose template carries the wrong name) and GetAllModsInSlot on "GetAllSlotMods". So
these are `[documented]` too, from those pages; what they leave open is marked.

Three catalogue rows were short of what their pages' examples pass, so scripts calling
them as the pages show did not compile; they are fixed in the generator's `PARAM_FIXES`
(`tools/capture/script_functions.py`) and the same rows edited in `functions.json`:
GetAllAttachedWeaponMods takes the slot after the weapon [documented: GetAttachedMod's
examples; optional, hypothesis: the table lists the weapon only], and OnAttachItemMod /
OnDetachItemMod take up to two filters after the flag, each its number and its form, the
flag and filters optional for the removal form `udf 0` [documented: OnAttachItemMod's
examples]. [decision: the rows were hand-edited and not regenerated, because a full
regeneration from today's wiki export also rewrites unrelated rows.]

## 1. AX variables

Values kept by a string key, optionally on a form, each key a list of entries; they last
for the session and are not saved [documented: every AX page, "all AX values remain valid
for the session"]. Keys are matched without regard to case, as the other extender keys
[hypothesis]; a key on a form and the same key without one are different variables
[documented: AX Exists, "If form is omitted, checks if the global key exists"].

| Function | NVForever |
|---|---|
| AX_Set `value key [index] [form]` | the value (number, form, string or array) at the index, 0 when omitted [documented: AX Get, "Index 0 is assumed"]; the list grows with null entries to reach it [hypothesis]; an array is held as it is, not copied or flattened [documented: AX Set; hypothesis: shared, like an array in an array]. Returns the value [hypothesis: the page's return type "variable"] |
| AX_Get `key [index] form` | the value; 0 for none or a null entry [hypothesis] |
| AX_GetType `key [index] form` | −2 no such aux (or no such index), −1 null, 0 number, 1 form, 2 string, 3 array [documented] |
| AX_Size `key form` | the number of entries, nulls included; 0 for no key [hypothesis] |
| AX_Exists `key [form]` | [documented] |
| AX_Find `value key [deep] [form]` | the index of the first equal entry (strings without regard to case, as Ar_Find), −1 for none; a deep search also looks inside arrays held, giving the index of the entry holding it [hypothesis: the page gives neither] |
| AX_Null `key [index] [form]` | the entry made null, the others keep their indexes; index omitted or −1 deletes the key [documented] |
| AX_Erase `key [index] form` | the entry removed, the later ones shift down; −1 deletes the key [documented] |
| AX_Dump `key [form]` | the entries to the console [documented]; 1 when the key exists [hypothesis] |

The catalogue's optional index comes before the form, so the trailing arguments are read
by type: the first number is the index, the first form the form.

## 2. Form instances

An instance is a new form standing for a base form, made by script and "tied to the users
save game" [documented: CreateFormInstance]. NVForever makes it a clone of the base form
(the same id space and field reading as TempCloneForm, `nvse_data`), with the editor ID
the page gives, the base form's own, "Inst" and a count (counted per base form
[hypothesis]), so GetEditorID and GetFormFromEditorID find it. Instances are saved with
the game (`GameSnapshot::form_instances`) and come back as clones on a load.

| Function | NVForever |
|---|---|
| CreateFormInstance `form [key]`, NewWeaponInstance `weapon [plugin]` | the new instance; 0 for an instance of an instance [documented: "You can not create instances of instances"]. NewWeaponInstance's plugin is not used and it returns the instance too [hypothesis: the page's return type says none] |
| GetStaticForm, GetWeaponBase | the base form an instance (or a reference's base) was made from; any other form itself [documented] |
| IsFormInstance, IsWeaponInstance | [documented]; a TempCloneForm clone is not an instance [hypothesis] |
| SetOnInstanceDeconstruct, SetOnInstanceReconstruct `priority udf [key]` | registered for the session; on a load the old game's instances are deconstructed, then each loaded one reconstructed, and a reconstruct handler that sets its function value true drops that instance [documented: both pages]; higher priority first [hypothesis, as event handlers]. They are carried across the load with the session (`Game::take_session`), function scripts only, as the event handlers. DeleteFormInstance has no catalogue entry, so instances are deconstructed on a load only |

## 3. Item mods

The vanilla slots "1"–"3" are the weapon's WMI1–3: on for an inventory item through its
own mod flags (as SetWeaponRefModFlags), else on the base form or instance AttachMod was
given (kept in `PpState::vanilla`). Every other slot name is one of the Extended Item Mods
System's, whose slots Devkit defines (§6): a script may attach any item mod in any named
slot [hypothesis], for the session [hypothesis].

| Function | NVForever |
|---|---|
| AttachMod `object slot [itemMod]` | a vanilla slot is turned on [documented: the first example]; an item mod given must be that slot's WMI [hypothesis]. A named slot gets the mod, or with none loses the one it had [documented: notes]; the detach handlers run for the mod replaced or removed and the attach handlers for the new one [documented: OnAttachItemMod]. Returns 1 when something was done [hypothesis: the page gives no return value] |
| GetAllAttachedMods `weapon` | a string map, slot name → item mod, of the vanilla slots on and the named ones [documented] |
| GetAllAttachedWeaponMods `weapon "slot"` (page GetAttachedMod) | the mod in that slot; no slot, none [hypothesis] |
| GetAllModsInSlot `object [slot]` (page GetAllSlotMods) | the mods that can go in the slot as a string map of slot → mod, every slot's without one [documented]; without Devkit's lists a vanilla slot can take its WMI and a named slot the mod attached there [hypothesis] |
| HasExtendedMods `object [checkVanillaMods]` | the object has a mod in a named slot, and with the flag also has vanilla mods [documented; hypothesis: "uses the extended mod system" read as having an extended mod] |
| OnAttachItemMod, OnDetachItemMod `priority udf runOn… "1" itemMod "2" object`, or `udf 0` | the handler, with its filters, or removed [documented: both pages' examples]; the udf gets the item mod and the object [documented]. runOnReconstruct/runOnDeconstruct are kept; named-slot attachments do not outlive a load, so they never fire on one |

## 4. The player, weapons, inventory and dialogue

| Function | NVForever |
|---|---|
| IsThirdPerson | 1 when the camera is not first person, as the engine tells it (`Game::set_first_person`) [documented] |
| GetPlayerCameraRotation `x y z` | the three variables get the player's angles in degrees [documented: "pitch, roll and yaw" in degrees; hypothesis: the first-person camera looks where the player does; the engine does not tell a third-person orbit] |
| FindWeaponAmmo `weapon [ammo]` on an actor | the first of the weapon's ammo (NAM0, an ammo or a form list of them) the actor carries; given an ammo, the next carried one after it, going round to it last; none carried, no form [documented: the page's 9mm example]. An ammo not in the list starts from the top [hypothesis] |
| ReplaceItemInInventory `newBase [copy] [count]` on an inventory reference | the item replaced by the new base form, keeping whether it is worn [documented: "This works on equipped items"]; `copy` keeps its condition and mods too, `count` how many (all when omitted) [hypothesis: the arguments' meaning] |
| SetSightingNode `name`, GetCurrentSightingNode | the node until the player's weapon changes, `##SightingNode` otherwise [documented] |
| SetWeaponDegradationMult `multiplier [weapon]` (or `weapon multiplier`, as the example) | the weapon base's per-shot wear [documented]. GetWeaponDegradationMult (catalogued under the setter's name, so a call with no number reads) gives it, else the record's Damage to Weapon Mult when its override flag is on, else fDamageToWeaponValue (0.2) [hypothesis: what it reads with nothing set] |
| GetTopicPrompt `info` | the INFO's RNAM prompt; "" for none [documented] |
| EvaluateString `"condition"` | 1 when the text holds as an `if` condition, compiled once per text as a function script [documented]; a text that does not compile is false and goes to the trace [hypothesis] |
| CallLoop `delay udf args…` on a reference (the player when none) | the function called every `delay` seconds of game time; a function that sets its value sets the next delay, below 0 ends the loop; `-1` as the delay removes it [documented]. The first call after one delay; the same udf on the same reference again replaces it [hypothesis]. Not kept across a load [documented: the page's example text, "auto-removed when loading a game"] |

## 5. Files

pReadFile `path [start] [end]` reads lines as strings, start counted from 0, end left out
[documented: "0 5" reads the first five lines; hypothesis: end not included], from
under the game's folder [documented: pWriteFile's notes]; a missing file gives an empty
array [hypothesis]. pWriteFile `path array [append]` writes one line per element
[documented] under the game's folder and pWriteFileAlt under Data [documented: "relative
to the game's DATA path"; hypothesis: a mod's staging folder is its place in Data]. Both
write into NVForever's own layer, never the game's folder (CLAUDE.md charter;
`mod_files` module notes), and readers read that copy first.

## 6. Not read: Devkit

ppNVSE's Devkit text files define form traits, item-mod slots and the mods they take,
akimbo sets, and kit commands (the catalogue's `Kit::` entries, which are not script
functions and not counted). NVForever does not read them [decision: a loader is a
separate design; it would fill `PpState` and a trait store]. Until then:

- GetFormTrait (the catalogue's one name for the pages GetFormTrait, GetFormTraitType and
  GetFormTraitSize), HasFormTrait: no trait exists: value 0, type −1 [documented: "Trait
  does not exist"], size 0 [hypothesis]. SetFormTrait has no catalogue entry.
- IsAkimboForm, HasAkimboSet: 0; GetAkimboWeaponsAlt writes no forms.

## 7. Follow-ups

- **Engine**: the iron sights (`nvforever::iron_sights`) look for the default node, not
  `Game::pp_sighting_node`; the weapon wear (`fnv_combat::condition::weapon_wear`) does
  not read `Game::pp_degradation_mult`.
- **Catalogue**: GetWeaponDegradationMult, GetFormTraitType and GetFormTraitSize share
  another function's catalogue name (their pages carry its template name), so scripts
  calling them by their own names do not compile yet; `nvse_pp` answers the names.
- Devkit (§6).
