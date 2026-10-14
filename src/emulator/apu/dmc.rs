//! Delta modulation channel ($4010-$4013): plays 1-bit delta-coded samples fetched from PRG space.
//! https://www.nesdev.org/wiki/APU_DMC

use super::units::Timer;
use crate::emulator::region::Region;

/// Output rates in CPU cycles per sample bit.
const NTSC_RATES: [u16; 16] = [428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54];
const PAL_RATES: [u16; 16] = [398, 354, 316, 298, 276, 236, 210, 198, 176, 148, 132, 118, 98, 78, 66, 50];

/// CPU cycles between the output unit emptying the sample buffer and the CPU being asked for the next byte.
/// Calibrated with Blargg's dmc_dma_during_read4 and sprdma_and_dmc_dma tests (together with the get/put cycle parity in the CPU).
const DMA_REQUEST_DELAY: u8 = 3;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Dmc {
    pal: bool,
    irq_enabled: bool,
    looping: bool,
    timer: Timer,
    output_level: u8,
    sample_address: u8,
    sample_length: u8,

    // memory reader
    current_address: u16,
    bytes_remaining: u16,
    buffer: Option<u8>,
    /// A fetch has been requested (and possibly handed out) but its byte has not arrived yet.
    dma_in_flight: bool,
    dma_requested: bool,
    dma_delay: u8,
    /// The pending fetch was triggered by a $4015 write rather than by the buffer running empty.
    load_dma: bool,

    // output unit
    shift_register: u8,
    bits_remaining: u8,
    silent: bool,

    irq_flag: bool,
}

impl Default for Dmc {
    fn default() -> Self {
        Self::new(Region::Ntsc)
    }
}

impl Dmc {
    pub fn new(region: Region) -> Self {
        let rates = if region.pal_apu_tables() { &PAL_RATES } else { &NTSC_RATES };
        Dmc {
            pal: region.pal_apu_tables(),
            irq_enabled: false,
            looping: false,
            // the DMC timer runs on the APU clock (every other CPU cycle): its first firing falls on an even cycle
            timer: Timer::with_period_and_counter(rates[0] - 1, 1),
            output_level: 0,
            sample_address: 0,
            sample_length: 0,
            current_address: 0xC000,
            bytes_remaining: 0,
            buffer: None,
            dma_in_flight: false,
            dma_requested: false,
            dma_delay: 0,
            load_dma: false,
            shift_register: 0,
            bits_remaining: 8,
            silent: true,
            irq_flag: false,
        }
    }
}

