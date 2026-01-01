use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use craterboy::domain::{Bus, CartridgeType, RomSize, RtcMode};
use craterboy::infrastructure::rom_loader;

const CYCLES_PER_SECOND: u32 = 4_194_304;
static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("roms")
        .join(name)
}

fn temp_save_root() -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let filename = format!("craterboy_fixture_saves_{}_{}", std::process::id(), id);
    std::env::temp_dir().join(filename)
}

fn load_fixture(name: &str) -> craterboy::domain::Cartridge {
    let path = fixture_path(name);
    let save_root = temp_save_root();
    rom_loader::load_rom_with_save_root(&path, Some(save_root.as_path()))
        .unwrap_or_else(|err| panic!("load fixture {name}: {err:?}"))
}

#[test]
fn rom_only_fixture_reads_bank0_and_bank1() {
    let cartridge = load_fixture("rom_only_32k.gb");
    assert_eq!(cartridge.header.cartridge_type, CartridgeType::RomOnly);
    assert_eq!(cartridge.header.rom_size, RomSize::Kb32);

    let bus = Bus::new(cartridge).expect("bus");
    assert_eq!(bus.read8(0x0000), 0x00);
    assert_eq!(bus.read8(0x4000), 0x01);
}

#[test]
fn mbc1_fixture_switches_rom_banks() {
    let cartridge = load_fixture("mbc1_64k.gb");
    assert_eq!(cartridge.header.cartridge_type, CartridgeType::Mbc1);
    assert_eq!(cartridge.header.rom_size, RomSize::Kb64);

    let mut bus = Bus::new(cartridge).expect("bus");
    assert_eq!(bus.read8(0x4000), 0x01);

    bus.write8(0x2000, 0x02);
    assert_eq!(bus.read8(0x4000), 0x02);
}

#[test]
fn mbc2_fixture_rom_and_ram_rules() {
    let cartridge = load_fixture("mbc2_64k.gb");
    assert_eq!(cartridge.header.cartridge_type, CartridgeType::Mbc2);
    assert_eq!(cartridge.header.rom_size, RomSize::Kb64);

    let mut bus = Bus::new(cartridge).expect("bus");
    assert_eq!(bus.read8(0x4000), 0x01);

    bus.write8(0x2100, 0x03);
    assert_eq!(bus.read8(0x4000), 0x03);

    bus.write8(0xA000, 0xAB);
    assert_eq!(bus.read8(0xA000), 0xFF);
    bus.write8(0x0000, 0x0A);
    bus.write8(0xA000, 0xAB);
    assert_eq!(bus.read8(0xA000), 0xFB);
}

#[test]
fn mbc3_rtc_fixture_ticks_seconds() {
    let cartridge = load_fixture("mbc3_rtc_128k.gb");
    assert_eq!(
        cartridge.header.cartridge_type,
        CartridgeType::Mbc3TimerRamBattery
    );
    assert_eq!(cartridge.header.rom_size, RomSize::Kb128);

    let mut bus = Bus::new(cartridge).expect("bus");
    bus.set_rtc_mode(RtcMode::Deterministic);
    bus.write8(0x0000, 0x0A);
    bus.write8(0x4000, 0x08);
    bus.step(CYCLES_PER_SECOND);

    assert_eq!(bus.read8(0xA000), 0x01);
}

#[test]
fn mbc5_fixture_switches_rom_banks() {
    let cartridge = load_fixture("mbc5_128k.gb");
    assert_eq!(
        cartridge.header.cartridge_type,
        CartridgeType::Mbc5RamBattery
    );
    assert_eq!(cartridge.header.rom_size, RomSize::Kb128);

    let mut bus = Bus::new(cartridge).expect("bus");
    assert_eq!(bus.read8(0x4000), 0x01);

    bus.write8(0x2000, 0x02);
    bus.write8(0x3000, 0x00);
    assert_eq!(bus.read8(0x4000), 0x02);
}
