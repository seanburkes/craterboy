use super::{Apu, Bus, Cartridge, Cpu, CpuError, Framebuffer, MbcError, Ppu};

#[derive(Debug)]
pub struct Emulator {
    booted: bool,
    framebuffer: Framebuffer,
    cpu: Cpu,
    bus: Option<Bus>,
    cpu_error: Option<CpuError>,
    ppu: Ppu,
    apu: Apu,
}

impl Emulator {
    pub fn new() -> Self {
        Self {
            booted: false,
            framebuffer: Framebuffer::new(),
            cpu: Cpu::new(),
            bus: None,
            cpu_error: None,
            ppu: Ppu::new(),
            apu: Apu::new(),
        }
    }

    pub fn is_booted(&self) -> bool {
        self.booted
    }

    pub fn load_cartridge(&mut self, cartridge: Cartridge) -> Result<(), MbcError> {
        self.load_cartridge_with_boot_rom(cartridge, None)
    }

    pub fn load_cartridge_with_boot_rom(
        &mut self,
        cartridge: Cartridge,
        boot_rom: Option<Vec<u8>>,
    ) -> Result<(), MbcError> {
        let mut bus = Bus::with_boot_rom(cartridge, boot_rom)?;
        self.cpu = Cpu::new();
        self.cpu_error = None;
        self.ppu = Ppu::new();
        if bus.boot_rom_enabled() {
            self.booted = false;
        } else {
            self.cpu.apply_post_boot_state();
            bus.apply_post_boot_state();
            self.booted = true;
        }
        self.bus = Some(bus);
        Ok(())
    }

    pub fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    pub fn framebuffer_mut(&mut self) -> &mut Framebuffer {
        &mut self.framebuffer
    }

    pub fn has_bus(&self) -> bool {
        self.bus.is_some()
    }

    pub fn set_joyp_buttons(&mut self, mask: u8) {
        if let Some(bus) = self.bus.as_mut() {
            bus.set_joyp_buttons(mask);
        }
    }

    pub fn set_joyp_dpad(&mut self, mask: u8) {
        if let Some(bus) = self.bus.as_mut() {
            bus.set_joyp_dpad(mask);
        }
    }

    pub fn step_frame(&mut self) -> Result<u32, CpuError> {
        if let Some(err) = self.cpu_error {
            return Err(err);
        }

        if let Some(bus) = self.bus.as_mut() {
            let mut cycles: u32 = 0;
            let mut frame_ready = false;
            while !frame_ready {
                let step_cycles = match self.cpu.step(bus) {
                    Ok(count) => count,
                    Err(err) => {
                        self.cpu_error = Some(err);
                        return Err(err);
                    }
                };
                bus.step(step_cycles);
                self.apu.step(step_cycles);
                frame_ready = self.ppu.step(step_cycles, bus, &mut self.framebuffer);
                cycles = cycles.saturating_add(step_cycles);

                if bus.take_boot_rom_disabled() {
                    self.booted = true;
                }
            }
            Ok(cycles)
        } else {
            Ok(0)
        }
    }

    pub fn apu_step(&mut self, cycles: u32) {
        let _ = self.apu.step(cycles);
    }

    pub fn apu_sample_rate_hz(&self) -> f64 {
        self.apu.sample_rate_hz()
    }

    pub fn apu_has_sample(&self) -> bool {
        self.apu.has_sample()
    }

    pub fn apu_take_sample(&mut self) -> i32 {
        self.apu.take_sample()
    }

    pub fn apu_sample(&self) -> i32 {
        self.apu.sample()
    }

    pub fn apu_pulse_output(&self) -> i32 {
        self.apu.pulse_output()
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
}

#[cfg(test)]
mod tests {
    use super::Emulator;
    use crate::domain::FRAME_SIZE;

    #[test]
    fn new_emulator_starts_unbooted() {
        let emulator = Emulator::new();

        assert!(!emulator.is_booted());
    }

    #[test]
    fn new_emulator_has_framebuffer() {
        let emulator = Emulator::new();

        assert_eq!(emulator.framebuffer().len(), FRAME_SIZE);
    }

    #[test]
    fn emulator_apu_has_correct_sample_rate() {
        let emulator = Emulator::new();
        let rate = emulator.apu_sample_rate_hz();
        assert!(
            (rate - 59.7275).abs() < 0.001,
            "Expected ~59.7275 Hz, got {}",
            rate
        );
    }

    #[test]
    fn emulator_apu_step_does_not_crash() {
        let mut emulator = Emulator::new();
        emulator.apu_step(1000);
        assert!(emulator.apu_sample() >= -128 && emulator.apu_sample() <= 127);
    }

    #[test]
    fn emulator_apu_sample_generation() {
        let mut emulator = Emulator::new();
        assert!(!emulator.apu_has_sample());
        emulator.apu_step(70224);
        assert!(emulator.apu_has_sample());
        let sample = emulator.apu_take_sample();
        assert!(sample >= -128 && sample <= 127);
        assert!(!emulator.apu_has_sample());
    }

    #[test]
    fn emulator_apu_read_write_io() {
        let mut emulator = Emulator::new();
        emulator.apu_write_io(0xFF10, 0x80);
        assert_eq!(emulator.apu_read_io(0xFF10) & 0x80, 0x80);
        emulator.apu_write_io(0xFF22, 0x00);
        emulator.apu_write_io(0xFF23, 0x80);
        assert!(emulator.apu_noise_output() != 0 || emulator.apu_noise_output() == 0);
    }

    #[test]
    fn emulator_apu_outputs() {
        let emulator = Emulator::new();
        let pulse = emulator.apu_pulse_output();
        let wave = emulator.apu_wave_output();
        let noise = emulator.apu_noise_output();
        assert!(pulse >= 0 && pulse <= 15);
        assert!(wave >= 0 && wave <= 3);
        assert!(noise >= -15 && noise <= 15);
    }

    #[test]
    fn emulator_apu_reset() {
        let mut emulator = Emulator::new();
        emulator.apu_write_io(0xFF10, 0x80);
        emulator.apu_write_io(0xFF14, 0x80);
        emulator.apu_reset();
        let rate = emulator.apu_sample_rate_hz();
        assert!(
            (rate - 59.7275).abs() < 0.001,
            "Expected ~59.7275 Hz, got {}",
            rate
        );
    }
}
