# The classic VATS style

Status: being built on branch `vats` (2026-11-09). Sections say what is built and what
is planned. The other style, "Dead Eye" (docs/design/vats-mode.md), is unchanged and
selectable; this one is the default.

Tags as everywhere: [documented] cites a public page, [observed] the game's data files,
[hypothesis] an inference, [design] NVForever's own choice. Combat is not a parity
target the project measures, so where the public pages give no number the choice is
[design] and says so.

Sources read 2026-11-09 (public pages only; no executable was read):

- fallout.wiki, "Fallout: New Vegas V.A.T.S.", "Fallout 3 V.A.T.S." (the game settings
  table; Fallout 3's defaults), "Action Points (Fallout: New Vegas)", "Special Attack
  (Fallout: New Vegas)", "Fallout: New Vegas perks", "Fallout: New Vegas traits".
- fallout.fandom.com "VATS" and wikihow.com "Use V.A.T.S. in Fallout 3 and New Vegas"
  refused automated reading; nothing here rests on them.

## 1. The owner's decisions (fork owner, 2026-11-09)

- The original's flow replaces Dead Eye as what V starts; Dead Eye stays as a style the
  settings page offers.
- A slider sets how far the world slows while targets are chosen: 50 % to 100 %, 100 %
  a full stop and the default, as the original.
- The slider acts on choosing targets only. The attacks then play as before.
- Slowed rather than stopped: chances are worked out every frame, an attack queued on a
  target that dies or leaves sight is dropped with its AP back, and the player is not
  protected while choosing.
- While the queued attacks play the player takes the original's share of a hit.
- Controls as the original's; where the pages do not give them, ours, rebindable.
- The rules (hit chance, AP, perks, helpers) are the ones VATS mode already has.
- V with nothing to target does nothing but play the refusal sound.

## 2. Time

- Choosing targets, the clock runs at `1 - slowdown` (`vats_classic::clock_scale`):
  0 at the default, at once, with no ease [documented: the game is paused while targets
  are chosen; design: the range, owner].
- At 0 the virtual clock is not paused, it runs at no speed, so nothing takes the
  picture and menus still open over it. The clocks that must not stop with the world
  count real time then (`pause::stands_still`): the test aids' clock and
  `--run-seconds`. The scripts' frame time stays the world's, none.
- The mode's own timing (the turn of the view, the scans) is in real seconds.
- Playing the attacks: `FIRE_SCALE`, as Dead Eye [design, until §8].
- Sounds follow the clock as they do for Dead Eye (`audio_out::slowed_sound`, floored).

## 3. Targets

Every living actor with a hit volume that is

- within the weapon's reach and `fVATSMaxEngageDistance` (5000) [documented:
  fallout.wiki "Fallout 3 V.A.T.S.", settings table];
- within `CLASSIC_FRONT` (55 degrees) of the line the player looked along on entering
  [design: about what the view shows];
- in sight from the eye (the world in the way hides it, another actor does not);
- not barred as for a Dead Eye mark: dead, cloaked without Enhanced Sensors, No VATS
  Melee with a melee weapon.

Hostile or not [documented: any actor can be targeted]. They are ordered left to right
across the entering view; the one nearest the crosshair is selected first [documented
for the first; design for the order]. With none, V is refused (`UIVATSEnterFail`).

The list is made again every 0.25 real seconds. A queued attack whose target is no
longer in the list is dropped (its AP was only reserved). When the selected target is
gone the selection moves to the one nearest the entering line; with none left the mode
ends.

## 4. Choosing and queueing

- Slide Left / Slide Right: the target before or after, round the ends.
- Forward / Back: the part before or after on the selected target.
- Attack (Use): queue an attack on the selected part. Each reserves the weapon's AP and
  rounds exactly as a Dead Eye mark (`MarkQueue`).
- Block: take the last queued attack back; with none queued, leave.
- Activate: play the queue. V: leave.
- Ready / Reload: queue the weapon's special attack on the selected part (§9).
- Any other action leaves, as Dead Eye (§11.8 there).

[documented: the game's menu file gives E for Accept, the right mouse button for Return
and R for the special attack (spec-notes/vats-menu.md §3); design: the controls those
keys are by default, and the target and part keys, which the file does not name.]

A new target starts on the part with the best chance [design]. Each part's label shows
its name and chance, the selected one bracketed, with "xN" when N attacks are queued on
it; with Concentrated Fire the label shows the chance the next attack on it would have.
Under each label a bar shows the part's condition (its condition actor value of 100; a
part without one has no bar), and at the top of the view the target's name stands over
a bar of its health [documented: the original shows each part's chance and condition;
design: this layout, the engine-drawn screen that stays as the fallback, §11].

## 5. The player and the view

While targets are chosen and while the attacks play the player does not move and the
mouse does not turn the view [documented: the player cannot move or dodge]. The view
turns onto the selected target in real time (`TURN_RATE`). Left without firing, the view
goes back to where it was on entering; after firing it stays on the last target.

