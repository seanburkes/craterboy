use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};

use rfd::FileDialog;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, WindowBuilder};

const EFFECT_SMOOTHING_STRENGTH: f32 = 0.2;
const EFFECT_OUTLINE_STRENGTH: f32 = 0.8;

use crate::application::app;
use crate::domain::{
    Cartridge, Emulator, FRAME_HEIGHT, FRAME_INTERVAL_NS, FRAME_SIZE, FRAME_WIDTH,
};
use crate::infrastructure::rom_loader::RomLoadError;
use crate::interface::menu::{MenuAction, MenuOverlay};

#[cfg(feature = "audio")]
use crate::interface::audio::AudioOutput;

#[cfg(feature = "gamepad")]
use gilrs::{Gamepad, Gilrs};

const FRAME_WIDTH_U32: u32 = FRAME_WIDTH as u32;
const VISUALIZER_HEIGHT: usize = 32;
const DISPLAY_HEIGHT: usize = FRAME_HEIGHT + VISUALIZER_HEIGHT;
const DISPLAY_HEIGHT_U32: u32 = DISPLAY_HEIGHT as u32;
const VISUALIZER_BARS: usize = 8;
const VISUALIZER_BG: [u8; 3] = [0x06, 0x08, 0x0B];
const VISUALIZER_GREEN: [u8; 3] = [0x24, 0xD1, 0x4C];
const VISUALIZER_YELLOW: [u8; 3] = [0xF2, 0xC9, 0x4C];
const VISUALIZER_RED: [u8; 3] = [0xE8, 0x4B, 0x4B];
const TILE_SIZE: usize = 8;
const TILE_BYTES: usize = 16;
const TILE_DATA_OFFSET: usize = 0x0000;
const DEFAULT_PALETTE_INDEX: usize = 0;

#[derive(Debug, Clone, Copy)]
struct PaletteDefinition {
    name: &'static str,
    colors: [[u8; 3]; 4],
}

const PALETTES: [PaletteDefinition; 4] = [
    PaletteDefinition {
        name: "DMG",
        colors: [
            [0xE0, 0xF8, 0xD0],
            [0x88, 0xC0, 0x70],
            [0x34, 0x68, 0x56],
            [0x08, 0x18, 0x20],
        ],
    },
    PaletteDefinition {
        name: "Pocket",
        colors: [
            [0xF8, 0xF8, 0xF8],
            [0xA8, 0xA8, 0xA8],
            [0x50, 0x50, 0x50],
            [0x10, 0x10, 0x10],
        ],
    },
    PaletteDefinition {
        name: "Ocean",
        colors: [
            [0xE0, 0xF4, 0xFF],
            [0x8A, 0xC6, 0xD8],
            [0x3E, 0x6F, 0x89],
            [0x16, 0x2A, 0x3B],
        ],
    },
    PaletteDefinition {
        name: "Amber",
        colors: [
            [0xFF, 0xF4, 0xCF],
            [0xE6, 0xC8, 0x73],
            [0xA6, 0x6E, 0x2B],
            [0x4A, 0x2B, 0x1A],
        ],
    },
];

#[derive(Debug, Clone, Copy)]
enum ShaderEffect {
    Nearest,
    Smooth,
    Toon,
    SmoothToon,
}

impl ShaderEffect {
    fn next(self) -> Self {
        match self {
            Self::Nearest => Self::Smooth,
            Self::Smooth => Self::Toon,
            Self::Toon => Self::SmoothToon,
            Self::SmoothToon => Self::Nearest,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Nearest => "Nearest",
            Self::Smooth => "Smooth",
            Self::Toon => "Toon",
            Self::SmoothToon => "Smooth+Toon",
        }
    }

    fn mode(self) -> u32 {
        match self {
            Self::Nearest => 0,
            Self::Smooth => 1,
            Self::Toon => 2,
            Self::SmoothToon => 3,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct EffectUniform {
    mode: u32,
    _pad0: [u32; 3],
    _pad1: [u32; 4],
    texel_size: [f32; 2],
    smoothing_strength: f32,
    outline_strength: f32,
}

impl EffectUniform {
    fn new(effect: ShaderEffect) -> Self {
        Self {
            mode: effect.mode(),
            _pad0: [0; 3],
            _pad1: [0; 4],
            texel_size: [
                1.0 / FRAME_WIDTH_U32 as f32,
                1.0 / DISPLAY_HEIGHT_U32 as f32,
            ],
            smoothing_strength: EFFECT_SMOOTHING_STRENGTH,
            outline_strength: EFFECT_OUTLINE_STRENGTH,
        }
    }

    fn as_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                (self as *const Self) as *const u8,
                std::mem::size_of::<Self>(),
            )
        }
    }
}

pub fn run(rom_path: Option<PathBuf>, boot_rom_path: Option<PathBuf>) {
    pollster::block_on(run_async(rom_path, boot_rom_path));
}

