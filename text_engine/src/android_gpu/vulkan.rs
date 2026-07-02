use super::{ANativeWindow, ANativeWindow_release};
use ash::{
    extensions::khr::{AndroidSurface, Surface as KhrSurface, Swapchain},
    vk,
    vk::Handle as AshHandle,
    Entry,
};
use libc::c_void;
use skia_safe::{
    gpu::{
        self, backend_render_targets, direct_contexts, surfaces, vk as skia_vk, FlushInfo,
        SubmitInfo, SurfaceOrigin, SyncCpu,
    },
    Color4f, ColorType, Surface,
};
use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
    ptr,
};

pub(super) struct AndroidVulkanRenderer {
    window: *mut ANativeWindow,
    _entry: Entry,
    instance: ash::Instance,
    surface_loader: KhrSurface,
    surface: vk::SurfaceKHR,
    device: ash::Device,
    queue: vk::Queue,
    queue_family_index: u32,
    swapchain_loader: Swapchain,
    swapchain: vk::SwapchainKHR,
    swapchain_format: vk::Format,
    color_type: ColorType,
    extent: vk::Extent2D,
    images: Vec<vk::Image>,
    acquire_fence: vk::Fence,
    direct_context: Option<gpu::DirectContext>,
}

impl AndroidVulkanRenderer {
    /// Build a Vulkan/Skia renderer from an acquired `ANativeWindow`.
    ///
    /// The caller keeps ownership on failure and transfers ownership on success.
    pub(super) unsafe fn from_native_window(
        window: *mut ANativeWindow,
        width: u32,
        height: u32,
    ) -> Result<Self, &'static str> {
        let entry = Entry::load().map_err(|_| "Vulkan loader unavailable")?;
        let app_name = CString::new("lyrics-ui").map_err(|_| "invalid Vulkan app name")?;
        let app_info = vk::ApplicationInfo::builder()
            .application_name(&app_name)
            .application_version(1)
            .engine_name(&app_name)
            .engine_version(1)
            .api_version(vk::API_VERSION_1_1);

        let instance_extensions = [KhrSurface::name().as_ptr(), AndroidSurface::name().as_ptr()];
        let instance_create_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_extension_names(&instance_extensions);
        let instance = entry
            .create_instance(&instance_create_info, None)
            .map_err(|_| "vkCreateInstance failed")?;

        let android_surface_loader = AndroidSurface::new(&entry, &instance);
        let surface_create_info =
            vk::AndroidSurfaceCreateInfoKHR::builder().window(window as *mut vk::ANativeWindow);
        let surface =
            match android_surface_loader.create_android_surface(&surface_create_info, None) {
                Ok(surface) => surface,
                Err(_) => {
                    instance.destroy_instance(None);
                    return Err("vkCreateAndroidSurfaceKHR failed");
                }
            };
        let surface_loader = KhrSurface::new(&entry, &instance);

        let (physical_device, queue_family_index) =
            match select_physical_device(&instance, &surface_loader, surface) {
                Ok(selection) => selection,
                Err(error) => {
                    surface_loader.destroy_surface(surface, None);
                    instance.destroy_instance(None);
                    return Err(error);
                }
            };

        let queue_priority = [1.0f32];
        let queue_create_infos = [vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(queue_family_index)
            .queue_priorities(&queue_priority)
            .build()];
        let device_extensions = [Swapchain::name().as_ptr()];
        let device_create_info = vk::DeviceCreateInfo::builder()
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(&device_extensions);
        let device = match instance.create_device(physical_device, &device_create_info, None) {
            Ok(device) => device,
            Err(_) => {
                surface_loader.destroy_surface(surface, None);
                instance.destroy_instance(None);
                return Err("vkCreateDevice failed");
            }
        };
        let queue = device.get_device_queue(queue_family_index, 0);
        let swapchain_loader = Swapchain::new(&instance, &device);

        let swapchain_bundle = match create_swapchain(
            &surface_loader,
            &swapchain_loader,
            physical_device,
            surface,
            width,
            height,
        ) {
            Ok(bundle) => bundle,
            Err(error) => {
                device.destroy_device(None);
                surface_loader.destroy_surface(surface, None);
                instance.destroy_instance(None);
                return Err(error);
            }
        };

        let acquire_fence_info = vk::FenceCreateInfo::default();
        let acquire_fence = match device.create_fence(&acquire_fence_info, None) {
            Ok(fence) => fence,
            Err(_) => {
                swapchain_loader.destroy_swapchain(swapchain_bundle.swapchain, None);
                device.destroy_device(None);
                surface_loader.destroy_surface(surface, None);
                instance.destroy_instance(None);
                return Err("vkCreateFence failed");
            }
        };

