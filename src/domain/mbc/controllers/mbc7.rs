use super::super::helpers::*;
use crate::domain::{Cartridge, RomBankMapping};

const MBC7_EEPROM_SIZE: usize = 256;
const MBC7_ACCEL_CENTER: u16 = 0x81D0;

#[derive(Debug, Clone)]
pub struct Mbc7 {
    rom_bank: u8,
    ram_enable_1: bool,
    ram_enable_2: bool,
    accel_x: u16,
    accel_y: u16,
    accel_latched: bool,
    eeprom: [u8; MBC7_EEPROM_SIZE],
    eeprom_cs: bool,
    eeprom_clk: bool,
    eeprom_di: bool,
    eeprom_do: bool,
    eeprom_write_enabled: bool,
    eeprom_command: u16,
    eeprom_bits: u8,
    eeprom_state: Mbc7EepromState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mbc7EepromState {
    Idle,
    ReadCommand,
    ReadData,
    _WriteCommand,
    WriteData,
    Busy,
}

impl Mbc7 {
    pub fn new() -> Self {
        Self {
            rom_bank: 1,
            ram_enable_1: false,
            ram_enable_2: false,
            accel_x: 0x8000,
            accel_y: 0x8000,
            accel_latched: false,
            eeprom: [0xFF; MBC7_EEPROM_SIZE],
            eeprom_cs: false,
            eeprom_clk: false,
            eeprom_di: false,
            eeprom_do: false,
            eeprom_write_enabled: false,
            eeprom_command: 0,
            eeprom_bits: 0,
            eeprom_state: Mbc7EepromState::Idle,
        }
    }

