use std::fs;
use nesrs::emulator::bus::Bus;
use nesrs::emulator::bus::cpu_bus::CpuBus;
use nesrs::emulator::cpu::CPU;
use nesrs::emulator::rom::ROM;

fn main() {
    let rom_data = fs::read("./test_rom/test_program.nes").expect("Could not read ROM file");
    let rom = ROM::from_nes_file(&rom_data).expect("Failed to parse NES ROM");

    let bus = Bus::new(rom);
    let mut cpu = CPU::new(Box::new(bus));

    cpu.program_counter = 0x8000;

    cpu.interpret(0xFFFF);

    println!("program end");
}
