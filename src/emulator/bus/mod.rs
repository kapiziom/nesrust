pub mod cpu_bus;
pub mod mock_bus;

use crate::emulator::ppu::PPU;
use crate::emulator::cartridge::Cartridge;
use crate::emulator::ram::RAM;

pub struct Bus {
    ram: RAM,
    ppu: PPU,
    pub cartridge: Cartridge,
    pub nmi_interrupt: Option<u8>,
    cycles: usize,
    /// Last value that travelled over the data bus; returned by reads of unmapped addresses (open bus).
    last_bus_value: u8,
    // todo
    // apu: APU,
}

impl Bus {
    pub fn new(cartridge: Cartridge) -> Self {
        Self {
            ram: RAM::new(),
            ppu: PPU::new(),
            cartridge,
            nmi_interrupt: None,
            cycles: 0,
            last_bus_value: 0,
        }
    }

    pub fn tick(&mut self, cycles: u16) {
        self.cycles += cycles as usize;

        for _ in 0..cycles {
            self.cartridge.cpu_cycle();

            // 3x PPU = 1x CPU
            for _ in 0..3 {
                self.ppu.tick();

                if self.ppu.fetch_nmi() {
                    self.nmi_interrupt = Some(0xFF);
                }
            }
        }
    }
}
