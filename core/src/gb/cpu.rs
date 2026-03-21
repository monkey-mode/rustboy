/// Sharp LR35902 CPU emulation.
///
/// Registers: A, F (flags), B, C, D, E, H, L, SP, PC
/// Flags (in F register):
///   Bit 7 – Z (Zero)
///   Bit 6 – N (Subtract)
///   Bit 5 – H (Half-carry)
///   Bit 4 – C (Carry)
///   Bits 3-0 are always 0

use super::mmu::Mmu;
use crate::save_state::*;

// ---------------------------------------------------------------------------
// Flag bit positions
// ---------------------------------------------------------------------------
const FLAG_Z: u8 = 1 << 7;
const FLAG_N: u8 = 1 << 6;
const FLAG_H: u8 = 1 << 5;
const FLAG_C: u8 = 1 << 4;

// ---------------------------------------------------------------------------
// Register file
// ---------------------------------------------------------------------------
#[derive(Default, Clone)]
pub struct Registers {
    pub a: u8,
    pub f: u8, // lower 4 bits always 0
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
}

impl Registers {
    fn af(&self) -> u16 { ((self.a as u16) << 8) | (self.f & 0xF0) as u16 }
    fn bc(&self) -> u16 { ((self.b as u16) << 8) | self.c as u16 }
    fn de(&self) -> u16 { ((self.d as u16) << 8) | self.e as u16 }
    fn hl(&self) -> u16 { ((self.h as u16) << 8) | self.l as u16 }

    fn set_af(&mut self, v: u16) { self.a = (v >> 8) as u8; self.f = (v & 0xF0) as u8; }
    fn set_bc(&mut self, v: u16) { self.b = (v >> 8) as u8; self.c = v as u8; }
    fn set_de(&mut self, v: u16) { self.d = (v >> 8) as u8; self.e = v as u8; }
    fn set_hl(&mut self, v: u16) { self.h = (v >> 8) as u8; self.l = v as u8; }

    fn flag_z(&self) -> bool { self.f & FLAG_Z != 0 }
    fn flag_n(&self) -> bool { self.f & FLAG_N != 0 }
    fn flag_h(&self) -> bool { self.f & FLAG_H != 0 }
    fn flag_c(&self) -> bool { self.f & FLAG_C != 0 }

    fn set_flags(&mut self, z: bool, n: bool, h: bool, c: bool) {
        self.f = 0;
        if z { self.f |= FLAG_Z; }
        if n { self.f |= FLAG_N; }
        if h { self.f |= FLAG_H; }
        if c { self.f |= FLAG_C; }
    }
}

// ---------------------------------------------------------------------------
// CPU
// ---------------------------------------------------------------------------
pub struct Cpu {
    pub regs: Registers,
    pub ime: bool,        // Interrupt Master Enable
    pub ime_pending: bool, // EI schedules IME for the *next* instruction
    pub halted: bool,
    pub stopped: bool,
}

impl Cpu {
    pub fn new() -> Self {
        // Post-bootrom state (DMG)
        let mut regs = Registers::default();
        regs.a = 0x01; regs.f = 0xB0;
        regs.b = 0x00; regs.c = 0x13;
        regs.d = 0x00; regs.e = 0xD8;
        regs.h = 0x01; regs.l = 0x4D;
        regs.sp = 0xFFFE;
        regs.pc = 0x0100;
        Cpu { regs, ime: false, ime_pending: false, halted: false, stopped: false }
    }

    // -----------------------------------------------------------------------
    // Memory helpers
    // -----------------------------------------------------------------------
    fn read(&self, mmu: &Mmu, addr: u16) -> u8 { mmu.read_byte(addr) }
    fn write(&self, mmu: &mut Mmu, addr: u16, val: u8) { mmu.write_byte(addr, val); }

    fn fetch_byte(&mut self, mmu: &Mmu) -> u8 {
        let b = self.read(mmu, self.regs.pc);
        self.regs.pc = self.regs.pc.wrapping_add(1);
        b
    }

    fn fetch_word(&mut self, mmu: &Mmu) -> u16 {
        let lo = self.fetch_byte(mmu) as u16;
        let hi = self.fetch_byte(mmu) as u16;
        (hi << 8) | lo
    }

    fn push_word(&mut self, mmu: &mut Mmu, val: u16) {
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        self.write(mmu, self.regs.sp, (val >> 8) as u8);
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        self.write(mmu, self.regs.sp, (val & 0xFF) as u8);
    }

    fn pop_word(&mut self, mmu: &mut Mmu) -> u16 {
        let lo = self.read(mmu, self.regs.sp) as u16;
        self.regs.sp = self.regs.sp.wrapping_add(1);
        let hi = self.read(mmu, self.regs.sp) as u16;
        self.regs.sp = self.regs.sp.wrapping_add(1);
        (hi << 8) | lo
    }

