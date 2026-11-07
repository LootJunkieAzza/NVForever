//! Latency reduction: AMD Anti-Lag through Vulkan (`VK_AMD_anti_lag`, #1778). The driver keeps
//! the CPU from preparing frames ahead of the GPU, so input reaches the screen sooner. Off by
//! default: Settings > Graphics > Latency reduction, or `--latency-reduction`.
//!
//! - **The extension.** Bevy creates the Vulkan device. With its `raw_vulkan_init` feature, a
//!   callback ([`AntiLagInitPlugin`]) adds `VK_AMD_anti_lag` and turns its `antiLag` feature on
//!   when the GPU's driver offers both [documented: Vulkan registry `vk.xml`, extension 477].
//!   AMD's drivers offer it: Windows Adrenalin 24.9.1 or later, Linux Mesa RADV 26.3 or later
//!   [documented: GPUOpen's Anti-Lag 2 page; Mesa 26.3]. Elsewhere the settings row is dimmed
//!   and the log says so, once.
//! - **The calls.** `vkAntiLagUpdateAMD` goes "immediately before input to the application is
//!   processed", where it may block to pace the CPU, and again "before `QueuePresent`" with the
//!   same frame index [documented: Khronos `proposals/VK_AMD_anti_lag.adoc`].
//!   - Input: winit gathers the next frame's input right after the main world's update ends.
//!     So the call is the last thing in `Last`, after the frame cap (`frame_pace::cap_rate`),
//!     with the next frame's index.
//!   - Present: Bevy's renderer draws a frame while the main world runs the next one. The
//!     frame's index is extracted with it, and the render world sends it just before Bevy's
//!     `render_system`, which submits and presents in one step [engineering choice: the latest
//!     point before the present that Bevy offers].
//! - **Off** sends mode OFF, so the driver paces nothing. `maxFPS` stays 0: the frame cap is
//!   `frame_pace`'s [engineering choice].
//!
//! With `raw_vulkan_init` on, Bevy creates a Vulkan-only wgpu instance whenever Vulkan starts.
//! Other backends remain only for when it can't (bevy_render 0.20.1
//! `renderer/raw_vulkan_init.rs`).

use std::ffi::{c_void, CStr};

use ash::vk;
use bevy::prelude::*;
use bevy::render::renderer::raw_vulkan_init::{AdditionalVulkanFeatures, RawVulkanInitSettings};
use bevy::render::renderer::{render_system, RenderAdapterInfo, RenderDevice};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};

const EXTENSION: &CStr = c"VK_AMD_anti_lag";
// Structure types: 1000000000 + (477 - 1) * 1000 + offset [documented: vk.xml, extension 477].
const PHYSICAL_DEVICE_ANTI_LAG_FEATURES: vk::StructureType = vk::StructureType::from_raw(1_000_476_000);
const ANTI_LAG_DATA: vk::StructureType = vk::StructureType::from_raw(1_000_476_001);
const ANTI_LAG_PRESENTATION_INFO: vk::StructureType = vk::StructureType::from_raw(1_000_476_002);
// `VkAntiLagModeAMD` and `VkAntiLagStageAMD` [documented: vk.xml].
const MODE_ON: i32 = 1;
const MODE_OFF: i32 = 2;
const STAGE_INPUT: i32 = 0;
const STAGE_PRESENT: i32 = 1;

/// `VkPhysicalDeviceAntiLagFeaturesAMD` (ash 0.38 predates the extension).
#[repr(C)]
struct PhysicalDeviceAntiLagFeatures {
    s_type: vk::StructureType,
    p_next: *mut c_void,
    anti_lag: vk::Bool32,
}

/// `VkAntiLagPresentationInfoAMD`.
#[repr(C)]
struct AntiLagPresentationInfo {
    s_type: vk::StructureType,
    p_next: *mut c_void,
    stage: i32,
    frame_index: u64,
}

