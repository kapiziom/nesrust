use crate::emulator::cpu::CPU;

pub trait AddressingModeOperations {
    fn get_immediate(&self, program: &[u8]) -> u8;
    fn get_zero_page(&self, program: &[u8]) -> u8;
    fn get_zero_page_x(&self, program: &[u8]) -> u8;
    fn get_zero_page_y(&self, program: &[u8]) -> u8;
    fn get_absolute(&self, program: &[u8]) -> u8;
    fn get_absolute_x(&self, program: &[u8]) -> u8;
    fn get_absolute_y(&self, program: &[u8]) -> u8;
    fn get_indirect(&self, program: &[u8]) -> u8;
    fn get_indirect_x(&self, program: &[u8]) -> u8;
    fn get_indirect_y(&self, program: &[u8]) -> u8;
    fn get_relative(&self, program: &[u8]) -> u8;
    fn get_accumulator(&self) -> u8;
}

impl<'a> AddressingModeOperations for CPU<'a> {
    fn get_immediate(&self, program: &[u8]) -> u8 {
        program[self.program_counter as usize]
    }

    fn get_zero_page(&self, program: &[u8]) -> u8 {
        self.bus.read(program[self.program_counter as usize] as u16)
    }

    fn get_zero_page_x(&self, program: &[u8]) -> u8 {
        self.bus.read(self.zero_page(program, self.register_x) as u16)
    }

    fn get_zero_page_y(&self, program: &[u8]) -> u8 {
        self.bus.read(self.zero_page(program, self.register_y) as u16)
    }

    fn get_absolute(&self, program: &[u8]) -> u8 {
        self.bus.read(self.absolute(program))
    }

    fn get_absolute_x(&self, program: &[u8]) -> u8 {
        self.bus.read(self.absolute_register(program, self.register_x))
    }

    fn get_absolute_y(&self, program: &[u8]) -> u8 {
        self.bus.read(self.absolute_register(program, self.register_y))
    }

    fn get_indirect(&self, program: &[u8]) -> u8 {
        let lo = program[self.program_counter as usize] as u16;
        let hi = program[(self.program_counter + 1) as usize] as u16;
        let pointer = (hi << 8) | lo;

        let lo_address = self.bus.read(pointer) as u16;
        let hi_address = self.bus.read((pointer & 0xFF00) | ((pointer + 1) & 0x00FF)) as u16;
        let actual_address = (hi_address << 8) | lo_address;

        self.bus.read(actual_address)
    }

    fn get_indirect_x(&self, program: &[u8]) -> u8 {
        let base_ptr = program[self.program_counter as usize]
            .wrapping_add(self.register_x) as u16;

        let lo_address = self.bus.read(base_ptr) as u16;
        let hi_address = self.bus.read((base_ptr + 1) & 0x00FF) as u16;
        let actual_address = (hi_address << 8) | lo_address;

        self.bus.read(actual_address)
    }

    fn get_indirect_y(&self, program: &[u8]) -> u8 {
        let base_ptr = program[self.program_counter as usize] as u16;

        let lo_address = self.bus.read(base_ptr) as u16;
        let hi_address = self.bus.read((base_ptr + 1) & 0x00FF) as u16;
        let base_address = (hi_address << 8) | lo_address;

        let actual_address = base_address.wrapping_add(self.register_y as u16);
        self.bus.read(actual_address)
    }

    fn get_relative(&self, program: &[u8]) -> u8 {
        let offset = program[self.program_counter as usize] as i8;

        let target_address = self.program_counter.wrapping_add(offset as u16);

        self.bus.read(target_address)
    }

    fn get_accumulator(&self) -> u8 {
        return self.register_a;
    }
}

impl<'a> CPU<'a> {

    fn zero_page(&self, program: &[u8], register: u8) -> u8 {
        let base_address = program[self.program_counter as usize];
        let address = base_address.wrapping_add(register) as u16; // <= 0xFF
        self.bus.read(address)
    }

    fn absolute(&self, program: &[u8]) -> u16 {
        let lo = program[self.program_counter as usize] as u16;
        let hi = program[(self.program_counter + 1) as usize] as u16;
        (hi << 8) | lo
    }

    fn absolute_register(&self, program: &[u8], register: u8) -> u16 {
        let lo = program[self.program_counter as usize] as u16;
        let hi = program[(self.program_counter + 1) as usize] as u16;
        let base_address = (hi << 8) | lo;

        base_address.wrapping_add(register as u16) // <= 0xFF
    }
}