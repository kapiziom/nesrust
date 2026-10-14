//! The interface between emulated machines and the programs that run them (windows, audio devices, test runners).
//! A machine takes input and produces pictures and sound; it does no I/O of its own, so one frontend can drive
//! any console that implements [`Machine`].

pub mod rewind;

pub use rewind::Rewind;

/// A save state could not be restored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateError {
    /// The bytes are not a state of this machine (truncated or damaged).
    Corrupt,
    /// Saved by an incompatible version of the emulator.
    WrongVersion,
    /// Saved while running another game.
    WrongCartridge,
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateError::Corrupt => write!(f, "the save state is corrupt"),
            StateError::WrongVersion => write!(f, "the save state was made by another version of the emulator"),
            StateError::WrongCartridge => write!(f, "the save state belongs to another game"),
        }
    }
}

impl std::error::Error for StateError {}

/// The picture a machine produced: 8-bit RGBA, rows top to bottom.
#[derive(Clone, Copy, Debug)]
pub struct VideoFrame<'a> {
    pub width: usize,
    pub height: usize,
    pub rgba: &'a [u8],
}

/// One button of a machine's controller: `bit` is its position in the mask given to [`Machine::set_input`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonInfo {
    pub name: &'static str,
    pub bit: u32,
}

/// An emulated machine as seen by a frontend.
pub trait Machine {
    /// The reset button.
    fn reset(&mut self);

    /// Runs until the next video frame is complete. Returns false when the machine has stopped (for example a CPU
    /// that jammed), until it is reset.
    fn run_frame(&mut self) -> bool;

    /// The most recent complete picture.
    fn video(&self) -> VideoFrame<'_>;

    /// Frames per second of the machine being emulated.
    fn frame_rate(&self) -> f64;

    /// Sets the rate of the audio produced by [`Machine::drain_audio`] (use the sound device's).
    fn set_audio_sample_rate(&mut self, hz: u32);

    /// Moves the audio produced since the last call (mono, `f32`) to `out`.
    fn drain_audio(&mut self, out: &mut Vec<f32>);

    /// The buttons of the controllers, for key and gamepad mapping.
    fn buttons(&self) -> &'static [ButtonInfo];

    /// Sets which buttons are held on controller `port`: bit `n` of `pressed` is the button with `bit == n`.
    /// They stay held until changed.
    fn set_input(&mut self, port: usize, pressed: u32);

    /// Everything that is the machine's own state, not the game's ROM: it can only be loaded back into the same game.
    fn save_state(&self) -> Vec<u8>;

    /// Restores `save_state` bytes. On error the machine is left as it was.
    fn load_state(&mut self, data: &[u8]) -> Result<(), StateError>;

    /// Battery-backed RAM to keep between sessions, if the game has any.
    fn battery_ram(&self) -> Option<&[u8]> {
        None
    }

    fn load_battery_ram(&mut self, _data: &[u8]) {}
}
