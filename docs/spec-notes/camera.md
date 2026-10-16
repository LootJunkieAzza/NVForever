# Spec note: camera field of view (Layer 3)

Tags as in `nif.md`. Measured against the owner's reference frames of the original game
(FNV 1.4.0.525, `tools/capture/README.md` §7); the frames and the per-camera renders are
private and not committed.

## 1. The setting

- The game's field of view is a single number in degrees, 75 by default: `fDefaultFOV=75`
  in the owner's `FalloutPrefs.ini` for every capture [observed], and the script function
  `SetCameraFOV` falls back to 75 when its argument is omitted or 0 [documented: GECK wiki
  via `script-functions-catalogue.md`].
- Which angle the number measures (horizontal or vertical, and at which aspect ratio) is
  not stated in any source the project has. §2 measures it.

## 2. Measurement (2026-10-09) [observed]

Reference frames: the four starter cameras of `tools/capture/frames/cameras.json`
(`goodsprings_noon`, `goodsprings_dusk`, `goodsprings_hill_lod`, `lake_shore_noon`),
captured in first person with the game windowed at 1280 × 720 (16:9), `fDefaultFOV=75`.

Method: `nvforever` renders each camera with the lab scenario's own arguments
(`L3.frame.*`: pose, `--eye-height=120`, hour, weather, `--size=1280x720`, no text
overlay) and only `--fov` changed. `--fov` was then the horizontal angle at the window's
aspect, so each value below is the horizontal angle at 16:9. Score: the lab's image SSIM
(`parity-lab/fnvlab/image.py`: luminance, 192-pixel working width). The remaining
differences after alignment are rendering (sky and clouds, grass, tone), so absolute SSIM
stays low; the peak position is what matters.

Candidate rules for "75":

| Rule | Horizontal at 16:9 | Vertical | noon | dusk | hill_lod | lake |
|---|---|---|---|---|---|---|
| 75 horizontal at the window's aspect (old `nvforever`) | 75.0° | 46.7° | 0.181 | 0.237 | 0.182 | 0.179 |
| 75 horizontal at 16:10, vertical kept | 80.9° | 51.2° | 0.222 | 0.267 | 0.239 | 0.201 |
| **75 horizontal at 4:3, vertical kept ("Hor+")** | **91.31°** | **59.8°** | **0.417** | **0.411** | **0.490** | **0.391** |
| 75 horizontal at 5:4, vertical kept | 95.0° | 63.1° | 0.306 | 0.326 | 0.405 | 0.322 |
| 75 vertical | 107.5° | 75.0° | 0.181 | 0.270 | 0.239 | 0.241 |

Fine sweep (horizontal at 16:9 in 0.5° steps from 89° to 93°, plus 87° and 95°): the best
value is 91.5° for noon (0.420), dusk (0.415) and lake (0.393) and 92.5° for hill_lod
(0.499, a flat top from 91.5° to 93° within 0.003; the view is mostly open road with few
vertical edges). Every camera scores within 0.01 of its best at 91.31°. The score falls
by 0.06–0.11 at 89° and by 0.03–0.07 at 93° (hill_lod's flat top aside), so the
measurement resolves the angle to about ±0.5°.

Visually (50/50 blends of the reference and the 91.31° render): the store signs, power
poles, crates and bushes at Goodsprings, the Prospector Saloon and road at the hill
camera, and the rocks and the tower across the lake overlap; at 75° the render is a 1.33×
zoom of the game's view.

## 3. Rule [hypothesis]

The FOV setting is the horizontal angle of a 4:3 view; at other aspect ratios the vertical
angle is kept and the horizontal angle grows with the width ("Hor+"):

- vertical = 2·atan(tan(setting / 2) · 3/4) (59.84° for 75);
- horizontal at aspect *a* = 2·atan(tan(vertical / 2) · *a*) (75° at 4:3, 85.3° at 16:10,
  91.3° at 16:9).

`nvforever` implements this (`vertical_fov` in `engine/nvforever/src/main.rs`; `--fov`
keeps meaning the game's setting). Bevy's perspective `fov` is the vertical angle, so a
resized window keeps the rule without further work.

Only 16:9 has been measured, so a rule that gives 91.3° at 16:9 by another route (for
example a fixed horizontal angle regardless of aspect) is not ruled out. Test: capture
one camera at 16:10 (e.g. 1280 × 800) or 4:3 (1024 × 768); the rule predicts 85.3° and
75° horizontal. The first-person weapon's separate FOV (`SetCameraFOV`'s second argument)
is not covered: the frames show no weapon.

## 4. Aiming down the sights

- While a gun is aimed down the sights the view zooms in to the weapon's Sight FOV
  (WEAP DNAM `sight_fov`, combat-data.md §1.1), the field of view down the sights; 0
  "essentially disables iron sights" [documented: GECK wiki Weapons]. The zoom is per
  weapon: the Varmint Rifle zooms in further than the Sturdy Caravan Shotgun [observed:
  the owner's playtest, 2026-10-16; reference shots private].
- The Sight FOV is read in the units of the FOV setting (§3: the horizontal angle at 4:3,
  the vertical angle kept) [hypothesis]. Test: a reference shot down a weapon's sights
  at 16:9, matched as in §2, should peak at the horizontal angle §3 gives for its Sight
  FOV.
- The zoom takes 0.25 s each way, eased at both ends [design: not captured]; both points
  of view zoom [hypothesis]. Scoped weapons (Has Scope) use the same angle for now; their
  scope overlay is not built.
- Built in `engine/nvf-play/src/aim_zoom.rs`; smoke scenario `aim-zoom`.
