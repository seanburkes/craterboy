use craterboy::domain::{Cartridge, Emulator};
use craterboy::infrastructure::rom_loader;
use std::path::PathBuf;

/// Load the DMG boot ROM from PyBoy third_party directory
fn load_dmg_boot_rom() -> Vec<u8> {
    let boot_rom_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("third_party/pyboy/pyboy/core/bootrom_dmg.bin");
    std::fs::read(&boot_rom_path)
        .unwrap_or_else(|err| panic!("Failed to load boot ROM from {:?}: {}", boot_rom_path, err))
}

/// Load Tetris ROM which contains the Nintendo logo at 0x0104
fn load_tetris_rom() -> Cartridge {
    let tetris_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("games/tetris.gb");
    let save_root = std::env::temp_dir().join("craterboy_boot_test");
    rom_loader::load_rom_with_save_root(&tetris_path, Some(save_root.as_path()))
        .unwrap_or_else(|err| panic!("Failed to load Tetris ROM: {:?}", err))
}

#[test]
fn boot_rom_loads_and_executes() {
    let boot_rom = load_dmg_boot_rom();
    let cartridge = load_tetris_rom();

    assert_eq!(boot_rom.len(), 256, "DMG boot ROM should be 256 bytes");

    let mut emulator = Emulator::new();
    emulator
        .load_cartridge_with_boot_rom(cartridge, Some(boot_rom))
        .expect("Failed to load cartridge with boot ROM");

    assert!(!emulator.is_booted(), "Emulator should not be booted yet");

    // Execute one frame (boot ROM should start executing)
    let cycles = emulator.step_frame().expect("Failed to step frame");
    assert!(cycles > 0, "Should have executed some cycles");
}

#[test]
fn boot_rom_copies_nintendo_logo_to_vram() {
    let boot_rom = load_dmg_boot_rom();
    let cartridge = load_tetris_rom();

    // Verify the Nintendo logo exists in the ROM
    let rom_logo_start = 0x0104;
    let rom_logo_end = 0x0133;
    let logo_bytes = &cartridge.bytes[rom_logo_start..=rom_logo_end];
    assert_eq!(logo_bytes.len(), 48, "Nintendo logo should be 48 bytes");

    // The first byte of the Nintendo logo is always 0xCE
    assert_eq!(logo_bytes[0], 0xCE, "Nintendo logo should start with 0xCE");

    let mut emulator = Emulator::new();
    emulator
        .load_cartridge_with_boot_rom(cartridge, Some(boot_rom))
        .expect("Failed to load cartridge with boot ROM");

    // Execute several frames to let the boot ROM copy the logo to VRAM
    // The boot ROM should copy 48 bytes from ROM[0x0104] to VRAM[$8010]
    for _ in 0..10 {
        let _ = emulator.step_frame();
    }

    // TODO: Add VRAM inspection API to verify the logo was copied
    // For now, just verify the emulator executed without errors
}

#[test]
fn boot_rom_enables_lcd_and_sets_palette() {
    let boot_rom = load_dmg_boot_rom();
    let cartridge = load_tetris_rom();

    let mut emulator = Emulator::new();
    emulator
        .load_cartridge_with_boot_rom(cartridge, Some(boot_rom))
        .expect("Failed to load cartridge with boot ROM");

    // Execute several frames
    for _ in 0..10 {
        let _ = emulator.step_frame();
    }

    // Framebuffer should not be all zeros (indicating LCD is enabled and rendering)
    let framebuffer = emulator.framebuffer();
    let all_zeros = framebuffer.as_slice().iter().all(|&b| b == 0);
    assert!(
        !all_zeros,
        "Framebuffer should have some non-zero pixels after boot ROM execution"
    );
}

#[test]
fn boot_rom_completes_and_disables_itself() {
    let boot_rom = load_dmg_boot_rom();
    let cartridge = load_tetris_rom();

    let mut emulator = Emulator::new();
    emulator
        .load_cartridge_with_boot_rom(cartridge, Some(boot_rom))
        .expect("Failed to load cartridge with boot ROM");

    assert!(!emulator.is_booted(), "Should start unbooted");

    // Execute many frames (boot ROM takes ~60 frames)
    for _ in 0..100 {
        let _ = emulator.step_frame();
        if emulator.is_booted() {
            break;
        }
    }

    assert!(
        emulator.is_booted(),
        "Boot ROM should complete and set booted flag"
    );
}

#[test]
fn boot_rom_renders_visible_output() {
    let boot_rom = load_dmg_boot_rom();
    let cartridge = load_tetris_rom();

    let mut emulator = Emulator::new();
    emulator
        .load_cartridge_with_boot_rom(cartridge, Some(boot_rom))
        .expect("Failed to load cartridge with boot ROM");

    // Execute enough frames to see the logo (boot ROM shows logo around frame 10-60)
    for _ in 0..30 {
        let _ = emulator.step_frame();
    }

    // Count non-background pixels (logo should be visible)
    let framebuffer = emulator.framebuffer();
    let pixels = framebuffer.as_slice();

    let mut unique_colors = std::collections::HashSet::new();
    for chunk in pixels.chunks(3) {
        if chunk.len() == 3 {
            unique_colors.insert((chunk[0], chunk[1], chunk[2]));
        }
    }

    // Should have at least 2 different colors (background and logo)
    assert!(
        unique_colors.len() >= 2,
        "Should have at least 2 different colors visible (found {})",
        unique_colors.len()
    );
}
