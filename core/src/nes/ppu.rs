/// NES PPU – Picture Processing Unit.
///
/// Renders 256×240 pixels at 60Hz.
/// PPU runs at 3× CPU clock → 341 cycles/scanline × 262 scanlines = 89342 PPU cycles per frame.

use super::cartridge::{Mapper, Mirroring};
use crate::save_state::*;

pub const SCREEN_W: usize = 256;
pub const SCREEN_H: usize = 240;

// ---------------------------------------------------------------------------
// NTSC NES palette – 64 colors as RGB
// ---------------------------------------------------------------------------
#[rustfmt::skip]
const NES_PALETTE: [[u8; 3]; 64] = [
    [84,84,84],   [0,30,116],    [8,16,144],    [48,0,136],
    [68,0,100],   [92,0,48],     [84,4,0],      [60,24,0],
    [32,42,0],    [8,58,0],      [0,64,0],      [0,60,0],
    [0,50,60],    [0,0,0],       [0,0,0],       [0,0,0],

    [152,150,152],[8,76,196],    [48,50,236],   [92,30,228],
    [136,20,176], [160,20,100],  [152,34,32],   [120,60,0],
    [84,90,0],    [40,114,0],    [8,124,0],     [0,118,40],
    [0,102,120],  [0,0,0],       [0,0,0],       [0,0,0],

    [236,238,236],[76,154,236],  [120,124,236], [176,98,236],
    [228,84,236], [236,88,180],  [236,106,100], [212,136,32],
    [160,170,0],  [116,196,0],   [76,208,32],   [56,204,108],
    [56,180,204], [60,60,60],    [0,0,0],       [0,0,0],

    [236,238,236],[168,204,236], [188,188,236], [212,178,236],
    [236,174,236],[236,174,212], [236,180,176], [228,196,144],
    [204,210,120],[180,222,120], [168,226,144], [152,226,180],
    [160,214,228],[160,162,160], [0,0,0],       [0,0,0],
];

// ---------------------------------------------------------------------------
// PPU struct
// ---------------------------------------------------------------------------
pub struct NesPpu {
    // Registers (CPU-mapped)
    pub ctrl: u8,    // 0x2000 PPUCTRL
    pub mask: u8,    // 0x2001 PPUMASK
    pub status: u8,  // 0x2002 PPUSTATUS
    pub oam_addr: u8,// 0x2003 OAMADDR

    // Loopy scroll registers
    v: u16,   // current VRAM address
    t: u16,   // temporary VRAM address
    fine_x: u8,
    w: bool,  // write latch

    // Internal VRAM
    nametable_ram: [u8; 2048],
    palette_ram: [u8; 32],
    pub oam: [u8; 256],

    // Read buffer for PPUDATA reads
    data_buffer: u8,

    // Timing
    pub cycle: u16,
    pub scanline: i16,
    pub frame: u64,

    // Background shift registers
    bg_pattern_lo: u16,
    bg_pattern_hi: u16,
    bg_attrib_lo: u16,
    bg_attrib_hi: u16,

    // Background fetched data
    nt_byte: u8,
    at_byte: u8,
    bg_lo: u8,
    bg_hi: u8,

    // Sprite data for current scanline
    sprite_count: usize,
    sprite_patterns_lo: [u8; 8],
    sprite_patterns_hi: [u8; 8],
    sprite_x: [u8; 8],
    sprite_attrs: [u8; 8],
    sprite0_hit_possible: bool,
    sprite0_being_rendered: bool,

    // Frame buffer: 256×240 RGBA
    pub frame_buffer: Vec<u8>,
    pub frame_ready: bool,
    pub nmi_requested: bool,
}

