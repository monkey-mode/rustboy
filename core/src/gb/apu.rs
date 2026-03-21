/// Audio Processing Unit for the Game Boy DMG.

use crate::save_state::*;
///
/// Four channels:
///   Channel 1 – Square wave with frequency sweep
///   Channel 2 – Square wave (no sweep)
///   Channel 3 – Wave RAM playback
///   Channel 4 – Noise (LFSR)
///
/// Output sample rate: 44100 Hz (stereo, but mixed to mono-ish f32 samples)

const SAMPLE_RATE: u32 = 44100;
const CPU_CLOCK: u32 = 4_194_304;

// ---------------------------------------------------------------------------
// Duty-cycle waveforms (8-step patterns)
// ---------------------------------------------------------------------------
const DUTY_PATTERNS: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1], // 12.5 %
    [1, 0, 0, 0, 0, 0, 0, 1], // 25 %
    [1, 0, 0, 0, 0, 1, 1, 1], // 50 %
    [0, 1, 1, 1, 1, 1, 1, 0], // 75 %
];

// ---------------------------------------------------------------------------
// Channel 1 – Square wave with sweep
// ---------------------------------------------------------------------------
struct Channel1 {
    // NR10
    sweep_period: u8,
    sweep_negate: bool,
    sweep_shift: u8,
    // NR11
    duty: u8,
    length_load: u8,
    // NR12
    env_initial: u8,
    env_add: bool,
    env_period: u8,
    // NR13 / NR14
    frequency: u16,
    length_enable: bool,

    // Internal state
    enabled: bool,
    duty_pos: u8,
    freq_timer: u16,
    length_counter: u8,
    env_volume: u8,
    env_timer: u8,
    sweep_timer: u8,
    sweep_freq: u16,
}

impl Channel1 {
    fn new() -> Self {
        Channel1 {
            sweep_period: 0,
            sweep_negate: false,
            sweep_shift: 0,
            duty: 2,
            length_load: 0,
            env_initial: 15,
            env_add: false,
            env_period: 3,
            frequency: 0,
            length_enable: false,
            enabled: false,
            duty_pos: 0,
            freq_timer: 0,
            length_counter: 0,
            env_volume: 15,
            env_timer: 3,
            sweep_timer: 0,
            sweep_freq: 0,
        }
    }


    fn trigger(&mut self) {
        self.enabled = true;
        if self.length_counter == 0 {
            self.length_counter = 64;
        }
        self.freq_timer = (2048 - self.frequency) * 4;
        self.env_volume = self.env_initial;
        self.env_timer = self.env_period;
        self.sweep_freq = self.frequency;
        self.sweep_timer = if self.sweep_period == 0 { 8 } else { self.sweep_period };
        if self.sweep_period != 0 || self.sweep_shift != 0 {
            self.calc_sweep();
        }
    }

    fn calc_sweep(&mut self) -> u16 {
        let delta = self.sweep_freq >> self.sweep_shift;
        if self.sweep_negate {
            self.sweep_freq.wrapping_sub(delta)
        } else {
            let new_freq = self.sweep_freq + delta;
            if new_freq > 2047 {
                self.enabled = false;
            }
            new_freq
        }
    }

    fn sample(&self) -> f32 {
        if !self.enabled { return 0.0; }
        let bit = DUTY_PATTERNS[self.duty as usize][self.duty_pos as usize];
        if bit == 1 { self.env_volume as f32 / 15.0 } else { 0.0 }
    }
}

// ---------------------------------------------------------------------------
// Channel 2 – Square wave (no sweep)
// ---------------------------------------------------------------------------
struct Channel2 {
    duty: u8,
    length_load: u8,
    env_initial: u8,
    env_add: bool,
    env_period: u8,
    frequency: u16,
    length_enable: bool,

    enabled: bool,
    duty_pos: u8,
    freq_timer: u16,
    length_counter: u8,
    env_volume: u8,
    env_timer: u8,
}

impl Channel2 {
    fn new() -> Self {
        Channel2 {
            duty: 2,
            length_load: 0,
            env_initial: 0,
            env_add: false,
            env_period: 0,
            frequency: 0,
            length_enable: false,
            enabled: false,
            duty_pos: 0,
            freq_timer: 0,
            length_counter: 0,
            env_volume: 0,
            env_timer: 0,
        }
    }

