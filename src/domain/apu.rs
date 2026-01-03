use super::Bus;

const SAMPLE_RATE: u32 = 4_194_304 / 70_224;
const FRAME_CYCLES: u32 = 70_224;

const REG_NR30: u16 = 0xFF1A;
const REG_NR31: u16 = 0xFF1B;
const REG_NR32: u16 = 0xFF1C;
const REG_NR33: u16 = 0xFF1D;
const REG_NR34: u16 = 0xFF1E;
const WAVE_RAM_START: u16 = 0xFF30;
const WAVE_RAM_SIZE: usize = 16;

const REG_NR41: u16 = 0xFF20;
const REG_NR42: u16 = 0xFF21;
const REG_NR43: u16 = 0xFF22;
const REG_NR44: u16 = 0xFF23;

const FREQ_DIVISOR: u32 = 131072;
const NOISE_CLOCK_BASE: u32 = 524288;

#[derive(Debug)]
pub struct WaveChannel {
    enabled: bool,
    length: u8,
    volume_code: u8,
    frequency: u16,
    length_enable: bool,
    trigger: bool,
    wave_ram: [u8; WAVE_RAM_SIZE],
    position: u8,
    timer: u32,
    output_volume: i32,
}

impl WaveChannel {
    pub fn new() -> Self {
        Self {
            enabled: false,
            length: 0,
            volume_code: 0,
            frequency: 0,
            length_enable: false,
            trigger: false,
            wave_ram: [0xFF; WAVE_RAM_SIZE],
            position: 0,
            timer: 0,
            output_volume: 0,
        }
    }

    pub fn step(&mut self, cycles: u32) -> i32 {
        if !self.enabled || self.frequency == 0 {
            self.output_volume = 0;
            return 0;
        }

        let freq = self.frequency as u32;
        let divisor = 2048 - freq;
        let step_rate = FREQ_DIVISOR / divisor;

        self.timer = self.timer.wrapping_add(cycles);

        let timer_threshold = (4_194_304 / step_rate) / 2;
        while self.timer >= timer_threshold {
            self.timer -= timer_threshold;
            self.position = (self.position + 1) & 0x3F;
        }

        let wave_byte = self.wave_ram[(self.position as usize / 2) % WAVE_RAM_SIZE];
        let sample = if self.position & 1 == 0 {
            wave_byte >> 4
        } else {
            wave_byte & 0x0F
        };

        let volume = match self.volume_code {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 4,
            _ => 0,
        };

        self.output_volume = ((sample as i32) * volume) as i32 / 4;
        self.output_volume
    }

    pub fn reset(&mut self) {
        self.enabled = false;
        self.length = 0;
        self.volume_code = 0;
        self.frequency = 0;
        self.length_enable = false;
        self.trigger = false;
        self.position = 0;
        self.timer = 0;
        self.output_volume = 0;
    }

    pub fn read_io(&self, addr: u16) -> u8 {
        match addr {
            REG_NR30 => {
                let mut value = 0;
                if self.enabled {
                    value |= 0x80;
                }
                value
            }
            REG_NR31 => self.length,
            REG_NR32 => (self.volume_code & 0x03) << 5,
            REG_NR33 => self.frequency as u8,
            REG_NR34 => {
                let mut value = 0;
                if self.length_enable {
                    value |= 0x40;
                }
                value | ((self.frequency >> 8) as u8) & 0x07
            }
            _ => 0,
        }
    }

    pub fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            REG_NR30 => {
                self.enabled = value & 0x80 != 0;
            }
            REG_NR31 => {
                self.length = value;
            }
            REG_NR32 => {
                self.volume_code = (value >> 5) & 0x03;
            }
            REG_NR33 => {
                self.frequency = (self.frequency & 0xFF00) | (value as u16);
            }
            REG_NR34 => {
                self.length_enable = value & 0x40 != 0;
                let new_freq_high = (value as u16) & 0x07;
                self.frequency = (self.frequency & 0x00FF) | (new_freq_high << 8);
                if value & 0x80 != 0 {
                    self.trigger = true;
                    self.enabled = true;
                    self.position = 0;
                    self.timer = 0;
                }
            }
            _ => {}
        }
    }

    pub fn read_wave_ram(&self, addr: u16) -> u8 {
        let index = (addr - WAVE_RAM_START) as usize;
        if index < WAVE_RAM_SIZE {
            self.wave_ram[index]
        } else {
            0xFF
        }
    }

    pub fn write_wave_ram(&mut self, addr: u16, value: u8) {
        let index = (addr - WAVE_RAM_START) as usize;
        if index < WAVE_RAM_SIZE {
            self.wave_ram[index] = value;
        }
    }

    pub fn output(&self) -> i32 {
        self.output_volume
    }
}

