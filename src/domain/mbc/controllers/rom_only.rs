use super::super::helpers::*;
use crate::domain::{Cartridge, RomBankMapping};

pub fn read_rom_only(cartridge: &Cartridge, addr: u16) -> u8 {
    match addr {
        0x0000..=0x7FFF => {
            let bank_count = bank_count(&cartridge.bytes);
            let bank = normalize_switchable_bank(1, bank_count);
            RomBankMapping::with_banks(&cartridge.bytes, 0, bank).read(addr)
        }
        EXT_RAM_START..=EXT_RAM_END => {
            let ram_bank = normalize_ram_bank(0, ram_bank_count_for(cartridge, 1));
            read_ext_ram(cartridge, ram_bank, addr)
        }
        _ => OPEN_BUS,
    }
}

pub fn write_rom_only(cartridge: &mut Cartridge, addr: u16, value: u8) {
    if matches!(addr, EXT_RAM_START..=EXT_RAM_END) {
        let ram_bank = normalize_ram_bank(0, ram_bank_count_for(cartridge, 1));
        write_ext_ram(cartridge, ram_bank, addr, value);
    }
}
