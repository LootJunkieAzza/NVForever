# The Anh NVSE plugin's functions (2026-10-30, #1015)

Anh's script functions as NVForever builds them: the pure ones in the script VM
(`crates/fnv-script/src/vm/anh.rs`), the ones on the game in `crates/fnv-game/src/
nvse_anh.rs`. Sources: the GECK wiki export [documented: the page named] and black-box
script probes in Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + AnhNVSE 131 with the 10
official base masters, run 2026-11-23 from `GSDocMitchellHouse` (`0x00103DF9`)
(`[observed]`, no DLL or source consulted). Unprobed details remain `[hypothesis]`, and
deliberate crash-avoidance fallbacks are marked `[design]`.

## 1. Arrays, angles and strings

| Function | NVForever |
|---|---|
| Ar_Max, Ar_Min, Ar_RangeMath `array` | the largest, the smallest and their difference of the array's numbers; other elements left out [documented: "*numeric*" values; observed: 2026-11-23, `GSDocMitchellHouse` (`0x00103DF9`), Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + AnhNVSE 131 with the 10 official base masters, clean-room script probe `anh_01_ar_max_min_range_empty` (`Ar_Max`/`Ar_Min`/`Ar_RangeMath`): `[6,2,10,7]` -> `10.00, 2.00, 8.00`; empty or string-only arrays crash AnhNVSE 131 with `0xC0000005`; no DLL or source consulted]; NVForever safely returns 0 when the array has no numbers [design] |
| DegToRad, RadToDeg `angle` | [documented] |
| GetAngleQuadrant `degrees` | 1-4 for an angle in **degrees**, truncated to integer degrees modulo 360 (`[0, 90) -> 1`, `[90, 180) -> 2`, `[180, 270) -> 3`, `[270, 360) -> 4`) [observed: 2026-11-23, `GSDocMitchellHouse` (`0x00103DF9`), Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + AnhNVSE 131 with the 10 official base masters, clean-room script probe `anh_02_angle_quadrant_edges` (`GetAngleQuadrant`): `0=1, 45=1, 90=2, 135=2, 180=3, 225=3, 270=4, 315=4, -45=4` and radian controls `1.5708, 3.1416, 4.7124, -0.5 -> 1`; no DLL or source consulted] |
| Sv_Capitalize `text` | the first character upper case [documented: "pepeglare snig" gives "P…"] |
| Sv_PadStart, Sv_PadEnd `text length pad` | the pad repeated, cut to fit, before or after the text until it is `length` long; a text already as long, or an empty pad, unchanged [documented] |
| ScancodeToChar `code` | uppercase letters and DirectInput key/mouse names (`1="Escape"`, `2="1"`, `14="Backspace"`, `15="Tab"`, `18="E"`, `28="Enter"`, `30="A"`, `57="Spacebar"`, `59="F1"`, `87="F11"`, `88="F12"`, `200="Up Arrow"`, `256="Left Mouse Button"`), while unmapped codes (`0`, `220`, `221`, `255`, `999`, `-1`) return a 1-byte non-printable string (`sv_Length=1`, `CharToAscii=-1`) [observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + AnhNVSE 131, 2026-11-23 & 2026-11-24, `GSDocMitchellHouse` (`0x00103DF9`), 10 official base masters, isolated clean-room staging copy (read-only `PrintToConsole` probe; zero writes to game install or Saves), `anh_03_scancode_to_char_layout` (`1=[Escape],59=[F1],18=[E],30=[A]`) and `w4d_29_anh_scancode_table_and_zone` (`tools/capture/nvse/catalogs/wave4d_extenders.json`, #3802): `PROBE\|w4d_29_anh_scancode_table_and_zone\|unmap(gsLen=1,0=1/x1D:0/o=-1,220=1/x1D:0/o=-1,221=1/x1D:0,255=1/x1D:0/o=-1,256=17[Left Mouse Button],999=1/x1D:0,-1=1/x1D:0)\|map(2=1,14=Backspace,15=Tab,28=Enter,57=Spacebar,87=F11,88=F12,200=Up Arrow)\|zone(0->0,docZ=00000000)`; zero DLL, PDB, or third-party source consulted]; the exact `0x1D` (`"\x1d"`) raw byte returned for unmapped codes (NVSE `AsciiToChar 29` and `CharToAscii` only cover printable ASCII `32..126`) and unprobed keyboard/mouse scan codes (reusing `keys::KEY_NAMES` for `2..=219` and unmapped for `257+`) remain [hypothesis] |

## 2. Vectors and quaternions (arrays)

Vector3s are three-number arrays and quaternions `[w, x, y, z]` [documented: QNormalizeAlt;
observed: 2026-11-23, `GSDocMitchellHouse` (`0x00103DF9`), `anh_09_q_axis_angle_zero_order`,
`anh_05_q_axis_angle_and_mult_vec3`].
Results are new arrays.

| Function | NVForever |
|---|---|
| V3Mult, V3MultByScalar | element by element; by the number [documented: (1,2,3)·(4,5,6) = (4,10,18); (1,2,3)×2 = (2,4,6)] |
| V3Dotproduct | a number [documented: "the dot product"; the page's return type says array] |
| V3NormalizeAlt, V3CrossproductAlt | unit length; A×B [documented: "the crossproduct of vectorA and vectorB"] |
| V3Lerp `a b t` | a + (b − a)·t on two 3-element arrays without clamping `t` [documented: "linearly interpolates between two points"; observed: 2026-11-23, `GSDocMitchellHouse` (`0x00103DF9`), Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + AnhNVSE 131 with the 10 official base masters, clean-room script probes `anh_04_v3lerp_arrays` (`[0,0,0]..[10,20,30]` at `0.5` -> `3, 5.00, 10.00, 15.00`) and `anh_10_v3lerp_extrapolate` (`t=2 -> 20.00, t=-1 -> -10.00`); no DLL or source consulted] |
| GenerateBezierPoint `p0 p1 p2 t` | the quadratic Bézier point (1−t)²p0 + 2(1−t)t·p1 + t²p2 [documented: "a Bezier curve created by 3 points"] |
| QNormalizeAlt | unit length [documented] |
| QMultQuatQuatAlt `a b` | the Hamilton product A·B [documented: "the product of quatA and quatB"] |
| QMultQuatVector3Alt `q v` | returns a 3-element vector3 turning `v` by `q` (`q·v·q⁻¹`) [observed for 3-element output shape: 2026-11-24, `GSDocMitchellHouse` (`0x00103DF9`), Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + AnhNVSE 131 with the 10 official base masters, `w4c_a09a_anh_quat_vec` (`q90(4)=(0.707,0,0,0.707)\|rX(3)=(1,0,0)\|rY=(0,0,-1)\|rZ=(0,1,0)`) and `w4c_a09b_anh_quat_shift` (`norm(4)=(0.730,0.183,0.365,0.548)\|mul(4)=(-1,4,3,-2)\|int(4)=(0,1,0,0)`) (`tools/capture/nvse/catalogs/wave4c_extenders.json`, #3679): AnhNVSE 131 reads quaternion inputs as `[x,y,z,w]` while writing outputs as `[w,x,y,z]` (cyclic shift by 1); no DLL or source consulted; design: NVForever keeps consistent documented `[w,x,y,z]` math across `QFromAxisAngleAlt`/`QNormalizeAlt`/`QMultQuatQuatAlt`/`QMultQuatVector3Alt`/`QInterpolateAlt` rather than reproducing the plugin's cyclic-shift bug] |
| QFromAxisAngleAlt `axis angle` | the angle in **degrees**, returning `[w, x, y, z]` [observed: 2026-11-23, `GSDocMitchellHouse` (`0x00103DF9`), Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + AnhNVSE 131 with the 10 official base masters, clean-room script probes `anh_09_q_axis_angle_zero_order` (`0 -> 1.000,0.000,0.000,0.000`) and `anh_05_q_axis_angle_and_mult_vec3` (`90` around `[0,0,1]` -> `q=(0.707,0.000,0.000,0.707)`); no DLL or source consulted] |
| QInterpolateAlt `a b ratio [spherical]` | normalised linear by default, spherical with the fourth argument, the shorter way round [documented: the two methods; hypothesis: the shorter way] |

xNVSE's own forms (QFromAxisAngle …) are different functions with observed conventions
(B·A, the conjugate turn, degrees; `vm/quat.rs`); Anh's are not assumed to share them.

## 3. On the game

| Function | NVForever |
|---|---|
| GetArmorArmorRatingAlt `[armor]` (GetArmorARAlt), SetArmorArmorRatingAlt `dr [armor]` (SetArmorARAlt) | as GetArmorArmorRating and SetArmorArmorRating: the argument, else the calling reference's base [documented] |
| GetQuestCompletedAlt `quest` | 1 when completed and not failed [documented] |
| GetZoneLevel `zone` | the level the zone took when its first cell loaded (encounter-zones.md); 0 before [documented: "the current level"; observed: 2026-11-23, `GSDocMitchellHouse` (`0x00103DF9`), Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + AnhNVSE 131 with the 10 official base masters, clean-room script probe `anh_07_zone_level_min_owner` (`GetZoneLevel`/`SetZoneMinLevel` on `Vault11Zone` `000E9081`): `0,6,12`; no DLL or source consulted] |
| GetZoneMinLevel, GetZoneMatchLevel, GetZoneOwner `zone` | the ECZN record's minimum level, "Match PC Below Minimum Level" flag and owner [documented] |
| SetZoneMinLevel `zone level`, SetZoneOwner `zone owner` | round-trips through `GetZoneMinLevel` / `GetZoneOwner`, `GetZoneLevel` on an unentered zone stays `0` after `SetZoneMinLevel 15`, and `SetZoneOwner zone 0` leaves the current owner unchanged [documented: the pages' examples; observed: 2026-11-23 & 2026-11-24, `GSDocMitchellHouse` (`0x00103DF9`), Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + AnhNVSE 131 with the 10 official base masters, probes `anh_07_zone_level_min_owner` (`0,6,12`), `anh_11_zone_owner_readback` (`Vault11Zone` `000E9081`: `GetZoneOwner` `00000000 -> 000A46E7 -> 00104C6E -> 00104C6E`), and `w4d_29_anh_scancode_table_and_zone` (`tools/capture/nvse/catalogs/wave4d_extenders.json`, #3802): `zone(0->0,docZ=00000000)`; no DLL or source consulted]; session-only lifetime (carried across same-run loads via `WorldSession` / `Game::Session`, not saved) and keeping an already-entered zone's level remain [hypothesis] |

### 3.1 Wave 6D-4 clean-room `[observed]` confirmation

Confirmed `Get/SetArmorArmorRatingAlt` (`0 -> 12`), `GetQuestCompletedAlt` (`0`), and
`GetZoneMatchLevel` / `GetZoneMinLevel` / `GetZoneOwner` (`0,6,00000000`) in Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9, 2026-11-25, `GSDocMitchellHouse` (`0x00103DF9`), 10 official base masters, isolated clean-room staging copy (read-only `PrintToConsole` probes; zero writes to game install or Saves); zero DLL, PDB, or third-party source consulted,
AnhNVSE 131 [observed: probe `w6d4p2_40_anh_game_safe`
(`tools/capture/nvse/catalogs/wave6d4_promote_observed.json`):
`PROBE|w6d4p2_40_anh_game_safe|ar(0->12)|qc=0|z(m=0,min=6,own=00000000)|a2s(l=[1.0000002.000000],sm=[1.000000])`].
(`ArrayToString` retains NVForever's documented `[decision]` formatting in `ext-small.md` §1.)

## 4. The last three (2026-11-01)

ArrayToString (the VM), GetCameraFOV and UpdateTubeMesh (`nvse_small.rs`): ext-small.md
§1.
