use super::super::helpers::*;
use crate::domain::{Cartridge, RomBankMapping};

// Bandai TAMA5 - 0xFD
// Register-based interface with RTC, used by Tamagotchi game
// This is the most complex MBC with a unique register-based command system
#[derive(Debug, Clone)]
pub struct Tama5 {
    rom_bank: u8,

    // Register interface - TAMA5 uses a special register-based access method
    // Commands and data are written/read through specific addresses
    data_out: u8,     // Data register for reads
    data_in_low: u8,  // Lower 4 bits of data input
    data_in_high: u8, // Upper 4 bits of data input
    addr_low: u8,     // Lower 4 bits of address/command
    addr_high: u8,    // Upper 4 bits of address/command

    // RTC registers
    rtc_seconds: u8,    // 0-59
    rtc_minutes: u8,    // 0-59
    rtc_hours_low: u8,  // Lower digit of hours (0-9)
    rtc_hours_high: u8, // Upper digit of hours (0-2)
    rtc_days_low: u8,   // Lower 4 bits of days
    rtc_days_high: u8,  // Upper 4 bits of days

    // Special purpose RAM (32 registers, 4 bits each)
    // TAMA5 doesn't use standard RAM banks - it has internal registers
    ram: [u8; 32],

    // Command/mode state
    command_mode: u8,
}

impl Tama5 {
    pub fn new() -> Self {
        Self {
            rom_bank: 1,
            data_out: 0,
            data_in_low: 0,
            data_in_high: 0,
            addr_low: 0,
            addr_high: 0,
            rtc_seconds: 0,
            rtc_minutes: 0,
            rtc_hours_low: 0,
            rtc_hours_high: 0,
            rtc_days_low: 0,
            rtc_days_high: 0,
            ram: [0; 32],
            command_mode: 0,
        }
    }

    pub fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match addr {
            // ROM bank 0 (fixed)
            0x0000..=0x3FFF => {
                let offset = addr as usize;
                cartridge.bytes.get(offset).copied().unwrap_or(OPEN_BUS)
            }
            // ROM bank 1-31 (switchable)
            0x4000..=0x7FFF => {
                let bank_count = bank_count(&cartridge.bytes);
                let bank = normalize_switchable_bank(self.rom_bank as usize, bank_count);
                RomBankMapping::with_banks(&cartridge.bytes, 0, bank).read(addr)
            }
            // Register interface
            // TAMA5 uses A000-A001 for reading data
            EXT_RAM_START..=EXT_RAM_END => {
                // A000 returns lower 4 bits of data_out
                // A001 returns upper 4 bits of data_out
                if addr == 0xA000 {
                    self.data_out & 0x0F
                } else if addr == 0xA001 {
                    (self.data_out >> 4) & 0x0F
                } else {
                    OPEN_BUS
                }
            }
            _ => OPEN_BUS,
        }
    }

    pub fn write8(&mut self, _cartridge: &mut Cartridge, addr: u16, value: u8) {
        match addr {
            // ROM bank selection (0x0000-0x1FFF)
            0x0000..=0x1FFF => {
                // Lower 5 bits select ROM bank (0-31)
                let mut bank = value & 0x1F;
                if bank == 0 {
                    bank = 1;
                }
                self.rom_bank = bank;
            }
            // Command/Address writes
            0x2000..=0x3FFF => {
                // This area is used for writing commands and addresses
                // The exact behavior depends on the address
                // Simplified implementation
                self.command_mode = value;
            }
            0x4000..=0x5FFF => {
                // Additional command space
                self.addr_low = value & 0x0F;
            }
            0x6000..=0x7FFF => {
                // Additional command space
                self.addr_high = value & 0x0F;
            }
            // Register interface
            EXT_RAM_START..=EXT_RAM_END => {
                match addr {
                    // A000 = lower 4 bits of data input
                    0xA000 => {
                        self.data_in_low = value & 0x0F;
                    }
                    // A001 = upper 4 bits of data input
                    0xA001 => {
                        self.data_in_high = value & 0x0F;
                        // When upper bits are written, process the command
                        self.process_command();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn process_command(&mut self) {
        // Combine the input data
        let data = self.data_in_low | (self.data_in_high << 4);
        let addr = self.addr_low | (self.addr_high << 4);

        // TAMA5 command processing (simplified)
        // Real hardware has complex command modes
        // We implement basic read/write for RAM and RTC

        match self.command_mode {
            // RAM write
            0x00 => {
                if (addr as usize) < self.ram.len() {
                    self.ram[addr as usize] = data & 0x0F;
                }
            }
            // RAM read
            0x01 => {
                if (addr as usize) < self.ram.len() {
                    self.data_out = self.ram[addr as usize] & 0x0F;
                }
            }
            // RTC read
            0x04 => {
                self.data_out = match addr {
                    0x00 => self.rtc_seconds & 0x0F,
                    0x01 => (self.rtc_seconds >> 4) & 0x0F,
                    0x02 => self.rtc_minutes & 0x0F,
                    0x03 => (self.rtc_minutes >> 4) & 0x0F,
                    0x04 => self.rtc_hours_low & 0x0F,
                    0x05 => self.rtc_hours_high & 0x0F,
                    0x06 => self.rtc_days_low & 0x0F,
                    0x07 => self.rtc_days_high & 0x0F,
                    _ => 0,
                };
            }
            // RTC write
            0x05 => match addr {
                0x00 => self.rtc_seconds = (self.rtc_seconds & 0xF0) | (data & 0x0F),
                0x01 => self.rtc_seconds = (self.rtc_seconds & 0x0F) | ((data & 0x0F) << 4),
                0x02 => self.rtc_minutes = (self.rtc_minutes & 0xF0) | (data & 0x0F),
                0x03 => self.rtc_minutes = (self.rtc_minutes & 0x0F) | ((data & 0x0F) << 4),
                0x04 => self.rtc_hours_low = data & 0x0F,
                0x05 => self.rtc_hours_high = data & 0x0F,
                0x06 => self.rtc_days_low = data & 0x0F,
                0x07 => self.rtc_days_high = data & 0x0F,
                _ => {}
            },
            _ => {
                // Unknown command - do nothing
            }
        }
    }
}
