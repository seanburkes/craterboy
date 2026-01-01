pub mod bus;
pub mod cartridge;
pub mod emulator;
pub mod framebuffer;
pub mod mbc;
pub mod rom;

pub use bus::Bus;
pub use cartridge::{Cartridge, RomBankMapping, RomBankView};
pub use emulator::Emulator;
pub use framebuffer::{FRAME_CHANNELS, FRAME_HEIGHT, FRAME_SIZE, FRAME_WIDTH, Framebuffer};
pub use mbc::{Mbc, MbcError, RtcMode};
pub use rom::{
    CartridgeType, CgbFlag, Destination, Licensee, RamSize, RomHeader, RomHeaderError, RomSize,
    SgbFlag, compute_global_checksum, compute_header_checksum, nintendo_logo_matches,
};
