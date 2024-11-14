use crate::emulator::cpu::{CPU, cpu_flags};
use crate::emulator::cpu::cpu_flags::CpuFlags;

pub trait FlagOperations {
    fn set_flag(&mut self, flag: CpuFlags, value: bool);

    fn clear_flag(&mut self, flag: CpuFlags);

    fn insert_flag(&mut self, flag: CpuFlags);

    fn get_flag_value(&mut self, flag: CpuFlags) -> u16;

    fn contains_flag(&mut self, flag: CpuFlags) -> bool;

    fn update_zero_and_negative_flags(&mut self, result: u8);
}

impl<'a> FlagOperations for CPU<'a> {
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
        if self.flags.contains(flag) { 1 } else { 0 }
    }

    fn contains_flag(&mut self, flag: CpuFlags) -> bool {
        self.flags.contains(flag)
    }

    fn update_zero_and_negative_flags(&mut self, result: u8) {
        self.set_flag(CpuFlags::ZERO, result == 0);
        self.set_flag(CpuFlags::NEGATIVE, result & 0x80 != 0);
    }
}