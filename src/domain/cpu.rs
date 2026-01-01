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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reg8 {
    B,
    C,
    D,
    E,
    H,
    L,
    Hl,
    A,
}

impl Reg8 {
    fn from_bits(bits: u8) -> Self {
        match bits & 0x07 {
            0 => Self::B,
            1 => Self::C,
            2 => Self::D,
            3 => Self::E,
            4 => Self::H,
            5 => Self::L,
            6 => Self::Hl,
            _ => Self::A,
        }
    }

    fn is_hl(self) -> bool {
        matches!(self, Self::Hl)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reg16 {
    BC,
    DE,
    HL,
    SP,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cond {
    Nz,
    Z,
    Nc,
    C,
}

impl Cond {
    fn from_bits(bits: u8) -> Self {
        match bits & 0x03 {
            0 => Self::Nz,
            1 => Self::Z,
            2 => Self::Nc,
            _ => Self::C,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reg16Stack {
    BC,
    DE,
    HL,
    AF,
}

impl Reg16Stack {
    fn from_bits(bits: u8) -> Self {
        match bits & 0x03 {
            0 => Self::BC,
            1 => Self::DE,
            2 => Self::HL,
            _ => Self::AF,
        }
    }
}

impl Reg16 {
    fn from_bits(bits: u8) -> Self {
        match bits & 0x03 {
            0 => Self::BC,
            1 => Self::DE,
            2 => Self::HL,
            _ => Self::SP,
        }
    }
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
            0x01 => {
                let value = self.fetch16(bus);
                self.regs.set_bc(value);
                Ok(12)
            }
            0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x34 | 0x3C => {
                let reg = Reg8::from_bits(opcode >> 3);
                let value = self.read_reg8(reg, bus);
                let next = self.inc8(value);
                self.write_reg8(reg, next, bus);
                Ok(if reg.is_hl() { 12 } else { 4 })
            }
            0x02 => {
                let addr = self.regs.bc();
                bus.write8(addr, self.regs.a);
                Ok(8)
            }
            0x03 => {
                let value = self.regs.bc().wrapping_add(1);
                self.regs.set_bc(value);
                Ok(8)
            }
            0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x35 | 0x3D => {
                let reg = Reg8::from_bits(opcode >> 3);
                let value = self.read_reg8(reg, bus);
                let next = self.dec8(value);
                self.write_reg8(reg, next, bus);
                Ok(if reg.is_hl() { 12 } else { 4 })
            }
            0x0B => {
                let value = self.regs.bc().wrapping_sub(1);
                self.regs.set_bc(value);
                Ok(8)
            }
            0x06 => {
                let value = self.fetch8(bus);
                self.regs.b = value;
                Ok(8)
            }
            0x0A => {
                let addr = self.regs.bc();
                self.regs.a = bus.read8(addr);
                Ok(8)
            }
            0x0E => {
                let value = self.fetch8(bus);
                self.regs.c = value;
                Ok(8)
            }
            0x11 => {
                let value = self.fetch16(bus);
                self.regs.set_de(value);
                Ok(12)
            }
            0x16 => {
                let value = self.fetch8(bus);
                self.regs.d = value;
                Ok(8)
            }
            0x12 => {
                let addr = self.regs.de();
                bus.write8(addr, self.regs.a);
                Ok(8)
            }
            0x13 => {
                let value = self.regs.de().wrapping_add(1);
                self.regs.set_de(value);
                Ok(8)
            }
            0x1A => {
                let addr = self.regs.de();
                self.regs.a = bus.read8(addr);
                Ok(8)
            }
            0x1E => {
                let value = self.fetch8(bus);
                self.regs.e = value;
                Ok(8)
            }
            0x1B => {
                let value = self.regs.de().wrapping_sub(1);
                self.regs.set_de(value);
                Ok(8)
            }
            0x21 => {
                let value = self.fetch16(bus);
                self.regs.set_hl(value);
                Ok(12)
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
            0x23 => {
                let value = self.regs.hl().wrapping_add(1);
                self.regs.set_hl(value);
                Ok(8)
            }
            0x31 => {
                let value = self.fetch16(bus);
                self.sp = value;
                Ok(12)
            }
            0x09 | 0x19 | 0x29 | 0x39 => {
                let reg = Reg16::from_bits(opcode >> 4);
                let value = self.read_reg16(reg);
                self.add_hl(value);
                Ok(8)
            }
            0x22 => {
                let addr = self.regs.hl();
                bus.write8(addr, self.regs.a);
                self.regs.set_hl(addr.wrapping_add(1));
                Ok(8)
            }
            0x2A => {
                let addr = self.regs.hl();
                self.regs.a = bus.read8(addr);
                self.regs.set_hl(addr.wrapping_add(1));
                Ok(8)
            }
            0x36 => {
                let value = self.fetch8(bus);
                let addr = self.regs.hl();
                bus.write8(addr, value);
                Ok(12)
            }
            0x32 => {
                let addr = self.regs.hl();
                bus.write8(addr, self.regs.a);
                self.regs.set_hl(addr.wrapping_sub(1));
                Ok(8)
            }
            0x3A => {
                let addr = self.regs.hl();
                self.regs.a = bus.read8(addr);
                self.regs.set_hl(addr.wrapping_sub(1));
                Ok(8)
            }
            0x2B => {
                let value = self.regs.hl().wrapping_sub(1);
                self.regs.set_hl(value);
                Ok(8)
            }
            0x33 => {
                self.sp = self.sp.wrapping_add(1);
                Ok(8)
            }
            0x3E => {
                let value = self.fetch8(bus);
                self.regs.a = value;
                Ok(8)
            }
            0xC1 | 0xD1 | 0xE1 | 0xF1 => {
                let reg = Reg16Stack::from_bits(opcode >> 4);
                let value = self.pop16(bus);
                self.write_reg16_stack(reg, value);
                Ok(12)
            }
            0x40..=0x7F if opcode != 0x76 => {
                let dst = Reg8::from_bits(opcode >> 3);
                let src = Reg8::from_bits(opcode);
                let value = self.read_reg8(src, bus);
                self.write_reg8(dst, value, bus);
                Ok(if dst.is_hl() || src.is_hl() { 8 } else { 4 })
            }
            0x80..=0x87 => {
                let reg = Reg8::from_bits(opcode);
                let value = self.read_reg8(reg, bus);
                self.alu_add(value);
                Ok(if reg.is_hl() { 8 } else { 4 })
            }
            0x88..=0x8F => {
                let reg = Reg8::from_bits(opcode);
                let value = self.read_reg8(reg, bus);
                self.alu_adc(value);
                Ok(if reg.is_hl() { 8 } else { 4 })
            }
            0x90..=0x97 => {
                let reg = Reg8::from_bits(opcode);
                let value = self.read_reg8(reg, bus);
                self.alu_sub(value);
                Ok(if reg.is_hl() { 8 } else { 4 })
            }
            0x98..=0x9F => {
                let reg = Reg8::from_bits(opcode);
                let value = self.read_reg8(reg, bus);
                self.alu_sbc(value);
                Ok(if reg.is_hl() { 8 } else { 4 })
            }
            0xA0..=0xA7 => {
                let reg = Reg8::from_bits(opcode);
                let value = self.read_reg8(reg, bus);
                self.alu_and(value);
                Ok(if reg.is_hl() { 8 } else { 4 })
            }
            0xA8..=0xAF => {
                let reg = Reg8::from_bits(opcode);
                let value = self.read_reg8(reg, bus);
                self.alu_xor(value);
                Ok(if reg.is_hl() { 8 } else { 4 })
            }
            0xB0..=0xB7 => {
                let reg = Reg8::from_bits(opcode);
                let value = self.read_reg8(reg, bus);
                self.alu_or(value);
                Ok(if reg.is_hl() { 8 } else { 4 })
            }
            0xB8..=0xBF => {
                let reg = Reg8::from_bits(opcode);
                let value = self.read_reg8(reg, bus);
                self.alu_cp(value);
                Ok(if reg.is_hl() { 8 } else { 4 })
            }
            0x18 => {
                let offset = self.fetch8(bus) as i8;
                self.pc = self.pc.wrapping_add(offset as u16);
                Ok(12)
            }
            0x20 | 0x28 | 0x30 | 0x38 => {
                let offset = self.fetch8(bus) as i8;
                let cond = Cond::from_bits(opcode >> 3);
                if self.test_cond(cond) {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    Ok(12)
                } else {
                    Ok(8)
                }
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
            0xC6 => {
                let value = self.fetch8(bus);
                self.alu_add(value);
                Ok(8)
            }
            0xC3 => {
                let addr = self.fetch16(bus);
                self.pc = addr;
                Ok(16)
            }
            0xC0 | 0xC8 | 0xD0 | 0xD8 => {
                let cond = Cond::from_bits(opcode >> 3);
                if self.test_cond(cond) {
                    let addr = self.pop16(bus);
                    self.pc = addr;
                    Ok(20)
                } else {
                    Ok(8)
                }
            }
            0xC2 | 0xCA | 0xD2 | 0xDA => {
                let addr = self.fetch16(bus);
                let cond = Cond::from_bits(opcode >> 3);
                if self.test_cond(cond) {
                    self.pc = addr;
                    Ok(16)
                } else {
                    Ok(12)
                }
            }
            0xC4 | 0xCC | 0xD4 | 0xDC => {
                let addr = self.fetch16(bus);
                let cond = Cond::from_bits(opcode >> 3);
                if self.test_cond(cond) {
                    let ret_addr = self.pc;
                    self.push16(bus, ret_addr);
                    self.pc = addr;
                    Ok(24)
                } else {
                    Ok(12)
                }
            }
            0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => {
                let addr = match opcode {
                    0xC7 => 0x0000,
                    0xCF => 0x0008,
                    0xD7 => 0x0010,
                    0xDF => 0x0018,
                    0xE7 => 0x0020,
                    0xEF => 0x0028,
                    0xF7 => 0x0030,
                    _ => 0x0038,
                };
                self.push16(bus, self.pc);
                self.pc = addr;
                Ok(16)
            }
            0xC9 => {
                let addr = self.pop16(bus);
                self.pc = addr;
                Ok(16)
            }
            0xC5 | 0xD5 | 0xE5 | 0xF5 => {
                let reg = Reg16Stack::from_bits(opcode >> 4);
                let value = self.read_reg16_stack(reg);
                self.push16(bus, value);
                Ok(16)
            }
            0xCD => {
                let addr = self.fetch16(bus);
                let ret_addr = self.pc;
                self.push16(bus, ret_addr);
                self.pc = addr;
                Ok(24)
            }
            0xCE => {
                let value = self.fetch8(bus);
                self.alu_adc(value);
                Ok(8)
            }
            0xD6 => {
                let value = self.fetch8(bus);
                self.alu_sub(value);
                Ok(8)
            }
            0xDE => {
                let value = self.fetch8(bus);
                self.alu_sbc(value);
                Ok(8)
            }
            0xE0 => {
                let offset = self.fetch8(bus);
                let addr = 0xFF00u16.wrapping_add(offset as u16);
                bus.write8(addr, self.regs.a);
                Ok(12)
            }
            0xEA => {
                let addr = self.fetch16(bus);
                bus.write8(addr, self.regs.a);
                Ok(16)
            }
            0xE6 => {
                let value = self.fetch8(bus);
                self.alu_and(value);
                Ok(8)
            }
            0xEE => {
                let value = self.fetch8(bus);
                self.alu_xor(value);
                Ok(8)
            }
            0xE8 => {
                let offset = self.fetch8(bus) as i8;
                let result = self.add_sp_offset(offset);
                self.sp = result;
                Ok(16)
            }
            0xE2 => {
                let addr = 0xFF00u16.wrapping_add(self.regs.c as u16);
                bus.write8(addr, self.regs.a);
                Ok(8)
            }
            0xFA => {
                let addr = self.fetch16(bus);
                self.regs.a = bus.read8(addr);
                Ok(16)
            }
            0xF0 => {
                let offset = self.fetch8(bus);
                let addr = 0xFF00u16.wrapping_add(offset as u16);
                self.regs.a = bus.read8(addr);
                Ok(12)
            }
            0xF6 => {
                let value = self.fetch8(bus);
                self.alu_or(value);
                Ok(8)
            }
            0xF8 => {
                let offset = self.fetch8(bus) as i8;
                let result = self.add_sp_offset(offset);
                self.regs.set_hl(result);
                Ok(12)
            }
            0xF2 => {
                let addr = 0xFF00u16.wrapping_add(self.regs.c as u16);
                self.regs.a = bus.read8(addr);
                Ok(8)
            }
            0x3B => {
                self.sp = self.sp.wrapping_sub(1);
                Ok(8)
            }
            0xFE => {
                let value = self.fetch8(bus);
                self.alu_cp(value);
                Ok(8)
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

    fn read_reg8(&self, reg: Reg8, bus: &Bus) -> u8 {
        match reg {
            Reg8::B => self.regs.b,
            Reg8::C => self.regs.c,
            Reg8::D => self.regs.d,
            Reg8::E => self.regs.e,
            Reg8::H => self.regs.h,
            Reg8::L => self.regs.l,
            Reg8::Hl => bus.read8(self.regs.hl()),
            Reg8::A => self.regs.a,
        }
    }

    fn write_reg8(&mut self, reg: Reg8, value: u8, bus: &mut Bus) {
        match reg {
            Reg8::B => self.regs.b = value,
            Reg8::C => self.regs.c = value,
            Reg8::D => self.regs.d = value,
            Reg8::E => self.regs.e = value,
            Reg8::H => self.regs.h = value,
            Reg8::L => self.regs.l = value,
            Reg8::Hl => bus.write8(self.regs.hl(), value),
            Reg8::A => self.regs.a = value,
        }
    }

    fn read_reg16(&self, reg: Reg16) -> u16 {
        match reg {
            Reg16::BC => self.regs.bc(),
            Reg16::DE => self.regs.de(),
            Reg16::HL => self.regs.hl(),
            Reg16::SP => self.sp,
        }
    }

    fn read_reg16_stack(&self, reg: Reg16Stack) -> u16 {
        match reg {
            Reg16Stack::BC => self.regs.bc(),
            Reg16Stack::DE => self.regs.de(),
            Reg16Stack::HL => self.regs.hl(),
            Reg16Stack::AF => self.regs.af(),
        }
    }

    fn write_reg16_stack(&mut self, reg: Reg16Stack, value: u16) {
        match reg {
            Reg16Stack::BC => self.regs.set_bc(value),
            Reg16Stack::DE => self.regs.set_de(value),
            Reg16Stack::HL => self.regs.set_hl(value),
            Reg16Stack::AF => self.regs.set_af(value),
        }
    }

    fn push16(&mut self, bus: &mut Bus, value: u16) {
        let [hi, lo] = value.to_be_bytes();
        self.sp = self.sp.wrapping_sub(1);
        bus.write8(self.sp, hi);
        self.sp = self.sp.wrapping_sub(1);
        bus.write8(self.sp, lo);
    }

    fn pop16(&mut self, bus: &Bus) -> u16 {
        let lo = bus.read8(self.sp);
        let sp_next = self.sp.wrapping_add(1);
        let hi = bus.read8(sp_next);
        self.sp = self.sp.wrapping_add(2);
        u16::from_be_bytes([hi, lo])
    }

    fn inc8(&mut self, value: u8) -> u8 {
        let next = value.wrapping_add(1);
        self.regs.set_flag_z(next == 0);
        self.regs.set_flag_n(false);
        self.regs.set_flag_h((value & 0x0F) + 1 > 0x0F);
        next
    }

    fn dec8(&mut self, value: u8) -> u8 {
        let next = value.wrapping_sub(1);
        self.regs.set_flag_z(next == 0);
        self.regs.set_flag_n(true);
        self.regs.set_flag_h((value & 0x0F) == 0);
        next
    }

    fn alu_add(&mut self, value: u8) {
        let a = self.regs.a;
        let (next, carry) = a.overflowing_add(value);
        self.regs.a = next;
        self.regs.set_flag_z(next == 0);
        self.regs.set_flag_n(false);
        self.regs.set_flag_h(((a & 0x0F) + (value & 0x0F)) > 0x0F);
        self.regs.set_flag_c(carry);
    }

    fn alu_adc(&mut self, value: u8) {
        let carry_in = if self.regs.flag_c() { 1 } else { 0 };
        let a = self.regs.a;
        let (tmp, carry1) = a.overflowing_add(value);
        let (next, carry2) = tmp.overflowing_add(carry_in);
        self.regs.a = next;
        self.regs.set_flag_z(next == 0);
        self.regs.set_flag_n(false);
        self.regs
            .set_flag_h(((a & 0x0F) + (value & 0x0F) + (carry_in as u8)) > 0x0F);
        self.regs.set_flag_c(carry1 || carry2);
    }

    fn alu_sub(&mut self, value: u8) {
        let a = self.regs.a;
        let (next, borrow) = a.overflowing_sub(value);
        self.regs.a = next;
        self.regs.set_flag_z(next == 0);
        self.regs.set_flag_n(true);
        self.regs.set_flag_h((a & 0x0F) < (value & 0x0F));
        self.regs.set_flag_c(borrow);
    }

    fn alu_sbc(&mut self, value: u8) {
        let carry_in = if self.regs.flag_c() { 1 } else { 0 };
        let a = self.regs.a;
        let value_with_carry = (value as u16) + (carry_in as u16);
        let result = (a as u16).wrapping_sub(value_with_carry);
        let next = result as u8;
        self.regs.a = next;
        self.regs.set_flag_z(next == 0);
        self.regs.set_flag_n(true);
        self.regs
            .set_flag_h((a & 0x0F) < ((value & 0x0F) + carry_in));
        self.regs.set_flag_c((a as u16) < value_with_carry);
    }

    fn add_hl(&mut self, value: u16) {
        let hl = self.regs.hl();
        let result = hl.wrapping_add(value);
        self.regs.set_flag_n(false);
        self.regs
            .set_flag_h(((hl & 0x0FFF) + (value & 0x0FFF)) > 0x0FFF);
        self.regs.set_flag_c((hl as u32 + value as u32) > 0xFFFF);
        self.regs.set_hl(result);
    }

    fn add_sp_offset(&mut self, offset: i8) -> u16 {
        let sp = self.sp;
        let offset_u16 = offset as i16 as u16;
        let result = sp.wrapping_add(offset_u16);
        let carry = ((sp ^ offset_u16 ^ result) & 0x0100) != 0;
        let half_carry = ((sp ^ offset_u16 ^ result) & 0x0010) != 0;
        self.regs.set_flag_z(false);
        self.regs.set_flag_n(false);
        self.regs.set_flag_h(half_carry);
        self.regs.set_flag_c(carry);
        result
    }

    fn test_cond(&self, cond: Cond) -> bool {
        match cond {
            Cond::Nz => !self.regs.flag_z(),
            Cond::Z => self.regs.flag_z(),
            Cond::Nc => !self.regs.flag_c(),
            Cond::C => self.regs.flag_c(),
        }
    }

    fn alu_and(&mut self, value: u8) {
        let next = self.regs.a & value;
        self.regs.a = next;
        self.regs.set_flag_z(next == 0);
        self.regs.set_flag_n(false);
        self.regs.set_flag_h(true);
        self.regs.set_flag_c(false);
    }

    fn alu_xor(&mut self, value: u8) {
        let next = self.regs.a ^ value;
        self.regs.a = next;
        self.regs.set_flag_z(next == 0);
        self.regs.set_flag_n(false);
        self.regs.set_flag_h(false);
        self.regs.set_flag_c(false);
    }

    fn alu_or(&mut self, value: u8) {
        let next = self.regs.a | value;
        self.regs.a = next;
        self.regs.set_flag_z(next == 0);
        self.regs.set_flag_n(false);
        self.regs.set_flag_h(false);
        self.regs.set_flag_c(false);
    }

    fn alu_cp(&mut self, value: u8) {
        let a = self.regs.a;
        let (next, borrow) = a.overflowing_sub(value);
        self.regs.set_flag_z(next == 0);
        self.regs.set_flag_n(true);
        self.regs.set_flag_h((a & 0x0F) < (value & 0x0F));
        self.regs.set_flag_c(borrow);
    }
}

#[cfg(test)]
mod tests {
    use super::{Cpu, Registers};
    use crate::domain::Bus;
    use crate::domain::Cartridge;
    use crate::domain::cartridge::ROM_BANK_SIZE;

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
        let rom = vec![0x00; ROM_BANK_SIZE];
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

    #[test]
    fn cpu_add_sets_flags() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0x3E;
        rom[0x0001] = 0x0F;
        rom[0x0002] = 0x06;
        rom[0x0003] = 0x01;
        rom[0x0004] = 0x80;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();

        cpu.step(&mut bus).expect("ld a,d8");
        cpu.step(&mut bus).expect("ld b,d8");
        cpu.step(&mut bus).expect("add a,b");

        assert_eq!(cpu.regs().a(), 0x10);
        assert!(!cpu.regs().flag_z());
        assert!(!cpu.regs().flag_n());
        assert!(cpu.regs().flag_h());
        assert!(!cpu.regs().flag_c());
    }

    #[test]
    fn cpu_and_xor_or_cp() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0x3E;
        rom[0x0001] = 0xF0;
        rom[0x0002] = 0xE6;
        rom[0x0003] = 0x0F;
        rom[0x0004] = 0xEE;
        rom[0x0005] = 0x0F;
        rom[0x0006] = 0xF6;
        rom[0x0007] = 0x01;
        rom[0x0008] = 0xFE;
        rom[0x0009] = 0x01;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();

        cpu.step(&mut bus).expect("ld a,d8");
        cpu.step(&mut bus).expect("and d8");
        assert_eq!(cpu.regs().a(), 0x00);
        assert!(cpu.regs().flag_z());
        assert!(cpu.regs().flag_h());

        cpu.step(&mut bus).expect("xor d8");
        assert_eq!(cpu.regs().a(), 0x0F);
        assert!(!cpu.regs().flag_z());
        assert!(!cpu.regs().flag_h());

        cpu.step(&mut bus).expect("or d8");
        assert_eq!(cpu.regs().a(), 0x0F);

        cpu.step(&mut bus).expect("cp d8");
        assert_eq!(cpu.regs().a(), 0x0F);
        assert!(!cpu.regs().flag_z());
        assert!(cpu.regs().flag_n());
        assert!(!cpu.regs().flag_c());
    }

    #[test]
    fn cpu_adc_and_sbc_use_carry() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0x3E;
        rom[0x0001] = 0x0F;
        rom[0x0002] = 0x06;
        rom[0x0003] = 0x01;
        rom[0x0004] = 0x88;
        rom[0x0005] = 0x98;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();

        cpu.regs_mut().set_flag_c(true);
        cpu.step(&mut bus).expect("ld a,d8");
        cpu.step(&mut bus).expect("ld b,d8");
        cpu.step(&mut bus).expect("adc a,b");

        assert_eq!(cpu.regs().a(), 0x11);
        assert!(cpu.regs().flag_h());
        assert!(!cpu.regs().flag_n());

        cpu.regs_mut().set_flag_c(true);
        cpu.step(&mut bus).expect("sbc a,b");

        assert_eq!(cpu.regs().a(), 0x0F);
        assert!(cpu.regs().flag_n());
        assert!(cpu.regs().flag_h());
        assert!(!cpu.regs().flag_c());
    }

    #[test]
    fn cpu_ld_rr_and_hl_increment_decrement() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0x77;
        rom[0x0001] = 0x7E;
        rom[0x0002] = 0x22;
        rom[0x0003] = 0x2A;
        rom[0x0004] = 0x32;
        rom[0x0005] = 0x3A;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();
        cpu.regs_mut().set_hl(0xC000);
        cpu.regs_mut().set_a(0x55);

        cpu.step(&mut bus).expect("ld (hl),a");
        assert_eq!(bus.read8(0xC000), 0x55);

        bus.write8(0xC000, 0xAA);
        cpu.step(&mut bus).expect("ld a,(hl)");
        assert_eq!(cpu.regs().a(), 0xAA);

        cpu.regs_mut().set_a(0x11);
        cpu.step(&mut bus).expect("ld (hl+),a");
        assert_eq!(cpu.regs().hl(), 0xC001);
        assert_eq!(bus.read8(0xC000), 0x11);

        bus.write8(0xC001, 0x22);
        cpu.step(&mut bus).expect("ld a,(hl+)");
        assert_eq!(cpu.regs().a(), 0x22);
        assert_eq!(cpu.regs().hl(), 0xC002);

        cpu.regs_mut().set_a(0x33);
        cpu.step(&mut bus).expect("ld (hl-),a");
        assert_eq!(cpu.regs().hl(), 0xC001);
        assert_eq!(bus.read8(0xC002), 0x33);

        bus.write8(0xC001, 0x44);
        cpu.step(&mut bus).expect("ld a,(hl-)");
        assert_eq!(cpu.regs().a(), 0x44);
        assert_eq!(cpu.regs().hl(), 0xC000);
    }

    #[test]
    fn cpu_add_hl_preserves_z_flag() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0x09;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();
        cpu.regs_mut().set_hl(0x0FFF);
        cpu.regs_mut().set_bc(0x0001);
        cpu.regs_mut().set_flag_z(true);

        cpu.step(&mut bus).expect("add hl,bc");
        assert_eq!(cpu.regs().hl(), 0x1000);
        assert!(cpu.regs().flag_z());
        assert!(cpu.regs().flag_h());
        assert!(!cpu.regs().flag_n());
    }

    #[test]
    fn cpu_loads_16bit_immediates() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0x01;
        rom[0x0001] = 0x34;
        rom[0x0002] = 0x12;
        rom[0x0003] = 0x11;
        rom[0x0004] = 0x78;
        rom[0x0005] = 0x56;
        rom[0x0006] = 0x21;
        rom[0x0007] = 0xBC;
        rom[0x0008] = 0x9A;
        rom[0x0009] = 0x31;
        rom[0x000A] = 0xEF;
        rom[0x000B] = 0xCD;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();

        cpu.step(&mut bus).expect("ld bc,d16");
        cpu.step(&mut bus).expect("ld de,d16");
        cpu.step(&mut bus).expect("ld hl,d16");
        cpu.step(&mut bus).expect("ld sp,d16");

        assert_eq!(cpu.regs().bc(), 0x1234);
        assert_eq!(cpu.regs().de(), 0x5678);
        assert_eq!(cpu.regs().hl(), 0x9ABC);
        assert_eq!(cpu.sp(), 0xCDEF);
    }

    #[test]
    fn cpu_loads_and_stores_via_bc_de() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0x01;
        rom[0x0001] = 0x00;
        rom[0x0002] = 0xC0;
        rom[0x0003] = 0x11;
        rom[0x0004] = 0x01;
        rom[0x0005] = 0xC0;
        rom[0x0006] = 0x3E;
        rom[0x0007] = 0x42;
        rom[0x0008] = 0x02;
        rom[0x0009] = 0x12;
        rom[0x000A] = 0x0A;
        rom[0x000B] = 0x1A;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();

        cpu.step(&mut bus).expect("ld bc,d16");
        cpu.step(&mut bus).expect("ld de,d16");
        cpu.step(&mut bus).expect("ld a,d8");
        cpu.step(&mut bus).expect("ld (bc),a");
        cpu.step(&mut bus).expect("ld (de),a");

        assert_eq!(bus.read8(0xC000), 0x42);
        assert_eq!(bus.read8(0xC001), 0x42);

        bus.write8(0xC000, 0x11);
        bus.write8(0xC001, 0x22);
        cpu.step(&mut bus).expect("ld a,(bc)");
        assert_eq!(cpu.regs().a(), 0x11);
        cpu.step(&mut bus).expect("ld a,(de)");
        assert_eq!(cpu.regs().a(), 0x22);
    }

    #[test]
    fn cpu_inc_dec_rr() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0x01;
        rom[0x0001] = 0xFF;
        rom[0x0002] = 0xFF;
        rom[0x0003] = 0x03;
        rom[0x0004] = 0x0B;
        rom[0x0005] = 0x11;
        rom[0x0006] = 0x00;
        rom[0x0007] = 0x10;
        rom[0x0008] = 0x13;
        rom[0x0009] = 0x1B;
        rom[0x000A] = 0x21;
        rom[0x000B] = 0x00;
        rom[0x000C] = 0x20;
        rom[0x000D] = 0x23;
        rom[0x000E] = 0x2B;
        rom[0x000F] = 0x31;
        rom[0x0010] = 0x00;
        rom[0x0011] = 0x30;
        rom[0x0012] = 0x33;
        rom[0x0013] = 0x3B;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();

        cpu.step(&mut bus).expect("ld bc,d16");
        cpu.step(&mut bus).expect("inc bc");
        cpu.step(&mut bus).expect("dec bc");
        assert_eq!(cpu.regs().bc(), 0xFFFF);

        cpu.step(&mut bus).expect("ld de,d16");
        cpu.step(&mut bus).expect("inc de");
        cpu.step(&mut bus).expect("dec de");
        assert_eq!(cpu.regs().de(), 0x1000);

        cpu.step(&mut bus).expect("ld hl,d16");
        cpu.step(&mut bus).expect("inc hl");
        cpu.step(&mut bus).expect("dec hl");
        assert_eq!(cpu.regs().hl(), 0x2000);

        cpu.step(&mut bus).expect("ld sp,d16");
        cpu.step(&mut bus).expect("inc sp");
        cpu.step(&mut bus).expect("dec sp");
        assert_eq!(cpu.sp(), 0x3000);
    }

    #[test]
    fn cpu_add_sp_and_ld_hl_with_offset() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0x31;
        rom[0x0001] = 0xF8;
        rom[0x0002] = 0xFF;
        rom[0x0003] = 0xE8;
        rom[0x0004] = 0x08;
        rom[0x0005] = 0xF8;
        rom[0x0006] = 0xF8;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();

        cpu.step(&mut bus).expect("ld sp,d16");
        cpu.step(&mut bus).expect("add sp,e8");
        assert_eq!(cpu.sp(), 0x0000);
        assert!(cpu.regs().flag_c());
        assert!(cpu.regs().flag_h());
        assert!(!cpu.regs().flag_z());
        assert!(!cpu.regs().flag_n());

        cpu.step(&mut bus).expect("ld hl,sp+e8");
        assert_eq!(cpu.regs().hl(), 0xFFF8);
        assert!(!cpu.regs().flag_c());
        assert!(!cpu.regs().flag_h());
    }

