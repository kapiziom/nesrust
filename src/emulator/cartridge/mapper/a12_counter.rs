//! The scanline counter of the MMC3 and its relatives (Taito TC0690, ...): clocked by filtered rising edges of PPU
//! address line A12, which the PPU's pattern fetches produce once per scanline when the sprites use $1000.
//! https://www.nesdev.org/wiki/MMC3#IRQ_Specifics

/// A12 must have been low for this many CPU cycles before a rising edge counts (filters the PPU's rapid toggling).
const LOW_CYCLES_REQUIRED: u8 = 3;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct A12Counter {
    latch: u8,
    counter: u8,
    reload: bool,
    enabled: bool,
    pending: bool,
    last_a12: bool,
    low_cycles: u8,
}

impl A12Counter {
    pub fn set_latch(&mut self, value: u8) {
        self.latch = value;
    }

    /// Clears the counter; the next clock reloads it from the latch.
    pub fn request_reload(&mut self) {
        self.counter = 0;
        self.reload = true;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.pending = false;
        }
    }

    pub fn pending(&self) -> bool {
        self.pending
    }

    fn clock(&mut self) {
        if self.counter == 0 || self.reload {
            self.counter = self.latch;
            self.reload = false;
        } else {
            self.counter -= 1;
        }

        if self.counter == 0 && self.enabled {
            self.pending = true;
        }
    }

    /// The PPU put `addr` on its address bus.
    pub fn watch(&mut self, addr: u16) {
        let a12 = addr & 0x1000 != 0;

        if a12 && !self.last_a12 && self.low_cycles >= LOW_CYCLES_REQUIRED {
            self.clock();
        }

        if a12 {
            self.low_cycles = 0;
        }
        self.last_a12 = a12;
    }

    /// Once per CPU cycle.
    pub fn cpu_cycle(&mut self) {
        if !self.last_a12 {
            self.low_cycles = self.low_cycles.saturating_add(1);
        }
    }
}