    // -----------------------------------------------------------------------
    // Interrupt handling
    // -----------------------------------------------------------------------
    /// Service pending interrupts. Returns cycles consumed (0 if none).
    pub fn handle_interrupts(&mut self, mmu: &mut Mmu) -> u32 {
        let pending = mmu.interrupt_flag & mmu.interrupt_enable & 0x1F;
        if pending == 0 { return 0; }

        // Any pending interrupt un-halts the CPU even if IME is off.
        if self.halted {
            self.halted = false;
        }

        if !self.ime { return 0; }
        self.ime = false;

        // Find lowest set bit (highest priority)
        let bit = pending.trailing_zeros() as u8;
        mmu.interrupt_flag &= !(1 << bit);

        // Push PC, jump to ISR
        self.push_word(mmu, self.regs.pc);
        self.regs.pc = match bit {
            0 => 0x0040, // VBlank
            1 => 0x0048, // LCD STAT
            2 => 0x0050, // Timer
            3 => 0x0058, // Serial
            4 => 0x0060, // Joypad
            _ => 0x0040,
        };

        20 // cycles for interrupt dispatch
    }

    // -----------------------------------------------------------------------
    // Main step – execute one instruction, return cycles taken
    // -----------------------------------------------------------------------
    pub fn step(&mut self, mmu: &mut Mmu) -> u32 {
        // Handle delayed IME enable (EI instruction)
        if self.ime_pending {
            self.ime = true;
            self.ime_pending = false;
        }

        if self.halted {
            return 4; // NOP while halted
        }

        let opcode = self.fetch_byte(mmu);
        self.execute(opcode, mmu)
    }