/// `VkAntiLagDataAMD`.
#[repr(C)]
struct AntiLagData {
    s_type: vk::StructureType,
    p_next: *const c_void,
    mode: i32,
    max_fps: u32,
    presentation_info: *const AntiLagPresentationInfo,
}

/// `vkAntiLagUpdateAMD`.
type AntiLagUpdate = unsafe extern "system" fn(vk::Device, *const AntiLagData);

/// Marks, in Bevy's `AdditionalVulkanFeatures`, a device made with `VK_AMD_anti_lag` on.
struct AntiLagEnabled;

/// Asks for the extension when Bevy creates the Vulkan device. Added before `DefaultPlugins`:
/// Bevy reads the callbacks when it creates the renderer.
pub struct AntiLagInitPlugin;

impl Plugin for AntiLagInitPlugin {
    fn build(&self, app: &mut App) {
        let mut settings = app.world_mut().get_resource_or_init::<RawVulkanInitSettings>();
        // SAFETY: the callback adds the extension and its feature only when the adapter offers
        // both, and removes nothing.
        unsafe {
            settings.add_create_device_callback(|args, adapter, features| {
                if !supported(adapter) {
                    return;
                }
                args.extensions.push(EXTENSION);
                // Must outlive the device's creation, which only the callback's caller sees
                // the end of: one small allocation is kept for each device made.
                let chained = Box::leak(Box::new(PhysicalDeviceAntiLagFeatures {
                    s_type: PHYSICAL_DEVICE_ANTI_LAG_FEATURES,
                    p_next: args.create_info.p_next.cast_mut(),
                    anti_lag: vk::TRUE,
                }));
                args.create_info.p_next = std::ptr::from_ref(chained).cast();
                features.insert::<AntiLagEnabled>();
            });
        }
    }
}

/// Whether the adapter's driver offers the extension and its `antiLag` feature.
fn supported(adapter: &wgpu::hal::vulkan::Adapter) -> bool {
    if !adapter.physical_device_capabilities().supports_extension(EXTENSION) {
        return false;
    }
    let mut anti_lag = PhysicalDeviceAntiLagFeatures { s_type: PHYSICAL_DEVICE_ANTI_LAG_FEATURES, p_next: std::ptr::null_mut(), anti_lag: vk::FALSE };
    let mut features = vk::PhysicalDeviceFeatures2 { p_next: std::ptr::from_mut(&mut anti_lag).cast(), ..Default::default() };
    // SAFETY: the chain holds one structure of an extension the device offers (Vulkan 1.1 and up,
    // which the extension requires).
    unsafe { adapter.shared_instance().raw_instance().get_physical_device_features2(adapter.raw_physical_device(), &mut features) };
    anti_lag.anti_lag == vk::TRUE
}

/// The latency reduction switch (the settings row, `--latency-reduction`).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LatencyReduction {
    pub on: bool,
    /// Frame generation turns it on while it runs (`frame_gen.rs`), whatever the switch says.
    pub by_frame_generation: bool,
}

impl LatencyReduction {
    /// Whether the driver is asked to pace frames.
    pub fn active(&self) -> bool {
        self.on || self.by_frame_generation
    }
}

/// The driver's `vkAntiLagUpdateAMD` for the render device, present only when the device was
/// made with the extension.
#[derive(Resource, Clone, Copy)]
pub struct AntiLagDriver {
    device: vk::Device,
    update: AntiLagUpdate,
}

impl AntiLagDriver {
    fn send(&self, on: bool, stage: i32, frame: u64) {
        let info = AntiLagPresentationInfo { s_type: ANTI_LAG_PRESENTATION_INFO, p_next: std::ptr::null_mut(), stage, frame_index: frame };
        let data = AntiLagData { s_type: ANTI_LAG_DATA, p_next: std::ptr::null(), mode: if on { MODE_ON } else { MODE_OFF }, max_fps: 0, presentation_info: &info };
        // SAFETY: the device was made with the extension and its feature on, and lasts as long as
        // the app; the call has no externally synchronized parameters [documented: vk.xml].
        unsafe { (self.update)(self.device, &data) };
    }
}

