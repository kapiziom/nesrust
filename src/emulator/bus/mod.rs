pub mod cpu_bus;
pub mod mock_bus;

use crate::emulator::bus::cpu_bus::CpuBus;
use crate::emulator::memory::Memory;

pub struct Bus {
    pub memory: Memory,
    pub nmi_interrupt: Option<u8>,
    cycles: usize,
    // todo
    // ppu: PPU,
    // apu: APU,
}

impl Bus {
    pub fn new() -> Self {
        Self {
            memory: Memory::new(),
            nmi_interrupt: None,
            cycles: 0
        }
    }
}
