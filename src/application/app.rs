use std::path::{Path, PathBuf};

use crate::domain::{Cartridge, Emulator, RomHeader};
use crate::infrastructure::persistence::{
    AutoResumeMetadata, ResumeError, default_resume_path, load_last_session, save_last_session,
};
use crate::infrastructure::rom_loader::{self, RomLoadError, RomSaveError};

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

pub fn load_rom_with_save_root(
    path: impl AsRef<Path>,
    save_root: Option<&Path>,
) -> Result<Cartridge, RomLoadError> {
    rom_loader::load_rom_with_save_root(path, save_root)
}

pub fn save_battery_ram(path: impl AsRef<Path>, cartridge: &Cartridge) -> Result<(), RomSaveError> {
    rom_loader::save_battery_ram(path, cartridge)
}

pub fn save_battery_ram_with_root(
    path: impl AsRef<Path>,
    save_root: Option<&Path>,
    cartridge: &Cartridge,
) -> Result<(), RomSaveError> {
    rom_loader::save_battery_ram_with_root(path, save_root, cartridge)
}

pub fn save_auto_resume(metadata: &AutoResumeMetadata) -> Result<(), ResumeError> {
    save_last_session(default_resume_path(), metadata)
}

pub fn save_auto_resume_for(
    path: impl Into<PathBuf>,
    save_root: Option<PathBuf>,
) -> Result<(), ResumeError> {
    let metadata = AutoResumeMetadata::with_save_root(path, save_root);
    save_auto_resume(&metadata)
}

pub fn load_auto_resume() -> Result<Option<AutoResumeMetadata>, ResumeError> {
    load_last_session(default_resume_path())
}

pub fn load_auto_resume_path() -> Result<Option<(PathBuf, Option<PathBuf>)>, ResumeError> {
    Ok(load_auto_resume()?.map(|meta| (meta.rom_path, meta.save_root)))
}