impl NesPpu {
    pub fn new() -> Self {
        NesPpu {
            ctrl: 0,
            mask: 0,
            status: 0,
            oam_addr: 0,
            v: 0,
            t: 0,
            fine_x: 0,
            w: false,
            nametable_ram: [0; 2048],
            palette_ram: [0; 32],
            oam: [0xFF; 256],
            data_buffer: 0,
            cycle: 0,
            scanline: 261, // pre-render scanline
            frame: 0,
            bg_pattern_lo: 0,
            bg_pattern_hi: 0,
            bg_attrib_lo: 0,
            bg_attrib_hi: 0,
            nt_byte: 0,
            at_byte: 0,
            bg_lo: 0,
            bg_hi: 0,
            sprite_count: 0,
            sprite_patterns_lo: [0; 8],
            sprite_patterns_hi: [0; 8],
            sprite_x: [0; 8],
            sprite_attrs: [0; 8],
            sprite0_hit_possible: false,
            sprite0_being_rendered: false,
            frame_buffer: vec![0; SCREEN_W * SCREEN_H * 4],
            frame_ready: false,
            nmi_requested: false,
        }
    }

    // -----------------------------------------------------------------------
    // Register access (CPU side)
    // -----------------------------------------------------------------------
    pub fn read_register(&mut self, addr: u16, cartridge: &mut dyn Mapper) -> u8 {
        match addr & 0x07 {
            // PPUSTATUS
            2 => {
                let val = (self.status & 0xE0) | (self.data_buffer & 0x1F);
                self.status &= !0x80; // clear VBlank
                self.w = false;
                val
            }
            // OAMDATA
            4 => self.oam[self.oam_addr as usize],
            // PPUDATA
            7 => {
                let vaddr = self.v & 0x3FFF;
                let val = if vaddr >= 0x3F00 {
                    // Palette reads return immediately (but still buffer nametable)
                    self.data_buffer = self.read_vram(vaddr - 0x1000, cartridge);
                    self.read_palette(vaddr)
                } else {
                    let old = self.data_buffer;
                    self.data_buffer = self.read_vram(vaddr, cartridge);
                    old
                };
                self.v = self.v.wrapping_add(if self.ctrl & 0x04 != 0 { 32 } else { 1 }) & 0x7FFF;
                val
            }
            _ => 0,
        }
    }

    pub fn write_register(&mut self, addr: u16, val: u8, cartridge: &mut dyn Mapper) {
        match addr & 0x07 {
            // PPUCTRL
            0 => {
                let old_nmi = self.ctrl & 0x80;
                self.ctrl = val;
                // t: ...GH..........  <- ...GH
                self.t = (self.t & 0xF3FF) | ((val as u16 & 0x03) << 10);
                // NMI can be triggered if VBlank is set and NMI enable goes high
                if old_nmi == 0 && self.ctrl & 0x80 != 0 && self.status & 0x80 != 0 {
                    self.nmi_requested = true;
                }
            }
            // PPUMASK
            1 => self.mask = val,
            // OAMADDR
            3 => self.oam_addr = val,
            // OAMDATA
            4 => {
                self.oam[self.oam_addr as usize] = val;
                self.oam_addr = self.oam_addr.wrapping_add(1);
            }
            // PPUSCROLL
            5 => {
                if !self.w {
                    // First write: coarse X and fine X
                    self.t = (self.t & 0xFFE0) | (val as u16 >> 3);
                    self.fine_x = val & 0x07;
                } else {
                    // Second write: coarse Y and fine Y
                    self.t = (self.t & 0x8FFF) | ((val as u16 & 0x07) << 12);
                    self.t = (self.t & 0xFC1F) | ((val as u16 & 0xF8) << 2);
                }
                self.w = !self.w;
            }
            // PPUADDR
            6 => {
                if !self.w {
                    // High byte (bits 14-8 of address, bit 15 cleared)
                    self.t = (self.t & 0x80FF) | ((val as u16 & 0x3F) << 8);
                } else {
                    // Low byte
                    self.t = (self.t & 0xFF00) | val as u16;
                    self.v = self.t;
                }
                self.w = !self.w;
            }
            // PPUDATA
            7 => {
                let vaddr = self.v & 0x3FFF;
                if vaddr >= 0x3F00 {
                    self.write_palette(vaddr, val);
                } else {
                    self.write_vram(vaddr, val, cartridge);
                }
                self.v = self.v.wrapping_add(if self.ctrl & 0x04 != 0 { 32 } else { 1 }) & 0x7FFF;
            }
            _ => {}
        }
    }

