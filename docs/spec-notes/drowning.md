# Spec note: breath and drowning

The player's breath under water, drowning, Water Breathing and the HUD's breath meter.
Code: `crates/fnv-game/src/breath.rs` (the rule), `engine/nvf-play/src/drowning.rs` (the
head under water, the damage, the sound), `engine/nvf-play/src/hud_breath.rs` (the meter).
Captures: `tools/capture/jobs/make_drown_jobs.py` (game data stays private).

## 1. Sources

- Captures of the original, save `nvfdoc`, Lake Mead at Boulder Beach (the swimming jobs'
  spot, physics.md §6.6): `sw_breath` (2026-10-08), `dr_short`, `dr_rules`,
  `dr_endurance`, `dr_endurance2` (2026-10-25). Each dive puts the player on the lake bed
  790 units under the surface with `SetPos` (a held forward key with the view turned down
  left the player floating in this setup), god mode off, Health raised by
  `ModAV Health 5000`; health sampled every 1–3 game loops against the game clock.
- `FalloutNV.esm` [observed]: MGEF `WaterBreathing` 0x0005876D (Value Modifier, actor value
  Water Breathing, flags No Magnitude, No Duration, Recover); its only givers are the
  Rebreather (ARMO `VMS15Rebreather` 0x00100633, object effect `VMS15WaterBreathing`; its
  script adds the ability `VMS15WaterBreathingActual` 0x00100630 on equip) and the
  mirelurks' and lakelurks' abilities, all with magnitude 0. SOUN `NPCHumanDrowning`
  0x0004D23F: the folder `fx\npc\human\drowning\`, three takes of 1.36–1.66 s. No GMST
  names breath or drowning (physics.md §6.6).
- `menus\main\hud_main_menu.xml` `BreathMeter` [documented]: the model
  `Interface\HUD\AirTimer01.NIF`, animation `Forward`, 64 × 64, colour `&hudmain;`, no
  position. `menus\breath_meter_menu.xml` [documented]: another breath meter (a bar,
  `Breath_Meter_empty.dds` / `_fill.dds`, 72 from the top, 64 high, centred), which the
  HUD file does not use.

## 2. The rule

| What | Value | Tag |
|---|---|---|
| Under water | the eye below the surface of the water it is in, swimming or walking the bed | [observed: `sw_walkin` drowned a player walking the bed; hypothesis: the eye as the head] |
| Breath | 10 s at Endurance 1–5, 12.5 s at 6–8, 15 s at 9–10 | [observed: onsets 10.0 (1, 3, 5), 12.4–12.5 (6, 7, 8), 14.9–15.0 (9, 10)] |
| Above 10 | 15 s | [hypothesis] |
| Back at the surface | whole again at once | [observed: whole after 0.5 s at the surface, after 6 s under and after drowning] |
| Drowning | a fifth of the maximum health a second, every frame | [observed: 24.0, 40.0, 60.0 a second at maximum health 120, 200, 300; `ModAV Health 5000` did not change it; 0.64 a game loop of 16 ms] |
| Water Breathing | no breath used, no drowning | [observed: the Rebreather's ability, and `SetAV WaterBreathing 1` alone; `GetAV WaterBreathing` read 1 after the latter] |

Water Breathing counts when the actor value is above 0 **or** an active Value Modifier
effect on Water Breathing is on the actor, whatever its magnitude: the masters' givers all
have magnitude 0 [hypothesis: the effect's presence is what counts; the capture with the
ability did not read the value].

Not measured: whether Endurance alone decides the breath (the level, Health and Agility
were not varied), why the steps fall at 6 and 9, whether a lower Endurance cuts a breath
already held (NVForever cuts it), NPCs and creatures drowning.

## 3. In NVForever

- **Who**: the player only [hypothesis: NPCs' paths keep out of deep water; a capture of an
  NPC held under would say].
- **Time** runs only while the game does (not in menus, dialogue or the loading screen).
- **Damage** (owner 2026-10-25): dealt in hits of a quarter of a second's worth
  [design], through `Game::hit_without_lines`: no Hit, Crippled, HealthHalf, HealthQuarter
  or Death lines, no flinch, no blood on the screen ("should not hear the player hit
  sounds", "remove the death sound effect from drowning to death"). God mode spares the
  player. The death music still plays.
- **Sound** (owner 2026-10-25, "specific drowning sound effects"): `NPCHumanDrowning`, a
  take at random, as the breath runs out and every 1.7 s while drowning [design: the
  longest take and a little; the original's rhythm is not measured].
- **Meter** (`hud_breath.rs`): the model's flat pieces (the word O2 and sixteen ticks cut
  from `textures\interface\hud\air_meter.dds`, in the model's x–z plane) drawn as turned
  pictures in the HUD's colour, the model's bounds filling the 64 × 64 box. `Forward`
  (16 frames of 1/30 s) switches `TickNN` off at frame NN: `Tick01`, just right of the top,
  first, round to `Tick16` at the top [observed: the vis keys]. The meter shows the
  sequence at the share of breath used [hypothesis] while the head is under water and
  until the breath is whole [hypothesis]. Its centre is in the middle across and 104 down
  the 960-high interface: `breath_meter_menu.xml`'s place, and where the owner put it
  (2026-10-25, "needs placing center high").
- Logged: `drowning: head under water …`, `drowning: out of breath …`,
  `drowning: <n> damage, health …`, `drowning: sound NPCHumanDrowning`,
  `drowning: head above water …`, `hud: breath meter read: …`.

## 4. Checking

- Unit tests: `fnv_game::breath::tests`, `nvf_play::hud_breath::tests`.
- Smoke scenarios `drowning` (on the bed with a new character: 10 s, then 40 a second
  with the drowning sound and no hit or death lines, dead after 5 s; the meter read) and
  `drowning-rebreather` (with the ability: never out of breath).
- Open for the owner: the sound's rhythm; the meter's look against the original's.
