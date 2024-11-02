use crate::emulator::cpu::CPU;

pub trait StackOperations {
    fn push_stack(&mut self, value: u8);
    fn pop_stack(&mut self) -> u8;
}
impl<'a> StackOperations for CPU<'a> {
    fn push_stack(&mut self, value: u8) {
        self.bus.write(0x0100 + self.stack_pointer as u16, value);
        self.stack_pointer = self.stack_pointer.wrapping_sub(1);
    }

    fn pop_stack(&mut self) -> u8 {
        self.stack_pointer = self.stack_pointer.wrapping_add(1);
        self.bus.read(0x0100 + self.stack_pointer as u16)
    }
}