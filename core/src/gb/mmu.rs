/// Memory Management Unit / Bus for the Game Boy DMG.

use crate::save_state::*;
///
/// Memory map:
///   0x0000–0x7FFF  ROM (cartridge, banked for MBC1)
///   0x8000–0x9FFF  VRAM  (PPU)
///   0xA000–0xBFFF  External RAM
///   0xC000–0xDFFF  Work RAM
///   0xE000–0xFDFF  Echo RAM (mirrors Work RAM)
///   0xFE00–0xFE9F  OAM (PPU)
///   0xFEA0–0xFEFF  Forbidden / unused
///   0xFF00–0xFF7F  I/O registers
///   0xFF80–0xFFFE  High RAM
///   0xFFFF         Interrupt Enable register

use super::ppu::Ppu;
use super::apu::Apu;
use super::timer::Timer;

// ---------------------------------------------------------------------------
// MBC types
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, PartialEq)]
enum MbcType {
    RomOnly,
    Mbc1,
}

// ---------------------------------------------------------------------------
// MMU
// ---------------------------------------------------------------------------
pub struct Mmu {
    rom: Vec<u8>,
    ext_ram: Vec<u8>,
    work_ram: [u8; 0x2000],
    high_ram: [u8; 0x7F],

    pub ppu: Ppu,
    pub apu: Apu,
    pub timer: Timer,

    // Interrupt registers
    pub interrupt_flag:   u8, // 0xFF0F
    pub interrupt_enable: u8, // 0xFFFF

    // Joypad (P1 / 0xFF00)
    //   bit 5 = select action buttons   (0 = select)
    //   bit 4 = select direction buttons (0 = select)
    //   bits 3-0 = button state (0 = pressed)
    joypad_select: u8,
    joypad_action: u8,    // A, B, Select, Start  (bit0-3)
    joypad_direction: u8, // Right, Left, Up, Down (bit0-3)

    // MBC state
    mbc_type: MbcType,
    rom_bank: u8,
    ram_bank: u8,
    ram_enabled: bool,
    mbc1_mode: bool, // false = ROM banking, true = RAM banking

    // DMA
    dma_active: bool,
    dma_source: u8,
    dma_index: u8,
    dma_cycles: u32,

    // Serial (stub)
    serial_data: u8,
    serial_control: u8,
}

impl Mmu {
    pub fn new(rom: Vec<u8>) -> Self {
        let mbc_type = match rom.get(0x0147).copied().unwrap_or(0) {
            0x00 => MbcType::RomOnly,
            0x01..=0x03 => MbcType::Mbc1,
            _ => MbcType::Mbc1, // fallback
        };

        let ram_size = match rom.get(0x0149).copied().unwrap_or(0) {
            0x01 => 2 * 1024,
            0x02 => 8 * 1024,
            0x03 => 32 * 1024,
            _ => 8 * 1024,
        };

        Mmu {
            rom,
            ext_ram: vec![0; ram_size],
            work_ram: [0; 0x2000],
            high_ram: [0; 0x7F],
            ppu: Ppu::new(),
            apu: Apu::new(),
            timer: Timer::new(),
            interrupt_flag: 0xE1,
            interrupt_enable: 0x00,
            joypad_select: 0xFF,
            joypad_action: 0x0F,
            joypad_direction: 0x0F,
            mbc_type,
            rom_bank: 1,
            ram_bank: 0,
            ram_enabled: false,
            mbc1_mode: false,
            dma_active: false,
            dma_source: 0,
            dma_index: 0,
            dma_cycles: 0,
            serial_data: 0,
            serial_control: 0,
        }
    }

