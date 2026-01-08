/// Game Boy Serial Transfer Module
///
/// Implements the Game Boy's serial communication system:
/// - SB register (serial transfer data)
/// - SC register (serial transfer control)
///
/// Serial transfer takes 8192 cycles (512 cycles per bit, 8 bits) on internal clock.
/// External clock mode is not fully implemented.

// Serial transfer takes 8192 cycles (512 cycles per bit, 8 bits)
// Internal clock: 8192 cycles (8KHz)
// External clock: transfers are controlled externally
pub const SERIAL_TRANSFER_CYCLES: u32 = 8192;
pub const IF_SERIAL: u8 = 0x08;

#[derive(Debug)]
pub struct Serial {
    sb: u8,
    sc: u8,
    cycles_remaining: u32,
}

impl Default for Serial {
    fn default() -> Self {
        Self::new()
    }
}

impl Serial {
    pub fn new() -> Self {
        Self {
            sb: 0x00,
            sc: 0x00,
            cycles_remaining: 0,
        }
    }

    pub fn sb(&self) -> u8 {
        self.sb
    }

    pub fn sc(&self) -> u8 {
        self.sc | 0x7E // Bits 1-6 are unused and read as 1
    }

    pub fn write_sb(&mut self, value: u8) {
        self.sb = value;
    }

    pub fn write_sc(&mut self, value: u8) -> u8 {
        let old_sc = self.sc;
        self.sc = value;

        // Check if transfer start bit (bit 7) is set and clock is internal (bit 0 = 1)
        // Transfer starts when SC is written with bit 7 = 1
        let start_transfer = (value & 0x80) != 0;
        let internal_clock = (value & 0x01) != 0;
        let was_transferring = (old_sc & 0x80) != 0;

        if start_transfer && internal_clock && !was_transferring {
            // Start a new serial transfer with internal clock
            self.cycles_remaining = SERIAL_TRANSFER_CYCLES;
        }

        // External clock mode (bit 0 = 0) is not fully implemented
        // In external clock mode, the transfer is controlled by an external device
        // For now, we just hold the state but don't perform actual transfers
        0
    }

    /// Step the serial transfer by the given number of cycles.
    /// Returns the interrupt flag to set (IF_SERIAL if transfer complete, 0 otherwise).
    pub fn step(&mut self, cycles: u32) -> u8 {
        // Only process if a transfer is in progress (SC bit 7 is set and internal clock)
        let is_transferring = (self.sc & 0x80) != 0;
        let internal_clock = (self.sc & 0x01) != 0;

        if !is_transferring || !internal_clock {
            return 0;
        }

        if self.cycles_remaining == 0 {
            return 0;
        }

        // Decrement remaining cycles
        if self.cycles_remaining <= cycles {
            // Transfer complete
            self.cycles_remaining = 0;

            // Shift out SB data (send 0xFF if no device connected)
            // When no device is connected, received bits are all 1
            self.sb = 0xFF;

            // Clear transfer start bit (bit 7) in SC
            self.sc &= 0x7F;

            // Request serial interrupt
            IF_SERIAL
        } else {
            self.cycles_remaining -= cycles;
            0
        }
    }

    pub fn apply_post_boot_state(&mut self) {
        self.sb = 0x00;
        self.sc = 0x00;
        self.cycles_remaining = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_new_has_correct_defaults() {
        let serial = Serial::new();
        assert_eq!(serial.sb(), 0x00);
        assert_eq!(serial.sc() & 0x81, 0x00); // Ignore unused bits
    }

    #[test]
    fn serial_read_write_sb() {
        let mut serial = Serial::new();
        serial.write_sb(0x42);
        assert_eq!(serial.sb(), 0x42);
    }

    #[test]
    fn serial_sc_unused_bits_read_as_one() {
        let mut serial = Serial::new();
        serial.write_sc(0x00);
        assert_eq!(serial.sc(), 0x7E); // Bits 1-6 read as 1
    }

    #[test]
    fn serial_transfer_internal_clock() {
        let mut serial = Serial::new();
        serial.write_sb(0x42);
        serial.write_sc(0x81); // Start transfer, internal clock

        // Transfer should be in progress
        assert!(serial.sc() & 0x80 != 0);

        // Step just before completion
        let interrupt = serial.step(SERIAL_TRANSFER_CYCLES - 1);
        assert_eq!(interrupt, 0, "Should not complete yet");
        assert!(serial.sc() & 0x80 != 0, "Transfer still in progress");

        // Complete transfer
        let interrupt = serial.step(1);
        assert_eq!(interrupt, IF_SERIAL, "Should trigger serial interrupt");
        assert_eq!(serial.sb(), 0xFF, "SB should be 0xFF (no device)");
        assert_eq!(serial.sc() & 0x80, 0, "Transfer bit should be cleared");
    }

    #[test]
    fn serial_no_transfer_without_start_bit() {
        let mut serial = Serial::new();
        serial.write_sb(0x42);
        serial.write_sc(0x01); // Internal clock but no start bit

        let interrupt = serial.step(SERIAL_TRANSFER_CYCLES);
        assert_eq!(interrupt, 0, "Should not trigger interrupt");
        assert_eq!(serial.sb(), 0x42, "SB should be unchanged");
    }

    #[test]
    fn serial_external_clock_not_implemented() {
        let mut serial = Serial::new();
        serial.write_sb(0x42);
        serial.write_sc(0x80); // Start transfer, external clock (bit 0 = 0)

        // External clock mode does nothing
        let interrupt = serial.step(SERIAL_TRANSFER_CYCLES * 2);
        assert_eq!(interrupt, 0, "External clock should not complete");
    }
}
