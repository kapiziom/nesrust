mod addressing;
mod operation_codes;
mod flags;
mod instructions;
mod interrupts;
mod stack;

use crate::emulator::bus::cpu_bus::CpuBus;
pub use operation_codes::*;
use addressing::{AddressingMode, Access};
use crate::emulator::cpu::flags::CpuFlags;

pub struct CPU<B: CpuBus> {
    pub (super) register_a: u8,
    pub (super) register_x: u8,
    pub (super) register_y: u8,
    pub (super) stack_pointer: u8,
    pub program_counter: u16,
    pub (super) flags: flags::CpuFlags,
    pub bus: B,
    /// Total CPU cycles executed since creation (for tracing/test comparison).
    pub cycles: u64,

    // Interrupt lines are sampled at the end of every cycle; an instruction only reacts to what was seen at the end
    // of its second-to-last cycle (hence the "ready" copies, one sample behind).
    /// An NMI edge was seen and not yet serviced.
    nmi_latched: bool,
    nmi_ready: bool,
    /// IRQ line was low and the I flag clear at the last sample.
    irq_sampled: bool,
    irq_ready: bool,
}

/// Snapshot of the programmer-visible CPU registers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuState {
    pub pc: u16,
    pub a: u8,
    pub x: u8,
    pub y: u8,
    /// Status register as stored (B is never set: it only exists in the copy pushed to the stack).
    pub p: u8,
    pub sp: u8,
}

impl<B: CpuBus> CPU<B> {
    /// Creates a CPU in the register state a RESET leaves behind (use `reset()` to also load PC from the vector).
    pub fn new(bus: B) -> CPU<B> {
        CPU {
            register_a: 0,
            register_x: 0,
            register_y: 0,
            stack_pointer: 0xfd,
            program_counter: 0,
            flags: CpuFlags::INTERRUPT_DISABLE | CpuFlags::UNUSED,
            bus,
            cycles: 0,
            nmi_latched: false,
            nmi_ready: false,
            irq_sampled: false,
            irq_ready: false,
        }
    }

    pub fn state(&self) -> CpuState {
        CpuState {
            pc: self.program_counter,
            a: self.register_a,
            x: self.register_x,
            y: self.register_y,
            p: self.flags.bits(),
            sp: self.stack_pointer,
        }
    }

    pub fn set_state(&mut self, state: CpuState) {
        self.program_counter = state.pc;
        self.register_a = state.a;
        self.register_x = state.x;
        self.register_y = state.y;
        self.flags = CpuFlags::from_bits_truncate(state.p);
        self.stack_pointer = state.sp;
    }

    /// Services a pending interrupt or executes a single instruction.
    /// Returns false when execution cannot continue (unknown opcode).
    pub fn step(&mut self) -> bool {
        if self.poll_interrupts() {
            return true;
        }

        self.execute_next()
    }

    /// Runs until `program_end`, an unknown opcode, or a BRK (BRK is executed, then used as the end-of-program marker).
    pub fn interpret(&mut self, program_end: usize) {
        while (self.program_counter as usize) < program_end {
            if self.poll_interrupts() {
                continue;
            }

            let operation_code = self.peek(self.program_counter);

            log::trace!("pc: {:#06X} op_code: {:#04X}", self.program_counter, operation_code);

            if !self.execute_next() || operation_code == 0x00 {
                log::debug!("program end");
                break
            }
        }
    }

    // test with skip_brk to prevent read 0x00 (and pc++) in single instruction test
    pub fn interpret_for_test(&mut self, program_end: usize, skip_brk: bool) {
        while (self.program_counter as usize) <= program_end {
            if self.poll_interrupts() {
                continue;
            }

            let operation_code = self.peek(self.program_counter);

            if operation_code == 0x00 && skip_brk {
                break;
            }

            if !self.execute_next() || operation_code == 0x00 {
                break;
            }
        }
    }

    /// Fetches and executes one instruction (no interrupt polling).
    fn execute_next(&mut self) -> bool {
        let operation_code = self.fetch();

        let Some(op) = CPU_OPERATION_TABLE[operation_code as usize] else {
            log::warn!("unknown operation code {:#04X}", operation_code);
            return false;
        };

        self.execute(&op);

        if let Some(page) = self.bus.take_oam_dma() {
            self.run_oam_dma(page);
        }

        true
    }

    /// $4014 OAM DMA: 256 read/write pairs stall the CPU for 513 cycles, 514 when it starts on an odd cycle.
    /// https://www.nesdev.org/wiki/DMA#OAM_DMA
    fn run_oam_dma(&mut self, page: u8) {
        // The decision whether an interrupt is taken after the STA that started the DMA was made before the DMA,
        // so interrupts that arrive during it wait for the next instruction.
        let seen_before_dma = self.interrupt_lines_seen();
        let odd_cycle = self.cycles & 1 == 0;

        self.tick(); // halt cycle
        if odd_cycle {
            self.tick(); // alignment cycle
        }

        for offset in 0..=0xFFu16 {
            let value = self.read((page as u16) << 8 | offset);
            self.write(0x2004, value);
        }

        self.restore_interrupt_lines_seen(seen_before_dma);
    }

