//! The expansion sound of the Sunsoft 5B: a Yamaha YM2149F, a compatible of the General Instrument
//! AY-3-8910, with three square-wave channels, a noise generator and an envelope generator. It sits on the FME-7
//! board (mapper 69): $C000-$DFFF selects a register, $E000-$FFFF writes it.
//! https://www.nesdev.org/wiki/Sunsoft_5B_audio

/// The chip runs from the CPU clock divided by 2 and divides that by 8 again for its tone counters, so a
/// square-wave half period of `P` counts takes `16 * P` CPU cycles.
const CPU_CYCLES_PER_STEP: u8 = 16;
/// Output of the three channels at full volume together, on the APU mixer's scale.
const OUTPUT_SCALE: f32 = 0.1;

/// Relative amplitude for volume levels 0-15 (logarithmic DAC).
const VOLUME_TABLE: [f32; 16] = [
    0.0, 0.0100, 0.0145, 0.0211, 0.0307, 0.0455, 0.0645, 0.1073, 0.1266, 0.2050, 0.2922, 0.3728, 0.4925, 0.6353,
    0.8056, 1.0,
];

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Sunsoft5bAudio {
    selected: u8,
    registers: [u8; 16],
    /// CPU cycles since the last chip step.
    prescaler: u8,
    tone_timer: [u16; 3],
    tone_high: [bool; 3],
    noise_timer: u16,
    noise_shift: u32,
    envelope_timer: u32,
    envelope_level: u8,
    envelope_rising: bool,
    envelope_holding: bool,
    /// The summed channel output, recomputed whenever something that influences it changes.
    level: f32,
}

impl Default for Sunsoft5bAudio {
    fn default() -> Self {
        Sunsoft5bAudio {
            selected: 0,
            registers: [0; 16],
            prescaler: 0,
            tone_timer: [0; 3],
            tone_high: [false; 3],
            noise_timer: 0,
            noise_shift: 1,
            envelope_timer: 0,
            envelope_level: 0,
            envelope_rising: false,
            envelope_holding: true,
            level: 0.0,
        }
    }
}

impl Sunsoft5bAudio {
    pub fn select(&mut self, register: u8) {
        self.selected = register & 0x0F;
    }

    pub fn write(&mut self, data: u8) {
        self.registers[self.selected as usize] = data;

        if self.selected == 13 {
            // writing the shape restarts the envelope
            self.envelope_rising = data & 0x04 != 0;
            self.envelope_level = if self.envelope_rising { 0 } else { 15 };
            self.envelope_holding = false;
            self.envelope_timer = 0;
        }
        self.level = self.mix();
    }

    fn tone_period(&self, channel: usize) -> u16 {
        let period = self.registers[channel * 2] as u16 | (self.registers[channel * 2 + 1] as u16 & 0x0F) << 8;
        period.max(1)
    }

    fn noise_period(&self) -> u16 {
        (self.registers[6] as u16 & 0x1F).max(1)
    }

    fn envelope_period(&self) -> u32 {
        (self.registers[11] as u32 | (self.registers[12] as u32) << 8).max(1)
    }

    fn envelope_step(&mut self) {
        if self.envelope_holding {
            return;
        }

        let shape = self.registers[13];
        let (cont, alternate, hold) = (shape & 0x08 != 0, shape & 0x02 != 0, shape & 0x01 != 0);
        let at_end = if self.envelope_rising { self.envelope_level == 15 } else { self.envelope_level == 0 };

        if !at_end {
            if self.envelope_rising {
                self.envelope_level += 1;
            } else {
                self.envelope_level -= 1;
            }
            return;
        }

        // one 16-step ramp is over
        if !cont {
            self.envelope_level = 0;
            self.envelope_holding = true;
        } else if hold {
            self.envelope_holding = true;
            if alternate {
                self.envelope_level = if self.envelope_rising { 0 } else { 15 };
            }
        } else if alternate {
            self.envelope_rising = !self.envelope_rising;
        } else {
            self.envelope_level = if self.envelope_rising { 0 } else { 15 };
        }
    }

    fn mix(&self) -> f32 {
        let mixer = self.registers[7];
        let noise_high = self.noise_shift & 1 != 0;
        let mut sum = 0.0;

        for channel in 0..3 {
            let tone_ok = self.tone_high[channel] || mixer & (1 << channel) != 0;
            let noise_ok = noise_high || mixer & (8 << channel) != 0;
            if tone_ok && noise_ok {
                let amplitude = self.registers[8 + channel];
                let volume = if amplitude & 0x10 != 0 { self.envelope_level } else { amplitude & 0x0F };
                sum += VOLUME_TABLE[volume as usize];
            }
        }

        sum * OUTPUT_SCALE
    }