    pub fn write_oam_dma(&mut self, data: &[u8; 256]) {
        for (i, &b) in data.iter().enumerate() {
            self.oam[(self.oam_addr as usize + i) & 0xFF] = b;
        }
    }

    // -----------------------------------------------------------------------
    // VRAM access
    // -----------------------------------------------------------------------
    pub fn read_vram(&self, addr: u16, cartridge: &dyn Mapper) -> u8 {
        let addr = addr & 0x3FFF;
        match addr {
            0x0000..=0x1FFF => cartridge.read_chr(addr),
            0x2000..=0x3EFF => {
                let mirrored = self.mirror_nametable(addr, cartridge.mirroring());
                self.nametable_ram[mirrored]
            }
            0x3F00..=0x3FFF => self.read_palette(addr),
            _ => 0,
        }
    }

    fn write_vram(&mut self, addr: u16, val: u8, cartridge: &mut dyn Mapper) {
        let addr = addr & 0x3FFF;
        match addr {
            0x0000..=0x1FFF => cartridge.write_chr(addr, val),
            0x2000..=0x3EFF => {
                let mirror = cartridge.mirroring();
                let mirrored = self.mirror_nametable(addr, mirror);
                self.nametable_ram[mirrored] = val;
            }
            0x3F00..=0x3FFF => self.write_palette(addr, val),
            _ => {}
        }
    }

    fn mirror_nametable(&self, addr: u16, mirroring: Mirroring) -> usize {
        let addr = (addr - 0x2000) & 0x0FFF;
        let table = addr / 0x400;
        let offset = addr % 0x400;
        let mapped_table: usize = match mirroring {
            Mirroring::Horizontal => {
                // Tables 0,1 -> 0; Tables 2,3 -> 1
                if table < 2 { 0 } else { 1 }
            }
            Mirroring::Vertical => {
                // Tables 0,2 -> 0; Tables 1,3 -> 1
                (table & 1) as usize
            }
            Mirroring::SingleScreenLow => 0,
            Mirroring::SingleScreenHigh => 1,
            Mirroring::FourScreen => table as usize % 2, // use what we have
        };
        mapped_table * 0x400 + offset as usize
    }

    fn read_palette(&self, addr: u16) -> u8 {
        let idx = Self::palette_index(addr);
        let val = self.palette_ram[idx];
        if self.mask & 0x01 != 0 { val & 0x30 } else { val }
    }

    fn write_palette(&mut self, addr: u16, val: u8) {
        let idx = Self::palette_index(addr);
        self.palette_ram[idx] = val & 0x3F;
    }

    fn palette_index(addr: u16) -> usize {
        let mut idx = (addr - 0x3F00) as usize & 0x1F;
        // Mirrors: $3F10/$3F14/$3F18/$3F1C -> $3F00/$3F04/$3F08/$3F0C
        if idx >= 0x10 && idx & 0x03 == 0 {
            idx &= 0x0F;
        }
        idx
    }

    // -----------------------------------------------------------------------
    // Main step function
    // -----------------------------------------------------------------------
    /// Step the PPU by one CPU cycle (= 3 PPU cycles).
    /// Returns (frame_ready, nmi_triggered).
    pub fn step(&mut self, cpu_cycles: u32, cartridge: &mut dyn Mapper) -> (bool, bool) {
        self.frame_ready = false;
        self.nmi_requested = false;

        for _ in 0..cpu_cycles * 3 {
            self.tick(cartridge);
        }

        (self.frame_ready, self.nmi_requested)
    }

