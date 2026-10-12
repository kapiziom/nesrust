use std::{env, fs, process};
use nesrs::emulator::bus::Bus;
use nesrs::emulator::cpu::CPU;
use nesrs::emulator::rom::ROM;

fn main() {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: nesrs <rom.nes>");
        process::exit(1);
    };

    let rom_data = fs::read(&path).expect("Could not read ROM file");
    let rom = ROM::from_nes_file(&rom_data).expect("Failed to parse NES ROM");

    let bus = Bus::new(rom);
    let mut cpu = CPU::new(Box::new(bus));

    cpu.program_counter = 0x8000;

    cpu.interpret(0xFFFF);

    println!("program end");
}
