//! Honor Windows Graphics Settings GPU assignment for this process.
//!
//! Hybrid laptops (Optimus / MUX-less AMD) often ignore per-app GPU preference for
//! plain WGL contexts. Creating a DXGI/D3D11 device with
//! [`DXGI_GPU_PREFERENCE_UNSPECIFIED`] first makes the OS apply the preference set
//! in **Settings → System → Display → Graphics** (High performance / Power saving
//! / Let Windows decide) before OpenGL is initialized.
//!
//! Configure with `gpu` in `config.json`, or override with env `ACCOMPANIST_GPU`:
//! - `auto` / unset → Windows Graphics Settings (`UNSPECIFIED`)
//! - `high` / `performance` → discrete / high-performance adapter
//! - `low` / `power` / `saving` → integrated / minimum-power adapter

#![cfg(windows)]

use std::sync::OnceLock;

/// Kept alive for the process so hybrid drivers keep the preferred GPU selected.
static PREFERRED_D3D_DEVICE: OnceLock<PreferredGpu> = OnceLock::new();

struct PreferredGpu {
    /// Strong refs so the adapter assignment sticks for the process lifetime.
    _device: windows::Win32::Graphics::Direct3D11::ID3D11Device,
    _context: windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext,
    description: String,
    vendor_id: u32,
    device_id: u32,
    #[allow(dead_code)]
    luid: (u32, i32),
}

/// Apply Windows GPU preference **before** creating any OpenGL context / window
/// surface that would lock the process onto the wrong adapter.
pub fn apply_windows_gpu_preference(configured: crate::GpuPreference) {
    match try_apply(configured) {
        Ok(desc) => {
            eprintln!("[gpu] Windows GPU preference applied: {desc}");
        }
        Err(error) => {
            eprintln!("[gpu] could not apply Windows GPU preference: {error}");
            eprintln!(
                "[gpu] OpenGL may use the default adapter; set Settings → Graphics for this exe, or ACCOMPANIST_GPU=high|low|auto"
            );
        }
    }
}

fn try_apply(configured: crate::GpuPreference) -> Result<String, String> {
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, ID3D11Device,
        ID3D11DeviceContext,
    };
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory2, IDXGIAdapter1, IDXGIFactory6, DXGI_ADAPTER_FLAG,
        DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_CREATE_FACTORY_FLAGS,
        DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE, DXGI_GPU_PREFERENCE_MINIMUM_POWER,
    };

    let preference = preference_from_env(configured);
    let preference_label = match preference {
        p if p == DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE => "HIGH_PERFORMANCE",
        p if p == DXGI_GPU_PREFERENCE_MINIMUM_POWER => "MINIMUM_POWER",
        _ => "UNSPECIFIED (Windows Graphics Settings)",
    };
    eprintln!("[gpu] DXGI preference mode: {preference_label}");

    let factory: IDXGIFactory6 = unsafe {
        CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))
            .map_err(|e| format!("CreateDXGIFactory2/IDXGIFactory6: {e}"))?
    };

    // Log available adapters for diagnostics.
    for index in 0u32..16 {
        let adapter = match unsafe { factory.EnumAdapters1(index) } {
            Ok(a) => a,
            Err(_) => break,
        };
        if let Ok(desc) = unsafe { adapter.GetDesc1() } {
            let flags = DXGI_ADAPTER_FLAG(desc.Flags as i32);
            let software = flags.0 & DXGI_ADAPTER_FLAG_SOFTWARE.0 != 0;
            let name = wchar_to_string(&desc.Description);
            eprintln!(
                "[gpu]   adapter[{index}]: {name} (vendor=0x{:04x} device=0x{:04x} software={software})",
                desc.VendorId, desc.DeviceId
            );
        }
    }

    let adapter: IDXGIAdapter1 = unsafe {
        factory
            .EnumAdapterByGpuPreference(0, preference)
            .map_err(|e| format!("EnumAdapterByGpuPreference: {e}"))?
    };

    let desc = unsafe {
        adapter
            .GetDesc1()
            .map_err(|e| format!("GetDesc1: {e}"))?
    };
    let description = wchar_to_string(&desc.Description);
    let luid = (desc.AdapterLuid.LowPart, desc.AdapterLuid.HighPart);

    // Creating a D3D11 device on the preferred adapter pins hybrid GPU drivers
    // (Optimus / switchable graphics) to the OS-assigned GPU for this process,
    // so the subsequent WGL context tends to land on the same adapter.
    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    let mut level = D3D_FEATURE_LEVEL_11_0;
    let levels = [D3D_FEATURE_LEVEL_11_0];
    unsafe {
        D3D11CreateDevice(
            &adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&levels),
            D3D11_SDK_VERSION,
            Some(&mut device),
            Some(&mut level),
            Some(&mut context),
        )
        .map_err(|e| format!("D3D11CreateDevice on preferred adapter: {e}"))?;
    }

    let device = device.ok_or("D3D11CreateDevice returned null device")?;
    let context = context.ok_or("D3D11CreateDevice returned null context")?;

    let summary = format!(
        "{description} (vendor=0x{:04x} device=0x{:04x} luid={}:{})",
        desc.VendorId, desc.DeviceId, luid.0, luid.1
    );

    let _ = PREFERRED_D3D_DEVICE.set(PreferredGpu {
        _device: device,
        _context: context,
        description: description.clone(),
        vendor_id: desc.VendorId,
        device_id: desc.DeviceId,
        luid,
    });

    Ok(summary)
}

fn preference_from_env(
    configured: crate::GpuPreference,
) -> windows::Win32::Graphics::Dxgi::DXGI_GPU_PREFERENCE {
    use windows::Win32::Graphics::Dxgi::{
        DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE, DXGI_GPU_PREFERENCE_MINIMUM_POWER,
        DXGI_GPU_PREFERENCE_UNSPECIFIED,
    };
    let configured = match configured {
        crate::GpuPreference::System => DXGI_GPU_PREFERENCE_UNSPECIFIED,
        crate::GpuPreference::HighPerformance => DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
        crate::GpuPreference::MinimumPower => DXGI_GPU_PREFERENCE_MINIMUM_POWER,
    };
    let Ok(value) = std::env::var("ACCOMPANIST_GPU") else {
        return configured;
    };
    match value.to_ascii_lowercase().as_str() {
        "auto" | "system" => DXGI_GPU_PREFERENCE_UNSPECIFIED,
        "high" | "performance" | "high_performance" | "dgpu" | "discrete" => {
            DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE
        }
        "low" | "power" | "saving" | "minimum_power" | "igpu" | "integrated" | "min" => {
            DXGI_GPU_PREFERENCE_MINIMUM_POWER
        }
        _ => configured,
    }
}

fn wchar_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// Description of the adapter selected for this process, if apply succeeded.
#[allow(dead_code)]
pub fn preferred_adapter_description() -> Option<String> {
    PREFERRED_D3D_DEVICE.get().map(|g| {
        format!(
            "{} (0x{:04x}:0x{:04x})",
            g.description, g.vendor_id, g.device_id
        )
    })
}
