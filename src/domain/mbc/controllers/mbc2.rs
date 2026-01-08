use super::super::helpers::*;
use crate::domain::{Cartridge, RomBankMapping};

#[derive(Debug, Clone)]
pub struct Mbc2 {
    rom_bank: u8,
    ram_enabled: bool,
}

impl Mbc2 {
    pub fn new() -> Self {
        Self {
            rom_bank: 1,
            ram_enabled: false,
        }
    }

    pub fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                let bank_count = bank_count(&cartridge.bytes);
                let bank = normalize_switchable_bank(self.rom_bank as usize, bank_count);
                RomBankMapping::with_banks(&cartridge.bytes, 0, bank).read(addr)
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return OPEN_BUS;
                }
                read_mbc2_ram(cartridge, addr)
            }
            _ => OPEN_BUS,
        }
    }

    pub fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                if addr & 0x0100 == 0 {
                    self.ram_enabled = (value & 0x0F) == 0x0A;
                }
            }
            0x2000..=0x3FFF => {
                if addr & 0x0100 != 0 {
                    let bank = value & 0x0F;
                    self.rom_bank = if bank == 0 { 1 } else { bank };
                }
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return;
                }
                write_mbc2_ram(cartridge, addr, value);
            }
            _ => {}
        }
    }
}
