mod addressing;
pub mod memory;
mod operation_codes;
mod cpu_flags;
mod cpu_traits;
mod interrupts;

use crate::emulator::core::cpu_bus::CpuBus;
pub use operation_codes::*;
pub use addressing::*;
pub use cpu_traits::*;
use crate::emulator::cpu::addressing_mode_operations::AddressingModeOperations;
use crate::emulator::cpu::cpu_flags::CpuFlags;
use crate::emulator::cpu::flag_operations::FlagOperations;
use crate::emulator::cpu::cpu_instructions::{CpuInstructions};

pub struct CPU<'a> {
    pub (super) register_a: u8,
    pub (super) register_x: u8,
    pub (super) register_y: u8,
    pub (super) stack_pointer: u8,
    pub program_counter: u16,
    pub (super) flags: cpu_flags::CpuFlags,
    pub bus: Box<dyn CpuBus + 'a>,
}

impl<'a> CPU<'a> {
    pub fn new<'b>(bus: Box<dyn CpuBus + 'b>) -> CPU<'b> {
        return CPU {
            register_a: 0,
            register_x: 0,
            register_y: 0,
            stack_pointer: 0xfd,
            program_counter: 0,
            flags: CpuFlags::INTERRUPT_DISABLE | CpuFlags::BREAK | CpuFlags::UNUSED,
            bus,
        };
    }

    pub fn interpret(&mut self, program: Vec<u8>) {
        self.program_counter = 0;

        loop {
            if self.program_counter as usize >= program.len() {
                break;
            }

            let operation_code = program[self.program_counter as usize];

            self.process_operation(&program, operation_code);
        }
    }

    fn process_operation(&mut self, program: &[u8], operation_code: u8) {
        self.program_counter += self.program_counter.wrapping_add(1);

        let mut is_jump = false;

        if let Some(op_code_info) = CPU_OPERATION_CODES_MAP.get(&operation_code) {
            match operation_code {
                // ADC - Add with Carry
                0x69 | 0x65 | 0x75 | 0x6D | 0x7D | 0x79 | 0x61 | 0x71
                => self.adc(&op_code_info.addressing_mode, &program),
                // AND - Logical AND
                0x29 | 0x25 | 0x35 | 0x2D | 0x3D | 0x39 | 0x21 | 0x31
                => self.and(&op_code_info.addressing_mode, &program),
                // ASL - Arithmetic Shift Left
                0x0A | 0x06 | 0x16 | 0x0E | 0x1E
                => self.asl(&op_code_info.addressing_mode, &program),
                // BCC - Branch if Carry Clear
                0x90
                => self.bcc(&program),
                // BCS - Branch if Carry Set
                0xB0
                => self.bcs(&program),
                // BEQ - Branch if Equal
                0xF0
                => self.beq(&program),
                // BIT - Bit Test
                0x24 | 0x2C
                => self.bit(&op_code_info.addressing_mode, &program),
                // BMI - Branch if Minus
                0x30
                => self.bmi(&program),
                // BNE - Branch if Not Equal
                0xD0
                => self.bne(&program),
                // BPL - Branch if Positive
                0x10
                => self.bpl(&program),
                // BVC - Branch if Overflow Clear
                0x50
                => self.bvc(&program),
                // BVS - Branch if Overflow Set
                0x70
                => self.bvs(&program),
                // CLC - Clear Carry Flag
                0x18
                => self.clc(),
                // CLD - Clear Decimal Mode
                0xD8
                => self.cld(),
                // CLI - Clear Interrupt Disable
                0x58
                => self.cli(),
                // CLV - Clear Overflow Flag
                0xB8
                => self.clv(),
                // CMP - Compare
                0xC9 | 0xC5 | 0xD5 | 0xCD | 0xDD | 0xD9 | 0xC1 | 0xD1
                => self.cmp(&op_code_info.addressing_mode, &program),
                // CPX - Compare X Register
                0xE0 | 0xE4 | 0xEC
                => self.cpx(&op_code_info.addressing_mode, &program),
                // CPY - Compare Y Register
                0xC0 | 0xC4 | 0xCC
                => self.cpy(&op_code_info.addressing_mode, &program),
                // DEC - Decrement Memory
                0xC6 | 0xD6 | 0xCE | 0xDE
                => self.dec(&op_code_info.addressing_mode, &program),
                // DEX - Decrement X Register
                0xCA
                => self.dex(),
                // DEY - Decrement Y Register
                0x88
                => self.dey(),
                // EOR - Exclusive OR
                0x49 | 0x45 | 0x55 | 0x4D | 0x5D | 0x59 | 0x41 | 0x51
                => self.eor(&op_code_info.addressing_mode, &program),
                // INC - Increment Memory
                0xE6 | 0xF6 | 0xEE | 0xFE
                => self.inc(&op_code_info.addressing_mode, &program),
                // INX - Increment X Register
                0xE8
                => self.inx(),
                // INY - Increment Y Register
                0xC8
                => self.iny(),
                // JMP - Jump
                0x4C | 0x6C => {
                    self.jmp(&op_code_info.addressing_mode, &program);
                    is_jump = true
                },
                // JSR - Jump to Subroutine
                0x20 => {
                    self.jsr(&program);
                    is_jump = true
                },
                // LDA - Load Accumulator
                0xA9 | 0xA5 | 0xB5 | 0xAD | 0xBD | 0xB9 | 0xA1 | 0xB1
                => self.lda(&op_code_info.addressing_mode, &program),
                // LDX - Load X Register
                0xA2 | 0xA6 | 0xB6 | 0xAE | 0xBE
                => self.ldx(&op_code_info.addressing_mode, &program),
                // LDY - Load Y Register
                0xA0 | 0xA4 | 0xB4 | 0xAC | 0xBC
                => self.ldy(&op_code_info.addressing_mode, &program),
                // LSR - Logical Shift Right
                0x4A | 0x46 | 0x56 | 0x4E | 0x5E
                => self.lsr(&op_code_info.addressing_mode, &program),
                // NOP - No Operation
                0xEA
                => self.nop(),
                // ORA - Logical Inclusive OR
                0x09 | 0x05 | 0x15 | 0x0D | 0x1D | 0x19 | 0x01 | 0x11
                => self.ora(&op_code_info.addressing_mode, &program),
                // PHA - Push Accumulator
                0x48
                => self.pha(),
                // PHP - Push Processor Status
                0x08
                => self.php(),
                // PLA - Pull Accumulator
                0x68
                => self.pla(),
                // PLP - Pull Processor Status
                0x28
                => self.plp(),
                // ROL - Rotate Left
                0x2A | 0x26 | 0x36 | 0x2E | 0x3E
                => self.rol(&op_code_info.addressing_mode, &program),
                // ROR - Rotate Right
                0x6A | 0x66 | 0x76 | 0x6E | 0x7E
                => self.ror(&op_code_info.addressing_mode, &program),
                // RTI - Return from Interrupt
                0x40
                => self.rti(),
                // RTS - Return from Subroutine
                0x60
                => self.rts(),
                // SBC - Subtract with Carry
                0xE9 | 0xE5 | 0xF5 | 0xED | 0xFD | 0xF9 | 0xE1 | 0xF1
                => self.sbc(&op_code_info.addressing_mode, &program),
                // SEC - Set Carry Flag
                0x38
                => self.sec(),
                // SED - Set Decimal Flag
                0xF8
                => self.sed(),
                // SEI - Set Interrupt Disable
                0x78
                => self.sei(),
                // STA - Store Accumulator
                0x85 | 0x95 | 0x8D | 0x9D | 0x99 | 0x81 | 0x91
                => self.sta(&op_code_info.addressing_mode, &program),
                // STX - Store X Register
                0x86 | 0x96 | 0x8E
                => self.stx(&op_code_info.addressing_mode, &program),
                // STY - Store Y Register
                0x84 | 0x94 | 0x8C
                => self.sty(&op_code_info.addressing_mode, &program),
                // TAX - Transfer Accumulator to X
                0xAA
                => self.tax(),
                // TAY - Transfer Accumulator to Y
                0xA8
                => self.tay(),
                // TSX - Transfer Stack Pointer to X
                0xBA
                => self.tsx(),
                // TXA - Transfer X to Accumulator
                0x8A
                => self.txa(),
                // TXS - Transfer X to Stack Pointer
                0x9A
                => self.txs(),
                // TYA - Transfer Y to Accumulator
                0x98
                => self.tya(),
                // BRK - Force Interrupt
                0x00 => return,
                _ => {
                    println!("Unknown operation_code: {}", operation_code);
                    return;
                }
            }

            // todo cycles
            if !is_jump {
                self.program_counter = self.program_counter.wrapping_add((op_code_info.bytes - 1) as u16);
            }
        }
        else {
            println!("Unknown operation_code: {}", operation_code);
            return;
        }
    }

    fn get_operand(&self, mode: &AddressingMode, program: &[u8]) -> u8 {
        match mode {
            AddressingMode::Implicit => 0,
            AddressingMode::Accumulator => self.get_accumulator(),
            AddressingMode::Immediate => self.get_immediate(program),
            AddressingMode::ZeroPage => self.get_zero_page(program),
            AddressingMode::ZeroPageX => self.get_zero_page_x(program),
            AddressingMode::ZeroPageY => self.get_zero_page_y(program),
            AddressingMode::Relative => self.get_relative(program),
            AddressingMode::Absolute => self.get_absolute(program),
            AddressingMode::AbsoluteX => self.get_absolute_x(program),
            AddressingMode::AbsoluteY => self.get_absolute_y(program),
            AddressingMode::Indirect => self.get_indirect(program),
            AddressingMode::IndirectX => self.get_indirect_x(program),
            AddressingMode::IndirectY => self.get_indirect_y(program),
            _ => panic!("Unimplemented addressing mode")
        }
    }

    fn get_address(&self, mode: &AddressingMode, program: &[u8]) -> u16 {
        match mode {
            AddressingMode::Implicit => 0,
            AddressingMode::Accumulator => 0,
            AddressingMode::ZeroPage => self.get_zero_page_address(program),
            AddressingMode::ZeroPageX => self.get_zero_page_x_address(program),
            AddressingMode::ZeroPageY => self.get_zero_page_y_address(program),
            AddressingMode::Absolute => self.get_absolute_address(program),
            AddressingMode::AbsoluteX => self.get_absolute_x_address(program),
            AddressingMode::AbsoluteY => self.get_absolute_y_address(program),
            AddressingMode::Indirect => self.get_indirect_address(program),
            AddressingMode::IndirectX => self.get_indirect_x_address(program),
            AddressingMode::IndirectY => self.get_indirect_y_address(program),
            _ => panic!("Unsupported addressing mode for get_address")
        }
    }

    pub (super) fn mem_read(&mut self, pos: u16) -> u8 {
        self.bus.read(pos)
    }

    pub (super) fn mem_read_u16(&mut self, pos: u16) -> u16 {
        self.bus.read_u16(pos)
    }

    pub (super) fn mem_write(&mut self, pos: u16, data: u8) {
        self.bus.write(pos, data);
    }

    pub (super) fn set_register_a(&mut self, data: u8) {
        self.register_a = data;
        self.update_zero_and_negative_flags(self.register_a);
    }

    pub (super) fn set_register_x(&mut self, data: u8) {
        self.register_x = data;
        self.update_zero_and_negative_flags(self.register_x);
    }

    pub (super) fn set_register_y(&mut self, data: u8) {
        self.register_y = data;
        self.update_zero_and_negative_flags(self.register_y);
    }

    fn interrupt(&mut self) {
        // todo
    }
}