    // -----------------------------------------------------------------------
    // Opcode execution
    // -----------------------------------------------------------------------
    fn execute(&mut self, op: u8, mmu: &mut Mmu) -> u32 {
        match op {
            // --- NOP ---
            0x00 => 4,

            // --- LD r16, d16 ---
            0x01 => { let v = self.fetch_word(mmu); self.regs.set_bc(v); 12 }
            0x11 => { let v = self.fetch_word(mmu); self.regs.set_de(v); 12 }
            0x21 => { let v = self.fetch_word(mmu); self.regs.set_hl(v); 12 }
            0x31 => { let v = self.fetch_word(mmu); self.regs.sp = v;    12 }

            // --- LD (r16), A ---
            0x02 => { self.write(mmu, self.regs.bc(), self.regs.a); 8 }
            0x12 => { self.write(mmu, self.regs.de(), self.regs.a); 8 }
            0x22 => {
                let hl = self.regs.hl();
                self.write(mmu, hl, self.regs.a);
                self.regs.set_hl(hl.wrapping_add(1));
                8
            }
            0x32 => {
                let hl = self.regs.hl();
                self.write(mmu, hl, self.regs.a);
                self.regs.set_hl(hl.wrapping_sub(1));
                8
            }

            // --- INC r16 ---
            0x03 => { let v = self.regs.bc().wrapping_add(1); self.regs.set_bc(v); 8 }
            0x13 => { let v = self.regs.de().wrapping_add(1); self.regs.set_de(v); 8 }
            0x23 => { let v = self.regs.hl().wrapping_add(1); self.regs.set_hl(v); 8 }
            0x33 => { self.regs.sp = self.regs.sp.wrapping_add(1); 8 }

            // --- INC r8 ---
            0x04 => { self.regs.b = self.inc8(self.regs.b); 4 }
            0x0C => { self.regs.c = self.inc8(self.regs.c); 4 }
            0x14 => { self.regs.d = self.inc8(self.regs.d); 4 }
            0x1C => { self.regs.e = self.inc8(self.regs.e); 4 }
            0x24 => { self.regs.h = self.inc8(self.regs.h); 4 }
            0x2C => { self.regs.l = self.inc8(self.regs.l); 4 }
            0x34 => {
                let hl = self.regs.hl();
                let v = self.read(mmu, hl);
                let r = self.inc8(v);
                self.write(mmu, hl, r);
                12
            }
            0x3C => { self.regs.a = self.inc8(self.regs.a); 4 }

            // --- DEC r8 ---
            0x05 => { self.regs.b = self.dec8(self.regs.b); 4 }
            0x0D => { self.regs.c = self.dec8(self.regs.c); 4 }
            0x15 => { self.regs.d = self.dec8(self.regs.d); 4 }
            0x1D => { self.regs.e = self.dec8(self.regs.e); 4 }
            0x25 => { self.regs.h = self.dec8(self.regs.h); 4 }
            0x2D => { self.regs.l = self.dec8(self.regs.l); 4 }
            0x35 => {
                let hl = self.regs.hl();
                let v = self.read(mmu, hl);
                let r = self.dec8(v);
                self.write(mmu, hl, r);
                12
            }
            0x3D => { self.regs.a = self.dec8(self.regs.a); 4 }

            // --- DEC r16 ---
            0x0B => { let v = self.regs.bc().wrapping_sub(1); self.regs.set_bc(v); 8 }
            0x1B => { let v = self.regs.de().wrapping_sub(1); self.regs.set_de(v); 8 }
            0x2B => { let v = self.regs.hl().wrapping_sub(1); self.regs.set_hl(v); 8 }
            0x3B => { self.regs.sp = self.regs.sp.wrapping_sub(1); 8 }

            // --- LD r8, d8 ---
            0x06 => { self.regs.b = self.fetch_byte(mmu); 8 }
            0x0E => { self.regs.c = self.fetch_byte(mmu); 8 }
            0x16 => { self.regs.d = self.fetch_byte(mmu); 8 }
            0x1E => { self.regs.e = self.fetch_byte(mmu); 8 }
            0x26 => { self.regs.h = self.fetch_byte(mmu); 8 }
            0x2E => { self.regs.l = self.fetch_byte(mmu); 8 }
            0x36 => {
                let v = self.fetch_byte(mmu);
                let hl = self.regs.hl();
                self.write(mmu, hl, v);
                12
            }
            0x3E => { self.regs.a = self.fetch_byte(mmu); 8 }

            // --- Rotate A ---
            0x07 => { self.rlca(); 4 }
            0x0F => { self.rrca(); 4 }
            0x17 => { self.rla();  4 }
            0x1F => { self.rra();  4 }

            // --- STOP ---
            0x10 => { self.stopped = true; 4 }

            // --- JR ---
            0x18 => { let e = self.fetch_byte(mmu) as i8; self.regs.pc = self.regs.pc.wrapping_add(e as u16); 12 }
            0x20 => {
                let e = self.fetch_byte(mmu) as i8;
                if !self.regs.flag_z() { self.regs.pc = self.regs.pc.wrapping_add(e as u16); 12 } else { 8 }
            }
            0x28 => {
                let e = self.fetch_byte(mmu) as i8;
                if self.regs.flag_z() { self.regs.pc = self.regs.pc.wrapping_add(e as u16); 12 } else { 8 }
            }
            0x30 => {
                let e = self.fetch_byte(mmu) as i8;
                if !self.regs.flag_c() { self.regs.pc = self.regs.pc.wrapping_add(e as u16); 12 } else { 8 }
            }
            0x38 => {
                let e = self.fetch_byte(mmu) as i8;
                if self.regs.flag_c() { self.regs.pc = self.regs.pc.wrapping_add(e as u16); 12 } else { 8 }
            }

            // --- LD A, (r16) ---
            0x0A => { self.regs.a = self.read(mmu, self.regs.bc()); 8 }
            0x1A => { self.regs.a = self.read(mmu, self.regs.de()); 8 }
            0x2A => {
                let hl = self.regs.hl();
                self.regs.a = self.read(mmu, hl);
                self.regs.set_hl(hl.wrapping_add(1));
                8
            }
            0x3A => {
                let hl = self.regs.hl();
                self.regs.a = self.read(mmu, hl);
                self.regs.set_hl(hl.wrapping_sub(1));
                8
            }

            // --- ADD HL, r16 ---
            0x09 => { let v = self.regs.bc(); self.add_hl(v); 8 }
            0x19 => { let v = self.regs.de(); self.add_hl(v); 8 }
            0x29 => { let v = self.regs.hl(); self.add_hl(v); 8 }
            0x39 => { let v = self.regs.sp;   self.add_hl(v); 8 }

            // --- DAA ---
            0x27 => { self.daa(); 4 }

            // --- CPL ---
            0x2F => {
                self.regs.a = !self.regs.a;
                self.regs.f |= FLAG_N | FLAG_H;
                4
            }

            // --- SCF ---
            0x37 => {
                self.regs.f &= FLAG_Z;
                self.regs.f |= FLAG_C;
                4
            }

            // --- CCF ---
            0x3F => {
                let c = self.regs.flag_c();
                self.regs.f &= FLAG_Z;
                if !c { self.regs.f |= FLAG_C; }
                4
            }

            // LD (HL), (HL) does not exist; 0x76 is HALT
            0x76 => { self.halted = true; 4 }

            // --- LD r8, r8 block (0x40-0x7F) ---
            0x40 => 4,
            0x41 => { self.regs.b = self.regs.c; 4 }
            0x42 => { self.regs.b = self.regs.d; 4 }
            0x43 => { self.regs.b = self.regs.e; 4 }
            0x44 => { self.regs.b = self.regs.h; 4 }
            0x45 => { self.regs.b = self.regs.l; 4 }
            0x46 => { self.regs.b = self.read(mmu, self.regs.hl()); 8 }
            0x47 => { self.regs.b = self.regs.a; 4 }
            0x48 => { self.regs.c = self.regs.b; 4 }
            0x49 => 4,
            0x4A => { self.regs.c = self.regs.d; 4 }
            0x4B => { self.regs.c = self.regs.e; 4 }
            0x4C => { self.regs.c = self.regs.h; 4 }
            0x4D => { self.regs.c = self.regs.l; 4 }
            0x4E => { self.regs.c = self.read(mmu, self.regs.hl()); 8 }
            0x4F => { self.regs.c = self.regs.a; 4 }
            0x50 => { self.regs.d = self.regs.b; 4 }
            0x51 => { self.regs.d = self.regs.c; 4 }
            0x52 => 4,
            0x53 => { self.regs.d = self.regs.e; 4 }
            0x54 => { self.regs.d = self.regs.h; 4 }
            0x55 => { self.regs.d = self.regs.l; 4 }
            0x56 => { self.regs.d = self.read(mmu, self.regs.hl()); 8 }
            0x57 => { self.regs.d = self.regs.a; 4 }
            0x58 => { self.regs.e = self.regs.b; 4 }
            0x59 => { self.regs.e = self.regs.c; 4 }
            0x5A => { self.regs.e = self.regs.d; 4 }
            0x5B => 4,
            0x5C => { self.regs.e = self.regs.h; 4 }
            0x5D => { self.regs.e = self.regs.l; 4 }
            0x5E => { self.regs.e = self.read(mmu, self.regs.hl()); 8 }
            0x5F => { self.regs.e = self.regs.a; 4 }
            0x60 => { self.regs.h = self.regs.b; 4 }
            0x61 => { self.regs.h = self.regs.c; 4 }
            0x62 => { self.regs.h = self.regs.d; 4 }
            0x63 => { self.regs.h = self.regs.e; 4 }
            0x64 => 4,
            0x65 => { self.regs.h = self.regs.l; 4 }
            0x66 => { self.regs.h = self.read(mmu, self.regs.hl()); 8 }
            0x67 => { self.regs.h = self.regs.a; 4 }
            0x68 => { self.regs.l = self.regs.b; 4 }
            0x69 => { self.regs.l = self.regs.c; 4 }
            0x6A => { self.regs.l = self.regs.d; 4 }
            0x6B => { self.regs.l = self.regs.e; 4 }
            0x6C => { self.regs.l = self.regs.h; 4 }
            0x6D => 4,
            0x6E => { self.regs.l = self.read(mmu, self.regs.hl()); 8 }
            0x6F => { self.regs.l = self.regs.a; 4 }
            0x70 => { self.write(mmu, self.regs.hl(), self.regs.b); 8 }
            0x71 => { self.write(mmu, self.regs.hl(), self.regs.c); 8 }
            0x72 => { self.write(mmu, self.regs.hl(), self.regs.d); 8 }
            0x73 => { self.write(mmu, self.regs.hl(), self.regs.e); 8 }
            0x74 => { self.write(mmu, self.regs.hl(), self.regs.h); 8 }
            0x75 => { self.write(mmu, self.regs.hl(), self.regs.l); 8 }
            0x77 => { self.write(mmu, self.regs.hl(), self.regs.a); 8 }
            0x78 => { self.regs.a = self.regs.b; 4 }
            0x79 => { self.regs.a = self.regs.c; 4 }
            0x7A => { self.regs.a = self.regs.d; 4 }
            0x7B => { self.regs.a = self.regs.e; 4 }
            0x7C => { self.regs.a = self.regs.h; 4 }
            0x7D => { self.regs.a = self.regs.l; 4 }
            0x7E => { self.regs.a = self.read(mmu, self.regs.hl()); 8 }
            0x7F => 4,

            // --- ADD A, r8 ---
            0x80 => { let v = self.regs.b; self.add_a(v, false); 4 }
            0x81 => { let v = self.regs.c; self.add_a(v, false); 4 }
            0x82 => { let v = self.regs.d; self.add_a(v, false); 4 }
            0x83 => { let v = self.regs.e; self.add_a(v, false); 4 }
            0x84 => { let v = self.regs.h; self.add_a(v, false); 4 }
            0x85 => { let v = self.regs.l; self.add_a(v, false); 4 }
            0x86 => { let v = self.read(mmu, self.regs.hl()); self.add_a(v, false); 8 }
            0x87 => { let v = self.regs.a; self.add_a(v, false); 4 }

            // --- ADC A, r8 ---
            0x88 => { let v = self.regs.b; self.add_a(v, true); 4 }
            0x89 => { let v = self.regs.c; self.add_a(v, true); 4 }
            0x8A => { let v = self.regs.d; self.add_a(v, true); 4 }
            0x8B => { let v = self.regs.e; self.add_a(v, true); 4 }
            0x8C => { let v = self.regs.h; self.add_a(v, true); 4 }
            0x8D => { let v = self.regs.l; self.add_a(v, true); 4 }
            0x8E => { let v = self.read(mmu, self.regs.hl()); self.add_a(v, true); 8 }
            0x8F => { let v = self.regs.a; self.add_a(v, true); 4 }

            // --- SUB A, r8 ---
            0x90 => { let v = self.regs.b; self.sub_a(v, false); 4 }
            0x91 => { let v = self.regs.c; self.sub_a(v, false); 4 }
            0x92 => { let v = self.regs.d; self.sub_a(v, false); 4 }
            0x93 => { let v = self.regs.e; self.sub_a(v, false); 4 }
            0x94 => { let v = self.regs.h; self.sub_a(v, false); 4 }
            0x95 => { let v = self.regs.l; self.sub_a(v, false); 4 }
            0x96 => { let v = self.read(mmu, self.regs.hl()); self.sub_a(v, false); 8 }
            0x97 => { let v = self.regs.a; self.sub_a(v, false); 4 }

            // --- SBC A, r8 ---
            0x98 => { let v = self.regs.b; self.sub_a(v, true); 4 }
            0x99 => { let v = self.regs.c; self.sub_a(v, true); 4 }
            0x9A => { let v = self.regs.d; self.sub_a(v, true); 4 }
            0x9B => { let v = self.regs.e; self.sub_a(v, true); 4 }
            0x9C => { let v = self.regs.h; self.sub_a(v, true); 4 }
            0x9D => { let v = self.regs.l; self.sub_a(v, true); 4 }
            0x9E => { let v = self.read(mmu, self.regs.hl()); self.sub_a(v, true); 8 }
            0x9F => { let v = self.regs.a; self.sub_a(v, true); 4 }

            // --- AND A, r8 ---
            0xA0 => { let v = self.regs.b; self.and_a(v); 4 }
            0xA1 => { let v = self.regs.c; self.and_a(v); 4 }
            0xA2 => { let v = self.regs.d; self.and_a(v); 4 }
            0xA3 => { let v = self.regs.e; self.and_a(v); 4 }
            0xA4 => { let v = self.regs.h; self.and_a(v); 4 }
            0xA5 => { let v = self.regs.l; self.and_a(v); 4 }
            0xA6 => { let v = self.read(mmu, self.regs.hl()); self.and_a(v); 8 }
            0xA7 => { let v = self.regs.a; self.and_a(v); 4 }

            // --- XOR A, r8 ---
            0xA8 => { let v = self.regs.b; self.xor_a(v); 4 }
            0xA9 => { let v = self.regs.c; self.xor_a(v); 4 }
            0xAA => { let v = self.regs.d; self.xor_a(v); 4 }
            0xAB => { let v = self.regs.e; self.xor_a(v); 4 }
            0xAC => { let v = self.regs.h; self.xor_a(v); 4 }
            0xAD => { let v = self.regs.l; self.xor_a(v); 4 }
            0xAE => { let v = self.read(mmu, self.regs.hl()); self.xor_a(v); 8 }
            0xAF => { let v = self.regs.a; self.xor_a(v); 4 }

            // --- OR A, r8 ---
            0xB0 => { let v = self.regs.b; self.or_a(v); 4 }
            0xB1 => { let v = self.regs.c; self.or_a(v); 4 }
            0xB2 => { let v = self.regs.d; self.or_a(v); 4 }
            0xB3 => { let v = self.regs.e; self.or_a(v); 4 }
            0xB4 => { let v = self.regs.h; self.or_a(v); 4 }
            0xB5 => { let v = self.regs.l; self.or_a(v); 4 }
            0xB6 => { let v = self.read(mmu, self.regs.hl()); self.or_a(v); 8 }
            0xB7 => { let v = self.regs.a; self.or_a(v); 4 }

            // --- CP A, r8 ---
            0xB8 => { let v = self.regs.b; self.cp_a(v); 4 }
            0xB9 => { let v = self.regs.c; self.cp_a(v); 4 }
            0xBA => { let v = self.regs.d; self.cp_a(v); 4 }
            0xBB => { let v = self.regs.e; self.cp_a(v); 4 }
            0xBC => { let v = self.regs.h; self.cp_a(v); 4 }
            0xBD => { let v = self.regs.l; self.cp_a(v); 4 }
            0xBE => { let v = self.read(mmu, self.regs.hl()); self.cp_a(v); 8 }
            0xBF => { let v = self.regs.a; self.cp_a(v); 4 }

            // --- RET cc ---
            0xC0 => { if !self.regs.flag_z() { self.regs.pc = self.pop_word(mmu); 20 } else { 8 } }
            0xC8 => { if  self.regs.flag_z() { self.regs.pc = self.pop_word(mmu); 20 } else { 8 } }
            0xD0 => { if !self.regs.flag_c() { self.regs.pc = self.pop_word(mmu); 20 } else { 8 } }
            0xD8 => { if  self.regs.flag_c() { self.regs.pc = self.pop_word(mmu); 20 } else { 8 } }

            // --- POP r16 ---
            0xC1 => { let v = self.pop_word(mmu); self.regs.set_bc(v); 12 }
            0xD1 => { let v = self.pop_word(mmu); self.regs.set_de(v); 12 }
            0xE1 => { let v = self.pop_word(mmu); self.regs.set_hl(v); 12 }
            0xF1 => { let v = self.pop_word(mmu); self.regs.set_af(v); 12 }

            // --- JP cc, a16 ---
            0xC2 => {
                let a = self.fetch_word(mmu);
                if !self.regs.flag_z() { self.regs.pc = a; 16 } else { 12 }
            }
            0xCA => {
                let a = self.fetch_word(mmu);
                if  self.regs.flag_z() { self.regs.pc = a; 16 } else { 12 }
            }
            0xD2 => {
                let a = self.fetch_word(mmu);
                if !self.regs.flag_c() { self.regs.pc = a; 16 } else { 12 }
            }
            0xDA => {
                let a = self.fetch_word(mmu);
                if  self.regs.flag_c() { self.regs.pc = a; 16 } else { 12 }
            }

            // --- JP a16 ---
            0xC3 => { self.regs.pc = self.fetch_word(mmu); 16 }

            // --- CB prefix ---
            0xCB => { let op2 = self.fetch_byte(mmu); self.execute_cb(op2, mmu) }

            // --- CALL cc, a16 ---
            0xC4 => {
                let a = self.fetch_word(mmu);
                if !self.regs.flag_z() { self.push_word(mmu, self.regs.pc); self.regs.pc = a; 24 } else { 12 }
            }
            0xCC => {
                let a = self.fetch_word(mmu);
                if  self.regs.flag_z() { self.push_word(mmu, self.regs.pc); self.regs.pc = a; 24 } else { 12 }
            }
            0xD4 => {
                let a = self.fetch_word(mmu);
                if !self.regs.flag_c() { self.push_word(mmu, self.regs.pc); self.regs.pc = a; 24 } else { 12 }
            }
            0xDC => {
                let a = self.fetch_word(mmu);
                if  self.regs.flag_c() { self.push_word(mmu, self.regs.pc); self.regs.pc = a; 24 } else { 12 }
            }

            // --- PUSH r16 ---
            0xC5 => { let v = self.regs.bc(); self.push_word(mmu, v); 16 }
            0xD5 => { let v = self.regs.de(); self.push_word(mmu, v); 16 }
            0xE5 => { let v = self.regs.hl(); self.push_word(mmu, v); 16 }
            0xF5 => { let v = self.regs.af(); self.push_word(mmu, v); 16 }

            // --- ADD/ADC/SUB/SBC/AND/XOR/OR/CP A, d8 ---
            0xC6 => { let v = self.fetch_byte(mmu); self.add_a(v, false); 8 }
            0xCE => { let v = self.fetch_byte(mmu); self.add_a(v, true);  8 }
            0xD6 => { let v = self.fetch_byte(mmu); self.sub_a(v, false); 8 }
            0xDE => { let v = self.fetch_byte(mmu); self.sub_a(v, true);  8 }
            0xE6 => { let v = self.fetch_byte(mmu); self.and_a(v); 8 }
            0xEE => { let v = self.fetch_byte(mmu); self.xor_a(v); 8 }
            0xF6 => { let v = self.fetch_byte(mmu); self.or_a(v);  8 }
            0xFE => { let v = self.fetch_byte(mmu); self.cp_a(v);  8 }

            // --- RST ---
            0xC7 => { self.push_word(mmu, self.regs.pc); self.regs.pc = 0x00; 16 }
            0xCF => { self.push_word(mmu, self.regs.pc); self.regs.pc = 0x08; 16 }
            0xD7 => { self.push_word(mmu, self.regs.pc); self.regs.pc = 0x10; 16 }
            0xDF => { self.push_word(mmu, self.regs.pc); self.regs.pc = 0x18; 16 }
            0xE7 => { self.push_word(mmu, self.regs.pc); self.regs.pc = 0x20; 16 }
            0xEF => { self.push_word(mmu, self.regs.pc); self.regs.pc = 0x28; 16 }
            0xF7 => { self.push_word(mmu, self.regs.pc); self.regs.pc = 0x30; 16 }
            0xFF => { self.push_word(mmu, self.regs.pc); self.regs.pc = 0x38; 16 }

            // --- RET ---
            0xC9 => { self.regs.pc = self.pop_word(mmu); 16 }

            // --- RETI ---
            0xD9 => {
                self.regs.pc = self.pop_word(mmu);
                self.ime = true;
                16
            }

            // --- JP (HL) ---
            0xE9 => { self.regs.pc = self.regs.hl(); 4 }

            // --- CALL a16 ---
            0xCD => {
                let a = self.fetch_word(mmu);
                self.push_word(mmu, self.regs.pc);
                self.regs.pc = a;
                24
            }

            // --- LD (0xFF00+C), A / LD A, (0xFF00+C) ---
            0xE2 => {
                let addr = 0xFF00 | self.regs.c as u16;
                self.write(mmu, addr, self.regs.a);
                8
            }
            0xF2 => {
                let addr = 0xFF00 | self.regs.c as u16;
                self.regs.a = self.read(mmu, addr);
                8
            }

            // --- LD (0xFF00+d8), A / LD A, (0xFF00+d8) ---
            0xE0 => {
                let offset = self.fetch_byte(mmu) as u16;
                self.write(mmu, 0xFF00 | offset, self.regs.a);
                12
            }
            0xF0 => {
                let offset = self.fetch_byte(mmu) as u16;
                self.regs.a = self.read(mmu, 0xFF00 | offset);
                12
            }

            // --- LD (a16), A / LD A, (a16) ---
            0xEA => {
                let addr = self.fetch_word(mmu);
                self.write(mmu, addr, self.regs.a);
                16
            }
            0xFA => {
                let addr = self.fetch_word(mmu);
                self.regs.a = self.read(mmu, addr);
                16
            }

            // --- LD (a16), SP ---
            0x08 => {
                let addr = self.fetch_word(mmu);
                mmu.write_word(addr, self.regs.sp);
                20
            }

            // --- ADD SP, r8 ---
            0xE8 => {
                let e = self.fetch_byte(mmu) as i8 as i16;
                let sp = self.regs.sp as i16;
                let result = sp.wrapping_add(e);
                let check = sp ^ e ^ result;
                self.regs.set_flags(false, false, check & 0x10 != 0, check & 0x100 != 0);
                self.regs.sp = result as u16;
                16
            }

            // --- LD HL, SP+r8 ---
            0xF8 => {
                let e = self.fetch_byte(mmu) as i8 as i16;
                let sp = self.regs.sp as i16;
                let result = sp.wrapping_add(e);
                let check = sp ^ e ^ result;
                self.regs.set_flags(false, false, check & 0x10 != 0, check & 0x100 != 0);
                self.regs.set_hl(result as u16);
                12
            }

            // --- LD SP, HL ---
            0xF9 => { self.regs.sp = self.regs.hl(); 8 }

            // --- DI / EI ---
            0xF3 => { self.ime = false; self.ime_pending = false; 4 }
            0xFB => { self.ime_pending = true; 4 }

            // --- Undefined / illegal opcodes – treat as NOP ---
            _ => 4,
        }
    }

