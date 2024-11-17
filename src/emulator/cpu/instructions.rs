
use crate::emulator::cpu::addressing::AddressingMode;
use crate::emulator::cpu::{AddressingModeOperations, CPU};
use crate::emulator::cpu::flags::{CpuFlags, FlagOperations};
use crate::emulator::cpu::stack::StackOperations;

pub trait CpuInstructions {
    fn adc(&mut self, mode: &AddressingMode);
    fn and(&mut self, mode: &AddressingMode);
    fn asl(&mut self, mode: &AddressingMode);
    fn bcc(&mut self);
    fn bcs(&mut self);
    fn beq(&mut self);
    fn bit(&mut self, mode: &AddressingMode);
    fn bmi(&mut self);
    fn bne(&mut self);
    fn bpl(&mut self);
    fn brk(&mut self);
    fn bvc(&mut self);
    fn bvs(&mut self);
    fn clc(&mut self);
    fn cld(&mut self);
    fn cli(&mut self);
    fn clv(&mut self);
    fn cmp(&mut self, mode: &AddressingMode);
    fn cpx(&mut self, mode: &AddressingMode);
    fn cpy(&mut self, mode: &AddressingMode);
    fn dec(&mut self, mode: &AddressingMode);
    fn dex(&mut self);
    fn dey(&mut self);
    fn eor(&mut self, mode: &AddressingMode);
    fn inc(&mut self, mode: &AddressingMode);
    fn inx(&mut self);
    fn iny(&mut self);
    fn jmp(&mut self, mode: &AddressingMode);
    fn jsr(&mut self);
    fn lda(&mut self, mode: &AddressingMode);
    fn ldx(&mut self, mode: &AddressingMode);
    fn ldy(&mut self, mode: &AddressingMode);
    fn lsr(&mut self, mode: &AddressingMode);
    fn nop(&mut self);
    fn ora(&mut self, mode: &AddressingMode);
    fn pha(&mut self);
    fn php(&mut self);
    fn pla(&mut self);
    fn plp(&mut self);
    fn rol(&mut self, mode: &AddressingMode);
    fn ror(&mut self, mode: &AddressingMode);
    fn rti(&mut self);
    fn rts(&mut self);
    fn sbc(&mut self, mode: &AddressingMode);
    fn sec(&mut self);
    fn sed(&mut self);
    fn sei(&mut self);
    fn sta(&mut self, mode: &AddressingMode);
    fn stx(&mut self, mode: &AddressingMode);
    fn sty(&mut self, mode: &AddressingMode);
    fn tax(&mut self);
    fn tay(&mut self);
    fn tsx(&mut self);
    fn txa(&mut self);
    fn txs(&mut self);
    fn tya(&mut self);
}

impl<'a> CpuInstructions for CPU<'a> {
    fn adc(&mut self, mode: &AddressingMode) {
        let operand = self.get_operand(mode);

        let carry = self.get_flag_value(CpuFlags::CARRY);

        let sum = (self.register_a as u16) + (operand as u16) + carry;

        let overflow = (!(self.register_a ^ operand) & (self.register_a ^ sum as u8) & 0x80) != 0;

        self.set_flag(CpuFlags::CARRY, sum > 0xFF);
        self.set_flag(CpuFlags::OVERFLOW, overflow);

        self.register_a = sum as u8;

        self.update_zero_and_negative_flags(self.register_a);
    }

    fn and(&mut self, mode: &AddressingMode) {
        let operand = self.get_operand(mode);

        self.register_a &= operand;

        self.update_zero_and_negative_flags(self.register_a);
    }

    fn asl(&mut self, mode: &AddressingMode) {
        let result: u8;

        match mode {
            AddressingMode::Accumulator => {
                let value = self.get_accumulator();
                self.set_flag(CpuFlags::CARRY, value & 0x80 != 0);
                result = value.wrapping_shl(1);
                self.register_a = result;
            },
            _ => {
                let addr = self.get_address(mode);
                let value = self.mem_read(addr);
                self.set_flag(CpuFlags::CARRY, value & 0x80 != 0);
                result = value.wrapping_shl(1);
                self.mem_write(addr, result)
            }
        }

        self.update_zero_and_negative_flags(result);
    }

