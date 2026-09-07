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
 * as ARRAY_2D_TILED_THIN1 even when Vulkan tiling is LINEAR
 * (rowPitch=width*4). River samples INVALID-modifier imports as
 * linear → repeated colored line-blocks.
 *
 * Do not SET LINEAR_ALIGNED on a 2D BO — that makes GET lie and is
 * indistinguishable from "importer ignores metadata". GET the real
 * array mode; if tiled, detile into a GTT LINEAR_ALIGNED BO and
 * export that fd. GBM map (Mesa GPU blit) is preferred; CPU addr
 * for 1D and for 2D with bank_w=bank_h=macro=1 (Pitcairn display
 * 32bpp tile[12]).
 */
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
#include <dlfcn.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <errno.h>

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

/* Mesa 24+ wsi_common.h: two C++ bools. Do not copy past blit_src. */
struct wsi_image_create_info {
	VkStructureType sType;
	const void *pNext;
	unsigned char scanout;
	unsigned char blit_src;
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
typedef VkResult(VKAPI_PTR *PFN_vkBindImageMemory)(VkDevice, VkImage, VkDeviceMemory, VkDeviceSize);
typedef void(VKAPI_PTR *PFN_vkDestroyImage)(VkDevice, VkImage, const VkAllocationCallbacks *);

#define VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO 5
#define VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT 0x00000002u
#define VK_MEMORY_PROPERTY_HOST_COHERENT_BIT 0x00000004u

typedef struct VkMemoryAllocateInfo {
	VkStructureType sType;
	const void *pNext;
	VkDeviceSize allocationSize;
	uint32_t memoryTypeIndex;
} VkMemoryAllocateInfo;
typedef struct VkMemoryType {
	uint32_t propertyFlags;
	uint32_t heapIndex;
} VkMemoryType;
typedef struct VkMemoryHeap {
	VkDeviceSize size;
	uint32_t flags;
} VkMemoryHeap;
typedef struct VkPhysicalDeviceMemoryProperties {
	uint32_t memoryTypeCount;
	VkMemoryType memoryTypes[32];
	uint32_t memoryHeapCount;
	VkMemoryHeap memoryHeaps[16];
} VkPhysicalDeviceMemoryProperties;
typedef VkResult(VKAPI_PTR *PFN_vkAllocateMemory)(VkDevice, const VkMemoryAllocateInfo *,
						  const VkAllocationCallbacks *, VkDeviceMemory *);
typedef void(VKAPI_PTR *PFN_vkGetPhysicalDeviceMemoryProperties)(VkPhysicalDevice,
								 VkPhysicalDeviceMemoryProperties *);

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
static PFN_vkBindImageMemory next_bind_image;
static PFN_vkDestroyImage next_destroy_image;
static PFN_vkAllocateMemory next_alloc;
static PFN_vkGetPhysicalDeviceMemoryProperties next_mem_props;
static VkPhysicalDeviceMemoryProperties mem_props;
static VkInstance the_instance;
static int have_mem_props;
static int gtt_once;
static int padded_once;
static int layout_logs;
static int scanout_once;
static int create_logs;
static int meta_once;
static int detile_once;

#define MAX_TRACK 96
struct track_img {
	VkImage image;
	VkDeviceMemory memory;
	uint32_t w, h, bpp;
	uint32_t pitch;
	int in_use;
};
static struct track_img tracked[MAX_TRACK];

static struct track_img *track_find_image(VkImage image) {
	uint32_t i;
	if (!image)
		return NULL;
	for (i = 0; i < MAX_TRACK; i++)
		if (tracked[i].in_use && tracked[i].image == image)
			return &tracked[i];
	return NULL;
}

static struct track_img *track_find_memory(VkDeviceMemory memory) {
	uint32_t i;
	if (!memory)
		return NULL;
	for (i = 0; i < MAX_TRACK; i++)
		if (tracked[i].in_use && tracked[i].memory == memory)
			return &tracked[i];
	return NULL;
}

static struct track_img *track_slot(VkImage image) {
	uint32_t i;
	struct track_img *t = track_find_image(image);
	if (t)
		return t;
	for (i = 0; i < MAX_TRACK; i++) {
		if (!tracked[i].in_use) {
			memset(&tracked[i], 0, sizeof(tracked[i]));
			tracked[i].in_use = 1;
			tracked[i].image = image;
			return &tracked[i];
		}
	}
	return NULL;
}

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
	{
		VkResult r = next_create_instance(info, a, out);
		if (r == VK_SUCCESS && out)
			the_instance = *out;
		return r;
	}
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
		next_bind_image = (PFN_vkBindImageMemory)next_gdpa(*out, "vkBindImageMemory");
		next_destroy_image = (PFN_vkDestroyImage)next_gdpa(*out, "vkDestroyImage");
		next_alloc = (PFN_vkAllocateMemory)next_gdpa(*out, "vkAllocateMemory");
		if (next_gipa && !have_mem_props) {
			next_mem_props = (PFN_vkGetPhysicalDeviceMemoryProperties)next_gipa(
				the_instance, "vkGetPhysicalDeviceMemoryProperties");
			if (next_mem_props) {
				next_mem_props(phys, &mem_props);
				have_mem_props = 1;
			}
		}
		fprintf(stderr, "[gamescope-pool] device created createImage=%p getFd=%p alloc=%p host_vis_types=%u\n",
			(void *)next_create_image, (void *)next_get_fd, (void *)next_alloc,
			have_mem_props ? mem_props.memoryTypeCount : 0);
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

static uint32_t pick_host_visible_type(uint32_t orig) {
	uint32_t i;

	if (!have_mem_props || orig >= mem_props.memoryTypeCount)
		return orig;
	for (i = 0; i < mem_props.memoryTypeCount; i++) {
		uint32_t f = mem_props.memoryTypes[i].propertyFlags;
		if ((f & VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT) && (f & VK_MEMORY_PROPERTY_HOST_COHERENT_BIT))
			return i;
	}
	return orig;
}

static VkResult VKAPI_CALL hook_AllocateMemory(VkDevice device, const VkMemoryAllocateInfo *info,
					       const VkAllocationCallbacks *a, VkDeviceMemory *out) {
	VkMemoryAllocateInfo local;
	uint32_t t;

	if (!next_alloc)
		return VK_ERROR_INITIALIZATION_FAILED;
	if (!info)
		return next_alloc(device, info, a, out);
	/* SI WSI LINEAR images still land in VRAM (GEM_MMAP EPERM). Put
	 * swapchain-sized allocs in GTT so CPU detile can mmap. */
	if (have_mem_props && info->allocationSize >= 256ull * 256ull * 4ull) {
		t = pick_host_visible_type(info->memoryTypeIndex);
		if (t != info->memoryTypeIndex) {
			local = *info;
			local.memoryTypeIndex = t;
			{
				VkResult r = next_alloc(device, &local, a, out);
				if (r == VK_SUCCESS) {
					if (!gtt_once) {
						fprintf(stderr,
							"[gamescope-pool] AllocateMemory %llu bytes type %u -> %u (host visible)\n",
							(unsigned long long)info->allocationSize, info->memoryTypeIndex,
							t);
						gtt_once = 1;
					}
					return r;
				}
			}
		}
	}
	return next_alloc(device, info, a, out);
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
	 * allocate ARRAY_2D_TILED_THIN1 + DISPLAY microtile. Clear
	 * scanout so RADV is allowed to allocate linear; GET on export
	 * still decides whether we must detile.
	 */
	use = strip_wsi_scanout(info, &local, &wsi_store, &ext_store);
	{
		VkResult r = next_create_image(device, use, a, out);
		if (r == VK_SUCCESS && out && *out && use->extent.width >= 256 && use->extent.height >= 256) {
			struct track_img *t = track_slot(*out);
			if (t) {
				t->w = use->extent.width;
				t->h = use->extent.height;
				t->bpp = 4;
				t->pitch = use->extent.width * 4;
			}
		}
		return r;
	}
}

static void VKAPI_CALL hook_GetImageSubresourceLayout(VkDevice device, VkImage image, const VkImageSubresource *sub,
						      VkSubresourceLayout *layout) {
	if (next_layout)
		next_layout(device, image, sub, layout);
	if (layout && layout->rowPitch) {
		struct track_img *t = track_find_image(image);
		if (t)
			t->pitch = (uint32_t)layout->rowPitch;
		if (layout_logs < 16) {
			fprintf(stderr, "[gamescope-pool] subresource offset=%llu size=%llu rowPitch=%llu\n",
				(unsigned long long)layout->offset, (unsigned long long)layout->size,
				(unsigned long long)layout->rowPitch);
			layout_logs++;
		}
	}
}

static VkResult VKAPI_CALL hook_BindImageMemory(VkDevice device, VkImage image, VkDeviceMemory memory,
						VkDeviceSize offset) {
	VkResult r;
	if (!next_bind_image)
		return VK_ERROR_INITIALIZATION_FAILED;
	r = next_bind_image(device, image, memory, offset);
	if (r == VK_SUCCESS) {
		struct track_img *t = track_find_image(image);
		if (t)
			t->memory = memory;
	}
	return r;
}

static void VKAPI_CALL hook_DestroyImage(VkDevice device, VkImage image, const VkAllocationCallbacks *a) {
	struct track_img *t = track_find_image(image);
	if (t)
		t->in_use = 0;
	if (next_destroy_image)
		next_destroy_image(device, image, a);
}

#define DRM_IOCTL_BASE 'd'
#define DRM_COMMAND_BASE 0x40
#define DRM_IOCTL_PRIME_HANDLE_TO_FD _IOWR(DRM_IOCTL_BASE, 0x2d, struct oath_drm_prime_handle)
#define DRM_IOCTL_PRIME_FD_TO_HANDLE _IOWR(DRM_IOCTL_BASE, 0x2e, struct oath_drm_prime_handle)
#define DRM_AMDGPU_GEM_CREATE 0x00
#define DRM_AMDGPU_GEM_MMAP 0x01
#define DRM_AMDGPU_GEM_METADATA 0x06
#define DRM_IOCTL_AMDGPU_GEM_CREATE \
	_IOWR(DRM_IOCTL_BASE, DRM_COMMAND_BASE + DRM_AMDGPU_GEM_CREATE, union oath_amdgpu_gem_create)
#define DRM_IOCTL_AMDGPU_GEM_MMAP \
	_IOWR(DRM_IOCTL_BASE, DRM_COMMAND_BASE + DRM_AMDGPU_GEM_MMAP, union oath_amdgpu_gem_mmap)
#define DRM_IOCTL_AMDGPU_GEM_METADATA \
	_IOWR(DRM_IOCTL_BASE, DRM_COMMAND_BASE + DRM_AMDGPU_GEM_METADATA, struct oath_amdgpu_gem_metadata)
#define AMDGPU_GEM_METADATA_OP_SET_METADATA 1
#define AMDGPU_GEM_METADATA_OP_GET_METADATA 2
#define AMDGPU_TILING_ARRAY_LINEAR_GENERAL 0ull
#define AMDGPU_TILING_ARRAY_LINEAR_ALIGNED 1ull
#define AMDGPU_TILING_ARRAY_1D_TILED_THIN1 2ull
#define AMDGPU_TILING_ARRAY_2D_TILED_THIN1 4ull
#define AMDGPU_GEM_DOMAIN_GTT 0x2ull
#define AMDGPU_GEM_CREATE_CPU_ACCESS_REQUIRED (1ull << 0)
#define DRM_CLOEXEC 0x00080000u
#define DRM_RDWR 0x00000002u

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
union oath_amdgpu_gem_create {
	struct {
		uint64_t bo_size;
		uint64_t alignment;
		uint64_t domains;
		uint64_t domain_flags;
	} in;
	struct {
		uint32_t handle;
		uint32_t _pad;
	} out;
};
union oath_amdgpu_gem_mmap {
	struct {
		uint32_t handle;
		uint32_t _pad;
	} in;
	struct {
		uint64_t addr_ptr;
	} out;
};

struct dma_buf_sync {
	uint64_t flags;
};
#define DMA_BUF_SYNC_READ (1ull << 0)
#define DMA_BUF_SYNC_WRITE (1ull << 1)
#define DMA_BUF_SYNC_RW (DMA_BUF_SYNC_READ | DMA_BUF_SYNC_WRITE)
#define DMA_BUF_SYNC_START (0ull << 2)
#define DMA_BUF_SYNC_END (1ull << 2)
#define DMA_BUF_BASE 'b'
#define DMA_BUF_IOCTL_SYNC _IOW(DMA_BUF_BASE, 0, struct dma_buf_sync)

static int open_render_node(void) {
	int fd = open("/dev/dri/renderD128", O_RDWR | O_CLOEXEC);
	if (fd < 0)
		fd = open("/dev/dri/renderD129", O_RDWR | O_CLOEXEC);
	return fd;
}

static int gem_metadata(int drm, int dmabuf_fd, uint32_t op, uint64_t *tiling_io) {
	struct oath_drm_prime_handle prime;
	struct oath_amdgpu_gem_metadata md;
	memset(&prime, 0, sizeof(prime));
	prime.fd = dmabuf_fd;
	if (ioctl(drm, DRM_IOCTL_PRIME_FD_TO_HANDLE, &prime) != 0)
		return -1;
	memset(&md, 0, sizeof(md));
	md.handle = prime.handle;
	md.op = op;
	if (op == AMDGPU_GEM_METADATA_OP_SET_METADATA)
		md.data.tiling_info = *tiling_io;
	if (ioctl(drm, DRM_IOCTL_AMDGPU_GEM_METADATA, &md) != 0)
		return -1;
	if (op == AMDGPU_GEM_METADATA_OP_GET_METADATA)
		*tiling_io = md.data.tiling_info;
	return 0;
}

static uint32_t si_num_pipes(uint32_t hw_pipe) {
	if (hw_pipe == 0)
		return 2;
	if (hw_pipe >= 4 && hw_pipe <= 7)
		return 4;
	if (hw_pipe >= 8 && hw_pipe <= 14)
		return 8;
	if (hw_pipe >= 16)
		return 16;
	return 8;
}

static uint32_t si_pipe_from_coord(uint32_t x, uint32_t y, uint32_t hw_pipe) {
	uint32_t tx = x / 8, ty = y / 8;
	uint32_t x3 = tx & 1, x4 = (tx >> 1) & 1, x5 = (tx >> 2) & 1;
	uint32_t y3 = ty & 1, y4 = (ty >> 1) & 1, y5 = (ty >> 2) & 1;
	uint32_t p0 = 0, p1 = 0, p2 = 0, n;
	switch (hw_pipe) {
	case 0:
		p0 = x3 ^ y3;
		n = 2;
		break;
	case 10: /* P8_32x32_8x16 — Pitcairn */
		p0 = x4 ^ y3 ^ x5;
		p1 = x3 ^ y4;
		p2 = x5 ^ y5;
		n = 8;
		break;
	case 4: /* P4_8x16 */
		p0 = x4 ^ y3;
		p1 = x3 ^ y4;
		n = 4;
		break;
	default:
		p0 = x4 ^ y3 ^ x5;
		p1 = x3 ^ y4;
		p2 = x5 ^ y5;
		n = si_num_pipes(hw_pipe);
		break;
	}
	return (p0 | (p1 << 1) | (p2 << 2)) & (n - 1);
}

static uint32_t si_bank_from_coord(uint32_t x, uint32_t y, uint32_t num_banks, uint32_t bank_w, uint32_t bank_h) {
	uint32_t tx = (x / 8) / (bank_w ? bank_w : 1);
	uint32_t ty = (y / 8) / (bank_h ? bank_h : 1);
	uint32_t x3 = tx & 1, x4 = (tx >> 1) & 1, x5 = (tx >> 2) & 1;
	uint32_t y3 = ty & 1, y4 = (ty >> 1) & 1, y5 = (ty >> 2) & 1, y6 = (ty >> 3) & 1;
	uint32_t bank;
	switch (num_banks) {
	case 2:
		bank = x3 ^ y3;
		break;
	case 4:
		bank = (x3 ^ y4) | ((x4 ^ y3) << 1);
		break;
	case 8:
		bank = (x4 ^ y5) | ((x3 ^ y4) << 1) | ((x5 ^ y3) << 2);
		break;
	default:
		bank = (x3 ^ y6) | ((x4 ^ y5) << 1) | ((x5 ^ y4) << 2) | ((x5 ^ y3) << 3);
		break;
	}
	return bank & (num_banks - 1);
}

static uint32_t si_micro_index_32(uint32_t x, uint32_t y, int display) {
	uint32_t x0 = x & 1, x1 = (x >> 1) & 1, x2 = (x >> 2) & 1;
	uint32_t y0 = y & 1, y1 = (y >> 1) & 1, y2 = (y >> 2) & 1;
	if (display)
		return x0 | (x1 << 1) | (x2 << 2) | (y1 << 3) | (y0 << 4) | (y2 << 5);
	return x0 | (y0 << 1) | (x1 << 2) | (y1 << 3) | (x2 << 4) | (y2 << 5);
}

static uint64_t si_addr_32(uint32_t x, uint32_t y, uint32_t pitch_px, uint64_t tiling) {
	uint32_t array_mode = (uint32_t)((tiling >> 0) & 0xf);
	uint32_t hw_pipe = (uint32_t)((tiling >> 4) & 0x1f);
	uint32_t micro = (uint32_t)((tiling >> 12) & 0x7);
	uint32_t bank_w = 1u << ((tiling >> 16) & 0x3);
	uint32_t bank_h = 1u << ((tiling >> 18) & 0x3);
	uint32_t mtilea = 1u << ((tiling >> 20) & 0x3);
	uint32_t num_banks = 2u << ((tiling >> 22) & 0x3);
	uint32_t pix = si_micro_index_32(x, y, micro == 0);
	uint32_t num_pipes, pipe, bank, mtile_w, mtile_h, pitch_mt;
	uint64_t mtile_n;

	if (array_mode == AMDGPU_TILING_ARRAY_1D_TILED_THIN1) {
		uint32_t mx = x / 8, my = y / 8;
		uint32_t pitch_m = pitch_px / 8;
		return ((uint64_t)my * pitch_m + mx) * 256ull + (uint64_t)pix * 4ull;
	}

	num_pipes = si_num_pipes(hw_pipe);
	pipe = si_pipe_from_coord(x, y, hw_pipe);
	bank = si_bank_from_coord(x, y, num_banks, bank_w, bank_h);
	mtile_w = 8 * bank_w * num_pipes * mtilea;
	mtile_h = 8 * bank_h * num_banks / (mtilea ? mtilea : 1);
	if (!mtile_w)
		mtile_w = 8;
	if (!mtile_h)
		mtile_h = 8;
	pitch_mt = pitch_px / mtile_w;
	if (!pitch_mt)
		pitch_mt = 1;
	mtile_n = (uint64_t)(y / mtile_h) * pitch_mt + (x / mtile_w);
	/* bank_w=bank_h=macro=1 → one microtile per pipe×bank (Pitcairn display 32). */
	return (uint64_t)pix * 4ull + (uint64_t)pipe * 256ull + (uint64_t)bank * 256ull * num_pipes +
	       mtile_n * 256ull * num_pipes * num_banks;
}

static void dma_sync(int fd, uint64_t flags) {
	struct dma_buf_sync s;
	s.flags = flags;
	ioctl(fd, DMA_BUF_IOCTL_SYNC, &s);
}

static int create_linear_dmabuf(int drm, size_t size, uint64_t *mmap_off, uint32_t *handle_out) {
	union oath_amdgpu_gem_create cr;
	union oath_amdgpu_gem_mmap mm;
	struct oath_drm_prime_handle prime;
	memset(&cr, 0, sizeof(cr));
	cr.in.bo_size = size;
	cr.in.alignment = 4096;
	cr.in.domains = AMDGPU_GEM_DOMAIN_GTT;
	cr.in.domain_flags = AMDGPU_GEM_CREATE_CPU_ACCESS_REQUIRED;
	if (ioctl(drm, DRM_IOCTL_AMDGPU_GEM_CREATE, &cr) != 0)
		return -1;
	{
		struct oath_amdgpu_gem_metadata md;
		memset(&md, 0, sizeof(md));
		md.handle = cr.out.handle;
		md.op = AMDGPU_GEM_METADATA_OP_SET_METADATA;
		md.data.tiling_info = AMDGPU_TILING_ARRAY_LINEAR_ALIGNED;
		ioctl(drm, DRM_IOCTL_AMDGPU_GEM_METADATA, &md);
	}
	memset(&mm, 0, sizeof(mm));
	mm.in.handle = cr.out.handle;
	if (ioctl(drm, DRM_IOCTL_AMDGPU_GEM_MMAP, &mm) != 0)
		return -1;
	memset(&prime, 0, sizeof(prime));
	prime.handle = cr.out.handle;
	prime.flags = DRM_CLOEXEC | DRM_RDWR;
	if (ioctl(drm, DRM_IOCTL_PRIME_HANDLE_TO_FD, &prime) != 0)
		return -1;
	*mmap_off = mm.out.addr_ptr;
	*handle_out = cr.out.handle;
	return prime.fd;
}

struct gbm_device;

static int gbm_detile_copy(int drm, int src_fd, uint32_t w, uint32_t h, uint32_t pitch, uint8_t *dst,
			   uint32_t dst_pitch, uint32_t dest_w, uint32_t dest_h) {
	static void *gbm;
	static struct gbm_device *(*create_dev)(int);
	static void *(*bo_import)(struct gbm_device *, uint32_t, void *, uint32_t);
	static void *(*bo_map)(void *, uint32_t, uint32_t, uint32_t, uint32_t, uint32_t, uint32_t *, void **);
	static void (*bo_unmap)(void *, void *);
	static void (*bo_destroy)(void *);
	static void (*dev_destroy)(struct gbm_device *);
	static int gbm_log;
	static struct gbm_device *dev;
	static int dev_drm = -1;
	void *bo, *map_data = NULL, *p;
	uint32_t map_stride = 0;
	uint32_t y;
	struct {
		int fd;
		uint32_t width, height, stride, format;
	} imp;
	if (!gbm) {
		gbm = dlopen("libgbm.so.1", RTLD_NOW | RTLD_LOCAL);
		if (!gbm) {
			if (!gbm_log)
				fprintf(stderr, "[gamescope-pool] dlopen libgbm.so.1: %s\n", dlerror());
			gbm_log = 1;
			return -1;
		}
		create_dev = (void *)dlsym(gbm, "gbm_create_device");
		bo_import = (void *)dlsym(gbm, "gbm_bo_import");
		bo_map = (void *)dlsym(gbm, "gbm_bo_map");
		bo_unmap = (void *)dlsym(gbm, "gbm_bo_unmap");
		bo_destroy = (void *)dlsym(gbm, "gbm_bo_destroy");
		dev_destroy = (void *)dlsym(gbm, "gbm_device_destroy");
		if (!create_dev || !bo_import || !bo_map || !bo_unmap || !bo_destroy) {
			fprintf(stderr, "[gamescope-pool] libgbm missing symbols\n");
			return -1;
		}
	}
	if (!dev) {
		dev_drm = dup(drm);
		if (dev_drm < 0)
			dev_drm = open_render_node();
		dev = create_dev(dev_drm);
		if (!dev) {
			if (!gbm_log)
				fprintf(stderr, "[gamescope-pool] gbm_create_device failed errno=%d\n", errno);
			gbm_log = 1;
			return -1;
		}
	}
	imp.fd = src_fd;
	imp.width = w;
	imp.height = h;
	imp.stride = pitch;
	imp.format = 0x34325241u; /* ARGB8888 */
	/* Implicit FD import treats INVALID as linear. Ask the driver to
	 * read GEM_METADATA (2D thin) via the modifier token. */
	{
		struct {
			uint32_t width, height, format, num_fds;
			int fds[4];
			int strides[4];
			int offsets[4];
			uint64_t modifier;
		} mod;
		memset(&mod, 0, sizeof(mod));
		mod.width = w;
		mod.height = h;
		mod.format = 0x34325241u;
		mod.num_fds = 1;
		mod.fds[0] = src_fd;
		mod.strides[0] = (int)pitch;
		mod.modifier = 0x00ffffffffffffffull; /* DRM_FORMAT_MOD_INVALID */
		bo = bo_import(dev, 0x5504u, &mod, 4u); /* GBM_BO_IMPORT_FD_MODIFIER */
		if (!bo) {
			mod.format = 0x34325258u;
			bo = bo_import(dev, 0x5504u, &mod, 4u);
		}
	}
	if (!bo)
		bo = bo_import(dev, 0x5503u, &imp, 4u);
	if (!bo) {
		imp.format = 0x34325258u;
		bo = bo_import(dev, 0x5503u, &imp, 4u);
	}
	if (!bo) {
		if (!gbm_log)
			fprintf(stderr, "[gamescope-pool] gbm_bo_import failed errno=%d w=%u h=%u stride=%u\n", errno, w,
				h, pitch);
		gbm_log = 1;
		return -1;
	}
	p = bo_map(bo, 0, 0, w, h, 1u, &map_stride, &map_data);
	if (!p) {
		if (!gbm_log)
			fprintf(stderr, "[gamescope-pool] gbm_bo_map failed errno=%d\n", errno);
		gbm_log = 1;
		bo_destroy(bo);
		return -1;
	}
	if (!map_stride)
		map_stride = pitch;
	if (dest_w == 0 || dest_w > w)
		dest_w = w;
	if (dest_h == 0 || dest_h > h)
		dest_h = h;
	for (y = 0; y < dest_h; y++)
		memcpy(dst + (size_t)y * dst_pitch, (const uint8_t *)p + (size_t)y * map_stride, (size_t)dest_w * 4);
	bo_unmap(bo, map_data);
	bo_destroy(bo);
	(void)dev_destroy;
	return 0;
}

static void cpu_detile(const uint8_t *src, size_t src_len, uint32_t w, uint32_t h, uint32_t pitch, uint64_t tiling,
		       uint8_t *dst, uint32_t dst_pitch) {
	uint32_t x, y;
	uint32_t array_mode = (uint32_t)((tiling >> 0) & 0xf);
	for (y = 0; y < h; y++) {
		for (x = 0; x < w; x++) {
			uint64_t off;
			if (array_mode == AMDGPU_TILING_ARRAY_1D_TILED_THIN1 ||
			    array_mode == AMDGPU_TILING_ARRAY_2D_TILED_THIN1)
				off = si_addr_32(x, y, pitch / 4, tiling);
			else
				off = (uint64_t)y * pitch + (uint64_t)x * 4;
			if (off + 4 > src_len)
				continue;
			memcpy(dst + (size_t)y * dst_pitch + (size_t)x * 4, src + off, 4);
		}
	}
}

#define LINEAR_SLOTS 4
struct linear_slot {
	int fd;
	int drm;
	uint32_t w, h, pitch;
	size_t map_len;
	void *map;
};
static struct linear_slot lin_slots[LINEAR_SLOTS];
static unsigned lin_next;
static int lin_inited;

static struct linear_slot *linear_slot_get(uint32_t w, uint32_t h) {
	struct linear_slot *s;
	uint64_t mmap_off = 0;
	uint32_t handle = 0;
	uint32_t dst_pitch = (w * 4 + 255u) & ~255u;
	size_t dst_len = (size_t)dst_pitch * h;
	int drm, fd;
	void *map;
	unsigned i;

