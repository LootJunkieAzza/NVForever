# NVIDIA Streamline (DLSS Super Resolution through Vulkan)

An optional renderer feature of NVForever's own, not a claim about the original game. What
NVIDIA Streamline 2.14.16 expects of a host and does on its side, as far as
`engine/nvforever/src/dlss/` relies on it. Design, decisions and the spike's results:
[dlss-upscaling](../design/dlss-upscaling.md).

Sources: the public repository NVIDIA-RTX/Streamline at tag v2.14.16 (MIT). `include/…` and the
guides (`docs/ProgrammingGuide.md`, `docs/ProgrammingGuideDLSS.md`) are cited by their path;
`src/<file>` means the source file of that name in the repository's `source/` tree (the copy read
kept file names only). Line numbers are those files' at the tag. Everything is described in our
words; no NVIDIA text or code is reproduced here beyond names.

## Files

- [documented] A Vulkan title with DLSS Super Resolution needs `sl.interposer.dll`,
  `sl.common.dll`, `sl.dlss.dll` and NVIDIA's `nvngx_dlss.dll` (`ProgrammingGuide.md:1440-1445`).
- [documented] The Streamline DLLs are published only inside the v2.14.16 release archive
  (`streamline-sdk-v2.14.16.zip`), whose `bin/x64` also holds `nvngx_dlss.dll` 310.9.16; the
  same DLL is a plain file in NVIDIA/DLSS `lib/Windows_x86_64/rel/` (both checked 2026-11-12).
- [documented] Streamline looks for plugins in the absolute folders the host lists, the first
  with the highest priority (`include/sl_core_types.h:565-568`).
- [documented] JSON configuration files are read only by non-production builds
  (`src/hook.cpp:43`; `Debugging-JSON-Configs.md`); NVIDIA's signed release DLLs are production
  builds.

## Signature check

- [documented] Every module of the SDK's `bin/x64` carries two signatures: a standard Windows
  one, checked with `WinVerifyTrust`, and NVIDIA's own. The host should check
  `sl.interposer.dll` both ways, with the full path, before loading it; the interposer itself
  checks the plugins it loads (`ProgrammingGuide.md:96-104`).
- [documented] Streamline's header helper does this: it verifies the primary signature with
  `WinVerifyTrust`, then compares the public key of the nested (secondary) signature's signer
  with a key it carries (`include/sl_security.h:134-459`, the key at `:274-301`). It does not
  show that the nested signature signs the file. NVForever's translation:
  `engine/nvforever/src/dlss/security.rs`, whose comment holds the open point.

## Loading and initialisation

- [documented] On Vulkan, the host loads `sl.interposer.dll` in place of `vulkan-1.dll` and takes
  `vkGetInstanceProcAddr` from it (`ProgrammingGuide.md:32`). The interposer forwards what it
  does not intercept to the system's Vulkan loader (`src/wrapper.cpp:2365-2403`).
- [documented] `slInit` loads the requested plugins and must come before any Vulkan call through
  the interposer; features are loaded but not initialised until the device exists, so set, get
  and evaluate calls cannot be used right after it (`ProgrammingGuide.md:115-119`).
- [documented] The preferences are a versioned structure whose C++ constructor sets defaults; the
  default flags are "disable command-list state tracking", "allow over-the-air updates" and
  "load downloaded plugins" (`include/sl_core_types.h:580`). Streamline copies what the
  preference pointers point at during `slInit` (`src/pluginManager.cpp:92-164`).
- [documented] The log callback receives a type and a NUL-terminated message
  (`include/sl_core_types.h:506`), called on Streamline's own log thread with the message valid
  only during the call (`src/log.cpp:107-112, 386-391`).
- [documented] With no application id, Streamline uses a temporary one of its own
  (`src/pluginManager.cpp:160`, `src/internal.h:68`). With an engine version and a project id
  both given, it starts NGX with the project id; without them and without an application id,
  NGX stays off (`src/commonEntry.cpp:1494-1535`).
- [documented] The interposer raises the instance's requested API version to at least Vulkan
  1.3 (`src/wrapper.cpp:129-132`) and, at device creation, adds features (timeline semaphores,
  descriptor indexing, buffer device address) and device extensions (timeline semaphore, push
  descriptor) of its own besides those its plugins ask for (`src/wrapper.cpp:281-294`). A device
  lacking a required extension is refused (`src/wrapper.cpp:816-820` onwards), which wgpu-hal
  30.0.1 turns into a panic (wgpu-hal `src/vulkan/adapter.rs:3044-3050`).
- [documented] After `slInit`, `slGetFeatureRequirements` tells a feature's requirements without
  a device (`include/sl_core_api.h:187-190`): queue counts, instance and device extensions and
  Vulkan 1.2 and 1.3 feature names among them (`include/sl_core_types.h:642-690`).

## Support and functions

- [documented] `slIsFeatureSupported` checks the adapter only when the adapter information
  carries a LUID; without one it answers yes for any adapter (`src/sl.cpp:855-860`).
