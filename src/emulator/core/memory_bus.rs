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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_read_write() {
        let mut bus = MemoryBus::new();
        bus.write(0x2000, 0x42);
        let value = bus.read(0x2000);

        assert_eq!(value, 0x42);
    }

    #[test]
    fn test_memory_write_read_u16() {
        let mut bus = MemoryBus::new();

        bus.write(0x2000, 0x42);
        bus.write(0x2001, 0x42);
        assert_eq!(bus.memory[0x2000], 0x42);
        assert_eq!(bus.memory[0x2001], 0x42);

        let value0 = bus.read(0x2000);
        assert_eq!(value0, 0x42);
        let value1 = bus.read(0x2001);
        assert_eq!(value1, 0x42);

        let value2 = bus.read_u16(0x2000);
        assert_eq!(value2, 0x4242);
    }

    #[test]
    fn test_memory_read_write_u16() {
        let mut bus = MemoryBus::new();
        bus.write_u16(0x3000, 0x1234);
        let value = bus.read_u16(0x3000);

        assert_eq!(value, 0x1234);
    }
}