#[cfg(test)]
mod test {
    use log::log;
    use super::*;
    use crate::emulator::core::mock_bus::MockBus;
    use crate::emulator::cpu::stack_operations::StackOperations;

    #[test]
    fn test_adc_immediate() {
        let mut bus = MockBus::new();
        let mut cpu = CPU::new(Box::new(bus));

        cpu.register_a = 0x05;

        let program = vec![0x69, 0x03, 0x00]; // 0x69 => ADC Immediate
        cpu.interpret(program);

        assert_eq!(cpu.register_a, 0x08); // (0x05 + 0x03)
        assert!(!cpu.flags.contains(CpuFlags::ZERO));
        assert!(!cpu.flags.contains(CpuFlags::CARRY));
    }

    #[test]
    fn test_adc_zero_flag() {
        let mut bus = MockBus::new();
        let mut cpu = CPU::new(Box::new(bus));

        cpu.register_a = 0xFF;

        let program = vec![0x69, 0x01, 0x00]; // 0x69 => ADC Immediate
        cpu.interpret(program);

        assert_eq!(cpu.register_a, 0x00); // ADC #0x01 (0xFF + 0x01 = 0x00)
        assert!(cpu.flags.contains(CpuFlags::CARRY));
        assert!(cpu.flags.contains(CpuFlags::ZERO));
        assert!(!cpu.flags.contains(CpuFlags::OVERFLOW));
    }

