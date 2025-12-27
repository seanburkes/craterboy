use std::path::Path;

use crate::domain::{Cartridge, Emulator, RomHeader};
use crate::infrastructure::rom_loader::{self, RomLoadError};

pub fn run() {
    let _emulator = Emulator::new();
}

pub fn load_rom_header(path: impl AsRef<Path>) -> Result<RomHeader, RomLoadError> {
    let cartridge = load_rom(path)?;
    Ok(cartridge.header)
}

pub fn load_rom(path: impl AsRef<Path>) -> Result<Cartridge, RomLoadError> {
    rom_loader::load_rom(path)
}
