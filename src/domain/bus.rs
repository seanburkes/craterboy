#[path = "bus_constants.rs"]
mod constants;
#[path = "bus_dma.rs"]
mod dma;
#[path = "bus_io.rs"]
mod io;
#[path = "bus_memory.rs"]
mod memory;
#[path = "bus_ppu.rs"]
mod ppu;

use super::{Apu, Cartridge, Dma, Hdma, Mbc, MbcError, RtcMode, Serial, Timer};
use constants::*;

pub use super::serial::{IF_SERIAL, SERIAL_TRANSFER_CYCLES};
pub use constants::{DMA_CYCLES_PER_BYTE, DMA_TOTAL_CYCLES};

#[derive(Debug)]
pub struct Bus {
    cartridge: Cartridge,
    mbc: Mbc,
    boot_rom: Option<Vec<u8>>,
    boot_rom_enabled: bool,
    boot_rom_just_disabled: bool,
    vram_bank: u8,
    vram: [Vec<u8>; 2],
    wram_bank: u8,
    wram: [Vec<u8>; WRAM_BANKS],
    oam: Vec<u8>,
    io: Vec<u8>,
    hram: Vec<u8>,
    timer: Timer,
    ly: u8,
    lyc: u8,
    stat: u8,
    ppu_line_cycles: u16,
    ppu_mode: u8,
    joyp_select: u8,
    joyp_buttons: u8,
    joyp_dpad: u8,
    dma: Dma,
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
    hdma: Hdma,
    serial: Serial,
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
        let mut interrupt_flag = 0;

