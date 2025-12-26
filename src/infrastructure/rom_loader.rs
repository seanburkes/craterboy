use std::path::Path;

use crate::domain::{Rom, RomHeaderError};

#[derive(Debug)]
pub enum RomLoadError {
    Io(std::io::Error),
    Header(RomHeaderError),
}

impl From<std::io::Error> for RomLoadError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<RomHeaderError> for RomLoadError {
    fn from(err: RomHeaderError) -> Self {
        Self::Header(err)
    }
}

pub fn load_rom(path: impl AsRef<Path>) -> Result<Rom, RomLoadError> {
    let bytes = std::fs::read(path)?;
    Ok(Rom::from_bytes(bytes)?)
}
