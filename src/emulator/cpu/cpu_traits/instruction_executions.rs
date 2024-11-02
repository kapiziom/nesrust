use crate::emulator::cpu::addressing::AddressingMode;
use crate::emulator::cpu::CPU;
use crate::emulator::cpu::cpu_flags::CpuFlags;
use crate::emulator::cpu::flag_operations::FlagOperations;

pub trait InstructionExecutions {
    fn adc(&mut self, mode: &AddressingMode, program: &[u8]);
}

impl<'a> InstructionExecutions for CPU<'a> {
    fn adc(&mut self, mode: &AddressingMode, program: &[u8]) {
        let operand = self.get_operand(mode, program);

        let carry = self.get_flag_value(CpuFlags::CARRY);

        let sum = (self.register_a as u16) + (operand as u16) + carry;

        let overflow = (!(self.register_a ^ operand) & (self.register_a ^ sum as u8) & 0x80) != 0;

        // Set the carry flag if the result exceeded 8 bits
        self.set_flag(CpuFlags::CARRY, sum > 0xFF);
        self.set_flag(CpuFlags::OVERFLOW, overflow);

        self.register_a = sum as u8;

        self.update_zero_and_negative_flags(self.register_a);
    }
}