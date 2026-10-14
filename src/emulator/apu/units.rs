//! Building blocks shared by the APU channels: timer, length counter and envelope.
//! https://www.nesdev.org/wiki/APU_Length_Counter, https://www.nesdev.org/wiki/APU_Envelope

pub const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14,
    12, 16, 24, 18, 48, 20, 96, 22, 192, 24, 72, 26, 16, 28, 32, 30,
];

/// A down-counter clocked once per CPU cycle: it fires every `period + 1` cycles.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Timer {
    pub period: u16,
    counter: u16,
}

impl Timer {
    pub fn with_period(period: u16) -> Self {
        Timer { period, counter: 0 }
    }

    /// A timer whose first firing is `counter` ticks away (its phase relative to the CPU clock).
    pub fn with_period_and_counter(period: u16, counter: u16) -> Self {
        Timer { period, counter }
    }

    pub fn tick(&mut self) -> bool {
        if self.counter == 0 {
            self.counter = self.period;
            true
        } else {
            self.counter -= 1;
            false
        }
    }
}

/// Silences a channel after a programmed number of half-frame clocks.
///
/// Writes (a load, or a change of the halt flag) are committed at the end of the CPU cycle, after the frame
/// counter has had its chance to clock the counter in that same cycle: a load that coincides with a clock of a
/// running counter is ignored, and a halt change only affects the clocks that come after it.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LengthCounter {
    value: u8,
    halt: bool,
    new_halt: bool,
    reload: u8,
    value_at_load: u8,
    enabled: bool,
}

impl Default for LengthCounter {
    /// At power-up $4015 is clear: the counters are disabled until a game enables them.
    fn default() -> Self {
        LengthCounter { value: 0, halt: false, new_halt: false, reload: 0, value_at_load: 0, enabled: false }
    }
}

impl LengthCounter {
    /// Clearing the enable bit in $4015 zeroes the counter; while disabled, loads are ignored.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.value = 0;
        }
    }

    /// What a reset does to the triangle: its enable flag is cleared but, unlike the other channels, the counter
    /// itself keeps running.
    pub fn disable_keeping_value(&mut self) {
        self.enabled = false;
    }

    /// The halt flag (bit 5 of $4000/$4004/$400C, bit 7 of $4008); takes effect at the end of the cycle.
    pub fn set_halt(&mut self, halt: bool) {
        self.new_halt = halt;
    }

    /// `index` is the 5-bit value written to the channel's last register; takes effect at the end of the cycle.
    pub fn load(&mut self, index: u8) {
        if self.enabled {
            self.reload = LENGTH_TABLE[index as usize & 0x1F];
            self.value_at_load = self.value;
        }
    }

    pub fn clock(&mut self) {
        if !self.halt && self.value > 0 {
            self.value -= 1;
        }
    }

    /// Commits the writes of this cycle (called after the frame counter has run).
    pub fn end_cycle(&mut self) {
        if self.reload != 0 {
            if self.value == self.value_at_load {
                self.value = self.reload;
            }
            self.reload = 0;
        }
        self.halt = self.new_halt;
    }

    pub fn active(&self) -> bool {
        self.value > 0
    }
}

/// Volume envelope of the pulse and noise channels.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Envelope {
    start: bool,
    divider: u8,
    decay: u8,
    volume: u8,
    constant: bool,
    looping: bool,
}

impl Envelope {
    /// Bits 0-5 of $4000/$4004/$400C: loop (bit 5), constant volume (bit 4), volume / divider period (bits 0-3).
    pub fn write(&mut self, data: u8) {
        self.looping = data & 0x20 != 0;
        self.constant = data & 0x10 != 0;
        self.volume = data & 0x0F;
    }

    pub fn restart(&mut self) {
        self.start = true;
    }

    pub fn clock(&mut self) {
        if self.start {
            self.start = false;
            self.decay = 15;
            self.divider = self.volume;
        } else if self.divider == 0 {
            self.divider = self.volume;
            if self.decay > 0 {
                self.decay -= 1;
            } else if self.looping {
                self.decay = 15;
            }
        } else {
            self.divider -= 1;
        }
    }