#[derive(Debug)]
pub struct NoiseChannel {
    enabled: bool,
    length: u8,
    volume: u8,
    envelope_add: bool,
    envelope_period: u8,
    length_enable: bool,
    shift_clock_frequency: u8,
    seven_bit_mode: bool,
    divisor_code: u8,
    lfsr: u16,
    timer: u32,
    envelope_counter: u8,
    current_volume: u8,
    output_volume: i32,
}

impl NoiseChannel {
    pub fn new() -> Self {
        Self {
            enabled: false,
            length: 0,
            volume: 0,
            envelope_add: false,
            envelope_period: 0,
            length_enable: false,
            shift_clock_frequency: 0,
            seven_bit_mode: false,
            divisor_code: 0,
            lfsr: 0,
            timer: 0,
            envelope_counter: 0,
            current_volume: 0,
            output_volume: 0,
        }
    }

    fn divisor_value(code: u8) -> u32 {
        match code {
            0 => 8,
            1 => 16,
            2 => 32,
            3 => 48,
            4 => 64,
            5 => 80,
            6 => 96,
            7 => 112,
            _ => 8,
        }
    }

    pub fn step(&mut self, cycles: u32) -> i32 {
        if !self.enabled {
            self.output_volume = 0;
            return 0;
        }

        let divisor = Self::divisor_value(self.divisor_code);
        let shift = self.shift_clock_frequency;
        let threshold = (NOISE_CLOCK_BASE / (divisor << shift)) / 2;

        self.timer = self.timer.wrapping_add(cycles);

        while self.timer >= threshold {
            self.timer -= threshold;
            self.clock_lfsr();
        }

        self.output_volume = if (self.lfsr & 0x01) == 0 {
            self.current_volume as i32
        } else {
            -(self.current_volume as i32)
        };

        self.output_volume
    }

    fn clock_lfsr(&mut self) {
        let feedback = (self.lfsr & 0x01) ^ ((self.lfsr >> 1) & 0x01);
        self.lfsr >>= 1;
        if self.seven_bit_mode {
            self.lfsr &= 0x7F;
            self.lfsr |= feedback << 6;
        } else {
            self.lfsr &= 0x7FFF;
            self.lfsr |= feedback << 14;
        }
    }

    pub fn reset(&mut self) {
        self.enabled = false;
        self.length = 0;
        self.volume = 0;
        self.envelope_add = false;
        self.envelope_period = 0;
        self.length_enable = false;
        self.shift_clock_frequency = 0;
        self.seven_bit_mode = false;
        self.divisor_code = 0;
        self.lfsr = 0;
        self.timer = 0;
        self.envelope_counter = 0;
        self.current_volume = 0;
        self.output_volume = 0;
    }

    pub fn read_io(&self, addr: u16) -> u8 {
        match addr {
            REG_NR41 => self.length & 0x3F,
            REG_NR42 => {
                (self.volume << 4)
                    | (if self.envelope_add { 0x08 } else { 0 })
                    | (self.envelope_period & 0x07)
            }
            REG_NR43 => {
                (self.shift_clock_frequency << 4)
                    | (if self.seven_bit_mode { 0x08 } else { 0 })
                    | (self.divisor_code & 0x07)
            }
            REG_NR44 => {
                let mut value = 0;
                if self.length_enable {
                    value |= 0x40;
                }
                value
            }
            _ => 0,
        }
    }

    pub fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            REG_NR41 => {
                self.length = value & 0x3F;
            }
            REG_NR42 => {
                self.volume = (value >> 4) & 0x0F;
                self.envelope_add = value & 0x08 != 0;
                self.envelope_period = value & 0x07;
                self.current_volume = self.volume;
            }
            REG_NR43 => {
                self.shift_clock_frequency = (value >> 4) & 0x0F;
                self.seven_bit_mode = value & 0x08 != 0;
                self.divisor_code = value & 0x07;
            }
            REG_NR44 => {
                self.length_enable = value & 0x40 != 0;
                if value & 0x80 != 0 {
                    self.trigger();
                }
            }
            _ => {}
        }
    }

    fn trigger(&mut self) {
        self.enabled = true;
        self.lfsr = 0x7FFF;
        self.timer = 0;
        self.current_volume = self.volume;
        self.envelope_counter = self.envelope_period;
    }

    pub fn output(&self) -> i32 {
        self.output_volume
    }
}

