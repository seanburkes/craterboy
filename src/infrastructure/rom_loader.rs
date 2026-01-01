use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::{Cartridge, RomHeaderError};

#[derive(Debug)]
pub enum RomLoadError {
    Io(std::io::Error),
    Header(RomHeaderError),
    SaveIo(std::io::Error),
}

impl From<std::io::Error> for RomLoadError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<RomHeaderError> for RomLoadError {
    fn from(err: RomHeaderError) -> Self {
        Self::Header(err)
    }
}

pub fn load_rom(path: impl AsRef<Path>) -> Result<Cartridge, RomLoadError> {
    load_rom_with_save_root(path, None)
}

pub fn load_rom_with_save_root(
    path: impl AsRef<Path>,
    save_root: Option<&Path>,
) -> Result<Cartridge, RomLoadError> {
    let path = path.as_ref();
    let bytes = std::fs::read(path)?;
    let mut cartridge = Cartridge::from_bytes(bytes)?;

    if cartridge.has_battery() && cartridge.has_ram() {
        let save_path = save_path_for_rom(path, save_root);
        if save_path.exists() {
            let ram = std::fs::read(&save_path).map_err(RomLoadError::SaveIo)?;
            cartridge.load_ram(&ram);
        }
    }

    Ok(cartridge)
}

#[derive(Debug)]
pub enum RomSaveError {
    Io(std::io::Error),
}

impl From<std::io::Error> for RomSaveError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

pub fn save_battery_ram(path: impl AsRef<Path>, cartridge: &Cartridge) -> Result<(), RomSaveError> {
    save_battery_ram_with_root(path, None, cartridge)
}

pub fn save_battery_ram_with_root(
    path: impl AsRef<Path>,
    save_root: Option<&Path>,
    cartridge: &Cartridge,
) -> Result<(), RomSaveError> {
    if !cartridge.has_battery() || !cartridge.has_ram() {
        return Ok(());
    }
    let save_path = save_path_for_rom(path.as_ref(), save_root);
    if let Some(parent) = save_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_atomic(&save_path, cartridge.ram())?;
    Ok(())
}

pub(crate) fn save_path_for_rom(path: &Path, save_root: Option<&Path>) -> PathBuf {
    let root = save_root
        .map(PathBuf::from)
        .unwrap_or_else(default_save_root);
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("rom");
    root.join(stem).join("ram.sav")
}

fn default_save_root() -> PathBuf {
    PathBuf::from("saves")
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<(), RomSaveError> {
    let mut temp_path = path.to_path_buf();
    let unique = format!(
        "tmp{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    if let Some(ext) = path.extension() {
        let mut ext = ext.to_os_string();
        ext.push(".");
        ext.push(unique);
        temp_path.set_extension(ext);
    } else {
        temp_path.set_extension(unique);
    }

    std::fs::write(&temp_path, data)?;
    std::fs::rename(&temp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load_rom_with_save_root, save_battery_ram_with_root, save_path_for_rom};
    use crate::domain::Cartridge;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn unique_path(name: &str) -> std::path::PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let filename = format!("{}_{}_{}", name, std::process::id(), id);
        std::env::temp_dir().join(filename)
    }

    fn unique_save_root() -> std::path::PathBuf {
        unique_path("craterboy_saves")
    }

    fn unique_rom_path() -> std::path::PathBuf {
        unique_path("craterboy_rom").with_extension("gb")
    }

    #[test]
    fn load_rom_reads_existing_save() {
        let rom_path = unique_rom_path();
        let save_root = unique_save_root();
        let save_path = save_path_for_rom(&rom_path, Some(&save_root));
        if let Some(parent) = save_path.parent() {
            std::fs::create_dir_all(parent).expect("save dir");
        }
        let mut rom = vec![0; 0x0150];
        rom[0x0147] = 0x09;
        rom[0x0149] = 0x02;

        std::fs::write(&rom_path, &rom).expect("rom write");
        std::fs::write(&save_path, vec![0xAA; 0x2000]).expect("save write");

        let cartridge = load_rom_with_save_root(&rom_path, Some(&save_root)).expect("load");
        assert_eq!(cartridge.ext_ram.len(), 0x2000);
        assert_eq!(cartridge.ext_ram[0], 0xAA);

        let _ = std::fs::remove_file(&rom_path);
        let _ = std::fs::remove_file(&save_path);
        let _ = std::fs::remove_dir_all(&save_root);
    }

    #[test]
    fn save_battery_ram_writes_save_file() {
        let rom_path = unique_rom_path();
        let mut rom = vec![0; 0x0150];
        rom[0x0147] = 0x09;
        rom[0x0149] = 0x02;
        std::fs::write(&rom_path, &rom).expect("rom write");

        let save_root = unique_save_root();
        let mut cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        cartridge.ext_ram.fill(0x5A);
        save_battery_ram_with_root(&rom_path, Some(&save_root), &cartridge).expect("save");

        let save_path = save_path_for_rom(&rom_path, Some(&save_root));
        let saved = std::fs::read(&save_path).expect("save read");
        assert_eq!(saved.len(), 0x2000);
        assert_eq!(saved[0], 0x5A);

        let _ = std::fs::remove_file(&rom_path);
        let _ = std::fs::remove_file(&save_path);
        let _ = std::fs::remove_dir_all(&save_root);
    }

    #[test]
    fn save_battery_ram_replaces_existing_file() {
        let rom_path = unique_rom_path();
        let mut rom = vec![0; 0x0150];
        rom[0x0147] = 0x09;
        rom[0x0149] = 0x02;
        std::fs::write(&rom_path, &rom).expect("rom write");

        let save_root = unique_save_root();
        let save_path = save_path_for_rom(&rom_path, Some(&save_root));
        if let Some(parent) = save_path.parent() {
            std::fs::create_dir_all(parent).expect("save dir");
        }
        std::fs::write(&save_path, vec![0x11; 0x2000]).expect("save write");

        let mut cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        cartridge.ext_ram.fill(0x22);
        save_battery_ram_with_root(&rom_path, Some(&save_root), &cartridge).expect("save");

        let saved = std::fs::read(&save_path).expect("save read");
        assert_eq!(saved[0], 0x22);

        let _ = std::fs::remove_file(&rom_path);
        let _ = std::fs::remove_file(&save_path);
        let _ = std::fs::remove_dir_all(&save_root);
    }
}
