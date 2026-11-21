/* SPDX-License-Identifier: MIT */
#include "kk_entrypoints.h"
#include <string.h>

VKAPI_ATTR PFN_vkVoidFunction VKAPI_CALL
kosmickrisp_get_instance_proc_addr(VkInstance instance, const char *name);

/* Direct callers have no Vulkan loader to enumerate layers. */
VKAPI_ATTR VkResult VKAPI_CALL
kk_EnumerateInstanceLayerProperties(uint32_t *count, VkLayerProperties *properties)
{
   *count = 0;
   return VK_SUCCESS;
}

__attribute__((visibility("default")))
VKAPI_ATTR PFN_vkVoidFunction VKAPI_CALL
kosmickrisp_get_instance_proc_addr(VkInstance instance, const char *name)
{
   return kk_GetInstanceProcAddr(instance, name);
}
