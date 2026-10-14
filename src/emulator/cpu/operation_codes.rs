use crate::emulator::cpu::addressing::AddressingMode;

/*
CPU instructions
https://www.nesdev.org/obelisk-6502-guide/reference.html
https://www.nesdev.org/wiki/CPU_unofficial_opcodes
*/

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mnemonic {
    ADC,
    AND,
    ASL,
    BCC,
    BCS,
    BEQ,
    BIT,
    BMI,
    BNE,
    BPL,
    BRK,
    BVC,
    BVS,
    CLC,
    CLD,
    CLI,
    CLV,
    CMP,
    CPX,
    CPY,
    DEC,
    DEX,
    DEY,
    EOR,
    INC,
    INX,
    INY,
    JMP,
    JSR,
    LDA,
    LDX,
    LDY,
    LSR,
    NOP,
    ORA,
    PHA,
    PHP,
    PLA,
    PLP,
    ROL,
    ROR,
    RTI,
    RTS,
    SBC,
    SEC,
    SED,
    SEI,
    STA,
    STX,
    STY,
    TAX,
    TAY,
    TSX,
    TXA,
    TXS,
    TYA,
    // unofficial
    LAX,
    SAX,
    DCP,
    ISB,
    SLO,
    RLA,
    SRE,
    RRA,
    ANC,
    ALR,
    ARR,
    AXS,
    // unofficial, unstable
    XAA,
    LXA,
    SHA,
    SHY,
    SHX,
    TAS,
    LAS,
}

#[derive(Clone, Copy, Debug)]
pub struct OperationCode {
    pub code: u8,
    pub mnemonic: Mnemonic,
    pub(super) addressing_mode: AddressingMode,
    pub bytes: u8,
    /// Base cycle count (without branch/page-cross penalties); the CPU derives real timing from bus accesses
    /// and the unit tests check it against this table.
    pub cycles: u8,
    pub official: bool,
    /// Read instructions take one extra cycle when indexing crosses a page boundary.
    /// Stores and read-modify-write instructions always take the worst-case cycles listed in the table.
    pub page_cross_penalty: bool,
}

impl OperationCode {
    const fn new(code: u8, mnemonic: Mnemonic, addressing_mode: AddressingMode, bytes: u8, cycles: u8) -> Self {
        Self::build(code, mnemonic, addressing_mode, bytes, cycles, true)
    }

    const fn unofficial(code: u8, mnemonic: Mnemonic, addressing_mode: AddressingMode, bytes: u8, cycles: u8) -> Self {
        Self::build(code, mnemonic, addressing_mode, bytes, cycles, false)
    }

    const fn build(
        code: u8,
        mnemonic: Mnemonic,
        addressing_mode: AddressingMode,
        bytes: u8,
        cycles: u8,
        official: bool,
    ) -> Self {
        let page_cross_penalty = matches!(
            addressing_mode,
            AddressingMode::AbsoluteX | AddressingMode::AbsoluteY | AddressingMode::IndirectY
        ) && matches!(
            mnemonic,
            Mnemonic::ADC
                | Mnemonic::AND
                | Mnemonic::CMP
                | Mnemonic::EOR
                | Mnemonic::LDA
                | Mnemonic::LDX
                | Mnemonic::LDY
                | Mnemonic::ORA
                | Mnemonic::SBC
                | Mnemonic::NOP
                | Mnemonic::LAX
                | Mnemonic::LAS
        );

        OperationCode { code, mnemonic, addressing_mode, bytes, cycles, official, page_cross_penalty }
    }
}

const fn build_table(codes: &[OperationCode]) -> [Option<OperationCode>; 256] {
    let mut table = [None; 256];
    let mut i = 0;
    while i < codes.len() {
        table[codes[i].code as usize] = Some(codes[i]);
        i += 1;
    }
    table
}

/// Opcode lookup by byte value; `None` for opcodes that are not implemented.
pub static CPU_OPERATION_TABLE: [Option<OperationCode>; 256] = build_table(CPU_OPERATION_CODES);

