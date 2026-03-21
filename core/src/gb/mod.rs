/// Game Boy DMG backend.

pub mod cpu;
pub mod ppu;
pub mod apu;
pub mod mmu;
pub mod timer;

pub use cpu::Cpu;
pub use mmu::Mmu;

use crate::save_state::*;

/// Cycles per frame: 4.194304 MHz / 59.7 fps ≈ 70224 cycles.
const CYCLES_PER_FRAME: u32 = 70_224;

/// High-level Game Boy emulator wrapping CPU + MMU.
pub struct GbEmulator {
    pub cpu: Cpu,
    pub mmu: Mmu,
    cycles_this_frame: u32,
}

impl GbEmulator {
    pub fn new(rom: &[u8]) -> Self {
        let mmu = Mmu::new(rom.to_vec());
        let cpu = Cpu::new();
        GbEmulator { cpu, mmu, cycles_this_frame: 0 }
    }

    pub fn step_frame(&mut self) {
        let target = self.cycles_this_frame + CYCLES_PER_FRAME;
        while self.cycles_this_frame < target {
            let irq_cycles = self.cpu.handle_interrupts(&mut self.mmu);
            if irq_cycles > 0 {
                self.tick(irq_cycles);
                continue;
            }
            let cycles = self.cpu.step(&mut self.mmu);
            self.tick(cycles);
        }
        self.cycles_this_frame -= CYCLES_PER_FRAME;
    }

    pub fn frame_buffer(&self) -> &[u8] {
        &self.mmu.ppu.frame_buffer
    }

    pub fn set_joypad(&mut self, button: u8, pressed: bool) {
        self.mmu.set_joypad(button, pressed);
    }

    pub fn get_audio_samples(&mut self) -> Vec<f32> {
        self.mmu.apu.get_samples()
    }

    fn tick(&mut self, cycles: u32) {
        self.cycles_this_frame += cycles;
        self.mmu.step_timer(cycles);
        self.mmu.step_ppu(cycles);
        self.mmu.step_apu(cycles);
    }

    // -----------------------------------------------------------------------
    // Save / Load state
    // Magic header: "GBSS" = [0x47, 0x42, 0x53, 0x53], version 0x01
    // -----------------------------------------------------------------------
    pub fn save_state(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        // Header
        buf.extend_from_slice(&[0x47, 0x42, 0x53, 0x53, 0x01]);
        // Frame cycles
        write_u32(&mut buf, self.cycles_this_frame);
        // CPU + MMU (MMU includes PPU, APU, Timer)
        self.cpu.save(&mut buf);
        self.mmu.save(&mut buf);
        buf
    }

    pub fn load_state(&mut self, data: &[u8]) {
        // Validate header
        if data.len() < 5 || &data[0..4] != &[0x47, 0x42, 0x53, 0x53] || data[4] != 0x01 {
            return; // invalid or incompatible save state
        }
        let mut off = 5usize;
        self.cycles_this_frame = read_u32(data, &mut off);
        self.cpu.load(data, &mut off);
        self.mmu.load(data, &mut off);
    }
}
