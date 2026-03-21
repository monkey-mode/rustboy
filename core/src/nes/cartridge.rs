/// NES cartridge / iNES format parser and mapper support.

use crate::save_state::*;

// ---------------------------------------------------------------------------
// Mirroring
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mirroring {
    Horizontal,
    Vertical,
    SingleScreenLow,
    SingleScreenHigh,
    FourScreen,
}

// ---------------------------------------------------------------------------
// Mapper trait
// ---------------------------------------------------------------------------
pub trait Mapper {
    fn read_prg(&self, addr: u16) -> u8;
    fn write_prg(&mut self, addr: u16, val: u8);
    fn read_chr(&self, addr: u16) -> u8;
    fn write_chr(&mut self, addr: u16, val: u8);
    fn mirroring(&self) -> Mirroring;
    /// Called once per PPU scanline (for MMC3 IRQ counter).
    fn notify_scanline(&mut self) {}
    /// True if the mapper has a pending IRQ.
    fn irq_pending(&mut self) -> bool { false }
    /// Save mapper banking/RAM state (NOT ROM data).
    fn save_mapper(&self, buf: &mut Vec<u8>);
    /// Restore mapper banking/RAM state.
    fn load_mapper(&mut self, data: &[u8], off: &mut usize);
}

// ---------------------------------------------------------------------------
// iNES header parser
// ---------------------------------------------------------------------------
pub fn parse_ines(rom: &[u8]) -> Box<dyn Mapper> {
    assert!(rom.len() >= 16, "ROM too small to contain iNES header");
    assert_eq!(&rom[0..4], &[0x4E, 0x45, 0x53, 0x1A], "Not a valid iNES ROM");

    let prg_banks = rom[4] as usize; // 16KB units
    let chr_banks = rom[5] as usize; // 8KB units
    let flags6 = rom[6];
    let flags7 = rom[7];

    let mapper_num = (flags7 & 0xF0) | (flags6 >> 4);
    let has_trainer = flags6 & 0x04 != 0;
    let four_screen = flags6 & 0x08 != 0;
    let vertical_mirror = flags6 & 0x01 != 0;

    let mirroring = if four_screen {
        Mirroring::FourScreen
    } else if vertical_mirror {
        Mirroring::Vertical
    } else {
        Mirroring::Horizontal
    };

    let trainer_size = if has_trainer { 512 } else { 0 };
    let prg_start = 16 + trainer_size;
    let prg_size = prg_banks * 16384;
    let chr_start = prg_start + prg_size;
    let chr_size = chr_banks * 8192;

    let prg_rom = rom[prg_start..prg_start + prg_size].to_vec();
    let chr_data = if chr_banks == 0 {
        vec![0u8; 8192] // CHR RAM
    } else {
        rom[chr_start..chr_start + chr_size].to_vec()
    };
    let chr_is_ram = chr_banks == 0;

    match mapper_num {
        0 => Box::new(Mapper0::new(prg_rom, chr_data, chr_is_ram, mirroring)),
        1 => Box::new(Mapper1::new(prg_rom, chr_data, chr_is_ram, mirroring)),
        2 => Box::new(Mapper2::new(prg_rom, chr_data, chr_is_ram, mirroring)),
        3 => Box::new(Mapper3::new(prg_rom, chr_data, chr_is_ram, mirroring)),
        4 => Box::new(Mapper4::new(prg_rom, chr_data, chr_is_ram, mirroring)),
        _ => {
            // Fallback to mapper 0 for unknown mappers
            Box::new(Mapper0::new(prg_rom, chr_data, chr_is_ram, mirroring))
        }
    }
}

// ---------------------------------------------------------------------------
// Mapper 0 – NROM
// ---------------------------------------------------------------------------
pub struct Mapper0 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,
}

impl Mapper0 {
    pub fn new(prg_rom: Vec<u8>, chr: Vec<u8>, chr_is_ram: bool, mirroring: Mirroring) -> Self {
        Mapper0 { prg_rom, chr, chr_is_ram, mirroring }
    }
}

