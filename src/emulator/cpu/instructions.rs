use crate::emulator::cpu::addressing::AddressingMode;
use crate::emulator::cpu::CPU;
use crate::emulator::cpu::flags::CpuFlags;

impl<'a> CPU<'a> {
    pub (super) fn adc(&mut self, mode: &AddressingMode) {
        let operand = self.get_operand(mode);

        self.add_to_accumulator(operand);
    }

    pub (super) fn and(&mut self, mode: &AddressingMode) {
        let operand = self.get_operand(mode);

        self.register_a &= operand;

        self.update_zero_and_negative_flags(self.register_a);
    }

    pub (super) fn asl(&mut self, mode: &AddressingMode) {
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

    pub (super) fn bcc(&mut self) {
        let should_branch = !self.contains_flag(CpuFlags::CARRY);
        self.branch_helper(should_branch);
    }

    pub (super) fn bcs(&mut self) {
        let should_branch = self.contains_flag(CpuFlags::CARRY);
        self.branch_helper(should_branch);
    }

    pub (super) fn beq(&mut self) {
        let should_branch = self.contains_flag(CpuFlags::ZERO);
        self.branch_helper(should_branch);
    }

    pub (super) fn bit(&mut self, mode: &AddressingMode) {
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

    pub (super) fn bmi(&mut self) {
        let should_branch = self.contains_flag(CpuFlags::NEGATIVE);
        self.branch_helper(should_branch);
    }

    pub (super) fn bne(&mut self) {
        let should_branch = !self.contains_flag(CpuFlags::ZERO);
        self.branch_helper(should_branch);
    }

    pub (super) fn bpl(&mut self) {
        let should_branch = !self.contains_flag(CpuFlags::NEGATIVE);
        self.branch_helper(should_branch);
    }

    pub (super) fn brk(&mut self) {
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

    pub (super) fn bvc(&mut self) {
        let should_branch = !self.contains_flag(CpuFlags::OVERFLOW);
        self.branch_helper(should_branch);
    }

    pub (super) fn bvs(&mut self) {
        let should_branch = self.contains_flag(CpuFlags::OVERFLOW);
        self.branch_helper(should_branch);
    }

    pub (super) fn clc(&mut self) {
        self.clear_flag(CpuFlags::CARRY);
    }

    pub (super) fn cld(&mut self) {
        self.clear_flag(CpuFlags::DECIMAL_MODE);
    }

    pub (super) fn cli(&mut self) {
        self.clear_flag(CpuFlags::INTERRUPT_DISABLE);
    }

    pub (super) fn clv(&mut self) {
        self.clear_flag(CpuFlags::OVERFLOW);
    }

    pub (super) fn cmp(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);

        self.compare(self.register_a, value);
    }

    pub (super) fn cpx(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);

        self.compare(self.register_x, value);
    }

    pub (super) fn cpy(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);

        self.compare(self.register_y, value);
    }

    pub (super) fn dec(&mut self, mode: &AddressingMode) {
        let address = self.get_address(mode);
        let value = self.mem_read(address);
        let result = value.wrapping_sub(1);

        self.mem_write(address, result);
        self.update_zero_and_negative_flags(result);
    }

