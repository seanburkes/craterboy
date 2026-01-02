use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};

use winit::dpi::PhysicalSize;
use winit::event::{ElementState, Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, WindowBuilder};

use crate::application::app;
use crate::domain::{
    Cartridge, Emulator, FRAME_HEIGHT, FRAME_INTERVAL_NS, FRAME_SIZE, FRAME_WIDTH,
};
use crate::infrastructure::rom_loader::RomLoadError;

#[cfg(feature = "gamepad")]
use gilrs::{Axis, Button, Gamepad, GamepadId, Gilrs};

const FRAME_WIDTH_U32: u32 = FRAME_WIDTH as u32;
const FRAME_HEIGHT_U32: u32 = FRAME_HEIGHT as u32;
const TILE_SIZE: usize = 8;
const TILE_BYTES: usize = 16;
const TILE_DATA_OFFSET: usize = 0x0000;
const DMG_PALETTE: [[u8; 3]; 4] = [
    [0xE0, 0xF8, 0xD0],
    [0x88, 0xC0, 0x70],
    [0x34, 0x68, 0x56],
    [0x08, 0x18, 0x20],
];

pub fn run(rom_path: Option<PathBuf>, boot_rom_path: Option<PathBuf>) {
    pollster::block_on(run_async(rom_path, boot_rom_path));
}

async fn run_async(rom_path: Option<PathBuf>, boot_rom_path: Option<PathBuf>) {
    let cartridge = load_rom_cartridge(rom_path);
    let rom_bytes = cartridge.as_ref().map(|cart| cart.bytes.clone());
    let boot_rom = load_boot_rom(boot_rom_path);
    let event_loop = EventLoop::new().expect("event loop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("craterboy")
            .with_inner_size(PhysicalSize::new(640, 576))
            .with_min_inner_size(PhysicalSize::new(FRAME_WIDTH_U32, FRAME_HEIGHT_U32))
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
    let mut state = State::new(instance, surface, size, cartridge, rom_bytes, boot_rom).await;
    let frame_interval = Duration::from_nanos(FRAME_INTERVAL_NS);
    let target_ms = frame_interval.as_secs_f64() * 1000.0;
    let mut next_frame = Instant::now();
    let mut fps_last = Instant::now();
    let mut fps_frames: u32 = 0;
    let mut frame_time_last = Instant::now();
    state.set_overlay_metric("FPS", "0.0");
    state.set_overlay_metric("Frame", "0.0 ms");
    state.set_overlay_metric("Target", &format!("{:.3} ms", target_ms));

    let _ = event_loop.run(move |event, elwt| match event {
        Event::WindowEvent { event, window_id } if window_id == target_window_id => match event {
            WindowEvent::CloseRequested => elwt.exit(),
            WindowEvent::Resized(size) => state.resize(size),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    let pressed = event.state == ElementState::Pressed;
                    if pressed && !event.repeat && code == KeyCode::F11 {
                        toggle_borderless_fullscreen(&window);
                    }
                    state.handle_key(code, pressed, event.repeat);
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
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

fn load_rom_cartridge(path: Option<PathBuf>) -> Option<Cartridge> {
    let mut path = path;
    if path.is_none() {
        if let Ok(Some((resume_path, _))) = app::load_auto_resume_path() {
            path = Some(resume_path);
        }
    }

    let Some(path) = path else {
        return None;
    };

    match app::load_rom(&path) {
        Ok(cartridge) => Some(cartridge),
        Err(err) => {
            report_rom_error(&path, err);
            None
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
    emulator: Emulator,
    frame_index: u8,
    rom_bytes: Option<Vec<u8>>,
    rom_frame_ready: bool,
    input: InputState,
    overlay: Overlay,
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
                height: FRAME_HEIGHT_U32,
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

        let mut emulator = Emulator::new();
        if let Some(cartridge) = cartridge {
            if let Err(err) = emulator.load_cartridge_with_boot_rom(cartridge, boot_rom) {
                eprintln!("Failed to initialize cartridge: {:?}", err);
            }
        }

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
            emulator,
            frame_index: 0,
            rom_bytes,
            rom_frame_ready: false,
            input: InputState::default(),
            overlay: Overlay::new(),
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
    }

    fn update_frame(&mut self) {
        // Poll gamepad input
        #[cfg(feature = "gamepad")]
        if let Some(ref gilrs) = self.gilrs {
            if let Some((id, gamepad)) = gilrs.gamepads().next() {
                self.input.handle_gamepad(&gamepad, 0.15);
            }
        }

        self.input.apply(&mut self.emulator);
        let _ = self.emulator.step_frame();
        if self.emulator.has_bus() {
            return;
        }
        if let Some(rom) = self.rom_bytes.as_deref() {
            if !self.rom_frame_ready {
                Self::render_rom_tiles(self.emulator.framebuffer_mut().as_mut_slice(), rom);
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
        self.input.handle_key(code, pressed);
        self.input.apply(&mut self.emulator);
    }

    fn set_overlay_metric(&mut self, label: &str, value: impl Into<String>) {
        self.overlay.set_metric(label, value);
    }

    fn render_rom_tiles(framebuffer: &mut [u8], rom: &[u8]) {
        let width = FRAME_WIDTH;
        let height = FRAME_HEIGHT;
        let bg = DMG_PALETTE[0];
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
            Self::draw_tile(framebuffer, tile, tile_x, tile_y);
        }
    }

    fn draw_tile(framebuffer: &mut [u8], tile: &[u8], tile_x: usize, tile_y: usize) {
        let width = FRAME_WIDTH;
        for row in 0..TILE_SIZE {
            let lo = tile[row * 2];
            let hi = tile[row * 2 + 1];
            for col in 0..TILE_SIZE {
                let bit = 7 - col;
                let color_index = ((hi >> bit) & 0x1) << 1 | ((lo >> bit) & 0x1);
                let color = DMG_PALETTE[color_index as usize];
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
        let max_scale_h = window_h / FRAME_HEIGHT_U32;
        let scale = max_scale_w.min(max_scale_h).max(1);
        let target_w = FRAME_WIDTH_U32 * scale;
        let target_h = FRAME_HEIGHT_U32 * scale;
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

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let (mut padded, bytes_per_row) =
            prepare_framebuffer_upload(self.emulator.framebuffer().as_slice());
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
                rows_per_image: Some(FRAME_HEIGHT_U32),
            },
            wgpu::Extent3d {
                width: FRAME_WIDTH_U32,
                height: FRAME_HEIGHT_U32,
                depth_or_array_layers: 1,
            },
        );

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
        }

        self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
    }
}

fn prepare_framebuffer_upload(frame: &[u8]) -> (Vec<u8>, u32) {
    let width = FRAME_WIDTH;
    let height = FRAME_HEIGHT;
    if frame.len() != FRAME_SIZE {
        return (vec![0u8; width * height * 4], (width * 4) as u32);
    }
    let unpadded = width * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
    let padded = ((unpadded + align - 1) / align) * align;
    let mut data = vec![0u8; padded * height];
    for y in 0..height {
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
