//! Triangle channel ($4008-$400B). https://www.nesdev.org/wiki/APU_Triangle

use super::units::{LengthCounter, Timer};

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Triangle {
    /// The 11-bit period register; the sequencer advances every `period + 1` CPU cycles.
    raw_period: u16,
    timer: Timer,
    pub length: LengthCounter,
    /// Bit 7 of $4008: both the linear counter's control flag and the length counter's halt flag.
    control: bool,
    linear_reload_value: u8,
    linear_counter: u8,
    linear_reload: bool,
    step: u8,
}

impl Triangle {
    /// `register` is 0-3 (the offset from $4008); 1 is unused.
    pub fn write(&mut self, register: u16, data: u8) {
        match register {
            0 => {
                self.control = data & 0x80 != 0;
                self.length.set_halt(self.control);
                self.linear_reload_value = data & 0x7F;
            }
            2 => self.set_period((self.raw_period & 0x0700) | data as u16),
            3 => {
                self.set_period((self.raw_period & 0x00FF) | ((data as u16 & 0x07) << 8));
                self.length.load(data >> 3);
                self.linear_reload = true;
            }
            _ => {}
        }
    }

    fn set_period(&mut self, period: u16) {
        self.raw_period = period;
        self.timer.period = period;
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

    /// Called every CPU cycle (unlike the pulse and noise channels, which effectively run at half rate).
    pub fn tick(&mut self) {
        if self.timer.tick() && self.linear_counter > 0 && self.length.active() && self.raw_period >= 2 {
            // periods below 2 are ultrasonic: the sequencer is frozen instead of aliasing
            self.step = (self.step + 1) & 31;
        }
    }

    pub fn clock_quarter_frame(&mut self) {
        if self.linear_reload {
            self.linear_counter = self.linear_reload_value;
        } else if self.linear_counter > 0 {
            self.linear_counter -= 1;
        }

        if !self.control {
            self.linear_reload = false;
        }
    }

    pub fn clock_half_frame(&mut self) {
        self.length.clock();
    }

    /// 0-15; holds its last value while the sequencer is halted.
    pub fn output(&self) -> u8 {
        if self.step < 16 {
            15 - self.step
        } else {
            self.step - 16
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running_triangle(period: u16) -> Triangle {
        let mut triangle = Triangle::default();
        triangle.length.set_enabled(true);
        triangle.write_now(0, 0xFF); // control set, linear reload value 127
        triangle.write_now(2, period as u8);
        triangle.write_now(3, 0x08 | (period >> 8) as u8); // length index 1 (254)
        triangle.clock_quarter_frame(); // loads the linear counter
        triangle
    }

    fn run(triangle: &mut Triangle, cycles: usize) -> Vec<u8> {
        (0..cycles)
            .map(|_| {
                triangle.tick();
                triangle.output()
            })
            .collect()
    }

    #[test]
    fn steps_through_the_32_value_triangle_every_period_plus_one_cycles() {
        let mut triangle = running_triangle(3); // a step every 4 cycles
        let levels = run(&mut triangle, 4 * 34);

        let mut steps: Vec<u8> = levels.iter().step_by(4).copied().collect();
        steps.dedup();
        // starts at 15 and walks down to 0, then back up to 15
        let expected_down: Vec<u8> = (0..16).rev().collect();
        let start = steps.iter().position(|&v| v == 14).unwrap();
        assert_eq!(&steps[start..start + 15], &expected_down[1..]);
        assert!(steps.contains(&0) && steps.contains(&15));
    }

    #[test]
    fn output_holds_when_the_linear_counter_is_zero() {
        let mut triangle = Triangle::default();
        triangle.length.set_enabled(true);
        triangle.write_now(0, 0x00); // control clear, reload value 0
        triangle.write_now(2, 3);
        triangle.write_now(3, 0x08);
        triangle.clock_quarter_frame(); // linear counter = 0

        let before = triangle.output();
        run(&mut triangle, 200);

        assert_eq!(triangle.output(), before);
    }

    #[test]
    fn output_holds_when_the_length_counter_is_zero() {
        let mut triangle = Triangle::default();
        triangle.length.set_enabled(false); // disabled: the length counter stays 0
        triangle.write_now(0, 0xFF);
        triangle.write_now(2, 3);
        triangle.write_now(3, 0x08);
        triangle.clock_quarter_frame();

        let before = triangle.output();
        run(&mut triangle, 200);

        assert_eq!(triangle.output(), before);
    }

    #[test]
    fn linear_counter_counts_down_on_quarter_frames_and_reload_flag_clears_without_control() {
        let mut triangle = Triangle::default();
        triangle.length.set_enabled(true);
        triangle.write_now(0, 0x03); // control clear, reload 3
        triangle.write_now(3, 0x08); // sets the reload flag

        triangle.clock_quarter_frame();
        assert_eq!(triangle.linear_counter, 3);
        triangle.clock_quarter_frame();
        assert_eq!(triangle.linear_counter, 2);
        triangle.clock_quarter_frame();
        triangle.clock_quarter_frame();
        triangle.clock_quarter_frame();
        assert_eq!(triangle.linear_counter, 0, "stops at zero");
    }

    #[test]
    fn with_the_control_flag_set_the_counter_keeps_reloading() {
        let mut triangle = Triangle::default();
        triangle.write_now(0, 0x85); // control set, reload 5
        triangle.write_now(3, 0x00);

        for _ in 0..5 {
            triangle.clock_quarter_frame();
            assert_eq!(triangle.linear_counter, 5);
        }
    }

    #[test]
    fn ultrasonic_periods_freeze_the_sequencer() {
        let mut triangle = running_triangle(1);
        let before = triangle.output();

        run(&mut triangle, 100);

        assert_eq!(triangle.output(), before);
    }
}
