//! Audio Processing Unit: two pulse channels, triangle, noise and DMC, driven by the frame counter.
//! https://www.nesdev.org/wiki/APU
//!
//! `tick` runs once per CPU cycle. The mixed output is averaged over each output sample period and passed
//! through the console's analog filters; `drain_samples` hands the finished samples (mono `f32`) to the frontend.

mod dmc;
mod frame_counter;
mod mixer;
mod noise;
pub(crate) mod pulse;
mod triangle;
pub(crate) mod units;

use crate::emulator::region::Region;
use dmc::Dmc;
use frame_counter::FrameCounter;
use mixer::{Mixer, OutputFilter};
use noise::Noise;
use pulse::Pulse;
use triangle::Triangle;

pub const NTSC_CPU_CLOCK_HZ: f64 = 1_789_773.0;
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;
/// If nobody drains the samples, only the most recent two seconds are kept.
const MAX_BUFFERED_SAMPLES: usize = 2 * 96_000;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Apu {
    region: Region,
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    dmc: Dmc,
    frame_counter: FrameCounter,
    cycle: u64,
    /// The IRQ output as the CPU sees it: it follows the flags with a one-cycle delay.
    irq_line: bool,

    #[serde(skip)]
    mixer: Mixer,
    filter: OutputFilter,
    sample_rate: u32,
    /// Sum and count of the mixer outputs since the last emitted sample, and the fractional position in the
    /// output sample period (in units of `sample_rate` CPU-clock ticks).
    accumulator: f64,
    accumulated: u32,
    phase: f64,
    #[serde(skip)]
    samples: Vec<f32>,
}

impl Default for Apu {
    fn default() -> Self {
        Self::new(DEFAULT_SAMPLE_RATE)
    }
}

impl Apu {
    pub fn new(sample_rate: u32) -> Self {
        Self::with_region(sample_rate, Region::Ntsc)
    }

