use super::{Bus, FRAME_HEIGHT, FRAME_WIDTH, Framebuffer};

const FRAME_CYCLES: u32 = 70224;
const REG_LCDC: u16 = 0xFF40;
const REG_SCY: u16 = 0xFF42;
const REG_SCX: u16 = 0xFF43;
const REG_BGP: u16 = 0xFF47;
const REG_WY: u16 = 0xFF4A;
const REG_WX: u16 = 0xFF4B;
const VRAM_SIZE: usize = 0x2000;
const TILE_BYTES: usize = 16;
const DMG_PALETTE: [[u8; 3]; 4] = [
    [0xE0, 0xF8, 0xD0],
    [0x88, 0xC0, 0x70],
    [0x34, 0x68, 0x56],
    [0x08, 0x18, 0x20],
];

#[derive(Debug)]
pub struct Ppu {
    cycle_counter: u32,
}

impl Ppu {
    pub fn new() -> Self {
        Self { cycle_counter: 0 }
    }

    pub fn step(&mut self, cycles: u32, bus: &Bus, framebuffer: &mut Framebuffer) -> bool {
        self.cycle_counter = self.cycle_counter.saturating_add(cycles);
        if self.cycle_counter < FRAME_CYCLES {
            return false;
        }
        self.cycle_counter -= FRAME_CYCLES;
        self.render_frame(bus, framebuffer);
        true
    }

    pub fn render_frame(&self, bus: &Bus, framebuffer: &mut Framebuffer) {
        let lcdc = bus.read8(REG_LCDC);
        if lcdc & 0x80 == 0 {
            self.clear_frame(framebuffer, DMG_PALETTE[0]);
            return;
        }
        if lcdc & 0x01 == 0 {
            self.clear_frame(framebuffer, DMG_PALETTE[0]);
            return;
        }

        let scx = bus.read8(REG_SCX);
        let scy = bus.read8(REG_SCY);
        let bgp = bus.read8(REG_BGP);
        let wy = bus.read8(REG_WY);
        let wx = bus.read8(REG_WX);
        let vram = bus.vram();
        if vram.len() < VRAM_SIZE {
            self.clear_frame(framebuffer, DMG_PALETTE[0]);
            return;
        }

        let bg_tile_map_base = if lcdc & 0x08 != 0 { 0x1C00 } else { 0x1800 };
        let win_tile_map_base = if lcdc & 0x40 != 0 { 0x1C00 } else { 0x1800 };
        let use_unsigned = lcdc & 0x10 != 0;
        let window_enabled = lcdc & 0x20 != 0;
        let window_active = window_enabled && wy <= 143 && wx <= 166;
        let width = FRAME_WIDTH;
        let height = FRAME_HEIGHT;
        let pixels = framebuffer.as_mut_slice();

        for y in 0..height {
            for x in 0..width {
                let use_window =
                    window_active && (y as u8) >= wy && (x as i16 + 7) >= wx as i16;

                let (tile_map_base, tile_x, tile_y, line_x, line_y) = if use_window {
                    let win_x = (x as i16 + 7 - wx as i16) as usize;
                    let win_y = (y as i16 - wy as i16) as usize;
                    let tile_x = win_x / 8;
                    let tile_y = win_y / 8;
                    let line_x = win_x % 8;
                    let line_y = win_y % 8;
                    (win_tile_map_base, tile_x, tile_y, line_x, line_y)
                } else {
                    let map_x = (x as u8).wrapping_add(scx) as usize;
                    let map_y = (y as u8).wrapping_add(scy) as usize;
                    let tile_x = map_x / 8;
                    let tile_y = map_y / 8;
                    let line_x = map_x % 8;
                    let line_y = map_y % 8;
                    (bg_tile_map_base, tile_x, tile_y, line_x, line_y)
                };

                let map_index = tile_y * 32 + tile_x;
                let tile_id = vram[tile_map_base + map_index];
                let tile_offset = if use_unsigned {
                    (tile_id as usize) * TILE_BYTES
                } else {
                    let signed = tile_id as i8 as i16;
                    (0x1000i16 + signed * 16) as usize
                };
                let row = tile_offset + line_y * 2;
                let lo = vram[row];
                let hi = vram[row + 1];
                let bit = 7 - line_x;
                let color_id = ((hi >> bit) & 0x1) << 1 | ((lo >> bit) & 0x1);
                let palette_index = (bgp >> (color_id * 2)) & 0x03;
                let color = DMG_PALETTE[palette_index as usize];
                let idx = (y * width + x) * 3;
                pixels[idx] = color[0];
                pixels[idx + 1] = color[1];
                pixels[idx + 2] = color[2];
            }
        }
    }

