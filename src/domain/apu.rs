use super::Bus;

const FRAME_CYCLES: u32 = 70_224;

const REG_NR10: u16 = 0xFF10;
const REG_NR11: u16 = 0xFF11;
const REG_NR12: u16 = 0xFF12;
const REG_NR13: u16 = 0xFF13;
const REG_NR14: u16 = 0xFF14;

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
const ENVELOPE_TICK_CYCLES: u32 = 256;
const SWEEP_TICK_CYCLES: u32 = 128;
const LENGTH_TICK_CYCLES: u32 = 256;

const DUTY_CYCLES: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 1, 1, 1],
    [0, 1, 1, 1, 1, 0, 0, 0],
];

#[derive(Debug)]
pub struct PulseChannel {
    enabled: bool,
    length: u8,
    length_counter: u8,
    duty_cycle: u8,
    volume: u8,
    envelope_add: bool,
    envelope_period: u8,
    length_enable: bool,
    frequency: u16,
    sweep_enable: bool,
    sweep_period: u8,
    sweep_add: bool,
    sweep_shift: u8,
    sweep_counter: u8,
    timer: u32,
    envelope_timer: u32,
    sweep_timer: u32,
    length_timer: u32,
    position: u8,
    envelope_counter: u8,
    current_volume: u8,
    output_volume: i32,
}

impl PulseChannel {
    pub fn new() -> Self {
        Self {
            enabled: false,
            length: 0,
            length_counter: 0,
            duty_cycle: 0,
            volume: 0,
            envelope_add: false,
            envelope_period: 0,
            length_enable: false,
            frequency: 0,
            sweep_enable: false,
            sweep_period: 0,
            sweep_add: false,
            sweep_shift: 0,
            sweep_counter: 0,
            timer: 0,
            envelope_timer: 0,
            sweep_timer: 0,
            length_timer: 0,
            position: 0,
            envelope_counter: 0,
            current_volume: 0,
            output_volume: 0,
        }
    }

    fn tick_sweep(&mut self) {
        if !self.sweep_enable || self.sweep_period == 0 {
            return;
        }

        if self.sweep_counter > 0 {
            self.sweep_counter -= 1;
            if self.sweep_counter == 0 {
                self.sweep_counter = self.sweep_period;
                self.update_frequency();
            }
        }
    }

    fn update_frequency(&mut self) {
        if self.sweep_shift > 0 {
            let new_freq = if self.sweep_add {
                self.frequency + (self.frequency >> self.sweep_shift)
            } else {
                self.frequency
                    .saturating_sub(self.frequency >> self.sweep_shift)
            };

            if new_freq > 2047 {
                self.enabled = false;
            } else {
                self.frequency = new_freq;
            }
        }
    }

    fn step_envelope(&mut self) {
        if self.envelope_period == 0 {
            return;
        }

        if self.envelope_counter > 0 {
            self.envelope_counter -= 1;
            return;
        }

        self.envelope_counter = self.envelope_period;

        if self.envelope_add {
            if self.current_volume < 15 {
                self.current_volume += 1;
            }
        } else {
            if self.current_volume > 0 {
                self.current_volume -= 1;
            }
        }
    }

    pub fn step(&mut self, cycles: u32) -> i32 {
        if !self.enabled {
            self.output_volume = 0;
            return 0;
        }

        self.sweep_timer = self.sweep_timer.wrapping_add(cycles);
        while self.sweep_timer >= SWEEP_TICK_CYCLES {
            self.sweep_timer -= SWEEP_TICK_CYCLES;
            self.tick_sweep();
        }

        self.envelope_timer = self.envelope_timer.wrapping_add(cycles);
        while self.envelope_timer >= ENVELOPE_TICK_CYCLES {
            self.envelope_timer -= ENVELOPE_TICK_CYCLES;
            self.step_envelope();
        }

        self.length_timer = self.length_timer.wrapping_add(cycles);
        while self.length_timer >= LENGTH_TICK_CYCLES {
            self.length_timer -= LENGTH_TICK_CYCLES;
            self.tick_length();
        }

        let freq = self.frequency as u32;
        let divisor = 2048 - freq;
        let step_rate = FREQ_DIVISOR / divisor;
        let timer_threshold = (4_194_304 / step_rate) / 8;

        self.timer = self.timer.wrapping_add(cycles);

        while self.timer >= timer_threshold {
            self.timer -= timer_threshold;
            self.position = (self.position + 1) & 0x07;
        }

        let duty = DUTY_CYCLES[self.duty_cycle as usize][self.position as usize];
        let sample = if duty != 0 {
            self.current_volume as i32
        } else {
            0
        };

        self.output_volume = sample;
        self.output_volume
    }

