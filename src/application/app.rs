use std::path::Path;

use crate::domain::{Emulator, Rom, RomHeader};
use crate::infrastructure::rom_loader::{self, RomLoadError};

pub fn run() {
    let _emulator = Emulator::new();
}

pub fn load_rom_header(path: impl AsRef<Path>) -> Result<RomHeader, RomLoadError> {
    let rom = load_rom(path)?;
    Ok(rom.header)
}

pub fn load_rom(path: impl AsRef<Path>) -> Result<Rom, RomLoadError> {
    rom_loader::load_rom(path)
}
