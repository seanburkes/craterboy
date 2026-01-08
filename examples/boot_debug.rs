use craterboy::domain::Emulator;
use craterboy::infrastructure::rom_loader;
use std::path::PathBuf;

fn main() {
    // Load boot ROM
    let boot_rom_path = PathBuf::from("third_party/pyboy/pyboy/core/bootrom_dmg.bin");
    let boot_rom = std::fs::read(&boot_rom_path).expect("Failed to load boot ROM");

    // Load Tetris
    let tetris_path = PathBuf::from("games/tetris.gb");
    let save_root = std::env::temp_dir().join("craterboy_debug");
    let cartridge = rom_loader::load_rom_with_save_root(&tetris_path, Some(save_root.as_path()))
        .expect("Failed to load Tetris");

    let mut emulator = Emulator::new();
    emulator
        .load_cartridge_with_boot_rom(cartridge, Some(boot_rom))
        .expect("Failed to load cartridge with boot ROM");

    println!("Starting boot sequence...");
    println!("Initial booted status: {}", emulator.is_booted());

    // Execute frames and check framebuffer
    for frame in 0..70 {
        emulator.step_frame().expect("Failed to step frame");

        // Check if framebuffer has any non-zero pixels
        let fb = emulator.framebuffer();
        let pixels = fb.as_slice();
        let non_zero_count = pixels.iter().filter(|&&b| b != 0).count();

        if frame < 10 || frame % 10 == 0 {
            println!(
                "Frame {}: non-zero pixels = {}/{}, booted = {}",
                frame,
                non_zero_count,
                pixels.len(),
                emulator.is_booted()
            );
        }

        if emulator.is_booted() {
            println!("Boot ROM completed at frame {}", frame);
            break;
        }
    }
}
