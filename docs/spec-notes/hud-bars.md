# SUP HUD bars: bound values and visibility conditions (2026-11-09, #1834)

Split out of #977 (docs/spec-notes/menu-xml.md §9, docs/design/ui-tile-tree.md). Code:
`crates/fnv-game/src/ui_hudbar.rs` (conditions, fades, HudBarGetVisible, `tick`),
`crates/fnv-game/src/nvse_sup7.rs` (the bound values, docs/spec-notes/ext-sup7.md §3),
the frame step in `Game::tick` (`crates/fnv-game/src/game.rs`). Tests: the units in
`ui_hudbar.rs`, `crates/fnv-game/tests/ui_more.rs` (`hud_bar_conditions_…`,
`hud_bar_item_condition_…`, `hud_bars_vanish_…`), `crates/fnv-game/tests/sup7.rs`
(`a_hidden_bar_is_not_read_…`). Smoke: `ui-hudbar-bound-conditions`.

Pages (GECK wiki export 2026-10-28): HudBarSetBarTrait (revision of 2021-05-15),
HudBarGetVisible, HudBarShowBar, HudBarSetItem, HudBarSetFormList, HudBarSetValueUDF,
HudBarSetValueScriptVar.

## 1. Bound values

- A bar bound by HudBarSetValueScriptVar or HudBarSetValueUDF reads its source each
  fUpdateTimer seconds (bar trait 2, 0.1 s by default) [documented: both pages, "Value
  will be updated each instance of Bar update"], with iCalculateValueMax as the share of
  the max [documented]; built in batch 7 (ext-sup7.md §3).
- A bar hidden by HudBarShowBar 0 is not read [documented: HudBarShowBar, "Bar value
  calculations will stop processing"]. Shown again, it is read in that frame [design].
  A bar its conditions hide is still read [hypothesis: the page names HudBarShowBar
  only].
- Timer catch-up [design]: one read per due frame, the next due fUpdateTimer seconds of
  game time later; reads missed during a long frame or a menu are not made up. A timer of
  0 or less reads every frame.
- A failing source [design]: a UDF that errors or gives no number, or a variable whose
  script instance is gone, leaves the value as it was; the binding stays and is tried
  again at the next interval. A destroyed bar drops its binding; a new source replaces
  the old.

## 2. Conditions

[documented: HudBarSetBarTrait] Trait 0 (menu type): 0 always shown, 1 shown only in game
mode; 21: 1 hidden while the player's movement controls are disabled; 22 sneaking, 23
combat, 24 aiming (iron sights or scope): 1 hidden while it holds, 2 shown only while it
holds; 25: 1 hidden unless the bar's item is equipped (HudBarSetItem, HudBarSetFormList);
26: 1 hidden with the weapon out, 2 hidden without it; 27: 0 both cameras, 1 hidden in
third person, 2 hidden in first person.

What NVForever chose where the pages leave it open:

| Open point | Choice | Tag |
|---|---|---|
| Trait 0's two alternatives both written "1" | 1 game mode only, 2 menu mode only, as 22-24 and 26 number their second alternative | [hypothesis] |
| How conditions combine: the note says "any combination is possible", its example "appear only when aiming in combat, in other cases it will be shown" contradicts itself | every set condition is a veto; shown only when none hides it (the example's first half: 23 and 24 at 2 hide the bar unless in combat and aiming) | [design] |
| Item and list both set | either equipped is enough | [design] |
| Trait 25 with no item and no list, or an empty list | hidden (nothing named can be equipped) | [hypothesis: the trait's words] |
| A value the page does not list (22 at 3, 21 at 2) | hides nothing | [design] |
| What the conditions read | the player's state as the engine tells it: `Game::in_menu`, the movement lock of DisablePlayerControls(Alt), sneaking, IsInCombat's player answer, the sights up (the engine's `Combat::player_sights_up`: a ready gun with sights, Block held, no reload or jam holding the sights down; so not while blocking with a melee weapon or reloading), weapon out, the camera | [design: `ui_hudbar::Situation`, `nvse_jip2::PlayerTold::sights_up`, told by `engine/nvf-play/src/script_input.rs`] |

## 3. Vanishing and appearing

[documented: HudBarSetBarTrait] 41 at 1: a bar hidden by traits 21-26 vanishes over 44's
seconds by becoming transparent; 42: 0 no movement, 1 horizontal, 2 vertical, by 43's
pixels (positive to the right); 61-64 the same for appearing, the bar coming from the
shift to its place.

- Hidden by trait 0 or 27, or by HudBarShowBar, the change is at once [hypothesis: the
  fades are written for traits 21-26]. Shown again after trait 0 or 27 hid it last, it is
  at once too; the condition that hid it last decides (hidden by 27, then held only by
  22, it fades in) [design]. A bar is shown by HudBarShowBar where its conditions put it,
  without a fade [design].
- [design] Linear in time, by the frame's real seconds. A change that turns back half way
  goes back from where it is at the other way's speed, without a jump in alpha or place:
  the bar moves on from its shift at that moment towards the new way's end (its place, or
  42/43's shift); from hidden it appears from 62/63's shift. A timer of 0 or less is at
  once. Every element's alpha is multiplied by the bar's share; at 0 the bar's rect is not
  drawn. Vertical positive is downwards [hypothesis: the interface's y].
- [design] The fades step in `Game::tick`: on in menu mode (the Pip-Boy, a conversation;
  game time stands still there but the frame's seconds count), waiting with the game
  under the pause menu and a loading screen, where the engine runs no `Game::tick`
  (`engine/nvf-play/src/script.rs`).
- [design] During a fade only the bar's place, its being drawn and the elements' alpha are
  written to the tiles.
- A tree built again (a load) finishes a running change at once [design].

## 4. HudBarGetVisible

[documented] Element 10: -1 when the bar is hidden by its conditions ("if it's set to be
hidden in First person"), 0 when hidden by HudBarShowBar, 1 visible. [hypothesis] Hidden
both ways answers 0; every condition (0, 21-27) gives -1; the answer is read at the call,
so -1 from the moment a condition hides the bar (while it still vanishes) and 1 from the
moment none does (while it still appears). Elements 0, 1, 3 answer their own
HudBarSetVisible.

## 5. Saved

Nothing [design]: the bars, their bindings, conditions and fade progress are session
state (`UiExtra`, `Sup7State`), as the bars were before; they live through a load in the
same session (the HUD is made again, iRemoveOnGameLoad bars dropped) and are not written
to a save. The ScriptVar page's own note says a script variable bound is lost on a hot
reload [documented: HudBarSetValueScriptVar].

## 6. To probe in the original

The trait 0 alternatives, the combination rule (23 and 24 at 2 outside combat), trait 25
with nothing named, whether a condition-hidden bar keeps reading its source, and the fade
shape: docs/handoff/observer-checks.md has no row yet; each is a short `bars` row.