    // -----------------------------------------------------------------------
    // CB-prefix opcodes
    // -----------------------------------------------------------------------
    fn execute_cb(&mut self, op: u8, mmu: &mut Mmu) -> u32 {
        let reg_idx = op & 0x07;
        let bit_num = (op >> 3) & 0x07;
        let group   = op >> 6;

        // Read operand
        let val = match reg_idx {
            0 => self.regs.b,
            1 => self.regs.c,
            2 => self.regs.d,
            3 => self.regs.e,
            4 => self.regs.h,
            5 => self.regs.l,
            6 => self.read(mmu, self.regs.hl()),
            7 => self.regs.a,
            _ => unreachable!(),
        };

        let is_hl = reg_idx == 6;

        let result = match group {
            0 => match bit_num {
                0 => self.op_rlc(val),
                1 => self.op_rrc(val),
                2 => self.op_rl(val),
                3 => self.op_rr(val),
                4 => self.op_sla(val),
                5 => self.op_sra(val),
                6 => self.op_swap(val),
                7 => self.op_srl(val),
                _ => unreachable!(),
            },
            1 => {
                // BIT n, r
                let z = (val & (1 << bit_num)) == 0;
                let n = false;
                let h = true;
                let c = self.regs.flag_c();
                self.regs.set_flags(z, n, h, c);
                val // value unchanged, no writeback needed for BIT
            }
            2 => val & !(1 << bit_num), // RES n, r
            3 => val |  (1 << bit_num), // SET n, r
            _ => unreachable!(),
        };

        // For BIT, no writeback
        if group != 1 {
            match reg_idx {
                0 => self.regs.b = result,
                1 => self.regs.c = result,
                2 => self.regs.d = result,
                3 => self.regs.e = result,
                4 => self.regs.h = result,
                5 => self.regs.l = result,
                6 => self.write(mmu, self.regs.hl(), result),
                7 => self.regs.a = result,
                _ => unreachable!(),
            }
        }

        if is_hl { 16 } else { 8 }
    }