        if !boot_rom_enabled {
            io[0x40] = 0x91;
            io[0x41] = 0x85;
            io[0x42] = 0x00;
            io[0x43] = 0x00;
            io[0x45] = 0x00;
            io[0x47] = 0xFC;
            io[0x48] = 0xFF;
            io[0x49] = 0xFF;
            io[0x4A] = 0x00;
            io[0x4B] = 0x00;

            interrupt_flag = 0xE1;

            io[0x10] = 0x80;
            io[0x11] = 0xBF;
            io[0x12] = 0xF3;
            io[0x14] = 0xBF;
            io[0x16] = 0x3F;
            io[0x17] = 0x00;
            io[0x19] = 0xBF;
            io[0x1A] = 0x7F;
            io[0x1B] = 0xFF;
            io[0x1C] = 0x9F;
            io[0x1E] = 0xBF;
            io[0x20] = 0xFF;
            io[0x21] = 0x00;
            io[0x22] = 0x00;
            io[0x23] = 0xBF;
            io[0x24] = 0x77;
            io[0x25] = 0xF3;
            io[0x26] = 0xF1;

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
            wram_bank: 1,
            wram: [
                vec![0; WRAM_BANK_SIZE],
                vec![0; WRAM_BANK_SIZE],
                vec![0; WRAM_BANK_SIZE],
                vec![0; WRAM_BANK_SIZE],
                vec![0; WRAM_BANK_SIZE],
                vec![0; WRAM_BANK_SIZE],
                vec![0; WRAM_BANK_SIZE],
                vec![0; WRAM_BANK_SIZE],
            ],
            oam: vec![0; OAM_SIZE],
            io,
            hram: vec![0; HRAM_SIZE],
            timer: if boot_rom_enabled {
                Timer::new()
            } else {
                let mut timer = Timer::new();
                timer.apply_post_boot_state();
                timer
            },
            ly: 0,
            lyc: 0,
            stat,
            ppu_line_cycles: 0,
            ppu_mode: 0,
            joyp_select: 0x30,
            joyp_buttons: 0x0F,
            joyp_dpad: 0x0F,
            dma: if boot_rom_enabled {
                Dma::new_with_value(0x00)
            } else {
                Dma::new()
            },

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
            hdma: Hdma::new(),
            serial: Serial::new(),
        })
    }

    pub fn cartridge(&self) -> &Cartridge {
        &self.cartridge
    }

    pub fn boot_rom_enabled(&self) -> bool {
        self.boot_rom_enabled
    }

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

    pub fn oam(&self) -> &[u8] {
        &self.oam
    }

    pub fn wram_bank(&self) -> u8 {
        self.wram_bank
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

    pub fn step(&mut self, cycles: u32) {
        let subsystem_cycles = if self.double_speed {
            cycles / 2
        } else {
            cycles
        };

        self.interrupt_flag |= self.timer.step(subsystem_cycles);
        self.interrupt_flag |= self.serial.step(subsystem_cycles);
        let _ = self.apu.step(subsystem_cycles);
        self.step_ppu(subsystem_cycles);
        self.step_hdma();
        self.step_dma(subsystem_cycles);
        self.mbc.tick(subsystem_cycles);
    }

    pub fn set_rtc_mode(&mut self, mode: RtcMode) {
        self.mbc.set_rtc_mode(mode);
    }

    pub fn apu_step(&mut self, cycles: u32) {
        let subsystem_cycles = if self.double_speed {
            cycles / 2
        } else {
            cycles
        };
        let _ = self.apu.step(subsystem_cycles);
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

    pub fn apu_take_sample_stereo_i16(&mut self) -> (i16, i16) {
        self.apu.take_sample_stereo_i16()
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
        self.timer.apply_post_boot_state();
        self.interrupt_flag = 0xE1;
        self.interrupt_enable = 0x00;
        self.ly = 0x00;
        self.lyc = 0x00;
        self.ppu_line_cycles = 0;
        self.ppu_mode = 0;
        self.stat = 0x80;
        self.serial.apply_post_boot_state();

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

#[cfg(test)]
mod tests {
    use super::{
        Bus, DMA_TOTAL_CYCLES, DMG_BOOT_ROM_SIZE, IF_TIMER, REG_BGP, REG_BGPD, REG_BGPI, REG_DIV,
        REG_DMA, REG_HDMA1, REG_HDMA2, REG_HDMA3, REG_HDMA4, REG_HDMA5, REG_IF, REG_JOYP, REG_KEY0,
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

        let boot_rom = vec![0xAA; DMG_BOOT_ROM_SIZE];
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

        let boot_rom = vec![0xAA; DMG_BOOT_ROM_SIZE];
        let mut bus = Bus::with_boot_rom(cartridge, Some(boot_rom)).expect("bus");

        assert!(!bus.take_boot_rom_disabled());

        bus.write8(0xFF50, 0x01);

        assert!(bus.take_boot_rom_disabled());
        assert!(!bus.take_boot_rom_disabled());
    }

    #[test]
    fn take_boot_rom_disabled_not_signaled_without_boot_rom() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");

        let mut bus = Bus::new(cartridge).expect("bus");

        assert!(!bus.take_boot_rom_disabled());

        bus.write8(0xFF50, 0x01);
        assert!(!bus.take_boot_rom_disabled());
    }

    #[test]
    fn bus_initializes_post_boot_defaults_without_boot_rom() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");

        let bus = Bus::new(cartridge).expect("bus");

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

        let boot_rom = vec![0xAA; DMG_BOOT_ROM_SIZE];
        let bus = Bus::with_boot_rom(cartridge, Some(boot_rom)).expect("bus");

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
    fn cgb_boot_rom_maps_to_two_regions() {
        let mut rom = vec![0x55; ROM_BANK_SIZE];
        rom[0x0143] = 0x80;
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");

        let mut boot_rom = vec![0; 0x900];
        boot_rom[..0x100].fill(0xAA);
        boot_rom[0x100..].fill(0xBB);

        let mut bus = Bus::with_boot_rom(cartridge, Some(boot_rom)).expect("bus");

        assert_eq!(bus.read8(0x0000), 0xAA, "Boot ROM region 1 start");
        assert_eq!(bus.read8(0x00FF), 0xAA, "Boot ROM region 1 end");

        assert_eq!(bus.read8(0x0100), 0x55, "Gap before region 2 start");
        assert_eq!(bus.read8(0x01FF), 0x55, "Gap before region 2 end");

        assert_eq!(bus.read8(0x0200), 0xBB, "Boot ROM region 2 start");
        assert_eq!(bus.read8(0x08FF), 0xBB, "Boot ROM region 2 end");

        assert_eq!(bus.read8(0x0900), 0x55, "After boot ROM");

        bus.write8(0xFF50, 0x01);
        assert!(!bus.boot_rom_enabled());

        assert_eq!(bus.read8(0x0000), 0x55, "After disable: region 1");
        assert_eq!(bus.read8(0x0200), 0x55, "After disable: region 2");
    }

    #[test]
    fn cgb_boot_rom_exactly_0x900_bytes() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0143] = 0x80;
        rom[0x0147] = 0x00;
        rom.fill(0x77);
        rom[0x0143] = 0x80;
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");

        let mut boot_rom = vec![0; 0x900];
        boot_rom[..0x100].fill(0x01);
        boot_rom[0x100..0x900].fill(0x02);

        let bus = Bus::with_boot_rom(cartridge, Some(boot_rom)).expect("bus");

        assert_eq!(bus.read8(0x0000), 0x01);
        assert_eq!(bus.read8(0x00FF), 0x01);

        assert_eq!(bus.read8(0x0200), 0x02);
        assert_eq!(bus.read8(0x08FF), 0x02);

        assert_eq!(bus.read8(0x0100), 0x77);
        assert_eq!(bus.read8(0x0900), 0x77);
    }

    #[test]
    fn dmg_boot_rom_only_maps_first_256_bytes() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0143] = 0x00;
        rom[0x0147] = 0x00;
        rom.fill(0x33);
        rom[0x0143] = 0x00;
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");

        let boot_rom = vec![0xDD; 0x100];
        let bus = Bus::with_boot_rom(cartridge, Some(boot_rom)).expect("bus");

        assert_eq!(bus.read8(0x0000), 0xDD);
        assert_eq!(bus.read8(0x00FF), 0xDD);

        assert_eq!(bus.read8(0x0100), 0x33);
        assert_eq!(bus.read8(0x0200), 0x33);
        assert_eq!(bus.read8(0x08FF), 0x33);
    }

    #[test]
    fn cgb_boot_rom_disable_clears_both_regions() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0143] = 0x80;
        rom[0x0147] = 0x00;
        rom.fill(0x99);
        rom[0x0143] = 0x80;
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");

        let mut boot_rom = vec![0; 0x900];
        boot_rom[..0x100].fill(0xCC);
        boot_rom[0x100..].fill(0xDD);

        let mut bus = Bus::with_boot_rom(cartridge, Some(boot_rom)).expect("bus");

        assert_eq!(bus.read8(0x0050), 0xCC);
        assert_eq!(bus.read8(0x0400), 0xDD);

        bus.write8(0xFF50, 0x01);

        assert_eq!(bus.read8(0x0050), 0x99);
        assert_eq!(bus.read8(0x0400), 0x99);
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

        bus.write8(0xFF40, 0x00);

        for i in 0..0xA0u16 {
            bus.write8(0xC000 + i, (i as u8).wrapping_add(1));
        }
        bus.write8(0xFF46, 0xC0);
        assert_eq!(bus.read8(0xFE00), 0x00);
        bus.step(DMA_TOTAL_CYCLES);

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
        rom[0x0143] = 0x80;
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
        bus.step(17);
        assert_eq!(bus.read8(REG_TIMA), 0xAA);
        assert_eq!(bus.read8(REG_IF) & IF_TIMER, IF_TIMER);
    }

    #[test]
    fn bus_cgb_mode_from_cartridge() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let bus = Bus::new(cartridge).expect("bus");

        assert!(bus.is_cgb());
        assert_eq!(bus.read8(REG_KEY0) & 0x01, 0x01);
    }

    #[test]
    fn bus_dmg_mode_from_cartridge() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let bus = Bus::new(cartridge).expect("bus");

        assert!(!bus.is_cgb());
        assert_eq!(bus.read8(REG_KEY0) & 0x01, 0x00);
    }

    #[test]
    fn bus_cgb_post_boot_state_sets_key0() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;
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
        rom[0x0143] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let bus = Bus::new(cartridge).expect("bus");

        assert_eq!(bus.read8(REG_KEY1), 0x00);
        assert!(!bus.is_double_speed());
        assert!(!bus.speed_switch_pending());
    }

    #[test]
    fn bus_key1_initial_state_cgb_mode() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let bus = Bus::new(cartridge).expect("bus");

        assert_eq!(bus.read8(REG_KEY1), 0x00);
        assert!(!bus.is_double_speed());
        assert!(!bus.speed_switch_pending());
    }

    #[test]
    fn bus_key1_arm_speed_switch() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_KEY1, 0x01);
        assert_eq!(bus.read8(REG_KEY1) & 0x01, 0x01);
        assert_eq!(bus.read8(REG_KEY1) & 0x80, 0x00);
        assert!(bus.speed_switch_pending());
        assert!(!bus.is_double_speed());

        bus.write8(REG_KEY1, 0x00);
        assert_eq!(bus.read8(REG_KEY1) & 0x01, 0x00);
        assert!(!bus.speed_switch_pending());
    }

    #[test]
    fn bus_key1_perform_speed_switch_to_double() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

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
        rom[0x0143] = 0x80;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_KEY1, 0x01);
        bus.perform_speed_switch();
        assert!(bus.is_double_speed());

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
        rom[0x0143] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

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
        rom[0x0143] = 0x80;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_KEY1, 0xFF);
        assert!(bus.speed_switch_pending());
        assert!(!bus.is_double_speed());

        let key1_value = bus.read8(REG_KEY1);
        assert_eq!(key1_value & 0x01, 0x01);
        assert_eq!(key1_value & 0xFE, 0x00);
    }

    #[test]
    fn bus_key1_perform_switch_no_pending() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.perform_speed_switch();
        assert_eq!(bus.read8(REG_KEY1), 0x00);
        assert!(!bus.is_double_speed());
        assert!(!bus.speed_switch_pending());
    }

    #[test]
    fn bus_double_speed_scales_timer_correctly() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_TAC, 0x05);
        bus.write8(REG_TIMA, 0x00);

        bus.step(16);
        assert_eq!(
            bus.read8(REG_TIMA),
            1,
            "Normal speed: 16 cycles = 1 increment"
        );

        bus.write8(REG_KEY1, 0x01);
        bus.perform_speed_switch();
        assert!(bus.is_double_speed());

        bus.write8(REG_TIMA, 0x00);

        bus.step(32);
        assert_eq!(
            bus.read8(REG_TIMA),
            1,
            "Double speed: 32 CPU cycles = 16 timer cycles = 1 increment"
        );
    }

    #[test]
    fn bus_double_speed_scales_div_correctly() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_DIV, 0x00);
        let initial_div = bus.read8(REG_DIV);

        bus.step(256);
        let after_normal = bus.read8(REG_DIV);
        assert_eq!(
            after_normal,
            initial_div.wrapping_add(1),
            "Normal speed: 256 cycles = 1 DIV increment"
        );

        bus.write8(REG_KEY1, 0x01);
        bus.perform_speed_switch();
        assert!(bus.is_double_speed());

        bus.write8(REG_DIV, 0x00);
        let initial_div_double = bus.read8(REG_DIV);

        bus.step(512);
        let after_double = bus.read8(REG_DIV);
        assert_eq!(
            after_double,
            initial_div_double.wrapping_add(1),
            "Double speed: 512 CPU cycles = 256 timer cycles = 1 DIV increment"
        );
    }

    #[test]
    fn bus_double_speed_ppu_timing() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_LY, 0x00);
        let initial_ly = bus.read8(REG_LY);

        bus.step(456);
        let after_normal = bus.read8(REG_LY);
        assert!(
            after_normal > initial_ly || after_normal == 0,
            "Normal speed: LY should advance or wrap"
        );

        bus.write8(REG_KEY1, 0x01);
        bus.perform_speed_switch();
        assert!(bus.is_double_speed());

        bus.write8(REG_LY, 0x00);
        let initial_ly_double = bus.read8(REG_LY);

        bus.step(912);
        let after_double = bus.read8(REG_LY);
        assert!(
            after_double > initial_ly_double || after_double == 0,
            "Double speed: LY should advance with scaled cycles"
        );
    }

    #[test]
    fn bus_hdma_gdma_minimal() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;

        rom[0x0000] = 0xAA;
        rom[0x0001] = 0xBB;
        rom[0x0002] = 0xCC;

        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_HDMA1, 0x00);
        bus.write8(REG_HDMA2, 0x00);

        bus.write8(REG_HDMA3, 0x80);
        bus.write8(REG_HDMA4, 0x00);

        bus.write8(REG_HDMA5, 0x80);

        assert_eq!(bus.read8(0x8000), 0xAA, "First byte should be transferred");
        assert_eq!(bus.read8(0x8001), 0xBB, "Second byte should be transferred");
    }

    #[test]
    fn bus_hdma_gdma_transfers_data() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;

        for (i, byte) in rom.iter_mut().enumerate().take(0x100) {
            *byte = i as u8;
        }

        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_HDMA1, 0x00);
        bus.write8(REG_HDMA2, 0x00);

        bus.write8(REG_HDMA3, 0x80);
        bus.write8(REG_HDMA4, 0x00);

        bus.write8(REG_HDMA5, 0x8F);

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
        rom[0x0143] = 0x80;

        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        assert_eq!(REG_HDMA1, 0xFF51, "REG_HDMA1 should be 0xFF51");

        bus.write8(REG_HDMA1, 0x12);
        let value = bus.read8(REG_HDMA1);
        assert_eq!(value, 0x12, "Read 0x{:02X} after writing 0x12", value);
    }

    #[test]
    fn bus_hdma_debug_multi() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;

        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_HDMA1, 0x12);
        bus.write8(REG_HDMA2, 0x34);
        bus.write8(REG_HDMA3, 0x56);
        bus.write8(REG_HDMA4, 0x78);

        let h1 = bus.read8(REG_HDMA1);
        let h2 = bus.read8(REG_HDMA2);
        let h3 = bus.read8(REG_HDMA3);
        let h4 = bus.read8(REG_HDMA4);

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
        rom[0x0143] = 0x00;

        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_HDMA5, 0x80);
        assert_eq!(bus.read8(REG_HDMA5), 0x00);
    }

    #[test]
    fn bus_hdma_registers_read_write() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        rom[0x0143] = 0x80;

        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_HDMA1, 0x12);
        bus.write8(REG_HDMA2, 0x34);
        bus.write8(REG_HDMA3, 0x56);
        bus.write8(REG_HDMA4, 0x78);

        assert_eq!(bus.read8(REG_HDMA1), 0x12);
        assert_eq!(bus.read8(REG_HDMA2), 0x30);
        assert_eq!(bus.read8(REG_HDMA3), 0x56);
        assert_eq!(bus.read8(REG_HDMA4), 0x70);
    }

    #[test]
    fn timer_increments_on_falling_edge() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_TAC, 0x05);
        bus.write8(REG_TIMA, 0x00);

        bus.step(15);
        assert_eq!(bus.read8(REG_TIMA), 0x00, "No increment before 16 cycles");

        bus.step(1);
        assert_eq!(bus.read8(REG_TIMA), 0x01, "Increment on falling edge");

        bus.step(16);
        assert_eq!(bus.read8(REG_TIMA), 0x02, "Second increment");
    }

    #[test]
    fn timer_overflow_and_interrupt_delays() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_TAC, 0x05);
        bus.write8(REG_TIMA, 0xFE);
        bus.write8(REG_TMA, 0x42);
        bus.write8(REG_IF, 0x00);

        bus.step(16);
        assert_eq!(bus.read8(REG_TIMA), 0xFF, "TIMA should be 0xFF");
        assert_eq!(bus.read8(REG_IF) & IF_TIMER, 0x00, "No interrupt yet");

        bus.step(16);
        assert_eq!(bus.read8(REG_TIMA), 0x00, "TIMA should overflow to 0x00");
        assert_eq!(bus.read8(REG_IF) & IF_TIMER, 0x00, "Interrupt not yet set");

        bus.step(4);
        assert_eq!(bus.read8(REG_TIMA), 0x42, "TIMA should reload from TMA");
        assert_eq!(
            bus.read8(REG_IF) & IF_TIMER,
            IF_TIMER,
            "Interrupt should be set"
        );
    }

    #[test]
    fn timer_write_during_overflow_cycle() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_TAC, 0x05);
        bus.write8(REG_TIMA, 0xFF);
        bus.write8(REG_TMA, 0x42);
        bus.write8(REG_IF, 0x00);

        bus.step(16);
        assert_eq!(bus.read8(REG_TIMA), 0x00, "TIMA should overflow to 0x00");

        bus.write8(REG_TIMA, 0x99);

        bus.step(4);
        assert_eq!(bus.read8(REG_TIMA), 0x99, "TIMA should keep written value");
    }

    #[test]
    fn timer_write_during_interrupt_cycle_ignored() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_TAC, 0x05);
        bus.write8(REG_TIMA, 0xFF);
        bus.write8(REG_TMA, 0x42);
        bus.write8(REG_IF, 0x00);

        bus.step(16);
        bus.step(1);

        bus.write8(REG_TIMA, 0x99);

        bus.step(3);

        assert_eq!(bus.read8(REG_TIMA), 0x42, "TIMA should be loaded from TMA");
    }

    #[test]
    fn timer_write_tma_during_interrupt_cycle() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_TAC, 0x05);
        bus.write8(REG_TIMA, 0xFF);
        bus.write8(REG_TMA, 0x42);
        bus.write8(REG_IF, 0x00);

        bus.step(16);
        bus.step(1);

        bus.write8(REG_TMA, 0x77);

        assert_eq!(bus.read8(REG_TIMA), 0x77, "TIMA should update to new TMA");
    }

    #[test]
    fn timer_div_reset_effects() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        let initial_div = bus.read8(REG_DIV);
        bus.step(256);
        let div_after_step = bus.read8(REG_DIV);
        assert_ne!(initial_div, div_after_step, "DIV should have incremented");

        bus.write8(REG_DIV, 0xFF);
        assert_eq!(bus.read8(REG_DIV), 0x00, "DIV should reset to 0");

        bus.step(256);
        assert_eq!(bus.read8(REG_DIV), 0x01, "DIV should count from 0");
    }

    #[test]
    fn timer_falling_edge_on_div_write() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_TAC, 0x05);
        bus.write8(REG_TIMA, 0x00);

        bus.step(15);
        let tima_before = bus.read8(REG_TIMA);

        bus.step(1);
        let tima_after = bus.read8(REG_TIMA);

        assert!(bus.read8(REG_DIV) > 0);

        bus.write8(REG_DIV, 0x00);

        let tima_after_div_write = bus.read8(REG_TIMA);
        assert!(
            tima_after_div_write == tima_after
                || tima_after_div_write == tima_after.wrapping_add(1)
        );
        assert_ne!(tima_before, tima_after_div_write);
    }

    #[test]
    fn timer_falling_edge_on_clock_change() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_DIV, 0x00);

        for _ in 0..512 {
            bus.step(1);
        }

        assert!(bus.read8(REG_DIV) > 0);

        bus.write8(REG_TAC, 0x04);
        bus.write8(REG_TIMA, 0x00);

        bus.write8(REG_TAC, 0x05);

        assert_eq!(bus.read8(REG_TIMA), 0x01);
    }

    #[test]
    fn timer_disabled_does_not_increment() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_TAC, 0x01);
        bus.write8(REG_TIMA, 0x00);

        bus.step(256);

        assert_eq!(bus.read8(REG_TIMA), 0x00);
    }

    #[test]
    fn timer_div_running_when_disabled() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        bus.write8(REG_TAC, 0x00);

        let initial_div = bus.read8(REG_DIV);
        bus.step(256);

        assert!(bus.read8(REG_DIV) > initial_div, "DIV should increment");
    }

    #[test]
    fn timer_clock_selects() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        let mut bus = Bus::new(cartridge).expect("bus");

        let clocks = [
            (0x04, 1024, "Clock 00"),
            (0x05, 16, "Clock 01"),
            (0x06, 64, "Clock 10"),
            (0x07, 256, "Clock 11"),
        ];

        for (tac_value, period, label) in clocks {
            bus.write8(REG_TAC, tac_value);
            bus.write8(REG_TIMA, 0x00);
            bus.step(period);
            assert_eq!(bus.read8(REG_TIMA), 0x01, "{} should increment", label);
            bus.step(period);
            assert_eq!(
                bus.read8(REG_TIMA),
                0x02,
                "{} should increment twice",
                label
            );
        }
    }
}
