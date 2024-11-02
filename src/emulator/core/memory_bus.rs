use crate::emulator::core::cpu_bus::{CpuBus, read_u16_core, write_u16_core};

pub struct MemoryBus {
    memory: [u8; 0x10000], // 64 KB
}

impl MemoryBus {
    pub fn new() -> Self {
        MemoryBus {
            memory: [0; 0x10000],
        }
    }

    pub fn load_rom(&mut self, rom: &[u8], start_address: u16) {
        for (i, &byte) in rom.iter().enumerate() {
            self.memory[(start_address as usize + i) % 0x10000] = byte;
        }
    }
}

impl CpuBus for MemoryBus {
    fn read(&self, addr: u16) -> u8 {
        self.memory[addr as usize]
    }

    fn read_u16(&self, addr: u16) -> u16 {
        read_u16_core(self, addr)
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.memory[addr as usize] = value;
    }

    fn write_u16(&mut self, addr: u16, value: u16) {
        write_u16_core(self, addr, value)
    }
}