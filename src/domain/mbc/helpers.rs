use super::super::Cartridge;

pub const ROM_BANK_SIZE: usize = 0x4000;

pub const EXT_RAM_START: u16 = 0xA000;
pub const EXT_RAM_END: u16 = 0xBFFF;
pub const EXT_RAM_BANK_SIZE: usize = 0x2000;
pub const MBC2_RAM_SIZE: usize = 512;
pub const MBC2_RAM_END: u16 = 0xA1FF;
pub const OPEN_BUS: u8 = 0xFF;

pub fn bank_count(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        0
    } else {
        bytes.len().div_ceil(ROM_BANK_SIZE)
    }
}

pub fn normalize_bank(bank: usize, bank_count: usize) -> usize {
    if bank_count == 0 {
        0
    } else {
        bank % bank_count
    }
}

pub fn normalize_switchable_bank(bank: usize, bank_count: usize) -> usize {
    if bank_count == 0 {
        0
    } else {
        let mut normalized = bank % bank_count;
        if normalized == 0 && bank_count > 1 {
            normalized = 1;
        }
        normalized
    }
}

pub fn ram_bank_count_for(cartridge: &Cartridge, max_banks: usize) -> usize {
    if cartridge.ext_ram.is_empty() {
        return 0;
    }
    let banks = cartridge.ext_ram.len().div_ceil(EXT_RAM_BANK_SIZE);
    banks.min(max_banks)
}

pub fn normalize_ram_bank(bank: usize, bank_count: usize) -> Option<usize> {
    if bank_count == 0 {
        None
    } else {
        Some(bank % bank_count)
    }
}

pub fn read_ext_ram(cartridge: &Cartridge, bank: Option<usize>, addr: u16) -> u8 {
    if cartridge.ext_ram.is_empty() {
        return OPEN_BUS;
    }
    let Some(bank) = bank else {
        return OPEN_BUS;
    };
    let offset = addr as usize - EXT_RAM_START as usize;
    let index = bank * EXT_RAM_BANK_SIZE + offset;
    cartridge.ext_ram.get(index).copied().unwrap_or(OPEN_BUS)
}

pub fn write_ext_ram(cartridge: &mut Cartridge, bank: Option<usize>, addr: u16, value: u8) {
    if cartridge.ext_ram.is_empty() {
        return;
    }
    let Some(bank) = bank else {
        return;
    };
    let offset = addr as usize - EXT_RAM_START as usize;
    let index = bank * EXT_RAM_BANK_SIZE + offset;
    if let Some(byte) = cartridge.ext_ram.get_mut(index)
        && *byte != value
    {
        *byte = value;
        cartridge.mark_ram_dirty();
    }
}

pub fn read_mbc2_ram(cartridge: &Cartridge, addr: u16) -> u8 {
    if addr > MBC2_RAM_END {
        return OPEN_BUS;
    }
    if cartridge.ext_ram.len() < MBC2_RAM_SIZE {
        return OPEN_BUS;
    }
    let offset = (addr as usize - EXT_RAM_START as usize) & 0x01FF;
    let value = cartridge.ext_ram.get(offset).copied().unwrap_or(0) & 0x0F;
    0xF0 | value
}

pub fn write_mbc2_ram(cartridge: &mut Cartridge, addr: u16, value: u8) {
    if addr > MBC2_RAM_END {
        return;
    }
    if cartridge.ext_ram.len() < MBC2_RAM_SIZE {
        return;
    }
    let offset = (addr as usize - EXT_RAM_START as usize) & 0x01FF;
    if let Some(byte) = cartridge.ext_ram.get_mut(offset) {
        let value = value & 0x0F;
        if *byte != value {
            *byte = value;
            cartridge.mark_ram_dirty();
        }
    }
}
