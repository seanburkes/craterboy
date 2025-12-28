use super::{Cartridge, Mbc, MbcError};

const BOOT_ROM_SIZE: usize = 0x100;

#[derive(Debug)]
pub struct Bus {
    cartridge: Cartridge,
    mbc: Mbc,
    boot_rom: Option<Vec<u8>>,
    boot_rom_enabled: bool,
}

impl Bus {
    pub fn new(cartridge: Cartridge) -> Result<Self, MbcError> {
        Self::with_boot_rom(cartridge, None)
    }

    pub fn with_boot_rom(
        cartridge: Cartridge,
        boot_rom: Option<Vec<u8>>,
    ) -> Result<Self, MbcError> {
        let mbc = Mbc::new(&cartridge)?;
        let boot_rom_enabled = boot_rom.is_some();
        Ok(Self {
            cartridge,
            mbc,
            boot_rom,
            boot_rom_enabled,
        })
    }

    pub fn cartridge(&self) -> &Cartridge {
        &self.cartridge
    }

    pub fn boot_rom_enabled(&self) -> bool {
        self.boot_rom_enabled
    }

    pub fn disable_boot_rom(&mut self) {
        self.boot_rom_enabled = false;
    }

    pub fn read8(&self, addr: u16) -> u8 {
        if self.boot_rom_enabled {
            if let Some(boot_rom) = &self.boot_rom {
                if (addr as usize) < BOOT_ROM_SIZE && boot_rom.len() >= BOOT_ROM_SIZE {
                    return boot_rom[addr as usize];
                }
            }
        }
        self.mbc.read8(&self.cartridge, addr)
    }

    pub fn write8(&mut self, addr: u16, value: u8) {
        if addr == 0xFF50 && self.boot_rom_enabled && value != 0 {
            self.boot_rom_enabled = false;
            return;
        }
        self.mbc.write8(&mut self.cartridge, addr, value);
    }
}

#[cfg(test)]
mod tests {
    use super::{BOOT_ROM_SIZE, Bus};
    use crate::domain::Cartridge;
    use crate::domain::cartridge::ROM_BANK_SIZE;

    #[test]
    fn bus_reads_from_selected_rom_bank() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 3];
        bytes[..ROM_BANK_SIZE].fill(0x10);
        bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x20);
        bytes[ROM_BANK_SIZE * 2..].fill(0x30);
        bytes[0x0147] = 0x00;

        let cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let bus = Bus::new(cartridge).expect("bus");

        assert_eq!(bus.read8(0x0000), 0x10);
        assert_eq!(bus.read8(0x4000), 0x20);
    }

    #[test]
    fn boot_rom_overlays_and_can_be_disabled() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[..ROM_BANK_SIZE].fill(0x11);
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");

        let boot_rom = vec![0xAA; BOOT_ROM_SIZE];
        let mut bus = Bus::with_boot_rom(cartridge, Some(boot_rom)).expect("bus");

        assert_eq!(bus.read8(0x0000), 0xAA);
        assert_eq!(bus.read8(0x00FF), 0xAA);
        assert_eq!(bus.read8(0x0100), 0x11);

        bus.write8(0xFF50, 0x01);
        assert!(!bus.boot_rom_enabled());
        assert_eq!(bus.read8(0x0000), 0x11);
    }
}