    fn clear_frame(&self, framebuffer: &mut Framebuffer, color: [u8; 3]) {
        let pixels = framebuffer.as_mut_slice();
        for idx in (0..pixels.len()).step_by(3) {
            pixels[idx] = color[0];
            pixels[idx + 1] = color[1];
            pixels[idx + 2] = color[2];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Ppu;
    use crate::domain::cartridge::ROM_BANK_SIZE;
    use crate::domain::{Bus, Cartridge, Framebuffer};

    fn bus_with_rom(mut rom: Vec<u8>) -> Bus {
        if rom.len() < 0x0150 {
            rom.resize(0x0150, 0);
        }
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        Bus::new(cartridge).expect("bus")
    }

    #[test]
    fn render_frame_reads_vram_tilemap() {
        let rom = vec![0; ROM_BANK_SIZE];
        let mut bus = bus_with_rom(rom);
        let mut framebuffer = Framebuffer::new();
        let ppu = Ppu::new();

        bus.write8(0xFF40, 0x91);
        bus.write8(0xFF47, 0xE4);
        bus.write8(0x8000, 0x80);
        bus.write8(0x8001, 0x00);
        bus.write8(0x9800, 0x00);

        ppu.render_frame(&bus, &mut framebuffer);
        assert_eq!(framebuffer.as_slice()[0], 0x88);
    }

    #[test]
    fn render_frame_window_overlays_bg() {
        let rom = vec![0; ROM_BANK_SIZE];
        let mut bus = bus_with_rom(rom);
        let mut framebuffer = Framebuffer::new();
        let ppu = Ppu::new();

        bus.write8(0xFF40, 0xF1);
        bus.write8(0xFF47, 0xE4);
        bus.write8(0xFF4A, 0x00);
        bus.write8(0xFF4B, 0x07);

        bus.write8(0x8000, 0x80);
        bus.write8(0x8001, 0x00);
        bus.write8(0x8010, 0x00);
        bus.write8(0x8011, 0x80);

        bus.write8(0x9800, 0x00);
        bus.write8(0x9C00, 0x01);

        ppu.render_frame(&bus, &mut framebuffer);
        assert_eq!(framebuffer.as_slice()[0], 0x34);
    }

    #[test]
    fn render_frame_window_disabled_by_wy() {
        let rom = vec![0; ROM_BANK_SIZE];
        let mut bus = bus_with_rom(rom);
        let mut framebuffer = Framebuffer::new();
        let ppu = Ppu::new();

        bus.write8(0xFF40, 0xF1);
        bus.write8(0xFF47, 0xE4);
        bus.write8(0xFF4A, 144);
        bus.write8(0xFF4B, 0x07);

        bus.write8(0x8000, 0x80);
        bus.write8(0x8001, 0x00);
        bus.write8(0x8010, 0x00);
        bus.write8(0x8011, 0x80);

        bus.write8(0x9800, 0x00);
        bus.write8(0x9C00, 0x01);

        ppu.render_frame(&bus, &mut framebuffer);
        assert_eq!(framebuffer.as_slice()[0], 0x88);
    }

    #[test]
    fn render_frame_window_disabled_by_wx() {
        let rom = vec![0; ROM_BANK_SIZE];
        let mut bus = bus_with_rom(rom);
        let mut framebuffer = Framebuffer::new();
        let ppu = Ppu::new();

        bus.write8(0xFF40, 0xF1);
        bus.write8(0xFF47, 0xE4);
        bus.write8(0xFF4A, 0x00);
        bus.write8(0xFF4B, 167);

        bus.write8(0x8000, 0x80);
        bus.write8(0x8001, 0x00);
        bus.write8(0x8010, 0x00);
        bus.write8(0x8011, 0x80);

        bus.write8(0x9800, 0x00);
        bus.write8(0x9C00, 0x01);

        ppu.render_frame(&bus, &mut framebuffer);
        assert_eq!(framebuffer.as_slice()[0], 0x88);
    }
}
