use super::{Bus, CYCLES_PER_LINE, IF_STAT, IF_VBLANK, REG_LCDC, TOTAL_LINES, VBLANK_START};

impl Bus {
    pub(super) fn step_ppu(&mut self, cycles: u32) {
        let lcdc = self.read_io(REG_LCDC);
        if lcdc & 0x80 == 0 {
            self.ly = 0;
            self.ppu_line_cycles = 0;
            self.ppu_mode = 0;
            self.update_stat();
            return;
        }

        let mut remaining = cycles;
        while remaining > 0 {
            let step = remaining.min(u32::from(u16::MAX));
            self.ppu_line_cycles = self.ppu_line_cycles.wrapping_add(step as u16);
            remaining -= step;
            while self.ppu_line_cycles >= CYCLES_PER_LINE {
                self.ppu_line_cycles -= CYCLES_PER_LINE;
                self.ly = self.ly.wrapping_add(1);
                if self.ly == VBLANK_START {
                    self.interrupt_flag |= IF_VBLANK;
                }
                if self.ly >= TOTAL_LINES {
                    self.ly = 0;
                }
                self.update_stat();
            }
        }

        self.update_stat();
    }

    pub(super) fn update_stat(&mut self) {
        let mode = if self.ly >= VBLANK_START {
            1
        } else if self.ppu_line_cycles < 80 {
            2
        } else if self.ppu_line_cycles < 252 {
            3
        } else {
            0
        };

        let mut stat = self.stat & 0xF8;
        if self.ly == self.lyc {
            stat |= 0x04;
            if self.stat & 0x40 != 0 {
                self.interrupt_flag |= IF_STAT;
            }
        }
        if mode != self.ppu_mode {
            match mode {
                0 if self.stat & 0x08 != 0 => self.interrupt_flag |= IF_STAT,
                1 if self.stat & 0x10 != 0 => self.interrupt_flag |= IF_STAT,
                2 if self.stat & 0x20 != 0 => self.interrupt_flag |= IF_STAT,
                _ => {}
            }
            self.ppu_mode = mode;
        }
        stat |= mode;
        self.stat = stat;
    }
}