    pub (super) fn dex(&mut self) {
        self.register_x = self.register_x.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.register_x);
    }

    pub (super) fn dey(&mut self) {
        self.register_y = self.register_y.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.register_y);
    }

    pub (super) fn eor(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);

        self.register_a ^= value;

        self.update_zero_and_negative_flags(self.register_a);
    }

    pub (super) fn inc(&mut self, mode: &AddressingMode) {
        let address = self.get_address(mode);
        let value = self.mem_read(address);
        let result = value.wrapping_add(1);

        self.mem_write(address, result);
        self.update_zero_and_negative_flags(result);
    }

    pub (super) fn inx(&mut self) {
        self.register_x = self.register_x.wrapping_add(1);

        self.update_zero_and_negative_flags(self.register_x);
    }

    pub (super) fn iny(&mut self) {
        self.register_y = self.register_y.wrapping_add(1);

        self.update_zero_and_negative_flags(self.register_y);
    }

    pub (super) fn jmp(&mut self, mode: &AddressingMode) {
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

    pub (super) fn jsr(&mut self) {
        let target_address = self.get_absolute_address();

        let return_address = self.program_counter.wrapping_add(2) - 1;
        self.push_stack((return_address >> 8) as u8);
        self.push_stack((return_address & 0xFF) as u8);

        self.program_counter = target_address;
    }

    pub (super) fn lda(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);

        self.register_a = value;

        self.update_zero_and_negative_flags(self.register_a);
    }

    pub (super) fn ldx(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);

        self.register_x = value;

        self.update_zero_and_negative_flags(self.register_x);
    }

    pub (super) fn ldy(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);

        self.register_y = value;

        self.update_zero_and_negative_flags(self.register_y);
    }

    pub (super) fn lsr(&mut self, mode: &AddressingMode) {
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

    pub (super) fn nop(&mut self) {
        // NOP - No Operation
    }

    pub (super) fn ora(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);

        self.register_a |= value;

        self.update_zero_and_negative_flags(self.register_a);
    }

    pub (super) fn pha(&mut self) {
        self.push_stack(self.register_a);
    }

    pub (super) fn php(&mut self) {
        let flags = self.flags.bits() | CpuFlags::BREAK.bits() | CpuFlags::UNUSED.bits();
        self.push_stack(flags);
    }

    pub (super) fn pla(&mut self) {
        self.register_a = self.pop_stack();
        self.update_zero_and_negative_flags(self.register_a);
    }

    pub (super) fn plp(&mut self) {
        let flags = self.pop_stack();
        let current_break_unused = self.flags.bits() & (CpuFlags::BREAK.bits() | CpuFlags::UNUSED.bits());
        let new_flags = (flags & !(CpuFlags::BREAK.bits() | CpuFlags::UNUSED.bits())) | current_break_unused;

        self.flags = CpuFlags::from_bits_truncate(new_flags);
    }

    pub (super) fn rol(&mut self, mode: &AddressingMode) {
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

    pub (super) fn ror(&mut self, mode: &AddressingMode) {
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

    pub (super) fn rti(&mut self) {
        let status = self.pop_stack();
        let pc_low = self.pop_stack() as u16;
        let pc_high = self.pop_stack() as u16;

        self.flags = CpuFlags::from_bits_truncate(status);
        self.clear_flag(CpuFlags::BREAK);
        self.insert_flag(CpuFlags::UNUSED);

        self.program_counter = (pc_high << 8) | pc_low;
    }

    pub (super) fn rts(&mut self) {
        let pc_low = self.pop_stack() as u16;
        let pc_high = self.pop_stack() as u16;

        self.program_counter = ((pc_high << 8) | pc_low).wrapping_add(1);
    }

    pub (super) fn sbc(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);

        self.sub_from_accumulator(value);
    }

    pub (super) fn sec(&mut self) {
        self.insert_flag(CpuFlags::CARRY);
    }

    pub (super) fn sed(&mut self) {
        self.insert_flag(CpuFlags::DECIMAL_MODE);
    }

    pub (super) fn sei(&mut self) {
        self.insert_flag(CpuFlags::INTERRUPT_DISABLE);
    }

    pub (super) fn sta(&mut self, mode: &AddressingMode) {
        let addr = self.get_address(mode);
        self.mem_write(addr, self.register_a);
    }

    pub (super) fn stx(&mut self, mode: &AddressingMode) {
        let addr = self.get_address(mode);
        self.mem_write(addr, self.register_x);
    }

    pub (super) fn sty(&mut self, mode: &AddressingMode) {
        let addr = self.get_address(mode);
        self.mem_write(addr, self.register_y);
    }

    pub (super) fn tax(&mut self) {
        self.register_x = self.register_a;
        self.update_zero_and_negative_flags(self.register_x);
    }

    pub (super) fn tay(&mut self) {
        self.register_y = self.register_a;
        self.update_zero_and_negative_flags(self.register_y);
    }

    pub (super) fn tsx(&mut self) {
        self.register_x = self.stack_pointer;
        self.update_zero_and_negative_flags(self.register_x);
    }

    pub (super) fn txa(&mut self) {
        self.register_a = self.register_x;
        self.update_zero_and_negative_flags(self.register_a);
    }

    pub (super) fn txs(&mut self) {
        self.stack_pointer = self.register_x;
    }

    pub (super) fn tya(&mut self) {
        self.register_a = self.register_y;
        self.update_zero_and_negative_flags(self.register_a);
    }

    pub (super) fn nop_with_operand(&mut self, mode: &AddressingMode) {
        // the operand is read and discarded (the read can have side effects on real hardware)
        self.get_operand(mode);
    }

    pub (super) fn lax(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);

        self.register_a = value;
        self.register_x = value;
        self.update_zero_and_negative_flags(value);
    }

    pub (super) fn sax(&mut self, mode: &AddressingMode) {
        let addr = self.get_address(mode);
        self.mem_write(addr, self.register_a & self.register_x);
    }

    pub (super) fn dcp(&mut self, mode: &AddressingMode) {
        let addr = self.get_address(mode);
        let value = self.mem_read(addr).wrapping_sub(1);

        self.mem_write(addr, value);
        self.compare(self.register_a, value);
    }

    pub (super) fn isb(&mut self, mode: &AddressingMode) {
        let addr = self.get_address(mode);
        let value = self.mem_read(addr).wrapping_add(1);

        self.mem_write(addr, value);
        self.sub_from_accumulator(value);
    }

    pub (super) fn slo(&mut self, mode: &AddressingMode) {
        let addr = self.get_address(mode);
        let value = self.mem_read(addr);
        let result = value << 1;

        self.mem_write(addr, result);
        self.set_flag(CpuFlags::CARRY, value & 0x80 != 0);
        self.set_register_a(self.register_a | result);
    }

    pub (super) fn rla(&mut self, mode: &AddressingMode) {
        let addr = self.get_address(mode);
        let value = self.mem_read(addr);
        let result = (value << 1) | self.get_flag_value(CpuFlags::CARRY) as u8;

        self.mem_write(addr, result);
        self.set_flag(CpuFlags::CARRY, value & 0x80 != 0);
        self.set_register_a(self.register_a & result);
    }

    pub (super) fn sre(&mut self, mode: &AddressingMode) {
        let addr = self.get_address(mode);
        let value = self.mem_read(addr);
        let result = value >> 1;

        self.mem_write(addr, result);
        self.set_flag(CpuFlags::CARRY, value & 0x01 != 0);
        self.set_register_a(self.register_a ^ result);
    }

    pub (super) fn rra(&mut self, mode: &AddressingMode) {
        let addr = self.get_address(mode);
        let value = self.mem_read(addr);
        let result = (value >> 1) | ((self.get_flag_value(CpuFlags::CARRY) as u8) << 7);

        self.mem_write(addr, result);
        // ROR's carry-out becomes the carry-in of the ADC
        self.set_flag(CpuFlags::CARRY, value & 0x01 != 0);
        self.add_to_accumulator(result);
    }

    pub (super) fn anc(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);

        self.set_register_a(self.register_a & value);
        self.set_flag(CpuFlags::CARRY, self.register_a & 0x80 != 0);
    }

    pub (super) fn alr(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);
        let anded = self.register_a & value;

        self.set_flag(CpuFlags::CARRY, anded & 0x01 != 0);
        self.set_register_a(anded >> 1);
    }

    pub (super) fn arr(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);
        let carry_in = (self.get_flag_value(CpuFlags::CARRY) as u8) << 7;
        let result = ((self.register_a & value) >> 1) | carry_in;

        self.set_register_a(result);
        // C = bit 6, V = bit 6 xor bit 5 of the result
        self.set_flag(CpuFlags::CARRY, result & 0x40 != 0);
        self.set_flag(CpuFlags::OVERFLOW, ((result >> 6) ^ (result >> 5)) & 0x01 != 0);
    }

    pub (super) fn axs(&mut self, mode: &AddressingMode) {
        let value = self.get_operand(mode);
        let anded = self.register_a & self.register_x;

        // flags as in CMP (no borrow-in); X receives the difference
        self.compare(anded, value);
        self.set_register_x(anded.wrapping_sub(value));
    }
}

