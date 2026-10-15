//! Playing a game without a person: bots that press buttons, a session runner that watches what happens and a report
//! of it. Used by `examples/gamecheck.rs` (screenshots and a contact sheet to look at) and `tests/games.rs`
//! (automatic smoke tests of the ROMs in `.games/`).

use std::collections::HashSet;

use emu_core::InputScript;

use crate::emulator::controller::Buttons;
use crate::emulator::nes::Nes;

/// Decides which buttons are held during a frame. It may look into the console (RAM) to react to the game.
pub trait Bot {
    fn buttons(&mut self, frame: u64, nes: &mut Nes) -> Buttons;
}

/// Presses nothing.
pub struct IdleBot;

impl Bot for IdleBot {
    fn buttons(&mut self, _frame: u64, _nes: &mut Nes) -> Buttons {
        Buttons::empty()
    }
}

/// Replays a recording (see `emu_core::InputScript`).
pub struct ScriptBot(pub InputScript);

impl Bot for ScriptBot {
    fn buttons(&mut self, frame: u64, _nes: &mut Nes) -> Buttons {
        Buttons::from_bits_truncate(self.0.at(frame) as u8)
    }
}

/// Presses random buttons for a few frames at a time (with a fixed seed, so a run can be repeated) and taps Start now
/// and then to get past title screens.
pub struct MashBot {
    state: u32,
    held: Buttons,
}

impl MashBot {
    pub fn new(seed: u32) -> Self {
        MashBot { state: seed.max(1), held: Buttons::empty() }
    }

    fn random(&mut self) -> u32 {
        // xorshift32
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        self.state
    }
}

impl Bot for MashBot {
    fn buttons(&mut self, frame: u64, _nes: &mut Nes) -> Buttons {
        if frame.is_multiple_of(6) {
            let bits = self.random();
            // A, B, Up, Down, Left, Right are each held with probability 1/4; Start and Select are rarer
            let mut held = Buttons::empty();
            for (index, button) in [Buttons::A, Buttons::B, Buttons::UP, Buttons::DOWN, Buttons::LEFT, Buttons::RIGHT]
                .into_iter()
                .enumerate()
            {
                if (bits >> (index * 2)) & 3 == 0 {
                    held |= button;
                }
            }
            if (bits >> 16) & 15 == 0 {
                held |= Buttons::START;
            }
            if (bits >> 20) & 31 == 0 {
                held |= Buttons::SELECT;
            }
            self.held = held;
        }
        // a tap of Start on a schedule as well
        if frame % 240 >= 60 && frame % 240 < 66 {
            return self.held | Buttons::START;
        }
        self.held
    }
}

/// Taps Start (to get past a title screen), then runs right with B held and jumps every second or so: it gets
/// through the opening stretch of most platformers.
pub struct RunRightBot;

impl Bot for RunRightBot {
    fn buttons(&mut self, frame: u64, _nes: &mut Nes) -> Buttons {
        match frame {
            60..=66 => Buttons::START,
            0..=200 => Buttons::empty(),
            _ => {
                let jumping = frame % 50 < 22;
                let mut held = Buttons::RIGHT | Buttons::B;
                if jumping {
                    held |= Buttons::A;
                }
                held
            }
        }
    }
}

/// What the audio of a session looked like.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AudioStats {
    pub samples: usize,
    pub peak: f32,
    pub rms: f32,
    /// Fraction of the samples that differ from the previous one (0 for silence or a constant level).
    pub changing: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub frames: u64,
    /// The frame in which the CPU stopped (an unimplemented or JAM opcode), if it did.
    pub stopped_at: Option<u64>,
    /// How many different pictures were shown.
    pub distinct_pictures: usize,
    /// Frames in which the whole picture was a single colour.
    pub blank_frames: usize,
    pub colors_in_last_picture: usize,
    pub audio: AudioStats,
    /// Fingerprints of the whole console state and of all the audio: an optimisation must not change them.
    pub state_hash: u64,
    pub audio_hash: u64,
    /// Fingerprint of every picture of the run, in order.
    pub picture_hash: u64,
}