async fn run_async(rom_path: Option<PathBuf>, boot_rom_path: Option<PathBuf>) {
    let (cartridge, loaded_path) = load_rom_cartridge(rom_path.clone());
    let rom_bytes = cartridge.as_ref().map(|cart| cart.bytes.clone());
    let boot_rom = load_boot_rom(boot_rom_path);
    let event_loop = EventLoop::new().expect("event loop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("craterboy")
            .with_inner_size(PhysicalSize::new(640, 576))
            .with_min_inner_size(PhysicalSize::new(FRAME_WIDTH_U32, DISPLAY_HEIGHT_U32))
            .build(&event_loop)
            .expect("window"),
    );

    let target_window_id = window.id();
    let size = window.inner_size();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });
    let surface = instance
        .create_surface(Arc::clone(&window))
        .expect("surface");
    let mut state = State::new(
        instance,
        surface,
        size,
        cartridge,
        rom_bytes,
        boot_rom,
        loaded_path.or(rom_path),
    )
    .await;
    let frame_interval = Duration::from_nanos(FRAME_INTERVAL_NS);
    let target_ms = frame_interval.as_secs_f64() * 1000.0;
    let mut next_frame = Instant::now();
    let mut fps_last = Instant::now();
    let mut fps_frames: u32 = 0;
    let mut frame_time_last = Instant::now();
    state.set_overlay_metric("FPS", "0.0");
    state.set_overlay_metric("Frame", "0.0 ms");
    state.set_overlay_metric("Target", format!("{:.3} ms", target_ms));
    state.set_overlay_metric("Palette", PALETTES[state.palette_index].name);
    state.set_overlay_metric("Shader", state.effect.name());

    let _ = event_loop.run(move |event, elwt| match event {
        Event::WindowEvent { event, window_id } if window_id == target_window_id => match event {
            WindowEvent::CloseRequested => {
                #[cfg(feature = "audio")]
                state.audio.stop();
                elwt.exit();
            }
            WindowEvent::Resized(size) => state.resize(size),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    let pressed = event.state == ElementState::Pressed;
                    if pressed && !event.repeat && code == KeyCode::F11 {
                        toggle_borderless_fullscreen(&window);
                    }
                    if pressed && !event.repeat && code == KeyCode::Escape {
                        state.toggle_menu();
                        window.request_redraw();
                        return;
                    }
                    if state.menu_visible {
                        state.update_input_state(code, pressed);
                        state.handle_menu_key_event(&event, Some(code));
                    } else {
                        state.handle_key(code, pressed, event.repeat);
                    }
                    window.request_redraw();
                } else if state.menu_visible {
                    state.handle_menu_key_event(&event, None);
                    window.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if state.menu_visible {
                    state.handle_menu_cursor(position);
                    window.request_redraw();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                if state.menu_visible {
                    state.handle_menu_cursor_left();
                    window.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => {
                if state.menu_visible {
                    state.handle_menu_mouse_input(button_state, button);
                    window.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if state.menu_visible {
                    state.handle_menu_scroll(delta);
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if state.quit_requested {
                    #[cfg(feature = "audio")]
                    state.audio.stop();
                    elwt.exit();
                    return;
                }
                state.update_frame();
                match state.render() {
                    Ok(()) => {}
                    Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                    Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                    Err(wgpu::SurfaceError::Outdated) => {}
                    Err(wgpu::SurfaceError::Timeout) => {}
                }
                fps_frames = fps_frames.saturating_add(1);

                let now = Instant::now();
                let frame_time = now.duration_since(frame_time_last);
                frame_time_last = now;
                state.set_overlay_metric(
                    "Frame",
                    format!("{:.2} ms", frame_time.as_secs_f64() * 1000.0),
                );

                let elapsed = now.duration_since(fps_last);
                if elapsed >= Duration::from_secs(1) {
                    let fps = fps_frames as f64 / elapsed.as_secs_f64();
                    state.set_overlay_metric("FPS", format!("{:.1}", fps));
                    fps_frames = 0;
                    fps_last = now;
                }
            }
            _ => {}
        },
        Event::AboutToWait => {
            let now = Instant::now();
            if now >= next_frame {
                while next_frame <= now {
                    next_frame += frame_interval;
                }
                window.request_redraw();
            }
            elwt.set_control_flow(ControlFlow::WaitUntil(next_frame));
        }
        _ => {}
    });
}

fn load_rom_cartridge(path: Option<PathBuf>) -> (Option<Cartridge>, Option<PathBuf>) {
    let mut path = path;
    if path.is_none()
        && let Ok(Some((resume_path, _))) = app::load_auto_resume_path()
    {
        path = Some(resume_path);
    }

    let Some(path) = path else {
        return (None, None);
    };

    match app::load_rom(&path) {
        Ok(cartridge) => (Some(cartridge), Some(path)),
        Err(err) => {
            report_rom_error(&path, err);
            (None, None)
        }
    }
}

fn load_boot_rom(path: Option<PathBuf>) -> Option<Vec<u8>> {
    let Some(path) = path else {
        return None;
    };
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(err) => {
            eprintln!("Failed to read boot ROM '{}': {}", path.display(), err);
            None
        }
    }
}

fn report_rom_error(path: &PathBuf, err: RomLoadError) {
    match err {
        RomLoadError::Io(io_err) => {
            eprintln!("Failed to read ROM '{}': {}", path.display(), io_err);
        }
        RomLoadError::Header(header_err) => {
            eprintln!(
                "Invalid ROM header for '{}': {:?}",
                path.display(),
                header_err
            );
        }
        RomLoadError::SaveIo(io_err) => {
            eprintln!(
                "Failed to read save data for '{}': {}",
                path.display(),
                io_err
            );
        }
    }
}

fn menu_error_message(path: &PathBuf, err: RomLoadError) -> String {
    match err {
        RomLoadError::Io(io_err) => {
            format!("Failed to read ROM '{}': {}", path.display(), io_err)
        }
        RomLoadError::Header(header_err) => {
            format!(
                "Invalid ROM header for '{}': {:?}",
                path.display(),
                header_err
            )
        }
        RomLoadError::SaveIo(io_err) => {
            format!(
                "Failed to read save data for '{}': {}",
                path.display(),
                io_err
            )
        }
    }
}

struct State {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    texture: wgpu::Texture,
    _texture_view: wgpu::TextureView,
    _texture_sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    menu_texture: wgpu::Texture,
    menu_texture_view: wgpu::TextureView,
    menu_texture_sampler: wgpu::Sampler,
    menu_bind_group: wgpu::BindGroup,
    menu_bind_group_layout: wgpu::BindGroupLayout,
    menu_pipeline: wgpu::RenderPipeline,
    emulator: Emulator,
    frame_index: u8,
    rom_bytes: Option<Vec<u8>>,
    rom_frame_ready: bool,
    rom_path: Option<PathBuf>,
    boot_rom: Option<Vec<u8>>,
    input: InputState,
    overlay: Overlay,
    palette_index: usize,
    effect: ShaderEffect,
    effect_uniform: wgpu::Buffer,
    visualizer_levels: Vec<f32>,
    menu: MenuOverlay,
    menu_visible: bool,
    menu_cursor: Option<slint::LogicalPosition>,
    quit_requested: bool,
    #[cfg(feature = "audio")]
    audio: AudioOutput,
    #[cfg(feature = "gamepad")]
    gilrs: Option<Gilrs>,
}

#[derive(Debug, Default, Clone, Copy)]
struct InputState {
    right: bool,
    left: bool,
    up: bool,
    down: bool,
    a: bool,
    b: bool,
    select: bool,
    start: bool,
}

impl InputState {
    fn handle_key(&mut self, code: KeyCode, pressed: bool) {
        match code {
            KeyCode::ArrowRight => self.right = pressed,
            KeyCode::ArrowLeft => self.left = pressed,
            KeyCode::ArrowUp => self.up = pressed,
            KeyCode::ArrowDown => self.down = pressed,
            KeyCode::KeyZ => self.a = pressed,
            KeyCode::KeyX => self.b = pressed,
            KeyCode::Enter => self.start = pressed,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => self.select = pressed,
            _ => {}
        }
    }

    fn apply(&self, emulator: &mut Emulator) {
        let mut dpad = 0x0F;
        if self.right {
            dpad &= !0x01;
        }
        if self.left {
            dpad &= !0x02;
        }
        if self.up {
            dpad &= !0x04;
        }
        if self.down {
            dpad &= !0x08;
        }

        let mut buttons = 0x0F;
        if self.a {
            buttons &= !0x01;
        }
        if self.b {
            buttons &= !0x02;
        }
        if self.select {
            buttons &= !0x04;
        }
        if self.start {
            buttons &= !0x08;
        }

        emulator.set_joyp_dpad(dpad);
        emulator.set_joyp_buttons(buttons);
    }

    #[cfg(feature = "gamepad")]
    fn handle_gamepad(&mut self, gamepad: &Gamepad, deadzone: f32) {
        // Standard Xbox/PS controller mapping
        // D-pad: Left stick
        let axis_x = gamepad
            .axis_data(gilrs::Axis::LeftStickX)
            .map(|a| a.value())
            .unwrap_or(0.0);
        let axis_y = gamepad
            .axis_data(gilrs::Axis::LeftStickY)
            .map(|a| a.value())
            .unwrap_or(0.0);

        self.left = axis_x < -deadzone;
        self.right = axis_x > deadzone;
        self.up = axis_y < -deadzone;
        self.down = axis_y > deadzone;

        // A/B face buttons
        self.a =
            gamepad.is_pressed(gilrs::Button::South) || gamepad.is_pressed(gilrs::Button::East);
        self.b =
            gamepad.is_pressed(gilrs::Button::West) || gamepad.is_pressed(gilrs::Button::North);

        // Start/Select
        self.start =
            gamepad.is_pressed(gilrs::Button::Start) || gamepad.is_pressed(gilrs::Button::Mode);
        self.select = gamepad.is_pressed(gilrs::Button::Select)
            || gamepad.is_pressed(gilrs::Button::LeftTrigger);
    }
}

impl State {
    async fn new(
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        size: PhysicalSize<u32>,
        cartridge: Option<Cartridge>,
        rom_bytes: Option<Vec<u8>>,
        boot_rom: Option<Vec<u8>>,
        rom_path: Option<PathBuf>,
    ) -> Self {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
            })
            .await
            .expect("adapter");
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .expect("device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|format| format.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: surface_caps.present_modes[0],
            desired_maximum_frame_latency: 2,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("framebuffer"),
            size: wgpu::Extent3d {
                width: FRAME_WIDTH_U32,
                height: DISPLAY_HEIGHT_U32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let texture_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("framebuffer_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let effect = ShaderEffect::Nearest;
        let effect_uniform = EffectUniform::new(effect);
        let effect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("effect_uniform"),
            size: std::mem::size_of::<EffectUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });
        {
            let mut view = effect_buffer.slice(..).get_mapped_range_mut();
            view.copy_from_slice(effect_uniform.as_bytes());
        }
        effect_buffer.unmap();

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&texture_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: effect_buffer.as_entire_binding(),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader_blit.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let menu_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("menu_bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let menu_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("menu_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader_menu.wgsl").into()),
        });

        let menu_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("menu_pipeline_layout"),
            bind_group_layouts: &[&menu_bind_group_layout],
            push_constant_ranges: &[],
        });

        let menu_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("menu_pipeline"),
            layout: Some(&menu_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &menu_shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &menu_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let menu_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("menu_texture"),
            size: wgpu::Extent3d {
                width: size.width.max(1),
                height: size.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let menu_texture_view = menu_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let menu_texture_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("menu_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let menu_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("menu_bind_group"),
            layout: &menu_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&menu_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&menu_texture_sampler),
                },
            ],
        });

        let mut emulator = Emulator::new();
        if let Some(cartridge) = cartridge
            && let Err(err) = emulator.load_cartridge_with_boot_rom(cartridge, boot_rom.clone())
        {
            eprintln!("Failed to initialize cartridge: {:?}", err);
        }
        let has_bus = emulator.has_bus();

        let palette_index = DEFAULT_PALETTE_INDEX;
        emulator.set_palette(PALETTES[palette_index].colors);

        let menu = MenuOverlay::new(size.width as usize, size.height as usize);
        if let Some(ref path) = rom_path {
            menu.set_rom_path(path.to_string_lossy().to_string());
        }
        menu.set_has_rom(emulator.has_bus());

        #[cfg(feature = "audio")]
        let mut audio = AudioOutput::new();

        #[cfg(feature = "audio")]
        audio.start(&mut emulator);

        #[cfg(feature = "gamepad")]
        let gilrs = Gilrs::new().ok();

        Self {
            surface,
            device,
            queue,
            config,
            size,
            texture,
            _texture_view: texture_view,
            _texture_sampler: texture_sampler,
            bind_group,
            pipeline,
            menu_texture,
            menu_texture_view,
            menu_texture_sampler,
            menu_bind_group,
            menu_bind_group_layout,
            menu_pipeline,
            emulator,
            frame_index: 0,
            rom_bytes,
            rom_frame_ready: false,
            rom_path,
            boot_rom,
            input: InputState::default(),
            overlay: Overlay::new(),
            palette_index,
            effect,
            effect_uniform: effect_buffer,
            visualizer_levels: vec![0.0; VISUALIZER_BARS],
            menu,
            menu_visible: !has_bus,
            menu_cursor: None,
            quit_requested: false,
            #[cfg(feature = "audio")]
            audio,
            #[cfg(feature = "gamepad")]
            gilrs,
        }
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.size = size;
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.resize_menu_resources();
        self.menu.resize(size.width as usize, size.height as usize);
    }

    fn update_frame(&mut self) {
        if self.menu_visible {
            self.menu.update_timers();
        } else {
            // Poll gamepad input
            #[cfg(feature = "gamepad")]
            if let Some(ref gilrs) = self.gilrs
                && let Some((_id, gamepad)) = gilrs.gamepads().next()
            {
                self.input.handle_gamepad(&gamepad, 0.15);
            }

            self.input.apply(&mut self.emulator);
            let _ = self.emulator.step_frame();
            #[cfg(feature = "audio")]
            self.audio.enqueue_emulator_samples(&mut self.emulator);
        }
        self.apply_menu_actions();
        self.update_visualizer();
        if self.emulator.has_bus() {
            return;
        }
        if let Some(rom) = self.rom_bytes.as_deref() {
            if !self.rom_frame_ready {
                let palette = PALETTES[self.palette_index].colors;
                Self::render_rom_tiles(
                    self.emulator.framebuffer_mut().as_mut_slice(),
                    rom,
                    palette,
                );
                self.rom_frame_ready = true;
            }
            return;
        }

        self.frame_index = self.frame_index.wrapping_add(1);
        let width = FRAME_WIDTH;
        let height = FRAME_HEIGHT;
        let pixels = self.emulator.framebuffer_mut().as_mut_slice();
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) * 3;
                let r = (x as u8).wrapping_add(self.frame_index);
                let g = (y as u8).wrapping_add(self.frame_index);
                let b = (x as u8).wrapping_add(y as u8);
                pixels[idx] = r;
                pixels[idx + 1] = g;
                pixels[idx + 2] = b;
            }
        }
    }

    fn handle_key(&mut self, code: KeyCode, pressed: bool, repeated: bool) {
        if pressed && !repeated && code == KeyCode::F1 {
            self.overlay.toggle();
        }
        if pressed && !repeated && code == KeyCode::F2 {
            self.cycle_palette(1);
        }
        if pressed && !repeated && code == KeyCode::F3 {
            self.cycle_shader();
        }
        self.input.handle_key(code, pressed);
        if !self.menu_visible {
            self.input.apply(&mut self.emulator);
        }
    }

    fn update_input_state(&mut self, code: KeyCode, pressed: bool) {
        self.input.handle_key(code, pressed);
    }

    fn toggle_menu(&mut self) {
        self.menu_visible = !self.menu_visible;
        self.menu.set_has_rom(self.emulator.has_bus());
        if let Some(ref path) = self.rom_path {
            self.menu.set_rom_path(path.to_string_lossy().to_string());
        }
        self.menu.set_status("");
        self.menu.request_redraw();
        if !self.menu_visible {
            self.menu_cursor = None;
        }
    }

    fn apply_menu_actions(&mut self) {
        let actions = self.menu.take_actions();
        for action in actions {
            match action {
                MenuAction::LoadRom(path) => self.handle_menu_load(path),
                MenuAction::Resume => {
                    if self.emulator.has_bus() {
                        self.menu_visible = false;
                        self.menu_cursor = None;
                    }
                }
                MenuAction::Quit => {
                    self.quit_requested = true;
                }
                MenuAction::ShowFilePicker => {
                    if let Some(path) = Self::show_file_picker() {
                        self.menu.set_selected_path(&path);
                    }
                }
            }
        }
    }

    fn show_file_picker() -> Option<std::path::PathBuf> {
        let dialog = FileDialog::new()
            .add_filter("Game Boy ROM", &["gb", "gbc"])
            .set_title("Select ROM");
        dialog.pick_file()
    }

    fn handle_menu_load(&mut self, path: String) {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            self.menu.set_status("Enter a ROM path.");
            return;
        }
        let path = PathBuf::from(trimmed);
        match app::load_rom(&path) {
            Ok(cartridge) => {
                let bytes = cartridge.bytes.clone();
                if let Err(err) = self
                    .emulator
                    .load_cartridge_with_boot_rom(cartridge, self.boot_rom.clone())
                {
                    self.menu.set_status(format!("Failed to init ROM: {err:?}"));
                    return;
                }
                self.emulator
                    .set_palette(PALETTES[self.palette_index].colors);
                self.rom_bytes = Some(bytes);
                self.rom_frame_ready = false;
                self.rom_path = Some(path.clone());
                let _ = app::save_auto_resume_for(path, None);
                self.menu.set_has_rom(true);
                self.menu.set_status("");
                self.menu_visible = false;
                self.menu_cursor = None;
            }
            Err(err) => {
                self.menu.set_status(menu_error_message(&path, err));
            }
        }
    }

    fn set_overlay_metric(&mut self, label: &str, value: impl Into<String>) {
        self.overlay.set_metric(label, value);
    }

    fn update_visualizer(&mut self) {
        let target = {
            #[cfg(feature = "audio")]
            {
                self.audio.visualizer_bars(VISUALIZER_BARS)
            }
            #[cfg(not(feature = "audio"))]
            {
                vec![0.0; VISUALIZER_BARS]
            }
        };
        for (level, target) in self.visualizer_levels.iter_mut().zip(target.iter()) {
            let t = target.clamp(0.0, 1.0);
            if t > *level {
                *level = *level * 0.7 + t * 0.3;
            } else {
                *level *= 0.88;
            }
        }
    }

    fn update_effect_uniform(&self) {
        let uniform = EffectUniform::new(self.effect);
        self.queue
            .write_buffer(&self.effect_uniform, 0, uniform.as_bytes());
    }

    fn cycle_shader(&mut self) {
        self.effect = self.effect.next();
        self.update_effect_uniform();
        self.set_overlay_metric("Shader", self.effect.name());
    }

    fn cycle_palette(&mut self, delta: isize) {
        let len = PALETTES.len() as isize;
        let next = (self.palette_index as isize + delta + len) % len;
        self.palette_index = next as usize;
        let palette = PALETTES[self.palette_index].colors;
        self.emulator.set_palette(palette);
        self.rom_frame_ready = false;
        self.set_overlay_metric("Palette", PALETTES[self.palette_index].name);
    }

    fn render_rom_tiles(framebuffer: &mut [u8], rom: &[u8], palette: [[u8; 3]; 4]) {
        let width = FRAME_WIDTH;
        let height = FRAME_HEIGHT;
        let bg = palette[0];
        for idx in (0..framebuffer.len()).step_by(3) {
            framebuffer[idx] = bg[0];
            framebuffer[idx + 1] = bg[1];
            framebuffer[idx + 2] = bg[2];
        }

        let tiles_per_row = width / TILE_SIZE;
        let tiles_per_col = height / TILE_SIZE;
        let tile_count = tiles_per_row * tiles_per_col;

        for tile_index in 0..tile_count {
            let tile_offset = TILE_DATA_OFFSET + tile_index * TILE_BYTES;
            if tile_offset + TILE_BYTES > rom.len() {
                break;
            }
            let tile = &rom[tile_offset..tile_offset + TILE_BYTES];
            let tile_x = (tile_index % tiles_per_row) * TILE_SIZE;
            let tile_y = (tile_index / tiles_per_row) * TILE_SIZE;
            Self::draw_tile(framebuffer, tile, tile_x, tile_y, palette);
        }
    }

    fn draw_tile(
        framebuffer: &mut [u8],
        tile: &[u8],
        tile_x: usize,
        tile_y: usize,
        palette: [[u8; 3]; 4],
    ) {
        let width = FRAME_WIDTH;
        for row in 0..TILE_SIZE {
            let lo = tile[row * 2];
            let hi = tile[row * 2 + 1];
            for col in 0..TILE_SIZE {
                let bit = 7 - col;
                let color_index = ((hi >> bit) & 0x1) << 1 | ((lo >> bit) & 0x1);
                let color = palette[color_index as usize];
                let x = tile_x + col;
                let y = tile_y + row;
                let idx = (y * width + x) * 3;
                framebuffer[idx] = color[0];
                framebuffer[idx + 1] = color[1];
                framebuffer[idx + 2] = color[2];
            }
        }
    }

    fn compute_viewport(&self) -> Viewport {
        let window_w = self.size.width;
        let window_h = self.size.height;
        if window_w == 0 || window_h == 0 {
            return Viewport::full(window_w, window_h);
        }

        let max_scale_w = window_w / FRAME_WIDTH_U32;
        let max_scale_h = window_h / DISPLAY_HEIGHT_U32;
        let scale = max_scale_w.min(max_scale_h).max(1);
        let target_w = FRAME_WIDTH_U32 * scale;
        let target_h = DISPLAY_HEIGHT_U32 * scale;
        let x = window_w.saturating_sub(target_w) / 2;
        let y = window_h.saturating_sub(target_h) / 2;

        Viewport {
            x: x as f32,
            y: y as f32,
            width: target_w as f32,
            height: target_h as f32,
            scissor_x: x,
            scissor_y: y,
            scissor_width: target_w,
            scissor_height: target_h,
        }
    }

    fn resize_menu_resources(&mut self) {
        let width = self.size.width.max(1);
        let height = self.size.height.max(1);
        self.menu_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("menu_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.menu_texture_view = self
            .menu_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.menu_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("menu_bind_group"),
            layout: &self.menu_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.menu_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.menu_texture_sampler),
                },
            ],
        });
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let (mut padded, bytes_per_row) = prepare_framebuffer_upload(
            self.emulator.framebuffer().as_slice(),
            &self.visualizer_levels,
        );
        self.overlay
            .draw(&mut padded, bytes_per_row, FRAME_WIDTH, FRAME_HEIGHT);

        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &padded,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(DISPLAY_HEIGHT_U32),
            },
            wgpu::Extent3d {
                width: FRAME_WIDTH_U32,
                height: DISPLAY_HEIGHT_U32,
                depth_or_array_layers: 1,
            },
        );

        if self.menu_visible {
            let (menu_rgba, menu_width, menu_height) = self.menu.render_rgba();
            if menu_width > 0 && menu_height > 0 {
                let (menu_padded, menu_bytes_per_row) =
                    prepare_overlay_upload(menu_rgba, menu_width, menu_height);
                self.queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &self.menu_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &menu_padded,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(menu_bytes_per_row),
                        rows_per_image: Some(menu_height as u32),
                    },
                    wgpu::Extent3d {
                        width: menu_width as u32,
                        height: menu_height as u32,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_encoder"),
            });

        {
            let viewport = self.compute_viewport();
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            render_pass.set_viewport(
                viewport.x,
                viewport.y,
                viewport.width,
                viewport.height,
                0.0,
                1.0,
            );
            render_pass.set_scissor_rect(
                viewport.scissor_x,
                viewport.scissor_y,
                viewport.scissor_width,
                viewport.scissor_height,
            );
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.draw(0..3, 0..1);

            if self.menu_visible {
                render_pass.set_viewport(
                    0.0,
                    0.0,
                    self.size.width as f32,
                    self.size.height as f32,
                    0.0,
                    1.0,
                );
                render_pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
                render_pass.set_pipeline(&self.menu_pipeline);
                render_pass.set_bind_group(0, &self.menu_bind_group, &[]);
                render_pass.draw(0..3, 0..1);
            }
        }

        self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
    }

    fn handle_menu_cursor(&mut self, position: PhysicalPosition<f64>) {
        let pos = self.menu_position(position);
        match (self.menu_cursor, pos) {
            (Some(_), None) => {
                self.menu
                    .dispatch_event(slint::platform::WindowEvent::PointerExited);
                self.menu_cursor = None;
            }
            (_, Some(pos)) => {
                self.menu
                    .dispatch_event(slint::platform::WindowEvent::PointerMoved { position: pos });
                self.menu_cursor = Some(pos);
            }
            (None, None) => {}
        }
    }

    fn handle_menu_cursor_left(&mut self) {
        if self.menu_cursor.is_some() {
            self.menu
                .dispatch_event(slint::platform::WindowEvent::PointerExited);
            self.menu_cursor = None;
        }
    }

    fn handle_menu_mouse_input(&mut self, button_state: ElementState, button: MouseButton) {
        let Some(position) = self.menu_cursor else {
            return;
        };
        let button = match button {
            MouseButton::Left => slint::platform::PointerEventButton::Left,
            MouseButton::Right => slint::platform::PointerEventButton::Right,
            MouseButton::Middle => slint::platform::PointerEventButton::Middle,
            _ => return,
        };
        let event = if button_state == ElementState::Pressed {
            slint::platform::WindowEvent::PointerPressed { position, button }
        } else {
            slint::platform::WindowEvent::PointerReleased { position, button }
        };
        self.menu.dispatch_event(event);
    }

    fn handle_menu_scroll(&mut self, delta: MouseScrollDelta) {
        let Some(position) = self.menu_cursor else {
            return;
        };
        let (dx, dy) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (x * 16.0, y * 16.0),
            MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
        };
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        self.menu
            .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                position,
                delta_x: dx,
                delta_y: dy,
            });
    }

    fn handle_menu_key_event(&mut self, event: &winit::event::KeyEvent, code: Option<KeyCode>) {
        let text = code
            .and_then(slint_key_text)
            .or_else(|| event.text.as_ref().map(|text| text.to_string().into()));
        let Some(text) = text else {
            return;
        };
        let slint_event = match event.state {
            ElementState::Pressed if event.repeat => {
                slint::platform::WindowEvent::KeyPressRepeated { text }
            }
            ElementState::Pressed => slint::platform::WindowEvent::KeyPressed { text },
            ElementState::Released => slint::platform::WindowEvent::KeyReleased { text },
        };
        self.menu.dispatch_event(slint_event);
    }

    fn menu_position(&self, position: PhysicalPosition<f64>) -> Option<slint::LogicalPosition> {
        let x = position.x as f32;
        let y = position.y as f32;
        if x < 0.0 || y < 0.0 || x >= self.size.width as f32 || y >= self.size.height as f32 {
            return None;
        }
        Some(slint::LogicalPosition { x, y })
    }
}

