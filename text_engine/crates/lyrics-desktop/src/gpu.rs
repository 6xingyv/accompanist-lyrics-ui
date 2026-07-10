use glutin::config::{Config, ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext, Version};
use glutin::display::{Display, DisplayApiPreference, GlDisplay};
use glutin::prelude::*;
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use skia_safe::{
    gpu::{self, backend_render_targets, gl::FramebufferInfo, surfaces, SurfaceOrigin},
    Color4f, ColorType,
};
use std::num::NonZeroU32;
use std::sync::Arc;
use tao::dpi::PhysicalSize;
use tao::window::Window;

type GlWindowSurface = Surface<WindowSurface>;

pub(crate) struct DesktopGpuRenderer {
    gl_surface: GlWindowSurface,
    gl_context: PossiblyCurrentContext,
    direct_context: gpu::DirectContext,
    skia_surface: Option<skia_safe::Surface>,
    size: PhysicalSize<u32>,
}

impl DesktopGpuRenderer {
    pub(crate) fn new(window: &Arc<Window>) -> Result<Self, String> {
        let size = normalized_size(window.inner_size());
        let raw_display = window
            .display_handle()
            .map_err(|error| format!("failed to get display handle: {error}"))?
            .as_raw();
        let raw_window = window
            .window_handle()
            .map_err(|error| format!("failed to get window handle: {error}"))?
            .as_raw();

        let display = unsafe {
            Display::new(raw_display, display_api_preference(raw_window))
                .map_err(|error| format!("failed to create GL display: {error}"))?
        };

        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_stencil_size(8)
            .build();
        let config = unsafe {
            display
                .find_configs(template)
                .map_err(|error| format!("failed to enumerate GL configs: {error}"))?
                .max_by_key(config_score)
                .ok_or("no compatible GL config")?
        };

        let context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .build(Some(raw_window));
        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(Some(Version::new(3, 0))))
            .build(Some(raw_window));
        let not_current = unsafe {
            display
                .create_context(&config, &context_attributes)
                .or_else(|_| display.create_context(&config, &fallback_context_attributes))
                .map_err(|error| format!("failed to create GL context: {error}"))?
        };

        let attrs = SurfaceAttributesBuilder::<WindowSurface>::new()
            .with_srgb(Some(true))
            .build(raw_window, non_zero(size.width), non_zero(size.height));
        let gl_surface = unsafe {
            display
                .create_window_surface(&config, &attrs)
                .map_err(|error| format!("failed to create GL window surface: {error}"))?
        };
        let gl_context = not_current
            .make_current(&gl_surface)
            .map_err(|error| format!("failed to make GL context current: {error}"))?;
        let _ = gl_surface.set_swap_interval(&gl_context, SwapInterval::Wait(non_zero(1)));

        let interface =
            gpu::gl::Interface::new_load_with_cstr(|name| display.get_proc_address(name))
                .ok_or("failed to create Skia GL interface")?;
        let mut direct_context = gpu::direct_contexts::make_gl(interface, None)
            .ok_or("failed to create Skia GL direct context")?;
        let skia_surface = make_skia_surface(&mut direct_context, size)
            .ok_or("failed to wrap GL framebuffer for Skia")?;

        Ok(Self {
            gl_surface,
            gl_context,
            direct_context,
            skia_surface: Some(skia_surface),
            size,
        })
    }

    pub(crate) fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), String> {
        let size = normalized_size(size);
        if self.size == size {
            return Ok(());
        }
        self.gl_surface.resize(
            &self.gl_context,
            non_zero(size.width),
            non_zero(size.height),
        );
        self.skia_surface.take();
        self.skia_surface = make_skia_surface(&mut self.direct_context, size);
        self.size = size;
        if self.skia_surface.is_none() {
            return Err("failed to recreate Skia GL surface after resize".to_string());
        }
        Ok(())
    }

    pub(crate) fn draw_frame<F>(&mut self, draw: F) -> Result<i32, String>
    where
        F: FnOnce(&skia_safe::Canvas) -> i32,
    {
        let surface = self
            .skia_surface
            .as_mut()
            .ok_or("missing Skia GL surface")?;
        let result = {
            let canvas = surface.canvas();
            canvas.clear(Color4f::new(0.0, 0.0, 0.0, 0.0));
            draw(canvas)
        };
        self.direct_context.flush_and_submit_surface(surface, None);
        self.gl_surface
            .swap_buffers(&self.gl_context)
            .map_err(|error| format!("failed to swap GL buffers: {error}"))?;
        Ok(result)
    }
}

fn make_skia_surface(
    direct_context: &mut gpu::DirectContext,
    size: PhysicalSize<u32>,
) -> Option<skia_safe::Surface> {
    let framebuffer_info = FramebufferInfo {
        fboid: 0,
        format: gpu::gl::Format::RGBA8.into(),
        ..Default::default()
    };
    let backend_render_target = backend_render_targets::make_gl(
        (size.width as i32, size.height as i32),
        0,
        8,
        framebuffer_info,
    );
    surfaces::wrap_backend_render_target(
        direct_context,
        &backend_render_target,
        SurfaceOrigin::BottomLeft,
        ColorType::RGBA8888,
        None,
        None,
    )
}

fn config_score(config: &Config) -> (u8, u8, bool) {
    (
        config.num_samples(),
        config.alpha_size(),
        config.supports_transparency().unwrap_or(false),
    )
}

fn normalized_size(size: PhysicalSize<u32>) -> PhysicalSize<u32> {
    PhysicalSize::new(size.width.max(1), size.height.max(1))
}

fn non_zero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value.max(1)).expect("value is clamped to non-zero")
}

#[cfg(target_os = "windows")]
fn display_api_preference(raw_window: raw_window_handle::RawWindowHandle) -> DisplayApiPreference {
    DisplayApiPreference::WglThenEgl(Some(raw_window))
}

#[cfg(target_os = "macos")]
fn display_api_preference(_raw_window: raw_window_handle::RawWindowHandle) -> DisplayApiPreference {
    DisplayApiPreference::Cgl
}

#[cfg(all(unix, not(target_os = "macos")))]
fn display_api_preference(_raw_window: raw_window_handle::RawWindowHandle) -> DisplayApiPreference {
    DisplayApiPreference::EglThenGlx(Box::new(|_hook| {}))
}
