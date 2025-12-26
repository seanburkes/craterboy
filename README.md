# Craterboy

Craterboy is a Rust-based Game Boy emulator focused on accuracy, performance, and a clean separation between core emulation and the frontend. The goal is a headless, testable core with a thin UI shell that runs well on desktop and Raspberry Pi handhelds.

## Goals

- Accurate DMG/CGB emulation with tight timing and hardware quirks.
- Clear core/frontend split so the emulator core is independent and testable.
- Modern UI with ROM management, overlays, and debugger panes.
- Save states, rewind, and structured logging.
- Strong testing culture with ROM suites, property tests, and fuzzing.

## Tech Stack (Planned)

- Rust for the emulator core and tooling.
- `winit` for the window/event loop.
- `wgpu` for GPU-backed frame presentation and filters.
- Slint for UI (ROM picker, HUD, debug overlays).
- `cpal` for audio output.
- `gilrs` for optional gamepad input.
- `proptest` and `cargo fuzz` for reliability testing.

## Roadmap

- FR0001: Bootable DMG display slice (winit/wgpu frame output).
- FR0002: Input + frame pacing.
- FR0003: Audio output.
- FR0004: Cartridge + save data support.
- FR0005: Slint ROM manager and HUD.
- FR0008: Reliability harness (ROM tests, property tests, fuzzers).

## Status

Early planning and scaffolding. Expect rapid iteration as the core and frontend take shape.
