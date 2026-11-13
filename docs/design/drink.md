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

**The marks.** Up to four drinkers stand a step back from the counter, facing across it
toward the bottle [design: parity, 2026-11-13: "like standing at a bar"]. Slot 0 stands
by the straight run, west of the stool 0x00106195 at (-134, -578). Slots 1 to 3 stand
round the curve. Slot 3 keeps clear of the stool 0x00107D6C at about (-277, -428).

| slot | feet (x, y) | heading |
| ---- | ----------- | ------- |
| 0 | (-125, -620) | 0.42 (24 deg) |
| 1 | (-205, -570) | 0.89 (51 deg) |
| 2 | (-245, -510) | 1.40 (80 deg) |
| 3 | (-300, -470) | 1.05 (60 deg) |

Why not nearer the bottle: the marks of 2026-11-12 stood 36 from the counter's end, by the
side wall. The third-person camera sits 40 right of the shoulder (`fOverShoulderPosX`),
then 170 back. There it was pulled in against the wall 0x00107D89, so the contest view ran
along the wall [observed: drink-b's frame `drink_tick_640`, 2026-11-13].

`fnvtool phys cell`, 2026-11-13 [observed]:
- A character dropped on each mark stands on the floor at 3456.7.
- The camera's two legs from each mark (40 right, then 170 back, at 107 up) meet nothing.
- From each such camera, the level ray along the heading first meets a wall 339 to 468
  away. The line to the bottle is clear.
- The side wall's face (x -19.8) ends at y -656; south of it the bar room opens east.
- The counter's outline at the waist runs from (-20, -562) by (-129, -550) and (-219,
  -470) to x -265 on its long run.

The unit test `the_contest_view_looks_across_the_counter_at_the_bottle_not_into_the_wall`
(nvf-play `drink_world.rs`) checks these against that outline:
- each camera stays 45 off the wall;
- the view's centre ray meets nothing within 60;
- the bottle is within 15 deg of the centre for the pair's two players, 30 deg for the others;
- neither of the pair stands on the other's line to the bottle.

**Passing out.** A drinker who passes out is tipped straight backwards, away from the
counter, onto open floor [design].

**The win stage.** The winner dances at (-60, -700), south-east of the marks, and faces
south-south-west (bearing 200 deg) toward the one win camera [design]:
- The stage is more than 60 from every mark and from where each body falls (backwards
  from its mark, 130 long).
- The camera stands 150 out from the winner and 160 up. It looks at the chest, 100 up.
  Behind the dancer are the counter's end with the bottle and the back bar, not a window.
  The dancer hides the side wall's end.
- `fnvtool phys cell`, 2026-11-13 [observed]: at the stage a character stands on the floor
  at 3456.7. Nothing is within 60 at the waist but the wall's end, 59 to the north-east.
  The line from the chest to the camera is clear, and nothing is within 60 of the camera
  round it.
- The ray from the dancer to the camera pulls the camera in if something is in the way.
- The fill light hangs between the dancer and the camera.
- The bar room is open floor west to x -496 and south to the wall `NVGSWallDoor`
  0x00107D81, about 220 away [observed: `fnvtool phys cell` rays at the waist from (-120,
  -660), 2026-11-12]. Its own light, `BasementLightKeyNeutral` 0x00106C17, hangs over the
  counter's end at (-96, -530, 3614).

**Smoke tests.** `drink-table-solo` (the prop, the marks, passing out) and `drink-table-win`
(the stage, the shot, the light), in `tools/smoke/scenarios.toml`.