The field of view narrows to frame the selected target whole, at
`dialogue_zoom::VATS_TARGET_SHARE` (0.6) of the view's height [design: no page gives a
number]. It is the conversation zoom's machinery (`dialogue_zoom::track`): the same
ownership of the field of view, the same depth of field, the third-person camera
travelling to the eye. Its ease runs in real time (`VATS_IN_SECONDS`), since the world's
clock may stand still, and it eases out when the attacks start to play or the mode ends.

## 6. What it shares with Dead Eye

Entering (weapon, AP and perk checks), the queue, each part's chance, the firing
sequence (shots, bursts, throws, melee runs), rolls and misses, the helpers, Grim
Reaper's Sprint, the kill cameras, the sounds, what the scripts see (GetVATSMode,
GetVATSValue). One set of rules for both styles.

## 7. Damage while the attacks play

A hit on the player is multiplied by `fVATSPlayerDamageMult` while the classic style's
attacks play: 0.75 without the setting [documented: fallout.wiki "Fallout: New Vegas
V.A.T.S.", 75 % in New Vegas, 10 % in Fallout 3]. Not while targets are chosen
[design: owner]. Dead Eye does not apply it, as before.

## 8. The cameras while the attacks play

Each queued attack plays the game's own camera shots (CAMS), chosen by its camera paths
(CPTH) exactly as a kill camera's are (`Game::pick_camera_path`, vats-mode.md §19): the
conditions run on the player with GetVATSValue answering about that attack.

- Before the attack leaves: the path's shots whose Camera Action is Shoot [documented:
  GECK wiki CameraShot, "Camera Action": when the command starts]. The firing waits for
  them, then the round is released.
- After it: the shots whose action is Hit. A burst's later rounds go on meanwhile; the
  next attack waits. The last attack asks for none: if it kills, the kill camera plays
  (its setting applies), so one hit is never shown twice.
- A shot plays for its Min Time, else its Max Time, within 0.5 s and 1.2 s (Shoot) or
  1.8 s (Hit) [design: the original cuts on the attack's own events; these bounds keep a
  queue moving]. The world's clock runs at the shot's Global time multiplier and its
  image space modifier plays, as for a kill camera.
- A path with no such shot leaves the view as it is. Fly shots are not played yet.
- The outcome is not known before the attack lands, so the paths are asked with a hit
  that neither kills nor cripples [design]. The Hit stage knows the roll.
- Melee queues play without attack cameras for now.

Where the camera stands, what it looks at, the walls it stays in front of, the held
controls and the hidden HUD are the kill camera's (`kill_cam.rs`).

Nothing of the HUD is drawn from Accept until the last attack has played, with or
without a camera: the meters, the compass, the enemy bar, the crosshair and its prompt,
the corner messages, the quest notices and the queue's marks [design: the fork owner
2026-11-10] (`VatsMode::playing_back`).

## 9. Special attacks

A melee weapon's special attack is its record's own (WEAP `VATS`: the effect it puts on
the target, the weapon skill it needs, the damage multiplier, its AP) [documented: xEdit
wbDefinitionsFNV.pas WEAP; fallout.wiki "Special Attack (Fallout: New Vegas)"; observed:
FalloutNV.esm, the baseball bat skill 50, x2.0, 35 AP beside its swing's 25; the machete
x0.7, 16 beside 20; the sledgehammer x0.5, 48 beside 38 with an effect; a weapon without
one has zeros]. So the AP is the special attack's whole cost, not an addition.

- Offered in the classic style, for a melee weapon, when the player's skill with the
  weapon is at least the one it needs.
- Ready / Reload (R) queues the special attack on the selected part, as Attack queues
  the weapon's own [documented: vats_menu.xml binds R to its special attack button;
  design: the press queues at once]. While targets are chosen that control does not
  leave the mode. The button reads "special attack: damage xN, M AP" [design: our own
  words; the moves' names are not in the data files]; on the engine-drawn screen the
  header says the same after the target's name.
- A queued special attack reserves and spends its own AP, scaled by the Action Point
  Cost perks as the weapon's attack is [design].
- Its blow's damage is multiplied (with a power attack's multiplier, in `HitInput::power`)
  and its effect goes on the target with the blow's other effects.
- Not built: the unarmed moves the page lists for bare fists (Uppercut, Stomp, Cross),
  which are not in the weapon records read here; the record's Silent and Mod Required
  fields are read by the schema and not used.

## 13. Dead Eye's parts on the same screen (2026-11-10)

Dead Eye's labels for the parts of the actor under or nearest the crosshair
(vats-mode.md §17.5) are the same element as the classic style's: the menu file's
bracket boxes with the chance and the condition meter [design: the fork owner
2026-11-10]. The file is built while Dead Eye marks. Only the boxes are drawn: no
arrows, buttons, queue list or pointer, and no part lit on the body. The HUD's enemy
bar names the previewed actor, hurt or not, and stands at the top of the view while
Dead Eye is on, as in the classic style. The part under the crosshair is named and drawn whole, the
others fainter. Dead Eye's marks ("x" and chance) and everything else of it are
unchanged. With `[UI] VatsMenuFile=0` or without the file its amber labels are drawn
as before.

