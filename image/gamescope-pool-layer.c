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
 *
 * Pitcairn has no DRM format modifiers. Nested Wayland present is a
 * dmabuf to River's radeonsi GLES. RADV SI still allocates those BOs
 * as ARRAY_2D_TILED_THIN1 (Vulkan rowPitch=width*4). River samples
 * INVALID-modifier imports as linear → repeated colored line-blocks.
 * Layer: pad pools; log CreateImage/rowPitch; clear WSI scanout
 * (kernel stays 2D); SET LINEAR_ALIGNED metadata on export (radeonsi
 * GLES still ignores it). Glass fix is importer 2D sampling or a
 * detile blit before present.
 */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/ioctl.h>

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
#define VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO 14
#define VK_IMAGE_TILING_LINEAR 0
#define VK_IMAGE_TILING_OPTIMAL 1
#define VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT 0x00000010u
#define VK_IMAGE_USAGE_SAMPLED_BIT 0x00000004u
#define VK_IMAGE_USAGE_STORAGE_BIT 0x00000008u
#define VK_IMAGE_USAGE_TRANSFER_SRC_BIT 0x00000001u
/* Mesa private: src/vulkan/wsi/wsi_common.h */
#define VK_STRUCTURE_TYPE_WSI_IMAGE_CREATE_INFO_MESA 1000001002u
#define VK_STRUCTURE_TYPE_EXTERNAL_MEMORY_IMAGE_CREATE_INFO 1000070000u

struct wsi_image_create_info {
	VkStructureType sType;
	const void *pNext;
	unsigned char scanout; /* C++ bool in Mesa/gamescope */
	unsigned char pad[3];
	uint32_t modifier_count;
	const uint64_t *modifiers;
};

struct VkExternalMemoryImageCreateInfo {
	VkStructureType sType;
	const void *pNext;
	uint32_t handleTypes;
};

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

typedef struct VkExtent3D {
	uint32_t width;
	uint32_t height;
	uint32_t depth;
} VkExtent3D;

typedef struct VkImageCreateInfo {
	VkStructureType sType;
	const void *pNext;
	uint32_t flags;
	uint32_t imageType;
	uint32_t format;
	VkExtent3D extent;
	uint32_t mipLevels;
	uint32_t arrayLayers;
	uint32_t samples;
	uint32_t tiling;
	uint32_t usage;
	uint32_t sharingMode;
	uint32_t queueFamilyIndexCount;
	const uint32_t *pQueueFamilyIndices;
	uint32_t initialLayout;
} VkImageCreateInfo;

typedef struct VkImage_T *VkImage;
typedef VkResult(VKAPI_PTR *PFN_vkCreateImage)(VkDevice, const VkImageCreateInfo *, const VkAllocationCallbacks *,
					       VkImage *);

typedef uint64_t VkDeviceSize;
typedef struct VkImageSubresource {
	uint32_t aspectMask;
	uint32_t mipLevel;
	uint32_t arrayLayer;
} VkImageSubresource;
typedef struct VkSubresourceLayout {
	VkDeviceSize offset;
	VkDeviceSize size;
	VkDeviceSize rowPitch;
	VkDeviceSize arrayPitch;
	VkDeviceSize depthPitch;
} VkSubresourceLayout;
typedef void(VKAPI_PTR *PFN_vkGetImageSubresourceLayout)(VkDevice, VkImage, const VkImageSubresource *,
							 VkSubresourceLayout *);

typedef struct VkDeviceMemory_T *VkDeviceMemory;
typedef struct VkMemoryGetFdInfoKHR {
	VkStructureType sType;
	const void *pNext;
	VkDeviceMemory memory;
	uint32_t handleType;
} VkMemoryGetFdInfoKHR;
typedef VkResult(VKAPI_PTR *PFN_vkGetMemoryFdKHR)(VkDevice, const VkMemoryGetFdInfoKHR *, int *);

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
static PFN_vkCreateImage next_create_image;
static PFN_vkGetImageSubresourceLayout next_layout;
static PFN_vkGetMemoryFdKHR next_get_fd;
static int padded_once;
static int linear_once;
static int layout_logs;
static int scanout_once;
static int create_logs;
static int meta_once;

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
	if (r == VK_SUCCESS && next_gdpa) {
		next_pool = (PFN_vkCreateDescriptorPool)next_gdpa(*out, "vkCreateDescriptorPool");
		next_create_image = (PFN_vkCreateImage)next_gdpa(*out, "vkCreateImage");
		next_layout = (PFN_vkGetImageSubresourceLayout)next_gdpa(*out, "vkGetImageSubresourceLayout");
		next_get_fd = (PFN_vkGetMemoryFdKHR)next_gdpa(*out, "vkGetMemoryFdKHR");
		fprintf(stderr, "[gamescope-pool] device created createImage=%p getFd=%p\n", (void *)next_create_image,
			(void *)next_get_fd);
	}
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