    fn execute(&mut self, op: &OperationCode) {
        let mode = op.addressing_mode;

        match op.mnemonic {
            // read-type instructions
            Mnemonic::ADC => { let value = self.read_operand(mode); self.adc(value) }
            Mnemonic::AND => { let value = self.read_operand(mode); self.and(value) }
            Mnemonic::BIT => { let value = self.read_operand(mode); self.bit(value) }
            Mnemonic::CMP => { let value = self.read_operand(mode); self.compare(self.register_a, value) }
            Mnemonic::CPX => { let value = self.read_operand(mode); self.compare(self.register_x, value) }
            Mnemonic::CPY => { let value = self.read_operand(mode); self.compare(self.register_y, value) }
            Mnemonic::EOR => { let value = self.read_operand(mode); self.eor(value) }
            Mnemonic::LDA => { let value = self.read_operand(mode); self.set_register_a(value) }
            Mnemonic::LDX => { let value = self.read_operand(mode); self.set_register_x(value) }
            Mnemonic::LDY => { let value = self.read_operand(mode); self.set_register_y(value) }
            Mnemonic::ORA => { let value = self.read_operand(mode); self.ora(value) }
            Mnemonic::SBC => { let value = self.read_operand(mode); self.sbc(value) }
            Mnemonic::NOP => {
                if mode == AddressingMode::Implicit {
                    self.implied();
                } else {
                    // unofficial NOP variants still perform the operand read
                    self.read_operand(mode);
                }
            }

            // stores
            Mnemonic::STA => self.store(mode, self.register_a),
            Mnemonic::STX => self.store(mode, self.register_x),
            Mnemonic::STY => self.store(mode, self.register_y),

            // read-modify-write
            Mnemonic::ASL => self.modify(mode, |cpu, value| cpu.asl(value)),
            Mnemonic::LSR => self.modify(mode, |cpu, value| cpu.lsr(value)),
            Mnemonic::ROL => self.modify(mode, |cpu, value| cpu.rol(value)),
            Mnemonic::ROR => self.modify(mode, |cpu, value| cpu.ror(value)),
            Mnemonic::INC => self.modify(mode, |cpu, value| cpu.inc(value)),
            Mnemonic::DEC => self.modify(mode, |cpu, value| cpu.dec(value)),

            // branches
            Mnemonic::BCC => { let taken = !self.contains_flag(CpuFlags::CARRY); self.branch(taken) }
            Mnemonic::BCS => { let taken = self.contains_flag(CpuFlags::CARRY); self.branch(taken) }
            Mnemonic::BEQ => { let taken = self.contains_flag(CpuFlags::ZERO); self.branch(taken) }
            Mnemonic::BMI => { let taken = self.contains_flag(CpuFlags::NEGATIVE); self.branch(taken) }
            Mnemonic::BNE => { let taken = !self.contains_flag(CpuFlags::ZERO); self.branch(taken) }
            Mnemonic::BPL => { let taken = !self.contains_flag(CpuFlags::NEGATIVE); self.branch(taken) }
            Mnemonic::BVC => { let taken = !self.contains_flag(CpuFlags::OVERFLOW); self.branch(taken) }
            Mnemonic::BVS => { let taken = self.contains_flag(CpuFlags::OVERFLOW); self.branch(taken) }

            // jumps, subroutines, interrupts
            Mnemonic::JMP => self.program_counter = self.effective_address(mode, Access::Read),
            Mnemonic::JSR => self.jsr(),
            Mnemonic::RTS => self.rts(),
            Mnemonic::RTI => self.rti(),
            Mnemonic::BRK => {
                // BRK has a padding byte: the pushed return address is opcode + 2
                self.fetch();
                self.enter_interrupt(interrupts::BRK);
            }

            // stack
            Mnemonic::PHA => self.pha(),
            Mnemonic::PHP => self.php(),
            Mnemonic::PLA => self.pla(),
            Mnemonic::PLP => self.plp(),

            // flags
            Mnemonic::CLC => { self.implied(); self.clear_flag(CpuFlags::CARRY) }
            Mnemonic::CLD => { self.implied(); self.clear_flag(CpuFlags::DECIMAL_MODE) }
            Mnemonic::CLI => { self.implied(); self.clear_flag(CpuFlags::INTERRUPT_DISABLE) }
            Mnemonic::CLV => { self.implied(); self.clear_flag(CpuFlags::OVERFLOW) }
            Mnemonic::SEC => { self.implied(); self.insert_flag(CpuFlags::CARRY) }
            Mnemonic::SED => { self.implied(); self.insert_flag(CpuFlags::DECIMAL_MODE) }
            Mnemonic::SEI => { self.implied(); self.insert_flag(CpuFlags::INTERRUPT_DISABLE) }

            // register operations
            Mnemonic::DEX => { self.implied(); self.set_register_x(self.register_x.wrapping_sub(1)) }
            Mnemonic::DEY => { self.implied(); self.set_register_y(self.register_y.wrapping_sub(1)) }
            Mnemonic::INX => { self.implied(); self.set_register_x(self.register_x.wrapping_add(1)) }
            Mnemonic::INY => { self.implied(); self.set_register_y(self.register_y.wrapping_add(1)) }
            Mnemonic::TAX => { self.implied(); self.set_register_x(self.register_a) }
            Mnemonic::TAY => { self.implied(); self.set_register_y(self.register_a) }
            Mnemonic::TSX => { self.implied(); self.set_register_x(self.stack_pointer) }
            Mnemonic::TXA => { self.implied(); self.set_register_a(self.register_x) }
            Mnemonic::TXS => { self.implied(); self.stack_pointer = self.register_x }
            Mnemonic::TYA => { self.implied(); self.set_register_a(self.register_y) }

            // unofficial opcodes
            Mnemonic::LAX => {
                let value = self.read_operand(mode);
                self.register_a = value;
                self.set_register_x(value);
            }
            Mnemonic::SAX => self.store(mode, self.register_a & self.register_x),
            Mnemonic::DCP => self.modify(mode, |cpu, value| {
                let result = cpu.dec(value);
                cpu.compare(cpu.register_a, result);
                result
            }),
            Mnemonic::ISB => self.modify(mode, |cpu, value| {
                let result = cpu.inc(value);
                cpu.sbc(result);
                result
            }),
            Mnemonic::SLO => self.modify(mode, |cpu, value| {
                let result = cpu.asl(value);
                cpu.ora(result);
                result
            }),
            Mnemonic::RLA => self.modify(mode, |cpu, value| {
                let result = cpu.rol(value);
                cpu.and(result);
                result
            }),
            Mnemonic::SRE => self.modify(mode, |cpu, value| {
                let result = cpu.lsr(value);
                cpu.eor(result);
                result
            }),
            Mnemonic::RRA => self.modify(mode, |cpu, value| {
                let result = cpu.ror(value); // its carry-out is the ADC's carry-in
                cpu.adc(result);
                result
            }),
            Mnemonic::XAA => { let value = self.read_operand(mode); self.set_register_a(self.register_x & value) }
            Mnemonic::LXA => {
                let value = self.read_operand(mode);
                self.register_a = value;
                self.set_register_x(value);
            }
            Mnemonic::LAS => {
                let value = self.read_operand(mode);
                let result = value & self.stack_pointer;
                self.register_a = result;
                self.stack_pointer = result;
                self.set_register_x(result);
            }
            Mnemonic::SHY => self.store_masked_by_high_byte(mode, self.register_y),
            Mnemonic::SHX => self.store_masked_by_high_byte(mode, self.register_x),
            Mnemonic::SHA => self.store_masked_by_high_byte(mode, self.register_a & self.register_x),
            Mnemonic::TAS => {
                self.stack_pointer = self.register_a & self.register_x;
                self.store_masked_by_high_byte(mode, self.stack_pointer);
            }
            Mnemonic::ANC => { let value = self.read_operand(mode); self.anc(value) }
            Mnemonic::ALR => { let value = self.read_operand(mode); self.alr(value) }
            Mnemonic::ARR => { let value = self.read_operand(mode); self.arr(value) }
            Mnemonic::AXS => { let value = self.read_operand(mode); self.axs(value) }
        }
    }

