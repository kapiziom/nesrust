use crate::emulator::cpu::CPU;

// https://www.nesdev.org/obelisk-6502-guide/addressing.html#IMM
#[derive(Clone, Copy, PartialEq)]
pub (super) enum AddressingMode {
    Implicit,
    Accumulator,
    Immediate,
    ZeroPage,
    ZeroPageX,
    ZeroPageY,
    Relative,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    Indirect,
    IndirectX,
    IndirectY
}

impl<'a> CPU<'a> {
    pub (super) fn get_immediate(&mut self) -> u8 {
        self.mem_read(self.program_counter)
    }

    pub (super) fn get_zero_page_address(&mut self) -> u16 {
        self.mem_read(self.program_counter) as u16
    }

    pub (super) fn get_zero_page(&mut self) -> u8 {
        let address = self.get_zero_page_address();
        self.mem_read(address)
    }

    pub (super) fn get_zero_page_x_address(&mut self) -> u16 {
        (self.mem_read(self.program_counter).wrapping_add(self.register_x)) as u16
    }

    pub (super) fn get_zero_page_x(&mut self) -> u8 {
        let address = self.get_zero_page_x_address();
        self.mem_read(address)
    }

    pub (super) fn get_zero_page_y_address(&mut self) -> u16 {
        (self.mem_read(self.program_counter).wrapping_add(self.register_y)) as u16
    }

    pub (super) fn get_zero_page_y(&mut self) -> u8 {
        let address = self.get_zero_page_y_address();
        self.mem_read(address)
    }

    pub (super) fn get_absolute_address(&mut self) -> u16 {
        let low = self.mem_read(self.program_counter) as u16;
        let high = self.mem_read(self.program_counter + 1) as u16;
        (high << 8) | low
    }

    pub (super) fn get_absolute(&mut self) -> u8 {
        let address = self.get_absolute_address();
        self.mem_read(address)
    }

    pub (super) fn get_absolute_x_address(&mut self) -> u16 {
        self.get_absolute_address().wrapping_add(self.register_x as u16)
    }

    pub (super) fn get_absolute_x(&mut self) -> u8 {
        let address = self.get_absolute_x_address();
        self.mem_read(address)
    }

    pub (super) fn get_absolute_y_address(&mut self) -> u16 {
        self.get_absolute_address().wrapping_add(self.register_y as u16)
    }

    pub (super) fn get_absolute_y(&mut self) -> u8 {
        let address = self.get_absolute_y_address();
        self.mem_read(address)
    }

    pub (super) fn get_indirect_address(&mut self) -> u16 {
        let ptr = self.get_absolute_address();
        let low = self.mem_read(ptr) as u16;
        let high = self.mem_read(ptr.wrapping_add(1)) as u16;
        (high << 8) | low
    }

    pub (super) fn get_indirect(&mut self) -> u8 {
        let address = self.get_indirect_address();
        self.mem_read(address)
    }

    pub (super) fn get_indirect_x_address(&mut self) -> u16 {
        let base = self.mem_read(self.program_counter);
        let ptr = base.wrapping_add(self.register_x);
        // the pointer wraps inside the zero page
        let low = self.mem_read(ptr as u16) as u16;
        let high = self.mem_read(ptr.wrapping_add(1) as u16) as u16;
        (high << 8) | low
    }

    pub (super) fn get_indirect_x(&mut self) -> u8 {
        let address = self.get_indirect_x_address();
        self.mem_read(address)
    }

    fn get_indirect_y_base(&mut self) -> u16 {
        let base = self.mem_read(self.program_counter);
        // the pointer wraps inside the zero page
        let low = self.mem_read(base as u16) as u16;
        let high = self.mem_read(base.wrapping_add(1) as u16) as u16;
        (high << 8) | low
    }

    pub (super) fn get_indirect_y_address(&mut self) -> u16 {
        self.get_indirect_y_base().wrapping_add(self.register_y as u16)
    }

    /// True when an indexed access in `mode` crosses a page boundary (costs an extra cycle on reads).
    pub (super) fn page_crossed(&mut self, mode: &AddressingMode) -> bool {
        let (base, index) = match mode {
            AddressingMode::AbsoluteX => (self.get_absolute_address(), self.register_x),
            AddressingMode::AbsoluteY => (self.get_absolute_address(), self.register_y),
            AddressingMode::IndirectY => (self.get_indirect_y_base(), self.register_y),
            _ => return false,
        };

        (base & 0xFF00) != (base.wrapping_add(index as u16) & 0xFF00)
    }

    pub (super) fn get_indirect_y(&mut self) -> u8 {
        let address = self.get_indirect_y_address();
        self.mem_read(address)
    }

    pub (super) fn get_relative(&mut self) -> u8 {
        self.mem_read(self.program_counter)
    }

    pub (super) fn get_accumulator(&mut self) -> u8 {
        self.register_a
    }
}