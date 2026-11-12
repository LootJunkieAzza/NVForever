# DLSS upscaling

An optional temporal upscaler beside [FSR 3.1](fsr-upscaling.md): NVIDIA DLSS Super Resolution,
reached through NVIDIA Streamline 2.14.16 (issue #2198). The 3D view is drawn at a lower
resolution with a sub-pixel jitter, and DLSS rebuilds it at the window's resolution from this
frame and its history, as FSR does. NVForever ships no NVIDIA file: the player puts Streamline's
and DLSS's DLLs in a folder beside the program, and without them nothing of this note runs.
Off is the default. What Streamline expects and does, as the code relies on it:
[spec notes](../spec-notes/streamline.md).

This note is the contract for the work. A spike proved the route (the "What the spike showed"
section); the rest is built in the steps of the "Steps" section, and every part not yet built is
marked [design] with the step that builds it.

## Why Streamline, and why the player's files

- [documented] Bevy 0.20.1's own DLSS (`bevy_anti_alias`, feature `dlss`) uses `dlss_wgpu`
  6.0.0, whose `build.rs` links NVIDIA's NGX SDK libraries statically into the program; NVIDIA's
  RTX SDKs licence forbids using the SDK in a way that would put it under an open-source licence
  (§3e; NVIDIA/DLSS `LICENSE.txt`). [design] So it cannot sit inside the GPL program. Streamline
  is MIT-licensed headers over DLLs loaded at run time: NVForever translates the few declarations
  it calls (`engine/nvforever/src/dlss/ffi.rs`) and loads NVIDIA's DLLs from the player's folder.
- [design] Windows only. Streamline is published for Windows (its DLLs are Windows DLLs); other
  systems keep FSR 3.1.
- [design] Vulkan only. Streamline's Vulkan route replaces the Vulkan loader with its interposer
  ([spec notes](../spec-notes/streamline.md), "Loading"), which NVForever does when it makes the
  Vulkan instance (vendored `bevy_render`, `renderer/raw_vulkan_init.rs`,
  `RawVulkanInitSettings::set_vulkan_library`). DX12 keeps its native rendering, as for FSR.

## Decisions

Every point is [design]; the step that builds it is in brackets.

- **Files.** `<program folder>\dlss\` holds `sl.interposer.dll`, `sl.common.dll`, `sl.dlss.dll`
  and `nvngx_dlss.dll` (the files Streamline's guide lists for DLSS, see the spec notes). The
  folder is named after what the player turns on, not after Streamline. `NVF_DLSS_DIR=<absolute
  path>` points test runs at another folder. A missing file leaves the stock path with one log
  line naming the missing files. (Built in the spike.)
- **No updater from the folder.** A folder that also holds a file named `nvngx_update.exe` (any
  case) is refused before the interposer is loaded: Streamline would start that file, unchecked,
  in place of the driver's updater (spec notes, "NVIDIA's updater"). Any error while looking at
  the folder refuses it too. (Step 3.)
- **Loaded only when asked for at start.** Streamline is loaded, its signature checked and
  `slInit` called only when the start-up setting asks for DLSS (and, until step 8, only with the
  development switch `NVF_DLSS_SPIKE=1`). Without that, no DLL is looked for and the renderer
  starts exactly as without this feature. (Step 8 replaces the switch with the setting.)
- **A device check before the interposer.** Streamline's device proxy refuses a device that
  lacks the extensions and features DLSS and the interposer need, and wgpu then panics at device
  creation. Before taking the interposer route, NVForever asks Streamline for DLSS's
  requirements and checks every adapter Bevy might pick on a probe instance; if any candidate is
  unsafe or the choice cannot be told, it stays on the stock path and says why. (Step 3.)
- **Restart to enter or leave DLSS.** Choosing DLSS while the game runs without Streamline, or
  leaving DLSS while it runs with it, is saved at once but takes effect at the next start; until
  then the renderer keeps drawing what it drew, and the setting says a restart is needed.
  Switching between FSR and Off stays live, and so does changing DLSS's quality. (Steps 2, 8.)
- **No FSR state while DLSS runs.** DLSS gets the common preparation (render size, jitter, mip
  bias, depth, motion) but no FSR context, masks or history textures. (Step 4.)
- **Fallback is native, not FSR.** When DLSS cannot draw a frame (first frame, a missing input,
  a failed evaluate), the common spatial fallback draws it and the history is reset; a failure
  that persists marks DLSS unavailable for the session. The spike's fallback to FSR's dispatch
  goes. (Step 5.)
- **Frame generation is off with DLSS.** FSR's frame generation assumes FSR drew the frame; its
  row says it is unavailable with DLSS. (Spike: forced off while DLSS evaluates; step 8: the
  row.)
- **One Upscaler row.** The settings row keeps its key `fsr-upscaling` and its six saved values
  (Off, Native AA, Quality, Balanced, Performance, Ultra Performance, which stay FSR's) and gains
  five DLSS values, one per quality. Saved files from before keep working without a migration.
  The labels are NVForever's own words, chosen in step 8 (e.g. `FSR 3.1 — Quality`,
  `DLSS — Quality`); a DLSS entry that cannot run is dimmed with the reason (step 7). (Step 8.)
- **Command line.** `--upscaler off|fsr|dlss` and `--upscaler-quality <quality>` sit beside the
  legacy `--fsr <mode>`, which keeps its meaning; contradicting selectors are refused, and an
  omitted `--fsr` is told apart from `--fsr off`. Saved settings win in normal play, the command
  line wins in test runs, as for every setting. (Step 8.)
- **The quality mapping.** One quality list for both upscalers: Native AA → DLAA, Quality →
  Quality (`eMaxQuality`), Balanced → Balanced, Performance → Performance (`eMaxPerformance`),
  Ultra Performance → Ultra Performance. DLSS's Ultra Quality is not offered. DLSS's render size
  is the one DLSS reports for the mode and window, never FSR's ratio. (Step 5.)
- **NVIDIA's updater.** With NVIDIA's release DLLs, `slInit` starts the driver's NGX updater
  even with Streamline's over-the-air flags cleared, and nothing an application sets stops it
  (spec notes, "NVIDIA's updater"). NVForever clears both flags (no optional updates, no
  downloaded plugins) and the player documentation says in one sentence that turning DLSS on
  lets NVIDIA's own updater run in the background, as in other DLSS games. (Steps 3, 10.)

## Internal interfaces

`engine/nvforever/src/upscaler.rs` holds the types every step shares (step 2):

- `Upscaler`: `Off`, `Fsr`, `Dlss`, the backend kind (`--upscaler` in step 8).
- `UpscaleQuality`: the five qualities above, the one list both upscalers use.
  `fsr::FsrQuality` is an alias of it, and FSR's ratios stay in `fsr.rs`.
- `UpscalerChoice`: `Off`, `Fsr(quality)` or `Dlss(quality)`, the **requested** choice. It is
  the one authoritative setting, `Graphics::upscaler`; `Graphics::fsr()` and `set_fsr()` read
  and write it the way the old `Graphics.fsr` field was used, so FSR's call sites did not change
  meaning. There is no second toggle that could disagree with it.
- `Unavailable`: why an upscaler cannot run (not Windows, not Vulkan, files missing, an updater
  in the folder, the signature refused, Streamline refused, the device unsuitable, the GPU not
  supported, the evaluate failing, FSR unsupported), with a stable key for logs and tests.
- `DlssState`: whether DLSS can run in this session: not loaded (the start did not ask for it,
  so a restart might), ready, or unavailable with a reason.
- `UpscalerStatus` (a resource): what this session can do (Streamline loaded, `DlssState`, FSR's
  support) and what follows from the requested choice: the **effective** choice, whether a
  **restart is pending**, and the reason when the request cannot run. `upscaler::resolve` is the
  pure rule: at start, the request runs if its backend can, else native; afterwards, leaving or
  entering DLSS keeps the effective choice and sets restart pending, a DLSS quality change
  follows at once, FSR and Off follow at once, and a DLSS that stops working while it runs falls
  back to native. Steps 4, 5 and 8 feed the session facts and read the effective choice.

**The prepared-view seam** [design] (step 4): the FSR-independent part of `fsr.rs`'s view
preparation (camera history and cuts, the render-size override, jitter and mip bias applied to
the camera, the corrected render-size depth, the depth expanded to display size afterwards, the
resolution uniform, the spatial fallback and the alpha copy) becomes a common prepared view that
SSAO and the passes after the upscaler read. Each backend adds only its own part: FSR its
context, masks and history; DLSS its options, constants and evaluate. Backend-specific sizing,
jitter phase count and mip bias stay behind small functions, without a trait-object framework.
FSR's allocations, arithmetic, pass order, reset thresholds and log lines stay as they are.

## What the spike showed

All [observed] on the user's RTX 5070 Ti (driver 610.88), Streamline 2.14.16's release DLLs and
DLSS 310.9.16, through `tools/smoke` with `NVF_DLSS_SPIKE=1`, 2026-11-12:

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
  to FSR there, as designed.
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
- Eight FSR and frame-generation scenarios pass with the spike off.

## Open points

- **The secondary signature is not bound to the file** (`dlss/security.rs`). The check verifies
  the primary signature against the file and compares the secondary signer's public key with
  Streamline's NVIDIA key, as Streamline's header does; nothing shows that the secondary
  signature signs this file [hypothesis: a file validly signed by someone else carrying a copy
  of NVIDIA's nested signature could pass]. Windows' own check of that signature rejects the
  genuine file, its root being untrusted (`0x800b0109`) [observed]. Before a release: either
  verify the nested signature's digest against the file, or also allow-list the hashes of the
  official interposer. A separately reviewed change.
- **Context release on resize.** Streamline releases the old DLSS context three frames after a
  resize or mode change, not when the GPU has finished with it ([spec notes](../spec-notes/streamline.md),
  "Viewports and resize"). Step 5 drains the GPU and frees the viewport explicitly before
  reconfiguring; until then the three-frame delay is the only guard.
- **Hybrid-GPU limits.** The device check of step 3 refuses the interposer when any adapter Bevy
  might pick is unsafe or the pick cannot be established. Some laptops with two GPUs may then not
  get DLSS; that is the price of never panicking at device creation, and the reason is shown.
- **An evaluate that returns `eOk` is not proof NGX drew anything** (spec notes, "Evaluate").
  Step 6 reads back a small statistic of the output once per context and latches NGX's own error
  lines; a black sample is inconclusive, not a failure.
- A panic or `process::exit` skips the ordered shutdown.

## Steps

1. This note and the spec notes.
2. The shared types (`upscaler.rs`), `Graphics::upscaler` as the one requested setting, and the
   module renamed from the spike's name to `dlss/` (log lines start `DLSS:`).
3. The safe start: the folder refused with `nvngx_update.exe`, DLSS's requirements and every
   possible adapter checked before the interposer is chosen (`dlss/preflight.rs`).
4. The common prepared view out of `fsr.rs`; DLSS allocates no FSR state.
5. All five DLSS modes, sizes from DLSS at view preparation, explicit recreation on resize,
   native fallback.
6. Proof that DLSS wrote output (a read-back statistic, NGX's error lines).
7. Per-entry labels and dimmed entries with a reason in the settings widgets (`nvf-play`).
8. The Upscaler row, the early start-up resolution of the setting, `--upscaler`, frame
   generation gated on effective FSR; `NVF_DLSS_SPIKE` removed (`NVF_DLSS_DIR` kept).
9. Smoke scenarios: the no-files path in every run, hardware scenarios gated on `NVF_DLSS_DIR`.
10. Player documentation (install steps naming the tested files, the updater sentence), the
    status page, the dependency notes, and Streamline's MIT notice in the generated licences.
