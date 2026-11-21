//! Static KosmicKrisp for ARM64 macOS 26 and later.
//!
//! Pass [`get_instance_proc_addr`] to a Vulkan loader adapter. This crate does
//! not initialize Vulkan or change kernel compilation. Its binding uses the
//! Vulkan C ABI and does not depend on a Rust Vulkan binding's types.

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
unsafe extern "system" {
    /// Returns a Vulkan function pointer from the linked KosmicKrisp implementation.
    ///
    /// # Safety
    /// `name` must point to a null-terminated name. `instance` is Vulkan's opaque
    /// dispatchable instance pointer, or null where the requested function permits
    /// it. Both arguments must satisfy Vulkan's `vkGetInstanceProcAddr` requirements.
    #[link_name = "kosmickrisp_get_instance_proc_addr"]
    pub fn get_instance_proc_addr(
        instance: *mut core::ffi::c_void,
        name: *const core::ffi::c_char,
    ) -> Option<unsafe extern "system" fn()>;
}
