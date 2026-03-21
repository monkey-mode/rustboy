/// NES APU – Ricoh 2A03 audio processing unit.
///
/// Channels: Pulse1, Pulse2, Triangle, Noise, DMC (stub)
/// Frame sequencer: 4-step and 5-step modes
/// Output sample rate: 44100 Hz

use crate::save_state::*;

const SAMPLE_RATE: u32 = 44100;
const CPU_CLOCK: f64 = 1_789_773.0;

// Length counter lookup table (index = 5-bit value from register)
const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6,
    160, 8, 60, 10, 14, 12, 26, 14,
    12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];

// Noise period table (NTSC)
const NOISE_PERIOD_TABLE: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160,
    202, 254, 380, 508, 762, 1016, 2034, 4068,
];

// Pulse duty cycle waveforms
const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 1, 0, 0, 0, 0, 0, 0], // 12.5%
    [0, 1, 1, 0, 0, 0, 0, 0], // 25%
    [0, 1, 1, 1, 1, 0, 0, 0], // 50%
    [1, 0, 0, 1, 1, 1, 1, 1], // 75% (negated 25%)
];

// ---------------------------------------------------------------------------
// Envelope
// ---------------------------------------------------------------------------
struct Envelope {
    start: bool,
    loop_flag: bool,
    constant: bool,
    period: u8,
    divider: u8,
    decay: u8,
}

impl Envelope {
    fn new() -> Self {
        Envelope { start: false, loop_flag: false, constant: false, period: 0, divider: 0, decay: 0 }
    }

    fn volume(&self) -> u8 {
        if self.constant { self.period } else { self.decay }
    }