    #[test]
    fn test_adc_carry_flag() {
        let mut bus = MockBus::new();
        let mut cpu = CPU::new(Box::new(bus));

        cpu.register_a = 0xFE;

        let program = vec![0x69, 0x02, 0x00]; // 0x69 => ADC Immediate
        cpu.interpret(program);

        assert_eq!(cpu.register_a, 0x00); // ADC #0x02 (0xFE + 0x02 = 0x00)
        assert!(cpu.flags.contains(CpuFlags::ZERO));
        assert!(cpu.flags.contains(CpuFlags::CARRY));
    }

    #[test]
    fn test_and_accumulator() {
        let mut bus = MockBus::new();
        let mut cpu = CPU::new(Box::new(bus));

        cpu.register_a = 0b11001100; // 204
        cpu.mem_write(0x2000, 0b10101010); // 170

        let program = vec![0x29, 0xAA]; // AND #$AA
        cpu.interpret(program);

        assert_eq!(cpu.register_a, 0b10001000); // 136
    }

    #[test]
    fn test_and_zero_page() {
        let mut bus = MockBus::new();
        let mut cpu = CPU::new(Box::new(bus));

        cpu.register_a = 0b11001100; // 204
        cpu.mem_write(0x00, 0b10101010); // 170

        let program = vec![0x25, 0x00]; // AND $00
        cpu.interpret(program);

        assert_eq!(cpu.register_a, 0b10001000); // 136
    }