fn prepare_framebuffer_upload(frame: &[u8], bars: &[f32]) -> (Vec<u8>, u32) {
    let width = FRAME_WIDTH;
    let height = DISPLAY_HEIGHT;
    if frame.len() != FRAME_SIZE {
        return (vec![0u8; width * height * 4], (width * 4) as u32);
    }
    let unpadded = width * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
    let padded = unpadded.div_ceil(align) * align;
    let mut data = vec![0u8; padded * height];
    for y in 0..height {
        let dst = y * padded;
        for x in 0..width {
            let dst_px = dst + x * 4;
            data[dst_px] = VISUALIZER_BG[0];
            data[dst_px + 1] = VISUALIZER_BG[1];
            data[dst_px + 2] = VISUALIZER_BG[2];
            data[dst_px + 3] = 0xFF;
        }
    }
    for y in 0..FRAME_HEIGHT {
        let src = y * width * 3;
        let dst = y * padded;
        for x in 0..width {
            let src_px = src + x * 3;
            let dst_px = dst + x * 4;
            data[dst_px] = frame[src_px];
            data[dst_px + 1] = frame[src_px + 1];
            data[dst_px + 2] = frame[src_px + 2];
            data[dst_px + 3] = 0xFF;
        }
    }

    if !bars.is_empty() {
        let bar_count = bars.len().min(width);
        let bar_width = width / bar_count;
        let bar_area_height = VISUALIZER_HEIGHT.saturating_sub(2).max(1);
        let y_base = FRAME_HEIGHT + VISUALIZER_HEIGHT - 1;
        for (i, level) in bars.iter().take(bar_count).enumerate() {
            let level = level.clamp(0.0, 1.0);
            let bar_height = (level * bar_area_height as f32).round() as usize;
            if bar_height == 0 || bar_width == 0 {
                continue;
            }
            let x_start = i * bar_width;
            let x_end = (i + 1) * bar_width;
            let x_bar_end = x_end.saturating_sub(1);
            for y in 0..bar_height {
                let y_pos = y_base.saturating_sub(y + 1);
                let t = y as f32 / bar_area_height as f32;
                let (r, g, b) = if t < 0.6 {
                    let local = t / 0.6;
                    let r = (VISUALIZER_GREEN[0] as f32 * (1.0 - local)
                        + VISUALIZER_YELLOW[0] as f32 * local) as u8;
                    let g = (VISUALIZER_GREEN[1] as f32 * (1.0 - local)
                        + VISUALIZER_YELLOW[1] as f32 * local) as u8;
                    let b = (VISUALIZER_GREEN[2] as f32 * (1.0 - local)
                        + VISUALIZER_YELLOW[2] as f32 * local) as u8;
                    (r, g, b)
                } else {
                    let local = (t - 0.6) / 0.4;
                    let r = (VISUALIZER_YELLOW[0] as f32 * (1.0 - local)
                        + VISUALIZER_RED[0] as f32 * local) as u8;
                    let g = (VISUALIZER_YELLOW[1] as f32 * (1.0 - local)
                        + VISUALIZER_RED[1] as f32 * local) as u8;
                    let b = (VISUALIZER_YELLOW[2] as f32 * (1.0 - local)
                        + VISUALIZER_RED[2] as f32 * local) as u8;
                    (r, g, b)
                };

                let line = if y % 2 == 0 { 0.88 } else { 1.0 };
                let r = (r as f32 * line) as u8;
                let g = (g as f32 * line) as u8;
                let b = (b as f32 * line) as u8;
                let dst = y_pos * padded;
                for x in x_start..x_bar_end {
                    let dst_px = dst + x * 4;
                    data[dst_px] = r;
                    data[dst_px + 1] = g;
                    data[dst_px + 2] = b;
                    data[dst_px + 3] = 0xFF;
                }
            }
        }
    }
    (data, padded as u32)
}

