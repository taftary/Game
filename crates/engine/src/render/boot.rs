//! Vulkan boot policy: the ADR-007 device floor as code.
//!
//! - Instance requests **at most** Vulkan 1.1 (`MAX_API_VERSION`): the
//!   gameplay-critical path must never require anything above Vulkan 1.1
//!   core, so the cap — not just the minimum — is pinned here.
//! - `ENUMERATE_PORTABILITY` is always set: Apple targets expose Vulkan
//!   only via MoltenVK portability-subset devices, which stay hidden
//!   without the flag (ADR-007 Apple note).
//! - Device selection requires `khr_swapchain` + a graphics-capable queue
//!   with presentation support, then prefers discrete GPUs.
//! - Validation layers are dev/debug-only; the selected adapter + driver
//!   are logged via `tracing` at every boot (rendering.md rule).

use std::sync::Arc;

use vulkano::device::DeviceExtensions;
use vulkano::device::physical::{PhysicalDevice, PhysicalDeviceType};
use vulkano::instance::{Instance, InstanceCreateFlags, InstanceCreateInfo, InstanceExtensions};
use vulkano::{Validated, Version, VulkanError, VulkanLibrary};

/// Highest Vulkan API version the engine will use: the 1.1 device floor
/// (ADR-007). Capping the maximum keeps >1.1 features out by construction.
pub const MAX_API_VERSION: Version = Version::V1_1;

/// Validation layer enabled in dev/debug builds (needs the Vulkan SDK
/// layers installed; absence fails instance creation with a clear error).
pub const VALIDATION_LAYER: &str = "VK_LAYER_KHRONOS_validation";

/// Builds the instance creation policy: portability enumeration always
/// on, API capped at 1.1, app/engine stamped, validation layers iff
/// `enable_validation`. `enabled_extensions` come from the platform
/// (`Surface::required_extensions`) — the caller fetches them so this
/// stays headless-testable.
pub fn instance_create_info(
    enabled_extensions: InstanceExtensions,
    enable_validation: bool,
) -> InstanceCreateInfo {
    InstanceCreateInfo {
        flags: InstanceCreateFlags::ENUMERATE_PORTABILITY,
        application_name: Some("planetcrafter-smoke".to_owned()),
        application_version: Version::major_minor(0, 1),
        engine_name: Some("game_engine".to_owned()),
        engine_version: Version::major_minor(0, 1),
        max_api_version: Some(MAX_API_VERSION),
        enabled_layers: if enable_validation {
            vec![VALIDATION_LAYER.to_owned()]
        } else {
            Vec::new()
        },
        enabled_extensions,
        ..Default::default()
    }
}

/// Device extensions the renderer requires: swapchain presentation only.
/// `khr_portability_subset` is auto-enabled by vulkano where advertised.
pub fn required_device_extensions() -> DeviceExtensions {
    DeviceExtensions {
        khr_swapchain: true,
        ..DeviceExtensions::empty()
    }
}

/// Device preference, lower is better: discrete > integrated > virtual >
/// CPU > other. Pure so tier/device policy is unit-testable.
pub fn device_score(device_type: PhysicalDeviceType) -> u8 {
    match device_type {
        PhysicalDeviceType::DiscreteGpu => 0,
        PhysicalDeviceType::IntegratedGpu => 1,
        PhysicalDeviceType::VirtualGpu => 2,
        PhysicalDeviceType::Cpu => 3,
        PhysicalDeviceType::Other => 4,
        _ => 5,
    }
}

/// Creates the Vulkan instance from the M1 policy.
pub fn create_instance(
    library: Arc<VulkanLibrary>,
    enabled_extensions: InstanceExtensions,
    enable_validation: bool,
) -> Result<Arc<Instance>, Validated<VulkanError>> {
    Instance::new(
        library,
        instance_create_info(enabled_extensions, enable_validation),
    )
}

/// Logs the selected adapter + driver (rendering.md: always logged at
/// startup) through the `tracing` backend (ADR-009).
pub fn log_physical_device(physical: &PhysicalDevice) {
    let properties = physical.properties();
    tracing::info!(
        adapter = properties.device_name.as_str(),
        device_type = ?properties.device_type,
        api = %format!(
            "{}.{}.{}",
            properties.api_version.major,
            properties.api_version.minor,
            properties.api_version.patch,
        ),
        driver_version = properties.driver_version,
        "selected Vulkan physical device",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_policy_pins_the_1_1_floor() {
        let info = instance_create_info(InstanceExtensions::empty(), false);
        assert_eq!(info.max_api_version, Some(Version::V1_1));
        assert!(
            info.flags
                .intersects(InstanceCreateFlags::ENUMERATE_PORTABILITY)
        );
        assert!(info.enabled_layers.is_empty());
        assert_eq!(
            info.application_name.as_deref(),
            Some("planetcrafter-smoke")
        );
    }

    #[test]
    fn validation_layer_is_opt_in_only() {
        let dev = instance_create_info(InstanceExtensions::empty(), true);
        assert_eq!(dev.enabled_layers, vec![VALIDATION_LAYER.to_owned()]);
    }

    #[test]
    fn device_extensions_require_swapchain_only() {
        let extensions = required_device_extensions();
        assert!(extensions.khr_swapchain);
    }

    #[test]
    fn discrete_gpus_win_device_scoring() {
        assert!(
            device_score(PhysicalDeviceType::DiscreteGpu)
                < device_score(PhysicalDeviceType::IntegratedGpu)
        );
        assert!(
            device_score(PhysicalDeviceType::IntegratedGpu) < device_score(PhysicalDeviceType::Cpu)
        );
        assert!(device_score(PhysicalDeviceType::Cpu) < device_score(PhysicalDeviceType::Other));
    }
}
