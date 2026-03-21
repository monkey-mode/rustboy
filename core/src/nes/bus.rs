/// NES memory bus.

use crate::save_state::*;
///
/// CPU address space:
///   0x0000-0x07FF: internal RAM (mirrored to 0x1FFF)
///   0x2000-0x3FFF: PPU registers (8 regs mirrored)
///   0x4000-0x4013, 0x4015: APU registers
///   0x4014: OAM DMA
///   0x4016: Joypad 1
///   0x4017: Joypad 2 / APU frame counter
///   0x4020-0xFFFF: cartridge (PRG ROM/RAM)

use super::ppu::NesPpu;
use super::apu::NesApu;
use super::cartridge::Mapper;

pub struct NesBus {
    pub ram: [u8; 2048],
    pub ppu: NesPpu,
    pub apu: NesApu,
    pub cartridge: Box<dyn Mapper>,

    // Joypad state
    joypad1_state: u8,     // current button states (bit per button)
    joypad1_shift: u8,     // shift register for serial reads
    joypad1_strobe: bool,  // strobe latch
    joypad2_state: u8,
    joypad2_shift: u8,

    // OAM DMA pending
    pub dma_pending: bool,
    pub dma_page: u8,
}

impl NesBus {
    pub fn new(cartridge: Box<dyn Mapper>) -> Self {
        NesBus {
            ram: [0; 2048],
            ppu: NesPpu::new(),
            apu: NesApu::new(),
            cartridge,
            joypad1_state: 0,
            joypad1_shift: 0,
            joypad1_strobe: false,
            joypad2_state: 0,
            joypad2_shift: 0,
            dma_pending: false,
            dma_page: 0,
        }
    }

    pub fn read(&mut self, addr: u16) -> u8 {
        match addr {
            // Internal RAM (mirrored)
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize],

            // PPU registers (mirrored every 8 bytes)
            0x2000..=0x3FFF => {
                let cart = &mut *self.cartridge;
                self.ppu.read_register(addr & 0x0007, cart)
            }

            // APU status
            0x4015 => self.apu.read_status(),

            // Joypad 1
            0x4016 => {
                if self.joypad1_strobe {
                    // While strobe high, return A button state continuously
                    self.joypad1_state & 0x01
                } else {
                    let val = self.joypad1_shift & 0x01;
                    self.joypad1_shift >>= 1;
                    self.joypad1_shift |= 0x80; // bus pull-up
                    val
                }
            }

            // Joypad 2
            0x4017 => {
                let val = self.joypad2_shift & 0x01;
                self.joypad2_shift >>= 1;
                self.joypad2_shift |= 0x80;
                val
            }

            // Cartridge
            0x4020..=0xFFFF => self.cartridge.read_prg(addr),

            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            // Internal RAM (mirrored)
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize] = val,

            // PPU registers
            0x2000..=0x3FFF => {
                let cart = &mut *self.cartridge;
                self.ppu.write_register(addr & 0x0007, val, cart);
            }

            // APU registers
            0x4000..=0x4013 => self.apu.write_register(addr, val),
            0x4015 => self.apu.write_register(addr, val),

            // OAM DMA
            0x4014 => {
                self.dma_pending = true;
                self.dma_page = val;
            }

            // Joypad strobe
            0x4016 => {
                let prev = self.joypad1_strobe;
                self.joypad1_strobe = val & 0x01 != 0;
                // Latch on falling edge ($01 → $00)
                if prev && !self.joypad1_strobe {
                    self.joypad1_shift = self.joypad1_state;
                    self.joypad2_shift = self.joypad2_state;
                }
            }

            // APU frame counter / joypad 2
            0x4017 => self.apu.write_register(addr, val),

            // Cartridge
            0x4020..=0xFFFF => self.cartridge.write_prg(addr, val),

            _ => {}
        }
    }

    /// Perform OAM DMA: copy 256 bytes from cpu_page*0x100 to PPU OAM.
    pub fn do_oam_dma(&mut self) {
        let page = self.dma_page;
        let mut buf = [0u8; 256];
        for i in 0..256usize {
            buf[i] = self.read((page as u16) << 8 | i as u16);
        }
        self.ppu.write_oam_dma(&buf);
        self.dma_pending = false;
    }

    /// Step PPU by cpu_cycles CPU cycles (= 3x PPU cycles each).
    /// Returns (frame_ready, nmi_triggered, irq_pending).
    pub fn step_ppu(&mut self, cpu_cycles: u32) -> (bool, bool, bool) {
        let cart = &mut *self.cartridge;
        let (frame, nmi) = self.ppu.step(cpu_cycles, cart);
        let irq = self.cartridge.irq_pending();
        (frame, nmi, irq)
    }

    /// Step APU by cpu_cycles.
    pub fn step_apu(&mut self, cpu_cycles: u32) {
        self.apu.step(cpu_cycles);
    }

    /// Set joypad button state.
    /// Frontend sends universal indices: Right=0, Left=1, Up=2, Down=3, A=4, B=5, Select=6, Start=7
    /// NES shift register bit order:     A=0,     B=1,    Select=2, Start=3, Up=4,  Down=5,  Left=6,  Right=7
    pub fn set_joypad(&mut self, button: u8, pressed: bool) {
        let bit: u8 = match button {
            0 => 7, // Right
            1 => 6, // Left
            2 => 4, // Up
            3 => 5, // Down
            4 => 0, // A
            5 => 1, // B
            6 => 2, // Select
            7 => 3, // Start
            _ => return,
        };
        if pressed {
            self.joypad1_state |= 1 << bit;
        } else {
            self.joypad1_state &= !(1 << bit);
        }
    }

    // -----------------------------------------------------------------------
    // Save / Load state
    // -----------------------------------------------------------------------
    pub fn save(&self, buf: &mut Vec<u8>) {
        write_slice(buf, &self.ram);
        // Joypad
        write_u8(buf, self.joypad1_state);
        write_u8(buf, self.joypad1_shift);
        write_bool(buf, self.joypad1_strobe);
        write_u8(buf, self.joypad2_state);
        write_u8(buf, self.joypad2_shift);
        // DMA
        write_bool(buf, self.dma_pending);
        write_u8(buf, self.dma_page);
        // Sub-components
        self.ppu.save(buf);
        self.apu.save(buf);
        self.cartridge.save_mapper(buf);
    }

    pub fn load(&mut self, data: &[u8], off: &mut usize) {
        self.ram.copy_from_slice(read_slice(data, off, 2048));
        self.joypad1_state  = read_u8(data, off);
        self.joypad1_shift  = read_u8(data, off);
        self.joypad1_strobe = read_bool(data, off);
        self.joypad2_state  = read_u8(data, off);
        self.joypad2_shift  = read_u8(data, off);
        self.dma_pending    = read_bool(data, off);
        self.dma_page       = read_u8(data, off);
        self.ppu.load(data, off);
        self.apu.load(data, off);
        self.cartridge.load_mapper(data, off);
    }
}