    #[test]
    fn cpu_ldh_immediate_and_c() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0x3E;
        rom[0x0001] = 0x77;
        rom[0x0002] = 0xE0;
        rom[0x0003] = 0x42;
        rom[0x0004] = 0xF0;
        rom[0x0005] = 0x42;
        rom[0x0006] = 0x0E;
        rom[0x0007] = 0x10;
        rom[0x0008] = 0x3E;
        rom[0x0009] = 0x99;
        rom[0x000A] = 0xE2;
        rom[0x000B] = 0xF2;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();

        cpu.step(&mut bus).expect("ld a,d8");
        cpu.step(&mut bus).expect("ldh (a8),a");
        assert_eq!(bus.read8(0xFF42), 0x77);

        cpu.step(&mut bus).expect("ldh a,(a8)");
        assert_eq!(cpu.regs().a(), 0x77);

        cpu.step(&mut bus).expect("ld c,d8");
        cpu.step(&mut bus).expect("ld a,d8");
        cpu.step(&mut bus).expect("ld (c),a");
        assert_eq!(bus.read8(0xFF10), 0x99);

        bus.write8(0xFF10, 0x55);
        cpu.step(&mut bus).expect("ld a,(c)");
        assert_eq!(cpu.regs().a(), 0x55);
    }

    #[test]
    fn cpu_push_pop_stack() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0x01;
        rom[0x0001] = 0x34;
        rom[0x0002] = 0x12;
        rom[0x0003] = 0x11;
        rom[0x0004] = 0x78;
        rom[0x0005] = 0x56;
        rom[0x0006] = 0x21;
        rom[0x0007] = 0xBC;
        rom[0x0008] = 0x9A;
        rom[0x0009] = 0xF5;
        rom[0x000A] = 0xC5;
        rom[0x000B] = 0xD5;
        rom[0x000C] = 0xE5;
        rom[0x000D] = 0xE1;
        rom[0x000E] = 0xD1;
        rom[0x000F] = 0xC1;
        rom[0x0010] = 0xF1;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();
        cpu.regs_mut().set_a(0xF0);
        cpu.regs_mut().set_flag_c(true);
        let sp_start = cpu.sp();

        cpu.step(&mut bus).expect("ld bc,d16");
        cpu.step(&mut bus).expect("ld de,d16");
        cpu.step(&mut bus).expect("ld hl,d16");
        cpu.step(&mut bus).expect("push af");
        cpu.step(&mut bus).expect("push bc");
        cpu.step(&mut bus).expect("push de");
        cpu.step(&mut bus).expect("push hl");

        assert_eq!(cpu.sp(), sp_start.wrapping_sub(8));

        cpu.step(&mut bus).expect("pop hl");
        cpu.step(&mut bus).expect("pop de");
        cpu.step(&mut bus).expect("pop bc");
        cpu.step(&mut bus).expect("pop af");

        assert_eq!(cpu.sp(), sp_start);
        assert_eq!(cpu.regs().af(), 0xF010);
        assert_eq!(cpu.regs().bc(), 0x1234);
        assert_eq!(cpu.regs().de(), 0x5678);
        assert_eq!(cpu.regs().hl(), 0x9ABC);
    }

    #[test]
    fn cpu_call_and_ret() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0xCD;
        rom[0x0001] = 0x05;
        rom[0x0002] = 0x00;
        rom[0x0003] = 0x00;
        rom[0x0004] = 0x00;
        rom[0x0005] = 0x00;
        rom[0x0006] = 0xC9;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();
        let sp_start = cpu.sp();

        cpu.step(&mut bus).expect("call a16");
        assert_eq!(cpu.pc(), 0x0005);
        assert_eq!(cpu.sp(), sp_start.wrapping_sub(2));
        assert_eq!(bus.read8(cpu.sp()), 0x03);
        assert_eq!(bus.read8(cpu.sp().wrapping_add(1)), 0x00);

        cpu.step(&mut bus).expect("nop");
        cpu.step(&mut bus).expect("ret");
        assert_eq!(cpu.pc(), 0x0003);
        assert_eq!(cpu.sp(), sp_start);
    }

    #[test]
    fn cpu_rst_vectors() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0xFF;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();
        let sp_start = cpu.sp();

        cpu.step(&mut bus).expect("rst 0x38");
        assert_eq!(cpu.pc(), 0x0038);
        assert_eq!(cpu.sp(), sp_start.wrapping_sub(2));
        assert_eq!(bus.read8(cpu.sp()), 0x01);
        assert_eq!(bus.read8(cpu.sp().wrapping_add(1)), 0x00);
    }

    #[test]
    fn cpu_jr_conditional() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0x20;
        rom[0x0001] = 0x02;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();

        cpu.regs_mut().set_flag_z(false);
        let cycles = cpu.step(&mut bus).expect("jr nz");
        assert_eq!(cycles, 12);
        assert_eq!(cpu.pc(), 0x0004);

        let mut bus = bus_with_rom(vec![0x28, 0x02]);
        let mut cpu = Cpu::new();
        cpu.regs_mut().set_flag_z(false);
        let cycles = cpu.step(&mut bus).expect("jr z");
        assert_eq!(cycles, 8);
        assert_eq!(cpu.pc(), 0x0002);
    }

    #[test]
    fn cpu_jp_call_ret_conditional() {
        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0xC2;
        rom[0x0001] = 0x05;
        rom[0x0002] = 0x00;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();
        cpu.regs_mut().set_flag_z(true);

        let cycles = cpu.step(&mut bus).expect("jp nz");
        assert_eq!(cycles, 12);
        assert_eq!(cpu.pc(), 0x0003);

        let mut bus = bus_with_rom(vec![0xC2, 0x34, 0x12]);
        let mut cpu = Cpu::new();
        cpu.regs_mut().set_flag_z(false);
        let cycles = cpu.step(&mut bus).expect("jp nz");
        assert_eq!(cycles, 16);
        assert_eq!(cpu.pc(), 0x1234);

        let mut rom = vec![0; ROM_BANK_SIZE];
        rom[0x0000] = 0xCC;
        rom[0x0001] = 0x05;
        rom[0x0002] = 0x00;
        let mut bus = bus_with_rom(rom);
        let mut cpu = Cpu::new();
        cpu.regs_mut().set_flag_z(true);
        let sp_start = cpu.sp();
        let cycles = cpu.step(&mut bus).expect("call z");
        assert_eq!(cycles, 24);
        assert_eq!(cpu.pc(), 0x0005);
        assert_eq!(cpu.sp(), sp_start.wrapping_sub(2));

        let mut bus = bus_with_rom(vec![0xCC, 0x05, 0x00]);
        let mut cpu = Cpu::new();
        cpu.regs_mut().set_flag_z(false);
        let cycles = cpu.step(&mut bus).expect("call z");
        assert_eq!(cycles, 12);
        assert_eq!(cpu.pc(), 0x0003);

        let mut bus = bus_with_rom(vec![0xD8]);
        let mut cpu = Cpu::new();
        cpu.regs_mut().set_flag_c(true);
        cpu.set_sp(0xFFFC);
        bus.write8(0xFFFC, 0x34);
        bus.write8(0xFFFD, 0x12);
        let cycles = cpu.step(&mut bus).expect("ret c");
        assert_eq!(cycles, 20);
        assert_eq!(cpu.pc(), 0x1234);
        assert_eq!(cpu.sp(), 0xFFFE);

        let mut bus = bus_with_rom(vec![0xD8]);
        let mut cpu = Cpu::new();
        cpu.regs_mut().set_flag_c(false);
        let sp_start = cpu.sp();
        let cycles = cpu.step(&mut bus).expect("ret c");
        assert_eq!(cycles, 8);
        assert_eq!(cpu.pc(), 0x0001);
        assert_eq!(cpu.sp(), sp_start);
    }
}
