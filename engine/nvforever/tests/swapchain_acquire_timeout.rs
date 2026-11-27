//! Regression tests for the vendored `wgpu-hal` Windows swapchain acquire-fence timeout fix.
//!
//! In upstream `wgpu-hal` 30.0.1 (`src/vulkan/swapchain/native.rs`), `NativeSwapchain::acquire`
//! passes `vkWaitForFences(&[fence], true, timeout_ns)` directly through
//! `map_host_device_oom_and_lost_err`, which maps `VK_TIMEOUT` to `DeviceError::Unexpected`
//! (`"Unexpected error variant (driver implementation is at fault)"`) and causes `wgpu-core`
//! to permanently lose a healthy Vulkan device.

use ash::vk;
use wgpu::hal::vulkan::nvf_swapchain_test_support::{
    map_acquire_next_image_error, map_present_error, wait_and_reset_acquire_fence,
    MAX_ACQUIRE_FENCE_WAITS,
};
use wgpu::hal::{DeviceError, SurfaceError};

#[test]
fn acquire_fence_wait_succeeds_and_resets_immediately() {
    let mut waits = 0u32;
    let mut resets = 0u32;
    let res = wait_and_reset_acquire_fence(
        1_000_000_000,
        |ns| {
            assert_eq!(ns, 1_000_000_000);
            waits += 1;
            Ok(())
        },
        || {
            resets += 1;
            Ok(())
        },
    );
    assert!(res.is_ok());
    assert_eq!(waits, 1);
    assert_eq!(resets, 1);
}

#[test]
fn acquire_fence_wait_retries_transient_vk_timeout_and_recovers() {
    let mut waits = 0u32;
    let mut resets = 0u32;
    let res = wait_and_reset_acquire_fence(
        1_000_000_000,
        |_| {
            waits += 1;
            if waits < 3 {
                Err(vk::Result::TIMEOUT)
            } else {
                Ok(())
            }
        },
        || {
            resets += 1;
            Ok(())
        },
    );
    assert!(res.is_ok());
    assert_eq!(waits, 3);
    assert_eq!(resets, 1);
}

#[test]
fn acquire_fence_wait_marks_outdated_after_repeated_timeouts() {
    let mut waits = 0u32;
    let mut resets = 0u32;
    let res = wait_and_reset_acquire_fence(
        1_000_000_000,
        |_| {
            waits += 1;
            Err(vk::Result::TIMEOUT)
        },
        || {
            resets += 1;
            Ok(())
        },
    );
    assert_eq!(res, Err(SurfaceError::Outdated));
    assert_eq!(waits, MAX_ACQUIRE_FENCE_WAITS);
    assert_eq!(resets, 0);
}

#[test]
fn acquire_fence_wait_preserves_real_device_lost_and_oom() {
    let lost = wait_and_reset_acquire_fence(
        1_000_000_000,
        |_| Err(vk::Result::ERROR_DEVICE_LOST),
        || Ok(()),
    );
    assert_eq!(lost, Err(SurfaceError::Device(DeviceError::Lost)));

    let oom = wait_and_reset_acquire_fence(
        1_000_000_000,
        |_| Err(vk::Result::ERROR_OUT_OF_DEVICE_MEMORY),
        || Ok(()),
    );
    assert_eq!(oom, Err(SurfaceError::Device(DeviceError::OutOfMemory)));
}

#[test]
fn acquire_and_present_map_fse_mode_lost_to_outdated() {
    assert_eq!(
        map_acquire_next_image_error(vk::Result::ERROR_FULL_SCREEN_EXCLUSIVE_MODE_LOST_EXT),
        SurfaceError::Outdated
    );
    assert_eq!(
        map_present_error(vk::Result::ERROR_FULL_SCREEN_EXCLUSIVE_MODE_LOST_EXT),
        SurfaceError::Outdated
    );
    assert_eq!(
        map_acquire_next_image_error(vk::Result::TIMEOUT),
        SurfaceError::Timeout
    );
    assert_eq!(
        map_acquire_next_image_error(vk::Result::ERROR_DEVICE_LOST),
        SurfaceError::Device(DeviceError::Lost)
    );
}
