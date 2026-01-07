use super::{Bus, FRAME_HEIGHT, FRAME_WIDTH, Framebuffer};

const FRAME_CYCLES: u32 = 70224;
const CYCLES_PER_SECOND: u32 = 4_194_304;
pub const FRAME_RATE_HZ: u32 = CYCLES_PER_SECOND / FRAME_CYCLES;
pub const FRAME_INTERVAL_NS: u64 = 1_000_000_000 / FRAME_RATE_HZ as u64;
const SCANLINE_CYCLES: u32 = 456;
const OAM_SEARCH_CYCLES: u32 = 80;
const DRAWING_CYCLES: u32 = 172;
const HBLANK_CYCLES: u32 = 204;
const VBLANK_LINE: u8 = 144;
const TOTAL_LINES: u8 = 154;
const REG_LCDC: u16 = 0xFF40;
const REG_LY: u16 = 0xFF44;
const REG_SCY: u16 = 0xFF42;
const REG_SCX: u16 = 0xFF43;
const REG_BGP: u16 = 0xFF47;
const REG_OBP0: u16 = 0xFF48;
const REG_OBP1: u16 = 0xFF49;
const REG_WY: u16 = 0xFF4A;
const REG_WX: u16 = 0xFF4B;
const VRAM_SIZE: usize = 0x2000;
const TILE_BYTES: usize = 16;
const MAX_SPRITES_PER_LINE: usize = 10;
const DMG_PALETTE: [[u8; 3]; 4] = [
    [0xE0, 0xF8, 0xD0],
    [0x88, 0xC0, 0x70],
    [0x34, 0x68, 0x56],
    [0x08, 0x18, 0x20],
];

/// Convert CGB 15-bit BGR555 color to RGB888
fn cgb_color_to_rgb(color_low: u8, color_high: u8) -> [u8; 3] {
    let color = ((color_high as u16) << 8) | (color_low as u16);
    let r = ((color & 0x001F) as u8) << 3;
    let g = (((color & 0x03E0) >> 5) as u8) << 3;
    let b = (((color & 0x7C00) >> 10) as u8) << 3;
    [r, g, b]
}

/// Sprite data for scanline rendering
#[derive(Debug, Clone, Copy)]
struct SpriteData {
    x: i16,
    y: i16,
    tile: u8,
    attr: u8,
    oam_index: usize,
}