    #[test]
    fn test_asl_accumulator() {
        let mut bus = MockBus::new();
        let mut cpu = CPU::new(Box::new(bus));

        cpu.register_a = 0b11001100; // 204

        let program = vec![0x0A]; // ASL
        cpu.interpret(program);

        assert_eq!(cpu.register_a, 0b10011000); // 152
        assert!(cpu.flags.contains(CpuFlags::CARRY));
    }

    #[test]
    fn test_asl_zero_page() {
        let mut bus = MockBus::new();
        let mut cpu = CPU::new(Box::new(bus));

        cpu.mem_write(0x00, 0b11001100); // 204

        let program = vec![0x06, 0x00]; // ASL $00
        cpu.interpret(program);

        assert_eq!(cpu.mem_read(0x00), 0b10011000); // 152
        assert!(cpu.flags.contains(CpuFlags::CARRY));
    }

    #[test]
    fn test_bcc_branch_taken() {
        let mut bus = MockBus::new();
        let mut cpu = CPU::new(Box::new(bus));
        cpu.flags.remove(CpuFlags::CARRY);

        let program = vec![0x90, 0x05]; // BCC with offset of 5
        cpu.interpret(program);

        assert_eq!(cpu.program_counter, 0x0007);
    }

    #[test]
    fn test_bcc_branch_not_taken() {
        let mut bus = MockBus::new();
        let mut cpu = CPU::new(Box::new(bus));
        cpu.flags.insert(CpuFlags::CARRY);

        let program = vec![0x90, 0x05]; // BCC with offset of 5
        cpu.interpret(program);

        assert_eq!(cpu.program_counter, 0x02);
    }

