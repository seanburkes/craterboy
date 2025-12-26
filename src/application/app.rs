use std::path::Path;

use crate::domain::Emulator;
use crate::domain::RomHeader;
use crate::infrastructure::rom_loader::{self, RomLoadError};

pub fn run() {
    let _emulator = Emulator::new();
}

pub fn load_rom_header(path: impl AsRef<Path>) -> Result<RomHeader, RomLoadError> {
    let rom = rom_loader::load_rom(path)?;
    Ok(rom.header)
}
