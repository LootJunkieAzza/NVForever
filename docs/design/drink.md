# The drinking contest in the world

NVForever's own mini-game [design: owner 2026-11-11; the original game has no drinking
contest]. This page covers where its pieces stand. The contest itself and its screen are
`crates/fnv-game/src/drink_contest.rs` and `engine/nvf-play/src/drink_contest.rs`. Co-op
and stakes are `engine/nvf-play/src/drink_coop.rs`. The prop is
`crates/fnv-game/src/drink_table.rs`, the world side `engine/nvf-play/src/drink_world.rs`,
and the win shot's camera and light `engine/nvforever/src/drink_stage.rs`.

## The Prospector Saloon (`goodsprings`)

**The prop.** A whiskey bottle stands on a $100 NCR bill on the bar counter, at the
counter's short end by the side wall, a hand's width in from the wall [design: owner, ship17
play, 2026-11-12: "Put the dollar/bottle here and get rid of the table it was on"].
Until then the prop stood on a round `Table01` that the game made in the middle of the
floor at (215, -725). That table is gone.
- The counter [observed: FalloutNV.esm]:
  - It is `OfficeReceptionDeskMid01` 0x00107D6E and `OfficeReceptionDeskEndcapL01`
    0x00107D6D at (-216, -152), plus `OfficeReceptionDeskCurveR01` 0x00107D6F at (-216,
    -408), all at z 3450 and turned a quarter.
  - The curve's end meets the wall `NVGSWallONLY` 0x00107D89 at (0, -528).
- Its top, from `fnvtool phys cell` rays, 2026-11-12 [observed]:
  - The rays hit the curve's collision at **3528.1** over the end: x -80 to -25, y -470
    to -560.
  - The wall's face is at x -19.8.
  - The customers' face of the counter is at y -562.4. The barkeep's side starts near y
    -465.
- The bill lies at (-38, -515), 0.15 over the top. The bottle stands 0.1 over the bill.

**Saves from before the move** [design]. The pieces keep their fixed numbers:
0xFF7FA101 is the bill and 0xFF7FA102 the bottle. On cell load:
- A bill or bottle found at the old spot is moved onto the counter.
- The old table, 0xFF7FA100, is marked deleted.

**The marks.** Up to four drinkers stand along the counter's front, facing it [design].
Slots 0 and 1 stand on its straight run by the bottle. Slots 2 and 3 stand round its curve,
either side of the saloon's stool 0x00106195 at (-134, -578).

| slot | feet (x, y) | heading |
| ---- | ----------- | ------- |
| 0 | (-52, -598) | 0 (north) |
| 1 | (-98, -598) | 0 |
| 2 | (-181, -560) | 0.51 |
| 3 | (-220, -530) | 0.81 |

`fnvtool phys cell`, 2026-11-12 [observed]:
- A character dropped on each mark stands on the floor at 3456.7, touching nothing else.
- From each mark, a ray along its heading meets the counter 36 to 43 away. A body's radius
  is 22.

**Passing out.** A drinker who passes out is tipped straight backwards, away from the
counter, onto open floor [design].

**The win stage.** The winner dances at (-140, -680), in front of the counter, and faces
south toward the one win camera [design]:
- The stage stands between where the bodies from marks 1 and 2 fall.
- The camera stands 150 south of the winner and 160 up. It looks at the chest, 100 up, so
  the counter and the back bar are behind the dancer, not a window.
- The ray from the dancer to the camera pulls the camera in if something is in the way.
- The fill light hangs between the dancer and the camera.
- The bar room is open floor west to x -496 and south to the wall `NVGSWallDoor`
  0x00107D81, about 220 away [observed: `fnvtool phys cell` rays at the waist from (-120,
  -660), 2026-11-12]. Its own light, `BasementLightKeyNeutral` 0x00106C17, hangs over the
  counter's end at (-96, -530, 3614).

**Smoke tests.** `drink-table-solo` (the prop, the marks, passing out) and `drink-table-win`
(the stage, the shot, the light), in `tools/smoke/scenarios.toml`.
