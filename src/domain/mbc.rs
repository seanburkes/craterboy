use super::{Cartridge, CartridgeType, RomBankMapping};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MbcError {
    UnsupportedCartridgeType(CartridgeType),
}

#[derive(Debug, Clone)]
pub enum Mbc {
    RomOnly,
}

impl Mbc {
    pub fn new(cartridge: &Cartridge) -> Result<Self, MbcError> {
        match cartridge.header.cartridge_type {
            CartridgeType::RomOnly => Ok(Self::RomOnly),
            other => Err(MbcError::UnsupportedCartridgeType(other)),
        }
    }

    pub fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match self {
            Self::RomOnly => RomBankMapping::with_switchable_bank(&cartridge.bytes, 1).read(addr),
        }
    }
}