- [documented] DLSS's own functions (`slDLSSGetOptimalSettings`, `slDLSSSetOptions` and others)
  are fetched by name with `slGetFeatureFunction` (`include/sl_dlss.h:151, 173, 177-192`),
  which answers "not initialised" until the device exists (`src/sl.cpp:1099-1118`).
- [documented] The structures share a header of a 16-byte type GUID and a version, chained by a
  `next` pointer, and Streamline finds a structure by all of the GUID
  (`include/sl_struct.h:32-41, 108-115`). Zeroed memory does not reproduce the constructors'
  defaults, so NVForever's `ffi.rs` writes each default out and tests the layouts.

## Modes and sizes

- [documented] DLSS's modes, in order: off, max performance, balanced, max quality, ultra
  performance, ultra quality, DLAA (`include/sl_dlss.h:34-44`).
- [documented] `slDLSSGetOptimalSettings` gives, for a mode and output size, the render size to
  draw at, and minimum and maximum render sizes when dynamic resolution is supported
  (`ProgrammingGuideDLSS.md:103-126`).

## Evaluate

- [documented] DLSS needs depth, motion vectors, the render-size colour and the output colour
  tagged (`ProgrammingGuideDLSS.md:130`), all valid until the evaluate
  (`src/dlssEntry.cpp:200-201`). Tags passed with the evaluate itself are neither copied nor kept
  (`src/commonEntry.cpp:248, 319`).
- [documented] The render size DLSS uses is the colour input's extent
  (`src/dlssEntry.cpp:567-576`). A depth image needs its depth aspect given, else NGX gets the
  colour aspect (`src/dlssEntry.cpp:151-158`). On Vulkan a resource also carries its size,
  format, mips and layers, as Streamline cannot query an image (`src/vulkan.cpp:2779-2844`).
- [documented] The motion-vector scale is multiplied by the render size before it reaches NGX
  (`src/dlssEntry.cpp:684-685`), so vectors in UV units take a scale of about one, not the render
  size. DLSS's evaluate does not read the near and far planes (`src/dlssEntry.cpp:683-736`).
- [documented] Each frame takes a token from `slGetNewFrameToken`; Streamline keeps a small ring
  of frames in flight (`src/sl.cpp:1120-1146`). The viewport id is any value the host picks
  (`ProgrammingGuide.md:646-656`).
- [documented] Streamline moves the tagged images to the layouts NGX needs and back to the state
  each resource reported, inside a scope around the NGX call (`src/dlssEntry.cpp:666-681`,
  `:132-139`; `ProgrammingGuide.md:1026-1027`). It restores no pipeline or descriptor state on
  Vulkan (`src/commonInterface.cpp:442-453, 542-547`).
- [documented] DLSS's evaluate drops NGX's result: the call returns success to the host even
  when NGX's evaluate failed (`src/dlssEntry.cpp:738`), while the common plugin logs an NGX
  evaluate failure as an error (`src/commonEntry.cpp:772-794`). A success code alone does not
  show that DLSS wrote the output.

## Viewports and resize

- [documented] A change of mode, output size or preset recreates the DLSS context and resets the
  history; the old context is released three frames later, not when the GPU is done with it
  (`src/dlssEntry.cpp:346-376`).
- [documented] `slFreeResources` frees a viewport's resources; a command list with a pending
  evaluate must be submitted first (`include/sl_core_api.h:228-239`).
- [documented] The tagging API assumes one swap chain (`ProgrammingGuide.md:1416-1418`).

## Shutdown

- [documented] `slShutdown`'s cleanup uses the Vulkan device Streamline stored, so it must run
  while that device is alive (`src/vulkan.cpp:1349-1407`).

## NVIDIA's updater

- [documented] In production builds, `slInit` reads the cached server manifest and asks for an
  update check on every call; the "allow over-the-air updates" flag only decides whether optional
  updates are requested, and no flag skips the check (`src/pluginManager.cpp:961-969`).
- [documented] The check starts the driver's `nvngx_update.exe` on a detached thread and does not
  wait for it (`src/ota.cpp:240-246`), only on a machine with an NVIDIA GPU
  (`src/ota.cpp:849-857`).
- [documented] When a file named `nvngx_update.exe` sits beside `sl.interposer.dll`, that file is
  started in place of the driver's, with no signature check (`src/ota.cpp:194-247`, the folder
  test at `:214-221`).
- [documented] With "load downloaded plugins" cleared, no downloaded plugin is loaded, except
  when the driver's DLSS override setting is on for the program or globally, which loads a
  complete downloaded override set in place of the folder's plugins
  (`src/pluginManager.cpp:994-1049`).
- [observed] In the spike's runs (2026-11-12), NGX's own log lines show it looking for DLSS in
  NVIDIA's model cache (`C:\ProgramData\NVIDIA\NGX\models`) before loading the player's
  `nvngx_dlss.dll`, which it then used. [hypothesis] A DLSS in that cache for NVForever's ids
  would be used instead of the player's file; NVIDIA does not document the choice.

Sources, licences and NVForever's translation of the headers: `engine/nvforever/src/dlss/`
(ffi.rs, security.rs); the vendored change for the Vulkan library: `engine/vendor/README.md`.