/* gamescope (modifierless) puts Mesa WSI scanout=true on flippable
 * images. Nested Wayland is not KMS; SI scanout microtiling is the
 * line-block present. Clear scanout. WSI is first pNext, or second
 * after EXTERNAL_MEMORY_IMAGE_CREATE_INFO.
 */
static const VkImageCreateInfo *strip_wsi_scanout(const VkImageCreateInfo *info, VkImageCreateInfo *local,
						  struct wsi_image_create_info *wsi_store,
						  struct VkExternalMemoryImageCreateInfo *ext_store) {
	const VkBaseInStructure *n;

	if (!info)
		return info;
	n = (const void *)info->pNext;
	if (!n)
		return info;

	if (n->sType == (int)VK_STRUCTURE_TYPE_WSI_IMAGE_CREATE_INFO_MESA) {
		memcpy(wsi_store, n, sizeof(*wsi_store));
		if (wsi_store->scanout) {
			wsi_store->scanout = 0;
			*local = *info;
			local->pNext = wsi_store;
			if (!scanout_once) {
				fprintf(stderr, "[gamescope-pool] cleared WSI scanout %ux%u tiling=%u usage=0x%x\n",
					info->extent.width, info->extent.height, info->tiling, info->usage);
				scanout_once = 1;
			}
			return local;
		}
		return info;
	}

	if (n->sType == (int)VK_STRUCTURE_TYPE_EXTERNAL_MEMORY_IMAGE_CREATE_INFO && n->pNext &&
	    n->pNext->sType == (int)VK_STRUCTURE_TYPE_WSI_IMAGE_CREATE_INFO_MESA) {
		memcpy(ext_store, n, sizeof(*ext_store));
		memcpy(wsi_store, n->pNext, sizeof(*wsi_store));
		if (wsi_store->scanout) {
			wsi_store->scanout = 0;
			ext_store->pNext = wsi_store;
			*local = *info;
			local->pNext = ext_store;
			if (!scanout_once) {
				fprintf(stderr,
					"[gamescope-pool] cleared WSI scanout (after external) %ux%u tiling=%u usage=0x%x\n",
					info->extent.width, info->extent.height, info->tiling, info->usage);
				scanout_once = 1;
			}
			return local;
		}
	}
	return info;
}

static void log_pnext(const VkImageCreateInfo *info) {
	const VkBaseInStructure *n;
	if (!info || create_logs >= 12)
		return;
	if (info->extent.width < 256 || info->extent.height < 256)
		return;
	fprintf(stderr, "[gamescope-pool] vkCreateImage %ux%u tiling=%u usage=0x%x pNext=",
		info->extent.width, info->extent.height, info->tiling, info->usage);
	for (n = (const void *)info->pNext; n; n = n->pNext)
		fprintf(stderr, " %d", (int)n->sType);
	fprintf(stderr, "\n");
	create_logs++;
}

static VkResult VKAPI_CALL hook_CreateImage(VkDevice device, const VkImageCreateInfo *info,
					    const VkAllocationCallbacks *a, VkImage *out) {
	VkImageCreateInfo local;
	struct wsi_image_create_info wsi_store;
	struct VkExternalMemoryImageCreateInfo ext_store;
	const VkImageCreateInfo *use = info;

	if (!next_create_image)
		return VK_ERROR_INITIALIZATION_FAILED;

	log_pnext(info);
	/* Nested Wayland is not KMS. WSI scanout=true makes RADV SI
	 * allocate ARRAY_2D_TILED_THIN1 + DISPLAY microtile (kernel
	 * tiling_info). Vulkan still samples that correctly (xwm
	 * screenshot is the Deck UI); River's radeonsi GLES imports
	 * INVALID modifiers as linear → line-blocks on the glass.
	 * Clear scanout so the exported BO is LINEAR_ALIGNED.
	 */
	use = strip_wsi_scanout(info, &local, &wsi_store, &ext_store);
	return next_create_image(device, use, a, out);
}

static void VKAPI_CALL hook_GetImageSubresourceLayout(VkDevice device, VkImage image, const VkImageSubresource *sub,
						      VkSubresourceLayout *layout) {
	if (next_layout)
		next_layout(device, image, sub, layout);
	if (layout && layout->rowPitch && layout_logs < 16) {
		fprintf(stderr, "[gamescope-pool] subresource offset=%llu size=%llu rowPitch=%llu\n",
			(unsigned long long)layout->offset, (unsigned long long)layout->size,
			(unsigned long long)layout->rowPitch);
		layout_logs++;
	}
}

