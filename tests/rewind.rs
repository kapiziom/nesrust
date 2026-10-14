//! Rewinding a real NES: states, pictures and the continuation of the game after going back.

use nesrs::emulator::cartridge::Cartridge;
use nesrs::emulator::nes::Nes;
use nesrs::emulator::rewind::Rewind;

/// NROM that counts frames in $00 from its NMI handler and shows the count as the backdrop colour.
fn counting_console() -> Nes {
    #[rustfmt::skip]
    let program = [
        0xA9, 0x80, 0x8D, 0x00, 0x20, // LDA #$80 : STA $2000 (NMI on)
        0x4C, 0x05, 0x80,             // loop: JMP loop
        // NMI at $8008
        0xE6, 0x00,                   // INC $00
        0xA9, 0x3F, 0x8D, 0x06, 0x20, // PPUADDR = $3F00
        0xA9, 0x00, 0x8D, 0x06, 0x20,
        0xA5, 0x00, 0x8D, 0x07, 0x20, // palette[0] = counter
        0xA9, 0x00, 0x8D, 0x06, 0x20, // PPUADDR = $0000 (rendering is off: the backdrop shows palette[0])
        0x8D, 0x06, 0x20,
        0x40,                         // RTI
    ];
    let mut data = vec![0u8; 16 + 16384 + 8192];
    data[0..4].copy_from_slice(b"NES\x1A");
    data[4] = 1;
    data[5] = 1;
    data[16..16 + program.len()].copy_from_slice(&program);
    let vectors = 16 + 0x3FFA;
    data[vectors..vectors + 2].copy_from_slice(&0x8008u16.to_le_bytes());
    data[vectors + 2..vectors + 4].copy_from_slice(&0x8000u16.to_le_bytes());
    Nes::new(Cartridge::from_bytes(&data).unwrap())
}

fn frame_counter(nes: &mut Nes) -> u8 {
    nes.cpu.peek(0)
}

#[test]
fn stepping_back_returns_to_the_state_before_each_frame() {
    let mut nes = counting_console();
    let mut rewind = Rewind::new(100);
    for _ in 0..10 {
        rewind.push(&nes);
        nes.run_frame();
    }
    let reached = frame_counter(&mut nes);
    assert!(reached >= 8);

    assert!(rewind.step_back(&mut nes));
    assert_eq!(frame_counter(&mut nes), reached - 1);
    assert!(rewind.step_back(&mut nes));
    assert_eq!(frame_counter(&mut nes), reached - 2);
    assert_eq!(rewind.len(), 8);
}

#[test]
fn playing_on_after_rewinding_repeats_what_happened() {
    let mut nes = counting_console();
    let mut rewind = Rewind::new(100);
    for _ in 0..12 {
        rewind.push(&nes);
        nes.run_frame();
    }
    let end = nes.save_state();

    for _ in 0..5 {
        assert!(rewind.step_back(&mut nes));
    }
    for _ in 0..5 {
        nes.run_frame();
    }

    assert_eq!(nes.save_state(), end);
}

#[test]
fn the_oldest_states_are_dropped_and_an_empty_buffer_stops() {
    let mut nes = counting_console();
    let mut rewind = Rewind::new(3);
    for _ in 0..10 {
        rewind.push(&nes);
        nes.run_frame();
    }
    assert_eq!(rewind.len(), 3);

    for _ in 0..3 {
        assert!(rewind.step_back(&mut nes));
    }
    assert!(!rewind.step_back(&mut nes));
    assert!(rewind.is_empty());
}

#[test]
fn the_picture_is_refreshed_while_rewinding() {
    let mut nes = counting_console();
    let mut rewind = Rewind::new(20);
    let mut pictures = Vec::new();
    for _ in 0..8 {
        rewind.push(&nes);
        nes.run_frame();
        pictures.push(nes.framebuffer().to_vec());
    }
    assert_ne!(pictures[6], pictures[7], "the backdrop changes every frame");

    for frame in (4..8).rev() {
        assert!(rewind.step_back(&mut nes));
        assert_eq!(nes.framebuffer(), &pictures[frame][..], "frame {frame}");
    }
}