    fn tick(&mut self, cartridge: &mut dyn Mapper) {
        let rendering_enabled = self.mask & 0x18 != 0;
        let is_visible = self.scanline >= 0 && self.scanline < 240;
        let is_prerender = self.scanline == 261;
        let is_render_line = is_visible || is_prerender;
        let is_fetch_cycle = (self.cycle >= 1 && self.cycle <= 256) || (self.cycle >= 321 && self.cycle <= 336);

        // VBlank logic
        if self.scanline == 241 && self.cycle == 1 {
            self.status |= 0x80; // set VBlank
            if self.ctrl & 0x80 != 0 {
                self.nmi_requested = true;
            }
        }
        if is_prerender && self.cycle == 1 {
            self.status &= !0x80; // clear VBlank
            self.status &= !0x40; // clear sprite 0 hit
            self.status &= !0x20; // clear sprite overflow
        }

        if rendering_enabled {
            if is_visible && self.cycle >= 1 && self.cycle <= 256 {
                self.render_pixel(cartridge);
            }

            if is_render_line && is_fetch_cycle {
                self.bg_pattern_lo <<= 1;
                self.bg_pattern_hi <<= 1;
                self.bg_attrib_lo <<= 1;
                self.bg_attrib_hi <<= 1;

                match (self.cycle - 1) & 7 {
                    0 => self.fetch_nt(cartridge),
                    2 => self.fetch_at(cartridge),
                    4 => self.fetch_bg_lo(cartridge),
                    6 => self.fetch_bg_hi(cartridge),
                    7 => self.load_shifters(),
                    _ => {}
                }
            }

            // Increment coarse X at cycle 256 and every 8 fetch cycles
            if is_render_line {
                if is_fetch_cycle && (self.cycle & 7) == 0 {
                    self.increment_x();
                }
                if self.cycle == 256 {
                    self.increment_y();
                }
                if self.cycle == 257 {
                    self.copy_x();
                }
                if is_prerender && self.cycle >= 280 && self.cycle <= 304 {
                    self.copy_y();
                }
            }

            // Sprite evaluation
            if self.cycle == 257 {
                if is_visible {
                    self.evaluate_sprites(cartridge);
                } else {
                    self.sprite_count = 0;
                }
            }
        }

        // Advance cycle/scanline
        self.cycle += 1;
        if self.cycle > 340 {
            self.cycle = 0;
            self.scanline += 1;
            // Notify mapper on each new visible scanline (0-239).
            // This drives the MMC3 IRQ scanline counter (A12 approximation).
            if self.scanline >= 0 && self.scanline < 240 {
                cartridge.notify_scanline();
            }
            if self.scanline > 261 {
                self.scanline = 0;
                self.frame = self.frame.wrapping_add(1);
                self.frame_ready = true;
            }
        }

        // Odd frame skip (skip cycle 0 on scanline 0 for odd frames when rendering)
        if rendering_enabled && self.frame & 1 == 1 && self.scanline == 0 && self.cycle == 0 {
            self.cycle = 1;
        }
    }

    // -----------------------------------------------------------------------
    // Background fetch helpers
    // -----------------------------------------------------------------------
    fn fetch_nt(&mut self, cartridge: &mut dyn Mapper) {
        let addr = 0x2000 | (self.v & 0x0FFF);
        self.nt_byte = self.read_vram(addr, cartridge);
    }

    fn fetch_at(&mut self, cartridge: &mut dyn Mapper) {
        let v = self.v;
        let addr = 0x23C0 | (v & 0x0C00) | ((v >> 4) & 0x38) | ((v >> 2) & 0x07);
        let at = self.read_vram(addr, cartridge);
        let shift = ((v >> 4) & 0x04) | (v & 0x02);
        self.at_byte = (at >> shift) & 0x03;
    }

    fn fetch_bg_lo(&mut self, cartridge: &mut dyn Mapper) {
        let fine_y = (self.v >> 12) & 0x07;
        let table = if self.ctrl & 0x10 != 0 { 0x1000u16 } else { 0x0000u16 };
        let addr = table + self.nt_byte as u16 * 16 + fine_y;
        self.bg_lo = self.read_vram(addr, cartridge);
    }

