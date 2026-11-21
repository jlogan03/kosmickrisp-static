# kosmickrisp-static

Static KosmicKrisp for ARM64 macOS 26 and later. The driver uses Apple's Metal
framework. Consumers do not need a Vulkan loader or a system driver installation.

The build pins Mesa 26.3.0 and SPIRV-Tools 2026.4. It packages the driver and its
runtime dependencies in `libKosmicKrisp.a`. Apple frameworks and the C++ runtime
remain system dependencies. The prototype disables Mesa's disk shader cache.

## Build from source

Install Xcode with the macOS 26 SDK, CMake, Ninja, pkg-config, LLVM, and
SPIRV-LLVM-Translator. The LLVM and translator versions must match. Mesa also
needs Python with Meson 1.9.1 or later, Mako, packaging, and PyYAML.

```sh
cargo run --features build-kosmickrisp --example probe
```

The probe creates a Vulkan device and checks a compute shader's output. It
requires a Metal 4 device. Use `-- --instance-only` to test instance creation
without a GPU.

CI builds the archive, links the probe, and checks instance creation and dynamic
dependencies. GitHub's hosted macOS runner exposes a virtual GPU without Metal 4,
so CI skips the compute test there. Compute validation requires a Metal 4 device.

## Package an archive

```sh
cargo run -p kosmickrisp-build -- dist
KOSMICKRISP_STATIC_DIR="$PWD/dist/artifact" cargo run --example probe
```

Use `KOSMICKRISP_STATIC_DIR` for an unpacked archive. To download an archive, set
`KOSMICKRISP_ARTIFACT_URL` and `KOSMICKRISP_ARTIFACT_SHA256`. No default release
URL is configured yet. Prebuilt archives need no Python or LLVM installation.

Pass `kosmickrisp_static::get_instance_proc_addr` to an ash or Vulkano loader
adapter. Its signature uses the Vulkan C ABI and does not depend on an ash version.
Direct loading does not load Vulkan validation layers.
