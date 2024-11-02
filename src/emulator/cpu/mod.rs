mod addressing;
mod operation_codes;
mod cpu_flags;
mod cpu_traits;

use crate::emulator::core::cpu_bus::CpuBus;
pub use operation_codes::*;
pub use addressing::*;
pub use cpu_traits::*;
use crate::emulator::cpu::addressing_mode_operations::AddressingModeOperations;
use crate::emulator::cpu::cpu_flags::CpuFlags;
use crate::emulator::cpu::flag_operations::FlagOperations;
use crate::emulator::cpu::instruction_executions::InstructionExecutions;

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
            flags: CpuFlags::INTERRUPT_DISABLE | CpuFlags::BREAK_COMMAND | CpuFlags::UNUSED,
            bus,
        };
    }

    pub fn interpret(&mut self, program: Vec<u8>) {
        self.program_counter = 0;

        loop {
            let operation_code = program[self.program_counter as usize];
            self.program_counter += 1;

            if let Some(info) = CPU_OPERATION_CODES_MAP.get(&operation_code) {
                match operation_code {
                    // ADC
                    0x69 | 0x65 | 0x75 | 0x6D | 0x7D | 0x79 | 0x61 | 0x71 => {
                        self.adc(&info.addressing_mode, &program);
                    }
                    0x00 => return, // BRK
                    _ => {
                        println!("Unknown operation_code: {}", operation_code);
                        return;
                    }
                }
                self.program_counter += (info.bytes - 1) as u16;
            }
            else {
                println!("Unknown operation_code: {}", operation_code);
                return;
            }
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

    pub (super) fn mem_read(&mut self, pos: u16) -> u8 {
        self.bus.read(pos)
    }

    pub (super) fn mem_read_u16(&mut self, pos: u16) -> u16 {
        self.bus.read_u16(pos)
    }

    pub (super) fn mem_write(&mut self, pos: u16, data: u8) {
        self.bus.write(pos, data);
    }
}

#[cfg(test)]
mod test {
    use super::*;
}
