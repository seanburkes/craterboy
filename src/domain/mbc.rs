use super::cartridge::ROM_BANK_SIZE;
use super::{Cartridge, CartridgeType, RomBankMapping};

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

#[derive(Debug, Clone)]
enum MbcKind {
    RomOnly,
    Mbc1(Mbc1),
    Mbc2(Mbc2),
    Mbc3(Mbc3),
    Mbc5(Mbc5),
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
            | CartridgeType::Mbc3TimerRamBattery => MbcKind::Mbc3(Mbc3::new()),
            CartridgeType::Mbc5
            | CartridgeType::Mbc5Ram
            | CartridgeType::Mbc5RamBattery
            | CartridgeType::Mbc5Rumble
            | CartridgeType::Mbc5RumbleRam
            | CartridgeType::Mbc5RumbleRamBattery => MbcKind::Mbc5(Mbc5::new()),
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
        }
    }

    pub fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
        match &mut self.kind {
            MbcKind::RomOnly => write_rom_only(cartridge, addr, value),
            MbcKind::Mbc1(mbc1) => mbc1.write8(cartridge, addr, value),
            MbcKind::Mbc2(mbc2) => mbc2.write8(cartridge, addr, value),
            MbcKind::Mbc3(mbc3) => mbc3.write8(cartridge, addr, value),
            MbcKind::Mbc5(mbc5) => mbc5.write8(cartridge, addr, value),
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        if let MbcKind::Mbc3(mbc3) = &mut self.kind {
            mbc3.tick(cycles);
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
            RtcRegister::DayHigh => self.day_high = value,
        }
    }

    fn tick_seconds(&mut self, seconds: u32) {
        if self.day_high & 0x40 != 0 {
            return;
        }

        let mut remaining = seconds;
        while remaining > 0 {
            remaining -= 1;
            self.increment_one_second();
        }
    }

    fn increment_one_second(&mut self) {
        self.seconds = self.seconds.wrapping_add(1);
        if self.seconds < 60 {
            return;
        }
        self.seconds = 0;
        self.minutes = self.minutes.wrapping_add(1);
        if self.minutes < 60 {
            return;
        }
        self.minutes = 0;
        self.hours = self.hours.wrapping_add(1);
        if self.hours < 24 {
            return;
        }
        self.hours = 0;

        let day = self.day_counter();
        if day == 0x1FF {
            self.set_day_counter(0);
            self.day_high |= 0x80;
        } else {
            self.set_day_counter(day + 1);
        }
    }

    fn day_counter(&self) -> u16 {
        let high = (self.day_high & 0x01) as u16;
        u16::from(self.day_low) | (high << 8)
    }

    fn set_day_counter(&mut self, day: u16) {
        self.day_low = day as u8;
        self.day_high = (self.day_high & 0xFE) | ((day >> 8) as u8 & 0x01);
    }
}

#[derive(Debug, Clone)]
struct Mbc3 {
    rom_bank: u8,
    ram_bank: u8,
    rtc_reg: Option<RtcRegister>,
    ram_enabled: bool,
    latch_pending: bool,
    rtc_counter: u32,
    rtc: Rtc,
    rtc_latched: Rtc,
    latched: bool,
}

impl Mbc3 {
    fn new() -> Self {
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
                if let Some(reg) = self.rtc_reg {
                    if self.latched {
                        self.rtc_latched.read(reg)
                    } else {
                        self.rtc.read(reg)
                    }
                } else {
                    read_ext_ram(cartridge, self.ram_bank as usize, addr)
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
                0x08 => self.rtc_reg = Some(RtcRegister::Seconds),
                0x09 => self.rtc_reg = Some(RtcRegister::Minutes),
                0x0A => self.rtc_reg = Some(RtcRegister::Hours),
                0x0B => self.rtc_reg = Some(RtcRegister::DayLow),
                0x0C => self.rtc_reg = Some(RtcRegister::DayHigh),
                _ => {}
            },
            0x6000..=0x7FFF => {
                if value == 0x00 {
                    self.latch_pending = true;
                } else if value == 0x01 && self.latch_pending {
                    self.rtc_latched = self.rtc;
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
                if let Some(reg) = self.rtc_reg {
                    self.rtc.write(reg, value);
                } else {
                    write_ext_ram(cartridge, self.ram_bank as usize, addr, value);
                }
            }
            _ => {}
        }
    }

    fn tick(&mut self, cycles: u32) {
        self.rtc_counter = self.rtc_counter.wrapping_add(cycles);
        while self.rtc_counter >= CYCLES_PER_SECOND {
            self.rtc_counter -= CYCLES_PER_SECOND;
            self.rtc.tick_seconds(1);
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
                read_ext_ram(cartridge, self.ram_bank as usize, addr)
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
                write_ext_ram(cartridge, self.ram_bank as usize, addr, value);
            }
            _ => {}
        }
    }
}

fn read_rom_only(cartridge: &Cartridge, addr: u16) -> u8 {
    match addr {
        0x0000..=0x7FFF => {
            let bank_count = bank_count(&cartridge.bytes);
            let bank = normalize_switchable_bank(1, bank_count);
            RomBankMapping::with_banks(&cartridge.bytes, 0, bank).read(addr)
        }
        EXT_RAM_START..=EXT_RAM_END => read_ext_ram(cartridge, 0, addr),
        _ => OPEN_BUS,
    }
}

fn write_rom_only(cartridge: &mut Cartridge, addr: u16, value: u8) {
    if matches!(addr, EXT_RAM_START..=EXT_RAM_END) {
        write_ext_ram(cartridge, 0, addr, value);
    }
}

fn read_ext_ram(cartridge: &Cartridge, bank: usize, addr: u16) -> u8 {
    if cartridge.ext_ram.is_empty() {
        return OPEN_BUS;
    }
    let offset = addr as usize - EXT_RAM_START as usize;
    let index = bank * EXT_RAM_BANK_SIZE + offset;
    cartridge.ext_ram.get(index).copied().unwrap_or(OPEN_BUS)
}

fn write_ext_ram(cartridge: &mut Cartridge, bank: usize, addr: u16, value: u8) {
    if cartridge.ext_ram.is_empty() {
        return;
    }
    let offset = addr as usize - EXT_RAM_START as usize;
    let index = bank * EXT_RAM_BANK_SIZE + offset;
    if let Some(byte) = cartridge.ext_ram.get_mut(index) {
        *byte = value;
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
        *byte = value & 0x0F;
    }
}

fn bank_count(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        0
    } else {
        (bytes.len() + ROM_BANK_SIZE - 1) / ROM_BANK_SIZE
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
    use super::{CYCLES_PER_SECOND, Mbc, bank_count};
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

        mbc.write8(&mut cartridge, 0x0000, 0x0A);
        mbc.write8(&mut cartridge, 0xA000, 0x55);
        assert_eq!(mbc.read8(&cartridge, 0xA000), 0x55);

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
        mbc.tick(CYCLES_PER_SECOND);

        assert_eq!(mbc.read8(&cartridge, 0xA000), 1);
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
}