// helper
impl<'a> CPU<'a> {
    fn add_to_accumulator(&mut self, operand: u8) {
        let carry = self.get_flag_value(CpuFlags::CARRY);

        let sum = (self.register_a as u16) + (operand as u16) + carry;

        let overflow = (!(self.register_a ^ operand) & (self.register_a ^ sum as u8) & 0x80) != 0;

        self.set_flag(CpuFlags::CARRY, sum > 0xFF);
        self.set_flag(CpuFlags::OVERFLOW, overflow);

        self.register_a = sum as u8;

        self.update_zero_and_negative_flags(self.register_a);
    }

    fn sub_from_accumulator(&mut self, value: u8) {
        let carry = if self.contains_flag(CpuFlags::CARRY) { 0 } else { 1 };

        let result = self.register_a as i16 - value as i16 - carry as i16;

        self.set_flag(CpuFlags::CARRY, result >= 0);

        let result = result as u8;
        self.set_flag(CpuFlags::OVERFLOW, (self.register_a ^ value) & (self.register_a ^ result) & 0x80 != 0);

        self.register_a = result;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn compare(&mut self, register: u8, value: u8) {
        let result = register.wrapping_sub(value);

        self.set_flag(CpuFlags::CARRY, register >= value);
        self.set_flag(CpuFlags::ZERO, register == value);
        self.set_flag(CpuFlags::NEGATIVE, result & 0x80 != 0);
    }

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