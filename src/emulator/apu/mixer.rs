//! Mixing of the five channels and the console's analog output filters.
//! https://www.nesdev.org/wiki/APU_Mixer

use std::f32::consts::PI;

/// The nonlinear DAC curves, as lookup tables indexed by the sums of the channel outputs.
pub struct Mixer {
    pulse_table: [f32; 31],
    tnd_table: [f32; 203],
}

impl Default for Mixer {
    fn default() -> Self {
        let mut pulse_table = [0.0; 31];
        for (n, value) in pulse_table.iter_mut().enumerate().skip(1) {
            *value = 95.52 / (8128.0 / n as f32 + 100.0);
        }

        let mut tnd_table = [0.0; 203];
        for (n, value) in tnd_table.iter_mut().enumerate().skip(1) {
            *value = 163.67 / (24329.0 / n as f32 + 100.0);
        }

        Mixer { pulse_table, tnd_table }
    }
}

impl Mixer {
    /// Pulse, triangle and noise are 0-15, the DMC level 0-127. The result is roughly 0.0 to 1.0.
    pub fn mix(&self, pulse1: u8, pulse2: u8, triangle: u8, noise: u8, dmc: u8) -> f32 {
        let pulse = self.pulse_table[(pulse1 + pulse2) as usize];
        let tnd = self.tnd_table[(3 * triangle as usize + 2 * noise as usize + dmc as usize).min(202)];
        pulse + tnd
    }
}

/// A first-order filter.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
enum Kind {
    HighPass { alpha: f32 },
    LowPass { alpha: f32 },
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
struct Filter {
    kind: Kind,
    previous_input: f32,
    previous_output: f32,
}

impl Filter {
    fn high_pass(cutoff_hz: f32, sample_rate: f32) -> Self {
        let rc = 1.0 / (2.0 * PI * cutoff_hz);
        let dt = 1.0 / sample_rate;
        Filter { kind: Kind::HighPass { alpha: rc / (rc + dt) }, previous_input: 0.0, previous_output: 0.0 }
    }

    fn low_pass(cutoff_hz: f32, sample_rate: f32) -> Self {
        let rc = 1.0 / (2.0 * PI * cutoff_hz);
        let dt = 1.0 / sample_rate;
        Filter { kind: Kind::LowPass { alpha: dt / (rc + dt) }, previous_input: 0.0, previous_output: 0.0 }
    }

    fn apply(&mut self, input: f32) -> f32 {
        let output = match self.kind {
            Kind::HighPass { alpha } => alpha * (self.previous_output + input - self.previous_input),
            Kind::LowPass { alpha } => self.previous_output + alpha * (input - self.previous_output),
        };
        self.previous_input = input;
        self.previous_output = output;
        output
    }
}

/// The console's output stage: two high-pass filters (90 Hz and 440 Hz) and a 14 kHz low-pass filter,
/// which remove the DC offset and the harshest highs. Runs at the output sample rate.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct OutputFilter {
    stages: [Filter; 3],
}

impl OutputFilter {
    pub fn new(sample_rate: f32) -> Self {
        OutputFilter {
            stages: [
                Filter::high_pass(90.0, sample_rate),
                Filter::high_pass(440.0, sample_rate),
                // keep the low-pass below Nyquist for low output rates
                Filter::low_pass(14_000.0_f32.min(sample_rate * 0.45), sample_rate),
            ],
        }
    }

    pub fn apply(&mut self, sample: f32) -> f32 {
        self.stages.iter_mut().fold(sample, |value, stage| stage.apply(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_mixes_to_zero() {
        assert_eq!(Mixer::default().mix(0, 0, 0, 0, 0), 0.0);
    }

    #[test]
    fn mixing_is_nonlinear_and_below_one() {
        let mixer = Mixer::default();
        let one_pulse = mixer.mix(15, 0, 0, 0, 0);
        let two_pulses = mixer.mix(15, 15, 0, 0, 0);

        assert!(two_pulses > one_pulse);
        assert!(two_pulses < 2.0 * one_pulse, "two pulses are quieter than twice one pulse");
        assert!(mixer.mix(15, 15, 15, 15, 127) < 1.1);
        assert!((one_pulse - 95.52 / (8128.0 / 15.0 + 100.0)).abs() < 1e-6);
    }

    #[test]
    fn the_extreme_inputs_stay_inside_the_tables() {
        let mixer = Mixer::default();

        assert!(mixer.mix(15, 15, 15, 15, 127).is_finite());
    }

    #[test]
    fn the_high_pass_filters_remove_a_constant_level() {
        let mut filter = OutputFilter::new(48_000.0);
        let mut last = 1.0;
        for _ in 0..48_000 {
            last = filter.apply(0.5);
        }

        assert!(last.abs() < 0.001, "DC offset remains: {last}");
    }

    #[test]
    fn midrange_tones_pass_through_mostly_unchanged() {
        let sample_rate = 48_000.0;
        let mut filter = OutputFilter::new(sample_rate);
        let frequency = 2_000.0;

        let mut peak: f32 = 0.0;
        for n in 0..48_000 {
            let input = (2.0 * PI * frequency * n as f32 / sample_rate).sin();
            let output = filter.apply(input);
            if n > 24_000 {
                peak = peak.max(output.abs());
            }
        }

        assert!(peak > 0.9 && peak < 1.01, "2 kHz peak after filtering: {peak}");
    }
}
