use crate::application::app;
use crate::domain::{
    Cartridge, CartridgeType, CgbFlag, Destination, Licensee, RamSize, RomHeader, RomSize, SgbFlag,
    compute_global_checksum, compute_header_checksum, nintendo_logo_matches,
};
use crate::infrastructure::rom_loader::RomLoadError;

pub fn run() {
    let mut args = std::env::args();
    let program = args.next().unwrap_or_else(|| "craterboy".to_string());
    let mut path: Option<String> = None;
    let mut verbose = false;

    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage(&program);
                return;
            }
            "-v" | "--verbose" => {
                verbose = true;
            }
            _ => {
                if path.is_some() {
                    print_usage(&program);
                    std::process::exit(2);
                }
                path = Some(arg);
            }
        }
    }

    let path = match path {
        Some(path) => path,
        None => {
            print_usage(&program);
            std::process::exit(2);
        }
    };

    match app::load_rom(&path) {
        Ok(cartridge) => print_report(&path, &cartridge, verbose),
        Err(err) => {
            report_load_error(&path, err);
            std::process::exit(1);
        }
    }
}

fn print_report(path: &str, cartridge: &Cartridge, verbose: bool) {
    println!("ROM: {}", path);
    println!(
        "File Size: {} bytes ({} KiB)",
        cartridge.bytes.len(),
        cartridge.bytes.len() / 1024
    );
    println!("ROM ID (fnv1a64): {:016X}", fnv1a64(&cartridge.bytes));
    print_header(&cartridge.header);
    print_checks(cartridge);

    if verbose {
        print_header_bytes(&cartridge.bytes);
    }
}

fn print_header(header: &RomHeader) {
    println!("Title: {}", header.title);
    println!("CGB: {}", cgb_flag_label(header.cgb_flag));
    println!("SGB: {}", sgb_flag_label(header.sgb_flag));
    println!(
        "Cartridge Type: {}",
        cartridge_type_label(header.cartridge_type)
    );
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
        RomLoadError::SaveIo(io_err) => {
            eprintln!("Failed to read save data for '{}': {}", path, io_err);
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

fn print_checks(cartridge: &Cartridge) {
    let mut warnings = Vec::new();

    let logo_ok = nintendo_logo_matches(&cartridge.bytes);
    println!("Nintendo Logo: {}", check_label(logo_ok));
    if logo_ok == Some(false) {
        warnings.push("Nintendo logo check failed".to_string());
    }

    let computed_header = compute_header_checksum(&cartridge.bytes);
    match computed_header {
        Some(computed) => {
            let ok = computed == cartridge.header.header_checksum;
            println!(
                "Header Checksum: {} (expected 0x{:02X}, computed 0x{:02X})",
                if ok { "OK" } else { "FAIL" },
                cartridge.header.header_checksum,
                computed
            );
            if !ok {
                warnings.push("Header checksum mismatch".to_string());
            }
        }
        None => {
            println!("Header Checksum: Unknown");
        }
    }

    let computed_global = compute_global_checksum(&cartridge.bytes);
    match computed_global {
        Some(computed) => {
            let ok = computed == cartridge.header.global_checksum;
            println!(
                "Global Checksum: {} (expected 0x{:04X}, computed 0x{:04X})",
                if ok { "OK" } else { "FAIL" },
                cartridge.header.global_checksum,
                computed
            );
            if !ok {
                warnings.push("Global checksum mismatch".to_string());
            }
        }
        None => {
            println!("Global Checksum: Unknown");
        }
    }

    let expected_size = cartridge.header.rom_size.bytes();
    match expected_size {
        Some(expected) => {
            if expected == cartridge.bytes.len() {
                println!(
                    "ROM Size Check: OK (expected {} bytes, file has {} bytes)",
                    expected,
                    cartridge.bytes.len()
                );
            } else {
                println!(
                    "ROM Size Check: FAIL (expected {} bytes, file has {} bytes)",
                    expected,
                    cartridge.bytes.len()
                );
                warnings.push(format!(
                    "ROM size mismatch: header expects {} bytes, file has {} bytes",
                    expected,
                    cartridge.bytes.len()
                ));
            }
        }
        None => {
            println!(
                "ROM Size Check: Unknown (code 0x{:02X})",
                cartridge.header.rom_size.code()
            );
            warnings.push("Unknown ROM size code".to_string());
        }
    }

    if matches!(cartridge.header.ram_size, RamSize::Unknown(_)) {
        warnings.push("Unknown RAM size code".to_string());
    }

    if matches!(cartridge.header.destination, Destination::Unknown(_)) {
        warnings.push("Unknown destination code".to_string());
    }

    if matches!(cartridge.header.cartridge_type, CartridgeType::Unknown(_)) {
        warnings.push("Unknown cartridge type".to_string());
    } else if !cartridge.header.cartridge_type.is_supported() {
        warnings.push(format!(
            "Cartridge type not supported yet: {}",
            cartridge_type_label(cartridge.header.cartridge_type)
        ));
    }

    if !warnings.is_empty() {
        println!("Warnings:");
        for warning in warnings {
            println!("- {}", warning);
        }
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

fn destination_label(destination: Destination) -> String {
    destination.label()
}

fn licensee_label(licensee: &Licensee) -> String {
    licensee.label()
}

fn print_usage(program: &str) {
    eprintln!("Usage: {} [--verbose] <rom-path>", program);
}

fn print_header_bytes(bytes: &[u8]) {
    if bytes.len() < 0x150 {
        return;
    }

    let header_slice = &bytes[0x0134..=0x014F];
    let line = header_slice
        .iter()
        .map(|byte| format!("{:02X}", byte))
        .collect::<Vec<_>>()
        .join(" ");
    println!("Header Bytes (0x0134..0x014F): {}", line);
}

fn check_label(result: Option<bool>) -> &'static str {
    match result {
        Some(true) => "OK",
        Some(false) => "FAIL",
        None => "Unknown",
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x00000100000001B3;

    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}
