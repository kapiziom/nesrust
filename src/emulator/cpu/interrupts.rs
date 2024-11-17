use bitflags::Flags;
use crate::emulator::cpu::CPU;
use crate::emulator::cpu::flags::{CpuFlags, FlagOperations};
use crate::emulator::cpu::instructions::CpuInstructions;
use crate::emulator::cpu::stack::StackOperations;

#[derive(PartialEq, Eq)]
pub enum InterruptType {
    BRK,
    IRQ, // APU, unimplemented mappers
    NMI,
}

#[derive(PartialEq, Eq)]
pub (super) struct Interrupt {
    pub(super) interrupt_type: InterruptType,
    pub(super) vector_addr: u16,
    pub(super) b_flag_mask: u8,
    pub(super) cpu_cycles: u8,
}

pub (super) const BRK: Interrupt = Interrupt {
    interrupt_type: InterruptType::BRK,
    vector_addr: 0xFFFE,
    b_flag_mask: 0b00110000,
    cpu_cycles: 7, // BRK op_code
};

pub (super) const IRQ: Interrupt = Interrupt {
    interrupt_type: InterruptType::IRQ,
    vector_addr: 0xFFFE,
    b_flag_mask: 0b00100000,
    cpu_cycles: 7 // 2
};

pub (super) const NMI: Interrupt = Interrupt {
    interrupt_type: InterruptType::NMI,
    vector_addr: 0xFFFA,
    b_flag_mask: 0b00100000,
    cpu_cycles: 8 // 2
};

pub trait CpuInterrupts {
    fn handle_interrupt(&mut self, interrupt: Interrupt);
}

impl<'a> CpuInterrupts for CPU<'a> {
    fn handle_interrupt(&mut self, interrupt: Interrupt) {
        self.push_stack_u16(self.program_counter);
        let mut flag = self.flags.clone();

        flag.set(CpuFlags::BREAK, interrupt.b_flag_mask & 0b010000 == 1);
        flag.set(CpuFlags::UNUSED, interrupt.b_flag_mask & 0b100000 == 1);

        self.push_stack(flag.bits());

        self.insert_flag(CpuFlags::INTERRUPT_DISABLE);

        self.tick(interrupt.cpu_cycles);

        let vector_address = self.mem_read_u16(interrupt.vector_addr);
        self.program_counter = vector_address;
    }
}