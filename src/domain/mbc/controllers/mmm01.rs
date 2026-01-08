use super::super::helpers::*;
use crate::domain::Cartridge;
use crate::domain::cartridge::ROM_BANK_SIZE;

// MMM01 - Multi-game mapper
// Works like MBC1 but allows selecting which game to boot from a multi-game cartridge
// The cartridge boots from the last ROM bank, then software writes to special registers
// to configure the ROM/RAM window and then "maps" itself into the normal address space
#[derive(Debug, Clone)]
pub struct Mmm01 {
    // Before mapping mode is enabled, reads come from the end of ROM
    mapped: bool,

    // ROM bank selection (like MBC1)
    rom_bank_low: u8,  // Lower 5 bits
    rom_bank_high: u8, // Upper 2 bits

    // RAM bank selection
    ram_bank: u8,
    ram_enabled: bool,

    // Mode: false = ROM banking, true = RAM banking
    mode: bool,

    // Multiplex registers - define the ROM/RAM window
    rom_base: u8, // Base ROM bank offset
    rom_mask: u8, // ROM bank mask (determines window size)
    ram_base: u8, // Base RAM bank offset
    ram_mask: u8, // RAM bank mask
}

impl Mmm01 {
    pub fn new() -> Self {
        Self {
            mapped: false,
            rom_bank_low: 1,
            rom_bank_high: 0,
            ram_bank: 0,
            ram_enabled: false,
            mode: false,
            rom_base: 0,
            rom_mask: 0xFF, // Start with full ROM visible
            ram_base: 0,
            ram_mask: 0xFF,
        }
    }

    pub fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => {
                if !self.mapped {
                    // Before mapping, read from the last ROM bank
                    let bank_count = bank_count(&cartridge.bytes);
                    let last_bank = bank_count.saturating_sub(2);
                    let offset = (last_bank * ROM_BANK_SIZE) + (addr as usize);
                    return cartridge.bytes.get(offset).copied().unwrap_or(OPEN_BUS);
                }

                // After mapping, use mode logic like MBC1
                let bank_count = bank_count(&cartridge.bytes);
                let (fixed, _) = self.effective_banks(bank_count);
                let mapped_bank =
                    (self.rom_base as usize + (fixed & self.rom_mask as usize)) % bank_count.max(1);
                let offset = mapped_bank * ROM_BANK_SIZE + (addr as usize);
                cartridge.bytes.get(offset).copied().unwrap_or(OPEN_BUS)
            }
            0x4000..=0x7FFF => {
                if !self.mapped {
                    // Before mapping, read from last bank + 1
                    let bank_count = bank_count(&cartridge.bytes);
                    let last_bank = bank_count.saturating_sub(1);
                    let offset = (last_bank * ROM_BANK_SIZE) + (addr as usize - 0x4000);
                    return cartridge.bytes.get(offset).copied().unwrap_or(OPEN_BUS);
                }

                // After mapping, use switchable bank
                let bank_count = bank_count(&cartridge.bytes);
                let (_, switchable) = self.effective_banks(bank_count);
                let mapped_bank = (self.rom_base as usize + (switchable & self.rom_mask as usize))
                    % bank_count.max(1);
                let offset = mapped_bank * ROM_BANK_SIZE + (addr as usize - 0x4000);
                cartridge.bytes.get(offset).copied().unwrap_or(OPEN_BUS)
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return OPEN_BUS;
                }
                let ram_bank = (self.ram_base + (self.ram_bank & self.ram_mask)) & 0x03;
                read_ext_ram(cartridge, Some(ram_bank as usize), addr)
            }
            _ => OPEN_BUS,
        }
    }

    pub fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        if !self.mapped {
            // Before mapping, writes configure the multiplex registers
            match addr {
                0x0000..=0x1FFF => {
                    // ROM base bank configuration
                    self.rom_base = value & 0x3F;
                }
                0x2000..=0x3FFF => {
                    // ROM mask configuration
                    self.rom_mask = value & 0x3F;
                    if value & 0x40 != 0 {
                        // Enable mapping mode
                        self.mapped = true;
                    }
                }
                0x4000..=0x5FFF => {
                    // RAM base/mask configuration
                    self.ram_base = (value >> 2) & 0x03;
                    self.ram_mask = value & 0x03;
                }
                0x6000..=0x7FFF => {
                    // Mode select (ignored in unmapped mode)
                }
                _ => {}
            }
        } else {
            // After mapping, behave like MBC1
            match addr {
                0x0000..=0x1FFF => {
                    self.ram_enabled = (value & 0x0F) == 0x0A;
                }
                0x2000..=0x3FFF => {
                    let mut bank = value & 0x1F;
                    if bank == 0 {
                        bank = 1;
                    }
                    self.rom_bank_low = bank;
                }
                0x4000..=0x5FFF => {
                    self.rom_bank_high = value & 0x03;
                    self.ram_bank = value & 0x03;
                }
                0x6000..=0x7FFF => {
                    self.mode = (value & 0x01) != 0;
                }
                EXT_RAM_START..=EXT_RAM_END => {
                    if !self.ram_enabled {
                        return;
                    }
                    let ram_bank = (self.ram_base + (self.ram_bank & self.ram_mask)) & 0x03;
                    write_ext_ram(cartridge, Some(ram_bank as usize), addr, value);
                }
                _ => {}
            }
        }
    }

    // Calculate effective banks using MBC1 logic
    fn effective_banks(&self, bank_count: usize) -> (usize, usize) {
        let low5 = self.rom_bank_low as usize;
        let upper = self.rom_bank_high as usize;

        if self.mode {
            // RAM banking mode - upper bits affect both banks
            let fixed = normalize_bank(upper << 5, bank_count);
            let switchable = normalize_switchable_bank(low5 | (upper << 5), bank_count);
            (fixed, switchable)
        } else {
            // ROM banking mode - upper bits only affect switchable bank
            let fixed = 0;
            let switchable = normalize_switchable_bank(low5 | (upper << 5), bank_count);
            (fixed, switchable)
        }
    }
}