/// Whether this GPU's driver takes latency reduction (for the settings row).
pub fn available(world: &World) -> bool {
    world.contains_resource::<AntiLagDriver>()
}

/// Frame indices in the main world: `current` is the frame whose input this update
/// processes, `next` the one announced at its end.
#[derive(Resource, Default)]
struct Frames {
    current: u64,
    next: u64,
}

/// The render world's copy: the frame it draws, and whether the switch was on for it.
#[derive(Resource, Default, Clone, Copy)]
struct Presenting {
    frame: u64,
    on: bool,
}

/// The switch and the calls to the driver.
pub struct AntiLagPlugin(pub bool);

impl Plugin for AntiLagPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(LatencyReduction { on: self.0, by_frame_generation: false })
            .init_resource::<Frames>()
            .add_systems(First, begin_frame)
            .add_systems(Update, log_switch)
            .add_systems(Last, announce_input.after(crate::frame_pace::cap_rate));
        let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
        render
            .init_resource::<Presenting>()
            .add_systems(ExtractSchedule, extract)
            .add_systems(Render, announce_present.in_set(RenderSystems::Render).before(render_system));
    }

    fn finish(&self, app: &mut App) {
        let Some(render) = app.get_sub_app(RenderApp) else { return };
        let world = render.world();
        let enabled = world.get_resource::<AdditionalVulkanFeatures>().is_some_and(|f| f.has::<AntiLagEnabled>());
        let adapter = world.get_resource::<RenderAdapterInfo>().map_or_else(String::new, |i| format!("{} ({:?}, {} {})", i.name, i.backend, i.driver, i.driver_info));
        let driver = if enabled { world.get_resource::<RenderDevice>().and_then(|d| load(d.wgpu_device())) } else { None };
        match driver {
            Some(driver) => {
                info!("latency reduction: AMD Anti-Lag (VK_AMD_anti_lag) available on {adapter}");
                app.insert_resource(driver);
                app.sub_app_mut(RenderApp).insert_resource(driver);
            }
            None => info!("latency reduction: not available on {adapter}: its Vulkan driver has no VK_AMD_anti_lag (AMD GPUs only)"),
        }
    }
}

/// Looks the driver's `vkAntiLagUpdateAMD` up for the render device.
fn load(device: &wgpu::Device) -> Option<AntiLagDriver> {
    // SAFETY: only the device's handle is read and a function looked up.
    let hal = unsafe { device.as_hal::<wgpu::hal::api::Vulkan>() }?;
    let raw = hal.raw_device().handle();
    // SAFETY: as above; the name is the extension's command [documented: vk.xml].
    let found = unsafe { hal.shared_instance().raw_instance().get_device_proc_addr(raw, c"vkAntiLagUpdateAMD".as_ptr()) }?;
    // SAFETY: the registry's signature of `vkAntiLagUpdateAMD`.
    let update = unsafe { std::mem::transmute::<unsafe extern "system" fn(), AntiLagUpdate>(found) };
    Some(AntiLagDriver { device: raw, update })
}

fn begin_frame(mut frames: ResMut<Frames>) {
    frames.current = frames.next;
}

fn announce_input(switch: Res<LatencyReduction>, driver: Option<Res<AntiLagDriver>>, mut frames: ResMut<Frames>) {
    frames.next = frames.current + 1;
    if let Some(driver) = driver {
        driver.send(switch.active(), STAGE_INPUT, frames.next);
    }
}

fn extract(mut presenting: ResMut<Presenting>, switch: Extract<Res<LatencyReduction>>, frames: Extract<Res<Frames>>) {
    *presenting = Presenting { frame: frames.current, on: switch.active() };
}

fn announce_present(presenting: Res<Presenting>, driver: Option<Res<AntiLagDriver>>) {
    if let Some(driver) = driver {
        driver.send(presenting.on, STAGE_PRESENT, presenting.frame);
    }
}