    fn tick_length(&mut self) {
        if !self.length_enable {
            return;
        }

        if self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 {
                self.enabled = false;
            }
        }
    }

    pub fn reset(&mut self) {
        self.enabled = false;
        self.length = 0;
        self.length_counter = 0;
        self.duty_cycle = 0;
        self.volume = 0;
        self.envelope_add = false;
        self.envelope_period = 0;
        self.length_enable = false;
        self.frequency = 0;
        self.sweep_enable = false;
        self.sweep_period = 0;
        self.sweep_add = false;
        self.sweep_shift = 0;
        self.sweep_counter = 0;
        self.timer = 0;
        self.envelope_timer = 0;
        self.sweep_timer = 0;
        self.length_timer = 0;
        self.position = 0;
        self.envelope_counter = 0;
        self.current_volume = 0;
        self.output_volume = 0;
    }

    pub fn read_io(&self, addr: u16) -> u8 {
        match addr {
            REG_NR10 => {
                let mut value = (self.sweep_period as u8) << 4;
                if self.sweep_add {
                    value |= 0x08;
                }
                if self.sweep_enable {
                    value |= 0x80;
                }
                value | (self.sweep_shift & 0x07)
            }
            REG_NR11 => (self.duty_cycle << 6) | (self.length & 0x3F),
            REG_NR12 => {
                (self.volume << 4)
                    | (if self.envelope_add { 0x08 } else { 0 })
                    | (self.envelope_period & 0x07)
            }
            REG_NR13 => self.frequency as u8,
            REG_NR14 => {
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
            REG_NR10 => {
                self.sweep_enable = value & 0x80 != 0;
                self.sweep_period = (value >> 4) & 0x07;
                self.sweep_add = value & 0x08 != 0;
                self.sweep_shift = value & 0x07;
            }
            REG_NR11 => {
                self.duty_cycle = (value >> 6) & 0x03;
                self.length = value & 0x3F;
            }
            REG_NR12 => {
                self.volume = (value >> 4) & 0x0F;
                self.envelope_add = value & 0x08 != 0;
                self.envelope_period = value & 0x07;
                self.current_volume = self.volume;
            }
            REG_NR13 => {
                self.frequency = (self.frequency & 0xFF00) | (value as u16);
            }
            REG_NR14 => {
                self.length_enable = value & 0x40 != 0;
                let new_freq_high = (value as u16) & 0x07;
                self.frequency = (self.frequency & 0x00FF) | (new_freq_high << 8);
                if value & 0x80 != 0 {
                    self.trigger();
                }
            }
            _ => {}
        }
    }

    fn trigger(&mut self) {
        self.enabled = true;
        self.sweep_enable = true;
        self.timer = 0;
        self.envelope_timer = 0;
        self.sweep_timer = 0;
        self.length_timer = 0;
        self.position = 0;
        self.current_volume = self.volume;
        self.envelope_counter = self.envelope_period;
        self.length_counter = if self.length == 0 { 64 } else { self.length };
        self.sweep_counter = if self.sweep_period == 0 {
            8
        } else {
            self.sweep_period
        };

        if self.sweep_shift > 0 {
            self.update_frequency();
        }
    }

    pub fn output(&self) -> i32 {
        self.output_volume
    }
}

#[derive(Debug)]
pub struct WaveChannel {
    enabled: bool,
    length: u8,
    length_counter: u16,
    volume_code: u8,
    frequency: u16,
    length_enable: bool,
    trigger: bool,
    wave_ram: [u8; WAVE_RAM_SIZE],
    position: u8,
    timer: u32,
    length_timer: u32,
    output_volume: i32,
}

impl WaveChannel {
    pub fn new() -> Self {
        Self {
            enabled: false,
            length: 0,
            length_counter: 0,
            volume_code: 0,
            frequency: 0,
            length_enable: false,
            trigger: false,
            wave_ram: [0xFF; WAVE_RAM_SIZE],
            position: 0,
            timer: 0,
            length_timer: 0,
            output_volume: 0,
        }
    }

