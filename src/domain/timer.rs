/// Game Boy Timer Module
///
/// Implements the Game Boy's timer system with cycle-accurate timing behavior:
/// - DIV register (divider register, read-only, increments at 16384 Hz)
/// - TIMA register (timer counter, increments at programmable rate)
/// - TMA register (timer modulo, loaded into TIMA on overflow)
/// - TAC register (timer control, enables timer and selects clock)
///
/// The timer uses a falling-edge detection scheme where TIMA increments
/// when the selected bit of the system counter transitions from 1 to 0.
/// Overflow handling is delayed by one M-cycle for accuracy.
const IF_TIMER: u8 = 0x04;

/// Timer overflow occurs in 3 stages:
/// 1. Normal: No overflow
/// 2. Overflow: TIMA = 0x00, TMA not yet loaded
/// 3. Interrupt: TMA loaded to TIMA, IF flag set
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimerOverflowState {
    Normal,
    Overflow,
    Interrupt,
}

#[derive(Debug)]
pub struct Timer {
    div: u8,
    // System counter (internal 16-bit counter, DIV is upper 8 bits)
    system_counter: u16,
    tima: u8,
    tma: u8,
    tac: u8,
    // Previous state of the timer bit (for falling edge detection)
    timer_bit_prev: bool,
    // Overflow delay state machine (None, Overflow, Interrupt)
    timer_overflow_state: TimerOverflowState,
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

impl Timer {
    pub fn new() -> Self {
        Self {
            div: 0,
            system_counter: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            timer_bit_prev: false,
            timer_overflow_state: TimerOverflowState::Normal,
        }
    }

    pub fn div(&self) -> u8 {
        self.div
    }

    pub fn tima(&self) -> u8 {
        self.tima
    }

    pub fn tma(&self) -> u8 {
        self.tma
    }

    pub fn tac(&self) -> u8 {
        self.tac
    }

    pub fn write_div(&mut self) -> u8 {
        // Check if resetting would cause a falling edge
        let old_bit = self.get_timer_bit();

        // Reset system counter
        self.system_counter = 0;
        self.div = 0;

        let new_bit = self.get_timer_bit();

        // If timer is enabled and we had a falling edge, increment TIMA
        let timer_enabled = (self.tac & 0x04) != 0;
        let interrupt = if timer_enabled && old_bit && !new_bit {
            self.increment_tima()
        } else {
            0
        };

        self.timer_bit_prev = new_bit;
        interrupt
    }

    pub fn write_tima(&mut self, value: u8) {
        // Writing to TIMA during overflow cycle prevents TMA reload and interrupt
        if self.timer_overflow_state == TimerOverflowState::Overflow {
            self.timer_overflow_state = TimerOverflowState::Normal;
        }
        // Writing during interrupt cycle is ignored (TMA will be loaded anyway)
        if self.timer_overflow_state != TimerOverflowState::Interrupt {
            self.tima = value;
        }
    }

    pub fn write_tma(&mut self, value: u8) {
        self.tma = value;
        // If written during interrupt cycle, the value is also copied to TIMA
        if self.timer_overflow_state == TimerOverflowState::Interrupt {
            self.tima = value;
        }
    }

    pub fn write_tac(&mut self, value: u8) -> u8 {
        let old_enabled = (self.tac & 0x04) != 0;
        let new_enabled = (value & 0x04) != 0;

        let old_bit = self.get_timer_bit();

        // Update TAC
        self.tac = value;

        let new_bit = self.get_timer_bit();

        // Falling edge detection on TAC change
        // On DMG: disabling timer with bit set causes increment
        // On CGB: behavior varies, we implement conservative DMG behavior
        let falling_edge = old_bit && !new_bit;

        let interrupt = if falling_edge {
            if old_enabled && new_enabled {
                // Both enabled: falling edge from changing clock select
                self.increment_tima()
            } else if old_enabled && !new_enabled {
                // Disabling timer: DMG glitch behavior
                self.increment_tima()
            } else {
                0
            }
        } else {
            0
        };

        self.timer_bit_prev = new_bit;
        interrupt
    }

    /// Step the timer by the given number of cycles.
    /// Returns the interrupt flag to set (IF_TIMER if overflow occurred, 0 otherwise).
    pub fn step(&mut self, cycles: u32) -> u8 {
        let mut interrupt = 0;
        // Step the system counter one M-cycle at a time to properly handle edge detection
        for _ in 0..cycles {
            interrupt |= self.step_single_cycle();
        }
        interrupt
    }

    pub fn apply_post_boot_state(&mut self) {
        self.div = 0xAB;
        self.system_counter = (self.div as u16) << 8;
        self.tima = 0x00;
        self.tma = 0x00;
        self.tac = 0x00;
        self.timer_bit_prev = false;
        self.timer_overflow_state = TimerOverflowState::Normal;
    }

    /// Returns the currently selected bit of the system counter based on TAC
    fn get_timer_bit(&self) -> bool {
        let bit_index = match self.tac & 0x03 {
            0x00 => 9, // Bit 9: increment every 1024 cycles
            0x01 => 3, // Bit 3: increment every 16 cycles
            0x02 => 5, // Bit 5: increment every 64 cycles
            0x03 => 7, // Bit 7: increment every 256 cycles
            _ => 9,
        };
        (self.system_counter & (1 << bit_index)) != 0
    }

