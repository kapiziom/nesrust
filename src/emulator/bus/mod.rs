pub mod cpu_bus;
pub mod mock_bus;

use crate::emulator::ppu::PPU;
use crate::emulator::ram::RAM;
use crate::emulator::rom::ROM;

pub struct Bus {
    ram: RAM,
    ppu: PPU,
    pub rom: ROM,
    pub nmi_interrupt: Option<u8>,
    cycles: usize,
    /// Last value that travelled over the data bus; returned by reads of unmapped addresses (open bus).
    last_bus_value: u8,
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
            cycles: 0,
            last_bus_value: 0,
        }
    }

    pub fn get_rom_data(&self) -> &[u8] {
        &self.rom.prg_rom
    }

    pub fn tick(&mut self, cycles: u16) {
        self.cycles += cycles as usize;

        // 3x PPU = 1x CPU
        for _ in 0..(cycles * 3) {
            self.ppu.tick();

            if self.ppu.fetch_nmi() {
                self.nmi_interrupt = Some(0xFF);
            }
        }
    }

}