//! Whole-console tests with tiny hand-assembled programs: bus routing, NMI delivery, OAM DMA, frame pacing.

use nesrs::emulator::cartridge::Cartridge;
use nesrs::emulator::controller::Buttons;
use nesrs::emulator::region::Region;
use nesrs::emulator::nes::Nes;

/// Builds an NROM-128 cartridge whose PRG starts at $8000 with `program` and the given interrupt vectors.
fn nes_with(program: &[u8], nmi: u16, reset: u16) -> Nes {
    nes_with_region(program, nmi, reset, false)
}

/// Same, flagged as a PAL cartridge in its NES 2.0 header (byte 12 = 1) when `pal` is set.
fn nes_with_region(program: &[u8], nmi: u16, reset: u16, pal: bool) -> Nes {
    let mut data = vec![0u8; 16 + 16384 + 8192];
    if pal {
        data[7] = 0x08; // NES 2.0
        data[12] = 1;
    }
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

/// The usual way games read a controller: strobe, then eight reads of `port_address` stored in $00-$07
/// (the raw value, upper bits included), then loop forever.
fn controller_reader(port_address: u16) -> Nes {
    let [low, high] = port_address.to_le_bytes();
    #[rustfmt::skip]
    let program = [
        0xA9, 0x01,                 // $8000 LDA #$01
        0x8D, 0x16, 0x40,           // $8002 STA $4016   strobe high
        0xA9, 0x00,                 // $8005 LDA #$00
        0x8D, 0x16, 0x40,           // $8007 STA $4016   strobe low: latch
        0xA2, 0x00,                 // $800A LDX #$00
        0xAD, low, high,            // $800C LDA port    <- loop
        0x95, 0x00,                 // $800F STA $00,X
        0xE8,                       // $8011 INX
        0xE0, 0x08,                 // $8012 CPX #$08
        0xD0, 0xF6,                 // $8014 BNE $800C
        0x4C, 0x16, 0x80,           // $8016 JMP $8016
    ];
    nes_with(&program, 0x8000, 0x8000)
}

fn bytes_read_by_the_program(nes: &mut Nes) -> Vec<u8> {
    nes.run_frame();
    (0..8u16).map(|offset| nes.cpu.peek(offset)).collect()
}

#[test]
fn a_program_reads_controller_1_through_4016() {
    let mut nes = controller_reader(0x4016);
    nes.set_buttons(0, Buttons::A | Buttons::SELECT | Buttons::LEFT);

    let read = bytes_read_by_the_program(&mut nes);

    // data on bit 0; the upper bits are open bus: the high byte of the address ($40) was the last thing on the bus
    assert_eq!(read, [0x41, 0x40, 0x41, 0x40, 0x40, 0x40, 0x41, 0x40]);
}

#[test]
fn controller_2_is_read_through_4017_and_is_independent() {
    let mut nes = controller_reader(0x4017);
    nes.set_buttons(0, Buttons::A);
    nes.set_buttons(1, Buttons::B | Buttons::RIGHT);

    let read = bytes_read_by_the_program(&mut nes);

    assert_eq!(read, [0x40, 0x41, 0x40, 0x40, 0x40, 0x40, 0x40, 0x41]);
}

#[test]
fn unpressed_buttons_read_as_open_bus_only() {
    let mut nes = controller_reader(0x4016);

    assert_eq!(bytes_read_by_the_program(&mut nes), [0x40; 8]);
}

#[test]
fn the_strobe_write_latches_both_ports_and_changes_are_seen_on_the_next_strobe() {
    let mut nes = nes_with(&[0x4C, 0x00, 0x80], 0x8000, 0x8000);
    nes.set_buttons(0, Buttons::UP);
    nes.set_buttons(1, Buttons::DOWN);

    nes.cpu.poke(0x4016, 1);
    nes.cpu.poke(0x4016, 0);
    nes.set_buttons(0, Buttons::empty()); // released after the latch: still reported as pressed

    let first_port: Vec<u8> = (0..8).map(|_| nes.cpu.peek(0x4016) & 1).collect();
    let second_port: Vec<u8> = (0..8).map(|_| nes.cpu.peek(0x4017) & 1).collect();

    assert_eq!(first_port, [0, 0, 0, 0, 1, 0, 0, 0]);
    assert_eq!(second_port, [0, 0, 0, 0, 0, 1, 0, 0]);
    assert_eq!(nes.cpu.peek(0x4016) & 1, 1, "official controllers return 1 after the 8th read");
}

#[test]
fn setting_buttons_on_a_missing_port_is_ignored() {
    let mut nes = nes_with(&[0xEA], 0x8000, 0x8000);

    nes.set_buttons(2, Buttons::A); // there are only two ports

    assert_eq!(nes.cpu.peek(0x4016) & 1, 0);
}

#[test]
fn the_region_comes_from_the_cartridge_header() {
    assert_eq!(nes_with(&[0xEA], 0x8000, 0x8000).region(), Region::Ntsc);
    assert_eq!(nes_with_region(&[0xEA], 0x8000, 0x8000, true).region(), Region::Pal);
}

#[test]
fn a_pal_frame_is_longer_in_cpu_cycles_and_the_ppu_runs_3_2_dots_per_cycle() {
    // JMP $8000 forever
    let mut nes = nes_with_region(&[0x4C, 0x00, 0x80], 0x8000, 0x8000, true);

    nes.run_frame();
    let start = nes.cpu.cycles;
    for _ in 0..4 {
        nes.run_frame();
    }
    let average = (nes.cpu.cycles - start) as f64 / 4.0;

    // 312 * 341 dots / 3.2 dots per cycle = 33247.5 CPU cycles
    assert!((average - 33_247.5).abs() < 4.0, "a PAL frame took {average} CPU cycles");
}

#[test]
fn pal_frames_come_at_about_50_hz() {
    let mut nes = nes_with_region(&[0x4C, 0x00, 0x80], 0x8000, 0x8000, true);
    nes.run_frame();
    let start = nes.cpu.cycles;
    for _ in 0..50 {
        nes.run_frame();
    }

    let seconds = (nes.cpu.cycles - start) as f64 / Region::Pal.cpu_clock_hz();

    assert!((seconds - 50.0 / 50.007).abs() < 0.005, "50 PAL frames took {seconds} s");
}

#[test]
fn pal_audio_has_the_requested_sample_rate_per_second_of_console_time() {
    let mut nes = nes_with_region(&[0x4C, 0x00, 0x80], 0x8000, 0x8000, true);
    nes.set_sample_rate(48_000);
    let mut samples = Vec::new();
    nes.run_frame(); // the first frame starts mid-frame, right after the reset sequence
    nes.drain_audio(&mut samples);
    samples.clear();

    for _ in 0..50 {
        nes.run_frame();
        nes.drain_audio(&mut samples);
    }

    // 50 frames = 0.99986 s of PAL time
    assert!((samples.len() as f64 - 48_000.0 * 50.0 / 50.007).abs() < 20.0, "{} samples", samples.len());
}
