// https://www.nesdev.org/obelisk-6502-guide/addressing.html#IMM
#[derive(Clone, Copy)]
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