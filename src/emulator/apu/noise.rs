//! Noise channel ($400C-$400F). https://www.nesdev.org/wiki/APU_Noise

use super::units::{Envelope, LengthCounter, Timer};
use crate::emulator::region::Region;

/// Timer periods in CPU cycles.
const NTSC_PERIODS: [u16; 16] = [4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068];
const PAL_PERIODS: [u16; 16] = [4, 8, 14, 30, 60, 88, 118, 148, 188, 236, 354, 472, 708, 944, 1890, 3778];

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Noise {
    pal: bool,
    timer: Timer,
    lfsr: u16,
    short_mode: bool,
    pub envelope: Envelope,
    pub length: LengthCounter,
}

impl Default for Noise {
    fn default() -> Self {
        Self::new(Region::Ntsc)
    }
}

impl Noise {
    pub fn new(region: Region) -> Self {
        let periods = if region.pal_apu_tables() { &PAL_PERIODS } else { &NTSC_PERIODS };
        Noise {
            pal: region.pal_apu_tables(),
            timer: Timer::with_period(periods[0] - 1),
            lfsr: 1,
            short_mode: false,
            envelope: Envelope::default(),
            length: LengthCounter::default(),
        }
    }
}

impl Noise {
    fn periods(&self) -> &'static [u16; 16] {
        if self.pal { &PAL_PERIODS } else { &NTSC_PERIODS }
    }

    /// `register` is 0-3 (the offset from $400C); 1 is unused.
    pub fn write(&mut self, register: u16, data: u8) {
        match register {
            0 => {
                self.length.set_halt(data & 0x20 != 0);
                self.envelope.write(data);
            }
            2 => {
                self.short_mode = data & 0x80 != 0;
                // a clock every `periods[n]` CPU cycles
                self.timer.period = self.periods()[data as usize & 0x0F] - 1;
            }
            3 => {
                self.length.load(data >> 3);
                self.envelope.restart();
            }
            _ => {}
        }
    }

    /// Commits this cycle's length counter writes; called by the APU at the end of every CPU cycle.
    pub fn end_cycle(&mut self) {
        self.length.end_cycle();
    }

    /// A register write followed by the end of the cycle, for tests that want its effect right away.
    #[cfg(test)]
    pub fn write_now(&mut self, register: u16, data: u8) {
        self.write(register, data);
        self.end_cycle();
    }

    /// Called every CPU cycle.
    pub fn tick(&mut self) {
        if self.timer.tick() {
            let tap = if self.short_mode { 6 } else { 1 };
            let feedback = (self.lfsr & 1) ^ ((self.lfsr >> tap) & 1);
            self.lfsr = (self.lfsr >> 1) | (feedback << 14);
        }
    }

    pub fn clock_quarter_frame(&mut self) {
        self.envelope.clock();
    }

    pub fn clock_half_frame(&mut self) {
        self.length.clock();
    }

    /// 0-15.
    pub fn output(&self) -> u8 {
        if self.lfsr & 1 != 0 || !self.length.active() { 0 } else { self.envelope.output() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(register_e: u8) -> Noise {
        let mut noise = Noise::default();
        noise.length.set_enabled(true);
        noise.write_now(0, 0x3F); // halt, constant volume 15
        noise.write_now(2, register_e);
        noise.write_now(3, 0x08);
        noise
    }

    #[test]
    fn the_shift_register_starts_at_one_and_clocks_every_period_cycles() {
        let mut noise = noise(0x00); // period 4
        assert_eq!(noise.lfsr, 1);

        for _ in 0..4 {
            noise.tick();
        }
        assert_eq!(noise.lfsr, 0x4000, "bit 0 and bit 1 differ: 1 is fed into bit 14");
        for _ in 0..4 {
            noise.tick();
        }
        assert_eq!(noise.lfsr, 0x2000);
    }

    #[test]
    fn long_mode_repeats_after_32767_steps() {
        let mut noise = noise(0x00);
        let start = noise.lfsr;
        let mut steps = 0u32;
        loop {
            noise.lfsr = {
                let feedback = (noise.lfsr & 1) ^ ((noise.lfsr >> 1) & 1);
                (noise.lfsr >> 1) | (feedback << 14)
            };
            steps += 1;
            if noise.lfsr == start {
                break;
            }
        }

        assert_eq!(steps, 32767);
    }

    #[test]
    fn short_mode_repeats_after_a_short_sequence() {
        let mut noise = noise(0x80); // mode 1
        noise.lfsr = 0x7FFF;
        let start = noise.lfsr;
        let mut steps = 0;
        loop {
            let feedback = (noise.lfsr & 1) ^ ((noise.lfsr >> 6) & 1);
            noise.lfsr = (noise.lfsr >> 1) | (feedback << 14);
            steps += 1;
            if noise.lfsr == start || steps > 40_000 {
                break;
            }
        }

        assert!(steps == 93 || steps == 31, "short sequence is 93 or 31 steps, got {steps}");
    }

    #[test]
    fn output_is_silent_when_lfsr_bit_0_is_set_or_the_length_counter_is_zero() {
        let mut noise = noise(0x00);
        noise.lfsr = 0b10;
        assert_eq!(noise.output(), 15);

        noise.lfsr = 0b11;
        assert_eq!(noise.output(), 0);

        let mut silent = Noise::default(); // length counter never loaded
        silent.write_now(0, 0x3F);
        silent.lfsr = 0b10;
        assert_eq!(silent.output(), 0);
    }

    #[test]
    fn pal_uses_its_own_period_table() {
        let mut pal = Noise::new(Region::Pal);
        pal.write(2, 0x02);
        assert_eq!(pal.timer.period, 13, "PAL index 2 is 14 CPU cycles");

        pal.write(2, 0x0F);
        assert_eq!(pal.timer.period, 3777);

        let mut ntsc = Noise::new(Region::Ntsc);
        ntsc.write(2, 0x02);
        assert_eq!(ntsc.timer.period, 15);
    }

    #[test]
    fn period_table_is_applied() {
        let mut noise = noise(0x0F);
        assert_eq!(noise.timer.period, 4067);

        noise.write_now(2, 0x03);
        assert_eq!(noise.timer.period, 31);
    }
}
