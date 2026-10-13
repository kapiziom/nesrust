//! Whole-console tests with tiny hand-assembled programs: bus routing, NMI delivery, OAM DMA, frame pacing.

use nesrs::emulator::cartridge::Cartridge;
use nesrs::emulator::nes::Nes;

/// Builds an NROM-128 cartridge whose PRG starts at $8000 with `program` and the given interrupt vectors.
fn nes_with(program: &[u8], nmi: u16, reset: u16) -> Nes {
    let mut data = vec![0u8; 16 + 16384 + 8192];
    data[0..4].copy_from_slice(b"NES\x1A");
    data[4] = 1;
    data[5] = 1;
    data[16..16 + program.len()].copy_from_slice(program);

    let vectors = 16 + 0x3FFA;
    data[vectors..vectors + 2].copy_from_slice(&nmi.to_le_bytes());
    data[vectors + 2..vectors + 4].copy_from_slice(&reset.to_le_bytes());
    data[vectors + 4..vectors + 6].copy_from_slice(&reset.to_le_bytes());

    Nes::new(Cartridge::from_bytes(&data).unwrap())
}

#[test]
fn reset_starts_execution_at_the_reset_vector() {
    let nes = nes_with(&[0xEA], 0x8000, 0x8123);

    assert_eq!(nes.cpu.program_counter, 0x8123);
    assert_eq!(nes.cpu.cycles, 7, "the reset sequence takes 7 cycles");
}

#[test]
fn a_frame_lasts_about_29781_cpu_cycles() {
    // JMP $8000 forever
    let mut nes = nes_with(&[0x4C, 0x00, 0x80], 0x8000, 0x8000);

    assert!(nes.run_frame());
    let first = nes.cpu.cycles;
    assert!(nes.run_frame());
    let second = nes.cpu.cycles - first;
    assert!(nes.run_frame());
    let third = nes.cpu.cycles - first - second;

    for cycles in [second, third] {
        // a frame is 29780.67 CPU cycles; run_frame stops at an instruction boundary (JMP = 3 cycles)
        assert!((29778..=29784).contains(&cycles), "frame took {cycles} cycles");
    }
}

#[test]
fn nmi_handler_runs_once_per_frame_when_enabled() {
    #[rustfmt::skip]
    let program = [
        0xA9, 0x80,             // $8000 LDA #$80
        0x8D, 0x00, 0x20,       // $8002 STA $2000   enable NMI
        0x4C, 0x05, 0x80,       // $8005 JMP $8005
        0xE6, 0x00,             // $8008 NMI: INC $00
        0x40,                   // $800A RTI
    ];
    let mut nes = nes_with(&program, 0x8008, 0x8000);

    for _ in 0..10 {
        assert!(nes.run_frame());
    }

    let count = nes.cpu.peek(0x0000);
    assert!((9..=10).contains(&count), "NMI count after 10 frames: {count}");
}

#[test]
fn nmi_does_not_run_while_disabled() {
    let mut nes = nes_with(&[0x4C, 0x00, 0x80, 0, 0, 0, 0, 0, 0xE6, 0x00, 0x40], 0x8008, 0x8000);

    for _ in 0..5 {
        nes.run_frame();
    }

    assert_eq!(nes.cpu.peek(0x0000), 0);
}

#[test]
fn ram_is_mirrored_every_2kb_and_ppu_registers_every_8_bytes() {
    let mut nes = nes_with(&[0xEA], 0x8000, 0x8000);

    nes.cpu.poke(0x0003, 0x42);
    assert_eq!(nes.cpu.peek(0x0803), 0x42);
    assert_eq!(nes.cpu.peek(0x1803), 0x42);

    // $2003/$2004 are OAMADDR/OAMDATA; $200B/$200C mirror them
    nes.cpu.poke(0x200B, 0x10);
    nes.cpu.poke(0x2004, 0x77);
    nes.cpu.poke(0x3FFB, 0x10);
    assert_eq!(nes.cpu.peek(0x200C), 0x77);
}

#[test]
fn unmapped_reads_return_the_last_bus_value() {
    let mut nes = nes_with(&[0xEA], 0x8000, 0x8000);

    nes.cpu.poke(0x0010, 0xA5);
    assert_eq!(nes.cpu.peek(0x0010), 0xA5);

    assert_eq!(nes.cpu.peek(0x5000), 0xA5, "nothing is mapped at $5000: open bus");
    assert_eq!(nes.cpu.peek(0x4018), 0xA5);
}

#[test]
fn prg_ram_works_at_6000() {
    let mut nes = nes_with(&[0xEA], 0x8000, 0x8000);

    nes.cpu.poke(0x6000, 0x99);

    assert_eq!(nes.cpu.peek(0x6000), 0x99);
}

#[test]
fn oam_dma_copies_a_page_and_stalls_the_cpu_for_513_or_514_cycles() {
    #[rustfmt::skip]
    let program = [
        0xA9, 0x02,             // LDA #$02
        0x8D, 0x14, 0x40,       // STA $4014
        0x4C, 0x05, 0x80,       // JMP $8005
    ];
    let mut nes = nes_with(&program, 0x8000, 0x8000);

    for index in 0..256u16 {
        nes.cpu.poke(0x0200 + index, index as u8 ^ 0x5A);
    }

    assert!(nes.cpu.step()); // LDA
    let before = nes.cpu.cycles;
    assert!(nes.cpu.step()); // STA $4014 (4 cycles) + DMA
    let elapsed = nes.cpu.cycles - before;

    assert!(elapsed == 4 + 513 || elapsed == 4 + 514, "STA $4014 took {elapsed} cycles");

    for index in 0..256u16 {
        nes.cpu.poke(0x2003, index as u8);
        let expected = if index % 4 == 2 { (index as u8 ^ 0x5A) & 0xE3 } else { index as u8 ^ 0x5A };
        assert_eq!(nes.cpu.peek(0x2004), expected, "OAM byte {index}");
    }
}

#[test]
fn oam_dma_starts_from_the_oam_address() {
    let program = [0xA9, 0x03, 0x8D, 0x14, 0x40, 0x4C, 0x05, 0x80];
    let mut nes = nes_with(&program, 0x8000, 0x8000);
    for index in 0..256u16 {
        nes.cpu.poke(0x0300 + index, 0x10);
    }
    nes.cpu.poke(0x2003, 0x20);

    nes.cpu.step();
    nes.cpu.step();

    // 256 bytes written starting at $20 wrap around: every byte was written exactly once
    nes.cpu.poke(0x2003, 0x00);
    assert_eq!(nes.cpu.peek(0x2004), 0x10);
    nes.cpu.poke(0x2003, 0x20);
    assert_eq!(nes.cpu.peek(0x2004), 0x10);
}

#[test]
fn framebuffer_is_exposed_through_the_console() {
    let mut nes = nes_with(&[0x4C, 0x00, 0x80], 0x8000, 0x8000);
    nes.run_frame();

    assert_eq!(nes.framebuffer().len(), 256 * 240 * 4);
    assert_eq!(nes.framebuffer()[3], 0xFF, "alpha is opaque");
}

#[test]
fn run_frame_reports_a_stopped_cpu() {
    // $02 is a KIL opcode, which is not implemented
    let mut nes = nes_with(&[0x02], 0x8000, 0x8000);

    assert!(!nes.run_frame());
}