    pub fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                let bank_count = bank_count(&cartridge.bytes);
                let bank = normalize_switchable_bank(self.rom_bank as usize, bank_count);
                RomBankMapping::with_banks(&cartridge.bytes, 0, bank).read(addr)
            }
            0xA000..=0xAFFF => {
                if !self.ram_enable_1 || !self.ram_enable_2 {
                    return OPEN_BUS;
                }
                // Register access based on bits 4-7 of address
                let reg = (addr >> 4) & 0x0F;
                match reg {
                    0x2 => (self.accel_x & 0xFF) as u8,
                    0x3 => (self.accel_x >> 8) as u8,
                    0x4 => (self.accel_y & 0xFF) as u8,
                    0x5 => (self.accel_y >> 8) as u8,
                    0x6 => 0x00,
                    0x7 => 0xFF,
                    0x8 => {
                        // EEPROM register
                        let mut value = 0;
                        if self.eeprom_do {
                            value |= 0x01;
                        }
                        if self.eeprom_di {
                            value |= 0x02;
                        }
                        if self.eeprom_clk {
                            value |= 0x40;
                        }
                        if self.eeprom_cs {
                            value |= 0x80;
                        }
                        value
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
                // RAM Enable 1
                self.ram_enable_1 = (value & 0x0F) == 0x0A;
            }
            0x2000..=0x3FFF => {
                // ROM bank select (7-bit like MBC5)
                let bank = value & 0x7F;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => {
                // RAM Enable 2
                self.ram_enable_2 = value == 0x40;
            }
            0xA000..=0xAFFF => {
                if !self.ram_enable_1 || !self.ram_enable_2 {
                    return;
                }
                let reg = (addr >> 4) & 0x0F;
                match reg {
                    0x0 => {
                        // Latch erase - write 0x55 to reset accel values
                        if value == 0x55 {
                            self.accel_x = 0x8000;
                            self.accel_y = 0x8000;
                            self.accel_latched = false;
                        }
                    }
                    0x1 => {
                        // Latch accelerometer - write 0xAA to latch values
                        if value == 0xAA && !self.accel_latched {
                            // Emulate centered accelerometer (no tilt)
                            self.accel_x = MBC7_ACCEL_CENTER;
                            self.accel_y = MBC7_ACCEL_CENTER;
                            self.accel_latched = true;
                        }
                    }
                    0x8 => {
                        // EEPROM control
                        self.write_eeprom_register(cartridge, value);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn write_eeprom_register(&mut self, cartridge: &mut Cartridge, value: u8) {
        let new_cs = (value & 0x80) != 0;
        let new_clk = (value & 0x40) != 0;
        let new_di = (value & 0x02) != 0;

        // CS rising edge - start of operation
        if new_cs && !self.eeprom_cs {
            self.eeprom_command = 0;
            self.eeprom_bits = 0;
            self.eeprom_state = Mbc7EepromState::ReadCommand;
        }

        // CS falling edge - end of operation
        if !new_cs && self.eeprom_cs {
            self.eeprom_state = Mbc7EepromState::Idle;
            self.eeprom_bits = 0;
        }

        // Clock rising edge - shift in/out data
        if new_clk && !self.eeprom_clk && new_cs {
            match self.eeprom_state {
                Mbc7EepromState::ReadCommand => {
                    self.eeprom_command = (self.eeprom_command << 1) | (new_di as u16);
                    self.eeprom_bits += 1;

                    if self.eeprom_bits >= 10 {
                        self.process_eeprom_command(cartridge);
                    }
                }
                Mbc7EepromState::WriteData => {
                    self.eeprom_command = (self.eeprom_command << 1) | (new_di as u16);
                    self.eeprom_bits += 1;

                    if self.eeprom_bits >= 16 {
                        self.complete_eeprom_write(cartridge);
                    }
                }
                Mbc7EepromState::ReadData => {
                    // DO should already be valid; don't change it on rising edge
                    // Just increment the bit counter for the next falling edge
                }
                Mbc7EepromState::Busy => {
                    // Simulate ready after some clocks
                    self.eeprom_do = true;
                    self.eeprom_state = Mbc7EepromState::Idle;
                }
                _ => {}
            }
        }

        // Clock falling edge - prepare next bit for ReadData
        if !new_clk
            && self.eeprom_clk
            && new_cs
            && self.eeprom_state == Mbc7EepromState::ReadData
            && self.eeprom_bits < 16
        {
            // Advance to next bit only after the current bit has been read
            // (i.e., after a complete clock cycle)
            let addr = (self.eeprom_command & 0x7F) as usize;
            let next_bit = self.eeprom_bits + 1;
            if next_bit < 16 {
                let byte_offset = addr * 2 + (next_bit / 8) as usize;
                if byte_offset < MBC7_EEPROM_SIZE {
                    let byte = self.eeprom[byte_offset];
                    let bit_in_byte = 7 - (next_bit % 8);
                    self.eeprom_do = (byte >> bit_in_byte) & 1 != 0;
                }
            }
            self.eeprom_bits = next_bit;
        }

        self.eeprom_cs = new_cs;
        self.eeprom_clk = new_clk;
        self.eeprom_di = new_di;
    }

    fn process_eeprom_command(&mut self, _cartridge: &mut Cartridge) {
        let opcode = (self.eeprom_command >> 8) & 0x3;
        let addr = (self.eeprom_command & 0x7F) as usize;

        match opcode {
            0b10 => {
                // READ command
                self.eeprom_state = Mbc7EepromState::ReadData;
                self.eeprom_bits = 0;
                // Pre-load first bit
                if addr * 2 < MBC7_EEPROM_SIZE {
                    let byte = self.eeprom[addr * 2];
                    self.eeprom_do = ((byte >> 7) & 1) != 0;
                } else {
                    self.eeprom_do = true;
                }
            }
            0b01 => {
                // WRITE command
                if self.eeprom_write_enabled {
                    self.eeprom_state = Mbc7EepromState::WriteData;
                    self.eeprom_bits = 0;
                } else {
                    self.eeprom_state = Mbc7EepromState::Idle;
                }
            }
            0b11 => {
                // ERASE command
                if self.eeprom_write_enabled && addr * 2 + 1 < MBC7_EEPROM_SIZE {
                    self.eeprom[addr * 2] = 0xFF;
                    self.eeprom[addr * 2 + 1] = 0xFF;
                    self.eeprom_state = Mbc7EepromState::Busy;
                } else {
                    self.eeprom_state = Mbc7EepromState::Idle;
                }
            }
            0b00 => {
                // Special commands (EWEN, EWDS, ERAL, WRAL)
                let special = (self.eeprom_command >> 6) & 0x3;
                match special {
                    0b11 => {
                        // EWEN - Enable erase/write
                        self.eeprom_write_enabled = true;
                    }
                    0b00 => {
                        // EWDS - Disable erase/write
                        self.eeprom_write_enabled = false;
                    }
                    0b10 => {
                        // ERAL - Erase all
                        if self.eeprom_write_enabled {
                            self.eeprom.fill(0xFF);
                            self.eeprom_state = Mbc7EepromState::Busy;
                        }
                    }
                    0b01 => {
                        // WRAL - Write all (needs 16 bits of data)
                        if self.eeprom_write_enabled {
                            self.eeprom_state = Mbc7EepromState::WriteData;
                            self.eeprom_bits = 0;
                        }
                    }
                    _ => {}
                }
                if self.eeprom_state != Mbc7EepromState::WriteData {
                    self.eeprom_state = Mbc7EepromState::Idle;
                }
            }
            _ => {}
        }
    }

    fn complete_eeprom_write(&mut self, cartridge: &mut Cartridge) {
        let addr = (self.eeprom_command & 0x7F) as usize;

        // Get the last 16 bits shifted in
        let write_data = self.eeprom_command;

        if addr * 2 + 1 < MBC7_EEPROM_SIZE {
            let high_byte = (write_data >> 8) as u8;
            let low_byte = (write_data & 0xFF) as u8;

            if self.eeprom[addr * 2] != high_byte {
                self.eeprom[addr * 2] = high_byte;
                cartridge.mark_ram_dirty();
            }
            if self.eeprom[addr * 2 + 1] != low_byte {
                self.eeprom[addr * 2 + 1] = low_byte;
                cartridge.mark_ram_dirty();
            }
        }

        self.eeprom_state = Mbc7EepromState::Busy;
        self.eeprom_bits = 0;
    }
}
