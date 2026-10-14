//! Rewinding: a ring buffer of save states, one per frame.

use std::collections::VecDeque;

use crate::Machine;

/// The last `capacity` frames as save states (a few KB each).
pub struct Rewind {
    states: VecDeque<Vec<u8>>,
    capacity: usize,
}

impl Rewind {
    pub fn new(capacity: usize) -> Self {
        Rewind { states: VecDeque::with_capacity(capacity.min(1024)), capacity: capacity.max(1) }
    }

    /// Number of frames that can be rewound.
    pub fn len(&self) -> usize {
        self.states.len()
    }

    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    pub fn clear(&mut self) {
        self.states.clear();
    }

    /// Remembers the machine as it is now; call once per frame, before `run_frame`.
    pub fn push<M: Machine>(&mut self, machine: &M) {
        if self.states.len() == self.capacity {
            self.states.pop_front();
        }
        self.states.push_back(machine.save_state());
    }

    /// Goes back one frame. Returns false when there is nothing left to rewind to.
    /// The picture is refreshed (a save state does not hold it) and any audio it produced is left in the machine
    /// for the caller to drain and discard.
    pub fn step_back<M: Machine>(&mut self, machine: &mut M) -> bool {
        let Some(state) = self.states.pop_back() else {
            return false;
        };

        if machine.load_state(&state).is_err() {
            // a state from another game (the cartridge was swapped): the history is useless
            self.states.clear();
            return false;
        }

        // draw the frame that follows the state, then return to the state itself
        machine.run_frame();
        machine.load_state(&state).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ButtonInfo, StateError, VideoFrame};

    /// A machine that counts frames and draws the count as a one-pixel picture.
    struct Counter {
        frames: u8,
        picture: [u8; 4],
    }

    impl Machine for Counter {
        fn reset(&mut self) {
            self.frames = 0;
        }

        fn run_frame(&mut self) -> bool {
            self.frames += 1;
            self.picture = [self.frames, 0, 0, 255];
            true
        }

        fn video(&self) -> VideoFrame<'_> {
            VideoFrame { width: 1, height: 1, rgba: &self.picture }
        }

        fn frame_rate(&self) -> f64 {
            60.0
        }

        fn set_audio_sample_rate(&mut self, _hz: u32) {}

        fn drain_audio(&mut self, _out: &mut Vec<f32>) {}

        fn buttons(&self) -> &'static [ButtonInfo] {
            &[]
        }

        fn set_input(&mut self, _port: usize, _pressed: u32) {}

        fn save_state(&self) -> Vec<u8> {
            vec![self.frames]
        }

        fn load_state(&mut self, data: &[u8]) -> Result<(), StateError> {
            self.frames = *data.first().ok_or(StateError::Corrupt)?;
            Ok(())
        }
    }

    fn counter() -> Counter {
        Counter { frames: 0, picture: [0, 0, 0, 255] }
    }

    #[test]
    fn stepping_back_returns_to_the_state_before_each_frame_and_redraws() {
        let mut machine = counter();
        let mut rewind = Rewind::new(100);
        for _ in 0..10 {
            rewind.push(&machine);
            machine.run_frame();
        }

        assert!(rewind.step_back(&mut machine));
        assert_eq!(machine.frames, 9);
        assert_eq!(machine.video().rgba[0], 10, "the picture of the frame that was just undone");
        assert!(rewind.step_back(&mut machine));
        assert_eq!(machine.frames, 8);
        assert_eq!(rewind.len(), 8);
    }

    #[test]
    fn the_oldest_states_are_dropped_and_an_empty_buffer_stops() {
        let mut machine = counter();
        let mut rewind = Rewind::new(3);
        for _ in 0..10 {
            rewind.push(&machine);
            machine.run_frame();
        }
        assert_eq!(rewind.len(), 3);

        for _ in 0..3 {
            assert!(rewind.step_back(&mut machine));
        }
        assert_eq!(machine.frames, 7);
        assert!(!rewind.step_back(&mut machine));
        assert!(rewind.is_empty());
    }
}
