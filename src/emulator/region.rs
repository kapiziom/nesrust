//! NTSC and PAL consoles differ in clock speeds, PPU geometry and a few APU tables.
//! https://www.nesdev.org/wiki/Cycle_reference_chart

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Region {
    #[default]
    Ntsc,
    Pal,
}

impl Region {
    /// CPU clock in Hz.
    pub fn cpu_clock_hz(self) -> f64 {
        match self {
            Region::Ntsc => 1_789_773.0,
            Region::Pal => 1_662_607.0,
        }
    }

    /// Scanlines per frame; the last one is the pre-render line.
    pub fn scanlines_per_frame(self) -> u16 {
        match self {
            Region::Ntsc => 262,
            Region::Pal => 312,
        }
    }

    /// PPU dots per CPU cycle as a fraction (dots, cycles): 3 on NTSC, 16/5 = 3.2 on PAL.
    pub fn dots_per_cycles(self) -> (u8, u8) {
        match self {
            Region::Ntsc => (3, 1),
            Region::Pal => (16, 5),
        }
    }

    /// NTSC drops one dot from the pre-render line of every other frame while rendering; PAL does not.
    pub fn skips_dot_on_odd_frames(self) -> bool {
        self == Region::Ntsc
    }

    /// Frames per second.
    pub fn frame_rate(self) -> f64 {
        match self {
            Region::Ntsc => 60.0988,
            Region::Pal => 50.0070,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ntsc_is_the_default() {
        assert_eq!(Region::default(), Region::Ntsc);
    }

    #[test]
    fn pal_runs_slower_with_a_longer_frame() {
        assert!(Region::Pal.cpu_clock_hz() < Region::Ntsc.cpu_clock_hz());
        assert_eq!(Region::Ntsc.scanlines_per_frame(), 262);
        assert_eq!(Region::Pal.scanlines_per_frame(), 312);
        assert!(!Region::Pal.skips_dot_on_odd_frames());
        assert!(Region::Ntsc.skips_dot_on_odd_frames());
    }

    #[test]
    fn the_frame_rate_follows_from_the_clocks() {
        for region in [Region::Ntsc, Region::Pal] {
            let (dots, cycles) = region.dots_per_cycles();
            let dots_per_frame = region.scanlines_per_frame() as f64 * 341.0 - 0.5; // average, with the odd-frame skip on NTSC
            let cpu_cycles = dots_per_frame * cycles as f64 / dots as f64;
            let rate = region.cpu_clock_hz() / cpu_cycles;
            assert!((rate - region.frame_rate()).abs() < 0.05, "{region:?}: {rate}");
        }
    }
}
