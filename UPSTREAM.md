# Upstream work

- Add an installed static archive target for KosmicKrisp and its Mesa dependencies.
- Use `weak_import` for optional Vulkan entry points on Darwin. This permits
  static consumers to link without `-undefined,dynamic_lookup`.
- Provide layer enumeration for direct callers without the Vulkan loader.
