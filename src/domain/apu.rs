use std::collections::VecDeque;

const CPU_HZ: f64 = 4_194_304.0;
const FRAME_CYCLES: u32 = 70_224;
pub(crate) const OUTPUT_SAMPLE_RATE_HZ: f64 = 48_000.0;
const CYCLES_PER_SAMPLE: f64 = CPU_HZ / OUTPUT_SAMPLE_RATE_HZ;
const MAX_SAMPLE_QUEUE: usize = 8_192;

const REG_NR10: u16 = 0xFF10;
const REG_NR11: u16 = 0xFF11;
const REG_NR12: u16 = 0xFF12;
const REG_NR13: u16 = 0xFF13;
const REG_NR14: u16 = 0xFF14;

const REG_NR21: u16 = 0xFF16;
const REG_NR22: u16 = 0xFF17;
const REG_NR23: u16 = 0xFF18;
const REG_NR24: u16 = 0xFF19;

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

const REG_NR50: u16 = 0xFF24;
const REG_NR51: u16 = 0xFF25;
const REG_NR52: u16 = 0xFF26;

const FREQ_DIVISOR: u32 = 131072;
const NOISE_CLOCK_BASE: u32 = 524288;
const FRAME_SEQUENCER_CYCLES: u32 = 8192;

const DUTY_CYCLES: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 1, 1, 1],
    [0, 1, 1, 1, 1, 0, 0, 0],
];

#[derive(Debug)]
pub struct PulseChannel {
    enabled: bool,
    has_sweep: bool,
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
    position: u8,
    envelope_counter: u8,
    current_volume: u8,
    output_volume: i32,
}

impl PulseChannel {
    pub fn new() -> Self {
        Self::new_with_sweep(true)
    }

    pub fn new_no_sweep() -> Self {
        Self::new_with_sweep(false)
    }

    fn new_with_sweep(has_sweep: bool) -> Self {
        Self {
            enabled: false,
            has_sweep,
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
            position: 0,
            envelope_counter: 0,
            current_volume: 0,
            output_volume: 0,
        }
    }

