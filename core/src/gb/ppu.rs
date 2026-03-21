/// Pixel Processing Unit for the Game Boy DMG.

use crate::save_state::*;
///
/// Modes and timing per scanline (456 cycles total):
///   OAM Scan  (mode 2):  80 cycles
///   Drawing   (mode 3): ~172 cycles (variable; we use 172)
///   HBlank    (mode 0): 204 cycles
///   VBlank    (mode 1): lines 144-153, 456 cycles each

pub const SCREEN_W: usize = 160;
pub const SCREEN_H: usize = 144;

// Classic Game Boy green palette (darkest to lightest)
const PALETTE: [[u8; 4]; 4] = [
    [0x9B, 0xBC, 0x0F, 0xFF], // shade 0 – lightest
    [0x8B, 0xAC, 0x0F, 0xFF], // shade 1
    [0x30, 0x62, 0x30, 0xFF], // shade 2
    [0x0F, 0x38, 0x0F, 0xFF], // shade 3 – darkest
];

#[derive(Clone, Copy, PartialEq)]
pub enum PpuMode {
    HBlank  = 0,
    VBlank  = 1,
    OamScan = 2,
    Drawing = 3,
}

pub struct Ppu {
    pub vram: [u8; 0x2000],     // 0x8000 – 0x9FFF
    pub oam:  [u8; 0xA0],       // 0xFE00 – 0xFE9F

    // LCD control / status registers
    pub lcdc: u8, // 0xFF40
    pub stat: u8, // 0xFF41
    pub scy:  u8, // 0xFF42
    pub scx:  u8, // 0xFF43
    pub ly:   u8, // 0xFF44
    pub lyc:  u8, // 0xFF45
    pub bgp:  u8, // 0xFF47
    pub obp0: u8, // 0xFF48
    pub obp1: u8, // 0xFF49
    pub wy:   u8, // 0xFF4A
    pub wx:   u8, // 0xFF4B

    pub mode: PpuMode,
    cycle_counter: u32,

    pub frame_buffer: Vec<u8>, // RGBA, 160*144*4 bytes
    pub frame_ready: bool,

    // Interrupt request flags (cleared by MMU/CPU after reading)
    pub vblank_irq: bool,
    pub stat_irq:   bool,

    window_line_counter: u8,
}

// ---------------------------------------------------------------------------
// LCDC helper bits
// ---------------------------------------------------------------------------
fn lcdc_display_enable(lcdc: u8) -> bool  { lcdc & 0x80 != 0 }
fn lcdc_window_tile_map(lcdc: u8) -> u16  { if lcdc & 0x40 != 0 { 0x9C00 } else { 0x9800 } }
fn lcdc_window_enable(lcdc: u8) -> bool   { lcdc & 0x20 != 0 }
fn lcdc_tile_data(lcdc: u8) -> u16        { if lcdc & 0x10 != 0 { 0x8000 } else { 0x8800 } }
fn lcdc_bg_tile_map(lcdc: u8) -> u16      { if lcdc & 0x08 != 0 { 0x9C00 } else { 0x9800 } }
fn lcdc_obj_size(lcdc: u8) -> u8          { if lcdc & 0x04 != 0 { 16 } else { 8 } }
fn lcdc_obj_enable(lcdc: u8) -> bool      { lcdc & 0x02 != 0 }
fn lcdc_bg_enable(lcdc: u8) -> bool       { lcdc & 0x01 != 0 }

impl Ppu {
    pub fn new() -> Self {
        Ppu {
            vram: [0; 0x2000],
            oam:  [0; 0xA0],
            lcdc: 0x91,
            stat: 0x00,
            scy:  0,
            scx:  0,
            ly:   0,
            lyc:  0,
            bgp:  0xFC,
            obp0: 0xFF,
            obp1: 0xFF,
            wy:   0,
            wx:   0,
            mode: PpuMode::OamScan,
            cycle_counter: 0,
            frame_buffer: vec![0xFF; SCREEN_W * SCREEN_H * 4],
            frame_ready: false,
            vblank_irq: false,
            stat_irq:   false,
            window_line_counter: 0,
        }
    }

    pub fn read_vram(&self, addr: u16) -> u8 {
        // During Drawing mode VRAM is inaccessible; return 0xFF
        if self.mode == PpuMode::Drawing {
            return 0xFF;
        }
        self.vram[(addr - 0x8000) as usize]
    }

