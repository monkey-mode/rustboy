/// Game Boy DMG backend.

pub mod cpu;
pub mod ppu;
pub mod apu;
pub mod mmu;
pub mod timer;

pub use cpu::Cpu;
pub use mmu::Mmu;

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
}
