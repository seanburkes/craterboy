use super::{Cartridge, Mbc, MbcError, RtcMode};

const BOOT_ROM_SIZE: usize = 0x100;
const VRAM_SIZE: usize = 0x2000;
const WRAM_SIZE: usize = 0x2000;
const OAM_SIZE: usize = 0xA0;
const IO_SIZE: usize = 0x80;
const HRAM_SIZE: usize = 0x7F;
const OPEN_BUS: u8 = 0xFF;

const CYCLES_PER_LINE: u16 = 456;
const VBLANK_START: u8 = 144;
const TOTAL_LINES: u8 = 154;

const REG_JOYP: u16 = 0xFF00;
const REG_LCDC: u16 = 0xFF40;
const REG_DIV: u16 = 0xFF04;
const REG_TIMA: u16 = 0xFF05;
const REG_TMA: u16 = 0xFF06;
const REG_TAC: u16 = 0xFF07;
const REG_IF: u16 = 0xFF0F;
const REG_STAT: u16 = 0xFF41;
const REG_LYC: u16 = 0xFF45;
const REG_DMA: u16 = 0xFF46;
const REG_LY: u16 = 0xFF44;
const IF_VBLANK: u8 = 0x01;
const IF_STAT: u8 = 0x02;
const IF_TIMER: u8 = 0x04;

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
    div: u8,
    div_counter: u16,
    tima: u8,
    tma: u8,
    tac: u8,
    tima_counter: u32,
    ly: u8,
    lyc: u8,
    stat: u8,
    ppu_line_cycles: u16,
    ppu_mode: u8,
    joyp_select: u8,
    joyp_buttons: u8,
    joyp_dpad: u8,
    dma: u8,
    interrupt_flag: u8,
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
            div: 0,
            div_counter: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            tima_counter: 0,
            ly: 0,
            lyc: 0,
            stat: 0,
            ppu_line_cycles: 0,
            ppu_mode: 0,
            joyp_select: 0x30,
            joyp_buttons: 0x0F,
            joyp_dpad: 0x0F,
            dma: 0,
            interrupt_flag: 0,
            interrupt_enable: 0,
        })
    }

    pub fn cartridge(&self) -> &Cartridge {
        &self.cartridge
    }

    pub fn boot_rom_enabled(&self) -> bool {
        self.boot_rom_enabled
    }

    pub fn vram(&self) -> &[u8] {
        &self.vram
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
            0xFF00..=0xFF7F => self.read_io(addr),
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
            0xFF00..=0xFF7F => self.write_io(addr, value),
            0xFF80..=0xFFFE => self.hram[(addr as usize - 0xFF80) % HRAM_SIZE] = value,
            0xFFFF => self.interrupt_enable = value,
        }
    }

    pub fn step(&mut self, cycles: u32) {
        self.step_div(cycles);
        self.step_timer(cycles);
        self.step_ppu(cycles);
        self.mbc.tick(cycles);
    }

    pub fn set_rtc_mode(&mut self, mode: RtcMode) {
        self.mbc.set_rtc_mode(mode);
    }
}

impl Bus {
    fn read_io(&self, addr: u16) -> u8 {
        match addr {
            REG_JOYP => self.read_joyp(),
            REG_DIV => self.div,
            REG_TIMA => self.tima,
            REG_TMA => self.tma,
            REG_TAC => self.tac,
            REG_IF => self.interrupt_flag,
            REG_STAT => self.stat,
            REG_LY => self.ly,
            REG_LYC => self.lyc,
            REG_DMA => self.dma,
            _ => self.io[(addr as usize - 0xFF00) % IO_SIZE],
        }
    }

    fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            REG_JOYP => self.joyp_select = value & 0x30,
            REG_DIV => {
                self.div = 0;
                self.div_counter = 0;
            }
            REG_TIMA => self.tima = value,
            REG_TMA => self.tma = value,
            REG_TAC => self.tac = value,
            REG_IF => self.interrupt_flag = value,
            REG_STAT => self.stat = (self.stat & 0x07) | (value & 0xF8),
            REG_LY => {
                self.ly = 0;
                self.ppu_line_cycles = 0;
                self.update_stat();
            }
            REG_LYC => {
                self.lyc = value;
                self.update_stat();
            }
            REG_DMA => {
                self.dma = value;
                let base = (value as u16) << 8;
                for i in 0..OAM_SIZE {
                    let byte = self.read8(base.wrapping_add(i as u16));
                    self.oam[i] = byte;
                }
            }
            _ => self.io[(addr as usize - 0xFF00) % IO_SIZE] = value,
        }
    }

    fn step_div(&mut self, cycles: u32) {
        let new = self.div_counter.wrapping_add(cycles as u16);
        self.div_counter = new;
        self.div = (new >> 8) as u8;
    }

    fn step_timer(&mut self, cycles: u32) {
        if self.tac & 0x04 == 0 {
            return;
        }

        let period = match self.tac & 0x03 {
            0x00 => 1024,
            0x01 => 16,
            0x02 => 64,
            0x03 => 256,
            _ => 1024,
        };

        self.tima_counter += cycles;
        while self.tima_counter >= period {
            self.tima_counter -= period;
            let (next, overflow) = self.tima.overflowing_add(1);
            if overflow {
                self.tima = self.tma;
                self.interrupt_flag |= IF_TIMER;
            } else {
                self.tima = next;
            }
        }
    }

    fn step_ppu(&mut self, cycles: u32) {
        let lcdc = self.read_io(REG_LCDC);
        if lcdc & 0x80 == 0 {
            self.ly = 0;
            self.ppu_line_cycles = 0;
            self.ppu_mode = 0;
            self.update_stat();
            return;
        }

        let mut remaining = cycles;
        while remaining > 0 {
            let step = remaining.min(u32::from(u16::MAX));
            self.ppu_line_cycles = self.ppu_line_cycles.wrapping_add(step as u16);
            remaining -= step;
            while self.ppu_line_cycles >= CYCLES_PER_LINE {
                self.ppu_line_cycles -= CYCLES_PER_LINE;
                self.ly = self.ly.wrapping_add(1);
                if self.ly == VBLANK_START {
                    self.interrupt_flag |= IF_VBLANK;
                }
                if self.ly >= TOTAL_LINES {
                    self.ly = 0;
                }
                self.update_stat();
            }
        }

        self.update_stat();
    }

    fn update_stat(&mut self) {
        let mode = if self.ly >= VBLANK_START {
            1
        } else if self.ppu_line_cycles < 80 {
            2
        } else if self.ppu_line_cycles < 252 {
            3
        } else {
            0
        };

        let mut stat = self.stat & 0xF8;
        if self.ly == self.lyc {
            stat |= 0x04;
            if self.stat & 0x40 != 0 {
                self.interrupt_flag |= IF_STAT;
            }
        }
        if mode != self.ppu_mode {
            match mode {
                0 if self.stat & 0x08 != 0 => self.interrupt_flag |= IF_STAT,
                1 if self.stat & 0x10 != 0 => self.interrupt_flag |= IF_STAT,
                2 if self.stat & 0x20 != 0 => self.interrupt_flag |= IF_STAT,
                _ => {}
            }
            self.ppu_mode = mode;
        }
        stat |= mode;
        self.stat = stat;
    }

    fn read_joyp(&self) -> u8 {
        let mut value = 0x0F;
        if self.joyp_select & 0x10 == 0 {
            value &= self.joyp_dpad;
        }
        if self.joyp_select & 0x20 == 0 {
            value &= self.joyp_buttons;
        }
        0xC0 | self.joyp_select | value
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BOOT_ROM_SIZE, Bus, IF_TIMER, REG_DIV, REG_IF, REG_JOYP, REG_LY, REG_LYC, REG_STAT,
        REG_TAC, REG_TIMA, REG_TMA,
    };
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

    #[test]
    fn bus_joyp_selects_groups() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_JOYP, 0x30);
        assert_eq!(bus.read8(REG_JOYP), 0xFF);
        bus.write8(REG_JOYP, 0x20);
        assert_eq!(bus.read8(REG_JOYP), 0xEF);
        bus.write8(REG_JOYP, 0x10);
        assert_eq!(bus.read8(REG_JOYP), 0xDF);
    }

    #[test]
    fn bus_dma_copies_to_oam() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        for i in 0..0xA0u16 {
            bus.write8(0xC000 + i, (i as u8).wrapping_add(1));
        }
        bus.write8(0xFF46, 0xC0);

        assert_eq!(bus.read8(0xFE00), 0x01);
        assert_eq!(bus.read8(0xFE9F), 0xA0);
    }

    #[test]
    fn bus_updates_ly_and_stat_mode() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(0xFF40, 0x80);
        bus.step(1);
        assert_eq!(bus.read8(REG_STAT) & 0x03, 0x02);

        bus.step(80);
        assert_eq!(bus.read8(REG_STAT) & 0x03, 0x03);

        bus.step(172);
        assert_eq!(bus.read8(REG_STAT) & 0x03, 0x00);

        bus.step(456);
        assert_eq!(bus.read8(REG_LY), 1);

        bus.write8(REG_LYC, 1);
        bus.step(1);
        assert_eq!(bus.read8(REG_STAT) & 0x04, 0x04);
    }

    #[test]
    fn bus_mmio_register_semantics() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_TIMA, 0x12);
        bus.write8(REG_TMA, 0x34);
        bus.write8(REG_TAC, 0x56);
        bus.write8(REG_STAT, 0x78);
        bus.write8(REG_IF, 0x9A);
        bus.write8(0xFFFF, 0xBC);

        assert_eq!(bus.read8(REG_TIMA), 0x12);
        assert_eq!(bus.read8(REG_TMA), 0x34);
        assert_eq!(bus.read8(REG_TAC), 0x56);
        assert_eq!(bus.read8(REG_STAT), 0x78);
        assert_eq!(bus.read8(REG_IF), 0x9A);
        assert_eq!(bus.read8(0xFFFF), 0xBC);

        bus.write8(REG_DIV, 0xFF);
        bus.write8(REG_LY, 0x55);
        assert_eq!(bus.read8(REG_DIV), 0x00);
        assert_eq!(bus.read8(REG_LY), 0x00);
    }

    #[test]
    fn bus_timer_steps_and_sets_interrupt() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_TAC, 0x05);
        bus.step(16);
        assert_eq!(bus.read8(REG_TIMA), 1);

        bus.write8(REG_TIMA, 0xFF);
        bus.write8(REG_TMA, 0xAA);
        bus.step(16);
        assert_eq!(bus.read8(REG_TIMA), 0xAA);
        assert_eq!(bus.read8(REG_IF) & IF_TIMER, IF_TIMER);
    }
}
