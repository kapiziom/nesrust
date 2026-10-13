use crate::emulator::bus::cpu_bus::CpuBus;
use crate::emulator::cpu::CPU;
use crate::emulator::cpu::flags::CpuFlags;

// ALU, flag and control-flow operations. Operand fetching and the cycles of the addressing modes
// live in addressing.rs; the dispatch by mnemonic lives in mod.rs.
impl<B: CpuBus> CPU<B> {
    pub (super) fn set_register_a(&mut self, data: u8) {
        self.register_a = data;
        self.update_zero_and_negative_flags(data);
    }

    pub (super) fn set_register_x(&mut self, data: u8) {
        self.register_x = data;
        self.update_zero_and_negative_flags(data);
    }

    pub (super) fn set_register_y(&mut self, data: u8) {
        self.register_y = data;
        self.update_zero_and_negative_flags(data);
    }

    pub (super) fn adc(&mut self, operand: u8) {
        let carry = self.get_flag_value(CpuFlags::CARRY);

        let sum = (self.register_a as u16) + (operand as u16) + carry;

        let overflow = (!(self.register_a ^ operand) & (self.register_a ^ sum as u8) & 0x80) != 0;

        self.set_flag(CpuFlags::CARRY, sum > 0xFF);
        self.set_flag(CpuFlags::OVERFLOW, overflow);

        self.set_register_a(sum as u8);
    }

    pub (super) fn sbc(&mut self, value: u8) {
        let carry = if self.contains_flag(CpuFlags::CARRY) { 0 } else { 1 };

        let result = self.register_a as i16 - value as i16 - carry as i16;

        self.set_flag(CpuFlags::CARRY, result >= 0);

        let result = result as u8;
        self.set_flag(CpuFlags::OVERFLOW, (self.register_a ^ value) & (self.register_a ^ result) & 0x80 != 0);

        self.set_register_a(result);
    }

    pub (super) fn and(&mut self, value: u8) {
        self.set_register_a(self.register_a & value);
    }

    pub (super) fn ora(&mut self, value: u8) {
        self.set_register_a(self.register_a | value);
    }

    pub (super) fn eor(&mut self, value: u8) {
        self.set_register_a(self.register_a ^ value);
    }

    pub (super) fn compare(&mut self, register: u8, value: u8) {
        let result = register.wrapping_sub(value);

        self.set_flag(CpuFlags::CARRY, register >= value);
        self.set_flag(CpuFlags::ZERO, register == value);
        self.set_flag(CpuFlags::NEGATIVE, result & 0x80 != 0);
    }

    pub (super) fn bit(&mut self, value: u8) {
        self.set_flag(CpuFlags::NEGATIVE, value & 0b1000_0000 != 0);
        self.set_flag(CpuFlags::OVERFLOW, value & 0b0100_0000 != 0);
        self.set_flag(CpuFlags::ZERO, self.register_a & value == 0);
    }

    pub (super) fn asl(&mut self, value: u8) -> u8 {
        let result = value << 1;
        self.set_flag(CpuFlags::CARRY, value & 0x80 != 0);
        self.update_zero_and_negative_flags(result);
        result
    }

    pub (super) fn lsr(&mut self, value: u8) -> u8 {
        let result = value >> 1;
        self.set_flag(CpuFlags::CARRY, value & 0x01 != 0);
        self.update_zero_and_negative_flags(result);
        result
    }

    pub (super) fn rol(&mut self, value: u8) -> u8 {
        let result = (value << 1) | self.get_flag_value(CpuFlags::CARRY) as u8;
        self.set_flag(CpuFlags::CARRY, value & 0x80 != 0);
        self.update_zero_and_negative_flags(result);
        result
    }

    pub (super) fn ror(&mut self, value: u8) -> u8 {
        let result = (value >> 1) | ((self.get_flag_value(CpuFlags::CARRY) as u8) << 7);
        self.set_flag(CpuFlags::CARRY, value & 0x01 != 0);
        self.update_zero_and_negative_flags(result);
        result
    }

