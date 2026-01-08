use super::super::helpers::*;
use crate::domain::{Cartridge, RomBankMapping};

#[derive(Debug, Clone)]
pub struct HuC1 {
    rom_bank: u8,
    ram_bank: u8,
    ir_mode: bool,
    ir_signal: bool,
}

impl HuC1 {
    pub fn new() -> Self {
        Self {
            rom_bank: 1,
            ram_bank: 0,
            ir_mode: false,
            ir_signal: false,
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
                if self.ir_mode {
                    // IR mode: Read IR register
                    // Returns 0xC1 if IR signal detected (light), 0xC0 if not
                    if self.ir_signal { 0xC1 } else { 0xC0 }
                } else {
                    // RAM mode: Normal RAM access
                    let ram_bank = normalize_ram_bank(
                        self.ram_bank as usize,
                        ram_bank_count_for(cartridge, 4),
                    );
                    read_ext_ram(cartridge, ram_bank, addr)
                }
            }
            _ => OPEN_BUS,
        }
    }

    pub fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                // RAM/IR mode select
                // Write 0x0E for IR mode, anything else for RAM mode
                self.ir_mode = value == 0x0E;
            }
            0x2000..=0x3FFF => {
                // ROM bank select (6 bits minimum according to Pan Docs)
                let bank = value & 0x3F;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => {
                // RAM bank select (2 bits minimum)
                self.ram_bank = value & 0x03;
            }
            0x6000..=0x7FFF => {
                // Unused - games write here but it has no effect
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if self.ir_mode {
                    // IR mode: Write IR signal control
                    // 0x01 = IR on, 0x00 = IR off
                    self.ir_signal = value & 0x01 != 0;
                } else {
                    // RAM mode: Normal RAM write
                    let ram_bank = normalize_ram_bank(
                        self.ram_bank as usize,
                        ram_bank_count_for(cartridge, 4),
                    );
                    write_ext_ram(cartridge, ram_bank, addr, value);
                }
            }
            _ => {}
        }
    }
}
