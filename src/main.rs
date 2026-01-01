fn main() {
    let mut args = std::env::args();
    let _program = args.next();
    let mut gui = false;
    let mut rom_path: Option<std::path::PathBuf> = None;

    for arg in args {
        if arg == "--gui" {
            gui = true;
            continue;
        }
        if gui && rom_path.is_none() && !arg.starts_with('-') {
            rom_path = Some(std::path::PathBuf::from(arg));
        }
    }

    if gui {
        craterboy::interface::gui::run(rom_path);
    } else {
        craterboy::interface::cli::run();
    }
}