## 9a. Frame time (2026-11-10)

The chances are worked out every frame. Measured on the owner's machine at the
Goodsprings smoke spot, display sync off (`--log-frame-times`), 40 s runs: 198.0 frames
a second with VATS never opened; 209.5 with the classic style open at a full stop for
the last 28 s with two attacks queued (six labels on a dog). Opening it did not lower
the frame rate; the stopped world costs less than the labels add. Not measured: a
crowded scene, the top graphics settings, a slowdown under 100 %.

## 9b. Planned

- Mines as targets. The unarmed special moves.
- The menu file's Accept and Return buttons as click targets (§11). The lines through
  the lit part (§12).

## 10. Test aids and smoke

- `NVF_PLAY_TEST_VATS_STYLE=classic|dead-eye`, `NVF_PLAY_TEST_VATS_SLOWDOWN=0.5..1`.
- Every Dead Eye scenario sets the first to `dead-eye`; their expectations are
  untouched.
- The classic scenarios are named `vats-classic-*`.
- `NVF_PLAY_TEST_VATS_MENU_FILE=0` keeps the engine-drawn target screen (§11).

## 11. The target screen from the game's menu file (2026-11-10)

By default the target screen is the game's own `menus\vats_menu.xml`, drawn by the tile
renderer (`engine/nvf-play/src/tiles/vats_menu.rs`); what the file holds and what is
written into it: spec-notes/vats-menu.md.

- **From the file**: a bracket on each body part of the selected target with its chance
  and its condition meter, the selected part named; the arrows at the view's edges;
  Accept (dim until an attack is queued), Return and the special attack button with
  their key hints; the queue's list of queued attacks with its divider.
- **Still the engine's**: the target's name and health, which are the HUD's own enemy
  bar (its bracket, ticks and name), shown for the selected target hurt or not and
  moved to the top of the view while targets are chosen [design: the fork owner
  2026-11-10; the original keeps it at the bottom]; the HUD's HP and AP meters; the
  amber grade.
- **The switch**: on unless `[UI] VatsMenuFile=0` is in `NVForever.ini` [design: the
  fork owner's decision, the game's file first]. Upstream's convention is the other
  way round (`[UI] TileMenus=` lists the menus drawn from files, none by default); to
  go upstream this would become a name in that list.
- **The fallback**: a missing file, or one without a path the code writes, keeps the
  engine-drawn screen of §4 for the run and warns once with the reason.
- **Life**: VATS is no menu to the runtime (`Game::menu`), so the file is built on the
  frame the screen comes up (`Game::ui_vats`) and dropped on the frame it goes. While
  it is up `screen()` is in interface units and its `cropx` and `cropy` are 15; a mod's
  HUD tiles that read `screen()` see those values for that time.
- **The pointer** is shown while targets are chosen. Coming onto a part's box, or
  within 45 units of the part's point on the target, selects that part; the file's
  Select button shows then, with the left button's hint. A click queues an attack on
  the selected part; a click on one of the two arrows changes the target instead. The
  keys of §4 go on working, and the pointer selects only when it moves. The buttons
  (Accept, Return) are not clicked yet.
- **The player's arms and weapon** are not drawn in first person while targets are
  chosen.
- **The UI scale** (the settings page's "UI scale", 50 to 150 %): the screen is drawn
  in the view's own frame as the HUD is, with `screen()` written for a view 960 / scale
  units high, so the arrows, the buttons and the queue keep to the window's edges and
  the boxes stay on the target. It shrinks with the scale and, like the menus, does
  not grow above 100 %: the file sets its buttons and queue out for a view 960 units
  high, and at 150 % they ran into each other [observed: the engine's screenshot]. The
  enemy bar is the HUD's and takes the whole scale; the engine-drawn labels and marks
  (the fallback, Dead Eye's marks) do too.
- **The two arrows** stand halfway down the view, on one line at the left and right
  edges, at any UI scale [design: the fork owner 2026-11-10; the file, and the
  original on the owner's screenshots, put them a quarter of the way down].
- **The right mouse button's hint** is NVForever's "[RMB]" (it was "[Right Click]"): the
  file gives a hint its button's width as room, and "Return" is short.

## 12. The selected part lit on the target (2026-11-10)

The part the next attack would be queued on glows in the interface's amber on the
target's own body, and its box on the screen is drawn fainter than the others
[observed: the owner's screenshots of the original's VATS, 2026-11-10]
(`engine/nvforever/src/vats_highlight.rs`).

- A part's triangles are those carried mostly by the bones of its chain, the bones the
  hit volumes use; a bone in no chain goes with the chain above it [design].
- Drawn as an unlit copy of those triangles on the same skin, blended over the surface. The original's
  fine horizontal lines through the glow are not drawn yet.
