use crate::application::app;
use crate::domain::{
    CartridgeType, CgbFlag, Destination, Licensee, RamSize, RomHeader, RomSize, SgbFlag,
};
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
    println!("Cartridge Type: {}", cartridge_type_label(header.cartridge_type));
    println!("ROM Size: {}", rom_size_label(header.rom_size));
    println!("RAM Size: {}", ram_size_label(header.ram_size));
    println!("Destination: {}", destination_label(header.destination));
    println!("Licensee: {}", licensee_label(&header.licensee));
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

fn cartridge_type_label(cartridge_type: CartridgeType) -> String {
    format!(
        "0x{:02X} ({})",
        cartridge_type.code(),
        cartridge_type.description()
    )
}

fn rom_size_label(rom_size: RomSize) -> String {
    match (rom_size.bytes(), rom_size.bank_count()) {
        (Some(bytes), Some(banks)) => format!(
            "0x{:02X} ({} KiB, {} banks)",
            rom_size.code(),
            bytes / 1024,
            banks
        ),
        _ => format!("0x{:02X} (Unknown)", rom_size.code()),
    }
}

fn ram_size_label(ram_size: RamSize) -> String {
    match ram_size {
        RamSize::None => format!("0x{:02X} (None)", ram_size.code()),
        _ => match ram_size.bytes() {
            Some(bytes) => format!("0x{:02X} ({} KiB)", ram_size.code(), bytes / 1024),
            None => format!("0x{:02X} (Unknown)", ram_size.code()),
        },
    }
}

fn destination_label(destination: Destination) -> &'static str {
    match destination {
        Destination::Japan => "Japan",
        Destination::NonJapan => "Non-Japan",
        Destination::Unknown(_) => "Unknown",
    }
}

fn licensee_label(licensee: &Licensee) -> String {
    match licensee {
        Licensee::Old(code) => format!("Old {}", format_old_licensee_code(*code)),
        Licensee::New(code) => format!("New {}", format_new_licensee_code(*code)),
    }
}

fn format_old_licensee_code(code: u8) -> String {
    format!("0x{:02X}", code)
}

fn format_new_licensee_code(code: [u8; 2]) -> String {
    if code.iter().all(|byte| byte.is_ascii_graphic()) {
        String::from_utf8_lossy(&code).to_string()
    } else {
        format!("{:02X}{:02X}", code[0], code[1])
    }
}

fn print_usage(program: &str) {
    eprintln!("Usage: {} <rom-path>", program);
}
