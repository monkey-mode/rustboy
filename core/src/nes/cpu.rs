/// Ricoh 2A03 CPU – 6502 without BCD mode.
///
/// All 56 official opcodes, all addressing modes.

use super::bus::NesBus;

// ---------------------------------------------------------------------------
// Flag bits
// ---------------------------------------------------------------------------
const FLAG_C: u8 = 0x01;
const FLAG_Z: u8 = 0x02;
const FLAG_I: u8 = 0x04;
const FLAG_D: u8 = 0x08; // decimal – ignored in 2A03 but present in P
const FLAG_B: u8 = 0x10;
const FLAG_U: u8 = 0x20; // always 1
const FLAG_V: u8 = 0x40;
const FLAG_N: u8 = 0x80;

// ---------------------------------------------------------------------------
// Addressing modes
// ---------------------------------------------------------------------------
#[derive(Clone, Copy)]
#[allow(dead_code)]
enum Mode {
    Imp,        // Implied
    Acc,        // Accumulator
    Imm,        // Immediate
    Zp0,        // Zero Page
    ZpX,        // Zero Page + X
    ZpY,        // Zero Page + Y
    Abs,        // Absolute
    AbX,        // Absolute + X (page-crossing penalty)
    AbY,        // Absolute + Y (page-crossing penalty)
    Ind,        // Indirect (JMP only)
    IdX,        // Indexed Indirect (X)  – (ZP,X)
    IdY,        // Indirect Indexed (Y)  – (ZP),Y
    Rel,        // Relative
}

// ---------------------------------------------------------------------------
// CPU
// ---------------------------------------------------------------------------
pub struct NesCpu {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub p: u8,  // status flags
    pub sp: u8,
    pub pc: u16,
    cycles: u32, // remaining cycles for current instruction
}

impl NesCpu {
    pub fn new() -> Self {
        NesCpu { a: 0, x: 0, y: 0, p: FLAG_U | FLAG_I, sp: 0xFD, pc: 0, cycles: 0 }
    }

    pub fn reset(&mut self, bus: &mut NesBus) {
        let lo = bus.read(0xFFFC) as u16;
        let hi = bus.read(0xFFFD) as u16;
        self.pc = (hi << 8) | lo;
        self.a = 0;
        self.x = 0;
        self.y = 0;
        self.sp = 0xFD;
        self.p = FLAG_U | FLAG_I;
        self.cycles = 8;
    }

    pub fn nmi(&mut self, bus: &mut NesBus) {
        self.push_word(bus, self.pc);
        self.push(bus, (self.p | FLAG_U) & !FLAG_B);
        self.p |= FLAG_I;
        let lo = bus.read(0xFFFA) as u16;
        let hi = bus.read(0xFFFB) as u16;
        self.pc = (hi << 8) | lo;
        self.cycles += 8;
    }

    pub fn irq(&mut self, bus: &mut NesBus) {
        if self.p & FLAG_I != 0 { return; }
        self.push_word(bus, self.pc);
        self.push(bus, (self.p | FLAG_U) & !FLAG_B);
        self.p |= FLAG_I;
        let lo = bus.read(0xFFFE) as u16;
        let hi = bus.read(0xFFFF) as u16;
        self.pc = (hi << 8) | lo;
        self.cycles += 7;
    }

    /// Execute one instruction and return the number of cycles consumed.
    pub fn step(&mut self, bus: &mut NesBus) -> u32 {
        let opcode = self.fetch(bus);
        self.execute(opcode, bus)
    }

    // -----------------------------------------------------------------------
    // Stack helpers
    // -----------------------------------------------------------------------
    fn push(&mut self, bus: &mut NesBus, val: u8) {
        bus.write(0x0100 | self.sp as u16, val);
        self.sp = self.sp.wrapping_sub(1);
    }

