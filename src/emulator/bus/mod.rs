pub mod cpu_bus;
pub mod mock_bus;

use crate::emulator::bus::cpu_bus::CpuBus;
use crate::emulator::ppu::PPU;
use crate::emulator::ram::RAM;
use crate::emulator::rom::ROM;

pub struct Bus {
    ram: RAM,
    ppu: PPU,
    pub rom: ROM,
    pub nmi_interrupt: Option<u8>,
    cycles: usize,
    // todo
    // apu: APU,
}

impl Bus {
    pub fn new(rom: ROM) -> Self {
        Self {
            ram: RAM::new(),
            ppu: PPU::new(),
            rom,
            nmi_interrupt: None,
            cycles: 0
        }
    }

    pub fn get_rom_data(&self) -> &[u8] {
        &self.rom.prg_rom
    }
}
