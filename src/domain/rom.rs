const MIN_ROM_SIZE: usize = 0x0150;
const TITLE_START: usize = 0x0134;
const TITLE_END_DMG: usize = 0x0143;
const TITLE_END_CGB: usize = 0x0142;
const CGB_FLAG_ADDR: usize = 0x0143;
const NEW_LICENSEE_START: usize = 0x0144;
const SGB_FLAG_ADDR: usize = 0x0146;
const CARTRIDGE_TYPE_ADDR: usize = 0x0147;
const ROM_SIZE_ADDR: usize = 0x0148;
const RAM_SIZE_ADDR: usize = 0x0149;
const DESTINATION_CODE_ADDR: usize = 0x014A;
const OLD_LICENSEE_ADDR: usize = 0x014B;
const MASK_ROM_VERSION_ADDR: usize = 0x014C;
const HEADER_CHECKSUM_ADDR: usize = 0x014D;
const GLOBAL_CHECKSUM_ADDR: usize = 0x014E;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CgbFlag {
    DmgOnly,
    CgbSupported,
    CgbOnly,
}

impl CgbFlag {
    fn from_byte(value: u8) -> Self {
        match value {
            0x00 => Self::DmgOnly,
            0x80 => Self::CgbSupported,
            0xC0 => Self::CgbOnly,
            _ => Self::DmgOnly,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SgbFlag {
    None,
    Supported,
    Unknown(u8),
}

impl SgbFlag {
    fn from_byte(value: u8) -> Self {
        match value {
            0x00 => Self::None,
            0x03 => Self::Supported,
            other => Self::Unknown(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CartridgeType {
    RomOnly,
    Mbc1,
    Mbc1Ram,
    Mbc1RamBattery,
    Mbc2,
    Mbc2Battery,
    RomRam,
    RomRamBattery,
    Mmm01,
    Mmm01Ram,
    Mmm01RamBattery,
    Mbc3TimerBattery,
    Mbc3TimerRamBattery,
    Mbc3,
    Mbc3Ram,
    Mbc3RamBattery,
    Mbc5,
    Mbc5Ram,
    Mbc5RamBattery,
    Mbc5Rumble,
    Mbc5RumbleRam,
    Mbc5RumbleRamBattery,
    Mbc6,
    Mbc7SensorRumbleRamBattery,
    PocketCamera,
    BandaiTama5,
    HuC3,
    HuC1RamBattery,
    Unknown(u8),
}

impl CartridgeType {
    fn from_byte(value: u8) -> Self {
        match value {
            0x00 => Self::RomOnly,
            0x01 => Self::Mbc1,
            0x02 => Self::Mbc1Ram,
            0x03 => Self::Mbc1RamBattery,
            0x05 => Self::Mbc2,
            0x06 => Self::Mbc2Battery,
            0x08 => Self::RomRam,
            0x09 => Self::RomRamBattery,
            0x0B => Self::Mmm01,
            0x0C => Self::Mmm01Ram,
            0x0D => Self::Mmm01RamBattery,
            0x0F => Self::Mbc3TimerBattery,
            0x10 => Self::Mbc3TimerRamBattery,
            0x11 => Self::Mbc3,
            0x12 => Self::Mbc3Ram,
            0x13 => Self::Mbc3RamBattery,
            0x19 => Self::Mbc5,
            0x1A => Self::Mbc5Ram,
            0x1B => Self::Mbc5RamBattery,
            0x1C => Self::Mbc5Rumble,
            0x1D => Self::Mbc5RumbleRam,
            0x1E => Self::Mbc5RumbleRamBattery,
            0x20 => Self::Mbc6,
            0x22 => Self::Mbc7SensorRumbleRamBattery,
            0xFC => Self::PocketCamera,
            0xFD => Self::BandaiTama5,
            0xFE => Self::HuC3,
            0xFF => Self::HuC1RamBattery,
            other => Self::Unknown(other),
        }
    }

    pub fn code(self) -> u8 {
        match self {
            Self::RomOnly => 0x00,
            Self::Mbc1 => 0x01,
            Self::Mbc1Ram => 0x02,
            Self::Mbc1RamBattery => 0x03,
            Self::Mbc2 => 0x05,
            Self::Mbc2Battery => 0x06,
            Self::RomRam => 0x08,
            Self::RomRamBattery => 0x09,
            Self::Mmm01 => 0x0B,
            Self::Mmm01Ram => 0x0C,
            Self::Mmm01RamBattery => 0x0D,
            Self::Mbc3TimerBattery => 0x0F,
            Self::Mbc3TimerRamBattery => 0x10,
            Self::Mbc3 => 0x11,
            Self::Mbc3Ram => 0x12,
            Self::Mbc3RamBattery => 0x13,
            Self::Mbc5 => 0x19,
            Self::Mbc5Ram => 0x1A,
            Self::Mbc5RamBattery => 0x1B,
            Self::Mbc5Rumble => 0x1C,
            Self::Mbc5RumbleRam => 0x1D,
            Self::Mbc5RumbleRamBattery => 0x1E,
            Self::Mbc6 => 0x20,
            Self::Mbc7SensorRumbleRamBattery => 0x22,
            Self::PocketCamera => 0xFC,
            Self::BandaiTama5 => 0xFD,
            Self::HuC3 => 0xFE,
            Self::HuC1RamBattery => 0xFF,
            Self::Unknown(value) => value,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::RomOnly => "ROM only",
            Self::Mbc1 => "MBC1",
            Self::Mbc1Ram => "MBC1 + RAM",
            Self::Mbc1RamBattery => "MBC1 + RAM + Battery",
            Self::Mbc2 => "MBC2",
            Self::Mbc2Battery => "MBC2 + Battery",
            Self::RomRam => "ROM + RAM",
            Self::RomRamBattery => "ROM + RAM + Battery",
            Self::Mmm01 => "MMM01",
            Self::Mmm01Ram => "MMM01 + RAM",
            Self::Mmm01RamBattery => "MMM01 + RAM + Battery",
            Self::Mbc3TimerBattery => "MBC3 + Timer + Battery",
            Self::Mbc3TimerRamBattery => "MBC3 + Timer + RAM + Battery",
            Self::Mbc3 => "MBC3",
            Self::Mbc3Ram => "MBC3 + RAM",
            Self::Mbc3RamBattery => "MBC3 + RAM + Battery",
            Self::Mbc5 => "MBC5",
            Self::Mbc5Ram => "MBC5 + RAM",
            Self::Mbc5RamBattery => "MBC5 + RAM + Battery",
            Self::Mbc5Rumble => "MBC5 + Rumble",
            Self::Mbc5RumbleRam => "MBC5 + Rumble + RAM",
            Self::Mbc5RumbleRamBattery => "MBC5 + Rumble + RAM + Battery",
            Self::Mbc6 => "MBC6",
            Self::Mbc7SensorRumbleRamBattery => "MBC7 + Sensor + Rumble + RAM + Battery",
            Self::PocketCamera => "Pocket Camera",
            Self::BandaiTama5 => "Bandai TAMA5",
            Self::HuC3 => "HuC3",
            Self::HuC1RamBattery => "HuC1 + RAM + Battery",
            Self::Unknown(_) => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RomSize {
    Kb32,
    Kb64,
    Kb128,
    Kb256,
    Kb512,
    Mb1,
    Mb2,
    Mb4,
    Mb8,
    Mb1_1,
    Mb1_2,
    Mb1_5,
    Unknown(u8),
}

impl RomSize {
    fn from_byte(value: u8) -> Self {
        match value {
            0x00 => Self::Kb32,
            0x01 => Self::Kb64,
            0x02 => Self::Kb128,
            0x03 => Self::Kb256,
            0x04 => Self::Kb512,
            0x05 => Self::Mb1,
            0x06 => Self::Mb2,
            0x07 => Self::Mb4,
            0x08 => Self::Mb8,
            0x52 => Self::Mb1_1,
            0x53 => Self::Mb1_2,
            0x54 => Self::Mb1_5,
            other => Self::Unknown(other),
        }
    }

    pub fn code(self) -> u8 {
        match self {
            Self::Kb32 => 0x00,
            Self::Kb64 => 0x01,
            Self::Kb128 => 0x02,
            Self::Kb256 => 0x03,
            Self::Kb512 => 0x04,
            Self::Mb1 => 0x05,
            Self::Mb2 => 0x06,
            Self::Mb4 => 0x07,
            Self::Mb8 => 0x08,
            Self::Mb1_1 => 0x52,
            Self::Mb1_2 => 0x53,
            Self::Mb1_5 => 0x54,
            Self::Unknown(value) => value,
        }
    }

    pub fn bytes(&self) -> Option<usize> {
        let bytes = match self {
            Self::Kb32 => 32 * 1024,
            Self::Kb64 => 64 * 1024,
            Self::Kb128 => 128 * 1024,
            Self::Kb256 => 256 * 1024,
            Self::Kb512 => 512 * 1024,
            Self::Mb1 => 1024 * 1024,
            Self::Mb2 => 2 * 1024 * 1024,
            Self::Mb4 => 4 * 1024 * 1024,
            Self::Mb8 => 8 * 1024 * 1024,
            Self::Mb1_1 => 72 * 16 * 1024,
            Self::Mb1_2 => 80 * 16 * 1024,
            Self::Mb1_5 => 96 * 16 * 1024,
            Self::Unknown(_) => return None,
        };
        Some(bytes)
    }

    pub fn bank_count(&self) -> Option<usize> {
        self.bytes().map(|bytes| bytes / 0x4000)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamSize {
    None,
    Kb2,
    Kb8,
    Kb32,
    Kb64,
    Kb128,
    Unknown(u8),
}

impl RamSize {
    fn from_byte(value: u8) -> Self {
        match value {
            0x00 => Self::None,
            0x01 => Self::Kb2,
            0x02 => Self::Kb8,
            0x03 => Self::Kb32,
            0x04 => Self::Kb128,
            0x05 => Self::Kb64,
            other => Self::Unknown(other),
        }
    }

    pub fn code(self) -> u8 {
        match self {
            Self::None => 0x00,
            Self::Kb2 => 0x01,
            Self::Kb8 => 0x02,
            Self::Kb32 => 0x03,
            Self::Kb128 => 0x04,
            Self::Kb64 => 0x05,
            Self::Unknown(value) => value,
        }
    }

    pub fn bytes(&self) -> Option<usize> {
        let bytes = match self {
            Self::None => 0,
            Self::Kb2 => 2 * 1024,
            Self::Kb8 => 8 * 1024,
            Self::Kb32 => 32 * 1024,
            Self::Kb64 => 64 * 1024,
            Self::Kb128 => 128 * 1024,
            Self::Unknown(_) => return None,
        };
        Some(bytes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    Japan,
    NonJapan,
    Unknown(u8),
}

impl Destination {
    fn from_byte(value: u8) -> Self {
        match value {
            0x00 => Self::Japan,
            0x01 => Self::NonJapan,
            other => Self::Unknown(other),
        }
    }

    pub fn code(self) -> u8 {
        match self {
            Self::Japan => 0x00,
            Self::NonJapan => 0x01,
            Self::Unknown(value) => value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Licensee {
    Old(u8),
    New([u8; 2]),
}

impl Licensee {
    fn from_codes(old_code: u8, new_code: [u8; 2]) -> Self {
        if old_code == 0x33 {
            Self::New(new_code)
        } else {
            Self::Old(old_code)
        }
    }

    pub fn code_string(&self) -> String {
        match self {
            Self::Old(code) => format!("0x{:02X}", code),
            Self::New(code) => format_new_licensee_code(*code),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomHeader {
    pub title: String,
    pub cgb_flag: CgbFlag,
    pub sgb_flag: SgbFlag,
    pub cartridge_type: CartridgeType,
    pub rom_size: RomSize,
    pub ram_size: RamSize,
    pub destination: Destination,
    pub licensee: Licensee,
    pub mask_rom_version: u8,
    pub header_checksum: u8,
    pub global_checksum: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RomHeaderError {
    TooSmall { actual: usize },
}

impl RomHeader {
    pub fn parse(bytes: &[u8]) -> Result<Self, RomHeaderError> {
        if bytes.len() < MIN_ROM_SIZE {
            return Err(RomHeaderError::TooSmall {
                actual: bytes.len(),
            });
        }

        let cgb_flag_value = bytes[CGB_FLAG_ADDR];
        let cgb_flag = CgbFlag::from_byte(cgb_flag_value);
        let title_end = if matches!(cgb_flag, CgbFlag::CgbSupported | CgbFlag::CgbOnly) {
            TITLE_END_CGB
        } else {
            TITLE_END_DMG
        };

        let title_bytes = &bytes[TITLE_START..=title_end];
        let title = parse_title(title_bytes);

        let new_licensee_code = [bytes[NEW_LICENSEE_START], bytes[NEW_LICENSEE_START + 1]];
        let old_licensee_code = bytes[OLD_LICENSEE_ADDR];

        let header_checksum = bytes[HEADER_CHECKSUM_ADDR];
        let global_checksum =
            u16::from_be_bytes([bytes[GLOBAL_CHECKSUM_ADDR], bytes[GLOBAL_CHECKSUM_ADDR + 1]]);

        Ok(Self {
            title,
            cgb_flag,
            sgb_flag: SgbFlag::from_byte(bytes[SGB_FLAG_ADDR]),
            cartridge_type: CartridgeType::from_byte(bytes[CARTRIDGE_TYPE_ADDR]),
            rom_size: RomSize::from_byte(bytes[ROM_SIZE_ADDR]),
            ram_size: RamSize::from_byte(bytes[RAM_SIZE_ADDR]),
            destination: Destination::from_byte(bytes[DESTINATION_CODE_ADDR]),
            licensee: Licensee::from_codes(old_licensee_code, new_licensee_code),
            mask_rom_version: bytes[MASK_ROM_VERSION_ADDR],
            header_checksum,
            global_checksum,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rom {
    pub bytes: Vec<u8>,
    pub header: RomHeader,
}

impl Rom {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, RomHeaderError> {
        let header = RomHeader::parse(&bytes)?;
        Ok(Self { bytes, header })
    }
}

fn parse_title(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    let title = &bytes[..end];
    String::from_utf8_lossy(title).trim().to_string()
}

fn format_new_licensee_code(code: [u8; 2]) -> String {
    if code.iter().all(|byte| byte.is_ascii_graphic()) {
        String::from_utf8_lossy(&code).to_string()
    } else {
        format!("{:02X}{:02X}", code[0], code[1])
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CartridgeType, CgbFlag, Destination, Licensee, RamSize, RomHeader, RomHeaderError, RomSize,
        SgbFlag,
    };

    #[test]
    fn parse_header_dmg_title_includes_last_byte() {
        let mut rom = vec![0; super::MIN_ROM_SIZE];
        let title = b"HELLO WORLD 1234";
        rom[super::TITLE_START..=super::TITLE_END_DMG].copy_from_slice(title);

        let header = RomHeader::parse(&rom).expect("header parse");

        assert_eq!(header.title, "HELLO WORLD 1234");
        assert_eq!(header.cgb_flag, CgbFlag::DmgOnly);
    }

    #[test]
    fn parse_header_cgb_title_uses_15_bytes() {
        let mut rom = vec![0; super::MIN_ROM_SIZE];
        let title = b"CGB TITLE 12345";
        rom[super::TITLE_START..=super::TITLE_END_CGB].copy_from_slice(title);
        rom[super::CGB_FLAG_ADDR] = 0x80;

        let header = RomHeader::parse(&rom).expect("header parse");

        assert_eq!(header.title, "CGB TITLE 12345");
        assert_eq!(header.cgb_flag, CgbFlag::CgbSupported);
    }

    #[test]
    fn parse_header_reads_flags_and_sizes() {
        let mut rom = vec![0; super::MIN_ROM_SIZE];
        rom[super::SGB_FLAG_ADDR] = 0x03;
        rom[super::CARTRIDGE_TYPE_ADDR] = 0x01;
        rom[super::ROM_SIZE_ADDR] = 0x02;
        rom[super::RAM_SIZE_ADDR] = 0x03;
        rom[super::DESTINATION_CODE_ADDR] = 0x01;
        rom[super::NEW_LICENSEE_START] = b'0';
        rom[super::NEW_LICENSEE_START + 1] = b'1';
        rom[super::OLD_LICENSEE_ADDR] = 0x33;
        rom[super::MASK_ROM_VERSION_ADDR] = 0x12;
        rom[super::HEADER_CHECKSUM_ADDR] = 0xAB;
        rom[super::GLOBAL_CHECKSUM_ADDR] = 0x12;
        rom[super::GLOBAL_CHECKSUM_ADDR + 1] = 0x34;

        let header = RomHeader::parse(&rom).expect("header parse");

        assert_eq!(header.sgb_flag, SgbFlag::Supported);
        assert_eq!(header.cartridge_type, CartridgeType::Mbc1);
        assert_eq!(header.rom_size, RomSize::Kb128);
        assert_eq!(header.ram_size, RamSize::Kb32);
        assert_eq!(header.destination, Destination::NonJapan);
        assert_eq!(header.licensee, Licensee::New([b'0', b'1']));
        assert_eq!(header.mask_rom_version, 0x12);
        assert_eq!(header.header_checksum, 0xAB);
        assert_eq!(header.global_checksum, 0x1234);
    }

    #[test]
    fn parse_header_requires_minimum_length() {
        let rom = vec![0; super::MIN_ROM_SIZE - 1];

        let err = RomHeader::parse(&rom).expect_err("expected error");

        assert_eq!(
            err,
            RomHeaderError::TooSmall {
                actual: super::MIN_ROM_SIZE - 1
            }
        );
    }
}
