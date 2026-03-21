/// NES backend.

pub mod cpu;
pub mod ppu;
pub mod apu;
pub mod bus;
pub mod cartridge;

pub use bus::NesBus;

use cpu::NesCpu;
use crate::save_state::*;

/// CPU cycles per NES NTSC frame: 1789773 Hz / 60.0988 fps ≈ 29780 cycles.
const CYCLES_PER_FRAME: u32 = 29_780;

/// High-level NES emulator.
pub struct NesEmulator {
    cpu: NesCpu,
    bus: NesBus,
    pending_nmi: bool,
    pending_irq: bool,
}

impl NesEmulator {
    pub fn new(rom: &[u8]) -> Self {
        let mapper = cartridge::parse_ines(rom);
        let mut bus = NesBus::new(mapper);
        let mut cpu = NesCpu::new();
        cpu.reset(&mut bus);
        NesEmulator {
            cpu,
            bus,
            pending_nmi: false,
            pending_irq: false,
        }
    }

    pub fn step_frame(&mut self) {
        let mut total_cycles: u32 = 0;

        while total_cycles < CYCLES_PER_FRAME {
            // Handle OAM DMA (takes 513/514 CPU cycles)
            if self.bus.dma_pending {
                self.bus.do_oam_dma();
                total_cycles += 513;
                // Step PPU for the DMA cycles
                let (frame_ready, nmi) = self.bus.step_ppu(513);
                self.bus.step_apu(513);
                if nmi { self.pending_nmi = true; }
                if frame_ready { /* frame done mid-DMA – rare, continue */ }
            }

            // Handle NMI
            if self.pending_nmi {
                self.pending_nmi = false;
                self.cpu.nmi(&mut self.bus);
            }

            // Handle IRQ
            if self.pending_irq {
                self.pending_irq = false;
                self.cpu.irq(&mut self.bus);
            }

            // Execute one CPU instruction
            let cycles = self.cpu.step(&mut self.bus);
            total_cycles += cycles;

            // Step PPU and APU
            let (_, nmi) = self.bus.step_ppu(cycles);
            self.bus.step_apu(cycles);

            if nmi { self.pending_nmi = true; }
        }
    }

    pub fn frame_buffer(&self) -> &[u8] {
        &self.bus.ppu.frame_buffer
    }

    pub fn set_joypad(&mut self, button: u8, pressed: bool) {
        self.bus.set_joypad(button, pressed);
    }

    pub fn get_audio_samples(&mut self) -> Vec<f32> {
        self.bus.apu.get_samples()
    }

    // -----------------------------------------------------------------------
    // Save / Load state
    // Magic header: "NESS" = [0x4E, 0x45, 0x53, 0x53], version 0x01
    // -----------------------------------------------------------------------
    pub fn save_state(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        // Header
        buf.extend_from_slice(&[0x4E, 0x45, 0x53, 0x53, 0x01]);
        // Pending interrupt flags
        write_bool(&mut buf, self.pending_nmi);
        write_bool(&mut buf, self.pending_irq);
        // CPU + Bus (Bus includes PPU, APU, Cartridge)
        self.cpu.save(&mut buf);
        self.bus.save(&mut buf);
        buf
    }

    pub fn load_state(&mut self, data: &[u8]) {
        if data.len() < 5 || &data[0..4] != &[0x4E, 0x45, 0x53, 0x53] || data[4] != 0x01 {
            return;
        }
        let mut off = 5usize;
        self.pending_nmi = read_bool(data, &mut off);
        self.pending_irq = read_bool(data, &mut off);
        self.cpu.load(data, &mut off);
        self.bus.load(data, &mut off);
    }
}
