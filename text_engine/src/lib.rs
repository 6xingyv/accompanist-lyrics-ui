#[macro_use]
extern crate log;

#[cfg(target_os = "android")]
mod android_gpu;
mod atlas;
mod audio;
mod core;
mod font;
mod jvm;
mod mesh;
mod native;
mod renderer;
#[cfg(target_os = "android")]
mod system_fonts;

/// Initialize logger - call this early from JNI init
#[cfg(target_os = "android")]
pub fn init_logging() {
    android_log::init("FontTower").ok();
}

#[cfg(not(target_os = "android"))]
pub fn init_logging() {
    // No-op on non-Android platforms, eprintln works fine
}