#[derive(Debug)]
pub struct Apu {
    frame_cycles: u32,
    wave_channel: WaveChannel,
    noise_channel: NoiseChannel,
}

impl Apu {
    pub fn new() -> Self {
        Self {
            frame_cycles: FRAME_CYCLES,
            wave_channel: WaveChannel::new(),
            noise_channel: NoiseChannel::new(),
        }
    }

    pub fn step(&mut self, cycles: u32) -> Result<(), ()> {
        self.frame_cycles = self.frame_cycles.wrapping_add(cycles);
        self.wave_channel.step(cycles);
        self.noise_channel.step(cycles);
        Ok(())
    }

    pub fn samples_per_frame(&self) -> u32 {
        self.frame_cycles
    }

    pub fn reset(&mut self) {
        self.frame_cycles = FRAME_CYCLES;
        self.wave_channel.reset();
        self.noise_channel.reset();
    }

    pub fn read_io(&self, addr: u16) -> u8 {
        match addr {
            REG_NR30 | REG_NR31 | REG_NR32 | REG_NR33 | REG_NR34 => self.wave_channel.read_io(addr),
            REG_NR41 | REG_NR42 | REG_NR43 | REG_NR44 => self.noise_channel.read_io(addr),
            WAVE_RAM_START..=0xFF3F => self.wave_channel.read_wave_ram(addr),
            _ => 0,
        }
    }

    pub fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            REG_NR30 | REG_NR31 | REG_NR32 | REG_NR33 | REG_NR34 => {
                self.wave_channel.write_io(addr, value);
            }
            REG_NR41 | REG_NR42 | REG_NR43 | REG_NR44 => {
                self.noise_channel.write_io(addr, value);
            }
            WAVE_RAM_START..=0xFF3F => {
                self.wave_channel.write_wave_ram(addr, value);
            }
            _ => {}
        }
    }

    pub fn wave_output(&self) -> i32 {
        self.wave_channel.output()
    }

    pub fn noise_output(&self) -> i32 {
        self.noise_channel.output()
    }
}

#[cfg(test)]
mod tests {
    use super::{Apu, FRAME_CYCLES, NoiseChannel, WaveChannel};

    #[test]
    fn new_apu_initializes_correctly() {
        let apu = Apu::new();
        assert_eq!(apu.samples_per_frame(), FRAME_CYCLES);
    }

    #[test]
    fn apu_step_does_not_crash() {
        let mut apu = Apu::new();
        assert!(!apu.step(10).is_err());
    }

    #[test]
    fn wave_channel_new_has_correct_defaults() {
        let channel = WaveChannel::new();
        assert!(!channel.enabled);
        assert_eq!(channel.volume_code, 0);
        assert_eq!(channel.frequency, 0);
    }

    #[test]
    fn wave_channel_read_write_nr30() {
        let mut channel = WaveChannel::new();
        assert_eq!(channel.read_io(0xFF1A), 0);
        channel.write_io(0xFF1A, 0x80);
        assert!(channel.enabled);
        assert_eq!(channel.read_io(0xFF1A), 0x80);
    }

    #[test]
    fn wave_channel_read_write_nr32() {
        let mut channel = WaveChannel::new();
        channel.write_io(0xFF1C, 0x60);
        assert_eq!(channel.read_io(0xFF1C), 0x60);
    }

    #[test]
    fn wave_channel_read_write_nr33_nr34() {
        let mut channel = WaveChannel::new();
        channel.write_io(0xFF1D, 0xAB);
        channel.write_io(0xFF1E, 0x80);
        assert_eq!(channel.read_io(0xFF1D), 0xAB);
        assert_eq!(channel.read_io(0xFF1E) & 0x07, 0x00);
    }

    #[test]
    fn wave_channel_trigger() {
        let mut channel = WaveChannel::new();
        channel.write_io(0xFF1D, 0x00);
        channel.write_io(0xFF1E, 0x80);
        assert!(channel.enabled);
    }

