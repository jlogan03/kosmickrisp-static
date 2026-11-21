# Upstream work

- Add an installed static archive target for KosmicKrisp and its Mesa dependencies.
- Resolve absent Vulkan entry points to null when building a static archive on
  Darwin. The package supplies private absolute symbols for these entries.
  Consumers do not need `-undefined,dynamic_lookup`.
- Remove the duplicate `libpoly.cpp` object from `libkk_shaders`.
  `libpoly_nir` already supplies that object.
- Provide layer enumeration for direct callers without the Vulkan loader.