    fn pull(&mut self, bus: &mut NesBus) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        bus.read(0x0100 | self.sp as u16)
    }

    fn push_word(&mut self, bus: &mut NesBus, val: u16) {
        self.push(bus, (val >> 8) as u8);
        self.push(bus, (val & 0xFF) as u8);
    }

    fn pull_word(&mut self, bus: &mut NesBus) -> u16 {
        let lo = self.pull(bus) as u16;
        let hi = self.pull(bus) as u16;
        (hi << 8) | lo
    }

    fn fetch(&mut self, bus: &mut NesBus) -> u8 {
        let val = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        val
    }

    fn fetch_word(&mut self, bus: &mut NesBus) -> u16 {
        let lo = self.fetch(bus) as u16;
        let hi = self.fetch(bus) as u16;
        (hi << 8) | lo
    }

    // -----------------------------------------------------------------------
    // Flag helpers
    // -----------------------------------------------------------------------
    fn set_flag(&mut self, flag: u8, val: bool) {
        if val { self.p |= flag; } else { self.p &= !flag; }
    }

    fn get_flag(&self, flag: u8) -> bool { self.p & flag != 0 }

    fn set_nz(&mut self, val: u8) {
        self.set_flag(FLAG_Z, val == 0);
        self.set_flag(FLAG_N, val & 0x80 != 0);
    }

    // -----------------------------------------------------------------------
    // Addressing mode resolution
    // Returns (effective address, page_crossed)
    // -----------------------------------------------------------------------
    fn resolve_addr(&mut self, mode: Mode, bus: &mut NesBus) -> (u16, bool) {
        match mode {
            Mode::Imp | Mode::Acc => (0, false),
            Mode::Imm => {
                let addr = self.pc;
                self.pc = self.pc.wrapping_add(1);
                (addr, false)
            }
            Mode::Zp0 => {
                let addr = self.fetch(bus) as u16;
                (addr, false)
            }
            Mode::ZpX => {
                let base = self.fetch(bus);
                (base.wrapping_add(self.x) as u16, false)
            }
            Mode::ZpY => {
                let base = self.fetch(bus);
                (base.wrapping_add(self.y) as u16, false)
            }
            Mode::Abs => {
                let addr = self.fetch_word(bus);
                (addr, false)
            }
            Mode::AbX => {
                let base = self.fetch_word(bus);
                let addr = base.wrapping_add(self.x as u16);
                let crossed = (base & 0xFF00) != (addr & 0xFF00);
                (addr, crossed)
            }
            Mode::AbY => {
                let base = self.fetch_word(bus);
                let addr = base.wrapping_add(self.y as u16);
                let crossed = (base & 0xFF00) != (addr & 0xFF00);
                (addr, crossed)
            }
            Mode::Ind => {
                let ptr = self.fetch_word(bus);
                // 6502 page-boundary bug: if ptr = $xxFF, high byte read from $xx00
                let lo = bus.read(ptr) as u16;
                let hi_ptr = if ptr & 0xFF == 0xFF { ptr & 0xFF00 } else { ptr + 1 };
                let hi = bus.read(hi_ptr) as u16;
                ((hi << 8) | lo, false)
            }
            Mode::IdX => {
                let base = self.fetch(bus);
                let ptr = base.wrapping_add(self.x) as u16;
                let lo = bus.read(ptr & 0xFF) as u16;
                let hi = bus.read((ptr + 1) & 0xFF) as u16;
                ((hi << 8) | lo, false)
            }
            Mode::IdY => {
                let ptr = self.fetch(bus) as u16;
                let lo = bus.read(ptr & 0xFF) as u16;
                let hi = bus.read((ptr + 1) & 0xFF) as u16;
                let base = (hi << 8) | lo;
                let addr = base.wrapping_add(self.y as u16);
                let crossed = (base & 0xFF00) != (addr & 0xFF00);
                (addr, crossed)
            }
            Mode::Rel => {
                let offset = self.fetch(bus) as i8 as i16;
                let addr = (self.pc as i16).wrapping_add(offset) as u16;
                (addr, (self.pc & 0xFF00) != (addr & 0xFF00))
            }
        }
    }

    // -----------------------------------------------------------------------
    // Execute opcode
    // -----------------------------------------------------------------------
    fn execute(&mut self, opcode: u8, bus: &mut NesBus) -> u32 {
        // Returns cycles consumed (base + any page-cross/branch penalties)
        match opcode {
            // BRK
            0x00 => {
                self.pc = self.pc.wrapping_add(1); // skip padding byte
                self.push_word(bus, self.pc);
                self.push(bus, self.p | FLAG_B | FLAG_U);
                self.p |= FLAG_I;
                let lo = bus.read(0xFFFE) as u16;
                let hi = bus.read(0xFFFF) as u16;
                self.pc = (hi << 8) | lo;
                7
            }
            // ORA
            0x01 => { let (a, _) = self.resolve_addr(Mode::IdX, bus); let v = bus.read(a); self.a |= v; let r = self.a; self.set_nz(r); 6 }
            0x05 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); let v = bus.read(a); self.a |= v; let r = self.a; self.set_nz(r); 3 }
            0x09 => { let (a, _) = self.resolve_addr(Mode::Imm, bus); let v = bus.read(a); self.a |= v; let r = self.a; self.set_nz(r); 2 }
            0x0D => { let (a, _) = self.resolve_addr(Mode::Abs, bus); let v = bus.read(a); self.a |= v; let r = self.a; self.set_nz(r); 4 }
            0x11 => { let (a, p) = self.resolve_addr(Mode::IdY, bus); let v = bus.read(a); self.a |= v; let r = self.a; self.set_nz(r); 5 + p as u32 }
            0x15 => { let (a, _) = self.resolve_addr(Mode::ZpX, bus); let v = bus.read(a); self.a |= v; let r = self.a; self.set_nz(r); 4 }
            0x19 => { let (a, p) = self.resolve_addr(Mode::AbY, bus); let v = bus.read(a); self.a |= v; let r = self.a; self.set_nz(r); 4 + p as u32 }
            0x1D => { let (a, p) = self.resolve_addr(Mode::AbX, bus); let v = bus.read(a); self.a |= v; let r = self.a; self.set_nz(r); 4 + p as u32 }
            // ASL
            0x06 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); let v = self.asl_mem(a, bus); self.set_nz(v); 5 }
            0x0A => { self.asl_acc(); 2 }
            0x0E => { let (a, _) = self.resolve_addr(Mode::Abs, bus); let v = self.asl_mem(a, bus); self.set_nz(v); 6 }
            0x16 => { let (a, _) = self.resolve_addr(Mode::ZpX, bus); let v = self.asl_mem(a, bus); self.set_nz(v); 6 }
            0x1E => { let (a, _) = self.resolve_addr(Mode::AbX, bus); let v = self.asl_mem(a, bus); self.set_nz(v); 7 }
            // PHP / PLP
            0x08 => { self.push(bus, self.p | FLAG_B | FLAG_U); 3 }
            0x28 => { let v = self.pull(bus); self.p = (v | FLAG_U) & !FLAG_B; 4 }
            // BPL
            0x10 => { let (a, p) = self.resolve_addr(Mode::Rel, bus); self.branch(!self.get_flag(FLAG_N), a, p) }
            // CLC
            0x18 => { self.set_flag(FLAG_C, false); 2 }
            // JSR
            0x20 => {
                let addr = self.fetch_word(bus);
                self.push_word(bus, self.pc.wrapping_sub(1));
                self.pc = addr;
                6
            }
            // AND
            0x21 => { let (a, _) = self.resolve_addr(Mode::IdX, bus); let v = bus.read(a); self.a &= v; let r = self.a; self.set_nz(r); 6 }
            0x25 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); let v = bus.read(a); self.a &= v; let r = self.a; self.set_nz(r); 3 }
            0x29 => { let (a, _) = self.resolve_addr(Mode::Imm, bus); let v = bus.read(a); self.a &= v; let r = self.a; self.set_nz(r); 2 }
            0x2D => { let (a, _) = self.resolve_addr(Mode::Abs, bus); let v = bus.read(a); self.a &= v; let r = self.a; self.set_nz(r); 4 }
            0x31 => { let (a, p) = self.resolve_addr(Mode::IdY, bus); let v = bus.read(a); self.a &= v; let r = self.a; self.set_nz(r); 5 + p as u32 }
            0x35 => { let (a, _) = self.resolve_addr(Mode::ZpX, bus); let v = bus.read(a); self.a &= v; let r = self.a; self.set_nz(r); 4 }
            0x39 => { let (a, p) = self.resolve_addr(Mode::AbY, bus); let v = bus.read(a); self.a &= v; let r = self.a; self.set_nz(r); 4 + p as u32 }
            0x3D => { let (a, p) = self.resolve_addr(Mode::AbX, bus); let v = bus.read(a); self.a &= v; let r = self.a; self.set_nz(r); 4 + p as u32 }
            // BIT
            0x24 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); self.bit_test(a, bus); 3 }
            0x2C => { let (a, _) = self.resolve_addr(Mode::Abs, bus); self.bit_test(a, bus); 4 }
            // ROL
            0x26 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); let v = self.rol_mem(a, bus); self.set_nz(v); 5 }
            0x2A => { self.rol_acc(); 2 }
            0x2E => { let (a, _) = self.resolve_addr(Mode::Abs, bus); let v = self.rol_mem(a, bus); self.set_nz(v); 6 }
            0x36 => { let (a, _) = self.resolve_addr(Mode::ZpX, bus); let v = self.rol_mem(a, bus); self.set_nz(v); 6 }
            0x3E => { let (a, _) = self.resolve_addr(Mode::AbX, bus); let v = self.rol_mem(a, bus); self.set_nz(v); 7 }
            // BMI
            0x30 => { let (a, p) = self.resolve_addr(Mode::Rel, bus); self.branch(self.get_flag(FLAG_N), a, p) }
            // SEC
            0x38 => { self.set_flag(FLAG_C, true); 2 }
            // RTI
            0x40 => {
                let p = self.pull(bus);
                self.p = (p | FLAG_U) & !FLAG_B;
                self.pc = self.pull_word(bus);
                6
            }
            // EOR
            0x41 => { let (a, _) = self.resolve_addr(Mode::IdX, bus); let v = bus.read(a); self.a ^= v; let r = self.a; self.set_nz(r); 6 }
            0x45 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); let v = bus.read(a); self.a ^= v; let r = self.a; self.set_nz(r); 3 }
            0x49 => { let (a, _) = self.resolve_addr(Mode::Imm, bus); let v = bus.read(a); self.a ^= v; let r = self.a; self.set_nz(r); 2 }
            0x4D => { let (a, _) = self.resolve_addr(Mode::Abs, bus); let v = bus.read(a); self.a ^= v; let r = self.a; self.set_nz(r); 4 }
            0x51 => { let (a, p) = self.resolve_addr(Mode::IdY, bus); let v = bus.read(a); self.a ^= v; let r = self.a; self.set_nz(r); 5 + p as u32 }
            0x55 => { let (a, _) = self.resolve_addr(Mode::ZpX, bus); let v = bus.read(a); self.a ^= v; let r = self.a; self.set_nz(r); 4 }
            0x59 => { let (a, p) = self.resolve_addr(Mode::AbY, bus); let v = bus.read(a); self.a ^= v; let r = self.a; self.set_nz(r); 4 + p as u32 }
            0x5D => { let (a, p) = self.resolve_addr(Mode::AbX, bus); let v = bus.read(a); self.a ^= v; let r = self.a; self.set_nz(r); 4 + p as u32 }
            // LSR
            0x46 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); let v = self.lsr_mem(a, bus); self.set_nz(v); 5 }
            0x4A => { self.lsr_acc(); 2 }
            0x4E => { let (a, _) = self.resolve_addr(Mode::Abs, bus); let v = self.lsr_mem(a, bus); self.set_nz(v); 6 }
            0x56 => { let (a, _) = self.resolve_addr(Mode::ZpX, bus); let v = self.lsr_mem(a, bus); self.set_nz(v); 6 }
            0x5E => { let (a, _) = self.resolve_addr(Mode::AbX, bus); let v = self.lsr_mem(a, bus); self.set_nz(v); 7 }
            // PHA / PLA
            0x48 => { self.push(bus, self.a); 3 }
            0x68 => { self.a = self.pull(bus); let v = self.a; self.set_nz(v); 4 }
            // JMP
            0x4C => { let (a, _) = self.resolve_addr(Mode::Abs, bus); self.pc = a; 3 }
            0x6C => { let (a, _) = self.resolve_addr(Mode::Ind, bus); self.pc = a; 5 }
            // BVC / BVS
            0x50 => { let (a, p) = self.resolve_addr(Mode::Rel, bus); self.branch(!self.get_flag(FLAG_V), a, p) }
            0x70 => { let (a, p) = self.resolve_addr(Mode::Rel, bus); self.branch(self.get_flag(FLAG_V), a, p) }
            // CLI / SEI
            0x58 => { self.set_flag(FLAG_I, false); 2 }
            0x78 => { self.set_flag(FLAG_I, true); 2 }
            // RTS
            0x60 => {
                self.pc = self.pull_word(bus).wrapping_add(1);
                6
            }
            // ADC
            0x61 => { let (a, _) = self.resolve_addr(Mode::IdX, bus); let v = bus.read(a); self.adc(v); 6 }
            0x65 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); let v = bus.read(a); self.adc(v); 3 }
            0x69 => { let (a, _) = self.resolve_addr(Mode::Imm, bus); let v = bus.read(a); self.adc(v); 2 }
            0x6D => { let (a, _) = self.resolve_addr(Mode::Abs, bus); let v = bus.read(a); self.adc(v); 4 }
            0x71 => { let (a, p) = self.resolve_addr(Mode::IdY, bus); let v = bus.read(a); self.adc(v); 5 + p as u32 }
            0x75 => { let (a, _) = self.resolve_addr(Mode::ZpX, bus); let v = bus.read(a); self.adc(v); 4 }
            0x79 => { let (a, p) = self.resolve_addr(Mode::AbY, bus); let v = bus.read(a); self.adc(v); 4 + p as u32 }
            0x7D => { let (a, p) = self.resolve_addr(Mode::AbX, bus); let v = bus.read(a); self.adc(v); 4 + p as u32 }
            // ROR
            0x66 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); let v = self.ror_mem(a, bus); self.set_nz(v); 5 }
            0x6A => { self.ror_acc(); 2 }
            0x6E => { let (a, _) = self.resolve_addr(Mode::Abs, bus); let v = self.ror_mem(a, bus); self.set_nz(v); 6 }
            0x76 => { let (a, _) = self.resolve_addr(Mode::ZpX, bus); let v = self.ror_mem(a, bus); self.set_nz(v); 6 }
            0x7E => { let (a, _) = self.resolve_addr(Mode::AbX, bus); let v = self.ror_mem(a, bus); self.set_nz(v); 7 }
            // STA
            0x81 => { let (a, _) = self.resolve_addr(Mode::IdX, bus); bus.write(a, self.a); 6 }
            0x85 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); bus.write(a, self.a); 3 }
            0x8D => { let (a, _) = self.resolve_addr(Mode::Abs, bus); bus.write(a, self.a); 4 }
            0x91 => { let (a, _) = self.resolve_addr(Mode::IdY, bus); bus.write(a, self.a); 6 }
            0x95 => { let (a, _) = self.resolve_addr(Mode::ZpX, bus); bus.write(a, self.a); 4 }
            0x99 => { let (a, _) = self.resolve_addr(Mode::AbY, bus); bus.write(a, self.a); 5 }
            0x9D => { let (a, _) = self.resolve_addr(Mode::AbX, bus); bus.write(a, self.a); 5 }
            // STX
            0x86 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); bus.write(a, self.x); 3 }
            0x8E => { let (a, _) = self.resolve_addr(Mode::Abs, bus); bus.write(a, self.x); 4 }
            0x96 => { let (a, _) = self.resolve_addr(Mode::ZpY, bus); bus.write(a, self.x); 4 }
            // STY
            0x84 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); bus.write(a, self.y); 3 }
            0x8C => { let (a, _) = self.resolve_addr(Mode::Abs, bus); bus.write(a, self.y); 4 }
            0x94 => { let (a, _) = self.resolve_addr(Mode::ZpX, bus); bus.write(a, self.y); 4 }
            // Transfers
            0x88 => { self.y = self.y.wrapping_sub(1); let v = self.y; self.set_nz(v); 2 } // DEY
            0x8A => { self.a = self.x; let v = self.a; self.set_nz(v); 2 } // TXA
            0x98 => { self.a = self.y; let v = self.a; self.set_nz(v); 2 } // TYA
            0x9A => { self.sp = self.x; 2 } // TXS
            0xA8 => { self.y = self.a; let v = self.y; self.set_nz(v); 2 } // TAY
            0xAA => { self.x = self.a; let v = self.x; self.set_nz(v); 2 } // TAX
            0xBA => { self.x = self.sp; let v = self.x; self.set_nz(v); 2 } // TSX
            // BCC / BCS
            0x90 => { let (a, p) = self.resolve_addr(Mode::Rel, bus); self.branch(!self.get_flag(FLAG_C), a, p) }
            0xB0 => { let (a, p) = self.resolve_addr(Mode::Rel, bus); self.branch(self.get_flag(FLAG_C), a, p) }
            // CLV
            0xB8 => { self.set_flag(FLAG_V, false); 2 }
            // LDA
            0xA1 => { let (a, _) = self.resolve_addr(Mode::IdX, bus); self.a = bus.read(a); let v = self.a; self.set_nz(v); 6 }
            0xA5 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); self.a = bus.read(a); let v = self.a; self.set_nz(v); 3 }
            0xA9 => { let (a, _) = self.resolve_addr(Mode::Imm, bus); self.a = bus.read(a); let v = self.a; self.set_nz(v); 2 }
            0xAD => { let (a, _) = self.resolve_addr(Mode::Abs, bus); self.a = bus.read(a); let v = self.a; self.set_nz(v); 4 }
            0xB1 => { let (a, p) = self.resolve_addr(Mode::IdY, bus); self.a = bus.read(a); let v = self.a; self.set_nz(v); 5 + p as u32 }
            0xB5 => { let (a, _) = self.resolve_addr(Mode::ZpX, bus); self.a = bus.read(a); let v = self.a; self.set_nz(v); 4 }
            0xB9 => { let (a, p) = self.resolve_addr(Mode::AbY, bus); self.a = bus.read(a); let v = self.a; self.set_nz(v); 4 + p as u32 }
            0xBD => { let (a, p) = self.resolve_addr(Mode::AbX, bus); self.a = bus.read(a); let v = self.a; self.set_nz(v); 4 + p as u32 }
            // LDX
            0xA2 => { let (a, _) = self.resolve_addr(Mode::Imm, bus); self.x = bus.read(a); let v = self.x; self.set_nz(v); 2 }
            0xA6 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); self.x = bus.read(a); let v = self.x; self.set_nz(v); 3 }
            0xAE => { let (a, _) = self.resolve_addr(Mode::Abs, bus); self.x = bus.read(a); let v = self.x; self.set_nz(v); 4 }
            0xB6 => { let (a, _) = self.resolve_addr(Mode::ZpY, bus); self.x = bus.read(a); let v = self.x; self.set_nz(v); 4 }
            0xBE => { let (a, p) = self.resolve_addr(Mode::AbY, bus); self.x = bus.read(a); let v = self.x; self.set_nz(v); 4 + p as u32 }
            // LDY
            0xA0 => { let (a, _) = self.resolve_addr(Mode::Imm, bus); self.y = bus.read(a); let v = self.y; self.set_nz(v); 2 }
            0xA4 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); self.y = bus.read(a); let v = self.y; self.set_nz(v); 3 }
            0xAC => { let (a, _) = self.resolve_addr(Mode::Abs, bus); self.y = bus.read(a); let v = self.y; self.set_nz(v); 4 }
            0xB4 => { let (a, _) = self.resolve_addr(Mode::ZpX, bus); self.y = bus.read(a); let v = self.y; self.set_nz(v); 4 }
            0xBC => { let (a, p) = self.resolve_addr(Mode::AbX, bus); self.y = bus.read(a); let v = self.y; self.set_nz(v); 4 + p as u32 }
            // CMP
            0xC1 => { let (a, _) = self.resolve_addr(Mode::IdX, bus); let v = bus.read(a); self.compare(self.a, v); 6 }
            0xC5 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); let v = bus.read(a); self.compare(self.a, v); 3 }
            0xC9 => { let (a, _) = self.resolve_addr(Mode::Imm, bus); let v = bus.read(a); self.compare(self.a, v); 2 }
            0xCD => { let (a, _) = self.resolve_addr(Mode::Abs, bus); let v = bus.read(a); self.compare(self.a, v); 4 }
            0xD1 => { let (a, p) = self.resolve_addr(Mode::IdY, bus); let v = bus.read(a); self.compare(self.a, v); 5 + p as u32 }
            0xD5 => { let (a, _) = self.resolve_addr(Mode::ZpX, bus); let v = bus.read(a); self.compare(self.a, v); 4 }
            0xD9 => { let (a, p) = self.resolve_addr(Mode::AbY, bus); let v = bus.read(a); self.compare(self.a, v); 4 + p as u32 }
            0xDD => { let (a, p) = self.resolve_addr(Mode::AbX, bus); let v = bus.read(a); self.compare(self.a, v); 4 + p as u32 }
            // CPX
            0xE0 => { let (a, _) = self.resolve_addr(Mode::Imm, bus); let v = bus.read(a); self.compare(self.x, v); 2 }
            0xE4 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); let v = bus.read(a); self.compare(self.x, v); 3 }
            0xEC => { let (a, _) = self.resolve_addr(Mode::Abs, bus); let v = bus.read(a); self.compare(self.x, v); 4 }
            // CPY
            0xC0 => { let (a, _) = self.resolve_addr(Mode::Imm, bus); let v = bus.read(a); self.compare(self.y, v); 2 }
            0xC4 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); let v = bus.read(a); self.compare(self.y, v); 3 }
            0xCC => { let (a, _) = self.resolve_addr(Mode::Abs, bus); let v = bus.read(a); self.compare(self.y, v); 4 }
            // DEC
            0xC6 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); let v = bus.read(a).wrapping_sub(1); bus.write(a, v); self.set_nz(v); 5 }
            0xCE => { let (a, _) = self.resolve_addr(Mode::Abs, bus); let v = bus.read(a).wrapping_sub(1); bus.write(a, v); self.set_nz(v); 6 }
            0xD6 => { let (a, _) = self.resolve_addr(Mode::ZpX, bus); let v = bus.read(a).wrapping_sub(1); bus.write(a, v); self.set_nz(v); 6 }
            0xDE => { let (a, _) = self.resolve_addr(Mode::AbX, bus); let v = bus.read(a).wrapping_sub(1); bus.write(a, v); self.set_nz(v); 7 }
            // DEX
            0xCA => { self.x = self.x.wrapping_sub(1); let v = self.x; self.set_nz(v); 2 }
            // INC
            0xE6 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); let v = bus.read(a).wrapping_add(1); bus.write(a, v); self.set_nz(v); 5 }
            0xEE => { let (a, _) = self.resolve_addr(Mode::Abs, bus); let v = bus.read(a).wrapping_add(1); bus.write(a, v); self.set_nz(v); 6 }
            0xF6 => { let (a, _) = self.resolve_addr(Mode::ZpX, bus); let v = bus.read(a).wrapping_add(1); bus.write(a, v); self.set_nz(v); 6 }
            0xFE => { let (a, _) = self.resolve_addr(Mode::AbX, bus); let v = bus.read(a).wrapping_add(1); bus.write(a, v); self.set_nz(v); 7 }
            // INX / INY
            0xE8 => { self.x = self.x.wrapping_add(1); let v = self.x; self.set_nz(v); 2 }
            0xC8 => { self.y = self.y.wrapping_add(1); let v = self.y; self.set_nz(v); 2 }
            // SBC
            0xE1 => { let (a, _) = self.resolve_addr(Mode::IdX, bus); let v = bus.read(a); self.sbc(v); 6 }
            0xE5 => { let (a, _) = self.resolve_addr(Mode::Zp0, bus); let v = bus.read(a); self.sbc(v); 3 }
            0xE9 => { let (a, _) = self.resolve_addr(Mode::Imm, bus); let v = bus.read(a); self.sbc(v); 2 }
            0xED => { let (a, _) = self.resolve_addr(Mode::Abs, bus); let v = bus.read(a); self.sbc(v); 4 }
            0xF1 => { let (a, p) = self.resolve_addr(Mode::IdY, bus); let v = bus.read(a); self.sbc(v); 5 + p as u32 }
            0xF5 => { let (a, _) = self.resolve_addr(Mode::ZpX, bus); let v = bus.read(a); self.sbc(v); 4 }
            0xF9 => { let (a, p) = self.resolve_addr(Mode::AbY, bus); let v = bus.read(a); self.sbc(v); 4 + p as u32 }
            0xFD => { let (a, p) = self.resolve_addr(Mode::AbX, bus); let v = bus.read(a); self.sbc(v); 4 + p as u32 }
            // BEQ / BNE
            0xF0 => { let (a, p) = self.resolve_addr(Mode::Rel, bus); self.branch(self.get_flag(FLAG_Z), a, p) }
            0xD0 => { let (a, p) = self.resolve_addr(Mode::Rel, bus); self.branch(!self.get_flag(FLAG_Z), a, p) }
            // CLD / SED
            0xD8 => { self.set_flag(FLAG_D, false); 2 }
            0xF8 => { self.set_flag(FLAG_D, true); 2 }
            // NOP
            0xEA => 2,
            // Unknown / NOP variants
            _ => 2,
        }
    }

    fn adc(&mut self, val: u8) {
        let a = self.a as u16;
        let v = val as u16;
        let c = if self.get_flag(FLAG_C) { 1u16 } else { 0 };
        let result = a + v + c;
        self.set_flag(FLAG_C, result > 0xFF);
        let r = result as u8;
        self.set_flag(FLAG_V, (!(a ^ v) & (a ^ result)) & 0x80 != 0);
        self.a = r;
        self.set_nz(r);
    }

    fn sbc(&mut self, val: u8) {
        self.adc(val ^ 0xFF);
    }

    fn compare(&mut self, reg: u8, val: u8) {
        let result = reg.wrapping_sub(val);
        self.set_flag(FLAG_C, reg >= val);
        self.set_flag(FLAG_Z, reg == val);
        self.set_flag(FLAG_N, result & 0x80 != 0);
    }

    fn branch(&mut self, condition: bool, addr: u16, page_crossed: bool) -> u32 {
        if condition {
            self.pc = addr;
            2 + 1 + page_crossed as u32
        } else {
            2
        }
    }

    fn bit_test(&mut self, addr: u16, bus: &mut NesBus) {
        let val = bus.read(addr);
        self.set_flag(FLAG_Z, self.a & val == 0);
        self.set_flag(FLAG_V, val & 0x40 != 0);
        self.set_flag(FLAG_N, val & 0x80 != 0);
    }

    fn asl_acc(&mut self) {
        self.set_flag(FLAG_C, self.a & 0x80 != 0);
        self.a <<= 1;
        let v = self.a;
        self.set_nz(v);
    }

    fn asl_mem(&mut self, addr: u16, bus: &mut NesBus) -> u8 {
        let val = bus.read(addr);
        self.set_flag(FLAG_C, val & 0x80 != 0);
        let result = val << 1;
        bus.write(addr, result);
        result
    }

    fn lsr_acc(&mut self) {
        self.set_flag(FLAG_C, self.a & 0x01 != 0);
        self.a >>= 1;
        let v = self.a;
        self.set_nz(v);
    }

    fn lsr_mem(&mut self, addr: u16, bus: &mut NesBus) -> u8 {
        let val = bus.read(addr);
        self.set_flag(FLAG_C, val & 0x01 != 0);
        let result = val >> 1;
        bus.write(addr, result);
        result
    }

    fn rol_acc(&mut self) {
        let old_c = self.get_flag(FLAG_C) as u8;
        self.set_flag(FLAG_C, self.a & 0x80 != 0);
        self.a = (self.a << 1) | old_c;
        let v = self.a;
        self.set_nz(v);
    }

    fn rol_mem(&mut self, addr: u16, bus: &mut NesBus) -> u8 {
        let val = bus.read(addr);
        let old_c = self.get_flag(FLAG_C) as u8;
        self.set_flag(FLAG_C, val & 0x80 != 0);
        let result = (val << 1) | old_c;
        bus.write(addr, result);
        result
    }

    fn ror_acc(&mut self) {
        let old_c = self.get_flag(FLAG_C) as u8;
        self.set_flag(FLAG_C, self.a & 0x01 != 0);
        self.a = (self.a >> 1) | (old_c << 7);
        let v = self.a;
        self.set_nz(v);
    }

    fn ror_mem(&mut self, addr: u16, bus: &mut NesBus) -> u8 {
        let val = bus.read(addr);
        let old_c = self.get_flag(FLAG_C) as u8;
        self.set_flag(FLAG_C, val & 0x01 != 0);
        let result = (val >> 1) | (old_c << 7);
        bus.write(addr, result);
        result
    }
}