/* SI: RADV stores LINEAR vulkan images as ARRAY_2D_TILED_THIN1 in the
 * kernel BO. River's radeonsi GLES imports INVALID modifiers; if it
 * honours GEM_METADATA it detiles linear pixels (or if it ignores
 * metadata it samples 2D as linear). Force LINEAR_ALIGNED metadata on
 * export so the importer treats the buffer as tightly packed rows.
 */
#define DRM_IOCTL_BASE 'd'
#define DRM_COMMAND_BASE 0x40
#define DRM_IOCTL_PRIME_FD_TO_HANDLE _IOWR(DRM_IOCTL_BASE, 0x2e, struct oath_drm_prime_handle)
#define DRM_AMDGPU_GEM_METADATA 0x06
#define DRM_IOCTL_AMDGPU_GEM_METADATA \
	_IOWR(DRM_IOCTL_BASE, DRM_COMMAND_BASE + DRM_AMDGPU_GEM_METADATA, struct oath_amdgpu_gem_metadata)
#define AMDGPU_GEM_METADATA_OP_SET_METADATA 1
#define AMDGPU_TILING_ARRAY_LINEAR_ALIGNED 1ull

struct oath_drm_prime_handle {
	uint32_t handle;
	uint32_t flags;
	int32_t fd;
};
struct oath_amdgpu_gem_metadata {
	uint32_t handle;
	uint32_t op;
	struct {
		uint64_t flags;
		uint64_t tiling_info;
		uint32_t data_size_bytes;
		uint32_t data[64];
	} data;
};

static void force_linear_metadata(int dmabuf_fd) {
	int drm, i;
	const char *nodes[] = { "/dev/dri/renderD128", "/dev/dri/renderD129", NULL };
	if (dmabuf_fd < 0)
		return;
	for (i = 0; nodes[i]; i++) {
		struct oath_drm_prime_handle prime;
		struct oath_amdgpu_gem_metadata md;
		drm = open(nodes[i], O_RDWR | O_CLOEXEC);
		if (drm < 0)
			continue;
		memset(&prime, 0, sizeof(prime));
		prime.fd = dmabuf_fd;
		if (ioctl(drm, DRM_IOCTL_PRIME_FD_TO_HANDLE, &prime) != 0) {
			close(drm);
			continue;
		}
		memset(&md, 0, sizeof(md));
		md.handle = prime.handle;
		md.op = AMDGPU_GEM_METADATA_OP_SET_METADATA;
		md.data.tiling_info = AMDGPU_TILING_ARRAY_LINEAR_ALIGNED;
		if (ioctl(drm, DRM_IOCTL_AMDGPU_GEM_METADATA, &md) == 0) {
			if (!meta_once) {
				fprintf(stderr, "[gamescope-pool] set LINEAR_ALIGNED metadata via %s handle=%u\n",
					nodes[i], prime.handle);
				meta_once = 1;
			}
			close(drm);
			return;
		}
		close(drm);
	}
}

static VkResult VKAPI_CALL hook_GetMemoryFdKHR(VkDevice device, const VkMemoryGetFdInfoKHR *info, int *pFd) {
	VkResult r;
	if (!next_get_fd)
		return VK_ERROR_INITIALIZATION_FAILED;
	r = next_get_fd(device, info, pFd);
	if (r == VK_SUCCESS && pFd)
		force_linear_metadata(*pFd);
	return r;
}

static PFN_vkVoidFunction VKAPI_CALL hook_GetDeviceProcAddr(VkDevice device, const char *name);
static PFN_vkVoidFunction VKAPI_CALL hook_GetInstanceProcAddr(VkInstance instance, const char *name);

static PFN_vkVoidFunction VKAPI_CALL hook_GetDeviceProcAddr(VkDevice device, const char *name) {
	if (!strcmp(name, "vkCreateDescriptorPool"))
		return (PFN_vkVoidFunction)hook_CreateDescriptorPool;
	if (!strcmp(name, "vkCreateImage"))
		return (PFN_vkVoidFunction)hook_CreateImage;
	if (!strcmp(name, "vkGetImageSubresourceLayout"))
		return (PFN_vkVoidFunction)hook_GetImageSubresourceLayout;
	if (!strcmp(name, "vkGetMemoryFdKHR"))
		return (PFN_vkVoidFunction)hook_GetMemoryFdKHR;
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
	if (!strcmp(name, "vkCreateImage"))
		return (PFN_vkVoidFunction)hook_CreateImage;
	if (!strcmp(name, "vkGetImageSubresourceLayout"))
		return (PFN_vkVoidFunction)hook_GetImageSubresourceLayout;
	if (!strcmp(name, "vkGetMemoryFdKHR"))
		return (PFN_vkVoidFunction)hook_GetMemoryFdKHR;
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