	if (!lin_inited) {
		for (i = 0; i < LINEAR_SLOTS; i++) {
			lin_slots[i].fd = -1;
			lin_slots[i].drm = -1;
		}
		lin_inited = 1;
	}
	if (dst_len < 4096)
		dst_len = 4096;
	for (i = 0; i < LINEAR_SLOTS; i++) {
		s = &lin_slots[(lin_next + i) % LINEAR_SLOTS];
		if (s->fd >= 0 && s->map && s->w == w && s->h == h && s->pitch == dst_pitch) {
			lin_next = (unsigned)((s - lin_slots) + 1);
			return s;
		}
	}
	drm = open_render_node();
	if (drm < 0)
		return NULL;
	fd = create_linear_dmabuf(drm, dst_len, &mmap_off, &handle);
	if (fd < 0) {
		close(drm);
		return NULL;
	}
	map = mmap(NULL, dst_len, PROT_READ | PROT_WRITE, MAP_SHARED, drm, (off_t)mmap_off);
	if (map == MAP_FAILED)
		map = mmap(NULL, dst_len, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
	if (map == MAP_FAILED) {
		close(fd);
		close(drm);
		return NULL;
	}
	s = &lin_slots[lin_next % LINEAR_SLOTS];
	lin_next++;
	if (s->map)
		munmap(s->map, s->map_len);
	if (s->fd >= 0)
		close(s->fd);
	if (s->drm >= 0)
		close(s->drm);
	s->fd = fd;
	s->drm = drm;
	s->w = w;
	s->h = h;
	s->pitch = dst_pitch;
	s->map_len = dst_len;
	s->map = map;
	return s;
}

static int detile_dmabuf(int src_fd, uint32_t w, uint32_t h, uint32_t pitch, uint64_t tiling) {
	struct linear_slot *s;
	uint32_t import_w, import_h;
	size_t src_len;
	int used_gbm = 0;
	int out;
	uint32_t mtile_w, pitch_px;
	uint8_t *src;

	if (w == 0 || h == 0 || pitch == 0)
		return -1;
	src_len = (size_t)lseek(src_fd, 0, SEEK_END);
	if (src_len == (size_t)-1 || src_len == 0)
		src_len = (size_t)pitch * h;
	/* WSI often GetMemoryFdKHR before GetImageSubresourceLayout, so
	 * tracked pitch is still w*4. SI pitch is 256-byte aligned. */
	if (pitch < ((w * 4 + 255u) & ~255u))
		pitch = (w * 4 + 255u) & ~255u;
	import_w = pitch / 4;
	if (import_w < w)
		import_w = w;
	import_h = h;
	if (pitch && src_len / pitch > h)
		import_h = (uint32_t)(src_len / pitch);

	s = linear_slot_get(w, h);
	if (!s) {
		fprintf(stderr, "[gamescope-pool] linear slot errno=%d\n", errno);
		return -1;
	}
	dma_sync(s->fd, DMA_BUF_SYNC_START | DMA_BUF_SYNC_WRITE);
	/* Prefer GEM_MMAP + CPU detile so we do not load radeonsi via GBM
	 * in the RADV process (that OOMs Pitcairn 2GB on remake). */
	src = MAP_FAILED;
	{
		struct oath_drm_prime_handle prime;
		union oath_amdgpu_gem_mmap mm;
		memset(&prime, 0, sizeof(prime));
		prime.fd = src_fd;
		if (ioctl(s->drm, DRM_IOCTL_PRIME_FD_TO_HANDLE, &prime) == 0) {
			memset(&mm, 0, sizeof(mm));
			mm.in.handle = prime.handle;
			if (ioctl(s->drm, DRM_IOCTL_AMDGPU_GEM_MMAP, &mm) == 0)
				src = mmap(NULL, src_len, PROT_READ, MAP_SHARED, s->drm, (off_t)mm.out.addr_ptr);
			else if (!detile_once)
				fprintf(stderr, "[gamescope-pool] GEM_MMAP errno=%d\n", errno);
		} else if (!detile_once)
			fprintf(stderr, "[gamescope-pool] PRIME_FD_TO_HANDLE errno=%d\n", errno);
	}
	if (src == MAP_FAILED)
		src = mmap(NULL, src_len, PROT_READ, MAP_SHARED, src_fd, 0);
	if (src != MAP_FAILED) {
		mtile_w = 8u * (1u << (unsigned)((tiling >> 16) & 0x3)) * si_num_pipes((uint32_t)((tiling >> 4) & 0x1f)) *
			  (1u << (unsigned)((tiling >> 20) & 0x3));
		pitch_px = pitch / 4;
		if (mtile_w && (pitch_px % mtile_w))
			pitch_px = (pitch_px + mtile_w - 1) / mtile_w * mtile_w;
		cpu_detile(src, src_len, w, h, pitch_px * 4, tiling, s->map, s->pitch);
		munmap(src, src_len);
		used_gbm = 0;
	} else {
		used_gbm = gbm_detile_copy(s->drm, src_fd, import_w, import_h, pitch, s->map, s->pitch, w, h) == 0;
		if (!used_gbm)
			used_gbm = gbm_detile_copy(s->drm, src_fd, w, h, pitch, s->map, s->pitch, w, h) == 0;
		if (!used_gbm) {
			fprintf(stderr, "[gamescope-pool] mmap src errno=%d gbm failed %ux%u len=%zu\n", errno, w, h,
				src_len);
			dma_sync(s->fd, DMA_BUF_SYNC_END | DMA_BUF_SYNC_WRITE);
			return -1;
		}
	}
	dma_sync(s->fd, DMA_BUF_SYNC_END | DMA_BUF_SYNC_WRITE);
	out = dup(s->fd);
	if (!detile_once) {
		fprintf(stderr, "[gamescope-pool] detile %ux%u import=%ux%u pitch=%u tiling=0x%llx via %s\n", w, h,
			import_w, import_h, pitch, (unsigned long long)tiling, used_gbm ? "gbm" : "cpu");
		detile_once = 1;
	}
	return out;
}

static VkResult VKAPI_CALL hook_GetMemoryFdKHR(VkDevice device, const VkMemoryGetFdInfoKHR *info, int *pFd) {
	VkResult r;
	int drm;
	uint64_t tiling = 0;
	uint32_t array_mode;
	struct track_img *t;
	if (!next_get_fd)
		return VK_ERROR_INITIALIZATION_FAILED;
	r = next_get_fd(device, info, pFd);
	if (r != VK_SUCCESS || !pFd || *pFd < 0)
		return r;
	{
		const char *nodes[] = { "/dev/dri/renderD128", "/dev/dri/renderD129", NULL };
		int i, got = 0;
		for (i = 0; nodes[i]; i++) {
			drm = open(nodes[i], O_RDWR | O_CLOEXEC);
			if (drm < 0)
				continue;
			if (gem_metadata(drm, *pFd, AMDGPU_GEM_METADATA_OP_GET_METADATA, &tiling) == 0)
				got = 1;
			close(drm);
			if (got)
				break;
		}
		if (!got)
			return r;
	}
	array_mode = (uint32_t)((tiling >> 0) & 0xf);
	t = info ? track_find_memory(info->memory) : NULL;
	if (!meta_once) {
		fprintf(stderr,
			"[gamescope-pool] export GET tiling=0x%llx array_mode=%u pipe=%u micro=%u bank_w=%u bank_h=%u "
			"mtilea=%u banks=%u %ux%u pitch=%u\n",
			(unsigned long long)tiling, array_mode, (unsigned)((tiling >> 4) & 0x1f),
			(unsigned)((tiling >> 12) & 0x7), 1u << (unsigned)((tiling >> 16) & 0x3),
			1u << (unsigned)((tiling >> 18) & 0x3), 1u << (unsigned)((tiling >> 20) & 0x3),
			2u << (unsigned)((tiling >> 22) & 0x3), t ? t->w : 0, t ? t->h : 0, t ? t->pitch : 0);
		meta_once = 1;
	}
	if (array_mode > AMDGPU_TILING_ARRAY_LINEAR_ALIGNED && t && t->w && t->h) {
		int linear_fd = detile_dmabuf(*pFd, t->w, t->h, t->pitch ? t->pitch : t->w * 4, tiling);
		if (linear_fd >= 0) {
			close(*pFd);
			*pFd = linear_fd;
		} else if (!detile_once) {
			fprintf(stderr, "[gamescope-pool] detile failed errno=%d, presenting tiled fd\n", errno);
			detile_once = 1;
		}
	}
	return r;
}

static PFN_vkVoidFunction VKAPI_CALL hook_GetDeviceProcAddr(VkDevice device, const char *name);
static PFN_vkVoidFunction VKAPI_CALL hook_GetInstanceProcAddr(VkInstance instance, const char *name);

static PFN_vkVoidFunction VKAPI_CALL hook_GetDeviceProcAddr(VkDevice device, const char *name) {
	if (!strcmp(name, "vkCreateDescriptorPool"))
		return (PFN_vkVoidFunction)hook_CreateDescriptorPool;
	if (!strcmp(name, "vkAllocateMemory"))
		return (PFN_vkVoidFunction)hook_AllocateMemory;
	if (!strcmp(name, "vkCreateImage"))
		return (PFN_vkVoidFunction)hook_CreateImage;
	if (!strcmp(name, "vkGetImageSubresourceLayout"))
		return (PFN_vkVoidFunction)hook_GetImageSubresourceLayout;
	if (!strcmp(name, "vkGetMemoryFdKHR"))
		return (PFN_vkVoidFunction)hook_GetMemoryFdKHR;
	if (!strcmp(name, "vkBindImageMemory"))
		return (PFN_vkVoidFunction)hook_BindImageMemory;
	if (!strcmp(name, "vkDestroyImage"))
		return (PFN_vkVoidFunction)hook_DestroyImage;
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
	if (!strcmp(name, "vkAllocateMemory"))
		return (PFN_vkVoidFunction)hook_AllocateMemory;
	if (!strcmp(name, "vkCreateImage"))
		return (PFN_vkVoidFunction)hook_CreateImage;
	if (!strcmp(name, "vkGetImageSubresourceLayout"))
		return (PFN_vkVoidFunction)hook_GetImageSubresourceLayout;
	if (!strcmp(name, "vkGetMemoryFdKHR"))
		return (PFN_vkVoidFunction)hook_GetMemoryFdKHR;
	if (!strcmp(name, "vkBindImageMemory"))
		return (PFN_vkVoidFunction)hook_BindImageMemory;
	if (!strcmp(name, "vkDestroyImage"))
		return (PFN_vkVoidFunction)hook_DestroyImage;
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
