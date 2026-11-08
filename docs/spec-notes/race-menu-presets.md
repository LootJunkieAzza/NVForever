# Race menu presets (`RaceMenuPresets\*.preset`)

Named character looks a player can pick in the race menu's Preset row, read from loose
`.preset` files in the data folder's `RaceMenuPresets` folder (installed as a mod). The
layout below was read from the eight files of one Nexus pack (mod 83357, 2026-11-07),
by comparing the files with each other and with the records they name [observed]. No
extender source was read.

## Layout (little-endian)

| Field | Size | Meaning |
|---|---|---|
| version | u32 | 1 in every file seen [observed] |
| size | u32 | the file's length minus 8 [observed] |
| FGGS | 50 × f32 | symmetric geometry coefficients [observed: same counts as NPC_ FGGS/FGGA/FGTS] |
| FGGA | 30 × f32 | asymmetric geometry |
| FGTS | 50 × f32 | symmetric texture |
| female | u32 | 1 female, 0 male [observed: matches the pack's female/male split] |
| hair length | f32 | 0..1 [hypothesis]; not used yet |
| hair colour | 4 × u8 | r, g, b, unused [hypothesis from the values: greys 80 80 80, browns 18 12 12] |
| race | form | the race |
| hair | form | the hair (HAIR) |
| eyes | form | the eyes (EYES) |
| head part count | u8 | |
| head parts | count × form | brows, beards (HDPT) |

A **form** is a u32 FormID followed by the defining plugin's file name, ended by a zero
byte. The FormID's top byte is the maker's load-order slot and means nothing on another
machine; the low 24 bits with the named plugin's place in the player's load order give
the form [observed: `CharacterKitRemake-Hair.esp` forms carry slot 0x4E].

## What NVForever does with them [design, owner 2026-11-07]

- The Preset row (was "Starting face") lists the race's starting faces (NPC_ "Is CharGen
  Face Preset"), then the named presets of the chosen sex from every race, by name.
- Picking a named preset sets its race, face, hair, hair colour, eyes and head parts. The
  surgeon, who never changes the race, is offered only the named presets of the look's race.
- Every preset, a starting face too, comes with a voice: one of the voices that fit the
  look, picked from the preset's name (or record) so the same preset always gives the same
  voice. The Voice row still changes it.
- A preset that names a plugin not in the load order, or a race the menu doesn't offer, is
  not listed; the log says why (`creator: preset ...`).
