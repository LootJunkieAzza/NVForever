# Grass fires

A design feature, not a parity target: the original game has no fire that spreads. The
owner asked for it on 2026-11-14 (#2567), modelled loosely on open-world shooters where a
flamer or a fire blast sets a field alight: a front of flame sweeps outward over the grass,
leaves the ground black behind it, catches anyone standing in it and goes out within a
hard limit. Every figure here is a design choice [design] unless tagged otherwise.

Code: `engine/nvf-play/src/grass_fire.rs` (the model, the ignition, the actors),
`engine/nvforever/src/grass.rs` and `grass.wesl` (fuel, burnt grass on screen),
`engine/nvforever/src/grass_fire_fx.rs` (burn map upload, cells forgotten, wind, flames).

## 1. What lights a fire

- A shot that lands on the ground, not on an actor, with a weapon whose object effect
  (WEAP `EITM`) burns, and a blast on dry ground whose EXPL object effect burns.
- An object effect burns when one of its effects is resisted by Fire Resistance (the
  base effect's resistance is actor value 52, `fnv_game::combat_lines::FIRE_RESISTANCE`):
  the flamer's and Shishkebab's "Burning", the incinerator's and incendiary grenade's
  blasts [observed: hit-effects.md §1 names these items]. Nothing names fire in the data
  other than this, so the resistance is the test.
- A harmless shot (Do No Damage) lights nothing; a blast that threw up water lights
  nothing.
- The landing lights its patch, or the nearest of the eight around it with grass when it
  landed just off the grass. Grass already burning or burnt does not light again.
- The switch `grass-fire` (Gameplay settings, NVForever.ini `[Settings]`) turns it off; it
  is on by default.

## 2. Patches and fuel

- The ground is split into squares of 64 game units (about 0.9 m). A patch has fuel when a
  grass plant stands on it; the renderer, which places the grass (grass.md), registers each
  cell's patches with the height of the lowest plant's ground.
- Grass switched off (`--no-grass`, settings) means no fuel: nothing burns.
- When a cell is neither loaded nor being loaded (it fell behind the player, or the player
  went indoors) its patches and what burnt of them are forgotten. The grass regrows the
  next time it loads: burnt grass is not kept in saves, and it comes back whole.

## 3. Spread

- A patch burns for 6 s (±20%, fixed per patch). When it catches it plans to light each
  patch up to two patches away (the 24 around it) that has fuel and has not burnt: after
  0.5–1.5 s times the distance in patches (1 for a side neighbour, √2 diagonally, 2 for
  one across a patch). Reaching two patches lets a fire cross the bare ground between the
  Mojave's tufts of grass (the grass near Goodsprings stands in tufts with gaps; a fire
  limited to touching patches stopped at the first gap).
- The wind (the weather's wind direction and speed, as the grass sways with) divides the
  delay by 1 + 0.6 × (wind strength × the cosine between the wind and the step): at full
  wind the fire runs 1.6 times as fast downwind and 0.4 times as fast upwind.
- The random values are fixed by the patch and the fire, so a fire spreads the same way
  each run (tests, smoke).

## 4. Limits

- **Reach**: no patch further than 1,024 units (about 15 m) from where the fire started.
- **Size**: no more than 500 patches per fire.
- **Fires**: no more than 8 at once; a landing beyond them is refused.
- **Burning**: no more than 1,500 patches burning at once over all fires.
- A landing on grass a live fire may still reach (within its reach, under its size) adds
  to that fire instead of starting another, so a flamer sweeping a field grows one fire
  until its limit.
- A fire is out when nothing of it burns and nothing waits to be lit.

## 5. Actors

- Twice a second, every actor whose middle is within 64 units (horizontally) of a burning
  patch's middle and within 160 units of its ground height catches the fire's object effect, as from a hit by the one who started the
  fire (`hit_effects::apply`): the same burn, damage over time, OnFire line, kill credit
  and resistances as a flamer hit. Standing in the fire refreshes the burn's time.
- The player catches it too (not in god mode, as with any hit effect).

## 6. On screen

- **Burn map**: a 512 × 512 single-byte texture, one texel per patch, wrapping every 512
  patches (32,768 units, more than the loaded cells span, so two patches that share a
  texel are never both loaded). 0 is untouched; up to 127 while burning (the grass
  blackens over the first 2 s); 128–254 burnt (the grass sinks into the ground over 5 s);
  255 burnt away.
- **Grass** (grass.wesl): each plant reads the patch it stands on. Its colour falls to
  5% as it chars, it glows orange like embers, flickering, while its patch burns, and once
  burnt it sinks into the ground the way grass beyond the grass distance does.
- **Scorched ground** (terrain.wesl "Scorch"): the landscape under a patch that caught
  turns black (its colour × 0.16), reaching full by the time the patch is charred, and
  stays black until its cell unloads. It is read between the patches' middles and broken
  up with noise about 20 units across, so the mark has a ragged edge, not 64-unit
  squares. Only the folded landscape draw reads it (every quadrant on a GPU with bindless
  textures, the usual case); a quadrant's layers past the sixth, drawn blended over it,
  cover the mark.
- **Flames** (grass_fire_fx.rs) [owner 2026-11-14: the first version looked "a bit
  sparse"; flames should start small and grow]: each patch that catches gets two small
  particle fires (`mps\mpsfiresmall01.nif`) at random spots and, on about a third of the
  patches, a larger one (`mps\mpsfiremed01.nif`). Each starts at 15% of its size and grows
  to it over 2 s (its particles' size, a size factor on the model's emitter added for
  this, `particles::ModelEmitter::size`, and its spray), and dies down over 1.2 s when its
  patch burns out. About one patch in six has a flickering orange light (at most 16).
  Half the burnt-out patches smoulder (`mps\mpssmokesmall01.nif`) for 5 s. At most 420
  flame and smoke models at once. (The ambient `fxfiresmall01.nif` holds no particle
  system or shape of its own and drew nothing.)

## 7. Not done (yet)

- Trees, shrubs and placed plants do not burn; only generated grass does.
- No light from the fire beyond the embers' glow and the flame model's own.
- Rain does not put fires out.

## Log

- `grass fire <n>: started at (x, y, z) by <who> with <item>`
- `grass fire <n>: out after <s> s: <p> patches burnt, reached <r> units of 1024, <h> spreads held by the limits, <a> actors caught fire`
- `grass fire: <actor> caught fire from fire <n>` (once per actor per fire)
- at exit `grass fires summary: …` and `grass fire fx summary: …`
