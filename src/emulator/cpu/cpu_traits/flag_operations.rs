use crate::emulator::cpu::CPU;
use crate::emulator::cpu::cpu_flags::CpuFlags;

pub trait FlagOperations {
    fn set_flag(&mut self, flag: CpuFlags, value: bool);
    fn clear_flag(&mut self, flag: CpuFlags);
}

impl<'a> FlagOperations for CPU<'a> {
    fn set_flag(&mut self, flag: CpuFlags, value: bool) {
        self.flags.set(flag, value);
    }

    fn clear_flag(&mut self, flag: CpuFlags) {
        self.flags.set(flag, false);
    }
}