# AGENTS.md - Craterboy Game Boy Emulator

Guidelines for AI coding agents working on this Rust Game Boy emulator codebase.

## Build & Test Commands

```bash
# Build the project
cargo build

# Run all tests
cargo test

# Run a single test by name
cargo test test_name
cargo test cpu_executes_nop

# Run tests in a specific module
cargo test domain::cpu::tests
cargo test domain::bus::tests

# Run tests with output
cargo test -- --nocapture

# Format code (required before commit)
cargo fmt --all

# Lint with clippy (required before commit)
cargo clippy --all-targets --all-features

# Run the emulator
cargo run -- --gui path/to/rom.gb
cargo run -- --gui --boot-rom path/to/boot.bin path/to/rom.gb
```

## Pre-commit Hooks (lefthook)

The project uses lefthook for git hooks:
- **pre-commit**: `cargo fmt --all` and `cargo clippy --all-targets --all-features`
- **pre-push**: `cargo test`

Always run `cargo fmt` and `cargo clippy` before committing.

## Project Architecture

This is a clean architecture Game Boy emulator with four layers:

```
src/
  domain/       # Core emulation logic (CPU, PPU, Bus, MBC, Cartridge)
  application/  # Application services, ROM loading orchestration
  infrastructure/  # File I/O, persistence, ROM loading
  interface/    # CLI and GUI entry points
```

### Layer Dependencies
- `domain` has no dependencies on other layers
- `application` depends on `domain` and `infrastructure`
- `infrastructure` depends on `domain`
- `interface` depends on all layers

### Key Domain Components
- `Cpu` - Sharp LR35902 CPU emulation with cycle-accurate timing
- `Bus` - Memory bus handling ROM, RAM, VRAM, I/O registers
- `Ppu` - Pixel Processing Unit for graphics
- `Mbc` - Memory Bank Controllers (MBC1, MBC2, MBC3 with RTC, MBC5)
- `Cartridge` - ROM and RAM storage with header parsing
- `Emulator` - High-level orchestration of CPU, Bus, PPU

## Code Style Guidelines

### Imports
- Use `super::` for sibling module imports within the same layer
- Use `crate::` for cross-layer imports
- Group imports: std first, then external crates, then internal modules
- Re-export public types from `mod.rs` files

```rust
// In domain/cpu.rs
use super::Bus;

// In infrastructure/rom_loader.rs  
use crate::domain::{Cartridge, RomHeaderError};
```

### Constants
- Use `SCREAMING_SNAKE_CASE` for constants
- Hardware registers use hex addresses: `const REG_LCDC: u16 = 0xFF40;`
- Memory sizes and timing constants at module top

```rust
const VRAM_SIZE: usize = 0x2000;
const CYCLES_PER_SECOND: u32 = 4_194_304;
const REG_IF: u16 = 0xFF0F;
const FLAG_Z: u8 = 0x80;
```

### Naming Conventions
- Structs: `PascalCase` (e.g., `Cpu`, `RomHeader`, `Framebuffer`)
- Functions/methods: `snake_case` (e.g., `read8`, `write8`, `step_frame`)
- Enum variants: `PascalCase` (e.g., `CartridgeType::Mbc1RamBattery`)
- Private fields: `snake_case`, no prefix
- Register accessors: named after register (e.g., `fn a(&self)`, `fn set_a(&mut self, value: u8)`)

### Error Handling
- Define custom error enums per module
- Use `#[derive(Debug, Clone, Copy, PartialEq, Eq)]` for simple error types
- Implement `From` traits for error conversion
- Return `Result<T, Error>` for fallible operations

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuError {
    UnimplementedOpcode(u8),
    UnimplementedCbOpcode(u8),
}

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
```

### Struct Design
- Use `pub` fields sparingly; prefer accessor methods
- Implement `new()` for constructors
- Use `#[derive(Debug)]` on all structs
- Add `Clone, Copy` for small value types

```rust
#[derive(Debug)]
pub struct Cpu {
    regs: Registers,
    pc: u16,
    sp: u16,
    ime: bool,
}

impl Cpu {
    pub fn new() -> Self { ... }
    pub fn pc(&self) -> u16 { self.pc }
    pub fn set_pc(&mut self, value: u16) { self.pc = value; }
}
```

### Match Expressions
- Use match for memory address decoding and opcode dispatch
- Use range patterns: `0x0000..=0x7FFF`
- Group related opcodes together

```rust
match addr {
    0x0000..=0x7FFF => self.mbc.read8(&self.cartridge, addr),
    0x8000..=0x9FFF => self.vram[(addr as usize - 0x8000) % VRAM_SIZE],
    0xFFFF => self.interrupt_enable,
}
```

### Testing
- Tests live in `#[cfg(test)] mod tests` at end of each file
- Use helper functions for test setup (e.g., `bus_with_rom()`)
- Test names: `test_` prefix or descriptive `cpu_executes_nop` style
- Integration tests in `tests/` directory use fixtures from `tests/fixtures/`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn bus_with_rom(mut rom: Vec<u8>) -> Bus {
        // Setup helper
    }

    #[test]
    fn cpu_executes_nop() {
        let rom = vec![0x00; ROM_BANK_SIZE];
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();
        let cycles = cpu.step(&mut bus).expect("step");
        assert_eq!(cycles, 4);
    }
}
```

### Documentation
- Add doc comments for public APIs
- Explain hardware behavior in comments for emulation code
- Reference Game Boy documentation where applicable

## Common Patterns

### Memory Read/Write
```rust
pub fn read8(&self, addr: u16) -> u8 { ... }
pub fn write8(&mut self, addr: u16, value: u8) { ... }
```

### Cycle Stepping
```rust
pub fn step(&mut self, cycles: u32) { ... }
```

### State Application
```rust
pub fn apply_post_boot_state(&mut self) { ... }
```

## Dependencies

- `serde` + `bincode`: Serialization for save states
- `wgpu`: GPU rendering
- `winit`: Window management
- `ab_glyph`: Font rendering
- `pollster`: Async runtime for wgpu

## Rust Edition

This project uses **Rust 2024 edition**. Use modern Rust idioms and features.