    fn clock(&mut self) {
        if self.start {
            self.start = false;
            self.decay = 15;
            self.divider = self.period;
        } else if self.divider == 0 {
            self.divider = self.period;
            if self.decay > 0 {
                self.decay -= 1;
            } else if self.loop_flag {
                self.decay = 15;
            }
        } else {
            self.divider -= 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Sweep unit (for pulse channels)
// ---------------------------------------------------------------------------
struct Sweep {
    enabled: bool,
    period: u8,
    negate: bool,
    shift: u8,
    reload: bool,
    divider: u8,
    mute: bool,
}

impl Sweep {
    fn new() -> Self {
        Sweep { enabled: false, period: 0, negate: false, shift: 0, reload: false, divider: 0, mute: false }
    }

    fn compute_target(&self, period: u16, channel_one: bool) -> u16 {
        let delta = period >> self.shift;
        if self.negate {
            if channel_one {
                // Pulse 1 uses one's complement
                period.wrapping_sub(delta).wrapping_sub(1)
            } else {
                period.wrapping_sub(delta)
            }
        } else {
            period + delta
        }
    }

    fn clock(&mut self, period: &mut u16, channel_one: bool) {
        let target = self.compute_target(*period, channel_one);
        self.mute = *period < 8 || target > 0x7FF;

        if self.divider == 0 && self.enabled && self.shift != 0 && !self.mute {
            *period = target;
        }

        if self.divider == 0 || self.reload {
            self.divider = self.period;
            self.reload = false;
        } else {
            self.divider -= 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Pulse channel
// ---------------------------------------------------------------------------
#[allow(dead_code)]
struct PulseChannel {
    enabled: bool,
    duty: u8,
    duty_pos: u8,
    timer_period: u16,
    timer: u16,
    length: u8,
    length_halt: bool,
    envelope: Envelope,
    sweep: Sweep,
    channel_one: bool,
}

impl PulseChannel {
    fn new(channel_one: bool) -> Self {
        PulseChannel {
            enabled: false,
            duty: 0,
            duty_pos: 0,
            timer_period: 0,
            timer: 0,
            length: 0,
            length_halt: false,
            envelope: Envelope::new(),
            sweep: Sweep::new(),
            channel_one,
        }
    }

    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            self.duty_pos = (self.duty_pos + 1) & 7;
        } else {
            self.timer -= 1;
        }
    }

    fn clock_length(&mut self) {
        if !self.length_halt && self.length > 0 {
            self.length -= 1;
        }
    }

    fn sample(&self) -> f32 {
        if !self.enabled { return 0.0; }
        if self.length == 0 { return 0.0; }
        if self.sweep.mute { return 0.0; }
        if self.timer_period < 8 { return 0.0; }
        if DUTY_TABLE[self.duty as usize][self.duty_pos as usize] == 0 { return 0.0; }
        self.envelope.volume() as f32 / 15.0
    }

    fn write_reg0(&mut self, val: u8) {
        self.duty = (val >> 6) & 0x03;
        self.length_halt = val & 0x20 != 0;
        self.envelope.loop_flag = val & 0x20 != 0;
        self.envelope.constant = val & 0x10 != 0;
        self.envelope.period = val & 0x0F;
    }

    fn write_reg1(&mut self, val: u8) {
        self.sweep.enabled = val & 0x80 != 0;
        self.sweep.period = (val >> 4) & 0x07;
        self.sweep.negate = val & 0x08 != 0;
        self.sweep.shift = val & 0x07;
        self.sweep.reload = true;
    }

    fn write_reg2(&mut self, val: u8) {
        self.timer_period = (self.timer_period & 0xFF00) | val as u16;
    }

    fn write_reg3(&mut self, val: u8) {
        self.timer_period = (self.timer_period & 0x00FF) | ((val as u16 & 0x07) << 8);
        if self.enabled {
            self.length = LENGTH_TABLE[(val >> 3) as usize];
        }
        self.duty_pos = 0;
        self.envelope.start = true;
    }
}

// ---------------------------------------------------------------------------
// Triangle channel
// ---------------------------------------------------------------------------
const TRIANGLE_TABLE: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
];

struct TriangleChannel {
    enabled: bool,
    timer_period: u16,
    timer: u16,
    seq_pos: u8,
    length: u8,
    length_halt: bool,
    linear_counter: u8,
    linear_reload: u8,
    linear_reload_flag: bool,
    control_flag: bool,
}

impl TriangleChannel {
    fn new() -> Self {
        TriangleChannel {
            enabled: false,
            timer_period: 0,
            timer: 0,
            seq_pos: 0,
            length: 0,
            length_halt: false,
            linear_counter: 0,
            linear_reload: 0,
            linear_reload_flag: false,
            control_flag: false,
        }
    }

    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            if self.length > 0 && self.linear_counter > 0 {
                self.seq_pos = (self.seq_pos + 1) & 31;
            }
        } else {
            self.timer -= 1;
        }
    }

    fn clock_length(&mut self) {
        if !self.length_halt && self.length > 0 {
            self.length -= 1;
        }
    }

    fn clock_linear(&mut self) {
        if self.linear_reload_flag {
            self.linear_counter = self.linear_reload;
        } else if self.linear_counter > 0 {
            self.linear_counter -= 1;
        }
        if !self.control_flag {
            self.linear_reload_flag = false;
        }
    }

    fn sample(&self) -> f32 {
        if !self.enabled { return 0.0; }
        if self.length == 0 { return 0.0; }
        if self.linear_counter == 0 { return 0.0; }
        TRIANGLE_TABLE[self.seq_pos as usize] as f32 / 15.0
    }
}

// ---------------------------------------------------------------------------
// Noise channel
// ---------------------------------------------------------------------------
struct NoiseChannel {
    enabled: bool,
    mode: bool, // bit 7 of $400E — short mode
    timer_period: u16,
    timer: u16,
    length: u8,
    length_halt: bool,
    envelope: Envelope,
    lfsr: u16,
}

impl NoiseChannel {
    fn new() -> Self {
        NoiseChannel {
            enabled: false,
            mode: false,
            timer_period: NOISE_PERIOD_TABLE[0],
            timer: 0,
            length: 0,
            length_halt: false,
            envelope: Envelope::new(),
            lfsr: 1,
        }
    }

    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            let feedback_bit = if self.mode { 6 } else { 1 };
            let feedback = (self.lfsr & 1) ^ ((self.lfsr >> feedback_bit) & 1);
            self.lfsr >>= 1;
            self.lfsr |= feedback << 14;
        } else {
            self.timer -= 1;
        }
    }

    fn clock_length(&mut self) {
        if !self.length_halt && self.length > 0 {
            self.length -= 1;
        }
    }

    fn sample(&self) -> f32 {
        if !self.enabled { return 0.0; }
        if self.length == 0 { return 0.0; }
        if self.lfsr & 1 != 0 { return 0.0; }
        self.envelope.volume() as f32 / 15.0
    }
}

