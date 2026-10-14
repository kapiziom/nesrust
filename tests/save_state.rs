//! Save states: restoring a state must make the console continue exactly as the saved one did.

use nesrs::emulator::cartridge::{Cartridge, StateError};
use nesrs::emulator::controller::Buttons;
use nesrs::emulator::nes::Nes;

const NMI: u16 = 0x8014;

/// NROM-128 whose program enables NMI and rendering, starts a pulse tone and changes the scroll and the pitch every
/// frame, so CPU, RAM, PPU, APU and the frame counter all keep changing.
fn cartridge(mapper_byte: u8, filler: u8) -> Cartridge {
    #[rustfmt::skip]
    let program = [
        0xA9, 0x80, 0x8D, 0x00, 0x20, // LDA #$80 : STA $2000
        0xA9, 0x1E, 0x8D, 0x01, 0x20, // LDA #$1E : STA $2001
        0xA9, 0x0F, 0x8D, 0x15, 0x40, // LDA #$0F : STA $4015
        0xE6, 0x00, 0x4C, 0x0F, 0x80, // loop: INC $00 : JMP loop
        // NMI at $8014
        0xE6, 0x01,                   // INC $01
        0xA5, 0x01,                   // LDA $01
        0x8D, 0x05, 0x20, 0x8D, 0x05, 0x20, // STA $2005 twice
        0x8D, 0x02, 0x40,             // STA $4002
        0xA9, 0xBF, 0x8D, 0x00, 0x40, // LDA #$BF : STA $4000
        0xA9, 0x08, 0x8D, 0x03, 0x40, // LDA #$08 : STA $4003
        0x40,                         // RTI
    ];

    let mut data = vec![filler; 16 + 16384 + 8192];
    data[..16].fill(0);
    data[0..4].copy_from_slice(b"NES\x1A");
    data[4] = 1;
    data[5] = 1;
    data[6] = mapper_byte;
    data[16..16 + program.len()].copy_from_slice(&program);
    data[16 + program.len()..16 + 0x3FFA].fill(0xEA);

    let vectors = 16 + 0x3FFA;
    data[vectors..vectors + 2].copy_from_slice(&NMI.to_le_bytes());
    data[vectors + 2..vectors + 4].copy_from_slice(&0x8000u16.to_le_bytes());
    data[vectors + 4..vectors + 6].copy_from_slice(&0x8000u16.to_le_bytes());

    Cartridge::from_bytes(&data).unwrap()
}

fn console() -> Nes {
    let mut nes = Nes::new(cartridge(0, 0));
    nes.set_sample_rate(44_100);
    nes
}

/// Runs `frames` frames and returns the last picture and all audio produced.
fn run(nes: &mut Nes, frames: usize) -> (Vec<u8>, Vec<f32>) {
    let mut audio = Vec::new();
    for _ in 0..frames {
        assert!(nes.run_frame());
        nes.drain_audio(&mut audio);
    }
    (nes.framebuffer().to_vec(), audio)
}

#[test]
fn a_restored_console_continues_exactly_like_the_saved_one() {
    let mut original = console();
    run(&mut original, 20);
    let state = original.save_state();
    let expected = run(&mut original, 40);

    let mut restored = console();
    restored.load_state(&state).unwrap();
    let actual = run(&mut restored, 40);

    assert_eq!(actual.0, expected.0, "picture");
    assert_eq!(actual.1, expected.1, "audio");
    assert_eq!(restored.cpu.cycles, original.cpu.cycles);
    assert_eq!(restored.cpu.bus.ppu().frame(), original.cpu.bus.ppu().frame());
}

#[test]
fn loading_a_state_into_the_console_it_came_from_rewinds_it() {
    let mut nes = console();
    run(&mut nes, 10);
    let state = nes.save_state();
    let first = run(&mut nes, 30);

    nes.load_state(&state).unwrap();
    let second = run(&mut nes, 30);

    assert_eq!(first, second);
}

#[test]
fn held_buttons_are_part_of_the_state() {
    let mut nes = console();
    nes.set_buttons(0, Buttons::A | Buttons::START);
    run(&mut nes, 2);
    let state = nes.save_state();

    nes.set_buttons(0, Buttons::empty());
    nes.load_state(&state).unwrap();

    assert_eq!(nes.bus().buttons(0), Buttons::A | Buttons::START);
}

#[test]
fn the_audio_rate_and_the_picture_of_the_running_console_survive_a_load() {
    let mut nes = console();
    run(&mut nes, 3);
    let state = nes.save_state();

    let mut other = Nes::new(cartridge(0, 0));
    other.set_sample_rate(22_050);
    run(&mut other, 1);
    let picture = other.framebuffer().to_vec();
    other.load_state(&state).unwrap();

    assert_eq!(other.framebuffer(), &picture[..], "no frame has been drawn since the load");
    let mut audio = Vec::new();
    run(&mut other, 10);
    other.drain_audio(&mut audio);
    let (_, produced) = run(&mut other, 10);
    // 10 frames at ~60.1 Hz and 22.05 kHz is ~3670 samples (it would be ~7340 at 44.1 kHz)
    assert!((3500..3900).contains(&produced.len()), "{} samples", produced.len());
}

#[test]
fn a_state_of_another_game_is_refused_and_changes_nothing() {
    let mut nes = console();
    run(&mut nes, 5);
    let state = nes.save_state();

    let mut other = Nes::new(cartridge(0, 0x11)); // different ROM contents (the filler differs)
    run(&mut other, 3);
    let before = other.save_state();

    assert_eq!(other.load_state(&state), Err(StateError::WrongCartridge));
    assert_eq!(other.save_state(), before);
}

#[test]
fn damaged_states_are_refused_and_change_nothing() {
    let mut nes = console();
    run(&mut nes, 5);
    let state = nes.save_state();
    let before = nes.save_state();

    assert_eq!(nes.load_state(&[]), Err(StateError::Corrupt));
    assert_eq!(nes.load_state(&state[..state.len() / 2]), Err(StateError::Corrupt));
    let mut wrong_magic = state.clone();
    wrong_magic[0] ^= 0xFF;
    assert_eq!(nes.load_state(&wrong_magic), Err(StateError::Corrupt));
    let mut wrong_version = state.clone();
    wrong_version[4] = wrong_version[4].wrapping_add(1);
    assert_eq!(nes.load_state(&wrong_version), Err(StateError::WrongVersion));

    assert_eq!(nes.save_state(), before);
}

/// Real games (the optional, git-ignored `.games` folder): each one must also continue identically after a load,
/// which exercises every mapper that is present.
#[test]
fn local_games_continue_identically_after_a_load() {
    let Ok(entries) = std::fs::read_dir(".games") else {
        eprintln!(".games not found, skipping");
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("nes") {
            continue;
        }
        let Ok(cartridge) = Cartridge::from_bytes(&std::fs::read(&path).unwrap()) else {
            continue;
        };

        let mut nes = Nes::new(cartridge);
        nes.set_sample_rate(44_100);
        run(&mut nes, 150);
        let state = nes.save_state();
        let expected = run(&mut nes, 100);

        nes.load_state(&state).unwrap();
        let actual = run(&mut nes, 100);

        assert!(actual == expected, "{}: diverged after the load", path.display());
        eprintln!("{}: ok, state of {} bytes", path.display(), state.len());
    }
}