    /// Set a joypad button state.
    /// button: Right=0, Left=1, Up=2, Down=3, A=4, B=5, Select=6, Start=7
    /// pressed: true = button down
    pub fn set_joypad(&mut self, button: u8, pressed: bool) {
        if button < 4 {
            // Direction
            if pressed {
                self.joypad_direction &= !(1 << button);
            } else {
                self.joypad_direction |= 1 << button;
            }
        } else {
            // Action
            let bit = button - 4;
            if pressed {
                self.joypad_action &= !(1 << bit);
            } else {
                self.joypad_action |= 1 << bit;
            }
        }
        // Request joypad interrupt
        if pressed {
            self.interrupt_flag |= 0x10;
        }
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            // ROM bank 0
            0x0000..=0x3FFF => self.rom.get(addr as usize).copied().unwrap_or(0xFF),

            // ROM bank N (MBC1)
            0x4000..=0x7FFF => {
                let bank = self.rom_bank as usize;
                let offset = (addr - 0x4000) as usize;
                let physical = bank * 0x4000 + offset;
                self.rom.get(physical).copied().unwrap_or(0xFF)
            }

            // VRAM
            0x8000..=0x9FFF => self.ppu.read_vram(addr),

            // External RAM
            0xA000..=0xBFFF => {
                if self.ram_enabled && !self.ext_ram.is_empty() {
                    let bank = if self.mbc1_mode { self.ram_bank as usize } else { 0 };
                    let offset = bank * 0x2000 + (addr - 0xA000) as usize;
                    self.ext_ram.get(offset).copied().unwrap_or(0xFF)
                } else {
                    0xFF
                }
            }

            // Work RAM
            0xC000..=0xDFFF => self.work_ram[(addr - 0xC000) as usize],

            // Echo RAM (mirrors Work RAM)
            0xE000..=0xFDFF => self.work_ram[(addr - 0xE000) as usize],

            // OAM
            0xFE00..=0xFE9F => self.ppu.read_oam(addr),

            // Unused / forbidden
            0xFEA0..=0xFEFF => 0xFF,

            // I/O registers
            0xFF00..=0xFF7F => self.read_io(addr),

            // High RAM
            0xFF80..=0xFFFE => self.high_ram[(addr - 0xFF80) as usize],

            // Interrupt enable
            0xFFFF => self.interrupt_enable,
        }
    }

    pub fn write_byte(&mut self, addr: u16, val: u8) {
        match addr {
            // MBC registers (ROM area)
            0x0000..=0x1FFF => {
                if self.mbc_type == MbcType::Mbc1 {
                    self.ram_enabled = (val & 0x0F) == 0x0A;
                }
            }
            0x2000..=0x3FFF => {
                if self.mbc_type == MbcType::Mbc1 {
                    let mut bank = val & 0x1F;
                    if bank == 0 { bank = 1; }
                    self.rom_bank = (self.rom_bank & 0x60) | bank;
                }
            }
            0x4000..=0x5FFF => {
                if self.mbc_type == MbcType::Mbc1 {
                    if self.mbc1_mode {
                        self.ram_bank = val & 0x03;
                    } else {
                        self.rom_bank = (self.rom_bank & 0x1F) | ((val & 0x03) << 5);
                    }
                }
            }
            0x6000..=0x7FFF => {
                if self.mbc_type == MbcType::Mbc1 {
                    self.mbc1_mode = val & 0x01 != 0;
                }
            }

            // VRAM
            0x8000..=0x9FFF => self.ppu.write_vram(addr, val),

            // External RAM
            0xA000..=0xBFFF => {
                if self.ram_enabled && !self.ext_ram.is_empty() {
                    let bank = if self.mbc1_mode { self.ram_bank as usize } else { 0 };
                    let offset = bank * 0x2000 + (addr - 0xA000) as usize;
                    if offset < self.ext_ram.len() {
                        self.ext_ram[offset] = val;
                    }
                }
            }

            // Work RAM
            0xC000..=0xDFFF => self.work_ram[(addr - 0xC000) as usize] = val,

            // Echo RAM
            0xE000..=0xFDFF => self.work_ram[(addr - 0xE000) as usize] = val,

            // OAM
            0xFE00..=0xFE9F => self.ppu.write_oam(addr, val),

            // Forbidden
            0xFEA0..=0xFEFF => {}

            // I/O registers
            0xFF00..=0xFF7F => self.write_io(addr, val),

            // High RAM
            0xFF80..=0xFFFE => self.high_ram[(addr - 0xFF80) as usize] = val,

            // Interrupt enable
            0xFFFF => self.interrupt_enable = val,
        }
    }

    pub fn write_word(&mut self, addr: u16, val: u16) {
        self.write_byte(addr, (val & 0xFF) as u8);
        self.write_byte(addr.wrapping_add(1), (val >> 8) as u8);
    }

    // -----------------------------------------------------------------------
    // I/O register read
    // -----------------------------------------------------------------------
    fn read_io(&self, addr: u16) -> u8 {
        match addr {
            // Joypad P1
            0xFF00 => {
                let sel = self.joypad_select & 0x30;
                if sel & 0x20 == 0 {
                    // Action buttons selected
                    0xC0 | sel | (self.joypad_action & 0x0F)
                } else if sel & 0x10 == 0 {
                    // Direction buttons selected
                    0xC0 | sel | (self.joypad_direction & 0x0F)
                } else {
                    0xFF
                }
            }

            // Serial
            0xFF01 => self.serial_data,
            0xFF02 => self.serial_control,

            // Timer
            0xFF03 => 0xFF, // unused
            0xFF04 => self.timer.read_div(),
            0xFF05 => self.timer.tima,
            0xFF06 => self.timer.tma,
            0xFF07 => self.timer.tac,

            // Interrupt flag
            0xFF0F => self.interrupt_flag | 0xE0,

            // APU
            0xFF10..=0xFF3F => self.apu.read_byte(addr),

            // PPU
            0xFF40 => self.ppu.lcdc,
            0xFF41 => self.ppu.stat | 0x80,
            0xFF42 => self.ppu.scy,
            0xFF43 => self.ppu.scx,
            0xFF44 => self.ppu.ly,
            0xFF45 => self.ppu.lyc,
            0xFF46 => self.dma_source,
            0xFF47 => self.ppu.bgp,
            0xFF48 => self.ppu.obp0,
            0xFF49 => self.ppu.obp1,
            0xFF4A => self.ppu.wy,
            0xFF4B => self.ppu.wx,

            _ => 0xFF,
        }
    }

    // -----------------------------------------------------------------------
    // I/O register write
    // -----------------------------------------------------------------------
    fn write_io(&mut self, addr: u16, val: u8) {
        match addr {
            // Joypad
            0xFF00 => { self.joypad_select = val & 0x30; }

            // Serial
            0xFF01 => { self.serial_data = val; }
            0xFF02 => { self.serial_control = val; }

            // Timer
            0xFF03 => {} // unused
            0xFF04 => { self.timer.write_div(); }
            0xFF05 => { self.timer.tima = val; }
            0xFF06 => { self.timer.tma = val; }
            0xFF07 => { self.timer.tac = val & 0x07; }

            // Interrupt flag
            0xFF0F => { self.interrupt_flag = val; }

            // APU
            0xFF10..=0xFF3F => { self.apu.write_byte(addr, val); }

            // PPU
            0xFF40 => { self.ppu.lcdc = val; }
            0xFF41 => { self.ppu.stat = (self.ppu.stat & 0x07) | (val & 0x78); }
            0xFF42 => { self.ppu.scy = val; }
            0xFF43 => { self.ppu.scx = val; }
            0xFF44 => {} // LY is read-only
            0xFF45 => { self.ppu.lyc = val; }
            0xFF46 => {
                // DMA transfer: copy 0xA0 bytes from (val << 8) to OAM
                self.dma_source = val;
                self.dma_active = true;
                self.dma_index = 0;
                self.dma_cycles = 0;
                // Perform DMA immediately (simplified)
                let src_base = (val as u16) << 8;
                for i in 0u8..0xA0 {
                    let byte = self.read_byte(src_base + i as u16);
                    self.ppu.dma_write_oam(i, byte);
                }
            }
            0xFF47 => { self.ppu.bgp = val; }
            0xFF48 => { self.ppu.obp0 = val; }
            0xFF49 => { self.ppu.obp1 = val; }
            0xFF4A => { self.ppu.wy = val; }
            0xFF4B => { self.ppu.wx = val; }

            _ => {}
        }
    }

    /// Advance timer and check for timer interrupt.
    pub fn step_timer(&mut self, cycles: u32) {
        if self.timer.step(cycles) {
            self.interrupt_flag |= 0x04; // Timer interrupt
        }
    }

    /// Advance PPU and check for VBlank / STAT interrupts.
    pub fn step_ppu(&mut self, cycles: u32) {
        self.ppu.step(cycles);
        if self.ppu.vblank_irq {
            self.interrupt_flag |= 0x01;
            self.ppu.vblank_irq = false;
        }
        if self.ppu.stat_irq {
            self.interrupt_flag |= 0x02;
            self.ppu.stat_irq = false;
        }
    }

    /// Advance APU.
    pub fn step_apu(&mut self, cycles: u32) {
        self.apu.step(cycles);
    }

    // -----------------------------------------------------------------------
    // Save / Load state (does NOT include ROM data)
    // -----------------------------------------------------------------------
    pub fn save(&self, buf: &mut Vec<u8>) {
        // RAM regions
        write_slice(buf, &self.work_ram);
        write_slice(buf, &self.high_ram);
        // Variable-size external RAM
        write_bytes(buf, &self.ext_ram);
        // Interrupt registers
        write_u8(buf, self.interrupt_flag);
        write_u8(buf, self.interrupt_enable);
        // Joypad
        write_u8(buf, self.joypad_select);
        write_u8(buf, self.joypad_action);
        write_u8(buf, self.joypad_direction);
        // MBC state (0 = RomOnly, 1 = Mbc1)
        write_u8(buf, match self.mbc_type { MbcType::RomOnly => 0, MbcType::Mbc1 => 1 });
        write_u8(buf, self.rom_bank);
        write_u8(buf, self.ram_bank);
        write_bool(buf, self.ram_enabled);
        write_bool(buf, self.mbc1_mode);
        // DMA state
        write_bool(buf, self.dma_active);
        write_u8(buf, self.dma_source);
        write_u8(buf, self.dma_index);
        write_u32(buf, self.dma_cycles);
        // Serial
        write_u8(buf, self.serial_data);
        write_u8(buf, self.serial_control);
        // Sub-components
        self.timer.save(buf);
        self.ppu.save(buf);
        self.apu.save(buf);
    }

    pub fn load(&mut self, data: &[u8], off: &mut usize) {
        self.work_ram.copy_from_slice(read_slice(data, off, 0x2000));
        self.high_ram.copy_from_slice(read_slice(data, off, 0x7F));
        self.ext_ram = read_bytes(data, off);
        self.interrupt_flag   = read_u8(data, off);
        self.interrupt_enable = read_u8(data, off);
        self.joypad_select    = read_u8(data, off);
        self.joypad_action    = read_u8(data, off);
        self.joypad_direction = read_u8(data, off);
        self.mbc_type = match read_u8(data, off) {
            1 => MbcType::Mbc1,
            _ => MbcType::RomOnly,
        };
        self.rom_bank    = read_u8(data, off);
        self.ram_bank    = read_u8(data, off);
        self.ram_enabled = read_bool(data, off);
        self.mbc1_mode   = read_bool(data, off);
        self.dma_active  = read_bool(data, off);
        self.dma_source  = read_u8(data, off);
        self.dma_index   = read_u8(data, off);
        self.dma_cycles  = read_u32(data, off);
        self.serial_data    = read_u8(data, off);
        self.serial_control = read_u8(data, off);
        self.timer.load(data, off);
        self.ppu.load(data, off);
        self.apu.load(data, off);
    }
}