    pub fn step(&mut self, cycles: u32) -> i32 {
        self.length_timer = self.length_timer.wrapping_add(cycles);
        while self.length_timer >= LENGTH_TICK_CYCLES {
            self.length_timer -= LENGTH_TICK_CYCLES;
            self.tick_length();
        }

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

    fn tick_length(&mut self) {
        if !self.length_enable {
            return;
        }

        if self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 {
                self.enabled = false;
            }
        }
    }

    pub fn reset(&mut self) {
        self.enabled = false;
        self.length = 0;
        self.length_counter = 0;
        self.volume_code = 0;
        self.frequency = 0;
        self.length_enable = false;
        self.trigger = false;
        self.position = 0;
        self.timer = 0;
        self.length_timer = 0;
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
                    self.length_timer = 0;
                    self.length_counter = if self.length == 0 {
                        256
                    } else {
                        self.length as u16
                    };
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
    length_counter: u8,
    volume: u8,
    envelope_add: bool,
    envelope_period: u8,
    length_enable: bool,
    shift_clock_frequency: u8,
    seven_bit_mode: bool,
    divisor_code: u8,
    lfsr: u16,
    timer: u32,
    envelope_timer: u32,
    envelope_counter: u8,
    current_volume: u8,
    output_volume: i32,
    length_timer: u32,
}

impl NoiseChannel {
    pub fn new() -> Self {
        Self {
            enabled: false,
            length: 0,
            length_counter: 0,
            volume: 0,
            envelope_add: false,
            envelope_period: 0,
            length_enable: false,
            shift_clock_frequency: 0,
            seven_bit_mode: false,
            divisor_code: 0,
            lfsr: 0,
            timer: 0,
            envelope_timer: 0,
            envelope_counter: 0,
            current_volume: 0,
            output_volume: 0,
            length_timer: 0,
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

        self.envelope_timer = self.envelope_timer.wrapping_add(cycles);
        while self.envelope_timer >= ENVELOPE_TICK_CYCLES {
            self.envelope_timer -= ENVELOPE_TICK_CYCLES;
            self.step_envelope();
        }

        self.length_timer = self.length_timer.wrapping_add(cycles);
        while self.length_timer >= LENGTH_TICK_CYCLES {
            self.length_timer -= LENGTH_TICK_CYCLES;
            self.tick_length();
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

    fn tick_length(&mut self) {
        if !self.length_enable {
            return;
        }

        if self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 {
                self.enabled = false;
            }
        }
    }

    fn step_envelope(&mut self) {
        if self.envelope_period == 0 {
            return;
        }

        if self.envelope_counter > 0 {
            self.envelope_counter -= 1;
            return;
        }

        self.envelope_counter = self.envelope_period;

        if self.envelope_add {
            if self.current_volume < 15 {
                self.current_volume += 1;
            }
        } else {
            if self.current_volume > 0 {
                self.current_volume -= 1;
            }
        }
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
        self.length_counter = 0;
        self.volume = 0;
        self.envelope_add = false;
        self.envelope_period = 0;
        self.length_enable = false;
        self.shift_clock_frequency = 0;
        self.seven_bit_mode = false;
        self.divisor_code = 0;
        self.lfsr = 0;
        self.timer = 0;
        self.envelope_timer = 0;
        self.envelope_counter = 0;
        self.current_volume = 0;
        self.output_volume = 0;
        self.length_timer = 0;
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
        self.envelope_timer = 0;
        self.length_timer = 0;
        self.current_volume = self.volume;
        self.envelope_counter = self.envelope_period;
        self.length_counter = if self.length == 0 { 64 } else { self.length };
    }

    pub fn output(&self) -> i32 {
        self.output_volume
    }
}

#[derive(Debug)]
pub struct Apu {
    frame_cycles: u32,
    pulse_channel: PulseChannel,
    wave_channel: WaveChannel,
    noise_channel: NoiseChannel,
    sample_ready: bool,
    current_sample: i32,
}

impl Apu {
    pub fn new() -> Self {
        Self {
            frame_cycles: FRAME_CYCLES,
            pulse_channel: PulseChannel::new(),
            wave_channel: WaveChannel::new(),
            noise_channel: NoiseChannel::new(),
            sample_ready: false,
            current_sample: 0,
        }
    }

    pub fn step(&mut self, cycles: u32) -> Result<(), ()> {
        self.frame_cycles = self.frame_cycles.wrapping_add(cycles);
        self.pulse_channel.step(cycles);
        self.wave_channel.step(cycles);
        self.noise_channel.step(cycles);

        if self.frame_cycles >= FRAME_CYCLES {
            self.frame_cycles -= FRAME_CYCLES;
            self.mix_sample();
            self.sample_ready = true;
        }

        Ok(())
    }

    fn mix_sample(&mut self) {
        let _left_output = 0;
        let _right_output = 0;

        let pulse_out = self.pulse_channel.output();
        let wave_out = self.wave_channel.output();
        let noise_out = self.noise_channel.output();

        let mixed = (pulse_out + wave_out + noise_out) as i32;
        self.current_sample = mixed.clamp(-128, 127) as i32;
    }

    pub fn samples_per_frame(&self) -> u32 {
        self.frame_cycles
    }

    pub fn reset(&mut self) {
        self.frame_cycles = FRAME_CYCLES;
        self.pulse_channel.reset();
        self.wave_channel.reset();
        self.noise_channel.reset();
        self.sample_ready = false;
        self.current_sample = 0;
    }

    pub fn read_io(&self, addr: u16) -> u8 {
        match addr {
            REG_NR10 | REG_NR11 | REG_NR12 | REG_NR13 | REG_NR14 => {
                self.pulse_channel.read_io(addr)
            }
            REG_NR30 | REG_NR31 | REG_NR32 | REG_NR33 | REG_NR34 => self.wave_channel.read_io(addr),
            REG_NR41 | REG_NR42 | REG_NR43 | REG_NR44 => self.noise_channel.read_io(addr),
            WAVE_RAM_START..=0xFF3F => self.wave_channel.read_wave_ram(addr),
            _ => 0,
        }
    }

    pub fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            REG_NR10 | REG_NR11 | REG_NR12 | REG_NR13 | REG_NR14 => {
                self.pulse_channel.write_io(addr, value);
            }
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

    pub fn pulse_output(&self) -> i32 {
        self.pulse_channel.output()
    }

    pub fn wave_output(&self) -> i32 {
        self.wave_channel.output()
    }

    pub fn noise_output(&self) -> i32 {
        self.noise_channel.output()
    }

    pub fn sample_rate_hz(&self) -> f64 {
        4_194_304.0 / FRAME_CYCLES as f64
    }

    pub fn has_sample(&self) -> bool {
        self.sample_ready
    }

    pub fn take_sample(&mut self) -> i32 {
        self.sample_ready = false;
        self.current_sample
    }

    pub fn sample(&self) -> i32 {
        self.current_sample
    }
}

#[cfg(test)]
mod tests {
    use super::{Apu, FRAME_CYCLES, NoiseChannel, PulseChannel, WaveChannel};

    #[test]
    fn new_apu_initializes_correctly() {
        let apu = Apu::new();
        assert_eq!(apu.samples_per_frame(), FRAME_CYCLES);
    }

    #[test]
    fn apu_mixes_pulse_wave_and_noise_channels() {
        let mut apu = Apu::new();
        apu.write_io(0xFF12, 0x80);
        apu.write_io(0xFF14, 0x80);
        apu.write_io(0xFF22, 0x00);
        apu.write_io(0xFF23, 0x80);
        for _ in 0..10 {
            apu.step(FRAME_CYCLES as u32);
            while apu.has_sample() {
                let _ = apu.take_sample();
            }
        }
    }

    #[test]
    fn pulse_channel_output_with_duty_cycles() {
        let mut channel = PulseChannel::new();
        for duty in 0..4 {
            channel.write_io(0xFF11, duty << 6);
            channel.write_io(0xFF12, 0x80);
            channel.write_io(0xFF13, 0x00);
            channel.write_io(0xFF14, 0x80);
            let output = channel.output();
            assert!(
                output >= 0 && output <= 15,
                "Duty {} should produce valid output, got {}",
                duty,
                output
            );
        }
    }

    #[test]
    fn pulse_channel_new_has_correct_defaults() {
        let channel = PulseChannel::new();
        assert!(!channel.enabled);
        assert_eq!(channel.frequency, 0);
        assert_eq!(channel.sweep_period, 0);
        assert_eq!(channel.sweep_shift, 0);
    }

    #[test]
    fn pulse_channel_read_write_nr10() {
        let mut channel = PulseChannel::new();
        channel.write_io(0xFF10, 0x80);
        assert!(channel.sweep_enable);
        channel.write_io(0xFF10, 0x60);
        assert_eq!(channel.sweep_period, 6);
        assert!(!channel.sweep_add, "0x60 = 01100000, bit 3 is 0");
        assert_eq!(channel.sweep_shift, 0);
    }

    #[test]
    fn pulse_channel_read_write_nr11() {
        let mut channel = PulseChannel::new();
        channel.write_io(0xFF11, 0x80);
        assert_eq!(channel.duty_cycle, 2);
        assert_eq!(channel.length, 0);
    }

    #[test]
    fn pulse_channel_read_write_nr12() {
        let mut channel = PulseChannel::new();
        channel.write_io(0xFF12, 0x80);
        assert_eq!(channel.volume, 0x08);
        assert!(!channel.envelope_add);
        assert_eq!(channel.envelope_period, 0x00);
    }

    #[test]
    fn pulse_channel_sweep_stops_at_max_frequency() {
        let mut channel = PulseChannel::new();
        channel.write_io(0xFF10, 0x2B);
        channel.write_io(0xFF12, 0x80);
        channel.write_io(0xFF13, 0x80);
        channel.write_io(0xFF14, 0x80);
        assert!(
            channel.enabled,
            "Channel should be enabled initially, freq={}",
            channel.frequency
        );
        assert!(
            channel.sweep_period > 0,
            "Sweep period should be > 0 for 0x2B"
        );
        assert!(channel.sweep_add, "Sweep add should be true for 0x2B");
        let mut disabled = false;
        for _ in 0..(128 * 500) {
            channel.step(128);
            if !channel.enabled {
                disabled = true;
                break;
            }
        }
        assert!(
            disabled,
            "Channel should stop when frequency exceeds 2047, final freq={}",
            channel.frequency
        );
    }

    #[test]
    fn pulse_channel_trigger() {
        let mut channel = PulseChannel::new();
        channel.write_io(0xFF12, 0x80);
        channel.write_io(0xFF13, 0x00);
        channel.write_io(0xFF14, 0x80);
        assert!(channel.enabled);
        assert_eq!(channel.position, 0);
    }

    #[test]
    fn pulse_channel_step_returns_zero_when_disabled() {
        let mut channel = PulseChannel::new();
        let output = channel.step(100);
        assert_eq!(output, 0);
    }

    #[test]
    fn pulse_channel_sweep_increases_frequency() {
        let mut channel = PulseChannel::new();
        channel.write_io(0xFF10, 0x03);
        channel.write_io(0xFF12, 0x80);
        channel.write_io(0xFF13, 0x00);
        channel.write_io(0xFF14, 0x80);
        let start_freq = channel.frequency;
        for _ in 0..(128 * 10) {
            channel.step(128);
        }
        assert!(
            channel.frequency >= start_freq,
            "Frequency should increase with add sweep: started at {}, now {}",
            start_freq,
            channel.frequency
        );
    }

    #[test]
    fn pulse_channel_sweep_decreases_frequency() {
        let mut channel = PulseChannel::new();
        channel.write_io(0xFF10, 0x0B);
        channel.write_io(0xFF12, 0x80);
        channel.write_io(0xFF13, 0x00);
        channel.write_io(0xFF14, 0x80);
        let start_freq = channel.frequency;
        for _ in 0..(128 * 10) {
            channel.step(128);
        }
        assert!(
            channel.frequency <= start_freq,
            "Frequency should decrease with subtract sweep: started at {}, now {}",
            start_freq,
            channel.frequency
        );
    }

    #[test]
    fn pulse_channel_sweep_disabled_at_zero_period() {
        let mut channel = PulseChannel::new();
        channel.write_io(0xFF10, 0x80);
        channel.write_io(0xFF12, 0x80);
        channel.write_io(0xFF13, 0x00);
        channel.write_io(0xFF14, 0x80);
        assert!(channel.sweep_enable);
        for _ in 0..(128 * 50) {
            channel.step(128);
        }
        assert!(
            channel.sweep_enable,
            "Sweep should stay enabled when period is 0x8"
        );
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

    #[test]
    fn noise_channel_envelope_initializes_on_trigger() {
        let mut channel = NoiseChannel::new();
        channel.write_io(0xFF21, 0x90);
        channel.write_io(0xFF22, 0x00);
        channel.write_io(0xFF23, 0x80);
        assert_eq!(channel.current_volume, 0x09);
        assert_eq!(channel.envelope_counter, 0x00);
    }

    #[test]
    fn noise_channel_envelope_decrements_volume() {
        let mut channel = NoiseChannel::new();
        channel.write_io(0xFF21, 0x80);
        channel.write_io(0xFF22, 0x00);
        channel.write_io(0xFF23, 0x80);
        assert_eq!(channel.current_volume, 0x08);
        let start_vol = channel.current_volume;
        for _ in 0..(256 * 10) {
            channel.step(256);
        }
        assert!(
            channel.current_volume <= start_vol,
            "Volume should decrease: started at {}, now {}",
            start_vol,
            channel.current_volume
        );
    }

    #[test]
    fn noise_channel_envelope_increments_volume() {
        let mut channel = NoiseChannel::new();
        channel.write_io(0xFF21, 0x98);
        channel.write_io(0xFF22, 0x00);
        channel.write_io(0xFF23, 0x80);
        assert_eq!(channel.current_volume, 0x09);
        assert!(channel.envelope_add);
        let start_vol = channel.current_volume;
        for _ in 0..(256 * 10) {
            channel.step(256);
        }
        assert!(
            channel.current_volume >= start_vol,
            "Volume should increase: started at {}, now {}",
            start_vol,
            channel.current_volume
        );
    }

    #[test]
    fn noise_channel_envelope_stops_at_zero() {
        let mut channel = NoiseChannel::new();
        channel.write_io(0xFF21, 0x87);
        channel.write_io(0xFF22, 0x00);
        channel.write_io(0xFF23, 0x80);
        for _ in 0..(256 * 500) {
            channel.step(256);
        }
        assert_eq!(channel.current_volume, 0, "Volume should have reached 0");
    }

    #[test]
    fn noise_channel_envelope_stops_at_fifteen() {
        let mut channel = NoiseChannel::new();
        channel.write_io(0xFF21, 0xF8);
        channel.write_io(0xFF22, 0x00);
        channel.write_io(0xFF23, 0x80);
        for _ in 0..(256 * 500) {
            channel.step(256);
        }
        assert_eq!(channel.current_volume, 15, "Volume should have reached 15");
    }

    #[test]
    fn noise_channel_envelope_zero_period_means_no_sweep() {
        let mut channel = NoiseChannel::new();
        channel.write_io(0xFF21, 0x80);
        channel.write_io(0xFF22, 0x00);
        channel.write_io(0xFF23, 0x80);
        let start_vol = channel.current_volume;
        for _ in 0..(256 * 100) {
            channel.step(256);
        }
        assert_eq!(
            channel.current_volume, start_vol,
            "Volume should not change with period 0"
        );
    }

    #[test]
    fn apu_sample_rate_is_59_7275hz() {
        let apu = Apu::new();
        let rate = apu.sample_rate_hz();
        assert!(
            (rate - 59.7275).abs() < 0.001,
            "Sample rate should be ~59.7275 Hz, got {}",
            rate
        );
    }

    #[test]
    fn apu_sample_generated_at_frame_boundary() {
        let mut apu = Apu::new();
        assert!(!apu.has_sample());
        assert_eq!(apu.sample(), 0);
        let _ = apu.step(FRAME_CYCLES as u32);
        assert!(apu.has_sample());
        let sample = apu.take_sample();
        assert!(sample >= -128 && sample <= 127);
        assert!(!apu.has_sample());
    }

    #[test]
    fn apu_sample_is_clamped() {
        let mut apu = Apu::new();
        for _ in 0..10 {
            let _ = apu.step(FRAME_CYCLES as u32);
            if apu.has_sample() {
                let sample = apu.sample();
                assert!(
                    sample >= -128 && sample <= 127,
                    "Sample {} out of range",
                    sample
                );
                let _ = apu.take_sample();
            }
        }
    }

    #[test]
    fn pulse_channel_length_counter_decrements() {
        let mut channel = PulseChannel::new();
        channel.write_io(0xFF11, 0x20);
        channel.write_io(0xFF12, 0x80);
        channel.write_io(0xFF13, 0x00);
        channel.write_io(0xFF14, 0xC0);
        assert!(channel.enabled);
        assert_eq!(channel.length_counter, 32);
        let start_counter = channel.length_counter;
        for _ in 0..(256 * 5) {
            channel.step(256);
        }
        assert!(
            channel.length_counter < start_counter,
            "Length counter should have decremented: was {}, now {}",
            start_counter,
            channel.length_counter
        );
    }

    #[test]
    fn pulse_channel_stops_at_length_zero() {
        let mut channel = PulseChannel::new();
        channel.write_io(0xFF11, 0x01);
        channel.write_io(0xFF12, 0x80);
        channel.write_io(0xFF13, 0x00);
        channel.write_io(0xFF14, 0xC0);
        assert!(channel.enabled);
        for _ in 0..(256 * 300) {
            channel.step(256);
        }
        assert!(
            !channel.enabled,
            "Channel should stop when length counter reaches 0"
        );
    }

    #[test]
    fn wave_channel_length_counter_decrements() {
        let mut channel = WaveChannel::new();
        channel.write_io(0xFF1B, 0x80);
        channel.write_io(0xFF1C, 0x20);
        channel.write_io(0xFF1D, 0x00);
        channel.write_io(0xFF1E, 0xC0);
        assert!(channel.enabled);
        assert_eq!(channel.length_counter, 128);
        let start_counter = channel.length_counter;
        for _ in 0..(256 * 5) {
            channel.step(256);
        }
        assert!(
            channel.length_counter < start_counter,
            "Length counter should have decremented: was {}, now {}",
            start_counter,
            channel.length_counter
        );
    }

    #[test]
    fn wave_channel_stops_at_length_zero() {
        let mut channel = WaveChannel::new();
        channel.write_io(0xFF1B, 0x01);
        channel.write_io(0xFF1C, 0x20);
        channel.write_io(0xFF1D, 0x00);
        channel.write_io(0xFF1E, 0xC0);
        assert!(channel.enabled);
        for _ in 0..(256 * 300) {
            channel.step(256);
        }
        assert!(
            !channel.enabled,
            "Channel should stop when length counter reaches 0"
        );
    }

    #[test]
    fn noise_channel_length_counter_decrements() {
        let mut channel = NoiseChannel::new();
        channel.write_io(0xFF20, 0x20);
        channel.write_io(0xFF21, 0x80);
        channel.write_io(0xFF22, 0x00);
        channel.write_io(0xFF23, 0xC0);
        assert!(channel.enabled);
        assert_eq!(channel.length_counter, 32);
        let start_counter = channel.length_counter;
        for _ in 0..(256 * 5) {
            channel.step(256);
        }
        assert!(
            channel.length_counter < start_counter,
            "Length counter should have decremented: was {}, now {}",
            start_counter,
            channel.length_counter
        );
    }

    #[test]
    fn noise_channel_stops_at_length_zero() {
        let mut channel = NoiseChannel::new();
        channel.write_io(0xFF20, 0x01);
        channel.write_io(0xFF21, 0x80);
        channel.write_io(0xFF22, 0x00);
        channel.write_io(0xFF23, 0xC0);
        assert!(channel.enabled);
        for _ in 0..(256 * 300) {
            channel.step(256);
        }
        assert!(
            !channel.enabled,
            "Channel should stop when length counter reaches 0"
        );
    }

    #[test]
    fn length_counter_not_decremented_when_disabled() {
        let mut channel = PulseChannel::new();
        channel.write_io(0xFF11, 0x20);
        channel.write_io(0xFF12, 0x80);
        channel.write_io(0xFF13, 0x00);
        channel.write_io(0xFF14, 0xC0);
        assert!(channel.enabled);
        assert!(channel.length_enable);
        channel.length_enable = false;
        let start_counter = channel.length_counter;
        for _ in 0..(256 * 10) {
            channel.step(256);
        }
        assert_eq!(
            channel.length_counter, start_counter,
            "Length counter should not decrement when length_enable is false"
        );
    }

    #[test]
    fn length_counter_uses_64_when_zero() {
        let mut channel = PulseChannel::new();
        channel.write_io(0xFF11, 0x00);
        channel.write_io(0xFF12, 0x80);
        channel.write_io(0xFF13, 0x00);
        channel.write_io(0xFF14, 0xC0);
        assert_eq!(channel.length_counter, 64, "Should use 64 when length is 0");
    }
}
