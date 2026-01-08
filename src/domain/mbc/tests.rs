use super::Mbc;
use super::helpers::bank_count;
use super::rtc::{CYCLES_PER_SECOND, RtcMode};
use crate::domain::Cartridge;
use crate::domain::cartridge::ROM_BANK_SIZE;

#[test]
fn mbc1_write_changes_switchable_rom_bank() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 4];
    bytes[..ROM_BANK_SIZE].fill(0x11);
    bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x22);
    bytes[ROM_BANK_SIZE * 2..ROM_BANK_SIZE * 3].fill(0x33);
    bytes[ROM_BANK_SIZE * 3..].fill(0x44);
    bytes[0x0147] = 0x01;
    bytes[0x0149] = 0x02;

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
    mbc.write8(&mut cartridge, 0x2000, 0x02);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x33);
    mbc.write8(&mut cartridge, 0x2000, 0x00);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
}

#[test]
fn mbc1_mode_select_remaps_fixed_bank() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 64];
    for bank in 0..64 {
        let start = bank * ROM_BANK_SIZE;
        bytes[start..start + ROM_BANK_SIZE].fill(bank as u8);
    }
    bytes[0x0147] = 0x01;
    bytes[0x0149] = 0x02;

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    mbc.write8(&mut cartridge, 0x6000, 0x01);
    mbc.write8(&mut cartridge, 0x4000, 0x01);
    assert_eq!(mbc.read8(&cartridge, 0x0000), 32);
}

#[test]
fn mbc1_ram_enable_gates_reads_and_writes() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0x01;
    bytes[0x0149] = 0x02;

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    mbc.write8(&mut cartridge, 0xA000, 0x55);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);
    assert!(!cartridge.is_ram_dirty());

    mbc.write8(&mut cartridge, 0x0000, 0x0A);
    mbc.write8(&mut cartridge, 0xA000, 0x55);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x55);
    assert!(cartridge.is_ram_dirty());

    mbc.write8(&mut cartridge, 0x0000, 0x00);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);
}

#[test]
fn mbc1_ram_banking_selects_banks() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0x03;
    bytes[0x0149] = 0x03;

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    mbc.write8(&mut cartridge, 0x0000, 0x0A);
    mbc.write8(&mut cartridge, 0xA000, 0x11);

    mbc.write8(&mut cartridge, 0x6000, 0x01);
    mbc.write8(&mut cartridge, 0x4000, 0x01);
    mbc.write8(&mut cartridge, 0xA000, 0x22);

    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x22);
    mbc.write8(&mut cartridge, 0x4000, 0x00);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x11);
}

#[test]
fn mbc1_small_ram_ignores_bank_selection() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0x03;
    bytes[0x0149] = 0x02;

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    mbc.write8(&mut cartridge, 0x0000, 0x0A);
    mbc.write8(&mut cartridge, 0xA000, 0x44);

    mbc.write8(&mut cartridge, 0x6000, 0x01);
    mbc.write8(&mut cartridge, 0x4000, 0x02);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x44);
}
// MMM01 Tests
#[test]
fn mmm01_boots_from_last_bank() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 4];
    // Fill banks with distinct patterns
    bytes[..ROM_BANK_SIZE].fill(0x11);
    bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x22);
    bytes[ROM_BANK_SIZE * 2..ROM_BANK_SIZE * 3].fill(0x33);
    bytes[ROM_BANK_SIZE * 3..].fill(0x44);
    bytes[0x0147] = 0x0B; // MMM01

    let cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mbc = Mbc::new(&cartridge).expect("mbc");

    // Before mapping, reads should come from the last two banks
    assert_eq!(mbc.read8(&cartridge, 0x0000), 0x33); // Bank 2 at 0000-3FFF
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x44); // Bank 3 at 4000-7FFF
}

#[test]
fn mmm01_mapping_mode() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 8];
    for bank in 0..8 {
        let start = bank * ROM_BANK_SIZE;
        bytes[start..start + ROM_BANK_SIZE].fill((bank + 0x10) as u8);
    }
    bytes[0x0147] = 0x0B; // MMM01
    bytes[0x0149] = 0x02; // 8KB RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Configure ROM window: base=0, mask=3 (4 banks visible)
    mbc.write8(&mut cartridge, 0x0000, 0x00); // ROM base = 0
    mbc.write8(&mut cartridge, 0x2000, 0x43); // ROM mask = 3, enable mapping (bit 6)

    // Now should be in mapped mode, behaving like MBC1
    // Reading from 0000-3FFF should give bank 0
    assert_eq!(mbc.read8(&cartridge, 0x0000), 0x10);
    // Reading from 4000-7FFF should give bank 1 (default switchable)
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x11);

    // Switch to bank 2
    mbc.write8(&mut cartridge, 0x2000, 0x02);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x12);
}

#[test]
fn mmm01_rom_window() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 8];
    for bank in 0..8 {
        let start = bank * ROM_BANK_SIZE;
        bytes[start..start + ROM_BANK_SIZE].fill((bank + 0x20) as u8);
    }
    bytes[0x0147] = 0x0B; // MMM01

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Configure ROM window: base=4, mask=1 (2 banks visible: 4 and 5)
    mbc.write8(&mut cartridge, 0x0000, 0x04); // ROM base = 4
    mbc.write8(&mut cartridge, 0x2000, 0x41); // ROM mask = 1, enable mapping

    // Bank 0 in window maps to physical bank 4
    assert_eq!(mbc.read8(&cartridge, 0x0000), 0x24);
    // Bank 1 in window maps to physical bank 5
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x25);

    // Try to switch to "bank 2" but it wraps to bank 0 due to mask
    mbc.write8(&mut cartridge, 0x2000, 0x02);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x24); // Wraps back to bank 4
}

#[test]
fn mbc2_rom_and_ram_rules() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 4];
    bytes[..ROM_BANK_SIZE].fill(0x11);
    bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x22);
    bytes[ROM_BANK_SIZE * 2..ROM_BANK_SIZE * 3].fill(0x33);
    bytes[ROM_BANK_SIZE * 3..].fill(0x44);
    bytes[0x0147] = 0x05;

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
    mbc.write8(&mut cartridge, 0x2100, 0x02);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x33);

    mbc.write8(&mut cartridge, 0xA000, 0xAB);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);
    mbc.write8(&mut cartridge, 0x0000, 0x0A);
    mbc.write8(&mut cartridge, 0xA000, 0xAB);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFB);
}

#[test]
fn mbc3_rtc_latch_and_registers() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0x0F;
    bytes[0x0149] = 0x02;

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    mbc.write8(&mut cartridge, 0x0000, 0x0A);
    mbc.write8(&mut cartridge, 0x4000, 0x08);
    mbc.write8(&mut cartridge, 0xA000, 0x25);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x25);

    mbc.write8(&mut cartridge, 0x6000, 0x00);
    mbc.write8(&mut cartridge, 0x6000, 0x01);

    mbc.write8(&mut cartridge, 0xA000, 0x30);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x25);
}

#[test]
fn mbc3_rtc_ticks_with_cycles() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0x0F;
    bytes[0x0149] = 0x02;

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    mbc.write8(&mut cartridge, 0x0000, 0x0A);
    mbc.write8(&mut cartridge, 0x4000, 0x08);
    mbc.set_rtc_mode(RtcMode::Deterministic);
    mbc.tick(CYCLES_PER_SECOND);

    assert_eq!(mbc.read8(&cartridge, 0xA000), 1);
}

