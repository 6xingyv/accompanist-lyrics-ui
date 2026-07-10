#[macro_use]
extern crate log;

#[cfg(target_os = "android")]
mod android_gpu;
mod jvm;
mod native;

#[cfg(target_os = "android")]
pub fn init_logging() {
    android_log::init("FontTower").ok();
}

#[cfg(not(target_os = "android"))]
pub fn init_logging() {}
