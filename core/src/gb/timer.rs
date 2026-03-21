/// Timer registers and logic for the Game Boy DMG.
///
/// DIV  0xFF04 – Divider register (upper 8 bits of internal 16-bit counter)
/// TIMA 0xFF05 – Timer counter
/// TMA  0xFF06 – Timer modulo (reload value)
/// TAC  0xFF07 – Timer control
///   bit 2 – Timer stop (0 = stop, 1 = run)
///   bits 1-0 – Input clock select
///     00 = CPU clock / 1024  (4096   Hz)
///     01 = CPU clock / 16    (262144 Hz)
///     10 = CPU clock / 64    (65536  Hz)
///     11 = CPU clock / 256   (16384  Hz)

pub struct Timer {
    /// Internal 16-bit counter; DIV is its upper byte.
    internal_counter: u16,
    /// TIMA – Timer counter (0xFF05)
    pub tima: u8,
    /// TMA  – Timer modulo  (0xFF06)
    pub tma: u8,
    /// TAC  – Timer control (0xFF07)
    pub tac: u8,
    /// Accumulator for fractional cycles toward the next TIMA tick.
    tima_cycles: u32,
}

impl Timer {
    pub fn new() -> Self {
        Timer {
            internal_counter: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            tima_cycles: 0,
        }
    }

    /// Read the DIV register (upper byte of the internal counter).
    pub fn read_div(&self) -> u8 {
        (self.internal_counter >> 8) as u8
    }

    /// Writing any value to DIV resets the internal counter to 0.
    pub fn write_div(&mut self) {
        self.internal_counter = 0;
        self.tima_cycles = 0;
    }

    /// Returns the clock period (in CPU cycles) for the current TAC setting.
    fn clock_period(&self) -> u32 {
        match self.tac & 0x03 {
            0 => 1024,
            1 => 16,
            2 => 64,
            3 => 256,
            _ => unreachable!(),
        }
    }

    /// Advance the timer by `cycles` CPU clock ticks.
    ///
    /// Returns `true` if a timer overflow interrupt should be requested.
    pub fn step(&mut self, cycles: u32) -> bool {
        // Always advance the internal divider counter.
        self.internal_counter = self.internal_counter.wrapping_add(cycles as u16);

        let timer_running = (self.tac & 0x04) != 0;
        if !timer_running {
            return false;
        }

        let period = self.clock_period();
        self.tima_cycles += cycles;

        let mut interrupt = false;
        while self.tima_cycles >= period {
            self.tima_cycles -= period;
            let (new_tima, overflow) = self.tima.overflowing_add(1);
            if overflow {
                // Reload TIMA from TMA and request interrupt.
                self.tima = self.tma;
                interrupt = true;
            } else {
                self.tima = new_tima;
            }
        }

        interrupt
    }
}
