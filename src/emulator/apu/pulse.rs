//! Pulse channels 1 ($4000-$4003) and 2 ($4004-$4007). https://www.nesdev.org/wiki/APU_Pulse

use super::units::{Envelope, LengthCounter, Timer};

/// Output waveforms; the sequencer index counts down and reads them from the front.
const DUTY: [[u8; 8]; 4] = [
    [0, 1, 0, 0, 0, 0, 0, 0],
    [0, 1, 1, 0, 0, 0, 0, 0],
    [0, 1, 1, 1, 1, 0, 0, 0],
    [1, 0, 0, 1, 1, 1, 1, 1],
];

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct Sweep {
    enabled: bool,
    period: u8,
    negate: bool,
    shift: u8,
    divider: u8,
    reload: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Pulse {
    /// Pulse 1 negates the sweep with one's complement, pulse 2 with two's complement.
    ones_complement: bool,
    duty: usize,
    duty_position: usize,
    /// The 11-bit period register `t`; the waveform advances every `t + 1` APU cycles.
    raw_period: u16,
    timer: Timer,
    pub envelope: Envelope,
    pub length: LengthCounter,
    sweep: Sweep,
}

impl Pulse {
    pub fn new(ones_complement: bool) -> Self {
        Pulse {
            ones_complement,
            duty: 0,
            duty_position: 0,
            raw_period: 0,
            timer: Timer::default(),
            envelope: Envelope::default(),
            length: LengthCounter::default(),
            sweep: Sweep::default(),
        }
    }

    /// `register` is 0-3 (the offset from $4000 or $4004).
    pub fn write(&mut self, register: u16, data: u8) {
        match register {
            0 => {
                self.duty = (data >> 6) as usize;
                self.length.set_halt(data & 0x20 != 0);
                self.envelope.write(data);
            }
            1 => {
                self.sweep.enabled = data & 0x80 != 0;
                self.sweep.period = (data >> 4) & 0x07;
                self.sweep.negate = data & 0x08 != 0;
                self.sweep.shift = data & 0x07;
                self.sweep.reload = true;
            }
            2 => self.set_period((self.raw_period & 0x0700) | data as u16),
            3 => {
                self.set_period((self.raw_period & 0x00FF) | ((data as u16 & 0x07) << 8));
                self.length.load(data >> 3);
                self.duty_position = 0;
                self.envelope.restart();
            }
            _ => {}
        }
    }

    fn set_period(&mut self, period: u16) {
        self.raw_period = period;
        // the timer steps once per APU cycle = 2 CPU cycles, and fires after `period` further steps
        self.timer.period = period * 2 + 1;
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
            self.duty_position = (self.duty_position + 7) & 7;
        }
    }

    pub fn clock_quarter_frame(&mut self) {
        self.envelope.clock();
    }

    pub fn clock_half_frame(&mut self) {
        self.length.clock();
        self.clock_sweep();
    }

    fn target_period(&self) -> i32 {
        let change = (self.raw_period >> self.sweep.shift) as i32;
        let change = if self.sweep.negate {
            if self.ones_complement { -change - 1 } else { -change }
        } else {
            change
        };
        (self.raw_period as i32 + change).max(0)
    }

    fn sweep_mutes(&self) -> bool {
        self.raw_period < 8 || self.target_period() > 0x7FF
    }

    fn clock_sweep(&mut self) {
        if self.sweep.divider == 0 && self.sweep.enabled && self.sweep.shift != 0 && !self.sweep_mutes() {
            let target = self.target_period() as u16;
            self.set_period(target);
        }

        if self.sweep.divider == 0 || self.sweep.reload {
            self.sweep.divider = self.sweep.period;
            self.sweep.reload = false;
        } else {
            self.sweep.divider -= 1;
        }
    }

    /// 0-15.
    pub fn output(&self) -> u8 {
        if !self.length.active() || self.sweep_mutes() || DUTY[self.duty][self.duty_position] == 0 {
            0
        } else {
            self.envelope.output()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_pulse(ones_complement: bool) -> Pulse {
        let mut pulse = Pulse::new(ones_complement);
        pulse.length.set_enabled(true);
        pulse
    }

    /// Output of each of `steps` consecutive waveform steps (one step lasts 2 * (t + 1) CPU cycles).
    fn waveform(pulse: &mut Pulse, period: u16, steps: usize) -> Vec<u8> {
        let cycles_per_step = 2 * (period as usize + 1);
        let mut out = Vec::new();
        for _ in 0..steps {
            out.push(pulse.output());
            for _ in 0..cycles_per_step {
                pulse.tick();
            }
        }
        out
    }

    #[test]
    fn duty_cycles_produce_the_documented_waveforms() {
        for (duty, expected) in [
            (0u8, [0, 1, 0, 0, 0, 0, 0, 0]),
            (1, [0, 1, 1, 0, 0, 0, 0, 0]),
            (2, [0, 1, 1, 1, 1, 0, 0, 0]),
            (3, [1, 0, 0, 1, 1, 1, 1, 1]),
        ] {
            let mut pulse = enabled_pulse(true);
            pulse.write_now(0, duty << 6 | 0x10 | 15); // constant volume 15
            pulse.write_now(2, 0x20);
            pulse.write_now(3, 0x08); // length index 1 (254), period 0x020

            let steps = waveform(&mut pulse, 0x20, 16);
            let high: Vec<u8> = steps.iter().map(|&v| (v != 0) as u8).collect();

            // the sequencer index counts down, so the pattern appears in the order 0, 7, 6, 5, 4, 3, 2, 1
            let mut order = vec![expected[0]];
            order.extend((1..8).rev().map(|i| expected[i]));
            assert_eq!(&high[..8], &order[..], "duty {duty}");
            assert_eq!(&high[8..], &order[..], "pattern repeats, duty {duty}");
        }
    }

    #[test]
    fn output_is_the_envelope_volume_when_the_waveform_is_high() {
        let mut pulse = enabled_pulse(true);
        pulse.write_now(0, 0b1011_1010); // duty 2, halt, constant volume 10
        pulse.write_now(2, 0x40);
        pulse.write_now(3, 0x08);

        let steps = waveform(&mut pulse, 0x40, 8);

        assert!(steps.iter().all(|&v| v == 0 || v == 10));
        assert!(steps.contains(&10));
    }

    #[test]
    fn short_periods_are_muted() {
        let mut pulse = enabled_pulse(true);
        pulse.write_now(0, 0x9F);
        pulse.write_now(2, 7); // period 7 < 8
        pulse.write_now(3, 0x08);

        assert!(waveform(&mut pulse, 7, 16).iter().all(|&v| v == 0));
    }

    #[test]
    fn a_zero_length_counter_silences_the_channel() {
        let mut pulse = Pulse::new(true);
        pulse.length.set_enabled(false); // disabled through $4015
        pulse.write_now(0, 0xBF);
        pulse.write_now(2, 0x40);
        pulse.write_now(3, 0x08);

        assert!(waveform(&mut pulse, 0x40, 16).iter().all(|&v| v == 0));
    }

    #[test]
    fn sweep_negate_differs_between_the_two_pulse_channels() {
        // period 0x100, shift 1, negate: change = 0x80
        let mut pulse1 = enabled_pulse(true);
        let mut pulse2 = enabled_pulse(false);
        for pulse in [&mut pulse1, &mut pulse2] {
            pulse.write_now(2, 0x00);
            pulse.write_now(3, 0x09); // high bits 1 -> period 0x100
            pulse.write_now(1, 0b1000_1001); // enabled, divider period 0, negate, shift 1
        }

        assert_eq!(pulse1.target_period(), 0x100 - 0x80 - 1, "pulse 1: one's complement");
        assert_eq!(pulse2.target_period(), 0x100 - 0x80, "pulse 2: two's complement");
    }

    #[test]
    fn sweep_updates_the_period_on_half_frame_clocks() {
        let mut pulse = enabled_pulse(false);
        pulse.write_now(2, 0x00);
        pulse.write_now(3, 0x09); // period 0x100
        pulse.write_now(1, 0b1000_0001); // enabled, period 0, add, shift 1

        pulse.clock_half_frame(); // the first clock only reloads the divider...
        assert_eq!(pulse.raw_period, 0x100 + 0x80, "...but with divider 0 the period changes right away");
        pulse.clock_half_frame();
        assert_eq!(pulse.raw_period, 0x180 + 0xC0);
    }

    #[test]
    fn sweep_overflow_mutes_even_when_the_sweep_is_disabled() {
        let mut pulse = enabled_pulse(true);
        pulse.write_now(0, 0xBF);
        pulse.write_now(2, 0xFF);
        pulse.write_now(3, 0x0F); // period 0x7FF
        pulse.write_now(1, 0x00); // sweep disabled, shift 0: target = 2 * period > 0x7FF

        assert!(waveform(&mut pulse, 0x7FF, 4).iter().all(|&v| v == 0));
    }

    #[test]
    fn writing_the_high_period_register_restarts_the_sequencer_and_envelope() {
        let mut pulse = enabled_pulse(true);
        pulse.write_now(0, 0x00);
        pulse.write_now(2, 0x40);
        pulse.write_now(3, 0x08);
        for _ in 0..900 {
            pulse.tick();
        }
        assert_ne!(pulse.duty_position, 0);

        pulse.write_now(3, 0x08);

        assert_eq!(pulse.duty_position, 0);
        pulse.clock_quarter_frame();
        assert_eq!(pulse.envelope.output(), 15, "envelope restarted");
    }
}