    fn trigger(&mut self) {
        self.enabled = true;
        if self.length_counter == 0 {
            self.length_counter = 64;
        }
        self.freq_timer = (2048 - self.frequency) * 4;
        self.env_volume = self.env_initial;
        self.env_timer = self.env_period;
    }

    fn sample(&self) -> f32 {
        if !self.enabled { return 0.0; }
        let bit = DUTY_PATTERNS[self.duty as usize][self.duty_pos as usize];
        if bit == 1 { self.env_volume as f32 / 15.0 } else { 0.0 }
    }
}

// ---------------------------------------------------------------------------
// Channel 3 – Wave RAM
// ---------------------------------------------------------------------------
struct Channel3 {
    dac_power: bool,
    length_load: u8,
    volume_code: u8,
    frequency: u16,
    length_enable: bool,
    wave_ram: [u8; 16],

    enabled: bool,
    position: u8,
    freq_timer: u16,
    length_counter: u16,
}

impl Channel3 {
    fn new() -> Self {
        Channel3 {
            dac_power: false,
            length_load: 0,
            volume_code: 0,
            frequency: 0,
            length_enable: false,
            wave_ram: [0; 16],
            enabled: false,
            position: 0,
            freq_timer: 0,
            length_counter: 0,
        }
    }

    fn trigger(&mut self) {
        self.enabled = true;
        if self.length_counter == 0 {
            self.length_counter = 256;
        }
        self.freq_timer = (2048 - self.frequency) * 2;
        self.position = 0;
    }

    fn sample(&self) -> f32 {
        if !self.enabled || !self.dac_power { return 0.0; }
        let byte = self.wave_ram[(self.position / 2) as usize];
        let nibble = if self.position & 1 == 0 { byte >> 4 } else { byte & 0x0F };
        let shifted = match self.volume_code & 0x03 {
            0 => 0,
            1 => nibble,
            2 => nibble >> 1,
            3 => nibble >> 2,
            _ => 0,
        };
        shifted as f32 / 15.0
    }
}

// ---------------------------------------------------------------------------
// Channel 4 – Noise (LFSR)
// ---------------------------------------------------------------------------
struct Channel4 {
    length_load: u8,
    env_initial: u8,
    env_add: bool,
    env_period: u8,
    clock_shift: u8,
    lfsr_width: bool, // true = 7-bit, false = 15-bit
    clock_divider: u8,
    length_enable: bool,

    enabled: bool,
    lfsr: u16,
    freq_timer: u32,
    length_counter: u8,
    env_volume: u8,
    env_timer: u8,
}

impl Channel4 {
    fn new() -> Self {
        Channel4 {
            length_load: 0,
            env_initial: 0,
            env_add: false,
            env_period: 0,
            clock_shift: 0,
            lfsr_width: false,
            clock_divider: 0,
            length_enable: false,
            enabled: false,
            lfsr: 0x7FFF,
            freq_timer: 0,
            length_counter: 0,
            env_volume: 0,
            env_timer: 0,
        }
    }

    fn trigger(&mut self) {
        self.enabled = true;
        if self.length_counter == 0 {
            self.length_counter = 64;
        }
        let divider = if self.clock_divider == 0 { 8u32 } else { self.clock_divider as u32 * 16 };
        self.freq_timer = divider << self.clock_shift;
        self.env_volume = self.env_initial;
        self.env_timer = self.env_period;
        self.lfsr = 0x7FFF;
    }

    fn sample(&self) -> f32 {
        if !self.enabled { return 0.0; }
        // Bit 0 of LFSR: 0 = high (sound), 1 = low (silence)
        if self.lfsr & 1 == 0 { self.env_volume as f32 / 15.0 } else { 0.0 }
    }
}

// ---------------------------------------------------------------------------
// APU
// ---------------------------------------------------------------------------
pub struct Apu {
    ch1: Channel1,
    ch2: Channel2,
    ch3: Channel3,
    ch4: Channel4,

    // NR50 / NR51 / NR52
    nr50: u8,
    nr51: u8,
    nr52: u8,

