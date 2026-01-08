pub const CYCLES_PER_SECOND: u32 = 4_194_304;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtcMode {
    Deterministic,
    HostSync,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtcRegister {
    Seconds,
    Minutes,
    Hours,
    DayLow,
    DayHigh,
}

#[derive(Debug, Clone, Copy)]
pub struct Rtc {
    pub seconds: u8,
    pub minutes: u8,
    pub hours: u8,
    pub day_low: u8,
    pub day_high: u8,
}

impl Rtc {
    pub fn read(&self, reg: RtcRegister) -> u8 {
        match reg {
            RtcRegister::Seconds => self.seconds,
            RtcRegister::Minutes => self.minutes,
            RtcRegister::Hours => self.hours,
            RtcRegister::DayLow => self.day_low,
            RtcRegister::DayHigh => self.day_high,
        }
    }

    pub fn write(&mut self, reg: RtcRegister, value: u8) {
        match reg {
            RtcRegister::Seconds => self.seconds = value,
            RtcRegister::Minutes => self.minutes = value,
            RtcRegister::Hours => self.hours = value,
            RtcRegister::DayLow => self.day_low = value,
            RtcRegister::DayHigh => self.day_high = value & 0xC1,
        }
    }

    pub fn tick_seconds(&mut self, seconds: u32) {
        self.add_seconds(u64::from(seconds));
    }

    pub fn day_counter(&self) -> u16 {
        let high = (self.day_high & 0x01) as u16;
        u16::from(self.day_low) | (high << 8)
    }

    pub fn add_seconds(&mut self, seconds: u64) {
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

    pub fn from_unix_seconds(seconds: u64) -> Self {
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
