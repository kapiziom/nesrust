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

    fn tick(&mut self, cycles: u8);

    fn fetch_nmi(&mut self) -> Option<u8>;
}


impl CpuBus for Bus {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            // Internal RAM + mirroring
            0x0000..=0x1FFF => {
                self.ram.read(addr & 0x07FF)
            }
            // PPU registers + mirroring
            0x2000..=0x3FFF => {
                self.ppu.read(0x2000 + (addr & 0x7))
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
                self.rom.read_expansion(addr)
            }
            // SRAM
            0x6000..=0x7FFF => {
                self.rom.read_sram(addr)
            }
            // PRG ROM
            0x8000..=0xFFFF => {
                self.rom.read_prg(addr)
            }
            _ => 0
        }
    }

    fn write(&mut self, addr: u16, data: u8) {
        match addr {
            // Internal RAM + mirroring
            0x0000..=0x1FFF => {
                self.ram.write(addr & 0x07FF, data);
            }
            // PPU registers + mirroring
            0x2000..=0x3FFF => {
                self.ppu.write(0x2000 + (addr & 0x7), data);
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
                self.rom.write_expansion(addr, data)
            }
            // SRAM
            0x6000..=0x7FFF => {
                self.rom.write_sram(addr, data)
            }
            // PRG ROM (mapper-dependent)
            0x8000..=0xFFFF => {
                self.rom.write_prg(addr, data)
            }
            _ => {}
        }
    }

    fn tick(&mut self, cycles: u8) {
        self.tick(cycles as u16)
    }

    fn fetch_nmi(&mut self) -> Option<u8> {
        self.nmi_interrupt.take()
    }
}