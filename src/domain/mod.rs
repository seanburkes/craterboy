pub mod emulator;
pub mod rom;

pub use emulator::Emulator;
pub use rom::{
    CartridgeType, CgbFlag, Destination, Licensee, RamSize, Rom, RomHeader, RomHeaderError,
    RomSize, SgbFlag, compute_global_checksum, compute_header_checksum, nintendo_logo_matches,
};