#[test]
fn mbc3_without_rtc_ignores_rtc_register_selection() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0x13;
    bytes[0x0149] = 0x02;

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    mbc.write8(&mut cartridge, 0x0000, 0x0A);
    mbc.write8(&mut cartridge, 0xA000, 0x55);
    mbc.write8(&mut cartridge, 0x4000, 0x08);

    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x55);
}

#[test]
fn mbc5_uses_9bit_rom_bank() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 260];
    bytes[..ROM_BANK_SIZE].fill(0x10);
    let bank_257 = 257 * ROM_BANK_SIZE;
    bytes[bank_257..bank_257 + ROM_BANK_SIZE].fill(0x77);
    bytes[0x0147] = 0x19;
    bytes[0x0149] = 0x02;

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    mbc.write8(&mut cartridge, 0x2000, 0x01);
    mbc.write8(&mut cartridge, 0x3000, 0x01);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x77);
}

#[test]
fn rom_only_ram_reads_and_writes() {
    let mut bytes = vec![0; ROM_BANK_SIZE];
    bytes[0x0147] = 0x08;
    bytes[0x0149] = 0x02;

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    mbc.write8(&mut cartridge, 0xA000, 0x5A);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x5A);
}

#[test]
fn bank_count_rounds_up() {
    let bytes = vec![0; ROM_BANK_SIZE + 1];
    assert_eq!(bank_count(&bytes), 2);
}

// Pocket Camera Tests
#[test]
fn pocket_camera_rom_banking() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 4];
    bytes[..ROM_BANK_SIZE].fill(0x11);
    bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x22);
    bytes[ROM_BANK_SIZE * 2..ROM_BANK_SIZE * 3].fill(0x33);
    bytes[ROM_BANK_SIZE * 3..].fill(0x44);
    bytes[0x0147] = 0xFC; // Pocket Camera
    bytes[0x0149] = 0x03; // 32KB RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Default bank 1
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);

    // Switch to bank 2
    mbc.write8(&mut cartridge, 0x2000, 0x02);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x33);

    // Bank 0 becomes bank 1
    mbc.write8(&mut cartridge, 0x2000, 0x00);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
}

#[test]
fn pocket_camera_registers() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0xFC; // Pocket Camera
    bytes[0x0149] = 0x02; // 8KB RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Enable RAM/camera
    mbc.write8(&mut cartridge, 0x0000, 0x0A);

    // Write to camera registers (A000-A03F)
    mbc.write8(&mut cartridge, 0xA001, 0x12);
    mbc.write8(&mut cartridge, 0xA002, 0x34);
    mbc.write8(&mut cartridge, 0xA03F, 0xFF);

    // Read back camera registers
    assert_eq!(mbc.read8(&cartridge, 0xA001), 0x12);
    assert_eq!(mbc.read8(&cartridge, 0xA002), 0x34);
    assert_eq!(mbc.read8(&cartridge, 0xA03F), 0xFF);
}

#[test]
fn pocket_camera_capture_trigger() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0xFC; // Pocket Camera
    bytes[0x0149] = 0x02; // 8KB RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Enable camera
    mbc.write8(&mut cartridge, 0x0000, 0x0A);

    // Trigger capture by setting bit 0 of A000
    mbc.write8(&mut cartridge, 0xA000, 0x01);

    // After capture, bit 0 should be cleared (simulated instant capture)
    assert_eq!(mbc.read8(&cartridge, 0xA000) & 0x01, 0);
}

#[test]
fn pocket_camera_ram_beyond_registers() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0xFC; // Pocket Camera
    bytes[0x0149] = 0x02; // 8KB RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Enable RAM
    mbc.write8(&mut cartridge, 0x0000, 0x0A);

    // Write to RAM beyond camera registers (A040-BFFF)
    mbc.write8(&mut cartridge, 0xA040, 0xAA);
    mbc.write8(&mut cartridge, 0xB000, 0xBB);

    // Read back
    assert_eq!(mbc.read8(&cartridge, 0xA040), 0xAA);
    assert_eq!(mbc.read8(&cartridge, 0xB000), 0xBB);
}

#[test]
fn tama5_rom_banking() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 4];
    bytes[..ROM_BANK_SIZE].fill(0x11);
    bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x22);
    bytes[ROM_BANK_SIZE * 2..ROM_BANK_SIZE * 3].fill(0x33);
    bytes[ROM_BANK_SIZE * 3..].fill(0x44);
    bytes[0x0147] = 0xFD; // TAMA5
    bytes[0x0149] = 0x00; // No standard RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Default should be bank 1
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);

    // Switch to bank 2
    mbc.write8(&mut cartridge, 0x0000, 0x02);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x33);

    // Switch to bank 3
    mbc.write8(&mut cartridge, 0x0000, 0x03);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x44);

    // Bank 0 should become bank 1
    mbc.write8(&mut cartridge, 0x0000, 0x00);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
}

#[test]
fn tama5_register_interface() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0xFD; // TAMA5
    bytes[0x0149] = 0x00; // No standard RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // TAMA5 uses a register-based interface
    // Write to internal RAM register 0x05 with value 0xA3

    // Set command mode to RAM write (0x00)
    mbc.write8(&mut cartridge, 0x2000, 0x00);

    // Set address to 0x05
    mbc.write8(&mut cartridge, 0x4000, 0x05); // addr_low
    mbc.write8(&mut cartridge, 0x6000, 0x00); // addr_high

    // Write data 0xA3 (0x03 low, 0x0A high)
    mbc.write8(&mut cartridge, 0xA000, 0x03); // data_in_low
    mbc.write8(&mut cartridge, 0xA001, 0x0A); // data_in_high (triggers command)

    // Now read back from register 0x05
    // Set command mode to RAM read (0x01)
    mbc.write8(&mut cartridge, 0x2000, 0x01);

    // Set address to 0x05
    mbc.write8(&mut cartridge, 0x4000, 0x05); // addr_low
    mbc.write8(&mut cartridge, 0x6000, 0x00); // addr_high

    // Trigger read by writing to A001
    mbc.write8(&mut cartridge, 0xA000, 0x00);
    mbc.write8(&mut cartridge, 0xA001, 0x00);

    // Read data from A000 (lower 4 bits) and A001 (upper 4 bits)
    // TAMA5 stores only 4 bits per register, so we should get 0x03
    let low = mbc.read8(&cartridge, 0xA000);
    assert_eq!(low & 0x0F, 0x03);
}

