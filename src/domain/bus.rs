use super::{Cartridge, Mbc, MbcError};

#[derive(Debug)]
pub struct Bus {
    cartridge: Cartridge,
    mbc: Mbc,
}

impl Bus {
    pub fn new(cartridge: Cartridge) -> Result<Self, MbcError> {
        let mbc = Mbc::new(&cartridge)?;
        Ok(Self { cartridge, mbc })
    }

    pub fn cartridge(&self) -> &Cartridge {
        &self.cartridge
    }

    pub fn read8(&self, addr: u16) -> u8 {
        self.mbc.read8(&self.cartridge, addr)
    }

    pub fn write8(&mut self, addr: u16, value: u8) {
        self.mbc.write8(&mut self.cartridge, addr, value);
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
        bytes[0x0147] = 0x00;

        let cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let bus = Bus::new(cartridge).expect("bus");

        assert_eq!(bus.read8(0x0000), 0x10);
        assert_eq!(bus.read8(0x4000), 0x20);
    }
}
