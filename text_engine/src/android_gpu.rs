#![cfg(target_os = "android")]

use jni::sys::{jobject, JNIEnv};
use libc::{c_char, c_int, c_void};
use log::warn;
use skia_safe::{
    gpu::{self, backend_render_targets, gl::FramebufferInfo, surfaces, SurfaceOrigin},
    Color4f, ColorType, Surface,
};
use std::ffi::CStr;
use std::ptr;

type EGLBoolean = c_int;
type EGLint = c_int;
type EGLDisplay = *mut c_void;
type EGLConfig = *mut c_void;
type EGLContext = *mut c_void;
type EGLSurface = *mut c_void;
type EGLNativeDisplayType = *mut c_void;
type EGLNativeWindowType = *mut c_void;

type GLenum = u32;
type GLint = c_int;

const EGL_FALSE: EGLBoolean = 0;
const EGL_DEFAULT_DISPLAY: EGLNativeDisplayType = ptr::null_mut();
const EGL_NO_DISPLAY: EGLDisplay = ptr::null_mut();
const EGL_NO_CONTEXT: EGLContext = ptr::null_mut();
const EGL_NO_SURFACE: EGLSurface = ptr::null_mut();

const EGL_NONE: EGLint = 0x3038;
const EGL_RED_SIZE: EGLint = 0x3024;
const EGL_GREEN_SIZE: EGLint = 0x3023;
const EGL_BLUE_SIZE: EGLint = 0x3022;
const EGL_ALPHA_SIZE: EGLint = 0x3021;
const EGL_STENCIL_SIZE: EGLint = 0x3026;
const EGL_RENDERABLE_TYPE: EGLint = 0x3040;
const EGL_SURFACE_TYPE: EGLint = 0x3033;
const EGL_WINDOW_BIT: EGLint = 0x0004;
const EGL_OPENGL_ES2_BIT: EGLint = 0x0004;
const EGL_CONTEXT_CLIENT_VERSION: EGLint = 0x3098;
const EGL_OPENGL_ES_API: EGLint = 0x30a0;

const GL_FRAMEBUFFER_BINDING: GLenum = 0x8ca6;

#[repr(C)]
struct ANativeWindow(c_void);

#[link(name = "android")]
extern "C" {
    fn ANativeWindow_fromSurface(env: *mut JNIEnv, surface: jobject) -> *mut ANativeWindow;
    fn ANativeWindow_release(window: *mut ANativeWindow);
}

#[link(name = "EGL")]
extern "C" {
    fn eglGetDisplay(display_id: EGLNativeDisplayType) -> EGLDisplay;
    fn eglInitialize(display: EGLDisplay, major: *mut EGLint, minor: *mut EGLint) -> EGLBoolean;
    fn eglChooseConfig(
        display: EGLDisplay,
        attrib_list: *const EGLint,
        configs: *mut EGLConfig,
        config_size: EGLint,
        num_config: *mut EGLint,
    ) -> EGLBoolean;
    fn eglBindAPI(api: EGLint) -> EGLBoolean;
    fn eglCreateContext(
        display: EGLDisplay,
        config: EGLConfig,
        share_context: EGLContext,
        attrib_list: *const EGLint,
    ) -> EGLContext;
    fn eglCreateWindowSurface(
        display: EGLDisplay,
        config: EGLConfig,
        native_window: EGLNativeWindowType,
        attrib_list: *const EGLint,
    ) -> EGLSurface;
    fn eglMakeCurrent(
        display: EGLDisplay,
        draw: EGLSurface,
        read: EGLSurface,
        context: EGLContext,
    ) -> EGLBoolean;
    fn eglSwapBuffers(display: EGLDisplay, surface: EGLSurface) -> EGLBoolean;
    fn eglSwapInterval(display: EGLDisplay, interval: EGLint) -> EGLBoolean;
    fn eglDestroySurface(display: EGLDisplay, surface: EGLSurface) -> EGLBoolean;
    fn eglDestroyContext(display: EGLDisplay, context: EGLContext) -> EGLBoolean;
    fn eglTerminate(display: EGLDisplay) -> EGLBoolean;
    fn eglGetProcAddress(procname: *const c_char) -> *const c_void;
}

