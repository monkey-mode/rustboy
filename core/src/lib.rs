/// rustboy-core – wasm-bindgen entry point.
///
/// Auto-detects the system from ROM bytes:
///   NES: first 4 bytes == [0x4E, 0x45, 0x53, 0x1A]
///   Otherwise: Game Boy DMG

mod gb;
mod nes;
mod save_state;

use wasm_bindgen::prelude::*;

enum SystemEmulator {
    Gb(gb::GbEmulator),
    Nes(nes::NesEmulator),
}

#[wasm_bindgen]
pub struct Emulator {
    inner: SystemEmulator,
}

#[wasm_bindgen]
impl Emulator {
    /// Create a new emulator, auto-detecting Game Boy or NES from ROM header.
    #[wasm_bindgen(constructor)]
    pub fn new(rom: &[u8]) -> Emulator {
        let inner = if rom.len() >= 4 && &rom[0..4] == &[0x4E, 0x45, 0x53, 0x1A] {
            SystemEmulator::Nes(nes::NesEmulator::new(rom))
        } else {
            SystemEmulator::Gb(gb::GbEmulator::new(rom))
        };
        Emulator { inner }
    }

    /// Run exactly one full frame.
    pub fn step_frame(&mut self) {
        match &mut self.inner {
            SystemEmulator::Gb(e) => e.step_frame(),
            SystemEmulator::Nes(e) => e.step_frame(),
        }
    }

    /// Returns the RGBA frame buffer as a copied Vec<u8>.
    /// GB: 160×144×4 = 92160 bytes. NES: 256×240×4 = 245760 bytes.
    pub fn frame_buffer(&self) -> Vec<u8> {
        match &self.inner {
            SystemEmulator::Gb(e) => e.frame_buffer().to_vec(),
            SystemEmulator::Nes(e) => e.frame_buffer().to_vec(),
        }
    }

    /// Screen width in pixels.
    pub fn frame_width(&self) -> u32 {
        match &self.inner {
            SystemEmulator::Gb(_) => 160,
            SystemEmulator::Nes(_) => 256,
        }
    }

    /// Screen height in pixels.
    pub fn frame_height(&self) -> u32 {
        match &self.inner {
            SystemEmulator::Gb(_) => 144,
            SystemEmulator::Nes(_) => 240,
        }
    }

    /// Set joypad button state.
    /// GB  – Right=0, Left=1, Up=2, Down=3, A=4, B=5, Select=6, Start=7
    /// NES – A=0, B=1, Select=2, Start=3, Up=4, Down=5, Left=6, Right=7
    pub fn set_joypad(&mut self, button: u8, pressed: bool) {
        match &mut self.inner {
            SystemEmulator::Gb(e) => e.set_joypad(button, pressed),
            SystemEmulator::Nes(e) => e.set_joypad(button, pressed),
        }
    }

    /// Drain and return accumulated audio samples since the last call.
    pub fn audio_buffer(&mut self) -> Vec<f32> {
        match &mut self.inner {
            SystemEmulator::Gb(e) => e.get_audio_samples(),
            SystemEmulator::Nes(e) => e.get_audio_samples(),
        }
    }

    /// Snapshot the full emulator state into a byte vector.
    pub fn save_state(&self) -> Vec<u8> {
        match &self.inner {
            SystemEmulator::Gb(e) => e.save_state(),
            SystemEmulator::Nes(e) => e.save_state(),
        }
    }

    /// Restore the emulator state from a previously saved byte vector.
    pub fn load_state(&mut self, data: &[u8]) {
        match &mut self.inner {
            SystemEmulator::Gb(e) => e.load_state(data),
            SystemEmulator::Nes(e) => e.load_state(data),
        }
    }
}
