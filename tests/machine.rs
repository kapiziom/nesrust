//! The NES seen only through `emu_core::Machine`, the way a console-independent frontend would use it.

use emu_core::{Machine, StateError};
use nesrs::emulator::cartridge::Cartridge;
use nesrs::emulator::nes::Nes;

fn nes() -> Nes {
    // NROM-128 that spins forever
    let mut data = vec![0u8; 16 + 16384 + 8192];
    data[0..4].copy_from_slice(b"NES\x1A");
    data[4] = 1;
    data[5] = 1;
    data[16..19].copy_from_slice(&[0x4C, 0x00, 0x80]);
    let vectors = 16 + 0x3FFA;
    data[vectors + 2..vectors + 4].copy_from_slice(&0x8000u16.to_le_bytes());
    Nes::new(Cartridge::from_bytes(&data).unwrap())
}

/// Everything a frontend does in one frame of its loop, written against the trait only.
fn run_a_frame_of_a_frontend(machine: &mut impl Machine) -> (usize, usize, usize) {
    machine.set_audio_sample_rate(48_000);
    machine.set_input(0, 0b1);
    assert!(machine.run_frame());
    let mut audio = Vec::new();
    machine.drain_audio(&mut audio);
    let video = machine.video();
    (video.width * video.height * 4 - video.rgba.len(), video.width, audio.len())
}

#[test]
fn a_frontend_can_drive_the_nes_through_the_trait() {
    let mut nes = nes();

    let (missing_pixels, width, audio) = run_a_frame_of_a_frontend(&mut nes);

    assert_eq!(missing_pixels, 0, "the picture is width * height RGBA pixels");
    assert_eq!(width, 256);
    assert!((700..900).contains(&audio), "{audio} samples for one 60 Hz frame at 48 kHz");
}

#[test]
fn the_trait_describes_the_nes_controller_and_timing() {
    let nes = nes();

    let names: Vec<_> = nes.buttons().iter().map(|button| button.name).collect();
    assert_eq!(names, ["A", "B", "Select", "Start", "Up", "Down", "Left", "Right"]);
    assert!((nes.frame_rate() - 60.0988).abs() < 1e-3);
}

#[test]
fn input_is_a_bit_mask_over_the_listed_buttons() {
    let mut nes = nes();
    let start = nes.buttons().iter().find(|button| button.name == "Start").unwrap().bit;

    Machine::set_input(&mut nes, 0, 1 << start);

    assert_eq!(nes.bus().buttons(0), nesrs::emulator::controller::Buttons::START);
}

#[test]
fn states_and_their_errors_come_through_the_trait() {
    let mut nes = nes();
    nes.run_frame();
    let state = Machine::save_state(&nes);

    assert_eq!(Machine::load_state(&mut nes, &state), Ok(()));
    assert_eq!(Machine::load_state(&mut nes, &[1, 2, 3]), Err(StateError::Corrupt));
}
