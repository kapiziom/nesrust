//! The FM sound of the Konami VRC7: a cut-down Yamaha YM2413 (OPLL) with six channels of two
//! operators each, 15 built-in instruments and one the game programs. This is a floating-point model of the chip
//! following its documented behaviour (phase generator, ADSR envelope with key scaling, feedback, tremolo and
//! vibrato), not a bit-exact copy of the hardware's integer pipeline.
//! https://www.nesdev.org/wiki/VRC7_audio

/// The chip produces one sample (all six channels) every 36 CPU cycles: 49716 Hz.
const CYCLES_PER_SAMPLE: u8 = 36;
const SAMPLE_RATE: f32 = 49_716.0;
/// Output of all six channels at full volume together, on the APU mixer's scale.
const OUTPUT_SCALE: f32 = 0.075;
/// The envelope generator covers 48 dB (7 bits of 0.375 dB).
const SILENCE_DB: f32 = 48.0;

/// The built-in instruments (1-15) of the VRC7's version of the chip, as eight register bytes each.
const PATCHES: [[u8; 8]; 15] = [
    [0x03, 0x21, 0x05, 0x06, 0xE8, 0x81, 0x42, 0x27],
    [0x13, 0x41, 0x14, 0x0D, 0xD8, 0xF6, 0x23, 0x12],
    [0x11, 0x11, 0x08, 0x08, 0xFA, 0xB2, 0x20, 0x12],
    [0x31, 0x61, 0x0C, 0x07, 0xA8, 0x64, 0x61, 0x27],
    [0x32, 0x21, 0x1E, 0x06, 0xE1, 0x76, 0x01, 0x28],
    [0x02, 0x01, 0x06, 0x00, 0xA3, 0xE2, 0xF4, 0xF4],
    [0x21, 0x61, 0x1D, 0x07, 0x82, 0x81, 0x11, 0x07],
    [0x23, 0x21, 0x22, 0x17, 0xA2, 0x72, 0x01, 0x17],
    [0x35, 0x11, 0x25, 0x00, 0x40, 0x73, 0x72, 0x01],
    [0xB5, 0x01, 0x0F, 0x0F, 0xA8, 0xA5, 0x51, 0x02],
    [0x17, 0xC1, 0x24, 0x07, 0xF8, 0xF8, 0x22, 0x12],
    [0x71, 0x23, 0x11, 0x06, 0x65, 0x74, 0x18, 0x16],
    [0x01, 0x02, 0xD3, 0x05, 0xC9, 0x95, 0x03, 0x02],
    [0x61, 0x63, 0x0C, 0x00, 0x94, 0xC0, 0x33, 0xF6],
    [0x21, 0x72, 0x0D, 0x00, 0xC1, 0xD5, 0x56, 0x06],
];

/// Frequency multiplier of the ML field.
const MULTIPLIER: [f32; 16] = [0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 10.0, 12.0, 12.0, 15.0, 15.0];

/// Key scale level: attenuation in 0.75 dB units by the top four bits of the F-number, before the octave correction.
const KSL_BASE: [i32; 16] = [0, 32, 40, 45, 48, 51, 53, 55, 56, 58, 59, 60, 61, 62, 63, 64];

/// Time in milliseconds for an envelope to run through its whole range at rate 1; every rate step doubles the speed.
const ATTACK_BASE_MS: f32 = 2826.0;
const DECAY_BASE_MS: f32 = 39_280.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum Stage {
    Attack,
    Decay,
    Sustain,
    Release,
}

/// Which parameters of the instrument a slot reads (a modulator or a carrier).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Modulator,
    Carrier,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Slot {
    phase: f32,
    stage: Stage,
    /// Attenuation of the envelope in dB (0 = loudest, `SILENCE_DB` = silent).
    attenuation: f32,
    /// Position inside the tremolo-free sample just computed, kept for the feedback of the modulator.
    previous_output: [f32; 2],
    output: f32,
}

