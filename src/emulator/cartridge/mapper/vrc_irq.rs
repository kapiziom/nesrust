//! The IRQ counter shared by Konami's VRC4, VRC6 and VRC7: an 8-bit counter that counts CPU cycles or (through a
//! prescaler that mimics the 341 PPU dots of a scanline) scanlines. https://www.nesdev.org/wiki/VRC_IRQ

/// PPU dots per scanline: the prescaler subtracts 3 per CPU cycle (3 dots) from this.
const PRESCALER_RELOAD: i16 = 341;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct VrcIrq {
    latch: u8,
    counter: u8,
    cycle_mode: bool,
    enabled: bool,
    enable_after_ack: bool,
    pending: bool,
    prescaler: i16,
}

impl Default for VrcIrq {
    fn default() -> Self {
        VrcIrq {
            latch: 0,
            counter: 0,
            cycle_mode: false,
            enabled: false,
            enable_after_ack: false,
            pending: false,
            prescaler: PRESCALER_RELOAD,
        }
    }
}

impl VrcIrq {
    /// Register 0: latch low nibble, 1: latch high nibble, 2: control, 3: acknowledge.
    pub fn write(&mut self, register: u16, data: u8) {
        match register {
            0 => self.latch = (self.latch & 0xF0) | (data & 0x0F),
            1 => self.latch = (self.latch & 0x0F) | (data << 4),
            2 => {
                self.enable_after_ack = data & 0x01 != 0;
                self.enabled = data & 0x02 != 0;
                self.cycle_mode = data & 0x04 != 0;
                self.pending = false;
                if self.enabled {
                    self.counter = self.latch;
                    self.prescaler = PRESCALER_RELOAD;
                }
            }
            _ => {
                self.pending = false;
                self.enabled = self.enable_after_ack;
            }
        }
    }

    /// VRC6 writes the whole latch at once.
    pub fn write_latch(&mut self, data: u8) {
        self.latch = data;
    }

    pub fn pending(&self) -> bool {
        self.pending
    }

    #[cfg(test)]
    pub fn counter(&self) -> u8 {
        self.counter
    }

    /// Once per CPU cycle.
    pub fn tick(&mut self) {
        if !self.enabled {
            return;
        }

        if self.cycle_mode {
            self.clock();
        } else {
            self.prescaler -= 3;
            if self.prescaler <= 0 {
                self.prescaler += PRESCALER_RELOAD;
                self.clock();
            }
        }
    }

    fn clock(&mut self) {
        if self.counter == 0xFF {
            self.counter = self.latch;
            self.pending = true;
        } else {
            self.counter += 1;
        }
    }
}