    #[test]
    fn wave_ram_read_write() {
        let mut channel = WaveChannel::new();
        channel.write_wave_ram(0xFF30, 0xAB);
        assert_eq!(channel.read_wave_ram(0xFF30), 0xAB);
        channel.write_wave_ram(0xFF3F, 0xCD);
        assert_eq!(channel.read_wave_ram(0xFF3F), 0xCD);
    }

    #[test]
    fn wave_channel_step_returns_zero_when_disabled() {
        let mut channel = WaveChannel::new();
        let output = channel.step(100);
        assert_eq!(output, 0);
    }

    #[test]
    fn wave_channel_step_with_frequency() {
        let mut channel = WaveChannel::new();
        channel.enabled = true;
        channel.frequency = 1024;
        channel.volume_code = 1;
        channel.wave_ram = [0xFF; 16];
        let _ = channel.step(100);
    }

    #[test]
    fn noise_channel_new_has_correct_defaults() {
        let channel = NoiseChannel::new();
        assert!(!channel.enabled);
        assert_eq!(channel.volume, 0);
        assert_eq!(channel.shift_clock_frequency, 0);
        assert_eq!(channel.divisor_code, 0);
    }

    #[test]
    fn noise_channel_read_write_nr41() {
        let mut channel = NoiseChannel::new();
        channel.write_io(0xFF20, 0x3F);
        assert_eq!(channel.read_io(0xFF20), 0x3F);
    }

    #[test]
    fn noise_channel_read_write_nr42() {
        let mut channel = NoiseChannel::new();
        channel.write_io(0xFF21, 0x80);
        assert_eq!(channel.volume, 0x08);
        assert!(!channel.envelope_add);
        assert_eq!(channel.envelope_period, 0x00);
        channel.write_io(0xFF21, 0xFF);
        assert_eq!(channel.volume, 0x0F);
        assert!(channel.envelope_add);
        assert_eq!(channel.envelope_period, 0x07);
    }

    #[test]
    fn noise_channel_read_write_nr43() {
        let mut channel = NoiseChannel::new();
        channel.write_io(0xFF22, 0x00);
        assert_eq!(channel.shift_clock_frequency, 0x00);
        assert!(!channel.seven_bit_mode);
        assert_eq!(channel.divisor_code, 0x00);
        channel.write_io(0xFF22, 0xF8);
        assert_eq!(channel.shift_clock_frequency, 0x0F);
        assert!(channel.seven_bit_mode);
        assert_eq!(channel.divisor_code, 0x00);
    }

    #[test]
    fn noise_channel_read_write_nr44() {
        let mut channel = NoiseChannel::new();
        channel.write_io(0xFF23, 0x40);
        assert!(channel.length_enable);
        assert_eq!(channel.read_io(0xFF23), 0x40);
    }

    #[test]
    fn noise_channel_trigger() {
        let mut channel = NoiseChannel::new();
        channel.write_io(0xFF21, 0x80);
        channel.write_io(0xFF22, 0x00);
        channel.write_io(0xFF23, 0x80);
        assert!(channel.enabled);
        assert_eq!(channel.lfsr, 0x7FFF);
    }

    #[test]
    fn noise_channel_step_returns_zero_when_disabled() {
        let mut channel = NoiseChannel::new();
        let output = channel.step(100);
        assert_eq!(output, 0);
    }

    #[test]
    fn noise_channel_step_with_trigger() {
        let mut channel = NoiseChannel::new();
        channel.write_io(0xFF21, 0x80);
        channel.write_io(0xFF22, 0x00);
        channel.write_io(0xFF23, 0x80);
        let _ = channel.step(100);
    }

    #[test]
    fn noise_channel_lfsr_15bit_mode() {
        let mut channel = NoiseChannel::new();
        channel.write_io(0xFF22, 0x00);
        channel.write_io(0xFF23, 0x80);
        channel.step(100000);
        let lfsr = channel.lfsr;
        assert_ne!(lfsr, 0x7FFF, "LFSR should have shifted at least once");
    }

    #[test]
    fn noise_channel_7bit_mode() {
        let mut channel = NoiseChannel::new();
        channel.write_io(0xFF22, 0x08);
        channel.write_io(0xFF23, 0x80);
        channel.step(100000);
        let lfsr = channel.lfsr;
        assert!(lfsr & 0xFF80 == 0, "LFSR should be 7-bit: {:016b}", lfsr);
    }
}
