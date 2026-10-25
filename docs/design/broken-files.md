# Broken and missing files

Owner's rule (2026-10-24): nothing in the engine may crash, hang or refuse to start
because a file is missing or broken, as happens with badly made or half-installed mods.
Every bad file is skipped (or drawn as a placeholder), logged once, and the game goes on.

This note says how, per kind of file, and what is left. The audit behind it (2026-10-24)
read every path from a file's bytes to the screen, the speakers and the physics.

## 1. The three ways a bad file can stop a Rust game

1. **A panic** (an index out of range, an `unwrap`, a `clamp` with a NaN bound). Caught:
   every step that turns a file into something used runs under
   `nvf_assets::guard::guarded`, which turns a panic into an error for that one file.
   The crash hook (`nvforever::crash`) writes no crash report for a panic caught there; it
   logs `a broken file panicked while loading at ...; that file is skipped`.
2. **Something no catch can stop**: an endless loop, an exponential walk (a list that
   contains itself), a stack overflow, an allocation of gigabytes (which ends the
   process). These are stopped at the source with limits: counts checked against the
   bytes left, visited sets, budgets.
3. **Bevy closing the game**: Bevy's default answer to a GPU error is to quit. Every image
   made from a file's numbers goes through `nvf_assets::texture::checked_image` (sides 1
   to 16384, no more mip levels than the size allows, data of exactly the right length),
   and the game's own `RenderErrorHandler` (`crash.rs`) lets the first 20 validation
   errors through with a warning. One that keeps coming back still stops the game,
   rather than flash the screen every frame.

NaN is the fourth hazard: a garbled number that reaches the bones, the camera or the
physics engine. Animation clips with non-finite keys are refused (`Clip::check`), and
physics conversion refuses non-finite poses.

## 2. Per kind of file

| File | Missing | Broken |
|---|---|---|
| Model (`.nif`) of a placed object | drawn as the game's error marker, `meshes\marker_error.nif` (a red sign with an exclamation mark) [observed: the owner's screenshot of the original with a mod's model missing] | the same |
| Other models (actor parts, effects, sky, grass, ...) | not drawn | not drawn |
| Texture (`.dds`) | the shape is drawn without it | the same |
| Animation (`.kf`) | that clip is not played | the same; also when a key is not a finite number |
| Collision (in the `.nif`) | no collision for that object | the same |
| Voice line | silent; the subtitle stays for the text's fitted length (1.1 s + 1/16 s per character); the line, conversation and quest go on | silent, the same length; a length in the file's header past 20 minutes is ignored |
| Holotape response | that response is skipped | silent for its length |
| Sound effect, music, radio, ambient loop | silent | silent; a file that breaks while playing stops |
| Lip-sync (`.lip`) | the mouth does not move | the same |
| FaceGen (`.egm`, `.egt`) | the face is not morphed | the same |
| Save thumbnail | no picture | the same |
| Intro or scripted film (`.bik`) | not played | not played |

The failures are logged as they happen: `model not loaded: <path>: <why>`,
`texture not loaded: ...`, `actor file not loaded: ...`, `sound not loaded: ...`.

## 3. Testing it

Test aid `NVF_PLAY_TEST_BROKEN_FILES="missing:<part>;corrupt:<part>;panic:<part>"`
(`nvf_assets::data`): archive paths containing `<part>` (any case, `/` or `\`) read as
missing, garbled after their first 64 bytes, or panicking. Nothing changes on disk. The
smoke scenario `broken-files` uses it in the Bison Steve lobby.

## 4. Plugins, archives and other data

- A plugin whose master is missing, or loads after it, is left out with a warning, and
  so is every plugin that needs it (`fnv_esm::loadorder::drop_unloadable`; the strict
  `LoadOrder::new` stays for the tools). So is a plugin that cannot be read. [design:
  the original refuses to start in that case; the owner's rule is that the game goes on.]
- An archive (`.bsa`) that cannot be opened, and a folder of `Data` that cannot be
  read, are left out with a warning (`Vfs::skipped`).
- Lists that name themselves (leveled lists, head parts, collision list shapes) are
  walked once per entry (visited sets); counts and sizes from files are checked against
  the bytes there are before anything is allocated (NIF arrays, saves, FaceGen, lip
  files); particle emitters, SpeedTree generation and leveled "each item" rolls have
  budgets.
- A worldspace with no climate at the start takes WastelandNV's sky (travel already kept
  the last one).
- Physics takes no non-finite pose, ragdoll joint, damping or hit capsule; a NaN bone
  is refused with its skeleton.

## 5. Left

- Nested "each item in count" leveled lists are limited per level (10,000 rolls), not
  in total.
- 256 nested script runs fit a 2 MiB worker stack [observed: measured, 2026-10-24]; the
  main thread's stack (1 MiB on Windows) was not measured for other nestings.
- A decoder panic deep inside a playing sound ends that sound (`GuardedSource`); a
  failure inside the audio device thread itself is not covered.
