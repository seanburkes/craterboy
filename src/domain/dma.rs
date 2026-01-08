/// Game Boy DMA (Direct Memory Access) Module
///
/// Implements OAM DMA transfer which copies 160 bytes from ROM/RAM to OAM.
/// DMA transfers take 160 microseconds (640 cycles on DMG, 4 cycles per byte).

const OAM_SIZE: usize = 0xA0; // 160 bytes
pub const DMA_CYCLES_PER_BYTE: u32 = 4;
pub const DMA_TOTAL_CYCLES: u32 = DMA_CYCLES_PER_BYTE * OAM_SIZE as u32;

#[derive(Debug)]
pub struct Dma {
    /// DMA register value (source address >> 8)
    dma: u8,
    /// Whether DMA is currently active
    active: bool,
    /// Cycles remaining for the DMA transfer
    cycles_remaining: u32,
    /// Number of bytes transferred so far
    bytes_transferred: u32,
    /// Base address for DMA source (DMA value << 8)
    base: u16,
}

impl Default for Dma {
    fn default() -> Self {
        Self::new()
    }
}

impl Dma {
    pub fn new() -> Self {
        Self {
            dma: 0xFF,
            active: false,
            cycles_remaining: 0,
            bytes_transferred: 0,
            base: 0,
        }
    }

    pub fn dma(&self) -> u8 {
        self.dma
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn bytes_transferred(&self) -> u32 {
        self.bytes_transferred
    }

    /// Start a new DMA transfer
    pub fn write_dma(&mut self, value: u8) {
        self.dma = value;
        self.active = true;
        self.cycles_remaining = DMA_TOTAL_CYCLES;
        self.bytes_transferred = 0;
        self.base = (value as u16) << 8;
    }

    /// Step the DMA transfer by the given number of cycles.
    /// Returns a list of (source_addr, oam_offset) pairs for bytes to transfer.
    pub fn step(&mut self, cycles: u32) -> Vec<(u16, usize)> {
        if !self.active {
            return Vec::new();
        }

        let previous_remaining = self.cycles_remaining;
        let consumed = cycles.min(previous_remaining);
        self.cycles_remaining = previous_remaining - consumed;

        let elapsed_before = DMA_TOTAL_CYCLES - previous_remaining;
        let elapsed_after = DMA_TOTAL_CYCLES - self.cycles_remaining;
        let bytes_before = elapsed_before / DMA_CYCLES_PER_BYTE;
        let bytes_after = elapsed_after / DMA_CYCLES_PER_BYTE;

        let mut transfers = Vec::new();
        for i in bytes_before..bytes_after.min(OAM_SIZE as u32) {
            let src_addr = self.base.wrapping_add(i as u16);
            let oam_offset = i as usize;
            transfers.push((src_addr, oam_offset));
        }

        self.bytes_transferred = bytes_after.min(OAM_SIZE as u32);

        if self.cycles_remaining == 0 || self.bytes_transferred >= OAM_SIZE as u32 {
            self.active = false;
            self.cycles_remaining = 0;
        }

        transfers
    }

    pub fn apply_post_boot_state(&mut self) {
        self.dma = 0xFF;
        self.active = false;
        self.cycles_remaining = 0;
        self.bytes_transferred = 0;
        self.base = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dma_new_has_correct_defaults() {
        let dma = Dma::new();
        assert_eq!(dma.dma(), 0xFF);
        assert!(!dma.is_active());
        assert_eq!(dma.bytes_transferred(), 0);
    }

    #[test]
    fn dma_write_starts_transfer() {
        let mut dma = Dma::new();
        dma.write_dma(0xC0);
        assert_eq!(dma.dma(), 0xC0);
        assert!(dma.is_active());
        assert_eq!(dma.bytes_transferred(), 0);
    }

    #[test]
    fn dma_step_transfers_bytes() {
        let mut dma = Dma::new();
        dma.write_dma(0xC0);

        // Transfer 1 byte (4 cycles)
        let transfers = dma.step(4);
        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers[0], (0xC000, 0));
        assert_eq!(dma.bytes_transferred(), 1);
        assert!(dma.is_active());
    }

    #[test]
    fn dma_completes_after_all_bytes() {
        let mut dma = Dma::new();
        dma.write_dma(0xC0);

        let transfers = dma.step(DMA_TOTAL_CYCLES);
        assert_eq!(transfers.len(), OAM_SIZE);
        assert!(!dma.is_active());
        assert_eq!(dma.bytes_transferred(), OAM_SIZE as u32);
    }

    #[test]
    fn dma_step_multiple_bytes_at_once() {
        let mut dma = Dma::new();
        dma.write_dma(0xC0);

        // Transfer 4 bytes (16 cycles)
        let transfers = dma.step(16);
        assert_eq!(transfers.len(), 4);
        assert_eq!(transfers[0], (0xC000, 0));
        assert_eq!(transfers[1], (0xC001, 1));
        assert_eq!(transfers[2], (0xC002, 2));
        assert_eq!(transfers[3], (0xC003, 3));
    }

    #[test]
    fn dma_incremental_transfers() {
        let mut dma = Dma::new();
        dma.write_dma(0xD0);

        // Transfer byte by byte
        for i in 0..OAM_SIZE {
            let transfers = dma.step(DMA_CYCLES_PER_BYTE);
            assert_eq!(transfers.len(), 1);
            assert_eq!(transfers[0], (0xD000 + i as u16, i));
        }

        assert!(!dma.is_active());
        assert_eq!(dma.bytes_transferred(), OAM_SIZE as u32);
    }
}
