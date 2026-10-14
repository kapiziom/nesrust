//! The frame counter: a sequencer that clocks the envelopes, linear counter, length counters and sweeps,
//! and can raise an IRQ. https://www.nesdev.org/wiki/APU_Frame_Counter
//!
//! Positions are CPU cycles since the sequence started (1 APU cycle = 2 CPU cycles).

use crate::emulator::region::Region;

/// The cycle numbers of the sequence steps for one region.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
struct Steps {
    quarter_1: u32,
    half_1: u32,
    quarter_3: u32,
    /// 4-step mode: the IRQ flag is set on three consecutive cycles, the half-frame clock lands in the middle one.
    four_step_irq: u32,
    four_step_half: u32,
    four_step_end: u32,
    five_step_half: u32,
    five_step_end: u32,
}

const NTSC: Steps = Steps {
    quarter_1: 7457,
    half_1: 14913,
    quarter_3: 22371,
    four_step_irq: 29828,
    four_step_half: 29829,
    four_step_end: 29830,
    five_step_half: 37281,
    five_step_end: 37282,
};

const PAL: Steps = Steps {
    quarter_1: 8313,
    half_1: 16627,
    quarter_3: 24939,
    four_step_irq: 33252,
    four_step_half: 33253,
    four_step_end: 33254,
    five_step_half: 41565,
    five_step_end: 41566,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FrameClock {
    /// Envelopes and the triangle's linear counter (~240 Hz).
    pub quarter: bool,
    /// Length counters and sweep units (~120 Hz).
    pub half: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FrameCounter {
    steps: Steps,
    position: u32,
    five_step: bool,
    irq_inhibit: bool,
    irq_flag: bool,
    /// A $4017 write takes effect a few cycles later: (cycles left, written value).
    pending_write: Option<(u8, u8)>,
    last_written: u8,
}

impl Default for FrameCounter {
    fn default() -> Self {
        Self::new(Region::Ntsc)
    }
}

impl FrameCounter {
    pub fn new(region: Region) -> Self {
        FrameCounter {
            steps: if region.pal_apu_tables() { PAL } else { NTSC },
            position: 0,
            five_step: false,
            irq_inhibit: false,
            irq_flag: false,
            pending_write: None,
            last_written: 0,
        }
    }

    /// $4017. The new mode and a sequence restart take effect 3 CPU cycles later (4 if the write
    /// lands on an odd cycle); the IRQ inhibit flag acts immediately.
    pub fn write(&mut self, data: u8, odd_cpu_cycle: bool) {
        self.last_written = data;
        self.irq_inhibit = data & 0x40 != 0;
        if self.irq_inhibit {
            self.irq_flag = false;
        }
        self.pending_write = Some((if odd_cpu_cycle { 4 } else { 3 }, data));
    }

    /// A reset behaves as if the last value had been written to $4017 just before the CPU's reset sequence began,
    /// so the sequence restarts at once (the 3-4 cycle write delay elapses during the reset sequence itself).
    pub fn restart_after_reset(&mut self) {
        self.pending_write = None;
        self.position = 0;
        self.five_step = self.last_written & 0x80 != 0;
        self.irq_inhibit = self.last_written & 0x40 != 0;
        self.irq_flag = false;
    }

    pub fn irq_flag(&self) -> bool {
        self.irq_flag
    }

    pub fn clear_irq(&mut self) {
        self.irq_flag = false;
    }

    /// Advances one CPU cycle and reports the clocks that fire on it.
    pub fn tick(&mut self) -> FrameClock {
        let mut clock = FrameClock::default();

        if let Some((delay, data)) = self.pending_write {
            if delay == 1 {
                self.pending_write = None;
                self.position = 0;
                self.five_step = data & 0x80 != 0;
                if self.five_step {
                    // 5-step mode clocks everything right away
                    clock = FrameClock { quarter: true, half: true };
                }
                return clock;
            }
            self.pending_write = Some((delay - 1, data));
        }

        self.position += 1;

        let steps = self.steps;
        let position = self.position;

        if self.five_step {
            if position == steps.quarter_1 || position == steps.quarter_3 {
                clock.quarter = true;
            } else if position == steps.half_1 || position == steps.five_step_half {
                clock = FrameClock { quarter: true, half: true };
            } else if position == steps.five_step_end {
                self.position = 0;
            }
        } else if position == steps.quarter_1 || position == steps.quarter_3 {
            clock.quarter = true;
        } else if position == steps.half_1 {
            clock = FrameClock { quarter: true, half: true };
        } else if position == steps.four_step_irq {
            self.set_irq();
        } else if position == steps.four_step_half {
            clock = FrameClock { quarter: true, half: true };
            self.set_irq();
        } else if position == steps.four_step_end {
            self.set_irq();
            self.position = 0;
        }

        clock
    }

    fn set_irq(&mut self) {
        if !self.irq_inhibit {
            self.irq_flag = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cycles (1-based, counted from the start of the sequence) at which quarter / half clocks fire.
    fn clocks(counter: &mut FrameCounter, cycles: u32) -> (Vec<u32>, Vec<u32>) {
        let (mut quarters, mut halves) = (Vec::new(), Vec::new());
        for cycle in 1..=cycles {
            let clock = counter.tick();
            if clock.quarter {
                quarters.push(cycle);
            }
            if clock.half {
                halves.push(cycle);
            }
        }
        (quarters, halves)
    }

    #[test]
    fn four_step_sequence() {
        let mut counter = FrameCounter::default();
        let (quarters, halves) = clocks(&mut counter, 29830 * 2);

        assert_eq!(quarters, [7457, 14913, 22371, 29829, 29830 + 7457, 29830 + 14913, 29830 + 22371, 29830 + 29829]);
        assert_eq!(halves, [14913, 29829, 29830 + 14913, 29830 + 29829]);
    }

    #[test]
    fn the_pal_sequence_is_longer() {
        let mut counter = FrameCounter::new(Region::Pal);
        let (quarters, halves) = clocks(&mut counter, 33254);

        assert_eq!(quarters, [8313, 16627, 24939, 33253]);
        assert_eq!(halves, [16627, 33253]);
        assert!(counter.irq_flag());

        let mut five = FrameCounter::new(Region::Pal);
        five.write(0x80, false);
        five.tick();
        five.tick();
        five.tick();
        let (quarters, halves) = clocks(&mut five, 41566);
        assert_eq!(quarters, [8313, 16627, 24939, 41565]);
        assert_eq!(halves, [16627, 41565]);
    }

    #[test]
    fn four_step_irq_flag_is_set_at_the_end_of_the_frame() {
        let mut counter = FrameCounter::default();
        clocks(&mut counter, 29827);
        assert!(!counter.irq_flag());

        counter.tick();
        assert!(counter.irq_flag(), "set on cycle 29828");
    }

    #[test]
    fn irq_inhibit_blocks_the_flag_and_clears_it() {
        let mut counter = FrameCounter::default();
        clocks(&mut counter, 29830);
        assert!(counter.irq_flag());

        counter.write(0x40, false);
        assert!(!counter.irq_flag(), "writing $4017 with the inhibit bit acknowledges");

        clocks(&mut counter, 29830);
        assert!(!counter.irq_flag());
    }

    #[test]
    fn five_step_mode_never_raises_the_irq_and_has_a_longer_sequence() {
        let mut counter = FrameCounter::default();
        counter.write(0x80, false);
        // 3 cycles of delay, the restart itself clocks everything
        let first = (counter.tick(), counter.tick(), counter.tick());
        assert_eq!(first.2, FrameClock { quarter: true, half: true });

        let (quarters, halves) = clocks(&mut counter, 37282);
        assert_eq!(quarters, [7457, 14913, 22371, 37281]);
        assert_eq!(halves, [14913, 37281]);
        assert!(!counter.irq_flag());
    }

    #[test]
    fn a_4017_write_restarts_the_sequence_after_the_delay() {
        let mut counter = FrameCounter::default();
        clocks(&mut counter, 5000);

        counter.write(0x00, false); // takes 3 cycles
        counter.tick();
        counter.tick();
        counter.tick();
        assert_eq!(counter.position, 0);

        let (quarters, _) = clocks(&mut counter, 7457);
        assert_eq!(quarters, [7457], "the first quarter clock is a full 7457 cycles after the restart");
    }

    #[test]
    fn the_write_delay_is_one_cycle_longer_on_odd_cycles() {
        let mut even = FrameCounter::default();
        let mut odd = FrameCounter::default();
        clocks(&mut even, 100);
        clocks(&mut odd, 100);
        even.write(0x00, false);
        odd.write(0x00, true);

        for _ in 0..3 {
            even.tick();
            odd.tick();
        }

        assert_eq!(even.position, 0);
        assert_ne!(odd.position, 0, "the odd-cycle write is still pending");
        odd.tick();
        assert_eq!(odd.position, 0);
    }
}