impl Slot {
    fn new() -> Self {
        Slot { phase: 0.0, stage: Stage::Release, attenuation: SILENCE_DB, previous_output: [0.0; 2], output: 0.0 }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Channel {
    fnum: u16,
    block: u8,
    key_on: bool,
    sustain: bool,
    instrument: u8,
    volume: u8,
    modulator: Slot,
    carrier: Slot,
}

impl Channel {
    fn new() -> Self {
        Channel {
            fnum: 0,
            block: 0,
            key_on: false,
            sustain: false,
            instrument: 0,
            volume: 0,
            modulator: Slot::new(),
            carrier: Slot::new(),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Opll {
    selected: u8,
    custom_patch: [u8; 8],
    channels: Vec<Channel>,
    prescaler: u8,
    /// Phases of the tremolo (3.7 Hz) and vibrato (6.4 Hz) oscillators, in cycles.
    tremolo_phase: f32,
    vibrato_phase: f32,
    level: f32,
}

impl Default for Opll {
    fn default() -> Self {
        Opll {
            selected: 0,
            custom_patch: [0; 8],
            channels: (0..6).map(|_| Channel::new()).collect(),
            prescaler: 0,
            tremolo_phase: 0.0,
            vibrato_phase: 0.0,
            level: 0.0,
        }
    }
}

impl Opll {
    pub fn select(&mut self, register: u8) {
        self.selected = register;
    }

    pub fn write(&mut self, data: u8) {
        match self.selected {
            0x00..=0x07 => self.custom_patch[self.selected as usize] = data,
            0x10..=0x15 => {
                let channel = &mut self.channels[(self.selected & 0x0F) as usize];
                channel.fnum = (channel.fnum & 0x100) | data as u16;
            }
            0x20..=0x25 => {
                let channel = &mut self.channels[(self.selected & 0x0F) as usize];
                channel.fnum = (channel.fnum & 0x0FF) | ((data as u16 & 1) << 8);
                channel.block = (data >> 1) & 7;
                channel.sustain = data & 0x20 != 0;

                let key_on = data & 0x10 != 0;
                if key_on && !channel.key_on {
                    Self::key_on(channel);
                } else if !key_on && channel.key_on {
                    channel.modulator.stage = Stage::Release;
                    channel.carrier.stage = Stage::Release;
                }
                channel.key_on = key_on;
            }
            0x30..=0x35 => {
                let channel = &mut self.channels[(self.selected & 0x0F) as usize];
                channel.instrument = data >> 4;
                channel.volume = data & 0x0F;
            }
            _ => {}
        }
    }

    fn key_on(channel: &mut Channel) {
        for slot in [&mut channel.modulator, &mut channel.carrier] {
            slot.phase = 0.0;
            slot.stage = Stage::Attack;
            slot.previous_output = [0.0; 2];
        }
    }

    fn patch(&self, instrument: u8) -> [u8; 8] {
        if instrument == 0 {
            self.custom_patch
        } else {
            PATCHES[(instrument - 1) as usize]
        }
    }

    /// Once per CPU cycle.
    pub fn tick(&mut self) {
        self.prescaler += 1;
        if self.prescaler < CYCLES_PER_SAMPLE {
            return;
        }
        self.prescaler = 0;
        self.level = self.sample() * OUTPUT_SCALE;
    }

    pub fn output(&self) -> f32 {
        self.level
    }

    /// Computes one sample of all six channels, -6..6.
    fn sample(&mut self) -> f32 {
        self.tremolo_phase = (self.tremolo_phase + 3.7 / SAMPLE_RATE).fract();
        self.vibrato_phase = (self.vibrato_phase + 6.4 / SAMPLE_RATE).fract();
        // triangle waves
        let tremolo = 1.0 - (self.tremolo_phase * 2.0 - 1.0).abs(); // 0..1
        let vibrato = (self.vibrato_phase * 2.0 - 1.0).abs() * 2.0 - 1.0; // -1..1

        let patches: Vec<[u8; 8]> = self.channels.iter().map(|channel| self.patch(channel.instrument)).collect();
        let mut sum = 0.0;

        for (channel, patch) in self.channels.iter_mut().zip(patches) {
            let (modulator_out, carrier_out) = Self::run_channel(channel, &patch, tremolo, vibrato);
            let _ = modulator_out;
            sum += carrier_out;
        }

        sum
    }

    fn run_channel(channel: &mut Channel, patch: &[u8; 8], tremolo: f32, vibrato: f32) -> (f32, f32) {
        let block = channel.block;
        let (key_on, sustain) = (channel.key_on, channel.sustain);
        let key_scale_index = ((block as u32) << 1 | (channel.fnum >> 8) as u32) as f32;
        let base_step = channel.fnum as f32 * (1u32 << block) as f32 / 524_288.0;

        let mod_byte = patch[0];
        let car_byte = patch[1];
        let feedback = patch[3] & 7;
        let modulator_wave_rectified = patch[3] & 0x08 != 0;
        let carrier_wave_rectified = patch[3] & 0x10 != 0;

        // modulator
        Self::advance_envelope(&mut channel.modulator, Role::Modulator, patch, (key_on, sustain), key_scale_index);
        let modulator_attenuation = {
            let tl = (patch[2] & 0x3F) as f32 * 0.75;
            let ksl = Self::key_scale_level(channel.fnum, block, patch[2] >> 6);
            let am = if mod_byte & 0x80 != 0 { tremolo * 4.875 } else { 0.0 };
            channel.modulator.attenuation + tl + ksl + am
        };
        let modulator_step =
            base_step * MULTIPLIER[(mod_byte & 0x0F) as usize] * Self::vibrato_factor(mod_byte, vibrato);
        channel.modulator.phase = (channel.modulator.phase + modulator_step).fract();
        let feedback_offset = if feedback == 0 {
            0.0
        } else {
            let average = (channel.modulator.previous_output[0] + channel.modulator.previous_output[1]) / 2.0;
            average * (1u32 << feedback) as f32 / 128.0
        };
        let modulator_out =
            Self::operator(channel.modulator.phase + feedback_offset, modulator_attenuation, modulator_wave_rectified);
        channel.modulator.previous_output = [channel.modulator.previous_output[1], modulator_out];
        channel.modulator.output = modulator_out;

        // carrier
        Self::advance_envelope(&mut channel.carrier, Role::Carrier, patch, (key_on, sustain), key_scale_index);
        let carrier_attenuation = {
            let volume = channel.volume as f32 * 3.0;
            let ksl = Self::key_scale_level(channel.fnum, block, patch[3] >> 6);
            let am = if car_byte & 0x80 != 0 { tremolo * 4.875 } else { 0.0 };
            channel.carrier.attenuation + volume + ksl + am
        };
        let carrier_step = base_step * MULTIPLIER[(car_byte & 0x0F) as usize] * Self::vibrato_factor(car_byte, vibrato);
        channel.carrier.phase = (channel.carrier.phase + carrier_step).fract();
        // the modulator output moves the carrier's phase by up to +-2 cycles
        let carrier_out =
            Self::operator(channel.carrier.phase + modulator_out * 2.0, carrier_attenuation, carrier_wave_rectified);
        channel.carrier.output = carrier_out;

        (modulator_out, carrier_out)
    }

    fn vibrato_factor(byte: u8, vibrato: f32) -> f32 {
        // +-14 cents
        if byte & 0x40 != 0 {
            2f32.powf(vibrato * 14.0 / 1200.0)
        } else {
            1.0
        }
    }

    fn key_scale_level(fnum: u16, block: u8, ksl_type: u8) -> f32 {
        let shift = [31u32, 1, 2, 0][ksl_type as usize & 3];
        if shift == 31 {
            return 0.0;
        }
        let level = (KSL_BASE[(fnum >> 5) as usize] - 8 * (7 - block as i32)).max(0);
        (level >> shift) as f32 * 0.75
    }

    /// A sine (or half-wave rectified sine) at `phase` cycles, attenuated by `attenuation_db`.
    fn operator(phase: f32, attenuation_db: f32, rectified: bool) -> f32 {
        if attenuation_db >= SILENCE_DB {
            return 0.0;
        }
        let sine = (phase.fract() * std::f32::consts::TAU).sin();
        let wave = if rectified && sine < 0.0 { 0.0 } else { sine };
        wave * 10f32.powf(-attenuation_db / 20.0)
    }

    fn advance_envelope(slot: &mut Slot, role: Role, patch: &[u8; 8], key: (bool, bool), key_scale_index: f32) {
        let (key_on, sustain) = key;
        let (type_byte, attack_decay, sustain_release) = match role {
            Role::Modulator => (patch[0], patch[4], patch[6]),
            Role::Carrier => (patch[1], patch[5], patch[7]),
        };
        let sustained_type = type_byte & 0x20 != 0;
        let key_scale_rate = if type_byte & 0x10 != 0 { key_scale_index } else { key_scale_index / 4.0 };
        let speed_up = 2f32.powf(key_scale_rate / 4.0);
        let attack_rate = attack_decay >> 4;
        let decay_rate = attack_decay & 0x0F;
        let sustain_level = if sustain_release >> 4 == 15 { SILENCE_DB } else { (sustain_release >> 4) as f32 * 3.0 };
        let release_rate = if !key_on {
            // a key-off slot releases at its own rate only when it is the sustained kind; the rest fall fast
            match (role, sustain, sustained_type) {
                (Role::Carrier, true, _) => 5,
                (_, _, true) => sustain_release & 0x0F,
                _ => 7,
            }
        } else {
            sustain_release & 0x0F
        };

        let linear_step = |rate: u8| -> f32 {
            // dB per sample for a full run through the range in DECAY_BASE_MS / 2^(rate - 1)
            if rate == 0 {
                0.0
            } else {
                let ms = DECAY_BASE_MS / 2f32.powi(rate as i32 - 1) / speed_up;
                SILENCE_DB / (ms * SAMPLE_RATE / 1000.0)
            }
        };

        match slot.stage {
            Stage::Attack => {
                if attack_rate == 0 {
                    // never rises
                } else if attack_rate >= 15 {
                    slot.attenuation = 0.0;
                } else {
                    let ms = ATTACK_BASE_MS / 2f32.powi(attack_rate as i32 - 1) / speed_up;
                    let samples = (ms * SAMPLE_RATE / 1000.0).max(1.0);
                    // the attenuation shrinks by a constant factor per sample: 48 dB -> 0.4 dB over the attack time
                    slot.attenuation *= (0.4f32 / SILENCE_DB).powf(1.0 / samples);
                }
                if slot.attenuation <= 0.4 {
                    slot.attenuation = 0.0;
                    slot.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                slot.attenuation = (slot.attenuation + linear_step(decay_rate)).min(SILENCE_DB);
                if slot.attenuation >= sustain_level {
                    slot.attenuation = sustain_level;
                    slot.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {
                if !sustained_type {
                    // a percussive instrument keeps fading with its release rate
                    slot.attenuation = (slot.attenuation + linear_step(sustain_release & 0x0F)).min(SILENCE_DB);
                }
            }
            Stage::Release => {
                slot.attenuation = (slot.attenuation + linear_step(release_rate)).min(SILENCE_DB);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(chip: &mut Opll, register: u8, data: u8) {
        chip.select(register);
        chip.write(data);
    }

    /// Plays instrument `instrument` on channel 0 at the given F-number and block, full volume.
    fn play(instrument: u8, fnum: u16, block: u8) -> Opll {
        let mut chip = Opll::default();
        write(&mut chip, 0x10, fnum as u8);
        write(&mut chip, 0x30, instrument << 4);
        write(&mut chip, 0x20, 0x10 | (block << 1) | (fnum >> 8) as u8);
        chip
    }

    /// A pure sine through the custom instrument: the carrier alone, at full level, with an instant attack.
    fn sine(fnum: u16, block: u8) -> Opll {
        let mut chip = Opll::default();
        for (register, value) in [0x20u8, 0x21, 0x3F, 0x00, 0x00, 0xF0, 0x00, 0x0F].iter().enumerate() {
            write(&mut chip, register as u8, *value);
        }
        write(&mut chip, 0x10, fnum as u8);
        write(&mut chip, 0x30, 0x00);
        write(&mut chip, 0x20, 0x10 | (block << 1) | (fnum >> 8) as u8);
        chip
    }

    fn samples(chip: &mut Opll, count: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            out.push(chip.sample());
        }
        out
    }

    fn rising_crossings(samples: &[f32]) -> usize {
        samples.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
    }

    #[test]
    fn a_silent_chip_produces_nothing() {
        let mut chip = Opll::default();
        assert!(samples(&mut chip, 500).iter().all(|&s| s == 0.0));
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().cloned().fold(0.0f32, |a, b| a.max(b.abs()))
    }

    #[test]
    fn a_key_on_note_has_the_pitch_of_its_f_number_and_block() {
        // fnum 290, block 4 = 440 Hz at 49716 Hz sampling
        let mut chip = sine(290, 4);
        samples(&mut chip, 500);
        let one_second = samples(&mut chip, 49_716);

        let frequency = rising_crossings(&one_second) as f32;
        assert!((frequency - 440.0).abs() < 25.0, "{frequency} Hz");
        assert!(peak(&one_second) > 0.8, "a full-scale sine");
    }

    #[test]
    fn a_higher_block_doubles_the_frequency() {
        let count = |block: u8| {
            let mut chip = sine(200, block);
            samples(&mut chip, 500);
            rising_crossings(&samples(&mut chip, 49_716)) as f32
        };

        let ratio = count(4) / count(3);
        assert!((ratio - 2.0).abs() < 0.15, "{ratio}");
    }

    #[test]
    fn the_volume_register_attenuates_in_3_db_steps() {
        let level = |volume: u8| {
            let mut chip = sine(290, 4);
            write(&mut chip, 0x30, volume);
            peak(&samples(&mut chip, 3_000))
        };

        let loud = level(0);
        assert!((level(2) / loud - 0.5).abs() < 0.05, "6 dB down is half the amplitude");
        assert!(level(15) < loud * 0.03, "-45 dB");
    }

    #[test]
    fn releasing_the_key_fades_the_note_out() {
        let mut chip = play(6, 290, 4);
        samples(&mut chip, 5_000);
        let before = peak(&samples(&mut chip, 400));
        assert!(before > 0.2, "{before}");

        write(&mut chip, 0x20, 4 << 1 | 1); // key off
        samples(&mut chip, 40_000);
        let after = peak(&samples(&mut chip, 400));
        assert!(after < before * 0.05, "{after} vs {before}");
    }

    #[test]
    fn a_percussive_instrument_dies_away_even_with_the_key_held() {
        // instrument 6 is percussive: after its attack and decay it keeps fading at the release rate
        let mut chip = play(6, 290, 4);
        let early = peak(&samples(&mut chip, 3_000));
        samples(&mut chip, 600_000);
        let late = peak(&samples(&mut chip, 400));

        assert!(early > 0.2, "{early}");
        assert!(late < early * 0.2, "{late} vs {early}");
    }

    #[test]
    fn the_custom_instrument_is_programmed_through_registers_0_to_7() {
        let mut chip = sine(290, 4);
        samples(&mut chip, 500);
        let tone = samples(&mut chip, 49_716);

        assert!(peak(&tone) > 0.8);
        assert!(peak(&tone) <= 1.0);
        write(&mut chip, 0x05, 0x00); // carrier attack rate 0: the next note never rises
        write(&mut chip, 0x20, 4 << 1 | 1); // key off
        samples(&mut chip, 5_000); // release rate 15: silent long before this
        write(&mut chip, 0x20, 0x10 | (4 << 1) | 1);
        assert!(peak(&samples(&mut chip, 1_000)) < 0.01);
    }

    #[test]
    fn the_output_follows_the_chip_clock_one_sample_per_36_cpu_cycles() {
        let mut chip = play(6, 290, 4);
        let mut updates = 0;
        let mut previous = chip.output();
        for _ in 0..(36 * 200) {
            chip.tick();
            if chip.output() != previous {
                updates += 1;
                previous = chip.output();
            }
        }

        assert!(updates > 150 && updates <= 200, "{updates} changes in 200 samples");
    }
}
