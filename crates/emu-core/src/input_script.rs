//! Recorded controller input: which buttons are held from which frame on. Plain text, one change per line
//! (`<frame> <hex mask>`, `#` starts a comment), so recordings can be written, read and edited by hand.

use std::fmt::Write as _;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputScript {
    /// (first frame, held buttons), in increasing frame order.
    changes: Vec<(u64, u32)>,
}

impl InputScript {
    pub fn new() -> Self {
        Self::default()
    }

    /// The buttons held during `frame`.
    pub fn at(&self, frame: u64) -> u32 {
        match self.changes.partition_point(|&(from, _)| from <= frame) {
            0 => 0,
            index => self.changes[index - 1].1,
        }
    }

    /// The frame of the last change (the script holds its final state forever after it).
    pub fn last_change(&self) -> u64 {
        self.changes.last().map_or(0, |&(frame, _)| frame)
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Records that `mask` is held from `frame` on. Does nothing when it is already the state at that frame; frames
    /// must not go backwards.
    pub fn set(&mut self, frame: u64, mask: u32) {
        assert!(frame >= self.last_change(), "input recorded out of order");
        if self.at(frame) != mask {
            if self.changes.last().is_some_and(|&(last, _)| last == frame) {
                self.changes.pop();
                if self.at(frame) == mask {
                    return;
                }
            }
            self.changes.push((frame, mask));
        }
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut script = InputScript::new();
        for (number, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut fields = line.split_whitespace();
            let (Some(frame), Some(mask), None) = (fields.next(), fields.next(), fields.next()) else {
                return Err(format!("line {}: expected `<frame> <hex mask>`", number + 1));
            };
            let frame: u64 = frame.parse().map_err(|_| format!("line {}: bad frame `{frame}`", number + 1))?;
            let mask = u32::from_str_radix(mask.trim_start_matches("0x"), 16)
                .map_err(|_| format!("line {}: bad mask `{mask}`", number + 1))?;
            if frame < script.last_change() {
                return Err(format!("line {}: frames must not go backwards", number + 1));
            }
            script.set(frame, mask);
        }
        Ok(script)
    }

    pub fn to_text(&self) -> String {
        let mut text = String::from("# frame  held buttons (hex bit mask)\n");
        for &(frame, mask) in &self.changes {
            writeln!(text, "{frame} {mask:02X}").unwrap();
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_holds_until_the_next_change() {
        let mut script = InputScript::new();
        script.set(10, 0b01);
        script.set(20, 0b11);
        script.set(30, 0);

        assert_eq!(script.at(0), 0);
        assert_eq!(script.at(9), 0);
        assert_eq!(script.at(10), 0b01);
        assert_eq!(script.at(19), 0b01);
        assert_eq!(script.at(20), 0b11);
        assert_eq!(script.at(30), 0);
        assert_eq!(script.at(10_000), 0);
    }

    #[test]
    fn repeating_the_same_state_records_nothing() {
        let mut script = InputScript::new();
        for frame in 0..100 {
            script.set(frame, if frame < 50 { 0 } else { 0x08 });
        }

        assert_eq!(script.to_text(), "# frame  held buttons (hex bit mask)\n50 08\n");
    }

    #[test]
    fn text_round_trips_and_ignores_comments() {
        let script = InputScript::parse("# a recording\n0 00\n30 08 # start\n\n40 00\n100 0x81\n").unwrap();

        assert_eq!(script.at(35), 0x08);
        assert_eq!(script.at(100), 0x81);
        assert_eq!(InputScript::parse(&script.to_text()).unwrap(), script);
    }

    #[test]
    fn malformed_scripts_are_errors_not_panics() {
        for bad in ["10", "x 01", "10 zz", "10 01 02", "20 01\n10 02"] {
            assert!(InputScript::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_change_back_within_the_same_frame_cancels_out() {
        let mut script = InputScript::new();
        script.set(5, 1);
        script.set(5, 0);

        assert!(script.is_empty());
    }
}