    pub fn write_vram(&mut self, addr: u16, val: u8) {
        if self.mode == PpuMode::Drawing {
            return;
        }
        self.vram[(addr - 0x8000) as usize] = val;
    }

    pub fn read_oam(&self, addr: u16) -> u8 {
        if self.mode == PpuMode::OamScan || self.mode == PpuMode::Drawing {
            return 0xFF;
        }
        self.oam[(addr - 0xFE00) as usize]
    }

    pub fn write_oam(&mut self, addr: u16, val: u8) {
        if self.mode == PpuMode::OamScan || self.mode == PpuMode::Drawing {
            return;
        }
        self.oam[(addr - 0xFE00) as usize] = val;
    }

    /// Force-write to OAM (used by DMA transfer).
    pub fn dma_write_oam(&mut self, index: u8, val: u8) {
        self.oam[index as usize] = val;
    }

    /// Advance PPU state by `cycles` CPU clock ticks.
    ///
    /// Returns true when a new frame is complete.
    pub fn step(&mut self, cycles: u32) -> bool {
        if !lcdc_display_enable(self.lcdc) {
            return false;
        }
        self.frame_ready = false;

        self.cycle_counter += cycles;

        match self.mode {
            PpuMode::OamScan => {
                if self.cycle_counter >= 80 {
                    self.cycle_counter -= 80;
                    self.set_mode(PpuMode::Drawing);
                }
            }
            PpuMode::Drawing => {
                if self.cycle_counter >= 172 {
                    self.cycle_counter -= 172;
                    self.render_scanline();
                    self.set_mode(PpuMode::HBlank);
                }
            }
            PpuMode::HBlank => {
                if self.cycle_counter >= 204 {
                    self.cycle_counter -= 204;
                    self.ly += 1;
                    self.check_lyc();

                    if self.ly == 144 {
                        self.set_mode(PpuMode::VBlank);
                        self.vblank_irq = true;
                        self.frame_ready = true;
                        self.window_line_counter = 0;
                    } else {
                        self.set_mode(PpuMode::OamScan);
                    }
                }
            }
            PpuMode::VBlank => {
                if self.cycle_counter >= 456 {
                    self.cycle_counter -= 456;
                    self.ly += 1;
                    self.check_lyc();

                    if self.ly > 153 {
                        self.ly = 0;
                        self.check_lyc();
                        self.set_mode(PpuMode::OamScan);
                        self.window_line_counter = 0;
                    }
                }
            }
        }

        self.frame_ready
    }

    fn set_mode(&mut self, mode: PpuMode) {
        self.mode = mode;
        // Update lower 2 bits of STAT
        self.stat = (self.stat & 0xFC) | (mode as u8);

        // STAT interrupt sources
        let irq = match mode {
            PpuMode::HBlank  => self.stat & 0x08 != 0,
            PpuMode::VBlank  => self.stat & 0x10 != 0,
            PpuMode::OamScan => self.stat & 0x20 != 0,
            PpuMode::Drawing => false,
        };
        if irq {
            self.stat_irq = true;
        }
    }

    fn check_lyc(&mut self) {
        if self.ly == self.lyc {
            self.stat |= 0x04;
            if self.stat & 0x40 != 0 {
                self.stat_irq = true;
            }
        } else {
            self.stat &= !0x04;
        }
    }

    // -----------------------------------------------------------------------
    // Scanline rendering
    // -----------------------------------------------------------------------

    fn render_scanline(&mut self) {
        let ly = self.ly as usize;
        if ly >= SCREEN_H { return; }

        let mut bg_priority = [false; SCREEN_W];
        let mut bg_pixel_ids = [0u8; SCREEN_W]; // raw palette IDs for BG/window

        // --- Background ---
        if lcdc_bg_enable(self.lcdc) {
            self.render_bg_line(ly, &mut bg_pixel_ids, &mut bg_priority);
        }

        // --- Window ---
        if lcdc_window_enable(self.lcdc)
            && (self.wx as i16 - 7) < SCREEN_W as i16
            && self.wy <= self.ly
        {
            self.render_window_line(ly, &mut bg_pixel_ids, &mut bg_priority);
        }

        // --- Sprites ---
        if lcdc_obj_enable(self.lcdc) {
            self.render_sprites_line(ly, &bg_pixel_ids, &bg_priority);
        } else {
            // Flush background to frame buffer
            for x in 0..SCREEN_W {
                let color = self.apply_palette(bg_pixel_ids[x], self.bgp);
                let base = (ly * SCREEN_W + x) * 4;
                self.frame_buffer[base..base + 4].copy_from_slice(&PALETTE[color as usize]);
            }
        }
    }