// ---------------------------------------------------------------------------
// DMC channel (stub – no sample playback)
// ---------------------------------------------------------------------------
struct DmcChannel {
    enabled: bool,
    output: u8,
    // Stub fields
    irq_enabled: bool,
    loop_flag: bool,
    rate_index: u8,
}

impl DmcChannel {
    fn new() -> Self {
        DmcChannel { enabled: false, output: 0, irq_enabled: false, loop_flag: false, rate_index: 0 }
    }

    fn sample(&self) -> f32 {
        if !self.enabled { return 0.0; }
        self.output as f32 / 127.0
    }
}

// ---------------------------------------------------------------------------
// APU
// ---------------------------------------------------------------------------
pub struct NesApu {
    pulse1: PulseChannel,
    pulse2: PulseChannel,
    triangle: TriangleChannel,
    noise: NoiseChannel,
    dmc: DmcChannel,

    // Frame sequencer
    frame_counter_mode: u8, // 0=4-step, 1=5-step
    frame_irq_inhibit: bool,
    frame_irq: bool,
    frame_cycles: u32,
    frame_step: u32,

    // Sampling
    cycle_acc: f64,
    cycles_per_sample: f64,
    buffer: Vec<f32>,
    cpu_cycles: u32,
}

impl NesApu {
    pub fn new() -> Self {
        NesApu {
            pulse1: PulseChannel::new(true),
            pulse2: PulseChannel::new(false),
            triangle: TriangleChannel::new(),
            noise: NoiseChannel::new(),
            dmc: DmcChannel::new(),
            frame_counter_mode: 0,
            frame_irq_inhibit: false,
            frame_irq: false,
            frame_cycles: 0,
            frame_step: 0,
            cycle_acc: 0.0,
            cycles_per_sample: CPU_CLOCK / SAMPLE_RATE as f64,
            buffer: Vec::new(),
            cpu_cycles: 0,
        }
    }