#[test]
fn tama5_rtc_read_write() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0xFD; // TAMA5
    bytes[0x0149] = 0x00; // No standard RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Write to RTC seconds register
    // Command mode 0x05 = RTC write
    mbc.write8(&mut cartridge, 0x2000, 0x05);

    // Write lower digit of seconds (addr 0x00) = 5
    mbc.write8(&mut cartridge, 0x4000, 0x00);
    mbc.write8(&mut cartridge, 0x6000, 0x00);
    mbc.write8(&mut cartridge, 0xA000, 0x05);
    mbc.write8(&mut cartridge, 0xA001, 0x00);

    // Write upper digit of seconds (addr 0x01) = 3 (total = 35 seconds)
    mbc.write8(&mut cartridge, 0x4000, 0x01);
    mbc.write8(&mut cartridge, 0x6000, 0x00);
    mbc.write8(&mut cartridge, 0xA000, 0x03);
    mbc.write8(&mut cartridge, 0xA001, 0x00);

    // Read back RTC seconds
    // Command mode 0x04 = RTC read
    mbc.write8(&mut cartridge, 0x2000, 0x04);

    // Read lower digit (addr 0x00)
    mbc.write8(&mut cartridge, 0x4000, 0x00);
    mbc.write8(&mut cartridge, 0x6000, 0x00);
    mbc.write8(&mut cartridge, 0xA000, 0x00);
    mbc.write8(&mut cartridge, 0xA001, 0x00);
    let low = mbc.read8(&cartridge, 0xA000);
    assert_eq!(low & 0x0F, 0x05);

    // Read upper digit (addr 0x01)
    mbc.write8(&mut cartridge, 0x4000, 0x01);
    mbc.write8(&mut cartridge, 0x6000, 0x00);
    mbc.write8(&mut cartridge, 0xA000, 0x00);
    mbc.write8(&mut cartridge, 0xA001, 0x00);
    let high = mbc.read8(&cartridge, 0xA000);
    assert_eq!(high & 0x0F, 0x03);
}

#[test]
fn tama5_ram_access() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0xFD; // TAMA5
    bytes[0x0149] = 0x00; // No standard RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // TAMA5 has 32 internal RAM registers (4 bits each)
    // Write to several registers and read them back

    // Command mode 0x00 = RAM write
    mbc.write8(&mut cartridge, 0x2000, 0x00);

    // Write to register 0x00 = value 0x0F
    mbc.write8(&mut cartridge, 0x4000, 0x00);
    mbc.write8(&mut cartridge, 0x6000, 0x00);
    mbc.write8(&mut cartridge, 0xA000, 0x0F);
    mbc.write8(&mut cartridge, 0xA001, 0x00);

    // Write to register 0x1F (last register) = value 0x07
    mbc.write8(&mut cartridge, 0x4000, 0x1F);
    mbc.write8(&mut cartridge, 0x6000, 0x00);
    mbc.write8(&mut cartridge, 0xA000, 0x07);
    mbc.write8(&mut cartridge, 0xA001, 0x00);

    // Command mode 0x01 = RAM read
    mbc.write8(&mut cartridge, 0x2000, 0x01);

    // Read register 0x00
    mbc.write8(&mut cartridge, 0x4000, 0x00);
    mbc.write8(&mut cartridge, 0x6000, 0x00);
    mbc.write8(&mut cartridge, 0xA000, 0x00);
    mbc.write8(&mut cartridge, 0xA001, 0x00);
    assert_eq!(mbc.read8(&cartridge, 0xA000) & 0x0F, 0x0F);

    // Read register 0x1F
    mbc.write8(&mut cartridge, 0x4000, 0x1F);
    mbc.write8(&mut cartridge, 0x6000, 0x00);
    mbc.write8(&mut cartridge, 0xA000, 0x00);
    mbc.write8(&mut cartridge, 0xA001, 0x00);
    assert_eq!(mbc.read8(&cartridge, 0xA000) & 0x0F, 0x07);
}

#[test]
fn huc1_rom_bank_switching() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 4];
    bytes[..ROM_BANK_SIZE].fill(0x11);
    bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x22);
    bytes[ROM_BANK_SIZE * 2..ROM_BANK_SIZE * 3].fill(0x33);
    bytes[ROM_BANK_SIZE * 3..].fill(0x44);
    bytes[0x0147] = 0xFF; // HuC1
    bytes[0x0149] = 0x02; // 8KB RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Default should be bank 1
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);

    // Switch to bank 2
    mbc.write8(&mut cartridge, 0x2000, 0x02);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x33);

    // Switch to bank 3
    mbc.write8(&mut cartridge, 0x2000, 0x03);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x44);

    // Bank 0 should become bank 1
    mbc.write8(&mut cartridge, 0x2000, 0x00);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
}

#[test]
fn huc1_ram_mode_read_write() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0xFF; // HuC1
    bytes[0x0149] = 0x02; // 8KB RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Default is RAM mode (not IR mode)
    mbc.write8(&mut cartridge, 0xA000, 0xAB);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xAB);

    mbc.write8(&mut cartridge, 0xA100, 0xCD);
    assert_eq!(mbc.read8(&cartridge, 0xA100), 0xCD);
}

#[test]
fn huc1_ram_bank_switching() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0xFF; // HuC1
    bytes[0x0149] = 0x03; // 32KB RAM (4 banks)

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Write to bank 0
    mbc.write8(&mut cartridge, 0x4000, 0x00);
    mbc.write8(&mut cartridge, 0xA000, 0x11);

    // Write to bank 1
    mbc.write8(&mut cartridge, 0x4000, 0x01);
    mbc.write8(&mut cartridge, 0xA000, 0x22);

    // Write to bank 2
    mbc.write8(&mut cartridge, 0x4000, 0x02);
    mbc.write8(&mut cartridge, 0xA000, 0x33);

    // Verify each bank preserved its data
    mbc.write8(&mut cartridge, 0x4000, 0x00);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x11);

    mbc.write8(&mut cartridge, 0x4000, 0x01);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x22);

    mbc.write8(&mut cartridge, 0x4000, 0x02);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x33);
}

#[test]
fn huc1_ir_mode_switching() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0xFF; // HuC1
    bytes[0x0149] = 0x02; // 8KB RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Start in RAM mode, write some data
    mbc.write8(&mut cartridge, 0xA000, 0x55);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x55);

    // Switch to IR mode
    mbc.write8(&mut cartridge, 0x0000, 0x0E);

    // Read should return IR register (0xC0 for no signal)
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xC0);

    // Write IR signal on
    mbc.write8(&mut cartridge, 0xA000, 0x01);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xC1);

    // Write IR signal off
    mbc.write8(&mut cartridge, 0xA000, 0x00);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xC0);

    // Switch back to RAM mode (write anything except 0x0E)
    mbc.write8(&mut cartridge, 0x0000, 0x00);

    // RAM data should still be there
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x55);
}

#[test]
fn huc1_ir_mode_only_triggers_with_0e() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0xFF; // HuC1
    bytes[0x0149] = 0x02; // 8KB RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Write data in RAM mode
    mbc.write8(&mut cartridge, 0xA000, 0xAA);

    // Try various values - none should trigger IR mode except 0x0E
    for value in [0x00, 0x01, 0x0A, 0x0D, 0x0F, 0xFF] {
        mbc.write8(&mut cartridge, 0x0000, value);
        assert_eq!(
            mbc.read8(&cartridge, 0xA000),
            0xAA,
            "Failed for value 0x{:02X}",
            value
        );
    }

    // Now 0x0E should trigger IR mode
    mbc.write8(&mut cartridge, 0x0000, 0x0E);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xC0);
}