    /// PAL consoles have a slower CPU clock, other noise periods and DMC rates and a longer frame counter sequence.
    pub fn with_region(sample_rate: u32, region: Region) -> Self {
        let sample_rate = sample_rate.max(8_000);
        Apu {
            region,
            pulse1: Pulse::new(true),
            pulse2: Pulse::new(false),
            triangle: Triangle::default(),
            noise: Noise::new(region),
            dmc: Dmc::new(region),
            frame_counter: FrameCounter::new(region),
            cycle: 0,
            irq_line: false,
            mixer: Mixer::default(),
            filter: OutputFilter::new(sample_rate as f32),
            sample_rate,
            accumulator: 0.0,
            accumulated: 0,
            phase: 0.0,
            samples: Vec::new(),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Changes the output sample rate (e.g. to the audio device's). Pending samples are discarded.
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        let sample_rate = sample_rate.max(8_000);
        self.sample_rate = sample_rate;
        self.filter = OutputFilter::new(sample_rate as f32);
        self.accumulator = 0.0;
        self.accumulated = 0;
        self.phase = 0.0;
        self.samples.clear();
    }

    /// Reset acts like writing 0 to $4015 (the pulse and noise length counters are cleared, the triangle's counter
    /// keeps its value, the DMC stops) and rewrites $4017 with the last value written to it. The other registers
    /// are untouched. https://www.nesdev.org/wiki/CPU_power_up_state
    pub fn reset(&mut self) {
        self.pulse1.length.set_enabled(false);
        self.pulse2.length.set_enabled(false);
        self.noise.length.set_enabled(false);
        self.triangle.length.disable_keeping_value();
        self.dmc.set_enabled(false);

        self.frame_counter.restart_after_reset();
        self.irq_line = false;
    }

    /// Writes to $4000-$4013, $4015 and $4017 (`$4014` is the OAM DMA port and belongs to the bus).
    pub fn write_register(&mut self, address: u16, data: u8) {
        match address {
            0x4000..=0x4003 => self.pulse1.write(address - 0x4000, data),
            0x4004..=0x4007 => self.pulse2.write(address - 0x4004, data),
            0x4008..=0x400B => self.triangle.write(address - 0x4008, data),
            0x400C..=0x400F => self.noise.write(address - 0x400C, data),
            0x4010..=0x4013 => self.dmc.write(address - 0x4010, data),
            0x4015 => {
                self.pulse1.length.set_enabled(data & 0x01 != 0);
                self.pulse2.length.set_enabled(data & 0x02 != 0);
                self.triangle.length.set_enabled(data & 0x04 != 0);
                self.noise.length.set_enabled(data & 0x08 != 0);
                self.dmc.set_enabled(data & 0x10 != 0);
            }
            // the APU ticks at the end of a CPU cycle, so `cycle` still counts the cycles before this one
            0x4017 => self.frame_counter.write(data, self.cycle & 1 == 0),
            _ => {}
        }
    }

    /// A read of $4015 (bit 5 is open bus and left to the caller). Clears the frame interrupt flag.
    pub fn read_status(&mut self) -> u8 {
        let status = (self.dmc.irq_flag() as u8) << 7
            | (self.frame_counter.irq_flag() as u8) << 6
            | (self.dmc.active() as u8) << 4
            | (self.noise.length.active() as u8) << 3
            | (self.triangle.length.active() as u8) << 2
            | (self.pulse2.length.active() as u8) << 1
            | self.pulse1.length.active() as u8;

        self.frame_counter.clear_irq();
        status
    }

    /// The level of the IRQ line: frame counter interrupt or DMC sample end.
    pub fn irq_pending(&self) -> bool {
        self.irq_line
    }

    /// The memory address the DMC wants to read, if it is waiting for a byte (see `load_dmc_sample_byte`).
    pub fn take_dmc_dma_request(&mut self) -> Option<u16> {
        self.dmc.take_dma_request()
    }

    /// The fetch just taken was triggered by a $4015 write: it costs 3 cycles instead of 4 (no alignment).
    pub fn dmc_dma_is_load(&self) -> bool {
        self.dmc.dma_is_load()
    }

    pub fn load_dmc_sample_byte(&mut self, byte: u8) {
        self.dmc.load_sample_byte(byte);
    }

    /// Advances one CPU cycle.
    pub fn tick(&mut self) {
        self.tick_with_expansion(0.0);
    }

    /// Advances one CPU cycle, mixing in the sound of the cartridge's own hardware (see `Mapper::audio_output`).
    pub fn tick_with_expansion(&mut self, expansion: f32) {
        self.cycle += 1;
        self.irq_line = self.frame_counter.irq_flag() || self.dmc.irq_flag();

        let clock = self.frame_counter.tick();
        if clock.quarter {
            self.pulse1.clock_quarter_frame();
            self.pulse2.clock_quarter_frame();
            self.triangle.clock_quarter_frame();
            self.noise.clock_quarter_frame();
        }
        if clock.half {
            self.pulse1.clock_half_frame();
            self.pulse2.clock_half_frame();
            self.triangle.clock_half_frame();
            self.noise.clock_half_frame();
        }

        self.pulse1.tick();
        self.pulse2.tick();
        self.triangle.tick();
        self.noise.tick();
        self.dmc.tick();

        self.pulse1.end_cycle();
        self.pulse2.end_cycle();
        self.triangle.end_cycle();
        self.noise.end_cycle();

        self.sample(expansion);
    }

    fn sample(&mut self, expansion: f32) {
        let level = self.mixer.mix(
            self.pulse1.output(),
            self.pulse2.output(),
            self.triangle.output(),
            self.noise.output(),
            self.dmc.output(),
        );

        self.accumulator += (level + expansion) as f64;
        self.accumulated += 1;
        self.phase += self.sample_rate as f64;

        let cpu_clock = self.region.cpu_clock_hz();
        if self.phase >= cpu_clock {
            self.phase -= cpu_clock;
            let average = (self.accumulator / self.accumulated as f64) as f32;
            self.accumulator = 0.0;
            self.accumulated = 0;

            if self.samples.len() >= MAX_BUFFERED_SAMPLES {
                self.samples.drain(..MAX_BUFFERED_SAMPLES / 2);
            }
            let filtered = self.filter.apply(average);
            self.samples.push(filtered);
        }
    }

    /// Moves the finished samples (mono, roughly -0.5..0.5) to `out`.
    pub fn drain_samples(&mut self, out: &mut Vec<f32>) {
        out.append(&mut self.samples);
    }

    pub fn buffered_samples(&self) -> usize {
        self.samples.len()
    }
}

#[cfg(test)]
mod tests;