fn prepare_overlay_upload(rgba: &[u8], width: usize, height: usize) -> (Vec<u8>, u32) {
    if width == 0 || height == 0 || rgba.len() < width * height * 4 {
        return (vec![], 0);
    }
    let unpadded = width * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
    let padded = unpadded.div_ceil(align) * align;
    let mut data = vec![0u8; padded * height];
    for y in 0..height {
        let src = y * unpadded;
        let dst = y * padded;
        data[dst..dst + unpadded].copy_from_slice(&rgba[src..src + unpadded]);
    }
    (data, padded as u32)
}

#[derive(Debug, Clone, Copy)]
struct Viewport {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    scissor_x: u32,
    scissor_y: u32,
    scissor_width: u32,
    scissor_height: u32,
}

impl Viewport {
    fn full(width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
            scissor_x: 0,
            scissor_y: 0,
            scissor_width: width,
            scissor_height: height,
        }
    }
}

fn toggle_borderless_fullscreen(window: &winit::window::Window) {
    if window.fullscreen().is_some() {
        window.set_fullscreen(None);
    } else {
        window.set_fullscreen(Some(Fullscreen::Borderless(None)));
    }
}

fn slint_key_text(code: KeyCode) -> Option<slint::SharedString> {
    use slint::platform::Key as SlintKey;

    let key = match code {
        KeyCode::Enter => SlintKey::Return,
        KeyCode::Backspace => SlintKey::Backspace,
        KeyCode::Tab => SlintKey::Tab,
        KeyCode::Escape => SlintKey::Escape,
        KeyCode::ArrowUp => SlintKey::UpArrow,
        KeyCode::ArrowDown => SlintKey::DownArrow,
        KeyCode::ArrowLeft => SlintKey::LeftArrow,
        KeyCode::ArrowRight => SlintKey::RightArrow,
        KeyCode::PageUp => SlintKey::PageUp,
        KeyCode::PageDown => SlintKey::PageDown,
        KeyCode::Home => SlintKey::Home,
        KeyCode::End => SlintKey::End,
        KeyCode::Insert => SlintKey::Insert,
        KeyCode::Delete => SlintKey::Delete,
        KeyCode::Space => SlintKey::Space,
        _ => return None,
    };
    Some(key.into())
}