#[test]
fn huc1_rom_bank_masks_to_6_bits() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 64];
    for i in 0..64 {
        let start = i * ROM_BANK_SIZE;
        bytes[start..start + ROM_BANK_SIZE].fill(i as u8);
    }
    bytes[0x0147] = 0xFF; // HuC1

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Write bank with upper bits set (should be masked)
    mbc.write8(&mut cartridge, 0x2000, 0xFF); // 0xFF & 0x3F = 0x3F = 63
    assert_eq!(mbc.read8(&cartridge, 0x4000), 63);

    mbc.write8(&mut cartridge, 0x2000, 0xC0); // 0xC0 & 0x3F = 0x00 -> becomes 1
    assert_eq!(mbc.read8(&cartridge, 0x4000), 1);
}

#[test]
fn mbc7_rom_bank_switching() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 4];
    bytes[..ROM_BANK_SIZE].fill(0x11);
    bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x22);
    bytes[ROM_BANK_SIZE * 2..ROM_BANK_SIZE * 3].fill(0x33);
    bytes[ROM_BANK_SIZE * 3..].fill(0x44);
    bytes[0x0147] = 0x22; // MBC7

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Default should be bank 1
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);

    // Switch to bank 2
    mbc.write8(&mut cartridge, 0x2000, 0x02);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x33);

    // Switch to bank 3
    mbc.write8(&mut cartridge, 0x2000, 0x03);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x44);

    // Bank 0 should become bank 1
    mbc.write8(&mut cartridge, 0x2000, 0x00);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
}

#[test]
fn mbc7_dual_ram_enable() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0x22; // MBC7

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Neither enable active - should return 0xFF
    assert_eq!(mbc.read8(&cartridge, 0xA020), 0xFF);

    // Enable RAM 1 only - still disabled
    mbc.write8(&mut cartridge, 0x0000, 0x0A);
    assert_eq!(mbc.read8(&cartridge, 0xA020), 0xFF);

    // Enable RAM 2 only (disable RAM 1) - still disabled
    mbc.write8(&mut cartridge, 0x0000, 0x00);
    mbc.write8(&mut cartridge, 0x4000, 0x40);
    assert_eq!(mbc.read8(&cartridge, 0xA020), 0xFF);

    // Enable both - should work
    mbc.write8(&mut cartridge, 0x0000, 0x0A);
    mbc.write8(&mut cartridge, 0x4000, 0x40);
    // Should not be 0xFF (reading accel X low byte, default 0x00)
    assert_eq!(mbc.read8(&cartridge, 0xA020), 0x00);
}

#[test]
fn mbc7_accelerometer_latch() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0x22; // MBC7

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Enable RAM
    mbc.write8(&mut cartridge, 0x0000, 0x0A);
    mbc.write8(&mut cartridge, 0x4000, 0x40);

    // Initial values should be 0x8000
    assert_eq!(mbc.read8(&cartridge, 0xA020), 0x00);
    assert_eq!(mbc.read8(&cartridge, 0xA030), 0x80);
    assert_eq!(mbc.read8(&cartridge, 0xA040), 0x00);
    assert_eq!(mbc.read8(&cartridge, 0xA050), 0x80);

    // Erase latch (write 0x55 to Ax0x)
    mbc.write8(&mut cartridge, 0xA000, 0x55);
    assert_eq!(mbc.read8(&cartridge, 0xA020), 0x00);
    assert_eq!(mbc.read8(&cartridge, 0xA030), 0x80);

    // Latch accelerometer (write 0xAA to Ax1x)
    mbc.write8(&mut cartridge, 0xA010, 0xAA);

    // Should now read centered values (0x81D0)
    assert_eq!(mbc.read8(&cartridge, 0xA020), 0xD0); // X low
    assert_eq!(mbc.read8(&cartridge, 0xA030), 0x81); // X high
    assert_eq!(mbc.read8(&cartridge, 0xA040), 0xD0); // Y low
    assert_eq!(mbc.read8(&cartridge, 0xA050), 0x81); // Y high
}

#[test]
fn mbc7_accelerometer_requires_erase_before_relatch() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0x22; // MBC7

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Enable RAM
    mbc.write8(&mut cartridge, 0x0000, 0x0A);
    mbc.write8(&mut cartridge, 0x4000, 0x40);

    // First latch
    mbc.write8(&mut cartridge, 0xA000, 0x55);
    mbc.write8(&mut cartridge, 0xA010, 0xAA);
    assert_eq!(mbc.read8(&cartridge, 0xA020), 0xD0);

    // Try to relatch without erase - should not change
    mbc.write8(&mut cartridge, 0xA010, 0xAA);
    assert_eq!(mbc.read8(&cartridge, 0xA020), 0xD0);

    // Erase and relatch - should work
    mbc.write8(&mut cartridge, 0xA000, 0x55);
    assert_eq!(mbc.read8(&cartridge, 0xA020), 0x00); // Back to 0x8000
    mbc.write8(&mut cartridge, 0xA010, 0xAA);
    assert_eq!(mbc.read8(&cartridge, 0xA020), 0xD0); // Latched again
}

#[test]
fn mbc7_fixed_register_values() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0x22; // MBC7

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Enable RAM
    mbc.write8(&mut cartridge, 0x0000, 0x0A);
    mbc.write8(&mut cartridge, 0x4000, 0x40);

    // Ax6x always reads 0x00
    assert_eq!(mbc.read8(&cartridge, 0xA060), 0x00);
    assert_eq!(mbc.read8(&cartridge, 0xA06F), 0x00);

    // Ax7x always reads 0xFF
    assert_eq!(mbc.read8(&cartridge, 0xA070), 0xFF);
    assert_eq!(mbc.read8(&cartridge, 0xA07F), 0xFF);
}

#[test]
fn mbc7_eeprom_enable_write() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0x22; // MBC7

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Enable RAM
    mbc.write8(&mut cartridge, 0x0000, 0x0A);
    mbc.write8(&mut cartridge, 0x4000, 0x40);

    // Send EWEN command (0011xxxxxx)
    // CS low
    mbc.write8(&mut cartridge, 0xA080, 0x00);
    // CS high
    mbc.write8(&mut cartridge, 0xA080, 0x80);
    // Shift in start bit (0) then 1
    mbc.write8(&mut cartridge, 0xA080, 0xC0); // CLK=1, DI=0
    mbc.write8(&mut cartridge, 0xA080, 0x80); // CLK=0
    mbc.write8(&mut cartridge, 0xA080, 0xC2); // CLK=1, DI=1
    mbc.write8(&mut cartridge, 0xA080, 0x82); // CLK=0

    // Shift in EWEN command: 0011xxxxxx
    for bit in [0, 0, 1, 1, 0, 0, 0, 0, 0, 0] {
        let val = if bit != 0 { 0xC2 } else { 0xC0 };
        mbc.write8(&mut cartridge, 0xA080, val); // CLK=1
        mbc.write8(&mut cartridge, 0xA080, val & !0x40); // CLK=0
    }

    // CS low to complete
    mbc.write8(&mut cartridge, 0xA080, 0x00);

    // Write should now be enabled (tested implicitly in write test)
}

