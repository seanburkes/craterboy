use super::super::helpers::*;
use crate::domain::{Cartridge, RomBankMapping};

#[derive(Debug, Clone)]
pub struct Mbc5 {
    rom_bank_low: u8,
    rom_bank_high: u8,
    ram_bank: u8,
    ram_enabled: bool,
}

impl Mbc5 {
    pub fn new() -> Self {
        Self {
            rom_bank_low: 1,
            rom_bank_high: 0,
            ram_bank: 0,
            ram_enabled: false,
        }
    }

    pub fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                let bank_count = bank_count(&cartridge.bytes);
                let bank = ((self.rom_bank_high as usize) << 8) | self.rom_bank_low as usize;
                let bank = normalize_bank(bank, bank_count);
                RomBankMapping::with_banks(&cartridge.bytes, 0, bank).read(addr)
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return OPEN_BUS;
                }
                let ram_bank =
                    normalize_ram_bank(self.ram_bank as usize, ram_bank_count_for(cartridge, 16));
                read_ext_ram(cartridge, ram_bank, addr)
            }
            _ => OPEN_BUS,
        }
    }

    pub fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.ram_enabled = (value & 0x0F) == 0x0A;
            }
            0x2000..=0x2FFF => {
                self.rom_bank_low = value;
            }
            0x3000..=0x3FFF => {
                self.rom_bank_high = value & 0x01;
            }
            0x4000..=0x5FFF => {
                self.ram_bank = value & 0x0F;
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return;
                }
                let ram_bank =
                    normalize_ram_bank(self.ram_bank as usize, ram_bank_count_for(cartridge, 16));
                write_ext_ram(cartridge, ram_bank, addr, value);
            }
            _ => {}
        }
    }
}
