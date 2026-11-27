# DLSS 5 Neural Rendering before the upscaler

Status: prototype in the `dlss5-nr` fork branch; proposed upstream in the issue that
links this note. Engine-side placement is [design]; the network's transforms are
[documented] from public sources; timings are [observed] on one machine.

## What runs where

The network is NVIDIA's DLSS 5 Neural Rendering (generative neural rendering, announced
GTC 2026, shipped Sept 2026) [documented: NVIDIA research report "DLSS 5: Generative
Neural Rendering", https://research.nvidia.com/labs/adlr/DLSS5/]. It is not an upscaler:
input and output share a resolution; it re-renders lighting and material detail from a
display proxy of the frame, injected noise, a reprojected previous output and five
conditioning scalars [documented: OpenDLSS-NR docs/network.md and docs/frame.md, MIT].

The pass runs at the DLSS render resolution, in `Core3dSystems::EarlyPostProcess` after
the ambient occlusion and before `dlss::dlss_pass`'s evaluate, so DLSS SR upscales the
neural-rendered frame and DLSS-G (which consumes presented frames) sees it too. The HUD
is Bevy UI drawn after the upscaler, so it is untouched by placement [design].

Inputs, all already produced for DLSS [observed: engine/nvforever/src/dlss/evaluate.rs
tags and upscaler/prep.rs]:
- main texture render rect, `Rgba16Float`, linear HDR, pre-tonemap;
- prepass motion vectors, `Rg16Float`, screen-space UV, current-minus-previous,
  unjittered, camera motion included;
- corrected depth (`PreparedUpscale::depth`, render-size `Depth32Float`);
- jitter offset and the history-reset flag.

## The transforms

Proxy encode [documented: OpenDLSS-NR docs/frame.md "The proxy"]: divide by paper white,
soft shoulder above 0.75 (`0.75 + 0.25 * (1 - exp(-5.770780 * (v - 0.75)))`), sRGB OETF,
round to the half grid. The composite inverts it [documented: same, "Composition"]:
`neural = clamp(proxy + rgb/4)`, history weight `clamp(sigmoid(logit) * blend_scale)`,
lerp with the reprojected history where valid, publish history truncated toward zero to
the half grid. `blend_scale` is a learned f16 in the model's post block [documented:
OpenDLSS-NR docs/frame.md; observed: 0.7397 full / 0.7671 small in build 310.9.16].

Conditioning scalars (lanes 10-14): style (0/1/2), localTone, localStructure,
skinStructure (-1 = follow), autoMask [documented: OpenDLSS-NR docs/network.md;
neural-forge docs/DLSSNR_PARAMETERS.md read as documentation only, AGPL, not copied].

History reprojection (next step in the fork): previous position = current + MV*scale +
(jitter[t-1] - jitter[t]); five-tap Catmull-Rom sample of the previous output; reset on
scene cut / exposure jump [documented: OpenDLSS-NR docs/frame.md; neural-forge
preupscale design read as documentation].

## How it is recorded

Same contract as the Streamline evaluate [observed: dlss/evaluate.rs]: a dedicated
`wgpu::CommandEncoder` touched only through `as_hal_mut::<Vulkan>`, full memory barriers
around the foreign work, `RenderContext::add_command_buffer`, textures/buffers kept
alive via `on_submitted_work_done`. The network DLL creates its own pipelines on the
adopted device and records into that command buffer; device features are merged through
`RawVulkanInitSettings::add_create_device_callback` (never turned off; duplicate sTypes
avoided — bevy's Functionality priority already chains the cooperative-matrix features
[observed: VK_ERROR_FEATURE_NOT_PRESENT when duplicated, 2026-11-24]).

RivaTuner's and ReShade's implicit Vulkan layers crash `vkCreateDevice` with the
network's feature structures [observed: crash dumps 2026-11-24, RTSSVkLayer64.dll in the
stack]; the process disables those two layers unless the player set
`VK_LOADER_LAYERS_DISABLE` themselves [design].

## Weights and licensing

The network implementation is MIT (OpenDLSS-NR); no NVIDIA code is linked or shipped.
The weights are NVIDIA's IP, read from the resource section (`WEIGHTS_HT`,
`WEIGHTS_CC_SMALL_FP8`) of the player's own nvngx_dlssnr.dll — plain records, no code
execution, no decryption [documented: neural-forge README "Legal", read as
documentation]. No official public download of the weights exists today; the acquisition
path is an open question for the maintainer (see the issue).

## Measurements

[observed: RTX 5090, driver 617.42, Windows 11, 2026-11-24] whole-network GPU time,
single-frame mode: 1114x627 ~1.6 ms (31-block) / ~2.6-3.2 ms (71-block); 1920x1080
~4.3 ms (71-block); in-game at 4K with DLSS Balanced the pass runs at 2227x1253.
Boiling in dark areas is expected in single-frame mode (noise re-drawn per frame);
the temporal history path is the fix [hypothesis, to be measured].