    pub fn output(&self) -> u8 {
        if self.constant { self.volume } else { self.decay }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_fires_every_period_plus_one_ticks() {
        let mut timer = Timer::with_period(3);
        let fired: Vec<bool> = (0..9).map(|_| timer.tick()).collect();

        assert_eq!(fired, [true, false, false, false, true, false, false, false, true]);
    }

    #[test]
    fn length_counter_counts_down_and_stops_at_zero() {
        let mut length = LengthCounter::default();
        length.set_enabled(true);
        length.load(3); // table[3] = 2
        length.end_cycle();

        assert!(length.active());
        length.clock();
        assert!(length.active());
        length.clock();
        assert!(!length.active());
        length.clock();
        assert!(!length.active());
    }

    #[test]
    fn length_counter_halt_freezes_it() {
        let mut length = LengthCounter::default();
        length.set_enabled(true);
        length.set_halt(true);
        length.load(3);
        length.end_cycle();

        length.clock();
        length.clock();
        length.clock();

        assert!(length.active());
    }

    #[test]
    fn length_counter_ignores_loads_while_disabled_and_clears_when_disabled() {
        let mut length = LengthCounter::default();
        length.load(1);
        length.end_cycle();
        assert!(!length.active(), "disabled at power-up");

        length.set_enabled(true);
        length.load(1);
        length.end_cycle();
        assert!(length.active());

        length.set_enabled(false);
        assert!(!length.active());
    }

    #[test]
    fn a_load_in_the_same_cycle_as_a_clock_of_a_running_counter_is_ignored() {
        let mut length = LengthCounter::default();
        length.set_enabled(true);
        length.load(1); // 254
        length.end_cycle();

        // a cycle with both a load (written during the cycle) and a half-frame clock
        length.load(0); // 10
        length.clock();
        length.end_cycle();

        assert_eq!(length.value, 253, "the clock won and the load was dropped");
    }

    #[test]
    fn a_load_in_the_same_cycle_as_a_clock_of_a_zero_counter_works() {
        let mut length = LengthCounter::default();
        length.set_enabled(true);
        length.load(0);
        length.clock();
        length.end_cycle();

        assert_eq!(length.value, 10);
    }

    #[test]
    fn a_halt_change_applies_after_the_clock_of_the_same_cycle() {
        let mut length = LengthCounter::default();
        length.set_enabled(true);
        length.load(1);
        length.end_cycle();

        length.set_halt(true);
        length.clock(); // still counts: the halt flag is not committed yet
        length.end_cycle();
        assert_eq!(length.value, 253);

        length.clock();
        assert_eq!(length.value, 253, "now halted");
    }

    #[test]
    fn length_table_matches_the_hardware() {
        assert_eq!(LENGTH_TABLE[0], 10);
        assert_eq!(LENGTH_TABLE[1], 254);
        assert_eq!(LENGTH_TABLE[0x1F], 30);
        assert_eq!(LENGTH_TABLE.len(), 32);
    }

    #[test]
    fn envelope_constant_volume_ignores_the_decay() {
        let mut envelope = Envelope::default();
        envelope.write(0x10 | 7);
        envelope.restart();
        for _ in 0..40 {
            envelope.clock();
        }

        assert_eq!(envelope.output(), 7);
    }

    #[test]
    fn envelope_decays_from_15_one_step_per_period() {
        let mut envelope = Envelope::default();
        envelope.write(2); // divider period 2: the level drops every 3 clocks
        envelope.restart();

        envelope.clock(); // start: decay = 15, divider = 2
        assert_eq!(envelope.output(), 15);
        for _ in 0..3 {
            envelope.clock();
        }
        assert_eq!(envelope.output(), 14);
        for _ in 0..3 {
            envelope.clock();
        }
        assert_eq!(envelope.output(), 13);
    }

    #[test]
    fn envelope_stops_at_zero_or_loops() {
        let mut stopping = Envelope::default();
        stopping.write(0); // divider period 0: one step per clock
        stopping.restart();
        for _ in 0..40 {
            stopping.clock();
        }
        assert_eq!(stopping.output(), 0);

        let mut looping = Envelope::default();
        looping.write(0x20);
        looping.restart();
        let levels: Vec<u8> = (0..18).map(|_| { looping.clock(); looping.output() }).collect();
        assert_eq!(levels[0], 15);
        assert_eq!(levels[15], 0);
        assert_eq!(levels[16], 15, "loops back to 15");
    }
}
