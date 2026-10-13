use crate::emulator::bus::Bus;

pub trait CpuBus {
    /// Reads can have side effects on real hardware (PPU status, controller shift registers), hence `&mut self`.
    fn read(&mut self, addr: u16) -> u8;

    fn read_u16(&mut self, addr: u16) -> u16 {
        let lo = self.read(addr) as u16;
        let hi = self.read(addr.wrapping_add(1)) as u16;
        (hi << 8) | lo
    }

    fn write(&mut self, addr: u16, data: u8);

    fn write_u16(&mut self, addr: u16, data: u16) {
        let lo = (data & 0xFF) as u8;
        let hi = (data >> 8) as u8;
        self.write(addr, lo);
        self.write(addr.wrapping_add(1), hi);
    }

    fn tick(&mut self, cycles: u8);

    fn fetch_nmi(&mut self) -> Option<u8>;

    /// Level of the IRQ line (APU frame counter/DMC, mappers). Stays true until the source is acknowledged.
    fn irq_pending(&self) -> bool {
        false
    }
}


impl CpuBus for Bus {
    fn read(&mut self, addr: u16) -> u8 {
        let value = match addr {
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
                self.last_bus_value
            }
            // Controller registers: only the low bits are driven, the rest is open bus
            0x4016..=0x4017 => {
                // TODO: Implement controller reading
                self.last_bus_value & 0xE0
            }
            // Cartridge: expansion area, PRG-RAM and PRG-ROM; nothing driving the bus means open bus
            0x4020..=0xFFFF => {
                self.cartridge.cpu_read(addr).unwrap_or(self.last_bus_value)
            }
            // $4018-$401F: disabled test registers
            _ => self.last_bus_value
        };

        self.last_bus_value = value;
        value
    }

    fn write(&mut self, addr: u16, data: u8) {
        self.last_bus_value = data;

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
            }
            // Controller registers
            0x4016..=0x4017 => {
                // TODO: Implement controller writing
            }
            // Cartridge: expansion area, PRG-RAM and mapper registers
            0x4020..=0xFFFF => {
                self.cartridge.cpu_write(addr, data)
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

    fn irq_pending(&self) -> bool {
        self.cartridge.irq_pending()
    }
}