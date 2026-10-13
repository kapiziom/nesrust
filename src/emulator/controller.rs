//! The standard NES controller: an 8-bit shift register read through $4016 (port 1) and $4017 (port 2).
//! https://www.nesdev.org/wiki/Standard_controller

bitflags::bitflags! {
    /// Pressed buttons, in the order the controller shifts them out (bit 0 first).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Buttons: u8 {
        const A      = 0b0000_0001;
        const B      = 0b0000_0010;
        const SELECT = 0b0000_0100;
        const START  = 0b0000_1000;
        const UP     = 0b0001_0000;
        const DOWN   = 0b0010_0000;
        const LEFT   = 0b0100_0000;
        const RIGHT  = 0b1000_0000;
    }
}

#[derive(Clone, Debug, Default)]
pub struct Controller {
    buttons: Buttons,
    strobe: bool,
    shift: u8,
}

impl Controller {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets which buttons are currently held. Takes effect for the program on the next strobe
    /// (or immediately for the A button while the strobe line is high).
    pub fn set_buttons(&mut self, buttons: Buttons) {
        self.buttons = buttons;
    }

    pub fn buttons(&self) -> Buttons {
        self.buttons
    }

    /// Bit 0 of a write to $4016. While it is high the register keeps reloading from the buttons;
    /// when it goes low the current state stays latched and is shifted out by reads.
    pub fn write_strobe(&mut self, high: bool) {
        self.strobe = high;
        if high {
            self.shift = self.buttons.bits();
        }
    }

    /// A read of $4016/$4017: the next button on bit 0. Official controllers return 1 after the eighth read.
    pub fn read(&mut self) -> u8 {
        if self.strobe {
            self.shift = self.buttons.bits();
            return self.shift & 1;
        }

        let bit = self.shift & 1;
        self.shift = (self.shift >> 1) | 0x80;
        bit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn latch(controller: &mut Controller) {
        controller.write_strobe(true);
        controller.write_strobe(false);
    }

    fn read_all(controller: &mut Controller) -> Vec<u8> {
        (0..8).map(|_| controller.read()).collect()
    }

    #[test]
    fn buttons_are_shifted_out_a_first() {
        let mut controller = Controller::new();
        controller.set_buttons(Buttons::A | Buttons::START | Buttons::RIGHT);
        latch(&mut controller);

        // A, B, Select, Start, Up, Down, Left, Right
        assert_eq!(read_all(&mut controller), [1, 0, 0, 1, 0, 0, 0, 1]);
    }

    #[test]
    fn every_button_has_its_own_position() {
        for (index, button) in [
            Buttons::A, Buttons::B, Buttons::SELECT, Buttons::START,
            Buttons::UP, Buttons::DOWN, Buttons::LEFT, Buttons::RIGHT,
        ].into_iter().enumerate() {
            let mut controller = Controller::new();
            controller.set_buttons(button);
            latch(&mut controller);

            let mut expected = [0u8; 8];
            expected[index] = 1;
            assert_eq!(read_all(&mut controller), expected, "{button:?}");
        }
    }

    #[test]
    fn official_controllers_return_ones_after_eight_reads() {
        let mut controller = Controller::new();
        latch(&mut controller);
        read_all(&mut controller);

        assert_eq!(read_all(&mut controller), [1; 8]);
    }

    #[test]
    fn while_the_strobe_is_high_every_read_returns_a() {
        let mut controller = Controller::new();
        controller.set_buttons(Buttons::A);
        controller.write_strobe(true);

        assert_eq!(read_all(&mut controller), [1; 8]);

        controller.set_buttons(Buttons::B);
        assert_eq!(controller.read(), 0, "A is released; the live state is read");
    }

    #[test]
    fn the_state_is_latched_when_the_strobe_goes_low() {
        let mut controller = Controller::new();
        controller.set_buttons(Buttons::B);
        latch(&mut controller);

        controller.set_buttons(Buttons::A | Buttons::SELECT);
        assert_eq!(read_all(&mut controller), [0, 1, 0, 0, 0, 0, 0, 0], "later changes are not seen");

        latch(&mut controller);
        assert_eq!(read_all(&mut controller), [1, 0, 1, 0, 0, 0, 0, 0], "a new strobe picks them up");
    }

    #[test]
    fn a_new_strobe_restarts_a_partly_read_sequence() {
        let mut controller = Controller::new();
        controller.set_buttons(Buttons::A | Buttons::B);
        latch(&mut controller);
        controller.read();
        controller.read();
        controller.read();

        latch(&mut controller);

        assert_eq!(read_all(&mut controller), [1, 1, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn nothing_pressed_reads_zeroes() {
        let mut controller = Controller::new();
        latch(&mut controller);

        assert_eq!(read_all(&mut controller), [0; 8]);
    }
}