    pub fn tick_sweep(&mut self) {
        if !self.has_sweep {
            return;
        }

        if !self.sweep_enable {
            return;
        }

        if self.sweep_period == 0 {
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
        if self.sweep_shift == 0 {
            return;
        }

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

    pub fn tick_envelope(&mut self) {
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

    pub fn tick_length(&mut self) {
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

    pub fn step(&mut self, cycles: u32) -> i32 {
        if !self.enabled {
            self.output_volume = 0;
            return 0;
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
        self.position = 0;
        self.envelope_counter = 0;
        self.current_volume = 0;
        self.output_volume = 0;
    }

    fn read_duty_length(&self) -> u8 {
        (self.duty_cycle << 6) | (self.length & 0x3F)
    }

    fn read_envelope(&self) -> u8 {
        (self.volume << 4)
            | (if self.envelope_add { 0x08 } else { 0 })
            | (self.envelope_period & 0x07)
    }

    fn read_frequency_low(&self) -> u8 {
        self.frequency as u8
    }

    fn read_frequency_high(&self) -> u8 {
        let mut value = 0;
        if self.length_enable {
            value |= 0x40;
        }
        value | ((self.frequency >> 8) as u8) & 0x07
    }

    fn write_duty_length(&mut self, value: u8) {
        self.duty_cycle = (value >> 6) & 0x03;
        self.length = value & 0x3F;
    }

    fn write_envelope(&mut self, value: u8) {
        self.volume = (value >> 4) & 0x0F;
        self.envelope_add = value & 0x08 != 0;
        self.envelope_period = value & 0x07;
        self.current_volume = self.volume;
    }

    fn write_frequency_low(&mut self, value: u8) {
        self.frequency = (self.frequency & 0xFF00) | (value as u16);
    }

    fn write_frequency_high(&mut self, value: u8) {
        self.length_enable = value & 0x40 != 0;
        let new_freq_high = (value as u16) & 0x07;
        self.frequency = (self.frequency & 0x00FF) | (new_freq_high << 8);
        if value & 0x80 != 0 {
            self.trigger();
        }
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
            REG_NR11 => self.read_duty_length(),
            REG_NR12 => self.read_envelope(),
            REG_NR13 => self.read_frequency_low(),
            REG_NR14 => self.read_frequency_high(),
            _ => 0,
        }
    }

    pub fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            REG_NR10 => {
                if !self.has_sweep {
                    return;
                }
                self.sweep_enable = value & 0x80 != 0;
                self.sweep_period = (value >> 4) & 0x07;
                self.sweep_add = value & 0x08 != 0;
                self.sweep_shift = value & 0x07;
            }
            REG_NR11 => self.write_duty_length(value),
            REG_NR12 => self.write_envelope(value),
            REG_NR13 => self.write_frequency_low(value),
            REG_NR14 => self.write_frequency_high(value),
            _ => {}
        }
    }

    fn trigger(&mut self) {
        self.enabled = true;
        self.sweep_enable = true;
        self.timer = 0;
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
        let timer_threshold = divisor * 2;

        self.timer = self.timer.wrapping_add(cycles);

        while self.timer >= timer_threshold {
            self.timer -= timer_threshold;
            self.position = (self.position + 1) & 0x1F;
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

    pub fn tick_length(&mut self) {
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
    envelope_counter: u8,
    current_volume: u8,
    output_volume: i32,
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

    pub fn tick_envelope(&mut self) {
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

    pub fn tick_length(&mut self) {
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
        self.length_counter = if self.length == 0 { 64 } else { self.length };
    }

    pub fn output(&self) -> i32 {
        self.output_volume
    }
}

#[derive(Debug)]
pub struct Apu {
    frame_sequencer_cycles: u32,
    frame_sequencer_step: u8,
    pulse_channel: PulseChannel,
    pulse_channel2: PulseChannel,
    wave_channel: WaveChannel,
    noise_channel: NoiseChannel,
    sample_cycle_accumulator: f64,
    samples: VecDeque<[i32; 2]>,
    current_sample: i32,
    current_sample_left: i32,
    current_sample_right: i32,
    master_volume_left: u8,
    master_volume_right: u8,
    nr51: u8,
    sound_enabled: bool,
}

impl Apu {
    pub fn new() -> Self {
        Self {
            frame_sequencer_cycles: 0,
            frame_sequencer_step: 0,
            pulse_channel: PulseChannel::new(),
            pulse_channel2: PulseChannel::new_no_sweep(),
            wave_channel: WaveChannel::new(),
            noise_channel: NoiseChannel::new(),
            sample_cycle_accumulator: 0.0,
            samples: VecDeque::new(),
            current_sample: 0,
            current_sample_left: 0,
            current_sample_right: 0,
            master_volume_left: 0,
            master_volume_right: 0,
            nr51: 0,
            sound_enabled: false,
        }
    }

    pub fn step(&mut self, cycles: u32) -> Result<(), ()> {
        self.frame_sequencer_cycles = self.frame_sequencer_cycles.wrapping_add(cycles);
        while self.frame_sequencer_cycles >= FRAME_SEQUENCER_CYCLES {
            self.frame_sequencer_cycles -= FRAME_SEQUENCER_CYCLES;
            self.step_frame_sequencer();
        }

        self.pulse_channel.step(cycles);
        self.pulse_channel2.step(cycles);
        self.wave_channel.step(cycles);
        self.noise_channel.step(cycles);

        self.sample_cycle_accumulator += cycles as f64;
        while self.sample_cycle_accumulator >= CYCLES_PER_SAMPLE {
            self.sample_cycle_accumulator -= CYCLES_PER_SAMPLE;
            self.mix_sample();
            if self.samples.len() >= MAX_SAMPLE_QUEUE {
                self.samples.pop_front();
            }
            self.samples
                .push_back([self.current_sample_left, self.current_sample_right]);
        }

        Ok(())
    }

    fn step_frame_sequencer(&mut self) {
        match self.frame_sequencer_step {
            0 => {
                self.pulse_channel.tick_length();
                self.pulse_channel2.tick_length();
                self.wave_channel.tick_length();
                self.noise_channel.tick_length();
            }
            1 => {
                self.pulse_channel.tick_sweep();
                self.pulse_channel.tick_length();
                self.pulse_channel2.tick_length();
                self.wave_channel.tick_length();
                self.noise_channel.tick_length();
            }
            2 => {
                self.pulse_channel.tick_length();
                self.pulse_channel2.tick_length();
                self.wave_channel.tick_length();
                self.noise_channel.tick_length();
            }
            3 => {
                self.pulse_channel.tick_envelope();
                self.pulse_channel2.tick_envelope();
                self.pulse_channel.tick_length();
                self.pulse_channel2.tick_length();
                self.wave_channel.tick_length();
                self.noise_channel.tick_length();
            }
            4 => {
                self.pulse_channel.tick_length();
                self.pulse_channel2.tick_length();
                self.wave_channel.tick_length();
                self.noise_channel.tick_length();
            }
            5 => {
                self.pulse_channel.tick_sweep();
                self.pulse_channel.tick_length();
                self.pulse_channel2.tick_length();
                self.wave_channel.tick_length();
                self.noise_channel.tick_length();
            }
            6 => {
                self.pulse_channel.tick_length();
                self.pulse_channel2.tick_length();
                self.wave_channel.tick_length();
                self.noise_channel.tick_length();
            }
            7 => {
                self.pulse_channel.tick_envelope();
                self.pulse_channel2.tick_envelope();
                self.pulse_channel.tick_length();
                self.pulse_channel2.tick_length();
                self.wave_channel.tick_length();
                self.noise_channel.tick_length();
            }
            _ => {}
        }

        self.frame_sequencer_step = (self.frame_sequencer_step + 1) & 0x07;
    }

    fn mix_sample(&mut self) {
        if !self.sound_enabled {
            self.current_sample = 0;
            self.current_sample_left = 0;
            self.current_sample_right = 0;
            return;
        }

        let pulse1_out = self.pulse_channel.output();
        let pulse2_out = self.pulse_channel2.output();
        let wave_out = self.wave_channel.output();
        let noise_out = self.noise_channel.output();

        let mut left = 0;
        let mut right = 0;

        if self.nr51 & 0x10 != 0 {
            left += pulse1_out;
        }
        if self.nr51 & 0x20 != 0 {
            left += pulse2_out;
        }
        if self.nr51 & 0x40 != 0 {
            left += wave_out;
        }
        if self.nr51 & 0x80 != 0 {
            left += noise_out;
        }

        if self.nr51 & 0x01 != 0 {
            right += pulse1_out;
        }
        if self.nr51 & 0x02 != 0 {
            right += pulse2_out;
        }
        if self.nr51 & 0x04 != 0 {
            right += wave_out;
        }
        if self.nr51 & 0x08 != 0 {
            right += noise_out;
        }

        let left_scaled = left * (self.master_volume_left as i32 + 1);
        let right_scaled = right * (self.master_volume_right as i32 + 1);
        self.current_sample_left = (left_scaled / 8).clamp(-128, 127);
        self.current_sample_right = (right_scaled / 8).clamp(-128, 127);
        self.current_sample =
            ((self.current_sample_left + self.current_sample_right) / 2).clamp(-128, 127);
    }

    pub fn samples_per_frame(&self) -> u32 {
        let frame_rate = CPU_HZ / FRAME_CYCLES as f64;
        (OUTPUT_SAMPLE_RATE_HZ / frame_rate).round() as u32
    }

    fn reset_state(&mut self) {
        self.frame_sequencer_cycles = 0;
        self.frame_sequencer_step = 0;
        self.pulse_channel.reset();
        self.pulse_channel2.reset();
        self.wave_channel.reset();
        self.noise_channel.reset();
        self.sample_cycle_accumulator = 0.0;
        self.samples.clear();
        self.current_sample = 0;
        self.current_sample_left = 0;
        self.current_sample_right = 0;
        self.master_volume_left = 0;
        self.master_volume_right = 0;
        self.nr51 = 0;
    }

    pub fn reset(&mut self) {
        self.reset_state();
        self.sound_enabled = false;
    }

    pub fn apply_post_boot_state(&mut self) {
        self.reset_state();
        self.sound_enabled = true;
        self.master_volume_left = 7;
        self.master_volume_right = 7;
        self.nr51 = 0xF3;

        self.pulse_channel.sweep_enable = true;
        self.pulse_channel.sweep_period = 0;
        self.pulse_channel.sweep_add = false;
        self.pulse_channel.sweep_shift = 0;
        self.pulse_channel.duty_cycle = 2;
        self.pulse_channel.length = 0x3F;
        self.pulse_channel.volume = 0x0F;
        self.pulse_channel.envelope_add = true;
        self.pulse_channel.envelope_period = 0x03;
        self.pulse_channel.current_volume = self.pulse_channel.volume;
        self.pulse_channel.frequency = 0x700;
        self.pulse_channel.length_enable = false;
        self.pulse_channel.enabled = false;

        self.pulse_channel2.duty_cycle = 0;
        self.pulse_channel2.length = 0x3F;
        self.pulse_channel2.volume = 0x00;
        self.pulse_channel2.envelope_add = false;
        self.pulse_channel2.envelope_period = 0x00;
        self.pulse_channel2.current_volume = 0x00;
        self.pulse_channel2.frequency = 0x700;
        self.pulse_channel2.length_enable = false;
        self.pulse_channel2.enabled = false;

        self.wave_channel.enabled = false;
        self.wave_channel.length = 0xFF;
        self.wave_channel.volume_code = 0x00;
        self.wave_channel.frequency = 0x700;
        self.wave_channel.length_enable = false;
        self.wave_channel.wave_ram = [0xFF; WAVE_RAM_SIZE];

        self.noise_channel.length = 0x3F;
        self.noise_channel.volume = 0x00;
        self.noise_channel.envelope_add = false;
        self.noise_channel.envelope_period = 0x00;
        self.noise_channel.shift_clock_frequency = 0x00;
        self.noise_channel.seven_bit_mode = false;
        self.noise_channel.divisor_code = 0x00;
        self.noise_channel.length_enable = false;
        self.noise_channel.current_volume = 0x00;
        self.noise_channel.enabled = false;
    }

    pub fn read_io(&self, addr: u16) -> u8 {
        match addr {
            REG_NR10 | REG_NR11 | REG_NR12 | REG_NR13 | REG_NR14 => {
                self.pulse_channel.read_io(addr)
            }
            REG_NR21 => self.pulse_channel2.read_duty_length(),
            REG_NR22 => self.pulse_channel2.read_envelope(),
            REG_NR23 => self.pulse_channel2.read_frequency_low(),
            REG_NR24 => self.pulse_channel2.read_frequency_high(),
            REG_NR30 | REG_NR31 | REG_NR32 | REG_NR33 | REG_NR34 => self.wave_channel.read_io(addr),
            REG_NR41 | REG_NR42 | REG_NR43 | REG_NR44 => self.noise_channel.read_io(addr),
            REG_NR50 => ((self.master_volume_left & 0x07) << 4) | (self.master_volume_right & 0x07),
            REG_NR51 => self.nr51,
            REG_NR52 => {
                let mut value = if self.sound_enabled { 0x80 } else { 0x00 };
                value |= 0x70;
                if self.pulse_channel.enabled {
                    value |= 0x01;
                }
                if self.pulse_channel2.enabled {
                    value |= 0x02;
                }
                if self.wave_channel.enabled {
                    value |= 0x04;
                }
                if self.noise_channel.enabled {
                    value |= 0x08;
                }
                value
            }
            WAVE_RAM_START..=0xFF3F => self.wave_channel.read_wave_ram(addr),
            _ => 0,
        }
    }

    pub fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            REG_NR10 | REG_NR11 | REG_NR12 | REG_NR13 | REG_NR14 => {
                if self.sound_enabled {
                    self.pulse_channel.write_io(addr, value);
                }
            }
            REG_NR21 => {
                if self.sound_enabled {
                    self.pulse_channel2.write_duty_length(value);
                }
            }
            REG_NR22 => {
                if self.sound_enabled {
                    self.pulse_channel2.write_envelope(value);
                }
            }
            REG_NR23 => {
                if self.sound_enabled {
                    self.pulse_channel2.write_frequency_low(value);
                }
            }
            REG_NR24 => {
                if self.sound_enabled {
                    self.pulse_channel2.write_frequency_high(value);
                }
            }
            REG_NR30 | REG_NR31 | REG_NR32 | REG_NR33 | REG_NR34 => {
                if self.sound_enabled {
                    self.wave_channel.write_io(addr, value);
                }
            }
            REG_NR41 | REG_NR42 | REG_NR43 | REG_NR44 => {
                if self.sound_enabled {
                    self.noise_channel.write_io(addr, value);
                }
            }
            REG_NR50 => {
                if self.sound_enabled {
                    self.master_volume_left = (value >> 4) & 0x07;
                    self.master_volume_right = value & 0x07;
                }
            }
            REG_NR51 => {
                if self.sound_enabled {
                    self.nr51 = value;
                }
            }
            REG_NR52 => {
                let was_enabled = self.sound_enabled;
                let now_enabled = value & 0x80 != 0;
                if was_enabled && !now_enabled {
                    self.reset();
                } else if !was_enabled && now_enabled {
                    self.reset_state();
                    self.sound_enabled = true;
                }
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

    pub fn pulse2_output(&self) -> i32 {
        self.pulse_channel2.output()
    }

    pub fn wave_output(&self) -> i32 {
        self.wave_channel.output()
    }

    pub fn noise_output(&self) -> i32 {
        self.noise_channel.output()
    }

    pub fn sample_rate_hz(&self) -> f64 {
        OUTPUT_SAMPLE_RATE_HZ
    }

    pub fn has_sample(&self) -> bool {
        !self.samples.is_empty()
    }

    pub fn take_sample(&mut self) -> i32 {
        let (left, right) = self.take_sample_stereo();
        ((left + right) / 2).clamp(-128, 127)
    }

    pub fn take_sample_stereo(&mut self) -> (i32, i32) {
        match self.samples.pop_front() {
            Some([left, right]) => (left, right),
            None => (0, 0),
        }
    }

    pub fn sample(&self) -> i32 {
        self.current_sample
    }

    pub fn sample_stereo(&self) -> (i32, i32) {
        (self.current_sample_left, self.current_sample_right)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Apu, CPU_HZ, CYCLES_PER_SAMPLE, FRAME_CYCLES, NoiseChannel, OUTPUT_SAMPLE_RATE_HZ,
        PulseChannel, WaveChannel,
    };

    #[test]
    fn new_apu_initializes_correctly() {
        let apu = Apu::new();
        let frame_rate = CPU_HZ / FRAME_CYCLES as f64;
        let expected = (apu.sample_rate_hz() / frame_rate).round() as u32;
        assert_eq!(apu.samples_per_frame(), expected);
    }

    #[test]
    fn apu_mixes_pulse_wave_and_noise_channels() {
        let mut apu = Apu::new();
        apu.apply_post_boot_state();
        apu.write_io(0xFF12, 0x80);
        apu.write_io(0xFF14, 0x80);
        apu.write_io(0xFF22, 0x00);
        apu.write_io(0xFF23, 0x80);
        for _ in 0..10 {
            let _ = apu.step(FRAME_CYCLES as u32);
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
            channel.tick_sweep();
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
            channel.tick_sweep();
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
            channel.tick_sweep();
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
            channel.tick_sweep();
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
            channel.tick_envelope();
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
            channel.tick_envelope();
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
            channel.tick_envelope();
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
            channel.tick_envelope();
        }
        assert_eq!(
            channel.current_volume, start_vol,
            "Volume should not change with period 0"
        );
    }

    #[test]
    fn apu_sample_rate_is_48khz() {
        let apu = Apu::new();
        let rate = apu.sample_rate_hz();
        assert!(
            (rate - OUTPUT_SAMPLE_RATE_HZ).abs() < 0.1,
            "Sample rate should be ~48kHz, got {}",
            rate
        );
    }

    #[test]
    fn apu_sample_generated_at_sample_boundary() {
        let mut apu = Apu::new();
        assert!(!apu.has_sample());
        assert_eq!(apu.sample(), 0);
        let cycles_per_sample = CYCLES_PER_SAMPLE.ceil() as u32;
        let _ = apu.step(cycles_per_sample);
        assert!(apu.has_sample());
        let sample = apu.take_sample();
        assert!(sample >= -128 && sample <= 127);
        assert!(!apu.has_sample());
    }

    #[test]
    fn apu_sample_is_clamped() {
        let mut apu = Apu::new();
        let cycles_per_sample = CYCLES_PER_SAMPLE.ceil() as u32;
        for _ in 0..10 {
            let _ = apu.step(cycles_per_sample);
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
        for _ in 0..5 {
            channel.tick_length();
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
        for _ in 0..300 {
            channel.tick_length();
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
        for _ in 0..5 {
            channel.tick_length();
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
        for _ in 0..300 {
            channel.tick_length();
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
        for _ in 0..5 {
            channel.tick_length();
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
        for _ in 0..300 {
            channel.tick_length();
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
        for _ in 0..10 {
            channel.tick_length();
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
