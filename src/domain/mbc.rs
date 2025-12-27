use super::cartridge::ROM_BANK_SIZE;
use super::{Cartridge, CartridgeType, RomBankMapping};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MbcError {
    UnsupportedCartridgeType(CartridgeType),
}

#[derive(Debug, Clone)]
pub enum Mbc {
    RomOnly,
    Mbc1(Mbc1),
}

impl Mbc {
    pub fn new(cartridge: &Cartridge) -> Result<Self, MbcError> {
        match cartridge.header.cartridge_type {
            CartridgeType::RomOnly => Ok(Self::RomOnly),
            CartridgeType::Mbc1 | CartridgeType::Mbc1Ram | CartridgeType::Mbc1RamBattery => {
                Ok(Self::Mbc1(Mbc1::new()))
            }
            other => Err(MbcError::UnsupportedCartridgeType(other)),
        }
    }

    pub fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match self {
            Self::RomOnly => RomBankMapping::with_banks(&cartridge.bytes, 0, 1).read(addr),
            Self::Mbc1(mbc1) => mbc1.read8(cartridge, addr),
        }
    }

    pub fn write8(&mut self, addr: u16, value: u8) {
        match self {
            Self::RomOnly => {}
            Self::Mbc1(mbc1) => mbc1.write8(addr, value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mbc1Mode {
    RomBanking,
    RamBanking,
}

#[derive(Debug, Clone)]
struct Mbc1 {
    rom_bank_low5: u8,
    bank_high2: u8,
    ram_bank: u8,
    mode: Mbc1Mode,
    ram_enabled: bool,
}

impl Mbc1 {
    fn new() -> Self {
        Self {
            rom_bank_low5: 1,
            bank_high2: 0,
            ram_bank: 0,
            mode: Mbc1Mode::RomBanking,
            ram_enabled: false,
        }
    }

    fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        let bank_count = bank_count(&cartridge.bytes);
        let (fixed_bank, switchable_bank) = self.rom_banks(bank_count);
        RomBankMapping::with_banks(&cartridge.bytes, fixed_bank, switchable_bank).read(addr)
    }

    fn write8(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.ram_enabled = (value & 0x0F) == 0x0A;
            }
            0x2000..=0x3FFF => {
                self.rom_bank_low5 = value & 0x1F;
            }
            0x4000..=0x5FFF => {
                let bank = value & 0x03;
                self.bank_high2 = bank;
                self.ram_bank = bank;
            }
            0x6000..=0x7FFF => {
                if value & 0x01 == 0 {
                    self.mode = Mbc1Mode::RomBanking;
                } else {
                    self.mode = Mbc1Mode::RamBanking;
                }
            }
            _ => {}
        }
    }

    fn rom_banks(&self, bank_count: usize) -> (usize, usize) {
        let mut low5 = (self.rom_bank_low5 & 0x1F) as usize;
        if low5 == 0 {
            low5 = 1;
        }
        let upper = (self.bank_high2 & 0x03) as usize;
        match self.mode {
            Mbc1Mode::RomBanking => {
                let switchable = normalize_switchable_bank((upper << 5) | low5, bank_count);
                (normalize_bank(0, bank_count), switchable)
            }
            Mbc1Mode::RamBanking => {
                let fixed = normalize_bank(upper << 5, bank_count);
                let switchable = normalize_switchable_bank(low5, bank_count);
                (fixed, switchable)
            }
        }
    }
}

fn bank_count(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        0
    } else {
        (bytes.len() + ROM_BANK_SIZE - 1) / ROM_BANK_SIZE
    }
}

fn normalize_bank(bank: usize, bank_count: usize) -> usize {
    if bank_count == 0 {
        0
    } else {
        bank % bank_count
    }
}

fn normalize_switchable_bank(bank: usize, bank_count: usize) -> usize {
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

#[cfg(test)]
mod tests {
    use super::{Mbc, bank_count};
    use crate::domain::Cartridge;
    use crate::domain::cartridge::ROM_BANK_SIZE;

    #[test]
    fn mbc1_write_changes_switchable_rom_bank() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 4];
        bytes[..ROM_BANK_SIZE].fill(0x11);
        bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x22);
        bytes[ROM_BANK_SIZE * 2..ROM_BANK_SIZE * 3].fill(0x33);
        bytes[ROM_BANK_SIZE * 3..].fill(0x44);
        bytes[0x0147] = 0x01;

        let cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
        mbc.write8(0x2000, 0x02);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x33);
        mbc.write8(0x2000, 0x00);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
    }

    #[test]
    fn bank_count_rounds_up() {
        let bytes = vec![0; ROM_BANK_SIZE + 1];
        assert_eq!(bank_count(&bytes), 2);
    }
}