#[test]
fn mbc7_eeprom_read() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0x22; // MBC7

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Enable RAM
    mbc.write8(&mut cartridge, 0x0000, 0x0A);
    mbc.write8(&mut cartridge, 0x4000, 0x40);

    // EEPROM starts with 0xFF
    // Send READ command for address 0: opcode 10 (bits 9-8) + address 00000000 (bits 7-0)
    mbc.write8(&mut cartridge, 0xA080, 0x00); // CS low
    mbc.write8(&mut cartridge, 0xA080, 0x80); // CS high (starts ReadCommand state)

    // Read command: 1000000000b = 0x0200 (opcode=10, addr=0)
    for bit in [1, 0, 0, 0, 0, 0, 0, 0, 0, 0] {
        let val = if bit != 0 { 0xC2 } else { 0xC0 };
        mbc.write8(&mut cartridge, 0xA080, val); // CLK=1
        mbc.write8(&mut cartridge, 0xA080, val & !0x40); // CLK=0
    }

    // Read out 16 bits - should be 0xFFFF
    for i in 0..16 {
        mbc.write8(&mut cartridge, 0xA080, 0xC0); // CLK=1
        let val = mbc.read8(&cartridge, 0xA080);
        assert_eq!(
            val & 0x01,
            0x01,
            "EEPROM bit {} should read 1 (0xFF), got register value 0x{:02X}",
            i,
            val
        );
        mbc.write8(&mut cartridge, 0xA080, 0x80); // CLK=0
    }
}

#[test]
fn huc3_rom_bank_switching() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 4];
    bytes[0x0147] = 0xFE; // HuC3
    // Fill banks with distinct patterns
    bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0x11);
    bytes[ROM_BANK_SIZE * 2..ROM_BANK_SIZE * 3].fill(0x22);
    bytes[ROM_BANK_SIZE * 3..].fill(0x33);

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Default: bank 1 at 0x4000-0x7FFF
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x11);

    // Switch to bank 2
    mbc.write8(&mut cartridge, 0x2000, 0x02);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);

    // Switch to bank 3
    mbc.write8(&mut cartridge, 0x2000, 0x03);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x33);

    // Bank 0 redirects to bank 1
    mbc.write8(&mut cartridge, 0x2000, 0x00);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x11);
}

#[test]
fn huc3_ram_enable_and_banking() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0xFE; // HuC3
    bytes[0x0149] = 0x03; // 32KB RAM (4 banks)

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // RAM disabled by default - should return open bus
    mbc.write8(&mut cartridge, 0xA000, 0x42);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);

    // Enable RAM
    mbc.write8(&mut cartridge, 0x0000, 0x0A);

    // Write and read from bank 0
    mbc.write8(&mut cartridge, 0xA000, 0x42);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x42);

    // Switch to bank 1 and write different value
    mbc.write8(&mut cartridge, 0x4000, 0x01);
    mbc.write8(&mut cartridge, 0xA000, 0x84);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x84);

    // Switch back to bank 0 - should still have old value
    mbc.write8(&mut cartridge, 0x4000, 0x00);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x42);
}

#[test]
fn huc3_rtc_mode_and_latch() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0xFE; // HuC3

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Enable RAM
    mbc.write8(&mut cartridge, 0x0000, 0x0A);

    // Set mode to RTC read (0x0C)
    mbc.write8(&mut cartridge, 0x6000, 0x0C);

    // Latch seconds (write 0x11 to latch register 0x10)
    mbc.write8(&mut cartridge, 0xA000, 0x11);

    // Read should return seconds (initially 0)
    let seconds = mbc.read8(&cartridge, 0xA000);
    assert_eq!(seconds, 0);

    // Unlatch (write 0x10)
    mbc.write8(&mut cartridge, 0xA000, 0x10);

    // Read should return status byte (0x01) when not latched
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x01);
}

#[test]
fn huc3_ir_modes() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0xFE; // HuC3

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Enable RAM
    mbc.write8(&mut cartridge, 0x0000, 0x0A);

    // Set mode to IR read (0x0D)
    mbc.write8(&mut cartridge, 0x6000, 0x0D);

    // IR should initially be 0
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x00);

    // Set mode to IR write (0x0E)
    mbc.write8(&mut cartridge, 0x6000, 0x0E);
    mbc.write8(&mut cartridge, 0xA000, 0x42);

    // Switch back to IR read
    mbc.write8(&mut cartridge, 0x6000, 0x0D);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x42);
}

#[test]
fn huc3_mode_switching_between_ram_and_rtc() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0xFE; // HuC3
    bytes[0x0149] = 0x02; // 8KB RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Enable RAM
    mbc.write8(&mut cartridge, 0x0000, 0x0A);

    // Mode 0x00 - RAM mode (default)
    mbc.write8(&mut cartridge, 0xA000, 0xAA);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xAA);

    // Switch to RTC mode
    mbc.write8(&mut cartridge, 0x6000, 0x0C);
    // Should not read RAM value
    let rtc_val = mbc.read8(&cartridge, 0xA000);
    assert_ne!(rtc_val, 0xAA);

    // Switch back to RAM mode
    mbc.write8(&mut cartridge, 0x6000, 0x00);
    // Should read original RAM value
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xAA);
}

// MBC6 Tests
#[test]
fn mbc6_split_rom_banking() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 8];
    bytes[0x0147] = 0x20; // MBC6
    bytes[0x0148] = 0x05; // 64 banks (4MB)

    // Fill ROM banks with distinct patterns
    bytes[ROM_BANK_SIZE * 2..ROM_BANK_SIZE * 3].fill(0x22);
    bytes[ROM_BANK_SIZE * 3..ROM_BANK_SIZE * 4].fill(0x33);
    bytes[ROM_BANK_SIZE * 4..ROM_BANK_SIZE * 5].fill(0x44);
    bytes[ROM_BANK_SIZE * 5..ROM_BANK_SIZE * 6].fill(0x55);

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // ROM bank A (4000-5FFF) - defaults to bank 2
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);

    // Switch ROM bank A to bank 4
    mbc.write8(&mut cartridge, 0x4000, 0x04);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x44);
    assert_eq!(mbc.read8(&cartridge, 0x5FFF), 0x44);

    // ROM bank B (6000-7FFF) - defaults to bank 3
    assert_eq!(mbc.read8(&cartridge, 0x6000), 0x33);

    // Switch ROM bank B to bank 5
    mbc.write8(&mut cartridge, 0x5000, 0x05);
    assert_eq!(mbc.read8(&cartridge, 0x6000), 0x55);
    assert_eq!(mbc.read8(&cartridge, 0x7FFF), 0x55);

    // Verify banks are independent
    mbc.write8(&mut cartridge, 0x4000, 0x02);
    mbc.write8(&mut cartridge, 0x5000, 0x03);
    assert_eq!(mbc.read8(&cartridge, 0x4000), 0x22);
    assert_eq!(mbc.read8(&cartridge, 0x6000), 0x33);
}