pub struct Session {
    pub report: Report,
    /// The pictures (RGBA, 256x240) of the frames asked for.
    pub shots: Vec<(u64, Vec<u8>)>,
}

/// Number of distinct colours in an RGBA picture.
pub fn count_colors(rgba: &[u8]) -> usize {
    rgba.as_chunks::<4>().0.iter().map(|pixel| u32::from_le_bytes(*pixel)).collect::<HashSet<_>>().len()
}

/// Whether every pixel has the same colour.
fn is_blank(rgba: &[u8]) -> bool {
    rgba.as_chunks::<4>().0.iter().all(|pixel| pixel == &rgba[..4])
}

fn hash(rgba: &[u8]) -> u64 {
    rgba.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, &byte| (hash ^ byte as u64).wrapping_mul(0x0000_0100_0000_01b3))
}

/// Runs `frames` frames, feeding the console the bot's buttons, and keeps the pictures of the frames in `shots`.
pub fn run(nes: &mut Nes, bot: &mut dyn Bot, frames: u64, shots: &[u64]) -> Session {
    nes.set_sample_rate(44_100);
    let mut report = Report::default();
    let mut seen = HashSet::new();
    let mut session_shots = Vec::new();
    let mut audio = Vec::new();
    let (mut sum_squares, mut changes, mut previous) = (0.0f64, 0usize, 0.0f32);
    let mut audio_hash = 0xcbf2_9ce4_8422_2325u64;
    let mut picture_hash = 0xcbf2_9ce4_8422_2325u64;

    for frame in 0..frames {
        let buttons = bot.buttons(frame, nes);
        nes.set_buttons(0, buttons);

        if !nes.run_frame() {
            report.stopped_at = Some(frame);
            break;
        }
        report.frames += 1;

        let picture = nes.framebuffer();
        let this_picture = hash(picture);
        seen.insert(this_picture);
        picture_hash = (picture_hash ^ this_picture).wrapping_mul(0x0000_0100_0000_01b3);
        if is_blank(picture) {
            report.blank_frames += 1;
        }
        if shots.contains(&frame) {
            session_shots.push((frame, picture.to_vec()));
        }

        audio.clear();
        nes.drain_audio(&mut audio);
        for &sample in &audio {
            report.audio.samples += 1;
            report.audio.peak = report.audio.peak.max(sample.abs());
            sum_squares += (sample as f64) * (sample as f64);
            if sample != previous {
                changes += 1;
            }
            previous = sample;
            audio_hash = (audio_hash ^ sample.to_bits() as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    report.distinct_pictures = seen.len();
    report.audio_hash = audio_hash;
    report.picture_hash = picture_hash;
    report.state_hash = hash(&nes.save_state());
    report.colors_in_last_picture = count_colors(nes.framebuffer());
    if report.audio.samples > 0 {
        report.audio.rms = (sum_squares / report.audio.samples as f64).sqrt() as f32;
        report.audio.changing = changes as f32 / report.audio.samples as f32;
    }

    Session { report, shots: session_shots }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emulator::cartridge::Cartridge;

    /// NROM that shows the number of NMIs as the backdrop colour (and so a changing picture) and plays a tone.
    fn console() -> Nes {
        #[rustfmt::skip]
        let program = [
            0xA9, 0x80, 0x8D, 0x00, 0x20, // NMI on
            0xA9, 0x01, 0x8D, 0x15, 0x40, // enable pulse 1
            0xA9, 0xBF, 0x8D, 0x00, 0x40, // constant volume 15
            0xA9, 0x30, 0x8D, 0x02, 0x40,
            0xA9, 0x08, 0x8D, 0x03, 0x40,
            0x4C, 0x19, 0x80,             // spin
            // NMI at $801C
            0xE6, 0x00,
            0xA9, 0x3F, 0x8D, 0x06, 0x20,
            0xA9, 0x00, 0x8D, 0x06, 0x20,
            0xA5, 0x00, 0x8D, 0x07, 0x20,
            0xA9, 0x00, 0x8D, 0x06, 0x20, 0x8D, 0x06, 0x20,
            0x40,
        ];
        let mut data = vec![0u8; 16 + 16384 + 8192];
        data[0..4].copy_from_slice(b"NES\x1A");
        data[4] = 1;
        data[5] = 1;
        data[16..16 + program.len()].copy_from_slice(&program);
        let vectors = 16 + 0x3FFA;
        data[vectors..vectors + 2].copy_from_slice(&0x801Cu16.to_le_bytes());
        data[vectors + 2..vectors + 4].copy_from_slice(&0x8000u16.to_le_bytes());
        Nes::new(Cartridge::from_bytes(&data).unwrap())
    }

    #[test]
    fn a_session_reports_pictures_audio_and_shots() {
        let mut nes = console();

        let session = run(&mut nes, &mut IdleBot, 40, &[5, 30]);

        assert_eq!(session.report.frames, 40);
        assert_eq!(session.report.stopped_at, None);
        assert!(session.report.distinct_pictures > 20, "the backdrop changes every frame");
        assert!(session.report.colors_in_last_picture >= 1);
        assert_eq!(session.shots.iter().map(|(frame, _)| *frame).collect::<Vec<_>>(), [5, 30]);
        assert_eq!(session.shots[0].1.len(), 256 * 240 * 4);
        assert!(session.report.audio.samples > 40 * 700, "{}", session.report.audio.samples);
        assert!(session.report.audio.peak > 0.05);
        assert!(session.report.audio.changing > 0.5, "a tone changes the level on most samples");
    }

    #[test]
    fn a_jammed_cpu_is_reported() {
        let mut data = vec![0u8; 16 + 16384 + 8192];
        data[0..4].copy_from_slice(b"NES\x1A");
        data[4] = 1;
        data[5] = 1;
        data[16] = 0x02; // JAM
        let vectors = 16 + 0x3FFA;
        data[vectors + 2..vectors + 4].copy_from_slice(&0x8000u16.to_le_bytes());
        let mut nes = Nes::new(Cartridge::from_bytes(&data).unwrap());

        let session = run(&mut nes, &mut IdleBot, 10, &[]);

        assert_eq!(session.report.stopped_at, Some(0));
    }

    #[test]
    fn the_mash_bot_is_repeatable_and_presses_a_variety_of_buttons() {
        let mut nes = console();
        let record = |seed: u32, nes: &mut Nes| {
            let mut bot = MashBot::new(seed);
            (0..600).map(|frame| bot.buttons(frame, nes).bits()).collect::<Vec<_>>()
        };

        let first = record(7, &mut nes);
        assert_eq!(first, record(7, &mut nes));
        assert_ne!(first, record(8, &mut nes));
        let used = first.iter().fold(0u8, |all, &mask| all | mask);
        assert_eq!(used, 0xFF, "every button gets pressed at some point");
        assert!(first.contains(&0), "and sometimes none");
    }

    #[test]
    fn a_script_bot_follows_its_recording() {
        let script = InputScript::parse("10 01\n20 00\n").unwrap();
        let mut bot = ScriptBot(script);
        let mut nes = console();

        assert_eq!(bot.buttons(9, &mut nes), Buttons::empty());
        assert_eq!(bot.buttons(10, &mut nes), Buttons::A);
        assert_eq!(bot.buttons(20, &mut nes), Buttons::empty());
    }

    #[test]
    fn colours_are_counted() {
        assert_eq!(count_colors(&[1, 2, 3, 4, 1, 2, 3, 4]), 1);
        assert_eq!(count_colors(&[1, 2, 3, 4, 1, 2, 3, 5]), 2);
    }
}