    pub fn write_register(&mut self, addr: u16, val: u8) {
        match addr {
            0x4000 => self.pulse1.write_reg0(val),
            0x4001 => self.pulse1.write_reg1(val),
            0x4002 => self.pulse1.write_reg2(val),
            0x4003 => self.pulse1.write_reg3(val),
            0x4004 => self.pulse2.write_reg0(val),
            0x4005 => self.pulse2.write_reg1(val),
            0x4006 => self.pulse2.write_reg2(val),
            0x4007 => self.pulse2.write_reg3(val),
            0x4008 => {
                self.triangle.control_flag = val & 0x80 != 0;
                self.triangle.length_halt = val & 0x80 != 0;
                self.triangle.linear_reload = val & 0x7F;
            }
            0x400A => {
                self.triangle.timer_period = (self.triangle.timer_period & 0xFF00) | val as u16;
            }
            0x400B => {
                self.triangle.timer_period = (self.triangle.timer_period & 0x00FF) | ((val as u16 & 0x07) << 8);
                if self.triangle.enabled {
                    self.triangle.length = LENGTH_TABLE[(val >> 3) as usize];
                }
                self.triangle.linear_reload_flag = true;
            }
            0x400C => {
                self.noise.length_halt = val & 0x20 != 0;
                self.noise.envelope.loop_flag = val & 0x20 != 0;
                self.noise.envelope.constant = val & 0x10 != 0;
                self.noise.envelope.period = val & 0x0F;
            }
            0x400E => {
                self.noise.mode = val & 0x80 != 0;
                let idx = (val & 0x0F) as usize;
                self.noise.timer_period = NOISE_PERIOD_TABLE[idx];
            }
            0x400F => {
                if self.noise.enabled {
                    self.noise.length = LENGTH_TABLE[(val >> 3) as usize];
                }
                self.noise.envelope.start = true;
            }
            0x4010 => {
                self.dmc.irq_enabled = val & 0x80 != 0;
                self.dmc.loop_flag = val & 0x40 != 0;
                self.dmc.rate_index = val & 0x0F;
            }
            0x4011 => {
                self.dmc.output = val & 0x7F;
            }
            0x4015 => {
                self.pulse1.enabled = val & 0x01 != 0;
                self.pulse2.enabled = val & 0x02 != 0;
                self.triangle.enabled = val & 0x04 != 0;
                self.noise.enabled = val & 0x08 != 0;
                self.dmc.enabled = val & 0x10 != 0;
                if !self.pulse1.enabled { self.pulse1.length = 0; }
                if !self.pulse2.enabled { self.pulse2.length = 0; }
                if !self.triangle.enabled { self.triangle.length = 0; }
                if !self.noise.enabled { self.noise.length = 0; }
                if !self.dmc.enabled { self.dmc.output = 0; }
            }
            0x4017 => {
                self.frame_counter_mode = (val >> 7) & 1;
                self.frame_irq_inhibit = val & 0x40 != 0;
                if self.frame_irq_inhibit {
                    self.frame_irq = false;
                }
                self.frame_cycles = 0;
                self.frame_step = 0;
                if self.frame_counter_mode == 1 {
                    // Clock all units immediately for 5-step mode
                    self.clock_quarter_frame();
                    self.clock_half_frame();
                }
            }
            _ => {}
        }
    }

    pub fn read_status(&self) -> u8 {
        let mut v = 0u8;
        if self.pulse1.length > 0 { v |= 0x01; }
        if self.pulse2.length > 0 { v |= 0x02; }
        if self.triangle.length > 0 { v |= 0x04; }
        if self.noise.length > 0 { v |= 0x08; }
        if self.frame_irq { v |= 0x40; }
        v
    }

    pub fn step(&mut self, cycles: u32) {
        for _ in 0..cycles {
            self.step_one_cpu_cycle();
        }
    }

    fn step_one_cpu_cycle(&mut self) {
        self.cpu_cycles += 1;

        // Triangle timer clocks every CPU cycle
        self.triangle.clock_timer();

        // Pulse and noise timers clock every other CPU cycle
        if self.cpu_cycles & 1 == 0 {
            self.pulse1.clock_timer();
            self.pulse2.clock_timer();
            self.noise.clock_timer();
        }

        // Frame sequencer
        self.frame_cycles += 1;
        self.tick_frame_sequencer();

        // Output sample
        self.cycle_acc += 1.0;
        if self.cycle_acc >= self.cycles_per_sample {
            self.cycle_acc -= self.cycles_per_sample;
            self.buffer.push(self.mix_sample());
        }
    }

    fn tick_frame_sequencer(&mut self) {
        // NTSC frame sequencer timings in CPU cycles
        // 4-step: 7457, 14913, 22371, 29829, 29830
        // 5-step: 7457, 14913, 22371, 29829, 37281, 37282
        let (steps_4, steps_5): (&[u32], &[u32]) = (
            &[7457, 14913, 22371, 29829, 29830],
            &[7457, 14913, 22371, 29829, 37281, 37282],
        );

        let steps = if self.frame_counter_mode == 0 { steps_4 } else { steps_5 };

        if self.frame_step < steps.len() as u32 && self.frame_cycles >= steps[self.frame_step as usize] {
            let step = self.frame_step;
            self.frame_step += 1;

            if self.frame_counter_mode == 0 {
                // 4-step mode
                match step {
                    0 => self.clock_quarter_frame(),
                    1 => { self.clock_quarter_frame(); self.clock_half_frame(); }
                    2 => self.clock_quarter_frame(),
                    3 => {}
                    4 => {
                        self.clock_quarter_frame();
                        self.clock_half_frame();
                        if !self.frame_irq_inhibit {
                            self.frame_irq = true;
                        }
                        // Reset for next frame
                        self.frame_cycles = 0;
                        self.frame_step = 0;
                    }
                    _ => {}
                }
            } else {
                // 5-step mode
                match step {
                    0 => self.clock_quarter_frame(),
                    1 => { self.clock_quarter_frame(); self.clock_half_frame(); }
                    2 => self.clock_quarter_frame(),
                    3 => {}
                    4 => { self.clock_quarter_frame(); self.clock_half_frame(); }
                    5 => {
                        // Reset
                        self.frame_cycles = 0;
                        self.frame_step = 0;
                    }
                    _ => {}
                }
            }
        }
    }

