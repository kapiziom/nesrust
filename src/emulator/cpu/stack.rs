use crate::emulator::bus::cpu_bus::CpuBus;
use crate::emulator::cpu::CPU;

impl<B: CpuBus> CPU<B> {
    pub(super) fn push_stack(&mut self, value: u8) {
        self.write(0x0100 | self.stack_pointer as u16, value);
        self.stack_pointer = self.stack_pointer.wrapping_sub(1);
    }

    pub(super) fn push_stack_u16(&mut self, value: u16) {
        self.push_stack((value >> 8) as u8);
        self.push_stack((value & 0xFF) as u8);
    }

    pub(super) fn pop_stack(&mut self) -> u8 {
        self.stack_pointer = self.stack_pointer.wrapping_add(1);
        self.read(0x0100 | self.stack_pointer as u16)
    }

    /// Dummy read of the current stack slot (the CPU does it while incrementing SP in PLA/PLP/RTS/RTI).
    pub(super) fn stack_dummy_read(&mut self) {
        self.read(0x0100 | self.stack_pointer as u16);
    }
}