    // -----------------------------------------------------------------------
    // ALU helpers
    // -----------------------------------------------------------------------

    fn inc8(&mut self, v: u8) -> u8 {
        let r = v.wrapping_add(1);
        let z = r == 0;
        let h = (v & 0x0F) == 0x0F;
        let c = self.regs.flag_c();
        self.regs.set_flags(z, false, h, c);
        r
    }

    fn dec8(&mut self, v: u8) -> u8 {
        let r = v.wrapping_sub(1);
        let z = r == 0;
        let h = (v & 0x0F) == 0x00;
        let c = self.regs.flag_c();
        self.regs.set_flags(z, true, h, c);
        r
    }

    fn add_a(&mut self, v: u8, with_carry: bool) {
        let carry = if with_carry && self.regs.flag_c() { 1u8 } else { 0 };
        let a = self.regs.a;
        let result = a.wrapping_add(v).wrapping_add(carry);
        let z = result == 0;
        let h = ((a & 0x0F) + (v & 0x0F) + carry) > 0x0F;
        let c = (a as u16 + v as u16 + carry as u16) > 0xFF;
        self.regs.set_flags(z, false, h, c);
        self.regs.a = result;
    }

    fn sub_a(&mut self, v: u8, with_carry: bool) {
        let carry = if with_carry && self.regs.flag_c() { 1u8 } else { 0 };
        let a = self.regs.a;
        let result = a.wrapping_sub(v).wrapping_sub(carry);
        let z = result == 0;
        let h = (a & 0x0F) < (v & 0x0F) + carry;
        let c = (a as u16) < (v as u16 + carry as u16);
        self.regs.set_flags(z, true, h, c);
        self.regs.a = result;
    }

