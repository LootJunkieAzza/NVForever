# DLSS upscaling

An optional temporal upscaler beside [FSR 3.1](fsr-upscaling.md): NVIDIA DLSS Super Resolution,
reached through NVIDIA Streamline 2.14.16 (issue #2198). The 3D view is drawn at a lower
resolution with a sub-pixel jitter, and DLSS rebuilds it at the window's resolution from this
frame and its history, as FSR does. NVForever ships no NVIDIA file: the player puts Streamline's
and DLSS's DLLs in a folder beside the program, and without them nothing of this note runs.
Off is the default. The player's page (install, choosing it, the greyed-out reasons):
[dlss.md](../dlss.md). What Streamline expects and does, as the code relies on it:
[spec notes](../spec-notes/streamline.md).

The work went in ten steps ("Steps", at the end); all ten are built. A spike proved the route
first, behind a development switch (`NVF_DLSS_SPIKE=1`) that step 8 replaced with the player's
setting. Code: `engine/nvforever/src/upscaler.rs` and `upscaler/prep.rs` (shared with FSR),
`engine/nvforever/src/dlss/` (DLSS's own).

## Why Streamline, and why the player's files

- [documented] Bevy 0.20.1's own DLSS (`bevy_anti_alias`, feature `dlss`) uses `dlss_wgpu`
  6.0.0, whose `build.rs` links NVIDIA's NGX SDK libraries statically into the program; NVIDIA's
  RTX SDKs licence forbids using the SDK in a way that would put it under an open-source licence
  (§3e; NVIDIA/DLSS `LICENSE.txt`). [design] So it cannot sit inside the GPL program. Streamline
  is MIT-licensed headers over DLLs loaded at run time: NVForever translates the few declarations
  it calls (`engine/nvforever/src/dlss/ffi.rs`, `security.rs`; the MIT notice beside them in
  `dlss/LICENSE-MIT`, carried into the releases' `THIRD-PARTY-LICENSES.txt` by
  `tools/release/third_party_licenses.py`) and loads NVIDIA's DLLs from the player's folder.
- [design] Windows only. Streamline is published for Windows (its DLLs are Windows DLLs); other
  systems keep FSR 3.1.
- [design] Vulkan only. Streamline's Vulkan route replaces the Vulkan loader with its interposer
  ([spec notes](../spec-notes/streamline.md), "Loading"), which NVForever does when it makes the
  Vulkan instance (vendored `bevy_render`, `renderer/raw_vulkan_init.rs`,
  `RawVulkanInitSettings::set_vulkan_library`). DX12 keeps its native rendering, as for FSR.

## How it works

Every point is [design]; the step that built it is in brackets. Each is described in more
detail in the module notes of the file named.

- **Files** (`dlss/mod.rs`). `<program folder>\dlss\` holds `sl.interposer.dll`,
  `sl.common.dll`, `sl.dlss.dll` and `nvngx_dlss.dll` (the files Streamline's guide lists for
  DLSS, see the spec notes). The folder is named after what the player turns on, not after
  Streamline. `NVF_DLSS_DIR=<absolute path>` points test runs at another folder. A missing
  folder or file leaves the stock path with one log line naming the missing files. (Spike.)
- **No updater from the folder** (`dlss/preflight.rs`). A folder that also holds an entry named
  `nvngx_update.exe` (any case) is refused before the interposer is loaded, and the folder is
  listed again just before `slInit`: Streamline would start that file, unchecked, in place of
  the driver's updater (spec notes, "NVIDIA's updater"). A folder that cannot be listed is
  refused too. (Step 3.)
- **The signature** (`dlss/security.rs`). The interposer is checked as Streamline's header
  helper does before it is loaded by its full path; the limit of that check is the first open
  point below. (Spike.)
- **Loaded whenever the files are there** (`main.rs`, `upscaler.rs` `UpscalerStartup`,
  `dlss::bundle_present`) [design: the user's decision after the play-test of #2206,
  2026-11-12]. `main` works out the requested choice before the renderer exists, from the
  command line (`upscaler::command_line_choice`) and then the INI by the settings' own
  precedence (`settings::upscaler_after_ini`), because Streamline must be loaded before the
  Vulkan instance is made. `dlss::DlssInitPlugin` is added for a DLSS choice, or, on Windows
  with Vulkan among the renderer's backends, when the four files are in the DLSS folder (a
  cheap existence check; the loader's own checks, the updater, the signature and the device
  check, still decide), so that every later switch is live. The test aid `NVF_DLSS_PRELOAD=1`
  adds it always, `=0` only for a DLSS choice (the earlier behaviour, for tests). Without the
  files and without a DLSS choice, no DLL is loaded and the renderer starts exactly as
  without this feature. The start's log line says which case it was (`StartLoad`). The same
  INI lines are applied again with the other rows at `PostStartup`, by the rows' same rules,
  so the two readings agree. (Step 8; the play-test follow-up.)
- **A device check before the interposer** (`dlss/preflight.rs`). Streamline's device proxy
  refuses a device that lacks an extension it asks for, and wgpu then panics at device
  creation; features it asks for are masked to what the device supports rather than refused
  (spec notes, "Loading and initialisation"). Before taking the interposer route, NVForever
  reads every GPU through the system's Vulkan loader on a probe instance (before any Streamline
  code is loaded), then, after `slInit`, checks every GPU Vulkan exposes, of any kind, against
  `slGetFeatureRequirements(DLSS)` plus what Streamline's own device creation always adds. No
  GPU is left out for the power preference, `WGPU_ADAPTER_NAME` or the adapter pin, because
  Bevy can still fall back to any of them (the next item). The features are required here on
  purpose, although the proxy would only mask them: DLSS needs them, and `privateData` too. If
  any GPU falls short, or Vulkan shows none, `slShutdown` runs and the stock path follows,
  before `set_vulkan_library`. (Step 3; every GPU since the final review.)
- **The adapter pin** (`preflight::pin`, `dlss::render_plugin`). When `WGPU_ADAPTER_NAME` is
  unset and exactly one checked GPU is NVIDIA's, its name picks it alone and it has a LUID,
  Bevy is asked for it by name (`WgpuSettings::adapter_name`). This is a preference only: Bevy
  passes over a named adapter that does not support the window's surface, or finds none of that
  name, and then takes wgpu's own pick [documented: vendor/bevy_render/src/renderer/mod.rs:
  166-194, 304]. So the pin cannot keep the renderer off a GPU the check did not judge; the
  check of every GPU Vulkan exposes does that, and the pin only prefers the NVIDIA GPU among
  checked ones. Once the device exists, the GPU Bevy chose is compared with the checked one by
  name and LUID and logged; a mismatch keeps DLSS off for the session. (Step 5, from the
  midpoint review; its limit, the second open point.)
- **Support** (`dlss/win.rs`). Once the device exists, `slIsFeatureSupported` with the
  physical device's LUID, the versions Streamline reports, and DLSS's two functions fetched.
  (Spike.)
- **Switching** (`upscaler::resolve`, `follow_request`). With Streamline loaded at start, Off,
  FSR 3.1 and DLSS all switch at once, every quality too; FSR then draws through the
  interposer (smoke `dlss-preload-fsr`, `dlss-live-switch`). A restart is pending only for DLSS
  asked for in a session that started without Streamline while the files are in the folder
  now (put there since the start, or kept out by `NVF_DLSS_PRELOAD=0`): the request is saved,
  what drew keeps drawing, and the Upscaler row reads "DLSS (restart)", its note saying why.
  With the files still missing, DLSS is dimmed as "files missing" instead (the status looks
  at the folder at most once a second). Once DLSS stopped working (`EvaluateFailed`, below), a
  DLSS request draws natively with the reason for the rest of the session; FSR and Off still
  follow at once. (Steps 2, 8; the play-test follow-up replaced the earlier rule that entering
  or leaving DLSS took a restart.)
- **Native fallback, not FSR** (`dlss/mod.rs`, `dlss_pass`). When DLSS cannot draw a frame
  (the first frame, a missing input, no options for the frame's mode and size, a render size
  outside the range DLSS reports, a failed call), the common spatial fallback draws it and the
  history is invalidated. 60 such frames in a row (`FAILED_FRAMES`) mark DLSS unavailable for
  the session (`EvaluateFailed`): the status then draws the camera natively, with the reason on
  the DLSS entries. When DLSS reports no usable render size for a mode, the view renders
  natively. The spike's fallback to FSR's dispatch is gone. (Step 5; the status, step 8.)
- **No FSR state while DLSS runs** (`upscaler/prep.rs`, `dlss/mod.rs`). DLSS gets the common
  preparation (render size, jitter, mip bias, depth, motion) but no FSR context, masks or
  history textures, and draws in its own pass. Step 4 split the common preparation out of
  `fsr.rs`; step 5 made DLSS independent of it (the spike had run inside FSR's pass, after
  FSR's state was made). (Steps 4, 5.)
- **The quality mapping** (`dlss/sizing.rs`). One quality list for both upscalers: Native AA →
  DLAA, Quality → Quality (`eMaxQuality`), Balanced → Balanced, Performance → Performance
  (`eMaxPerformance`), Ultra Performance → Ultra Performance. DLSS's Ultra Quality is not
  offered. DLSS's render size is the one `slDLSSGetOptimalSettings` reports for the mode,
  output size and HDR-ness, asked during the view preparation once per change and validated
  (never zero, never larger than the output), never FSR's ratio. DLSS has its own jitter phase
  count and mip bias (spec notes, "Modes and sizes"); FSR's `NVF_FSR_MIP_BIAS_OFFSET` does not
  apply. (Step 5.)
- **Recreation on resize or mode change** (`dlss::configure`). When the mode, output size or
  HDR-ness changes after DLSS's options were set, a render system before anything of the frame
  is recorded drains the GPU, frees the viewport (`slFreeResources`) and sets the options
  again; the history resets. A drain that fails leaves the options unset, and the spatial
  fallback draws. (Step 5.)
- **Proof that DLSS drew.** `eOk` from the evaluate is not proof that NGX wrote anything (spec
  notes, "Evaluate"), so once per DLSS setup NVForever checks that DLSS wrote the picture
  (step 6, `dlss/evaluate.rs` `OutputProof`) [design]. A setup (a generation) begins with each
  `slDLSSSetOptions` that sets new options: the start, a mode change, a resize. Right after the
  evaluate, before the alpha pass or any effect touches the output, a small compute pass reads a
  16x16 grid of the output's texels into a buffer that is copied out and mapped without the frame
  waiting for it. Since the output was cleared to black just before the evaluate, finite RGB that
  is not all black can only be DLSS's: `output observed`. All black is inconclusive (a black scene
  looks the same) and is tried again, 30 evaluated frames apart, ten times, then said as `output
  not observed` without counting DLSS as failed; NaN or infinity is said as `output not valid`.
  Streamline's log lines that report an NGX evaluate error are latched without taking the
  evaluator's lock and each counts as one frame without output for the 60-frame rule above. This
  proves NGX wrote the picture, not that it is a good picture; that stays with screenshots and
  the smoke scenarios.
- **Two rows: Upscaler and Upscaling quality** (`settings.rs`) [design: the user's, after the
  play-test of #2206, where the one dropdown of eleven entries was too long for its box].
  **Upscaler** (key `upscaler`: `Off`, `FSR 3.1`, `DLSS`) and, below it, **Upscaling quality**
  (key `upscaler-quality`: Native AA, Quality, Balanced, Performance, Ultra Performance) both
  keep the one request, `Graphics::upscaler`. The quality row is dimmed with "Needs an
  upscaler" while Upscaler is Off, and keeps its quality meanwhile (`KeptQuality`), so the next
  upscaler takes it up. DLSS that cannot run is dimmed with its reason (`Unavailable::words`),
  using the per-choice labels and disabled entries of step 7 ([settings-page](settings-page.md));
  a stored DLSS choice still loads, so the request survives a start in which it cannot run.
  FSR is never dimmed. The earlier single row, `fsr-upscaling` with its eleven values
  (`graphics::FSR_UPSCALING_CHOICES`: Off, the five FSR qualities, `DLSS Native AA` …
  `DLSS Ultra Performance`) and the early slider build's numbers, still loads from an INI that
  lacks `upscaler` (`set_older_key`), into both rows; the test aid's `fsr-upscaling=<value>`
  still sets both rows and logs `settings: fsr-upscaling <value>` (`set_list_alias`), so the
  scenarios written for it keep working. Both rows' notes name `--upscaler`,
  `--upscaler-quality` and `--fsr`, so a test run that gives any of them keeps both at the
  command line's choice. (Steps 7, 8; the play-test follow-up.)
- **The anti-aliasing row** reads "Off (upscaling active)" while FSR or DLSS draws
  (`upscaler::temporal_active`) and the saved choice comes back when Upscaler is Off. DLSS
  sharpens nothing itself: the Sharpening switch drives the separate sharpening pass with DLSS,
  as it does natively. (Step 8.)
- **Frame generation is off with DLSS** (`frame_gen::usable`). FSR's frame generation assumes
  FSR drew the frame; it runs only while FSR is the effective upscaler, and its row says
  "Not with DLSS" while DLSS is ([frame-generation](frame-generation.md)). (Step 8.)
- **Command line** (`main.rs`, `upscaler::command_line_choice`). `--upscaler off|fsr|dlss`
  and `--upscaler-quality <quality>` sit beside the legacy `--fsr <mode>`, which keeps its
  meaning; contradicting selectors are refused (`--fsr quality --upscaler dlss`, also
  `--fsr off --upscaler dlss`), as is a quality without `--upscaler fsr|dlss`, and an omitted
  `--fsr` is told apart from `--fsr off`. Saved settings win in normal play, the command line
  wins in test runs, as for every setting. (Step 8.)
- **NVIDIA's updater.** With NVIDIA's release DLLs, `slInit` starts the driver's NGX updater
  even with Streamline's over-the-air flags cleared, and nothing an application sets stops it
  (spec notes, "NVIDIA's updater"). NVForever clears both flags (no optional updates, no
  downloaded plugins) and the player's page says that whenever the files are in the dlss
  folder NVIDIA's driver may update DLSS in the background, as in other DLSS games, whichever
  upscaler is chosen (Streamline is loaded whenever the files are there), and that NVForever
  cannot switch that off. (Steps 3, 10; the play-test follow-up.)

### Combinations not supported

[design], each with what happens instead:

- DLSS off Windows, or with `WGPU_BACKEND` leaving Vulkan out (DX12): the DLSS entries are
  dimmed ("DLSS needs Windows", "DLSS needs the Vulkan renderer"); nothing is loaded.
- DLSS with FSR's frame generation: the frame-generation row is dimmed.
- DLSS on more than one camera: one Streamline viewport, so the first DLSS camera only; the
  tagging API assumes one swap chain (spec notes, "Viewports and resize").
- DLSS's Ultra Quality mode, its dynamic resolution and its own sharpness value: not used.
- DLSS Frame Generation and Reflex through Streamline: not built (later issues).

## Internal interfaces

`engine/nvforever/src/upscaler.rs` holds the types every step shares:

- `Upscaler`: `Off`, `Fsr`, `Dlss`, the backend kind (`--upscaler`).
- `UpscaleQuality`: the five qualities above, the one list both upscalers use.
  `fsr::FsrQuality` is an alias of it, and FSR's ratios stay in `fsr.rs`.
- `UpscalerChoice`: `Off`, `Fsr(quality)` or `Dlss(quality)`, the **requested** choice. It is
  the one authoritative setting, `Graphics::upscaler`; `Graphics::fsr()` and `set_fsr()` read
  and write it the way the old `Graphics.fsr` field was used, so FSR's call sites did not change
  meaning. There is no second toggle that could disagree with it.
- `Unavailable`: why an upscaler cannot run (not Windows, not Vulkan, files missing, an updater
  in the folder, the signature refused, Streamline refused, the device unsuitable, the GPU not
  supported, the evaluate failing, FSR unsupported), with a stable key for logs and tests and
  the settings' words (`words`).
- `DlssState`: whether DLSS can run in this session: not loaded (Streamline was not loaded at
  start and the files are in the folder now, so a restart brings it), ready, or unavailable
  with a reason. `dlss_state_now` reads it from the loader (`dlss::session_state`, which also
  reports `EvaluateFailed` once DLSS's pass gave up), else from what is known without loading
  anything (the platform, the files in the folder now).
- `Session`: what this session can do: whether the device came through Streamline
  (`dlss::streamline_loaded`, true also when DLSS turned out unsupported or stopped working),
  `DlssState`, FSR's support.
- `UpscalerStartup` (a resource): the requested choice the start resolved, and whether and why
  the loader runs (`StartLoad`: DLSS chosen, the files present, the preload aid on or off, no
  files, an unsupported platform).
- `UpscalerStatus` (a resource): the session and the last `Resolution`: the
  **effective** choice, whether a **restart is pending**, and the reason when the request
  cannot run. `upscaler::resolve` is the pure rule (the restart point above).
  `upscaler::follow_request` makes the status on the first frame and refreshes it every frame
  before `graphics::apply` puts the effective choice on the camera. `dlss_unavailable` is the
  reason the DLSS entries are dimmed, by the same rule.
- `upscaler::effective` and `temporal_active`: what the shared parts ask (HDR, prepasses,
  cut-outs drawn alpha-tested, the anti-aliasing lock, the ambient occlusion at the render
  size); `graphics::fsr_active` stays FSR's own question (its sharpening, frame generation).
  Both go by the effective choice.
- `DlssUpscaling` (a component): DLSS on a camera, the counterpart of `fsr::FsrUpscaling`; the
  graphics settings put one or the other on the main camera, never both.

**The prepared-view seam** (`upscaler/prep.rs`, step 4): the FSR-independent part of the view
preparation (camera history and cuts, the render-size override, jitter and mip bias applied to
the camera, the corrected render-size depth, the depth expanded to display size afterwards, the
resolution uniform, the spatial fallback and the alpha copy) is a common prepared view
(`PreparedUpscale`) that SSAO and the passes after the upscaler read. Each backend adds only its
own part: FSR its context, masks and history (`fsr::PreparedFsr`); DLSS its options, constants
and evaluate. The backend's sizing, jitter phase count and mip bias are one function
(`prep::policy`; DLSS's in `dlss/sizing.rs`), without a trait-object framework. FSR's
allocations, arithmetic, pass order, reset thresholds and log lines stayed as they were; a
change of backend is one more history cut. DLSS's pass (`dlss::dlss_pass`) sits where FSR's
does: after the ambient occlusion, before bloom, the bright pass and the other full-resolution
effects.

## What the runs showed

All [observed] on the user's RTX 5070 Ti (driver 610.88), Streamline 2.14.16's release DLLs and
DLSS 310.9.16, through `tools/smoke`, 2026-11-12. The spike (DLSS fixed at Quality, drawing
inside FSR's pass, behind the development switch):

- The entry route works: the interposer's signature check passed, `slInit` returned `eOk`, the
  Vulkan instance and device were made through Streamline's proxies, `slIsFeatureSupported`
  said DLSS is supported (Streamline 2.14.16, DLSS 310.9.16), and the four files were enough.
  The made-up project id with no NVIDIA application id was accepted. The shutdown drained the
  GPU, freed DLSS's resources and called `slShutdown` before the device was destroyed.
- One DLSS evaluate per frame drew the picture: the output is cleared just before the evaluate
  and FSR's dispatch skipped, so a non-black frame is DLSS's. Screenshots matched FSR's (mean
  difference about 2.5/255) without ghosting at rest, turning, walking or reloading. NGX made
  the context at 853x480 → 1280x720 (preset K); 1489 evaluates succeeded and none failed in
  18 s. A resize to 1600x900 recreated the context. Ultra Performance's 533x300 is below the
  800x450 minimum DLSS reports for its Quality mode, so the spike (fixed at Quality) fell back
  to FSR there, the reason step 5 asks DLSS for each mode's own size.
- With the Khronos validation layer, three errors inside NGX's evaluate were found and fixed
  (a clear without the transfer usage, write-after-write hazards around that clear, the
  `privateData` feature not enabled); after that the spike reported only what the same run
  reports without it. Over 10 minutes, memory stayed flat: DLSS 1.5-1.6 GB of video memory and
  2.6 GB of RAM, FSR 1.4 GB and 2.5 GB; 54,973 evaluates, none failed.
- The negative starts kept today's path: no switch, an empty folder, a missing `sl.dlss.dll`,
  and `WGPU_BACKEND=dx12` (Streamline not loaded). The DX12 run itself fails the same way
  without the spike (pipeline validation errors on main).
- NVIDIA's `nvngx_update.exe` was running at the moment of `slInit`, with both over-the-air
  flags cleared. Streamline logged that downloaded plugins will not be loaded.
- Streamline logs an error for a missing `NvLowLatencyVk.dll` (DLSS works without it) and warns
  that three `sl.common` Vulkan hooks are "NOT supported" on this route; neither stopped DLSS.

After steps 3, 4 and 7: the FSR, frame-generation and SSAO scenarios (19) and the DLSS runs
passed, and a folder holding `nvngx_update.exe` was refused. The two runs meant to decline the
interposer for an unsuitable second GPU cannot decline on that PC, whose Vulkan loader sees only
the RTX: the hybrid cases are covered by unit tests only. Runs of steps 5, 6 and 8 (the five
modes, resize, the switching rule, the output check) are recorded with the pull request as they
are made; the smoke scenarios of step 9 run the no-files path everywhere and the hardware cases
only where `NVF_DLSS_DIR` names a bundle.

## Open points

- **The secondary signature is not bound to the file** (`dlss/security.rs`). The check verifies
  the primary signature against the file and compares the secondary signer's public key with
  Streamline's NVIDIA key, as Streamline's header does; nothing shows that the secondary
  signature signs this file [hypothesis: a file validly signed by someone else carrying a copy
  of NVIDIA's nested signature could pass]. Windows' own check of that signature rejects the
  genuine file, its root being untrusted (`0x800b0109`) [observed]. Before a release: either
  verify the nested signature's digest against the file, or also allow-list the hashes of the
  official interposer. A separately reviewed change.
- **Hybrid GPUs.** The device check refuses the interposer when any GPU Vulkan exposes falls
  short, so laptops whose built-in graphics also show up in Vulkan beside the NVIDIA card
  usually get no DLSS [design]. That is the price of never panicking at device creation, and
  the reason is shown. A narrower check cannot be trusted: Bevy's adapter name (the pin, or
  `WGPU_ADAPTER_NAME`) is only a preference, skipped when the named adapter does not support
  the window's surface, after which Bevy takes wgpu's own pick
  (`vendor/bevy_render/src/renderer/mod.rs:166-194`, `:304`), and an unchecked GPU without
  Streamline's device extensions would then make wgpu-hal panic at device creation, before the
  identity check after it could act. Letting such laptops use DLSS needs the checked identity
  enforced before the device is made, with a way back to the stock loader. The pin stays set
  when Streamline later falls back to the stock path.
- **Context release on resize.** Streamline itself releases a replaced DLSS context three
  frames after a resize or mode change, not when the GPU has finished with it
  ([spec notes](../spec-notes/streamline.md), "Viewports and resize"). Step 5 drains the GPU
  and frees the viewport explicitly before setting the new options, which releases the old
  context at once and leaves Streamline's delayed release nothing to do [documented:
  `src/dlssEntry.cpp:363-373`, `:884-902`]. Not yet observed in a run of step 5.
- **NVIDIA's model cache.** NGX looked for DLSS in the driver's model cache before loading the
  player's `nvngx_dlss.dll` [observed in the spike]; a DLSS there may be used instead of the
  player's file [hypothesis] (spec notes, "NVIDIA's updater").
- A panic or `process::exit` skips the ordered shutdown.

## Steps

All built.

1. This note and the spec notes.
2. The shared types (`upscaler.rs`), `Graphics::upscaler` as the one requested setting, and the
   module renamed from the spike's name to `dlss/` (log lines start `DLSS:`).
3. The safe start: the folder refused with `nvngx_update.exe`, DLSS's requirements and every
   possible adapter checked before the interposer is chosen (`dlss/preflight.rs`).
4. The common prepared view out of `fsr.rs` (`upscaler/prep.rs`).
5. DLSS's own pass with no FSR state, all five DLSS modes, sizes from DLSS at view preparation,
   explicit recreation on resize, native fallback, the adapter pin.
6. The check that DLSS wrote the picture.
7. Per-entry labels and dimmed entries with a reason in the settings widgets (`nvf-play`).
8. The Upscaling row, the early start-up resolution of the setting, `--upscaler`, frame
   generation gated on effective FSR; `NVF_DLSS_SPIKE` removed (`NVF_DLSS_DIR` kept,
   `NVF_DLSS_PRELOAD` added). After the play-test of #2206: Streamline loaded whenever the
   files are there, so every switch is live, and the one row split into Upscaler and Upscaling
   quality (the older key still read and taken by the test aid).
9. Smoke scenarios: the no-files path in every run, hardware scenarios gated on `NVF_DLSS_DIR`.
10. The player's page ([dlss.md](../dlss.md)), the status page, the dependency notes, and
    Streamline's MIT notice in the generated licences.