impl Mapper for Mapper0 {
    fn read_prg(&self, addr: u16) -> u8 {
        if addr < 0x8000 {
            return 0;
        }
        let offset = (addr - 0x8000) as usize;
        // Mirror if 16KB PRG
        let idx = offset % self.prg_rom.len();
        self.prg_rom[idx]
    }

    fn write_prg(&mut self, _addr: u16, _val: u8) {
        // NROM has no PRG banking
    }

    fn read_chr(&self, addr: u16) -> u8 {
        self.chr[addr as usize % self.chr.len()]
    }

    fn write_chr(&mut self, addr: u16, val: u8) {
        if self.chr_is_ram {
            let idx = addr as usize % self.chr.len();
            self.chr[idx] = val;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn save_mapper(&self, buf: &mut Vec<u8>) {
        // Mapper type tag
        write_u8(buf, 0);
        // CHR RAM (if present)
        write_bool(buf, self.chr_is_ram);
        if self.chr_is_ram {
            write_bytes(buf, &self.chr);
        }
    }

    fn load_mapper(&mut self, data: &[u8], off: &mut usize) {
        let _tag = read_u8(data, off);
        let chr_is_ram = read_bool(data, off);
        if chr_is_ram {
            self.chr = read_bytes(data, off);
        }
    }
}

// ---------------------------------------------------------------------------
// Mapper 1 – MMC1
// ---------------------------------------------------------------------------
pub struct Mapper1 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    base_mirroring: Mirroring,

    shift: u8,
    shift_count: u8,
    control: u8,
    chr_bank0: u8,
    chr_bank1: u8,
    prg_bank: u8,
}

impl Mapper1 {
    pub fn new(prg_rom: Vec<u8>, chr: Vec<u8>, chr_is_ram: bool, mirroring: Mirroring) -> Self {
        Mapper1 {
            prg_rom,
            chr,
            chr_is_ram,
            base_mirroring: mirroring,
            shift: 0,
            shift_count: 0,
            control: 0x0C, // PRG mode 3 (fix last bank), CHR mode 0
            chr_bank0: 0,
            chr_bank1: 0,
            prg_bank: 0,
        }
    }

    fn prg_bank_count(&self) -> usize {
        self.prg_rom.len() / 16384
    }

    fn chr_bank_count(&self) -> usize {
        let s = self.chr.len() / 4096;
        if s == 0 { 2 } else { s }
    }
}

impl Mapper for Mapper1 {
    fn read_prg(&self, addr: u16) -> u8 {
        if addr < 0x8000 {
            return 0;
        }
        let num_banks = self.prg_bank_count();
        let prg_mode = (self.control >> 2) & 0x03;
        let bank = self.prg_bank as usize & 0x0F;

        let offset = match prg_mode {
            0 | 1 => {
                // Switch 32KB at $8000
                let b = bank & !1;
                if addr < 0xC000 {
                    b * 16384 + (addr - 0x8000) as usize
                } else {
                    (b + 1) * 16384 + (addr - 0xC000) as usize
                }
            }
            2 => {
                // Fix first bank at $8000, switch $C000
                if addr < 0xC000 {
                    (addr - 0x8000) as usize
                } else {
                    bank * 16384 + (addr - 0xC000) as usize
                }
            }
            3 => {
                // Switch $8000, fix last bank at $C000
                if addr < 0xC000 {
                    bank * 16384 + (addr - 0x8000) as usize
                } else {
                    (num_banks - 1) * 16384 + (addr - 0xC000) as usize
                }
            }
            _ => unreachable!(),
        };
        self.prg_rom.get(offset).copied().unwrap_or(0)
    }

