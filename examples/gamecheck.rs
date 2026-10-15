//! Plays a ROM without a window and reports what happened: pictures at several moments (one PNG each and a contact
//! sheet to look at), whether the CPU stopped, how much the picture and the sound changed and how fast the
//! emulation ran.
//!
//!   cargo run --release --example gamecheck -- game.nes [--bot mash|run-right|idle|script:FILE] [--frames 1800]
//!                                                      [--shots 12] [--out DIR] [--seed 1]
//!
//! Bots: `mash` presses random buttons (and Start now and then), `run-right` taps Start and runs right with
//! jumps (it gets through the opening of most platformers), `idle` presses nothing, `script:FILE` replays a recording made with `viewer --record`.

use std::path::PathBuf;
use std::time::Instant;
use std::{env, fs, process};

use emu_core::{png, InputScript};
use nesrs::emulator::cartridge::Cartridge;
use nesrs::emulator::nes::Nes;
use nesrs::harness::{self, Bot, IdleBot, MashBot, RunRightBot, ScriptBot};

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    process::exit(1);
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(rom) = args.first().cloned() else {
        fail("usage: gamecheck <rom.nes> [--bot mash|run-right|idle|script:FILE] [--frames N] [--shots N] [--out DIR]");
    };
    let option = |name: &str| args.iter().position(|arg| arg == name).and_then(|index| args.get(index + 1)).cloned();

    let frames: u64 = option("--frames").and_then(|n| n.parse().ok()).unwrap_or(1800);
    let shot_count: u64 = option("--shots").and_then(|n| n.parse().ok()).unwrap_or(12);
    let seed: u32 = option("--seed").and_then(|n| n.parse().ok()).unwrap_or(1);
    let bot_name = option("--bot").unwrap_or_else(|| "mash".to_string());

    let data = fs::read(&rom).unwrap_or_else(|error| fail(&format!("cannot read {rom}: {error}")));
    let cartridge = Cartridge::from_bytes(&data).unwrap_or_else(|error| fail(&format!("cannot load {rom}: {error}")));
    let (mapper, region) = (cartridge.mapper_id(), cartridge.region());
    let mut nes = Nes::new(cartridge);

    let mut bot: Box<dyn Bot> = match bot_name.as_str() {
        "idle" => Box::new(IdleBot),
        "mash" => Box::new(MashBot::new(seed)),
        "run-right" => Box::new(RunRightBot),
        other => match other.strip_prefix("script:") {
            Some(path) => {
                let text =
                    fs::read_to_string(path).unwrap_or_else(|error| fail(&format!("cannot read {path}: {error}")));
                Box::new(ScriptBot(InputScript::parse(&text).unwrap_or_else(|error| fail(&format!("{path}: {error}")))))
            }
            None => fail(&format!("unknown bot `{other}`")),
        },
    };

    let stem = PathBuf::from(&rom).file_stem().map(|stem| stem.to_string_lossy().to_string()).unwrap_or_default();
    let out = option("--out").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("target/gamecheck").join(&stem));
    fs::create_dir_all(&out).unwrap_or_else(|error| fail(&format!("cannot create {}: {error}", out.display())));

    // shots spread over the run, the last one on the last frame
    let shot_frames: Vec<u64> = (1..=shot_count).map(|n| (frames * n / shot_count).saturating_sub(1)).collect();

    let started = Instant::now();
    let session = harness::run(&mut nes, bot.as_mut(), frames, &shot_frames);
    let seconds = started.elapsed().as_secs_f64();
    let report = &session.report;

    println!("{stem}: mapper {mapper}, {region:?}");
    println!(
        "  ran {} of {frames} frames in {seconds:.2} s ({:.0} fps, {:.1}x real time)",
        report.frames,
        report.frames as f64 / seconds,
        report.frames as f64 / seconds / 60.0
    );
    match report.stopped_at {
        Some(frame) => println!("  THE CPU STOPPED in frame {frame} (PC = {:#06X})", nes.cpu.program_counter),
        None => println!("  the CPU kept running"),
    }
    println!(
        "  {} distinct pictures, {} blank frames, {} colours in the last picture",
        report.distinct_pictures, report.blank_frames, report.colors_in_last_picture
    );
    let audio = report.audio;
    println!(
        "  audio: {} samples, peak {:.3}, rms {:.4}, level changes on {:.0}% of the samples",
        audio.samples,
        audio.peak,
        audio.rms,
        audio.changing * 100.0
    );
    println!(
        "  fingerprints: pictures {:016x}, audio {:016x}, state {:016x}",
        report.picture_hash, report.audio_hash, report.state_hash
    );

    let mut pictures = Vec::new();
    for (frame, rgba) in &session.shots {
        let path = out.join(format!("frame_{frame:05}.png"));
        fs::write(&path, png::encode_rgba(256, 240, rgba))
            .unwrap_or_else(|error| eprintln!("{}: {error}", path.display()));
        pictures.push(rgba.as_slice());
    }
    if !pictures.is_empty() {
        let (width, height, sheet) = png::contact_sheet(&pictures, 256, 240, 4);
        let path = out.join("sheet.png");
        fs::write(&path, png::encode_rgba(width, height, &sheet))
            .unwrap_or_else(|error| eprintln!("{}: {error}", path.display()));
        println!("  pictures: {} (contact sheet: sheet.png)", out.display());
    }
}
