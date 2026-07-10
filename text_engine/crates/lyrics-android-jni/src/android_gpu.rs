#![cfg(target_os = "android")]

use jni::sys::{jobject, JNIEnv};
use libc::c_void;
use log::warn;
use std::ptr;

mod gl;
mod vulkan;

#[repr(C)]
pub(super) struct ANativeWindow(c_void);

#[link(name = "android")]
extern "C" {
    pub(super) fn ANativeWindow_fromSurface(
        env: *mut JNIEnv,
        surface: jobject,
    ) -> *mut ANativeWindow;
    pub(super) fn ANativeWindow_release(window: *mut ANativeWindow);
    pub(super) fn ANativeWindow_setBuffersGeometry(
        window: *mut ANativeWindow,
        width: i32,
        height: i32,
        format: i32,
    ) -> i32;
}

pub struct AndroidGpuRenderer {
    backend: AndroidGpuBackend,
}

enum AndroidGpuBackend {
    Vulkan(vulkan::AndroidVulkanRenderer),
    Gl(gl::AndroidGlRenderer),
}

unsafe impl Send for AndroidGpuRenderer {}

impl AndroidGpuRenderer {
    pub unsafe fn from_java_surface(
        env: *mut JNIEnv,
        surface_object: jobject,
        surface_width: u32,
        surface_height: u32,
        frame_width: u32,
        frame_height: u32,
    ) -> Result<Self, &'static str> {
        if env.is_null()
            || surface_object.is_null()
            || surface_width == 0
            || surface_height == 0
            || frame_width == 0
            || frame_height == 0
        {
            return Err("invalid Java surface");
        }

        let window = ANativeWindow_fromSurface(env, surface_object);
        if window.is_null() {
            return Err("ANativeWindow_fromSurface failed");
        }

        // Hand the freshly-acquired window ref to the shared constructor, which
        // takes ownership of it (and releases it on final failure).
        Self::from_window_ptr(window as *mut c_void, frame_width, frame_height)
    }

    /// Build a renderer from an already-acquired `ANativeWindow` pointer.
    ///
    /// The pointer must come from [`acquire_native_window`] (i.e. from
    /// `ANativeWindow_fromSurface` on a JVM-attached thread). This entry point
    /// carries NO `JNIEnv`, so it is what the dedicated render thread calls.
    ///
    /// OWNERSHIP: consumes the window's reference in every case. On success the
    /// backend releases it in `Drop`; on failure it is released here.
    pub unsafe fn from_window_ptr(
        window: *mut c_void,
        frame_width: u32,
        frame_height: u32,
    ) -> Result<Self, &'static str> {
        let window = window as *mut ANativeWindow;
        if window.is_null() || frame_width == 0 || frame_height == 0 {
            if !window.is_null() {
                ANativeWindow_release(window);
            }
            return Err("invalid native window");
        }

        // SurfaceTexture.setDefaultBufferSize is only a default and some Android
        // window/surface restore paths replace it while the TextureView survives.
        // Pin the producer geometry immediately before EGL creates its window
        // surface; otherwise Skia can keep a downscaled target inside a restored
        // full-size buffer, producing a small image in a black window.
        if ANativeWindow_setBuffersGeometry(window, frame_width as i32, frame_height as i32, 0) != 0
        {
            ANativeWindow_release(window);
            return Err("ANativeWindow_setBuffersGeometry failed");
        }

        // match vulkan::AndroidVulkanRenderer::from_native_window(window, frame_width, frame_height) {
        //     Ok(renderer) => {
        //         return Ok(Self {
        //             backend: AndroidGpuBackend::Vulkan(renderer),
        //         });
        //     }
        //     Err(error) => {
        //         warn!(
        //             "Failed to create Android Vulkan lyrics surface, falling back to GL: {}",
        //             error
        //         );
        //     }
        // }

        match gl::AndroidGlRenderer::from_native_window(window, frame_width, frame_height) {
            Ok(renderer) => Ok(Self {
                backend: AndroidGpuBackend::Gl(renderer),
            }),
            Err(error) => {
                ANativeWindow_release(window);
                Err(error)
            }
        }
    }

    /// Draw and present one frame.
    ///
    /// INVARIANT: this renderer is single-thread-affine. Creation, every
    /// `draw_frame`, and `Drop` must all run on the SAME dedicated render thread.
    pub fn draw_frame<F>(&mut self, draw: F) -> Result<(), &'static str>
    where
        F: FnOnce(&skia_safe::Canvas),
    {
        match &mut self.backend {
            AndroidGpuBackend::Vulkan(renderer) => renderer.draw_frame(draw),
            AndroidGpuBackend::Gl(renderer) => renderer.draw_frame(draw),
        }
    }

    pub fn clear(&mut self) -> Result<(), &'static str> {
        self.draw_frame(|_| {})
    }
}

/// Acquire an `ANativeWindow` from a Java `Surface`.
///
/// MUST run on a JVM-attached thread with a valid `JNIEnv` and a thread-local
/// `jobject` — in practice the UI/main thread. `ANativeWindow_fromSurface`
/// returns a +1 reference; ownership of that reference transfers to the caller,
/// who must eventually pass it to [`AndroidGpuRenderer::from_window_ptr`] (which
/// consumes it) or free it with [`release_native_window`]. This split lets the
/// GPU setup happen on the render thread while the only `JNIEnv`-dependent call
/// stays on the main thread.
///
/// Returns null on invalid input or failure.
pub unsafe fn acquire_native_window(env: *mut JNIEnv, surface: jobject) -> *mut c_void {
    if env.is_null() || surface.is_null() {
        return ptr::null_mut();
    }
    ANativeWindow_fromSurface(env, surface) as *mut c_void
}

/// Release a window reference acquired by [`acquire_native_window`] that was NOT
/// handed to [`AndroidGpuRenderer::from_window_ptr`]. No `JNIEnv` required, so it
/// is safe to call from any thread.
pub unsafe fn release_native_window(window: *mut c_void) {
    if !window.is_null() {
        ANativeWindow_release(window as *mut ANativeWindow);
    }
}
