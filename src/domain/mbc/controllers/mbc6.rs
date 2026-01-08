use super::super::helpers::*;
use crate::domain::{Cartridge, RomBankMapping};

const MBC6_FLASH_SIZE: usize = 128 * 1024; // 128KB flash
const MBC6_SRAM_SIZE: usize = 8 * 1024; // 8KB SRAM

#[derive(Debug, Clone)]
pub struct Mbc6 {
    // ROM banking - two separate banks
    rom_bank_a: u8, // 4000-5FFF
    rom_bank_b: u8, // 6000-7FFF

    // RAM/Flash banking
    flash_bank_a: u8, // A000-AFFF
    flash_bank_b: u8, // B000-BFFF

    // RAM enable
    ram_enable: bool,
    flash_enable: bool,

    // Flash memory (128KB)
    flash: [u8; MBC6_FLASH_SIZE],

    // SRAM (8KB)
    sram: [u8; MBC6_SRAM_SIZE],

    // Flash command register
    flash_command: u8,
}

impl Mbc6 {
    pub fn new() -> Self {
        Self {
            rom_bank_a: 2,
            rom_bank_b: 3,
            flash_bank_a: 0,
            flash_bank_b: 0,
            ram_enable: false,
            flash_enable: false,
            flash: [0xFF; MBC6_FLASH_SIZE],
            sram: [0; MBC6_SRAM_SIZE],
            flash_command: 0,
        }
    }

    pub fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match addr {
            // Bank 0: 0000-3FFF (fixed)
            0x0000..=0x3FFF => RomBankMapping::with_banks(&cartridge.bytes, 0, 0).read(addr),
            // Bank A: 4000-5FFF (switchable)
            0x4000..=0x5FFF => {
                let bank_count = bank_count(&cartridge.bytes);
                let bank = normalize_switchable_bank(self.rom_bank_a as usize, bank_count);
                let offset = (bank * ROM_BANK_SIZE) + (addr as usize - 0x4000);
                cartridge.bytes.get(offset).copied().unwrap_or(OPEN_BUS)
            }
            // Bank B: 6000-7FFF (switchable)
            0x6000..=0x7FFF => {
                let bank_count = bank_count(&cartridge.bytes);
                let bank = normalize_switchable_bank(self.rom_bank_b as usize, bank_count);
                let offset = (bank * ROM_BANK_SIZE) + (addr as usize - 0x6000);
                cartridge.bytes.get(offset).copied().unwrap_or(OPEN_BUS)
            }
            // Flash Bank A: A000-AFFF
            0xA000..=0xAFFF => {
                if !self.flash_enable {
                    return OPEN_BUS;
                }
                let flash_offset = (self.flash_bank_a as usize * 0x1000) + (addr as usize - 0xA000);
                if flash_offset < MBC6_FLASH_SIZE {
                    self.flash[flash_offset]
                } else {
                    OPEN_BUS
                }
            }
            // Flash Bank B or SRAM: B000-BFFF
            0xB000..=0xBFFF => {
                if self.ram_enable {
                    // SRAM mode
                    let sram_offset = (addr as usize - 0xB000) % MBC6_SRAM_SIZE;
                    self.sram[sram_offset]
                } else if self.flash_enable {
                    // Flash mode
                    let flash_offset =
                        (self.flash_bank_b as usize * 0x1000) + (addr as usize - 0xB000);
                    if flash_offset < MBC6_FLASH_SIZE {
                        self.flash[flash_offset]
                    } else {
                        OPEN_BUS
                    }
                } else {
                    OPEN_BUS
                }
            }
            _ => OPEN_BUS,
        }
    }

    pub fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        match addr {
            // RAM/Flash enable A (A000-AFFF): 0x0A enables flash
            0x0000..=0x0FFF => {
                self.flash_enable = value == 0x0A;
            }
            // RAM/Flash enable B (B000-BFFF): 0x0A enables SRAM
            0x1000..=0x1FFF => {
                self.ram_enable = value == 0x0A;
            }
            // Flash control A (2000-27FF): 0x00=read, 0x01=write, 0x10=erase
            0x2000..=0x27FF => {
                self.flash_command = value;
            }
            // Flash Bank A select (2800-28FF)
            0x2800..=0x28FF => {
                self.flash_bank_a = value & 0x7F; // 7-bit
            }
            // Flash control B (3000-37FF): 0x00=read, 0x01=write, 0x10=erase
            0x3000..=0x37FF => {
                self.flash_command = value;
            }
            // Flash Bank B select (3800-38FF)
            0x3800..=0x38FF => {
                self.flash_bank_b = value & 0x7F; // 7-bit
            }
            // ROM Bank A select (4000-4FFF)
            0x4000..=0x4FFF => {
                self.rom_bank_a = value & 0x7F; // 7-bit
            }
            // ROM Bank B select (5000-5FFF)
            0x5000..=0x5FFF => {
                self.rom_bank_b = value & 0x7F; // 7-bit
            }
            // Flash write A: A000-AFFF
            0xA000..=0xAFFF => {
                if !self.flash_enable {
                    return;
                }

                match self.flash_command {
                    0x01 => {
                        // Write mode
                        let flash_offset =
                            (self.flash_bank_a as usize * 0x1000) + (addr as usize - 0xA000);
                        if flash_offset < MBC6_FLASH_SIZE {
                            // Flash can only change 1 bits to 0 bits
                            self.flash[flash_offset] &= value;
                            cartridge.mark_ram_dirty();
                        }
                    }
                    0x10 => {
                        // Erase sector (4KB)
                        let sector_start = (self.flash_bank_a as usize) * 0x1000;
                        if sector_start + 0x1000 <= MBC6_FLASH_SIZE {
                            self.flash[sector_start..sector_start + 0x1000].fill(0xFF);
                            cartridge.mark_ram_dirty();
                        }
                    }
                    _ => {
                        // Read mode (0x00) or unknown - ignore writes
                    }
                }
            }
            // Flash write B or SRAM: B000-BFFF
            0xB000..=0xBFFF => {
                if self.ram_enable {
                    // SRAM write
                    let sram_offset = (addr as usize - 0xB000) % MBC6_SRAM_SIZE;
                    self.sram[sram_offset] = value;
                    cartridge.mark_ram_dirty();
                } else if self.flash_enable {
                    // Flash write B
                    match self.flash_command {
                        0x01 => {
                            // Write mode
                            let flash_offset =
                                (self.flash_bank_b as usize * 0x1000) + (addr as usize - 0xB000);
                            if flash_offset < MBC6_FLASH_SIZE {
                                // Flash can only change 1 bits to 0 bits
                                self.flash[flash_offset] &= value;
                                cartridge.mark_ram_dirty();
                            }
                        }
                        0x10 => {
                            // Erase sector (4KB)
                            let sector_start = (self.flash_bank_b as usize) * 0x1000;
                            if sector_start + 0x1000 <= MBC6_FLASH_SIZE {
                                self.flash[sector_start..sector_start + 0x1000].fill(0xFF);
                                cartridge.mark_ram_dirty();
                            }
                        }
                        _ => {
                            // Read mode (0x00) or unknown - ignore writes
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
