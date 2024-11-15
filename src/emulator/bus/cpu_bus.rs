use crate::emulator::bus::Bus;

pub trait CpuBus {
    fn read(&self, addr: u16) -> u8;

    fn read_u16(&self, addr: u16) -> u16 {
        let lo = self.read(addr) as u16;
        let hi = self.read(addr + 1) as u16;
        (hi << 8) | lo
    }

    fn write(&mut self, addr: u16, data: u8);

    fn write_u16(&mut self, addr: u16, data: u16) {
        let lo = (data & 0xFF) as u8;
        let hi = (data >> 8) as u8;
        self.write(addr, lo);
        self.write(addr + 1, hi);
    }
}


impl CpuBus for Bus {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            // Internal RAM + mirroring
            0x0000..=0x1FFF => {
                let mirrored_addr = addr & 0x07FF;
                self.memory.w_ram[mirrored_addr as usize]
            }
            // PPU registers + mirroring
            0x2000..=0x3FFF => {
                let mirrored_addr = 0x2000 + (addr & 0x7);
                // TODO: Implement PPU register reading
                panic!("not implemented")
            }
            // APU & I/O registers
            0x4000..=0x4015 => {
                // TODO: Implement APU register reading
                panic!("not implemented")
            }
            // Controller registers
            0x4016..=0x4017 => {
                // TODO: Implement controller reading
                panic!("not implemented")
            }
            // Expansion ROM
            0x4020..=0x5FFF => {
                // TODO: Expansion ROM reading
                panic!("not implemented")
            }
            // SRAM
            0x6000..=0x7FFF => {
                // TODO: Battery backed save or work RAM
                panic!("not implemented")
            }
            // PRG ROM
            0x8000..=0xFFFF => {
                let rom_addr = addr - 0x8000;
                self.memory.prg_rom[rom_addr as usize]
            }
            _ => 0
        }
    }

    fn write(&mut self, addr: u16, data: u8) {
        match addr {
            // Internal RAM + mirroring
            0x0000..=0x1FFF => {
                let mirrored_addr = addr & 0x07FF;
                self.memory.w_ram[mirrored_addr as usize] = data;
            }
            // PPU registers + mirroring
            0x2000..=0x3FFF => {
                let mirrored_addr = 0x2000 + (addr & 0x7);
                // TODO: Implement PPU register writing
                panic!("not implemented")
            }
            // APU & I/O registers
            0x4000..=0x4015 => {
                // TODO: Implement APU register writing
                panic!("not implemented")
            }
            // Controller registers
            0x4016..=0x4017 => {
                // TODO: Implement controller writing
                panic!("not implemented")
            }
            // Expansion ROM
            0x4020..=0x5FFF => {
                // TODO: Expansion ROM writing (if supported by mapper)
                panic!("not implemented")
            }
            // SRAM
            0x6000..=0x7FFF => {
                // TODO: Battery backed save or work RAM
                panic!("not implemented")
            }
            // PRG ROM (mapper-dependent)
            0x8000..=0xFFFF => {
                // TODO: Implement mapper-specific ROM writing
                // panic!("write rom section detected: {}", addr)
                panic!("not implemented")
            }
            _ => {}
        }
    }
}