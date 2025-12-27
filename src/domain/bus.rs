use super::cartridge::{Cartridge, RomBankMapping};

#[derive(Debug)]
pub struct Bus {
    cartridge: Cartridge,
    switchable_rom_bank: usize,
}

impl Bus {
    pub fn new(cartridge: Cartridge) -> Self {
        Self {
            cartridge,
            switchable_rom_bank: 1,
        }
    }

    pub fn cartridge(&self) -> &Cartridge {
        &self.cartridge
    }

    pub fn switchable_rom_bank(&self) -> usize {
        self.switchable_rom_bank
    }

    pub fn set_switchable_rom_bank(&mut self, bank: usize) {
        self.switchable_rom_bank = bank;
    }

    pub fn read8(&self, addr: u16) -> u8 {
        RomBankMapping::with_switchable_bank(&self.cartridge.bytes, self.switchable_rom_bank)
            .read(addr)
    }
}

#[cfg(test)]
mod tests {
    use super::Bus;
    use crate::domain::Cartridge;
    use crate::domain::cartridge::ROM_BANK_SIZE;

    #[test]
    fn bus_reads_from_selected_rom_bank() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 3];
        bytes[..ROM_BANK_SIZE].fill(0x10);
        bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x20);
        bytes[ROM_BANK_SIZE * 2..].fill(0x30);

        let cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut bus = Bus::new(cartridge);

        assert_eq!(bus.read8(0x0000), 0x10);
        assert_eq!(bus.read8(0x4000), 0x20);

        bus.set_switchable_rom_bank(2);
        assert_eq!(bus.read8(0x4000), 0x30);
    }
}