#[link(name = "GLESv2")]
extern "C" {
    fn glGetIntegerv(pname: GLenum, data: *mut GLint);
}

pub struct AndroidGpuRenderer {
    window: *mut ANativeWindow,
    display: EGLDisplay,
    surface: EGLSurface,
    context: EGLContext,
    direct_context: Option<gpu::DirectContext>,
    skia_surface: Option<Surface>,
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
        // takes ownership of it (and releases it on failure).
        Self::from_window_ptr(window as *mut c_void, frame_width, frame_height)
    }

    /// Build a renderer from an already-acquired `ANativeWindow` pointer.
    ///
    /// The pointer must come from [`acquire_native_window`] (i.e. from
    /// `ANativeWindow_fromSurface` on a JVM-attached thread). This entry point
    /// carries NO `JNIEnv`, so it is what the dedicated render thread calls.
    ///
    /// OWNERSHIP: consumes the window's reference in every case — on success the
    /// returned renderer releases it in `Drop`; on failure it is released here.
    /// The caller must not touch the pointer afterwards.
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

        match Self::from_native_window(window, frame_width, frame_height) {
            Ok(renderer) => Ok(renderer),
            Err(error) => {
                ANativeWindow_release(window);
                Err(error)
            }
        }
    }

    unsafe fn from_native_window(
        window: *mut ANativeWindow,
        width: u32,
        height: u32,
    ) -> Result<Self, &'static str> {
        let display = eglGetDisplay(EGL_DEFAULT_DISPLAY);
        if display == EGL_NO_DISPLAY {
            return Err("eglGetDisplay failed");
        }

        let mut major = 0;
        let mut minor = 0;
        if eglInitialize(display, &mut major, &mut minor) == EGL_FALSE {
            return Err("eglInitialize failed");
        }

        let config_attribs = [
            EGL_RENDERABLE_TYPE,
            EGL_OPENGL_ES2_BIT,
            EGL_SURFACE_TYPE,
            EGL_WINDOW_BIT,
            EGL_RED_SIZE,
            8,
            EGL_GREEN_SIZE,
            8,
            EGL_BLUE_SIZE,
            8,
            EGL_ALPHA_SIZE,
            8,
            EGL_STENCIL_SIZE,
            8,
            EGL_NONE,
        ];
        let mut config = ptr::null_mut();
        let mut config_count = 0;
        if eglChooseConfig(
            display,
            config_attribs.as_ptr(),
            &mut config,
            1,
            &mut config_count,
        ) == EGL_FALSE
            || config_count <= 0
            || config.is_null()
        {
            eglTerminate(display);
            return Err("eglChooseConfig failed");
        }

        if eglBindAPI(EGL_OPENGL_ES_API) == EGL_FALSE {
            eglTerminate(display);
            return Err("eglBindAPI failed");
        }

        let context_attribs = [EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE];
        let context = eglCreateContext(display, config, EGL_NO_CONTEXT, context_attribs.as_ptr());
        if context == EGL_NO_CONTEXT {
            eglTerminate(display);
            return Err("eglCreateContext failed");
        }

        let surface =
            eglCreateWindowSurface(display, config, window as EGLNativeWindowType, ptr::null());
        if surface == EGL_NO_SURFACE {
            eglDestroyContext(display, context);
            eglTerminate(display);
            return Err("eglCreateWindowSurface failed");
        }

        if eglMakeCurrent(display, surface, surface, context) == EGL_FALSE {
            eglDestroySurface(display, surface);
            eglDestroyContext(display, context);
            eglTerminate(display);
            return Err("eglMakeCurrent failed");
        }

        // Pace presentation to the display's vsync (one swap per refresh). This is
        // the EGL default, but set it explicitly so the behaviour is not left to
        // the driver. The blocking wait in `eglSwapBuffers` is only acceptable
        // because this renderer lives on its own dedicated thread (see the
        // single-thread invariant on `draw_frame`); it must never run on the UI
        // thread or it would cap the whole app to the swap cadence.
        eglSwapInterval(display, 1);

        let interface = gpu::gl::Interface::new_load_with_cstr(|name| gl_proc_address(name))
            .ok_or("Skia GL interface creation failed")?;
        if !interface.validate() {
            warn!("Skia GL interface validation failed");
        }
        let mut direct_context =
            gpu::direct_contexts::make_gl(interface, None).ok_or("Skia GL context failed")?;

        let mut fboid: GLint = 0;
        glGetIntegerv(GL_FRAMEBUFFER_BINDING, &mut fboid);
        let framebuffer_info = FramebufferInfo {
            fboid: fboid as u32,
            format: gpu::gl::Format::RGBA8.into(),
            ..Default::default()
        };
        let backend_render_target =
            backend_render_targets::make_gl((width as i32, height as i32), 0, 8, framebuffer_info);
        let skia_surface = surfaces::wrap_backend_render_target(
            &mut direct_context,
            &backend_render_target,
            SurfaceOrigin::BottomLeft,
            ColorType::RGBA8888,
            None,
            None,
        )
        .ok_or("Skia surface wrap failed")?;

        Ok(Self {
            window,
            display,
            surface,
            context,
            direct_context: Some(direct_context),
            skia_surface: Some(skia_surface),
        })
    }

    /// Draw and present one frame.
    ///
    /// INVARIANT: this renderer is single-thread-affine. Creation
    /// (`from_native_window`, which makes the EGL context current), every
    /// `draw_frame`, and `Drop` must all run on the SAME thread — the dedicated
    /// render thread. Because the context is made current once at creation and
    /// nothing else on that thread rebinds EGL, we do NOT re-`eglMakeCurrent`
    /// per frame here (it was pure overhead).
    pub fn draw_frame<F>(&mut self, draw: F) -> Result<(), &'static str>
    where
        F: FnOnce(&skia_safe::Canvas),
    {
        let surface = self.skia_surface.as_mut().ok_or("missing Skia surface")?;
        let direct_context = self
            .direct_context
            .as_mut()
            .ok_or("missing Skia direct context")?;

        {
            let canvas = surface.canvas();
            canvas.clear(Color4f::new(0.0, 0.0, 0.0, 0.0));
            draw(canvas);
        }

        direct_context.flush_and_submit_surface(surface, None);
        if unsafe { eglSwapBuffers(self.display, self.surface) } == EGL_FALSE {
            return Err("eglSwapBuffers failed");
        }
        Ok(())
    }

    pub fn clear(&mut self) -> Result<(), &'static str> {
        self.draw_frame(|_| {})
    }
}

