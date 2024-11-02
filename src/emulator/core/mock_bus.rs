use crate::emulator::core::cpu_bus::{CpuBus, read_u16_core, write_u16_core};

struct MockBus {
    pub memory: [u8; 0x10000], // 64 KB
    pub nmi: Option<u8>, // non-maskable interrupt
}

impl MockBus {
    pub fn new() -> Self {
        Self {
            memory: [0; 0x10000],
            nmi: None,
        }
    }
}

impl CpuBus for MockBus {
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
        let mut bus = MockBus::new();
        bus.write(0x2000, 0x42);
        let value = bus.read(0x2000);

        assert_eq!(value, 0x42);
    }

    #[test]
    fn test_memory_read_write_u16() {
        let mut bus = MockBus::new();
        bus.write_u16(0x3000, 0x1234);
        let value = bus.read_u16(0x3000);

        assert_eq!(value, 0x1234);
    }

    #[test]
    fn test_non_maskable_interrupt() {
        let mut bus = MockBus::new();
        bus.nmi = Some(0x10);
        assert_eq!(bus.nmi, Some(0x10));
    }
}
