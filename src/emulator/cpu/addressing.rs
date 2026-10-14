use crate::emulator::bus::cpu_bus::CpuBus;
use crate::emulator::cpu::CPU;

// https://www.nesdev.org/obelisk-6502-guide/addressing.html#IMM
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum AddressingMode {
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
    IndirectY,
}

/// What the instruction does with the effective address. Indexed modes behave differently:
/// reads only pay for a page crossing, writes and read-modify-writes always do the extra (dummy) read.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Access {
    Read,
    Write,
    ReadModifyWrite,
}

// Every bus access below costs exactly one CPU cycle, including the dummy reads the real CPU performs.
// https://www.nesdev.org/6502_cpu.txt
impl<B: CpuBus> CPU<B> {
    /// Reads the byte at PC and advances PC.
    pub(super) fn fetch(&mut self) -> u8 {
        let value = self.read(self.program_counter);
        self.program_counter = self.program_counter.wrapping_add(1);
        value
    }

    pub(super) fn fetch_u16(&mut self) -> u16 {
        let low = self.fetch() as u16;
        let high = self.fetch() as u16;
        (high << 8) | low
    }

    /// Resolves the operand address of `mode`, consuming the operand bytes and the cycles of the addressing mode.
    pub(super) fn effective_address(&mut self, mode: AddressingMode, access: Access) -> u16 {
        match mode {
            // the operand itself is the byte after the opcode
            AddressingMode::Immediate => {
                let address = self.program_counter;
                self.program_counter = self.program_counter.wrapping_add(1);
                address
            }
            AddressingMode::ZeroPage => self.fetch() as u16,
            AddressingMode::ZeroPageX => self.indexed_zero_page(self.register_x),
            AddressingMode::ZeroPageY => self.indexed_zero_page(self.register_y),
            AddressingMode::Absolute => self.fetch_u16(),
            AddressingMode::AbsoluteX => {
                let base = self.fetch_u16();
                self.add_index(base, self.register_x, access)
            }
            AddressingMode::AbsoluteY => {
                let base = self.fetch_u16();
                self.add_index(base, self.register_y, access)
            }
            AddressingMode::Indirect => {
                // JMP ($xxFF) reads the high byte from $xx00 (6502 page-wrap bug)
                let pointer = self.fetch_u16();
                let low = self.read(pointer) as u16;
                let high = self.read((pointer & 0xFF00) | (pointer.wrapping_add(1) & 0x00FF)) as u16;
                (high << 8) | low
            }
            AddressingMode::IndirectX => {
                let base = self.fetch();
                self.read(base as u16); // dummy read while X is added
                let pointer = base.wrapping_add(self.register_x);
                // the pointer wraps inside the zero page
                let low = self.read(pointer as u16) as u16;
                let high = self.read(pointer.wrapping_add(1) as u16) as u16;
                (high << 8) | low
            }
            AddressingMode::IndirectY => {
                let base = self.fetch_indirect_base();
                self.add_index(base, self.register_y, access)
            }
            AddressingMode::Implicit | AddressingMode::Accumulator | AddressingMode::Relative => {
                unreachable!("{:?} has no operand address", mode)
            }
        }
    }

    /// The 16-bit pointer of a (zp),Y operand, read from the zero page (wrapping inside it).
    fn fetch_indirect_base(&mut self) -> u16 {
        let pointer = self.fetch();
        let low = self.read(pointer as u16) as u16;
        let high = self.read(pointer.wrapping_add(1) as u16) as u16;
        (high << 8) | low
    }

    fn indexed_zero_page(&mut self, index: u8) -> u16 {
        let base = self.fetch();
        self.read(base as u16); // dummy read while the index is added
        base.wrapping_add(index) as u16
    }

    /// base + index. The CPU first reads from the address with an uncorrected high byte;
    /// that read is only visible (and costs a cycle) when the page is crossed, or always for writes.
    fn add_index(&mut self, base: u16, index: u8, access: Access) -> u16 {
        let address = base.wrapping_add(index as u16);
        let uncorrected = (base & 0xFF00) | (address & 0x00FF);

        if access != Access::Read || uncorrected != address {
            self.read(uncorrected);
        }

        address
    }

    /// Reads the operand of a read-type instruction.
    pub(super) fn read_operand(&mut self, mode: AddressingMode) -> u8 {
        let address = self.effective_address(mode, Access::Read);
        self.read(address)
    }

    pub(super) fn store(&mut self, mode: AddressingMode, value: u8) {
        let address = self.effective_address(mode, Access::Write);
        self.write(address, value);
    }

    /// Read-modify-write: the CPU writes the unmodified value back before writing the result.
    pub(super) fn modify(&mut self, mode: AddressingMode, operation: impl FnOnce(&mut Self, u8) -> u8) {
        if mode == AddressingMode::Accumulator {
            self.read(self.program_counter); // dummy read
            let result = operation(self, self.register_a);
            self.register_a = result;
            return;
        }

        let address = self.effective_address(mode, Access::ReadModifyWrite);
        let value = self.read(address);
        self.write(address, value);
        let result = operation(self, value);
        self.write(address, result);
    }

    /// SHA/SHX/SHY/TAS: stores `value & (high byte of the base address + 1)`. When the index crosses a page the
    /// value also replaces the high byte of the target address. https://www.nesdev.org/wiki/CPU_unofficial_opcodes
    pub(super) fn store_masked_by_high_byte(&mut self, mode: AddressingMode, value: u8) {
        let (base, index) = match mode {
            AddressingMode::AbsoluteX => (self.fetch_u16(), self.register_x),
            AddressingMode::AbsoluteY => (self.fetch_u16(), self.register_y),
            AddressingMode::IndirectY => (self.fetch_indirect_base(), self.register_y),
            _ => unreachable!("{:?} is not an indexed addressing mode", mode),
        };

        let address = base.wrapping_add(index as u16);
        self.read((base & 0xFF00) | (address & 0x00FF)); // dummy read at the uncorrected address

        let result = value & ((base >> 8) as u8).wrapping_add(1);
        let crossed_page = (base & 0xFF00) != (address & 0xFF00);
        let target = if crossed_page { ((result as u16) << 8) | (address & 0x00FF) } else { address };

        self.write(target, result);
    }

    /// The extra cycle of implied/accumulator instructions: a dummy read of the next byte.
    pub(super) fn implied(&mut self) {
        self.read(self.program_counter);
    }
}
