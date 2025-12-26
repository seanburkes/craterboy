use crate::application::app;
use crate::domain::{CgbFlag, RomHeader, SgbFlag};
use crate::infrastructure::rom_loader::RomLoadError;

pub fn run() {
    let mut args = std::env::args();
    let program = args.next().unwrap_or_else(|| "craterboy".to_string());
    let path = match args.next() {
        Some(path) => path,
        None => {
            print_usage(&program);
            std::process::exit(2);
        }
    };

    if path == "-h" || path == "--help" {
        print_usage(&program);
        return;
    }

    if args.next().is_some() {
        print_usage(&program);
        std::process::exit(2);
    }

    match app::load_rom_header(&path) {
        Ok(header) => print_header(&header),
        Err(err) => {
            report_load_error(&path, err);
            std::process::exit(1);
        }
    }
}

fn print_header(header: &RomHeader) {
    println!("Title: {}", header.title);
    println!("CGB: {}", cgb_flag_label(header.cgb_flag));
    println!("SGB: {}", sgb_flag_label(header.sgb_flag));
    println!("Cartridge Type: 0x{:02X}", header.cartridge_type);
    println!("ROM Size Code: 0x{:02X}", header.rom_size_code);
    println!("RAM Size Code: 0x{:02X}", header.ram_size_code);
    println!("Destination Code: 0x{:02X}", header.destination_code);
    println!(
        "New Licensee Code: {}",
        format_licensee_code(header.new_licensee_code)
    );
    println!("Old Licensee Code: 0x{:02X}", header.old_licensee_code);
    println!("Mask ROM Version: 0x{:02X}", header.mask_rom_version);
    println!("Header Checksum: 0x{:02X}", header.header_checksum);
    println!("Global Checksum: 0x{:04X}", header.global_checksum);
}

fn report_load_error(path: &str, err: RomLoadError) {
    match err {
        RomLoadError::Io(io_err) => {
            eprintln!("Failed to read ROM '{}': {}", path, io_err);
        }
        RomLoadError::Header(header_err) => {
            eprintln!("Invalid ROM header for '{}': {:?}", path, header_err);
        }
    }
}

fn cgb_flag_label(flag: CgbFlag) -> String {
    match flag {
        CgbFlag::DmgOnly => "DMG only".to_string(),
        CgbFlag::CgbSupported => "CGB supported".to_string(),
        CgbFlag::CgbOnly => "CGB only".to_string(),
    }
}

fn sgb_flag_label(flag: SgbFlag) -> String {
    match flag {
        SgbFlag::None => "No".to_string(),
        SgbFlag::Supported => "Yes".to_string(),
        SgbFlag::Unknown(value) => format!("Unknown (0x{:02X})", value),
    }
}

fn format_licensee_code(code: [u8; 2]) -> String {
    format!("{:02X}{:02X}", code[0], code[1])
}

fn print_usage(program: &str) {
    eprintln!("Usage: {} <rom-path>", program);
}
