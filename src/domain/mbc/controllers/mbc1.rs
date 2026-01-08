use super::super::helpers::*;
use crate::domain::{Cartridge, RomBankMapping};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mbc1Mode {
    RomBanking,
    RamBanking,
}

#[derive(Debug, Clone)]
pub struct Mbc1 {
    rom_bank_low5: u8,
    bank_high2: u8,
    ram_bank: u8,
    mode: Mbc1Mode,
    ram_enabled: bool,
}

impl Mbc1 {
    pub fn new() -> Self {
        Self {
            rom_bank_low5: 1,
            bank_high2: 0,
            ram_bank: 0,
            mode: Mbc1Mode::RomBanking,
            ram_enabled: false,
        }
    }

    pub fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                let bank_count = bank_count(&cartridge.bytes);
                let (fixed_bank, switchable_bank) = self.rom_banks(bank_count);
                RomBankMapping::with_banks(&cartridge.bytes, fixed_bank, switchable_bank).read(addr)
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return OPEN_BUS;
                }
                let ram_bank = match self.mode {
                    Mbc1Mode::RomBanking => 0,
                    Mbc1Mode::RamBanking => self.ram_bank as usize,
                };
                let ram_bank = normalize_ram_bank(ram_bank, ram_bank_count_for(cartridge, 4));
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
            0x2000..=0x3FFF => {
                self.rom_bank_low5 = value & 0x1F;
            }
            0x4000..=0x5FFF => {
                let bank = value & 0x03;
                self.bank_high2 = bank;
                self.ram_bank = bank;
            }
            0x6000..=0x7FFF => {
                if value & 0x01 == 0 {
                    self.mode = Mbc1Mode::RomBanking;
                } else {
                    self.mode = Mbc1Mode::RamBanking;
                }
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return;
                }
                let ram_bank = match self.mode {
                    Mbc1Mode::RomBanking => 0,
                    Mbc1Mode::RamBanking => self.ram_bank as usize,
                };
                let ram_bank = normalize_ram_bank(ram_bank, ram_bank_count_for(cartridge, 4));
                write_ext_ram(cartridge, ram_bank, addr, value);
            }
            _ => {}
        }
    }

    fn rom_banks(&self, bank_count: usize) -> (usize, usize) {
        let mut low5 = (self.rom_bank_low5 & 0x1F) as usize;
        if low5 == 0 {
            low5 = 1;
        }
        let upper = (self.bank_high2 & 0x03) as usize;
        match self.mode {
            Mbc1Mode::RomBanking => {
                let switchable = normalize_switchable_bank((upper << 5) | low5, bank_count);
                (normalize_bank(0, bank_count), switchable)
            }
            Mbc1Mode::RamBanking => {
                let fixed = normalize_bank(upper << 5, bank_count);
                let switchable = normalize_switchable_bank(low5, bank_count);
                (fixed, switchable)
            }
        }
    }
}