    fn clock_quarter_frame(&mut self) {
        self.pulse1.envelope.clock();
        self.pulse2.envelope.clock();
        self.noise.envelope.clock();
        self.triangle.clock_linear();
    }

    fn clock_half_frame(&mut self) {
        self.pulse1.clock_length();
        self.pulse2.clock_length();
        self.triangle.clock_length();
        self.noise.clock_length();

        let p1_period = self.pulse1.timer_period;
        self.pulse1.sweep.clock(&mut self.pulse1.timer_period, true);
        let _ = p1_period; // silence unused warning

        let p2_period = self.pulse2.timer_period;
        self.pulse2.sweep.clock(&mut self.pulse2.timer_period, false);
        let _ = p2_period;
    }

    fn mix_sample(&self) -> f32 {
        let p1 = self.pulse1.sample();
        let p2 = self.pulse2.sample();
        let tri = self.triangle.sample();
        let noise = self.noise.sample();
        let dmc = self.dmc.sample();

        // NES lookup table approximations
        let pulse_out = if p1 + p2 == 0.0 {
            0.0
        } else {
            95.88 / ((8128.0 / (p1 * 15.0 + p2 * 15.0)) + 100.0)
        };

        let tnd_sum = tri / 8227.0 + noise / 12241.0 + dmc / 22638.0;
        let tnd_out = if tnd_sum == 0.0 {
            0.0
        } else {
            159.79 / (1.0 / tnd_sum + 100.0)
        };

        // Scale to [-1, 1] with 0 = silence.
        // Max combined output is ~0.95, so multiply by ~1.05 to reach full range.
        ((pulse_out + tnd_out) * 1.05).clamp(-1.0, 1.0)
    }

