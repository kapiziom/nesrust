
/* https://www.nesdev.org/wiki/Status_flags
7  bit  0
---- ----
NV1B DIZC
|||| ||||
|||| |||+- Carry
|||| ||+-- Zero
|||| |+--- Interrupt Disable
|||| +---- Decimal
|||+------ (No CPU effect; see: the B flag) // Break Command (B)
||+------- (No CPU effect; always pushed as 1) // Unused (always 1)
|+-------- Overflow
+--------- Negative
*/
use crate::emulator::cpu::CPU;

bitflags::bitflags! {
    #[derive(Clone, Copy)]
    pub struct CpuFlags: u8 {
        const CARRY = 0b0000_0001;
        const ZERO = 0b0000_0010;
        const INTERRUPT_DISABLE = 0b0000_0100;
        const DECIMAL_MODE = 0b0000_1000;
        const BREAK = 0b0001_0000;
        const UNUSED = 0b0010_0000;
        const OVERFLOW = 0b0100_0000;
        const NEGATIVE = 0b1000_0000;
    }
}


pub trait FlagOperations {
    fn get_status_register(&self) -> u8;

    fn set_flag(&mut self, flag: CpuFlags, value: bool);

    fn clear_flag(&mut self, flag: CpuFlags);

    fn insert_flag(&mut self, flag: CpuFlags);

    fn get_flag_value(&mut self, flag: CpuFlags) -> u16;

    fn contains_flag(&mut self, flag: CpuFlags) -> bool;

    fn update_zero_and_negative_flags(&mut self, result: u8);
}

impl<'a> FlagOperations for CPU<'a> {
    fn get_status_register(&self) -> u8 {
        self.flags.bits()
    }

    fn set_flag(&mut self, flag: CpuFlags, value: bool) {
        self.flags.set(flag, value);
    }

    fn clear_flag(&mut self, flag: CpuFlags) {
        self.flags.set(flag, false);
    }

    fn insert_flag(&mut self, flag: CpuFlags) {
        self.flags.set(flag, true);
    }

    fn get_flag_value(&mut self, flag: CpuFlags) -> u16 {
        if self.contains_flag(flag) { 1 } else { 0 }
    }

    fn contains_flag(&mut self, flag: CpuFlags) -> bool {
        self.flags.contains(flag)
    }

    fn update_zero_and_negative_flags(&mut self, result: u8) {
        self.set_flag(CpuFlags::ZERO, result == 0);
        self.set_flag(CpuFlags::NEGATIVE, result & 0x80 != 0);
    }
}