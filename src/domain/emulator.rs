use super::Framebuffer;

#[derive(Debug)]
pub struct Emulator {
    booted: bool,
    framebuffer: Framebuffer,
}

impl Emulator {
    pub fn new() -> Self {
        Self {
            booted: false,
            framebuffer: Framebuffer::new(),
        }
    }

    pub fn is_booted(&self) -> bool {
        self.booted
    }

    pub fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    pub fn framebuffer_mut(&mut self) -> &mut Framebuffer {
        &mut self.framebuffer
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
