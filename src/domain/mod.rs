pub mod emulator;
pub mod rom;

pub use emulator::Emulator;
pub use rom::{
    compute_global_checksum, compute_header_checksum, nintendo_logo_matches, CartridgeType,
    CgbFlag, Destination, Licensee, RamSize, Rom, RomHeader, RomHeaderError, RomSize, SgbFlag,
};