    pub fn get_samples(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.buffer)
    }

    // -----------------------------------------------------------------------
    // Save / Load state
    // -----------------------------------------------------------------------
    pub fn save(&self, buf: &mut Vec<u8>) {
        // Pulse 1
        write_bool(buf, self.pulse1.enabled);
        write_u8(buf, self.pulse1.duty);
        write_u8(buf, self.pulse1.duty_pos);
        write_u16(buf, self.pulse1.timer_period);
        write_u16(buf, self.pulse1.timer);
        write_u8(buf, self.pulse1.length);
        write_bool(buf, self.pulse1.length_halt);
        // Pulse 1 envelope
        write_bool(buf, self.pulse1.envelope.start);
        write_bool(buf, self.pulse1.envelope.loop_flag);
        write_bool(buf, self.pulse1.envelope.constant);
        write_u8(buf, self.pulse1.envelope.period);
        write_u8(buf, self.pulse1.envelope.divider);
        write_u8(buf, self.pulse1.envelope.decay);
        // Pulse 1 sweep
        write_bool(buf, self.pulse1.sweep.enabled);
        write_u8(buf, self.pulse1.sweep.period);
        write_bool(buf, self.pulse1.sweep.negate);
        write_u8(buf, self.pulse1.sweep.shift);
        write_bool(buf, self.pulse1.sweep.reload);
        write_u8(buf, self.pulse1.sweep.divider);
        write_bool(buf, self.pulse1.sweep.mute);

        // Pulse 2
        write_bool(buf, self.pulse2.enabled);
        write_u8(buf, self.pulse2.duty);
        write_u8(buf, self.pulse2.duty_pos);
        write_u16(buf, self.pulse2.timer_period);
        write_u16(buf, self.pulse2.timer);
        write_u8(buf, self.pulse2.length);
        write_bool(buf, self.pulse2.length_halt);
        // Pulse 2 envelope
        write_bool(buf, self.pulse2.envelope.start);
        write_bool(buf, self.pulse2.envelope.loop_flag);
        write_bool(buf, self.pulse2.envelope.constant);
        write_u8(buf, self.pulse2.envelope.period);
        write_u8(buf, self.pulse2.envelope.divider);
        write_u8(buf, self.pulse2.envelope.decay);
        // Pulse 2 sweep
        write_bool(buf, self.pulse2.sweep.enabled);
        write_u8(buf, self.pulse2.sweep.period);
        write_bool(buf, self.pulse2.sweep.negate);
        write_u8(buf, self.pulse2.sweep.shift);
        write_bool(buf, self.pulse2.sweep.reload);
        write_u8(buf, self.pulse2.sweep.divider);
        write_bool(buf, self.pulse2.sweep.mute);

        // Triangle
        write_bool(buf, self.triangle.enabled);
        write_u16(buf, self.triangle.timer_period);
        write_u16(buf, self.triangle.timer);
        write_u8(buf, self.triangle.seq_pos);
        write_u8(buf, self.triangle.length);
        write_bool(buf, self.triangle.length_halt);
        write_u8(buf, self.triangle.linear_counter);
        write_u8(buf, self.triangle.linear_reload);
        write_bool(buf, self.triangle.linear_reload_flag);
        write_bool(buf, self.triangle.control_flag);

        // Noise
        write_bool(buf, self.noise.enabled);
        write_bool(buf, self.noise.mode);
        write_u16(buf, self.noise.timer_period);
        write_u16(buf, self.noise.timer);
        write_u8(buf, self.noise.length);
        write_bool(buf, self.noise.length_halt);
        write_u16(buf, self.noise.lfsr);
        // Noise envelope
        write_bool(buf, self.noise.envelope.start);
        write_bool(buf, self.noise.envelope.loop_flag);
        write_bool(buf, self.noise.envelope.constant);
        write_u8(buf, self.noise.envelope.period);
        write_u8(buf, self.noise.envelope.divider);
        write_u8(buf, self.noise.envelope.decay);

        // DMC
        write_bool(buf, self.dmc.enabled);
        write_u8(buf, self.dmc.output);
        write_bool(buf, self.dmc.irq_enabled);
        write_bool(buf, self.dmc.loop_flag);
        write_u8(buf, self.dmc.rate_index);

        // Frame sequencer
        write_u8(buf, self.frame_counter_mode);
        write_bool(buf, self.frame_irq_inhibit);
        write_bool(buf, self.frame_irq);
        write_u32(buf, self.frame_cycles);
        write_u32(buf, self.frame_step);

        // Sampling accumulators (store as u64 bits of f64)
        write_u64(buf, self.cycle_acc.to_bits());
        write_u32(buf, self.cpu_cycles);
    }

    pub fn load(&mut self, data: &[u8], off: &mut usize) {
        // Pulse 1
        self.pulse1.enabled      = read_bool(data, off);
        self.pulse1.duty         = read_u8(data, off);
        self.pulse1.duty_pos     = read_u8(data, off);
        self.pulse1.timer_period = read_u16(data, off);
        self.pulse1.timer        = read_u16(data, off);
        self.pulse1.length       = read_u8(data, off);
        self.pulse1.length_halt  = read_bool(data, off);
        self.pulse1.envelope.start      = read_bool(data, off);
        self.pulse1.envelope.loop_flag  = read_bool(data, off);
        self.pulse1.envelope.constant   = read_bool(data, off);
        self.pulse1.envelope.period     = read_u8(data, off);
        self.pulse1.envelope.divider    = read_u8(data, off);
        self.pulse1.envelope.decay      = read_u8(data, off);
        self.pulse1.sweep.enabled  = read_bool(data, off);
        self.pulse1.sweep.period   = read_u8(data, off);
        self.pulse1.sweep.negate   = read_bool(data, off);
        self.pulse1.sweep.shift    = read_u8(data, off);
        self.pulse1.sweep.reload   = read_bool(data, off);
        self.pulse1.sweep.divider  = read_u8(data, off);
        self.pulse1.sweep.mute     = read_bool(data, off);

        // Pulse 2
        self.pulse2.enabled      = read_bool(data, off);
        self.pulse2.duty         = read_u8(data, off);
        self.pulse2.duty_pos     = read_u8(data, off);
        self.pulse2.timer_period = read_u16(data, off);
        self.pulse2.timer        = read_u16(data, off);
        self.pulse2.length       = read_u8(data, off);
        self.pulse2.length_halt  = read_bool(data, off);
        self.pulse2.envelope.start      = read_bool(data, off);
        self.pulse2.envelope.loop_flag  = read_bool(data, off);
        self.pulse2.envelope.constant   = read_bool(data, off);
        self.pulse2.envelope.period     = read_u8(data, off);
        self.pulse2.envelope.divider    = read_u8(data, off);
        self.pulse2.envelope.decay      = read_u8(data, off);
        self.pulse2.sweep.enabled  = read_bool(data, off);
        self.pulse2.sweep.period   = read_u8(data, off);
        self.pulse2.sweep.negate   = read_bool(data, off);
        self.pulse2.sweep.shift    = read_u8(data, off);
        self.pulse2.sweep.reload   = read_bool(data, off);
        self.pulse2.sweep.divider  = read_u8(data, off);
        self.pulse2.sweep.mute     = read_bool(data, off);

        // Triangle
        self.triangle.enabled           = read_bool(data, off);
        self.triangle.timer_period      = read_u16(data, off);
        self.triangle.timer             = read_u16(data, off);
        self.triangle.seq_pos           = read_u8(data, off);
        self.triangle.length            = read_u8(data, off);
        self.triangle.length_halt       = read_bool(data, off);
        self.triangle.linear_counter    = read_u8(data, off);
        self.triangle.linear_reload     = read_u8(data, off);
        self.triangle.linear_reload_flag = read_bool(data, off);
        self.triangle.control_flag      = read_bool(data, off);

        // Noise
        self.noise.enabled      = read_bool(data, off);
        self.noise.mode         = read_bool(data, off);
        self.noise.timer_period = read_u16(data, off);
        self.noise.timer        = read_u16(data, off);
        self.noise.length       = read_u8(data, off);
        self.noise.length_halt  = read_bool(data, off);
        self.noise.lfsr         = read_u16(data, off);
        self.noise.envelope.start      = read_bool(data, off);
        self.noise.envelope.loop_flag  = read_bool(data, off);
        self.noise.envelope.constant   = read_bool(data, off);
        self.noise.envelope.period     = read_u8(data, off);
        self.noise.envelope.divider    = read_u8(data, off);
        self.noise.envelope.decay      = read_u8(data, off);

        // DMC
        self.dmc.enabled     = read_bool(data, off);
        self.dmc.output      = read_u8(data, off);
        self.dmc.irq_enabled = read_bool(data, off);
        self.dmc.loop_flag   = read_bool(data, off);
        self.dmc.rate_index  = read_u8(data, off);

        // Frame sequencer
        self.frame_counter_mode = read_u8(data, off);
        self.frame_irq_inhibit  = read_bool(data, off);
        self.frame_irq          = read_bool(data, off);
        self.frame_cycles       = read_u32(data, off);
        self.frame_step         = read_u32(data, off);

        // Sampling accumulators
        self.cycle_acc  = f64::from_bits(read_u64(data, off));
        self.cpu_cycles = read_u32(data, off);
        // Clear audio buffer on state load
        self.buffer.clear();
    }
}