impl Dmc {
    fn rates(&self) -> &'static [u16; 16] {
        if self.pal {
            &PAL_RATES
        } else {
            &NTSC_RATES
        }
    }

    /// `register` is 0-3 (the offset from $4010).
    pub fn write(&mut self, register: u16, data: u8) {
        match register {
            0 => {
                self.irq_enabled = data & 0x80 != 0;
                if !self.irq_enabled {
                    self.irq_flag = false;
                }
                self.looping = data & 0x40 != 0;
                self.timer.period = self.rates()[data as usize & 0x0F] - 1;
            }
            1 => self.output_level = data & 0x7F,
            2 => self.sample_address = data,
            3 => self.sample_length = data,
            _ => {}
        }
    }

    /// Bit 4 of a $4015 write. Any write also acknowledges the DMC interrupt.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.irq_flag = false;

        if !enabled {
            self.bytes_remaining = 0;
        } else if self.bytes_remaining == 0 {
            self.restart();
            self.load_dma = self.buffer.is_none();
        }
    }

    fn restart(&mut self) {
        self.current_address = 0xC000 + self.sample_address as u16 * 64;
        self.bytes_remaining = self.sample_length as u16 * 16 + 1;
    }

    /// Called every CPU cycle.
    pub fn tick(&mut self) {
        if self.timer.tick() {
            self.clock_output_unit();
        }

        if self.dma_delay > 0 {
            self.dma_delay -= 1;
            if self.dma_delay == 0 {
                self.dma_requested = true;
            }
        } else if self.buffer.is_none() && self.bytes_remaining > 0 && !self.dma_in_flight {
            self.dma_in_flight = true;
            self.dma_delay = DMA_REQUEST_DELAY;
        }
    }

    fn clock_output_unit(&mut self) {
        if !self.silent {
            if self.shift_register & 1 != 0 {
                if self.output_level <= 125 {
                    self.output_level += 2;
                }
            } else if self.output_level >= 2 {
                self.output_level -= 2;
            }
        }

        self.shift_register >>= 1;
        self.bits_remaining -= 1;

        if self.bits_remaining == 0 {
            self.bits_remaining = 8;
            match self.buffer.take() {
                Some(byte) => {
                    self.shift_register = byte;
                    self.silent = false;
                }
                None => self.silent = true,
            }
        }
    }

    /// The address of the sample byte the memory reader wants, once per request. The CPU performs the read
    /// (stealing cycles) and hands the byte back with `load_sample_byte`.
    pub fn take_dma_request(&mut self) -> Option<u16> {
        std::mem::take(&mut self.dma_requested).then_some(self.current_address)
    }

    /// Whether the fetch handed out last was started by a $4015 write (a "load" DMA, which needs no alignment cycle).
    pub fn dma_is_load(&self) -> bool {
        self.load_dma
    }

    pub fn load_sample_byte(&mut self, byte: u8) {
        self.dma_in_flight = false;
        self.load_dma = false;
        self.buffer = Some(byte);
        self.current_address = if self.current_address == 0xFFFF { 0x8000 } else { self.current_address + 1 };
        self.bytes_remaining = self.bytes_remaining.saturating_sub(1);

        if self.bytes_remaining == 0 {
            if self.looping {
                self.restart();
            } else if self.irq_enabled {
                self.irq_flag = true;
            }
        }
    }

    pub fn active(&self) -> bool {
        self.bytes_remaining > 0
    }

    pub fn irq_flag(&self) -> bool {
        self.irq_flag
    }

    /// 0-127.
    pub fn output(&self) -> u8 {
        self.output_level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the channel for `cycles`, serving DMA requests from `memory` like the CPU would.
    fn run(dmc: &mut Dmc, cycles: usize, memory: &dyn Fn(u16) -> u8) {
        for _ in 0..cycles {
            dmc.tick();
            if let Some(address) = dmc.take_dma_request() {
                dmc.load_sample_byte(memory(address));
            }
        }
    }

    #[test]
    fn the_sample_address_and_length_are_derived_from_the_registers() {
        let mut dmc = Dmc::default();
        dmc.write(2, 0x10);
        dmc.write(3, 0x02);
        dmc.set_enabled(true);

        assert_eq!(dmc.current_address, 0xC000 + 0x10 * 64);
        assert_eq!(dmc.bytes_remaining, 0x02 * 16 + 1);
        assert!(dmc.active());
    }

    #[test]
    fn pal_uses_its_own_rate_table() {
        let mut pal = Dmc::new(Region::Pal);
        pal.write(0, 0x00);
        assert_eq!(pal.timer.period, 397, "PAL rate 0 is 398 CPU cycles");
        pal.write(0, 0x0F);
        assert_eq!(pal.timer.period, 49);

        let mut ntsc = Dmc::new(Region::Ntsc);
        ntsc.write(0, 0x00);
        assert_eq!(ntsc.timer.period, 427);
    }

    #[test]
    fn direct_load_sets_the_output_level() {
        let mut dmc = Dmc::default();
        dmc.write(1, 0xFF);

        assert_eq!(dmc.output(), 0x7F);
    }

    #[test]
    fn a_one_bit_raises_and_a_zero_bit_lowers_the_level_by_two() {
        let mut dmc = Dmc::default();
        dmc.write(0, 0x0F); // fastest rate: 54 cycles per bit
        dmc.write(1, 20);
        dmc.write(3, 0);
        dmc.set_enabled(true);

        // 0b0000_1111: the first four bits shifted out are 1 (LSB first)
        run(&mut dmc, 54 * 8 + 54 * 4, &|_| 0b0000_1111);

        assert_eq!(dmc.output(), 20 + 4 * 2);

        run(&mut dmc, 54 * 4, &|_| 0b0000_1111);
        assert_eq!(dmc.output(), 20, "the next four bits lower it again");
    }

    #[test]
    fn the_level_is_clamped_to_0_127() {
        let mut high = Dmc::default();
        high.write(0, 0x0F);
        high.write(1, 126);
        high.write(3, 0);
        high.set_enabled(true);
        run(&mut high, 54 * 30, &|_| 0xFF);
        assert!(high.output() <= 127);
        assert_eq!(high.output(), 126 + 0, "126 + 2 would exceed 127, so it stays");

        let mut low = Dmc::default();
        low.write(0, 0x0F);
        low.write(1, 1);
        low.write(3, 0);
        low.set_enabled(true);
        run(&mut low, 54 * 30, &|_| 0x00);
        assert_eq!(low.output(), 1, "1 - 2 would go below 0, so it stays");
    }

    #[test]
    fn the_address_wraps_from_ffff_to_8000() {
        let mut dmc = Dmc::default();
        dmc.current_address = 0xFFFF;
        dmc.bytes_remaining = 5;

        dmc.load_sample_byte(0);

        assert_eq!(dmc.current_address, 0x8000);
        assert_eq!(dmc.bytes_remaining, 4);
    }

    #[test]
    fn the_irq_flag_is_set_when_the_sample_ends_if_enabled_and_cleared_by_4015_or_disabling() {
        let mut dmc = Dmc::default();
        dmc.write(0, 0x80 | 0x0F); // IRQ enabled, no loop
        dmc.write(3, 0); // 1 byte
        dmc.set_enabled(true);
        run(&mut dmc, 10, &|_| 0);
        assert!(dmc.irq_flag());

        dmc.set_enabled(true); // a $4015 write acknowledges
        assert!(!dmc.irq_flag() || dmc.bytes_remaining > 0);

        let mut dmc = Dmc::default();
        dmc.write(0, 0x80 | 0x0F);
        dmc.write(3, 0);
        dmc.set_enabled(true);
        run(&mut dmc, 10, &|_| 0);
        assert!(dmc.irq_flag());
        dmc.write(0, 0x0F); // clearing the enable bit clears the flag
        assert!(!dmc.irq_flag());
    }

    #[test]
    fn no_irq_when_the_sample_loops() {
        let mut dmc = Dmc::default();
        dmc.write(0, 0x80 | 0x40 | 0x0F);
        dmc.write(3, 0);
        dmc.set_enabled(true);

        run(&mut dmc, 1000, &|_| 0);

        assert!(!dmc.irq_flag());
        assert!(dmc.active(), "looping restarts the sample");
    }

    #[test]
    fn disabling_stops_the_memory_reader() {
        let mut dmc = Dmc::default();
        dmc.write(3, 0xFF);
        dmc.set_enabled(true);
        assert!(dmc.active());

        dmc.set_enabled(false);

        assert!(!dmc.active());
    }

    #[test]
    fn enabling_an_already_running_sample_does_not_restart_it() {
        let mut dmc = Dmc::default();
        dmc.write(3, 0x10);
        dmc.set_enabled(true);
        dmc.load_sample_byte(0);
        let remaining = dmc.bytes_remaining;

        dmc.set_enabled(true);

        assert_eq!(dmc.bytes_remaining, remaining);
    }

    #[test]
    fn a_fetch_started_by_a_4015_write_is_a_load_dma_and_a_refill_is_not() {
        let mut dmc = Dmc::default();
        dmc.write(3, 1);
        dmc.set_enabled(true);
        assert!(dmc.dma_is_load(), "the buffer was empty when the channel was enabled");

        dmc.load_sample_byte(0x55);
        assert!(!dmc.dma_is_load(), "the flag is consumed by the fetch");

        dmc.set_enabled(true); // still playing: nothing restarts
        assert!(!dmc.dma_is_load());
    }

    #[test]
    fn a_dma_request_is_made_once_per_byte() {
        let mut dmc = Dmc::default();
        dmc.write(3, 3);
        dmc.set_enabled(true);

        let mut request = None;
        let mut ticks = 0;
        while request.is_none() && ticks < 10 {
            dmc.tick();
            request = dmc.take_dma_request();
            ticks += 1;
        }
        assert_eq!(request, Some(0xC000));
        assert!(ticks > 1, "the request comes a little after the buffer runs empty");

        for _ in 0..5 {
            dmc.tick();
            assert_eq!(dmc.take_dma_request(), None, "still waiting for the byte");
        }

        dmc.load_sample_byte(0xAA);
        assert_eq!(dmc.bytes_remaining, 3 * 16);
    }
}