    #[test]
    fn test_bcc_negative_offset() {
        let mut bus = MockBus::new();
        let mut cpu = CPU::new(Box::new(bus));
        cpu.flags.remove(CpuFlags::CARRY);

        let program = vec![0x90, 0xFB]; // BCC with offset of -5
        cpu.interpret(program);

        assert_eq!(cpu.program_counter, 0x00u16.wrapping_add(2).wrapping_sub(5));
    }

    #[test]
    fn test_bcs_branch_taken() {
        let mut bus = MockBus::new();
        let mut cpu = CPU::new(Box::new(bus));
        cpu.flags.insert(CpuFlags::CARRY);

        let program = vec![0xB0, 0x05]; // BCS with offset of 5
        cpu.interpret(program);

        assert_eq!(cpu.program_counter, 0x0007);
    }

    #[test]
    fn test_bcs_branch_not_taken() {
        let mut bus = MockBus::new();
        let mut cpu = CPU::new(Box::new(bus));
        cpu.flags.remove(CpuFlags::CARRY);

        let program = vec![0xB0, 0x05]; // BCS with offset of 5
        cpu.interpret(program);

        assert_eq!(cpu.program_counter, 0x02);
    }

    #[test]
    fn test_bcs_negative_offset() {
        let mut bus = MockBus::new();
        let mut cpu = CPU::new(Box::new(bus));
        cpu.flags.insert(CpuFlags::CARRY);

        let program = vec![0xB0, 0xFB]; // BCS with offset of -5
        cpu.interpret(program);

        assert_eq!(cpu.program_counter, (0xFFFBu16).wrapping_add(2));
    }

    #[test]
    fn test_bcs_nop5() {
        let mut bus = MockBus::new();
        let mut cpu = CPU::new(Box::new(bus));
        cpu.flags.insert(CpuFlags::CARRY);
        cpu.program_counter = 0;

        let program = vec![0xB0, 0x05, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA]; // BCS +5, followed by 5 NOPs
        cpu.interpret(program);

        assert_eq!(cpu.program_counter, 7);
    }

    #[test]
    fn test_jmp_absolute() {
        let mut bus = MockBus::new();
        let mut cpu = CPU::new(Box::new(bus));

        // JMP Absolute 0x4C 0x1234
        let program = vec![0x4C, 0x34, 0x12];
        cpu.interpret(program);

        assert_eq!(cpu.program_counter, 0x1234);
    }

    #[test]
    fn test_jmp_indirect() {
        let mut bus = MockBus::new();
        let mut cpu = CPU::new(Box::new(bus));
        cpu.program_counter = 0;

        // set up the JMP indirect
        cpu.mem_write(0x2000, 0x78); // LSB
        cpu.mem_write(0x2001, 0x56); // MSB

        // JMP Indirect 0x6C 0x2000
        let program = vec![0x6C, 0x00, 0x20];
        cpu.interpret(program);

        assert_eq!(cpu.program_counter, 0x5678);
    }

    #[test]
    fn test_jsr() {
        let mut bus = MockBus::new();
        let mut cpu = CPU::new(Box::new(bus));

        cpu.stack_pointer = 0xFF;

        let program = vec![0x20, 0x34, 0x12]; // JSR $1234
        cpu.interpret(program);

        assert_eq!(cpu.program_counter, 0x1234, "Program counter should be set to the target address");

        assert_eq!(cpu.stack_pointer, 0xFD, "Stack pointer should be decremented by 2");

        // Check the values on the stack
        assert_eq!(cpu.pop_stack(), 0x02, "Low byte of return address should be on the stack");
        assert_eq!(cpu.pop_stack(), 0x00, "High byte of return address should be on the stack");
    }
}
