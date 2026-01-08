/// Game Boy Color HDMA (Horizontal DMA) Module
///
/// Implements CGB HDMA transfer which copies data from ROM/RAM to VRAM in blocks.
/// HDMA has two modes:
/// - General Purpose DMA (GDMA): Transfers all blocks immediately
/// - H-Blank DMA: Transfers one block per H-Blank period

const HDMA_BLOCK_SIZE: usize = 0x10; // 16 bytes per block
const VBLANK_START: u8 = 144;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HdmaMode {
    Inactive,
    HBlank,
    General,
}

#[derive(Debug)]
pub struct Hdma {
    source: u16,
    dest: u16,
    blocks_remaining: u8,
    active: bool,
    mode: HdmaMode,
}

impl Default for Hdma {
    fn default() -> Self {
        Self::new()
    }
}

impl Hdma {
    pub fn new() -> Self {
        Self {
            source: 0,
            dest: 0,
            blocks_remaining: 0,
            active: false,
            mode: HdmaMode::Inactive,
        }
    }

    pub fn source(&self) -> u16 {
        self.source
    }

    pub fn dest(&self) -> u16 {
        self.dest
    }

    pub fn blocks_remaining(&self) -> u8 {
        self.blocks_remaining
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn is_hblank_mode(&self) -> bool {
        self.mode == HdmaMode::HBlank
    }

    pub fn write_hdma1(&mut self, value: u8) {
        if !self.active {
            self.source = ((value as u16) << 8) | (self.source & 0x00FF);
        }
    }

    pub fn write_hdma2(&mut self, value: u8) {
        if !self.active {
            self.source = (self.source & 0xFF00) | ((value as u16) & 0x00F0);
        }
    }

    pub fn write_hdma3(&mut self, value: u8) {
        if !self.active {
            self.dest = ((value as u16) << 8) | (self.dest & 0x00FF);
        }
    }

    pub fn write_hdma4(&mut self, value: u8) {
        if !self.active {
            self.dest = (self.dest & 0xFF00) | ((value as u16) & 0x00F0);
        }
    }

    /// Write to HDMA5 to start/stop transfer.
    /// Returns (should_transfer_now, blocks_to_transfer) for GDMA, or (false, 0) for HDMA.
    pub fn write_hdma5(&mut self, value: u8) -> (bool, u8) {
        let is_gdma = value & 0x80 != 0;
        let blocks = value & 0x7F;

        if self.active {
            if is_gdma {
                return (false, 0);
            }
            if blocks >= self.blocks_remaining {
                self.active = false;
                self.blocks_remaining = 0;
                return (false, 0);
            }
        }

        self.blocks_remaining = blocks.wrapping_add(1);
        self.mode = if is_gdma {
            HdmaMode::General
        } else {
            HdmaMode::HBlank
        };

        if is_gdma {
            // GDMA: transfer all blocks immediately
            self.active = true;
            let blocks_to_transfer = self.blocks_remaining;
            (true, blocks_to_transfer)
        } else {
            // HDMA: transfer one block per H-Blank
            self.active = true;
            (false, 0)
        }
    }

    pub fn read_hdma5(&self) -> u8 {
        let mut value = self.blocks_remaining;
        if self.active {
            value |= 0x80;
        }
        match self.mode {
            HdmaMode::HBlank => value,
            HdmaMode::General => value | 0x80,
            HdmaMode::Inactive => value,
        }
    }

    /// Check if we should transfer a block during H-Blank.
    /// Returns true if a block should be transferred.
    pub fn should_transfer_hblank(&self, ly: u8, ppu_mode: u8, ppu_line_cycles: u16) -> bool {
        if !self.active || self.mode != HdmaMode::HBlank {
            return false;
        }

        if ly >= VBLANK_START {
            return false;
        }

        if ppu_mode != 0 {
            return false;
        }

        let cycles_into_hblank = ppu_line_cycles;
        if cycles_into_hblank < 252 {
            return false;
        }

        self.blocks_remaining > 0
    }

    /// Transfer blocks and update state.
    /// Returns a list of (source_start, dest_start, block_count) for the transfer.
    pub fn transfer_blocks(&mut self, count: u8) -> Vec<(u16, u16, u8)> {
        if count == 0 || self.blocks_remaining == 0 {
            return Vec::new();
        }

        let actual_count = count.min(self.blocks_remaining);
        // Dest is always in VRAM (0x8000-0x9FFF), masked to valid VRAM offset
        let dest_addr = 0x8000 | (self.dest & 0x1FF0);
        let transfers = vec![(self.source, dest_addr, actual_count)];

        // Update source and dest
        let bytes_transferred = (actual_count as u16) * (HDMA_BLOCK_SIZE as u16);
        self.source = self.source.wrapping_add(bytes_transferred);
        self.dest = self.dest.wrapping_add(bytes_transferred);

        // Update blocks remaining
        self.blocks_remaining -= actual_count;

        if self.blocks_remaining == 0 {
            self.active = false;
            self.mode = HdmaMode::Inactive;
        }

        transfers
    }

    pub fn apply_post_boot_state(&mut self) {
        self.source = 0;
        self.dest = 0;
        self.blocks_remaining = 0;
        self.active = false;
        self.mode = HdmaMode::Inactive;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hdma_new_has_correct_defaults() {
        let hdma = Hdma::new();
        assert_eq!(hdma.source(), 0);
        assert_eq!(hdma.dest(), 0);
        assert_eq!(hdma.blocks_remaining(), 0);
        assert!(!hdma.is_active());
    }

    #[test]
    fn hdma_write_source_dest_registers() {
        let mut hdma = Hdma::new();
        hdma.write_hdma1(0x12);
        hdma.write_hdma2(0x30);
        hdma.write_hdma3(0x80);
        hdma.write_hdma4(0x10);

        assert_eq!(hdma.source(), 0x1230);
        assert_eq!(hdma.dest(), 0x8010);
    }

    #[test]
    fn hdma_gdma_transfers_immediately() {
        let mut hdma = Hdma::new();
        hdma.write_hdma1(0xC0);
        hdma.write_hdma2(0x00);
        hdma.write_hdma3(0x80);
        hdma.write_hdma4(0x00);

        let (should_transfer, blocks) = hdma.write_hdma5(0x80); // GDMA, 1 block
        assert!(should_transfer);
        assert_eq!(blocks, 1);
        assert!(hdma.is_active());

        let transfers = hdma.transfer_blocks(blocks);
        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers[0], (0xC000, 0x8000, 1));
        assert!(!hdma.is_active());
        assert_eq!(hdma.blocks_remaining(), 0);
    }

    #[test]
    fn hdma_hblank_mode_transfers_one_block_per_hblank() {
        let mut hdma = Hdma::new();
        hdma.write_hdma1(0xC0);
        hdma.write_hdma2(0x00);
        hdma.write_hdma3(0x80);
        hdma.write_hdma4(0x00);

        let (should_transfer, _) = hdma.write_hdma5(0x01); // HDMA, 2 blocks
        assert!(!should_transfer);
        assert!(hdma.is_active());
        assert!(hdma.is_hblank_mode());

        // Should transfer during H-Blank (mode 0, after cycle 252)
        assert!(hdma.should_transfer_hblank(0, 0, 252));

        // Transfer first block
        let transfers = hdma.transfer_blocks(1);
        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers[0], (0xC000, 0x8000, 1));
        assert_eq!(hdma.blocks_remaining(), 1);

        // Transfer second block
        let transfers = hdma.transfer_blocks(1);
        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers[0], (0xC010, 0x8010, 1));
        assert_eq!(hdma.blocks_remaining(), 0);
        assert!(!hdma.is_active());
    }

    #[test]
    fn hdma_does_not_transfer_during_vblank() {
        let mut hdma = Hdma::new();
        hdma.write_hdma1(0xC0);
        hdma.write_hdma2(0x00);
        hdma.write_hdma3(0x80);
        hdma.write_hdma4(0x00);
        hdma.write_hdma5(0x00); // HDMA, 1 block

        // During V-Blank (LY >= 144)
        assert!(!hdma.should_transfer_hblank(144, 0, 252));
        assert!(!hdma.should_transfer_hblank(153, 0, 252));
    }

    #[test]
    fn hdma_does_not_transfer_outside_hblank() {
        let mut hdma = Hdma::new();
        hdma.write_hdma1(0xC0);
        hdma.write_hdma2(0x00);
        hdma.write_hdma3(0x80);
        hdma.write_hdma4(0x00);
        hdma.write_hdma5(0x00); // HDMA, 1 block

        // Mode 2 (OAM Search)
        assert!(!hdma.should_transfer_hblank(0, 2, 0));

        // Mode 3 (Drawing)
        assert!(!hdma.should_transfer_hblank(0, 3, 100));

        // Mode 0 (H-Blank) but too early
        assert!(!hdma.should_transfer_hblank(0, 0, 251));
    }

    #[test]
    fn hdma_read_hdma5_reflects_state() {
        let mut hdma = Hdma::new();
        hdma.write_hdma5(0x02); // HDMA, 3 blocks

        let value = hdma.read_hdma5();
        assert!(value & 0x80 != 0, "Should indicate active transfer");
        assert_eq!(value & 0x7F, 3, "Should show 3 blocks remaining");
    }

    #[test]
    fn hdma_cannot_write_registers_during_active_transfer() {
        let mut hdma = Hdma::new();
        hdma.write_hdma1(0xC0);
        hdma.write_hdma2(0x00);
        hdma.write_hdma3(0x80);
        hdma.write_hdma4(0x00);
        hdma.write_hdma5(0x00); // Start HDMA

        // Try to write registers while active
        hdma.write_hdma1(0xFF);
        hdma.write_hdma2(0xFF);
        hdma.write_hdma3(0xFF);
        hdma.write_hdma4(0xFF);

        // Registers should be unchanged
        assert_eq!(hdma.source(), 0xC000);
        assert_eq!(hdma.dest(), 0x8000);
    }
}
