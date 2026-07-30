use super::{
    Bus, IO_SIZE, REG_BGPD, REG_BGPI, REG_DIV, REG_DMA, REG_HDMA1, REG_HDMA2, REG_HDMA3, REG_HDMA4,
    REG_HDMA5, REG_IF, REG_JOYP, REG_KEY0, REG_KEY1, REG_LY, REG_LYC, REG_OBPD, REG_OBPI, REG_SB,
    REG_SC, REG_STAT, REG_SVBK, REG_TAC, REG_TIMA, REG_TMA, REG_VBK,
};

impl Bus {
    pub(super) fn read_io(&self, addr: u16) -> u8 {
        match addr {
            REG_JOYP => self.read_joyp(),
            REG_SB => self.serial.sb(),
            REG_SC => self.serial.sc(),
            REG_DIV => self.timer.div(),
            REG_TIMA => self.timer.tima(),
            REG_TMA => self.timer.tma(),
            REG_TAC => self.timer.tac(),
            REG_IF => self.interrupt_flag,
            REG_STAT => self.stat,
            REG_LY => self.ly,
            REG_LYC => self.lyc,
            REG_DMA => self.dma.dma(),
            REG_KEY0 => self.read_key0(),
            REG_KEY1 => self.read_key1(),
            REG_VBK => self.vram_bank | 0xFE,
            REG_HDMA1 => (self.hdma.source() >> 8) as u8,
            REG_HDMA2 => self.hdma.source() as u8,
            REG_HDMA3 => (self.hdma.dest() >> 8) as u8,
            REG_HDMA4 => self.hdma.dest() as u8,
            REG_HDMA5 => self.hdma.read_hdma5(),
            REG_BGPI => self.read_bgpi(),
            REG_BGPD => self.read_bgpdata(),
            REG_OBPI => self.read_obpi(),
            REG_OBPD => self.read_obpdata(),
            REG_SVBK => self.wram_bank | 0xF8,
            0xFF10..=0xFF14
            | 0xFF16..=0xFF19
            | 0xFF1A..=0xFF1E
            | 0xFF20..=0xFF23
            | 0xFF24..=0xFF26
            | 0xFF30..=0xFF3F => self.apu.read_io(addr),
            _ => self.io[(addr as usize - 0xFF00) % IO_SIZE],
        }
    }

    pub(super) fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            REG_JOYP => self.joyp_select = value & 0x30,
            REG_SB => self.serial.write_sb(value),
            REG_SC => {
                self.interrupt_flag |= self.serial.write_sc(value);
            }
            REG_DIV => {
                self.interrupt_flag |= self.timer.write_div();
            }
            REG_TIMA => {
                self.timer.write_tima(value);
            }
            REG_TMA => {
                self.timer.write_tma(value);
            }
            REG_TAC => {
                self.interrupt_flag |= self.timer.write_tac(value);
            }
            REG_IF => self.interrupt_flag = value,
            REG_STAT => self.stat = (self.stat & 0x07) | (value & 0xF8),
            REG_LY => {
                self.ly = 0;
                self.ppu_line_cycles = 0;
                self.update_stat();
            }
            REG_LYC => {
                self.lyc = value;
                self.update_stat();
            }
            REG_DMA => {
                self.dma.write_dma(value);
            }
            REG_KEY0 => {}
            REG_KEY1 => {
                if self.cgb_mode {
                    self.speed_switch_pending = value & 0x01 != 0;
                }
            }
            REG_VBK => {
                self.vram_bank = value & 0x01;
            }
            REG_HDMA1 => {
                self.hdma.write_hdma1(value);
            }
            REG_HDMA2 => {
                self.hdma.write_hdma2(value);
            }
            REG_HDMA3 => {
                self.hdma.write_hdma3(value);
            }
            REG_HDMA4 => {
                self.hdma.write_hdma4(value);
            }
            REG_HDMA5 => {
                if !self.cgb_mode {
                    return;
                }
                let (should_transfer_now, blocks_to_transfer) = self.hdma.write_hdma5(value);
                if should_transfer_now {
                    self.perform_hdma_transfer(blocks_to_transfer);
                }
            }
            REG_BGPI => self.write_bgpi(value),
            REG_BGPD => self.write_bgpdata(value),
            REG_OBPI => self.write_obpi(value),
            REG_OBPD => self.write_obpdata(value),
            REG_SVBK => {
                if self.cgb_mode {
                    let bank = value & 0x07;
                    self.wram_bank = if bank == 0 { 1 } else { bank };
                }
            }
            0xFF10..=0xFF14
            | 0xFF16..=0xFF19
            | 0xFF1A..=0xFF1E
            | 0xFF20..=0xFF23
            | 0xFF24..=0xFF26
            | 0xFF30..=0xFF3F => {
                self.apu.write_io(addr, value);
            }
            _ => self.io[(addr as usize - 0xFF00) % IO_SIZE] = value,
        }
    }

    fn read_joyp(&self) -> u8 {
        let mut value = 0x0F;
        if self.joyp_select & 0x10 == 0 {
            value &= self.joyp_dpad;
        }
        if self.joyp_select & 0x20 == 0 {
            value &= self.joyp_buttons;
        }
        0xC0 | self.joyp_select | value
    }

    fn read_key1(&self) -> u8 {
        let mut value = 0x00;
        if self.double_speed {
            value |= 0x80;
        }
        if self.speed_switch_pending {
            value |= 0x01;
        }
        value
    }

    fn read_key0(&self) -> u8 {
        let mut value = 0xFE;
        if self.cgb_mode {
            value |= 0x01;
        }
        value
    }

    fn read_bgpi(&self) -> u8 {
        let mut value = self.bg_palette_index;
        if self.bg_palette_auto_increment {
            value |= 0x80;
        }
        value
    }

    fn read_bgpdata(&self) -> u8 {
        let idx = self.bg_palette_index as usize;
        self.bg_palette_data[idx]
    }

    fn write_bgpi(&mut self, value: u8) {
        self.bg_palette_index = value & 0x3F;
        self.bg_palette_auto_increment = value & 0x80 != 0;
    }

    fn write_bgpdata(&mut self, value: u8) {
        let idx = self.bg_palette_index as usize;
        self.bg_palette_data[idx] = value;
        if self.bg_palette_auto_increment {
            self.bg_palette_index = self.bg_palette_index.wrapping_add(1) & 0x3F;
        }
    }

    fn read_obpi(&self) -> u8 {
        let mut value = self.ob_palette_index;
        if self.ob_palette_auto_increment {
            value |= 0x80;
        }
        value
    }

    fn read_obpdata(&self) -> u8 {
        let idx = self.ob_palette_index as usize;
        self.ob_palette_data[idx]
    }

    fn write_obpi(&mut self, value: u8) {
        self.ob_palette_index = value & 0x3F;
        self.ob_palette_auto_increment = value & 0x80 != 0;
    }

    fn write_obpdata(&mut self, value: u8) {
        let idx = self.ob_palette_index as usize;
        self.ob_palette_data[idx] = value;
        if self.ob_palette_auto_increment {
            self.ob_palette_index = self.ob_palette_index.wrapping_add(1) & 0x3F;
        }
    }
}