    fn tile_pixel(&self, tile_data_base: u16, tile_id: u8, tx: u8, ty: u8) -> u8 {
        let tile_addr = if tile_data_base == 0x8000 {
            tile_data_base + tile_id as u16 * 16
        } else {
            // 0x8800 signed addressing: base 0x9000, tile_id is signed
            let signed = tile_id as i8 as i32;
            (0x9000i32 + signed * 16) as u16
        };
        let row_addr = tile_addr + ty as u16 * 2;
        let lo = self.vram[(row_addr - 0x8000) as usize];
        let hi = self.vram[(row_addr - 0x8000 + 1) as usize];
        let bit = 7 - (tx & 7);
        (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1)
    }

    fn render_bg_line(&mut self, ly: usize, ids: &mut [u8; SCREEN_W], _priority: &mut [bool; SCREEN_W]) {
        let tile_data = lcdc_tile_data(self.lcdc);
        let tile_map  = lcdc_bg_tile_map(self.lcdc);

        let py = (ly as u8).wrapping_add(self.scy) as usize;
        let tile_row = py / 8;

        for x in 0..SCREEN_W {
            let px = (x as u8).wrapping_add(self.scx) as usize;
            let tile_col = px / 8;

            let map_addr = tile_map + (tile_row * 32 + tile_col) as u16;
            let tile_id = self.vram[(map_addr - 0x8000) as usize];

            let pixel = self.tile_pixel(tile_data, tile_id, (px % 8) as u8, (py % 8) as u8);
            ids[x] = pixel;
        }
    }

    fn render_window_line(&mut self, ly: usize, ids: &mut [u8; SCREEN_W], _priority: &mut [bool; SCREEN_W]) {
        let tile_data = lcdc_tile_data(self.lcdc);
        let tile_map  = lcdc_window_tile_map(self.lcdc);

        let wx_offset = (self.wx as i16) - 7;
        let wy = self.wy as usize;
        if ly < wy { return; }

        let win_y = self.window_line_counter as usize;
        let tile_row = win_y / 8;

        let mut rendered = false;
        for x in 0..SCREEN_W {
            let win_x = x as i16 - wx_offset;
            if win_x < 0 { continue; }
            let tile_col = win_x as usize / 8;

            let map_addr = tile_map + (tile_row * 32 + tile_col) as u16;
            let tile_id = self.vram[(map_addr - 0x8000) as usize];

            let pixel = self.tile_pixel(tile_data, tile_id, (win_x % 8) as u8, (win_y % 8) as u8);
            ids[x] = pixel;
            rendered = true;
        }
        if rendered {
            self.window_line_counter = self.window_line_counter.wrapping_add(1);
        }
    }

