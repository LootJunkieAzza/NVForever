# JIP LN NVSE, batch 12: world, weather, cells, barter and menus (2026-11-09, #1006)

The climate, the wind, the moons and lightning, the image space modifiers on, the cells
kept and found by grid, the light at a point, fast travel and the load order, the game
settings as a map, purged references and internal markers, the barter menu's trade, the
item menus' filters and lists, the recipe menu and AddRecipeCondition, the quantity and
text input menus, and GetWheelDisabled, in `crates/fnv-game/src/nvse_jip12.rs`. Sources:
the GECK wiki export, each function's page [documented: the page named]. The probe set
`jip12` (`tools/capture/jip_probes.py`, lab `L5.ext.jip12`) asks the original what the
[hypothesis] values below leave open.

34 functions; JIP 142 → 108 on main, 102 → 68 once batch 11 (#1915) is merged
[observed: the extender_census test, 2026-11-09].

List getters with nothing to give return no array [observed: the jip10 probes, for JIP's
list getters; hypothesis: the same here].

The engine tells the game what only it knows, and takes back what the scripts asked for:

| From the engine | `Game` method | Engine side |
|---|---|---|
| the cells kept | `set_buffered_cells` | `nvforever::cell_buffer::tell_game` |
| the detection round's lights | `set_light_field` | `nvforever::detection` |
| a fast travel | `note_fast_travel` | `nvf_play::pipboy::data::travel::depart` |
| a save of another load order | `set_load_order_changed` | `nvf_play::save` (the engine's part of a load) |
| the item menus' filters and lists | `set_item_menu_view` | `nvf_play::menu::publish_jip_menus` |
| the barter menu's trade | `set_barter_view` | the same |
| the recipe menu's category and selection | `set_recipe_menu` | `nvf_play::crafting::draw` |

| To the engine | `Game` method | Engine side |
|---|---|---|
| a climate | `script_climate` | `nvforever::worldspace::script_sky` |
| the wind | `script_wind` | `nvforever::tree::wind_vector` (trees and grass) |
| the moons' textures | `moon_textures` | `nvforever::sky::setup_sky` |
| a lightning flash | `take_lightning` | `nvforever::weather_fx` |
| internal markers | `internal_marker_changes` | `nvforever::world::script_markers` |
| the price multipliers | `barter_price_mults` | `nvf_play::menu::model::Trade::mult_set` |
| AddRecipeCondition | `recipe_conditions_pass` | `nvf_play::crafting::listed` |
| the quantity menu | `quantity_ask`, `finish_quantity` | `nvf_play::script_quantity` |
| the text input menu | `text_input`, `finish_text_input` | `nvf_play::chargen` (the name menu's box) |
| SetWheelDisabled | `wheel_disabled` | `Game::has_companion_wheel` |

## 1. The sky

| Function | NVForever |
|---|---|
| ForceClimate `[climate]` | the climate until ForceClimate without one or SetCurrentClimate [documented]; the sky is made again with it, keeping its weather [hypothesis: the original goes on to the new climate's weathers from there]; GetCurrentClimate reads it |
| SetCurrentClimate `climate` | the climate while the player stays in the worldspace it was set in; returns the climate [documented: "climate:ref"; hypothesis: the one set]. [decision: the page's "Bugs" note, the climate going back after about 7 seconds, is not followed] |
| GetWindDirection, SetWindDirection `angle` | degrees, 0 north, 90 east, -90 west, ±180 south [documented]; the set angle read back as given, 400 as 400 [observed: jip12 setwind]; -57.2958 (-1 radian in degrees) when no script set one [observed: jip12 wind, the probes' start cell 0x000DEB16; hypothesis: the same everywhere until a script sets it]. NVForever's own wind still blows where it did when no script set one [decision]. The trees and grass bend to a set angle |
| GetMoonTexture, SetMoonTexture `phase [path]` | phases 0 full to 7 three waxing [documented: the table]; for a phase no script set, `\Textures\Sky\Masser_<phase>.dds` (the files the sky draws) and "" for 4, the new moon [observed: jip12 moon.0, moon.3, moon.4]; a set path is drawn from then on, a leading backslash dropped |
| TriggerLightningFX | outdoors in the rain only [documented]; a flash in the weather's lightning colour (white without one [hypothesis]) and, in a weather with thunder sounds, a thunder clap |

## 2. The world

| Function | NVForever |
|---|---|
| GetActiveIMODs | the modifiers on the screen, once each, in the order applied [documented; hypothesis: the order]; the weather's image space adapters are not modifiers [hypothesis] |
| GetBufferedCells `interiors` | the cells the engine keeps (`nvforever::cell_buffer`: 36 exterior squares, 3 interiors) and the loaded ones, interiors (1) or exteriors (0) [documented]; an empty array when there are none [observed: jip12 buffered.interiors] |
| GetCellBuffered `[cell]` | whether the cell, else the calling reference's, is among them [documented] |
| GetCellNorthRotation `cell` | the Z angle in degrees of a reference of NorthMarker in the cell; 0 without one [documented; hypothesis: the marker is the static FalloutNV.esm names NorthMarker] |
| GetExteriorCell `world x y` | the worldspace's cell at that grid square [documented], only while it is in memory: 0 for WastelandNV -18 0 from the probes' start cell 0x000DEB16 [observed: jip12 extcell; hypothesis: in memory = the buffered and loaded cells] |
| GetWorldspacePersistentCell `world` | the cell of the WRLD group that holds its persistent references [documented]; `RefTable::world_persistent` |
| GetLightAmountAtPoint `x y z` | 0 to 100 [documented], by the rule the detection round measures an actor with: the base light (the sun's outdoors while it is up, the cell's ambient indoors) plus each light within reach, `strength × (1 − (d / reach)²)`; a point outdoors counts as in the sun [hypothesis: no ray is cast to it]; 0 before a round has measured [hypothesis]. The original gave 100 at the player in the probes' start cell 0x000DEB16 [observed: jip12 light.player]; the lab's candidate has no engine to measure, so that row stays apart |
| GetPCFastTravelled | whether the player fast travelled since the calling script last asked [documented]; a script's first ask counts from the start of the session [hypothesis] |
| GetLoadOrderChanged | the loaded save's plugins moved or are gone [documented: "the load-order has changed"; hypothesis: plugins only added after the save's own do not count] |
| GetGameSettings `[kinds]` | a map of setting name to value: 1 the numbers, 2 the texts, 3 both by default [documented]; the settings the load order's records hold, named as the records spell them [decision: the settings only the executable defines are not listed]. The original lists 3192 numbers and 1769 texts (sOk among them) where the load order's records hold 519 and 130 [observed: jip12 gmst.numbers, gmst.strings, gmst.string]; keys ignore case [observed: gmst.lowercase] |
| GetStringHash `text` | 0 [decision: the page is marked Incomplete and names neither the arguments nor the hash]. It takes a string: `GetStringHash "NVForever"` compiles in the original and gives -439019037 [observed: jip12 hash]; none of FNV-1/1a, djb2, sdbm, ×31, ×131, CRC-32 or Adler-32 gives that on the text as written, lower or upper case, so the hash stays unknown. The compiler takes the argument (encodings.json `extra_params`) |
| TogglePurgeOnUnload `[toggle]` | created references only [documented]; one marked is deleted when its cell leaves the loaded area (`Game::cells_left`) [documented]; returns the mark |
| SetInternalMarker `form [toggle]` | whether the form is not drawn, changed when a toggle is given [documented]; by default a form with a marker's model (`marker*` or under `markers\`), or, for a record whose fields the game does not keep (statics), an Editor ID naming a marker [hypothesis]. The engine leaves the form's references undrawn from the next cell drawn [hypothesis: the original's change shows on the next load of the 3D] |

## 3. The barter menu

| Function | NVForever |
|---|---|
| GetBarterGoldAlt | the merchant's caps shown in the trade [documented]; 0 with no trade [hypothesis] |
| GetBarterItems `sold` | inventory references to the items put up, bought (0) or sold (1), one per item; none with none [documented: "Returns 0 if there are no items"] |
| GetBarterPriceMult `sold`, SetBarterPriceMult `sold mult` | the multiplier on an item's value, buying (at least 1) or selling (at most 1) [documented]; set for the trade open [hypothesis: a new trade starts from the rules]; 0 with no trade [hypothesis]. The rows are priced again with it |

## 4. The item and recipe menus

| Function | NVForever |
|---|---|
| GetMenuItemFilter `menu [right]` | 0 all, 1 weapons, 2 apparel, 3 aid, 4 misc, 5 ammo [documented]; the inventory, container and barter menus; the filter kept after the menu closes [hypothesis]; before the inventory was ever open, 1 [observed: jip12 filter.inventory], 0 for the others [hypothesis] |
| GetMenuItemListRefs `[all] [right]` | a map of row index to inventory reference [documented]; the rows shown [decision: NVForever's lists hold only the rows the filter lets through, so `all` gives the same and the keys count the shown rows; the Misc folder rows are left out]; the repair menus give none [decision] |
| GetRecipeMenuCategory, GetRecipeMenuSelection | the open recipe menu's category and highlighted recipe [documented] |
| AddRecipeCondition `recipe func param comp value` | a condition joined with AND to the record's, run on the player, for the session: function `func` (its index; an opcode less 0x1000), the parameter, comparison `comp` against `value` [hypothesis: the page is marked "TO DOCUMENT"; `comp` read as the CTDA operator code, 0 `==` 1 `!=` 2 `>` 3 `>=` 4 `<` 5 `<=`] |

## 5. The quantity and text input menus, the wheel

| Function | NVForever |
|---|---|
| ShowQuantityMenu `callback max [initial]` | menu 1016, the item menus' "How many?" prompt on its own, 1 to `max` from `initial` (`max` without one [hypothesis]); the callback gets the amount next frame, -1 for Cancel [documented]. Not while another menu is open, its own included: 0 and no callback, as ShowTextInputMenu [hypothesis; #2018] |
| ShowTextInputMenu `callback width height title …` | the name menu's box (menu 1051) with the formatted title and the box's size [documented]; Ok gives the callback the text [documented]; 1 when it opened, 0 while another menu is open [hypothesis] |
| SetTextInputExtendedProps `x y [min] [max] [switches]` | the length limits (1 and 32,767 by default), digits and the point only (1), Enter closes (2) [documented]; without switch 2 Enter does nothing [decision: the box is one line]; the box stays centred [decision: the offsets are kept, not drawn]; a call while the box is open changes it at once, the text cut to a shorter limit [decision: #2018] |
| SetTextInputString `text …` | the box's text, formatted [documented] |
| GetWheelDisabled | SetWheelDisabled's flag, 0 for a non-actor [documented]; NVForever's wheel now stays shut for such an actor |

## 6. Follow-ups

- GetStringHash: the hash, once a page or probe says what it is.
- GetGameSettings: the settings only the executable defines (NVForever's `EXE_SETTINGS`
  fallbacks).
- GetMenuItemListRefs: the rows the filter leaves out, and the repair menus.
- SetTextInputExtendedProps: the offsets, and a box of several lines.
- GetLightAmountAtPoint: a ray to the sun for a point outdoors.