        let direct_context = match make_skia_direct_context(
            &entry,
            &instance,
            &device,
            physical_device,
            queue,
            queue_family_index,
        ) {
            Some(context) => context,
            None => {
                device.destroy_fence(acquire_fence, None);
                swapchain_loader.destroy_swapchain(swapchain_bundle.swapchain, None);
                device.destroy_device(None);
                surface_loader.destroy_surface(surface, None);
                instance.destroy_instance(None);
                return Err("Skia Vulkan context failed");
            }
        };

        Ok(Self {
            window,
            _entry: entry,
            instance,
            surface_loader,
            surface,
            device,
            queue,
            queue_family_index,
            swapchain_loader,
            swapchain: swapchain_bundle.swapchain,
            swapchain_format: swapchain_bundle.format,
            color_type: swapchain_bundle.color_type,
            extent: swapchain_bundle.extent,
            images: swapchain_bundle.images,
            acquire_fence,
            direct_context: Some(direct_context),
        })
    }

    pub(super) fn draw_frame<F>(&mut self, draw: F) -> Result<(), &'static str>
    where
        F: FnOnce(&skia_safe::Canvas),
    {
        let (image_index, acquire_suboptimal) = match unsafe {
            self.swapchain_loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                vk::Semaphore::null(),
                self.acquire_fence,
            )
        } {
            Ok(result) => result,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Err("Vulkan swapchain out of date"),
            Err(_) => return Err("vkAcquireNextImageKHR failed"),
        };

        unsafe {
            self.device
                .wait_for_fences(&[self.acquire_fence], true, u64::MAX)
                .map_err(|_| "vkWaitForFences failed")?;
            self.device
                .reset_fences(&[self.acquire_fence])
                .map_err(|_| "vkResetFences failed")?;
        }

        let image = *self
            .images
            .get(image_index as usize)
            .ok_or("invalid Vulkan swapchain image index")?;
        let extent = self.extent;
        let swapchain_format = self.swapchain_format;
        let color_type = self.color_type;
        let queue_family_index = self.queue_family_index;
        let direct_context = self
            .direct_context
            .as_mut()
            .ok_or("missing Skia Vulkan direct context")?;
        let mut surface = surface_for_image(
            direct_context,
            image,
            extent,
            swapchain_format,
            color_type,
            queue_family_index,
        )?;

        {
            let canvas = surface.canvas();
            canvas.clear(Color4f::new(0.0, 0.0, 0.0, 0.0));
            draw(canvas);
        }

        let flush_info = FlushInfo::default();
        let present_state = skia_vk::mutable_texture_states::new_vulkan(
            skia_vk::ImageLayout::PRESENT_SRC_KHR,
            self.queue_family_index,
        );
        direct_context.flush_surface_with_texture_state(
            &mut surface,
            &flush_info,
            Some(&present_state),
        );
        direct_context.submit(SubmitInfo {
            sync: SyncCpu::Yes,
            ..SubmitInfo::default()
        });

        let swapchains = [self.swapchain];
        let image_indices = [image_index];
        let present_info = vk::PresentInfoKHR::builder()
            .swapchains(&swapchains)
            .image_indices(&image_indices);
        let present_suboptimal = match unsafe {
            self.swapchain_loader
                .queue_present(self.queue, &present_info)
        } {
            Ok(suboptimal) => suboptimal,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Err("Vulkan swapchain out of date"),
            Err(_) => return Err("vkQueuePresentKHR failed"),
        };

        if acquire_suboptimal || present_suboptimal {
            return Err("Vulkan swapchain suboptimal");
        }

        Ok(())
    }
}

impl Drop for AndroidVulkanRenderer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.direct_context.take();
            if self.acquire_fence != vk::Fence::null() {
                self.device.destroy_fence(self.acquire_fence, None);
            }
            if self.swapchain != vk::SwapchainKHR::null() {
                self.swapchain_loader
                    .destroy_swapchain(self.swapchain, None);
            }
            self.device.destroy_device(None);
            if self.surface != vk::SurfaceKHR::null() {
                self.surface_loader.destroy_surface(self.surface, None);
            }
            self.instance.destroy_instance(None);
            if !self.window.is_null() {
                ANativeWindow_release(self.window);
            }
        }
    }
}

struct SwapchainBundle {
    swapchain: vk::SwapchainKHR,
    format: vk::Format,
    color_type: ColorType,
    extent: vk::Extent2D,
    images: Vec<vk::Image>,
}

