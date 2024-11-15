use crate::emulator::bus::cpu_bus::CpuBus;

pub struct MockBus {
    pub memory: [u8; 0x10000],
    pub nmi: Option<u8>,
    pub cycles: usize,
}

impl MockBus {
    pub fn new() -> Self {
        Self {
            memory: [0; 0x10000],
            nmi: None,
            cycles: 0
        }
    }
}

impl CpuBus for MockBus {
    fn read(&self, addr: u16) -> u8 {
        self.memory[addr as usize]
    }

    fn write(&mut self, addr: u16, data: u8) {
        self.memory[addr as usize] = data;
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_read_write() {
        let mut bus = MockBus::new();

        bus.write(0x2000, 0x42);
        assert_eq!(bus.memory[0x2000], 0x42);

        let value = bus.read(0x2000);
        assert_eq!(value, 0x42);
    }

    #[test]
    fn test_memory_write_read_u16() {
        let mut bus = MockBus::new();

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