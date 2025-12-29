use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::domain::Cartridge;
use crate::infrastructure::rom_loader::{RomSaveError, save_battery_ram};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct SaveManager {
    inactivity: Duration,
    last_dirty_at: Option<Instant>,
    last_dirty_generation: u64,
}

impl SaveManager {
    pub fn new(inactivity: Duration) -> Self {
        Self {
            inactivity,
            last_dirty_at: None,
            last_dirty_generation: 0,
        }
    }

    pub fn maybe_flush(
        &mut self,
        path: impl AsRef<Path>,
        cartridge: &mut Cartridge,
    ) -> Result<bool, RomSaveError> {
        self.maybe_flush_at(Instant::now(), path, cartridge)
    }

    pub fn maybe_flush_at(
        &mut self,
        now: Instant,
        path: impl AsRef<Path>,
        cartridge: &mut Cartridge,
    ) -> Result<bool, RomSaveError> {
        if !cartridge.is_ram_dirty() {
            self.last_dirty_at = None;
            return Ok(false);
        }

        let generation = cartridge.ram_dirty_generation();
        if self.last_dirty_generation != generation {
            self.last_dirty_generation = generation;
            self.last_dirty_at = Some(now);
        }

        let should_flush = self
            .last_dirty_at
            .is_some_and(|dirty_at| now.duration_since(dirty_at) >= self.inactivity);
        if !should_flush {
            return Ok(false);
        }

        save_battery_ram(path, cartridge)?;
        cartridge.clear_ram_dirty();
        self.last_dirty_at = None;
        Ok(true)
    }

    pub fn flush_now(
        &mut self,
        path: impl AsRef<Path>,
        cartridge: &mut Cartridge,
    ) -> Result<bool, RomSaveError> {
        if !cartridge.is_ram_dirty() {
            return Ok(false);
        }
        save_battery_ram(path, cartridge)?;
        cartridge.clear_ram_dirty();
        self.last_dirty_at = None;
        Ok(true)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoResumeMetadata {
    pub rom_path: PathBuf,
    pub state_path: Option<PathBuf>,
    pub saved_at_unix: u64,
}

impl AutoResumeMetadata {
    pub fn new(rom_path: impl Into<PathBuf>) -> Self {
        let saved_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            rom_path: rom_path.into(),
            state_path: None,
            saved_at_unix,
        }
    }
}

#[derive(Debug)]
pub enum ResumeError {
    Io(std::io::Error),
    Codec(Box<bincode::ErrorKind>),
}

impl From<std::io::Error> for ResumeError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<Box<bincode::ErrorKind>> for ResumeError {
    fn from(err: Box<bincode::ErrorKind>) -> Self {
        Self::Codec(err)
    }
}

pub fn save_last_session(
    path: impl AsRef<Path>,
    metadata: &AutoResumeMetadata,
) -> Result<(), ResumeError> {
    let bytes = bincode::serialize(metadata)?;
    write_atomic(path.as_ref(), &bytes)?;
    Ok(())
}

pub fn load_last_session(path: impl AsRef<Path>) -> Result<Option<AutoResumeMetadata>, ResumeError> {
    let path = path.as_ref();
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    let metadata = bincode::deserialize(&bytes)?;
    Ok(Some(metadata))
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<(), ResumeError> {
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
    use super::SaveManager;
    use super::{AutoResumeMetadata, load_last_session, save_last_session};
    use crate::domain::Cartridge;
    use crate::infrastructure::rom_loader::save_path_for_rom;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn unique_rom_path() -> std::path::PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let filename = format!("craterboy_save_manager_{}_{}", std::process::id(), id);
        std::env::temp_dir().join(filename).with_extension("gb")
    }

    fn unique_meta_path() -> std::path::PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let filename = format!("craterboy_resume_{}_{}", std::process::id(), id);
        std::env::temp_dir().join(filename).with_extension("bin")
    }

    #[test]
    fn flushes_after_inactivity_and_clears_dirty() {
        let rom_path = unique_rom_path();
        let mut rom = vec![0; 0x0150];
        rom[0x0147] = 0x09;
        rom[0x0149] = 0x02;
        let mut cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        cartridge.ram_mut()[0] = 0x5A;

        let mut manager = SaveManager::new(Duration::from_secs(5));
        let start = Instant::now();
        assert!(
            !manager
                .maybe_flush_at(start, &rom_path, &mut cartridge)
                .expect("maybe flush")
        );

        assert!(
            manager
                .maybe_flush_at(start + Duration::from_secs(6), &rom_path, &mut cartridge)
                .expect("maybe flush")
        );
        assert!(!cartridge.is_ram_dirty());

        let save_path = save_path_for_rom(&rom_path);
        let saved = std::fs::read(&save_path).expect("save read");
        assert_eq!(saved[0], 0x5A);

        let _ = std::fs::remove_file(&save_path);
    }

    #[test]
    fn auto_resume_roundtrip() {
        let meta_path = unique_meta_path();
        let metadata = AutoResumeMetadata::new("roms/tetris.gb");
        save_last_session(&meta_path, &metadata).expect("save");

        let loaded = load_last_session(&meta_path).expect("load");
        assert_eq!(loaded, Some(metadata));

        let _ = std::fs::remove_file(&meta_path);
    }

    #[test]
    fn auto_resume_missing_is_none() {
        let meta_path = unique_meta_path();
        let loaded = load_last_session(&meta_path).expect("load");
        assert!(loaded.is_none());
    }
}
