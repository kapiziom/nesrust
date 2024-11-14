use crate::emulator::cpu::CPU;

pub trait AddressingModeOperations {
    fn get_immediate(&self, program: &[u8]) -> u8;
    fn get_zero_page_address(&self, program: &[u8]) -> u16;
    fn get_zero_page(&self, program: &[u8]) -> u8;
    fn get_zero_page_x_address(&self, program: &[u8]) -> u16;
    fn get_zero_page_x(&self, program: &[u8]) -> u8;
    fn get_zero_page_y_address(&self, program: &[u8]) -> u16;
    fn get_zero_page_y(&self, program: &[u8]) -> u8;
    fn get_absolute_address(&self, program: &[u8]) -> u16;
    fn get_absolute(&self, program: &[u8]) -> u8;
    fn get_absolute_x_address(&self, program: &[u8]) -> u16;
    fn get_absolute_x(&self, program: &[u8]) -> u8;
    fn get_absolute_y_address(&self, program: &[u8]) -> u16;
    fn get_absolute_y(&self, program: &[u8]) -> u8;
    fn get_indirect_address(&self, program: &[u8]) -> u16;
    fn get_indirect(&self, program: &[u8]) -> u8;
    fn get_indirect_x_address(&self, program: &[u8]) -> u16;
    fn get_indirect_x(&self, program: &[u8]) -> u8;
    fn get_indirect_y_address(&self, program: &[u8]) -> u16;
    fn get_indirect_y(&self, program: &[u8]) -> u8;
    fn get_relative(&self, program: &[u8]) -> u8;
    fn get_accumulator(&self) -> u8;
}

impl<'a> AddressingModeOperations for CPU<'a> {
    fn get_immediate(&self, program: &[u8]) -> u8 {
        program[self.program_counter as usize]
    }

    fn get_zero_page_address(&self, program: &[u8]) -> u16 {
        program[self.program_counter as usize] as u16
    }

    fn get_zero_page(&self, program: &[u8]) -> u8 {
        let address = self.get_zero_page_address(program);
        self.bus.read(address)
    }

    fn get_zero_page_x_address(&self, program: &[u8]) -> u16 {
        (program[self.program_counter as usize].wrapping_add(self.register_x)) as u16
    }

    fn get_zero_page_x(&self, program: &[u8]) -> u8 {
        let address = self.get_zero_page_x_address(program);
        self.bus.read(address)
    }

    fn get_zero_page_y_address(&self, program: &[u8]) -> u16 {
        (program[self.program_counter as usize].wrapping_add(self.register_y)) as u16
    }

    fn get_zero_page_y(&self, program: &[u8]) -> u8 {
        let address = self.get_zero_page_y_address(program);
        self.bus.read(address)
    }

    fn get_absolute_address(&self, program: &[u8]) -> u16 {
        let low = program[self.program_counter as usize] as u16;
        let high = program[self.program_counter as usize + 1] as u16;
        (high << 8) | low
    }

    fn get_absolute(&self, program: &[u8]) -> u8 {
        let address = self.get_absolute_address(program);
        self.bus.read(address)
    }

    fn get_absolute_x_address(&self, program: &[u8]) -> u16 {
        self.get_absolute_address(program).wrapping_add(self.register_x as u16)
    }

    fn get_absolute_x(&self, program: &[u8]) -> u8 {
        let address = self.get_absolute_x_address(program);
        self.bus.read(address)
    }

    fn get_absolute_y_address(&self, program: &[u8]) -> u16 {
        self.get_absolute_address(program).wrapping_add(self.register_y as u16)
    }

    fn get_absolute_y(&self, program: &[u8]) -> u8 {
        let address = self.get_absolute_y_address(program);
        self.bus.read(address)
    }

    fn get_indirect_address(&self, program: &[u8]) -> u16 {
        let ptr = self.get_absolute_address(program);
        let low = self.bus.read(ptr) as u16;
        let high = self.bus.read(ptr.wrapping_add(1)) as u16;
        (high << 8) | low
    }

    fn get_indirect(&self, program: &[u8]) -> u8 {
        let address = self.get_indirect_address(program);
        self.bus.read(address)
    }

    fn get_indirect_x_address(&self, program: &[u8]) -> u16 {
        let base = program[self.program_counter as usize];
        let ptr = base.wrapping_add(self.register_x) as u16;
        let low = self.bus.read(ptr) as u16;
        let high = self.bus.read(ptr.wrapping_add(1)) as u16;
        (high << 8) | low
    }

    fn get_indirect_x(&self, program: &[u8]) -> u8 {
        let address = self.get_indirect_x_address(program);
        self.bus.read(address)
    }

    fn get_indirect_y_address(&self, program: &[u8]) -> u16 {
        let base = program[self.program_counter as usize] as u16;
        let low = self.bus.read(base) as u16;
        let high = self.bus.read(base.wrapping_add(1)) as u16;
        let deref_base = (high << 8) | low;
        deref_base.wrapping_add(self.register_y as u16)
    }

    fn get_indirect_y(&self, program: &[u8]) -> u8 {
        let address = self.get_indirect_y_address(program);
        self.bus.read(address)
    }

    fn get_relative(&self, program: &[u8]) -> u8 {
        program[self.program_counter as usize]
    }

    fn get_accumulator(&self) -> u8 {
        self.register_a
    }
}