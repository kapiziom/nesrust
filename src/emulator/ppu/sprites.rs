//! Sprite evaluation and fetching.
//! https://www.nesdev.org/wiki/PPU_sprite_evaluation, https://www.nesdev.org/wiki/PPU_OAM
//!
//! Evaluation of the next scanline's sprites is done in one go at dot 257 instead of during dots 65-256;
//! the fetches of the pattern bytes happen on the real dots so mappers watching A12 (MMC3) see them.

use super::{PPU, CTRL_SPRITE_SIZE, CTRL_SPRITE_TABLE, MASK_SPRITES, MASK_SPRITES_LEFT, STATUS_SPRITE_OVERFLOW};
use crate::emulator::cartridge::Cartridge;

const ATTR_PALETTE: u8 = 0x03;
const ATTR_BEHIND_BACKGROUND: u8 = 0x20;
const ATTR_FLIP_HORIZONTAL: u8 = 0x40;
const ATTR_FLIP_VERTICAL: u8 = 0x80;

/// One of the eight sprite units for the scanline being drawn.
#[derive(Clone, Copy, Default)]
pub(super) struct Sprite {
    pattern_lo: u8,
    pattern_hi: u8,
    attributes: u8,
    /// Counts down the dots until the sprite starts; then the pattern shifts out.
    x_counter: u8,
    is_sprite_zero: bool,
}

pub(super) struct SpritePixel {
    /// 1-3 (0 is transparent and never returned).
    pub pixel: u8,
    pub palette: u8,
    pub behind_background: bool,
    pub is_sprite_zero: bool,
}

impl PPU {
    fn sprite_height(&self) -> i16 {
        if self.ctrl & CTRL_SPRITE_SIZE != 0 { 16 } else { 8 }
    }

    /// Finds the (up to 8) sprites that appear on the next scanline and emulates the hardware's overflow bug.
    pub(super) fn prepare_next_scanline_sprites(&mut self, visible: bool) {
        self.found_sprites = [[0xFF; 4]; 8];
        self.found_count = 0;
        self.found_sprite_zero = false;

        if visible {
            self.evaluate_sprites();
        }

        // the fetches of dots 257-320 fill the sprite units for the next scanline
        self.sprite_count = self.found_count;
    }

    fn evaluate_sprites(&mut self) {
        let height = self.sprite_height();
        let line = self.scanline as i16;
        let in_range = |y: u8| {
            let row = line - y as i16;
            (0..height).contains(&row)
        };

        let mut sprite = 0usize;

        while sprite < 64 && self.found_count < 8 {
            let base = sprite * 4;
            if in_range(self.oam[base]) {
                self.found_sprites[self.found_count].copy_from_slice(&self.oam[base..base + 4]);
                self.found_sprite_zero |= sprite == 0;
                self.found_count += 1;
            }
            sprite += 1;
        }

        // Looking for a ninth sprite: the hardware advances the byte index together with the sprite index, so after
        // the first miss it compares tile/attribute/x bytes against the scanline ("diagonal" scan).
        let mut byte = 0usize;
        while self.found_count == 8 && sprite < 64 {
            if in_range(self.oam[(sprite * 4 + byte) & 0xFF]) {
                self.status |= STATUS_SPRITE_OVERFLOW;
                break;
            }
            sprite += 1;
            byte = (byte + 1) & 3;
        }
    }

    /// Called on dots 257-320 of every visible and pre-render line.
    pub(super) fn fetch_sprite_patterns(&mut self, cartridge: &mut Cartridge) {
        let offset = self.dot - 257;
        let slot = (offset / 8) as usize;
        let phase = offset % 8;

        if phase != 4 && phase != 6 {
            return;
        }

        let real = slot < self.found_count;
        // empty slots fetch tile $FF (the address still reaches the cartridge) and stay transparent
        let [y, tile, attributes, x] = if real { self.found_sprites[slot] } else { [0, 0xFF, 0, 0xFF] };

        let height = self.sprite_height();
        let mut row = if real { self.scanline as i16 - y as i16 } else { 0 };
        if attributes & ATTR_FLIP_VERTICAL != 0 {
            row = height - 1 - row;
        }
        let row = row as u16;

        let address = if height == 8 {
            let table = if self.ctrl & CTRL_SPRITE_TABLE != 0 { 0x1000 } else { 0 };
            table + tile as u16 * 16 + row
        } else {
            // 8x16: bit 0 of the tile number selects the pattern table, the rest the top tile
            let table = (tile as u16 & 1) * 0x1000;
            let top_tile = tile as u16 & 0xFE;
            table + (top_tile + (row >> 3)) * 16 + (row & 7)
        };

        if phase == 4 {
            let pattern = self.bus_read(cartridge, address);
            self.sprites[slot].pattern_lo = if real { pattern } else { 0 };
        } else {
            let pattern = self.bus_read(cartridge, address + 8);
            let unit = &mut self.sprites[slot];
            unit.pattern_hi = if real { pattern } else { 0 };
            unit.attributes = attributes;
            unit.x_counter = x;
            unit.is_sprite_zero = real && slot == 0 && self.found_sprite_zero;

            if attributes & ATTR_FLIP_HORIZONTAL != 0 {
                unit.pattern_lo = unit.pattern_lo.reverse_bits();
                unit.pattern_hi = unit.pattern_hi.reverse_bits();
            }
        }
    }

    pub(super) fn shift_sprites(&mut self) {
        for unit in &mut self.sprites[..self.sprite_count] {
            if unit.x_counter > 0 {
                unit.x_counter -= 1;
            } else {
                unit.pattern_lo <<= 1;
                unit.pattern_hi <<= 1;
            }
        }
    }

    /// The first (highest priority) opaque sprite pixel at `x`, if sprites are enabled and not clipped there.
    pub(super) fn sprite_pixel(&self, x: usize) -> Option<SpritePixel> {
        if self.mask & MASK_SPRITES == 0 || (x < 8 && self.mask & MASK_SPRITES_LEFT == 0) {
            return None;
        }

        self.sprites[..self.sprite_count]
            .iter()
            .filter(|unit| unit.x_counter == 0)
            .find_map(|unit| {
                let pixel = ((unit.pattern_hi >> 7) & 1) << 1 | ((unit.pattern_lo >> 7) & 1);
                (pixel != 0).then_some(SpritePixel {
                    pixel,
                    palette: unit.attributes & ATTR_PALETTE,
                    behind_background: unit.attributes & ATTR_BEHIND_BACKGROUND != 0,
                    is_sprite_zero: unit.is_sprite_zero,
                })
            })
    }
}
