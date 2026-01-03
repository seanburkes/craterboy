use super::{Apu, Cartridge, Mbc, MbcError, RtcMode};

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
const DMA_CYCLES: u32 = 160;

const REG_JOYP: u16 = 0xFF00;
const REG_LCDC: u16 = 0xFF40;
const REG_DIV: u16 = 0xFF04;
const REG_TIMA: u16 = 0xFF05;
const REG_TMA: u16 = 0xFF06;
const REG_TAC: u16 = 0xFF07;
const REG_IF: u16 = 0xFF0F;
const REG_NR10: u16 = 0xFF10;
const REG_NR11: u16 = 0xFF11;
const REG_NR12: u16 = 0xFF12;
const REG_NR14: u16 = 0xFF14;
const REG_NR21: u16 = 0xFF16;
const REG_NR22: u16 = 0xFF17;
const REG_NR24: u16 = 0xFF19;
const REG_NR30: u16 = 0xFF1A;
const REG_NR31: u16 = 0xFF1B;
const REG_NR32: u16 = 0xFF1C;
const REG_NR34: u16 = 0xFF1E;
const REG_NR41: u16 = 0xFF20;
const REG_NR42: u16 = 0xFF21;
const REG_NR43: u16 = 0xFF22;
const REG_NR44: u16 = 0xFF23;
const REG_NR50: u16 = 0xFF24;
const REG_NR51: u16 = 0xFF25;
const REG_NR52: u16 = 0xFF26;
const REG_STAT: u16 = 0xFF41;
const REG_SCY: u16 = 0xFF42;
const REG_SCX: u16 = 0xFF43;
const REG_LYC: u16 = 0xFF45;
const REG_DMA: u16 = 0xFF46;
const REG_LY: u16 = 0xFF44;
const REG_BGP: u16 = 0xFF47;
const REG_OBP0: u16 = 0xFF48;
const REG_OBP1: u16 = 0xFF49;
const REG_WY: u16 = 0xFF4A;
const REG_WX: u16 = 0xFF4B;
const REG_KEY1: u16 = 0xFF4D;
const IF_VBLANK: u8 = 0x01;
const IF_STAT: u8 = 0x02;
const IF_TIMER: u8 = 0x04;

#[derive(Debug)]
pub struct Bus {
    cartridge: Cartridge,
    mbc: Mbc,
    boot_rom: Option<Vec<u8>>,
    boot_rom_enabled: bool,
    boot_rom_just_disabled: bool,
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
    dma_active: bool,
    dma_cycles_remaining: u32,
    dma_base: u16,
    double_speed: bool,
    speed_switch_pending: bool,
    interrupt_flag: u8,
    interrupt_enable: u8,
    apu: Apu,
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

        let mut io = vec![0; IO_SIZE];
        let mut stat = 0;
        let mut dma = 0;
        let mut interrupt_flag = 0;

        if !boot_rom_enabled {
            // Initialize IO registers to post-boot DMG defaults when no boot ROM
            // See: https://gbdev.io/pandocs/#power-up-sequence
            io[0x40] = 0x91; // LCDC - LCD enabled, BG/OBJ enabled, correct tile/map areas
            io[0x41] = 0x85; // STAT - mode 1 (V-Blank), no interrupts
            io[0x42] = 0x00; // SCY
            io[0x43] = 0x00; // SCX
            io[0x45] = 0x00; // LYC
            dma = 0xFF; // DMA
            io[0x47] = 0xFC; // BGP - standard grayscale palette
            io[0x48] = 0xFF; // OBP0 - all white/transparent
            io[0x49] = 0xFF; // OBP1 - all white/transparent
            io[0x4A] = 0x00; // WY
            io[0x4B] = 0x00; // WX

            // Interrupt flags
            interrupt_flag = 0xE1; // IF - VBLANK, STAT, TIMER, SERIAL, JOYPAD

            // Sound registers (post-boot DMG defaults)
            io[0x10] = 0x80; // NR10
            io[0x11] = 0xBF; // NR11
            io[0x12] = 0xF3; // NR12
            io[0x14] = 0xBF; // NR14
            io[0x16] = 0x3F; // NR21
            io[0x17] = 0x00; // NR22
            io[0x19] = 0xBF; // NR24
            io[0x1A] = 0x7F; // NR30
            io[0x1B] = 0xFF; // NR31
            io[0x1C] = 0x9F; // NR32
            io[0x1E] = 0xBF; // NR34
            io[0x20] = 0xFF; // NR41
            io[0x21] = 0x00; // NR42
            io[0x22] = 0x00; // NR43
            io[0x23] = 0xBF; // NR44
            io[0x24] = 0x77; // NR50
            io[0x25] = 0xF3; // NR51
            io[0x26] = 0xF1; // NR52 - sound on

            stat = 0x85;
        }

