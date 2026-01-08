/// Visual inspection test for boot ROM logo display
///
/// This test generates ASCII art representations of the framebuffer
/// during boot ROM execution so developers can visually verify the
/// Nintendo logo is being rendered correctly.
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

/// Convert framebuffer to ASCII art for visual inspection
/// Uses brightness mapping: dark pixels = '#', light pixels = ' '
fn framebuffer_to_ascii(pixels: &[u8], width: usize, height: usize) -> String {
    let mut output = String::new();

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 3;
            if idx + 2 < pixels.len() {
                let r = pixels[idx];
                let g = pixels[idx + 1];
                let b = pixels[idx + 2];

                // Calculate brightness (0-255)
                let brightness = (r as u32 + g as u32 + b as u32) / 3;

                // Map brightness to ASCII characters (4 levels matching DMG 2-bit palette)
                let ch = match brightness {
                    0..=63 => '█',    // Darkest (color 3)
                    64..=127 => '▓',  // Dark (color 2)
                    128..=191 => '▒', // Light (color 1)
                    _ => '░',         // Lightest (color 0)
                };

                output.push(ch);
            }
        }
        output.push('\n');
    }

    output
}

/// Save framebuffer as ASCII to a file
fn save_framebuffer_ascii(
    emulator: &Emulator,
    frame_num: usize,
    output_dir: &std::path::Path,
) -> std::io::Result<()> {
    std::fs::create_dir_all(output_dir)?;

    let framebuffer = emulator.framebuffer();
    let ascii = framebuffer_to_ascii(framebuffer.as_slice(), 160, 144);

    let filename = output_dir.join(format!("frame_{:03}.txt", frame_num));
    std::fs::write(&filename, ascii)?;

    println!("Saved frame {} to {:?}", frame_num, filename);
    Ok(())
}

#[test]
#[ignore] // Run with: cargo test --test boot_logo_visual -- --ignored --nocapture
fn dump_boot_sequence_frames() {
    let boot_rom = load_dmg_boot_rom();
    let cartridge = load_tetris_rom();

    let mut emulator = Emulator::new();
    emulator
        .load_cartridge_with_boot_rom(cartridge, Some(boot_rom))
        .expect("Failed to load cartridge with boot ROM");

    let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("boot_frames");

    println!("\nDumping boot ROM sequence to {:?}", output_dir);
    println!("This will capture ~60 frames showing the Nintendo logo boot animation\n");

    // Dump first 70 frames (boot ROM completes around frame 60)
    for frame in 0..70 {
        emulator.step_frame().expect("Failed to step frame");

        // Save every 5th frame to avoid generating too many files
        if frame % 5 == 0 || frame < 10 {
            save_framebuffer_ascii(&emulator, frame, &output_dir)
                .expect("Failed to save framebuffer");
        }

        if emulator.is_booted() {
            println!("\nBoot ROM completed at frame {}", frame);
            break;
        }
    }

    println!("\nDone! Check {:?} for ASCII art frames", output_dir);
    println!("The Nintendo logo should be visible in frames 10-60");
}

#[test]
fn boot_logo_visible_in_center_region() {
    let boot_rom = load_dmg_boot_rom();
    let cartridge = load_tetris_rom();

    let mut emulator = Emulator::new();
    emulator
        .load_cartridge_with_boot_rom(cartridge, Some(boot_rom))
        .expect("Failed to load cartridge with boot ROM");

    // Execute to frame 30 (middle of boot sequence)
    for _ in 0..30 {
        emulator.step_frame().expect("Failed to step frame");
    }

    // Check that the center region (where logo should be) has dark pixels
    let framebuffer = emulator.framebuffer();
    let pixels = framebuffer.as_slice();

    // Logo should be roughly in the center: rows 40-90, columns 40-120
    let mut dark_pixel_count = 0;
    let mut total_checked = 0;

    for y in 40..90 {
        for x in 40..120 {
            let idx = (y * 160 + x) * 3;
            if idx + 2 < pixels.len() {
                let r = pixels[idx];
                let g = pixels[idx + 1];
                let b = pixels[idx + 2];
                let brightness = (r as u32 + g as u32 + b as u32) / 3;

                if brightness < 128 {
                    // Dark pixel (logo foreground)
                    dark_pixel_count += 1;
                }
                total_checked += 1;
            }
        }
    }

    let dark_percentage = (dark_pixel_count as f64 / total_checked as f64) * 100.0;

    println!(
        "Dark pixels in logo region: {}/{} ({:.1}%)",
        dark_pixel_count, total_checked, dark_percentage
    );

    // The Nintendo logo should have at least 5% dark pixels in this region
    assert!(
        dark_percentage >= 5.0,
        "Expected at least 5% dark pixels in logo region, got {:.1}%",
        dark_percentage
    );
}
