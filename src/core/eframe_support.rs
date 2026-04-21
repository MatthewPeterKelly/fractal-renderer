//! Shared construction of [`eframe::NativeOptions`] for all interactive binaries.
//!
//! Centralizing this ensures every interactive entry point (explore mode,
//! color editor, future unified GUI) picks up the same wgpu device settings.

use std::sync::Arc;

use eframe::{NativeOptions, egui_wgpu, wgpu};

/// Build `NativeOptions` that target the wgpu backend with limits sized to
/// the adapter actually available at runtime.
///
/// The stock eframe device descriptor requests `wgpu::Limits::default()`,
/// which insists on `max_color_attachments: 8`. Virtualized and software
/// drivers (WSL/XWayland, llvmpipe, older integrated GPUs) often expose only
/// 2–4, at which point `run_native` aborts with `LimitsExceeded`. Cloning
/// the adapter's own limits makes the request self-adapting: we ask for
/// exactly what the device offers, nothing more.
pub fn wgpu_native_options(viewport: egui::ViewportBuilder) -> NativeOptions {
    let wgpu_setup = egui_wgpu::WgpuSetupCreateNew {
        device_descriptor: Arc::new(|adapter| wgpu::DeviceDescriptor {
            label: Some("fractal-renderer wgpu device"),
            required_limits: adapter.limits(),
            ..Default::default()
        }),
        ..egui_wgpu::WgpuSetupCreateNew::without_display_handle()
    };

    NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: egui_wgpu::WgpuConfiguration {
            wgpu_setup: wgpu_setup.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
