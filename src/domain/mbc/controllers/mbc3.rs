use super::super::helpers::*;
use super::super::rtc::{CYCLES_PER_SECOND, Rtc, RtcMode, RtcRegister};
use crate::domain::{Cartridge, RomBankMapping};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct Mbc3 {
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
    pub fn new(has_rtc: bool) -> Self {
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

    pub fn read8(&self, cartridge: &Cartridge, addr: u16) -> u8 {
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

    pub fn write8(&mut self, cartridge: &mut Cartridge, addr: u16, value: u8) {
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

    pub fn tick(&mut self, cycles: u32) {
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

    pub fn set_rtc_mode(&mut self, mode: RtcMode) {
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
