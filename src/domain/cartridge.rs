use super::rom::{RomHeader, RomHeaderError};

const ROM_BANK_SIZE: usize = 0x4000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cartridge {
    pub bytes: Vec<u8>,
    pub header: RomHeader,
}

impl Cartridge {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, RomHeaderError> {
        let header = RomHeader::parse(&bytes)?;
        Ok(Self { bytes, header })
    }

    pub fn banked_rom(&self) -> RomBankView<'_> {
        RomBankView::new(&self.bytes)
    }

    pub fn declared_bank_count(&self) -> Option<usize> {
        self.header.rom_size.bank_count()
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
}
