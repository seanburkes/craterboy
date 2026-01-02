use super::Bus;

const SAMPLE_RATE: u32 = 4_194_304 / 70_224;
const FRAME_CYCLES: u32 = 70_224;

#[derive(Debug)]
pub struct Apu {
    frame_cycles: u32,
}

impl Apu {
    pub fn new() -> Self {
        Self {
            frame_cycles: FRAME_CYCLES,
        }
    }

    pub fn step(&mut self, cycles: u32) -> Result<(), ()> {
        self.frame_cycles = self.frame_cycles.wrapping_add(cycles);
        Ok(())
    }

    pub fn samples_per_frame(&self) -> u32 {
        self.frame_cycles
    }

    pub fn reset(&mut self) {
        self.frame_cycles = FRAME_CYCLES;
    }

    pub fn read_io(&self, _addr: u16) -> u8 {
        0
    }

    pub fn write_io(&mut self, _addr: u16, _value: u8) {}
}

#[cfg(test)]
mod tests {
    use super::{Apu, FRAME_CYCLES};

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
}