#[derive(Debug)]
struct Overlay {
    entries: Vec<OverlayEntry>,
    enabled: bool,
    font: FontArc,
    scale: PxScale,
}

#[derive(Debug)]
struct OverlayEntry {
    label: String,
    text: String,
}

impl Overlay {
    fn new() -> Self {
        let font =
            FontArc::try_from_slice(include_bytes!("../../assets/fonts/RobotoMono[wght].ttf"))
                .expect("overlay font");
        Self {
            entries: Vec::new(),
            enabled: false,
            font,
            scale: PxScale::from(24.0),
        }
    }

    fn set_metric(&mut self, label: &str, value: impl Into<String>) {
        let value = value.into();
        let text = format!("{label}: {value}");
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.label == label) {
            entry.text = text;
            return;
        }
        self.entries.push(OverlayEntry {
            label: label.to_string(),
            text,
        });
    }

    fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    fn draw(&self, rgba: &mut [u8], bytes_per_row: u32, width: usize, height: usize) {
        if !self.enabled || self.entries.is_empty() {
            return;
        }
        let stride = bytes_per_row as usize / 4;
        if stride == 0 || rgba.len() < bytes_per_row as usize * height {
            return;
        }

        let scaled = self.font.as_scaled(self.scale);
        let line_gap = scaled.line_gap().ceil().max(1.0) as usize;
        let line_height = scaled.height().ceil() as usize + line_gap;
        let text_padding = 4;
        let margin = 6;

        let mut max_width: f32 = 0.0;
        for entry in &self.entries {
            max_width = max_width.max(text_width(&self.font, self.scale, &entry.text));
        }
        let text_height = self
            .entries
            .len()
            .saturating_mul(line_height)
            .saturating_sub(line_gap);
        let box_width = max_width.ceil() as usize + text_padding * 2;
        let box_height = text_height.saturating_add(text_padding * 2);

        let box_x = width.saturating_sub(box_width + margin);
        let box_y = margin;

        draw_rect_blend(
            rgba,
            stride,
            width,
            height,
            box_x,
            box_y,
            box_width,
            box_height,
            [0x10, 0x10, 0x14],
            180,
        );

        let mut y = box_y + text_padding;
        for entry in &self.entries {
            let line_width = text_width(&self.font, self.scale, &entry.text).ceil() as usize;
            let x = box_x + text_padding + (max_width.ceil() as usize).saturating_sub(line_width);
            draw_text(
                rgba,
                stride,
                width,
                height,
                x,
                y,
                &entry.text,
                [0xF2, 0xF2, 0xF2],
                &self.font,
                self.scale,
            );
            y = y.saturating_add(line_height);
        }
    }
}

