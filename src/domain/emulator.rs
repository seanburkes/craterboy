use super::{Bus, Cartridge, Cpu, CpuError, Framebuffer, MbcError, Ppu};

#[derive(Debug)]
pub struct Emulator {
    booted: bool,
    framebuffer: Framebuffer,
    cpu: Cpu,
    bus: Option<Bus>,
    cpu_error: Option<CpuError>,
    ppu: Ppu,
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
        }
    }

    pub fn is_booted(&self) -> bool {
        self.booted
    }

    pub fn load_cartridge(&mut self, cartridge: Cartridge) -> Result<(), MbcError> {
        self.bus = Some(Bus::new(cartridge)?);
        self.cpu = Cpu::new();
        self.cpu_error = None;
        self.ppu = Ppu::new();
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
                frame_ready = self.ppu.step(step_cycles, bus, &mut self.framebuffer);
                cycles = cycles.saturating_add(step_cycles);
            }
            Ok(cycles)
        } else {
            Ok(0)
        }
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
}