    fn fetch_bg_hi(&mut self, cartridge: &mut dyn Mapper) {
        let fine_y = (self.v >> 12) & 0x07;
        let table = if self.ctrl & 0x10 != 0 { 0x1000u16 } else { 0x0000u16 };
        let addr = table + self.nt_byte as u16 * 16 + fine_y + 8;
        self.bg_hi = self.read_vram(addr, cartridge);
    }

    fn load_shifters(&mut self) {
        self.bg_pattern_lo = (self.bg_pattern_lo & 0xFF00) | self.bg_lo as u16;
        self.bg_pattern_hi = (self.bg_pattern_hi & 0xFF00) | self.bg_hi as u16;
        self.bg_attrib_lo = (self.bg_attrib_lo & 0xFF00) | (if self.at_byte & 0x01 != 0 { 0xFF } else { 0 });
        self.bg_attrib_hi = (self.bg_attrib_hi & 0xFF00) | (if self.at_byte & 0x02 != 0 { 0xFF } else { 0 });
    }

    // -----------------------------------------------------------------------
    // Loopy scroll helpers
    // -----------------------------------------------------------------------
    fn increment_x(&mut self) {
        if self.v & 0x001F == 31 {
            self.v &= !0x001F;
            self.v ^= 0x0400; // switch horizontal nametable
        } else {
            self.v += 1;
        }
    }

    fn increment_y(&mut self) {
        if (self.v & 0x7000) != 0x7000 {
            self.v += 0x1000;
        } else {
            self.v &= !0x7000;
            let mut y = (self.v & 0x03E0) >> 5;
            if y == 29 {
                y = 0;
                self.v ^= 0x0800; // switch vertical nametable
            } else if y == 31 {
                y = 0;
            } else {
                y += 1;
            }
            self.v = (self.v & !0x03E0) | (y << 5);
        }
    }

    fn copy_x(&mut self) {
        // Copy horizontal bits from t to v
        self.v = (self.v & 0xFBE0) | (self.t & 0x041F);
    }

    fn copy_y(&mut self) {
        // Copy vertical bits from t to v
        self.v = (self.v & 0x841F) | (self.t & 0x7BE0);
    }