    fn and_a(&mut self, v: u8) {
        self.regs.a &= v;
        let z = self.regs.a == 0;
        self.regs.set_flags(z, false, true, false);
    }

    fn or_a(&mut self, v: u8) {
        self.regs.a |= v;
        let z = self.regs.a == 0;
        self.regs.set_flags(z, false, false, false);
    }

    fn xor_a(&mut self, v: u8) {
        self.regs.a ^= v;
        let z = self.regs.a == 0;
        self.regs.set_flags(z, false, false, false);
    }

    fn cp_a(&mut self, v: u8) {
        let a = self.regs.a;
        let result = a.wrapping_sub(v);
        let z = result == 0;
        let h = (a & 0x0F) < (v & 0x0F);
        let c = a < v;
        self.regs.set_flags(z, true, h, c);
    }

    fn add_hl(&mut self, v: u16) {
        let hl = self.regs.hl();
        let result = hl.wrapping_add(v);
        let h = ((hl & 0x0FFF) + (v & 0x0FFF)) > 0x0FFF;
        let c = (hl as u32 + v as u32) > 0xFFFF;
        let z = self.regs.flag_z();
        self.regs.set_flags(z, false, h, c);
        self.regs.set_hl(result);
    }

    fn daa(&mut self) {
        let mut a = self.regs.a;
        let n = self.regs.flag_n();
        let h = self.regs.flag_h();
        let c = self.regs.flag_c();
        let mut new_c = false;

        if !n {
            if c || a > 0x99 { a = a.wrapping_add(0x60); new_c = true; }
            if h || (a & 0x0F) > 0x09 { a = a.wrapping_add(0x06); }
        } else {
            if c { a = a.wrapping_sub(0x60); new_c = true; }
            if h { a = a.wrapping_sub(0x06); }
        }

        self.regs.a = a;
        let z = a == 0;
        self.regs.set_flags(z, n, false, new_c);
    }

