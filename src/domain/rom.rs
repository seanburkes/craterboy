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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomHeader {
    pub title: String,
    pub cgb_flag: CgbFlag,
    pub sgb_flag: SgbFlag,
    pub cartridge_type: u8,
    pub rom_size_code: u8,
    pub ram_size_code: u8,
    pub destination_code: u8,
    pub new_licensee_code: [u8; 2],
    pub old_licensee_code: u8,
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

        let header_checksum = bytes[HEADER_CHECKSUM_ADDR];
        let global_checksum =
            u16::from_be_bytes([bytes[GLOBAL_CHECKSUM_ADDR], bytes[GLOBAL_CHECKSUM_ADDR + 1]]);

        Ok(Self {
            title,
            cgb_flag,
            sgb_flag: SgbFlag::from_byte(bytes[SGB_FLAG_ADDR]),
            cartridge_type: bytes[CARTRIDGE_TYPE_ADDR],
            rom_size_code: bytes[ROM_SIZE_ADDR],
            ram_size_code: bytes[RAM_SIZE_ADDR],
            destination_code: bytes[DESTINATION_CODE_ADDR],
            new_licensee_code,
            old_licensee_code: bytes[OLD_LICENSEE_ADDR],
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

#[cfg(test)]
mod tests {
    use super::{CgbFlag, RomHeader, RomHeaderError, SgbFlag};

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
        rom[super::OLD_LICENSEE_ADDR] = 0x33;
        rom[super::MASK_ROM_VERSION_ADDR] = 0x12;
        rom[super::HEADER_CHECKSUM_ADDR] = 0xAB;
        rom[super::GLOBAL_CHECKSUM_ADDR] = 0x12;
        rom[super::GLOBAL_CHECKSUM_ADDR + 1] = 0x34;

        let header = RomHeader::parse(&rom).expect("header parse");

        assert_eq!(header.sgb_flag, SgbFlag::Supported);
        assert_eq!(header.cartridge_type, 0x01);
        assert_eq!(header.rom_size_code, 0x02);
        assert_eq!(header.ram_size_code, 0x03);
        assert_eq!(header.destination_code, 0x01);
        assert_eq!(header.old_licensee_code, 0x33);
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