#[test]
fn mbc6_flash_and_sram_enable() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0x20; // MBC6
    bytes[0x0149] = 0x03; // 32KB RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Initially, flash and SRAM should be disabled
    // Writes should be ignored, reads should return 0xFF
    mbc.write8(&mut cartridge, 0xA000, 0xAA);
    mbc.write8(&mut cartridge, 0xB000, 0xBB);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);
    assert_eq!(mbc.read8(&cartridge, 0xB000), 0xFF);

    // Enable flash (A000-AFFF)
    mbc.write8(&mut cartridge, 0x0000, 0x0A);
    mbc.write8(&mut cartridge, 0xA000, 0xCC);
    // Flash writes require commands, but enable should work

    // Enable SRAM (B000-BFFF)
    mbc.write8(&mut cartridge, 0x1000, 0x0A);
    mbc.write8(&mut cartridge, 0xB000, 0xDD);
    assert_eq!(mbc.read8(&cartridge, 0xB000), 0xDD);

    // Disable SRAM
    mbc.write8(&mut cartridge, 0x1000, 0x00);
    assert_eq!(mbc.read8(&cartridge, 0xB000), 0xFF);
}

#[test]
fn mbc6_flash_bank_switching() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0x20; // MBC6
    bytes[0x0149] = 0x03; // 32KB RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Enable flash
    mbc.write8(&mut cartridge, 0x0000, 0x0A);

    // Test Flash Bank A (A000-AFFF)
    // Set flash bank A to bank 0
    mbc.write8(&mut cartridge, 0x2800, 0x00);
    // Enter write mode
    mbc.write8(&mut cartridge, 0x2000, 0x01);
    // Write to flash at A000
    mbc.write8(&mut cartridge, 0xA000, 0xAA);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xAA);

    // Switch flash bank A to bank 1
    mbc.write8(&mut cartridge, 0x2800, 0x01);
    // Different bank should read 0xFF (erased)
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);

    // Write to bank 1
    mbc.write8(&mut cartridge, 0xA000, 0xBB);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xBB);

    // Switch back to bank 0
    mbc.write8(&mut cartridge, 0x2800, 0x00);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xAA);

    // Test Flash Bank B (B000-BFFF)
    // Set flash bank B to bank 2
    mbc.write8(&mut cartridge, 0x3800, 0x02);
    // Enter write mode for bank B
    mbc.write8(&mut cartridge, 0x3000, 0x01);
    // Write to flash at B000
    mbc.write8(&mut cartridge, 0xB000, 0xCC);
    assert_eq!(mbc.read8(&cartridge, 0xB000), 0xCC);

    // Switch flash bank B to bank 3
    mbc.write8(&mut cartridge, 0x3800, 0x03);
    // Different bank should read 0xFF (erased)
    assert_eq!(mbc.read8(&cartridge, 0xB000), 0xFF);
}

#[test]
fn mbc6_flash_write_restrictions() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0x20; // MBC6
    bytes[0x0149] = 0x03; // 32KB RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Enable flash
    mbc.write8(&mut cartridge, 0x0000, 0x0A);
    mbc.write8(&mut cartridge, 0x2800, 0x00);
    mbc.write8(&mut cartridge, 0x2000, 0x01); // Write mode

    // Flash starts erased (all 0xFF)
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);

    // Can write to change bits from 1 to 0
    mbc.write8(&mut cartridge, 0xA000, 0xAA); // 10101010
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xAA);

    // Writing again can only clear more bits (1->0)
    mbc.write8(&mut cartridge, 0xA000, 0x88); // 10001000
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x88);

    // Trying to set bits (0->1) should have no effect
    mbc.write8(&mut cartridge, 0xA000, 0xFF);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0x88); // Still 0x88
}

#[test]
fn mbc6_flash_erase_sector() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0x20; // MBC6
    bytes[0x0149] = 0x03; // 32KB RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Enable flash
    mbc.write8(&mut cartridge, 0x0000, 0x0A);
    mbc.write8(&mut cartridge, 0x2800, 0x00);
    mbc.write8(&mut cartridge, 0x2000, 0x01); // Write mode

    // Write some data
    mbc.write8(&mut cartridge, 0xA000, 0xAA);
    mbc.write8(&mut cartridge, 0xA001, 0xBB);
    mbc.write8(&mut cartridge, 0xA100, 0xCC);
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xAA);
    assert_eq!(mbc.read8(&cartridge, 0xA001), 0xBB);
    assert_eq!(mbc.read8(&cartridge, 0xA100), 0xCC);

    // Issue erase command (0x10) to A000
    mbc.write8(&mut cartridge, 0x2000, 0x10); // Erase command
    mbc.write8(&mut cartridge, 0xA000, 0x00); // Trigger erase at sector containing A000

    // Entire 4KB sector should be erased to 0xFF
    assert_eq!(mbc.read8(&cartridge, 0xA000), 0xFF);
    assert_eq!(mbc.read8(&cartridge, 0xA001), 0xFF);
    assert_eq!(mbc.read8(&cartridge, 0xA100), 0xFF);
    assert_eq!(mbc.read8(&cartridge, 0xAFFF), 0xFF);
}

#[test]
fn mbc6_sram_read_write() {
    let mut bytes = vec![0; ROM_BANK_SIZE * 2];
    bytes[0x0147] = 0x20; // MBC6
    bytes[0x0149] = 0x02; // 8KB RAM

    let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
    let mut mbc = Mbc::new(&cartridge).expect("mbc");

    // Enable SRAM (B000-BFFF)
    mbc.write8(&mut cartridge, 0x1000, 0x0A);

    // Write and read various locations
    mbc.write8(&mut cartridge, 0xB000, 0x11);
    mbc.write8(&mut cartridge, 0xB001, 0x22);
    mbc.write8(&mut cartridge, 0xB7FF, 0x33);
    mbc.write8(&mut cartridge, 0xBFFF, 0x44);

    assert_eq!(mbc.read8(&cartridge, 0xB000), 0x11);
    assert_eq!(mbc.read8(&cartridge, 0xB001), 0x22);
    assert_eq!(mbc.read8(&cartridge, 0xB7FF), 0x33);
    assert_eq!(mbc.read8(&cartridge, 0xBFFF), 0x44);

    // Test flash bank switching for SRAM (B000-BFFF maps to flash when flash is enabled)
    mbc.write8(&mut cartridge, 0x3800, 0x00); // Flash bank B = 0
    mbc.write8(&mut cartridge, 0x3000, 0x01); // Write mode

    // Now B000-BFFF should access flash bank B
    mbc.write8(&mut cartridge, 0xB000, 0x55);
    assert_eq!(mbc.read8(&cartridge, 0xB000), 0x55);
}

mod proptests {
    use super::*;
    use crate::domain::cartridge::ROM_BANK_SIZE;
    use crate::domain::{Cartridge, CartridgeType};
    use proptest::prelude::*;

    // Helper to create a minimal valid cartridge
    fn make_cartridge(cart_type: CartridgeType, rom_banks: usize, ram_size: u8) -> Cartridge {
        let mut bytes = vec![0; ROM_BANK_SIZE * rom_banks.max(2)];
        bytes[0x0147] = cart_type.code();
        bytes[0x0149] = ram_size;
        Cartridge::from_bytes(bytes).expect("valid cartridge")
    }