    /// One CPU cycle in which the bus is not used (the bus and everything behind it still advances).
    pub (super) fn tick(&mut self) {
        self.cycles += 1;
        self.bus.tick(1);
        self.end_cycle();
    }

    /// Bus read; costs one CPU cycle. If the DMC is waiting for a sample byte the CPU is halted first.
    pub (super) fn read(&mut self, address: u16) -> u8 {
        if let Some(sample_address) = self.bus.take_dmc_dma() {
            self.run_dmc_dma(address, sample_address);
        }

        self.read_cycle(address)
    }

    fn read_cycle(&mut self, address: u16) -> u8 {
        self.cycles += 1;
        self.bus.tick_before_access(false);
        let value = self.bus.read(address);
        self.bus.tick_after_access(false);
        self.end_cycle();
        value
    }

    /// Bus write; costs one CPU cycle.
    pub (super) fn write(&mut self, address: u16, data: u8) {
        self.cycles += 1;
        self.bus.tick_before_access(true);
        self.bus.write(address, data);
        self.bus.tick_after_access(true);
        self.end_cycle();
    }

    /// The DMC fetches a sample byte: the CPU is halted on its read, which it repeats (with the side effects of
    /// reading e.g. $4016 or $2002), then idles one more cycle for alignment before the sample read. 4 cycles.
    /// https://www.nesdev.org/wiki/DMA#DMC_DMA
    fn run_dmc_dma(&mut self, cpu_address: u16, sample_address: u16) {
        // The halted CPU keeps re-reading its address, which repeats the side effects of $2002/$2007 reads. Reads of
        // the controller ports and $4015 are one continuous access, so only the first of them counts.
        let repeats_are_visible = !(0x4015..=0x4017).contains(&cpu_address);
        let repeat = |cpu: &mut Self| {
            if repeats_are_visible {
                cpu.read_cycle(cpu_address);
            } else {
                cpu.tick();
            }
        };

        self.read_cycle(cpu_address); // halt cycle
        repeat(self); // dummy cycle
        if self.cycles & 1 == 0 {
            repeat(self); // alignment: the sample is read on a "get" cycle
        }
        let byte = self.read_cycle(sample_address);
        self.bus.dmc_dma_done(byte);
    }

    /// The NMI and IRQ requests as seen at the end of the previous cycle (what the next poll would act on).
    pub (super) fn interrupt_lines_seen(&self) -> (bool, bool) {
        (self.nmi_ready, self.irq_ready)
    }

    pub (super) fn restore_interrupt_lines_seen(&mut self, seen: (bool, bool)) {
        (self.nmi_ready, self.irq_ready) = seen;
    }

    /// Samples the interrupt lines at the end of a cycle.
    fn end_cycle(&mut self) {
        self.nmi_ready = self.nmi_latched;
        self.irq_ready = self.irq_sampled;

        if self.bus.fetch_nmi().is_some() {
            self.nmi_latched = true;
        }
        self.irq_sampled = self.bus.irq_pending() && !self.flags.contains(CpuFlags::INTERRUPT_DISABLE);
    }

    /// Bus read without cycle cost, for tests and debugging.
    pub fn peek(&mut self, address: u16) -> u8 {
        self.bus.read(address)
    }