    pub (super) fn inc(&mut self, value: u8) -> u8 {
        let result = value.wrapping_add(1);
        self.update_zero_and_negative_flags(result);
        result
    }

    pub (super) fn dec(&mut self, value: u8) -> u8 {
        let result = value.wrapping_sub(1);
        self.update_zero_and_negative_flags(result);
        result
    }

    // unofficial opcodes

    pub (super) fn anc(&mut self, value: u8) {
        self.and(value);
        self.set_flag(CpuFlags::CARRY, self.register_a & 0x80 != 0);
    }

    pub (super) fn alr(&mut self, value: u8) {
        let anded = self.register_a & value;
        let result = self.lsr(anded);
        self.register_a = result;
    }

    pub (super) fn arr(&mut self, value: u8) {
        let carry_in = (self.get_flag_value(CpuFlags::CARRY) as u8) << 7;
        let result = ((self.register_a & value) >> 1) | carry_in;

        self.set_register_a(result);
        // C = bit 6, V = bit 6 xor bit 5 of the result
        self.set_flag(CpuFlags::CARRY, result & 0x40 != 0);
        self.set_flag(CpuFlags::OVERFLOW, ((result >> 6) ^ (result >> 5)) & 0x01 != 0);
    }

    pub (super) fn axs(&mut self, value: u8) {
        let anded = self.register_a & self.register_x;

        // flags as in CMP (no borrow-in); X receives the difference
        self.compare(anded, value);
        self.set_register_x(anded.wrapping_sub(value));
    }

    // control flow

    /// Relative branch. Taken: +1 cycle, +1 more when the target is on another page than the next instruction.
    pub (super) fn branch(&mut self, condition: bool) {
        let offset = self.fetch() as i8;

        if !condition {
            return;
        }

        self.read(self.program_counter); // dummy read of the next opcode

        let next_instruction = self.program_counter;
        let target = next_instruction.wrapping_add(offset as u16);

        if (next_instruction & 0xFF00) != (target & 0xFF00) {
            // the low byte is already corrected, the high byte is not yet
            self.read((next_instruction & 0xFF00) | (target & 0x00FF));
        }

        self.program_counter = target;
    }

    pub (super) fn jsr(&mut self) {
        let low = self.fetch() as u16;
        self.stack_dummy_read();
        // the pushed address is that of the last byte of the JSR instruction
        self.push_stack_u16(self.program_counter);
        let high = self.fetch() as u16;
        self.program_counter = (high << 8) | low;
    }

    pub (super) fn rts(&mut self) {
        self.implied();
        self.stack_dummy_read();
        let low = self.pop_stack() as u16;
        let high = self.pop_stack() as u16;
        self.program_counter = (high << 8) | low;
        self.read(self.program_counter); // dummy read while PC is incremented
        self.program_counter = self.program_counter.wrapping_add(1);
    }

    pub (super) fn rti(&mut self) {
        self.implied();
        self.stack_dummy_read();
        let status = self.pop_stack();
        self.set_status_from_stack(status);
        let low = self.pop_stack() as u16;
        let high = self.pop_stack() as u16;

        self.program_counter = (high << 8) | low;
    }

    pub (super) fn pha(&mut self) {
        self.implied();
        self.push_stack(self.register_a);
    }

    pub (super) fn php(&mut self) {
        self.implied();
        self.push_stack(self.flags.bits() | CpuFlags::BREAK.bits() | CpuFlags::UNUSED.bits());
    }

    pub (super) fn pla(&mut self) {
        self.implied();
        self.stack_dummy_read();
        let value = self.pop_stack();
        self.set_register_a(value);
    }

    pub (super) fn plp(&mut self) {
        self.implied();
        self.stack_dummy_read();
        let status = self.pop_stack();
        self.set_status_from_stack(status);
    }

    /// B does not exist in the register and bit 5 is always set, whatever was pulled.
    fn set_status_from_stack(&mut self, status: u8) {
        self.flags = CpuFlags::from_bits_truncate(status);
        self.clear_flag(CpuFlags::BREAK);
        self.insert_flag(CpuFlags::UNUSED);
    }
}
