use std::path::Path;
use std::time::{Duration, Instant};

use crate::domain::Cartridge;
use crate::infrastructure::rom_loader::{RomSaveError, save_battery_ram};

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

#[cfg(test)]
mod tests {
    use super::SaveManager;
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
}
