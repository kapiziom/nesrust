mod addressing;
mod operation_codes;
mod cpu_flags;
mod cpu_traits;

use crate::emulator::core::cpu_bus::CpuBus;
pub use operation_codes::*;
pub use addressing::*;
pub use cpu_traits::*;
use crate::emulator::cpu::flag_operations::FlagOperations;

pub struct CPU<'a> {
    pub register_a: u8,
    pub register_x: u8,
    pub register_y: u8,
    pub stack_pointer: u8,
    pub program_counter: u16,
    pub(crate) flags: cpu_flags::CpuFlags,
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
            flags: cpu_flags::CpuFlags::INTERRUPT_DISABLE | cpu_flags::CpuFlags::BREAK_COMMAND | cpu_flags::CpuFlags::UNUSED,
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

    fn update_zero_and_negative_flags(&mut self, result: u8) {
        self.set_flag(cpu_flags::CpuFlags::ZERO, result == 0);
        self.set_flag(cpu_flags::CpuFlags::NEGATIVE, result & 0x80 != 0);
    }

    fn push_stack(&mut self, value: u8) {
        self.bus.write(0x0100 + self.stack_pointer as u16, value);
        self.stack_pointer = self.stack_pointer.wrapping_sub(1);
    }

    fn pop_stack(&mut self) -> u8 {
        self.stack_pointer = self.stack_pointer.wrapping_add(1);
        self.bus.read(0x0100 + self.stack_pointer as u16)
    }

    fn adc(&mut self, mode: &AddressingMode, program: &[u8]) {
        let operand = self.get_operand(mode, program);
        // TODO
    }

    fn get_operand(&self, mode: &AddressingMode, program: &[u8]) -> u8 {
        match mode {
            AddressingMode::Immediate => program[self.program_counter as usize],
            AddressingMode::ZeroPage => {
                let addr = program[self.program_counter as usize] as u16;
                self.bus.read(addr)
            },
            _ => panic!("Unimplemented addressing mode"),
            // TODO
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
