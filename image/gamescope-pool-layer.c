/*
 * Explicit Vulkan layer for pkg:gamescope.
 *
 * Ubuntu gamescope 3.16 sizes its descriptor pool with
 *   (2 * sampler_slots + 2 * lut) * maxSets
 * which is 108 combined-image-samplers for 3 sets. RADV 26.x uses one
 * descriptor per YCbCr plane, so the same layout consumes ~156 and
 * vkAllocateDescriptorSets returns VK_ERROR_OUT_OF_POOL_MEMORY on
 * Southern Islands (and other pre-GFX9). Pad COMBINED_IMAGE_SAMPLER
 * pool sizes. Enabled only when the gamescope wrapper sets
 * VK_INSTANCE_LAYERS / VK_LAYER_PATH.
 */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <stdio.h>

#define VKAPI_ATTR
#define VKAPI_CALL
#define VKAPI_PTR

typedef uint32_t VkFlags;
typedef int32_t VkResult;
typedef uint32_t VkStructureType;
typedef uint32_t VkDescriptorType;
typedef uint32_t VkDescriptorPoolCreateFlags;
typedef struct VkInstance_T *VkInstance;
typedef struct VkPhysicalDevice_T *VkPhysicalDevice;
typedef struct VkDevice_T *VkDevice;
typedef struct VkDescriptorPool_T *VkDescriptorPool;
typedef struct VkAllocationCallbacks VkAllocationCallbacks;

#define VK_SUCCESS 0
#define VK_ERROR_INITIALIZATION_FAILED -3
#define VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER 1
#define VK_STRUCTURE_TYPE_LOADER_INSTANCE_CREATE_INFO 47
#define VK_STRUCTURE_TYPE_LOADER_DEVICE_CREATE_INFO 48

typedef void(VKAPI_PTR *PFN_vkVoidFunction)(void);
typedef PFN_vkVoidFunction(VKAPI_PTR *PFN_vkGetInstanceProcAddr)(VkInstance, const char *);
typedef PFN_vkVoidFunction(VKAPI_PTR *PFN_vkGetDeviceProcAddr)(VkDevice, const char *);
typedef PFN_vkVoidFunction(VKAPI_PTR *PFN_GetPhysicalDeviceProcAddr)(VkInstance, const char *);

typedef struct VkBaseInStructure {
	VkStructureType sType;
	const struct VkBaseInStructure *pNext;
} VkBaseInStructure;

typedef struct VkDescriptorPoolSize {
	VkDescriptorType type;
	uint32_t descriptorCount;
} VkDescriptorPoolSize;

typedef struct VkDescriptorPoolCreateInfo {
	VkStructureType sType;
	const void *pNext;
	VkDescriptorPoolCreateFlags flags;
	uint32_t maxSets;
	uint32_t poolSizeCount;
	const VkDescriptorPoolSize *pPoolSizes;
} VkDescriptorPoolCreateInfo;

typedef struct VkInstanceCreateInfo {
	VkStructureType sType;
	const void *pNext;
	uint32_t flags;
	const void *pApplicationInfo;
	uint32_t enabledLayerCount;
	const char *const *ppEnabledLayerNames;
	uint32_t enabledExtensionCount;
	const char *const *ppEnabledExtensionNames;
} VkInstanceCreateInfo;

typedef struct VkDeviceCreateInfo {
	VkStructureType sType;
	const void *pNext;
	uint32_t flags;
	uint32_t queueCreateInfoCount;
	const void *pQueueCreateInfos;
	uint32_t enabledLayerCount;
	const char *const *ppEnabledLayerNames;
	uint32_t enabledExtensionCount;
	const char *const *ppEnabledExtensionNames;
	const void *pEnabledFeatures;
} VkDeviceCreateInfo;

typedef VkResult(VKAPI_PTR *PFN_vkCreateInstance)(const VkInstanceCreateInfo *, const VkAllocationCallbacks *,
						  VkInstance *);
typedef VkResult(VKAPI_PTR *PFN_vkCreateDevice)(VkPhysicalDevice, const VkDeviceCreateInfo *,
						const VkAllocationCallbacks *, VkDevice *);
typedef VkResult(VKAPI_PTR *PFN_vkCreateDescriptorPool)(VkDevice, const VkDescriptorPoolCreateInfo *,
							const VkAllocationCallbacks *, VkDescriptorPool *);

enum { LAYER_NEGOTIATE_INTERFACE_STRUCT = 1 };
enum { CURRENT_LOADER_LAYER_INTERFACE_VERSION = 2, MIN_SUPPORTED_LOADER_LAYER_INTERFACE_VERSION = 1 };
enum { VK_LAYER_LINK_INFO = 0 };