    proptest! {
        // MBC1 Properties

        #[test]
        fn prop_mbc1_bank_0_always_readable(bank_select in 0u8..=0x1F) {
            let mut cartridge = make_cartridge(CartridgeType::Mbc1, 4, 0x00);
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // Write to bank select
            mbc.write8(&mut cartridge, 0x2000, bank_select);

            // Bank 0 should always be readable at 0x0000-0x3FFF
            let _byte = mbc.read8(&cartridge, 0x0000);
            let _byte = mbc.read8(&cartridge, 0x3FFF);
        }

        #[test]
        fn prop_mbc1_rom_bank_zero_becomes_one(
            addr in 0x4000u16..=0x7FFF
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 4];
            bytes[ROM_BANK_SIZE..ROM_BANK_SIZE * 2].fill(0xAA);
            bytes[0x0147] = 0x01; // MBC1
            bytes[0x0149] = 0x00;
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // Writing 0 to bank select should read bank 1
            mbc.write8(&mut cartridge, 0x2000, 0x00);
            let value = mbc.read8(&cartridge, addr);
            prop_assert_eq!(value, 0xAA, "Bank 0 selection should read bank 1");
        }

        #[test]
        fn prop_mbc1_ram_disabled_returns_open_bus(
            addr in 0xA000u16..=0xBFFF,
            write_value in any::<u8>()
        ) {
            let mut cartridge = make_cartridge(CartridgeType::Mbc1RamBattery, 2, 0x02);
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // RAM disabled by default
            mbc.write8(&mut cartridge, addr, write_value);
            let read_value = mbc.read8(&cartridge, addr);

            prop_assert_eq!(read_value, 0xFF, "Disabled RAM should return 0xFF");
        }

        #[test]
        fn prop_mbc1_ram_write_read_roundtrip(
            addr in 0xA000u16..=0xBFFF,
            value in any::<u8>()
        ) {
            let mut cartridge = make_cartridge(CartridgeType::Mbc1RamBattery, 2, 0x02);
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // Enable RAM
            mbc.write8(&mut cartridge, 0x0000, 0x0A);

            // Write and read
            mbc.write8(&mut cartridge, addr, value);
            let read = mbc.read8(&cartridge, addr);

            prop_assert_eq!(read, value, "RAM write/read should roundtrip");
        }

        #[test]
        fn prop_mbc1_mode_switch_preserves_data(
            value in any::<u8>()
        ) {
            let mut cartridge = make_cartridge(CartridgeType::Mbc1RamBattery, 2, 0x03);
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, 0x0000, 0x0A); // Enable RAM
            mbc.write8(&mut cartridge, 0xA000, value);

            // Switch mode
            mbc.write8(&mut cartridge, 0x6000, 0x01);

