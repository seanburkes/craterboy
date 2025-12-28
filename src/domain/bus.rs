use super::{Cartridge, Mbc, MbcError};

const BOOT_ROM_SIZE: usize = 0x100;
const VRAM_SIZE: usize = 0x2000;
const WRAM_SIZE: usize = 0x2000;
const OAM_SIZE: usize = 0xA0;
const IO_SIZE: usize = 0x80;
const HRAM_SIZE: usize = 0x7F;
const OPEN_BUS: u8 = 0xFF;

#[derive(Debug)]
pub struct Bus {
    cartridge: Cartridge,
    mbc: Mbc,
    boot_rom: Option<Vec<u8>>,
    boot_rom_enabled: bool,
    vram: Vec<u8>,
    wram: Vec<u8>,
    oam: Vec<u8>,
    io: Vec<u8>,
    hram: Vec<u8>,
    interrupt_enable: u8,
}

impl Bus {
    pub fn new(cartridge: Cartridge) -> Result<Self, MbcError> {
        Self::with_boot_rom(cartridge, None)
    }

    pub fn with_boot_rom(
        cartridge: Cartridge,
        boot_rom: Option<Vec<u8>>,
    ) -> Result<Self, MbcError> {
        let mbc = Mbc::new(&cartridge)?;
        let boot_rom_enabled = boot_rom.is_some();
        Ok(Self {
            cartridge,
            mbc,
            boot_rom,
            boot_rom_enabled,
            vram: vec![0; VRAM_SIZE],
            wram: vec![0; WRAM_SIZE],
            oam: vec![0; OAM_SIZE],
            io: vec![0; IO_SIZE],
            hram: vec![0; HRAM_SIZE],
            interrupt_enable: 0,
        })
    }

    pub fn cartridge(&self) -> &Cartridge {
        &self.cartridge
    }

    pub fn boot_rom_enabled(&self) -> bool {
        self.boot_rom_enabled
    }

    pub fn disable_boot_rom(&mut self) {
        self.boot_rom_enabled = false;
    }

    pub fn read8(&self, addr: u16) -> u8 {
        if self.boot_rom_enabled {
            if let Some(boot_rom) = &self.boot_rom {
                if (addr as usize) < BOOT_ROM_SIZE && boot_rom.len() >= BOOT_ROM_SIZE {
                    return boot_rom[addr as usize];
                }
            }
        }

        match addr {
            0x0000..=0x7FFF => self.mbc.read8(&self.cartridge, addr),
            0x8000..=0x9FFF => self.vram[(addr as usize - 0x8000) % VRAM_SIZE],
            0xA000..=0xBFFF => self.mbc.read8(&self.cartridge, addr),
            0xC000..=0xDFFF => self.wram[(addr as usize - 0xC000) % WRAM_SIZE],
            0xE000..=0xFDFF => self.wram[(addr as usize - 0xE000) % WRAM_SIZE],
            0xFE00..=0xFE9F => self.oam[(addr as usize - 0xFE00) % OAM_SIZE],
            0xFEA0..=0xFEFF => OPEN_BUS,
            0xFF00..=0xFF7F => self.io[(addr as usize - 0xFF00) % IO_SIZE],
            0xFF80..=0xFFFE => self.hram[(addr as usize - 0xFF80) % HRAM_SIZE],
            0xFFFF => self.interrupt_enable,
        }
    }

    pub fn write8(&mut self, addr: u16, value: u8) {
        if addr == 0xFF50 && self.boot_rom_enabled && value != 0 {
            self.boot_rom_enabled = false;
        }

        match addr {
            0x0000..=0x7FFF => self.mbc.write8(&mut self.cartridge, addr, value),
            0x8000..=0x9FFF => self.vram[(addr as usize - 0x8000) % VRAM_SIZE] = value,
            0xA000..=0xBFFF => self.mbc.write8(&mut self.cartridge, addr, value),
            0xC000..=0xDFFF => self.wram[(addr as usize - 0xC000) % WRAM_SIZE] = value,
            0xE000..=0xFDFF => self.wram[(addr as usize - 0xE000) % WRAM_SIZE] = value,
            0xFE00..=0xFE9F => self.oam[(addr as usize - 0xFE00) % OAM_SIZE] = value,
            0xFEA0..=0xFEFF => {}
            0xFF00..=0xFF7F => self.io[(addr as usize - 0xFF00) % IO_SIZE] = value,
            0xFF80..=0xFFFE => self.hram[(addr as usize - 0xFF80) % HRAM_SIZE] = value,
            0xFFFF => self.interrupt_enable = value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BOOT_ROM_SIZE, Bus};
    use crate::domain::Cartridge;
    use crate::domain::cartridge::ROM_BANK_SIZE;

    #[test]
    fn bus_reads_from_selected_rom_bank() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 3];
        bytes[..ROM_BANK_SIZE].fill(0x10);
        bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x20);
        bytes[ROM_BANK_SIZE * 2..].fill(0x30);
        bytes[0x0147] = 0x00;

        let cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let bus = Bus::new(cartridge).expect("bus");

        assert_eq!(bus.read8(0x0000), 0x10);
        assert_eq!(bus.read8(0x4000), 0x20);
    }

    #[test]
    fn boot_rom_overlays_and_can_be_disabled() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[..ROM_BANK_SIZE].fill(0x11);
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");

        let boot_rom = vec![0xAA; BOOT_ROM_SIZE];
        let mut bus = Bus::with_boot_rom(cartridge, Some(boot_rom)).expect("bus");

        assert_eq!(bus.read8(0x0000), 0xAA);
        assert_eq!(bus.read8(0x00FF), 0xAA);
        assert_eq!(bus.read8(0x0100), 0x11);

        bus.write8(0xFF50, 0x01);
        assert!(!bus.boot_rom_enabled());
        assert_eq!(bus.read8(0x0000), 0x11);
    }

    #[test]
    fn bus_decodes_non_rom_regions() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(0x8000, 0x12);
        bus.write8(0xC000, 0x34);
        bus.write8(0xE000, 0x56);
        bus.write8(0xFE00, 0x78);
        bus.write8(0xFF80, 0x9A);
        bus.write8(0xFFFF, 0xBC);

        assert_eq!(bus.read8(0x8000), 0x12);
        assert_eq!(bus.read8(0xC000), 0x56);
        assert_eq!(bus.read8(0xE000), 0x56);
        assert_eq!(bus.read8(0xFE00), 0x78);
        assert_eq!(bus.read8(0xFF80), 0x9A);
        assert_eq!(bus.read8(0xFFFF), 0xBC);
    }
}
