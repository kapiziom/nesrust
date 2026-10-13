//! The NES master palette: 64 colours (6-bit index) in 8 variants for the colour emphasis bits of PPUMASK.
//!
//! The default is nesdev's "2C02G (wiki)" palette, `assets/palettes/2C02G_U_wiki.pal`: 512 RGB triplets, the
//! variant selected by PPUMASK bits 5-7 (bit 0 red, bit 1 green, bit 2 blue). Other `.pal` files can be loaded with
//! `Palette::from_pal_bytes`.

const COLORS: usize = 64;
const VARIANTS: usize = 8;

/// Each colour channel is attenuated by this factor for every emphasis bit that does not select it; only used for
/// 192-byte `.pal` files that carry no emphasis variants.
const EMPHASIS_ATTENUATION: f32 = 0.75;

#[derive(Clone)]
pub struct Palette {
    /// `[emphasis][colour]` as RGB triplets, flattened.
    rgb: [u8; COLORS * VARIANTS * 3],
}

impl Default for Palette {
    fn default() -> Self {
        Palette::from_pal_bytes(include_bytes!("../../../assets/palettes/2C02G_U_wiki.pal"))
            .expect("the bundled palette is a valid .pal file")
    }
}

impl Palette {
    /// A `.pal` file holds 64 RGB triplets (192 bytes), or 512 (1536 bytes) when it includes the 8 emphasis
    /// variants. A 192-byte file gets approximated emphasis variants. Returns `None` for anything shorter.
    pub fn from_pal_bytes(bytes: &[u8]) -> Option<Self> {
        let mut rgb = [0u8; COLORS * VARIANTS * 3];

        if bytes.len() >= COLORS * VARIANTS * 3 {
            rgb.copy_from_slice(&bytes[..COLORS * VARIANTS * 3]);
            return Some(Palette { rgb });
        }

        let base = bytes.get(..COLORS * 3)?;
        for emphasis in 0..VARIANTS {
            for color in 0..COLORS {
                for channel in 0..3 {
                    let emphasised = emphasis & (1 << channel) != 0;
                    let others = emphasis.count_ones() as i32 - emphasised as i32;
                    let value = base[color * 3 + channel] as f32 * EMPHASIS_ATTENUATION.powi(others);
                    rgb[(emphasis * COLORS + color) * 3 + channel] = value as u8;
                }
            }
        }

        Some(Palette { rgb })
    }

    /// RGBA bytes of a palette colour with the PPUMASK emphasis bits (bit 0 red, bit 1 green, bit 2 blue).
    pub fn rgba(&self, color: u8, emphasis: u8) -> [u8; 4] {
        let index = ((emphasis as usize & 0x07) * COLORS + (color as usize & 0x3F)) * 3;
        [self.rgb[index], self.rgb[index + 1], self.rgb[index + 2], 0xFF]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_palette_is_the_bundled_nesdev_palette() {
        let palette = Palette::default();

        assert_eq!(palette.rgba(0x0F, 0), [0, 0, 0, 0xFF], "$0F is black");
        assert_eq!(palette.rgba(0x30, 0), [0xFF, 0xFF, 0xFF, 0xFF], "$30 is white");
        assert_eq!(palette.rgba(0x00, 0), [87, 87, 87, 0xFF]);
        assert_eq!(palette.rgba(0x3F, 0), palette.rgba(0xFF, 0), "only 6 bits are used");
    }

    #[test]
    fn bundled_emphasis_variants_favour_the_selected_channel() {
        let palette = Palette::default();
        let plain = palette.rgba(0x30, 0);

        let red = palette.rgba(0x30, 0b001);
        assert!(red[0] > red[1] && red[0] > red[2], "red emphasis keeps red: {red:?}");
        let blue = palette.rgba(0x30, 0b100);
        assert!(blue[2] > blue[0] && blue[2] > blue[1], "blue emphasis keeps blue: {blue:?}");

        let all = palette.rgba(0x30, 0b111);
        assert!(all[0] < plain[0] && all[1] < plain[1] && all[2] < plain[2], "all three dim the picture");
    }

    #[test]
    fn short_pal_files_get_approximated_emphasis() {
        let mut bytes = vec![0u8; 192];
        bytes[3..6].copy_from_slice(&[100, 200, 40]);
        let palette = Palette::from_pal_bytes(&bytes).unwrap();

        assert_eq!(palette.rgba(1, 0), [100, 200, 40, 0xFF]);
        assert_eq!(palette.rgba(1, 0b001), [100, 150, 30, 0xFF], "red emphasis dims green and blue by 0.75");
        assert_eq!(palette.rgba(1, 0b111)[0], (100.0 * 0.75 * 0.75) as u8);
    }

    #[test]
    fn full_pal_files_are_used_as_they_are() {
        let mut bytes = vec![0u8; 1536];
        bytes[(3 * 64 + 5) * 3..(3 * 64 + 5) * 3 + 3].copy_from_slice(&[9, 8, 7]);

        assert_eq!(Palette::from_pal_bytes(&bytes).unwrap().rgba(5, 3), [9, 8, 7, 0xFF]);
    }

    #[test]
    fn too_short_files_are_rejected() {
        assert!(Palette::from_pal_bytes(&[0; 191]).is_none());
        assert!(Palette::from_pal_bytes(&[]).is_none());
    }
}