unsafe fn select_physical_device(
    instance: &ash::Instance,
    surface_loader: &KhrSurface,
    surface: vk::SurfaceKHR,
) -> Result<(vk::PhysicalDevice, u32), &'static str> {
    let devices = instance
        .enumerate_physical_devices()
        .map_err(|_| "vkEnumeratePhysicalDevices failed")?;
    for physical_device in devices {
        let properties = instance.get_physical_device_properties(physical_device);
        if properties.api_version < vk::API_VERSION_1_1 {
            continue;
        }
        if !supports_device_extension(instance, physical_device, Swapchain::name()) {
            continue;
        }

        let queue_families = instance.get_physical_device_queue_family_properties(physical_device);
        for (index, family) in queue_families.iter().enumerate() {
            let supports_graphics = family.queue_flags.contains(vk::QueueFlags::GRAPHICS);
            let supports_present = surface_loader
                .get_physical_device_surface_support(physical_device, index as u32, surface)
                .unwrap_or(false);
            if supports_graphics && supports_present {
                return Ok((physical_device, index as u32));
            }
        }
    }

    Err("no suitable Vulkan physical device")
}

unsafe fn supports_device_extension(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    extension_name: &CStr,
) -> bool {
    instance
        .enumerate_device_extension_properties(physical_device)
        .map(|properties| {
            properties.iter().any(|property| {
                CStr::from_ptr(property.extension_name.as_ptr() as *const c_char) == extension_name
            })
        })
        .unwrap_or(false)
}

unsafe fn create_swapchain(
    surface_loader: &KhrSurface,
    swapchain_loader: &Swapchain,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    width: u32,
    height: u32,
) -> Result<SwapchainBundle, &'static str> {
    let capabilities = surface_loader
        .get_physical_device_surface_capabilities(physical_device, surface)
        .map_err(|_| "vkGetPhysicalDeviceSurfaceCapabilitiesKHR failed")?;
    if !capabilities
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
    {
        return Err("Vulkan surface lacks color attachment support");
    }

    let formats = surface_loader
        .get_physical_device_surface_formats(physical_device, surface)
        .map_err(|_| "vkGetPhysicalDeviceSurfaceFormatsKHR failed")?;
    let surface_format = choose_surface_format(&formats)?;
    let (_, color_type) =
        skia_format_for_vk(surface_format.format).ok_or("unsupported Vulkan surface format")?;
    let present_mode = choose_present_mode(surface_loader, physical_device, surface)?;
    let extent = choose_extent(&capabilities, width, height);
    let composite_alpha = choose_composite_alpha(capabilities.supported_composite_alpha);

    let mut min_image_count = capabilities.min_image_count.saturating_add(1).max(2);
    if capabilities.max_image_count > 0 {
        min_image_count = min_image_count.min(capabilities.max_image_count);
    }

    let create_info = vk::SwapchainCreateInfoKHR::builder()
        .surface(surface)
        .min_image_count(min_image_count)
        .image_format(surface_format.format)
        .image_color_space(surface_format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(
            if capabilities
                .supported_transforms
                .contains(vk::SurfaceTransformFlagsKHR::IDENTITY)
            {
                vk::SurfaceTransformFlagsKHR::IDENTITY
            } else {
                capabilities.current_transform
            },
        )
        .composite_alpha(composite_alpha)
        .present_mode(present_mode)
        .clipped(true);

    let swapchain = swapchain_loader
        .create_swapchain(&create_info, None)
        .map_err(|_| "vkCreateSwapchainKHR failed")?;
    let images = match swapchain_loader.get_swapchain_images(swapchain) {
        Ok(images) => images,
        Err(_) => {
            swapchain_loader.destroy_swapchain(swapchain, None);
            return Err("vkGetSwapchainImagesKHR failed");
        }
    };

    Ok(SwapchainBundle {
        swapchain,
        format: surface_format.format,
        color_type,
        extent,
        images,
    })
}

unsafe fn make_skia_direct_context(
    entry: &Entry,
    instance: &ash::Instance,
    device: &ash::Device,
    physical_device: vk::PhysicalDevice,
    queue: vk::Queue,
    queue_family_index: u32,
) -> Option<gpu::DirectContext> {
    let get_proc = |gpo: skia_vk::GetProcOf| match gpo {
        skia_vk::GetProcOf::Instance(instance_handle, name) => entry
            .get_instance_proc_addr(vk::Instance::from_raw(instance_handle as u64), name)
            .map(|proc| proc as *const c_void)
            .unwrap_or(ptr::null()),
        skia_vk::GetProcOf::Device(device_handle, name) => instance
            .get_device_proc_addr(vk::Device::from_raw(device_handle as u64), name)
            .map(|proc| proc as *const c_void)
            .unwrap_or(ptr::null()),
    };

    let instance_extensions = [
        KhrSurface::name().to_str().ok()?,
        AndroidSurface::name().to_str().ok()?,
    ];
    let device_extensions = [Swapchain::name().to_str().ok()?];
    let backend_context = skia_vk::BackendContext::new_builder(
        instance.handle().as_raw() as _,
        physical_device.as_raw() as _,
        device.handle().as_raw() as _,
        (queue.as_raw() as _, queue_family_index as usize),
        &get_proc,
        Some(skia_vk::Version::new(1, 1, 0)),
    )
    .with_extensions(&instance_extensions, &device_extensions)
    .build();

    direct_contexts::make_vulkan(&backend_context, None)
}

