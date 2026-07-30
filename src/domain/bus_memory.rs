use super::{
    Bus, CGB_BOOT_ROM_SIZE, DMG_BOOT_ROM_SIZE, HRAM_SIZE, IO_SIZE, OAM_SIZE, OPEN_BUS, REG_LCDC,
    VRAM_SIZE, WRAM_BANK_SIZE,
};

impl Bus {
    pub fn read8(&self, addr: u16) -> u8 {
        if self.boot_rom_enabled
            && let Some(boot_rom) = &self.boot_rom
        {
            if boot_rom.len() >= CGB_BOOT_ROM_SIZE {
                if addr <= 0x00FF {
                    return boot_rom[addr as usize];
                } else if (0x0200..=0x08FF).contains(&addr) {
                    let offset = addr - 0x0200 + 0x100;
                    return boot_rom[offset as usize];
                }
            } else if boot_rom.len() >= DMG_BOOT_ROM_SIZE && addr < DMG_BOOT_ROM_SIZE as u16 {
                return boot_rom[addr as usize];
            }
        }

        match addr {
            0x0000..=0x7FFF => self.mbc.read8(&self.cartridge, addr),
            0x8000..=0x9FFF => {
                if self.is_vram_accessible() {
                    self.vram[self.vram_bank as usize][(addr as usize - 0x8000) % VRAM_SIZE]
                } else {
                    0xFF
                }
            }
            0xA000..=0xBFFF => self.mbc.read8(&self.cartridge, addr),
            0xC000..=0xCFFF => self.wram[0][(addr as usize - 0xC000) % WRAM_BANK_SIZE],
            0xD000..=0xDFFF => {
                self.wram[self.wram_bank as usize][(addr as usize - 0xD000) % WRAM_BANK_SIZE]
            }
            0xE000..=0xEFFF => self.wram[0][(addr as usize - 0xE000) % WRAM_BANK_SIZE],
            0xF000..=0xFDFF => {
                self.wram[self.wram_bank as usize][(addr as usize - 0xF000) % WRAM_BANK_SIZE]
            }
            0xFE00..=0xFE9F => {
                if self.is_oam_accessible() {
                    self.oam[(addr as usize - 0xFE00) % OAM_SIZE]
                } else {
                    0xFF
                }
            }
            0xFEA0..=0xFEFF => OPEN_BUS,
            0xFF00..=0xFF7F => self.read_io(addr),
            0xFF80..=0xFFFE => self.hram[(addr as usize - 0xFF80) % HRAM_SIZE],
            0xFFFF => self.interrupt_enable,
        }
    }

    pub fn write8(&mut self, addr: u16, value: u8) {
        if addr == 0xFF50 && self.boot_rom_enabled && value != 0 {
            self.boot_rom_enabled = false;
            self.boot_rom_just_disabled = true;
        }

        match addr {
            0x0000..=0x7FFF => self.mbc.write8(&mut self.cartridge, addr, value),
            0x8000..=0x9FFF => {
                if self.is_vram_accessible() {
                    self.vram[self.vram_bank as usize][(addr as usize - 0x8000) % VRAM_SIZE] = value
                }
            }
            0xA000..=0xBFFF => self.mbc.write8(&mut self.cartridge, addr, value),
            0xC000..=0xCFFF => self.wram[0][(addr as usize - 0xC000) % WRAM_BANK_SIZE] = value,
            0xD000..=0xDFFF => {
                self.wram[self.wram_bank as usize][(addr as usize - 0xD000) % WRAM_BANK_SIZE] =
                    value
            }
            0xE000..=0xEFFF => self.wram[0][(addr as usize - 0xE000) % WRAM_BANK_SIZE] = value,
            0xF000..=0xFDFF => {
                self.wram[self.wram_bank as usize][(addr as usize - 0xF000) % WRAM_BANK_SIZE] =
                    value
            }
            0xFE00..=0xFE9F => {
                if self.is_oam_accessible() {
                    self.oam[(addr as usize - 0xFE00) % OAM_SIZE] = value
                }
            }
            0xFEA0..=0xFEFF => {}
            0xFF00..=0xFF7F => self.write_io(addr, value),
            0xFF80..=0xFFFE => self.hram[(addr as usize - 0xFF80) % HRAM_SIZE] = value,
            0xFFFF => self.interrupt_enable = value,
        }
    }

    pub(super) fn set_io_reg(&mut self, addr: u16, value: u8) {
        let idx = addr.wrapping_sub(0xFF00) as usize;
        if idx < IO_SIZE {
            self.io[idx] = value;
        }
    }

    pub(super) fn is_vram_accessible(&self) -> bool {
        let lcdc = self.read_io(REG_LCDC);
        lcdc & 0x80 == 0 || self.ppu_mode != 3
    }

    pub(super) fn is_oam_accessible(&self) -> bool {
        if self.dma.is_active() && self.dma.bytes_transferred() > 0 {
            return false;
        }

        let lcdc = self.read_io(REG_LCDC);
        if lcdc & 0x80 == 0 {
            return true;
        }

        self.ppu_mode != 2 && self.ppu_mode != 3
    }
}