    /// Increment TIMA and handle overflow.
    /// Returns IF_TIMER if an interrupt should be triggered, 0 otherwise.
    fn increment_tima(&mut self) -> u8 {
        // Only increment if not already in an overflow state
        if self.timer_overflow_state != TimerOverflowState::Normal {
            return 0;
        }

        let (next, overflow) = self.tima.overflowing_add(1);
        if overflow {
            // TIMA overflows to 0x00
            self.tima = 0x00;
            // Set overflow state - TMA will be loaded NEXT cycle
            self.timer_overflow_state = TimerOverflowState::Overflow;
        } else {
            self.tima = next;
        }
        0
    }

    fn step_single_cycle(&mut self) -> u8 {
        // Process overflow state machine BEFORE incrementing
        // This ensures proper timing: overflow in cycle N, reload in cycle N+1
        let prev_overflow_state = self.timer_overflow_state;
        let mut interrupt = 0;

        match prev_overflow_state {
            TimerOverflowState::Normal => {
                // Nothing to do yet
            }
            TimerOverflowState::Overflow => {
                // One M-cycle after overflow: load TMA and request interrupt
                // This happens at START of next cycle, overwriting any CPU writes from previous cycle
                self.tima = self.tma;
                interrupt = IF_TIMER;
                self.timer_overflow_state = TimerOverflowState::Interrupt;
            }
            TimerOverflowState::Interrupt => {
                // Overflow handling complete, back to normal
                self.timer_overflow_state = TimerOverflowState::Normal;
            }
        }

        // Increment system counter (DIV is upper 8 bits)
        self.system_counter = self.system_counter.wrapping_add(1);
        self.div = (self.system_counter >> 8) as u8;

        // Get the currently selected timer bit
        let timer_bit = self.get_timer_bit();

        // Detect falling edge (1 -> 0) when timer is enabled
        let timer_enabled = (self.tac & 0x04) != 0;
        if timer_enabled && self.timer_bit_prev && !timer_bit {
            // Only increment if we haven't just started processing an overflow
            // (i.e., don't increment on the same cycle we're loading TMA)
            if prev_overflow_state == TimerOverflowState::Normal {
                self.increment_tima();
            }
        }

        // Save current bit for next cycle
        self.timer_bit_prev = timer_bit;

        interrupt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_increments_div() {
        let mut timer = Timer::new();
        assert_eq!(timer.div(), 0);
        timer.step(256);
        assert_eq!(timer.div(), 1);
    }

    #[test]
    fn timer_write_div_resets_counter() {
        let mut timer = Timer::new();
        timer.step(256); // Need 256 cycles for DIV to increment
        assert_ne!(timer.div(), 0);
        timer.write_div();
        assert_eq!(timer.div(), 0);
    }

    #[test]
    fn timer_tima_increments_at_correct_rate() {
        let mut timer = Timer::new();
        timer.write_tac(0x04); // Enable timer, rate = 1024 cycles
        timer.write_tma(0);
        timer.write_tima(0);

        timer.step(1024);
        assert_eq!(timer.tima(), 1);
    }

    #[test]
    fn timer_tima_overflow_loads_tma() {
        let mut timer = Timer::new();
        timer.write_tac(0x04); // Enable timer
        timer.write_tma(0xAB);
        timer.write_tima(0xFF);

        // The timer increments on falling edge of bit 9 (every 1024 cycles)
        // Starting from 0xFF, it should overflow after one increment
        // Bit 9 goes: 0 (0-511) -> 1 (512-1023) -> 0 (1024) [falling edge, increment]

        // Step to just before the falling edge
        for _ in 0..1023 {
            timer.step(1);
        }
        assert_eq!(
            timer.tima(),
            0xFF,
            "TIMA should still be 0xFF before increment"
        );

        // Step one more to trigger the falling edge and overflow
        let mut total_interrupt = timer.step(1);

        // The overflow causes TIMA = 0x00 immediately
        // Then TMA is loaded in the NEXT cycle, which is cycle 1025
        total_interrupt |= timer.step(1);

        assert!(
            total_interrupt & IF_TIMER != 0,
            "Should set timer interrupt"
        );
        assert_eq!(timer.tima(), 0xAB, "TIMA should be loaded with TMA");
    }

    #[test]
    fn timer_write_tima_during_overflow_cancels_reload() {
        let mut timer = Timer::new();
        timer.write_tac(0x04);
        timer.write_tma(0xAB);
        timer.write_tima(0xFF);

        timer.step(1024); // Trigger overflow
        timer.write_tima(0x12); // Write during overflow cycle
        timer.step(1); // Complete cycle

        assert_eq!(timer.tima(), 0x12, "TIMA write should cancel TMA reload");
    }

    #[test]
    fn timer_tac_frequency_modes() {
        let frequencies = [
            (0x04, 1024), // Mode 0: 4096 Hz
            (0x05, 16),   // Mode 1: 262144 Hz
            (0x06, 64),   // Mode 2: 65536 Hz
            (0x07, 256),  // Mode 3: 16384 Hz
        ];

        for (tac, cycles) in frequencies {
            let mut timer = Timer::new();
            timer.write_tac(tac);
            timer.write_tima(0);

            timer.step(cycles);
            assert_eq!(
                timer.tima(),
                1,
                "TAC mode {:02X} should increment TIMA after {} cycles",
                tac,
                cycles
            );
        }
    }
}
