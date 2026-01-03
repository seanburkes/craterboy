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

const FREQ_DIVISOR: u32 = 131072;

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
pub struct Apu {
    frame_cycles: u32,
    wave_channel: WaveChannel,
}

impl Apu {
    pub fn new() -> Self {
        Self {
            frame_cycles: FRAME_CYCLES,
            wave_channel: WaveChannel::new(),
        }
    }

    pub fn step(&mut self, cycles: u32) -> Result<(), ()> {
        self.frame_cycles = self.frame_cycles.wrapping_add(cycles);
        self.wave_channel.step(cycles);
        Ok(())
    }

    pub fn samples_per_frame(&self) -> u32 {
        self.frame_cycles
    }

    pub fn reset(&mut self) {
        self.frame_cycles = FRAME_CYCLES;
        self.wave_channel.reset();
    }

    pub fn read_io(&self, addr: u16) -> u8 {
        match addr {
            REG_NR30 | REG_NR31 | REG_NR32 | REG_NR33 | REG_NR34 => self.wave_channel.read_io(addr),
            WAVE_RAM_START..=0xFF3F => self.wave_channel.read_wave_ram(addr),
            _ => 0,
        }
    }

    pub fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            REG_NR30 | REG_NR31 | REG_NR32 | REG_NR33 | REG_NR34 => {
                self.wave_channel.write_io(addr, value);
            }
            WAVE_RAM_START..=0xFF3F => {
                self.wave_channel.write_wave_ram(addr, value);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Apu, FRAME_CYCLES, WaveChannel};

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
}
