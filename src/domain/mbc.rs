use super::cartridge::ROM_BANK_SIZE;
use super::{Cartridge, CartridgeType, RomBankMapping};

const EXT_RAM_START: u16 = 0xA000;
const EXT_RAM_END: u16 = 0xBFFF;
const EXT_RAM_BANK_SIZE: usize = 0x2000;
const OPEN_BUS: u8 = 0xFF;

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
            CartridgeType::RomOnly | CartridgeType::RomRam | CartridgeType::RomRamBattery => {
                Ok(Self::RomOnly)
            }
            CartridgeType::Mbc1 | CartridgeType::Mbc1Ram | CartridgeType::Mbc1RamBattery => {
                Ok(Self::Mbc1(Mbc1::new()))
            }
            other => Err(MbcError::UnsupportedCartridgeType(other)),
        }
    }

    pub fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match self {
            Self::RomOnly => read_rom_only(cartridge, addr),
            Self::Mbc1(mbc1) => mbc1.read8(cartridge, addr),
        }
    }

    pub fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        match self {
            Self::RomOnly => write_rom_only(cartridge, addr, value),
            Self::Mbc1(mbc1) => mbc1.write8(cartridge, addr, value),
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
        match addr {
            0x0000..=0x7FFF => {
                let bank_count = bank_count(&cartridge.bytes);
                let (fixed_bank, switchable_bank) = self.rom_banks(bank_count);
                RomBankMapping::with_banks(&cartridge.bytes, fixed_bank, switchable_bank).read(addr)
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return OPEN_BUS;
                }
                let ram_bank = match self.mode {
                    Mbc1Mode::RomBanking => 0,
                    Mbc1Mode::RamBanking => self.ram_bank as usize,
                };
                read_ext_ram(cartridge, ram_bank, addr)
            }
            _ => OPEN_BUS,
        }
    }

    fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
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
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return;
                }
                let ram_bank = match self.mode {
                    Mbc1Mode::RomBanking => 0,
                    Mbc1Mode::RamBanking => self.ram_bank as usize,
                };
                write_ext_ram(cartridge, ram_bank, addr, value);
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

fn read_rom_only(cartridge: &Cartridge, addr: u16) -> u8 {
    match addr {
        0x0000..=0x7FFF => RomBankMapping::with_banks(&cartridge.bytes, 0, 1).read(addr),
        EXT_RAM_START..=EXT_RAM_END => read_ext_ram(cartridge, 0, addr),
        _ => OPEN_BUS,
    }
}

fn write_rom_only(cartridge: &mut Cartridge, addr: u16, value: u8) {
    if matches!(addr, EXT_RAM_START..=EXT_RAM_END) {
        write_ext_ram(cartridge, 0, addr, value);
    }
}

fn read_ext_ram(cartridge: &Cartridge, bank: usize, addr: u16) -> u8 {
    if cartridge.ext_ram.is_empty() {
        return OPEN_BUS;
    }
    let offset = addr as usize - EXT_RAM_START as usize;
    let index = bank * EXT_RAM_BANK_SIZE + offset;
    cartridge.ext_ram.get(index).copied().unwrap_or(OPEN_BUS)
}

fn write_ext_ram(cartridge: &mut Cartridge, bank: usize, addr: u16, value: u8) {
    if cartridge.ext_ram.is_empty() {
        return;
    }
    let offset = addr as usize - EXT_RAM_START as usize;
    let index = bank * EXT_RAM_BANK_SIZE + offset;
    if let Some(byte) = cartridge.ext_ram.get_mut(index) {
        *byte = value;
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
        bytes[0x0149] = 0x02;

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
        mbc.write8(&mut cartridge, 0x2000, 0x02);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x33);
        mbc.write8(&mut cartridge, 0x2000, 0x00);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
    }

    #[test]
    fn mbc1_ram_enable_gates_reads_and_writes() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x01;
        bytes[0x0149] = 0x02;

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        mbc.write8(&mut cartridge, 0xA000, 0x55);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);

        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0xA000, 0x55);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x55);

        mbc.write8(&mut cartridge, 0x0000, 0x00);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);
    }

    #[test]
    fn mbc1_ram_banking_selects_banks() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x03;
        bytes[0x0149] = 0x03;

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0xA000, 0x11);

        mbc.write8(&mut cartridge, 0x6000, 0x01);
        mbc.write8(&mut cartridge, 0x4000, 0x01);
        mbc.write8(&mut cartridge, 0xA000, 0x22);

        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x22);
        mbc.write8(&mut cartridge, 0x4000, 0x00);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x11);
    }

    #[test]
    fn rom_only_ram_reads_and_writes() {
        let mut bytes = vec![0; ROM_BANK_SIZE];
        bytes[0x0147] = 0x08;
        bytes[0x0149] = 0x02;

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        mbc.write8(&mut cartridge, 0xA000, 0x5A);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x5A);
    }

    #[test]
    fn bank_count_rounds_up() {
        let bytes = vec![0; ROM_BANK_SIZE + 1];
        assert_eq!(bank_count(&bytes), 2);
    }
}