    fn bcc(&mut self) {
        let should_branch = !self.contains_flag(CpuFlags::CARRY);
        self.branch_helper(should_branch);
    }

    fn bcs(&mut self) {
        let should_branch = self.contains_flag(CpuFlags::CARRY);
        self.branch_helper(should_branch);
    }

    fn beq(&mut self) {
        let should_branch = self.contains_flag(CpuFlags::ZERO);
        self.branch_helper(should_branch);
    }

    fn bit(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);

        if value & 0b1000_0000 != 0 {
            self.insert_flag(CpuFlags::NEGATIVE);
        } else {
            self.flags.remove(CpuFlags::NEGATIVE);
        }
        if value & 0b0100_0000 != 0 {
            self.insert_flag(CpuFlags::OVERFLOW);
        } else {
            self.flags.remove(CpuFlags::OVERFLOW);
        }

        if self.register_a & value == 0 {
            self.insert_flag(CpuFlags::ZERO);
        } else {
            self.flags.remove(CpuFlags::ZERO);
        }
    }

    fn bmi(&mut self) {
        let should_branch = self.contains_flag(CpuFlags::NEGATIVE);
        self.branch_helper(should_branch);
    }

    fn bne(&mut self) {
        let should_branch = !self.contains_flag(CpuFlags::ZERO);
        self.branch_helper(should_branch);
    }

    fn bpl(&mut self) {
        let should_branch = !self.contains_flag(CpuFlags::NEGATIVE);
        self.branch_helper(should_branch);
    }

    fn brk(&mut self) {
        self.program_counter = self.program_counter.wrapping_add(2);

        self.push_stack((self.program_counter >> 8) as u8);
        self.push_stack(self.program_counter as u8);

        self.insert_flag(CpuFlags::BREAK);
        self.insert_flag(CpuFlags::INTERRUPT_DISABLE);

        self.push_stack(self.flags.bits());

        let low_byte = self.mem_read(0xFFFE);
        let high_byte = self.mem_read(0xFFFF);
        self.program_counter = ((high_byte as u16) << 8) | (low_byte as u16);
    }

    fn bvc(&mut self) {
        let should_branch = !self.contains_flag(CpuFlags::OVERFLOW);
        self.branch_helper(should_branch);
    }

    fn bvs(&mut self) {
        let should_branch = self.contains_flag(CpuFlags::OVERFLOW);
        self.branch_helper(should_branch);
    }

    fn clc(&mut self) {
        self.clear_flag(CpuFlags::CARRY);
    }

    fn cld(&mut self) {
        self.clear_flag(CpuFlags::DECIMAL_MODE);
    }

    fn cli(&mut self) {
        self.clear_flag(CpuFlags::INTERRUPT_DISABLE);
    }

    fn clv(&mut self) {
        self.clear_flag(CpuFlags::OVERFLOW);
    }

    fn cmp(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);
        let result = self.register_a.wrapping_sub(value);

        self.set_flag(CpuFlags::CARRY, self.register_a >= value);
        self.set_flag(CpuFlags::ZERO, self.register_a == value);
        self.set_flag(CpuFlags::NEGATIVE, result & 0x80 != 0);
    }

    fn cpx(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);
        let result = self.register_x.wrapping_sub(value);

        self.set_flag(CpuFlags::CARRY, self.register_x >= value);
        self.set_flag(CpuFlags::ZERO, self.register_x == value);
        self.set_flag(CpuFlags::NEGATIVE, result & 0x80 != 0);
    }

    fn cpy(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);
        let result = self.register_y.wrapping_sub(value);

        self.set_flag(CpuFlags::CARRY, self.register_y >= value);
        self.set_flag(CpuFlags::ZERO, self.register_y == value);
        self.set_flag(CpuFlags::NEGATIVE, result & 0x80 != 0);
    }

    fn dec(&mut self, mode: &AddressingMode) {
        let address = self.get_address(mode);
        let value = self.mem_read(address);
        let result = value.wrapping_sub(1);

        self.mem_write(address, result);
        self.update_zero_and_negative_flags(result);
    }

    fn dex(&mut self) {
        self.register_x = self.register_x.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn dey(&mut self) {
        self.register_y = self.register_y.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.register_y);
    }

    fn eor(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);

        self.register_a ^= value;

        self.update_zero_and_negative_flags(self.register_a);
    }

    fn inc(&mut self, mode: &AddressingMode) {
        let address = self.get_address(mode);
        let value = self.mem_read(address);
        let result = value.wrapping_add(1);

        self.mem_write(address, result);
        self.update_zero_and_negative_flags(result);
    }

    fn inx(&mut self) {
        self.register_x = self.register_x.wrapping_add(1);

        self.update_zero_and_negative_flags(self.register_x);
    }

    fn iny(&mut self) {
        self.register_y = self.register_y.wrapping_add(1);

        self.update_zero_and_negative_flags(self.register_y);
    }

    fn jmp(&mut self, mode: &AddressingMode) {
        let address = self.get_absolute_address();

        match mode {
            AddressingMode::Absolute => {
                self.program_counter = address;
            }
            AddressingMode::Indirect => {
                let target_address = if address & 0x00FF == 0x00FF {
                    let lo = self.mem_read(address);
                    let hi = self.mem_read(address & 0xFF00);
                    u16::from_le_bytes([lo, hi])
                } else {
                    self.mem_read_u16(address)
                };

                self.program_counter = target_address;
            }
            _ => panic!("Unsupported addressing mode for JMP"),
        }
    }

    fn jsr(&mut self) {
        let target_address = self.get_absolute_address();

        let return_address = self.program_counter.wrapping_add(2) - 1;
        self.push_stack((return_address >> 8) as u8);
        self.push_stack((return_address & 0xFF) as u8);

        self.program_counter = target_address;
    }

    fn lda(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);

        self.register_a = value;

        self.update_zero_and_negative_flags(self.register_a);
    }

    fn ldx(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);

        self.register_x = value;

        self.update_zero_and_negative_flags(self.register_x);
    }

    fn ldy(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);

        self.register_y = value;

        self.update_zero_and_negative_flags(self.register_y);
    }

    fn lsr(&mut self, mode: &AddressingMode) {
        match mode {
            AddressingMode::Accumulator => {
                let carry = self.register_a & 0x01 != 0;
                self.register_a >>= 1;
                self.set_flag(CpuFlags::CARRY, carry);
                self.update_zero_and_negative_flags(self.register_a);
            }
            _ => {
                let address = self.get_address(mode);
                let mut value = self.mem_read(address);
                let carry = value & 0x01 != 0;
                value >>= 1;
                self.mem_write(address, value);
                self.set_flag(CpuFlags::CARRY, carry);
                self.update_zero_and_negative_flags(value);
            }
        }
    }

    fn nop(&mut self) {
        // NOP - No Operation
    }

    fn ora(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);

        self.register_a |= value;

        self.update_zero_and_negative_flags(self.register_a);
    }

    fn pha(&mut self) {
        self.push_stack(self.register_a);
    }

    fn php(&mut self) {
        let flags = self.flags.bits() | CpuFlags::BREAK.bits() | CpuFlags::UNUSED.bits();
        self.push_stack(flags);
    }

    fn pla(&mut self) {
        self.register_a = self.pop_stack();
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn plp(&mut self) {
        let flags = self.pop_stack();
        let current_break_unused = self.flags.bits() & (CpuFlags::BREAK.bits() | CpuFlags::UNUSED.bits());
        let new_flags = (flags & !(CpuFlags::BREAK.bits() | CpuFlags::UNUSED.bits())) | current_break_unused;

        self.flags = CpuFlags::from_bits_truncate(new_flags);
    }

    fn rol(&mut self, mode: &AddressingMode) {
        let carry = self.contains_flag(CpuFlags::CARRY) as u8;
        let address = self.get_address(mode);

        let value = if *mode == AddressingMode::Accumulator {
            self.register_a
        } else {
            self.mem_read(address)
        };

        let result = (value << 1) | carry;
        self.set_flag(CpuFlags::CARRY, value & 0x80 != 0);
        self.update_zero_and_negative_flags(result);

        match mode {
            AddressingMode::Accumulator => self.register_a = result,
            _ => self.mem_write(address, result),
        }
    }

    fn ror(&mut self, mode: &AddressingMode) {
        let carry = (self.contains_flag(CpuFlags::CARRY) as u8) << 7;
        let address = self.get_address(mode);

        let value = if *mode == AddressingMode::Accumulator {
            self.register_a
        } else {
            self.mem_read(address)
        };

        let result = (value >> 1) | carry;
        self.set_flag(CpuFlags::CARRY, value & 0x01 != 0);
        self.update_zero_and_negative_flags(result);

        match mode {
            AddressingMode::Accumulator => self.register_a = result,
            _ => self.mem_write(address, result),
        }
    }

    fn rti(&mut self) {
        let status = self.pop_stack();
        let pc_low = self.pop_stack() as u16;
        let pc_high = self.pop_stack() as u16;

        self.flags = CpuFlags::from_bits_truncate(status);
        self.clear_flag(CpuFlags::BREAK);
        self.insert_flag(CpuFlags::UNUSED);

        self.program_counter = (pc_high << 8) | pc_low;
    }

    fn rts(&mut self) {
        let pc_low = self.pop_stack() as u16;
        let pc_high = self.pop_stack() as u16;

        self.program_counter = ((pc_high << 8) | pc_low).wrapping_add(1);
    }

    fn sbc(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);
        let carry = if self.contains_flag(CpuFlags::CARRY) { 0 } else { 1 };

        let result = self.register_a as i16 - value as i16 - carry as i16;

        self.set_flag(CpuFlags::CARRY, result >= 0);

        let result = result as u8;
        self.set_flag(CpuFlags::OVERFLOW, (self.register_a ^ value) & (self.register_a ^ result) & 0x80 != 0);

        self.register_a = result;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn sec(&mut self) {
        self.insert_flag(CpuFlags::CARRY);
    }

    fn sed(&mut self) {
        self.insert_flag(CpuFlags::DECIMAL_MODE);
    }

    fn sei(&mut self) {
        self.insert_flag(CpuFlags::INTERRUPT_DISABLE);
    }

    fn sta(&mut self, mode: &AddressingMode) {
        let addr = self.get_address(mode);
        self.mem_write(addr, self.register_a);
    }

    fn stx(&mut self, mode: &AddressingMode) {
        let addr = self.get_address(mode);
        self.mem_write(addr, self.register_x);
    }

    fn sty(&mut self, mode: &AddressingMode) {
        let addr = self.get_address(mode);
        self.mem_write(addr, self.register_y);
    }

    fn tax(&mut self) {
        self.register_x = self.register_a;
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn tay(&mut self) {
        self.register_y = self.register_a;
        self.update_zero_and_negative_flags(self.register_y);
    }

    fn tsx(&mut self) {
        self.register_x = self.stack_pointer;
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn txa(&mut self) {
        self.register_a = self.register_x;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn txs(&mut self) {
        self.stack_pointer = self.register_x;
    }

    fn tya(&mut self) {
        self.register_a = self.register_y;
        self.update_zero_and_negative_flags(self.register_a);
    }
}

// helper
impl<'a> CPU<'a> {
    fn branch_helper(&mut self, condition: bool) {
        if condition {
            self.tick(1);

            let offset = self.get_relative() as i8;
            let old_pc = self.program_counter;
            let new_pc = self.program_counter.wrapping_add(offset as u16);

            if (old_pc & 0xFF00) != (new_pc & 0xFF00) {
                self.tick(1);
            }

            self.program_counter = new_pc;
        }
    }
}