    /// Bus write without cycle cost, for tests and debugging.
    pub fn poke(&mut self, address: u16, data: u8) {
        self.bus.write(address, data);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::emulator::bus::mock_bus::MockBus;

    fn prepare_test_cpu(program: &[u8]) -> CPU<MockBus> {
        let mut bus = MockBus::new();

        bus.load_program(&program, 0x8000);

        let mut cpu = CPU::new(bus);

        cpu.program_counter = 0x8000;

        cpu
    }

    fn get_program_end(pc: u16, program_len: usize) -> usize {
        (pc + program_len as u16) as usize
    }

    #[test]
    fn test_program_execution() {
        let program = [0xA9, 0x05, 0x85, 0x00, 0x00]; // LDA #$05, STA $00, BRK
        let mut cpu = prepare_test_cpu(&program);

        cpu.interpret(get_program_end(cpu.program_counter, program.len()));

        assert_eq!(cpu.register_a, 0x05);
        assert_eq!(cpu.peek(0x00), 0x05);
    }

    #[test]
    fn test_brk_interrupt() {
        let program = vec![0x00];
        let mut cpu = prepare_test_cpu(&program);

        let program_end = get_program_end(cpu.program_counter, program.len());

        cpu.clear_flag(CpuFlags::INTERRUPT_DISABLE);

        cpu.poke(0xFFFE, 0x34); // lo
        cpu.poke(0xFFFF, 0x12); // hi

        cpu.interpret(program_end);

        assert_eq!(cpu.program_counter, 0x1234);

        assert!(cpu.contains_flag(CpuFlags::INTERRUPT_DISABLE));
        assert!(!cpu.contains_flag(CpuFlags::BREAK), "B only exists in the pushed copy of the status");

        assert_eq!(cpu.stack_pointer, 0xFA);
        assert_eq!(cpu.cycles, 7);

        // pushed: return address = BRK + 2, status with B and bit 5 set
        let status = cpu.pop_stack();
        assert_eq!(status & 0b0011_0000, 0b0011_0000);
        assert_eq!(cpu.pop_stack(), 0x02);
        assert_eq!(cpu.pop_stack(), 0x80);
    }

    #[test]
    fn test_brk_does_not_stop_step() {
        let program = [0x00, 0xEA];
        let mut cpu = prepare_test_cpu(&program);
        cpu.poke(0xFFFE, 0x00);
        cpu.poke(0xFFFF, 0x90);
        cpu.poke(0x9000, 0xEA); // NOP at the handler

        assert!(cpu.step());
        assert_eq!(cpu.program_counter, 0x9000);

        assert!(cpu.step());
        assert_eq!(cpu.program_counter, 0x9001);
    }

    #[test]
    fn test_initial_status_is_reset_state() {
        let cpu = CPU::new(MockBus::new());

        assert_eq!(cpu.state().p, 0x24);
        assert_eq!(cpu.state().sp, 0xFD);
    }

    fn interrupt_test_cpu(configure: impl FnOnce(&mut MockBus)) -> CPU<MockBus> {
        let mut bus = MockBus::new();
        bus.load_program(&[0xEA, 0xEA], 0x8000); // NOP, NOP
        bus.memory[0xFFFA] = 0x00; // NMI vector -> $9000
        bus.memory[0xFFFB] = 0x90;
        bus.memory[0xFFFE] = 0x00; // IRQ/BRK vector -> $A000
        bus.memory[0xFFFF] = 0xA0;
        configure(&mut bus);

        let mut cpu = CPU::new(bus);
        cpu.program_counter = 0x8000;
        cpu
    }

    #[test]
    fn test_nmi_is_taken_before_the_next_opcode() {
        let mut cpu = interrupt_test_cpu(|bus| bus.nmi_interrupt = Some(1));
        cpu.clear_flag(CpuFlags::INTERRUPT_DISABLE); // NMI ignores the I flag anyway
        cpu.insert_flag(CpuFlags::CARRY);

        // the line is sampled at the end of the NOP's first cycle, so the NOP finishes before the NMI is taken
        assert!(cpu.step());
        assert_eq!(cpu.program_counter, 0x8001);
        assert!(cpu.step());

        assert_eq!(cpu.program_counter, 0x9000, "jumped through the NMI vector");
        assert_eq!(cpu.cycles, 2 + 7);
        assert!(cpu.contains_flag(CpuFlags::INTERRUPT_DISABLE));

        let status = cpu.pop_stack();
        assert_eq!(status & 0b0011_0000, 0b0010_0000, "NMI pushes B=0, bit 5=1");
        assert_ne!(status & CpuFlags::CARRY.bits(), 0);
        assert_eq!(cpu.pop_stack(), 0x01);
        assert_eq!(cpu.pop_stack(), 0x80);
    }

    #[test]
    fn test_nmi_is_edge_triggered() {
        let mut cpu = interrupt_test_cpu(|bus| bus.nmi_interrupt = Some(1));

        cpu.step(); // NOP
        cpu.step(); // NMI
        cpu.poke(0x9000, 0xEA);
        cpu.step(); // NOP in the handler, no second NMI

        assert_eq!(cpu.program_counter, 0x9001);
    }

    #[test]
    fn test_irq_is_masked_by_interrupt_disable() {
        let mut cpu = interrupt_test_cpu(|bus| bus.irq_line = true);
        assert!(cpu.contains_flag(CpuFlags::INTERRUPT_DISABLE));

        cpu.step();

        assert_eq!(cpu.program_counter, 0x8001, "IRQ ignored, NOP executed");
    }

    #[test]
    fn test_irq_is_taken_when_unmasked() {
        let mut cpu = interrupt_test_cpu(|bus| bus.irq_line = true);
        cpu.clear_flag(CpuFlags::INTERRUPT_DISABLE);

        cpu.step(); // NOP, the level is sampled during it
        cpu.step();

        assert_eq!(cpu.program_counter, 0xA000);
        assert_eq!(cpu.cycles, 2 + 7);
        assert!(cpu.contains_flag(CpuFlags::INTERRUPT_DISABLE), "handler runs with IRQs masked");
        assert_eq!(cpu.pop_stack() & 0b0011_0000, 0b0010_0000, "IRQ pushes B=0, bit 5=1");
    }

    #[test]
    fn test_nmi_has_priority_over_irq() {
        let mut cpu = interrupt_test_cpu(|bus| { bus.nmi_interrupt = Some(1); bus.irq_line = true; });
        cpu.clear_flag(CpuFlags::INTERRUPT_DISABLE);

        cpu.step(); // NOP
        cpu.step();

        assert_eq!(cpu.program_counter, 0x9000);
    }

    #[test]
    fn test_cli_delays_irq_by_one_instruction() {
        let mut cpu = interrupt_test_cpu(|bus| {
            bus.irq_line = true;
            bus.load_program(&[0x58, 0xEA, 0xEA], 0x8000); // CLI, NOP, NOP
        });

        cpu.step(); // CLI
        assert_eq!(cpu.program_counter, 0x8001);
        cpu.step(); // the IRQ is still masked in CLI's polling window: the NOP runs
        assert_eq!(cpu.program_counter, 0x8002);
        cpu.step();

        assert_eq!(cpu.program_counter, 0xA000, "IRQ is taken after the instruction following CLI");
        assert_eq!(cpu.pop_stack() & 0b0011_0000, 0b0010_0000);
        assert_eq!(cpu.pop_stack(), 0x02);
    }

    #[test]
    fn test_sei_does_not_stop_an_irq_that_was_already_seen() {
        let mut cpu = interrupt_test_cpu(|bus| {
            bus.irq_line = true;
            bus.load_program(&[0x78, 0xEA], 0x8000); // SEI, NOP
        });
        cpu.clear_flag(CpuFlags::INTERRUPT_DISABLE);

        cpu.step(); // SEI
        assert!(cpu.contains_flag(CpuFlags::INTERRUPT_DISABLE));
        cpu.step();

        assert_eq!(cpu.program_counter, 0xA000, "the IRQ still fires right after SEI");
    }

    #[test]
    fn test_nmi_hijacks_the_vector_of_a_brk() {
        let mut cpu = interrupt_test_cpu(|bus| {
            bus.nmi_interrupt = Some(1);
            bus.load_program(&[0x00, 0x00], 0x8000); // BRK
            bus.memory[0x9000] = 0xEA;
        });

        cpu.step();

        assert_eq!(cpu.program_counter, 0x9000, "NMI vector, not the BRK vector");
        assert_eq!(cpu.pop_stack() & 0b0011_0000, 0b0011_0000, "the pushed status still has B set");
        assert_eq!(cpu.pop_stack(), 0x02);
        assert_eq!(cpu.pop_stack(), 0x80);

        cpu.step();
        assert_eq!(cpu.program_counter, 0x9001, "the NMI was consumed by the hijack");
    }

    #[test]
    fn test_rti_returns_from_nmi_and_restores_flags() {
        let mut cpu = interrupt_test_cpu(|bus| {
            bus.nmi_interrupt = Some(1);
            bus.memory[0x9000] = 0x40; // RTI
        });
        cpu.clear_flag(CpuFlags::INTERRUPT_DISABLE);
        cpu.insert_flag(CpuFlags::CARRY);

        cpu.step(); // NOP
        let before = cpu.state();

        cpu.step(); // NMI
        cpu.step(); // RTI

        assert_eq!(cpu.state(), before);
        assert!(!cpu.contains_flag(CpuFlags::BREAK));
    }

    #[test]
    fn test_adc_immediate() {
        let program = vec![0x69, 0x03]; // 0x69 => ADC Immediate

        let mut cpu = prepare_test_cpu(&program);

        cpu.register_a = 0x05;

        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), false);

