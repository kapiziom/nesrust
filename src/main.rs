use std::{env, fs, process};
use nesrs::emulator::bus::Bus;
use nesrs::emulator::cpu::CPU;
use nesrs::emulator::cartridge::Cartridge;

fn main() {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: nesrs <rom.nes>");
        process::exit(1);
    };

    let rom_data = fs::read(&path).expect("Could not read ROM file");
    let cartridge = match Cartridge::from_bytes(&rom_data) {
        Ok(cartridge) => cartridge,
        Err(error) => {
            eprintln!("cannot load {path}: {error}");
            process::exit(1);
        }
    };

    let bus = Bus::new(cartridge);
    let mut cpu = CPU::new(bus);

    cpu.program_counter = 0x8000;

    cpu.interpret(0xFFFF);

    println!("program end");
}
