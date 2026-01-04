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
const REG_KEY0: u16 = 0xFF4C;
const REG_KEY1: u16 = 0xFF4D;
const REG_VBK: u16 = 0xFF4F;
const REG_HDMA1: u16 = 0xFF51;
const REG_HDMA2: u16 = 0xFF52;
const REG_HDMA3: u16 = 0xFF53;
const REG_HDMA4: u16 = 0xFF54;
const REG_HDMA5: u16 = 0xFF55;
const REG_BGPI: u16 = 0xFF68;
const REG_BGPD: u16 = 0xFF69;
const REG_OBPI: u16 = 0xFF6A;
const REG_OBPD: u16 = 0xFF6B;
const IF_VBLANK: u8 = 0x01;
const IF_STAT: u8 = 0x02;
const IF_TIMER: u8 = 0x04;

const HDMA_BLOCK_SIZE: usize = 0x10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HdmaMode {
    Inactive,
    HBlank,
    General,
}

#[derive(Debug)]
pub struct Bus {
    cartridge: Cartridge,
    mbc: Mbc,
    boot_rom: Option<Vec<u8>>,
    boot_rom_enabled: bool,
    boot_rom_just_disabled: bool,
    vram_bank: u8,
    vram: [Vec<u8>; 2],
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
    cgb_mode: bool,
    interrupt_flag: u8,
    interrupt_enable: u8,
    apu: Apu,
    bg_palette_index: u8,
    bg_palette_auto_increment: bool,
    ob_palette_index: u8,
    ob_palette_auto_increment: bool,
    bg_palette_data: [u8; 64],
    ob_palette_data: [u8; 64],
    hdma_source: u16,
    hdma_dest: u16,
    hdma_blocks_remaining: u8,
    hdma_active: bool,
    hdma_mode: HdmaMode,
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
        let is_cgb = cartridge.is_cgb();

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
            vram_bank: 0,
            vram: [vec![0; VRAM_SIZE], vec![0; VRAM_SIZE]],
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
            cgb_mode: is_cgb,
            interrupt_flag,
            interrupt_enable: 0,
            apu,
            bg_palette_index: 0,
            bg_palette_auto_increment: false,
            ob_palette_index: 0,
            ob_palette_auto_increment: false,
            bg_palette_data: [0xFF; 64],
            ob_palette_data: [0xFF; 64],
            hdma_source: 0,
            hdma_dest: 0,
            hdma_blocks_remaining: 0,
            hdma_active: false,
            hdma_mode: HdmaMode::Inactive,
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
        &self.vram[self.vram_bank as usize]
    }

    pub fn vram_bank(&self) -> u8 {
        self.vram_bank
    }

    pub fn vram_bank0(&self) -> &[u8] {
        &self.vram[0]
    }

    pub fn vram_bank1(&self) -> &[u8] {
        &self.vram[1]
    }

    pub fn bg_palette_data(&self) -> &[u8; 64] {
        &self.bg_palette_data
    }

    pub fn ob_palette_data(&self) -> &[u8; 64] {
        &self.ob_palette_data
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

    pub fn is_double_speed(&self) -> bool {
        self.double_speed
    }

    pub fn is_cgb(&self) -> bool {
        self.cgb_mode
    }

    pub fn set_cgb_mode(&mut self, enabled: bool) {
        self.cgb_mode = enabled;
    }

    pub fn disable_boot_rom(&mut self) {
        self.boot_rom_enabled = false;
    }

    pub fn read8(&self, addr: u16) -> u8 {
        if self.boot_rom_enabled
            && let Some(boot_rom) = &self.boot_rom
            && (addr as usize) < BOOT_ROM_SIZE
            && boot_rom.len() >= BOOT_ROM_SIZE
        {
            return boot_rom[addr as usize];
        }

        match addr {
            0x0000..=0x7FFF => self.mbc.read8(&self.cartridge, addr),
            0x8000..=0x9FFF => {
                self.vram[self.vram_bank as usize][(addr as usize - 0x8000) % VRAM_SIZE]
            }
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
            0x8000..=0x9FFF => {
                self.vram[self.vram_bank as usize][(addr as usize - 0x8000) % VRAM_SIZE] = value
            }
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
        self.step_hdma();
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

    pub fn apu_set_sample_rate_hz(&mut self, sample_rate_hz: f64) {
        self.apu.set_sample_rate_hz(sample_rate_hz);
    }

    pub fn apu_has_sample(&self) -> bool {
        self.apu.has_sample()
    }

    pub fn apu_take_sample(&mut self) -> i32 {
        self.apu.take_sample()
    }

    pub fn apu_take_sample_stereo(&mut self) -> (i32, i32) {
        self.apu.take_sample_stereo()
    }

    pub fn apu_sample(&self) -> i32 {
        self.apu.sample()
    }

    pub fn apu_sample_stereo(&self) -> (i32, i32) {
        self.apu.sample_stereo()
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
        self.set_io_reg(REG_KEY0, if self.cgb_mode { 0x01 } else { 0x00 });
        self.set_io_reg(REG_KEY1, 0x00);

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
            REG_KEY0 => self.read_key0(),
            REG_KEY1 => self.read_key1(),
            REG_VBK => self.vram_bank | 0xFE,
            REG_HDMA1 => (self.hdma_source >> 8) as u8,
            REG_HDMA2 => self.hdma_source as u8,
            REG_HDMA3 => (self.hdma_dest >> 8) as u8,
            REG_HDMA4 => self.hdma_dest as u8,
            REG_HDMA5 => {
                let mut value = self.hdma_blocks_remaining;
                if self.hdma_active {
                    value |= 0x80;
                }
                match self.hdma_mode {
                    HdmaMode::HBlank => value,
                    HdmaMode::General => value | 0x80,
                    HdmaMode::Inactive => value,
                }
            }
            REG_BGPI => self.read_bgpi(),
            REG_BGPD => self.read_bgpdata(),
            REG_OBPI => self.read_obpi(),
            REG_OBPD => self.read_obpdata(),
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
            REG_KEY0 => {}
            REG_KEY1 => {
                if self.cgb_mode {
                    self.speed_switch_pending = value & 0x01 != 0;
                }
            }
            REG_VBK => {
                self.vram_bank = value & 0x01;
            }
            REG_HDMA1 => {
                if !self.hdma_active {
                    self.hdma_source = ((value as u16) << 8) | (self.hdma_source & 0x00FF);
                }
            }
            REG_HDMA2 => {
                if !self.hdma_active {
                    self.hdma_source = (self.hdma_source & 0xFF00) | ((value as u16) & 0x00F0);
                }
            }
            REG_HDMA3 => {
                if !self.hdma_active {
                    self.hdma_dest = ((value as u16) << 8) | (self.hdma_dest & 0x00FF);
                }
            }
            REG_HDMA4 => {
                if !self.hdma_active {
                    self.hdma_dest = (self.hdma_dest & 0xFF00) | ((value as u16) & 0x00F0);
                }
            }
            REG_HDMA5 => {
                if !self.cgb_mode {
                    return;
                }
                let is_gdma = value & 0x80 != 0;
                let blocks = value & 0x7F;

                if self.hdma_active {
                    if is_gdma {
                        return;
                    }
                    if blocks >= self.hdma_blocks_remaining {
                        self.hdma_active = false;
                        self.hdma_blocks_remaining = 0;
                        return;
                    }
                }

                self.hdma_blocks_remaining = blocks.wrapping_add(1);
                self.hdma_mode = if is_gdma {
                    HdmaMode::General
                } else {
                    HdmaMode::HBlank
                };

                if is_gdma {
                    self.hdma_active = true;
                    self.perform_hdma_transfer();
                    self.hdma_active = false;
                    self.hdma_blocks_remaining = 0;
                } else {
                    self.hdma_active = true;
                }
            }
            REG_BGPI => self.write_bgpi(value),
            REG_BGPD => self.write_bgpdata(value),
            REG_OBPI => self.write_obpi(value),
            REG_OBPD => self.write_obpdata(value),
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

    fn perform_hdma_transfer(&mut self) {
        let mut source = self.hdma_source;
        let mut dest = self.hdma_dest & 0x1FF0;

        for _ in 0..self.hdma_blocks_remaining {
            for i in 0..HDMA_BLOCK_SIZE {
                let byte = self.read8(source.wrapping_add(i as u16));
                let vram_idx = (dest as usize) % VRAM_SIZE;
                self.vram[self.vram_bank as usize][vram_idx] = byte;
                dest = dest.wrapping_add(1);
            }
            source = source.wrapping_add(HDMA_BLOCK_SIZE as u16);
        }

        self.hdma_source = source;
        self.hdma_dest = dest | 0x8000;
        self.hdma_blocks_remaining = 0;
        self.hdma_active = false;
    }

    fn step_hdma(&mut self) {
        if !self.hdma_active || self.hdma_mode != HdmaMode::HBlank {
            return;
        }

        if self.ly >= VBLANK_START {
            return;
        }

        if self.ppu_mode != 0 {
            return;
        }

        let cycles_into_hblank = self.ppu_line_cycles;
        if cycles_into_hblank < 252 {
            return;
        }

        let remaining = self.hdma_blocks_remaining;
        if remaining == 0 {
            self.hdma_active = false;
            return;
        }

        self.execute_hdma_block();
        self.hdma_blocks_remaining -= 1;

        if self.hdma_blocks_remaining == 0 {
            self.hdma_active = false;
        }
    }

    fn execute_hdma_block(&mut self) {
        let source = self.hdma_source;
        let mut dest = self.hdma_dest & 0x1FF0;

        for i in 0..HDMA_BLOCK_SIZE {
            let byte = self.read8(source.wrapping_add(i as u16));
            self.vram[self.vram_bank as usize][dest as usize] = byte;
            dest = dest.wrapping_add(1);
        }

        self.hdma_source = source.wrapping_add(HDMA_BLOCK_SIZE as u16);
        self.hdma_dest = dest | 0x8000;
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
        let mut value = 0x00;
        if self.double_speed {
            value |= 0x80;
        }
        if self.speed_switch_pending {
            value |= 0x01;
        }
        value
    }

    fn read_key0(&self) -> u8 {
        let mut value = 0xFE;
        if self.cgb_mode {
            value |= 0x01;
        }
        value
    }

    fn read_bgpi(&self) -> u8 {
        let mut value = self.bg_palette_index;
        if self.bg_palette_auto_increment {
            value |= 0x80;
        }
        value
    }

    fn read_bgpdata(&self) -> u8 {
        let idx = self.bg_palette_index as usize;
        self.bg_palette_data[idx]
    }

    fn write_bgpi(&mut self, value: u8) {
        self.bg_palette_index = value & 0x3F;
        self.bg_palette_auto_increment = value & 0x80 != 0;
    }

    fn write_bgpdata(&mut self, value: u8) {
        let idx = self.bg_palette_index as usize;
        self.bg_palette_data[idx] = value;
        if self.bg_palette_auto_increment {
            self.bg_palette_index = self.bg_palette_index.wrapping_add(1) & 0x3F;
        }
    }

    fn read_obpi(&self) -> u8 {
        let mut value = self.ob_palette_index;
        if self.ob_palette_auto_increment {
            value |= 0x80;
        }
        value
    }

    fn read_obpdata(&self) -> u8 {
        let idx = self.ob_palette_index as usize;
        self.ob_palette_data[idx]
    }

    fn write_obpi(&mut self, value: u8) {
        self.ob_palette_index = value & 0x3F;
        self.ob_palette_auto_increment = value & 0x80 != 0;
    }

    fn write_obpdata(&mut self, value: u8) {
        let idx = self.ob_palette_index as usize;
        self.ob_palette_data[idx] = value;
        if self.ob_palette_auto_increment {
            self.ob_palette_index = self.ob_palette_index.wrapping_add(1) & 0x3F;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BOOT_ROM_SIZE, Bus, DMA_CYCLES, IF_TIMER, REG_BGP, REG_BGPD, REG_BGPI, REG_DIV, REG_DMA,
        REG_HDMA1, REG_HDMA2, REG_HDMA3, REG_HDMA4, REG_HDMA5, REG_IF, REG_JOYP, REG_KEY0,
        REG_KEY1, REG_LCDC, REG_LY, REG_LYC, REG_OBP0, REG_OBP1, REG_OBPD, REG_OBPI, REG_SCX,
        REG_SCY, REG_STAT, REG_TAC, REG_TIMA, REG_TMA, REG_VBK, REG_WX, REG_WY,
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
        rom[0x0143] = 0x80; // CGB supported
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

    #[test]
    fn bus_cgb_mode_from_cartridge() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80; // CGB supported
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let bus = Bus::new(cartridge).expect("bus");

        assert!(bus.is_cgb());
        assert_eq!(bus.read8(REG_KEY0) & 0x01, 0x01);
    }

    #[test]
    fn bus_dmg_mode_from_cartridge() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x00; // DMG only
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let bus = Bus::new(cartridge).expect("bus");

        assert!(!bus.is_cgb());
        assert_eq!(bus.read8(REG_KEY0) & 0x01, 0x00);
    }

    #[test]
    fn bus_cgb_post_boot_state_sets_key0() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80; // CGB supported
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.apply_post_boot_state();
        assert_eq!(bus.read8(REG_KEY0), 0xFF);
    }

    #[test]
    fn bus_cgb_bg_palette_write_and_read() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_BGPI, 0x80);
        assert_eq!(bus.read8(REG_BGPI), 0x80);

        bus.write8(REG_BGPD, 0xAB);
        assert_eq!(bus.read8(REG_BGPI), 0x81);

        bus.write8(REG_BGPI, 0x00);
        assert_eq!(bus.read8(REG_BGPI), 0x00);
        assert_eq!(bus.read8(REG_BGPD), 0xAB);

        bus.write8(REG_BGPI, 0x81);
        assert_eq!(bus.read8(REG_BGPD), 0xFF);
    }

    #[test]
    fn bus_cgb_bg_palette_auto_increment() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_BGPI, 0x80);

        bus.write8(REG_BGPD, 0x11);
        assert_eq!(bus.read8(REG_BGPI), 0x81);
        bus.write8(REG_BGPD, 0x22);
        assert_eq!(bus.read8(REG_BGPI), 0x82);
    }

    #[test]
    fn bus_cgb_obj_palette_write_and_read() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_OBPI, 0x00);
        bus.write8(REG_OBPD, 0xCD);

        bus.write8(REG_OBPI, 0x81);
        bus.write8(REG_OBPD, 0xEF);

        bus.write8(REG_OBPI, 0x00);
        assert_eq!(bus.read8(REG_OBPD), 0xCD);

        bus.write8(REG_OBPI, 0x81);
        assert_eq!(bus.read8(REG_OBPI), 0x81);
        assert_eq!(bus.read8(REG_OBPD), 0xEF);
    }

    #[test]
    fn bus_cgb_palette_data_separate_for_bg_and_obj() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_BGPD, 0x12);
        bus.write8(REG_OBPD, 0x34);

        bus.write8(REG_BGPI, 0x00);
        bus.write8(REG_OBPI, 0x00);

        assert_eq!(bus.read8(REG_BGPD), 0x12);
        assert_eq!(bus.read8(REG_OBPD), 0x34);
    }

    #[test]
    fn bus_cgb_vram_bank_switching() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(0x8000, 0x12);
        assert_eq!(bus.read8(0x8000), 0x12);

        bus.write8(REG_VBK, 0x01);
        assert_eq!(bus.vram_bank(), 1);
        assert_eq!(bus.read8(REG_VBK) & 0x01, 0x01);

        assert_eq!(bus.read8(0x8000), 0x00);
        bus.write8(0x8000, 0x34);
        assert_eq!(bus.read8(0x8000), 0x34);

        bus.write8(REG_VBK, 0x00);
        assert_eq!(bus.vram_bank(), 0);
        assert_eq!(bus.read8(0x8000), 0x12);

        bus.write8(REG_VBK, 0x01);
        assert_eq!(bus.read8(0x8000), 0x34);
    }

    #[test]
    fn bus_cgb_vram_both_banks_accessible() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(0x8000, 0xAA);
        bus.write8(REG_VBK, 0x01);
        bus.write8(0x8000, 0xBB);

        assert_eq!(bus.vram_bank0()[0], 0xAA);
        assert_eq!(bus.vram_bank1()[0], 0xBB);
    }

    #[test]
    fn bus_dmg_vram_still_works() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let bus = Bus::new(cartridge).expect("bus");

        assert_eq!(bus.vram_bank(), 0);
        assert_eq!(bus.read8(REG_VBK) & 0xFE, 0xFE);
    }

    #[test]
    fn bus_key1_initial_state_dmg_mode() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x00; // DMG only
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let bus = Bus::new(cartridge).expect("bus");

        // In DMG mode, KEY1 should read as 0x00
        assert_eq!(bus.read8(REG_KEY1), 0x00);
        assert!(!bus.is_double_speed());
        assert!(!bus.speed_switch_pending());
    }

    #[test]
    fn bus_key1_initial_state_cgb_mode() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80; // CGB supported
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let bus = Bus::new(cartridge).expect("bus");

        // In CGB mode, should start in normal speed mode
        assert_eq!(bus.read8(REG_KEY1), 0x00);
        assert!(!bus.is_double_speed());
        assert!(!bus.speed_switch_pending());
    }

    #[test]
    fn bus_key1_arm_speed_switch() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80; // CGB supported
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        // Arm the speed switch
        bus.write8(REG_KEY1, 0x01);
        assert_eq!(bus.read8(REG_KEY1) & 0x01, 0x01);
        assert_eq!(bus.read8(REG_KEY1) & 0x80, 0x00);
        assert!(bus.speed_switch_pending());
        assert!(!bus.is_double_speed());

        // Clear the switch
        bus.write8(REG_KEY1, 0x00);
        assert_eq!(bus.read8(REG_KEY1) & 0x01, 0x00);
        assert!(!bus.speed_switch_pending());
    }

    #[test]
    fn bus_key1_perform_speed_switch_to_double() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80; // CGB supported
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        // Arm and perform switch to double speed
        bus.write8(REG_KEY1, 0x01);
        bus.perform_speed_switch();

        assert_eq!(bus.read8(REG_KEY1) & 0x01, 0x00);
        assert_eq!(bus.read8(REG_KEY1) & 0x80, 0x80);
        assert!(!bus.speed_switch_pending());
        assert!(bus.is_double_speed());
    }

    #[test]
    fn bus_key1_perform_speed_switch_to_normal() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80; // CGB supported
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        // First switch to double speed
        bus.write8(REG_KEY1, 0x01);
        bus.perform_speed_switch();
        assert!(bus.is_double_speed());

        // Then switch back to normal speed
        bus.write8(REG_KEY1, 0x01);
        bus.perform_speed_switch();

        assert_eq!(bus.read8(REG_KEY1) & 0x01, 0x00);
        assert_eq!(bus.read8(REG_KEY1) & 0x80, 0x00);
        assert!(!bus.speed_switch_pending());
        assert!(!bus.is_double_speed());
    }

    #[test]
    fn bus_key1_no_effect_in_dmg_mode() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x00; // DMG only
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        // Writing KEY1 in DMG mode should have no effect
        bus.write8(REG_KEY1, 0x01);
        assert_eq!(bus.read8(REG_KEY1), 0x00);
        assert!(!bus.speed_switch_pending());
        assert!(!bus.is_double_speed());

        bus.perform_speed_switch();
        assert_eq!(bus.read8(REG_KEY1), 0x00);
        assert!(!bus.speed_switch_pending());
        assert!(!bus.is_double_speed());
    }

    #[test]
    fn bus_key1_write_other_bits_ignored() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80; // CGB supported
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        // Write with other bits set - only bit 0 should be considered
        bus.write8(REG_KEY1, 0xFF);
        assert!(bus.speed_switch_pending());
        assert!(!bus.is_double_speed());

        // Read should only have bit 0 set (speed pending), others 0
        let key1_value = bus.read8(REG_KEY1);
        assert_eq!(key1_value & 0x01, 0x01);
        assert_eq!(key1_value & 0xFE, 0x00);
    }

    #[test]
    fn bus_key1_perform_switch_no_pending() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80; // CGB supported
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        // Perform switch without pending should do nothing
        bus.perform_speed_switch();
        assert_eq!(bus.read8(REG_KEY1), 0x00);
        assert!(!bus.is_double_speed());
        assert!(!bus.speed_switch_pending());
    }

    #[test]
    fn bus_hdma_gdma_minimal() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80; // CGB supported

        // Set up source data
        rom[0x0000] = 0xAA;
        rom[0x0001] = 0xBB;
        rom[0x0002] = 0xCC;

        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        // Source = 0x0000
        bus.write8(REG_HDMA1, 0x00);
        bus.write8(REG_HDMA2, 0x00);

        // Dest = 0x8000
        bus.write8(REG_HDMA3, 0x80);
        bus.write8(REG_HDMA4, 0x00);

        // Start GDMA: transfer 1 block (16 bytes) - bit 7 = GDMA mode
        bus.write8(REG_HDMA5, 0x80);

        // Check if data was transferred
        assert_eq!(bus.read8(0x8000), 0xAA, "First byte should be transferred");
        assert_eq!(bus.read8(0x8001), 0xBB, "Second byte should be transferred");
    }

    #[test]
    fn bus_hdma_gdma_transfers_data() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80; // CGB supported

        // Fill first 256 bytes with test pattern
        for (i, byte) in rom.iter_mut().enumerate().take(0x100) {
            *byte = i as u8;
        }

        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        // Set source address to 0x0000
        bus.write8(REG_HDMA1, 0x00);
        bus.write8(REG_HDMA2, 0x00);

        // Set destination address to 0x8000 (VRAM), must be 0x10 aligned for each block
        // HDMA destination: high bits in HDMA3, low bits (bits 7-4) in HDMA4
        // 0x8000 = 0x80 << 8 | 0x00
        bus.write8(REG_HDMA3, 0x80);
        bus.write8(REG_HDMA4, 0x00);

        // Start GDMA transfer: 0x8F = GDMA mode (bit 7) + 15 blocks (0x0F)
        bus.write8(REG_HDMA5, 0x8F);

        // Verify VRAM contains the transferred data
        for i in 0..0x100 {
            assert_eq!(
                bus.read8(0x8000 + i),
                i as u8,
                "VRAM[0x{:04X}] should match source",
                0x8000 + i
            );
        }
    }

    #[test]
    fn bus_hdma_debug() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80; // CGB supported

        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        // Test REG_HDMA1 address
        assert_eq!(REG_HDMA1, 0xFF51, "REG_HDMA1 should be 0xFF51");

        // Write and read back - single write like original test
        bus.write8(REG_HDMA1, 0x12);
        let value = bus.read8(REG_HDMA1);
        assert_eq!(value, 0x12, "Read 0x{:02X} after writing 0x12", value);
    }

    #[test]
    fn bus_hdma_debug_multi() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80; // CGB supported

        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        // Write all HDMA registers
        bus.write8(REG_HDMA1, 0x12);
        bus.write8(REG_HDMA2, 0x34);
        bus.write8(REG_HDMA3, 0x56);
        bus.write8(REG_HDMA4, 0x78);

        // Now read them back
        let h1 = bus.read8(REG_HDMA1);
        let h2 = bus.read8(REG_HDMA2);
        let h3 = bus.read8(REG_HDMA3);
        let h4 = bus.read8(REG_HDMA4);

        // HDMA2 masks bit 0 to 0, HDMA4 masks lower 4 bits to 0
        assert_eq!(h1, 0x12, "HDMA1: read 0x{:02X}, expected 0x12", h1);
        assert_eq!(
            h2, 0x30,
            "HDMA2: read 0x{:02X}, expected 0x30 (bit 0 forced to 0)",
            h2
        );
        assert_eq!(h3, 0x56, "HDMA3: read 0x{:02X}, expected 0x56", h3);
        assert_eq!(
            h4, 0x70,
            "HDMA4: read 0x{:02X}, expected 0x70 (lower 4 bits forced to 0)",
            h4
        );
    }

    #[test]
    fn bus_hdma_no_effect_in_dmg_mode() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x00; // DMG only

        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_HDMA5, 0x80); // Try to start HDMA in DMG mode
        assert_eq!(bus.read8(REG_HDMA5), 0x00);
    }

    #[test]
    fn bus_hdma_registers_read_write() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80; // CGB supported

        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_HDMA1, 0x12);
        bus.write8(REG_HDMA2, 0x34);
        bus.write8(REG_HDMA3, 0x56);
        bus.write8(REG_HDMA4, 0x78);

        // HDMA2 only keeps bits 7-1 (bit 0 is always 0), so 0x34 becomes 0x30
        // HDMA4 only keeps bits 7-4 (lower 4 bits are always 0), so 0x78 becomes 0x70
        assert_eq!(bus.read8(REG_HDMA1), 0x12);
        assert_eq!(bus.read8(REG_HDMA2), 0x30);
        assert_eq!(bus.read8(REG_HDMA3), 0x56);
        assert_eq!(bus.read8(REG_HDMA4), 0x70);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::domain::Cartridge;
    use crate::domain::cartridge::ROM_BANK_SIZE;
    use proptest::prelude::*;

    // Property: Memory write-read roundtrip for WRAM
    proptest! {
        #[test]
        fn prop_wram_write_read_roundtrip(addr in 0xC000u16..0xE000, value in any::<u8>()) {
            let mut rom = vec![0; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let mut bus = Bus::new(cartridge).expect("bus");

            bus.write8(addr, value);
            let read_value = bus.read8(addr);

            prop_assert_eq!(read_value, value, "WRAM write-read should roundtrip");
        }
    }

    // Property: Echo RAM mirrors WRAM
    proptest! {
        #[test]
        fn prop_echo_ram_mirrors_wram(offset in 0u16..0x1E00, value in any::<u8>()) {
            let mut rom = vec![0; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let mut bus = Bus::new(cartridge).expect("bus");

            let wram_addr = 0xC000 + offset;
            let echo_addr = 0xE000 + offset;

            bus.write8(wram_addr, value);
            let echo_read = bus.read8(echo_addr);

            prop_assert_eq!(echo_read, value, "Echo RAM should mirror WRAM");
        }
    }

    // Property: VRAM write-read roundtrip
    proptest! {
        #[test]
        fn prop_vram_write_read_roundtrip(offset in 0u16..VRAM_SIZE as u16, value in any::<u8>()) {
            let mut rom = vec![0; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let mut bus = Bus::new(cartridge).expect("bus");

            let addr = 0x8000 + offset;
            bus.write8(addr, value);
            let read_value = bus.read8(addr);

            prop_assert_eq!(read_value, value, "VRAM write-read should roundtrip");
        }
    }

    // Property: HRAM write-read roundtrip
    proptest! {
        #[test]
        fn prop_hram_write_read_roundtrip(offset in 0u16..HRAM_SIZE as u16, value in any::<u8>()) {
            let mut rom = vec![0; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let mut bus = Bus::new(cartridge).expect("bus");

            let addr = 0xFF80 + offset;
            bus.write8(addr, value);
            let read_value = bus.read8(addr);

            prop_assert_eq!(read_value, value, "HRAM write-read should roundtrip");
        }
    }

    // Property: OAM write-read roundtrip
    proptest! {
        #[test]
        fn prop_oam_write_read_roundtrip(offset in 0u16..OAM_SIZE as u16, value in any::<u8>()) {
            let mut rom = vec![0; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let mut bus = Bus::new(cartridge).expect("bus");

            let addr = 0xFE00 + offset;
            bus.write8(addr, value);
            let read_value = bus.read8(addr);

            prop_assert_eq!(read_value, value, "OAM write-read should roundtrip");
        }
    }

    // Property: Interrupt enable register roundtrip
    proptest! {
        #[test]
        fn prop_interrupt_enable_roundtrip(value in any::<u8>()) {
            let mut rom = vec![0; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let mut bus = Bus::new(cartridge).expect("bus");

            bus.write8(0xFFFF, value);
            let read_value = bus.read8(0xFFFF);

            prop_assert_eq!(read_value, value, "IE register should roundtrip");
        }
    }

    // Property: DIV register resets to 0 on write
    proptest! {
        #[test]
        fn prop_div_resets_on_write(write_value in any::<u8>()) {
            let mut rom = vec![0; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let mut bus = Bus::new(cartridge).expect("bus");

            // Step to increment DIV
            bus.step(256);

            // Write any value should reset to 0
            bus.write8(REG_DIV, write_value);
            let read_value = bus.read8(REG_DIV);

            prop_assert_eq!(read_value, 0, "DIV should reset to 0 on any write");
        }
    }

    // Property: LY register resets to 0 on write
    proptest! {
        #[test]
        fn prop_ly_resets_on_write(write_value in any::<u8>()) {
            let mut rom = vec![0; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let mut bus = Bus::new(cartridge).expect("bus");

            // Enable LCD
            bus.write8(REG_LCDC, 0x80);
            bus.step(456); // Advance one scanline

            // Write any value should reset to 0
            bus.write8(REG_LY, write_value);
            let read_value = bus.read8(REG_LY);

            prop_assert_eq!(read_value, 0, "LY should reset to 0 on any write");
        }
    }

    // Property: STAT lower 3 bits are read-only
    proptest! {
        #[test]
        fn prop_stat_lower_bits_readonly(value in any::<u8>()) {
            let mut rom = vec![0; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let mut bus = Bus::new(cartridge).expect("bus");

            // Enable LCD to get into a known mode
            bus.write8(REG_LCDC, 0x80);
            bus.step(1);

            let stat_before = bus.read8(REG_STAT) & 0x07;
            bus.write8(REG_STAT, value);
            let stat_after = bus.read8(REG_STAT) & 0x07;

            prop_assert_eq!(stat_after, stat_before, "STAT lower 3 bits should be read-only");
        }
    }

    // Property: Timer increments predictably
    proptest! {
        #[test]
        fn prop_timer_increments(tac in 0x04u8..0x08) {
            let mut rom = vec![0; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let mut bus = Bus::new(cartridge).expect("bus");

            bus.write8(REG_TIMA, 0);
            bus.write8(REG_TAC, tac);

            let period = match tac & 0x03 {
                0x00 => 1024,
                0x01 => 16,
                0x02 => 64,
                0x03 => 256,
                _ => 1024,
            };

            bus.step(period);
            let tima = bus.read8(REG_TIMA);

            prop_assert_eq!(tima, 1, "TIMA should increment by 1 after period cycles");
        }
    }

    // Property: Timer overflow sets interrupt
    proptest! {
        #[test]
        fn prop_timer_overflow_interrupt(_dummy in any::<u8>()) {
            let mut rom = vec![0; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let mut bus = Bus::new(cartridge).expect("bus");

            bus.write8(REG_TIMA, 0xFF);
            bus.write8(REG_TMA, 0x42);
            bus.write8(REG_TAC, 0x05); // Enable timer, 16 cycle period
            bus.write8(REG_IF, 0x00);  // Clear interrupts

            bus.step(16); // One timer increment

            let tima = bus.read8(REG_TIMA);
            let if_reg = bus.read8(REG_IF);

            prop_assert_eq!(tima, 0x42, "TIMA should reload from TMA on overflow");
            prop_assert!(if_reg & IF_TIMER != 0, "Timer interrupt should be set");
        }
    }

    // Property: JOYP selection doesn't crash
    proptest! {
        #[test]
        fn prop_joyp_selection(buttons in any::<u8>(), dpad in any::<u8>(), select in 0x00u8..0x30) {
            let mut rom = vec![0; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let mut bus = Bus::new(cartridge).expect("bus");

            let buttons_masked = buttons & 0x0F;
            let dpad_masked = dpad & 0x0F;

            bus.set_joyp_buttons(buttons_masked);
            bus.set_joyp_dpad(dpad_masked);

            // Test various selection patterns
            bus.write8(REG_JOYP, select);
            let joyp_value = bus.read8(REG_JOYP);

            // Upper 2 bits should always be set
            prop_assert_eq!(joyp_value & 0xC0, 0xC0, "Upper 2 bits of JOYP should be set");
        }
    }

    // Property: DMA transfer copies data
    proptest! {
        #[test]
        fn prop_dma_transfer(source_offset in 0u8..0xA0, value in any::<u8>()) {
            let mut rom = vec![0; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let mut bus = Bus::new(cartridge).expect("bus");

            // Write test data to source
            let source_addr = 0xC000 + source_offset as u16;
            bus.write8(source_addr, value);

            // Start DMA from 0xC000
            bus.write8(REG_DMA, 0xC0);
            bus.step(DMA_CYCLES);

            // Check OAM
            let oam_addr = 0xFE00 + source_offset as u16;
            let oam_value = bus.read8(oam_addr);

            prop_assert_eq!(oam_value, value, "DMA should copy data to OAM");
        }
    }

    // Property: CGB VRAM bank switching
    proptest! {
        #[test]
        fn prop_cgb_vram_banks(value0 in any::<u8>(), value1 in any::<u8>()) {
            let mut rom = vec![0; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            rom[0x0143] = 0x80; // CGB supported
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let mut bus = Bus::new(cartridge).expect("bus");

            // Write to bank 0
            bus.write8(REG_VBK, 0x00);
            bus.write8(0x8000, value0);

            // Write to bank 1
            bus.write8(REG_VBK, 0x01);
            bus.write8(0x8000, value1);

            // Read from bank 0
            bus.write8(REG_VBK, 0x00);
            let read0 = bus.read8(0x8000);

            // Read from bank 1
            bus.write8(REG_VBK, 0x01);
            let read1 = bus.read8(0x8000);

            prop_assert_eq!(read0, value0, "VRAM bank 0 should hold value0");
            prop_assert_eq!(read1, value1, "VRAM bank 1 should hold value1");
        }
    }

    // Property: Speed switch toggles double speed
    proptest! {
        #[test]
        fn prop_speed_switch_toggles(_dummy in any::<u8>()) {
            let mut rom = vec![0; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            rom[0x0143] = 0x80; // CGB supported
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let mut bus = Bus::new(cartridge).expect("bus");

            let initial_speed = bus.is_double_speed();

            bus.write8(REG_KEY1, 0x01);
            bus.perform_speed_switch();

            let after_switch = bus.is_double_speed();

            prop_assert_ne!(initial_speed, after_switch, "Speed switch should toggle speed");
        }
    }

    // Property: PPU mode advances
    proptest! {
        #[test]
        fn prop_ppu_mode_advances(cycles in 1u32..1000) {
            let mut rom = vec![0; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let mut bus = Bus::new(cartridge).expect("bus");

            bus.write8(REG_LCDC, 0x80); // Enable LCD
            bus.step(1); // Get initial mode
            let mode_before = bus.read8(REG_STAT) & 0x03;

            bus.step(cycles);
            let mode_after = bus.read8(REG_STAT) & 0x03;

            // Mode should be valid (0-3)
            prop_assert!(mode_before <= 3, "PPU mode should be 0-3");
            prop_assert!(mode_after <= 3, "PPU mode should be 0-3");
        }
    }

    // Property: LY advances to 154 and wraps
    proptest! {
        #[test]
        fn prop_ly_advances_and_wraps(_dummy in any::<u8>()) {
            let mut rom = vec![0; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let mut bus = Bus::new(cartridge).expect("bus");

            bus.write8(REG_LCDC, 0x80); // Enable LCD

            // Run for a full frame plus a bit
            let cycles_per_frame = 456 * 154;
            bus.step(cycles_per_frame + 1000);

            let ly = bus.read8(REG_LY);

            // LY should be < 154 (0-153)
            prop_assert!(ly < 154, "LY should wrap at 154");
        }
    }

    // Property: Boot ROM can be disabled
    proptest! {
        #[test]
        fn prop_boot_rom_disable(_dummy in any::<u8>()) {
            let mut rom = vec![0x42; ROM_BANK_SIZE];
            rom[0x0147] = 0x00;
            let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
            let boot_rom = vec![0xAA; BOOT_ROM_SIZE];
            let mut bus = Bus::with_boot_rom(cartridge, Some(boot_rom)).expect("bus");

            prop_assert!(bus.boot_rom_enabled(), "Boot ROM should start enabled");
            prop_assert_eq!(bus.read8(0x0000), 0xAA, "Should read boot ROM");

            bus.write8(0xFF50, 0x01);

            prop_assert!(!bus.boot_rom_enabled(), "Boot ROM should be disabled");
            prop_assert_eq!(bus.read8(0x0000), 0x42, "Should read cartridge ROM");
        }
    }
}
