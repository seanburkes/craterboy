use super::Bus;

const FLAG_Z: u8 = 0x80;
const FLAG_N: u8 = 0x40;
const FLAG_H: u8 = 0x20;
const FLAG_C: u8 = 0x10;

#[derive(Debug, Clone, Copy)]
pub struct Registers {
    a: u8,
    f: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    h: u8,
    l: u8,
}

impl Registers {
    pub fn new() -> Self {
        Self {
            a: 0,
            f: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            h: 0,
            l: 0,
        }
    }

    pub fn a(&self) -> u8 {
        self.a
    }

    pub fn b(&self) -> u8 {
        self.b
    }

    pub fn c(&self) -> u8 {
        self.c
    }

    pub fn d(&self) -> u8 {
        self.d
    }

    pub fn e(&self) -> u8 {
        self.e
    }

    pub fn h(&self) -> u8 {
        self.h
    }

    pub fn l(&self) -> u8 {
        self.l
    }

    pub fn f(&self) -> u8 {
        self.f
    }

    pub fn set_a(&mut self, value: u8) {
        self.a = value;
    }

    pub fn set_b(&mut self, value: u8) {
        self.b = value;
    }

    pub fn set_c(&mut self, value: u8) {
        self.c = value;
    }

    pub fn set_d(&mut self, value: u8) {
        self.d = value;
    }

    pub fn set_e(&mut self, value: u8) {
        self.e = value;
    }

    pub fn set_h(&mut self, value: u8) {
        self.h = value;
    }

    pub fn set_l(&mut self, value: u8) {
        self.l = value;
    }

    pub fn af(&self) -> u16 {
        u16::from_be_bytes([self.a, self.f])
    }

