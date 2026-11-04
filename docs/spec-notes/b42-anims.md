# B42 Inject / B42 Interact animation folders

Clean-room note on the conventions two popular mods use to give items animations, so
the engine can play them natively (they otherwise rely on script-extender plugins the
engine does not load). Learnt from the mods' folder layout, their readme/ini text and
the KF files' text keys; nothing of theirs is copied here or shipped.
Code: `crates/fnv-actor/src/b42.rs` (folders, rules), `engine/nvforever/src/consume_anims.rs`
(playing), `engine/nvf-coop/src/net.rs` (`AnimCue`, kind 16).

## Consume (B42 Inject)

- `meshes\characters\_male\idleanims\b42inject\<pa>\<view>\<EditorID>\*.kf`
  [observed]: `<pa>` is `0` (normal) or `1` (power armour), `<view>` is `0` third
  person or `1` first person, `<EditorID>` the aid item's. One clip is picked at
  random per use [observed: the readme].
- Text keys [observed: the KFs]:
  - `ScriptLine:` lines (several separated by `%r`) set the attach model
    (`AuxVarSetStr "*_InjMP" "<model>"`), attach (`AuxVarSetFlt "*_InjM" 1`), detach
    (`… 3`), play a file (`"*_InJSP" "<wav>"`) or the item's own consume sound
    (`"*_InJS" 1`). Older packs use `DispatchEventAlt "B42IA"/"B42ID"/"B42IS"/"B42IP"/"B42ISP"`
    and `B42InjectQ.iRunAttach/iRunDetach`.
  - `Sound: B42InjectSound` is the item's consume sound; `SoundPath:<wav>` a file.
- The model attaches at a node `B42AttachPointL` under `Bip01 L Hand`; the clip has
  a track of that name, and tracks named after the model's nodes with `_B42`
  appended [observed: the tracks].
- Rules [observed: the readme and ini]: not while swimming, ragdolled, reloading,
  attacking; a use from a menu plays once it is closed; attack or aim cuts it short
  unless `Hardcore=1` (`Config\B42Inject.ini`), which instead refuses an aid item
  while busy. `InjectInPA` allows it in power armour.
- The item's consume sound plays only from its key [observed]; a clip without one is
  silent for it [design: kept].

## Interact (B42 Interact)

- `meshes\b42interact\edid\<EditorID>\add\<dflt|once>\[<creature folder>\]1st\*.kf`
  with a `cnd.ini` beside [observed]. `once` plays on the first pickup only (weapon
  inspects) [observed: readme]; first person only [observed: no `3rd` folders].
- `cnd.ini` is a script; the packs use two kinds [observed]: not in combat, or the
  item came from a dead creature within 500 units (each body once, optionally a
  `GetCreatureType` test). The creature folder names the creature's model folder.
  The engine reads those two kinds only [design].
- Crafting: `meshes\b42interact\awr\edid\<Recipe EditorID>\1st\*.kf` [observed].

## Co-op [design]

Every aid item used (and each interact/crafting clip played) is sent as an `AnimCue`
(kind, a style code for the fallback, power armour, item, EditorID, the clip path).
The other games play the item's third-person B42 clip of the same name when present,
else the base game's idle for the item's kind (drinking, eating, the stimpak idle,
smoking, repairing, the workbench), upper body only, at most 8 s.

## Not done

kNVSE `AnimGroupOverride` folders (Idle Variety); "once" remembered across saves;
the crouch (`*_Sit`) keys; Hardcore closing the Pip-Boy.
