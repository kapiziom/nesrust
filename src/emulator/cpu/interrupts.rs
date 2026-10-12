use crate::emulator::cpu::CPU;
use crate::emulator::cpu::flags::CpuFlags;

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

// https://www.nesdev.org/wiki/Status_flags#The_B_flag
// The pushed status always has bit 5 set; bit 4 (B) is set only for BRK (and PHP), never for NMI/IRQ.
pub (super) const BRK: Interrupt = Interrupt {
    interrupt_type: InterruptType::BRK,
    vector_addr: 0xFFFE,
    b_flag_mask: 0b00110000,
    cpu_cycles: 7,
};

pub (super) const IRQ: Interrupt = Interrupt {
    interrupt_type: InterruptType::IRQ,
    vector_addr: 0xFFFE,
    b_flag_mask: 0b00100000,
    cpu_cycles: 7,
};

pub (super) const NMI: Interrupt = Interrupt {
    interrupt_type: InterruptType::NMI,
    vector_addr: 0xFFFA,
    b_flag_mask: 0b00100000,
    cpu_cycles: 7,
};

impl<'a> CPU<'a> {
    /// Pushes PC and status, masks IRQs and jumps through the interrupt vector.
    /// `program_counter` must already hold the address to return to.
    pub (super) fn handle_interrupt(&mut self, interrupt: Interrupt) {
        self.push_stack_u16(self.program_counter);

        // B is not a real register bit: it only exists in the pushed copy
        let pushed_status = (self.flags.bits() & !0b0011_0000) | interrupt.b_flag_mask;
        self.push_stack(pushed_status);

        self.insert_flag(CpuFlags::INTERRUPT_DISABLE);

        self.tick(interrupt.cpu_cycles);

        let vector_address = self.mem_read_u16(interrupt.vector_addr);
        self.program_counter = vector_address;
    }

    /// Services a pending NMI (edge, has priority) or IRQ (level, masked by the I flag)
    /// before the next opcode is fetched. Returns true if an interrupt was taken.
    pub (super) fn poll_interrupts(&mut self) -> bool {
        if self.bus.fetch_nmi().is_some() {
            self.handle_interrupt(NMI);
            return true;
        }

        if self.bus.irq_pending() && !self.contains_flag(CpuFlags::INTERRUPT_DISABLE) {
            self.handle_interrupt(IRQ);
            return true;
        }

        false
    }
}