impl Drop for AndroidGpuRenderer {
    fn drop(&mut self) {
        unsafe {
            let _ = eglMakeCurrent(self.display, self.surface, self.surface, self.context);
            self.skia_surface.take();
            self.direct_context.take();
            let _ = eglMakeCurrent(self.display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
            if self.surface != EGL_NO_SURFACE {
                let _ = eglDestroySurface(self.display, self.surface);
            }
            if self.context != EGL_NO_CONTEXT {
                let _ = eglDestroyContext(self.display, self.context);
            }
            if self.display != EGL_NO_DISPLAY {
                let _ = eglTerminate(self.display);
            }
            if !self.window.is_null() {
                ANativeWindow_release(self.window);
            }
        }
    }
}

fn gl_proc_address(name: &CStr) -> *const c_void {
    unsafe { eglGetProcAddress(name.as_ptr()) }
}

/// Acquire an `ANativeWindow` from a Java `Surface`.
///
/// MUST run on a JVM-attached thread with a valid `JNIEnv` and a thread-local
/// `jobject` — in practice the UI/main thread. `ANativeWindow_fromSurface`
/// returns a +1 reference; ownership of that reference transfers to the caller,
/// who must eventually pass it to [`AndroidGpuRenderer::from_window_ptr`] (which
/// consumes it) or free it with [`release_native_window`]. This split lets the
/// EGL setup happen on the render thread while the only `JNIEnv`-dependent call
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
