use crate::emulator::bus::cpu_bus::CpuBus;
use crate::emulator::cpu::CPU;
use crate::emulator::cpu::flags::CpuFlags;

pub (super) struct Interrupt {
    pub(super) vector_addr: u16,
    pub(super) b_flag_mask: u8,
}

// https://www.nesdev.org/wiki/Status_flags#The_B_flag
// The pushed status always has bit 5 set; bit 4 (B) is set only for BRK (and PHP), never for NMI/IRQ.
pub (super) const BRK: Interrupt = Interrupt {
    vector_addr: 0xFFFE,
    b_flag_mask: 0b00110000,
};

pub (super) const IRQ: Interrupt = Interrupt {
    vector_addr: 0xFFFE,
    b_flag_mask: 0b00100000,
};

pub (super) const NMI: Interrupt = Interrupt {
    vector_addr: 0xFFFA,
    b_flag_mask: 0b00100000,
};

const RESET_VECTOR: u16 = 0xFFFC;

impl<B: CpuBus> CPU<B> {
    /// The last five cycles shared by BRK, IRQ and NMI: push PC and status, mask IRQs, load the vector.
    /// `program_counter` must already hold the address to return to.
    pub (super) fn enter_interrupt(&mut self, interrupt: Interrupt) {
        self.push_stack_u16(self.program_counter);

        // B is not a real register bit: it only exists in the pushed copy
        let pushed_status = (self.flags.bits() & !0b0011_0000) | interrupt.b_flag_mask;
        self.push_stack(pushed_status);

        self.insert_flag(CpuFlags::INTERRUPT_DISABLE);

        let low = self.read(interrupt.vector_addr) as u16;
        let high = self.read(interrupt.vector_addr.wrapping_add(1)) as u16;
        self.program_counter = (high << 8) | low;
    }

    /// Services a pending NMI (edge, has priority) or IRQ (level, masked by the I flag)
    /// before the next opcode is fetched. Returns true if an interrupt was taken (7 cycles).
    pub (super) fn poll_interrupts(&mut self) -> bool {
        let interrupt = if self.bus.fetch_nmi().is_some() {
            NMI
        } else if self.bus.irq_pending() && !self.contains_flag(CpuFlags::INTERRUPT_DISABLE) {
            IRQ
        } else {
            return false;
        };

        // two cycles are spent re-reading the interrupted instruction's opcode
        self.read(self.program_counter);
        self.read(self.program_counter);
        self.enter_interrupt(interrupt);
        true
    }

    /// Soft reset: registers keep their values, SP drops by 3 (nothing is written), PC comes from $FFFC. 7 cycles.
    pub fn reset(&mut self) {
        self.read(self.program_counter);
        self.read(self.program_counter);

        for _ in 0..3 {
            self.stack_dummy_read();
            self.stack_pointer = self.stack_pointer.wrapping_sub(1);
        }

        self.insert_flag(CpuFlags::INTERRUPT_DISABLE);

        let low = self.read(RESET_VECTOR) as u16;
        let high = self.read(RESET_VECTOR + 1) as u16;
        self.program_counter = (high << 8) | low;
    }
}