    fn render_sprites_line(&mut self, ly: usize, bg_ids: &[u8; SCREEN_W], _bg_priority: &[bool; SCREEN_W]) {
        let sprite_height = lcdc_obj_size(self.lcdc) as usize;
        let mut sprites: Vec<(i16, i16, u8, u8)> = Vec::new(); // (x, y, tile, attrs)

        for i in 0..40usize {
            let base = i * 4;
            let sy = self.oam[base] as i16 - 16;
            let sx = self.oam[base + 1] as i16 - 8;
            let tile = self.oam[base + 2];
            let attr = self.oam[base + 3];

            if ly as i16 >= sy && (ly as i16) < sy + sprite_height as i16 {
                sprites.push((sx, sy, tile, attr));
                if sprites.len() == 10 { break; }
            }
        }

        // Render BG into buffer first
        let mut line_buf = [[0u8; 4]; SCREEN_W];
        let mut line_is_obj = [false; SCREEN_W];

        for x in 0..SCREEN_W {
            let color = self.apply_palette(bg_ids[x], self.bgp);
            line_buf[x] = PALETTE[color as usize];
        }

        // Draw sprites (lower index = higher priority on ties)
        for (sx, sy, tile, attr) in sprites.iter().rev() {
            let flip_y = attr & 0x40 != 0;
            let flip_x = attr & 0x20 != 0;
            let palette = if attr & 0x10 != 0 { self.obp1 } else { self.obp0 };
            let bg_over_obj = attr & 0x80 != 0;

            let mut row_in_sprite = ly as i16 - sy;
            if flip_y {
                row_in_sprite = sprite_height as i16 - 1 - row_in_sprite;
            }

            let tile_id = if sprite_height == 16 {
                if row_in_sprite < 8 { tile & 0xFE } else { tile | 0x01 }
            } else {
                *tile
            };
            let row_in_tile = (row_in_sprite % 8) as u8;

            for col in 0..8i16 {
                let screen_x = sx + col;
                if screen_x < 0 || screen_x >= SCREEN_W as i16 { continue; }
                let tx = if flip_x { 7 - col } else { col } as u8;
                let pixel = self.tile_pixel(0x8000, tile_id, tx, row_in_tile);
                if pixel == 0 { continue; } // transparent

                let sx_usize = screen_x as usize;
                if line_is_obj[sx_usize] { continue; } // already drawn by higher-priority sprite

                // BG-over-OBJ priority: if BG pixel is non-zero, sprite is behind BG
                if bg_over_obj && bg_ids[sx_usize] != 0 { continue; }

                let color = self.apply_palette(pixel, palette);
                line_buf[sx_usize] = PALETTE[color as usize];
                line_is_obj[sx_usize] = true;
            }
        }

        // Write line buffer to frame buffer
        for x in 0..SCREEN_W {
            let base = (ly * SCREEN_W + x) * 4;
            self.frame_buffer[base..base + 4].copy_from_slice(&line_buf[x]);
        }
    }

    /// Map a 2-bit pixel ID through a palette register into a shade index.
    fn apply_palette(&self, pixel_id: u8, palette: u8) -> u8 {
        (palette >> (pixel_id * 2)) & 0x03
    }

    // -----------------------------------------------------------------------
    // Save / Load state
    // -----------------------------------------------------------------------
    pub fn save(&self, buf: &mut Vec<u8>) {
        // Registers
        write_u8(buf, self.lcdc);
        write_u8(buf, self.stat);
        write_u8(buf, self.scy);
        write_u8(buf, self.scx);
        write_u8(buf, self.ly);
        write_u8(buf, self.lyc);
        write_u8(buf, self.bgp);
        write_u8(buf, self.obp0);
        write_u8(buf, self.obp1);
        write_u8(buf, self.wy);
        write_u8(buf, self.wx);
        // Mode (store as u8)
        write_u8(buf, self.mode as u8);
        write_u32(buf, self.cycle_counter);
        // IRQ flags
        write_bool(buf, self.vblank_irq);
        write_bool(buf, self.stat_irq);
        write_bool(buf, self.frame_ready);
        write_u8(buf, self.window_line_counter);
        // VRAM (8192 bytes), OAM (160 bytes), frame_buffer (160*144*4 bytes)
        write_slice(buf, &self.vram);
        write_slice(buf, &self.oam);
        write_slice(buf, &self.frame_buffer);
    }

    pub fn load(&mut self, data: &[u8], off: &mut usize) {
        self.lcdc = read_u8(data, off);
        self.stat = read_u8(data, off);
        self.scy  = read_u8(data, off);
        self.scx  = read_u8(data, off);
        self.ly   = read_u8(data, off);
        self.lyc  = read_u8(data, off);
        self.bgp  = read_u8(data, off);
        self.obp0 = read_u8(data, off);
        self.obp1 = read_u8(data, off);
        self.wy   = read_u8(data, off);
        self.wx   = read_u8(data, off);
        self.mode = match read_u8(data, off) {
            0 => PpuMode::HBlank,
            1 => PpuMode::VBlank,
            2 => PpuMode::OamScan,
            _ => PpuMode::Drawing,
        };
        self.cycle_counter = read_u32(data, off);
        self.vblank_irq = read_bool(data, off);
        self.stat_irq   = read_bool(data, off);
        self.frame_ready = read_bool(data, off);
        self.window_line_counter = read_u8(data, off);
        self.vram.copy_from_slice(read_slice(data, off, 0x2000));
        self.oam.copy_from_slice(read_slice(data, off, 0xA0));
        let fb_len = SCREEN_W * SCREEN_H * 4;
        self.frame_buffer.copy_from_slice(read_slice(data, off, fb_len));
    }
}