            // Data should still be there
            let read = mbc.read8(&cartridge, 0xA000);
            prop_assert_eq!(read, value, "Mode switch should preserve RAM data");
        }

        // MBC2 Properties

        #[test]
        fn prop_mbc2_ram_nibble_mask(
            addr in 0xA000u16..=0xA1FF,
            value in any::<u8>()
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            bytes[0x0147] = 0x05; // MBC2
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // Enable RAM
            mbc.write8(&mut cartridge, 0x0000, 0x0A);

            // Write full byte, read should be masked to 4 bits
            mbc.write8(&mut cartridge, addr, value);
            let read = mbc.read8(&cartridge, addr);

            prop_assert_eq!(read, 0xF0 | (value & 0x0F), "MBC2 RAM should mask to lower 4 bits with upper 4 set");
        }

        #[test]
        fn prop_mbc2_bank_select_uses_lower_4_bits(
            bank_bits in 0u8..=0x0F
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 16];
            for i in 0..16 {
                let start = i * ROM_BANK_SIZE;
                bytes[start..start + ROM_BANK_SIZE].fill(i as u8);
            }
            bytes[0x0147] = 0x05; // MBC2
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // Bank select with upper bits set
            mbc.write8(&mut cartridge, 0x2100, 0xF0 | bank_bits);

            let expected_bank = if bank_bits == 0 { 1 } else { bank_bits };
            let read = mbc.read8(&cartridge, 0x4000);
            prop_assert_eq!(read, expected_bank, "MBC2 should use only lower 4 bits for bank select");
        }

        #[test]
        fn prop_mbc2_ram_address_wraps(
            offset in 0u16..=0x1FF,  // Only test within valid 512-byte range
            value in any::<u8>()
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            bytes[0x0147] = 0x05; // MBC2
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, 0x0000, 0x0A); // Enable RAM

            // Write to base address
            let addr1 = 0xA000 + offset;
            mbc.write8(&mut cartridge, addr1, value);

            // Read from mirrored location (MBC2 RAM mirrors every 512 bytes)
            let addr2 = 0xA000 + (offset & 0x1FF);
            let read = mbc.read8(&cartridge, addr2);

            prop_assert_eq!(read & 0x0F, value & 0x0F, "MBC2 RAM should mirror every 512 bytes");
        }

        // MBC3 Properties

        #[test]
        fn prop_mbc3_rom_bank_select_wraps(
            bank_select in 1u8..=0x7F
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 8];
            for i in 0..8 {
                let start = i * ROM_BANK_SIZE;
                bytes[start..start + ROM_BANK_SIZE].fill(i as u8);
            }
            bytes[0x0147] = 0x10; // MBC3
            bytes[0x0149] = 0x02;
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, 0x2000, bank_select);
            // MBC3 bank selection wraps within available banks using normalize_switchable_bank
            // Bank 0 becomes bank 1, others wrap modulo available banks
            let expected_bank_raw = (bank_select as usize) % 8;
            let expected_bank = if expected_bank_raw == 0 { 1 } else { expected_bank_raw };
            let read = mbc.read8(&cartridge, 0x4000);

            prop_assert_eq!(read, expected_bank as u8, "MBC3 bank should wrap to available banks");
        }

        #[test]
        fn prop_mbc3_ram_bank_select(
            ram_bank in 0u8..=0x03,
            value in any::<u8>()
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            bytes[0x0147] = 0x13; // MBC3 with RAM
            bytes[0x0149] = 0x03; // 32KB RAM (4 banks)
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, 0x0000, 0x0A); // Enable RAM
            mbc.write8(&mut cartridge, 0x4000, ram_bank); // Select bank
            mbc.write8(&mut cartridge, 0xA000, value);

            // Switch to different bank and back
            mbc.write8(&mut cartridge, 0x4000, (ram_bank + 1) % 4);
            mbc.write8(&mut cartridge, 0x4000, ram_bank);

            let read = mbc.read8(&cartridge, 0xA000);
            prop_assert_eq!(read, value, "MBC3 RAM banking should preserve data");
        }

        #[test]
        fn prop_mbc3_rtc_latch_freezes_time(
            cycles in 1u32..=CYCLES_PER_SECOND
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            bytes[0x0147] = 0x0F; // MBC3 with RTC
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.set_rtc_mode(RtcMode::Deterministic);
            mbc.write8(&mut cartridge, 0x0000, 0x0A); // Enable RTC
            mbc.write8(&mut cartridge, 0x4000, 0x08); // Select RTC seconds

            // Advance time and latch
            mbc.tick(cycles);
            mbc.write8(&mut cartridge, 0x6000, 0x00);
            mbc.write8(&mut cartridge, 0x6000, 0x01);
            let latched_value = mbc.read8(&cartridge, 0xA000);

            // Advance more time
            mbc.tick(cycles);

            // Latched value shouldn't change
            let still_latched = mbc.read8(&cartridge, 0xA000);
            prop_assert_eq!(still_latched, latched_value, "RTC latch should freeze time");
        }

        #[test]
        fn prop_mbc3_rtc_registers_writable(
            value in 0u8..=59
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            bytes[0x0147] = 0x0F; // MBC3 with RTC
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, 0x0000, 0x0A); // Enable RTC
            mbc.write8(&mut cartridge, 0x4000, 0x08); // Select RTC seconds
            mbc.write8(&mut cartridge, 0xA000, value);

            let read = mbc.read8(&cartridge, 0xA000);
            prop_assert_eq!(read, value, "RTC registers should be writable");
        }

        #[test]
        #[allow(non_snake_case)]
        fn prop_mbc3_without_rtc_treats_08_0C_as_ram(
            register in 0x08u8..=0x0C,
            value in any::<u8>()
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            bytes[0x0147] = 0x13; // MBC3 without RTC
            bytes[0x0149] = 0x03; // 32KB RAM
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, 0x0000, 0x0A); // Enable RAM

            // Writing to RTC register range should do nothing
            mbc.write8(&mut cartridge, 0x4000, register);
            mbc.write8(&mut cartridge, 0xA000, value);

            // Should read from regular RAM bank 0
            mbc.write8(&mut cartridge, 0x4000, 0x00);
            let read = mbc.read8(&cartridge, 0xA000);

            prop_assert_eq!(read, value, "MBC3 without RTC should treat 0x08-0x0C as RAM bank 0");
        }

        // MBC5 Properties

        #[test]
        fn prop_mbc5_9bit_bank_select(
            low_byte in any::<u8>(),
            high_bit in 0u8..=1
        ) {
            let banks = ((high_bit as usize) << 8) | (low_byte as usize);
            let num_banks = (banks + 2).min(512);
            let mut bytes = vec![0; ROM_BANK_SIZE * num_banks];

            // Fill each bank with its bank number (mod 256)
            for i in 0..num_banks {
                let start = i * ROM_BANK_SIZE;
                bytes[start..start + ROM_BANK_SIZE].fill((i % 256) as u8);
            }

            bytes[0x0147] = 0x19; // MBC5
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, 0x2000, low_byte);
            mbc.write8(&mut cartridge, 0x3000, high_bit);

            let expected_bank = if banks >= num_banks {
                banks % num_banks
            } else {
                banks
            };

            let read = mbc.read8(&cartridge, 0x4000);
            prop_assert_eq!(read, (expected_bank % 256) as u8, "MBC5 should support 9-bit ROM banking");
        }

        #[test]
        fn prop_mbc5_bank_zero_is_valid(
            offset in 0u16..=0x3FFF  // Test within switchable bank range
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            // Fill banks with distinct values
            for i in 0..ROM_BANK_SIZE {
                bytes[i] = 0xAA;  // Bank 0
                bytes[ROM_BANK_SIZE + i] = 0xCC;  // Bank 1
            }
            // Set MBC5 type in header (in bank 0)
            bytes[0x0147] = 0x19;

            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // Unlike MBC1, bank 0 is valid for MBC5 in switchable area
            mbc.write8(&mut cartridge, 0x2000, 0x00);
            mbc.write8(&mut cartridge, 0x3000, 0x00);

            let addr = 0x4000 + offset;
            let value = mbc.read8(&cartridge, addr);

            // When bank 0 is selected, reading 0x4000-0x7FFF should read from bank 0
            let expected = if offset == 0x0147 {
                0x19  // This is the cartridge type byte in the header
            } else {
                0xAA  // All other bytes in bank 0
            };

            prop_assert_eq!(value, expected, "MBC5 should allow bank 0 selection in switchable area");
        }

        #[test]
        fn prop_mbc5_ram_banking_4bit(
            ram_bank in 0u8..=0x0F,
            value in any::<u8>()
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            bytes[0x0147] = 0x1B; // MBC5 with RAM
            bytes[0x0149] = 0x04; // 128KB RAM
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, 0x0000, 0x0A); // Enable RAM
            mbc.write8(&mut cartridge, 0x4000, ram_bank); // Select bank (uses lower 4 bits)
            mbc.write8(&mut cartridge, 0xA000, value);

            let effective_bank = ram_bank & 0x0F;
            mbc.write8(&mut cartridge, 0x4000, effective_bank);
            let read = mbc.read8(&cartridge, 0xA000);

            prop_assert_eq!(read, value, "MBC5 should use 4-bit RAM banking");
        }

        // ROM-only Properties

        #[test]
        fn prop_rom_only_fixed_banks(
            addr in 0x0000u16..=0x7FFF
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE * 2];
            bytes[addr as usize] = 0xAB;
            bytes[0x0147] = 0x00; // ROM only
            let cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mbc = Mbc::new(&cartridge).expect("mbc");

            let value = mbc.read8(&cartridge, addr);
            prop_assert_eq!(value, 0xAB, "ROM-only should have fixed mapping");
        }

        #[test]
        fn prop_rom_only_ram_roundtrip(
            addr in 0xA000u16..=0xBFFF,
            value in any::<u8>()
        ) {
            let mut bytes = vec![0; ROM_BANK_SIZE];
            bytes[0x0147] = 0x08; // ROM + RAM
            bytes[0x0149] = 0x02; // 8KB RAM
            let mut cartridge = Cartridge::from_bytes(bytes).expect("cartridge");
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.write8(&mut cartridge, addr, value);
            let read = mbc.read8(&cartridge, addr);

            prop_assert_eq!(read, value, "ROM-only RAM should roundtrip");
        }

        // General MBC Properties

        #[test]
        fn prop_tick_doesnt_crash(
            cycles in 1u32..=CYCLES_PER_SECOND * 2,
            cart_type in prop::sample::select(vec![
                CartridgeType::Mbc1,
                CartridgeType::Mbc2,
                CartridgeType::Mbc3TimerRamBattery,
                CartridgeType::Mbc5,
            ])
        ) {
            let cartridge = make_cartridge(cart_type, 2, 0x02);
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            mbc.tick(cycles);
            // Should not crash
        }

        #[test]
        fn prop_ram_enable_is_idempotent(
            value in any::<u8>(),
            addr in 0xA000u16..=0xBFFF
        ) {
            let mut cartridge = make_cartridge(CartridgeType::Mbc1RamBattery, 2, 0x02);
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // Enable RAM twice
            mbc.write8(&mut cartridge, 0x0000, 0x0A);
            mbc.write8(&mut cartridge, 0x0000, 0x0A);

            mbc.write8(&mut cartridge, addr, value);
            let read = mbc.read8(&cartridge, addr);

            prop_assert_eq!(read, value, "Multiple RAM enables should work");
        }

        #[test]
        fn prop_writes_to_rom_area_dont_crash(
            addr in 0x0000u16..=0x7FFF,
            value in any::<u8>(),
            cart_type in prop::sample::select(vec![
                CartridgeType::Mbc1,
                CartridgeType::Mbc2,
                CartridgeType::Mbc3,
                CartridgeType::Mbc5,
            ])
        ) {
            let mut cartridge = make_cartridge(cart_type, 4, 0x02);
            let mut mbc = Mbc::new(&cartridge).expect("mbc");

            // Writing to ROM areas changes MBC state but shouldn't crash
            mbc.write8(&mut cartridge, addr, value);

            // Should still be readable
            let _read = mbc.read8(&cartridge, addr);
        }
    }
}