fn choose_surface_format(
    formats: &[vk::SurfaceFormatKHR],
) -> Result<vk::SurfaceFormatKHR, &'static str> {
    if formats.is_empty() {
        return Err("Vulkan surface returned no formats");
    }
    if formats.len() == 1 && formats[0].format == vk::Format::UNDEFINED {
        return Ok(vk::SurfaceFormatKHR {
            format: vk::Format::R8G8B8A8_UNORM,
            color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
        });
    }

    for preferred in [
        vk::Format::R8G8B8A8_UNORM,
        vk::Format::B8G8R8A8_UNORM,
        vk::Format::R8G8B8A8_SRGB,
        vk::Format::B8G8R8A8_SRGB,
    ] {
        if let Some(format) = formats
            .iter()
            .copied()
            .find(|format| format.format == preferred)
        {
            return Ok(format);
        }
    }

    formats
        .iter()
        .copied()
        .find(|format| skia_format_for_vk(format.format).is_some())
        .ok_or("Vulkan surface returned no Skia-supported formats")
}

unsafe fn choose_present_mode(
    surface_loader: &KhrSurface,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
) -> Result<vk::PresentModeKHR, &'static str> {
    let present_modes = surface_loader
        .get_physical_device_surface_present_modes(physical_device, surface)
        .map_err(|_| "vkGetPhysicalDeviceSurfacePresentModesKHR failed")?;
    Ok(if present_modes.contains(&vk::PresentModeKHR::FIFO) {
        vk::PresentModeKHR::FIFO
    } else {
        present_modes
            .first()
            .copied()
            .ok_or("Vulkan surface returned no present modes")?
    })
}

fn choose_extent(
    capabilities: &vk::SurfaceCapabilitiesKHR,
    width: u32,
    height: u32,
) -> vk::Extent2D {
    if capabilities.current_extent.width != u32::MAX {
        return capabilities.current_extent;
    }

    vk::Extent2D {
        width: width.clamp(
            capabilities.min_image_extent.width,
            capabilities.max_image_extent.width,
        ),
        height: height.clamp(
            capabilities.min_image_extent.height,
            capabilities.max_image_extent.height,
        ),
    }
}

fn choose_composite_alpha(flags: vk::CompositeAlphaFlagsKHR) -> vk::CompositeAlphaFlagsKHR {
    for candidate in [
        vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
        vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
        vk::CompositeAlphaFlagsKHR::INHERIT,
        vk::CompositeAlphaFlagsKHR::OPAQUE,
    ] {
        if flags.contains(candidate) {
            return candidate;
        }
    }
    vk::CompositeAlphaFlagsKHR::OPAQUE
}

fn surface_for_image(
    direct_context: &mut gpu::DirectContext,
    image: vk::Image,
    extent: vk::Extent2D,
    swapchain_format: vk::Format,
    color_type: ColorType,
    queue_family_index: u32,
) -> Result<Surface, &'static str> {
    let (skia_format, _) =
        skia_format_for_vk(swapchain_format).ok_or("unsupported Vulkan swapchain format")?;
    let image_info = unsafe {
        skia_vk::ImageInfo::new(
            image.as_raw() as _,
            skia_vk::Alloc::default(),
            skia_vk::ImageTiling::OPTIMAL,
            skia_vk::ImageLayout::UNDEFINED,
            skia_format,
            1,
            Some(queue_family_index),
            None,
            None,
            None,
        )
    };
    let render_target =
        backend_render_targets::make_vk((extent.width as i32, extent.height as i32), &image_info);
    surfaces::wrap_backend_render_target(
        direct_context,
        &render_target,
        SurfaceOrigin::TopLeft,
        color_type,
        None,
        None,
    )
    .ok_or("Skia Vulkan surface wrap failed")
}

fn skia_format_for_vk(format: vk::Format) -> Option<(skia_vk::Format, ColorType)> {
    match format {
        vk::Format::R8G8B8A8_UNORM => Some((skia_vk::Format::R8G8B8A8_UNORM, ColorType::RGBA8888)),
        vk::Format::B8G8R8A8_UNORM => Some((skia_vk::Format::B8G8R8A8_UNORM, ColorType::BGRA8888)),
        vk::Format::R8G8B8A8_SRGB => Some((skia_vk::Format::R8G8B8A8_SRGB, ColorType::RGBA8888)),
        vk::Format::B8G8R8A8_SRGB => Some((skia_vk::Format::B8G8R8A8_SRGB, ColorType::BGRA8888)),
        _ => None,
    }
}
