/// Renderer auto-detection and selection.
///
/// On machines without a GPU (e.g. Windows Server), wgpu may fail to find an
/// adapter while glow may lack OpenGL 2.0+.  This module probes for a wgpu
/// hardware adapter at startup and picks the most likely backend, with a CLI
/// override (`--renderer auto|wgpu|glow`) for manual control.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererPreference {
    /// Probe for a wgpu hardware adapter; fall back to glow if absent.
    Auto,
    /// Force the wgpu (DirectX 12 / Vulkan) backend.
    Wgpu,
    /// Force the glow (OpenGL) backend.
    Glow,
}

impl Default for RendererPreference {
    fn default() -> Self {
        Self::Auto
    }
}

/// Parse `--renderer <value>` from the CLI argument list.
///
/// Returns [`RendererPreference::Auto`] when the flag is absent or the value is
/// unrecognised.
pub fn parse_renderer_arg(args: &[String]) -> RendererPreference {
    args.windows(2)
        .find(|pair| pair[0] == "--renderer")
        .map(|pair| match pair[1].to_ascii_lowercase().as_str() {
            "wgpu" => RendererPreference::Wgpu,
            "glow" | "opengl" | "gl" => RendererPreference::Glow,
            _ => RendererPreference::Auto,
        })
        .unwrap_or_default()
}

/// Choose an [`eframe::Renderer`] based on the user preference and hardware
/// availability.
pub fn select_renderer(pref: RendererPreference) -> eframe::Renderer {
    match pref {
        RendererPreference::Wgpu => eframe::Renderer::Wgpu,
        RendererPreference::Glow => eframe::Renderer::Glow,
        RendererPreference::Auto => {
            if has_wgpu_adapter() {
                eframe::Renderer::Wgpu
            } else {
                eframe::Renderer::Glow
            }
        }
    }
}

/// Returns the alternative renderer (wgpu ↔ glow).
pub fn alternate_renderer(r: eframe::Renderer) -> eframe::Renderer {
    match r {
        eframe::Renderer::Glow => eframe::Renderer::Wgpu,
        eframe::Renderer::Wgpu => eframe::Renderer::Glow,
    }
}

/// Return a human-readable label for a renderer.
pub fn renderer_name(r: eframe::Renderer) -> &'static str {
    match r {
        eframe::Renderer::Glow => "glow (OpenGL)",
        eframe::Renderer::Wgpu => "wgpu (DX12/Vulkan)",
    }
}

/// Probe whether the system has a wgpu-compatible hardware GPU adapter.
///
/// This creates a throw-away wgpu [`Instance`](wgpu::Instance) and asks for an
/// adapter **without** a surface (and with `force_fallback_adapter: false`,
/// matching what eframe does internally).  If no hardware adapter is found the
/// caller should fall back to glow.
fn has_wgpu_adapter() -> bool {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .is_some()
}