        assert_eq!(cpu.register_a, 0x08); // (0x05 + 0x03)
        assert!(!cpu.flags.contains(CpuFlags::ZERO));
        assert!(!cpu.flags.contains(CpuFlags::CARRY));
    }

    #[test]
    fn test_adc_zero_flag() {
        let program = vec![0x69, 0x01]; // 0x69 => ADC Immediate

        let mut cpu = prepare_test_cpu(&program);

        cpu.register_a = 0xFF;

        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), false);

        assert_eq!(cpu.register_a, 0x00); // ADC #0x01 (0xFF + 0x01 = 0x00)
        assert!(cpu.flags.contains(CpuFlags::CARRY));
        assert!(cpu.flags.contains(CpuFlags::ZERO));
        assert!(!cpu.flags.contains(CpuFlags::OVERFLOW));
    }

    #[test]
    fn test_adc_carry_flag() {
        let program = vec![0x69, 0x02]; // 0x69 => ADC Immediate
        let mut cpu = prepare_test_cpu(&program);

        cpu.register_a = 0xFE;

        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), false);

        assert_eq!(cpu.register_a, 0x00); // ADC #0x02 (0xFE + 0x02 = 0x00)
        assert!(cpu.flags.contains(CpuFlags::ZERO));
        assert!(cpu.flags.contains(CpuFlags::CARRY));
    }

    #[test]
    fn test_and_accumulator() {
        let program = vec![0x29, 0xAA]; // AND #$AA
        let mut cpu = prepare_test_cpu(&program);

        cpu.register_a = 0b11001100; // 204
        cpu.poke(0x2000, 0b10101010); // 170

        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), false);

        assert_eq!(cpu.register_a, 0b10001000); // 136
    }

    #[test]
    fn test_and_zero_page() {
        let program = vec![0x25, 0x00]; // AND $00
        let mut cpu = prepare_test_cpu(&program);

        cpu.register_a = 0b11001100; // 204
        cpu.poke(0x00, 0b10101010); // 170

        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), false);

        assert_eq!(cpu.register_a, 0b10001000); // 136
    }

    #[test]
    fn test_asl_accumulator() {
        let program = vec![0x0A]; // ASL
        let mut cpu = prepare_test_cpu(&program);

        cpu.register_a = 0b11001100; // 204

        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), false);

        assert_eq!(cpu.register_a, 0b10011000); // 152
        assert!(cpu.flags.contains(CpuFlags::CARRY));
    }

    #[test]
    fn test_asl_zero_page() {
        let program = vec![0x06, 0x00]; // ASL $00
        let mut cpu = prepare_test_cpu(&program);

        cpu.poke(0x00, 0b11001100); // 204

        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), false);

        assert_eq!(cpu.peek(0x00), 0b10011000); // 152
        assert!(cpu.flags.contains(CpuFlags::CARRY));
    }

    #[test]
    fn test_bcc_branch_taken() {
        let program = vec![0x90, 0x05]; // BCC with offset of 5
        let mut cpu = prepare_test_cpu(&program);

        cpu.flags.remove(CpuFlags::CARRY);

        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), false);

        assert_eq!(cpu.program_counter, 0x8007);
    }

    #[test]
    fn test_bcc_branch_not_taken() {
        let program = vec![0x90, 0x05]; // BCC with offset of 5
        let mut cpu = prepare_test_cpu(&program);

        cpu.flags.insert(CpuFlags::CARRY);

        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), true);

        assert_eq!(cpu.program_counter, 0x8002);
    }

    #[test]
    fn test_bcc_negative_offset() {
        let program = vec![0x90, 0xFB]; // BCC with offset of -5
        let mut cpu = prepare_test_cpu(&program);

        cpu.flags.remove(CpuFlags::CARRY);

        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), true);

        // Expected PC: 0x8000 (start) + 2 (instruction length) - 5 (offset) = 0x7FFD
        assert_eq!(cpu.program_counter, 0x7FFD);
    }

    #[test]
    fn test_bcs_branch_taken() {
        let program = vec![0xB0, 0x05]; // BCS with offset of 5
        let mut cpu = prepare_test_cpu(&program);

        cpu.flags.insert(CpuFlags::CARRY);

        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), true);

        assert_eq!(cpu.program_counter, 0x8007);
    }

    #[test]
    fn test_bcs_branch_not_taken() {
        let program = vec![0xB0, 0x05]; // BCS with offset of 5
        let mut cpu = prepare_test_cpu(&program);

        cpu.flags.remove(CpuFlags::CARRY);

        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), true);

        assert_eq!(cpu.program_counter, 0x8002);
    }

    #[test]
    fn test_bcs_negative_offset() {
        let program = vec![0xB0, 0xFB]; // BCS with offset of -5
        let mut cpu = prepare_test_cpu(&program);

        cpu.flags.insert(CpuFlags::CARRY);

        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), true);

        assert_eq!(cpu.program_counter, 0x8000u16.wrapping_add(2).wrapping_sub(5));
    }

    #[test]
    fn test_bcs_nop5() {
        let program = vec![0xB0, 0x05, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA]; // BCS +5, followed by 5 NOPs
        let mut cpu = prepare_test_cpu(&program);

        cpu.flags.insert(CpuFlags::CARRY);

        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), true);

        assert_eq!(cpu.program_counter, 0x8007);
    }

    #[test]
    fn test_jmp_absolute() {
        let program = vec![0x4C, 0x34, 0x12]; // JMP Absolute 0x4C 0x1234
        let mut cpu = prepare_test_cpu(&program);

        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), true);

        assert_eq!(cpu.program_counter, 0x1234);
    }

    #[test]
    fn test_jmp_indirect() {
        let program = vec![0x6C, 0x00, 0x20]; // JMP Indirect 0x6C 0x2000
        let mut cpu = prepare_test_cpu(&program);

        // set up the JMP indirect
        cpu.poke(0x2000, 0x78); // LSB
        cpu.poke(0x2001, 0x56); // MSB

        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), true);

        assert_eq!(cpu.program_counter, 0x5678);
    }

    #[test]
    fn test_jsr() {
        let program = vec![0x20, 0x34, 0x12]; // JSR $1234
        let mut cpu = prepare_test_cpu(&program);

        cpu.stack_pointer = 0xFF;

        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), true);

        assert_eq!(cpu.program_counter, 0x1234, "Program counter should be set to the target address");

        assert_eq!(cpu.stack_pointer, 0xFD, "Stack pointer should be decremented by 2");

        // Check the values on the stack
        assert_eq!(cpu.pop_stack(), 0x02, "Low byte of return address should be on the stack");
        assert_eq!(cpu.pop_stack(), 0x80, "High byte of return address should be on the stack");
    }

    fn run_single(program: &[u8], setup: impl FnOnce(&mut CPU<MockBus>)) -> CPU<MockBus> {
        let mut cpu = prepare_test_cpu(program);
        setup(&mut cpu);
        cpu.interpret_for_test(get_program_end(cpu.program_counter, program.len()), true);
        cpu
    }

    #[test]
    fn test_opcode_table_is_consistent() {
        let mut seen = std::collections::HashSet::new();

        for op in CPU_OPERATION_CODES.iter() {
            assert!(seen.insert(op.code), "duplicate opcode {:#04X}", op.code);
            assert!(CPU_OPERATION_TABLE[op.code as usize].is_some());
        }

        assert_eq!(CPU_OPERATION_CODES.iter().filter(|op| op.official).count(), 151);
        assert_eq!(CPU_OPERATION_TABLE.iter().flatten().count(), CPU_OPERATION_CODES.len());
    }

    /// The CPU derives timing from bus accesses; the cycle counts in the table must agree with it
    /// (no page crossing, branches not taken) and PC must advance by the instruction length.
    #[test]
    fn test_every_opcode_takes_table_cycles_and_advances_pc() {
        for op in CPU_OPERATION_CODES.iter() {
            if op.addressing_mode == AddressingMode::Relative {
                continue; // branch timing depends on the outcome, see the branch tests
            }

            let program = [op.code, 0x00, 0x00];
            let mut cpu = prepare_test_cpu(&program);

            assert!(cpu.step(), "opcode {:#04X} {:?} not executed", op.code, op.mnemonic);

            assert_eq!(cpu.cycles, op.cycles as u64, "cycles of {:#04X} {:?} {:?}", op.code, op.mnemonic, op.addressing_mode);

            let jumps = matches!(op.mnemonic, Mnemonic::JMP | Mnemonic::JSR | Mnemonic::RTS | Mnemonic::RTI | Mnemonic::BRK);
            if !jumps {
                assert_eq!(cpu.program_counter, 0x8000 + op.bytes as u16, "length of {:#04X} {:?} {:?}", op.code, op.mnemonic, op.addressing_mode);
            }
        }
    }

    #[test]
    fn test_branch_cycles() {
        // BEQ +2 at $8000: not taken 2, taken 3, taken across a page 4
        let not_taken = run_single(&[0xF0, 0x02], |_| {});
        let taken = run_single(&[0xF0, 0x02], |cpu| cpu.insert_flag(CpuFlags::ZERO));
        let crossed = run_single(&[0xF0, 0x7F], |cpu| {
            cpu.program_counter = 0x80F0; // next instruction $80F2, target $8171
            cpu.poke(0x80F0, 0xF0);
            cpu.poke(0x80F1, 0x7F);
            cpu.insert_flag(CpuFlags::ZERO);
        });

        assert_eq!(not_taken.cycles, 2);
        assert_eq!(taken.cycles, 3);
        assert_eq!(taken.program_counter, 0x8004);
        assert_eq!(crossed.cycles, 4);
    }

    #[test]
    fn test_reset_loads_vector_and_takes_seven_cycles() {
        let mut bus = MockBus::new();
        bus.memory[0xFFFC] = 0x34;
        bus.memory[0xFFFD] = 0x12;
        let mut cpu = CPU::new(bus);
        cpu.register_a = 0x55;
        cpu.clear_flag(CpuFlags::INTERRUPT_DISABLE);

        cpu.reset();

        assert_eq!(cpu.program_counter, 0x1234);
        assert_eq!(cpu.stack_pointer, 0xFA);
        assert_eq!(cpu.cycles, 7);
        assert!(cpu.flags.contains(CpuFlags::INTERRUPT_DISABLE));
        assert_eq!(cpu.register_a, 0x55);
    }

    #[test]
    fn test_unofficial_nop_skips_operand() {
        // 0x1A implied, 0x04 zero page, 0x0C absolute, 0x1C absolute,X, 0x80 immediate
        for (program, len) in [(vec![0x1A], 1u16), (vec![0x04, 0x10], 2), (vec![0x0C, 0x00, 0x20], 3), (vec![0x1C, 0x00, 0x20], 3), (vec![0x80, 0x44], 2)] {
            let cpu = run_single(&program, |cpu| cpu.register_a = 0x42);
            assert_eq!(cpu.program_counter, 0x8000 + len, "program {:02X?}", program);
            assert_eq!(cpu.register_a, 0x42);
        }
    }

    #[test]
    fn test_sbc_unofficial_eb() {
        let cpu = run_single(&[0xEB, 0x05], |cpu| { cpu.register_a = 0x10; cpu.insert_flag(CpuFlags::CARRY); });

        assert_eq!(cpu.register_a, 0x0B);
        assert!(cpu.flags.contains(CpuFlags::CARRY));
    }

    #[test]
    fn test_lax_zero_page() {
        let cpu = run_single(&[0xA7, 0x10], |cpu| cpu.poke(0x10, 0x80));

        assert_eq!(cpu.register_a, 0x80);
        assert_eq!(cpu.register_x, 0x80);
        assert!(cpu.flags.contains(CpuFlags::NEGATIVE));
        assert!(!cpu.flags.contains(CpuFlags::ZERO));
    }

    #[test]
    fn test_lax_indirect_y() {
        let cpu = run_single(&[0xB3, 0x20], |cpu| {
            cpu.poke(0x20, 0x00);
            cpu.poke(0x21, 0x30);
            cpu.register_y = 0x05;
            cpu.poke(0x3005, 0x7F);
        });

        assert_eq!(cpu.register_a, 0x7F);
        assert_eq!(cpu.register_x, 0x7F);
    }

    #[test]
    fn test_sax_zero_page() {
        let mut cpu = run_single(&[0x87, 0x10], |cpu| { cpu.register_a = 0xF0; cpu.register_x = 0x3C; });

        assert_eq!(cpu.peek(0x10), 0x30);
        assert!(!cpu.flags.contains(CpuFlags::ZERO), "SAX does not touch flags");
    }

    #[test]
    fn test_dcp_zero_page() {
        let mut cpu = run_single(&[0xC7, 0x10], |cpu| { cpu.poke(0x10, 0x10); cpu.register_a = 0x0F; });

        assert_eq!(cpu.peek(0x10), 0x0F);
        assert_eq!(cpu.register_a, 0x0F);
        assert!(cpu.flags.contains(CpuFlags::ZERO));
        assert!(cpu.flags.contains(CpuFlags::CARRY));
    }

    #[test]
    fn test_isb_zero_page() {
        let mut cpu = run_single(&[0xE7, 0x10], |cpu| {
            cpu.poke(0x10, 0x0F);
            cpu.register_a = 0x20;
            cpu.insert_flag(CpuFlags::CARRY);
        });

        assert_eq!(cpu.peek(0x10), 0x10);
        assert_eq!(cpu.register_a, 0x10); // 0x20 - 0x10
        assert!(cpu.flags.contains(CpuFlags::CARRY));
    }

    #[test]
    fn test_slo_zero_page() {
        let mut cpu = run_single(&[0x07, 0x10], |cpu| { cpu.poke(0x10, 0x81); cpu.register_a = 0x01; });

        assert_eq!(cpu.peek(0x10), 0x02);
        assert_eq!(cpu.register_a, 0x03);
        assert!(cpu.flags.contains(CpuFlags::CARRY));
    }

    #[test]
    fn test_rla_zero_page() {
        let mut cpu = run_single(&[0x27, 0x10], |cpu| {
            cpu.poke(0x10, 0x81);
            cpu.register_a = 0xFF;
            cpu.insert_flag(CpuFlags::CARRY);
        });

        assert_eq!(cpu.peek(0x10), 0x03);
        assert_eq!(cpu.register_a, 0x03);
        assert!(cpu.flags.contains(CpuFlags::CARRY));
    }

    #[test]
    fn test_sre_zero_page() {
        let mut cpu = run_single(&[0x47, 0x10], |cpu| { cpu.poke(0x10, 0x03); cpu.register_a = 0xFF; });

        assert_eq!(cpu.peek(0x10), 0x01);
        assert_eq!(cpu.register_a, 0xFE);
        assert!(cpu.flags.contains(CpuFlags::CARRY));
        assert!(cpu.flags.contains(CpuFlags::NEGATIVE));
    }

    #[test]
    fn test_rra_zero_page() {
        let mut cpu = run_single(&[0x67, 0x10], |cpu| { cpu.poke(0x10, 0x03); cpu.register_a = 0x10; });

        assert_eq!(cpu.peek(0x10), 0x01); // ROR: 0x03 >> 1, carry out = 1
        assert_eq!(cpu.register_a, 0x12);     // ADC: 0x10 + 0x01 + carry(1)
        assert!(!cpu.flags.contains(CpuFlags::CARRY));
    }

    #[test]
    fn test_anc_immediate() {
        let cpu = run_single(&[0x0B, 0x80], |cpu| cpu.register_a = 0xFF);

        assert_eq!(cpu.register_a, 0x80);
        assert!(cpu.flags.contains(CpuFlags::NEGATIVE));
        assert!(cpu.flags.contains(CpuFlags::CARRY));
    }

    #[test]
    fn test_alr_immediate() {
        let cpu = run_single(&[0x4B, 0x0F], |cpu| cpu.register_a = 0xFF);

        assert_eq!(cpu.register_a, 0x07);
        assert!(cpu.flags.contains(CpuFlags::CARRY));
    }

    #[test]
    fn test_arr_immediate() {
        let cpu = run_single(&[0x6B, 0xFF], |cpu| { cpu.register_a = 0xFF; cpu.insert_flag(CpuFlags::CARRY); });

        assert_eq!(cpu.register_a, 0xFF);
        assert!(cpu.flags.contains(CpuFlags::CARRY));   // bit 6
        assert!(!cpu.flags.contains(CpuFlags::OVERFLOW)); // bit 6 xor bit 5
    }

    #[test]
    fn test_axs_immediate() {
        let cpu = run_single(&[0xCB, 0x05], |cpu| { cpu.register_a = 0xFF; cpu.register_x = 0x0F; });

        assert_eq!(cpu.register_x, 0x0A); // (0xFF & 0x0F) - 5
        assert!(cpu.flags.contains(CpuFlags::CARRY));
        assert_eq!(cpu.register_a, 0xFF);
    }

    #[test]
    fn test_indirect_x_pointer_wraps_in_zero_page() {
        // LDA ($FF,X) with X=0: the pointer high byte comes from $0000, not $0100
        let cpu = run_single(&[0xA1, 0xFF], |cpu| {
            cpu.poke(0x00FF, 0x00);
            cpu.poke(0x0000, 0x04);
            cpu.poke(0x0100, 0x05);
            cpu.poke(0x0400, 0x5D);
            cpu.poke(0x0500, 0x11);
        });

        assert_eq!(cpu.register_a, 0x5D);
    }

    #[test]
    fn test_indirect_y_pointer_wraps_in_zero_page() {
        let cpu = run_single(&[0xB1, 0xFF], |cpu| {
            cpu.poke(0x00FF, 0x00);
            cpu.poke(0x0000, 0x04);
            cpu.poke(0x0100, 0x05);
            cpu.poke(0x0400, 0x5D);
            cpu.poke(0x0500, 0x11);
        });

        assert_eq!(cpu.register_a, 0x5D);
    }

    #[test]
    fn test_page_cross_penalty_on_indexed_read() {
        // LDA $20FF,X : 4 cycles, 5 when X pushes the address into the next page
        let no_cross = run_single(&[0xBD, 0x00, 0x20], |cpu| cpu.register_x = 0x01);
        let cross = run_single(&[0xBD, 0xFF, 0x20], |cpu| cpu.register_x = 0x01);

        assert_eq!(no_cross.cycles, 4);
        assert_eq!(cross.cycles, 5);
    }

    #[test]
    fn test_no_page_cross_penalty_on_indexed_store() {
        // STA $20FF,X always takes 5 cycles
        let cross = run_single(&[0x9D, 0xFF, 0x20], |cpu| cpu.register_x = 0x01);

        assert_eq!(cross.cycles, 5);
    }

    #[test]
    fn test_branch_page_cross_is_relative_to_next_instruction() {
        // BEQ at $80FE: next instruction is $8100, target $8105 -> same page, only +1 cycle
        let mut program = vec![0xEA; 0xFE];
        program.extend([0xF0, 0x05]);
        let mut cpu = prepare_test_cpu(&program);
        cpu.program_counter = 0x80FE;
        cpu.insert_flag(CpuFlags::ZERO);

        assert!(cpu.step());

        assert_eq!(cpu.program_counter, 0x8105);
        assert_eq!(cpu.cycles, 3);
    }
}
