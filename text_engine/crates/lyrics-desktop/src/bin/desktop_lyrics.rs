#[cfg(not(target_os = "android"))]
fn main() {
    // Do **not** export NvOptimusEnablement / AmdPowerXpressRequestHighPerformance
    // as fixed "always dGPU" symbols — that would ignore Windows Graphics Settings.
    // GPU selection is applied in `gpu_preference::apply_windows_gpu_preference`
    // via DXGI_GPU_PREFERENCE_UNSPECIFIED (honors Settings → Graphics).
    if let Err(error) = lyrics_desktop::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(target_os = "android")]
fn main() {
    eprintln!("desktop_lyrics requires a non-Android target");
    std::process::exit(1);
}
