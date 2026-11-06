# Latency reduction (AMD Anti-Lag)

An optional switch: the GPU driver keeps the game from preparing frames ahead of the graphics
card, so input reaches the screen sooner. It is the first half of #1778; FSR 3.1 frame
generation, which adds latency that this offsets, comes later. Code: `engine/nvforever/src/anti_lag.rs`.

## Using it

**Settings > Graphics > Latency reduction**, or `--latency-reduction`. Off by default, saved like
every switch. It works on AMD graphics cards whose Vulkan driver has Anti-Lag:

- Windows: Adrenalin 24.9.1 or later [documented: GPUOpen, Anti-Lag 2 SDK requirements].
- Linux: Mesa RADV 26.3 or later; Mesa 25.3 to 26.2 had it as an implicit layer [documented:
  Mesa release notes, via Phoronix].

On other GPUs the row is dimmed ("Not on this GPU") and the start says so once:
`latency reduction: not available on <adapter>`.

## How it works

- **The extension.** `VK_AMD_anti_lag` (Vulkan registry extension 477) adds one call,
  `vkAntiLagUpdateAMD`, and one feature, `antiLag` [documented: `vk.xml`]. NVForever ships no
  vendor code: the driver does the pacing.
- **Device creation.** Bevy creates the Vulkan device. Its `raw_vulkan_init` feature (bevy_render
  0.20.1) lets `AntiLagInitPlugin`, added before `DefaultPlugins`, add the extension and chain
  `VkPhysicalDeviceAntiLagFeaturesAMD` with `antiLag` on. It does that only when the adapter
  offers both (`vkGetPhysicalDeviceFeatures2`). This is the hook Bevy's own DLSS support uses.
- **The calls** [documented: Khronos `proposals/VK_AMD_anti_lag.adoc`].
  - *Input*: "immediately before input to the application is processed". The call may block, to
    keep the CPU from running ahead. Winit gathers the next frame's input right after the main
    world's update, so the call is the last system of `Last`, after the frame cap
    (`frame_pace::cap_rate`). It carries the next frame's index.
  - *Present*: "before `QueuePresent`", with the same index. Bevy's renderer draws frame n while
    the main world runs frame n + 1. The index is extracted with the frame, and the render world
    sends it just before Bevy's `render_system`, which submits and presents in one step
    [engineering choice: the latest point before the present that Bevy offers].
- **Off** sends mode OFF every frame. `maxFPS` is 0: the frame cap stays `frame_pace`'s
  [engineering choice; the driver's limiter could take it over once Anti-Lag can be measured].
- **Side effect of `raw_vulkan_init`.** Bevy then creates a Vulkan-only wgpu instance whenever
  Vulkan starts. DX12 and GL stay only for when Vulkan can't start (bevy_render 0.20.1
  `renderer/raw_vulkan_init.rs`). FSR, the GPU memory budget and the gpu-allocator patch already
  work through Vulkan.

## Checks

- Unit tests (`anti_lag.rs`):
  - the structures have Vulkan's layout (sizes and offsets on 64-bit);
  - each frame is announced at input, at the end of the update before it, then at present;
  - Off sends mode OFF.
- Smoke `latency-reduction`: `--latency-reduction` turns the switch on, the start says whether
  the driver offers Anti-Lag, and the game runs without validation errors.

## Limits

- **AMD only, and untested on AMD so far.** The machines this was built and measured on have
  NVIDIA GPUs (RTX 5090, the owner's RTX 4070), whose drivers lack `VK_AMD_anti_lag`. So only the
  "not available" path has run. Latency with it on needs an AMD tester (PresentMon, or the
  driver's own overlay).
- **No NVIDIA path yet.** `VK_NV_low_latency2` needs the swapchain created with
  `VkSwapchainLatencyCreateInfoNV`, and present IDs on each present. wgpu creates and presents
  the swapchain itself, and neither it nor Bevy has a hook for those.
- **Pacing points.** The present call comes before Bevy's `render_system` submits, not right
  before `vkQueuePresentKHR`: the closest point Bevy offers [hypothesis: the driver uses it to
  match frames, not as a timestamp; unmeasured].