    pub fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }

    pub fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }

    pub fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }

    pub fn set_af(&mut self, value: u16) {
        let [hi, lo] = value.to_be_bytes();
        self.a = hi;
        self.f = lo & 0xF0;
    }

    pub fn set_bc(&mut self, value: u16) {
        let [hi, lo] = value.to_be_bytes();
        self.b = hi;
        self.c = lo;
    }

    pub fn set_de(&mut self, value: u16) {
        let [hi, lo] = value.to_be_bytes();
        self.d = hi;
        self.e = lo;
    }

    pub fn set_hl(&mut self, value: u16) {
        let [hi, lo] = value.to_be_bytes();
        self.h = hi;
        self.l = lo;
    }

    pub fn flag_z(&self) -> bool {
        self.f & FLAG_Z != 0
    }

    pub fn flag_n(&self) -> bool {
        self.f & FLAG_N != 0
    }

    pub fn flag_h(&self) -> bool {
        self.f & FLAG_H != 0
    }

    pub fn flag_c(&self) -> bool {
        self.f & FLAG_C != 0
    }

    pub fn set_flag_z(&mut self, on: bool) {
        self.set_flag(FLAG_Z, on);
    }

    pub fn set_flag_n(&mut self, on: bool) {
        self.set_flag(FLAG_N, on);
    }

    pub fn set_flag_h(&mut self, on: bool) {
        self.set_flag(FLAG_H, on);
    }

    pub fn set_flag_c(&mut self, on: bool) {
        self.set_flag(FLAG_C, on);
    }

    fn set_flag(&mut self, mask: u8, on: bool) {
        if on {
            self.f |= mask;
        } else {
            self.f &= !mask;
        }
        self.f &= 0xF0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuError {
    UnimplementedOpcode(u8),
    UnimplementedCbOpcode(u8),
}

#[derive(Debug)]
pub struct Cpu {
    regs: Registers,
    pc: u16,
    sp: u16,
    ime: bool,
    halted: bool,
    stopped: bool,
}

impl Cpu {
    pub fn new() -> Self {
        Self {
            regs: Registers::new(),
            pc: 0x0000,
            sp: 0xFFFE,
            ime: false,
            halted: false,
            stopped: false,
        }
    }

    pub fn regs(&self) -> &Registers {
        &self.regs
    }

    pub fn regs_mut(&mut self) -> &mut Registers {
        &mut self.regs
    }

    pub fn pc(&self) -> u16 {
        self.pc
    }

    pub fn sp(&self) -> u16 {
        self.sp
    }

    pub fn set_pc(&mut self, value: u16) {
        self.pc = value;
    }

    pub fn set_sp(&mut self, value: u16) {
        self.sp = value;
    }

    pub fn ime(&self) -> bool {
        self.ime
    }

    pub fn set_ime(&mut self, value: bool) {
        self.ime = value;
    }

    pub fn step(&mut self, bus: &mut Bus) -> Result<u32, CpuError> {
        if self.halted || self.stopped {
            return Ok(4);
        }

        let opcode = self.fetch8(bus);
        match opcode {
            0x00 => Ok(4),
            0x06 => {
                let value = self.fetch8(bus);
                self.regs.b = value;
                Ok(8)
            }
            0x0E => {
                let value = self.fetch8(bus);
                self.regs.c = value;
                Ok(8)
            }
            0x16 => {
                let value = self.fetch8(bus);
                self.regs.d = value;
                Ok(8)
            }
            0x1E => {
                let value = self.fetch8(bus);
                self.regs.e = value;
                Ok(8)
            }
            0x26 => {
                let value = self.fetch8(bus);
                self.regs.h = value;
                Ok(8)
            }
            0x2E => {
                let value = self.fetch8(bus);
                self.regs.l = value;
                Ok(8)
            }
            0x36 => {
                let value = self.fetch8(bus);
                let addr = self.regs.hl();
                bus.write8(addr, value);
                Ok(12)
            }
            0x3E => {
                let value = self.fetch8(bus);
                self.regs.a = value;
                Ok(8)
            }
            0x18 => {
                let offset = self.fetch8(bus) as i8;
                self.pc = self.pc.wrapping_add(offset as u16);
                Ok(12)
            }
            0x76 => {
                self.halted = true;
                Ok(4)
            }
            0x10 => {
                let _ = self.fetch8(bus);
                self.stopped = true;
                Ok(4)
            }
            0xC3 => {
                let addr = self.fetch16(bus);
                self.pc = addr;
                Ok(16)
            }
            0xEA => {
                let addr = self.fetch16(bus);
                bus.write8(addr, self.regs.a);
                Ok(16)
            }
            0xFA => {
                let addr = self.fetch16(bus);
                self.regs.a = bus.read8(addr);
                Ok(16)
            }
            0xCB => {
                let opcode = self.fetch8(bus);
                Err(CpuError::UnimplementedCbOpcode(opcode))
            }
            _ => Err(CpuError::UnimplementedOpcode(opcode)),
        }
    }

    fn fetch8(&mut self, bus: &mut Bus) -> u8 {
        let value = bus.read8(self.pc);
        self.pc = self.pc.wrapping_add(1);
        value
    }

    fn fetch16(&mut self, bus: &mut Bus) -> u16 {
        let lo = self.fetch8(bus);
        let hi = self.fetch8(bus);
        u16::from_le_bytes([lo, hi])
    }
}

#[cfg(test)]
mod tests {
    use super::{Cpu, Registers};
    use crate::domain::Bus;
    use crate::domain::cartridge::ROM_BANK_SIZE;
    use crate::domain::Cartridge;

    fn bus_with_rom(mut rom: Vec<u8>) -> Bus {
        if rom.len() < 0x0150 {
            rom.resize(0x0150, 0);
        }
        rom[0x0147] = 0x00;
        let cartridge = Cartridge::from_bytes(rom).expect("cartridge");
        Bus::new(cartridge).expect("bus")
    }

    #[test]
    fn registers_mask_lower_flags() {
        let mut regs = Registers::new();
        regs.set_af(0x12F3);
        assert_eq!(regs.af(), 0x12F0);
    }

    #[test]
    fn cpu_executes_nop() {
        let mut rom = vec![0x00; ROM_BANK_SIZE];
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();

        let cycles = cpu.step(&mut bus).expect("step");
        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 1);
    }

    #[test]
    fn cpu_loads_and_writes_memory() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0x3E;
        rom[0x0001] = 0x12;
        rom[0x0002] = 0xEA;
        rom[0x0003] = 0x00;
        rom[0x0004] = 0xC0;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();

        cpu.step(&mut bus).expect("ld a,d8");
        assert_eq!(cpu.regs().a(), 0x12);

        cpu.step(&mut bus).expect("ld (a16),a");
        assert_eq!(bus.read8(0xC000), 0x12);
    }

    #[test]
    fn cpu_jumps_relative() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0x18;
        rom[0x0001] = 0x02;
        rom[0x0002] = 0x00;
        rom[0x0003] = 0x00;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();

        cpu.step(&mut bus).expect("jr");
        assert_eq!(cpu.pc(), 0x0004);
    }
}
