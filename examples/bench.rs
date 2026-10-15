//! Measures how fast the parts of the emulator run (build with --release!):
//!
//!   cargo run --release --example bench -- [game.nes]
//!
//! A real NES executes 1.79 million CPU cycles, 5.37 million PPU dots and 60 frames per second. Every measurement is
//! repeated and the best run is reported, because other programs on the machine only ever make a run slower.

use std::time::Instant;
use std::{env, fs};

use mos6502::mock_bus::MockBus;
use mos6502::CPU;
use nesrs::emulator::apu::Apu;
use nesrs::emulator::cartridge::Cartridge;
use nesrs::emulator::nes::Nes;
use nesrs::emulator::ppu::PPU;

const REPEATS: usize = 7;

/// Runs `work` (which does `units` units of work) several times and prints the best rate.
fn measure(what: &str, units: f64, unit_name: &str, real_hz: f64, mut work: impl FnMut()) {
    let best = (0..REPEATS)
        .map(|_| {
            let started = Instant::now();
            work();
            started.elapsed().as_secs_f64()
        })
        .fold(f64::MAX, f64::min);
    let rate = units / best;
    println!("{what:<28} {:>7.1} M{unit_name}/s = {:>6.1}x real time", rate / 1e6, rate / real_hz);
}

fn blank_cartridge() -> Cartridge {
    let mut data = vec![0u8; 16 + 16384 + 8192];
    data[0..4].copy_from_slice(b"NES\x1A");
    data[4] = 1;
    data[5] = 1;
    data[16..19].copy_from_slice(&[0x4C, 0x00, 0x80]);
    data[16 + 0x3FFC..16 + 0x3FFE].copy_from_slice(&0x8000u16.to_le_bytes());
    Cartridge::from_bytes(&data).unwrap()
}

fn main() {
    let rom = env::args().nth(1).map(|path| fs::read(&path).expect("cannot read the ROM"));
    let cartridge = || match &rom {
        Some(data) => Cartridge::from_bytes(data).expect("cannot load the ROM"),
        None => blank_cartridge(),
    };

    // the CPU alone on a flat RAM: a loop of INX / BNE and a few other instructions
    measure("CPU only (flat RAM)", 10_000_000.0, "cycles", 1_789_773.0, || {
        let mut bus = MockBus::new();
        bus.load_program(&[0xE8, 0xD0, 0xFD, 0x18, 0x69, 0x01, 0x4C, 0x00, 0x80], 0x8000);
        let mut cpu = CPU::new(bus);
        cpu.program_counter = 0x8000;
        while cpu.cycles < 10_000_000 {
            cpu.step();
        }
    });

    for (what, mask) in
        [("PPU, rendering off", 0x00), ("PPU, background only", 0x0A), ("PPU, background+sprites", 0x1E)]
    {
        let mut cartridge = cartridge();
        let mut ppu = PPU::new();
        ppu.write_register(1, mask, &mut cartridge);
        measure(what, 5_000_000.0, "dots", 5_369_318.0, || {
            for _ in 0..5_000_000u32 {
                ppu.tick(&mut cartridge);
            }
        });
    }

    let mut apu = Apu::new(44_100);
    let mut samples = Vec::new();
    measure("APU only", 3_000_000.0, "cycles", 1_789_773.0, || {
        for cycle in 0..3_000_000u32 {
            apu.tick();
            if cycle % 30_000 == 0 {
                apu.drain_samples(&mut samples);
            }
        }
    });

    let mut nes = Nes::new(cartridge());
    nes.set_sample_rate(44_100);
    nes.cpu.poke(0x2001, 0x1E); // rendering on
    let mut audio = Vec::new();
    measure("whole console (frames)", 120.0, "frames", 60.0, || {
        for _ in 0..120 {
            nes.run_frame();
            audio.clear();
            nes.drain_audio(&mut audio);
        }
    });
}