typedef struct VkNegotiateLayerInterface {
	uint32_t sType;
	void *pNext;
	uint32_t loaderLayerInterfaceVersion;
	PFN_vkGetInstanceProcAddr pfnGetInstanceProcAddr;
	PFN_vkGetDeviceProcAddr pfnGetDeviceProcAddr;
	PFN_GetPhysicalDeviceProcAddr pfnGetPhysicalDeviceProcAddr;
} VkNegotiateLayerInterface;

typedef struct VkLayerInstanceLink {
	struct VkLayerInstanceLink *pNext;
	PFN_vkGetInstanceProcAddr pfnNextGetInstanceProcAddr;
	PFN_GetPhysicalDeviceProcAddr pfnNextGetPhysicalDeviceProcAddr;
} VkLayerInstanceLink;

typedef struct VkLayerInstanceCreateInfo {
	VkStructureType sType;
	const void *pNext;
	uint32_t function;
	union {
		VkLayerInstanceLink *pLayerInfo;
	} u;
} VkLayerInstanceCreateInfo;

typedef struct VkLayerDeviceLink {
	struct VkLayerDeviceLink *pNext;
	PFN_vkGetInstanceProcAddr pfnNextGetInstanceProcAddr;
	PFN_vkGetDeviceProcAddr pfnNextGetDeviceProcAddr;
} VkLayerDeviceLink;

typedef struct VkLayerDeviceCreateInfo {
	VkStructureType sType;
	const void *pNext;
	uint32_t function;
	union {
		VkLayerDeviceLink *pLayerInfo;
	} u;
} VkLayerDeviceCreateInfo;

static PFN_vkGetInstanceProcAddr next_gipa;
static PFN_vkGetDeviceProcAddr next_gdpa;
static PFN_vkCreateInstance next_create_instance;
static PFN_vkCreateDevice next_create_device;
static PFN_vkCreateDescriptorPool next_pool;
static int padded_once;

static VkLayerInstanceCreateInfo *find_inst_link(const VkInstanceCreateInfo *info) {
	for (const VkBaseInStructure *c = (const void *)info->pNext; c; c = c->pNext) {
		if (c->sType == VK_STRUCTURE_TYPE_LOADER_INSTANCE_CREATE_INFO) {
			VkLayerInstanceCreateInfo *lic = (VkLayerInstanceCreateInfo *)c;
			if (lic->function == VK_LAYER_LINK_INFO)
				return lic;
		}
	}
	return NULL;
}

static VkLayerDeviceCreateInfo *find_dev_link(const VkDeviceCreateInfo *info) {
	for (const VkBaseInStructure *c = (const void *)info->pNext; c; c = c->pNext) {
		if (c->sType == VK_STRUCTURE_TYPE_LOADER_DEVICE_CREATE_INFO) {
			VkLayerDeviceCreateInfo *lic = (VkLayerDeviceCreateInfo *)c;
			if (lic->function == VK_LAYER_LINK_INFO)
				return lic;
		}
	}
	return NULL;
}

static VkResult VKAPI_CALL hook_CreateInstance(const VkInstanceCreateInfo *info, const VkAllocationCallbacks *a,
					       VkInstance *out) {
	VkLayerInstanceCreateInfo *link = find_inst_link(info);
	if (!link || !link->u.pLayerInfo)
		return VK_ERROR_INITIALIZATION_FAILED;
	next_gipa = link->u.pLayerInfo->pfnNextGetInstanceProcAddr;
	link->u.pLayerInfo = link->u.pLayerInfo->pNext;
	next_create_instance = (PFN_vkCreateInstance)next_gipa(NULL, "vkCreateInstance");
	return next_create_instance(info, a, out);
}

static VkResult VKAPI_CALL hook_CreateDevice(VkPhysicalDevice phys, const VkDeviceCreateInfo *info,
					     const VkAllocationCallbacks *a, VkDevice *out) {
	VkLayerDeviceCreateInfo *link = find_dev_link(info);
	if (link && link->u.pLayerInfo) {
		next_gdpa = link->u.pLayerInfo->pfnNextGetDeviceProcAddr;
		next_gipa = link->u.pLayerInfo->pfnNextGetInstanceProcAddr;
		link->u.pLayerInfo = link->u.pLayerInfo->pNext;
	}
	if (!next_create_device)
		next_create_device = (PFN_vkCreateDevice)next_gipa(NULL, "vkCreateDevice");
	VkResult r = next_create_device(phys, info, a, out);
	if (r == VK_SUCCESS && next_gdpa)
		next_pool = (PFN_vkCreateDescriptorPool)next_gdpa(*out, "vkCreateDescriptorPool");
	return r;
}