pub const CPU_OPERATION_CODES: &[OperationCode] = &[
    // ADC - Add with Carry
    OperationCode::new(0x69, Mnemonic::ADC, AddressingMode::Immediate, 2, 2),
    OperationCode::new(0x65, Mnemonic::ADC, AddressingMode::ZeroPage, 2, 3),
    OperationCode::new(0x75, Mnemonic::ADC, AddressingMode::ZeroPageX, 2, 4),
    OperationCode::new(0x6D, Mnemonic::ADC, AddressingMode::Absolute, 3, 4),
    OperationCode::new(0x7D, Mnemonic::ADC, AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
    OperationCode::new(0x79, Mnemonic::ADC, AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),
    OperationCode::new(0x61, Mnemonic::ADC, AddressingMode::IndirectX, 2, 6),
    OperationCode::new(0x71, Mnemonic::ADC, AddressingMode::IndirectY, 2, 5 /* (+1 if page crossed) */),
    // AND - Logical AND
    OperationCode::new(0x29, Mnemonic::AND, AddressingMode::Immediate, 2, 2),
    OperationCode::new(0x25, Mnemonic::AND, AddressingMode::ZeroPage, 2, 3),
    OperationCode::new(0x35, Mnemonic::AND, AddressingMode::ZeroPageX, 2, 4),
    OperationCode::new(0x2D, Mnemonic::AND, AddressingMode::Absolute, 3, 4),
    OperationCode::new(0x3D, Mnemonic::AND, AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
    OperationCode::new(0x39, Mnemonic::AND, AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),
    OperationCode::new(0x21, Mnemonic::AND, AddressingMode::IndirectX, 2, 6),
    OperationCode::new(0x31, Mnemonic::AND, AddressingMode::IndirectY, 2, 5 /* (+1 if page crossed) */),
    // ASL - Arithmetic Shift Left
    OperationCode::new(0x0A, Mnemonic::ASL, AddressingMode::Accumulator, 1, 2),
    OperationCode::new(0x06, Mnemonic::ASL, AddressingMode::ZeroPage, 2, 5),
    OperationCode::new(0x16, Mnemonic::ASL, AddressingMode::ZeroPageX, 2, 6),
    OperationCode::new(0x0E, Mnemonic::ASL, AddressingMode::Absolute, 3, 6),
    OperationCode::new(0x1E, Mnemonic::ASL, AddressingMode::AbsoluteX, 3, 7),
    // BCC - Branch if Carry Clear
    OperationCode::new(
        0x90,
        Mnemonic::BCC,
        AddressingMode::Relative,
        2,
        2, /* (+1 if branch succeeds+2 if to a new page) */
    ),
    // BCS - Branch if Carry Set
    OperationCode::new(
        0xB0,
        Mnemonic::BCS,
        AddressingMode::Relative,
        2,
        2, /* (+1 if branch succeeds+2 if to a new page) */
    ),
    // BEQ - Branch if Equal
    OperationCode::new(
        0xF0,
        Mnemonic::BEQ,
        AddressingMode::Relative,
        2,
        2, /* (+1 if branch succeeds+2 if to a new page) */
    ),
    // BIT - Bit Test
    OperationCode::new(0x24, Mnemonic::BIT, AddressingMode::ZeroPage, 2, 3),
    OperationCode::new(0x2C, Mnemonic::BIT, AddressingMode::Absolute, 3, 4),
    // BMI - Branch if Minus
    OperationCode::new(
        0x30,
        Mnemonic::BMI,
        AddressingMode::Relative,
        2,
        2, /* (+1 if branch succeeds+2 if to a new page) */
    ),
    // BNE - Branch if Not Equal
    OperationCode::new(
        0xD0,
        Mnemonic::BNE,
        AddressingMode::Relative,
        2,
        2, /* (+1 if branch succeeds+2 if to a new page) */
    ),
    // BPL - Branch if Positive
    OperationCode::new(
        0x10,
        Mnemonic::BPL,
        AddressingMode::Relative,
        2,
        2, /* (+1 if branch succeeds+2 if to a new page) */
    ),
    // BRK - Force Interrupt
    OperationCode::new(0x00, Mnemonic::BRK, AddressingMode::Implicit, 1, 7),
    // BVC - Branch if Overflow Clear
    OperationCode::new(
        0x50,
        Mnemonic::BVC,
        AddressingMode::Relative,
        2,
        2, /* (+1 if branch succeeds+2 if to a new page) */
    ),
    // BVS - Branch if Overflow Set
    OperationCode::new(
        0x70,
        Mnemonic::BVS,
        AddressingMode::Relative,
        2,
        2, /* (+1 if branch succeeds+2 if to a new page) */
    ),
    // CLC - Clear Carry Flag
    OperationCode::new(0x18, Mnemonic::CLC, AddressingMode::Implicit, 1, 2),
    // CLD - Clear Decimal Mode
    OperationCode::new(0xD8, Mnemonic::CLD, AddressingMode::Implicit, 1, 2),
    // CLI - Clear Interrupt Disable
    OperationCode::new(0x58, Mnemonic::CLI, AddressingMode::Implicit, 1, 2),
    // CLV - Clear Overflow Flag
    OperationCode::new(0xB8, Mnemonic::CLV, AddressingMode::Implicit, 1, 2),
    // CMP - Compare
    OperationCode::new(0xC9, Mnemonic::CMP, AddressingMode::Immediate, 2, 2),
    OperationCode::new(0xC5, Mnemonic::CMP, AddressingMode::ZeroPage, 2, 3),
    OperationCode::new(0xD5, Mnemonic::CMP, AddressingMode::ZeroPageX, 2, 4),
    OperationCode::new(0xCD, Mnemonic::CMP, AddressingMode::Absolute, 3, 4),
    OperationCode::new(0xDD, Mnemonic::CMP, AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
    OperationCode::new(0xD9, Mnemonic::CMP, AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),
    OperationCode::new(0xC1, Mnemonic::CMP, AddressingMode::IndirectX, 2, 6),
    OperationCode::new(0xD1, Mnemonic::CMP, AddressingMode::IndirectY, 2, 5 /* (+1 if page crossed) */),
    // CPX - Compare X Register
    OperationCode::new(0xE0, Mnemonic::CPX, AddressingMode::Immediate, 2, 2),
    OperationCode::new(0xE4, Mnemonic::CPX, AddressingMode::ZeroPage, 2, 3),
    OperationCode::new(0xEC, Mnemonic::CPX, AddressingMode::Absolute, 3, 4),
    // CPY - Compare Y Register
    OperationCode::new(0xC0, Mnemonic::CPY, AddressingMode::Immediate, 2, 2),
    OperationCode::new(0xC4, Mnemonic::CPY, AddressingMode::ZeroPage, 2, 3),
    OperationCode::new(0xCC, Mnemonic::CPY, AddressingMode::Absolute, 3, 4),
    // DEC - Decrement Memory
    OperationCode::new(0xC6, Mnemonic::DEC, AddressingMode::ZeroPage, 2, 5),
    OperationCode::new(0xD6, Mnemonic::DEC, AddressingMode::ZeroPageX, 2, 6),
    OperationCode::new(0xCE, Mnemonic::DEC, AddressingMode::Absolute, 3, 6),
    OperationCode::new(0xDE, Mnemonic::DEC, AddressingMode::AbsoluteX, 3, 7),
    // DEX - Decrement X Register
    OperationCode::new(0xCA, Mnemonic::DEX, AddressingMode::Implicit, 1, 2),
    // DEY - Decrement Y Register
    OperationCode::new(0x88, Mnemonic::DEY, AddressingMode::Implicit, 1, 2),
    // EOR - Exclusive OR
    OperationCode::new(0x49, Mnemonic::EOR, AddressingMode::Immediate, 2, 2),
    OperationCode::new(0x45, Mnemonic::EOR, AddressingMode::ZeroPage, 2, 3),
    OperationCode::new(0x55, Mnemonic::EOR, AddressingMode::ZeroPageX, 2, 4),
    OperationCode::new(0x4D, Mnemonic::EOR, AddressingMode::Absolute, 3, 4),
    OperationCode::new(0x5D, Mnemonic::EOR, AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
    OperationCode::new(0x59, Mnemonic::EOR, AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),
    OperationCode::new(0x41, Mnemonic::EOR, AddressingMode::IndirectX, 2, 6),
    OperationCode::new(0x51, Mnemonic::EOR, AddressingMode::IndirectY, 2, 5 /* (+1 if page crossed) */),
    // INC - Increment Memory
    OperationCode::new(0xE6, Mnemonic::INC, AddressingMode::ZeroPage, 2, 5),
    OperationCode::new(0xF6, Mnemonic::INC, AddressingMode::ZeroPageX, 2, 6),
    OperationCode::new(0xEE, Mnemonic::INC, AddressingMode::Absolute, 3, 6),
    OperationCode::new(0xFE, Mnemonic::INC, AddressingMode::AbsoluteX, 3, 7),
    // INX - Increment X Register
    OperationCode::new(0xE8, Mnemonic::INX, AddressingMode::Implicit, 1, 2),
    // INY - Increment Y Register
    OperationCode::new(0xC8, Mnemonic::INY, AddressingMode::Implicit, 1, 2),
    // JMP - Jump
    OperationCode::new(0x4C, Mnemonic::JMP, AddressingMode::Absolute, 3, 3),
    OperationCode::new(0x6C, Mnemonic::JMP, AddressingMode::Indirect, 3, 5),
    // JSR - Jump to Subroutine
    OperationCode::new(0x20, Mnemonic::JSR, AddressingMode::Absolute, 3, 6),
    // LDA - Load Accumulator
    OperationCode::new(0xA9, Mnemonic::LDA, AddressingMode::Immediate, 2, 2),
    OperationCode::new(0xA5, Mnemonic::LDA, AddressingMode::ZeroPage, 2, 3),
    OperationCode::new(0xB5, Mnemonic::LDA, AddressingMode::ZeroPageX, 2, 4),
    OperationCode::new(0xAD, Mnemonic::LDA, AddressingMode::Absolute, 3, 4),
    OperationCode::new(0xBD, Mnemonic::LDA, AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
    OperationCode::new(0xB9, Mnemonic::LDA, AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),
    OperationCode::new(0xA1, Mnemonic::LDA, AddressingMode::IndirectX, 2, 6),
    OperationCode::new(0xB1, Mnemonic::LDA, AddressingMode::IndirectY, 2, 5 /* (+1 if page crossed) */),
    // LDX - Load X Register
    OperationCode::new(0xA2, Mnemonic::LDX, AddressingMode::Immediate, 2, 2),
    OperationCode::new(0xA6, Mnemonic::LDX, AddressingMode::ZeroPage, 2, 3),
    OperationCode::new(0xB6, Mnemonic::LDX, AddressingMode::ZeroPageY, 2, 4),
    OperationCode::new(0xAE, Mnemonic::LDX, AddressingMode::Absolute, 3, 4),
    OperationCode::new(0xBE, Mnemonic::LDX, AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),
    // LDY - Load Y Register
    OperationCode::new(0xA0, Mnemonic::LDY, AddressingMode::Immediate, 2, 2),
    OperationCode::new(0xA4, Mnemonic::LDY, AddressingMode::ZeroPage, 2, 3),
    OperationCode::new(0xB4, Mnemonic::LDY, AddressingMode::ZeroPageX, 2, 4),
    OperationCode::new(0xAC, Mnemonic::LDY, AddressingMode::Absolute, 3, 4),
    OperationCode::new(0xBC, Mnemonic::LDY, AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
    // LSR - Logical Shift Right
    OperationCode::new(0x4A, Mnemonic::LSR, AddressingMode::Accumulator, 1, 2),
    OperationCode::new(0x46, Mnemonic::LSR, AddressingMode::ZeroPage, 2, 5),
    OperationCode::new(0x56, Mnemonic::LSR, AddressingMode::ZeroPageX, 2, 6),
    OperationCode::new(0x4E, Mnemonic::LSR, AddressingMode::Absolute, 3, 6),
    OperationCode::new(0x5E, Mnemonic::LSR, AddressingMode::AbsoluteX, 3, 7),
    // NOP - No Operation
    OperationCode::new(0xEA, Mnemonic::NOP, AddressingMode::Implicit, 1, 2),
    // ORA - Logical Inclusive OR
    OperationCode::new(0x09, Mnemonic::ORA, AddressingMode::Immediate, 2, 2),
    OperationCode::new(0x05, Mnemonic::ORA, AddressingMode::ZeroPage, 2, 3),
    OperationCode::new(0x15, Mnemonic::ORA, AddressingMode::ZeroPageX, 2, 4),
    OperationCode::new(0x0D, Mnemonic::ORA, AddressingMode::Absolute, 3, 4),
    OperationCode::new(0x1D, Mnemonic::ORA, AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
    OperationCode::new(0x19, Mnemonic::ORA, AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),
    OperationCode::new(0x01, Mnemonic::ORA, AddressingMode::IndirectX, 2, 6),
    OperationCode::new(0x11, Mnemonic::ORA, AddressingMode::IndirectY, 2, 5 /* (+1 if page crossed) */),
    // PHA - Push Accumulator
    OperationCode::new(0x48, Mnemonic::PHA, AddressingMode::Implicit, 1, 3),
    // PHP - Push Processor Status
    OperationCode::new(0x08, Mnemonic::PHP, AddressingMode::Implicit, 1, 3),
    // PLA - Pull Accumulator
    OperationCode::new(0x68, Mnemonic::PLA, AddressingMode::Implicit, 1, 4),
    // PLP - Pull Processor Status
    OperationCode::new(0x28, Mnemonic::PLP, AddressingMode::Implicit, 1, 4),
    // ROL - Rotate Left
    OperationCode::new(0x2A, Mnemonic::ROL, AddressingMode::Accumulator, 1, 2),
    OperationCode::new(0x26, Mnemonic::ROL, AddressingMode::ZeroPage, 2, 5),
    OperationCode::new(0x36, Mnemonic::ROL, AddressingMode::ZeroPageX, 2, 6),
    OperationCode::new(0x2E, Mnemonic::ROL, AddressingMode::Absolute, 3, 6),
    OperationCode::new(0x3E, Mnemonic::ROL, AddressingMode::AbsoluteX, 3, 7),
    // ROR - Rotate Right
    OperationCode::new(0x6A, Mnemonic::ROR, AddressingMode::Accumulator, 1, 2),
    OperationCode::new(0x66, Mnemonic::ROR, AddressingMode::ZeroPage, 2, 5),
    OperationCode::new(0x76, Mnemonic::ROR, AddressingMode::ZeroPageX, 2, 6),
    OperationCode::new(0x6E, Mnemonic::ROR, AddressingMode::Absolute, 3, 6),
    OperationCode::new(0x7E, Mnemonic::ROR, AddressingMode::AbsoluteX, 3, 7),
    // RTI - Return from Interrupt
    OperationCode::new(0x40, Mnemonic::RTI, AddressingMode::Implicit, 1, 6),
    // RTS - Return from Subroutine
    OperationCode::new(0x60, Mnemonic::RTS, AddressingMode::Implicit, 1, 6),
    // SBC - Subtract with Carry
    OperationCode::new(0xE9, Mnemonic::SBC, AddressingMode::Immediate, 2, 2),
    OperationCode::new(0xE5, Mnemonic::SBC, AddressingMode::ZeroPage, 2, 3),
    OperationCode::new(0xF5, Mnemonic::SBC, AddressingMode::ZeroPageX, 2, 4),
    OperationCode::new(0xED, Mnemonic::SBC, AddressingMode::Absolute, 3, 4),
    OperationCode::new(0xFD, Mnemonic::SBC, AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
    OperationCode::new(0xF9, Mnemonic::SBC, AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),
    OperationCode::new(0xE1, Mnemonic::SBC, AddressingMode::IndirectX, 2, 6),
    OperationCode::new(0xF1, Mnemonic::SBC, AddressingMode::IndirectY, 2, 5 /* (+1 if page crossed) */),
    // SEC - Set Carry Flag
    OperationCode::new(0x38, Mnemonic::SEC, AddressingMode::Implicit, 1, 2),
    // SED - Set Decimal Flag
    OperationCode::new(0xF8, Mnemonic::SED, AddressingMode::Implicit, 1, 2),
    // SEI - Set Interrupt Disable
    OperationCode::new(0x78, Mnemonic::SEI, AddressingMode::Implicit, 1, 2),
    // STA - Store Accumulator
    OperationCode::new(0x85, Mnemonic::STA, AddressingMode::ZeroPage, 2, 3),
    OperationCode::new(0x95, Mnemonic::STA, AddressingMode::ZeroPageX, 2, 4),
    OperationCode::new(0x8D, Mnemonic::STA, AddressingMode::Absolute, 3, 4),
    OperationCode::new(0x9D, Mnemonic::STA, AddressingMode::AbsoluteX, 3, 5),
    OperationCode::new(0x99, Mnemonic::STA, AddressingMode::AbsoluteY, 3, 5),
    OperationCode::new(0x81, Mnemonic::STA, AddressingMode::IndirectX, 2, 6),
    OperationCode::new(0x91, Mnemonic::STA, AddressingMode::IndirectY, 2, 6),
    // STX - Store X Register
    OperationCode::new(0x86, Mnemonic::STX, AddressingMode::ZeroPage, 2, 3),
    OperationCode::new(0x96, Mnemonic::STX, AddressingMode::ZeroPageY, 2, 4),
    OperationCode::new(0x8E, Mnemonic::STX, AddressingMode::Absolute, 3, 4),
    // STY - Store Y Register
    OperationCode::new(0x84, Mnemonic::STY, AddressingMode::ZeroPage, 2, 3),
    OperationCode::new(0x94, Mnemonic::STY, AddressingMode::ZeroPageX, 2, 4),
    OperationCode::new(0x8C, Mnemonic::STY, AddressingMode::Absolute, 3, 4),
    // TAX - Transfer Accumulator to X
    OperationCode::new(0xAA, Mnemonic::TAX, AddressingMode::Implicit, 1, 2),
    // TAY - Transfer Accumulator to Y
    OperationCode::new(0xA8, Mnemonic::TAY, AddressingMode::Implicit, 1, 2),
    // TSX - Transfer Stack Pointer to X
    OperationCode::new(0xBA, Mnemonic::TSX, AddressingMode::Implicit, 1, 2),
    // TXA - Transfer X to Accumulator
    OperationCode::new(0x8A, Mnemonic::TXA, AddressingMode::Implicit, 1, 2),
    // TXS - Transfer X to Stack Pointer
    OperationCode::new(0x9A, Mnemonic::TXS, AddressingMode::Implicit, 1, 2),
    // TYA - Transfer Y to Accumulator
    OperationCode::new(0x98, Mnemonic::TYA, AddressingMode::Implicit, 1, 2),
    // Unofficial opcodes (https://www.nesdev.org/wiki/CPU_unofficial_opcodes)

    // NOP - implied
    OperationCode::unofficial(0x1A, Mnemonic::NOP, AddressingMode::Implicit, 1, 2),
    OperationCode::unofficial(0x3A, Mnemonic::NOP, AddressingMode::Implicit, 1, 2),
    OperationCode::unofficial(0x5A, Mnemonic::NOP, AddressingMode::Implicit, 1, 2),
    OperationCode::unofficial(0x7A, Mnemonic::NOP, AddressingMode::Implicit, 1, 2),
    OperationCode::unofficial(0xDA, Mnemonic::NOP, AddressingMode::Implicit, 1, 2),
    OperationCode::unofficial(0xFA, Mnemonic::NOP, AddressingMode::Implicit, 1, 2),
    // DOP/TOP - NOP that reads (and discards) an operand
    OperationCode::unofficial(0x80, Mnemonic::NOP, AddressingMode::Immediate, 2, 2),
    OperationCode::unofficial(0x82, Mnemonic::NOP, AddressingMode::Immediate, 2, 2),
    OperationCode::unofficial(0x89, Mnemonic::NOP, AddressingMode::Immediate, 2, 2),
    OperationCode::unofficial(0xC2, Mnemonic::NOP, AddressingMode::Immediate, 2, 2),
    OperationCode::unofficial(0xE2, Mnemonic::NOP, AddressingMode::Immediate, 2, 2),
    OperationCode::unofficial(0x04, Mnemonic::NOP, AddressingMode::ZeroPage, 2, 3),
    OperationCode::unofficial(0x44, Mnemonic::NOP, AddressingMode::ZeroPage, 2, 3),
    OperationCode::unofficial(0x64, Mnemonic::NOP, AddressingMode::ZeroPage, 2, 3),
    OperationCode::unofficial(0x14, Mnemonic::NOP, AddressingMode::ZeroPageX, 2, 4),
    OperationCode::unofficial(0x34, Mnemonic::NOP, AddressingMode::ZeroPageX, 2, 4),
    OperationCode::unofficial(0x54, Mnemonic::NOP, AddressingMode::ZeroPageX, 2, 4),
    OperationCode::unofficial(0x74, Mnemonic::NOP, AddressingMode::ZeroPageX, 2, 4),
    OperationCode::unofficial(0xD4, Mnemonic::NOP, AddressingMode::ZeroPageX, 2, 4),
    OperationCode::unofficial(0xF4, Mnemonic::NOP, AddressingMode::ZeroPageX, 2, 4),
    OperationCode::unofficial(0x0C, Mnemonic::NOP, AddressingMode::Absolute, 3, 4),
    OperationCode::unofficial(0x1C, Mnemonic::NOP, AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
    OperationCode::unofficial(0x3C, Mnemonic::NOP, AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
    OperationCode::unofficial(0x5C, Mnemonic::NOP, AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
    OperationCode::unofficial(0x7C, Mnemonic::NOP, AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
    OperationCode::unofficial(0xDC, Mnemonic::NOP, AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
    OperationCode::unofficial(0xFC, Mnemonic::NOP, AddressingMode::AbsoluteX, 3, 4 /* (+1 if page crossed) */),
    // SBC - duplicate of $E9
    OperationCode::unofficial(0xEB, Mnemonic::SBC, AddressingMode::Immediate, 2, 2),
    // LAX - LDA + LDX
    OperationCode::unofficial(0xA7, Mnemonic::LAX, AddressingMode::ZeroPage, 2, 3),
    OperationCode::unofficial(0xB7, Mnemonic::LAX, AddressingMode::ZeroPageY, 2, 4),
    OperationCode::unofficial(0xAF, Mnemonic::LAX, AddressingMode::Absolute, 3, 4),
    OperationCode::unofficial(0xBF, Mnemonic::LAX, AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),
    OperationCode::unofficial(0xA3, Mnemonic::LAX, AddressingMode::IndirectX, 2, 6),
    OperationCode::unofficial(0xB3, Mnemonic::LAX, AddressingMode::IndirectY, 2, 5 /* (+1 if page crossed) */),
    // SAX - store A & X
    OperationCode::unofficial(0x87, Mnemonic::SAX, AddressingMode::ZeroPage, 2, 3),
    OperationCode::unofficial(0x97, Mnemonic::SAX, AddressingMode::ZeroPageY, 2, 4),
    OperationCode::unofficial(0x8F, Mnemonic::SAX, AddressingMode::Absolute, 3, 4),
    OperationCode::unofficial(0x83, Mnemonic::SAX, AddressingMode::IndirectX, 2, 6),
    // DCP - DEC + CMP
    OperationCode::unofficial(0xC7, Mnemonic::DCP, AddressingMode::ZeroPage, 2, 5),
    OperationCode::unofficial(0xD7, Mnemonic::DCP, AddressingMode::ZeroPageX, 2, 6),
    OperationCode::unofficial(0xCF, Mnemonic::DCP, AddressingMode::Absolute, 3, 6),
    OperationCode::unofficial(0xDF, Mnemonic::DCP, AddressingMode::AbsoluteX, 3, 7),
    OperationCode::unofficial(0xDB, Mnemonic::DCP, AddressingMode::AbsoluteY, 3, 7),
    OperationCode::unofficial(0xC3, Mnemonic::DCP, AddressingMode::IndirectX, 2, 8),
    OperationCode::unofficial(0xD3, Mnemonic::DCP, AddressingMode::IndirectY, 2, 8),
    // ISB (ISC) - INC + SBC
    OperationCode::unofficial(0xE7, Mnemonic::ISB, AddressingMode::ZeroPage, 2, 5),
    OperationCode::unofficial(0xF7, Mnemonic::ISB, AddressingMode::ZeroPageX, 2, 6),
    OperationCode::unofficial(0xEF, Mnemonic::ISB, AddressingMode::Absolute, 3, 6),
    OperationCode::unofficial(0xFF, Mnemonic::ISB, AddressingMode::AbsoluteX, 3, 7),
    OperationCode::unofficial(0xFB, Mnemonic::ISB, AddressingMode::AbsoluteY, 3, 7),
    OperationCode::unofficial(0xE3, Mnemonic::ISB, AddressingMode::IndirectX, 2, 8),
    OperationCode::unofficial(0xF3, Mnemonic::ISB, AddressingMode::IndirectY, 2, 8),
    // SLO - ASL + ORA
    OperationCode::unofficial(0x07, Mnemonic::SLO, AddressingMode::ZeroPage, 2, 5),
    OperationCode::unofficial(0x17, Mnemonic::SLO, AddressingMode::ZeroPageX, 2, 6),
    OperationCode::unofficial(0x0F, Mnemonic::SLO, AddressingMode::Absolute, 3, 6),
    OperationCode::unofficial(0x1F, Mnemonic::SLO, AddressingMode::AbsoluteX, 3, 7),
    OperationCode::unofficial(0x1B, Mnemonic::SLO, AddressingMode::AbsoluteY, 3, 7),
    OperationCode::unofficial(0x03, Mnemonic::SLO, AddressingMode::IndirectX, 2, 8),
    OperationCode::unofficial(0x13, Mnemonic::SLO, AddressingMode::IndirectY, 2, 8),
    // RLA - ROL + AND
    OperationCode::unofficial(0x27, Mnemonic::RLA, AddressingMode::ZeroPage, 2, 5),
    OperationCode::unofficial(0x37, Mnemonic::RLA, AddressingMode::ZeroPageX, 2, 6),
    OperationCode::unofficial(0x2F, Mnemonic::RLA, AddressingMode::Absolute, 3, 6),
    OperationCode::unofficial(0x3F, Mnemonic::RLA, AddressingMode::AbsoluteX, 3, 7),
    OperationCode::unofficial(0x3B, Mnemonic::RLA, AddressingMode::AbsoluteY, 3, 7),
    OperationCode::unofficial(0x23, Mnemonic::RLA, AddressingMode::IndirectX, 2, 8),
    OperationCode::unofficial(0x33, Mnemonic::RLA, AddressingMode::IndirectY, 2, 8),
    // SRE - LSR + EOR
    OperationCode::unofficial(0x47, Mnemonic::SRE, AddressingMode::ZeroPage, 2, 5),
    OperationCode::unofficial(0x57, Mnemonic::SRE, AddressingMode::ZeroPageX, 2, 6),
    OperationCode::unofficial(0x4F, Mnemonic::SRE, AddressingMode::Absolute, 3, 6),
    OperationCode::unofficial(0x5F, Mnemonic::SRE, AddressingMode::AbsoluteX, 3, 7),
    OperationCode::unofficial(0x5B, Mnemonic::SRE, AddressingMode::AbsoluteY, 3, 7),
    OperationCode::unofficial(0x43, Mnemonic::SRE, AddressingMode::IndirectX, 2, 8),
    OperationCode::unofficial(0x53, Mnemonic::SRE, AddressingMode::IndirectY, 2, 8),
    // RRA - ROR + ADC
    OperationCode::unofficial(0x67, Mnemonic::RRA, AddressingMode::ZeroPage, 2, 5),
    OperationCode::unofficial(0x77, Mnemonic::RRA, AddressingMode::ZeroPageX, 2, 6),
    OperationCode::unofficial(0x6F, Mnemonic::RRA, AddressingMode::Absolute, 3, 6),
    OperationCode::unofficial(0x7F, Mnemonic::RRA, AddressingMode::AbsoluteX, 3, 7),
    OperationCode::unofficial(0x7B, Mnemonic::RRA, AddressingMode::AbsoluteY, 3, 7),
    OperationCode::unofficial(0x63, Mnemonic::RRA, AddressingMode::IndirectX, 2, 8),
    OperationCode::unofficial(0x73, Mnemonic::RRA, AddressingMode::IndirectY, 2, 8),
    // ANC - AND #imm, C = N
    OperationCode::unofficial(0x0B, Mnemonic::ANC, AddressingMode::Immediate, 2, 2),
    OperationCode::unofficial(0x2B, Mnemonic::ANC, AddressingMode::Immediate, 2, 2),
    // ALR (ASR) - AND #imm, LSR A
    OperationCode::unofficial(0x4B, Mnemonic::ALR, AddressingMode::Immediate, 2, 2),
    // ARR - AND #imm, ROR A (special C/V)
    OperationCode::unofficial(0x6B, Mnemonic::ARR, AddressingMode::Immediate, 2, 2),
    // AXS (SBX) - X = (A & X) - #imm
    OperationCode::unofficial(0xCB, Mnemonic::AXS, AddressingMode::Immediate, 2, 2),
    // Unstable on real hardware; implemented with the behaviour the common test ROMs expect.
    // XAA - A = X & #imm
    OperationCode::unofficial(0x8B, Mnemonic::XAA, AddressingMode::Immediate, 2, 2),
    // LXA - A = X = #imm
    OperationCode::unofficial(0xAB, Mnemonic::LXA, AddressingMode::Immediate, 2, 2),
    // SHA (AHX) - store A & X & (high byte of the base address + 1)
    OperationCode::unofficial(0x93, Mnemonic::SHA, AddressingMode::IndirectY, 2, 6),
    OperationCode::unofficial(0x9F, Mnemonic::SHA, AddressingMode::AbsoluteY, 3, 5),
    // SHY - store Y & (H + 1);  SHX - store X & (H + 1)
    OperationCode::unofficial(0x9C, Mnemonic::SHY, AddressingMode::AbsoluteX, 3, 5),
    OperationCode::unofficial(0x9E, Mnemonic::SHX, AddressingMode::AbsoluteY, 3, 5),
    // TAS (XAS) - SP = A & X, then store SP & (H + 1)
    OperationCode::unofficial(0x9B, Mnemonic::TAS, AddressingMode::AbsoluteY, 3, 5),
    // LAS - A = X = SP = M & SP
    OperationCode::unofficial(0xBB, Mnemonic::LAS, AddressingMode::AbsoluteY, 3, 4 /* (+1 if page crossed) */),
];