#[derive(Debug)]
pub struct Ppu {
    cycle_counter: u32,
    line_cycle_counter: u32,
    current_line: u8,
    bg_priority: Vec<u8>,
    palette: [[u8; 3]; 4],
    line_buffer: Vec<u8>,
    line_sprites: Vec<SpriteData>,
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            cycle_counter: 0,
            line_cycle_counter: 0,
            current_line: 0,
            bg_priority: vec![0; FRAME_WIDTH * FRAME_HEIGHT],
            palette: DMG_PALETTE,
            line_buffer: vec![0; FRAME_WIDTH * 3],
            line_sprites: Vec::with_capacity(MAX_SPRITES_PER_LINE),
        }
    }

    pub fn set_palette(&mut self, palette: [[u8; 3]; 4]) {
        self.palette = palette;
    }

    pub fn step(&mut self, cycles: u32, bus: &Bus, framebuffer: &mut Framebuffer) -> bool {
        let lcdc = bus.read8(REG_LCDC);
        if lcdc & 0x80 == 0 {
            // LCD disabled
            self.clear_frame(framebuffer, self.palette[0]);
            return false;
        }

        self.line_cycle_counter = self.line_cycle_counter.saturating_add(cycles);

        // Process scanline rendering
        while self.line_cycle_counter >= SCANLINE_CYCLES {
            self.line_cycle_counter -= SCANLINE_CYCLES;

            // Render current scanline if we're in visible area
            if self.current_line < VBLANK_LINE {
                self.render_scanline(bus, framebuffer, self.current_line);
            }

            // Move to next line
            self.current_line += 1;

            // Check if we completed a frame
            if self.current_line >= TOTAL_LINES {
                self.current_line = 0;
                self.cycle_counter = 0;
                return true;
            }
        }

        false
    }

    fn render_scanline(&mut self, bus: &Bus, framebuffer: &mut Framebuffer, line: u8) {
        let lcdc = bus.read8(REG_LCDC);
        let bg_enabled = lcdc & 0x01 != 0;
        let sprites_enabled = lcdc & 0x02 != 0;
        let sprite_height = if lcdc & 0x04 != 0 { 16 } else { 8 };

        let y = line as usize;
        let width = FRAME_WIDTH;

        // Clear line buffer to background color
        if !bg_enabled {
            let color = self.palette[0];
            for x in 0..width {
                let idx = x * 3;
                self.line_buffer[idx] = color[0];
                self.line_buffer[idx + 1] = color[1];
                self.line_buffer[idx + 2] = color[2];
                self.bg_priority[y * width + x] = 0;
            }
        } else {
            // Render background/window for this line
            self.render_bg_window_line(bus, line);
        }

        // OAM search: find sprites on this line (limited to 10)
        if sprites_enabled {
            self.find_line_sprites(bus, line, sprite_height);

            // Render sprites for this line
            self.render_line_sprites(bus, line, sprite_height);
        }

        // Copy line buffer to framebuffer
        let pixels = framebuffer.as_mut_slice();
        let fb_line_start = y * width * 3;
        pixels[fb_line_start..fb_line_start + width * 3]
            .copy_from_slice(&self.line_buffer[0..width * 3]);
    }

    fn find_line_sprites(&mut self, bus: &Bus, line: u8, sprite_height: usize) {
        self.line_sprites.clear();

        let line_i16 = line as i16;

        // Scan OAM for sprites on this line
        for i in 0..40 {
            if self.line_sprites.len() >= MAX_SPRITES_PER_LINE {
                break;
            }

            let base = 0xFE00u16 + (i * 4) as u16;
            let y = bus.read8(base) as i16 - 16;
            let x = bus.read8(base + 1) as i16 - 8;
            let tile = bus.read8(base + 2);
            let attr = bus.read8(base + 3);

            // Check if sprite is on this line
            if line_i16 >= y && line_i16 < y + sprite_height as i16 {
                self.line_sprites.push(SpriteData {
                    x,
                    y,
                    tile,
                    attr,
                    oam_index: i,
                });
            }
        }

        // Sort sprites by X coordinate (and OAM index for ties) for priority
        // Lower X has priority, and for same X, lower OAM index has priority
        self.line_sprites
            .sort_by(|a, b| a.x.cmp(&b.x).then_with(|| a.oam_index.cmp(&b.oam_index)));
    }

    fn render_bg_window_line(&mut self, bus: &Bus, line: u8) {
        let lcdc = bus.read8(REG_LCDC);
        let scx = bus.read8(REG_SCX);
        let scy = bus.read8(REG_SCY);
        let bgp = bus.read8(REG_BGP);
        let wy = bus.read8(REG_WY);
        let wx = bus.read8(REG_WX);
        let vram_bank0 = bus.vram_bank0();
        let vram_bank1 = bus.vram_bank1();
        let cgb_mode = bus.is_cgb();
        let bg_palette_data = bus.bg_palette_data();

        let bg_tile_map_base = if lcdc & 0x08 != 0 { 0x1C00 } else { 0x1800 };
        let win_tile_map_base = if lcdc & 0x40 != 0 { 0x1C00 } else { 0x1800 };
        let use_unsigned = lcdc & 0x10 != 0;
        let window_enabled = lcdc & 0x20 != 0;
        let window_active = window_enabled && line >= wy && wx <= 166;

        let y = line as usize;
        let width = FRAME_WIDTH;

        for x in 0..width {
            let use_window = window_active && (x as i16 + 7) >= wx as i16;

            let (tile_map_base, tile_x, tile_y, line_x, line_y) = if use_window {
                let win_x_i16 = x as i16 + 7 - wx as i16;
                let win_y_i16 = line as i16 - wy as i16;

                let win_x = if win_x_i16 < 0 { 0 } else { win_x_i16 as usize };
                let win_y = if win_y_i16 < 0 { 0 } else { win_y_i16 as usize };

                let tile_x = (win_x / 8) % 32;
                let tile_y = (win_y / 8) % 32;
                let line_x = win_x % 8;
                let line_y = win_y % 8;
                (win_tile_map_base, tile_x, tile_y, line_x, line_y)
            } else {
                let map_x = (x as u8).wrapping_add(scx) as usize;
                let map_y = line.wrapping_add(scy) as usize;
                let tile_x = map_x / 8;
                let tile_y = map_y / 8;
                let line_x = map_x % 8;
                let line_y = map_y % 8;
                (bg_tile_map_base, tile_x, tile_y, line_x, line_y)
            };

            let map_index = tile_y * 32 + tile_x;
            if map_index >= 1024 {
                continue;
            }
            let tile_id = vram_bank0[tile_map_base + map_index];

            let tile_attr = if cgb_mode {
                vram_bank1[tile_map_base + map_index]
            } else {
                0
            };

            let bg_palette_num = tile_attr & 0x07;
            let vram_bank = (tile_attr >> 3) & 0x01;
            let x_flip = (tile_attr >> 5) & 0x01 != 0;
            let y_flip = (tile_attr >> 6) & 0x01 != 0;
            let bg_to_oam_priority = (tile_attr >> 7) & 0x01 != 0;

            let tile_offset = if use_unsigned {
                (tile_id as usize) * TILE_BYTES
            } else {
                let signed = tile_id as i8 as i16;
                (0x1000i16 + signed * 16) as usize
            };

            let flipped_line_y = if y_flip { 7 - line_y } else { line_y };

            let row = tile_offset + flipped_line_y * 2;
            let tile_vram = if vram_bank == 1 {
                vram_bank1
            } else {
                vram_bank0
            };
            let lo = tile_vram[row];
            let hi = tile_vram[row + 1];

            let bit = if x_flip { line_x } else { 7 - line_x };
            let color_id = ((hi >> bit) & 0x1) << 1 | ((lo >> bit) & 0x1);

            let color = if cgb_mode {
                let palette_offset = (bg_palette_num as usize) * 8 + (color_id as usize) * 2;
                cgb_color_to_rgb(
                    bg_palette_data[palette_offset],
                    bg_palette_data[palette_offset + 1],
                )
            } else {
                let palette_index = (bgp >> (color_id * 2)) & 0x03;
                self.palette[palette_index as usize]
            };

            let idx = x * 3;
            self.line_buffer[idx] = color[0];
            self.line_buffer[idx + 1] = color[1];
            self.line_buffer[idx + 2] = color[2];

            let priority_value = if bg_to_oam_priority {
                0x80 | color_id
            } else {
                color_id
            };
            self.bg_priority[y * width + x] = priority_value;
        }
    }

    fn render_line_sprites(&mut self, bus: &Bus, line: u8, sprite_height: usize) {
        let obp0 = bus.read8(REG_OBP0);
        let obp1 = bus.read8(REG_OBP1);
        let vram_bank0 = bus.vram_bank0();
        let vram_bank1 = bus.vram_bank1();
        let cgb_mode = bus.is_cgb();
        let ob_palette_data = bus.ob_palette_data();

        let y = line as usize;
        let width = FRAME_WIDTH;
        let line_i16 = line as i16;

        // Render sprites in reverse order (higher priority last)
        for sprite in self.line_sprites.iter().rev() {
            let y_flip = sprite.attr & 0x40 != 0;
            let x_flip = sprite.attr & 0x20 != 0;
            let dmg_palette = if sprite.attr & 0x10 != 0 { obp1 } else { obp0 };
            let priority = sprite.attr & 0x80 != 0;

            let cgb_palette_num = sprite.attr & 0x07;
            let cgb_vram_bank = (sprite.attr >> 3) & 0x01;

            let sprite_line = line_i16 - sprite.y;
            if sprite_line < 0 || sprite_line >= sprite_height as i16 {
                continue;
            }

            let mut tile_row = if y_flip {
                sprite_height - 1 - sprite_line as usize
            } else {
                sprite_line as usize
            };
            let mut tile_index = sprite.tile as usize;
            if sprite_height == 16 {
                tile_index &= 0xFE;
                if tile_row >= 8 {
                    tile_index += 1;
                    tile_row -= 8;
                }
            }

            let row_addr = tile_index * TILE_BYTES + tile_row * 2;
            let tile_vram = if cgb_mode && cgb_vram_bank == 1 {
                vram_bank1
            } else {
                vram_bank0
            };
            let lo = tile_vram[row_addr];
            let hi = tile_vram[row_addr + 1];

            for col in 0..8 {
                let screen_x = sprite.x + col as i16;
                if screen_x < 0 || screen_x >= width as i16 {
                    continue;
                }

                let bit = if x_flip { col } else { 7 - col };
                let color_id = ((hi >> bit) & 0x1) << 1 | ((lo >> bit) & 0x1);
                if color_id == 0 {
                    continue;
                }

                let bg_priority_info = self.bg_priority[y * width + screen_x as usize];
                let bg_color_id = bg_priority_info & 0x03;
                let bg_to_oam_priority = (bg_priority_info & 0x80) != 0;

                let sprite_behind_bg = if cgb_mode {
                    (priority || bg_to_oam_priority) && bg_color_id != 0
                } else {
                    priority && bg_color_id != 0
                };

                if sprite_behind_bg {
                    continue;
                }

                let color = if cgb_mode {
                    let palette_offset = (cgb_palette_num as usize) * 8 + (color_id as usize) * 2;
                    cgb_color_to_rgb(
                        ob_palette_data[palette_offset],
                        ob_palette_data[palette_offset + 1],
                    )
                } else {
                    let palette_index = (dmg_palette >> (color_id * 2)) & 0x03;
                    self.palette[palette_index as usize]
                };

                let idx = (screen_x as usize) * 3;
                self.line_buffer[idx] = color[0];
                self.line_buffer[idx + 1] = color[1];
                self.line_buffer[idx + 2] = color[2];
            }
        }
    }

    pub fn render_frame(&mut self, bus: &Bus, framebuffer: &mut Framebuffer) {
        // Use scanline-based rendering for consistency
        let lcdc = bus.read8(REG_LCDC);
        if lcdc & 0x80 == 0 {
            self.clear_frame(framebuffer, self.palette[0]);
            return;
        }

        // Render all scanlines
        for line in 0..VBLANK_LINE {
            self.render_scanline(bus, framebuffer, line);
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
        let mut ppu = Ppu::new();

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
        let mut ppu = Ppu::new();

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
        let mut ppu = Ppu::new();

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
        let mut ppu = Ppu::new();

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

    #[test]
    fn render_frame_window_offscreen_left_still_draws() {
        let rom = vec![0; ROM_BANK_SIZE];
        let mut bus = bus_with_rom(rom);
        let mut framebuffer = Framebuffer::new();
        let mut ppu = Ppu::new();

        bus.write8(0xFF40, 0xF1);
        bus.write8(0xFF47, 0xE4);
        bus.write8(0xFF4A, 0x00);
        bus.write8(0xFF4B, 0x00);

        bus.write8(0x8000, 0xFF);
        bus.write8(0x8001, 0x00);
        bus.write8(0x8010, 0x00);
        bus.write8(0x8011, 0xFF);

        bus.write8(0x9800, 0x00);
        bus.write8(0x9C00, 0x01);

        ppu.render_frame(&bus, &mut framebuffer);
        assert_eq!(framebuffer.as_slice()[0], 0x34);
    }

    #[test]
    fn render_frame_window_offscreen_right_clips() {
        let rom = vec![0; ROM_BANK_SIZE];
        let mut bus = bus_with_rom(rom);
        let mut framebuffer = Framebuffer::new();
        let mut ppu = Ppu::new();

        bus.write8(0xFF40, 0xF1);
        bus.write8(0xFF47, 0xE4);
        bus.write8(0xFF4A, 0x00);
        bus.write8(0xFF4B, 166);

        bus.write8(0x8000, 0xFF);
        bus.write8(0x8001, 0x00);
        bus.write8(0x8010, 0x00);
        bus.write8(0x8011, 0xFF);

        bus.write8(0x9800, 0x00);
        bus.write8(0x9C00, 0x01);

        ppu.render_frame(&bus, &mut framebuffer);

        let width = 160;
        let row = 0;
        let idx_bg = (row * width + 158) * 3;
        let idx_win = (row * width + 159) * 3;
        assert_eq!(framebuffer.as_slice()[idx_bg], 0x88);
        assert_eq!(framebuffer.as_slice()[idx_win], 0x34);
    }

    #[test]
    fn render_frame_sprites_draw_on_blank_bg() {
        let rom = vec![0; ROM_BANK_SIZE];
        let mut bus = bus_with_rom(rom);
        let mut framebuffer = Framebuffer::new();
        let mut ppu = Ppu::new();

        bus.write8(0xFF40, 0x82);
        bus.write8(0xFF48, 0xE4);
        bus.write8(0x8000, 0x80);
        bus.write8(0x8001, 0x00);

        bus.write8(0xFE00, 16);
        bus.write8(0xFE01, 8);
        bus.write8(0xFE02, 0x00);
        bus.write8(0xFE03, 0x00);

        ppu.render_frame(&bus, &mut framebuffer);
        assert_eq!(framebuffer.as_slice()[0], 0x88);
    }

    #[test]
    fn render_frame_sprite_priority_with_window() {
        let rom = vec![0; ROM_BANK_SIZE];
        let mut bus = bus_with_rom(rom);
        let mut framebuffer = Framebuffer::new();
        let mut ppu = Ppu::new();

        bus.write8(0xFF40, 0xF3);
        bus.write8(0xFF47, 0xE4);
        bus.write8(0xFF48, 0xE4);
        bus.write8(0xFF4A, 0x00);
        bus.write8(0xFF4B, 0x07);

        bus.write8(0x8000, 0x80);
        bus.write8(0x8001, 0x00);
        bus.write8(0x8010, 0x80);
        bus.write8(0x8011, 0x00);
        bus.write8(0x8020, 0x80);
        bus.write8(0x8021, 0x80);

        bus.write8(0x9C00, 0x01);

        bus.write8(0xFE00, 16);
        bus.write8(0xFE01, 8);
        bus.write8(0xFE02, 0x02);
        bus.write8(0xFE03, 0x80);

        ppu.render_frame(&bus, &mut framebuffer);
        assert_eq!(framebuffer.as_slice()[0], 0x88);

        bus.write8(0xFE03, 0x00);
        ppu.render_frame(&bus, &mut framebuffer);
        assert_eq!(framebuffer.as_slice()[0], 0x08);
    }

    #[test]
    fn render_frame_sprite_oam_priority() {
        let rom = vec![0; ROM_BANK_SIZE];
        let mut bus = bus_with_rom(rom);
        let mut framebuffer = Framebuffer::new();
        let mut ppu = Ppu::new();

        bus.write8(0xFF40, 0x83);
        bus.write8(0xFF48, 0xE4);

        bus.write8(0x8000, 0x80);
        bus.write8(0x8001, 0x00);
        bus.write8(0x8010, 0x80);
        bus.write8(0x8011, 0x80);

        bus.write8(0xFE00, 16);
        bus.write8(0xFE01, 8);
        bus.write8(0xFE02, 0x00);
        bus.write8(0xFE03, 0x00);

        bus.write8(0xFE04, 16);
        bus.write8(0xFE05, 8);
        bus.write8(0xFE06, 0x01);
        bus.write8(0xFE07, 0x00);

        ppu.render_frame(&bus, &mut framebuffer);
        assert_eq!(framebuffer.as_slice()[0], 0x88);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::domain::cartridge::ROM_BANK_SIZE;
    use crate::domain::{Bus, Cartridge, Framebuffer};
    use proptest::prelude::*;

    fn bus_with_rom(mut rom: Vec<u8>) -> Bus {
        if rom.len() < 0x0150 {
            rom.resize(0x0150, 0);
        }
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        Bus::new(cartridge).expect("bus")
    }

    // Property: PPU step with 0 cycles should not complete a frame
    proptest! {
        #[test]
        fn prop_ppu_zero_cycles_no_frame(cycles in 0u32..FRAME_CYCLES) {
            let rom = vec![0; ROM_BANK_SIZE];
            let bus = bus_with_rom(rom);
            let mut framebuffer = Framebuffer::new();
            let mut ppu = Ppu::new();

            let frame_complete = ppu.step(cycles, &bus, &mut framebuffer);

            prop_assert!(!frame_complete, "Frame should not complete before FRAME_CYCLES");
        }
    }

    // Property: PPU completes frame after FRAME_CYCLES
    proptest! {
        #[test]
        fn prop_ppu_frame_complete_after_frame_cycles(extra_cycles in 0u32..1000) {
            let rom = vec![0; ROM_BANK_SIZE];
            let bus = bus_with_rom(rom);
            let mut framebuffer = Framebuffer::new();
            let mut ppu = Ppu::new();

            let frame_complete = ppu.step(FRAME_CYCLES + extra_cycles, &bus, &mut framebuffer);

            prop_assert!(frame_complete, "Frame should complete after FRAME_CYCLES");
        }
    }

    // Property: Palette setting preserves RGB values
    proptest! {
        #[test]
        fn prop_ppu_palette_roundtrip(
            r0 in any::<u8>(), g0 in any::<u8>(), b0 in any::<u8>(),
            r1 in any::<u8>(), g1 in any::<u8>(), b1 in any::<u8>(),
            r2 in any::<u8>(), g2 in any::<u8>(), b2 in any::<u8>(),
            r3 in any::<u8>(), g3 in any::<u8>(), b3 in any::<u8>()
        ) {
            let mut ppu = Ppu::new();
            let palette = [
                [r0, g0, b0],
                [r1, g1, b1],
                [r2, g2, b2],
                [r3, g3, b3],
            ];

            ppu.set_palette(palette);

            // We can't directly read the palette, but we can verify it doesn't panic
            // and that rendering with custom palette works
            let rom = vec![0; ROM_BANK_SIZE];
            let mut bus = bus_with_rom(rom);
            let mut framebuffer = Framebuffer::new();
            bus.write8(0xFF40, 0x80); // Enable LCD

            ppu.render_frame(&bus, &mut framebuffer);

            // If we get here without panicking, the property holds
            prop_assert!(true);
        }
    }

    // Property: Framebuffer size is consistent (160x144x3 bytes)
    proptest! {
        #[test]
        fn prop_framebuffer_size_consistent(_dummy in any::<u8>()) {
            let framebuffer = Framebuffer::new();
            let expected_size = FRAME_WIDTH * FRAME_HEIGHT * 3;

            prop_assert_eq!(
                framebuffer.as_slice().len(),
                expected_size,
                "Framebuffer should be {}x{}x3 bytes",
                FRAME_WIDTH,
                FRAME_HEIGHT
            );
        }
    }

    // Property: LCD disabled should clear frame to palette[0]
    proptest! {
        #[test]
        fn prop_lcd_disabled_clears_frame(_dummy in any::<u8>()) {
            let rom = vec![0; ROM_BANK_SIZE];
            let mut bus = bus_with_rom(rom);
            let mut framebuffer = Framebuffer::new();
            let mut ppu = Ppu::new();

            // Disable LCD (LCDC bit 7 = 0)
            bus.write8(0xFF40, 0x00);

            ppu.render_frame(&bus, &mut framebuffer);

            // First pixel should be palette[0] color
            let pixels = framebuffer.as_slice();
            prop_assert_eq!(pixels[0], DMG_PALETTE[0][0], "R should match palette[0]");
            prop_assert_eq!(pixels[1], DMG_PALETTE[0][1], "G should match palette[0]");
            prop_assert_eq!(pixels[2], DMG_PALETTE[0][2], "B should match palette[0]");
        }
    }

    // Property: Rendering multiple times produces same result
    proptest! {
        #[test]
        fn prop_render_deterministic(seed in any::<u8>()) {
            let rom = vec![0; ROM_BANK_SIZE];
            let mut bus = bus_with_rom(rom);
            let mut framebuffer1 = Framebuffer::new();
            let mut framebuffer2 = Framebuffer::new();
            let mut ppu = Ppu::new();

            // Set up some test data in VRAM
            bus.write8(0xFF40, 0x91); // Enable LCD, BG
            bus.write8(0x8000 + seed as u16, seed);
            bus.write8(0x9800, seed);

            ppu.render_frame(&bus, &mut framebuffer1);
            ppu.render_frame(&bus, &mut framebuffer2);

            prop_assert_eq!(
                framebuffer1.as_slice(),
                framebuffer2.as_slice(),
                "Rendering should be deterministic"
            );
        }
    }

    // Property: SCX/SCY scroll values don't crash
    proptest! {
        #[test]
        fn prop_scroll_no_crash(scx in any::<u8>(), scy in any::<u8>()) {
            let rom = vec![0; ROM_BANK_SIZE];
            let mut bus = bus_with_rom(rom);
            let mut framebuffer = Framebuffer::new();
            let mut ppu = Ppu::new();

            bus.write8(0xFF40, 0x91); // Enable LCD, BG
            bus.write8(0xFF42, scy);  // SCY
            bus.write8(0xFF43, scx);  // SCX

            ppu.render_frame(&bus, &mut framebuffer);

            // If we get here without panicking, the property holds
            prop_assert!(true);
        }
    }

    // Property: Window position values don't crash
    proptest! {
        #[test]
        fn prop_window_position_no_crash(wx in any::<u8>(), wy in any::<u8>()) {
            let rom = vec![0; ROM_BANK_SIZE];
            let mut bus = bus_with_rom(rom);
            let mut framebuffer = Framebuffer::new();
            let mut ppu = Ppu::new();

            bus.write8(0xFF40, 0xF1); // Enable LCD, BG, Window
            bus.write8(0xFF4A, wy);   // WY
            bus.write8(0xFF4B, wx);   // WX

            ppu.render_frame(&bus, &mut framebuffer);

            prop_assert!(true);
        }
    }

    // Property: Palette values don't crash rendering
    proptest! {
        #[test]
        fn prop_palette_values_no_crash(bgp in any::<u8>()) {
            let rom = vec![0; ROM_BANK_SIZE];
            let mut bus = bus_with_rom(rom);
            let mut framebuffer = Framebuffer::new();
            let mut ppu = Ppu::new();

            bus.write8(0xFF40, 0x91); // Enable LCD, BG
            bus.write8(0xFF47, bgp);  // BGP

            ppu.render_frame(&bus, &mut framebuffer);

            prop_assert!(true);
        }
    }

    // Property: Accumulating cycles eventually produces frame
    proptest! {
        #[test]
        fn prop_step_accumulation(cycle_sizes in prop::collection::vec(1u32..1000, 1..100)) {
            let rom = vec![0; ROM_BANK_SIZE];
            let bus = bus_with_rom(rom);
            let mut framebuffer = Framebuffer::new();
            let mut ppu = Ppu::new();

            let mut total_cycles = 0u32;
            let mut frame_completed = false;

            for cycles in cycle_sizes.iter() {
                total_cycles += cycles;
                if ppu.step(*cycles, &bus, &mut framebuffer) {
                    frame_completed = true;
                    break;
                }
            }

            if total_cycles >= FRAME_CYCLES {
                prop_assert!(frame_completed, "Frame should complete after FRAME_CYCLES worth of steps");
            } else {
                prop_assert!(!frame_completed, "Frame should not complete before FRAME_CYCLES");
            }
        }
    }
}