    // --- Rotate / shift ---

    fn rlca(&mut self) {
        let c = (self.regs.a >> 7) & 1;
        self.regs.a = (self.regs.a << 1) | c;
        self.regs.set_flags(false, false, false, c != 0);
    }

    fn rrca(&mut self) {
        let c = self.regs.a & 1;
        self.regs.a = (self.regs.a >> 1) | (c << 7);
        self.regs.set_flags(false, false, false, c != 0);
    }

    fn rla(&mut self) {
        let old_c = if self.regs.flag_c() { 1u8 } else { 0 };
        let new_c = (self.regs.a >> 7) & 1;
        self.regs.a = (self.regs.a << 1) | old_c;
        self.regs.set_flags(false, false, false, new_c != 0);
    }

    fn rra(&mut self) {
        let old_c = if self.regs.flag_c() { 0x80u8 } else { 0 };
        let new_c = self.regs.a & 1;
        self.regs.a = (self.regs.a >> 1) | old_c;
        self.regs.set_flags(false, false, false, new_c != 0);
    }

    // CB rotate/shift helpers – update flags, return result

    fn op_rlc(&mut self, v: u8) -> u8 {
        let c = (v >> 7) & 1;
        let r = (v << 1) | c;
        self.regs.set_flags(r == 0, false, false, c != 0);
        r
    }

