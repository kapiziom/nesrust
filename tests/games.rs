//! Smoke tests that play the ROMs in the git-ignored `.games` folder with bots (see `nesrs::harness`). They catch
//! what unit tests cannot: a mapper that selects the wrong banks so the game crashes or draws nothing, a hang,
//! a game that is not deterministic. Without `.games` they do nothing. `examples/gamecheck.rs` plays the same way
//! and writes pictures to look at.

use std::fs;
use std::path::PathBuf;

use nesrs::emulator::cartridge::Cartridge;
use nesrs::emulator::nes::Nes;
use nesrs::harness::{self, MashBot};

fn games() -> Vec<(String, Vec<u8>)> {
    let Ok(entries) = fs::read_dir(".games") else {
        eprintln!(".games not found, skipping");
        return Vec::new();
    };
    let mut roms: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("nes"))
        .collect();
    roms.sort();
    roms.into_iter()
        .map(|path| (path.file_stem().unwrap().to_string_lossy().to_string(), fs::read(&path).unwrap()))
        .collect()
}

fn console(data: &[u8]) -> Option<Nes> {
    Cartridge::from_bytes(data).ok().map(Nes::new)
}

#[test]
fn every_game_runs_draws_and_changes_under_a_button_masher() {
    for (name, data) in games() {
        let Some(mut nes) = console(&data) else {
            eprintln!("{name}: not a supported cartridge, skipped");
            continue;
        };

        let session = harness::run(&mut nes, &mut MashBot::new(1), 900, &[]);
        let report = &session.report;

        assert_eq!(report.stopped_at, None, "{name}: the CPU stopped");
        assert!(report.distinct_pictures >= 2, "{name}: the picture never changed");
        assert!(report.blank_frames < 800, "{name}: {} of 900 frames were a single colour", report.blank_frames);
        eprintln!(
            "{name}: {} distinct pictures, {} blank frames, {} colours at the end, audio peak {:.2}",
            report.distinct_pictures, report.blank_frames, report.colors_in_last_picture, report.audio.peak
        );
    }
}

#[test]
fn playing_a_game_twice_gives_exactly_the_same_result() {
    for (name, data) in games() {
        let (Some(mut first), Some(mut second)) = (console(&data), console(&data)) else {
            continue;
        };

        let a = harness::run(&mut first, &mut MashBot::new(3), 400, &[399]);
        let b = harness::run(&mut second, &mut MashBot::new(3), 400, &[399]);

        assert_eq!(a.shots, b.shots, "{name}: the last picture differs between two identical runs");
        assert_eq!(a.report.distinct_pictures, b.report.distinct_pictures, "{name}");
        assert_eq!(a.report.audio, b.report.audio, "{name}: the audio differs");
        assert_eq!(first.save_state(), second.save_state(), "{name}: the console state differs");
    }
}
