use super::rom::{RomHeader, RomHeaderError};

pub(crate) const ROM_BANK_SIZE: usize = 0x4000;
const ROM_FIXED_START: usize = 0x0000;
const ROM_FIXED_END: usize = 0x3FFF;
const ROM_SWITCH_START: usize = 0x4000;
const ROM_SWITCH_END: usize = 0x7FFF;
const OPEN_BUS: u8 = 0xFF;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cartridge {
    pub bytes: Vec<u8>,
    pub header: RomHeader,
    pub ext_ram: Vec<u8>,
}

impl Cartridge {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, RomHeaderError> {
        let header = RomHeader::parse(&bytes)?;
        let ext_ram = vec![0; header.ram_size.bytes().unwrap_or(0)];
        Ok(Self {
            bytes,
            header,
            ext_ram,
        })
    }

    pub fn banked_rom(&self) -> RomBankView<'_> {
        RomBankView::new(&self.bytes)
    }

    pub fn rom_mapping(&self) -> RomBankMapping<'_> {
        RomBankMapping::new(&self.bytes)
    }

    pub fn read_rom(&self, addr: u16, switchable_bank: usize) -> u8 {
        RomBankMapping::with_switchable_bank(&self.bytes, switchable_bank).read(addr)
    }

    pub fn declared_bank_count(&self) -> Option<usize> {
        self.header.rom_size.bank_count()
    }

    pub fn has_ram(&self) -> bool {
        !self.ext_ram.is_empty()
    }

    pub fn has_battery(&self) -> bool {
        self.header.cartridge_type.has_battery()
    }

    pub fn ram(&self) -> &[u8] {
        &self.ext_ram
    }

    pub fn ram_mut(&mut self) -> &mut [u8] {
        &mut self.ext_ram
    }

    pub fn load_ram(&mut self, data: &[u8]) {
        let len = self.ext_ram.len().min(data.len());
        self.ext_ram[..len].copy_from_slice(&data[..len]);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RomBankView<'a> {
    bytes: &'a [u8],
}

impl<'a> RomBankView<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    pub fn bank_size(&self) -> usize {
        ROM_BANK_SIZE
    }

    pub fn bank_count(&self) -> usize {
        if self.bytes.is_empty() {
            0
        } else {
            (self.bytes.len() + ROM_BANK_SIZE - 1) / ROM_BANK_SIZE
        }
    }

    pub fn bank(&self, index: usize) -> Option<&'a [u8]> {
        let start = index.checked_mul(ROM_BANK_SIZE)?;
        if start >= self.bytes.len() {
            return None;
        }
        let end = (start + ROM_BANK_SIZE).min(self.bytes.len());
        Some(&self.bytes[start..end])
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RomBankMapping<'a> {
    bytes: &'a [u8],
    fixed_bank: usize,
    switchable_bank: usize,
}

impl<'a> RomBankMapping<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            fixed_bank: 0,
            switchable_bank: 1,
        }
    }

    pub fn with_switchable_bank(bytes: &'a [u8], switchable_bank: usize) -> Self {
        Self {
            bytes,
            fixed_bank: 0,
            switchable_bank,
        }
    }

    pub fn with_banks(bytes: &'a [u8], fixed_bank: usize, switchable_bank: usize) -> Self {
        Self {
            bytes,
            fixed_bank,
            switchable_bank,
        }
    }

    pub fn fixed_bank(&self) -> usize {
        self.fixed_bank
    }

    pub fn switchable_bank(&self) -> usize {
        self.switchable_bank
    }

    pub fn set_fixed_bank(&mut self, bank: usize) {
        self.fixed_bank = bank;
    }

    pub fn set_switchable_bank(&mut self, bank: usize) {
        self.switchable_bank = bank;
    }

    pub fn read(&self, addr: u16) -> u8 {
        let addr = addr as usize;
        match addr {
            ROM_FIXED_START..=ROM_FIXED_END => {
                let offset = addr - ROM_FIXED_START;
                self.read_bank(self.fixed_bank, offset)
            }
            ROM_SWITCH_START..=ROM_SWITCH_END => {
                let offset = addr - ROM_SWITCH_START;
                self.read_bank(self.switchable_bank, offset)
            }
            _ => OPEN_BUS,
        }
    }

    fn read_bank(&self, bank: usize, offset: usize) -> u8 {
        let base = match bank.checked_mul(ROM_BANK_SIZE) {
            Some(base) => base,
            None => return OPEN_BUS,
        };
        let index = match base.checked_add(offset) {
            Some(index) => index,
            None => return OPEN_BUS,
        };
        self.read_at(index)
    }

    fn read_at(&self, index: usize) -> u8 {
        self.bytes.get(index).copied().unwrap_or(OPEN_BUS)
    }
}

#[cfg(test)]
mod tests {
    use super::{Cartridge, ROM_BANK_SIZE};

    #[test]
    fn banked_rom_splits_into_16k_chunks() {
        let bytes = vec![0; ROM_BANK_SIZE * 2];
        let cart = Cartridge::from_bytes(bytes).expect("cartridge");
        let banks = cart.banked_rom();

        assert_eq!(banks.bank_count(), 2);
        assert_eq!(banks.bank(0).expect("bank 0").len(), ROM_BANK_SIZE);
        assert_eq!(banks.bank(1).expect("bank 1").len(), ROM_BANK_SIZE);
        assert!(banks.bank(2).is_none());
    }

    #[test]
    fn banked_rom_handles_partial_last_bank() {
        let bytes = vec![0; ROM_BANK_SIZE + 1];
        let cart = Cartridge::from_bytes(bytes).expect("cartridge");
        let banks = cart.banked_rom();

        assert_eq!(banks.bank_count(), 2);
        assert_eq!(banks.bank(0).expect("bank 0").len(), ROM_BANK_SIZE);
        assert_eq!(banks.bank(1).expect("bank 1").len(), 1);
    }

    #[test]
    fn rom_mapping_reads_fixed_and_switchable_banks() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[..ROM_BANK_SIZE].fill(0xAA);
        bytes[ROM_BANK_SIZE..].fill(0xBB);

        let cart = Cartridge::from_bytes(bytes).expect("cartridge");
        let mapping = cart.rom_mapping();

        assert_eq!(mapping.read(0x0000), 0xAA);
        assert_eq!(mapping.read(0x3FFF), 0xAA);
        assert_eq!(mapping.read(0x4000), 0xBB);
        assert_eq!(mapping.read(0x7FFF), 0xBB);
    }

    #[test]
    fn rom_mapping_switches_banks() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 3];
        bytes[..ROM_BANK_SIZE].fill(0x11);
        bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x22);
        bytes[ROM_BANK_SIZE * 2..].fill(0x33);

        let cart = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mapping = cart.rom_mapping();

        mapping.set_switchable_bank(2);
        assert_eq!(mapping.read(0x4000), 0x33);

        mapping.set_switchable_bank(0);
        assert_eq!(mapping.read(0x4000), 0x11);
    }

    #[test]
    fn rom_mapping_returns_open_bus_for_unmapped_addrs() {
        let bytes = vec![0; ROM_BANK_SIZE];
        let cart = Cartridge::from_bytes(bytes).expect("cartridge");
        let mapping = cart.rom_mapping();

        assert_eq!(mapping.read(0x8000), 0xFF);
    }
}