    // Cycle accumulator for sampling
    cycle_acc: u32,
    // Cycles per sample (CPU_CLOCK / SAMPLE_RATE)
    cycles_per_sample: u32,
    // Sample buffer
    buffer: Vec<f32>,

    // Frame sequencer (runs at 512 Hz → every 8192 CPU cycles)
    frame_seq_cycles: u32,
    frame_seq_step: u8,
}

impl Apu {
    pub fn new() -> Self {
        Apu {
            ch1: Channel1::new(),
            ch2: Channel2::new(),
            ch3: Channel3::new(),
            ch4: Channel4::new(),
            nr50: 0x77,
            nr51: 0xF3,
            nr52: 0xF1,
            cycle_acc: 0,
            cycles_per_sample: CPU_CLOCK / SAMPLE_RATE,
            buffer: Vec::new(),
            frame_seq_cycles: 0,
            frame_seq_step: 0,
        }
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            0xFF10 => {
                0x80 | (self.ch1.sweep_period << 4)
                    | (if self.ch1.sweep_negate { 0x08 } else { 0 })
                    | self.ch1.sweep_shift
            }
            0xFF11 => (self.ch1.duty << 6) | (self.ch1.length_load & 0x3F),
            0xFF12 => {
                (self.ch1.env_initial << 4)
                    | (if self.ch1.env_add { 0x08 } else { 0 })
                    | self.ch1.env_period
            }
            0xFF13 => 0xFF, // write-only
            0xFF14 => 0x80 | (if self.ch1.length_enable { 0x40 } else { 0 }) | 0x3F,

            0xFF16 => (self.ch2.duty << 6) | (self.ch2.length_load & 0x3F),
            0xFF17 => {
                (self.ch2.env_initial << 4)
                    | (if self.ch2.env_add { 0x08 } else { 0 })
                    | self.ch2.env_period
            }
            0xFF18 => 0xFF,
            0xFF19 => 0x80 | (if self.ch2.length_enable { 0x40 } else { 0 }) | 0x3F,

            0xFF1A => if self.ch3.dac_power { 0xFF } else { 0x7F },
            0xFF1B => self.ch3.length_load,
            0xFF1C => 0x9F | (self.ch3.volume_code << 5),
            0xFF1D => 0xFF,
            0xFF1E => 0x80 | (if self.ch3.length_enable { 0x40 } else { 0 }) | 0x3F,

            0xFF20 => 0xFF,
            0xFF21 => {
                (self.ch4.env_initial << 4)
                    | (if self.ch4.env_add { 0x08 } else { 0 })
                    | self.ch4.env_period
            }
            0xFF22 => {
                (self.ch4.clock_shift << 4)
                    | (if self.ch4.lfsr_width { 0x08 } else { 0 })
                    | self.ch4.clock_divider
            }
            0xFF23 => 0x80 | (if self.ch4.length_enable { 0x40 } else { 0 }),

            0xFF24 => self.nr50,
            0xFF25 => self.nr51,
            0xFF26 => {
                let mut v = if self.nr52 & 0x80 != 0 { 0x80 } else { 0 };
                if self.ch1.enabled { v |= 0x01; }
                if self.ch2.enabled { v |= 0x02; }
                if self.ch3.enabled { v |= 0x04; }
                if self.ch4.enabled { v |= 0x08; }
                v | 0x70
            }

            0xFF30..=0xFF3F => self.ch3.wave_ram[(addr - 0xFF30) as usize],

            _ => 0xFF,
        }
    }

    pub fn write_byte(&mut self, addr: u16, val: u8) {
        match addr {
            // Channel 1
            0xFF10 => {
                self.ch1.sweep_period = (val >> 4) & 0x07;
                self.ch1.sweep_negate = val & 0x08 != 0;
                self.ch1.sweep_shift = val & 0x07;
            }
            0xFF11 => {
                self.ch1.duty = val >> 6;
                self.ch1.length_load = val & 0x3F;
                self.ch1.length_counter = 64 - (val & 0x3F);
            }
            0xFF12 => {
                self.ch1.env_initial = val >> 4;
                self.ch1.env_add = val & 0x08 != 0;
                self.ch1.env_period = val & 0x07;
            }
            0xFF13 => {
                self.ch1.frequency = (self.ch1.frequency & 0x0700) | val as u16;
            }
            0xFF14 => {
                self.ch1.frequency = (self.ch1.frequency & 0x00FF) | (((val & 0x07) as u16) << 8);
                self.ch1.length_enable = val & 0x40 != 0;
                if val & 0x80 != 0 {
                    self.ch1.trigger();
                }
            }

            // Channel 2
            0xFF16 => {
                self.ch2.duty = val >> 6;
                self.ch2.length_load = val & 0x3F;
                self.ch2.length_counter = 64 - (val & 0x3F);
            }
            0xFF17 => {
                self.ch2.env_initial = val >> 4;
                self.ch2.env_add = val & 0x08 != 0;
                self.ch2.env_period = val & 0x07;
            }
            0xFF18 => {
                self.ch2.frequency = (self.ch2.frequency & 0x0700) | val as u16;
            }
            0xFF19 => {
                self.ch2.frequency = (self.ch2.frequency & 0x00FF) | (((val & 0x07) as u16) << 8);
                self.ch2.length_enable = val & 0x40 != 0;
                if val & 0x80 != 0 {
                    self.ch2.trigger();
                }
            }

            // Channel 3
            0xFF1A => { self.ch3.dac_power = val & 0x80 != 0; }
            0xFF1B => {
                self.ch3.length_load = val;
                self.ch3.length_counter = 256 - val as u16;
            }
            0xFF1C => { self.ch3.volume_code = (val >> 5) & 0x03; }
            0xFF1D => {
                self.ch3.frequency = (self.ch3.frequency & 0x0700) | val as u16;
            }
            0xFF1E => {
                self.ch3.frequency = (self.ch3.frequency & 0x00FF) | (((val & 0x07) as u16) << 8);
                self.ch3.length_enable = val & 0x40 != 0;
                if val & 0x80 != 0 {
                    self.ch3.trigger();
                }
            }

            // Channel 4
            0xFF20 => {
                self.ch4.length_load = val & 0x3F;
                self.ch4.length_counter = 64 - (val & 0x3F);
            }
            0xFF21 => {
                self.ch4.env_initial = val >> 4;
                self.ch4.env_add = val & 0x08 != 0;
                self.ch4.env_period = val & 0x07;
            }
            0xFF22 => {
                self.ch4.clock_shift = val >> 4;
                self.ch4.lfsr_width = val & 0x08 != 0;
                self.ch4.clock_divider = val & 0x07;
            }
            0xFF23 => {
                self.ch4.length_enable = val & 0x40 != 0;
                if val & 0x80 != 0 {
                    self.ch4.trigger();
                }
            }

            // Master
            0xFF24 => { self.nr50 = val; }
            0xFF25 => { self.nr51 = val; }
            0xFF26 => {
                self.nr52 = val & 0x80;
                if self.nr52 == 0 {
                    // Power off – reset all channels
                    self.ch1 = Channel1::new();
                    self.ch2 = Channel2::new();
                    self.ch3 = Channel3::new();
                    self.ch4 = Channel4::new();
                }
            }

            // Wave RAM
            0xFF30..=0xFF3F => {
                self.ch3.wave_ram[(addr - 0xFF30) as usize] = val;
            }

            _ => {}
        }
    }

    /// Advance APU by `cycles` CPU clock ticks.
    pub fn step(&mut self, cycles: u32) {
        let apu_on = self.nr52 & 0x80 != 0;

        // Frame sequencer ticks at 512 Hz (every 8192 CPU cycles)
        self.frame_seq_cycles += cycles;
        while self.frame_seq_cycles >= 8192 {
            self.frame_seq_cycles -= 8192;
            if apu_on {
                self.tick_frame_sequencer();
            }
        }

        if apu_on {
            self.tick_ch1(cycles);
            self.tick_ch2(cycles);
            self.tick_ch3(cycles);
            self.tick_ch4(cycles);
        }

        // Emit samples
        self.cycle_acc += cycles;
        while self.cycle_acc >= self.cycles_per_sample {
            self.cycle_acc -= self.cycles_per_sample;
            let sample = if apu_on {
                let s1 = self.ch1.sample();
                let s2 = self.ch2.sample();
                let s3 = self.ch3.sample();
                let s4 = self.ch4.sample();
                (s1 + s2 + s3 + s4) * 0.25
            } else {
                0.0
            };
            self.buffer.push(sample);
        }
    }

    fn tick_frame_sequencer(&mut self) {
        match self.frame_seq_step {
            0 => { self.clock_length(); }
            1 => {}
            2 => { self.clock_length(); self.clock_sweep(); }
            3 => {}
            4 => { self.clock_length(); }
            5 => {}
            6 => { self.clock_length(); self.clock_sweep(); }
            7 => { self.clock_envelope(); }
            _ => {}
        }
        self.frame_seq_step = (self.frame_seq_step + 1) & 7;
    }

    fn clock_length(&mut self) {
        if self.ch1.length_enable && self.ch1.length_counter > 0 {
            self.ch1.length_counter -= 1;
            if self.ch1.length_counter == 0 { self.ch1.enabled = false; }
        }
        if self.ch2.length_enable && self.ch2.length_counter > 0 {
            self.ch2.length_counter -= 1;
            if self.ch2.length_counter == 0 { self.ch2.enabled = false; }
        }
        if self.ch3.length_enable && self.ch3.length_counter > 0 {
            self.ch3.length_counter -= 1;
            if self.ch3.length_counter == 0 { self.ch3.enabled = false; }
        }
        if self.ch4.length_enable && self.ch4.length_counter > 0 {
            self.ch4.length_counter -= 1;
            if self.ch4.length_counter == 0 { self.ch4.enabled = false; }
        }
    }

    fn clock_sweep(&mut self) {
        if self.ch1.sweep_timer > 0 { self.ch1.sweep_timer -= 1; }
        if self.ch1.sweep_timer == 0 {
            self.ch1.sweep_timer = if self.ch1.sweep_period == 0 { 8 } else { self.ch1.sweep_period };
            if self.ch1.sweep_period != 0 {
                let new_freq = self.ch1.calc_sweep();
                if new_freq <= 2047 && self.ch1.sweep_shift != 0 {
                    self.ch1.sweep_freq = new_freq;
                    self.ch1.frequency = new_freq;
                    self.ch1.calc_sweep(); // check again for overflow
                }
            }
        }
    }

    fn clock_envelope(&mut self) {
        macro_rules! env_tick {
            ($ch:expr) => {
                if $ch.env_period != 0 {
                    if $ch.env_timer > 0 { $ch.env_timer -= 1; }
                    if $ch.env_timer == 0 {
                        $ch.env_timer = $ch.env_period;
                        if $ch.env_add && $ch.env_volume < 15 {
                            $ch.env_volume += 1;
                        } else if !$ch.env_add && $ch.env_volume > 0 {
                            $ch.env_volume -= 1;
                        }
                    }
                }
            };
        }
        env_tick!(self.ch1);
        env_tick!(self.ch2);
        env_tick!(self.ch4);
    }

    fn tick_ch1(&mut self, cycles: u32) {
        let mut rem = cycles as i32;
        while rem > 0 {
            let step = rem.min(self.ch1.freq_timer as i32);
            self.ch1.freq_timer -= step as u16;
            rem -= step;
            if self.ch1.freq_timer == 0 {
                self.ch1.freq_timer = (2048 - self.ch1.frequency) * 4;
                self.ch1.duty_pos = (self.ch1.duty_pos + 1) & 7;
            }
        }
    }

    fn tick_ch2(&mut self, cycles: u32) {
        let mut rem = cycles as i32;
        while rem > 0 {
            let step = rem.min(self.ch2.freq_timer as i32);
            self.ch2.freq_timer -= step as u16;
            rem -= step;
            if self.ch2.freq_timer == 0 {
                self.ch2.freq_timer = (2048 - self.ch2.frequency) * 4;
                self.ch2.duty_pos = (self.ch2.duty_pos + 1) & 7;
            }
        }
    }

    fn tick_ch3(&mut self, cycles: u32) {
        if !self.ch3.enabled { return; }
        let mut rem = cycles as i32;
        while rem > 0 {
            let step = rem.min(self.ch3.freq_timer as i32);
            self.ch3.freq_timer -= step as u16;
            rem -= step;
            if self.ch3.freq_timer == 0 {
                self.ch3.freq_timer = (2048 - self.ch3.frequency) * 2;
                self.ch3.position = (self.ch3.position + 1) & 31;
            }
        }
    }

    fn tick_ch4(&mut self, cycles: u32) {
        if !self.ch4.enabled { return; }
        let mut rem = cycles as i32;
        while rem > 0 {
            let timer = self.ch4.freq_timer.min(u32::MAX) as i32;
            let step = rem.min(timer);
            self.ch4.freq_timer -= step as u32;
            rem -= step;
            if self.ch4.freq_timer == 0 {
                let divider = if self.ch4.clock_divider == 0 { 8u32 } else { self.ch4.clock_divider as u32 * 16 };
                self.ch4.freq_timer = divider << self.ch4.clock_shift;
                let bit = (self.ch4.lfsr ^ (self.ch4.lfsr >> 1)) & 1;
                self.ch4.lfsr >>= 1;
                self.ch4.lfsr |= bit << 14;
                if self.ch4.lfsr_width {
                    self.ch4.lfsr = (self.ch4.lfsr & !0x40) | (bit << 6);
                }
            }
        }
    }

    /// Drain and return accumulated audio samples.
    pub fn get_samples(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.buffer)
    }

    // -----------------------------------------------------------------------
    // Save / Load state
    // -----------------------------------------------------------------------
    pub fn save(&self, buf: &mut Vec<u8>) {
        // Channel 1
        write_u8(buf, self.ch1.sweep_period);
        write_bool(buf, self.ch1.sweep_negate);
        write_u8(buf, self.ch1.sweep_shift);
        write_u8(buf, self.ch1.duty);
        write_u8(buf, self.ch1.length_load);
        write_u8(buf, self.ch1.env_initial);
        write_bool(buf, self.ch1.env_add);
        write_u8(buf, self.ch1.env_period);
        write_u16(buf, self.ch1.frequency);
        write_bool(buf, self.ch1.length_enable);
        write_bool(buf, self.ch1.enabled);
        write_u8(buf, self.ch1.duty_pos);
        write_u16(buf, self.ch1.freq_timer);
        write_u8(buf, self.ch1.length_counter);
        write_u8(buf, self.ch1.env_volume);
        write_u8(buf, self.ch1.env_timer);
        write_u8(buf, self.ch1.sweep_timer);
        write_u16(buf, self.ch1.sweep_freq);
        // Channel 2
        write_u8(buf, self.ch2.duty);
        write_u8(buf, self.ch2.length_load);
        write_u8(buf, self.ch2.env_initial);
        write_bool(buf, self.ch2.env_add);
        write_u8(buf, self.ch2.env_period);
        write_u16(buf, self.ch2.frequency);
        write_bool(buf, self.ch2.length_enable);
        write_bool(buf, self.ch2.enabled);
        write_u8(buf, self.ch2.duty_pos);
        write_u16(buf, self.ch2.freq_timer);
        write_u8(buf, self.ch2.length_counter);
        write_u8(buf, self.ch2.env_volume);
        write_u8(buf, self.ch2.env_timer);
        // Channel 3
        write_bool(buf, self.ch3.dac_power);
        write_u8(buf, self.ch3.length_load);
        write_u8(buf, self.ch3.volume_code);
        write_u16(buf, self.ch3.frequency);
        write_bool(buf, self.ch3.length_enable);
        write_slice(buf, &self.ch3.wave_ram);
        write_bool(buf, self.ch3.enabled);
        write_u8(buf, self.ch3.position);
        write_u16(buf, self.ch3.freq_timer);
        write_u16(buf, self.ch3.length_counter);
        // Channel 4
        write_u8(buf, self.ch4.length_load);
        write_u8(buf, self.ch4.env_initial);
        write_bool(buf, self.ch4.env_add);
        write_u8(buf, self.ch4.env_period);
        write_u8(buf, self.ch4.clock_shift);
        write_bool(buf, self.ch4.lfsr_width);
        write_u8(buf, self.ch4.clock_divider);
        write_bool(buf, self.ch4.length_enable);
        write_bool(buf, self.ch4.enabled);
        write_u16(buf, self.ch4.lfsr);
        write_u32(buf, self.ch4.freq_timer);
        write_u8(buf, self.ch4.length_counter);
        write_u8(buf, self.ch4.env_volume);
        write_u8(buf, self.ch4.env_timer);
        // Master
        write_u8(buf, self.nr50);
        write_u8(buf, self.nr51);
        write_u8(buf, self.nr52);
        // Frame sequencer
        write_u32(buf, self.frame_seq_cycles);
        write_u8(buf, self.frame_seq_step);
        // Cycle accumulator (do NOT save audio buffer – will be regenerated)
        write_u32(buf, self.cycle_acc);
    }

    pub fn load(&mut self, data: &[u8], off: &mut usize) {
        // Channel 1
        self.ch1.sweep_period = read_u8(data, off);
        self.ch1.sweep_negate = read_bool(data, off);
        self.ch1.sweep_shift = read_u8(data, off);
        self.ch1.duty = read_u8(data, off);
        self.ch1.length_load = read_u8(data, off);
        self.ch1.env_initial = read_u8(data, off);
        self.ch1.env_add = read_bool(data, off);
        self.ch1.env_period = read_u8(data, off);
        self.ch1.frequency = read_u16(data, off);
        self.ch1.length_enable = read_bool(data, off);
        self.ch1.enabled = read_bool(data, off);
        self.ch1.duty_pos = read_u8(data, off);
        self.ch1.freq_timer = read_u16(data, off);
        self.ch1.length_counter = read_u8(data, off);
        self.ch1.env_volume = read_u8(data, off);
        self.ch1.env_timer = read_u8(data, off);
        self.ch1.sweep_timer = read_u8(data, off);
        self.ch1.sweep_freq = read_u16(data, off);
        // Channel 2
        self.ch2.duty = read_u8(data, off);
        self.ch2.length_load = read_u8(data, off);
        self.ch2.env_initial = read_u8(data, off);
        self.ch2.env_add = read_bool(data, off);
        self.ch2.env_period = read_u8(data, off);
        self.ch2.frequency = read_u16(data, off);
        self.ch2.length_enable = read_bool(data, off);
        self.ch2.enabled = read_bool(data, off);
        self.ch2.duty_pos = read_u8(data, off);
        self.ch2.freq_timer = read_u16(data, off);
        self.ch2.length_counter = read_u8(data, off);
        self.ch2.env_volume = read_u8(data, off);
        self.ch2.env_timer = read_u8(data, off);
        // Channel 3
        self.ch3.dac_power = read_bool(data, off);
        self.ch3.length_load = read_u8(data, off);
        self.ch3.volume_code = read_u8(data, off);
        self.ch3.frequency = read_u16(data, off);
        self.ch3.length_enable = read_bool(data, off);
        self.ch3.wave_ram.copy_from_slice(read_slice(data, off, 16));
        self.ch3.enabled = read_bool(data, off);
        self.ch3.position = read_u8(data, off);
        self.ch3.freq_timer = read_u16(data, off);
        self.ch3.length_counter = read_u16(data, off);
        // Channel 4
        self.ch4.length_load = read_u8(data, off);
        self.ch4.env_initial = read_u8(data, off);
        self.ch4.env_add = read_bool(data, off);
        self.ch4.env_period = read_u8(data, off);
        self.ch4.clock_shift = read_u8(data, off);
        self.ch4.lfsr_width = read_bool(data, off);
        self.ch4.clock_divider = read_u8(data, off);
        self.ch4.length_enable = read_bool(data, off);
        self.ch4.enabled = read_bool(data, off);
        self.ch4.lfsr = read_u16(data, off);
        self.ch4.freq_timer = read_u32(data, off);
        self.ch4.length_counter = read_u8(data, off);
        self.ch4.env_volume = read_u8(data, off);
        self.ch4.env_timer = read_u8(data, off);
        // Master
        self.nr50 = read_u8(data, off);
        self.nr51 = read_u8(data, off);
        self.nr52 = read_u8(data, off);
        // Frame sequencer
        self.frame_seq_cycles = read_u32(data, off);
        self.frame_seq_step = read_u8(data, off);
        // Cycle accumulator
        self.cycle_acc = read_u32(data, off);
        // Clear audio buffer on state load
        self.buffer.clear();
    }
}
