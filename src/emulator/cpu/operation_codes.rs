use lazy_static::lazy_static;
use std::collections::HashMap;
use crate::emulator::cpu::addressing::AddressingMode;

/*
CPU instructions
https://www.nesdev.org/obelisk-6502-guide/reference.html
*/

#[derive(Clone, Copy)]
pub struct OperationCode {
    pub code: u8,
    pub mnemonic: &'static str,
    pub (super) addressing_mode: AddressingMode,
    pub (super) bytes: u8,
    pub cycles: u8,
}

impl OperationCode {
    fn new(code: u8, mnemonic: &'static str, addressing_mode: AddressingMode, bytes: u8, cycles: u8)
        -> Self { OperationCode { code, mnemonic, addressing_mode, bytes, cycles } }
}

lazy_static! {
    pub static ref CPU_OPERATION_CODES_VEC: Vec<OperationCode> = vec![
        // ADC - Add with Carry
        OperationCode::new(0x69, "ADC", AddressingMode::Immediate, 2, 2),
        OperationCode::new(0x65, "ADC", AddressingMode::ZeroPage, 2, 3),
        OperationCode::new(0x75, "ADC", AddressingMode::ZeroPageX, 2, 4),
        OperationCode::new(0x6D, "ADC", AddressingMode::Absolute, 3, 4),
        OperationCode::new(0x7D, "ADC", AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
        OperationCode::new(0x79, "ADC", AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),
        OperationCode::new(0x61, "ADC", AddressingMode::IndirectX, 2, 6),
        OperationCode::new(0x71, "ADC", AddressingMode::IndirectY, 2, 5 /* (+1 if page crossed) */),

        // AND - Logical AND
        OperationCode::new(0x29, "AND", AddressingMode::Immediate, 2, 2),
        OperationCode::new(0x25, "AND", AddressingMode::ZeroPage, 2, 3),
        OperationCode::new(0x35, "AND", AddressingMode::ZeroPageX, 2, 4),
        OperationCode::new(0x2D, "AND", AddressingMode::Absolute, 3, 4),
        OperationCode::new(0x3D, "AND", AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
        OperationCode::new(0x39, "AND", AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),
        OperationCode::new(0x21, "AND", AddressingMode::IndirectX, 2, 6),
        OperationCode::new(0x31, "AND", AddressingMode::IndirectY, 2, 5 /* (+1 if page crossed) */),

        // ASL - Arithmetic Shift Left
        OperationCode::new(0x0A, "ASL", AddressingMode::Accumulator, 1, 2),
        OperationCode::new(0x06, "ASL", AddressingMode::ZeroPage, 2, 5),
        OperationCode::new(0x16, "ASL", AddressingMode::ZeroPageX, 2, 6),
        OperationCode::new(0x0E, "ASL", AddressingMode::Absolute, 3, 6),
        OperationCode::new(0x1E, "ASL", AddressingMode::AbsoluteX, 3, 7),

        // BCC - Branch if Carry Clear
        OperationCode::new(0x90, "BCC", AddressingMode::Relative, 2, 2 /* (+1 if branch succeeds+2 if to a new page) */),

        // BCS - Branch if Carry Set
        OperationCode::new(0xB0, "BCS", AddressingMode::Relative, 2, 2 /* (+1 if branch succeeds+2 if to a new page) */),

        // BEQ - Branch if Equal
        OperationCode::new(0xF0, "BEQ", AddressingMode::Relative, 2, 2 /* (+1 if branch succeeds+2 if to a new page) */),

        // BIT - Bit Test
        OperationCode::new(0x24, "BIT", AddressingMode::ZeroPage, 2, 3),
        OperationCode::new(0x2C, "BIT", AddressingMode::Absolute, 3, 4),

        // BMI - Branch if Minus
        OperationCode::new(0x30, "BMI", AddressingMode::Relative, 2, 2 /* (+1 if branch succeeds+2 if to a new page) */),

        // BNE - Branch if Not Equal
        OperationCode::new(0xD0, "BNE", AddressingMode::Relative, 2, 2 /* (+1 if branch succeeds+2 if to a new page) */),

        // BPL - Branch if Positive
        OperationCode::new(0x10, "BPL", AddressingMode::Relative, 2, 2 /* (+1 if branch succeeds+2 if to a new page) */),

        // BRK - Force Interrupt
        OperationCode::new(0x00, "BRK", AddressingMode::Implicit, 1, 7),

        // BVC - Branch if Overflow Clear
        OperationCode::new(0x50, "BVC", AddressingMode::Relative, 2, 2 /* (+1 if branch succeeds+2 if to a new page) */),

        // BVS - Branch if Overflow Set
        OperationCode::new(0x70, "BVS", AddressingMode::Relative, 2, 2 /* (+1 if branch succeeds+2 if to a new page) */),

        // CLC - Clear Carry Flag
        OperationCode::new(0x18, "CLC", AddressingMode::Implicit, 1, 2),

        // CLD - Clear Decimal Mode
        OperationCode::new(0xD8, "CLD", AddressingMode::Implicit, 1, 2),

        // CLI - Clear Interrupt Disable
        OperationCode::new(0x58, "CLI", AddressingMode::Implicit, 1, 2),

        // CLV - Clear Overflow Flag
        OperationCode::new(0xB8, "CLV", AddressingMode::Implicit, 1, 2),

        // CMP - Compare
        OperationCode::new(0xC9, "CMP", AddressingMode::Immediate, 2, 2),
        OperationCode::new(0xC5, "CMP", AddressingMode::ZeroPage, 2, 3),
        OperationCode::new(0xD5, "CMP", AddressingMode::ZeroPageX, 2, 4),
        OperationCode::new(0xCD, "CMP", AddressingMode::Absolute, 3, 4),
        OperationCode::new(0xDD, "CMP", AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
        OperationCode::new(0xD9, "CMP", AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),
        OperationCode::new(0xC1, "CMP", AddressingMode::IndirectX, 2, 6),
        OperationCode::new(0xD1, "CMP", AddressingMode::IndirectY, 2, 5 /* (+1 if page crossed) */),

        // CPX - Compare X Register
        OperationCode::new(0xE0, "CPX", AddressingMode::Immediate, 2, 2),
        OperationCode::new(0xE4, "CPX", AddressingMode::ZeroPage, 2, 3),
        OperationCode::new(0xEC, "CPX", AddressingMode::Absolute, 3, 4),

        // CPY - Compare Y Register
        OperationCode::new(0xC0, "CPY", AddressingMode::Immediate, 2, 2),
        OperationCode::new(0xC4, "CPY", AddressingMode::ZeroPage, 2, 3),
        OperationCode::new(0xCC, "CPY", AddressingMode::Absolute, 3, 4),

        // DEC - Decrement Memory
        OperationCode::new(0xC6, "DEC", AddressingMode::ZeroPage, 2, 5),
        OperationCode::new(0xD6, "DEC", AddressingMode::ZeroPageX, 2, 6),
        OperationCode::new(0xCE, "DEC", AddressingMode::Absolute, 3, 6),
        OperationCode::new(0xDE, "DEC", AddressingMode::AbsoluteX, 3, 7),

        // DEX - Decrement X Register
        OperationCode::new(0xCA, "DEX", AddressingMode::Implicit, 1, 2),

        // DEY - Decrement Y Register
        OperationCode::new(0x88, "DEY", AddressingMode::Implicit, 1, 2),

        // EOR - Exclusive OR
        OperationCode::new(0x49, "EOR", AddressingMode::Immediate, 2, 2),
        OperationCode::new(0x45, "EOR", AddressingMode::ZeroPage, 2, 3),
        OperationCode::new(0x55, "EOR", AddressingMode::ZeroPageX, 2, 4),
        OperationCode::new(0x4D, "EOR", AddressingMode::Absolute, 3, 4),
        OperationCode::new(0x5D, "EOR", AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
        OperationCode::new(0x59, "EOR", AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),
        OperationCode::new(0x41, "EOR", AddressingMode::IndirectX, 2, 6),
        OperationCode::new(0x51, "EOR", AddressingMode::IndirectY, 2, 5 /* (+1 if page crossed) */),

        // INC - Increment Memory
        OperationCode::new(0xE6, "INC", AddressingMode::ZeroPage, 2, 5),
        OperationCode::new(0xF6, "INC", AddressingMode::ZeroPageX, 2, 6),
        OperationCode::new(0xEE, "INC", AddressingMode::Absolute, 3, 6),
        OperationCode::new(0xFE, "INC", AddressingMode::AbsoluteX, 3, 7),

        // INX - Increment X Register
        OperationCode::new(0xE8, "INX", AddressingMode::Implicit, 1, 2),

        // INY - Increment Y Register
        OperationCode::new(0xC8, "INY", AddressingMode::Implicit, 1, 2),

        // JMP - Jump
        OperationCode::new(0x4C, "JMP", AddressingMode::Absolute, 3, 3),
        OperationCode::new(0x6C, "JMP", AddressingMode::Indirect, 3, 5),

        // JSR - Jump to Subroutine
        OperationCode::new(0x20, "JSR", AddressingMode::Absolute, 3, 6),

        // LDA - Load Accumulator
        OperationCode::new(0xA9, "LDA", AddressingMode::Immediate, 2, 2),
        OperationCode::new(0xA5, "LDA", AddressingMode::ZeroPage, 2, 3),
        OperationCode::new(0xB5, "LDA", AddressingMode::ZeroPageX, 2, 4),
        OperationCode::new(0xAD, "LDA", AddressingMode::Absolute, 3, 4),
        OperationCode::new(0xBD, "LDA", AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
        OperationCode::new(0xB9, "LDA", AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),
        OperationCode::new(0xA1, "LDA", AddressingMode::IndirectX, 2, 6),
        OperationCode::new(0xB1, "LDA", AddressingMode::IndirectY, 2, 5 /* (+1 if page crossed) */),

        // LDX - Load X Register
        OperationCode::new(0xA2, "LDX", AddressingMode::Immediate, 2, 2),
        OperationCode::new(0xA6, "LDX", AddressingMode::ZeroPage, 2, 3),
        OperationCode::new(0xB6, "LDX", AddressingMode::ZeroPageY, 2, 4),
        OperationCode::new(0xAE, "LDX", AddressingMode::Absolute, 3, 4),
        OperationCode::new(0xBE, "LDX", AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),

        // LDY - Load Y Register
        OperationCode::new(0xA0, "LDY", AddressingMode::Immediate, 2, 2),
        OperationCode::new(0xA4, "LDY", AddressingMode::ZeroPage, 2, 3),
        OperationCode::new(0xB4, "LDY", AddressingMode::ZeroPageX, 2, 4),
        OperationCode::new(0xAC, "LDY", AddressingMode::Absolute, 3, 4),
        OperationCode::new(0xBC, "LDY", AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),

        // LSR - Logical Shift Right
        OperationCode::new(0x4A, "LSR", AddressingMode::Accumulator, 1, 2),
        OperationCode::new(0x46, "LSR", AddressingMode::ZeroPage, 2, 5),
        OperationCode::new(0x56, "LSR", AddressingMode::ZeroPageX, 2, 6),
        OperationCode::new(0x4E, "LSR", AddressingMode::Absolute, 3, 6),
        OperationCode::new(0x5E, "LSR", AddressingMode::AbsoluteX, 3, 7),

        // NOP - No Operation
        OperationCode::new(0xEA, "NOP", AddressingMode::Implicit, 1, 2),

        // ORA - Logical Inclusive OR
        OperationCode::new(0x09, "ORA", AddressingMode::Immediate, 2, 2),
        OperationCode::new(0x05, "ORA", AddressingMode::ZeroPage, 2, 3),
        OperationCode::new(0x15, "ORA", AddressingMode::ZeroPageX, 2, 4),
        OperationCode::new(0x0D, "ORA", AddressingMode::Absolute, 3, 4),
        OperationCode::new(0x1D, "ORA", AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
        OperationCode::new(0x19, "ORA", AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),
        OperationCode::new(0x01, "ORA", AddressingMode::IndirectX, 2, 6),
        OperationCode::new(0x11, "ORA", AddressingMode::IndirectY, 2, 5 /* (+1 if page crossed) */),

        // PHA - Push Accumulator
        OperationCode::new(0x48, "PHA", AddressingMode::Implicit, 1, 3),

        // PHP - Push Processor Status
        OperationCode::new(0x08, "PHP", AddressingMode::Implicit, 1, 3),

        // PLA - Pull Accumulator
        OperationCode::new(0x68, "PLA", AddressingMode::Implicit, 1, 4),

        // PLP - Pull Processor Status
        OperationCode::new(0x28, "PLP", AddressingMode::Implicit, 1, 4),

        // ROL - Rotate Left
        OperationCode::new(0x2A, "ROL", AddressingMode::Accumulator, 1, 2),
        OperationCode::new(0x26, "ROL", AddressingMode::ZeroPage, 2, 5),
        OperationCode::new(0x36, "ROL", AddressingMode::ZeroPageX, 2, 6),
        OperationCode::new(0x2E, "ROL", AddressingMode::Absolute, 3, 6),
        OperationCode::new(0x3E, "ROL", AddressingMode::AbsoluteX, 3, 7),

        // ROR - Rotate Right
        OperationCode::new(0x6A, "ROR", AddressingMode::Accumulator, 1, 2),
        OperationCode::new(0x66, "ROR", AddressingMode::ZeroPage, 2, 5),
        OperationCode::new(0x76, "ROR", AddressingMode::ZeroPageX, 2, 6),
        OperationCode::new(0x6E, "ROR", AddressingMode::Absolute, 3, 6),
        OperationCode::new(0x7E, "ROR", AddressingMode::AbsoluteX, 3, 7),

        // RTI - Return from Interrupt
        OperationCode::new(0x40, "RTI", AddressingMode::Implicit, 1, 6),

        // RTS - Return from Subroutine
        OperationCode::new(0x60, "RTS", AddressingMode::Implicit, 1, 6),

        // SBC - Subtract with Carry
        OperationCode::new(0xE9, "SBC", AddressingMode::Immediate, 2, 2),
        OperationCode::new(0xE5, "SBC", AddressingMode::ZeroPage, 2, 3),
        OperationCode::new(0xF5, "SBC", AddressingMode::ZeroPageX, 2, 4),
        OperationCode::new(0xED, "SBC", AddressingMode::Absolute, 3, 4),
        OperationCode::new(0xFD, "SBC", AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
        OperationCode::new(0xF9, "SBC", AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),
        OperationCode::new(0xE1, "SBC", AddressingMode::IndirectX, 2, 6),
        OperationCode::new(0xF1, "SBC", AddressingMode::IndirectY, 2, 5 /* (+1 if page crossed) */),

        // SEC - Set Carry Flag
        OperationCode::new(0x38, "SEC", AddressingMode::Implicit, 1, 2),

        // SED - Set Decimal Flag
        OperationCode::new(0xF8, "SED", AddressingMode::Implicit, 1, 2),

        // SEI - Set Interrupt Disable
        OperationCode::new(0x78, "SEI", AddressingMode::Implicit, 1, 2),

        // STA - Store Accumulator
        OperationCode::new(0x85, "STA", AddressingMode::ZeroPage, 2, 3),
        OperationCode::new(0x95, "STA", AddressingMode::ZeroPageX, 2, 4),
        OperationCode::new(0x8D, "STA", AddressingMode::Absolute, 3, 4),
        OperationCode::new(0x9D, "STA", AddressingMode::AbsoluteX, 3, 5),
        OperationCode::new(0x99, "STA", AddressingMode::AbsoluteY, 3, 5),
        OperationCode::new(0x81, "STA", AddressingMode::IndirectX, 2, 6),
        OperationCode::new(0x91, "STA", AddressingMode::IndirectY, 2, 6),

        // STX - Store X Register
        OperationCode::new(0x86, "STX", AddressingMode::ZeroPage, 2, 3),
        OperationCode::new(0x96, "STX", AddressingMode::ZeroPageY, 2, 4),
        OperationCode::new(0x8E, "STX", AddressingMode::Absolute, 3, 4),

        // STY - Store Y Register
        OperationCode::new(0x84, "STY", AddressingMode::ZeroPage, 2, 3),
        OperationCode::new(0x94, "STY", AddressingMode::ZeroPageX, 2, 4),
        OperationCode::new(0x8C, "STY", AddressingMode::Absolute, 3, 4),

        // TAX - Transfer Accumulator to X
        OperationCode::new(0xAA, "TAX", AddressingMode::Implicit, 1, 2),

        // TAY - Transfer Accumulator to Y
        OperationCode::new(0xA8, "TAY", AddressingMode::Implicit, 1, 2),

        // TSX - Transfer Stack Pointer to X
        OperationCode::new(0xBA, "TSX", AddressingMode::Implicit, 1, 2),

        // TXA - Transfer X to Accumulator
        OperationCode::new(0x8A, "TXA", AddressingMode::Implicit, 1, 2),

        // TXS - Transfer X to Stack Pointer
        OperationCode::new(0x9A, "TXS", AddressingMode::Implicit, 1, 2),

        // TYA - Transfer Y to Accumulator
        OperationCode::new(0x98, "TYA", AddressingMode::Implicit, 1, 2)
    ];

    pub static ref CPU_OPERATION_CODES_MAP: HashMap<u8, &'static OperationCode> = {
       let mut map = HashMap::new();
       for opcode in CPU_OPERATION_CODES_VEC.iter() {
           map.insert(opcode.code, opcode);
       }
       map
   };
}