    fn write_prg(&mut self, addr: u16, val: u8) {
        if addr < 0x8000 {
            return;
        }
        if val & 0x80 != 0 {
            // Reset shift register
            self.shift = 0;
            self.shift_count = 0;
            self.control |= 0x0C;
            return;
        }
        self.shift = (self.shift >> 1) | ((val & 1) << 4);
        self.shift_count += 1;
        if self.shift_count == 5 {
            let data = self.shift;
            self.shift = 0;
            self.shift_count = 0;
            match addr {
                0x8000..=0x9FFF => self.control = data,
                0xA000..=0xBFFF => self.chr_bank0 = data,
                0xC000..=0xDFFF => self.chr_bank1 = data,
                0xE000..=0xFFFF => self.prg_bank = data,
                _ => {}
            }
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let chr_mode = (self.control >> 4) & 0x01;
        let num_banks = self.chr_bank_count();
        let offset = if chr_mode == 0 {
            // 8KB mode
            let bank = (self.chr_bank0 as usize & !1) % (num_banks / 2);
            bank * 8192 + addr as usize
        } else {
            // 4KB mode
            if addr < 0x1000 {
                let bank = self.chr_bank0 as usize % num_banks;
                bank * 4096 + addr as usize
            } else {
                let bank = self.chr_bank1 as usize % num_banks;
                bank * 4096 + (addr - 0x1000) as usize
            }
        };
        self.chr.get(offset).copied().unwrap_or(0)
    }

    fn write_chr(&mut self, addr: u16, val: u8) {
        if self.chr_is_ram {
            let idx = addr as usize % self.chr.len();
            self.chr[idx] = val;
        }
    }

    fn mirroring(&self) -> Mirroring {
        match self.control & 0x03 {
            0 => Mirroring::SingleScreenLow,
            1 => Mirroring::SingleScreenHigh,
            2 => Mirroring::Vertical,
            3 => Mirroring::Horizontal,
            _ => self.base_mirroring,
        }
    }

    fn save_mapper(&self, buf: &mut Vec<u8>) {
        write_u8(buf, 1); // mapper tag
        write_u8(buf, self.shift);
        write_u8(buf, self.shift_count);
        write_u8(buf, self.control);
        write_u8(buf, self.chr_bank0);
        write_u8(buf, self.chr_bank1);
        write_u8(buf, self.prg_bank);
        write_bool(buf, self.chr_is_ram);
        if self.chr_is_ram {
            write_bytes(buf, &self.chr);
        }
    }

    fn load_mapper(&mut self, data: &[u8], off: &mut usize) {
        let _tag = read_u8(data, off);
        self.shift       = read_u8(data, off);
        self.shift_count = read_u8(data, off);
        self.control     = read_u8(data, off);
        self.chr_bank0   = read_u8(data, off);
        self.chr_bank1   = read_u8(data, off);
        self.prg_bank    = read_u8(data, off);
        let chr_is_ram = read_bool(data, off);
        if chr_is_ram {
            self.chr = read_bytes(data, off);
        }
    }
}

// ---------------------------------------------------------------------------
// Mapper 2 – UxROM
// ---------------------------------------------------------------------------
pub struct Mapper2 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,
    prg_bank: usize,
}

impl Mapper2 {
    pub fn new(prg_rom: Vec<u8>, chr: Vec<u8>, chr_is_ram: bool, mirroring: Mirroring) -> Self {
        Mapper2 { prg_rom, chr, chr_is_ram, mirroring, prg_bank: 0 }
    }
}

impl Mapper for Mapper2 {
    fn read_prg(&self, addr: u16) -> u8 {
        let num_banks = self.prg_rom.len() / 16384;
        if addr < 0x8000 {
            return 0;
        }
        let offset = if addr < 0xC000 {
            self.prg_bank * 16384 + (addr - 0x8000) as usize
        } else {
            (num_banks - 1) * 16384 + (addr - 0xC000) as usize
        };
        self.prg_rom.get(offset).copied().unwrap_or(0)
    }