        let mut apu = Apu::new();
        if !boot_rom_enabled {
            apu.apply_post_boot_state();
        }

        Ok(Self {
            cartridge,
            mbc,
            boot_rom,
            boot_rom_enabled,
            boot_rom_just_disabled: false,
            vram: vec![0; VRAM_SIZE],
            wram: vec![0; WRAM_SIZE],
            oam: vec![0; OAM_SIZE],
            io,
            hram: vec![0; HRAM_SIZE],
            div: 0,
            div_counter: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            tima_counter: 0,
            ly: 0,
            lyc: 0,
            stat,
            ppu_line_cycles: 0,
            ppu_mode: 0,
            joyp_select: 0x30,
            joyp_buttons: 0x0F,
            joyp_dpad: 0x0F,
            dma,
            dma_active: false,
            dma_cycles_remaining: 0,
            dma_base: 0,
            double_speed: false,
            speed_switch_pending: false,
            interrupt_flag,
            interrupt_enable: 0,
            apu,
        })
    }

    pub fn cartridge(&self) -> &Cartridge {
        &self.cartridge
    }

    pub fn boot_rom_enabled(&self) -> bool {
        self.boot_rom_enabled
    }

    /// Returns true if the boot ROM was disabled since the last call to this method.
    /// Clears the flag after reading.
    pub fn take_boot_rom_disabled(&mut self) -> bool {
        let was_disabled = self.boot_rom_just_disabled;
        self.boot_rom_just_disabled = false;
        was_disabled
    }

    pub fn vram(&self) -> &[u8] {
        &self.vram
    }

    pub fn set_joyp_buttons(&mut self, mask: u8) {
        self.joyp_buttons = mask & 0x0F;
    }

    pub fn set_joyp_dpad(&mut self, mask: u8) {
        self.joyp_dpad = mask & 0x0F;
    }

    pub fn speed_switch_pending(&self) -> bool {
        self.speed_switch_pending
    }

    pub fn perform_speed_switch(&mut self) {
        if self.speed_switch_pending {
            self.speed_switch_pending = false;
            self.double_speed = !self.double_speed;
        }
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
            self.boot_rom_just_disabled = true;
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
        let _ = self.apu.step(cycles);
        self.step_ppu(cycles);
        self.step_dma(cycles);
        self.mbc.tick(cycles);
    }

    pub fn set_rtc_mode(&mut self, mode: RtcMode) {
        self.mbc.set_rtc_mode(mode);
    }

    pub fn apu_step(&mut self, cycles: u32) {
        let _ = self.apu.step(cycles);
    }

    pub fn apu_sample_rate_hz(&self) -> f64 {
        self.apu.sample_rate_hz()
    }

    pub fn apu_has_sample(&self) -> bool {
        self.apu.has_sample()
    }

    pub fn apu_take_sample(&mut self) -> i32 {
        self.apu.take_sample()
    }

    pub fn apu_sample(&self) -> i32 {
        self.apu.sample()
    }

    pub fn apu_pulse_output(&self) -> i32 {
        self.apu.pulse_output()
    }

    pub fn apu_pulse2_output(&self) -> i32 {
        self.apu.pulse2_output()
    }

    pub fn apu_wave_output(&self) -> i32 {
        self.apu.wave_output()
    }

    pub fn apu_noise_output(&self) -> i32 {
        self.apu.noise_output()
    }

    pub fn apu_read_io(&self, addr: u16) -> u8 {
        self.apu.read_io(addr)
    }

    pub fn apu_write_io(&mut self, addr: u16, value: u8) {
        self.apu.write_io(addr, value);
    }

    pub fn apu_reset(&mut self) {
        self.apu.reset();
    }

    pub fn apply_post_boot_state(&mut self) {
        self.boot_rom_enabled = false;
        self.div = 0xAB;
        self.div_counter = (self.div as u16) << 8;
        self.tima = 0x00;
        self.tma = 0x00;
        self.tac = 0x00;
        self.tima_counter = 0;
        self.interrupt_flag = 0xE1;
        self.interrupt_enable = 0x00;
        self.ly = 0x00;
        self.lyc = 0x00;
        self.ppu_line_cycles = 0;
        self.ppu_mode = 0;
        self.stat = 0x80;

        self.set_io_reg(REG_NR10, 0x80);
        self.set_io_reg(REG_NR11, 0xBF);
        self.set_io_reg(REG_NR12, 0xF3);
        self.set_io_reg(REG_NR14, 0xBF);
        self.set_io_reg(REG_NR21, 0x3F);
        self.set_io_reg(REG_NR22, 0x00);
        self.set_io_reg(REG_NR24, 0xBF);
        self.set_io_reg(REG_NR30, 0x7F);
        self.set_io_reg(REG_NR31, 0xFF);
        self.set_io_reg(REG_NR32, 0x9F);
        self.set_io_reg(REG_NR34, 0xBF);
        self.set_io_reg(REG_NR41, 0xFF);
        self.set_io_reg(REG_NR42, 0x00);
        self.set_io_reg(REG_NR43, 0x00);
        self.set_io_reg(REG_NR44, 0xBF);
        self.set_io_reg(REG_NR50, 0x77);
        self.set_io_reg(REG_NR51, 0xF3);
        self.set_io_reg(REG_NR52, 0xF1);

        self.apu.apply_post_boot_state();

        self.set_io_reg(REG_LCDC, 0x91);
        self.set_io_reg(REG_SCY, 0x00);
        self.set_io_reg(REG_SCX, 0x00);
        self.set_io_reg(REG_BGP, 0xFC);
        self.set_io_reg(REG_OBP0, 0xFF);
        self.set_io_reg(REG_OBP1, 0xFF);
        self.set_io_reg(REG_WY, 0x00);
        self.set_io_reg(REG_WX, 0x00);

        self.update_stat();
    }
}

