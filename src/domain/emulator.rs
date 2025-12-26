#[derive(Debug)]
pub struct Emulator {
    booted: bool,
}

impl Emulator {
    pub fn new() -> Self {
        Self { booted: false }
    }

    pub fn is_booted(&self) -> bool {
        self.booted
    }
}

#[cfg(test)]
mod tests {
    use super::Emulator;

    #[test]
    fn new_emulator_starts_unbooted() {
        let emulator = Emulator::new();

        assert!(!emulator.is_booted());
    }
}