/// Says when the switch turns on or off (and that it does nothing where the driver lacks it).
fn log_switch(switch: Res<LatencyReduction>, driver: Option<Res<AntiLagDriver>>, mut said: Local<Option<bool>>) {
    let on = switch.active();
    if *said == Some(on) || (said.is_none() && !on) {
        *said = Some(on);
        return;
    }
    *said = Some(on);
    let how = if driver.is_some() { "AMD Anti-Lag" } else { "not available on this GPU" };
    let why = if switch.by_frame_generation && !switch.on { ", with frame generation" } else { "" };
    info!("latency reduction: {} ({how}{why})", if on { "on" } else { "off" });
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use std::sync::Mutex;

    /// The Vulkan layouts on 64-bit targets [documented: vk.xml member order and C alignment].
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn the_structures_have_vulkans_layout() {
        use std::mem::{offset_of, size_of};
        assert_eq!((size_of::<PhysicalDeviceAntiLagFeatures>(), offset_of!(PhysicalDeviceAntiLagFeatures, anti_lag)), (24, 16));
        assert_eq!((size_of::<AntiLagPresentationInfo>(), offset_of!(AntiLagPresentationInfo, stage), offset_of!(AntiLagPresentationInfo, frame_index)), (32, 16, 24));
        assert_eq!((size_of::<AntiLagData>(), offset_of!(AntiLagData, mode), offset_of!(AntiLagData, max_fps), offset_of!(AntiLagData, presentation_info)), (32, 16, 20, 24));
    }

    /// (stage, frame, mode) of each call the test driver got.
    static CALLS: Mutex<Vec<(i32, u64, i32)>> = Mutex::new(Vec::new());

    unsafe extern "system" fn record(_: vk::Device, data: *const AntiLagData) {
        // SAFETY: the driver is only called with a valid structure and presentation info.
        let (data, info) = unsafe { (&*data, &*(*data).presentation_info) };
        CALLS.lock().unwrap().push((info.stage, info.frame_index, data.mode));
    }

    #[test]
    fn each_frame_is_announced_at_input_then_at_present() {
        let mut world = World::new();
        world.insert_resource(LatencyReduction { on: true, by_frame_generation: false });
        world.init_resource::<Frames>();
        world.insert_resource(AntiLagDriver { device: vk::Device::null(), update: record });
        // Three main-world updates; each frame is presented while the next update runs.
        let mut presented = Vec::new();
        for _ in 0..3 {
            world.run_system_once(begin_frame).unwrap();
            let current = world.resource::<Frames>().current;
            world.run_system_once(announce_input).unwrap();
            presented.push(current);
            world.insert_resource(Presenting { frame: current, on: true });
            world.run_system_once(announce_present).unwrap();
        }
        world.resource_mut::<LatencyReduction>().on = false;
        world.run_system_once(begin_frame).unwrap();
        world.run_system_once(announce_input).unwrap();
        let calls = std::mem::take(&mut *CALLS.lock().unwrap());
        assert_eq!(presented, [0, 1, 2]);
        assert_eq!(
            calls,
            [
                (STAGE_INPUT, 1, MODE_ON),
                (STAGE_PRESENT, 0, MODE_ON),
                (STAGE_INPUT, 2, MODE_ON),
                (STAGE_PRESENT, 1, MODE_ON),
                (STAGE_INPUT, 3, MODE_ON),
                (STAGE_PRESENT, 2, MODE_ON),
                (STAGE_INPUT, 4, MODE_OFF),
            ],
            "frame n is announced at input (end of update n - 1) before its present"
        );
    }

    #[test]
    fn frame_generation_turns_latency_reduction_on_whatever_the_switch_says() {
        assert!(!LatencyReduction { on: false, by_frame_generation: false }.active());
        assert!(LatencyReduction { on: false, by_frame_generation: true }.active());
        assert!(LatencyReduction { on: true, by_frame_generation: false }.active());
    }
}
