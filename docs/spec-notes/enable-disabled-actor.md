# An actor enabled while the game is held (#2616)

What happens when a script enables an actor that was disabled when its cell loaded, and
the same script starts a conversation with it in the same frame. Found with the Legion
route of the Hoover Dam battle (quest `VHDLegionBattle`), in the intake tower.

## 1. The data

- `HooverDamIntIntakeTower01` (CELL 0x00133FB1, interior) holds
  `VHDIntakeTowerCenturionREF` (ACHR 0x0013CA6B, base NPC_ `VHDIntakeTowerCenturion`
  0x0013CA37), placed persistent and **initially disabled**, with no enable parent
  [observed: `fnvtool world refs --cell 0x00133FB1`, FalloutNV.esm]. Its base takes its
  traits from the leveled list `EncVHDCLCenturion`, which stands for `VHDCLCenturion1`
  [observed: NVForever log, `zones: leveled actor 0x0013CA6B (EncVHDCLCenturion) ...
  stands for VHDCLCenturion1`]. The Legionary `VHDIntakeTowerLegionary` 0x00177D8A in the
  same cell is placed the same way [observed: same dump].
- The hatch terminal `VHDIntakeTowerHatchUnlockTerminal` (TERM 0x0013CA36, placed
  0x0013CA6D) lists one item, "Unlock Maintenance Hatch". Its result script enables the
  Centurion with a fade, and in the same run his package is chosen again and a
  conversation with him begins [observed: NVForever log, `terminal: its result script:
  ... RefEnabled { reference: 0x0013CA6B, fade: true }; PackageChosen { actor:
  0x0013CA6B, package: Some(0x0013CA7E) }; DialogueStarted ...`]. The Centurion stands
  about 100 units from the terminal [observed: the two placements' positions].
- Enable fades a reference in unless its FadeIn is 0 [documented: GECK wiki Enable;
  docs/design/script-function-params.md §3].

## 2. What NVForever did (before the fix)

- Actors of a loaded cell are residents (`engine/nvforever/src/residents.rs`) whether
  enabled or not; each frame the residents read whether each actor is present (in the
  world and enabled) and draw those that are [observed: code].
- That reading was done after the AI's update. While a menu, a conversation or a loading
  screen holds the game the AI's update returns early, so the presence was not read
  again until the game went on [observed: code, `Core::think`].
- The terminal's result script runs while the terminal menu holds the game, and the
  conversation it starts holds it from the same frame on. The Centurion stayed "absent"
  for the residents through the whole conversation: no body, no head or face rig, the
  view turned to the air above his reference, no zoom, no lip sync [observed: smoke
  `enable-disabled-actor-talks` on main, `dialogue look: VHDIntakeTowerCenturionREF is
  not drawn as an actor`, `lip: skipped ... has no face rig here`, no line of his body
  being drawn].
- Enabled while the game runs (a console Enable, a StartConversation he walks up for),
  the same actor was drawn at once [observed: the same cell with a console Enable and
  StartConversation from 593 units away: he walked up drawn and talked with lip sync].
  So it was not the disabled-at-load path or the leveled template: it was the held game.

## 3. What NVForever does now

- The residents read who is present also while the game is held, so an actor a held
  script enables (a terminal's result, a dialogue result) is drawn in that frame's
  drawing pass and fades in, and one it disables is taken away [hypothesis: the original
  draws it at once; the conversation shows the speaker, the owner's report of #2616
  expects that].
- The body is drawn once its parts are read (`readahead::ready_to_draw`), a few frames
  after the Enable (0.09 s in the smoke run) [observed]. The conversation's look and zoom
  find it then, as they look for the speaker every frame. The lip sync of a response
  whose speaker has no face rig yet looks for it again for 1.5 s of real time
  (`lips.rs` `RIG_WAIT_SECONDS`) [engineering choice] and then follows the line from
  where its sound is; after that the response is skipped as before (a creature, a
  robot, an activator).
- Still open: the view's opening turn is aimed in the conversation's first frame, before
  the body is drawn, at the point above the reference (`dialogue look: the view turns
  to ... (above its reference: it is not drawn)`); the zoom then frames the drawn head.
