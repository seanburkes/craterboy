pub const FRAME_WIDTH: usize = 160;
pub const FRAME_HEIGHT: usize = 144;
pub const FRAME_CHANNELS: usize = 3;
pub const FRAME_SIZE: usize = FRAME_WIDTH * FRAME_HEIGHT * FRAME_CHANNELS;

#[derive(Debug, Clone)]
pub struct Framebuffer {
    pixels: Vec<u8>,
}

impl Framebuffer {
    pub fn new() -> Self {
        Self {
            pixels: vec![0; FRAME_SIZE],
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.pixels
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    pub fn len(&self) -> usize {
        self.pixels.len()
    }
}