fn text_width(font: &FontArc, scale: PxScale, text: &str) -> f32 {
    let scaled = font.as_scaled(scale);
    let mut width = 0.0;
    let mut prev = None;
    for ch in text.chars() {
        let id = scaled.glyph_id(ch);
        if let Some(prev_id) = prev {
            width += scaled.kern(prev_id, id);
        }
        width += scaled.h_advance(id);
        prev = Some(id);
    }
    width
}

fn draw_text(
    rgba: &mut [u8],
    stride: usize,
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    text: &str,
    color: [u8; 3],
    font: &FontArc,
    scale: PxScale,
) {
    let scaled = font.as_scaled(scale);
    let mut caret = point(x as f32, y as f32 + scaled.ascent());
    let mut prev = None;
    for ch in text.chars() {
        let id = scaled.glyph_id(ch);
        if let Some(prev_id) = prev {
            caret.x += scaled.kern(prev_id, id);
        }
        let glyph = id.with_scale_and_position(scale, caret);
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, v| {
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                if px < 0 || py < 0 {
                    return;
                }
                let px = px as usize;
                let py = py as usize;
                if px >= width || py >= height {
                    return;
                }
                let idx = (py * stride + px) * 4;
                if idx + 3 >= rgba.len() {
                    return;
                }
                let alpha = (v * 255.0).round() as u8;
                blend_pixel(rgba, idx, color, alpha);
            });
        }
        caret.x += scaled.h_advance(id);
        prev = Some(id);
    }
}

fn draw_rect_blend(
    rgba: &mut [u8],
    stride: usize,
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    color: [u8; 3],
    alpha: u8,
) {
    if w == 0 || h == 0 {
        return;
    }
    let x_end = (x + w).min(width);
    let y_end = (y + h).min(height);
    for py in y..y_end {
        for px in x..x_end {
            let idx = (py * stride + px) * 4;
            if idx + 3 >= rgba.len() {
                continue;
            }
            blend_pixel(rgba, idx, color, alpha);
        }
    }
}

fn blend_pixel(rgba: &mut [u8], idx: usize, color: [u8; 3], alpha: u8) {
    if alpha == 0 {
        return;
    }
    let inv = 255u16.saturating_sub(alpha as u16);
    let alpha = alpha as u16;
    rgba[idx] = ((rgba[idx] as u16 * inv + color[0] as u16 * alpha) / 255) as u8;
    rgba[idx + 1] = ((rgba[idx + 1] as u16 * inv + color[1] as u16 * alpha) / 255) as u8;
    rgba[idx + 2] = ((rgba[idx + 2] as u16 * inv + color[2] as u16 * alpha) / 255) as u8;
    rgba[idx + 3] = 0xFF;
}
