use super::super::helpers::*;
use crate::domain::{Cartridge, RomBankMapping};

// Pocket Camera (Game Boy Camera) - 0xFC
// Essentially MBC3 with additional camera hardware registers at A000-BFFF
#[derive(Debug, Clone)]
pub struct PocketCamera {
    rom_bank: u8,
    ram_bank: u8,
    ram_enabled: bool,
    // Camera registers (A000-A03F when camera mode is selected)
    // We implement a minimal stub - real camera hardware is complex
    camera_registers: [u8; 0x40],
}

impl PocketCamera {
    pub fn new() -> Self {
        Self {
            rom_bank: 1,
            ram_bank: 0,
            ram_enabled: false,
            camera_registers: [0; 0x40],
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

                // Camera registers are at A000-A03F
                // This is a simplified implementation - real hardware has complex image processing
                if addr < 0xA040 {
                    let offset = (addr - EXT_RAM_START) as usize;
                    return self.camera_registers[offset];
                }

                // Regular RAM banks
                read_ext_ram(cartridge, Some(self.ram_bank as usize), addr)
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
                let mut bank = value & 0x7F; // 7-bit bank selection (0-127)
                if bank == 0 {
                    bank = 1;
                }
                self.rom_bank = bank;
            }
            0x4000..=0x5FFF => {
                self.ram_bank = value & 0x0F; // Camera can have more RAM banks
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return;
                }

                // Camera registers
                if addr < 0xA040 {
                    let offset = (addr - EXT_RAM_START) as usize;
                    self.camera_registers[offset] = value;

                    // Special handling for camera trigger register (A000)
                    // Writing 1 to bit 0 starts camera capture
                    // After "capture", we set bit 0 to 0 to indicate completion
                    // This is a simplified simulation - real hardware takes time
                    if offset == 0 && (value & 0x01) != 0 {
                        // Simulate instant capture completion
                        self.camera_registers[0] &= !0x01;
                    }
                    return;
                }

                // Regular RAM write
                write_ext_ram(cartridge, Some(self.ram_bank as usize), addr, value);
            }
            _ => {}
        }
    }
}