    // -----------------------------------------------------------------------
    // Sprite evaluation
    // -----------------------------------------------------------------------
    fn evaluate_sprites(&mut self, cartridge: &mut dyn Mapper) {
        let sprite_size = if self.ctrl & 0x20 != 0 { 16u8 } else { 8u8 };
        self.sprite_count = 0;
        self.sprite0_hit_possible = false;
        self.sprite0_being_rendered = false;

        let next_scanline = self.scanline as u16;

        for i in 0..64usize {
            let y = self.oam[i * 4] as u16;
            let diff = next_scanline.wrapping_sub(y);

            if diff < sprite_size as u16 {
                if self.sprite_count < 8 {
                    if i == 0 {
                        self.sprite0_hit_possible = true;
                    }

                    let tile_idx = self.oam[i * 4 + 1];
                    let attrs = self.oam[i * 4 + 2];
                    let x = self.oam[i * 4 + 3];

                    let flip_v = attrs & 0x80 != 0;
                    let mut row = if flip_v { sprite_size as u16 - 1 - diff } else { diff };

                    let (table, tile) = if sprite_size == 16 {
                        let t = if tile_idx & 0x01 != 0 { 0x1000u16 } else { 0x0000u16 };
                        let ti = (tile_idx & 0xFE) as u16;
                        if row >= 8 {
                            row -= 8;
                            (t, ti + 1)
                        } else {
                            (t, ti)
                        }
                    } else {
                        let t = if self.ctrl & 0x08 != 0 { 0x1000u16 } else { 0x0000u16 };
                        (t, tile_idx as u16)
                    };

                    let addr_lo = table + tile * 16 + row;
                    let addr_hi = addr_lo + 8;

                    let mut lo = self.read_vram(addr_lo, cartridge);
                    let mut hi = self.read_vram(addr_hi, cartridge);

                    if attrs & 0x40 != 0 {
                        // Flip horizontally
                        lo = lo.reverse_bits();
                        hi = hi.reverse_bits();
                    }

                    let sc = self.sprite_count;
                    self.sprite_patterns_lo[sc] = lo;
                    self.sprite_patterns_hi[sc] = hi;
                    self.sprite_x[sc] = x;
                    self.sprite_attrs[sc] = attrs;
                    self.sprite_count += 1;
                } else {
                    self.status |= 0x20; // sprite overflow
                    break;
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Pixel rendering
    // -----------------------------------------------------------------------
    fn render_pixel(&mut self, cartridge: &mut dyn Mapper) {
        let x = (self.cycle - 1) as usize;
        let y = self.scanline as usize;

        let show_bg = self.mask & 0x08 != 0;
        let show_sprites = self.mask & 0x10 != 0;
        let show_bg_left = self.mask & 0x02 != 0;
        let show_spr_left = self.mask & 0x04 != 0;

        let bg_pixel_active = show_bg && (x >= 8 || show_bg_left);
        let spr_pixel_active = show_sprites && (x >= 8 || show_spr_left);

        // Background pixel
        let (bg_pixel, bg_palette) = if bg_pixel_active {
            let bit = 15 - self.fine_x as u16;
            let lo = (self.bg_pattern_lo >> bit) & 1;
            let hi = (self.bg_pattern_hi >> bit) & 1;
            let pixel = (hi << 1) | lo;
            let pal_lo = (self.bg_attrib_lo >> bit) & 1;
            let pal_hi = (self.bg_attrib_hi >> bit) & 1;
            let palette = (pal_hi << 1) | pal_lo;
            (pixel as u8, palette as u8)
        } else {
            (0, 0)
        };

        // Sprite pixel
        let (spr_pixel, spr_palette, spr_behind_bg, spr_zero) = if spr_pixel_active {
            let mut found = (0u8, 0u8, false, false);
            for i in 0..self.sprite_count {
                let sx = self.sprite_x[i] as usize;
                if x < sx || x >= sx + 8 {
                    continue;
                }
                let col = (x - sx) as u8;
                let bit = 7 - col;
                let lo = (self.sprite_patterns_lo[i] >> bit) & 1;
                let hi = (self.sprite_patterns_hi[i] >> bit) & 1;
                let pixel = (hi << 1) | lo;
                if pixel == 0 { continue; }
                let palette = (self.sprite_attrs[i] & 0x03) + 4;
                let behind = self.sprite_attrs[i] & 0x20 != 0;
                found = (pixel, palette, behind, i == 0 && self.sprite0_hit_possible);
                break;
            }
            found
        } else {
            (0, 0, false, false)
        };

        // Sprite 0 hit detection
        if spr_zero && bg_pixel != 0 && spr_pixel != 0 && x != 255 {
            self.status |= 0x40;
        }

        // Pixel priority
        let (final_pixel, final_palette) = match (bg_pixel, spr_pixel) {
            (0, 0) => (0u8, 0u8),
            (0, sp) => (sp, spr_palette),
            (bg, 0) => (bg, bg_palette),
            (bg, sp) => {
                if spr_behind_bg {
                    (bg, bg_palette)
                } else {
                    (sp, spr_palette)
                }
            }
        };

        // Look up color in palette
        let palette_addr = if final_pixel == 0 {
            0x3F00u16
        } else {
            0x3F00 | ((final_palette as u16) << 2) | final_pixel as u16
        };
        let color_idx = self.read_palette(palette_addr) as usize;
        let rgb = NES_PALETTE[color_idx % 64];

        let base = (y * SCREEN_W + x) * 4;
        self.frame_buffer[base] = rgb[0];
        self.frame_buffer[base + 1] = rgb[1];
        self.frame_buffer[base + 2] = rgb[2];
        self.frame_buffer[base + 3] = 0xFF;

        // Suppress unused warning for cartridge param
        let _ = cartridge;
    }

    // -----------------------------------------------------------------------
    // Save / Load state
    // -----------------------------------------------------------------------
    pub fn save(&self, buf: &mut Vec<u8>) {
        // Registers
        write_u8(buf, self.ctrl);
        write_u8(buf, self.mask);
        write_u8(buf, self.status);
        write_u8(buf, self.oam_addr);
        // Loopy
        write_u16(buf, self.v);
        write_u16(buf, self.t);
        write_u8(buf, self.fine_x);
        write_bool(buf, self.w);
        // Internal VRAM
        write_slice(buf, &self.nametable_ram);
        write_slice(buf, &self.palette_ram);
        write_slice(buf, &self.oam);
        // Data buffer & timing
        write_u8(buf, self.data_buffer);
        write_u16(buf, self.cycle);
        write_u16(buf, self.scanline as u16); // i16 stored as u16 (two's complement)
        write_u64(buf, self.frame);
        // BG shift registers
        write_u16(buf, self.bg_pattern_lo);
        write_u16(buf, self.bg_pattern_hi);
        write_u16(buf, self.bg_attrib_lo);
        write_u16(buf, self.bg_attrib_hi);
        // BG fetch bytes
        write_u8(buf, self.nt_byte);
        write_u8(buf, self.at_byte);
        write_u8(buf, self.bg_lo);
        write_u8(buf, self.bg_hi);
        // Sprite state
        write_u32(buf, self.sprite_count as u32);
        write_slice(buf, &self.sprite_patterns_lo);
        write_slice(buf, &self.sprite_patterns_hi);
        write_slice(buf, &self.sprite_x);
        write_slice(buf, &self.sprite_attrs);
        write_bool(buf, self.sprite0_hit_possible);
        write_bool(buf, self.sprite0_being_rendered);
        // Misc flags
        write_bool(buf, self.frame_ready);
        write_bool(buf, self.nmi_requested);
        // Frame buffer (256*240*4)
        write_slice(buf, &self.frame_buffer);
    }

    pub fn load(&mut self, data: &[u8], off: &mut usize) {
        self.ctrl     = read_u8(data, off);
        self.mask     = read_u8(data, off);
        self.status   = read_u8(data, off);
        self.oam_addr = read_u8(data, off);
        self.v        = read_u16(data, off);
        self.t        = read_u16(data, off);
        self.fine_x   = read_u8(data, off);
        self.w        = read_bool(data, off);
        self.nametable_ram.copy_from_slice(read_slice(data, off, 2048));
        self.palette_ram.copy_from_slice(read_slice(data, off, 32));
        self.oam.copy_from_slice(read_slice(data, off, 256));
        self.data_buffer = read_u8(data, off);
        self.cycle       = read_u16(data, off);
        self.scanline    = read_u16(data, off) as i16;
        self.frame       = read_u64(data, off);
        self.bg_pattern_lo = read_u16(data, off);
        self.bg_pattern_hi = read_u16(data, off);
        self.bg_attrib_lo  = read_u16(data, off);
        self.bg_attrib_hi  = read_u16(data, off);
        self.nt_byte = read_u8(data, off);
        self.at_byte = read_u8(data, off);
        self.bg_lo   = read_u8(data, off);
        self.bg_hi   = read_u8(data, off);
        self.sprite_count = read_u32(data, off) as usize;
        self.sprite_patterns_lo.copy_from_slice(read_slice(data, off, 8));
        self.sprite_patterns_hi.copy_from_slice(read_slice(data, off, 8));
        self.sprite_x.copy_from_slice(read_slice(data, off, 8));
        self.sprite_attrs.copy_from_slice(read_slice(data, off, 8));
        self.sprite0_hit_possible    = read_bool(data, off);
        self.sprite0_being_rendered  = read_bool(data, off);
        self.frame_ready    = read_bool(data, off);
        self.nmi_requested  = read_bool(data, off);
        let fb_len = SCREEN_W * SCREEN_H * 4;
        self.frame_buffer.copy_from_slice(read_slice(data, off, fb_len));
    }
}
