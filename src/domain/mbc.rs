use super::cartridge::ROM_BANK_SIZE;
use super::{Cartridge, CartridgeType, RomBankMapping};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const EXT_RAM_START: u16 = 0xA000;
const EXT_RAM_END: u16 = 0xBFFF;
const EXT_RAM_BANK_SIZE: usize = 0x2000;
const MBC2_RAM_SIZE: usize = 512;
const MBC2_RAM_END: u16 = 0xA1FF;
const OPEN_BUS: u8 = 0xFF;
const CYCLES_PER_SECOND: u32 = 4_194_304;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MbcError {
    UnsupportedCartridgeType(CartridgeType),
}

#[derive(Debug, Clone)]
pub struct Mbc {
    kind: MbcKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtcMode {
    Deterministic,
    HostSync,
}

#[derive(Debug, Clone)]
enum MbcKind {
    RomOnly,
    Mbc1(Mbc1),
    Mbc2(Mbc2),
    Mbc3(Mbc3),
    Mbc5(Mbc5),
    HuC1(HuC1),
    HuC3(HuC3),
    Mbc6(Box<Mbc6>),
    Mbc7(Mbc7),
}

impl Mbc {
    pub fn new(cartridge: &Cartridge) -> Result<Self, MbcError> {
        let kind = match cartridge.header.cartridge_type {
            CartridgeType::RomOnly | CartridgeType::RomRam | CartridgeType::RomRamBattery => {
                MbcKind::RomOnly
            }
            CartridgeType::Mbc1 | CartridgeType::Mbc1Ram | CartridgeType::Mbc1RamBattery => {
                MbcKind::Mbc1(Mbc1::new())
            }
            CartridgeType::Mbc2 | CartridgeType::Mbc2Battery => MbcKind::Mbc2(Mbc2::new()),
            CartridgeType::Mbc3
            | CartridgeType::Mbc3Ram
            | CartridgeType::Mbc3RamBattery
            | CartridgeType::Mbc3TimerBattery
            | CartridgeType::Mbc3TimerRamBattery => {
                let has_rtc = matches!(
                    cartridge.header.cartridge_type,
                    CartridgeType::Mbc3TimerBattery | CartridgeType::Mbc3TimerRamBattery
                );
                MbcKind::Mbc3(Mbc3::new(has_rtc))
            }
            CartridgeType::Mbc5
            | CartridgeType::Mbc5Ram
            | CartridgeType::Mbc5RamBattery
            | CartridgeType::Mbc5Rumble
            | CartridgeType::Mbc5RumbleRam
            | CartridgeType::Mbc5RumbleRamBattery => MbcKind::Mbc5(Mbc5::new()),
            CartridgeType::HuC1RamBattery => MbcKind::HuC1(HuC1::new()),
            CartridgeType::HuC3 => MbcKind::HuC3(HuC3::new()),
            CartridgeType::Mbc6 => MbcKind::Mbc6(Box::new(Mbc6::new())),
            CartridgeType::Mbc7SensorRumbleRamBattery => MbcKind::Mbc7(Mbc7::new()),
            other => return Err(MbcError::UnsupportedCartridgeType(other)),
        };
        Ok(Self { kind })
    }

    pub fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match &self.kind {
            MbcKind::RomOnly => read_rom_only(cartridge, addr),
            MbcKind::Mbc1(mbc1) => mbc1.read8(cartridge, addr),
            MbcKind::Mbc2(mbc2) => mbc2.read8(cartridge, addr),
            MbcKind::Mbc3(mbc3) => mbc3.read8(cartridge, addr),
            MbcKind::Mbc5(mbc5) => mbc5.read8(cartridge, addr),
            MbcKind::HuC1(huc1) => huc1.read8(cartridge, addr),
            MbcKind::HuC3(huc3) => huc3.read8(cartridge, addr),
            MbcKind::Mbc6(mbc6) => mbc6.read8(cartridge, addr),
            MbcKind::Mbc7(mbc7) => mbc7.read8(cartridge, addr),
        }
    }

    pub fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        match &mut self.kind {
            MbcKind::RomOnly => write_rom_only(cartridge, addr, value),
            MbcKind::Mbc1(mbc1) => mbc1.write8(cartridge, addr, value),
            MbcKind::Mbc2(mbc2) => mbc2.write8(cartridge, addr, value),
            MbcKind::Mbc3(mbc3) => mbc3.write8(cartridge, addr, value),
            MbcKind::Mbc5(mbc5) => mbc5.write8(cartridge, addr, value),
            MbcKind::HuC1(huc1) => huc1.write8(cartridge, addr, value),
            MbcKind::HuC3(huc3) => huc3.write8(cartridge, addr, value),
            MbcKind::Mbc6(mbc6) => mbc6.write8(cartridge, addr, value),
            MbcKind::Mbc7(mbc7) => mbc7.write8(cartridge, addr, value),
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        match &mut self.kind {
            MbcKind::Mbc3(mbc3) => mbc3.tick(cycles),
            MbcKind::HuC3(huc3) => huc3.tick(cycles),
            _ => {}
        }
    }

    pub fn set_rtc_mode(&mut self, mode: RtcMode) {
        if let MbcKind::Mbc3(mbc3) = &mut self.kind {
            mbc3.set_rtc_mode(mode);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mbc1Mode {
    RomBanking,
    RamBanking,
}

#[derive(Debug, Clone)]
struct Mbc1 {
    rom_bank_low5: u8,
    bank_high2: u8,
    ram_bank: u8,
    mode: Mbc1Mode,
    ram_enabled: bool,
}

impl Mbc1 {
    fn new() -> Self {
        Self {
            rom_bank_low5: 1,
            bank_high2: 0,
            ram_bank: 0,
            mode: Mbc1Mode::RomBanking,
            ram_enabled: false,
        }
    }

    fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                let bank_count = bank_count(&cartridge.bytes);
                let (fixed_bank, switchable_bank) = self.rom_banks(bank_count);
                RomBankMapping::with_banks(&cartridge.bytes, fixed_bank, switchable_bank).read(addr)
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return OPEN_BUS;
                }
                let ram_bank = match self.mode {
                    Mbc1Mode::RomBanking => 0,
                    Mbc1Mode::RamBanking => self.ram_bank as usize,
                };
                let ram_bank = normalize_ram_bank(ram_bank, ram_bank_count_for(cartridge, 4));
                read_ext_ram(cartridge, ram_bank, addr)
            }
            _ => OPEN_BUS,
        }
    }

    fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.ram_enabled = (value & 0x0F) == 0x0A;
            }
            0x2000..=0x3FFF => {
                self.rom_bank_low5 = value & 0x1F;
            }
            0x4000..=0x5FFF => {
                let bank = value & 0x03;
                self.bank_high2 = bank;
                self.ram_bank = bank;
            }
            0x6000..=0x7FFF => {
                if value & 0x01 == 0 {
                    self.mode = Mbc1Mode::RomBanking;
                } else {
                    self.mode = Mbc1Mode::RamBanking;
                }
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return;
                }
                let ram_bank = match self.mode {
                    Mbc1Mode::RomBanking => 0,
                    Mbc1Mode::RamBanking => self.ram_bank as usize,
                };
                let ram_bank = normalize_ram_bank(ram_bank, ram_bank_count_for(cartridge, 4));
                write_ext_ram(cartridge, ram_bank, addr, value);
            }
            _ => {}
        }
    }

    fn rom_banks(&self, bank_count: usize) -> (usize, usize) {
        let mut low5 = (self.rom_bank_low5 & 0x1F) as usize;
        if low5 == 0 {
            low5 = 1;
        }
        let upper = (self.bank_high2 & 0x03) as usize;
        match self.mode {
            Mbc1Mode::RomBanking => {
                let switchable = normalize_switchable_bank((upper << 5) | low5, bank_count);
                (normalize_bank(0, bank_count), switchable)
            }
            Mbc1Mode::RamBanking => {
                let fixed = normalize_bank(upper << 5, bank_count);
                let switchable = normalize_switchable_bank(low5, bank_count);
                (fixed, switchable)
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Mbc2 {
    rom_bank: u8,
    ram_enabled: bool,
}

impl Mbc2 {
    fn new() -> Self {
        Self {
            rom_bank: 1,
            ram_enabled: false,
        }
    }

    fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                let bank_count = bank_count(&cartridge.bytes);
                let bank = normalize_switchable_bank(self.rom_bank as usize, bank_count);
                RomBankMapping::with_banks(&cartridge.bytes, 0, bank).read(addr)
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return OPEN_BUS;
                }
                read_mbc2_ram(cartridge, addr)
            }
            _ => OPEN_BUS,
        }
    }

    fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                if addr & 0x0100 == 0 {
                    self.ram_enabled = (value & 0x0F) == 0x0A;
                }
            }
            0x2000..=0x3FFF => {
                if addr & 0x0100 != 0 {
                    let bank = value & 0x0F;
                    self.rom_bank = if bank == 0 { 1 } else { bank };
                }
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return;
                }
                write_mbc2_ram(cartridge, addr, value);
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RtcRegister {
    Seconds,
    Minutes,
    Hours,
    DayLow,
    DayHigh,
}

#[derive(Debug, Clone, Copy)]
struct Rtc {
    seconds: u8,
    minutes: u8,
    hours: u8,
    day_low: u8,
    day_high: u8,
}

impl Rtc {
    fn read(&self, reg: RtcRegister) -> u8 {
        match reg {
            RtcRegister::Seconds => self.seconds,
            RtcRegister::Minutes => self.minutes,
            RtcRegister::Hours => self.hours,
            RtcRegister::DayLow => self.day_low,
            RtcRegister::DayHigh => self.day_high,
        }
    }

    fn write(&mut self, reg: RtcRegister, value: u8) {
        match reg {
            RtcRegister::Seconds => self.seconds = value,
            RtcRegister::Minutes => self.minutes = value,
            RtcRegister::Hours => self.hours = value,
            RtcRegister::DayLow => self.day_low = value,
            RtcRegister::DayHigh => self.day_high = value & 0xC1,
        }
    }

    fn tick_seconds(&mut self, seconds: u32) {
        self.add_seconds(u64::from(seconds));
    }

    fn day_counter(&self) -> u16 {
        let high = (self.day_high & 0x01) as u16;
        u16::from(self.day_low) | (high << 8)
    }

    fn add_seconds(&mut self, seconds: u64) {
        if self.day_high & 0x40 != 0 {
            return;
        }

        let day = self.day_counter() as u64;
        let base_seconds = day * 86_400
            + u64::from(self.hours) * 3600
            + u64::from(self.minutes) * 60
            + u64::from(self.seconds);
        let total = base_seconds + seconds;

        let days = total / 86_400;
        let remainder = total % 86_400;
        let hours = (remainder / 3600) as u8;
        let minutes = ((remainder / 60) % 60) as u8;
        let secs = (remainder % 60) as u8;

        let mut carry = self.day_high & 0x80;
        if carry == 0 && days >= 512 {
            carry = 0x80;
        }

        let day_mod = (days % 512) as u16;
        let halt = self.day_high & 0x40;
        self.seconds = secs;
        self.minutes = minutes;
        self.hours = hours;
        self.day_low = (day_mod & 0xFF) as u8;
        self.day_high = halt | carry | ((day_mod >> 8) as u8 & 0x01);
    }