    /// Once per CPU cycle.
    pub fn tick(&mut self) {
        self.prescaler += 1;
        if self.prescaler < CPU_CYCLES_PER_STEP {
            return;
        }
        self.prescaler = 0;

        for channel in 0..3 {
            self.tone_timer[channel] += 1;
            if self.tone_timer[channel] >= self.tone_period(channel) {
                self.tone_timer[channel] = 0;
                self.tone_high[channel] = !self.tone_high[channel];
            }
        }

        // the noise shift register (17 bits, taps 0 and 3) steps every second noise period
        self.noise_timer += 1;
        if self.noise_timer >= self.noise_period() * 2 {
            self.noise_timer = 0;
            let feedback = (self.noise_shift ^ (self.noise_shift >> 3)) & 1;
            self.noise_shift = (self.noise_shift >> 1) | (feedback << 16);
        }

        self.envelope_timer += 1;
        if self.envelope_timer >= self.envelope_period() * 2 {
            self.envelope_timer = 0;
            self.envelope_step();
        }

        self.level = self.mix();
    }

    pub fn output(&self) -> f32 {
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(chip: &mut Sunsoft5bAudio, register: u8, data: u8) {
        chip.select(register);
        chip.write(data);
    }

    /// A chip with channel A playing a tone of the given period at full volume (noise and the other channels off).
    fn tone(period: u16) -> Sunsoft5bAudio {
        let mut chip = Sunsoft5bAudio::default();
        write(&mut chip, 0, period as u8);
        write(&mut chip, 1, (period >> 8) as u8);
        write(&mut chip, 7, 0b0011_1110); // tone A on, everything else off
        write(&mut chip, 8, 0x0F);
        chip
    }

    fn run(chip: &mut Sunsoft5bAudio, cycles: usize) -> Vec<f32> {
        (0..cycles)
            .map(|_| {
                chip.tick();
                chip.output()
            })
            .collect()
    }

    #[test]
    fn a_tone_toggles_every_period_chip_steps() {
        let mut chip = tone(3);
        let samples = run(&mut chip, 16 * 3 * 8);

        let edges = samples.windows(2).filter(|w| w[0] != w[1]).count();
        assert_eq!(edges, 8, "a half period is 3 steps of 16 CPU cycles");
        assert!(samples.iter().any(|&s| (s - OUTPUT_SCALE).abs() < 1e-6), "full volume = one channel's share");
    }

    #[test]
    fn the_volume_register_scales_the_output_logarithmically() {
        let mut quiet = tone(1);
        write(&mut quiet, 8, 0x08);
        let mut loud = tone(1);

        let quiet_peak = run(&mut quiet, 200).into_iter().fold(0.0, f32::max);
        let loud_peak = run(&mut loud, 200).into_iter().fold(0.0, f32::max);

        assert!((quiet_peak / loud_peak - VOLUME_TABLE[8]).abs() < 1e-4);
    }

    #[test]
    fn a_channel_with_both_tone_and_noise_disabled_is_a_constant_level() {
        let mut chip = Sunsoft5bAudio::default();
        write(&mut chip, 7, 0b0011_1111);
        write(&mut chip, 8, 0x0F);

        let samples = run(&mut chip, 100);

        assert!(samples.iter().all(|&s| (s - OUTPUT_SCALE).abs() < 1e-6));
    }

    #[test]
    fn noise_makes_the_output_jump_around_pseudo_randomly() {
        let mut chip = Sunsoft5bAudio::default();
        write(&mut chip, 6, 1); // fastest noise
        write(&mut chip, 7, 0b0000_0111); // noise A on, tone off (disabled = always high)
        write(&mut chip, 8, 0x0F);

        let samples = run(&mut chip, 16 * 2 * 200);
        let edges = samples.windows(2).filter(|w| w[0] != w[1]).count();

        assert!(edges > 50 && edges < 300, "{edges} edges");
    }

    #[test]
    fn a_falling_envelope_ramps_down_in_16_steps_and_holds_at_silence() {
        let mut chip = Sunsoft5bAudio::default();
        write(&mut chip, 7, 0b0011_1111); // constant level
        write(&mut chip, 8, 0x10); // envelope controls the volume
        write(&mut chip, 11, 1); // period 1: a step every 2 chip steps
        write(&mut chip, 13, 0b0000_0000); // shape 0: fall once, then silence

        let samples = run(&mut chip, 16 * 2 * 40);
        let mut levels: Vec<f32> = samples.clone();
        levels.dedup();

        assert_eq!(levels.len(), 16, "15 -> 0 in 16 distinct levels");
        assert!(levels.windows(2).all(|w| w[0] > w[1]));
        assert_eq!(*samples.last().unwrap(), 0.0);
    }

    #[test]
    fn a_repeating_sawtooth_envelope_keeps_running() {
        let mut chip = Sunsoft5bAudio::default();
        write(&mut chip, 7, 0b0011_1111);
        write(&mut chip, 8, 0x10);
        write(&mut chip, 11, 1);
        write(&mut chip, 13, 0b0000_1000); // shape 8: repeated falling ramp

        let samples = run(&mut chip, 16 * 2 * 100);

        assert!(samples[samples.len() - 16 * 2 * 20..].iter().any(|&s| s > 0.05), "still ramping at the end");
        assert!(samples.contains(&0.0));
    }

    #[test]
    fn only_the_low_nibble_selects_the_register() {
        let mut chip = Sunsoft5bAudio::default();
        chip.select(0xF8);
        chip.write(0x0F);

        assert_eq!(chip.registers[8], 0x0F);
    }
}
