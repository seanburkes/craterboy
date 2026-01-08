mod controllers;
mod helpers;
mod rtc;

pub use rtc::{CYCLES_PER_SECOND, RtcMode};

use crate::domain::{Cartridge, CartridgeType};
use controllers::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MbcError {
    UnsupportedCartridgeType(CartridgeType),
}

#[derive(Debug, Clone)]
pub struct Mbc {
    kind: MbcKind,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
enum MbcKind {
    RomOnly,
    Mbc1(mbc1::Mbc1),
    Mmm01(mmm01::Mmm01),
    Mbc2(mbc2::Mbc2),
    Mbc3(mbc3::Mbc3),
    Mbc5(mbc5::Mbc5),
    HuC1(huc1::HuC1),
    HuC3(huc3::HuC3),
    Mbc6(Box<mbc6::Mbc6>),
    Mbc7(mbc7::Mbc7),
    PocketCamera(pocket_camera::PocketCamera),
    Tama5(tama5::Tama5),
}

impl Mbc {
    pub fn new(cartridge: &Cartridge) -> Result<Self, MbcError> {
        let kind = match cartridge.header.cartridge_type {
            CartridgeType::RomOnly | CartridgeType::RomRam | CartridgeType::RomRamBattery => {
                MbcKind::RomOnly
            }
            CartridgeType::Mbc1 | CartridgeType::Mbc1Ram | CartridgeType::Mbc1RamBattery => {
                MbcKind::Mbc1(mbc1::Mbc1::new())
            }
            CartridgeType::Mmm01 | CartridgeType::Mmm01Ram | CartridgeType::Mmm01RamBattery => {
                MbcKind::Mmm01(mmm01::Mmm01::new())
            }
            CartridgeType::Mbc2 | CartridgeType::Mbc2Battery => MbcKind::Mbc2(mbc2::Mbc2::new()),
            CartridgeType::Mbc3
            | CartridgeType::Mbc3Ram
            | CartridgeType::Mbc3RamBattery
            | CartridgeType::Mbc3TimerBattery
            | CartridgeType::Mbc3TimerRamBattery => {
                let has_rtc = matches!(
                    cartridge.header.cartridge_type,
                    CartridgeType::Mbc3TimerBattery | CartridgeType::Mbc3TimerRamBattery
                );
                MbcKind::Mbc3(mbc3::Mbc3::new(has_rtc))
            }
            CartridgeType::Mbc5
            | CartridgeType::Mbc5Ram
            | CartridgeType::Mbc5RamBattery
            | CartridgeType::Mbc5Rumble
            | CartridgeType::Mbc5RumbleRam
            | CartridgeType::Mbc5RumbleRamBattery => MbcKind::Mbc5(mbc5::Mbc5::new()),
            CartridgeType::HuC1RamBattery => MbcKind::HuC1(huc1::HuC1::new()),
            CartridgeType::HuC3 => MbcKind::HuC3(huc3::HuC3::new()),
            CartridgeType::Mbc6 => MbcKind::Mbc6(Box::new(mbc6::Mbc6::new())),
            CartridgeType::Mbc7SensorRumbleRamBattery => MbcKind::Mbc7(mbc7::Mbc7::new()),
            CartridgeType::PocketCamera => {
                MbcKind::PocketCamera(pocket_camera::PocketCamera::new())
            }
            CartridgeType::BandaiTama5 => MbcKind::Tama5(tama5::Tama5::new()),
            other => return Err(MbcError::UnsupportedCartridgeType(other)),
        };
        Ok(Self { kind })
    }

    pub fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match &self.kind {
            MbcKind::RomOnly => rom_only::read_rom_only(cartridge, addr),
            MbcKind::Mbc1(mbc1) => mbc1.read8(cartridge, addr),
            MbcKind::Mmm01(mmm01) => mmm01.read8(cartridge, addr),
            MbcKind::Mbc2(mbc2) => mbc2.read8(cartridge, addr),
            MbcKind::Mbc3(mbc3) => mbc3.read8(cartridge, addr),
            MbcKind::Mbc5(mbc5) => mbc5.read8(cartridge, addr),
            MbcKind::HuC1(huc1) => huc1.read8(cartridge, addr),
            MbcKind::HuC3(huc3) => huc3.read8(cartridge, addr),
            MbcKind::Mbc6(mbc6) => mbc6.read8(cartridge, addr),
            MbcKind::Mbc7(mbc7) => mbc7.read8(cartridge, addr),
            MbcKind::PocketCamera(cam) => cam.read8(cartridge, addr),
            MbcKind::Tama5(tama5) => tama5.read8(cartridge, addr),
        }
    }

    pub fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        match &mut self.kind {
            MbcKind::RomOnly => rom_only::write_rom_only(cartridge, addr, value),
            MbcKind::Mbc1(mbc1) => mbc1.write8(cartridge, addr, value),
            MbcKind::Mmm01(mmm01) => mmm01.write8(cartridge, addr, value),
            MbcKind::Mbc2(mbc2) => mbc2.write8(cartridge, addr, value),
            MbcKind::Mbc3(mbc3) => mbc3.write8(cartridge, addr, value),
            MbcKind::Mbc5(mbc5) => mbc5.write8(cartridge, addr, value),
            MbcKind::HuC1(huc1) => huc1.write8(cartridge, addr, value),
            MbcKind::HuC3(huc3) => huc3.write8(cartridge, addr, value),
            MbcKind::Mbc6(mbc6) => mbc6.write8(cartridge, addr, value),
            MbcKind::Mbc7(mbc7) => mbc7.write8(cartridge, addr, value),
            MbcKind::PocketCamera(cam) => cam.write8(cartridge, addr, value),
            MbcKind::Tama5(tama5) => tama5.write8(cartridge, addr, value),
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        match &mut self.kind {
            MbcKind::Mbc3(mbc3) => mbc3.tick(cycles),
            MbcKind::HuC3(huc3) => huc3.tick(cycles),
            _ => {}
        }
    }

    pub fn set_rtc_mode(&mut self, mode: RtcMode) {
        if let MbcKind::Mbc3(mbc3) = &mut self.kind {
            mbc3.set_rtc_mode(mode);
        }
    }
}

#[cfg(test)]
mod tests;