impl Bus {
    fn set_io_reg(&mut self, addr: u16, value: u8) {
        let idx = addr.wrapping_sub(0xFF00) as usize;
        if idx < IO_SIZE {
            self.io[idx] = value;
        }
    }

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
            REG_KEY1 => self.read_key1(),
            0xFF10..=0xFF14
            | 0xFF16..=0xFF19
            | 0xFF1A..=0xFF1E
            | 0xFF20..=0xFF23
            | 0xFF24..=0xFF26
            | 0xFF30..=0xFF3F => self.apu.read_io(addr),
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
                self.dma_active = true;
                self.dma_cycles_remaining = DMA_CYCLES;
                self.dma_base = (value as u16) << 8;
            }
            REG_KEY1 => {
                self.speed_switch_pending = value & 0x01 != 0;
            }
            0xFF10..=0xFF14
            | 0xFF16..=0xFF19
            | 0xFF1A..=0xFF1E
            | 0xFF20..=0xFF23
            | 0xFF24..=0xFF26
            | 0xFF30..=0xFF3F => {
                self.apu.write_io(addr, value);
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

    fn step_dma(&mut self, cycles: u32) {
        if !self.dma_active {
            return;
        }
        if cycles >= self.dma_cycles_remaining {
            self.dma_cycles_remaining = 0;
        } else {
            self.dma_cycles_remaining -= cycles;
        }
        if self.dma_cycles_remaining == 0 {
            let base = self.dma_base;
            for i in 0..OAM_SIZE {
                let byte = self.read8(base.wrapping_add(i as u16));
                self.oam[i] = byte;
            }
            self.dma_active = false;
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

    fn read_key1(&self) -> u8 {
        let mut value = 0x7E;
        if self.double_speed {
            value |= 0x80;
        }
        if self.speed_switch_pending {
            value |= 0x01;
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BOOT_ROM_SIZE, Bus, DMA_CYCLES, IF_TIMER, REG_BGP, REG_DIV, REG_DMA, REG_IF, REG_JOYP,
        REG_KEY1, REG_LCDC, REG_LY, REG_LYC, REG_OBP0, REG_OBP1, REG_SCX, REG_SCY, REG_STAT,
        REG_TAC, REG_TIMA, REG_TMA, REG_WX, REG_WY,
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
    fn take_boot_rom_disabled_signals_transition() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");

        let boot_rom = vec![0xAA; BOOT_ROM_SIZE];
        let mut bus = Bus::with_boot_rom(cartridge, Some(boot_rom)).expect("bus");

        // Initially not signaled
        assert!(!bus.take_boot_rom_disabled());

        // Disable boot ROM
        bus.write8(0xFF50, 0x01);

        // First call returns true
        assert!(bus.take_boot_rom_disabled());

        // Subsequent calls return false (flag cleared)
        assert!(!bus.take_boot_rom_disabled());
    }

    #[test]
    fn take_boot_rom_disabled_not_signaled_without_boot_rom() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");

        let mut bus = Bus::new(cartridge).expect("bus");

        // No boot ROM means no transition signal
        assert!(!bus.take_boot_rom_disabled());

        // Writing to 0xFF50 has no effect
        bus.write8(0xFF50, 0x01);
        assert!(!bus.take_boot_rom_disabled());
    }

    #[test]
    fn bus_initializes_post_boot_defaults_without_boot_rom() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");

        let bus = Bus::new(cartridge).expect("bus");

        // Verify key registers have post-boot DMG defaults
        assert_eq!(bus.read8(REG_LCDC), 0x91, "LCDC should be 0x91");
        assert_eq!(bus.read8(REG_STAT), 0x85, "STAT should be 0x85");
        assert_eq!(bus.read8(REG_DMA), 0xFF, "DMA should be 0xFF");
        assert_eq!(bus.read8(REG_BGP), 0xFC, "BGP should be 0xFC");
        assert_eq!(bus.read8(REG_OBP0), 0xFF, "OBP0 should be 0xFF");
        assert_eq!(bus.read8(REG_OBP1), 0xFF, "OBP1 should be 0xFF");
        assert_eq!(bus.read8(REG_SCY), 0x00, "SCY should be 0x00");
        assert_eq!(bus.read8(REG_SCX), 0x00, "SCX should be 0x00");
        assert_eq!(bus.read8(REG_WY), 0x00, "WY should be 0x00");
        assert_eq!(bus.read8(REG_WX), 0x00, "WX should be 0x00");
        assert_eq!(bus.read8(REG_IF), 0xE1, "IF should be 0xE1");
    }

    #[test]
    fn bus_zeroes_io_registers_with_boot_rom() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");

        let boot_rom = vec![0xAA; BOOT_ROM_SIZE];
        let bus = Bus::with_boot_rom(cartridge, Some(boot_rom)).expect("bus");

        // With boot ROM, registers are zeroed (boot ROM will initialize them)
        assert_eq!(
            bus.read8(REG_LCDC),
            0x00,
            "LCDC should be 0x00 with boot ROM"
        );
        assert_eq!(
            bus.read8(REG_STAT),
            0x00,
            "STAT should be 0x00 with boot ROM"
        );
        assert_eq!(bus.read8(REG_BGP), 0x00, "BGP should be 0x00 with boot ROM");
        assert_eq!(bus.read8(REG_DMA), 0x00, "DMA should be 0x00 with boot ROM");
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

        bus.set_joyp_buttons(0x0E);
        bus.set_joyp_dpad(0x0D);

        bus.write8(REG_JOYP, 0x30);
        assert_eq!(bus.read8(REG_JOYP), 0xFF);
        bus.write8(REG_JOYP, 0x20);
        assert_eq!(bus.read8(REG_JOYP), 0xED);
        bus.write8(REG_JOYP, 0x10);
        assert_eq!(bus.read8(REG_JOYP), 0xDE);
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
        assert_eq!(bus.read8(0xFE00), 0x00);
        bus.step(DMA_CYCLES);

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
    fn bus_key1_speed_switch() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_KEY1, 0x01);
        assert_eq!(bus.read8(REG_KEY1) & 0x01, 0x01);
        assert_eq!(bus.read8(REG_KEY1) & 0x80, 0x00);

        bus.perform_speed_switch();
        assert_eq!(bus.read8(REG_KEY1) & 0x01, 0x00);
        assert_eq!(bus.read8(REG_KEY1) & 0x80, 0x80);
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
        // STAT preserves lower 3 bits (mode and LYC flag) on write
        assert_eq!(bus.read8(REG_STAT) & 0xF8, 0x78);
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