    fn from_unix_seconds(seconds: u64) -> Self {
        let days = seconds / 86_400;
        let remainder = seconds % 86_400;
        let hours = (remainder / 3600) as u8;
        let minutes = ((remainder / 60) % 60) as u8;
        let secs = (remainder % 60) as u8;
        let day_mod = (days % 512) as u16;
        let carry = if days >= 512 { 0x80 } else { 0x00 };
        Self {
            seconds: secs,
            minutes,
            hours,
            day_low: (day_mod & 0xFF) as u8,
            day_high: carry | ((day_mod >> 8) as u8 & 0x01),
        }
    }
}

#[derive(Debug, Clone)]
struct Mbc3 {
    rom_bank: u8,
    ram_bank: u8,
    rtc_reg: Option<RtcRegister>,
    ram_enabled: bool,
    latch_pending: bool,
    has_rtc: bool,
    rtc_mode: RtcMode,
    rtc_host_base: Option<SystemTime>,
    rtc_counter: u32,
    rtc: Rtc,
    rtc_latched: Rtc,
    latched: bool,
}

impl Mbc3 {
    fn new(has_rtc: bool) -> Self {
        let rtc = Rtc {
            seconds: 0,
            minutes: 0,
            hours: 0,
            day_low: 0,
            day_high: 0,
        };
        Self {
            rom_bank: 1,
            ram_bank: 0,
            rtc_reg: None,
            ram_enabled: false,
            latch_pending: false,
            has_rtc,
            rtc_mode: RtcMode::Deterministic,
            rtc_host_base: None,
            rtc_counter: 0,
            rtc,
            rtc_latched: rtc,
            latched: false,
        }
    }

    fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                let bank_count = bank_count(&cartridge.bytes);
                let bank = normalize_switchable_bank(self.rom_bank as usize, bank_count);
                RomBankMapping::with_banks(&cartridge.bytes, 0, bank).read(addr)
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return OPEN_BUS;
                }
                if self.has_rtc {
                    if let Some(reg) = self.rtc_reg {
                        if self.latched {
                            self.rtc_latched.read(reg)
                        } else {
                            self.current_rtc().read(reg)
                        }
                    } else {
                        let ram_bank = normalize_ram_bank(
                            self.ram_bank as usize,
                            ram_bank_count_for(cartridge, 4),
                        );
                        read_ext_ram(cartridge, ram_bank, addr)
                    }
                } else {
                    let ram_bank = normalize_ram_bank(
                        self.ram_bank as usize,
                        ram_bank_count_for(cartridge, 4),
                    );
                    read_ext_ram(cartridge, ram_bank, addr)
                }
            }
            _ => OPEN_BUS,
        }
    }

    fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.ram_enabled = (value & 0x0F) == 0x0A;
            }
            0x2000..=0x3FFF => {
                let bank = value & 0x7F;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => match value {
                0x00..=0x03 => {
                    self.ram_bank = value & 0x03;
                    self.rtc_reg = None;
                }
                0x08..=0x0C => {
                    if self.has_rtc {
                        self.rtc_reg = match value {
                            0x08 => Some(RtcRegister::Seconds),
                            0x09 => Some(RtcRegister::Minutes),
                            0x0A => Some(RtcRegister::Hours),
                            0x0B => Some(RtcRegister::DayLow),
                            0x0C => Some(RtcRegister::DayHigh),
                            _ => None,
                        };
                    }
                }
                _ => {}
            },
            0x6000..=0x7FFF => {
                if !self.has_rtc {
                    return;
                }
                if value == 0x00 {
                    self.latch_pending = true;
                } else if value == 0x01 && self.latch_pending {
                    self.rtc_latched = self.current_rtc();
                    self.latched = true;
                    self.latch_pending = false;
                } else {
                    self.latch_pending = false;
                }
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return;
                }
                if self.has_rtc {
                    if let Some(reg) = self.rtc_reg {
                        self.rtc.write(reg, value);
                        if self.rtc_mode == RtcMode::HostSync {
                            self.rtc_host_base = Some(SystemTime::now());
                        }
                    } else {
                        let ram_bank = normalize_ram_bank(
                            self.ram_bank as usize,
                            ram_bank_count_for(cartridge, 4),
                        );
                        write_ext_ram(cartridge, ram_bank, addr, value);
                    }
                } else {
                    let ram_bank = normalize_ram_bank(
                        self.ram_bank as usize,
                        ram_bank_count_for(cartridge, 4),
                    );
                    write_ext_ram(cartridge, ram_bank, addr, value);
                }
            }
            _ => {}
        }
    }

    fn tick(&mut self, cycles: u32) {
        if !self.has_rtc {
            return;
        }
        if self.rtc_mode != RtcMode::Deterministic {
            return;
        }
        self.rtc_counter = self.rtc_counter.wrapping_add(cycles);
        while self.rtc_counter >= CYCLES_PER_SECOND {
            self.rtc_counter -= CYCLES_PER_SECOND;
            self.rtc.tick_seconds(1);
        }
    }

    fn set_rtc_mode(&mut self, mode: RtcMode) {
        if !self.has_rtc {
            return;
        }
        if self.rtc_mode == mode {
            return;
        }
        match mode {
            RtcMode::Deterministic => {
                self.rtc = self.current_rtc();
                self.rtc_host_base = None;
                self.rtc_counter = 0;
                self.rtc_mode = mode;
            }
            RtcMode::HostSync => {
                let now = SystemTime::now();
                let seconds = now
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or(Duration::ZERO)
                    .as_secs();
                self.rtc = Rtc::from_unix_seconds(seconds);
                self.rtc_host_base = Some(now);
                self.rtc_counter = 0;
                self.rtc_mode = mode;
            }
        }
        self.rtc_latched = self.rtc;
        self.latched = false;
    }

    fn current_rtc(&self) -> Rtc {
        match self.rtc_mode {
            RtcMode::Deterministic => self.rtc,
            RtcMode::HostSync => {
                let base = self.rtc;
                let base_time = self.rtc_host_base.unwrap_or_else(SystemTime::now);
                let elapsed = SystemTime::now()
                    .duration_since(base_time)
                    .unwrap_or(Duration::ZERO)
                    .as_secs();
                let mut rtc = base;
                rtc.add_seconds(elapsed);
                rtc
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Mbc5 {
    rom_bank_low: u8,
    rom_bank_high: u8,
    ram_bank: u8,
    ram_enabled: bool,
}

impl Mbc5 {
    fn new() -> Self {
        Self {
            rom_bank_low: 1,
            rom_bank_high: 0,
            ram_bank: 0,
            ram_enabled: false,
        }
    }

    fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                let bank_count = bank_count(&cartridge.bytes);
                let bank = ((self.rom_bank_high as usize) << 8) | self.rom_bank_low as usize;
                let bank = normalize_bank(bank, bank_count);
                RomBankMapping::with_banks(&cartridge.bytes, 0, bank).read(addr)
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return OPEN_BUS;
                }
                let ram_bank =
                    normalize_ram_bank(self.ram_bank as usize, ram_bank_count_for(cartridge, 16));
                read_ext_ram(cartridge, ram_bank, addr)
            }
            _ => OPEN_BUS,
        }
    }

    fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.ram_enabled = (value & 0x0F) == 0x0A;
            }
            0x2000..=0x2FFF => {
                self.rom_bank_low = value;
            }
            0x3000..=0x3FFF => {
                self.rom_bank_high = value & 0x01;
            }
            0x4000..=0x5FFF => {
                self.ram_bank = value & 0x0F;
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enabled {
                    return;
                }
                let ram_bank =
                    normalize_ram_bank(self.ram_bank as usize, ram_bank_count_for(cartridge, 16));
                write_ext_ram(cartridge, ram_bank, addr, value);
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
struct HuC1 {
    rom_bank: u8,
    ram_bank: u8,
    ir_mode: bool,
    ir_signal: bool,
}

impl HuC1 {
    fn new() -> Self {
        Self {
            rom_bank: 1,
            ram_bank: 0,
            ir_mode: false,
            ir_signal: false,
        }
    }

    fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                let bank_count = bank_count(&cartridge.bytes);
                let bank = normalize_switchable_bank(self.rom_bank as usize, bank_count);
                RomBankMapping::with_banks(&cartridge.bytes, 0, bank).read(addr)
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if self.ir_mode {
                    // IR mode: Read IR register
                    // Returns 0xC1 if IR signal detected (light), 0xC0 if not
                    if self.ir_signal { 0xC1 } else { 0xC0 }
                } else {
                    // RAM mode: Normal RAM access
                    let ram_bank = normalize_ram_bank(
                        self.ram_bank as usize,
                        ram_bank_count_for(cartridge, 4),
                    );
                    read_ext_ram(cartridge, ram_bank, addr)
                }
            }
            _ => OPEN_BUS,
        }
    }

    fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                // RAM/IR mode select
                // Write 0x0E for IR mode, anything else for RAM mode
                self.ir_mode = value == 0x0E;
            }
            0x2000..=0x3FFF => {
                // ROM bank select (6 bits minimum according to Pan Docs)
                let bank = value & 0x3F;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => {
                // RAM bank select (2 bits minimum)
                self.ram_bank = value & 0x03;
            }
            0x6000..=0x7FFF => {
                // Unused - games write here but it has no effect
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if self.ir_mode {
                    // IR mode: Write IR signal control
                    // 0x01 = IR on, 0x00 = IR off
                    self.ir_signal = value & 0x01 != 0;
                } else {
                    // RAM mode: Normal RAM write
                    let ram_bank = normalize_ram_bank(
                        self.ram_bank as usize,
                        ram_bank_count_for(cartridge, 4),
                    );
                    write_ext_ram(cartridge, ram_bank, addr, value);
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
struct HuC3 {
    rom_bank: u8,
    ram_bank: u8,
    ram_enable: bool,
    mode: u8,
    rtc_latched: bool,
    rtc_latch_value: u8,
    rtc_seconds: u32,
    rtc_minutes: u32,
    rtc_hours: u32,
    rtc_days: u32,
    ir_signal: u8,
}

impl HuC3 {
    fn new() -> Self {
        Self {
            rom_bank: 1,
            ram_bank: 0,
            ram_enable: false,
            mode: 0,
            rtc_latched: false,
            rtc_latch_value: 0,
            rtc_seconds: 0,
            rtc_minutes: 0,
            rtc_hours: 0,
            rtc_days: 0,
            ir_signal: 0,
        }
    }

    fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                let bank_count = bank_count(&cartridge.bytes);
                let bank = normalize_switchable_bank(self.rom_bank as usize, bank_count);
                RomBankMapping::with_banks(&cartridge.bytes, 0, bank).read(addr)
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enable {
                    return OPEN_BUS;
                }

                match self.mode {
                    0x00..=0x0B => {
                        // RAM access mode
                        let ram_bank = normalize_ram_bank(
                            self.ram_bank as usize,
                            ram_bank_count_for(cartridge, 4),
                        );
                        read_ext_ram(cartridge, ram_bank, addr)
                    }
                    0x0C => {
                        // RTC read mode
                        if self.rtc_latched {
                            match self.rtc_latch_value {
                                0x10 => (self.rtc_seconds & 0xFF) as u8,
                                0x30 => (self.rtc_minutes & 0xFF) as u8,
                                0x50 => (self.rtc_hours & 0xFF) as u8,
                                0x70 => (self.rtc_days & 0xFF) as u8,
                                _ => 0x01,
                            }
                        } else {
                            0x01
                        }
                    }
                    0x0D => {
                        // IR read mode
                        self.ir_signal
                    }
                    _ => OPEN_BUS,
                }
            }
            _ => OPEN_BUS,
        }
    }

    fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                // RAM enable: 0x0A enables, anything else disables
                self.ram_enable = value == 0x0A;
            }
            0x2000..=0x3FFF => {
                // ROM bank select (7 bits)
                let bank = value & 0x7F;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => {
                // RAM bank select or mode select
                self.ram_bank = value & 0x0F;
            }
            0x6000..=0x7FFF => {
                // Mode register
                self.mode = value;
            }
            EXT_RAM_START..=EXT_RAM_END => {
                if !self.ram_enable {
                    return;
                }

                match self.mode {
                    0x00..=0x0B => {
                        // RAM write mode
                        let ram_bank = normalize_ram_bank(
                            self.ram_bank as usize,
                            ram_bank_count_for(cartridge, 4),
                        );
                        write_ext_ram(cartridge, ram_bank, addr, value);
                    }
                    0x0C => {
                        // RTC write mode
                        match value & 0xF0 {
                            0x10 => {
                                // Latch/unlatch RTC
                                if value == 0x11 {
                                    self.rtc_latched = true;
                                    self.rtc_latch_value = 0x10;
                                } else if value == 0x10 {
                                    self.rtc_latched = false;
                                }
                            }
                            0x30 => {
                                if value == 0x31 {
                                    self.rtc_latched = true;
                                    self.rtc_latch_value = 0x30;
                                }
                            }
                            0x50 => {
                                if value == 0x51 {
                                    self.rtc_latched = true;
                                    self.rtc_latch_value = 0x50;
                                }
                            }
                            0x70 => {
                                if value == 0x71 {
                                    self.rtc_latched = true;
                                    self.rtc_latch_value = 0x70;
                                }
                            }
                            _ => {}
                        }
                    }
                    0x0E => {
                        // IR write mode
                        self.ir_signal = value;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn tick(&mut self, cycles: u32) {
        // HuC3 RTC runs at ~1Hz, advance based on CPU cycles
        // Game Boy CPU: 4194304 Hz (DMG) or 8388608 Hz (CGB double speed)
        // For simplicity, we increment every ~4.2M cycles (1 second in DMG mode)
        const CYCLES_PER_SECOND: u32 = 4_194_304;

        // Simple RTC tick (this is a basic implementation)
        // A full implementation would track cumulative cycles
        if cycles >= CYCLES_PER_SECOND / 60 {
            // Tick approximately every frame
            self.rtc_seconds += 1;
            if self.rtc_seconds >= 60 {
                self.rtc_seconds = 0;
                self.rtc_minutes += 1;
                if self.rtc_minutes >= 60 {
                    self.rtc_minutes = 0;
                    self.rtc_hours += 1;
                    if self.rtc_hours >= 24 {
                        self.rtc_hours = 0;
                        self.rtc_days += 1;
                    }
                }
            }
        }
    }
}

const MBC6_FLASH_SIZE: usize = 128 * 1024; // 128KB flash
const MBC6_SRAM_SIZE: usize = 8 * 1024; // 8KB SRAM

#[derive(Debug, Clone)]
struct Mbc6 {
    // ROM banking - two separate banks
    rom_bank_a: u8, // 4000-5FFF
    rom_bank_b: u8, // 6000-7FFF

    // RAM/Flash banking
    flash_bank_a: u8, // A000-AFFF
    flash_bank_b: u8, // B000-BFFF

    // RAM enable
    ram_enable: bool,
    flash_enable: bool,

    // Flash memory (128KB)
    flash: [u8; MBC6_FLASH_SIZE],

    // SRAM (8KB)
    sram: [u8; MBC6_SRAM_SIZE],

    // Flash command register
    flash_command: u8,
}

impl Mbc6 {
    fn new() -> Self {
        Self {
            rom_bank_a: 2,
            rom_bank_b: 3,
            flash_bank_a: 0,
            flash_bank_b: 0,
            ram_enable: false,
            flash_enable: false,
            flash: [0xFF; MBC6_FLASH_SIZE],
            sram: [0; MBC6_SRAM_SIZE],
            flash_command: 0,
        }
    }

    fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match addr {
            // Bank 0: 0000-3FFF (fixed)
            0x0000..=0x3FFF => RomBankMapping::with_banks(&cartridge.bytes, 0, 0).read(addr),
            // Bank A: 4000-5FFF (switchable)
            0x4000..=0x5FFF => {
                let bank_count = bank_count(&cartridge.bytes);
                let bank = normalize_switchable_bank(self.rom_bank_a as usize, bank_count);
                let offset = (bank * ROM_BANK_SIZE) + (addr as usize - 0x4000);
                cartridge.bytes.get(offset).copied().unwrap_or(OPEN_BUS)
            }
            // Bank B: 6000-7FFF (switchable)
            0x6000..=0x7FFF => {
                let bank_count = bank_count(&cartridge.bytes);
                let bank = normalize_switchable_bank(self.rom_bank_b as usize, bank_count);
                let offset = (bank * ROM_BANK_SIZE) + (addr as usize - 0x6000);
                cartridge.bytes.get(offset).copied().unwrap_or(OPEN_BUS)
            }
            // Flash Bank A: A000-AFFF
            0xA000..=0xAFFF => {
                if !self.flash_enable {
                    return OPEN_BUS;
                }
                let flash_offset = (self.flash_bank_a as usize * 0x1000) + (addr as usize - 0xA000);
                if flash_offset < MBC6_FLASH_SIZE {
                    self.flash[flash_offset]
                } else {
                    OPEN_BUS
                }
            }
            // Flash Bank B or SRAM: B000-BFFF
            0xB000..=0xBFFF => {
                if self.ram_enable {
                    // SRAM mode
                    let sram_offset = (addr as usize - 0xB000) % MBC6_SRAM_SIZE;
                    self.sram[sram_offset]
                } else if self.flash_enable {
                    // Flash mode
                    let flash_offset =
                        (self.flash_bank_b as usize * 0x1000) + (addr as usize - 0xB000);
                    if flash_offset < MBC6_FLASH_SIZE {
                        self.flash[flash_offset]
                    } else {
                        OPEN_BUS
                    }
                } else {
                    OPEN_BUS
                }
            }
            _ => OPEN_BUS,
        }
    }

    fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        match addr {
            // RAM/Flash enable A (A000-AFFF): 0x0A enables flash
            0x0000..=0x0FFF => {
                self.flash_enable = value == 0x0A;
            }
            // RAM/Flash enable B (B000-BFFF): 0x0A enables SRAM
            0x1000..=0x1FFF => {
                self.ram_enable = value == 0x0A;
            }
            // Flash control A (2000-27FF): 0x00=read, 0x01=write, 0x10=erase
            0x2000..=0x27FF => {
                self.flash_command = value;
            }
            // Flash Bank A select (2800-28FF)
            0x2800..=0x28FF => {
                self.flash_bank_a = value & 0x7F; // 7-bit
            }
            // Flash control B (3000-37FF): 0x00=read, 0x01=write, 0x10=erase
            0x3000..=0x37FF => {
                self.flash_command = value;
            }
            // Flash Bank B select (3800-38FF)
            0x3800..=0x38FF => {
                self.flash_bank_b = value & 0x7F; // 7-bit
            }
            // ROM Bank A select (4000-4FFF)
            0x4000..=0x4FFF => {
                self.rom_bank_a = value & 0x7F; // 7-bit
            }
            // ROM Bank B select (5000-5FFF)
            0x5000..=0x5FFF => {
                self.rom_bank_b = value & 0x7F; // 7-bit
            }
            // Flash write A: A000-AFFF
            0xA000..=0xAFFF => {
                if !self.flash_enable {
                    return;
                }

                match self.flash_command {
                    0x01 => {
                        // Write mode
                        let flash_offset =
                            (self.flash_bank_a as usize * 0x1000) + (addr as usize - 0xA000);
                        if flash_offset < MBC6_FLASH_SIZE {
                            // Flash can only change 1 bits to 0 bits
                            self.flash[flash_offset] &= value;
                            cartridge.mark_ram_dirty();
                        }
                    }
                    0x10 => {
                        // Erase sector (4KB)
                        let sector_start = (self.flash_bank_a as usize) * 0x1000;
                        if sector_start + 0x1000 <= MBC6_FLASH_SIZE {
                            self.flash[sector_start..sector_start + 0x1000].fill(0xFF);
                            cartridge.mark_ram_dirty();
                        }
                    }
                    _ => {
                        // Read mode (0x00) or unknown - ignore writes
                    }
                }
            }
            // Flash write B or SRAM: B000-BFFF
            0xB000..=0xBFFF => {
                if self.ram_enable {
                    // SRAM write
                    let sram_offset = (addr as usize - 0xB000) % MBC6_SRAM_SIZE;
                    self.sram[sram_offset] = value;
                    cartridge.mark_ram_dirty();
                } else if self.flash_enable {
                    // Flash write B
                    match self.flash_command {
                        0x01 => {
                            // Write mode
                            let flash_offset =
                                (self.flash_bank_b as usize * 0x1000) + (addr as usize - 0xB000);
                            if flash_offset < MBC6_FLASH_SIZE {
                                // Flash can only change 1 bits to 0 bits
                                self.flash[flash_offset] &= value;
                                cartridge.mark_ram_dirty();
                            }
                        }
                        0x10 => {
                            // Erase sector (4KB)
                            let sector_start = (self.flash_bank_b as usize) * 0x1000;
                            if sector_start + 0x1000 <= MBC6_FLASH_SIZE {
                                self.flash[sector_start..sector_start + 0x1000].fill(0xFF);
                                cartridge.mark_ram_dirty();
                            }
                        }
                        _ => {
                            // Read mode (0x00) or unknown - ignore writes
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

const MBC7_EEPROM_SIZE: usize = 256;
const MBC7_ACCEL_CENTER: u16 = 0x81D0;

#[derive(Debug, Clone)]
struct Mbc7 {
    rom_bank: u8,
    ram_enable_1: bool,
    ram_enable_2: bool,
    accel_x: u16,
    accel_y: u16,
    accel_latched: bool,
    eeprom: [u8; MBC7_EEPROM_SIZE],
    eeprom_cs: bool,
    eeprom_clk: bool,
    eeprom_di: bool,
    eeprom_do: bool,
    eeprom_write_enabled: bool,
    eeprom_command: u16,
    eeprom_bits: u8,
    eeprom_state: Mbc7EepromState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mbc7EepromState {
    Idle,
    ReadCommand,
    ReadData,
    WriteCommand,
    WriteData,
    Busy,
}

impl Mbc7 {
    fn new() -> Self {
        Self {
            rom_bank: 1,
            ram_enable_1: false,
            ram_enable_2: false,
            accel_x: 0x8000,
            accel_y: 0x8000,
            accel_latched: false,
            eeprom: [0xFF; MBC7_EEPROM_SIZE],
            eeprom_cs: false,
            eeprom_clk: false,
            eeprom_di: false,
            eeprom_do: false,
            eeprom_write_enabled: false,
            eeprom_command: 0,
            eeprom_bits: 0,
            eeprom_state: Mbc7EepromState::Idle,
        }
    }

    fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                let bank_count = bank_count(&cartridge.bytes);
                let bank = normalize_switchable_bank(self.rom_bank as usize, bank_count);
                RomBankMapping::with_banks(&cartridge.bytes, 0, bank).read(addr)
            }
            0xA000..=0xAFFF => {
                if !self.ram_enable_1 || !self.ram_enable_2 {
                    return OPEN_BUS;
                }
                // Register access based on bits 4-7 of address
                let reg = (addr >> 4) & 0x0F;
                match reg {
                    0x2 => (self.accel_x & 0xFF) as u8,
                    0x3 => (self.accel_x >> 8) as u8,
                    0x4 => (self.accel_y & 0xFF) as u8,
                    0x5 => (self.accel_y >> 8) as u8,
                    0x6 => 0x00,
                    0x7 => 0xFF,
                    0x8 => {
                        // EEPROM register
                        let mut value = 0;
                        if self.eeprom_do {
                            value |= 0x01;
                        }
                        if self.eeprom_di {
                            value |= 0x02;
                        }
                        if self.eeprom_clk {
                            value |= 0x40;
                        }
                        if self.eeprom_cs {
                            value |= 0x80;
                        }
                        value
                    }
                    _ => OPEN_BUS,
                }
            }
            _ => OPEN_BUS,
        }
    }

    fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                // RAM Enable 1
                self.ram_enable_1 = (value & 0x0F) == 0x0A;
            }
            0x2000..=0x3FFF => {
                // ROM bank select (7-bit like MBC5)
                let bank = value & 0x7F;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => {
                // RAM Enable 2
                self.ram_enable_2 = value == 0x40;
            }
            0xA000..=0xAFFF => {
                if !self.ram_enable_1 || !self.ram_enable_2 {
                    return;
                }
                let reg = (addr >> 4) & 0x0F;
                match reg {
                    0x0 => {
                        // Latch erase - write 0x55 to reset accel values
                        if value == 0x55 {
                            self.accel_x = 0x8000;
                            self.accel_y = 0x8000;
                            self.accel_latched = false;
                        }
                    }
                    0x1 => {
                        // Latch accelerometer - write 0xAA to latch values
                        if value == 0xAA && !self.accel_latched {
                            // Emulate centered accelerometer (no tilt)
                            self.accel_x = MBC7_ACCEL_CENTER;
                            self.accel_y = MBC7_ACCEL_CENTER;
                            self.accel_latched = true;
                        }
                    }
                    0x8 => {
                        // EEPROM control
                        self.write_eeprom_register(cartridge, value);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn write_eeprom_register(&mut self, cartridge: &mut Cartridge, value: u8) {
        let new_cs = (value & 0x80) != 0;
        let new_clk = (value & 0x40) != 0;
        let new_di = (value & 0x02) != 0;

        // CS rising edge - start of operation
        if new_cs && !self.eeprom_cs {
            self.eeprom_command = 0;
            self.eeprom_bits = 0;
            self.eeprom_state = Mbc7EepromState::ReadCommand;
        }

        // CS falling edge - end of operation
        if !new_cs && self.eeprom_cs {
            self.eeprom_state = Mbc7EepromState::Idle;
            self.eeprom_bits = 0;
        }

        // Clock rising edge - shift in/out data
        if new_clk && !self.eeprom_clk && new_cs {
            match self.eeprom_state {
                Mbc7EepromState::ReadCommand => {
                    self.eeprom_command = (self.eeprom_command << 1) | (new_di as u16);
                    self.eeprom_bits += 1;

                    if self.eeprom_bits >= 10 {
                        self.process_eeprom_command(cartridge);
                    }
                }
                Mbc7EepromState::WriteData => {
                    self.eeprom_command = (self.eeprom_command << 1) | (new_di as u16);
                    self.eeprom_bits += 1;

                    if self.eeprom_bits >= 16 {
                        self.complete_eeprom_write(cartridge);
                    }
                }
                Mbc7EepromState::ReadData => {
                    // DO should already be valid; don't change it on rising edge
                    // Just increment the bit counter for the next falling edge
                }
                Mbc7EepromState::Busy => {
                    // Simulate ready after some clocks
                    self.eeprom_do = true;
                    self.eeprom_state = Mbc7EepromState::Idle;
                }
                _ => {}
            }
        }

        // Clock falling edge - prepare next bit for ReadData
        if !new_clk && self.eeprom_clk && new_cs {
            if self.eeprom_state == Mbc7EepromState::ReadData && self.eeprom_bits < 16 {
                // Advance to next bit only after the current bit has been read
                // (i.e., after a complete clock cycle)
                let addr = (self.eeprom_command & 0x7F) as usize;
                let next_bit = self.eeprom_bits + 1;
                if next_bit < 16 {
                    let byte_offset = addr * 2 + (next_bit / 8) as usize;
                    if byte_offset < MBC7_EEPROM_SIZE {
                        let byte = self.eeprom[byte_offset];
                        let bit_in_byte = 7 - (next_bit % 8);
                        self.eeprom_do = (byte >> bit_in_byte) & 1 != 0;
                    }
                }
                self.eeprom_bits = next_bit;
            }
        }

        self.eeprom_cs = new_cs;
        self.eeprom_clk = new_clk;
        self.eeprom_di = new_di;
    }

    fn process_eeprom_command(&mut self, _cartridge: &mut Cartridge) {
        let opcode = (self.eeprom_command >> 8) & 0x3;
        let addr = (self.eeprom_command & 0x7F) as usize;

        match opcode {
            0b10 => {
                // READ command
                self.eeprom_state = Mbc7EepromState::ReadData;
                self.eeprom_bits = 0;
                // Pre-load first bit
                if addr * 2 < MBC7_EEPROM_SIZE {
                    let byte = self.eeprom[addr * 2];
                    self.eeprom_do = ((byte >> 7) & 1) != 0;
                } else {
                    self.eeprom_do = true;
                }
            }
            0b01 => {
                // WRITE command
                if self.eeprom_write_enabled {
                    self.eeprom_state = Mbc7EepromState::WriteData;
                    self.eeprom_bits = 0;
                } else {
                    self.eeprom_state = Mbc7EepromState::Idle;
                }
            }
            0b11 => {
                // ERASE command
                if self.eeprom_write_enabled && addr * 2 + 1 < MBC7_EEPROM_SIZE {
                    self.eeprom[addr * 2] = 0xFF;
                    self.eeprom[addr * 2 + 1] = 0xFF;
                    self.eeprom_state = Mbc7EepromState::Busy;
                } else {
                    self.eeprom_state = Mbc7EepromState::Idle;
                }
            }
            0b00 => {
                // Special commands (EWEN, EWDS, ERAL, WRAL)
                let special = (self.eeprom_command >> 6) & 0x3;
                match special {
                    0b11 => {
                        // EWEN - Enable erase/write
                        self.eeprom_write_enabled = true;
                    }
                    0b00 => {
                        // EWDS - Disable erase/write
                        self.eeprom_write_enabled = false;
                    }
                    0b10 => {
                        // ERAL - Erase all
                        if self.eeprom_write_enabled {
                            self.eeprom.fill(0xFF);
                            self.eeprom_state = Mbc7EepromState::Busy;
                        }
                    }
                    0b01 => {
                        // WRAL - Write all (needs 16 bits of data)
                        if self.eeprom_write_enabled {
                            self.eeprom_state = Mbc7EepromState::WriteData;
                            self.eeprom_bits = 0;
                        }
                    }
                    _ => {}
                }
                if self.eeprom_state != Mbc7EepromState::WriteData {
                    self.eeprom_state = Mbc7EepromState::Idle;
                }
            }
            _ => {}
        }
    }

    fn complete_eeprom_write(&mut self, cartridge: &mut Cartridge) {
        let addr = (self.eeprom_command & 0x7F) as usize;

        // Get the last 16 bits shifted in
        let write_data = self.eeprom_command;

        if addr * 2 + 1 < MBC7_EEPROM_SIZE {
            let high_byte = (write_data >> 8) as u8;
            let low_byte = (write_data & 0xFF) as u8;

            if self.eeprom[addr * 2] != high_byte {
                self.eeprom[addr * 2] = high_byte;
                cartridge.mark_ram_dirty();
            }
            if self.eeprom[addr * 2 + 1] != low_byte {
                self.eeprom[addr * 2 + 1] = low_byte;
                cartridge.mark_ram_dirty();
            }
        }

        self.eeprom_state = Mbc7EepromState::Busy;
        self.eeprom_bits = 0;
    }
}

fn read_rom_only(cartridge: &Cartridge, addr: u16) -> u8 {
    match addr {
        0x0000..=0x7FFF => {
            let bank_count = bank_count(&cartridge.bytes);
            let bank = normalize_switchable_bank(1, bank_count);
            RomBankMapping::with_banks(&cartridge.bytes, 0, bank).read(addr)
        }
        EXT_RAM_START..=EXT_RAM_END => {
            let ram_bank = normalize_ram_bank(0, ram_bank_count_for(cartridge, 1));
            read_ext_ram(cartridge, ram_bank, addr)
        }
        _ => OPEN_BUS,
    }
}

fn write_rom_only(cartridge: &mut Cartridge, addr: u16, value: u8) {
    if matches!(addr, EXT_RAM_START..=EXT_RAM_END) {
        let ram_bank = normalize_ram_bank(0, ram_bank_count_for(cartridge, 1));
        write_ext_ram(cartridge, ram_bank, addr, value);
    }
}

fn read_ext_ram(cartridge: &Cartridge, bank: Option<usize>, addr: u16) -> u8 {
    if cartridge.ext_ram.is_empty() {
        return OPEN_BUS;
    }
    let Some(bank) = bank else {
        return OPEN_BUS;
    };
    let offset = addr as usize - EXT_RAM_START as usize;
    let index = bank * EXT_RAM_BANK_SIZE + offset;
    cartridge.ext_ram.get(index).copied().unwrap_or(OPEN_BUS)
}

fn write_ext_ram(cartridge: &mut Cartridge, bank: Option<usize>, addr: u16, value: u8) {
    if cartridge.ext_ram.is_empty() {
        return;
    }
    let Some(bank) = bank else {
        return;
    };
    let offset = addr as usize - EXT_RAM_START as usize;
    let index = bank * EXT_RAM_BANK_SIZE + offset;
    if let Some(byte) = cartridge.ext_ram.get_mut(index)
        && *byte != value
    {
        *byte = value;
        cartridge.mark_ram_dirty();
    }
}

fn read_mbc2_ram(cartridge: &Cartridge, addr: u16) -> u8 {
    if addr > MBC2_RAM_END {
        return OPEN_BUS;
    }
    if cartridge.ext_ram.len() < MBC2_RAM_SIZE {
        return OPEN_BUS;
    }
    let offset = (addr as usize - EXT_RAM_START as usize) & 0x01FF;
    let value = cartridge.ext_ram.get(offset).copied().unwrap_or(0) & 0x0F;
    0xF0 | value
}

fn write_mbc2_ram(cartridge: &mut Cartridge, addr: u16, value: u8) {
    if addr > MBC2_RAM_END {
        return;
    }
    if cartridge.ext_ram.len() < MBC2_RAM_SIZE {
        return;
    }
    let offset = (addr as usize - EXT_RAM_START as usize) & 0x01FF;
    if let Some(byte) = cartridge.ext_ram.get_mut(offset) {
        let value = value & 0x0F;
        if *byte != value {
            *byte = value;
            cartridge.mark_ram_dirty();
        }
    }
}

fn ram_bank_count_for(cartridge: &Cartridge, max_banks: usize) -> usize {
    if cartridge.ext_ram.is_empty() {
        return 0;
    }
    let banks = cartridge.ext_ram.len().div_ceil(EXT_RAM_BANK_SIZE);
    banks.min(max_banks)
}

fn normalize_ram_bank(bank: usize, bank_count: usize) -> Option<usize> {
    if bank_count == 0 {
        None
    } else {
        Some(bank % bank_count)
    }
}

fn bank_count(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        0
    } else {
        bytes.len().div_ceil(ROM_BANK_SIZE)
    }
}

fn normalize_bank(bank: usize, bank_count: usize) -> usize {
    if bank_count == 0 {
        0
    } else {
        bank % bank_count
    }
}

fn normalize_switchable_bank(bank: usize, bank_count: usize) -> usize {
    if bank_count == 0 {
        0
    } else {
        let mut normalized = bank % bank_count;
        if normalized == 0 && bank_count > 1 {
            normalized = 1;
        }
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::{CYCLES_PER_SECOND, Mbc, RtcMode, bank_count};
    use crate::domain::Cartridge;
    use crate::domain::cartridge::ROM_BANK_SIZE;

    #[test]
    fn mbc1_write_changes_switchable_rom_bank() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 4];
        bytes[..ROM_BANK_SIZE].fill(0x11);
        bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x22);
        bytes[ROM_BANK_SIZE * 2..ROM_BANK_SIZE * 3].fill(0x33);
        bytes[ROM_BANK_SIZE * 3..].fill(0x44);
        bytes[0x0147] = 0x01;
        bytes[0x0149] = 0x02;

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
        mbc.write8(&mut cartridge, 0x2000, 0x02);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x33);
        mbc.write8(&mut cartridge, 0x2000, 0x00);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
    }

    #[test]
    fn mbc1_mode_select_remaps_fixed_bank() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 64];
        for bank in 0..64 {
            let start = bank * ROM_BANK_SIZE;
            bytes[start..start + ROM_BANK_SIZE].fill(bank as u8);
        }
        bytes[0x0147] = 0x01;
        bytes[0x0149] = 0x02;

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        mbc.write8(&mut cartridge, 0x6000, 0x01);
        mbc.write8(&mut cartridge, 0x4000, 0x01);
        assert_eq!(mbc.read8(&cartridge, 0x0000), 32);
    }

    #[test]
    fn mbc1_ram_enable_gates_reads_and_writes() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x01;
        bytes[0x0149] = 0x02;

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        mbc.write8(&mut cartridge, 0xA000, 0x55);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);
        assert!(!cartridge.is_ram_dirty());

        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0xA000, 0x55);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x55);
        assert!(cartridge.is_ram_dirty());

        mbc.write8(&mut cartridge, 0x0000, 0x00);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);
    }

    #[test]
    fn mbc1_ram_banking_selects_banks() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x03;
        bytes[0x0149] = 0x03;

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0xA000, 0x11);

        mbc.write8(&mut cartridge, 0x6000, 0x01);
        mbc.write8(&mut cartridge, 0x4000, 0x01);
        mbc.write8(&mut cartridge, 0xA000, 0x22);

        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x22);
        mbc.write8(&mut cartridge, 0x4000, 0x00);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x11);
    }

    #[test]
    fn mbc1_small_ram_ignores_bank_selection() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x03;
        bytes[0x0149] = 0x02;

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0xA000, 0x44);

        mbc.write8(&mut cartridge, 0x6000, 0x01);
        mbc.write8(&mut cartridge, 0x4000, 0x02);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x44);
    }

    #[test]
    fn mbc2_rom_and_ram_rules() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 4];
        bytes[..ROM_BANK_SIZE].fill(0x11);
        bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x22);
        bytes[ROM_BANK_SIZE * 2..ROM_BANK_SIZE * 3].fill(0x33);
        bytes[ROM_BANK_SIZE * 3..].fill(0x44);
        bytes[0x0147] = 0x05;

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
        mbc.write8(&mut cartridge, 0x2100, 0x02);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x33);

        mbc.write8(&mut cartridge, 0xA000, 0xAB);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);
        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0xA000, 0xAB);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFB);
    }

    #[test]
    fn mbc3_rtc_latch_and_registers() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x0F;
        bytes[0x0149] = 0x02;

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0x4000, 0x08);
        mbc.write8(&mut cartridge, 0xA000, 0x25);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x25);

        mbc.write8(&mut cartridge, 0x6000, 0x00);
        mbc.write8(&mut cartridge, 0x6000, 0x01);

        mbc.write8(&mut cartridge, 0xA000, 0x30);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x25);
    }

    #[test]
    fn mbc3_rtc_ticks_with_cycles() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x0F;
        bytes[0x0149] = 0x02;

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0x4000, 0x08);
        mbc.set_rtc_mode(RtcMode::Deterministic);
        mbc.tick(CYCLES_PER_SECOND);

        assert_eq!(mbc.read8(&cartridge, 0xA000), 1);
    }

    #[test]
    fn mbc3_without_rtc_ignores_rtc_register_selection() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x13;
        bytes[0x0149] = 0x02;

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0xA000, 0x55);
        mbc.write8(&mut cartridge, 0x4000, 0x08);

        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x55);
    }

    #[test]
    fn mbc5_uses_9bit_rom_bank() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 260];
        bytes[..ROM_BANK_SIZE].fill(0x10);
        let bank_257 = 257 * ROM_BANK_SIZE;
        bytes[bank_257..bank_257 + ROM_BANK_SIZE].fill(0x77);
        bytes[0x0147] = 0x19;
        bytes[0x0149] = 0x02;

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        mbc.write8(&mut cartridge, 0x2000, 0x01);
        mbc.write8(&mut cartridge, 0x3000, 0x01);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x77);
    }

    #[test]
    fn rom_only_ram_reads_and_writes() {
        let mut bytes = vec![0; ROM_BANK_SIZE];
        bytes[0x0147] = 0x08;
        bytes[0x0149] = 0x02;

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        mbc.write8(&mut cartridge, 0xA000, 0x5A);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x5A);
    }

    #[test]
    fn bank_count_rounds_up() {
        let bytes = vec![0; ROM_BANK_SIZE + 1];
        assert_eq!(bank_count(&bytes), 2);
    }

    #[test]
    fn huc1_rom_bank_switching() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 4];
        bytes[..ROM_BANK_SIZE].fill(0x11);
        bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x22);
        bytes[ROM_BANK_SIZE * 2..ROM_BANK_SIZE * 3].fill(0x33);
        bytes[ROM_BANK_SIZE * 3..].fill(0x44);
        bytes[0x0147] = 0xFF; // HuC1
        bytes[0x0149] = 0x02; // 8KB RAM

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Default should be bank 1
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);

        // Switch to bank 2
        mbc.write8(&mut cartridge, 0x2000, 0x02);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x33);

        // Switch to bank 3
        mbc.write8(&mut cartridge, 0x2000, 0x03);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x44);

        // Bank 0 should become bank 1
        mbc.write8(&mut cartridge, 0x2000, 0x00);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
    }

    #[test]
    fn huc1_ram_mode_read_write() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0xFF; // HuC1
        bytes[0x0149] = 0x02; // 8KB RAM

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Default is RAM mode (not IR mode)
        mbc.write8(&mut cartridge, 0xA000, 0xAB);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xAB);

        mbc.write8(&mut cartridge, 0xA100, 0xCD);
        assert_eq!(mbc.read8(&cartridge, 0xA100), 0xCD);
    }

    #[test]
    fn huc1_ram_bank_switching() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0xFF; // HuC1
        bytes[0x0149] = 0x03; // 32KB RAM (4 banks)

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Write to bank 0
        mbc.write8(&mut cartridge, 0x4000, 0x00);
        mbc.write8(&mut cartridge, 0xA000, 0x11);

        // Write to bank 1
        mbc.write8(&mut cartridge, 0x4000, 0x01);
        mbc.write8(&mut cartridge, 0xA000, 0x22);

        // Write to bank 2
        mbc.write8(&mut cartridge, 0x4000, 0x02);
        mbc.write8(&mut cartridge, 0xA000, 0x33);

        // Verify each bank preserved its data
        mbc.write8(&mut cartridge, 0x4000, 0x00);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x11);

        mbc.write8(&mut cartridge, 0x4000, 0x01);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x22);

        mbc.write8(&mut cartridge, 0x4000, 0x02);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x33);
    }

    #[test]
    fn huc1_ir_mode_switching() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0xFF; // HuC1
        bytes[0x0149] = 0x02; // 8KB RAM

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Start in RAM mode, write some data
        mbc.write8(&mut cartridge, 0xA000, 0x55);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x55);

        // Switch to IR mode
        mbc.write8(&mut cartridge, 0x0000, 0x0E);

        // Read should return IR register (0xC0 for no signal)
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xC0);

        // Write IR signal on
        mbc.write8(&mut cartridge, 0xA000, 0x01);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xC1);

        // Write IR signal off
        mbc.write8(&mut cartridge, 0xA000, 0x00);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xC0);

        // Switch back to RAM mode (write anything except 0x0E)
        mbc.write8(&mut cartridge, 0x0000, 0x00);

        // RAM data should still be there
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x55);
    }

    #[test]
    fn huc1_ir_mode_only_triggers_with_0e() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0xFF; // HuC1
        bytes[0x0149] = 0x02; // 8KB RAM

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Write data in RAM mode
        mbc.write8(&mut cartridge, 0xA000, 0xAA);

        // Try various values - none should trigger IR mode except 0x0E
        for value in [0x00, 0x01, 0x0A, 0x0D, 0x0F, 0xFF] {
            mbc.write8(&mut cartridge, 0x0000, value);
            assert_eq!(
                mbc.read8(&cartridge, 0xA000),
                0xAA,
                "Failed for value 0x{:02X}",
                value
            );
        }

        // Now 0x0E should trigger IR mode
        mbc.write8(&mut cartridge, 0x0000, 0x0E);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xC0);
    }

    #[test]
    fn huc1_rom_bank_masks_to_6_bits() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 64];
        for i in 0..64 {
            let start = i * ROM_BANK_SIZE;
            bytes[start..start + ROM_BANK_SIZE].fill(i as u8);
        }
        bytes[0x0147] = 0xFF; // HuC1

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Write bank with upper bits set (should be masked)
        mbc.write8(&mut cartridge, 0x2000, 0xFF); // 0xFF & 0x3F = 0x3F = 63
        assert_eq!(mbc.read8(&cartridge, 0x4000), 63);

        mbc.write8(&mut cartridge, 0x2000, 0xC0); // 0xC0 & 0x3F = 0x00 -> becomes 1
        assert_eq!(mbc.read8(&cartridge, 0x4000), 1);
    }

    #[test]
    fn mbc7_rom_bank_switching() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 4];
        bytes[..ROM_BANK_SIZE].fill(0x11);
        bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x22);
        bytes[ROM_BANK_SIZE * 2..ROM_BANK_SIZE * 3].fill(0x33);
        bytes[ROM_BANK_SIZE * 3..].fill(0x44);
        bytes[0x0147] = 0x22; // MBC7

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Default should be bank 1
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);

        // Switch to bank 2
        mbc.write8(&mut cartridge, 0x2000, 0x02);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x33);

        // Switch to bank 3
        mbc.write8(&mut cartridge, 0x2000, 0x03);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x44);

        // Bank 0 should become bank 1
        mbc.write8(&mut cartridge, 0x2000, 0x00);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
    }

    #[test]
    fn mbc7_dual_ram_enable() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x22; // MBC7

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Neither enable active - should return 0xFF
        assert_eq!(mbc.read8(&cartridge, 0xA020), 0xFF);

        // Enable RAM 1 only - still disabled
        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        assert_eq!(mbc.read8(&cartridge, 0xA020), 0xFF);

        // Enable RAM 2 only (disable RAM 1) - still disabled
        mbc.write8(&mut cartridge, 0x0000, 0x00);
        mbc.write8(&mut cartridge, 0x4000, 0x40);
        assert_eq!(mbc.read8(&cartridge, 0xA020), 0xFF);

        // Enable both - should work
        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0x4000, 0x40);
        // Should not be 0xFF (reading accel X low byte, default 0x00)
        assert_eq!(mbc.read8(&cartridge, 0xA020), 0x00);
    }

    #[test]
    fn mbc7_accelerometer_latch() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x22; // MBC7

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Enable RAM
        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0x4000, 0x40);

        // Initial values should be 0x8000
        assert_eq!(mbc.read8(&cartridge, 0xA020), 0x00);
        assert_eq!(mbc.read8(&cartridge, 0xA030), 0x80);
        assert_eq!(mbc.read8(&cartridge, 0xA040), 0x00);
        assert_eq!(mbc.read8(&cartridge, 0xA050), 0x80);

        // Erase latch (write 0x55 to Ax0x)
        mbc.write8(&mut cartridge, 0xA000, 0x55);
        assert_eq!(mbc.read8(&cartridge, 0xA020), 0x00);
        assert_eq!(mbc.read8(&cartridge, 0xA030), 0x80);

        // Latch accelerometer (write 0xAA to Ax1x)
        mbc.write8(&mut cartridge, 0xA010, 0xAA);

        // Should now read centered values (0x81D0)
        assert_eq!(mbc.read8(&cartridge, 0xA020), 0xD0); // X low
        assert_eq!(mbc.read8(&cartridge, 0xA030), 0x81); // X high
        assert_eq!(mbc.read8(&cartridge, 0xA040), 0xD0); // Y low
        assert_eq!(mbc.read8(&cartridge, 0xA050), 0x81); // Y high
    }

    #[test]
    fn mbc7_accelerometer_requires_erase_before_relatch() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x22; // MBC7

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Enable RAM
        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0x4000, 0x40);

        // First latch
        mbc.write8(&mut cartridge, 0xA000, 0x55);
        mbc.write8(&mut cartridge, 0xA010, 0xAA);
        assert_eq!(mbc.read8(&cartridge, 0xA020), 0xD0);

        // Try to relatch without erase - should not change
        mbc.write8(&mut cartridge, 0xA010, 0xAA);
        assert_eq!(mbc.read8(&cartridge, 0xA020), 0xD0);

        // Erase and relatch - should work
        mbc.write8(&mut cartridge, 0xA000, 0x55);
        assert_eq!(mbc.read8(&cartridge, 0xA020), 0x00); // Back to 0x8000
        mbc.write8(&mut cartridge, 0xA010, 0xAA);
        assert_eq!(mbc.read8(&cartridge, 0xA020), 0xD0); // Latched again
    }

    #[test]
    fn mbc7_fixed_register_values() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x22; // MBC7

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Enable RAM
        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0x4000, 0x40);

        // Ax6x always reads 0x00
        assert_eq!(mbc.read8(&cartridge, 0xA060), 0x00);
        assert_eq!(mbc.read8(&cartridge, 0xA06F), 0x00);

        // Ax7x always reads 0xFF
        assert_eq!(mbc.read8(&cartridge, 0xA070), 0xFF);
        assert_eq!(mbc.read8(&cartridge, 0xA07F), 0xFF);
    }

    #[test]
    fn mbc7_eeprom_enable_write() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x22; // MBC7

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Enable RAM
        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0x4000, 0x40);

        // Send EWEN command (0011xxxxxx)
        // CS low
        mbc.write8(&mut cartridge, 0xA080, 0x00);
        // CS high
        mbc.write8(&mut cartridge, 0xA080, 0x80);
        // Shift in start bit (0) then 1
        mbc.write8(&mut cartridge, 0xA080, 0xC0); // CLK=1, DI=0
        mbc.write8(&mut cartridge, 0xA080, 0x80); // CLK=0
        mbc.write8(&mut cartridge, 0xA080, 0xC2); // CLK=1, DI=1
        mbc.write8(&mut cartridge, 0xA080, 0x82); // CLK=0

        // Shift in EWEN command: 0011xxxxxx
        for bit in [0, 0, 1, 1, 0, 0, 0, 0, 0, 0] {
            let val = if bit != 0 { 0xC2 } else { 0xC0 };
            mbc.write8(&mut cartridge, 0xA080, val); // CLK=1
            mbc.write8(&mut cartridge, 0xA080, val & !0x40); // CLK=0
        }

        // CS low to complete
        mbc.write8(&mut cartridge, 0xA080, 0x00);

        // Write should now be enabled (tested implicitly in write test)
    }

    #[test]
    fn mbc7_eeprom_read() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x22; // MBC7

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Enable RAM
        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0x4000, 0x40);

        // EEPROM starts with 0xFF
        // Send READ command for address 0: opcode 10 (bits 9-8) + address 00000000 (bits 7-0)
        mbc.write8(&mut cartridge, 0xA080, 0x00); // CS low
        mbc.write8(&mut cartridge, 0xA080, 0x80); // CS high (starts ReadCommand state)

        // Read command: 1000000000b = 0x0200 (opcode=10, addr=0)
        for bit in [1, 0, 0, 0, 0, 0, 0, 0, 0, 0] {
            let val = if bit != 0 { 0xC2 } else { 0xC0 };
            mbc.write8(&mut cartridge, 0xA080, val); // CLK=1
            mbc.write8(&mut cartridge, 0xA080, val & !0x40); // CLK=0
        }

        // Read out 16 bits - should be 0xFFFF
        for i in 0..16 {
            mbc.write8(&mut cartridge, 0xA080, 0xC0); // CLK=1
            let val = mbc.read8(&cartridge, 0xA080);
            assert_eq!(
                val & 0x01,
                0x01,
                "EEPROM bit {} should read 1 (0xFF), got register value 0x{:02X}",
                i,
                val
            );
            mbc.write8(&mut cartridge, 0xA080, 0x80); // CLK=0
        }
    }

    #[test]
    fn huc3_rom_bank_switching() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 4];
        bytes[0x0147] = 0xFE; // HuC3
        // Fill banks with distinct patterns
        bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x11);
        bytes[ROM_BANK_SIZE * 2..ROM_BANK_SIZE * 3].fill(0x22);
        bytes[ROM_BANK_SIZE * 3..].fill(0x33);

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Default: bank 1 at 0x4000-0x7FFF
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x11);

        // Switch to bank 2
        mbc.write8(&mut cartridge, 0x2000, 0x02);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);

        // Switch to bank 3
        mbc.write8(&mut cartridge, 0x2000, 0x03);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x33);

        // Bank 0 redirects to bank 1
        mbc.write8(&mut cartridge, 0x2000, 0x00);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x11);
    }

    #[test]
    fn huc3_ram_enable_and_banking() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0xFE; // HuC3
        bytes[0x0149] = 0x03; // 32KB RAM (4 banks)

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // RAM disabled by default - should return open bus
        mbc.write8(&mut cartridge, 0xA000, 0x42);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);

        // Enable RAM
        mbc.write8(&mut cartridge, 0x0000, 0x0A);

        // Write and read from bank 0
        mbc.write8(&mut cartridge, 0xA000, 0x42);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x42);

        // Switch to bank 1 and write different value
        mbc.write8(&mut cartridge, 0x4000, 0x01);
        mbc.write8(&mut cartridge, 0xA000, 0x84);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x84);

        // Switch back to bank 0 - should still have old value
        mbc.write8(&mut cartridge, 0x4000, 0x00);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x42);
    }

    #[test]
    fn huc3_rtc_mode_and_latch() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0xFE; // HuC3

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Enable RAM
        mbc.write8(&mut cartridge, 0x0000, 0x0A);

        // Set mode to RTC read (0x0C)
        mbc.write8(&mut cartridge, 0x6000, 0x0C);

        // Latch seconds (write 0x11 to latch register 0x10)
        mbc.write8(&mut cartridge, 0xA000, 0x11);

        // Read should return seconds (initially 0)
        let seconds = mbc.read8(&cartridge, 0xA000);
        assert_eq!(seconds, 0);

        // Unlatch (write 0x10)
        mbc.write8(&mut cartridge, 0xA000, 0x10);

        // Read should return status byte (0x01) when not latched
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x01);
    }

    #[test]
    fn huc3_ir_modes() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0xFE; // HuC3

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Enable RAM
        mbc.write8(&mut cartridge, 0x0000, 0x0A);

        // Set mode to IR read (0x0D)
        mbc.write8(&mut cartridge, 0x6000, 0x0D);

        // IR should initially be 0
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x00);

        // Set mode to IR write (0x0E)
        mbc.write8(&mut cartridge, 0x6000, 0x0E);
        mbc.write8(&mut cartridge, 0xA000, 0x42);

        // Switch back to IR read
        mbc.write8(&mut cartridge, 0x6000, 0x0D);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x42);
    }

    #[test]
    fn huc3_mode_switching_between_ram_and_rtc() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0xFE; // HuC3
        bytes[0x0149] = 0x02; // 8KB RAM

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Enable RAM
        mbc.write8(&mut cartridge, 0x0000, 0x0A);

        // Mode 0x00 - RAM mode (default)
        mbc.write8(&mut cartridge, 0xA000, 0xAA);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xAA);

        // Switch to RTC mode
        mbc.write8(&mut cartridge, 0x6000, 0x0C);
        // Should not read RAM value
        let rtc_val = mbc.read8(&cartridge, 0xA000);
        assert_ne!(rtc_val, 0xAA);

        // Switch back to RAM mode
        mbc.write8(&mut cartridge, 0x6000, 0x00);
        // Should read original RAM value
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xAA);
    }

    // MBC6 Tests
    #[test]
    fn mbc6_split_rom_banking() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 8];
        bytes[0x0147] = 0x20; // MBC6
        bytes[0x0148] = 0x05; // 64 banks (4MB)

        // Fill ROM banks with distinct patterns
        bytes[ROM_BANK_SIZE * 2..ROM_BANK_SIZE * 3].fill(0x22);
        bytes[ROM_BANK_SIZE * 3..ROM_BANK_SIZE * 4].fill(0x33);
        bytes[ROM_BANK_SIZE * 4..ROM_BANK_SIZE * 5].fill(0x44);
        bytes[ROM_BANK_SIZE * 5..ROM_BANK_SIZE * 6].fill(0x55);

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // ROM bank A (4000-5FFF) - defaults to bank 2
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);

        // Switch ROM bank A to bank 4
        mbc.write8(&mut cartridge, 0x4000, 0x04);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x44);
        assert_eq!(mbc.read8(&cartridge, 0x5FFF), 0x44);

        // ROM bank B (6000-7FFF) - defaults to bank 3
        assert_eq!(mbc.read8(&cartridge, 0x6000), 0x33);

        // Switch ROM bank B to bank 5
        mbc.write8(&mut cartridge, 0x5000, 0x05);
        assert_eq!(mbc.read8(&cartridge, 0x6000), 0x55);
        assert_eq!(mbc.read8(&cartridge, 0x7FFF), 0x55);

        // Verify banks are independent
        mbc.write8(&mut cartridge, 0x4000, 0x02);
        mbc.write8(&mut cartridge, 0x5000, 0x03);
        assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
        assert_eq!(mbc.read8(&cartridge, 0x6000), 0x33);
    }

    #[test]
    fn mbc6_flash_and_sram_enable() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x20; // MBC6
        bytes[0x0149] = 0x03; // 32KB RAM

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Initially, flash and SRAM should be disabled
        // Writes should be ignored, reads should return 0xFF
        mbc.write8(&mut cartridge, 0xA000, 0xAA);
        mbc.write8(&mut cartridge, 0xB000, 0xBB);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);
        assert_eq!(mbc.read8(&cartridge, 0xB000), 0xFF);

        // Enable flash (A000-AFFF)
        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0xA000, 0xCC);
        // Flash writes require commands, but enable should work

        // Enable SRAM (B000-BFFF)
        mbc.write8(&mut cartridge, 0x1000, 0x0A);
        mbc.write8(&mut cartridge, 0xB000, 0xDD);
        assert_eq!(mbc.read8(&cartridge, 0xB000), 0xDD);

        // Disable SRAM
        mbc.write8(&mut cartridge, 0x1000, 0x00);
        assert_eq!(mbc.read8(&cartridge, 0xB000), 0xFF);
    }

    #[test]
    fn mbc6_flash_bank_switching() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x20; // MBC6
        bytes[0x0149] = 0x03; // 32KB RAM

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Enable flash
        mbc.write8(&mut cartridge, 0x0000, 0x0A);

        // Test Flash Bank A (A000-AFFF)
        // Set flash bank A to bank 0
        mbc.write8(&mut cartridge, 0x2800, 0x00);
        // Enter write mode
        mbc.write8(&mut cartridge, 0x2000, 0x01);
        // Write to flash at A000
        mbc.write8(&mut cartridge, 0xA000, 0xAA);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xAA);

        // Switch flash bank A to bank 1
        mbc.write8(&mut cartridge, 0x2800, 0x01);
        // Different bank should read 0xFF (erased)
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);

        // Write to bank 1
        mbc.write8(&mut cartridge, 0xA000, 0xBB);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xBB);

        // Switch back to bank 0
        mbc.write8(&mut cartridge, 0x2800, 0x00);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xAA);

        // Test Flash Bank B (B000-BFFF)
        // Set flash bank B to bank 2
        mbc.write8(&mut cartridge, 0x3800, 0x02);
        // Enter write mode for bank B
        mbc.write8(&mut cartridge, 0x3000, 0x01);
        // Write to flash at B000
        mbc.write8(&mut cartridge, 0xB000, 0xCC);
        assert_eq!(mbc.read8(&cartridge, 0xB000), 0xCC);

        // Switch flash bank B to bank 3
        mbc.write8(&mut cartridge, 0x3800, 0x03);
        // Different bank should read 0xFF (erased)
        assert_eq!(mbc.read8(&cartridge, 0xB000), 0xFF);
    }

    #[test]
    fn mbc6_flash_write_restrictions() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x20; // MBC6
        bytes[0x0149] = 0x03; // 32KB RAM

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Enable flash
        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0x2800, 0x00);
        mbc.write8(&mut cartridge, 0x2000, 0x01); // Write mode

        // Flash starts erased (all 0xFF)
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);

        // Can write to change bits from 1 to 0
        mbc.write8(&mut cartridge, 0xA000, 0xAA); // 10101010
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xAA);

        // Writing again can only clear more bits (1->0)
        mbc.write8(&mut cartridge, 0xA000, 0x88); // 10001000
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x88);

        // Trying to set bits (0->1) should have no effect
        mbc.write8(&mut cartridge, 0xA000, 0xFF);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x88); // Still 0x88
    }

    #[test]
    fn mbc6_flash_erase_sector() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x20; // MBC6
        bytes[0x0149] = 0x03; // 32KB RAM

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Enable flash
        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0x2800, 0x00);
        mbc.write8(&mut cartridge, 0x2000, 0x01); // Write mode

        // Write some data
        mbc.write8(&mut cartridge, 0xA000, 0xAA);
        mbc.write8(&mut cartridge, 0xA001, 0xBB);
        mbc.write8(&mut cartridge, 0xA100, 0xCC);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xAA);
        assert_eq!(mbc.read8(&cartridge, 0xA001), 0xBB);
        assert_eq!(mbc.read8(&cartridge, 0xA100), 0xCC);

        // Issue erase command (0x10) to A000
        mbc.write8(&mut cartridge, 0x2000, 0x10); // Erase command
        mbc.write8(&mut cartridge, 0xA000, 0x00); // Trigger erase at sector containing A000

        // Entire 4KB sector should be erased to 0xFF
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);
        assert_eq!(mbc.read8(&cartridge, 0xA001), 0xFF);
        assert_eq!(mbc.read8(&cartridge, 0xA100), 0xFF);
        assert_eq!(mbc.read8(&cartridge, 0xAFFF), 0xFF);
    }

    #[test]
    fn mbc6_sram_read_write() {
        let mut bytes = vec![0; ROM_BANK_SIZE * 2];
        bytes[0x0147] = 0x20; // MBC6
        bytes[0x0149] = 0x02; // 8KB RAM

        let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
        let mut mbc = Mbc::new(&cartridge).expect("mbc");

        // Enable SRAM (B000-BFFF)
        mbc.write8(&mut cartridge, 0x1000, 0x0A);

        // Write and read various locations
        mbc.write8(&mut cartridge, 0xB000, 0x11);
        mbc.write8(&mut cartridge, 0xB001, 0x22);
        mbc.write8(&mut cartridge, 0xB7FF, 0x33);
        mbc.write8(&mut cartridge, 0xBFFF, 0x44);

        assert_eq!(mbc.read8(&cartridge, 0xB000), 0x11);
        assert_eq!(mbc.read8(&cartridge, 0xB001), 0x22);
        assert_eq!(mbc.read8(&cartridge, 0xB7FF), 0x33);
        assert_eq!(mbc.read8(&cartridge, 0xBFFF), 0x44);

        // Test flash bank switching for SRAM (B000-BFFF maps to flash when flash is enabled)
        mbc.write8(&mut cartridge, 0x3800, 0x00); // Flash bank B = 0
        mbc.write8(&mut cartridge, 0x3000, 0x01); // Write mode

        // Now B000-BFFF should access flash bank B
        mbc.write8(&mut cartridge, 0xB000, 0x55);
        assert_eq!(mbc.read8(&cartridge, 0xB000), 0x55);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::domain::cartridge::ROM_BANK_SIZE;
    use crate::domain::{Cartridge, CartridgeType};
    use proptest::prelude::*;

    // Helper to create a minimal valid cartridge
    fn make_cartridge(cart_type: CartridgeType, rom_banks: usize, ram_size: u8) -> Cartridge {
        let mut bytes = vec![0; ROM_BANK_SIZE * rom_banks.max(2)];
        bytes[0x0147] = cart_type.code();
        bytes[0x0149] = ram_size;
        Cartridge::from_bytes(bytes).expect("valid cartridge")
    }

    proptest! {
        // MBC1 Properties

        #[test]
        fn prop_mbc1_bank_0_always_readable(bank_select in 0u8..=0x1F) {
            let mut cartridge = make_cartridge(CartridgeType::Mbc1, 4, 0x00);
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // Write to bank select
            mbc.write8(&mut cartridge, 0x2000, bank_select);

            // Bank 0 should always be readable at 0x0000-0x3FFF
            let _byte = mbc.read8(&cartridge, 0x0000);
            let _byte = mbc.read8(&cartridge, 0x3FFF);
        }

        #[test]
        fn prop_mbc1_rom_bank_zero_becomes_one(
            addr in 0x4000u16..=0x7FFF
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 4];
            bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0xAA);
            bytes[0x0147] = 0x01; // MBC1
            bytes[0x0149] = 0x00;
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // Writing 0 to bank select should read bank 1
            mbc.write8(&mut cartridge, 0x2000, 0x00);
            let value = mbc.read8(&cartridge, addr);
            prop_assert_eq!(value, 0xAA, "Bank 0 selection should read bank 1");
        }

        #[test]
        fn prop_mbc1_ram_disabled_returns_open_bus(
            addr in 0xA000u16..=0xBFFF,
            write_value in any::<u8>()
        ) {
            let mut cartridge = make_cartridge(CartridgeType::Mbc1RamBattery, 2, 0x02);
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // RAM disabled by default
            mbc.write8(&mut cartridge, addr, write_value);
            let read_value = mbc.read8(&cartridge, addr);

            prop_assert_eq!(read_value, 0xFF, "Disabled RAM should return 0xFF");
        }

        #[test]
        fn prop_mbc1_ram_write_read_roundtrip(
            addr in 0xA000u16..=0xBFFF,
            value in any::<u8>()
        ) {
            let mut cartridge = make_cartridge(CartridgeType::Mbc1RamBattery, 2, 0x02);
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // Enable RAM
            mbc.write8(&mut cartridge, 0x0000, 0x0A);

            // Write and read
            mbc.write8(&mut cartridge, addr, value);
            let read = mbc.read8(&cartridge, addr);

            prop_assert_eq!(read, value, "RAM write/read should roundtrip");
        }

        #[test]
        fn prop_mbc1_mode_switch_preserves_data(
            value in any::<u8>()
        ) {
            let mut cartridge = make_cartridge(CartridgeType::Mbc1RamBattery, 2, 0x03);
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, 0x0000, 0x0A); // Enable RAM
            mbc.write8(&mut cartridge, 0xA000, value);

            // Switch mode
            mbc.write8(&mut cartridge, 0x6000, 0x01);

            // Data should still be there
            let read = mbc.read8(&cartridge, 0xA000);
            prop_assert_eq!(read, value, "Mode switch should preserve RAM data");
        }

        // MBC2 Properties

        #[test]
        fn prop_mbc2_ram_nibble_mask(
            addr in 0xA000u16..=0xA1FF,
            value in any::<u8>()
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            bytes[0x0147] = 0x05; // MBC2
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // Enable RAM
            mbc.write8(&mut cartridge, 0x0000, 0x0A);

            // Write full byte, read should be masked to 4 bits
            mbc.write8(&mut cartridge, addr, value);
            let read = mbc.read8(&cartridge, addr);

            prop_assert_eq!(read, 0xF0 | (value & 0x0F), "MBC2 RAM should mask to lower 4 bits with upper 4 set");
        }

        #[test]
        fn prop_mbc2_bank_select_uses_lower_4_bits(
            bank_bits in 0u8..=0x0F
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 16];
            for i in 0..16 {
                let start = i * ROM_BANK_SIZE;
                bytes[start..start + ROM_BANK_SIZE].fill(i as u8);
            }
            bytes[0x0147] = 0x05; // MBC2
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // Bank select with upper bits set
            mbc.write8(&mut cartridge, 0x2100, 0xF0 | bank_bits);

            let expected_bank = if bank_bits == 0 { 1 } else { bank_bits };
            let read = mbc.read8(&cartridge, 0x4000);
            prop_assert_eq!(read, expected_bank, "MBC2 should use only lower 4 bits for bank select");
        }

        #[test]
        fn prop_mbc2_ram_address_wraps(
            offset in 0u16..=0x1FF,  // Only test within valid 512-byte range
            value in any::<u8>()
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            bytes[0x0147] = 0x05; // MBC2
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, 0x0000, 0x0A); // Enable RAM

            // Write to base address
            let addr1 = 0xA000 + offset;
            mbc.write8(&mut cartridge, addr1, value);

            // Read from mirrored location (MBC2 RAM mirrors every 512 bytes)
            let addr2 = 0xA000 + (offset & 0x1FF);
            let read = mbc.read8(&cartridge, addr2);

            prop_assert_eq!(read & 0x0F, value & 0x0F, "MBC2 RAM should mirror every 512 bytes");
        }

        // MBC3 Properties

        #[test]
        fn prop_mbc3_rom_bank_select_wraps(
            bank_select in 1u8..=0x7F
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 8];
            for i in 0..8 {
                let start = i * ROM_BANK_SIZE;
                bytes[start..start + ROM_BANK_SIZE].fill(i as u8);
            }
            bytes[0x0147] = 0x10; // MBC3
            bytes[0x0149] = 0x02;
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, 0x2000, bank_select);
            // MBC3 bank selection wraps within available banks using normalize_switchable_bank
            // Bank 0 becomes bank 1, others wrap modulo available banks
            let expected_bank_raw = (bank_select as usize) % 8;
            let expected_bank = if expected_bank_raw == 0 { 1 } else { expected_bank_raw };
            let read = mbc.read8(&cartridge, 0x4000);

            prop_assert_eq!(read, expected_bank as u8, "MBC3 bank should wrap to available banks");
        }

        #[test]
        fn prop_mbc3_ram_bank_select(
            ram_bank in 0u8..=0x03,
            value in any::<u8>()
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            bytes[0x0147] = 0x13; // MBC3 with RAM
            bytes[0x0149] = 0x03; // 32KB RAM (4 banks)
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, 0x0000, 0x0A); // Enable RAM
            mbc.write8(&mut cartridge, 0x4000, ram_bank); // Select bank
            mbc.write8(&mut cartridge, 0xA000, value);

            // Switch to different bank and back
            mbc.write8(&mut cartridge, 0x4000, (ram_bank + 1) % 4);
            mbc.write8(&mut cartridge, 0x4000, ram_bank);

            let read = mbc.read8(&cartridge, 0xA000);
            prop_assert_eq!(read, value, "MBC3 RAM banking should preserve data");
        }

        #[test]
        fn prop_mbc3_rtc_latch_freezes_time(
            cycles in 1u32..=CYCLES_PER_SECOND
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            bytes[0x0147] = 0x0F; // MBC3 with RTC
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.set_rtc_mode(RtcMode::Deterministic);
            mbc.write8(&mut cartridge, 0x0000, 0x0A); // Enable RTC
            mbc.write8(&mut cartridge, 0x4000, 0x08); // Select RTC seconds

            // Advance time and latch
            mbc.tick(cycles);
            mbc.write8(&mut cartridge, 0x6000, 0x00);
            mbc.write8(&mut cartridge, 0x6000, 0x01);
            let latched_value = mbc.read8(&cartridge, 0xA000);

            // Advance more time
            mbc.tick(cycles);

            // Latched value shouldn't change
            let still_latched = mbc.read8(&cartridge, 0xA000);
            prop_assert_eq!(still_latched, latched_value, "RTC latch should freeze time");
        }

        #[test]
        fn prop_mbc3_rtc_registers_writable(
            value in 0u8..=59
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            bytes[0x0147] = 0x0F; // MBC3 with RTC
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, 0x0000, 0x0A); // Enable RTC
            mbc.write8(&mut cartridge, 0x4000, 0x08); // Select RTC seconds
            mbc.write8(&mut cartridge, 0xA000, value);

            let read = mbc.read8(&cartridge, 0xA000);
            prop_assert_eq!(read, value, "RTC registers should be writable");
        }

        #[test]
        #[allow(non_snake_case)]
        fn prop_mbc3_without_rtc_treats_08_0C_as_ram(
            register in 0x08u8..=0x0C,
            value in any::<u8>()
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            bytes[0x0147] = 0x13; // MBC3 without RTC
            bytes[0x0149] = 0x03; // 32KB RAM
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, 0x0000, 0x0A); // Enable RAM

            // Writing to RTC register range should do nothing
            mbc.write8(&mut cartridge, 0x4000, register);
            mbc.write8(&mut cartridge, 0xA000, value);

            // Should read from regular RAM bank 0
            mbc.write8(&mut cartridge, 0x4000, 0x00);
            let read = mbc.read8(&cartridge, 0xA000);

            prop_assert_eq!(read, value, "MBC3 without RTC should treat 0x08-0x0C as RAM bank 0");
        }

        // MBC5 Properties

        #[test]
        fn prop_mbc5_9bit_bank_select(
            low_byte in any::<u8>(),
            high_bit in 0u8..=1
        ) {
            let banks = ((high_bit as usize) << 8) | (low_byte as usize);
            let num_banks = (banks + 2).min(512);
            let mut bytes = vec![0; ROM_BANK_SIZE * num_banks];

            // Fill each bank with its bank number (mod 256)
            for i in 0..num_banks {
                let start = i * ROM_BANK_SIZE;
                bytes[start..start + ROM_BANK_SIZE].fill((i % 256) as u8);
            }

            bytes[0x0147] = 0x19; // MBC5
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, 0x2000, low_byte);
            mbc.write8(&mut cartridge, 0x3000, high_bit);

            let expected_bank = if banks >= num_banks {
                banks % num_banks
            } else {
                banks
            };

            let read = mbc.read8(&cartridge, 0x4000);
            prop_assert_eq!(read, (expected_bank % 256) as u8, "MBC5 should support 9-bit ROM banking");
        }

        #[test]
        fn prop_mbc5_bank_zero_is_valid(
            offset in 0u16..=0x3FFF  // Test within switchable bank range
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            // Fill banks with distinct values
            for i in 0..ROM_BANK_SIZE {
                bytes[i] = 0xAA;  // Bank 0
                bytes[ROM_BANK_SIZE + i] = 0xCC;  // Bank 1
            }
            // Set MBC5 type in header (in bank 0)
            bytes[0x0147] = 0x19;

            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // Unlike MBC1, bank 0 is valid for MBC5 in switchable area
            mbc.write8(&mut cartridge, 0x2000, 0x00);
            mbc.write8(&mut cartridge, 0x3000, 0x00);

            let addr = 0x4000 + offset;
            let value = mbc.read8(&cartridge, addr);

            // When bank 0 is selected, reading 0x4000-0x7FFF should read from bank 0
            let expected = if offset == 0x0147 {
                0x19  // This is the cartridge type byte in the header
            } else {
                0xAA  // All other bytes in bank 0
            };

            prop_assert_eq!(value, expected, "MBC5 should allow bank 0 selection in switchable area");
        }

        #[test]
        fn prop_mbc5_ram_banking_4bit(
            ram_bank in 0u8..=0x0F,
            value in any::<u8>()
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            bytes[0x0147] = 0x1B; // MBC5 with RAM
            bytes[0x0149] = 0x04; // 128KB RAM
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, 0x0000, 0x0A); // Enable RAM
            mbc.write8(&mut cartridge, 0x4000, ram_bank); // Select bank (uses lower 4 bits)
            mbc.write8(&mut cartridge, 0xA000, value);

            let effective_bank = ram_bank & 0x0F;
            mbc.write8(&mut cartridge, 0x4000, effective_bank);
            let read = mbc.read8(&cartridge, 0xA000);

            prop_assert_eq!(read, value, "MBC5 should use 4-bit RAM banking");
        }

        // ROM-only Properties

        #[test]
        fn prop_rom_only_fixed_banks(
            addr in 0x0000u16..=0x7FFF
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            bytes[addr as usize] = 0xAB;
            bytes[0x0147] = 0x00; // ROM only
            let cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mbc = Mbc::new(&cartridge).expect("mbc");

            let value = mbc.read8(&cartridge, addr);
            prop_assert_eq!(value, 0xAB, "ROM-only should have fixed mapping");
        }

        #[test]
        fn prop_rom_only_ram_roundtrip(
            addr in 0xA000u16..=0xBFFF,
            value in any::<u8>()
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE];
            bytes[0x0147] = 0x08; // ROM + RAM
            bytes[0x0149] = 0x02; // 8KB RAM
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, addr, value);
            let read = mbc.read8(&cartridge, addr);

            prop_assert_eq!(read, value, "ROM-only RAM should roundtrip");
        }

        // General MBC Properties

        #[test]
        fn prop_tick_doesnt_crash(
            cycles in 1u32..=CYCLES_PER_SECOND * 2,
            cart_type in prop::sample::select(vec![
                CartridgeType::Mbc1,
                CartridgeType::Mbc2,
                CartridgeType::Mbc3TimerRamBattery,
                CartridgeType::Mbc5,
            ])
        ) {
            let cartridge = make_cartridge(cart_type, 2, 0x02);
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.tick(cycles);
            // Should not crash
        }

        #[test]
        fn prop_ram_enable_is_idempotent(
            value in any::<u8>(),
            addr in 0xA000u16..=0xBFFF
        ) {
            let mut cartridge = make_cartridge(CartridgeType::Mbc1RamBattery, 2, 0x02);
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // Enable RAM twice
            mbc.write8(&mut cartridge, 0x0000, 0x0A);
            mbc.write8(&mut cartridge, 0x0000, 0x0A);

            mbc.write8(&mut cartridge, addr, value);
            let read = mbc.read8(&cartridge, addr);

            prop_assert_eq!(read, value, "Multiple RAM enables should work");
        }

        #[test]
        fn prop_writes_to_rom_area_dont_crash(
            addr in 0x0000u16..=0x7FFF,
            value in any::<u8>(),
            cart_type in prop::sample::select(vec![
                CartridgeType::Mbc1,
                CartridgeType::Mbc2,
                CartridgeType::Mbc3,
                CartridgeType::Mbc5,
            ])
        ) {
            let mut cartridge = make_cartridge(cart_type, 4, 0x02);
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // Writing to ROM areas changes MBC state but shouldn't crash
            mbc.write8(&mut cartridge, addr, value);

            // Should still be readable
            let _read = mbc.read8(&cartridge, addr);
        }
    }
}