static VkResult VKAPI_CALL hook_CreateDescriptorPool(VkDevice device, const VkDescriptorPoolCreateInfo *info,
						     const VkAllocationCallbacks *a, VkDescriptorPool *out) {
	VkDescriptorPoolSize padded[8];
	VkDescriptorPoolCreateInfo local = *info;
	if (info->poolSizeCount > 0 && info->poolSizeCount <= 8) {
		for (uint32_t i = 0; i < info->poolSizeCount; i++) {
			padded[i] = info->pPoolSizes[i];
			if (padded[i].type == VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER) {
				uint32_t n = padded[i].descriptorCount * 4u;
				if (n < padded[i].descriptorCount)
					n = 0xffffffffu;
				if (!padded_once) {
					fprintf(stderr,
						"[gamescope-pool] combined image samplers %u -> %u (RADV YCbCr planes)\n",
						padded[i].descriptorCount, n);
					padded_once = 1;
				}
				padded[i].descriptorCount = n;
			}
		}
		local.pPoolSizes = padded;
	}
	return next_pool(device, &local, a, out);
}

static PFN_vkVoidFunction VKAPI_CALL hook_GetDeviceProcAddr(VkDevice device, const char *name);
static PFN_vkVoidFunction VKAPI_CALL hook_GetInstanceProcAddr(VkInstance instance, const char *name);

static PFN_vkVoidFunction VKAPI_CALL hook_GetDeviceProcAddr(VkDevice device, const char *name) {
	if (!strcmp(name, "vkCreateDescriptorPool"))
		return (PFN_vkVoidFunction)hook_CreateDescriptorPool;
	if (!strcmp(name, "vkGetDeviceProcAddr"))
		return (PFN_vkVoidFunction)hook_GetDeviceProcAddr;
	if (!strcmp(name, "vkCreateDevice"))
		return (PFN_vkVoidFunction)hook_CreateDevice;
	return next_gdpa ? next_gdpa(device, name) : NULL;
}

static PFN_vkVoidFunction VKAPI_CALL hook_GetInstanceProcAddr(VkInstance instance, const char *name) {
	if (!strcmp(name, "vkCreateInstance"))
		return (PFN_vkVoidFunction)hook_CreateInstance;
	if (!strcmp(name, "vkCreateDevice"))
		return (PFN_vkVoidFunction)hook_CreateDevice;
	if (!strcmp(name, "vkGetDeviceProcAddr"))
		return (PFN_vkVoidFunction)hook_GetDeviceProcAddr;
	if (!strcmp(name, "vkGetInstanceProcAddr"))
		return (PFN_vkVoidFunction)hook_GetInstanceProcAddr;
	if (!strcmp(name, "vkCreateDescriptorPool"))
		return (PFN_vkVoidFunction)hook_CreateDescriptorPool;
	return next_gipa ? next_gipa(instance, name) : NULL;
}

VKAPI_ATTR VkResult VKAPI_CALL vkNegotiateLoaderLayerInterfaceVersion(VkNegotiateLayerInterface *p) {
	if (!p || p->sType != LAYER_NEGOTIATE_INTERFACE_STRUCT)
		return VK_ERROR_INITIALIZATION_FAILED;
	if (p->loaderLayerInterfaceVersion < MIN_SUPPORTED_LOADER_LAYER_INTERFACE_VERSION)
		return VK_ERROR_INITIALIZATION_FAILED;
	if (p->loaderLayerInterfaceVersion > CURRENT_LOADER_LAYER_INTERFACE_VERSION)
		p->loaderLayerInterfaceVersion = CURRENT_LOADER_LAYER_INTERFACE_VERSION;
	p->pfnGetInstanceProcAddr = hook_GetInstanceProcAddr;
	p->pfnGetDeviceProcAddr = hook_GetDeviceProcAddr;
	p->pfnGetPhysicalDeviceProcAddr = NULL;
	return VK_SUCCESS;
}

VKAPI_ATTR PFN_vkVoidFunction VKAPI_CALL vkGetInstanceProcAddr(VkInstance instance, const char *name) {
	return hook_GetInstanceProcAddr(instance, name);
}

VKAPI_ATTR PFN_vkVoidFunction VKAPI_CALL vkGetDeviceProcAddr(VkDevice device, const char *name) {
	return hook_GetDeviceProcAddr(device, name);
}
