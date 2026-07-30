use super::Bus;

impl Bus {
    pub(super) fn step_dma(&mut self, cycles: u32) {
        let transfers = self.dma.step(cycles);
        for (src_addr, oam_offset) in transfers {
            let byte = self.read8(src_addr);
            self.oam[oam_offset] = byte;
        }
    }

    pub(super) fn perform_hdma_transfer(&mut self, blocks_to_transfer: u8) {
        let transfers = self.hdma.transfer_blocks(blocks_to_transfer);
        for (source, dest, block_count) in transfers {
            for block in 0..block_count {
                let block_source = source.wrapping_add((block as u16) * 16);
                let block_dest = dest.wrapping_add((block as u16) * 16);

                for i in 0..16 {
                    let byte = self.read8(block_source.wrapping_add(i));
                    let vram_addr = (block_dest.wrapping_add(i) - 0x8000) & 0x1FFF;
                    self.vram[self.vram_bank as usize][vram_addr as usize] = byte;
                }
            }
        }
    }

    pub(super) fn step_hdma(&mut self) {
        if self
            .hdma
            .should_transfer_hblank(self.ly, self.ppu_mode, self.ppu_line_cycles)
        {
            self.perform_hdma_transfer(1);
        }
    }
}