    fn write_prg(&mut self, addr: u16, val: u8) {
        if addr >= 0x8000 {
            let num_banks = self.prg_rom.len() / 16384;
            self.prg_bank = (val as usize) % num_banks;
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        self.chr.get(addr as usize).copied().unwrap_or(0)
    }

    fn write_chr(&mut self, addr: u16, val: u8) {
        if self.chr_is_ram {
            if (addr as usize) < self.chr.len() {
                self.chr[addr as usize] = val;
            }
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn save_mapper(&self, buf: &mut Vec<u8>) {
        write_u8(buf, 2); // mapper tag
        write_u32(buf, self.prg_bank as u32);
        write_bool(buf, self.chr_is_ram);
        if self.chr_is_ram {
            write_bytes(buf, &self.chr);
        }
    }

    fn load_mapper(&mut self, data: &[u8], off: &mut usize) {
        let _tag = read_u8(data, off);
        self.prg_bank = read_u32(data, off) as usize;
        let chr_is_ram = read_bool(data, off);
        if chr_is_ram {
            self.chr = read_bytes(data, off);
        }
    }
}

// ---------------------------------------------------------------------------
// Mapper 3 – CNROM
// ---------------------------------------------------------------------------
pub struct Mapper3 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,
    chr_bank: usize,
}

impl Mapper3 {
    pub fn new(prg_rom: Vec<u8>, chr: Vec<u8>, chr_is_ram: bool, mirroring: Mirroring) -> Self {
        Mapper3 { prg_rom, chr, chr_is_ram, mirroring, chr_bank: 0 }
    }
}

impl Mapper for Mapper3 {
    fn read_prg(&self, addr: u16) -> u8 {
        if addr < 0x8000 {
            return 0;
        }
        let offset = (addr - 0x8000) as usize % self.prg_rom.len();
        self.prg_rom[offset]
    }

    fn write_prg(&mut self, addr: u16, val: u8) {
        if addr >= 0x8000 {
            let num_banks = self.chr.len() / 8192;
            let banks = if num_banks == 0 { 1 } else { num_banks };
            self.chr_bank = (val as usize) % banks;
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let offset = self.chr_bank * 8192 + addr as usize;
        self.chr.get(offset).copied().unwrap_or(0)
    }

    fn write_chr(&mut self, addr: u16, val: u8) {
        if self.chr_is_ram {
            let offset = self.chr_bank * 8192 + addr as usize;
            if offset < self.chr.len() {
                self.chr[offset] = val;
            }
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn save_mapper(&self, buf: &mut Vec<u8>) {
        write_u8(buf, 3); // mapper tag
        write_u32(buf, self.chr_bank as u32);
        write_bool(buf, self.chr_is_ram);
        if self.chr_is_ram {
            write_bytes(buf, &self.chr);
        }
    }

    fn load_mapper(&mut self, data: &[u8], off: &mut usize) {
        let _tag = read_u8(data, off);
        self.chr_bank = read_u32(data, off) as usize;
        let chr_is_ram = read_bool(data, off);
        if chr_is_ram {
            self.chr = read_bytes(data, off);
        }
    }
}

// ---------------------------------------------------------------------------
// Mapper 4 – MMC3
// ---------------------------------------------------------------------------
pub struct Mapper4 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: Vec<u8>,     // $6000-$7FFF, 8KB
    mirroring: Mirroring,

    // Bank select register ($8000)
    bank_select: u8,      // bits 0-2 = reg index, bit 6 = PRG mode, bit 7 = CHR mode

    // Bank registers R0-R7
    regs: [u8; 8],

    // PRG RAM protect ($A001)
    prg_ram_enable: bool,
    prg_ram_protect: bool,

    // IRQ
    irq_latch: u8,
    irq_counter: u8,
    irq_enable: bool,
    irq_reload: bool,
    irq_pending: bool,
}

impl Mapper4 {
    pub fn new(prg_rom: Vec<u8>, chr: Vec<u8>, chr_is_ram: bool, mirroring: Mirroring) -> Self {
        Mapper4 {
            prg_rom,
            chr,
            chr_is_ram,
            prg_ram: vec![0u8; 8192],
            mirroring,
            bank_select: 0,
            regs: [0u8; 8],
            prg_ram_enable: true,
            prg_ram_protect: false,
            irq_latch: 0,
            irq_counter: 0,
            irq_enable: false,
            irq_reload: false,
            irq_pending: false,
        }
    }

    fn prg_bank_count(&self) -> usize {
        self.prg_rom.len() / 8192 // 8KB units
    }

    fn chr_bank_count(&self) -> usize {
        let n = self.chr.len() / 1024; // 1KB units
        if n == 0 { 8 } else { n }
    }

    fn prg_addr(&self, addr: u16) -> usize {
        let n = self.prg_bank_count();
        let prg_mode = (self.bank_select >> 6) & 1;
        let r6 = (self.regs[6] as usize) % n;
        let r7 = (self.regs[7] as usize) % n;
        match addr {
            0x6000..=0x7FFF => return usize::MAX, // PRG RAM — handled by caller
            0x8000..=0x9FFF => {
                let bank = if prg_mode == 0 { r6 } else { n - 2 };
                bank * 8192 + (addr - 0x8000) as usize
            }
            0xA000..=0xBFFF => r7 * 8192 + (addr - 0xA000) as usize,
            0xC000..=0xDFFF => {
                let bank = if prg_mode == 0 { n - 2 } else { r6 };
                bank * 8192 + (addr - 0xC000) as usize
            }
            0xE000..=0xFFFF => (n - 1) * 8192 + (addr - 0xE000) as usize,
            _ => 0,
        }
    }

    fn chr_addr(&self, addr: u16) -> usize {
        let n = self.chr_bank_count(); // 1KB units
        let chr_mode = (self.bank_select >> 7) & 1;
        let addr = addr as usize;

        // Each register selects 1KB CHR banks; R0/R1 are 2KB (bit 0 ignored)
        let (bank_1k, offset) = if chr_mode == 0 {
            // $0000-$0FFF = 2KB banks (R0, R1); $1000-$1FFF = 1KB banks (R2-R5)
            match addr {
                0x0000..=0x07FF => ((self.regs[0] & 0xFE) as usize, addr),
                0x0800..=0x0FFF => ((self.regs[1] & 0xFE) as usize, addr - 0x0800),
                0x1000..=0x13FF => (self.regs[2] as usize,          addr - 0x1000),
                0x1400..=0x17FF => (self.regs[3] as usize,          addr - 0x1400),
                0x1800..=0x1BFF => (self.regs[4] as usize,          addr - 0x1800),
                0x1C00..=0x1FFF => (self.regs[5] as usize,          addr - 0x1C00),
                _ => (0, addr),
            }
        } else {
            // $0000-$0FFF = 1KB banks (R2-R5); $1000-$1FFF = 2KB banks (R0, R1)
            match addr {
                0x0000..=0x03FF => (self.regs[2] as usize,          addr),
                0x0400..=0x07FF => (self.regs[3] as usize,          addr - 0x0400),
                0x0800..=0x0BFF => (self.regs[4] as usize,          addr - 0x0800),
                0x0C00..=0x0FFF => (self.regs[5] as usize,          addr - 0x0C00),
                0x1000..=0x17FF => ((self.regs[0] & 0xFE) as usize, addr - 0x1000),
                0x1800..=0x1FFF => ((self.regs[1] & 0xFE) as usize, addr - 0x1800),
                _ => (0, addr),
            }
        };
        (bank_1k % n) * 1024 + offset
    }
}

impl Mapper for Mapper4 {
    fn read_prg(&self, addr: u16) -> u8 {
        if addr >= 0x6000 && addr <= 0x7FFF {
            if self.prg_ram_enable {
                return self.prg_ram[(addr - 0x6000) as usize];
            }
            return 0;
        }
        let off = self.prg_addr(addr);
        self.prg_rom.get(off).copied().unwrap_or(0)
    }

    fn write_prg(&mut self, addr: u16, val: u8) {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_enable && !self.prg_ram_protect {
                    self.prg_ram[(addr - 0x6000) as usize] = val;
                }
            }
            // Even = bank select, odd = bank data
            0x8000..=0x9FFF => {
                if addr & 1 == 0 {
                    self.bank_select = val;
                } else {
                    let reg = (self.bank_select & 0x07) as usize;
                    self.regs[reg] = val;
                }
            }
            // Even = mirroring, odd = PRG RAM protect
            0xA000..=0xBFFF => {
                if addr & 1 == 0 {
                    self.mirroring = if val & 1 == 0 { Mirroring::Vertical } else { Mirroring::Horizontal };
                } else {
                    self.prg_ram_enable  = val & 0x80 != 0;
                    self.prg_ram_protect = val & 0x40 != 0;
                }
            }
            // Even = IRQ latch, odd = IRQ reload
            0xC000..=0xDFFF => {
                if addr & 1 == 0 {
                    self.irq_latch = val;
                } else {
                    self.irq_counter = 0;
                    self.irq_reload = true;
                }
            }
            // Even = IRQ disable, odd = IRQ enable
            0xE000..=0xFFFF => {
                if addr & 1 == 0 {
                    self.irq_enable = false;
                    self.irq_pending = false;
                } else {
                    self.irq_enable = true;
                }
            }
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let off = self.chr_addr(addr);
        self.chr.get(off).copied().unwrap_or(0)
    }

    fn write_chr(&mut self, addr: u16, val: u8) {
        if self.chr_is_ram {
            let off = self.chr_addr(addr);
            if off < self.chr.len() {
                self.chr[off] = val;
            }
        }
    }

    fn mirroring(&self) -> Mirroring { self.mirroring }

    fn notify_scanline(&mut self) {
        if self.irq_counter == 0 || self.irq_reload {
            self.irq_counter = self.irq_latch;
            self.irq_reload = false;
        } else {
            self.irq_counter -= 1;
        }
        if self.irq_counter == 0 && self.irq_enable {
            self.irq_pending = true;
        }
    }

    fn irq_pending(&mut self) -> bool {
        let p = self.irq_pending;
        self.irq_pending = false;
        p
    }

    fn save_mapper(&self, buf: &mut Vec<u8>) {
        write_u8(buf, 4); // mapper tag
        write_u8(buf, self.bank_select);
        write_slice(buf, &self.regs);
        write_bool(buf, self.prg_ram_enable);
        write_bool(buf, self.prg_ram_protect);
        write_u8(buf, self.irq_latch);
        write_u8(buf, self.irq_counter);
        write_bool(buf, self.irq_enable);
        write_bool(buf, self.irq_reload);
        write_bool(buf, self.irq_pending);
        write_bytes(buf, &self.prg_ram);
        write_bool(buf, self.chr_is_ram);
        if self.chr_is_ram {
            write_bytes(buf, &self.chr);
        }
    }

    fn load_mapper(&mut self, data: &[u8], off: &mut usize) {
        let _tag       = read_u8(data, off);
        self.bank_select      = read_u8(data, off);
        self.regs.copy_from_slice(read_slice(data, off, 8));
        self.prg_ram_enable   = read_bool(data, off);
        self.prg_ram_protect  = read_bool(data, off);
        self.irq_latch        = read_u8(data, off);
        self.irq_counter      = read_u8(data, off);
        self.irq_enable       = read_bool(data, off);
        self.irq_reload       = read_bool(data, off);
        self.irq_pending      = read_bool(data, off);
        self.prg_ram          = read_bytes(data, off);
        let chr_is_ram        = read_bool(data, off);
        if chr_is_ram {
            self.chr = read_bytes(data, off);
        }
    }
}