    fn op_rrc(&mut self, v: u8) -> u8 {
        let c = v & 1;
        let r = (v >> 1) | (c << 7);
        self.regs.set_flags(r == 0, false, false, c != 0);
        r
    }

    fn op_rl(&mut self, v: u8) -> u8 {
        let old_c = if self.regs.flag_c() { 1u8 } else { 0 };
        let new_c = (v >> 7) & 1;
        let r = (v << 1) | old_c;
        self.regs.set_flags(r == 0, false, false, new_c != 0);
        r
    }

    fn op_rr(&mut self, v: u8) -> u8 {
        let old_c = if self.regs.flag_c() { 0x80u8 } else { 0 };
        let new_c = v & 1;
        let r = (v >> 1) | old_c;
        self.regs.set_flags(r == 0, false, false, new_c != 0);
        r
    }

    fn op_sla(&mut self, v: u8) -> u8 {
        let c = (v >> 7) & 1;
        let r = v << 1;
        self.regs.set_flags(r == 0, false, false, c != 0);
        r
    }

    fn op_sra(&mut self, v: u8) -> u8 {
        let c = v & 1;
        let r = (v >> 1) | (v & 0x80);
        self.regs.set_flags(r == 0, false, false, c != 0);
        r
    }

    fn op_swap(&mut self, v: u8) -> u8 {
        let r = (v >> 4) | (v << 4);
        self.regs.set_flags(r == 0, false, false, false);
        r
    }

    fn op_srl(&mut self, v: u8) -> u8 {
        let c = v & 1;
        let r = v >> 1;
        self.regs.set_flags(r == 0, false, false, c != 0);
        r
    }

    // -----------------------------------------------------------------------
    // Save / Load state
    // -----------------------------------------------------------------------
    pub fn save(&self, buf: &mut Vec<u8>) {
        write_u8(buf, self.regs.a);
        write_u8(buf, self.regs.f);
        write_u8(buf, self.regs.b);
        write_u8(buf, self.regs.c);
        write_u8(buf, self.regs.d);
        write_u8(buf, self.regs.e);
        write_u8(buf, self.regs.h);
        write_u8(buf, self.regs.l);
        write_u16(buf, self.regs.sp);
        write_u16(buf, self.regs.pc);
        write_bool(buf, self.ime);
        write_bool(buf, self.halted);
        write_bool(buf, self.stopped);
        write_bool(buf, self.ime_pending);
    }

    pub fn load(&mut self, data: &[u8], off: &mut usize) {
        self.regs.a = read_u8(data, off);
        self.regs.f = read_u8(data, off);
        self.regs.b = read_u8(data, off);
        self.regs.c = read_u8(data, off);
        self.regs.d = read_u8(data, off);
        self.regs.e = read_u8(data, off);
        self.regs.h = read_u8(data, off);
        self.regs.l = read_u8(data, off);
        self.regs.sp = read_u16(data, off);
        self.regs.pc = read_u16(data, off);
        self.ime = read_bool(data, off);
        self.halted = read_bool(data, off);
        self.stopped = read_bool(data, off);
        self.ime_pending = read_bool(data, off);
    }
}
