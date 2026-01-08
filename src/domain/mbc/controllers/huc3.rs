use super::super::helpers::*;
use super::super::rtc::CYCLES_PER_SECOND;
use crate::domain::{Cartridge, RomBankMapping};

#[derive(Debug, Clone)]
pub struct HuC3 {
    rom_bank: u8,
    ram_bank: u8,
    ram_enable: bool,
    mode: u8,
    rtc_latched: bool,
    rtc_latch_value: u8,
    rtc_seconds: u32,
    rtc_minutes: u32,
    rtc_hours: u32,
    rtc_days: u32,
    ir_signal: u8,
}

impl HuC3 {
    pub fn new() -> Self {
        Self {
            rom_bank: 1,
            ram_bank: 0,
            ram_enable: false,
            mode: 0,
            rtc_latched: false,
            rtc_latch_value: 0,
            rtc_seconds: 0,
            rtc_minutes: 0,
            rtc_hours: 0,
            rtc_days: 0,
            ir_signal: 0,
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
                if !self.ram_enable {
                    return OPEN_BUS;
                }

                match self.mode {
                    0x00..=0x0B => {
                        // RAM access mode
                        let ram_bank = normalize_ram_bank(
                            self.ram_bank as usize,
                            ram_bank_count_for(cartridge, 4),
                        );
                        read_ext_ram(cartridge, ram_bank, addr)
                    }
                    0x0C => {
                        // RTC read mode
                        if self.rtc_latched {
                            match self.rtc_latch_value {
                                0x10 => (self.rtc_seconds & 0xFF) as u8,
                                0x30 => (self.rtc_minutes & 0xFF) as u8,
                                0x50 => (self.rtc_hours & 0xFF) as u8,
                                0x70 => (self.rtc_days & 0xFF) as u8,
                                _ => 0x01,
                            }
                        } else {
                            0x01
                        }
                    }
                    0x0D => {
                        // IR read mode
                        self.ir_signal
                    }
                    _ => OPEN_BUS,
                }
            }
            _ => OPEN_BUS,
        }
    }

    pub fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                // RAM enable: 0x0A enables, anything else disables
                self.ram_enable = value == 0x0A;
            }
            0x2000..=0x3FFF => {
                // ROM bank select (7 bits)
                let bank = value & 0x7F;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => {
                // RAM bank select or mode select
                self.ram_bank = value & 0x0F;
            }
            0x6000..=0x7FFF => {
                // Mode register
                self.mode = value;
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enable {
                    return;
                }

                match self.mode {
                    0x00..=0x0B => {
                        // RAM write mode
                        let ram_bank = normalize_ram_bank(
                            self.ram_bank as usize,
                            ram_bank_count_for(cartridge, 4),
                        );
                        write_ext_ram(cartridge, ram_bank, addr, value);
                    }
                    0x0C => {
                        // RTC write mode
                        match value & 0xF0 {
                            0x10 => {
                                // Latch/unlatch RTC
                                if value == 0x11 {
                                    self.rtc_latched = true;
                                    self.rtc_latch_value = 0x10;
                                } else if value == 0x10 {
                                    self.rtc_latched = false;
                                }
                            }
                            0x30 => {
                                if value == 0x31 {
                                    self.rtc_latched = true;
                                    self.rtc_latch_value = 0x30;
                                }
                            }
                            0x50 => {
                                if value == 0x51 {
                                    self.rtc_latched = true;
                                    self.rtc_latch_value = 0x50;
                                }
                            }
                            0x70 => {
                                if value == 0x71 {
                                    self.rtc_latched = true;
                                    self.rtc_latch_value = 0x70;
                                }
                            }
                            _ => {}
                        }
                    }
                    0x0E => {
                        // IR write mode
                        self.ir_signal = value;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        // HuC3 RTC runs at ~1Hz, advance based on CPU cycles
        // Game Boy CPU: 4194304 Hz (DMG) or 8388608 Hz (CGB double speed)
        // For simplicity, we increment every ~4.2M cycles (1 second in DMG mode)

        // Simple RTC tick (this is a basic implementation)
        // A full implementation would track cumulative cycles
        if cycles >= CYCLES_PER_SECOND / 60 {
            // Tick approximately every frame
            self.rtc_seconds += 1;
            if self.rtc_seconds >= 60 {
                self.rtc_seconds = 0;
                self.rtc_minutes += 1;
                if self.rtc_minutes >= 60 {
                    self.rtc_minutes = 0;
                    self.rtc_hours += 1;
                    if self.rtc_hours >= 24 {
                        self.rtc_hours = 0;
                        self.rtc_days += 1;
                    }
                }
            }
        }
    }
}
