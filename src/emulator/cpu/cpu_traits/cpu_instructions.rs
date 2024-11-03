use crate::emulator::cpu::addressing::AddressingMode;
use crate::emulator::cpu::addressing_mode_operations::AddressingModeOperations;
use crate::emulator::cpu::CPU;
use crate::emulator::cpu::cpu_flags::CpuFlags;
use crate::emulator::cpu::flag_operations::FlagOperations;

pub trait CpuInstructions {
    fn adc(&mut self, mode: &AddressingMode, program: &[u8]);
    fn and(&mut self, mode: &AddressingMode, program: &[u8]);
    fn asl(&mut self, mode: &AddressingMode, program: &[u8]);
    fn bcc(&mut self, mode: &AddressingMode, program: &[u8]);
    fn bcs(&mut self, mode: &AddressingMode, program: &[u8]);
    fn beq(&mut self, mode: &AddressingMode, program: &[u8]);
    fn bit(&mut self, mode: &AddressingMode, program: &[u8]);
    fn bmi(&mut self, mode: &AddressingMode, program: &[u8]);
    fn bne(&mut self, mode: &AddressingMode, program: &[u8]);
    fn bpl(&mut self, mode: &AddressingMode, program: &[u8]);
    fn brk(&mut self, mode: &AddressingMode, program: &[u8]);
    fn bvc(&mut self, mode: &AddressingMode, program: &[u8]);
    fn bvs(&mut self, mode: &AddressingMode, program: &[u8]);
    fn clc(&mut self, mode: &AddressingMode, program: &[u8]);
    fn cld(&mut self, mode: &AddressingMode, program: &[u8]);
    fn cli(&mut self, mode: &AddressingMode, program: &[u8]);
    fn clv(&mut self, mode: &AddressingMode, program: &[u8]);
    fn cmp(&mut self, mode: &AddressingMode, program: &[u8]);
    fn cpx(&mut self, mode: &AddressingMode, program: &[u8]);
    fn cpy(&mut self, mode: &AddressingMode, program: &[u8]);
    fn dec(&mut self, mode: &AddressingMode, program: &[u8]);
    fn dex(&mut self, mode: &AddressingMode, program: &[u8]);
    fn dey(&mut self, mode: &AddressingMode, program: &[u8]);
    fn eor(&mut self, mode: &AddressingMode, program: &[u8]);
    fn inc(&mut self, mode: &AddressingMode, program: &[u8]);
    fn inx(&mut self, mode: &AddressingMode, program: &[u8]);
    fn iny(&mut self, mode: &AddressingMode, program: &[u8]);
    fn jmp(&mut self, mode: &AddressingMode, program: &[u8]);
    fn jsr(&mut self, mode: &AddressingMode, program: &[u8]);
    fn lda(&mut self, mode: &AddressingMode, program: &[u8]);
    fn ldx(&mut self, mode: &AddressingMode, program: &[u8]);
    fn ldy(&mut self, mode: &AddressingMode, program: &[u8]);
    fn lsr(&mut self, mode: &AddressingMode, program: &[u8]);
    fn nop(&mut self, mode: &AddressingMode, program: &[u8]);
    fn ora(&mut self, mode: &AddressingMode, program: &[u8]);
    fn pha(&mut self, mode: &AddressingMode, program: &[u8]);
    fn php(&mut self, mode: &AddressingMode, program: &[u8]);
    fn pla(&mut self, mode: &AddressingMode, program: &[u8]);
    fn plp(&mut self, mode: &AddressingMode, program: &[u8]);
    fn rol(&mut self, mode: &AddressingMode, program: &[u8]);
    fn ror(&mut self, mode: &AddressingMode, program: &[u8]);
    fn rti(&mut self, mode: &AddressingMode, program: &[u8]);
    fn rts(&mut self, mode: &AddressingMode, program: &[u8]);
    fn sbc(&mut self, mode: &AddressingMode, program: &[u8]);
    fn sec(&mut self, mode: &AddressingMode, program: &[u8]);
    fn sed(&mut self, mode: &AddressingMode, program: &[u8]);
    fn sei(&mut self, mode: &AddressingMode, program: &[u8]);
    fn sta(&mut self, mode: &AddressingMode, program: &[u8]);
    fn stx(&mut self, mode: &AddressingMode, program: &[u8]);
    fn sty(&mut self, mode: &AddressingMode, program: &[u8]);
    fn tax(&mut self, mode: &AddressingMode, program: &[u8]);
    fn tay(&mut self, mode: &AddressingMode, program: &[u8]);
    fn tsx(&mut self, mode: &AddressingMode, program: &[u8]);
    fn txa(&mut self, mode: &AddressingMode, program: &[u8]);
    fn txs(&mut self, mode: &AddressingMode, program: &[u8]);
    fn tya(&mut self, mode: &AddressingMode, program: &[u8]);
}

impl<'a> CpuInstructions for CPU<'a> {
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

    fn and(&mut self, mode: &AddressingMode, program: &[u8]) {
        let operand = self.get_operand(mode, program);

        self.register_a &= operand;

        self.update_zero_and_negative_flags(self.register_a);
    }

    fn asl(&mut self, mode: &AddressingMode, program: &[u8]) {
        let mut operand = self.get_operand(mode, program);

        let carry_flag = (operand >> 7) & 0x01;
        operand <<= 1;

        self.set_flag(CpuFlags::ZERO, operand == 0);
        self.set_flag(CpuFlags::NEGATIVE, (operand >> 7) & 0x01 == 1);

        match mode {
            AddressingMode::Accumulator => {
                self.register_a = operand;
            }
            AddressingMode::ZeroPage => {
                let addr = self.get_zero_page(program) as u16;
                self.mem_write(addr, operand);
            }
            AddressingMode::ZeroPageX => {
                let addr = self.get_zero_page_x(program) as u16;
                self.mem_write(addr, operand);
            }
            AddressingMode::Absolute => {
                let addr = self.get_absolute(program) as u16;
                self.mem_write(addr, operand);
            }
            AddressingMode::AbsoluteX => {
                let addr = self.get_absolute_x(program) as u16;
                self.mem_write(addr, operand);
            }
            _ => panic!("Unsupported addressing mode for ASL"),
        }

        self.set_flag(CpuFlags::ZERO, carry_flag == 1);
    }

    fn bcc(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn bcs(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn beq(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn bit(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn bmi(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn bne(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn bpl(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn brk(&mut self, mode: &AddressingMode, program: &[u8]) {
        return;
    }

    fn bvc(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn bvs(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn clc(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn cld(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn cli(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn clv(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn cmp(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn cpx(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn cpy(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn dec(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn dex(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn dey(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn eor(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn inc(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn inx(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn iny(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn jmp(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn jsr(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn lda(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn ldx(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn ldy(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn lsr(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn nop(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn ora(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn pha(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn php(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn pla(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn plp(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn rol(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn ror(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn rti(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn rts(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn sbc(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn sec(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn sed(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn sei(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn sta(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn stx(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn sty(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn tax(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn tay(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn tsx(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn txa(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn txs(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }

    fn tya(&mut self, mode: &AddressingMode, program: &[u8]) {
        todo!()
    }
}
